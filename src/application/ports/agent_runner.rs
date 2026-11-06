//! Puerto de aplicación para el ejecutor del agente Claude Headless

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::dtos::agent::{AgentMessage, AgentTurnOutput, ClaudeModel, TurnoDirigido};
use crate::application::dtos::tools::{LakehouseToolName, ToolDefinition};
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

    /// Como `ejecutar_turno`, pero notifica en vivo los fragmentos de texto VISIBLES de la
    /// respuesta final (nunca los de una petición de herramientas). Por defecto no hay
    /// streaming y se delega en `ejecutar_turno`.
    async fn ejecutar_turno_en_vivo(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
        observador: ObservadorTexto,
    ) -> Result<AgentTurnOutput, DomainError> {
        let _ = observador;
        self.ejecutar_turno(modelo, system_prompt, historial, herramientas)
            .await
    }

    /// Solicita la llamada a `dirigida`, sin garantizarla: el modelo puede no hacerla y elige los
    /// argumentos. `herramientas` es el catálogo que ve el modelo, como en un turno normal.
    async fn ejecutar_turno_dirigido(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
        dirigida: LakehouseToolName,
    ) -> Result<TurnoDirigido, DomainError> {
        let _ = (modelo, system_prompt, historial, herramientas);
        Err(DomainError::AgentError(format!(
            "Este ejecutor no admite turnos dirigidos ({})",
            dirigida.as_str()
        )))
    }
}

/// Recibe cada fragmento de texto visible de la respuesta mientras llega
pub type ObservadorTexto = Arc<dyn Fn(&str) + Send + Sync>;
