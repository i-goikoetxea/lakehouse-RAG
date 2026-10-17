//! Adaptador que descubre los modelos disponibles consultando a Claude Code en modo headless
//!
//! Usa el protocolo de control `stream-json` de la CLI: se envía una solicitud `initialize`
//! por stdin y la respuesta (`control_response`) incluye el catálogo `models` de la cuenta.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::application::dtos::agent::{ClaudeModel, ModelDescriptor};
use crate::application::ports::model_catalog::ModelCatalogPort;
use crate::domain::errors::DomainError;
use crate::infrastructure::llm::ocultar_consola_en_windows;

/// Configuración del descubrimiento de modelos vía Claude Code headless
#[derive(Debug, Clone)]
pub struct ClaudeModelCatalogConfig {
    pub binary_path: PathBuf,
    pub timeout_duration: Duration,
    /// Máximo de líneas de stdout a inspeccionar antes de abandonar
    pub max_lineas_salida: usize,
}

impl Default for ClaudeModelCatalogConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("claude"),
            timeout_duration: Duration::from_secs(45),
            max_lineas_salida: 500,
        }
    }
}

/// Catálogo de modelos respaldado por la instalación local de Claude Code
pub struct ClaudeHeadlessModelCatalog {
    config: ClaudeModelCatalogConfig,
}

impl ClaudeHeadlessModelCatalog {
    /// Identificador de correlación de la solicitud de control
    pub const REQUEST_ID: &'static str = "lakehouse-model-catalog";
    /// Longitud máxima de textos descriptivos aceptados para la UI
    pub const MAX_TEXTO_LEN: usize = 200;

    pub fn new(config: ClaudeModelCatalogConfig) -> Self {
        Self { config }
    }

    /// Línea JSON (terminada en salto de línea) con la solicitud `initialize`
    pub fn construir_solicitud_initialize() -> String {
        let solicitud = serde_json::json!({
            "type": "control_request",
            "request_id": Self::REQUEST_ID,
            "request": { "subtype": "initialize" }
        });
        format!("{solicitud}\n")
    }

    /// Interpreta una línea de stdout. Devuelve `None` si no es la respuesta esperada.
    pub fn parsear_respuesta_initialize(
        linea: &str,
    ) -> Option<Result<Vec<ModelDescriptor>, DomainError>> {
        let mensaje: Value = serde_json::from_str(linea.trim()).ok()?;
        if mensaje.get("type").and_then(Value::as_str) != Some("control_response") {
            return None;
        }
        let respuesta = mensaje.get("response")?;
        if respuesta.get("request_id").and_then(Value::as_str) != Some(Self::REQUEST_ID) {
            return None;
        }
        Some(Self::extraer_modelos(respuesta))
    }

    fn extraer_modelos(respuesta: &Value) -> Result<Vec<ModelDescriptor>, DomainError> {
        if respuesta.get("subtype").and_then(Value::as_str) != Some("success") {
            let detalle = respuesta
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("respuesta sin detalle");
            return Err(DomainError::AgentError(format!(
                "Claude Code rechazó la solicitud de catálogo de modelos: {detalle}"
            )));
        }

        let entradas = respuesta
            .get("response")
            .and_then(|r| r.get("models"))
            .and_then(Value::as_array)
            .ok_or_else(|| {
                DomainError::AgentError(
                    "La respuesta de Claude Code no contiene el catálogo 'models'".to_string(),
                )
            })?;

        let mut vistos = HashSet::new();
        let modelos: Vec<ModelDescriptor> = entradas
            .iter()
            .filter_map(Self::descriptor_desde_json)
            .filter(|d| vistos.insert(d.alias.clone()))
            .collect();

        if modelos.is_empty() {
            return Err(DomainError::AgentError(
                "Claude Code no reportó ningún modelo válido disponible".to_string(),
            ));
        }
        Ok(modelos)
    }

    fn descriptor_desde_json(entrada: &Value) -> Option<ModelDescriptor> {
        let alias = ClaudeModel::new(entrada.get("value")?.as_str()?).ok()?;
        let texto = |campo: &str| {
            entrada
                .get(campo)
                .and_then(Value::as_str)
                .map(Self::recortar_texto)
        };
        Some(ModelDescriptor {
            resolved_model: texto("resolvedModel")
                .unwrap_or_else(|| alias.cli_model_name().to_string()),
            display_name: texto("displayName")
                .unwrap_or_else(|| alias.cli_model_name().to_string()),
            description: texto("description").unwrap_or_default(),
            alias,
        })
    }

    fn recortar_texto(texto: &str) -> String {
        texto.trim().chars().take(Self::MAX_TEXTO_LEN).collect()
    }

    fn construir_comando(&self) -> Command {
        let mut cmd = Command::new(&self.config.binary_path);
        cmd.args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--no-session-persistence",
            "--strict-mcp-config",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
        ocultar_consola_en_windows(&mut cmd);
        cmd
    }

    async fn consultar_catalogo(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        let error_io = |e: std::io::Error| {
            DomainError::AgentError(format!(
                "Fallo comunicando con Claude CLI en '{:?}': {e}",
                self.config.binary_path
            ))
        };

        let mut child = self.construir_comando().spawn().map_err(error_io)?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            DomainError::AgentError("No se pudo abrir stdin de Claude CLI".to_string())
        })?;
        stdin
            .write_all(Self::construir_solicitud_initialize().as_bytes())
            .await
            .map_err(error_io)?;
        stdin.flush().await.map_err(error_io)?;

        let stdout = child.stdout.take().ok_or_else(|| {
            DomainError::AgentError("No se pudo abrir stdout de Claude CLI".to_string())
        })?;
        let mut lineas = BufReader::new(stdout).lines();

        for _ in 0..self.config.max_lineas_salida {
            match lineas.next_line().await.map_err(error_io)? {
                Some(linea) => {
                    if let Some(resultado) = Self::parsear_respuesta_initialize(&linea) {
                        return resultado;
                    }
                }
                None => break,
            }
        }

        Err(DomainError::AgentError(
            "Claude CLI finalizó sin devolver el catálogo de modelos".to_string(),
        ))
    }
}

#[async_trait]
impl ModelCatalogPort for ClaudeHeadlessModelCatalog {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        timeout(self.config.timeout_duration, self.consultar_catalogo())
            .await
            .map_err(|_| {
                DomainError::AgentError(format!(
                    "Timeout agotado ({:?}) esperando el catálogo de modelos de Claude CLI",
                    self.config.timeout_duration
                ))
            })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn respuesta_exitosa(models: Value) -> String {
        json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": ClaudeHeadlessModelCatalog::REQUEST_ID,
                "response": { "commands": [], "models": models }
            }
        })
        .to_string()
    }

    #[test]
    fn test_solicitud_initialize_es_linea_json_de_control() {
        let linea = ClaudeHeadlessModelCatalog::construir_solicitud_initialize();
        assert!(linea.ends_with('\n'));
        let val: Value = serde_json::from_str(linea.trim()).unwrap();
        assert_eq!(val["type"], "control_request");
        assert_eq!(val["request_id"], ClaudeHeadlessModelCatalog::REQUEST_ID);
        assert_eq!(val["request"]["subtype"], "initialize");
    }

    #[test]
    fn test_parsea_catalogo_real_preservando_orden() {
        let linea = respuesta_exitosa(json!([
            { "value": "default", "resolvedModel": "claude-opus-5-5",
              "displayName": "Default (recommended)", "description": "Opus 5.5" },
            { "value": "haiku", "resolvedModel": "claude-haiku-5-5",
              "displayName": "Haiku 5.5", "description": "Fastest for quick answers" },
            { "value": "claude-haiku-4-5-20251001", "displayName": "Haiku 4.5" }
        ]));

        let modelos = ClaudeHeadlessModelCatalog::parsear_respuesta_initialize(&linea)
            .expect("Debe reconocer la respuesta")
            .expect("Debe ser exitosa");

        assert_eq!(modelos.len(), 3);
        assert_eq!(modelos[0].alias.cli_model_name(), "default");
        assert_eq!(modelos[0].resolved_model, "claude-opus-5-5");
        assert_eq!(modelos[1].display_name, "Haiku 5.5");
        assert_eq!(modelos[2].resolved_model, "claude-haiku-4-5-20251001");
        assert_eq!(modelos[2].description, "");
    }

    #[test]
    fn test_ignora_lineas_ajenas_a_la_respuesta() {
        for linea in [
            r#"{"type":"system","subtype":"init","model":"claude-opus-5-5"}"#,
            r#"{"type":"control_response","response":{"subtype":"success","request_id":"otro","response":{"models":[]}}}"#,
            "texto libre no JSON",
            "",
        ] {
            assert!(ClaudeHeadlessModelCatalog::parsear_respuesta_initialize(linea).is_none());
        }
    }

    #[test]
    fn test_respuesta_de_error_o_sin_models_retorna_error_controlado() {
        let error = json!({
            "type": "control_response",
            "response": { "subtype": "error", "request_id": ClaudeHeadlessModelCatalog::REQUEST_ID,
                          "error": "not logged in" }
        })
        .to_string();
        let sin_models = json!({
            "type": "control_response",
            "response": { "subtype": "success", "request_id": ClaudeHeadlessModelCatalog::REQUEST_ID,
                          "response": { "models": "no-es-array" } }
        })
        .to_string();

        for linea in [error, sin_models, respuesta_exitosa(json!([]))] {
            let resultado = ClaudeHeadlessModelCatalog::parsear_respuesta_initialize(&linea)
                .expect("Debe reconocer la respuesta");
            assert!(matches!(resultado, Err(DomainError::AgentError(_))));
        }
    }

    #[test]
    fn test_descarta_entradas_invalidas_y_duplicadas() {
        let linea = respuesta_exitosa(json!([
            { "value": "--dangerously-skip-permissions", "displayName": "Malicioso" },
            { "displayName": "Sin value" },
            "no-es-objeto",
            { "value": 42 },
            { "value": "opus", "displayName": "Opus 5.5" },
            { "value": "opus", "displayName": "Opus duplicado" }
        ]));

        let modelos = ClaudeHeadlessModelCatalog::parsear_respuesta_initialize(&linea)
            .unwrap()
            .unwrap();
        assert_eq!(modelos.len(), 1);
        assert_eq!(modelos[0].display_name, "Opus 5.5");
    }

    #[tokio::test]
    async fn test_binario_inexistente_retorna_error_controlado() {
        let catalogo = ClaudeHeadlessModelCatalog::new(ClaudeModelCatalogConfig {
            binary_path: PathBuf::from("binario_claude_no_existente_xyz"),
            timeout_duration: Duration::from_millis(500),
            max_lineas_salida: 10,
        });
        let err = catalogo.listar_modelos().await.unwrap_err();
        assert!(matches!(err, DomainError::AgentError(_)));
    }
}
