//! Puerto para motores de generación de embeddings vectoriales

use async_trait::async_trait;

use crate::domain::errors::DomainError;

/// Puerto de abstracción para la generación de representaciones vectoriales densas
#[async_trait]
pub trait EmbeddingEnginePort: Send + Sync {
    /// Genera el vector de embeddings para una cadena de texto
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, DomainError>;

    /// Genera un lote de vectores para múltiples textos
    async fn generate_batch_embeddings(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, DomainError>;

    /// Retorna la dimensión esperada de los vectores (ej. 1024)
    fn dimension(&self) -> usize;
}
