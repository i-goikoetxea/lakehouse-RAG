//! Suite de pruebas adversariales de seguridad y resiliencia para el Servidor de Herramientas y Agente (Fase 6)

use std::sync::Arc;

use arrow_array::{BooleanArray, Int8Array, Int32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use serde_json::json;

use lakehouse_rag::application::dtos::agent::{AgentMessage, AgentTurnOutput, ClaudeModel};
use lakehouse_rag::application::dtos::tools::{LakehouseToolName, ToolCall};
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::application::ports::analytical_engine::AnalyticalEnginePort;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::application::use_cases::lakehouse_tool_service::LakehouseToolService;
use lakehouse_rag::application::use_cases::table_lookup_service::TableLookupService;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::query_engine::petgraph_engine::PetgraphEngine;

fn lote_minimo_seguridad() -> RecordBatch {
    let schema = Arc::new(Schema::new(vec![
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("titulo", DataType::Utf8, false),
        Field::new("tipo", DataType::Utf8, false),
        Field::new("nivel_prelacion", DataType::Int8, false),
        Field::new("version", DataType::Int32, false),
        Field::new("fecha_emision", DataType::Utf8, false),
        Field::new("vigente", DataType::Boolean, false),
    ]));

    let doc_ids = Arc::new(StringArray::from(vec!["DOC-CONFIDENCIAL"]));
    let titulos = Arc::new(StringArray::from(vec!["Documento Secreto"]));
    let tipos = Arc::new(StringArray::from(vec!["Norma"]));
    let prelaciones = Arc::new(Int8Array::from(vec![1]));
    let versiones = Arc::new(Int32Array::from(vec![1]));
    let fechas = Arc::new(StringArray::from(vec!["2026-01-01"]));
    let vigentes = Arc::new(BooleanArray::from(vec![true]));

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

#[tokio::test]
async fn test_adv_bloqueo_inyecciones_sql_destructivas_en_herramienta() {
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let petgraph = Arc::new(PetgraphEngine::new());
    let service = LakehouseToolService::new(datafusion, lookup, petgraph);

    let inyecciones = [
        "DROP TABLE doc_catalog",
        "DELETE FROM doc_catalog WHERE 1=1",
        "INSERT INTO doc_catalog VALUES ('X', 'Y', 'Z', 1, 1, '2026', true)",
        "ALTER TABLE doc_catalog ADD COLUMN backdoor TEXT",
        "SELECT * FROM doc_catalog; DROP TABLE doc_catalog;",
        "TRUNCATE TABLE doc_catalog",
    ];

    for iny in inyecciones {
        let call = ToolCall {
            id: "call_iny".to_string(),
            name: LakehouseToolName::ExecuteAthenaSql.as_str().to_string(),
            arguments: json!({
                "centro_id": "planta_segura",
                "sql_query": iny
            }),
        };

        let res = service.ejecutar_herramienta(&call).await.unwrap();
        assert!(res.is_error, "Se debió bloquear la consulta: {iny}");
        assert!(
            res.output["error"]
                .as_str()
                .unwrap()
                .contains("Operación no permitida")
                || res.output["error"]
                    .as_str()
                    .unwrap()
                    .contains("debe comenzar con SELECT")
        );
    }
}

#[tokio::test]
async fn test_adv_aislamiento_estricto_anti_cross_center_en_sql_tool() {
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let petgraph = Arc::new(PetgraphEngine::new());

    let centro_a = CenterId::new("planta_alfa").unwrap();
    let centro_b = CenterId::new("planta_beta").unwrap();

    // Solo registramos datos en planta_alfa
    datafusion
        .registrar_centro(&centro_a, "doc_catalog", vec![lote_minimo_seguridad()])
        .await
        .unwrap();

    let service = LakehouseToolService::new(datafusion, lookup, petgraph);

    // Intentamos consultar desde planta_beta hacia las tablas de planta_alfa o tabla no cualificada
    let call_aislamiento = ToolCall {
        id: "call_aisla".to_string(),
        name: LakehouseToolName::ExecuteAthenaSql.as_str().to_string(),
        arguments: json!({
            "centro_id": centro_b.as_str(),
            "sql_query": "SELECT * FROM doc_catalog"
        }),
    };

    let res = service
        .ejecutar_herramienta(&call_aislamiento)
        .await
        .unwrap();
    // Debe dar error porque la tabla mapeada planta_beta_doc_catalog no existe para planta_beta
    assert!(res.is_error);
    assert!(
        res.output["error"]
            .as_str()
            .unwrap()
            .contains("Error de sintaxis o ejecución SQL")
    );
}

#[tokio::test]
async fn test_adv_tolerancia_a_argumentos_json_corruptos_o_incompletos() {
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let petgraph = Arc::new(PetgraphEngine::new());
    let service = LakehouseToolService::new(datafusion, lookup, petgraph);

    let payloads_corruptos = [
        ("inspect_lakehouse_catalog", json!({})), // Falta centro_id
        ("query_operational_matrix", json!({ "fase": 12345 })), // centro_id ausente y tipo incorrecto
        (
            "lookup_double_entry_table",
            json!({ "centro_id": "p", "valor_x": "no_es_numero" }),
        ),
        ("execute_athena_sql", json!({ "sql_query": 42 })),
        (
            "traverse_normative_graph",
            json!({ "centro_id": "p", "modo": 999 }),
        ),
    ];

    for (name, payload) in payloads_corruptos {
        let call = ToolCall {
            id: "call_err".to_string(),
            name: name.to_string(),
            arguments: payload,
        };

        let err = service.ejecutar_herramienta(&call).await.unwrap_err();
        assert!(matches!(err, DomainError::ToolExecutionError(_)));
        assert!(err.to_string().contains("Argumentos inválidos"));
    }
}

#[tokio::test]
async fn test_adv_resistencia_a_inyecciones_en_filtros_de_matriz_operativa() {
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let petgraph = Arc::new(PetgraphEngine::new());
    let centro = CenterId::new("planta_alfa").unwrap();

    let service = LakehouseToolService::new(datafusion, lookup, petgraph);

    // Intentar romper el WHERE de SQL inyectando comillas y sentencias
    let call_iny = ToolCall {
        id: "call_mat_iny".to_string(),
        name: LakehouseToolName::QueryOperationalMatrix
            .as_str()
            .to_string(),
        arguments: json!({
            "centro_id": centro.as_str(),
            "fase": "Recepcion' OR '1'='1",
            "parametro": "temp'; DROP TABLE operational_matrix; --"
        }),
    };

    let res = service.ejecutar_herramienta(&call_iny).await.unwrap();
    // No debe paniquear ni romper el motor (sanitización de comillas simples escapadas)
    assert!(res.is_error || res.output.as_array().is_some());
}

#[tokio::test]
async fn test_adv_bucle_infinito_de_herramientas_cortado_por_max_steps() {
    use async_trait::async_trait;
    use lakehouse_rag::application::dtos::tools::ToolDefinition;
    use std::sync::Mutex;

    struct InfiniteLoopRunner {
        contador: Mutex<usize>,
    }

    #[async_trait]
    impl AgentRunnerPort for InfiniteLoopRunner {
        async fn ejecutar_turno(
            &self,
            _modelo: &ClaudeModel,
            _system_prompt: &str,
            _historial: &[AgentMessage],
            _herramientas: &[ToolDefinition],
        ) -> Result<AgentTurnOutput, DomainError> {
            let mut c = self.contador.lock().unwrap();
            *c += 1;
            // Siempre responde pidiendo ejecutar una herramienta sin llegar a una respuesta final
            Ok(AgentTurnOutput::ToolCalls(vec![ToolCall {
                id: format!("call_{c}"),
                name: "inspect_lakehouse_catalog".to_string(),
                arguments: json!({ "centro_id": "planta_loop", "solo_vigentes": true }),
            }]))
        }
    }

    let runner = Arc::new(InfiniteLoopRunner {
        contador: Mutex::new(0),
    });
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let petgraph = Arc::new(PetgraphEngine::new());
    let tools = Arc::new(LakehouseToolService::new(datafusion, lookup, petgraph));

    // Configuramos max_steps a 3
    let orchestrator = AgentOrchestrator::new(runner, tools).with_max_steps(3);
    let centro = CenterId::new("planta_loop").unwrap();
    let mut historial = vec![AgentMessage::user("Inicia bucle")];

    let err = orchestrator
        .procesar_turno(
            &ClaudeModel::new("opus").unwrap(),
            &centro,
            &mut historial,
            None,
        )
        .await
        .unwrap_err();

    assert!(matches!(err, DomainError::AgentError(_)));
    assert!(
        err.to_string()
            .contains("Se superó el límite máximo de iteraciones")
    );
}
