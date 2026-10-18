//! Pruebas Adversariales de Robustez y Casos Límite para el motor de Embeddings

use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;

#[tokio::test]
async fn test_adv_rechazo_dimension_incompatible_en_engine() {
    let engine = MockEmbeddingEngine::new(Some(1024));
    // Intentar registrar un vector de 512 elementos cuando se espera 1024
    let vector_invalido = vec![0.1f32; 512];
    let res = engine.registrar_vector("texto_invalido", vector_invalido);
    assert!(
        matches!(res, Err(DomainError::EmbeddingError(_))),
        "Debe rechazar vector con dimensión incongruente"
    );
}
