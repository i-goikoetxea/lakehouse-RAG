//! Puerto de aplicación para el ejecutor del agente Claude Headless

use async_trait::async_trait;

use crate::application::dtos::agent::{AgentMessage, AgentTurnOutput, ClaudeModel};
use crate::application::dtos::tools::ToolDefinition;
use crate::domain::errors::DomainError;

/// Puerto para la ejecución de turnos de conversación con LLM y llamada a herramientas
#[async_trait]
pub trait AgentRunnerPort: Send + Sync {
    /// Ejecuta un turno del agente recibiendo el historial, modelo y catálogo de herramientas
    async fn ejecutar_turno(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError>;
}
