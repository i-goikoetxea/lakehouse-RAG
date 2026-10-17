//! Motor FastEmbed de carga diferida: el modelo (≈ 2,2 GB en E5-large) solo se carga la
//! primera vez que se vectoriza algo. Así `--ask`, `--centros` o el MCP arrancan al instante.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use fastembed::EmbeddingModel;
use tokio::sync::OnceCell;

use crate::application::dtos::centros::ModeloEmbeddingsActivo;
use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::domain::errors::DomainError;
use crate::infrastructure::embeddings::fastembed_engine::{
    FastEmbedEngine, MODELO_POR_DEFECTO, dimension_del_modelo,
};

/// Nombre estable del modelo para `center_manifest.json`
pub fn nombre_del_modelo(modelo: &EmbeddingModel) -> String {
    format!("{modelo:?}")
}

/// Modelo activo por defecto de la aplicación
pub fn modelo_embeddings_por_defecto() -> ModeloEmbeddingsActivo {
    ModeloEmbeddingsActivo::new(
        nombre_del_modelo(&MODELO_POR_DEFECTO),
        dimension_del_modelo(&MODELO_POR_DEFECTO),
    )
}

pub struct LazyFastEmbedEngine {
    modelo: EmbeddingModel,
    directorio_cache: Option<PathBuf>,
    dimension: usize,
    motor: OnceCell<Arc<FastEmbedEngine>>,
}

impl LazyFastEmbedEngine {
    pub fn new(modelo: EmbeddingModel, directorio_cache: Option<PathBuf>) -> Self {
        let dimension = dimension_del_modelo(&modelo);
        Self {
            modelo,
            directorio_cache,
            dimension,
            motor: OnceCell::new(),
        }
    }

    pub fn modelo_activo(&self) -> ModeloEmbeddingsActivo {
        ModeloEmbeddingsActivo::new(nombre_del_modelo(&self.modelo), self.dimension)
    }

    async fn motor(&self) -> Result<&Arc<FastEmbedEngine>, DomainError> {
        self.motor
            .get_or_try_init(|| async {
                let modelo = self.modelo.clone();
                let cache = self.directorio_cache.clone();
                tokio::task::spawn_blocking(move || {
                    FastEmbedEngine::con_cache(Some(modelo), cache.as_deref()).map(Arc::new)
                })
                .await
                .map_err(|e| {
                    DomainError::EmbeddingError(format!("Error cargando FastEmbed: {e}"))
                })?
            })
            .await
    }
}

#[async_trait]
impl EmbeddingEnginePort for LazyFastEmbedEngine {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, DomainError> {
        self.motor().await?.generate_embedding(text).await
    }

    async fn generate_batch_embeddings(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, DomainError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.motor().await?.generate_batch_embeddings(texts).await
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modelo_por_defecto_es_e5_large_de_1024() {
        let modelo = modelo_embeddings_por_defecto();
        assert_eq!(modelo.nombre, "MultilingualE5Large");
        assert_eq!(modelo.dimension, 1024);
    }

    #[tokio::test]
    async fn test_motor_diferido_no_carga_el_modelo_hasta_usarlo() {
        let motor = LazyFastEmbedEngine::new(MODELO_POR_DEFECTO, None);
        assert_eq!(motor.dimension(), 1024);
        assert!(
            motor
                .generate_batch_embeddings(&[])
                .await
                .unwrap()
                .is_empty()
        );
        assert!(motor.motor.get().is_none());
    }
}
