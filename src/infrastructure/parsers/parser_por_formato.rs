//! Selección del parser por extensión del archivo: PDF por un lado y Markdown o texto por otro.

use std::path::Path;

use async_trait::async_trait;

use crate::application::dtos::centros::es_ruta_pdf;
use crate::application::dtos::ingestion::ParsedDocument;
use crate::application::ports::document_parser::DocumentParserPort;
use crate::domain::errors::DomainError;
use crate::infrastructure::parsers::markdown_parser::MarkdownParser;
use crate::infrastructure::parsers::pdf_parser::PdfParser;

/// Parser único de la composición real
#[derive(Default)]
pub struct ParserPorFormato {
    markdown: MarkdownParser,
    pdf: PdfParser,
}

impl ParserPorFormato {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DocumentParserPort for ParserPorFormato {
    async fn parse(
        &self,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<ParsedDocument, DomainError> {
        self.parse_bytes(nombre_archivo, contenido.as_bytes()).await
    }

    async fn parse_bytes(
        &self,
        nombre_archivo: &str,
        contenido: &[u8],
    ) -> Result<ParsedDocument, DomainError> {
        if es_ruta_pdf(Path::new(nombre_archivo)) {
            self.pdf.parse_bytes(nombre_archivo, contenido).await
        } else {
            let texto = std::str::from_utf8(contenido).map_err(|_| {
                DomainError::IngestionError(format!("'{nombre_archivo}' no es texto UTF-8"))
            })?;
            self.markdown.parse(nombre_archivo, texto).await
        }
    }
}
