//! Suite adversarial del dominio v2 (Fase 9)
//!
//! Ataca la normalización, los enums canónicos, `DocId`, los identificadores versionados,
//! las ventanas de vigencia, la pila de encabezados y la detección de roles fusionados con
//! Unicode y emoji, cadenas enormes, inyección SQL, fechas frontera y niveles extremos.

use chrono::NaiveDate;
use lakehouse_rag::domain::models::alcance::{
    MAX_CARACTERES_TITULO, PilaEncabezados, extraer_condicion,
};
use lakehouse_rag::domain::models::document::{DocId, TipoDocumento};
use lakehouse_rag::domain::models::identificadores::{TipoEntidad, id_versionado};
use lakehouse_rag::domain::models::operational_matrix::{
    AlertaExtraccion, BorradorHito, FaseOperativa, HitoOperativo,
};
use lakehouse_rag::domain::models::reference_graph::TipoRelacionNormativa;
use lakehouse_rag::domain::models::vigencia::VentanaVigencia;
use lakehouse_rag::domain::services::normalizacion::{
    codigo_canonico, normalizar_token, separar_revision,
};

const CARGAS_SQL: [&str; 6] = [
    "X' OR '1'='1",
    "PO-ALM-03'; DROP TABLE documentos;--",
    "doc\" UNION SELECT * FROM secretos --",
    "a`b`; DELETE FROM t",
    "PO/*comentario*/ALM",
    "PO-ALM-03\0",
];

fn fecha(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn hito_con_responsable(ejecutor: &str, responsable: &str) -> HitoOperativo {
    HitoOperativo::new(BorradorHito {
        hito_id: "PO-ALM-03@v3-HIT-0002".to_string(),
        doc_id: DocId::new("PO-ALM-03").unwrap(),
        orden_secuencia: 2,
        fase: FaseOperativa::Custodia,
        objeto_alcance: "Material acopiado".to_string(),
        rol_ejecutor: ejecutor.to_string(),
        rol_responsable: responsable.to_string(),
        registro_codigo: Some("FOR-ALM-02".to_string()),
        registro_nombre: None,
        criterio_aceptacion: "Sin daños".to_string(),
        condicion_aplicacion: None,
    })
    .expect("Hito válido")
}

// =========================================================================
// VECTOR 1: Unicode, emoji y homoglifos
// =========================================================================

#[test]
fn test_adv_normalizacion_con_emoji_y_unicode() {
    assert_eq!(normalizar_token("🔥Recepción🔥"), "RECEPCION");
    assert_eq!(normalizar_token("Straße"), "STRASSE");
    assert_eq!(
        normalizar_token("Jefe\u{200B}de\u{00A0}Obra"),
        "JEFE_DE_OBRA"
    );
    assert_eq!(normalizar_token("日本語"), "");
    // Los dígitos y letras de ancho completo no son ASCII: no se cuelan en el código
    assert_eq!(codigo_canonico("ＰＯ－ＡＬＭ－０３"), "");
    assert!(DocId::desde_codigo_libre("ＰＯ－ＡＬＭ").is_err());
}

#[test]
fn test_adv_enums_toleran_emoji_pero_no_inventan_valores() {
    assert_eq!(
        FaseOperativa::parse("📦 CUSTODIA").unwrap(),
        FaseOperativa::Custodia
    );
    assert_eq!(
        TipoDocumento::parse("📜 Norma ISO 3834").unwrap(),
        TipoDocumento::NormaInternacional
    );
    assert!(FaseOperativa::parse("🔥🔥🔥").is_err());
    assert!(TipoRelacionNormativa::parse("💣").is_err());
    assert_eq!(
        TipoRelacionNormativa::parse_con_respaldo("💣"),
        (TipoRelacionNormativa::ReferenciaA, true)
    );
}

#[test]
fn test_adv_doc_id_rechaza_unicode_invisible_y_emoji() {
    for ataque in [
        "PO-🔥",
        "PO\u{200B}ALM",
        "PO\u{00A0}ALM",
        "ＰＯ",
        "PO-ALM-03\u{202E}",
    ] {
        assert!(DocId::new(ataque).is_err(), "Debe rechazar {ataque:?}");
    }
}

// =========================================================================
// VECTOR 2: Cadenas enormes (DoS)
// =========================================================================

#[test]
fn test_adv_cadenas_enormes_no_rompen_la_normalizacion() {
    let enorme = "Á".repeat(1_000_000);
    assert_eq!(normalizar_token(&enorme).len(), 1_000_000);
    assert!(FaseOperativa::parse("A".repeat(100_000)).is_err());
    assert!(DocId::desde_codigo_libre(&"A".repeat(100_000)).is_err());

    let revision_desbordada = format!("PO-ALM-03 Rev. {}", "9".repeat(100_000));
    let (codigo, revision) = separar_revision(&revision_desbordada);
    assert_eq!(
        revision, None,
        "Un número de revisión desbordado no es una revisión"
    );
    assert!(codigo.starts_with("PO-ALM-03"));

    let (_, revision) = separar_revision(&"REV ".repeat(50_000));
    assert_eq!(revision, None);
}

#[test]
fn test_adv_titulo_enorme_se_recorta_en_la_pila() {
    let mut pila = PilaEncabezados::default();
    pila.apilar(1, &format!("Cuando {}", "ñ".repeat(1_000_000)));
    assert_eq!(pila.breadcrumb().chars().count(), MAX_CARACTERES_TITULO);
    assert!(pila.condicion_heredada().is_some());
    assert!(extraer_condicion(&"(".repeat(200_000)).is_none());
}

// =========================================================================
// VECTOR 3: Inyección SQL en códigos e identificadores
// =========================================================================

#[test]
fn test_adv_inyeccion_sql_rechazada_o_neutralizada() {
    for carga in CARGAS_SQL {
        assert!(
            DocId::new(carga).is_err(),
            "DocId::new debe rechazar {carga:?}"
        );

        let canonico = codigo_canonico(carga);
        assert!(
            canonico
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-' || c == '.'),
            "Código no canónico para {carga:?}: {canonico}"
        );
        assert!(
            !canonico.contains("--"),
            "Comentario SQL superviviente: {canonico}"
        );

        if let Ok(doc) = DocId::desde_codigo_libre(carga) {
            let id = id_versionado(&doc, 1, TipoEntidad::Chunk, 0);
            assert!(!id.contains('\'') && !id.contains(';') && !id.contains(' '));
        }
    }
    assert!(TipoRelacionNormativa::parse("DEROGA_A'; DROP TABLE x").is_err());
}

#[test]
fn test_adv_id_versionado_en_valores_extremos() {
    let doc = DocId::new("A").unwrap();
    let id = id_versionado(&doc, u32::MAX, TipoEntidad::Arista, usize::MAX);
    assert!(id.starts_with("A@v4294967295-ARI-"));
    assert_eq!(
        id.matches('@').count(),
        1,
        "El separador de versión es inequívoco"
    );
}

// =========================================================================
// VECTOR 4: Fechas frontera
// =========================================================================

#[test]
fn test_adv_vigencia_29_de_febrero() {
    let ventana = VentanaVigencia::new(fecha(2024, 2, 29), Some(fecha(2024, 3, 1))).unwrap();
    assert!(!ventana.vigente_en(fecha(2024, 2, 28)));
    assert!(ventana.vigente_en(fecha(2024, 2, 29)));
    assert!(!ventana.vigente_en(fecha(2024, 3, 1)));
    assert!(NaiveDate::from_ymd_opt(2023, 2, 29).is_none());
}

#[test]
fn test_adv_vigencia_ano_9999_y_extremos_del_calendario() {
    let fin_de_los_tiempos = fecha(9999, 12, 31);
    let abierta = VentanaVigencia::new(fin_de_los_tiempos, None).unwrap();
    assert!(abierta.vigente_en(fin_de_los_tiempos));
    assert!(!abierta.vigente_en(fecha(9999, 12, 30)));

    let mut total = VentanaVigencia::new(NaiveDate::MIN, None).unwrap();
    assert!(total.vigente_en(NaiveDate::MAX));
    total.derogar(NaiveDate::MAX).unwrap();
    assert!(!total.vigente_en(NaiveDate::MAX));
    assert!(
        total.derogar(fecha(2030, 1, 1)).is_err(),
        "La derogación es inmutable"
    );

    let mut en_el_limite = VentanaVigencia::new(NaiveDate::MAX, None).unwrap();
    assert!(en_el_limite.derogar(NaiveDate::MAX).is_err());
}

// =========================================================================
// VECTOR 5: Niveles de encabezado 0 y 255
// =========================================================================

#[test]
fn test_adv_pila_con_niveles_extremos_esta_acotada() {
    let mut pila = PilaEncabezados::default();
    for nivel in 0..=u8::MAX {
        pila.apilar(nivel, &format!("Cuando nivel {nivel}"));
    }
    assert_eq!(pila.profundidad(), 256);
    assert_eq!(
        pila.condicion_heredada().unwrap().matches(" Y ").count(),
        255
    );

    pila.apilar(255, "Hermano en 255");
    assert_eq!(pila.profundidad(), 256);
    pila.apilar(0, "Reinicio");
    assert_eq!(pila.breadcrumb(), "Reinicio");
    assert!(pila.condicion_heredada().is_none());

    pila.apilar(255, "\0\r\n\t");
    assert_eq!(pila.profundidad(), 1, "Un título solo de control se ignora");
}

// =========================================================================
// VECTOR 6: Roles fusionados camuflados
// =========================================================================

#[test]
fn test_adv_roles_fusionados_camuflados_se_detectan() {
    for responsable in [
        "Jefe de Producción\u{037E} Almacenero",
        "Jefe de Producción；Encargado",
        "Jefe de Producción／Inspector",
        "Jefe\u{00A0}de\u{00A0}Producción\u{00A0}y\u{00A0}Encargado de almacén",
        "Jefe de Producción\u{200B}y\u{200B}Inspector",
        "JEFE DE PRODUCCIÓN E INSPECTORA",
        "Jefe de Producción y el Técnico de Calidad",
        "Jefe de Producción (ejecuta el operario)",
        "  ALMACENERO  ",
    ] {
        let hito = hito_con_responsable("Almacenero", responsable);
        assert!(
            hito.alertas_calidad()
                .contains(&AlertaExtraccion::RolesFusionados),
            "No detectó la fusión en {responsable:?}"
        );
    }
}

#[test]
fn test_adv_roles_legitimos_no_dan_falsos_positivos() {
    for responsable in [
        "Jefe de Calidad y Medio Ambiente",
        "Responsable de Seguridad e Higiene",
        "Inspector jefe",
        "Responsable técnico",
    ] {
        let hito = hito_con_responsable("Almacenero", responsable);
        assert!(
            hito.alertas_calidad().is_empty(),
            "Falso positivo en {responsable:?}: {:?}",
            hito.alertas_calidad()
        );
    }
}
