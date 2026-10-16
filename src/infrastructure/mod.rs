//! Capa de Infraestructura (Adaptadores)
//!
//! Implementa los puertos definidos en la capa de aplicación interactuando
//! con el sistema de archivos, LanceDB, Apache DataFusion, FastEmbed y Claude CLI.

pub mod embeddings;
pub mod llm;
pub mod parsers;
pub mod query_engine;
pub mod storage;
