//! Parsers documentales estructurales (Markdown, PDF, tablas)

pub mod markdown_parser;
pub mod parser_por_formato;
pub mod pdf_estructura;
pub mod pdf_parser;

pub use markdown_parser::MarkdownParser;
pub use parser_por_formato::ParserPorFormato;
pub use pdf_parser::PdfParser;
