//! Puerto y estructuras para el motor de búsqueda híbrida (Vectorial + BM25 + Filtros Arrow)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;

/// Registro de coincidencia enriquecido obtenido tras la búsqueda híbrida
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub chunk_id: String,
    pub doc_id: String,
    pub centro_id: String,
    pub seccion: String,
    pub contenido: String,
    pub score_vectorial: f32,
    pub score_bm25: f32,
    pub score_hibrido: f32,
    pub vigente: bool,
}

/// Parámetros de configuración para la ejecución de una búsqueda híbrida
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchParams {
    pub query: String,
    pub centro_id: String,
    pub top_k: usize,
    pub solo_vigentes: bool,
    pub peso_vectorial: f32,
    pub peso_bm25: f32,
    pub rrf_k: f32,
}

impl Default for HybridSearchParams {
    fn default() -> Self {
        Self {
            query: String::new(),
            centro_id: String::new(),
            top_k: 10,
            solo_vigentes: true,
            peso_vectorial: 0.6,
            peso_bm25: 0.4,
            rrf_k: 60.0,
        }
    }
}

/// Puerto de salida para el motor de búsqueda híbrida
#[async_trait]
pub trait HybridSearchPort: Send + Sync {
    /// Ejecuta una búsqueda híbrida combinando similitud vectorial y coincidencia léxica BM25
    async fn hybrid_search(
        &self,
        params: &HybridSearchParams,
    ) -> Result<Vec<SearchHit>, DomainError>;
}
