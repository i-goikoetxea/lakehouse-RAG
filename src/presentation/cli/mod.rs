//! Módulo de Comandos de Consola (CLI) y Configuración de Lanzamiento
//!
//! Proporciona parsing de argumentos de consola para alternar entre el servidor
//! MCP por stdio, la aplicación de escritorio y los subcomandos de la fachada
//! `LakehouseApp` (centros, ingesta, preguntas e integridad).

pub mod ejecutor;
pub mod proceso;
pub mod salida;

/// Subcomandos de consola sobre la composición única
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComandoCli {
    Centros,
    CrearCentro {
        id: String,
        ruta: String,
        nombre: Option<String>,
    },
    Activar(String),
    Desactivar(String),
    EliminarCentro(String),
    Ingest {
        archivo: String,
        centro: String,
    },
    Ask {
        texto: String,
        centro: String,
        modelo: Option<String>,
    },
    Integridad(String),
}

/// Formato de la salida de los subcomandos
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatoSalida {
    Legible,
    Json,
}

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
    /// Subcomando de la fachada (`--centros`, `--ingest`, `--ask`…)
    Comando(ComandoCli, FormatoSalida),
    /// Argumentos mal formados (mensaje para stderr)
    ErrorArgumentos(String),
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

        let formato = if lista.iter().any(|arg| arg == "--json") {
            FormatoSalida::Json
        } else {
            FormatoSalida::Legible
        };
        match Self::comando_desde(&lista) {
            Ok(Some(comando)) => Self::Comando(comando, formato),
            Ok(None) => Self::Gui,
            Err(mensaje) => Self::ErrorArgumentos(mensaje),
        }
    }

    fn comando_desde(lista: &[String]) -> Result<Option<ComandoCli>, String> {
        let argumentos = ArgumentosCli { lista };
        if argumentos.presente("--centros") {
            return Ok(Some(ComandoCli::Centros));
        }
        if let Some(posicion) = argumentos.posicion("--crear-centro") {
            let uso = "--crear-centro <id> <ruta> [nombre]";
            let id = argumentos.valor_en(posicion + 1, uso)?;
            let ruta = argumentos.valor_en(posicion + 2, uso)?;
            let nombre = argumentos.valor_en(posicion + 3, uso).ok();
            return Ok(Some(ComandoCli::CrearCentro { id, ruta, nombre }));
        }
        let por_identificador: [(&str, ConstructorPorId); 4] = [
            ("--activar", ComandoCli::Activar),
            ("--desactivar", ComandoCli::Desactivar),
            ("--eliminar-centro", ComandoCli::EliminarCentro),
            ("--integridad", ComandoCli::Integridad),
        ];
        for (bandera, constructor) in por_identificador {
            if let Some(id) = argumentos.valor_de(bandera)? {
                return Ok(Some(constructor(id)));
            }
        }
        if let Some(archivo) = argumentos.valor_de("--ingest")? {
            let centro = argumentos.requerido("--centro", "--ingest <archivo> --centro <id>")?;
            return Ok(Some(ComandoCli::Ingest { archivo, centro }));
        }
        if let Some(texto) = argumentos.valor_de("--ask")? {
            let centro = argumentos.requerido("--centro", "--ask \"<texto>\" --centro <id>")?;
            let modelo = argumentos.valor_de("--modelo")?;
            return Ok(Some(ComandoCli::Ask {
                texto,
                centro,
                modelo,
            }));
        }
        Ok(None)
    }

    /// Retorna el texto de ayuda del CLI
    pub fn texto_ayuda() -> &'static str {
        r#"
=== Lakehouse RAG Industrial (SOTA 2026) ===

USO:
    lakehouse_rag.exe [OPCIONES]        Ventana (por defecto) o subcomandos
    lakehouse_rag_cli.exe [OPCIONES]    Solo consola: PowerShell lo espera y $LASTEXITCODE es fiable

OPCIONES:
    --mcp-server, -m    Inicia el servidor MCP en modo stdio (para Claude Desktop, Claude Code, Cursor, Gemini)
    --mcp-info          Muestra la configuración JSON para instalar este servidor en Claude Desktop / Claude Code
    --gui               Inicia la aplicación gráfica de escritorio (por defecto)
    -h, --help          Muestra esta ayuda

CENTROS E INGESTA (salida legible; añade --json para JSON):
    --centros                                Lista los centros registrados
    --crear-centro <id> <ruta> [nombre]      Crea un centro en <ruta> (queda inactivo)
    --activar <id> | --desactivar <id>       Activa (proyecta) o desactiva (cero acceso)
    --eliminar-centro <id>                   Lo quita del registro sin borrar el disco
    --ingest <archivo> --centro <id>         Ingesta transaccional de un Markdown
    --ask "<texto>" --centro <id> [--modelo <alias>]
                                             Pregunta al agente sobre un centro activo
    --integridad <id>                        Huérfanos, STAGED colgadas y tablas ausentes

    El registro vive en %APPDATA%\lakehouse_rag\centers_registry.json
    (o en el directorio de LAKEHOUSE_RAG_CONFIG_DIR).
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

/// Subcomando que solo recibe el identificador de un centro
type ConstructorPorId = fn(String) -> ComandoCli;

/// Acceso a los argumentos con mensajes de error uniformes
struct ArgumentosCli<'a> {
    lista: &'a [String],
}

impl ArgumentosCli<'_> {
    fn presente(&self, bandera: &str) -> bool {
        self.lista.iter().any(|a| a == bandera)
    }

    fn posicion(&self, bandera: &str) -> Option<usize> {
        self.lista.iter().position(|a| a == bandera)
    }

    /// Valor posicional que no sea otra bandera
    fn valor_en(&self, posicion: usize, uso: &str) -> Result<String, String> {
        match self.lista.get(posicion) {
            Some(valor) if !valor.starts_with("--") => Ok(valor.clone()),
            _ => Err(format!("Faltan argumentos. Uso: {uso}")),
        }
    }

    fn valor_de(&self, bandera: &str) -> Result<Option<String>, String> {
        match self.posicion(bandera) {
            Some(posicion) => self
                .valor_en(posicion + 1, &format!("{bandera} <valor>"))
                .map(Some),
            None => Ok(None),
        }
    }

    fn requerido(&self, bandera: &str, uso: &str) -> Result<String, String> {
        self.valor_de(bandera)?
            .ok_or_else(|| format!("Falta {bandera}. Uso: {uso}"))
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

    fn args(lista: &[&str]) -> Vec<String> {
        std::iter::once("lakehouse_rag")
            .chain(lista.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn test_subcomandos_de_centros() {
        assert_eq!(
            ModoEjecucion::desde_args(args(&["--centros", "--json"])),
            ModoEjecucion::Comando(ComandoCli::Centros, FormatoSalida::Json)
        );
        assert_eq!(
            ModoEjecucion::desde_args(args(&["--crear-centro", "obra-a", "D:/a", "Obra A"])),
            ModoEjecucion::Comando(
                ComandoCli::CrearCentro {
                    id: "obra-a".into(),
                    ruta: "D:/a".into(),
                    nombre: Some("Obra A".into())
                },
                FormatoSalida::Legible
            )
        );
        assert_eq!(
            ModoEjecucion::desde_args(args(&["--crear-centro", "obra-a", "D:/a", "--json"])),
            ModoEjecucion::Comando(
                ComandoCli::CrearCentro {
                    id: "obra-a".into(),
                    ruta: "D:/a".into(),
                    nombre: None
                },
                FormatoSalida::Json
            )
        );
        assert_eq!(
            ModoEjecucion::desde_args(args(&["--activar", "obra-a"])),
            ModoEjecucion::Comando(ComandoCli::Activar("obra-a".into()), FormatoSalida::Legible)
        );
        assert_eq!(
            ModoEjecucion::desde_args(args(&["--integridad", "obra-a"])),
            ModoEjecucion::Comando(
                ComandoCli::Integridad("obra-a".into()),
                FormatoSalida::Legible
            )
        );
    }

    #[test]
    fn test_subcomandos_de_ingesta_y_pregunta() {
        assert_eq!(
            ModoEjecucion::desde_args(args(&["--ingest", "rev03.md", "--centro", "obra-a"])),
            ModoEjecucion::Comando(
                ComandoCli::Ingest {
                    archivo: "rev03.md".into(),
                    centro: "obra-a".into()
                },
                FormatoSalida::Legible
            )
        );
        assert_eq!(
            ModoEjecucion::desde_args(args(&[
                "--ask",
                "¿Quién custodia?",
                "--centro",
                "obra-a",
                "--modelo",
                "haiku",
                "--json"
            ])),
            ModoEjecucion::Comando(
                ComandoCli::Ask {
                    texto: "¿Quién custodia?".into(),
                    centro: "obra-a".into(),
                    modelo: Some("haiku".into())
                },
                FormatoSalida::Json
            )
        );
    }

    #[test]
    fn test_argumentos_incompletos_dan_error_de_uso() {
        for incompleto in [
            args(&["--ingest", "rev03.md"]),
            args(&["--ask", "hola"]),
            args(&["--activar"]),
            args(&["--activar", "--json"]),
            args(&["--crear-centro", "obra-a"]),
        ] {
            assert!(
                matches!(
                    ModoEjecucion::desde_args(incompleto.clone()),
                    ModoEjecucion::ErrorArgumentos(_)
                ),
                "{incompleto:?}"
            );
        }
    }

    #[test]
    fn test_snippet_instalacion_escapa_barras_windows() {
        let snippet = ModoEjecucion::snippet_instalacion_mcp(r"D:\Proyectos\lakehouse_rag.exe");
        assert!(snippet.contains(r"D:\\Proyectos\\lakehouse_rag.exe"));
        assert!(snippet.contains("--mcp-server"));
    }
}
