//! Aceptación real del chat de la GUI con Claude Code headless (requiere `claude` autenticado).
//! Ejecutar con: cargo test --test acceptance_gui_chat_claude_headless -- --ignored

use std::sync::Arc;

use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::application::use_cases::lakehouse_tool_service::LakehouseToolService;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::llm::{
    ClaudeAgentConfig, ClaudeAgentRunner, ClaudeHeadlessModelCatalog, ClaudeModelCatalogConfig,
};
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::query_engine::indice_semantico_memoria::IndiceSemanticoMemoria;
use lakehouse_rag::presentation::gui::chat_service::GuiChatService;

#[tokio::test]
#[ignore = "Requiere Claude Code instalado y autenticado"]
async fn test_chat_gui_responde_con_modelo_del_catalogo_real() {
    let herramientas = Arc::new(LakehouseToolService::new(
        Arc::new(DataFusionEngine::new()),
        Arc::new(IndiceSemanticoMemoria::new()),
        Arc::new(MockEmbeddingEngine::new(Some(1024))),
    ));
    let runner = Arc::new(ClaudeAgentRunner::new(ClaudeAgentConfig::default()));
    let servicio = GuiChatService::new(
        AgentOrchestrator::new(runner, herramientas),
        Arc::new(ClaudeHeadlessModelCatalog::new(
            ClaudeModelCatalogConfig::default(),
        )),
    );

    let modelos = servicio
        .listar_modelos()
        .await
        .expect("Catálogo disponible");
    let haiku = modelos
        .iter()
        .find(|m| m.alias.cli_model_name() == "haiku")
        .expect("El catálogo debe incluir 'haiku'");

    let respuesta = servicio
        .enviar_mensaje(
            "Saluda al operador en una sola frase, sin usar herramientas.",
            "planta_alfa",
            haiku.alias.cli_model_name(),
        )
        .await
        .expect("El chat debe responder");

    assert!(!respuesta.reply_html.trim().is_empty());
}
