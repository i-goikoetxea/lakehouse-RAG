//! Binario de consola de Lakehouse RAG (Fase 15): los mismos subcomandos que `lakehouse_rag`
//! (`--centros`, `--crear-centro`, `--activar`, `--ingest`, `--ask`, `--integridad`,
//! `--mcp-server`…) sobre la misma composición, pero compilado para el subsistema consola:
//! en release PowerShell y `cmd` lo esperan, la salida sale en orden y `$LASTEXITCODE` es fiable.
//! La ventana se abre con `lakehouse_rag.exe`.

use lakehouse_rag::presentation::cli::ModoEjecucion;
use lakehouse_rag::presentation::cli::ejecutor::CODIGO_ARGUMENTOS;
use lakehouse_rag::presentation::cli::proceso;

fn main() -> anyhow::Result<()> {
    let argumentos: Vec<String> = std::env::args().collect();
    match ModoEjecucion::desde_args(&argumentos) {
        ModoEjecucion::ErrorArgumentos(mensaje) => {
            std::process::exit(proceso::informar_error_argumentos(&mensaje))
        }
        ModoEjecucion::Comando(comando, formato) => {
            std::process::exit(proceso::ejecutar_comando(comando, formato))
        }
        ModoEjecucion::Help => println!("{}", ModoEjecucion::texto_ayuda()),
        ModoEjecucion::McpInfo => proceso::imprimir_mcp_info(),
        ModoEjecucion::McpServer => proceso::servir_mcp()?,
        ModoEjecucion::Gui if argumentos.len() <= 1 => {
            println!("{}", ModoEjecucion::texto_ayuda())
        }
        ModoEjecucion::Gui => {
            eprintln!(
                "lakehouse_rag_cli es solo de consola: abre la ventana con lakehouse_rag.exe (o `lakehouse_rag --gui`)."
            );
            std::process::exit(CODIGO_ARGUMENTOS);
        }
    }
    Ok(())
}
