//! Adaptador de embeddings locales utilizando FastEmbed (ONNX Runtime)

use std::sync::Arc;

use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::domain::errors::DomainError;

/// Adaptador de infraestructura que ejecuta inferencia ONNX local mediante FastEmbed
pub struct FastEmbedEngine {
    model: Arc<tokio::sync::Mutex<TextEmbedding>>,
    dimension: usize,
}

impl FastEmbedEngine {
    /// Inicializa el motor con un modelo FastEmbed específico
    pub fn try_new(model_variant: Option<EmbeddingModel>) -> Result<Self, DomainError> {
        let model_type = model_variant.unwrap_or(EmbeddingModel::BGELargeENV15);
        let dimension = match model_type {
            EmbeddingModel::BGELargeENV15
            | EmbeddingModel::BGELargeENV15Q
            | EmbeddingModel::BGELargeZHV15
            | EmbeddingModel::MultilingualE5Large
            | EmbeddingModel::ModernBertEmbedLarge
            | EmbeddingModel::MxbaiEmbedLargeV1
            | EmbeddingModel::MxbaiEmbedLargeV1Q => 1024,
            EmbeddingModel::BGEBaseENV15
            | EmbeddingModel::BGEBaseENV15Q
            | EmbeddingModel::MultilingualE5Base
            | EmbeddingModel::NomicEmbedTextV1
            | EmbeddingModel::NomicEmbedTextV15
            | EmbeddingModel::NomicEmbedTextV15Q
            | EmbeddingModel::GTEBaseENV15
            | EmbeddingModel::GTEBaseENV15Q
            | EmbeddingModel::ParaphraseMLMpnetBaseV2 => 768,
            _ => 384,
        };

        let options = InitOptions::new(model_type).with_show_download_progress(false);
        let embedding_model = TextEmbedding::try_new(options).map_err(|e| {
            DomainError::EmbeddingError(format!("Error inicializando FastEmbed: {e}"))
        })?;

        Ok(Self {
            model: Arc::new(tokio::sync::Mutex::new(embedding_model)),
            dimension,
        })
    }
}

#[async_trait]
impl EmbeddingEnginePort for FastEmbedEngine {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, DomainError> {
        let batch = vec![text.to_string()];
        let results = self.generate_batch_embeddings(&batch).await?;
        results.into_iter().next().ok_or_else(|| {
            DomainError::EmbeddingError("No se generó ningún vector de salida".to_string())
        })
    }

    async fn generate_batch_embeddings(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, DomainError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let model_arc = Arc::clone(&self.model);
        let owned_texts = texts.to_vec();

        // Ejecutar inferencia en el pool bloqueante de Tokio para no detener el reactor asíncrono
        let vectors = tokio::task::spawn_blocking(move || {
            let model = model_arc.blocking_lock();
            model.embed(owned_texts, None)
        })
        .await
        .map_err(|e| DomainError::EmbeddingError(format!("Error en tarea de inferencia: {e}")))?
        .map_err(|e| DomainError::EmbeddingError(format!("Error generando embeddings: {e}")))?;

        Ok(vectors)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}
