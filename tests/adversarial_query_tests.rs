//! Pruebas Adversariales de Seguridad y Resiliencia para el Motor Analítico y Grafo Normativo
//!
//! Cubre: Inyecciones SQL maliciosas, aislamiento estricto multi-centro, valores numéricos extremos (NaN, Inf),
//! paradojas de derogación circular y robustez ante grafos desconectados.

use std::sync::Arc;

use arrow_array::{Int32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use lakehouse_rag::application::ports::analytical_engine::AnalyticalEnginePort;
use lakehouse_rag::application::ports::graph_engine::GraphEnginePort;
use lakehouse_rag::application::ports::table_lookup::TableLookupPort;
use lakehouse_rag::application::use_cases::table_lookup_service::TableLookupService;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::double_entry_table::{CeldaDobleEntrada, RangoNumerico};
use lakehouse_rag::domain::models::reference_graph::{AristaGrafo, TipoRelacionNormativa};
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::query_engine::petgraph_engine::PetgraphEngine;

fn crear_lote_base_seguridad() -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("valor", DataType::Int32, false),
    ]));

    let id_col = Arc::new(StringArray::from(vec!["SEC-01", "SEC-02"]));
    let val_col = Arc::new(Int32Array::from(vec![100, 200]));

    RecordBatch::try_new(schema, vec![id_col, val_col]).unwrap()
}

#[tokio::test]
async fn test_adv_bloqueo_inyecciones_sql_destructivas_y_sintaxis_maliciosa() {
    let engine = DataFusionEngine::new();
    let batch = crear_lote_base_seguridad();
    engine
        .registrar_tabla("datos_seguros", vec![batch])
        .await
        .unwrap();

    let payloads_adversariales = [
        "DROP TABLE datos_seguros",
        "drop table datos_seguros",
        "DELETE FROM datos_seguros WHERE 1=1",
        "INSERT INTO datos_seguros VALUES ('HACK', 999)",
        "UPDATE datos_seguros SET valor = 0",
        "ALTER TABLE datos_seguros ADD COLUMN hack INT",
        "TRUNCATE TABLE datos_seguros",
        "GRANT ALL PRIVILEGES ON datos_seguros TO public",
        "REVOKE ALL ON datos_seguros FROM public",
        "SELECT * FROM datos_seguros; DROP TABLE datos_seguros;",
        "SELECT * FROM datos_seguros; DELETE FROM datos_seguros;",
        "",
        "   ",
        "EXEC sp_executesql N'SELECT 1'",
        "SELECT * FROM (SELECT 1) WHERE",
    ];

    for payload in payloads_adversariales {
        let res = engine.ejecutar_sql(payload).await;
        assert!(
            res.is_err(),
            "El payload malicioso o inválido debió ser rechazado: '{}'",
            payload
        );
        match res.unwrap_err() {
            DomainError::QueryError(msg) => {
                assert!(
                    !msg.is_empty(),
                    "El mensaje de error no debe ser vacío para: '{}'",
                    payload
                );
            }
            other => panic!("Tipo de error inesperado: {:?}", other),
        }
    }
}

#[tokio::test]
async fn test_adv_aislamiento_estricto_anti_cross_contamination() {
    let engine = DataFusionEngine::new();
    let centro_confidencial = CenterId::new("planta_nuclear_secreta").unwrap();
    let centro_publico = CenterId::new("planta_agua_potable").unwrap();

    let batch_secreto = crear_lote_base_seguridad();
    let batch_publico = crear_lote_base_seguridad();

    engine
        .registrar_centro(&centro_confidencial, "inventario", vec![batch_secreto])
        .await
        .unwrap();
    engine
        .registrar_centro(&centro_publico, "inventario", vec![batch_publico])
        .await
        .unwrap();

    // Intentar acceder a la tabla confidencial desde una consulta que busca la pública
    let tablas = engine.listar_tablas_registradas().await.unwrap();
    assert_eq!(tablas.len(), 2);
    assert!(tablas.contains(&"planta_nuclear_secreta_inventario".to_string()));
    assert!(tablas.contains(&"planta_agua_potable_inventario".to_string()));

    // Desregistrar centro confidencial
    engine
        .desregistrar_centro(&centro_confidencial)
        .await
        .unwrap();

    // Comprobar que no queda residuo de la tabla confidencial
    let tablas_post = engine.listar_tablas_registradas().await.unwrap();
    assert_eq!(tablas_post.len(), 1);
    assert!(!tablas_post.contains(&"planta_nuclear_secreta_inventario".to_string()));

    let query_ataque = engine
        .ejecutar_sql("SELECT * FROM planta_nuclear_secreta_inventario")
        .await;
    assert!(
        query_ataque.is_err(),
        "No debe ser posible acceder a datos de un centro desregistrado"
    );
}

#[tokio::test]
async fn test_adv_lookup_resistencia_a_nan_infinito_y_valores_extremos() {
    let service = TableLookupService::new();
    let doc_id = DocId::new("DOC-PRUEBA").unwrap();
    let rango_finito = RangoNumerico::entre_inclusivo(0.0, 100.0).unwrap();

    let celda = CeldaDobleEntrada::new(
        "C-ADV",
        doc_id,
        "TABLA_LIMITES",
        "Material Test",
        None,
        "Col Test",
        Some(rango_finito),
        "Valor Valido",
        None,
        vec![],
    )
    .unwrap();

    service.registrar_celdas(vec![celda]).unwrap();

    // Consultas con valores numéricos anómalos no deben causar pánico ni coincidir falsamente
    let valores_adversariales = [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        f64::MIN,
        -0.000000000001,
        100.00000000001,
    ];

    for val in valores_adversariales {
        let res = service
            .buscar_celda(
                "TABLA_LIMITES",
                Some("Material Test"),
                None,
                None,
                Some(val),
            )
            .await
            .expect("La búsqueda con valores anómalos no debe fallar con pánico");

        assert!(
            res.is_none(),
            "El valor anómalo {:?} no debió coincidir con el rango [0, 100]",
            val
        );
    }
}

#[tokio::test]
async fn test_adv_petgraph_resistencia_a_bucles_de_derogacion_paradojicos() {
    let engine = PetgraphEngine::new();

    let doc_a = DocId::new("NORMA-A").unwrap();
    let doc_b = DocId::new("NORMA-B").unwrap();
    let doc_c = DocId::new("NORMA-C").unwrap();

    // Bucle circular paradójico:
    // A deroga a B
    // B deroga a C
    // C deroga a A
    let edge_ab = AristaGrafo::new(
        "E-AB",
        doc_a.clone(),
        "Sec 1",
        doc_b.clone(),
        "Sec 1",
        TipoRelacionNormativa::DerogaA,
        1.0,
        None,
    )
    .unwrap();

    let edge_bc = AristaGrafo::new(
        "E-BC",
        doc_b.clone(),
        "Sec 1",
        doc_c.clone(),
        "Sec 1",
        TipoRelacionNormativa::DerogaA,
        1.0,
        None,
    )
    .unwrap();

    let edge_ca = AristaGrafo::new(
        "E-CA",
        doc_c.clone(),
        "Sec 1",
        doc_a.clone(),
        "Sec 1",
        TipoRelacionNormativa::DerogaA,
        1.0,
        None,
    )
    .unwrap();

    engine
        .cargar_aristas(&[edge_ab, edge_bc, edge_ca])
        .await
        .unwrap();

    // 1. Detección explícita de ciclos
    let ciclos = engine.detectar_ciclos().await.unwrap();
    assert_eq!(ciclos.len(), 1, "Debe detectar exactamente un ciclo");
    assert_eq!(ciclos[0].len(), 3, "El ciclo debe contener los 3 nodos");

    // 2. Intento de resolver norma prevalente en un bucle cerrado
    // Debe devolver DomainError::GraphError en lugar de desbordar la pila en loop infinito
    let res = engine.obtener_norma_prevalente(&doc_a).await;
    assert!(
        res.is_err(),
        "Debe fallar ante un bucle cerrado de derogación"
    );
    match res.unwrap_err() {
        DomainError::GraphError(msg) => {
            assert!(
                msg.contains("Ciclo o paradoja de derogación detectada"),
                "Mensaje de error debe describir la paradoja: {}",
                msg
            );
        }
        other => panic!("Tipo de error inesperado: {:?}", other),
    }
}

#[tokio::test]
async fn test_adv_consultas_sobre_grafo_vacio_o_nodos_inexistentes() {
    let engine = PetgraphEngine::new();

    // Grafo completamente vacío
    let (nodos, aristas) = engine.estadisticas_grafo().await.unwrap();
    assert_eq!(nodos, 0);
    assert_eq!(aristas, 0);

    let doc_fantasma = DocId::new("DOC-INEXISTENTE").unwrap();

    // Si consultamos prevalente de un documento aislado o inexistente, él mismo es su prevalente
    let prevalente = engine
        .obtener_norma_prevalente(&doc_fantasma)
        .await
        .unwrap();
    assert_eq!(prevalente, doc_fantasma);

    // Linaje de un documento sin referencias
    let linaje = engine.obtener_linaje(&doc_fantasma).await.unwrap();
    assert_eq!(linaje.raiz, doc_fantasma);
    assert_eq!(linaje.prevalente, doc_fantasma);
    assert!(linaje.referencias_directas.is_empty());
    assert_eq!(linaje.camino_derogacion, vec![doc_fantasma]);
}
