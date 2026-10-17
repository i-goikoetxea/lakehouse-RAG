//! Pruebas de integración E2E para el Servidor de Herramientas y Agente (Fase 6)

use std::sync::Arc;

use arrow_array::{BooleanArray, Float64Array, Int8Array, Int32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use serde_json::json;

use lakehouse_rag::application::dtos::agent::{AgentMessage, ClaudeModel};
use lakehouse_rag::application::dtos::tools::{LakehouseToolName, ToolCall};
use lakehouse_rag::application::ports::analytical_engine::AnalyticalEnginePort;
use lakehouse_rag::application::ports::graph_engine::GraphEnginePort;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::application::use_cases::lakehouse_tool_service::LakehouseToolService;
use lakehouse_rag::application::use_cases::table_lookup_service::TableLookupService;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::double_entry_table::{
    CeldaDobleEntrada, LimiteRango, RangoNumerico,
};
use lakehouse_rag::domain::models::reference_graph::{AristaGrafo, TipoRelacionNormativa};
use lakehouse_rag::infrastructure::llm::mock_agent_runner::MockAgentRunner;
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::query_engine::petgraph_engine::PetgraphEngine;

fn crear_lote_catalogo_prueba() -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("titulo", DataType::Utf8, false),
        Field::new("tipo", DataType::Utf8, false),
        Field::new("nivel_prelacion", DataType::Int8, false),
        Field::new("version", DataType::Int32, false),
        Field::new("fecha_emision", DataType::Utf8, false),
        Field::new("vigente", DataType::Boolean, false),
    ]));

    let doc_ids = Arc::new(StringArray::from(vec!["DOC-001", "DOC-002"]));
    let titulos = Arc::new(StringArray::from(vec![
        "Especificación Acero",
        "Procedimiento Soldadura Obsoleto",
    ]));
    let tipos = Arc::new(StringArray::from(vec!["Norma", "Procedimiento"]));
    let prelaciones = Arc::new(Int8Array::from(vec![1, 3]));
    let versiones = Arc::new(Int32Array::from(vec![2, 1]));
    let fechas = Arc::new(StringArray::from(vec!["2026-03-01", "2024-01-01"]));
    let vigentes = Arc::new(BooleanArray::from(vec![true, false]));

    RecordBatch::try_new(
        schema,
        vec![
            doc_ids,
            titulos,
            tipos,
            prelaciones,
            versiones,
            fechas,
            vigentes,
        ],
    )
    .unwrap()
}

fn crear_lote_matriz_operativa_prueba() -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("fase_operativa", DataType::Utf8, false),
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("parametro_tecnico", DataType::Utf8, false),
        Field::new("valor_nominal", DataType::Float64, false),
        Field::new("valor_minimo", DataType::Float64, true),
        Field::new("valor_maximo", DataType::Float64, true),
        Field::new("unidad_medida", DataType::Utf8, false),
        Field::new("condicion_aplicacion", DataType::Utf8, true),
        Field::new("vigente", DataType::Boolean, false),
    ]));

    let fases = Arc::new(StringArray::from(vec!["Recepcion", "Inspeccion"]));
    let doc_ids = Arc::new(StringArray::from(vec!["DOC-001", "DOC-001"]));
    let params = Arc::new(StringArray::from(vec![
        "Temperatura Almacén",
        "Presión Hidráulica",
    ]));
    let nom = Arc::new(Float64Array::from(vec![20.0, 150.0]));
    let min = Arc::new(Float64Array::from(vec![Some(15.0), Some(140.0)]));
    let max = Arc::new(Float64Array::from(vec![Some(25.0), Some(160.0)]));
    let unidades = Arc::new(StringArray::from(vec!["°C", "bar"]));
    let conds = Arc::new(StringArray::from(vec![
        Some("Ambiente controlado"),
        Some("Carga máxima"),
    ]));
    let vigentes = Arc::new(BooleanArray::from(vec![true, true]));

    RecordBatch::try_new(
        schema,
        vec![
            fases, doc_ids, params, nom, min, max, unidades, conds, vigentes,
        ],
    )
    .unwrap()
}

#[tokio::test]
async fn test_integration_e2e_las_5_tools_del_lakehouse() {
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let petgraph = Arc::new(PetgraphEngine::new());

    let centro = CenterId::new("planta_norte").unwrap();

    // 1. Poblado DataFusion
    datafusion
        .registrar_centro(&centro, "doc_catalog", vec![crear_lote_catalogo_prueba()])
        .await
        .unwrap();
    datafusion
        .registrar_centro(
            &centro,
            "operational_matrix",
            vec![crear_lote_matriz_operativa_prueba()],
        )
        .await
        .unwrap();

    // 2. Poblado TableLookup
    let doc_id = DocId::new("DOC-001").unwrap();
    let rango_x =
        RangoNumerico::new(LimiteRango::Inclusivo(10.0), LimiteRango::Exclusivo(20.0)).unwrap();
    let celda = CeldaDobleEntrada::new(
        "celda_1",
        doc_id.clone(),
        "tabla_soldadura",
        "Espesor 10-20mm",
        Some(rango_x),
        "Acero Inox",
        None,
        "Electrodo E-308",
        Some("tipo".to_string()),
        vec!["Nota técnica 1: Precalentar a 100°C".to_string()],
    )
    .unwrap();
    lookup.registrar_celdas(vec![celda]).unwrap();

    // 3. Poblado Petgraph
    let doc_nuevo = DocId::new("DOC-003").unwrap();
    let arista = AristaGrafo::new(
        "arista_deroga",
        doc_nuevo.clone(),
        "sec_nueva",
        doc_id.clone(),
        "sec_general",
        TipoRelacionNormativa::DerogaA,
        1.0,
        Some("Deroga norma previa por actualización tecnológica".to_string()),
    )
    .unwrap();
    petgraph.cargar_aristas(&[arista]).await.unwrap();

    let tool_service = LakehouseToolService::new(datafusion, lookup, petgraph);

    // --- TOOL 1: inspect_lakehouse_catalog ---
    let call_1 = ToolCall {
        id: "call_t1".to_string(),
        name: LakehouseToolName::InspectLakehouseCatalog
            .as_str()
            .to_string(),
        arguments: json!({ "centro_id": "planta_norte", "solo_vigentes": true }),
    };
    let res_1 = tool_service.ejecutar_herramienta(&call_1).await.unwrap();
    assert!(!res_1.is_error);
    let docs = res_1.output.as_array().unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0]["doc_id"], "DOC-001");

    // --- TOOL 2: query_operational_matrix ---
    let call_2 = ToolCall {
        id: "call_t2".to_string(),
        name: LakehouseToolName::QueryOperationalMatrix
            .as_str()
            .to_string(),
        arguments: json!({ "centro_id": "planta_norte", "fase": "Recepcion" }),
    };
    let res_2 = tool_service.ejecutar_herramienta(&call_2).await.unwrap();
    assert!(!res_2.is_error);
    let params = res_2.output.as_array().unwrap();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0]["parametro_tecnico"], "Temperatura Almacén");

    // --- TOOL 3: lookup_double_entry_table ---
    let call_3 = ToolCall {
        id: "call_t3".to_string(),
        name: LakehouseToolName::LookupDoubleEntryTable
            .as_str()
            .to_string(),
        arguments: json!({
            "centro_id": "planta_norte",
            "tabla_id": "tabla_soldadura",
            "valor_x": 15.0,
            "categoria_y": "Acero Inox"
        }),
    };
    let res_3 = tool_service.ejecutar_herramienta(&call_3).await.unwrap();
    assert!(!res_3.is_error);
    assert_eq!(res_3.output["encontrado"], true);
    assert_eq!(res_3.output["valor_resultado"], "Electrodo E-308");
    assert!(
        res_3.output["notas_al_pie"].as_array().unwrap()[0]
            .as_str()
            .unwrap()
            .contains("Precalentar")
    );

    // --- TOOL 4: execute_athena_sql ---
    let call_4 = ToolCall {
        id: "call_t4".to_string(),
        name: LakehouseToolName::ExecuteAthenaSql.as_str().to_string(),
        arguments: json!({
            "centro_id": "planta_norte",
            "sql_query": "SELECT COUNT(*) as total FROM doc_catalog WHERE vigente = true"
        }),
    };
    let res_4 = tool_service.ejecutar_herramienta(&call_4).await.unwrap();
    assert!(!res_4.is_error);
    let rows = res_4.output.as_array().unwrap();
    assert_eq!(rows.len(), 1);

    // --- TOOL 5: traverse_normative_graph ---
    let call_5 = ToolCall {
        id: "call_t5".to_string(),
        name: LakehouseToolName::TraverseNormativeGraph
            .as_str()
            .to_string(),
        arguments: json!({
            "centro_id": "planta_norte",
            "doc_origen_id": "DOC-001",
            "modo": "prevalencia"
        }),
    };
    let res_5 = tool_service.ejecutar_herramienta(&call_5).await.unwrap();
    assert!(!res_5.is_error);
    assert_eq!(res_5.output["norma_prevalente"], "DOC-003");
    assert_eq!(res_5.output["ha_sido_derogado"], true);
}

#[tokio::test]
async fn test_integration_agent_orchestrator_ciclo_completo() {
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let petgraph = Arc::new(PetgraphEngine::new());
    let centro = CenterId::new("planta_alfa").unwrap();

    datafusion
        .registrar_centro(&centro, "doc_catalog", vec![crear_lote_catalogo_prueba()])
        .await
        .unwrap();

    let tool_service = Arc::new(LakehouseToolService::new(datafusion, lookup, petgraph));
    let runner = Arc::new(MockAgentRunner::new_heuristic());
    let orchestrator = AgentOrchestrator::new(runner, tool_service);

    let mut historial = vec![AgentMessage::user(
        "Por favor revisa el catálogo de normas vigentes",
    )];

    let respuesta_final = orchestrator
        .procesar_turno(&ClaudeModel::sonnet(), &centro, &mut historial, None)
        .await
        .unwrap();

    assert!(respuesta_final.contains("Dictamen Técnico Final"));
    // El historial debe contener: [user, assistant_tools, tool_result, assistant_final]
    assert_eq!(historial.len(), 4);
    assert_eq!(
        historial[1].tool_calls.as_ref().unwrap()[0].name,
        "inspect_lakehouse_catalog"
    );
}
