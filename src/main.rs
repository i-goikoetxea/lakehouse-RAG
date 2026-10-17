//! Punto de entrada de la aplicación Lakehouse RAG Industrial
//!
//! Soporta modo dual:
//! 1. `--mcp-server` o `-m`: Servidor MCP por stdio (Claude Desktop, Claude Code, Cursor, Gemini).
//! 2. `--mcp-info`: Genera el fragmento de configuración JSON para instalar el MCP.
//! 3. `--gui` o sin argumentos: Aplicación gráfica de escritorio (Linear + NotebookLM).
//! 4. Subcomandos (`--centros`, `--crear-centro`, `--activar`, `--ingest`, `--ask`,
//!    `--integridad`…): misma composición (`LakehouseApp`) que la GUI y el MCP.
//!
//! En release el binario es de subsistema Windows (sin consola). Los modos de consola se
//! adjuntan a la consola del proceso padre (`AttachConsole`) para que la salida sea visible;
//! si stdout ya está redirigido (tubería o archivo), se usa ese destino tal cual.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lakehouse_rag::infrastructure::bootstrap::{ConfiguracionComposicion, componer_lakehouse};
use lakehouse_rag::infrastructure::mcp::mcp_server::McpServer;
use lakehouse_rag::presentation::cli::FormatoSalida;
use lakehouse_rag::presentation::cli::ModoEjecucion;
use lakehouse_rag::presentation::cli::ejecutor::{self, CODIGO_ARGUMENTOS, SalidaComando};
use lakehouse_rag::presentation::gui::GuiController;

fn main() -> anyhow::Result<()> {
    let modo = ModoEjecucion::desde_args(std::env::args());
    if !matches!(modo, ModoEjecucion::Gui) {
        consola::adjuntar_a_consola_padre();
    }

    match modo {
        ModoEjecucion::ErrorArgumentos(mensaje) => {
            eprintln!(
                "ERROR: {mensaje}
{}",
                ModoEjecucion::texto_ayuda()
            );
            std::process::exit(CODIGO_ARGUMENTOS);
        }
        ModoEjecucion::Comando(comando, formato) => {
            let salida = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt.block_on(async {
                    let app = componer_lakehouse(&ConfiguracionComposicion::desde_entorno());
                    ejecutor::ejecutar(&app, comando, formato).await
                }),
                Err(e) => SalidaComando::error(
                    &format!("No se pudo iniciar el runtime: {e}"),
                    formato,
                    ejecutor::CODIGO_ERROR,
                ),
            };
            emitir(&salida, formato);
            std::process::exit(salida.codigo);
        }
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

                let app = componer_lakehouse(&ConfiguracionComposicion::desde_entorno());
                for centro in app.hidratar_centros_activos().await? {
                    if let Err(e) = centro.resultado {
                        tracing::warn!("Centro '{}' no proyectado: {e}", centro.centro_id);
                    }
                }
                let server = McpServer::new(app.servidor_herramientas());

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
            let app = componer_lakehouse(&ConfiguracionComposicion::desde_entorno());
            hidratar_en_arranque(&app);
            if let Err(e) = lakehouse_rag::presentation::gui::iniciar_app_tauri(app) {
                eprintln!("[TAURI ERROR]: {e}");
                return Err(anyhow::anyhow!("Fallo al iniciar Tauri: {e}"));
            }
        }
    }

    Ok(())
}

/// Reconstruye las vistas de los centros activos antes de abrir la ventana
fn hidratar_en_arranque(app: &lakehouse_rag::application::use_cases::LakehouseApp) {
    let Ok(rt) = tokio::runtime::Runtime::new() else {
        eprintln!("[ARRANQUE] No se pudo crear el runtime para hidratar los centros");
        return;
    };
    match rt.block_on(app.hidratar_centros_activos()) {
        Ok(centros) => {
            for centro in centros {
                match centro.resultado {
                    Ok(resumen) => println!(
                        "  • Centro '{}' proyectado ({} vistas)",
                        centro.centro_id,
                        resumen.vistas.len()
                    ),
                    Err(e) => eprintln!("  • Centro '{}' no proyectado: {e}", centro.centro_id),
                }
            }
        }
        Err(e) => eprintln!("[ARRANQUE] No se pudo leer el registro de centros: {e}"),
    }
}

/// Escribe la salida de un subcomando en stdout/stderr
fn emitir(salida: &SalidaComando, formato: FormatoSalida) {
    use std::io::Write;
    if !salida.stdout.is_empty() {
        println!("{}", salida.stdout);
    }
    if !salida.stderr.is_empty() && (formato == FormatoSalida::Legible || salida.codigo != 0) {
        eprintln!("{}", salida.stderr);
    }
    let _ = std::io::stdout().flush();
}

/// Consola en release (`windows_subsystem = "windows"`)
mod consola {
    #[cfg(all(windows, not(debug_assertions)))]
    pub fn adjuntar_a_consola_padre() {
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
        unsafe extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
            fn GetStdHandle(std_handle: u32) -> *mut core::ffi::c_void;
        }
        // SAFETY: llamadas Win32 sin punteros de entrada; si stdout ya está redirigido
        // (tubería/archivo heredado) no se toca, y si no hay consola padre, no hace nada.
        unsafe {
            if GetStdHandle(STD_OUTPUT_HANDLE).is_null() {
                AttachConsole(ATTACH_PARENT_PROCESS);
            }
        }
    }

    #[cfg(not(all(windows, not(debug_assertions))))]
    pub fn adjuntar_a_consola_padre() {}
}
