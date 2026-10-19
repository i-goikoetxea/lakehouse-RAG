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
//! si stdout ya está redirigido (tubería o archivo), se usa ese destino tal cual. Para
//! terminales y scripts existe además el binario de consola `lakehouse_rag_cli`
//! (`src/bin/lakehouse_rag_cli.rs`), que PowerShell espera y cuyo código de salida es fiable.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lakehouse_rag::infrastructure::bootstrap::{ConfiguracionComposicion, componer_lakehouse};
use lakehouse_rag::presentation::cli::ModoEjecucion;
use lakehouse_rag::presentation::cli::proceso;
use lakehouse_rag::presentation::gui::GuiController;

fn main() -> anyhow::Result<()> {
    let modo = ModoEjecucion::desde_args(std::env::args());
    if !matches!(modo, ModoEjecucion::Gui) {
        consola::adjuntar_a_consola_padre();
    }

    match modo {
        ModoEjecucion::ErrorArgumentos(mensaje) => {
            std::process::exit(proceso::informar_error_argumentos(&mensaje));
        }
        ModoEjecucion::Comando(comando, formato) => {
            std::process::exit(proceso::ejecutar_comando(comando, formato));
        }
        ModoEjecucion::Help => {
            println!("{}", ModoEjecucion::texto_ayuda());
        }
        ModoEjecucion::McpInfo => proceso::imprimir_mcp_info(),
        ModoEjecucion::McpServer => proceso::servir_mcp()?,
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
