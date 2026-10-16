//! Puerto para el parser jerárquico de documentos técnicos

use async_trait::async_trait;

use crate::application::dtos::ingestion::ParsedDocument;
use crate::domain::errors::DomainError;

#[async_trait]
pub trait DocumentParserPort: Send + Sync {
    /// Parsea el contenido de un documento estructurado preservando jerarquía de títulos y tablas
    async fn parse(
        &self,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<ParsedDocument, DomainError>;
}
