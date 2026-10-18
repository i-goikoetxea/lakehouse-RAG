//! Pruebas de integración del Motor Analítico (Apache DataFusion)

use arrow_array::{Int64Array, RecordBatch, StringArray};
use lakehouse_rag::application::dtos::lakehouse_filas::{ClaveDocumental, FilaHito, LoteIngesta};
use lakehouse_rag::application::ports::analytical_engine::AnalyticalEnginePort;
use lakehouse_rag::application::ports::lakehouse_store::EstadoIngesta;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::{DocId, TipoDocumento};
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::domain::models::operational_matrix::{CalidadExtraccion, FaseOperativa};
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::storage::lotes_arrow::{
    ColumnasTransaccionales, lote_a_record_batch,
};

fn clave(centro: &str, doc_id: &str) -> ClaveDocumental {
    ClaveDocumental {
        centro_id: CenterId::new(centro).unwrap(),
        doc_id: DocId::new(doc_id).unwrap(),
        version_num: 1,
    }
}

/// Convierte un lote tipado en un `RecordBatch` confirmado y vigente
fn lote_confirmado(lote: LoteIngesta) -> RecordBatch {
    let transaccion = ColumnasTransaccionales {
        ingest_id: IngestId::new("ING-QUERY-01").unwrap(),
        estado: EstadoIngesta::Committed,
        vigente: true,
    };
    lote_a_record_batch(&lote, &transaccion).expect("Construir RecordBatch v2")
}

fn hito(
    centro: &str,
    doc_id: &str,
    idx: u32,
    fase: FaseOperativa,
    rol_ejecutor: &str,
    criterio: &str,
) -> FilaHito {
    FilaHito {
        hito_id: format!("{doc_id}@v1-HIT-{idx:04}"),
        clave: clave(centro, doc_id),
        orden_secuencia: idx + 1,
        fase_operativa: fase,
        objeto_alcance: "Tubería principal".to_string(),
        rol_ejecutor: rol_ejecutor.to_string(),
        rol_responsable: "Jefe de Calidad".to_string(),
        registro_codigo: None,
        registro_nombre: None,
        criterio_aceptacion: criterio.to_string(),
        condicion_aplicacion: None,
        nivel_prelacion: TipoDocumento::ProcedimientoObra.nivel_prelacion(),
        calidad_extraccion: CalidadExtraccion::Ok,
        chunk_origen_id: None,
        vector_hito: None,
    }
}

#[tokio::test]
async fn test_integration_datafusion_consultas_analiticas_sobre_matriz_operativa() {
    let engine = DataFusionEngine::new();

    let batch = lote_confirmado(LoteIngesta::Hitos(vec![
        hito(
            "planta_central",
            "DOC-EXP-01",
            0,
            FaseOperativa::Inspeccion,
            "Inspector Calidad",
            ">= 10.5 mm",
        ),
        hito(
            "planta_central",
            "DOC-EXP-01",
            1,
            FaseOperativa::Recepcion,
            "Operador Recepcion",
            "Sin fisuras",
        ),
        hito(
            "planta_central",
            "DOC-EXP-02",
            0,
            FaseOperativa::Inspeccion,
            "Supervisor Ensayos",
            "150 psi +/- 5",
        ),
        hito(
            "planta_central",
            "DOC-EXP-02",
            1,
            FaseOperativa::Custodia,
            "Custodio Almacen",
            "Intacto",
        ),
    ]));

    // Registrar en DataFusion
    engine
        .registrar_tabla("matriz_operativa", vec![batch])
        .await
        .expect("Registrar tabla");

    // 1. Consulta con filtro WHERE y proyección de columnas
    let resultado_inspeccion = engine
        .ejecutar_sql("SELECT hito_id, rol_ejecutor, criterio_aceptacion FROM matriz_operativa WHERE fase_operativa = 'INSPECCION' ORDER BY hito_id ASC")
        .await
        .expect("Ejecutar consulta SELECT");

    assert_eq!(resultado_inspeccion.len(), 1);
    let batch_res = &resultado_inspeccion[0];
    assert_eq!(batch_res.num_rows(), 2);

    let hitos_res = batch_res
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(hitos_res.value(0), "DOC-EXP-01@v1-HIT-0000");
    assert_eq!(hitos_res.value(1), "DOC-EXP-02@v1-HIT-0000");

    // 2. Consulta con agregación GROUP BY
    let resultado_agregacion = engine
        .ejecutar_sql("SELECT fase_operativa, COUNT(*) as total FROM matriz_operativa GROUP BY fase_operativa ORDER BY total DESC, fase_operativa ASC")
        .await
        .expect("Ejecutar agregación SQL");

    assert_eq!(resultado_agregacion.len(), 1);
    let batch_agg = &resultado_agregacion[0];
    assert_eq!(batch_agg.num_rows(), 3); // INSPECCION (2), CUSTODIA (1), RECEPCION (1)

    let total_col = batch_agg
        .column(1)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert_eq!(total_col.value(0), 2); // INSPECCION tiene 2
}

#[tokio::test]
async fn test_integration_datafusion_aislamiento_estricto_entre_centros() {
    let engine = DataFusionEngine::new();

    let centro_alfa = CenterId::new("planta_alfa").unwrap();
    let centro_beta = CenterId::new("planta_beta").unwrap();

    let batch_alfa = lote_confirmado(LoteIngesta::Hitos(vec![hito(
        "planta_alfa",
        "DOC-A1",
        0,
        FaseOperativa::Recepcion,
        "Operador A",
        "Criterio A",
    )]));
    let batch_beta = lote_confirmado(LoteIngesta::Hitos(vec![hito(
        "planta_beta",
        "DOC-B1",
        0,
        FaseOperativa::Custodia,
        "Operador B",
        "Criterio B",
    )]));

    engine
        .registrar_centro(&centro_alfa, "matriz", vec![batch_alfa])
        .await
        .unwrap();
    engine
        .registrar_centro(&centro_beta, "matriz", vec![batch_beta])
        .await
        .unwrap();

    // Consultar tabla de alfa
    let res_alfa = engine
        .ejecutar_sql("SELECT hito_id FROM planta_alfa_matriz")
        .await
        .unwrap();
    assert_eq!(res_alfa[0].num_rows(), 1);

    // Desregistrar centro alfa
    engine.desregistrar_centro(&centro_alfa).await.unwrap();

    // Ahora consultar alfa debe fallar porque ya no está registrada
    let err_alfa = engine
        .ejecutar_sql("SELECT hito_id FROM planta_alfa_matriz")
        .await;
    assert!(err_alfa.is_err(), "La tabla de alfa ya no debe existir");

    // Pero la de beta sigue activa e intacta
    let res_beta = engine
        .ejecutar_sql("SELECT hito_id FROM planta_beta_matriz")
        .await
        .unwrap();
    assert_eq!(res_beta[0].num_rows(), 1);
}
