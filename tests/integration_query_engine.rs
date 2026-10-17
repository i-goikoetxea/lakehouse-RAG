//! Pruebas de integración del Motor Analítico (Apache DataFusion) y Grafo Normativo (Petgraph)

use std::sync::Arc;

use arrow_array::{
    Array, BooleanArray, Float32Array, Float64Array, Int64Array, RecordBatch, StringArray,
};
use lakehouse_rag::application::ports::analytical_engine::AnalyticalEnginePort;
use lakehouse_rag::application::ports::graph_engine::GraphEnginePort;
use lakehouse_rag::application::ports::table_lookup::TableLookupPort;
use lakehouse_rag::application::use_cases::table_lookup_service::TableLookupService;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::reference_graph::TipoRelacionNormativa;
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::query_engine::petgraph_engine::PetgraphEngine;
use lakehouse_rag::infrastructure::storage::schemas::{
    schema_double_entry_cells, schema_operational_matrix, schema_reference_graph,
};

#[tokio::test]
async fn test_integration_datafusion_consultas_analiticas_sobre_matriz_operativa() {
    let engine = DataFusionEngine::new();
    let schema = schema_operational_matrix();

    let tupla_ids = Arc::new(StringArray::from(vec!["T-01", "T-02", "T-03", "T-04"]));
    let doc_ids = Arc::new(StringArray::from(vec![
        "DOC-EXP-01",
        "DOC-EXP-01",
        "DOC-EXP-02",
        "DOC-EXP-02",
    ]));
    let centros = Arc::new(StringArray::from(vec![
        "planta_central",
        "planta_central",
        "planta_central",
        "planta_central",
    ]));
    let fases = Arc::new(StringArray::from(vec![
        "Inspeccion",
        "Recepcion",
        "Inspeccion",
        "Custodia",
    ]));
    let roles = Arc::new(StringArray::from(vec![
        "Inspector Calidad",
        "Operador Recepcion",
        "Supervisor Ensayos",
        "Custodio Almacen",
    ]));
    let acciones = Arc::new(StringArray::from(vec![
        "Medicion ultrasonica",
        "Verificacion visual",
        "Prueba hidrostatica",
        "Control de precinto",
    ]));
    let parametros = Arc::new(StringArray::from(vec![
        "Espesor pared",
        "Integridad sello",
        "Presion sostenida",
        "Estado precinto",
    ]));
    let tolerancias = Arc::new(StringArray::from(vec![
        ">= 10.5 mm",
        "Sin fisuras",
        "150 psi +/- 5",
        "Intacto",
    ]));
    let condiciones = Arc::new(StringArray::from(vec![
        "Tuberias principales",
        "Llegada a puerto",
        "Previo a despacho",
        "En almacenamiento",
    ]));
    let referencias = Arc::new(StringArray::from(vec![
        "Norma ASTM A106",
        "Procedimiento P-01",
        "Norma ASME B31.3",
        "Procedimiento P-04",
    ]));
    let vigentes = Arc::new(BooleanArray::from(vec![true, true, true, true]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            tupla_ids,
            doc_ids,
            centros,
            fases,
            roles,
            acciones,
            parametros,
            tolerancias,
            condiciones,
            referencias,
            vigentes,
        ],
    )
    .expect("Construir RecordBatch");

    // Registrar en DataFusion
    engine
        .registrar_tabla("matriz_operativa", vec![batch])
        .await
        .expect("Registrar tabla");

    // 1. Consulta con filtro WHERE y proyección de columnas
    let resultado_inspeccion = engine
        .ejecutar_sql("SELECT tupla_id, rol_responsable, parametro_control FROM matriz_operativa WHERE fase_operativa = 'Inspeccion' ORDER BY tupla_id ASC")
        .await
        .expect("Ejecutar consulta SELECT");

    assert_eq!(resultado_inspeccion.len(), 1);
    let batch_res = &resultado_inspeccion[0];
    assert_eq!(batch_res.num_rows(), 2);

    let tuplas_res = batch_res
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(tuplas_res.value(0), "T-01");
    assert_eq!(tuplas_res.value(1), "T-03");

    // 2. Consulta con agregación GROUP BY
    let resultado_agregacion = engine
        .ejecutar_sql("SELECT fase_operativa, COUNT(*) as total FROM matriz_operativa GROUP BY fase_operativa ORDER BY total DESC, fase_operativa ASC")
        .await
        .expect("Ejecutar agregación SQL");

    assert_eq!(resultado_agregacion.len(), 1);
    let batch_agg = &resultado_agregacion[0];
    assert_eq!(batch_agg.num_rows(), 3); // Inspeccion (2), Custodia (1), Recepcion (1)

    let total_col = batch_agg
        .column(1)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert_eq!(total_col.value(0), 2); // Inspeccion tiene 2
}

#[tokio::test]
async fn test_integration_datafusion_aislamiento_estricto_entre_centros() {
    let engine = DataFusionEngine::new();
    let schema = schema_operational_matrix();

    let centro_alfa = CenterId::new("planta_alfa").unwrap();
    let centro_beta = CenterId::new("planta_beta").unwrap();

    let batch_alfa = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(vec!["T-ALFA-01"])),
            Arc::new(StringArray::from(vec!["DOC-A1"])),
            Arc::new(StringArray::from(vec!["planta_alfa"])),
            Arc::new(StringArray::from(vec!["Recepcion"])),
            Arc::new(StringArray::from(vec!["Operador A"])),
            Arc::new(StringArray::from(vec!["Inspeccion A"])),
            Arc::new(StringArray::from(vec!["Param A"])),
            Arc::new(StringArray::from(vec!["Tol A"])),
            Arc::new(StringArray::from(vec!["Cond A"])),
            Arc::new(StringArray::from(vec!["Ref A"])),
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .unwrap();

    let batch_beta = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec!["T-BETA-01"])),
            Arc::new(StringArray::from(vec!["DOC-B1"])),
            Arc::new(StringArray::from(vec!["planta_beta"])),
            Arc::new(StringArray::from(vec!["Custodia"])),
            Arc::new(StringArray::from(vec!["Operador B"])),
            Arc::new(StringArray::from(vec!["Inspeccion B"])),
            Arc::new(StringArray::from(vec!["Param B"])),
            Arc::new(StringArray::from(vec!["Tol B"])),
            Arc::new(StringArray::from(vec!["Cond B"])),
            Arc::new(StringArray::from(vec!["Ref B"])),
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .unwrap();

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
        .ejecutar_sql("SELECT tupla_id FROM planta_alfa_matriz")
        .await
        .unwrap();
    assert_eq!(res_alfa[0].num_rows(), 1);

    // Desregistrar centro alfa
    engine.desregistrar_centro(&centro_alfa).await.unwrap();

    // Ahora consultar alfa debe fallar porque ya no está registrada
    let err_alfa = engine
        .ejecutar_sql("SELECT tupla_id FROM planta_alfa_matriz")
        .await;
    assert!(err_alfa.is_err(), "La tabla de alfa ya no debe existir");

    // Pero la de beta sigue activa e intacta
    let res_beta = engine
        .ejecutar_sql("SELECT tupla_id FROM planta_beta_matriz")
        .await
        .unwrap();
    assert_eq!(res_beta[0].num_rows(), 1);
}

#[tokio::test]
async fn test_integration_table_lookup_hidratacion_arrow_y_resolucion_exacta() {
    let service = TableLookupService::new();
    let schema = schema_double_entry_cells();

    // Creamos lote Arrow de celdas técnicas con rangos continuos
    let celda_ids = Arc::new(StringArray::from(vec!["C-101", "C-102", "C-103"]));
    let tabla_ids = Arc::new(StringArray::from(vec![
        "TABLA_TEMPERATURAS",
        "TABLA_TEMPERATURAS",
        "TABLA_TEMPERATURAS",
    ]));
    let doc_ids = Arc::new(StringArray::from(vec!["DOC-001", "DOC-001", "DOC-001"]));
    let centro_ids = Arc::new(StringArray::from(vec![
        "planta_norte",
        "planta_norte",
        "planta_norte",
    ]));

    // Rango X: Categórico (Material)
    let min_x = Arc::new(Float64Array::from(vec![None, None, None]));
    let max_x = Arc::new(Float64Array::from(vec![None, None, None]));
    let x_inc_min = Arc::new(BooleanArray::from(vec![false, false, false]));
    let x_inc_max = Arc::new(BooleanArray::from(vec![false, false, false]));
    let cat_x = Arc::new(StringArray::from(vec![
        Some("Acero al Carbono"),
        Some("Acero al Carbono"),
        Some("Acero Inoxidable 316"),
    ]));

    // Rango Y: Numérico continuo (Temperatura en °C)
    // C-101: [0.0, 100.0]
    // C-102: (100.0, 300.0]
    // C-103: [0.0, 300.0]
    let min_y = Arc::new(Float64Array::from(vec![Some(0.0), Some(100.0), Some(0.0)]));
    let max_y = Arc::new(Float64Array::from(vec![
        Some(100.0),
        Some(300.0),
        Some(300.0),
    ]));
    let y_inc_min = Arc::new(BooleanArray::from(vec![true, false, true]));
    let y_inc_max = Arc::new(BooleanArray::from(vec![true, true, true]));
    let cat_y = Arc::new(StringArray::from(vec![None as Option<&str>, None, None]));

    let valor_res = Arc::new(StringArray::from(vec!["5.5", "8.2", "4.0"]));
    let unidad = Arc::new(StringArray::from(vec![Some("mm"), Some("mm"), Some("mm")]));
    let notas = Arc::new(StringArray::from(vec![
        Some("Espesor estándar para baja temperatura"),
        Some("Requiere revestimiento térmico exterior"),
        Some("Apto para ambientes salinos"),
    ]));
    let vigentes = Arc::new(BooleanArray::from(vec![true, true, true]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            celda_ids, tabla_ids, doc_ids, centro_ids, min_x, max_x, x_inc_min, x_inc_max, cat_x,
            min_y, max_y, y_inc_min, y_inc_max, cat_y, valor_res, unidad, notas, vigentes,
        ],
    )
    .expect("Crear lote celdas");

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
    let schema = schema_reference_graph();

    // Caso de linaje normativo industrial:
    // DOC-LEY-2010 derogado por DOC-LEY-2018 y este a su vez derogado por DOC-LEY-2024
    // DOC-LEY-2024 a su vez referencia a DOC-ESTANDAR-ISO
    let arista_ids = Arc::new(StringArray::from(vec!["E-01", "E-02", "E-03"]));
    let centro_ids = Arc::new(StringArray::from(vec![
        "planta_norte",
        "planta_norte",
        "planta_norte",
    ]));
    let origenes = Arc::new(StringArray::from(vec![
        "DOC-LEY-2018",
        "DOC-LEY-2024",
        "DOC-LEY-2024",
    ]));
    let destinos = Arc::new(StringArray::from(vec![
        "DOC-LEY-2010",
        "DOC-LEY-2018",
        "DOC-ISO-9001",
    ]));
    let tipos = Arc::new(StringArray::from(vec!["Deroga", "Deroga", "Referencia"]));
    let pesos = Arc::new(Float32Array::from(vec![1.0, 1.0, 0.5]));
    let citas = Arc::new(StringArray::from(vec![
        Some("Disposicion transitoria primera"),
        Some("Articulo derogatorio unico"),
        Some("Capitulo de gestion de calidad"),
    ]));
    let vigentes = Arc::new(BooleanArray::from(vec![true, true, true]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            arista_ids, centro_ids, origenes, destinos, tipos, pesos, citas, vigentes,
        ],
    )
    .expect("Construir RecordBatch grafo");

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
