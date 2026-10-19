//! Arranque de los modos de consola, compartido por los dos binarios:
//! - `lakehouse_rag`: la aplicación de escritorio (en release, subsistema Windows) que además
//!   acepta subcomandos adjuntándose a la consola padre;
//! - `lakehouse_rag_cli`: binario de subsistema consola para terminales y scripts. PowerShell
//!   y `cmd` lo esperan, ven su salida en orden y reciben su código de salida.

use crate::infrastructure::bootstrap::{ConfiguracionComposicion, componer_lakehouse};
use crate::infrastructure::mcp::mcp_server::McpServer;
use crate::presentation::cli::ejecutor::{self, CODIGO_ARGUMENTOS, SalidaComando};
use crate::presentation::cli::{ComandoCli, FormatoSalida, ModoEjecucion};

/// Ejecuta un subcomando sobre la composición real, escribe su salida y devuelve el código
pub fn ejecutar_comando(comando: ComandoCli, formato: FormatoSalida) -> i32 {
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
    salida.codigo
}

/// Informa de unos argumentos inválidos junto con la ayuda y devuelve el código de salida
pub fn informar_error_argumentos(mensaje: &str) -> i32 {
    eprintln!("ERROR: {mensaje}\n{}", ModoEjecucion::texto_ayuda());
    CODIGO_ARGUMENTOS
}

/// Imprime el fragmento de instalación del servidor MCP para este ejecutable
pub fn imprimir_mcp_info() {
    let exe_path = std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("lakehouse_rag.exe"))
        .to_string_lossy()
        .to_string();

    println!("=== Configuración de Instalación MCP para Lakehouse RAG ===");
    println!("\nAñade este bloque a tu archivo de configuración (ej. claude_desktop_config.json):");
    println!("{}", ModoEjecucion::snippet_instalacion_mcp(&exe_path));
    println!("\nO regístralo directamente en Claude Code ejecutando:");
    println!("  claude mcp add lakehouse-rag -- \"{exe_path}\" --mcp-server");
}

/// Servidor MCP por stdio con los centros activos proyectados
pub fn servir_mcp() -> anyhow::Result<()> {
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
        McpServer::new(app.servidor_herramientas())
            .run_stdio()
            .await
    })?;
    Ok(())
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
