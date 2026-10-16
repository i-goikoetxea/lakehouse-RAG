//! Puerto para la destilación estructurada de documentos vía LLM (Claude Headless)

use async_trait::async_trait;

use crate::application::dtos::ingestion::{DistilledDocumentPackage, ParsedDocument};
use crate::domain::errors::DomainError;

#[async_trait]
pub trait LlmDistillerPort: Send + Sync {
    /// Destila la información no estructurada de un documento técnico en el paquete estructurado para el Lakehouse
    async fn distill(
        &self,
        parsed: &ParsedDocument,
    ) -> Result<DistilledDocumentPackage, DomainError>;

    /// Retorna el JSON Schema en formato string para garantizar salidas estructuradas
    fn json_schema(&self) -> String {
        let schema = schemars::schema_for!(DistilledDocumentPackage);
        serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string())
    }
}
