//! DTOs para el agente de lenguaje Claude Headless, selector de modelos y mensajes

use serde::{Deserialize, Serialize};

use crate::application::dtos::tools::ToolCall;
use crate::domain::errors::DomainError;

/// Modelos Claude disponibles con conmutación dinámica
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClaudeModel {
    #[serde(rename = "sonnet")]
    Sonnet,
    #[serde(rename = "opus")]
    Opus,
    #[serde(rename = "haiku")]
    Haiku,
}

impl ClaudeModel {
    /// Identificador oficial del modelo de Anthropic
    pub fn model_id(&self) -> &'static str {
        match self {
            Self::Sonnet => "claude-3-7-sonnet-20250219",
            Self::Opus => "claude-3-opus-20240229",
            Self::Haiku => "claude-3-5-haiku-20241022",
        }
    }

    /// Nombre corto de alias para la CLI `claude --model <alias>`
    pub fn cli_model_name(&self) -> &'static str {
        match self {
            Self::Sonnet => "sonnet",
            Self::Opus => "opus",
            Self::Haiku => "haiku",
        }
    }

    /// Parsea de forma flexible cadenas de texto para conmutador rápido de UI/CLI
    pub fn from_str_loose(s: &str) -> Result<Self, DomainError> {
        let limpio = s.trim().to_lowercase();
        if limpio.contains("sonnet") || limpio == "3.7" || limpio == "3.5-s" {
            Ok(Self::Sonnet)
        } else if limpio.contains("opus") || limpio == "3-opus" {
            Ok(Self::Opus)
        } else if limpio.contains("haiku") || limpio == "flash" || limpio == "mini" {
            Ok(Self::Haiku)
        } else {
            Err(DomainError::AgentError(format!(
                "Modelo Claude no reconocido: '{s}'. Modelos válidos: sonnet, opus, haiku"
            )))
        }
    }
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
    fn test_claude_model_selection_and_loose_parsing() {
        assert_eq!(
            ClaudeModel::from_str_loose("sonnet").unwrap(),
            ClaudeModel::Sonnet
        );
        assert_eq!(
            ClaudeModel::from_str_loose("Claude-3.7-Sonnet").unwrap(),
            ClaudeModel::Sonnet
        );
        assert_eq!(
            ClaudeModel::from_str_loose("opus").unwrap(),
            ClaudeModel::Opus
        );
        assert_eq!(
            ClaudeModel::from_str_loose("haiku").unwrap(),
            ClaudeModel::Haiku
        );

        assert_eq!(ClaudeModel::Sonnet.cli_model_name(), "sonnet");
        assert_eq!(ClaudeModel::Sonnet.model_id(), "claude-3-7-sonnet-20250219");

        let err = ClaudeModel::from_str_loose("gpt-4o").unwrap_err();
        assert!(matches!(err, DomainError::AgentError(_)));
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
