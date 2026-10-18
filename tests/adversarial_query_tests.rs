//! Pruebas Adversariales de Seguridad y Resiliencia para el Motor Analítico (DataFusion)
//!
//! Cubre: Inyecciones SQL maliciosas y aislamiento estricto multi-centro.

use std::sync::Arc;

use arrow_array::{Int32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use lakehouse_rag::application::ports::analytical_engine::AnalyticalEnginePort;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;

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
