//! Motor de embeddings simulado determinista para pruebas unitarias y de integración offline

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::domain::errors::DomainError;
use crate::infrastructure::storage::schemas::DIMENSION_EMBEDDINGS;

/// Motor determinista de embeddings en memoria.
/// Produce vectores consistentes, normalizados L2, ideales para tests sin depender de pesos ONNX ni red.
pub struct MockEmbeddingEngine {
    dimension: usize,
    predefined_vectors: RwLock<HashMap<String, Vec<f32>>>,
}

impl MockEmbeddingEngine {
    /// Crea una nueva instancia con la dimensión especificada (o DIMENSION_EMBEDDINGS si es 0)
    pub fn new(dimension: Option<usize>) -> Self {
        Self {
            dimension: dimension.unwrap_or(DIMENSION_EMBEDDINGS as usize),
            predefined_vectors: RwLock::new(HashMap::new()),
        }
    }

    /// Registra un vector prefijado para un texto específico
    pub fn registrar_vector(&self, texto: &str, vector: Vec<f32>) -> Result<(), DomainError> {
        if vector.len() != self.dimension {
            return Err(DomainError::EmbeddingError(format!(
                "Dimensión incompatible: esperado {}, recibido {}",
                self.dimension,
                vector.len()
            )));
        }
        let mut map = self
            .predefined_vectors
            .write()
            .map_err(|e| DomainError::EmbeddingError(e.to_string()))?;
        map.insert(texto.to_string(), vector);
        Ok(())
    }

    /// Genera un vector pseudo-semántico determinista basado en hash criptográfico y tokens
    fn generar_vector_determinista(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; self.dimension];

        // Mapear cada palabra en posiciones del vector
        let words: Vec<&str> = text
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|w| !w.is_empty())
            .collect();

        if words.is_empty() {
            // Si el texto no tiene palabras alfanuméricas, usar el hash crudo del texto
            let hash = Sha256::digest(text.as_bytes());
            for (i, byte) in hash.iter().cycle().take(self.dimension).enumerate() {
                vec[i] = (*byte as f32 / 128.0) - 1.0;
            }
        } else {
            for word in &words {
                let lower = word.to_lowercase();
                let hash = Sha256::digest(lower.as_bytes());
                for chunk_idx in 0..(self.dimension / 32).max(1) {
                    for (byte_idx, &b) in hash.iter().enumerate() {
                        let target_idx = (chunk_idx * 32 + byte_idx) % self.dimension;
                        vec[target_idx] += (b as f32 / 128.0) - 1.0;
                    }
                }
            }
        }

        // Normalización L2 (Norma Euclídea = 1.0)
        let sum_sq: f32 = vec.iter().map(|v| v * v).sum();
        let norm = sum_sq.sqrt();
        if norm > 1e-6 {
            for v in &mut vec {
                *v /= norm;
            }
        }

        vec
    }
}

impl Default for MockEmbeddingEngine {
    fn default() -> Self {
        Self::new(None)
    }
}

#[async_trait]
impl EmbeddingEnginePort for MockEmbeddingEngine {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, DomainError> {
        let predefined = {
            let map = self
                .predefined_vectors
                .read()
                .map_err(|e| DomainError::EmbeddingError(e.to_string()))?;
            map.get(text).cloned()
        };

        if let Some(v) = predefined {
            return Ok(v);
        }

        Ok(self.generar_vector_determinista(text))
    }

    async fn generate_batch_embeddings(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, DomainError> {
        let mut results = Vec::with_capacity(texts.len());
        for t in texts {
            results.push(self.generate_embedding(t).await?);
        }
        Ok(results)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_engine_dimension_y_normalizacion() {
        let engine = MockEmbeddingEngine::new(Some(1024));
        assert_eq!(engine.dimension(), 1024);

        let emb = engine
            .generate_embedding("Válvula de seguridad en caldera de vapor")
            .await
            .expect("Debe generar embedding");
        assert_eq!(emb.len(), 1024);

        // Verificar norma L2 ~ 1.0
        let norm: f32 = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "Norma L2 debe ser ~1.0: {}",
            norm
        );
    }

    #[tokio::test]
    async fn test_mock_engine_determinismo() {
        let engine = MockEmbeddingEngine::new(Some(1024));
        let emb1 = engine
            .generate_embedding("mantenimiento preventivo mensual")
            .await
            .unwrap();
        let emb2 = engine
            .generate_embedding("mantenimiento preventivo mensual")
            .await
            .unwrap();
        assert_eq!(
            emb1, emb2,
            "El mismo texto debe producir idéntico embedding"
        );
    }

    #[tokio::test]
    async fn test_mock_engine_vectores_predefinidos() {
        let engine = MockEmbeddingEngine::new(Some(4));
        let custom = vec![0.5, 0.5, 0.5, 0.5];
        engine
            .registrar_vector("emergencia", custom.clone())
            .unwrap();

        let res = engine.generate_embedding("emergencia").await.unwrap();
        assert_eq!(res, custom);
    }
}
