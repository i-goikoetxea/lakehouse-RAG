//! DTOs para el agente de lenguaje Claude Headless, selector de modelos y mensajes

use serde::{Deserialize, Serialize};

use crate::application::dtos::tools::ToolCall;
use crate::domain::errors::DomainError;

/// Identificador validado de un modelo Claude (alias como `opus` o ID completo como
/// `claude-opus-5-5`). El catálogo real lo provee el puerto `ModelCatalogPort`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClaudeModel(String);

impl ClaudeModel {
    /// Longitud máxima admitida para un identificador de modelo
    pub const MAX_LEN: usize = 128;

    /// Crea un identificador validado; rechaza vacíos, flags de CLI y metacaracteres
    pub fn new(identificador: impl AsRef<str>) -> Result<Self, DomainError> {
        let limpio = identificador.as_ref().trim();
        if Self::es_identificador_seguro(limpio) {
            Ok(Self(limpio.to_string()))
        } else {
            Err(DomainError::AgentError(format!(
                "Identificador de modelo Claude inválido: '{}'",
                identificador.as_ref()
            )))
        }
    }

    /// Modelo por defecto del Lakehouse cuando la UI no especifica ninguno
    pub fn sonnet() -> Self {
        Self("sonnet".to_string())
    }

    /// Valor que se entrega a `claude --model <valor>`
    pub fn cli_model_name(&self) -> &str {
        &self.0
    }

    fn es_identificador_seguro(candidato: &str) -> bool {
        !candidato.is_empty()
            && candidato.len() <= Self::MAX_LEN
            && !candidato.starts_with('-')
            && candidato
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '[' | ']'))
    }
}

impl TryFrom<String> for ClaudeModel {
    type Error = DomainError;

    fn try_from(identificador: String) -> Result<Self, Self::Error> {
        Self::new(identificador)
    }
}

impl From<ClaudeModel> for String {
    fn from(modelo: ClaudeModel) -> Self {
        modelo.0
    }
}

/// Entrada del catálogo de modelos disponibles en la instalación local de Claude Code
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDescriptor {
    /// Valor a usar con `--model` (alias o ID completo)
    pub alias: ClaudeModel,
    /// ID real al que resuelve el alias
    pub resolved_model: String,
    /// Nombre legible para la UI
    pub display_name: String,
    /// Descripción corta de uso recomendado
    pub description: String,
}

/// Rol del emisor del mensaje
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "tool")]
    Tool,
}

/// Mensaje en el historial de conversación del agente
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: AgentRole,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

impl AgentMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: AgentRole::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: AgentRole::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn assistant_with_tools(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: AgentRole::Assistant,
            content: String::new(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: AgentRole::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// Salida de un turno del agente
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentTurnOutput {
    /// Respuesta final en texto al usuario
    FinalMessage(String),
    /// Solicitud de ejecución de una o más herramientas
    ToolCalls(Vec<ToolCall>),
}

/// Eventos emitidos durante la ejecución reactiva del agente (para TUI y streaming)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentExecutionEvent {
    Thought(String),
    CallingTool {
        name: String,
        arguments: serde_json::Value,
    },
    ToolResultReceived {
        name: String,
        result: serde_json::Value,
        is_error: bool,
    },
    TextDelta(String),
    Completed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_model_acepta_alias_e_ids_completos_dinamicos() {
        for alias in [
            "default",
            "sonnet",
            "opus",
            "haiku",
            "fable",
            "claude-opus-5-5",
            "claude-haiku-4-5-20251001",
            "sonnet[1m]",
        ] {
            let modelo = ClaudeModel::new(alias).expect("Alias válido rechazado");
            assert_eq!(modelo.cli_model_name(), alias);
        }
        assert_eq!(
            ClaudeModel::new("  opus  ").unwrap().cli_model_name(),
            "opus"
        );
        assert_eq!(ClaudeModel::sonnet().cli_model_name(), "sonnet");
    }

    #[test]
    fn test_claude_model_rechaza_identificadores_peligrosos_o_vacios() {
        let excesivo = "a".repeat(ClaudeModel::MAX_LEN + 1);
        for invalido in [
            "",
            "   ",
            "--dangerously-skip-permissions",
            "-p",
            "opus; rm -rf /",
            "opus && calc",
            "op us",
            "opus\n--tools",
            "modelo$(whoami)",
            excesivo.as_str(),
        ] {
            let err = ClaudeModel::new(invalido).unwrap_err();
            assert!(matches!(err, DomainError::AgentError(_)), "{invalido:?}");
        }
    }

    #[test]
    fn test_claude_model_serde_valida_al_deserializar() {
        let modelo: ClaudeModel = serde_json::from_str("\"claude-sonnet-5-5\"").unwrap();
        assert_eq!(modelo.cli_model_name(), "claude-sonnet-5-5");
        assert_eq!(
            serde_json::to_string(&modelo).unwrap(),
            "\"claude-sonnet-5-5\""
        );
        assert!(serde_json::from_str::<ClaudeModel>("\"--help\"").is_err());
    }

    #[test]
    fn test_model_descriptor_serializa_en_camel_case() {
        let descriptor = ModelDescriptor {
            alias: ClaudeModel::new("opus").unwrap(),
            resolved_model: "claude-opus-5-5".to_string(),
            display_name: "Opus 5.5".to_string(),
            description: "Best for everyday, complex tasks".to_string(),
        };
        let json = serde_json::to_value(&descriptor).unwrap();
        assert_eq!(json["alias"], "opus");
        assert_eq!(json["resolvedModel"], "claude-opus-5-5");
        assert_eq!(json["displayName"], "Opus 5.5");
    }

    #[test]
    fn test_agent_message_builders() {
        let u = AgentMessage::user("Hola Lakehouse");
        assert_eq!(u.role, AgentRole::User);
        assert_eq!(u.content, "Hola Lakehouse");

        let a = AgentMessage::assistant("Respuesta final");
        assert_eq!(a.role, AgentRole::Assistant);

        let t = AgentMessage::tool_result("call_1", "{\"ok\": true}");
        assert_eq!(t.role, AgentRole::Tool);
        assert_eq!(t.tool_call_id.as_deref(), Some("call_1"));
    }
}
