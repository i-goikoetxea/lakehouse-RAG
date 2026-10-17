//! Módulo de Comandos de Consola (CLI) y Configuración de Lanzamiento
//!
//! Proporciona parsing de argumentos de consola para alternar entre el servidor
//! MCP por stdio y la aplicación de escritorio, además de ofrecer información
//! de autoinstalación para agentes externos (Claude Desktop, Claude Code, Cursor).

/// Modos de ejecución disponibles para la aplicación
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModoEjecucion {
    /// Inicia el servidor MCP JSON-RPC 2.0 por stdio
    McpServer,
    /// Imprime la configuración de instalación MCP en formato JSON para otros agentes
    McpInfo,
    /// Inicia la interfaz gráfica de usuario (GUI) de escritorio
    Gui,
    /// Muestra la ayuda de consola
    Help,
}

impl ModoEjecucion {
    /// Determina el modo de ejecución a partir de los argumentos del sistema
    pub fn desde_args<I, T>(args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let lista: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();

        if lista.iter().any(|arg| arg == "--help" || arg == "-h") {
            return Self::Help;
        }

        if lista.iter().any(|arg| arg == "--mcp-info") {
            return Self::McpInfo;
        }

        if lista.iter().any(|arg| arg == "--mcp-server" || arg == "-m") {
            return Self::McpServer;
        }

        Self::Gui
    }

    /// Retorna el texto de ayuda del CLI
    pub fn texto_ayuda() -> &'static str {
        r#"
=== Lakehouse RAG Industrial (SOTA 2026) ===

USO:
    lakehouse_rag.exe [OPCIONES]

OPCIONES:
    --mcp-server, -m    Inicia el servidor MCP en modo stdio (para Claude Desktop, Claude Code, Cursor, Gemini)
    --mcp-info          Muestra la configuración JSON para instalar este servidor en Claude Desktop / Claude Code
    --gui               Inicia la aplicación gráfica de escritorio (por defecto)
    -h, --help          Muestra esta ayuda
"#
    }

    /// Retorna el snippet JSON listo para configurar en Claude Desktop / Claude Code
    pub fn snippet_instalacion_mcp(exe_path: &str) -> String {
        format!(
            r#"{{
  "mcpServers": {{
    "lakehouse-rag": {{
      "command": "{}",
      "args": ["--mcp-server"]
    }}
  }}
}}"#,
            exe_path.replace('\\', "\\\\")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deteccion_mcp_server_flag() {
        let args = vec!["lakehouse_rag".to_string(), "--mcp-server".to_string()];
        assert_eq!(ModoEjecucion::desde_args(args), ModoEjecucion::McpServer);

        let args_corto = vec!["lakehouse_rag".to_string(), "-m".to_string()];
        assert_eq!(
            ModoEjecucion::desde_args(args_corto),
            ModoEjecucion::McpServer
        );
    }

    #[test]
    fn test_deteccion_mcp_info_flag() {
        let args = vec!["lakehouse_rag".to_string(), "--mcp-info".to_string()];
        assert_eq!(ModoEjecucion::desde_args(args), ModoEjecucion::McpInfo);
    }

    #[test]
    fn test_deteccion_help_flag() {
        let args = vec!["lakehouse_rag".to_string(), "--help".to_string()];
        assert_eq!(ModoEjecucion::desde_args(args), ModoEjecucion::Help);
    }

    #[test]
    fn test_modo_por_defecto_es_gui() {
        let args = vec!["lakehouse_rag".to_string()];
        assert_eq!(ModoEjecucion::desde_args(args), ModoEjecucion::Gui);
    }

    #[test]
    fn test_snippet_instalacion_escapa_barras_windows() {
        let snippet = ModoEjecucion::snippet_instalacion_mcp(r"D:\Proyectos\lakehouse_rag.exe");
        assert!(snippet.contains(r"D:\\Proyectos\\lakehouse_rag.exe"));
        assert!(snippet.contains("--mcp-server"));
    }
}
