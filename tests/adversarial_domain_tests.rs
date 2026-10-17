//! Suite de Pruebas Adversariales y Resiliencia para el Núcleo de Dominio (Fase 1)
//!
//! Pone a prueba la robustez del sistema frente a inyecciones de código/rutas,
//! caos aritmético en punto flotante (NaN, Infinitos), y paradojas de grafo.

use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::double_entry_table::{LimiteRango, RangoNumerico};
use lakehouse_rag::domain::models::operational_matrix::{
    BorradorHito, FaseOperativa, HitoOperativo,
};
use lakehouse_rag::domain::models::reference_graph::{AristaGrafo, TipoRelacionNormativa};

// =========================================================================
// VECTOR 1: Inyección y Path Traversal en Identificadores
// =========================================================================

#[test]
fn test_adv_center_id_rechaza_path_traversal() {
    let ataques = [
        "../../etc/passwd",
        "..\\..\\windows\\system32",
        "centro/../../data",
        "/absoluto/invalido",
        "C:\\archivos\\centro",
    ];

    for ataque in ataques {
        let res = CenterId::new(ataque);
        assert!(
            res.is_err(),
            "CenterId debió rechazar el intento de traversal: '{ataque}'"
        );
        assert!(matches!(res.unwrap_err(), DomainError::InvalidCenterId(_)));
    }
}

#[test]
fn test_adv_center_id_rechaza_bytes_nulos_y_caracteres_control() {
    let ataques = [
        "centro\0malicioso",
        "centro\r\ninvalido",
        "centro\tinvalido",
        "centro\u{200B}espacio_cero",
    ];

    for ataque in ataques {
        let res = CenterId::new(ataque);
        assert!(
            res.is_err(),
            "CenterId debió rechazar caracteres de control o invisibles: '{ataque}'"
        );
    }
}

#[test]
fn test_adv_doc_id_rechaza_strings_desmesurados_y_bytes_nulos() {
    // Intento con byte nulo
    let res_null = DocId::new("DOC-001\0INYECCION");
    assert!(res_null.is_err());

    // Intento con string gigante (100,000 caracteres para provocar agotamiento)
    let string_gigante = "A".repeat(100_000);
    let res_tamano = DocId::new(string_gigante);
    assert!(
        res_tamano.is_err(),
        "DocId debió rechazar un identificador con tamaño desmesurado"
    );
}

// =========================================================================
// VECTOR 2: Caos en Punto Flotante y Precisión de Fronteras
// =========================================================================

#[test]
fn test_adv_rango_rechaza_nan_e_infinitos_en_limites_finitos() {
    // 1. NaN como límite inferior
    let res_nan_inf = RangoNumerico::new(
        LimiteRango::Inclusivo(f64::NAN),
        LimiteRango::Inclusivo(10.0),
    );
    assert!(
        res_nan_inf.is_err(),
        "RangoNumerico debe rechazar NaN como límite inferior"
    );

    // 2. NaN como límite superior
    let res_nan_sup = RangoNumerico::new(
        LimiteRango::Inclusivo(0.0),
        LimiteRango::Inclusivo(f64::NAN),
    );
    assert!(
        res_nan_sup.is_err(),
        "RangoNumerico debe rechazar NaN como límite superior"
    );

    // 3. Infinito en límite inclusivo (el infinito no puede ser un extremo cerrado alcanzable)
    let res_inf_inc =
        RangoNumerico::new(LimiteRango::Inclusivo(f64::INFINITY), LimiteRango::Infinito);
    assert!(
        res_inf_inc.is_err(),
        "Límite inclusivo con infinito debe ser rechazado"
    );
}

#[test]
fn test_adv_rango_evaluacion_con_nan() {
    let rango = RangoNumerico::entre_inclusivo(10.0, 20.0).expect("Rango válido");
    // Al evaluar si NaN está contenido, jamás debe arrojar true ni entrar en pánico
    assert!(!rango.contiene(f64::NAN));
}

#[test]
fn test_adv_rango_precision_milimetrica_epsilon() {
    // Intervalo semiabierto: (1.0, 2.0]
    let rango = RangoNumerico::new(LimiteRango::Exclusivo(1.0), LimiteRango::Inclusivo(2.0))
        .expect("Rango semiabierto válido");

    // En 1.0 exacto debe ser falso (abierto)
    assert!(!rango.contiene(1.0));

    // En 1.0 + EPSILON debe ser verdadero (estrictamente mayor a 1.0)
    assert!(rango.contiene(1.0 + f64::EPSILON));

    // En 2.0 exacto debe ser verdadero (cerrado)
    assert!(rango.contiene(2.0));

    // En 2.0 + (2.0 * EPSILON) debe ser falso
    assert!(!rango.contiene(2.0 + (2.0 * f64::EPSILON)));
}

// =========================================================================
// VECTOR 3: Grafo Normativo y Paradojas Lógicas
// =========================================================================

#[test]
fn test_adv_grafo_rechaza_pesos_no_finitos() {
    let doc_a = DocId::new("DOC-001").unwrap();
    let doc_b = DocId::new("DOC-002").unwrap();

    // Peso con NaN
    let res_nan = AristaGrafo::new(
        "EDGE-001",
        doc_a.clone(),
        "Sec 1",
        doc_b.clone(),
        "Sec 2",
        TipoRelacionNormativa::DerogaA,
        f32::NAN,
        None,
    );
    assert!(
        res_nan.is_err(),
        "AristaGrafo debe rechazar peso con valor NaN"
    );

    // Peso infinito
    let res_inf = AristaGrafo::new(
        "EDGE-002",
        doc_a,
        "Sec 1",
        doc_b,
        "Sec 2",
        TipoRelacionNormativa::DerogaA,
        f32::INFINITY,
        None,
    );
    assert!(
        res_inf.is_err(),
        "AristaGrafo debe rechazar peso con valor infinito"
    );
}

#[test]
fn test_adv_grafo_rechaza_autoderogacion_paradojica() {
    let doc_a = DocId::new("DOC-001").unwrap();

    // Una cláusula no puede derogarse a sí misma (paradoja lógica)
    let res = AristaGrafo::new(
        "EDGE-PARADOJA",
        doc_a.clone(),
        "Sección 4.1",
        doc_a,
        "Sección 4.1",
        TipoRelacionNormativa::DerogaA,
        1.0,
        Some("Intento de autoderogación".to_string()),
    );

    assert!(
        res.is_err(),
        "AristaGrafo debe rechazar una cláusula que se deroga a sí misma"
    );
}

// =========================================================================
// VECTOR 4: Prevención de DoS por Payloads Excesivos en Hitos
// =========================================================================

#[test]
fn test_adv_hito_rechaza_payload_desmesurado() {
    let doc_id = DocId::new("DOC-001").unwrap();
    let texto_abusivo = "X".repeat(500_000); // 500 KB en un solo campo

    let res = HitoOperativo::new(BorradorHito {
        hito_id: "DOC-001@v1-HIT-0000".to_string(),
        doc_id,
        orden_secuencia: 0,
        fase: FaseOperativa::Inspeccion,
        objeto_alcance: texto_abusivo,
        rol_ejecutor: "Inspector".to_string(),
        rol_responsable: "Jefe de Calidad".to_string(),
        registro_codigo: Some("FOR-CAL-01".to_string()),
        registro_nombre: None,
        criterio_aceptacion: "Conforme".to_string(),
        condicion_aplicacion: None,
    });

    assert!(
        res.is_err(),
        "HitoOperativo debe rechazar strings de longitud abusiva"
    );
}
