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

    /// Parsea los bytes del original. Por defecto los interpreta como texto UTF-8; los formatos
    /// binarios (PDF) lo sustituyen.
    async fn parse_bytes(
        &self,
        nombre_archivo: &str,
        contenido: &[u8],
    ) -> Result<ParsedDocument, DomainError> {
        let texto = std::str::from_utf8(contenido).map_err(|_| {
            DomainError::IngestionError(format!("'{nombre_archivo}' no es texto UTF-8"))
        })?;
        self.parse(nombre_archivo, texto).await
    }
}
