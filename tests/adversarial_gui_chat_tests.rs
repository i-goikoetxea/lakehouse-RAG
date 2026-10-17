//! Suite adversarial del servicio de chat de la GUI (comandos Tauri)

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use lakehouse_rag::application::dtos::agent::{
    AgentMessage, AgentTurnOutput, ClaudeModel, ModelDescriptor,
};
use lakehouse_rag::application::dtos::tools::{ToolCall, ToolDefinition, ToolResult};
use lakehouse_rag::application::ports::model_catalog::ModelCatalogPort;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::infrastructure::llm::MockAgentRunner;
use lakehouse_rag::presentation::gui::chat_service::GuiChatService;

struct ToolServerFijo;

#[async_trait]
impl ToolServerPort for ToolServerFijo {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        ToolDefinition::lakehouse_tools()
    }

    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            output: json!({ "doc_id": "DOC-001" }),
            is_error: false,
        })
    }
}

struct CatalogoFijo(Result<Vec<ModelDescriptor>, String>);

#[async_trait]
impl ModelCatalogPort for CatalogoFijo {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        self.0.clone().map_err(DomainError::AgentError)
    }
}

fn descriptor(alias: &str) -> ModelDescriptor {
    ModelDescriptor {
        alias: ClaudeModel::new(alias).unwrap(),
        resolved_model: format!("claude-{alias}"),
        display_name: alias.to_uppercase(),
        description: String::new(),
    }
}

fn servicio_con(runner: MockAgentRunner) -> GuiChatService {
    let orquestador = AgentOrchestrator::new(Arc::new(runner), Arc::new(ToolServerFijo));
    GuiChatService::new(
        orquestador,
        Arc::new(CatalogoFijo(Ok(vec![
            descriptor("opus"),
            descriptor("haiku"),
        ]))),
    )
}

#[tokio::test]
async fn test_respuesta_con_html_malicioso_se_escapa() {
    let servicio = servicio_con(MockAgentRunner::new_scripted(vec![
        AgentTurnOutput::FinalMessage("<img src=x onerror=alert(1)>\nLínea 2 & fin".to_string()),
    ]));

    let respuesta = servicio
        .enviar_mensaje("hola", "planta_alfa", "opus")
        .await
        .expect("Debe responder");

    assert!(!respuesta.reply_html.contains("<img"));
    assert!(respuesta.reply_html.contains("&lt;img"));
    assert!(respuesta.reply_html.contains("<br>"));
    assert!(respuesta.reply_html.contains("&amp; fin"));
}

#[tokio::test]
async fn test_tool_cards_reflejan_herramientas_ejecutadas() {
    let servicio = servicio_con(MockAgentRunner::new_scripted(vec![
        AgentTurnOutput::ToolCalls(vec![ToolCall {
            id: "call_1".to_string(),
            name: "inspect_lakehouse_catalog".to_string(),
            arguments: json!({ "centro_id": "planta_alfa" }),
        }]),
        AgentTurnOutput::FinalMessage("Listo".to_string()),
    ]));

    let respuesta = servicio
        .enviar_mensaje("catálogo", "planta_alfa", "haiku")
        .await
        .unwrap();

    assert_eq!(respuesta.tools_used.len(), 1);
    let card = &respuesta.tools_used[0];
    assert_eq!(card.name, "inspect_lakehouse_catalog");
    assert_eq!(card.status, "ok");
    assert_eq!(card.args["centro_id"], "planta_alfa");
    assert_eq!(card.result["doc_id"], "DOC-001");

    let json = serde_json::to_value(&respuesta).unwrap();
    assert!(
        json.get("toolsUsed").is_some(),
        "El frontend espera camelCase"
    );
    assert!(json.get("replyHtml").is_some());
}

#[tokio::test]
async fn test_entradas_invalidas_se_rechazan_sin_invocar_al_modelo() {
    let servicio = servicio_con(MockAgentRunner::with_error("no debería invocarse"));
    let gigante = "x".repeat(GuiChatService::MAX_MENSAJE_LEN + 1);

    let casos = [
        ("   ", "planta_alfa", "opus"),
        (gigante.as_str(), "planta_alfa", "opus"),
        ("hola", "planta_alfa", "--dangerously-skip-permissions"),
        ("hola", "planta_alfa", "opus; calc.exe"),
        ("hola", "../../etc", "opus"),
    ];

    for (mensaje, centro, modelo) in casos {
        let err = servicio
            .enviar_mensaje(mensaje, centro, modelo)
            .await
            .unwrap_err();
        assert!(
            !err.contains("no debería invocarse"),
            "Se invocó al modelo con entrada inválida: {err}"
        );
    }
}

#[tokio::test]
async fn test_error_del_modelo_no_contamina_el_historial() {
    let servicio = servicio_con(MockAgentRunner::new_scripted(vec![
        AgentTurnOutput::FinalMessage("Primera".to_string()),
    ]));

    servicio
        .enviar_mensaje("uno", "planta_alfa", "opus")
        .await
        .unwrap();
    // El guion se agota: el segundo turno falla
    let _ = servicio.enviar_mensaje("dos", "planta_alfa", "opus").await;

    let historial = servicio.historial("planta_alfa").await;
    assert_eq!(
        historial,
        vec![
            AgentMessage::user("uno"),
            AgentMessage::assistant("Primera")
        ]
    );
}

#[tokio::test]
async fn test_historial_aislado_por_centro_y_acotado() {
    let respuestas = (0..GuiChatService::MAX_HISTORIAL)
        .map(|i| AgentTurnOutput::FinalMessage(format!("r{i}")))
        .collect();
    let servicio = servicio_con(MockAgentRunner::new_scripted(respuestas));

    for i in 0..GuiChatService::MAX_HISTORIAL {
        servicio
            .enviar_mensaje(&format!("m{i}"), "planta_alfa", "opus")
            .await
            .unwrap();
    }

    let historial = servicio.historial("planta_alfa").await;
    assert!(historial.len() <= GuiChatService::MAX_HISTORIAL);
    assert_eq!(
        historial.first().map(|m| m.role),
        Some(lakehouse_rag::application::dtos::agent::AgentRole::User)
    );
    assert!(servicio.historial("planta_beta").await.is_empty());
}

#[tokio::test]
async fn test_listar_modelos_propaga_catalogo_y_errores() {
    let servicio = servicio_con(MockAgentRunner::new_heuristic());
    let modelos = servicio.listar_modelos().await.unwrap();
    assert_eq!(modelos.len(), 2);
    assert_eq!(modelos[0].alias.cli_model_name(), "opus");

    let orquestador = AgentOrchestrator::new(
        Arc::new(MockAgentRunner::new_heuristic()),
        Arc::new(ToolServerFijo),
    );
    let fallido = GuiChatService::new(
        orquestador,
        Arc::new(CatalogoFijo(Err("claude no autenticado".to_string()))),
    );
    let err = fallido.listar_modelos().await.unwrap_err();
    assert!(err.contains("claude no autenticado"));
}
