//! Pruebas de integración E2E para el Servidor de Herramientas y Agente (Fase 6)

use std::sync::Arc;

use arrow_array::{BooleanArray, Int8Array, Int32Array, RecordBatch, StringArray};
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

/// Lote con la forma de la vista v2 `01_documentos_catalogo` (sin columnas vectoriales)
fn crear_lote_catalogo_prueba() -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("version_num", DataType::Int32, false),
        Field::new("vigente", DataType::Boolean, false),
        Field::new("titulo", DataType::Utf8, false),
        Field::new("tipo_documento", DataType::Utf8, false),
        Field::new("nivel_prelacion", DataType::Int8, false),
        Field::new("revision", DataType::Utf8, false),
        Field::new("fecha_vigor", DataType::Utf8, false),
        Field::new("fecha_derogacion", DataType::Utf8, true),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec!["DOC-001", "DOC-002"])),
            Arc::new(Int32Array::from(vec![2, 1])),
            Arc::new(BooleanArray::from(vec![true, false])),
            Arc::new(StringArray::from(vec![
                "Especificación Acero",
                "Procedimiento Soldadura Obsoleto",
            ])),
            Arc::new(StringArray::from(vec![
                "NORMA_INTERNACIONAL",
                "PROCEDIMIENTO_GENERAL",
            ])),
            Arc::new(Int8Array::from(vec![1, 3])),
            Arc::new(StringArray::from(vec!["Rev. 02", "Rev. 01"])),
            Arc::new(StringArray::from(vec!["2026-03-01", "2024-01-01"])),
            Arc::new(StringArray::from(vec![None, Some("2026-03-01")])),
        ],
    )
    .unwrap()
}

/// Lote con la forma de la vista v2 `03_matriz_operativa` (un hito por fila)
fn crear_lote_matriz_operativa_prueba() -> RecordBatch {
    let texto = |nombre: &str, nulable: bool| Field::new(nombre, DataType::Utf8, nulable);
    let schema = Arc::new(Schema::new(vec![
        texto("hito_id", false),
        texto("doc_id", false),
        Field::new("version_num", DataType::Int32, false),
        Field::new("vigente", DataType::Boolean, false),
        Field::new("orden_secuencia", DataType::Int32, false),
        texto("fase_operativa", false),
        texto("objeto_alcance", false),
        texto("rol_ejecutor", false),
        texto("rol_responsable", false),
        texto("registro_codigo", true),
        texto("registro_nombre", true),
        texto("criterio_aceptacion", false),
        texto("condicion_aplicacion", true),
        texto("calidad_extraccion", false),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec![
                "DOC-001@v2-HIT-0001",
                "DOC-001@v2-HIT-0002",
            ])),
            Arc::new(StringArray::from(vec!["DOC-001", "DOC-001"])),
            Arc::new(Int32Array::from(vec![2, 2])),
            Arc::new(BooleanArray::from(vec![true, true])),
            Arc::new(Int32Array::from(vec![1, 2])),
            Arc::new(StringArray::from(vec!["RECEPCION", "INSPECCION"])),
            Arc::new(StringArray::from(vec![
                "Temperatura Almacén",
                "Presión Hidráulica",
            ])),
            Arc::new(StringArray::from(vec!["Almacenero", "Inspector QA/QC"])),
            Arc::new(StringArray::from(vec![
                "Jefe de Almacén",
                "Jefe de Calidad",
            ])),
            Arc::new(StringArray::from(vec![
                Some("FOR-ALM-01"),
                Some("PPI-MAT-09"),
            ])),
            Arc::new(StringArray::from(vec![Some("Acta"), Some("PPI")])),
            Arc::new(StringArray::from(vec!["15-25 °C", "140-160 bar"])),
            Arc::new(StringArray::from(vec![
                Some("Ambiente controlado"),
                Some("Carga máxima"),
            ])),
            Arc::new(StringArray::from(vec!["OK", "OK"])),
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
        .registrar_centro(
            &centro,
            "01_documentos_catalogo",
            vec![crear_lote_catalogo_prueba()],
        )
        .await
        .unwrap();
    datafusion
        .registrar_centro(
            &centro,
            "03_matriz_operativa",
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
        arguments: json!({ "centro_id": "planta_norte", "fase": "recepcion" }),
    };
    let res_2 = tool_service.ejecutar_herramienta(&call_2).await.unwrap();
    assert!(!res_2.is_error);
    let params = res_2.output.as_array().unwrap();
    assert_eq!(params.len(), 1);
    assert_eq!(params[0]["objeto_alcance"], "Temperatura Almacén");
    assert_eq!(params[0]["rol_responsable"], "Jefe de Almacén");
    assert_eq!(params[0]["registro_codigo"], "FOR-ALM-01");

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
            "sql_query": "SELECT COUNT(*) as total FROM 01_documentos_catalogo WHERE vigente = true"
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
        .registrar_centro(
            &centro,
            "01_documentos_catalogo",
            vec![crear_lote_catalogo_prueba()],
        )
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
