//! Punto de entrada de la aplicación Lakehouse RAG Industrial
//!
//! Soporta modo dual:
//! 1. `--mcp-server` o `-m`: Servidor MCP por stdio (Claude Desktop, Claude Code, Cursor, Gemini).
//! 2. `--mcp-info`: Genera el fragmento de configuración JSON para instalar el MCP.
//! 3. `--gui` o sin argumentos: Aplicación gráfica de escritorio (Linear + NotebookLM).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::application::use_cases::lakehouse_tool_service::LakehouseToolService;
use lakehouse_rag::application::use_cases::table_lookup_service::TableLookupService;
use lakehouse_rag::infrastructure::llm::{
    ClaudeAgentConfig, ClaudeAgentRunner, ClaudeHeadlessModelCatalog, ClaudeModelCatalogConfig,
};
use lakehouse_rag::infrastructure::mcp::mcp_server::McpServer;
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::query_engine::petgraph_engine::PetgraphEngine;
use lakehouse_rag::presentation::cli::ModoEjecucion;
use lakehouse_rag::presentation::gui::GuiController;
use lakehouse_rag::presentation::gui::chat_service::GuiChatService;

fn main() -> anyhow::Result<()> {
    let modo = ModoEjecucion::desde_args(std::env::args());

    match modo {
        ModoEjecucion::Help => {
            println!("{}", ModoEjecucion::texto_ayuda());
        }
        ModoEjecucion::McpInfo => {
            let exe_path = std::env::current_exe()
                .unwrap_or_else(|_| std::path::PathBuf::from("lakehouse_rag.exe"))
                .to_string_lossy()
                .to_string();

            println!("=== Configuración de Instalación MCP para Lakehouse RAG ===");
            println!(
                "\nAñade este bloque a tu archivo de configuración (ej. claude_desktop_config.json):"
            );
            println!("{}", ModoEjecucion::snippet_instalacion_mcp(&exe_path));
            println!("\nO regístralo directamente en Claude Code ejecutando:");
            println!(
                "  claude mcp add lakehouse-rag -- \"{}\" --mcp-server",
                exe_path
            );
        }
        ModoEjecucion::McpServer => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                tracing_subscriber::fmt()
                    .with_writer(std::io::stderr)
                    .init();

                let server = McpServer::new(construir_servicio_herramientas());

                server.run_stdio().await
            })?;
        }
        ModoEjecucion::Gui => {
            tracing_subscriber::fmt::init();
            let config = GuiController::config_ventana();

            println!("===========================================================");
            println!("   🏭 Lakehouse RAG Industrial — GUI SOTA 2026            ");
            println!("   Diseño: Linear + Google NotebookLM (3 Paneles)          ");
            println!("===========================================================");
            println!("Configuración de Ventana de Escritorio:");
            println!("  • Título: {}", config.title);
            println!(
                "  • Resolución 16:9: {} x {} px",
                config.width, config.height
            );
            println!("  • Frameless: {}", !config.decorations);
            println!("  • Centrada: {}", config.center);
            println!("  • Modo Dual: Oscuro (#08090a Linear) y Claro");
            println!("  • Activos Web: src/presentation/gui/web/index.html");
            println!("===========================================================");

            println!("Iniciando ventana nativa de escritorio con Tauri 2.0...");
            if let Err(e) =
                lakehouse_rag::presentation::gui::iniciar_app_tauri(construir_servicio_chat_gui())
            {
                eprintln!("[TAURI ERROR]: {e}");
                return Err(anyhow::anyhow!("Fallo al iniciar Tauri: {e}"));
            }
        }
    }

    Ok(())
}

/// Composición del servicio de herramientas deterministas del Lakehouse
fn construir_servicio_herramientas() -> Arc<LakehouseToolService> {
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let petgraph = Arc::new(PetgraphEngine::new());
    Arc::new(LakehouseToolService::new(datafusion, lookup, petgraph))
}

/// Composición del chat de la GUI: agente y catálogo respaldados por Claude Code headless
fn construir_servicio_chat_gui() -> GuiChatService {
    let runner = Arc::new(ClaudeAgentRunner::new(ClaudeAgentConfig::default()));
    let orquestador = AgentOrchestrator::new(runner, construir_servicio_herramientas());
    let catalogo = Arc::new(ClaudeHeadlessModelCatalog::new(
        ClaudeModelCatalogConfig::default(),
    ));
    GuiChatService::new(orquestador, catalogo)
}
