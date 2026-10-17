//! Pruebas de integración del Motor Analítico (Apache DataFusion) y Grafo Normativo (Petgraph)

use arrow_array::{Int64Array, RecordBatch, StringArray};
use lakehouse_rag::application::dtos::lakehouse_filas::{
    ClaveDocumental, FilaArista, FilaCelda, FilaHito, LimitesEje, LoteIngesta, TipoNodoGrafo,
};
use lakehouse_rag::application::ports::analytical_engine::AnalyticalEnginePort;
use lakehouse_rag::application::ports::graph_engine::GraphEnginePort;
use lakehouse_rag::application::ports::lakehouse_store::EstadoIngesta;
use lakehouse_rag::application::ports::table_lookup::TableLookupPort;
use lakehouse_rag::application::use_cases::table_lookup_service::TableLookupService;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::{DocId, TipoDocumento};
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::domain::models::operational_matrix::{CalidadExtraccion, FaseOperativa};
use lakehouse_rag::domain::models::reference_graph::TipoRelacionNormativa;
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::query_engine::petgraph_engine::PetgraphEngine;
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

fn celda(
    id: &str,
    categoria_x: &str,
    minimo_y: f64,
    maximo_y: f64,
    minimo_y_inclusivo: bool,
    valor: &str,
    nota: &str,
) -> FilaCelda {
    FilaCelda {
        celda_id: id.to_string(),
        tabla_id: "TABLA_TEMPERATURAS".to_string(),
        clave: clave("planta_norte", "DOC-001"),
        eje_x: LimitesEje {
            categoria: Some(categoria_x.to_string()),
            ..LimitesEje::default()
        },
        eje_y: LimitesEje {
            minimo: Some(minimo_y),
            minimo_inclusivo: minimo_y_inclusivo,
            maximo: Some(maximo_y),
            maximo_inclusivo: true,
            ..LimitesEje::default()
        },
        eje_z_filtro_adicional: None,
        valor_resultado_texto: valor.to_string(),
        valor_resultado_num: valor.parse().ok(),
        unidad_medida: Some("mm".to_string()),
        nota_pie_condicion: Some(nota.to_string()),
        nivel_prelacion: TipoDocumento::ProcedimientoObra.nivel_prelacion(),
    }
}

fn arista(id: &str, origen: &str, relacion: TipoRelacionNormativa, destino: &str) -> FilaArista {
    FilaArista {
        edge_id: id.to_string(),
        clave: clave("planta_norte", origen),
        origen_id: origen.to_string(),
        origen_tipo: TipoNodoGrafo::Documento,
        tipo_relacion: relacion,
        destino_id: destino.to_string(),
        destino_tipo: TipoNodoGrafo::destino_de(relacion),
        contexto_relacion: None,
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

#[tokio::test]
async fn test_integration_table_lookup_hidratacion_arrow_y_resolucion_exacta() {
    let service = TableLookupService::new();

    // Eje X categórico (material) y eje Y numérico continuo (temperatura en °C)
    // C-101: [0.0, 100.0]
    // C-102: (100.0, 300.0]
    // C-103: [0.0, 300.0]
    let batch = lote_confirmado(LoteIngesta::Celdas(vec![
        celda(
            "C-101",
            "Acero al Carbono",
            0.0,
            100.0,
            true,
            "5.5",
            "Espesor estándar para baja temperatura",
        ),
        celda(
            "C-102",
            "Acero al Carbono",
            100.0,
            300.0,
            false,
            "8.2",
            "Requiere revestimiento térmico exterior",
        ),
        celda(
            "C-103",
            "Acero Inoxidable 316",
            0.0,
            300.0,
            true,
            "4.0",
            "Apto para ambientes salinos",
        ),
    ]));

    // Hidratar desde RecordBatch
    let total_cargadas = service.cargar_desde_record_batch(&batch).unwrap();
    assert_eq!(total_cargadas, 3);

    // Consulta 1: Acero al Carbono a 50°C -> debe caer en C-101
    let celda_50 = service
        .buscar_celda(
            "TABLA_TEMPERATURAS",
            Some("Acero al Carbono"),
            None,
            None,
            Some(50.0),
        )
        .await
        .unwrap()
        .expect("Debe encontrar celda");
    assert_eq!(celda_50.valor_resultado, "5.5");
    assert_eq!(celda_50.id, "C-101");

    // Consulta 2: Límite frontera exacto 100°C -> por inclusión [0.0, 100.0] corresponde a C-101
    let celda_100 = service
        .buscar_celda(
            "TABLA_TEMPERATURAS",
            Some("Acero al Carbono"),
            None,
            None,
            Some(100.0),
        )
        .await
        .unwrap()
        .expect("Debe encontrar celda en límite 100");
    assert_eq!(celda_100.id, "C-101");

    // Consulta 3: 100.1°C -> debe saltar a C-102 ((100.0, 300.0])
    let celda_100_1 = service
        .buscar_celda(
            "TABLA_TEMPERATURAS",
            Some("Acero al Carbono"),
            None,
            None,
            Some(100.1),
        )
        .await
        .unwrap()
        .expect("Debe encontrar celda en rango superior");
    assert_eq!(celda_100_1.id, "C-102");
    assert_eq!(celda_100_1.valor_resultado, "8.2");

    // Consulta 4: Acero Inoxidable a 200°C -> debe ser C-103
    let celda_inox = service
        .buscar_celda(
            "TABLA_TEMPERATURAS",
            Some("Acero Inoxidable 316"),
            None,
            None,
            Some(200.0),
        )
        .await
        .unwrap()
        .expect("Debe encontrar celda de acero inoxidable");
    assert_eq!(celda_inox.id, "C-103");
}

#[tokio::test]
async fn test_integration_petgraph_hidratacion_arrow_y_resolucion_prevalencia() {
    let engine = PetgraphEngine::new();

    // Caso de linaje normativo industrial:
    // DOC-LEY-2010 derogado por DOC-LEY-2018 y este a su vez derogado por DOC-LEY-2024
    // DOC-LEY-2024 a su vez referencia a DOC-ISO-9001
    let batch = lote_confirmado(LoteIngesta::Aristas(vec![
        arista(
            "E-01",
            "DOC-LEY-2018",
            TipoRelacionNormativa::DerogaA,
            "DOC-LEY-2010",
        ),
        arista(
            "E-02",
            "DOC-LEY-2024",
            TipoRelacionNormativa::DerogaA,
            "DOC-LEY-2018",
        ),
        arista(
            "E-03",
            "DOC-LEY-2024",
            TipoRelacionNormativa::ReferenciaA,
            "DOC-ISO-9001",
        ),
    ]));

    let cargadas = engine.cargar_desde_record_batch(&batch).await.unwrap();
    assert_eq!(cargadas, 3);

    let (nodos, aristas) = engine.estadisticas_grafo().await.unwrap();
    assert_eq!(nodos, 4); // LEY-2010, LEY-2018, LEY-2024, ISO-9001
    assert_eq!(aristas, 3);

    // Resolución de prevalencia:
    // Preguntamos por la norma derogada de 2010 -> debe devolver la vigente prevalente LEY-2024
    let doc_2010 = DocId::new("DOC-LEY-2010").unwrap();
    let prevalente = engine.obtener_norma_prevalente(&doc_2010).await.unwrap();
    assert_eq!(prevalente.as_str(), "DOC-LEY-2024");

    // Linaje completo de 2024
    let doc_2024 = DocId::new("DOC-LEY-2024").unwrap();
    let linaje_2024 = engine.obtener_linaje(&doc_2024).await.unwrap();
    assert_eq!(linaje_2024.prevalente.as_str(), "DOC-LEY-2024");
    assert!(
        linaje_2024
            .referencias_directas
            .iter()
            .any(|(d, t)| d.as_str() == "DOC-ISO-9001" && *t == TipoRelacionNormativa::ReferenciaA)
    );
}
