//! Puerto de aplicación para el servidor de herramientas del Lakehouse

use async_trait::async_trait;

use crate::application::dtos::tools::{ToolCall, ToolDefinition, ToolResult};
use crate::domain::errors::DomainError;

/// Puerto que expone las herramientas operativas del Lakehouse para agentes y LLMs
#[async_trait]
pub trait ToolServerPort: Send + Sync {
    /// Retorna el catálogo de herramientas disponibles con sus esquemas JSON Schema
    fn listar_herramientas(&self) -> Vec<ToolDefinition>;

    /// Ejecuta de manera determinista y segura una herramienta solicitada
    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError>;
}
