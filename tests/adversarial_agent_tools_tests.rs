//! Suite de pruebas adversariales de seguridad y resiliencia para el Servidor de Herramientas y
//! Agente (Fase 6, adaptada al contrato v2 de la Fase 13)

mod comun_agent_tools;

use std::sync::Arc;

use serde_json::{Value, json};

use comun_agent_tools::*;
use lakehouse_rag::application::dtos::agent::{AgentMessage, AgentTurnOutput, ClaudeModel};
use lakehouse_rag::application::dtos::tools::{LakehouseToolName, ToolCall, ToolResult};
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;

async fn invocar(servicio: &dyn ToolServerPort, nombre: &str, argumentos: Value) -> ToolResult {
    servicio
        .ejecutar_herramienta(&ToolCall {
            id: format!("call_{nombre}"),
            name: nombre.to_string(),
            arguments: argumentos,
        })
        .await
        .expect("los fallos de la tool se devuelven como ToolResult con is_error")
}

fn mensaje_error(resultado: &ToolResult) -> String {
    assert!(
        resultado.is_error,
        "se esperaba error: {}",
        resultado.output
    );
    resultado.output["error"]
        .as_str()
        .expect("el error viaja en output.error")
        .to_string()
}

async fn total_documentos(servicio: &dyn ToolServerPort, centro: &str) -> Value {
    let resultado = invocar(
        servicio,
        LakehouseToolName::ExecuteAthenaSql.as_str(),
        json!({ "centro_id": centro, "sql_query": "SELECT COUNT(*) AS n FROM 01_documentos_catalogo" }),
    )
    .await;
    assert!(!resultado.is_error, "{}", resultado.output);
    resultado.output["resultado"][0]["n"].clone()
}

#[tokio::test]
async fn test_adv_bloqueo_inyecciones_sql_destructivas_en_herramienta() {
    let entorno = entorno_vacio();
    let centro = CenterId::new("planta_segura").unwrap();
    sembrar_centro(&entorno, &centro).await;
    let servicio = entorno.servicio.as_ref();

    let inyecciones = [
        "DROP TABLE 01_documentos_catalogo",
        "DELETE FROM 01_documentos_catalogo WHERE 1=1",
        "INSERT INTO 01_documentos_catalogo VALUES ('X', 'Y', 'Z', 1, 1, '2026', true)",
        "ALTER TABLE 01_documentos_catalogo ADD COLUMN backdoor TEXT",
        "SELECT * FROM 01_documentos_catalogo; DROP TABLE 01_documentos_catalogo;",
        "TRUNCATE TABLE 01_documentos_catalogo",
        "UPDATE 01_documentos_catalogo SET vigente = false",
        "SELECT 1; DELETE FROM 01_documentos_catalogo",
    ];

    for iny in inyecciones {
        let resultado = invocar(
            servicio,
            LakehouseToolName::ExecuteAthenaSql.as_str(),
            json!({ "centro_id": "planta_segura", "sql_query": iny }),
        )
        .await;
        let error = mensaje_error(&resultado);
        assert!(
            error.contains("Operación no permitida")
                || error.contains("debe comenzar con SELECT")
                || error.contains("exactamente una sentencia"),
            "Se debió bloquear la consulta: {iny} → {error}"
        );
    }

    // Las vistas siguen intactas tras los intentos
    assert_eq!(total_documentos(servicio, "planta_segura").await, 4);
}

#[tokio::test]
async fn test_adv_aislamiento_estricto_anti_cross_center_en_sql_tool() {
    let entorno = entorno_vacio();
    let centro_a = CenterId::new("planta_alfa").unwrap();
    let centro_b = CenterId::new("planta_beta").unwrap();

    // Solo planta_alfa tiene documentos; planta_beta está activo pero vacío
    sembrar_centro(&entorno, &centro_a).await;
    registrar_centro_vacio(&entorno, &centro_b).await;
    let servicio = entorno.servicio.as_ref();

    // La tabla no cualificada se resuelve contra las vistas del centro que consulta
    assert_eq!(total_documentos(servicio, centro_b.as_str()).await, 0);
    assert_eq!(total_documentos(servicio, centro_a.as_str()).await, 4);

    // Intentos explícitos de leer las vistas de planta_alfa desde planta_beta
    for sql in [
        "SELECT * FROM \"planta_alfa_01_documentos_catalogo\"".to_string(),
        "SELECT * FROM planta_alfa_03_matriz_operativa".to_string(),
        "SELECT doc_id FROM 01_documentos_catalogo UNION ALL SELECT doc_id FROM \"planta_alfa_01_documentos_catalogo\"".to_string(),
        "WITH robo AS (SELECT * FROM \"planta_alfa_06_grafo_referencias\") SELECT * FROM robo".to_string(),
    ] {
        let resultado = invocar(
            servicio,
            LakehouseToolName::ExecuteAthenaSql.as_str(),
            json!({ "centro_id": centro_b.as_str(), "sql_query": sql }),
        )
        .await;
        let error = mensaje_error(&resultado);
        assert!(error.contains("Acceso denegado"), "{sql} → {error}");
    }

    // Un centro sin vistas proyectadas no puede usar ninguna tool
    let resultado = invocar(
        servicio,
        LakehouseToolName::ExecuteAthenaSql.as_str(),
        json!({ "centro_id": "planta_gamma", "sql_query": "SELECT * FROM 01_documentos_catalogo" }),
    )
    .await;
    assert!(mensaje_error(&resultado).contains("no está activo"));

    // Las tools no SQL tampoco ven datos de planta_alfa desde planta_beta
    let matriz = invocar(
        servicio,
        LakehouseToolName::QueryOperationalMatrix.as_str(),
        json!({ "centro_id": centro_b.as_str() }),
    )
    .await;
    assert!(!matriz.is_error, "{}", matriz.output);
    assert_eq!(matriz.output["total"], 0);
}

#[tokio::test]
async fn test_adv_tolerancia_a_argumentos_json_corruptos_o_incompletos() {
    let entorno = entorno_vacio();
    let centro = CenterId::new("p").unwrap();
    sembrar_centro(&entorno, &centro).await;
    let servicio = entorno.servicio.as_ref();

    let payloads_corruptos = [
        ("inspect_lakehouse_catalog", json!({})), // Falta centro_id
        (
            "inspect_lakehouse_catalog",
            json!({ "centro_id": "p", "solo_vigentes": true }),
        ), // Campo v1 ya inexistente
        ("query_operational_matrix", json!({ "fase": 12345 })), // centro_id ausente y tipo incorrecto
        (
            "query_operational_matrix",
            json!({ "centro_id": "p", "parametro": "temp" }),
        ), // Campo v1 ya inexistente
        (
            "lookup_double_entry_table",
            json!({ "centro_id": "p", "tabla_id": "4-A", "valor_x": [15], "valor_y": "Acero" }),
        ),
        (
            "lookup_double_entry_table",
            json!({ "centro_id": "p", "tabla_id": "4-A", "valor_x": 15, "categoria_y": "Acero" }),
        ), // valor_y ausente y campo v1
        ("execute_athena_sql", json!({ "sql_query": 42 })),
        (
            "traverse_normative_graph",
            json!({ "centro_id": "p", "nodo_origen": "DOC-001", "modo": 999 }),
        ),
        (
            "traverse_normative_graph",
            json!({ "centro_id": "p", "doc_origen_id": "DOC-001" }),
        ),
        (
            "search_normative_context",
            json!({ "centro_id": "p", "consulta": "x", "top_k": "muchos" }),
        ),
    ];

    for (name, payload) in payloads_corruptos {
        let resultado = invocar(servicio, name, payload.clone()).await;
        let error = mensaje_error(&resultado);
        assert!(
            error.contains("Argumentos inválidos"),
            "{name} {payload} → {error}"
        );
    }

    let desconocida = invocar(servicio, "herramienta_inexistente", json!({})).await;
    assert!(desconocida.is_error);
}

#[tokio::test]
async fn test_adv_resistencia_a_inyecciones_en_filtros_de_matriz_operativa() {
    let entorno = entorno_vacio();
    let centro = CenterId::new("planta_alfa").unwrap();
    sembrar_centro(&entorno, &centro).await;
    let servicio = entorno.servicio.as_ref();

    // Intentar romper el WHERE de SQL inyectando comillas y sentencias en los filtros
    for filtros in [
        json!({ "rol": "Almacenero' OR '1'='1" }),
        json!({ "registro": "FOR-ALM-01'; DROP TABLE \"planta_alfa_03_matriz_operativa\"; --" }),
        json!({ "doc_id": "DOC-001' OR '1'='1" }),
        json!({ "objeto": "%' UNION SELECT * FROM \"planta_alfa_01_documentos_catalogo\" --" }),
    ] {
        let mut argumentos = filtros.clone();
        argumentos["centro_id"] = json!(centro.as_str());
        let resultado = invocar(
            servicio,
            LakehouseToolName::QueryOperationalMatrix.as_str(),
            argumentos,
        )
        .await;
        assert!(!resultado.is_error, "{filtros}: {}", resultado.output);
        assert_eq!(
            resultado.output["total"], 0,
            "{filtros}: {}",
            resultado.output
        );
    }

    // La fase se normaliza a su vocabulario canónico y se filtra en Rust: el «OR '1'='1»
    // nunca amplía el resultado a otras fases (sin inyección sería RECEPCION → 1 fila)
    let fase = invocar(
        servicio,
        LakehouseToolName::QueryOperationalMatrix.as_str(),
        json!({ "centro_id": centro.as_str(), "fase": "Recepcion' OR '1'='1" }),
    )
    .await;
    if !fase.is_error {
        let filas = fase.output["filas"].as_array().unwrap();
        assert!(filas.len() <= 1, "{}", fase.output);
        assert!(
            filas.iter().all(|f| f["fase_operativa"] == "RECEPCION"),
            "{}",
            fase.output
        );
    }

    // Los datos siguen intactos
    let intacta = invocar(
        servicio,
        LakehouseToolName::QueryOperationalMatrix.as_str(),
        json!({ "centro_id": centro.as_str() }),
    )
    .await;
    assert_eq!(intacta.output["total"], 2, "{}", intacta.output);
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
                arguments: json!({ "centro_id": "planta_loop" }),
            }]))
        }
    }

    let runner = Arc::new(InfiniteLoopRunner {
        contador: Mutex::new(0),
    });
    let entorno = entorno_vacio();
    let centro = CenterId::new("planta_loop").unwrap();
    sembrar_centro(&entorno, &centro).await;

    // Configuramos max_steps a 3
    let orchestrator = AgentOrchestrator::new(runner, entorno.servicio.clone()).with_max_steps(3);
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
