//! Adaptador para invocar el agente Claude Headless CLI con selección dinámica de modelo

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use crate::application::dtos::agent::{AgentMessage, AgentTurnOutput, ClaudeModel};
use crate::application::dtos::tools::{ToolCall, ToolDefinition};
use crate::application::ports::agent_runner::AgentRunnerPort;
use crate::domain::errors::DomainError;

/// Configuración para la ejecución de Claude Headless
#[derive(Debug, Clone)]
pub struct ClaudeAgentConfig {
    pub binary_path: PathBuf,
    pub timeout_duration: Duration,
    pub extra_args: Vec<String>,
}

impl Default for ClaudeAgentConfig {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("claude"),
            timeout_duration: Duration::from_secs(60),
            extra_args: Vec::new(),
        }
    }
}

/// Adaptador de infraestructura que orquesta llamadas al ejecutable Claude CLI
pub struct ClaudeAgentRunner {
    config: ClaudeAgentConfig,
}

impl ClaudeAgentRunner {
    pub fn new(config: ClaudeAgentConfig) -> Self {
        Self { config }
    }

    /// Limpia bloques delimitadores de markdown tipo ```json ... ```
    pub fn limpiar_json_markdown(raw: &str) -> String {
        let trimmed = raw.trim();
        if trimmed.starts_with("```") {
            let mut lineas = trimmed.lines();
            lineas.next(); // saltar primera línea ```json
            let mut contenido = Vec::new();
            for linea in lineas {
                if linea.trim() == "```" {
                    break;
                }
                contenido.push(linea);
            }
            contenido.join("\n").trim().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Intenta parsear si la respuesta del LLM es una solicitud de llamada a herramientas
    pub fn parsear_salida_claude(stdout: &str) -> AgentTurnOutput {
        let limpio = Self::limpiar_json_markdown(stdout);

        // 1. Intentar parsear como objeto JSON estructurado con "tool_calls" o "type": "tool_use"
        if let Ok(val) = serde_json::from_str::<Value>(&limpio) {
            if let Some(tool_calls_arr) = val.get("tool_calls").and_then(|v| v.as_array()) {
                let mut llamadas = Vec::new();
                for (idx, tc) in tool_calls_arr.iter().enumerate() {
                    let name = tc
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let id = tc
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("call_{idx}"))
                        .to_string();
                    let arguments = tc
                        .get("arguments")
                        .or_else(|| tc.get("input"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    if !name.is_empty() {
                        llamadas.push(ToolCall {
                            id,
                            name,
                            arguments,
                        });
                    }
                }
                if !llamadas.is_empty() {
                    return AgentTurnOutput::ToolCalls(llamadas);
                }
            }

            if val.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                let name = val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let id = val
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("call_0")
                    .to_string();
                let arguments = val
                    .get("input")
                    .or_else(|| val.get("arguments"))
                    .cloned()
                    .unwrap_or(Value::Null);
                if !name.is_empty() {
                    return AgentTurnOutput::ToolCalls(vec![ToolCall {
                        id,
                        name,
                        arguments,
                    }]);
                }
            }
        }

        // Si no es un bloque de llamadas a herramientas, es el mensaje final
        AgentTurnOutput::FinalMessage(stdout.trim().to_string())
    }
}

#[async_trait]
impl AgentRunnerPort for ClaudeAgentRunner {
    async fn ejecutar_turno(
        &self,
        modelo: ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        let prompt_payload = serde_json::json!({
            "system": system_prompt,
            "tools": herramientas,
            "messages": historial,
        });

        let prompt_str = serde_json::to_string(&prompt_payload).map_err(|e| {
            DomainError::AgentError(format!("Error serializando payload para Claude: {e}"))
        })?;

        let mut cmd = Command::new(&self.config.binary_path);
        cmd.arg("-p")
            .arg(&prompt_str)
            .arg("--model")
            .arg(modelo.cli_model_name());

        for arg in &self.config.extra_args {
            cmd.arg(arg);
        }

        let child_future = cmd.output();

        let output = match timeout(self.config.timeout_duration, child_future).await {
            Ok(res) => res.map_err(|e| {
                DomainError::AgentError(format!(
                    "Fallo ejecutando binario Claude CLI en '{:?}': {e}",
                    self.config.binary_path
                ))
            })?,
            Err(_) => {
                return Err(DomainError::AgentError(format!(
                    "Timeout agotado ({:?}) esperando respuesta de Claude CLI",
                    self.config.timeout_duration
                )));
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::AgentError(format!(
                "Claude CLI terminó con error (código {:?}): {stderr}",
                output.status.code()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(Self::parsear_salida_claude(&stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parsear_salida_claude_tool_calls_array() {
        let raw = r#"```json
        {
            "tool_calls": [
                {
                    "id": "call_123",
                    "name": "inspect_lakehouse_catalog",
                    "arguments": { "centro_id": "planta_alfa", "solo_vigentes": true }
                }
            ]
        }
        ```"#;

        let res = ClaudeAgentRunner::parsear_salida_claude(raw);
        match res {
            AgentTurnOutput::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].id, "call_123");
                assert_eq!(calls[0].name, "inspect_lakehouse_catalog");
                assert_eq!(calls[0].arguments["centro_id"], "planta_alfa");
            }
            _ => panic!("Se esperaba ToolCalls"),
        }
    }

    #[test]
    fn test_parsear_salida_claude_mensaje_final_plano() {
        let raw = "El documento DOC-001 es la norma vigente para este proceso.";
        let res = ClaudeAgentRunner::parsear_salida_claude(raw);
        assert_eq!(res, AgentTurnOutput::FinalMessage(raw.to_string()));
    }

    #[tokio::test]
    async fn test_claude_runner_binario_inexistente_retorna_error_controlado() {
        let runner = ClaudeAgentRunner::new(ClaudeAgentConfig {
            binary_path: PathBuf::from("binario_claude_no_existente_xyz"),
            timeout_duration: Duration::from_millis(500),
            extra_args: Vec::new(),
        });

        let err = runner
            .ejecutar_turno(ClaudeModel::Sonnet, "sys", &[], &[])
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::AgentError(_)));
    }
}
