//! Pruebas Adversariales de Seguridad, Robustez y Casos Límite para Embeddings y Búsqueda Híbrida

use std::sync::Arc;

use arrow_array::{BooleanArray, FixedSizeListArray, Float32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field};
use tempfile::tempdir;

use lakehouse_rag::application::ports::hybrid_search::{HybridSearchParams, HybridSearchPort};
use lakehouse_rag::application::ports::lakehouse_store::{LakehouseStorePort, TABLA_DOC_CHUNKS};
use lakehouse_rag::application::use_cases::hybrid_search_service::HybridSearchService;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::schemas::{DIMENSION_EMBEDDINGS, schema_doc_chunks};

fn crear_lote_minimo(centro: &str) -> RecordBatch {
    let item_field = Arc::new(Field::new("item", DataType::Float32, true));
    let valores_vector = Arc::new(Float32Array::from(vec![
        0.05f32;
        DIMENSION_EMBEDDINGS as usize
    ]));
    let array_vector = Arc::new(FixedSizeListArray::new(
        item_field,
        DIMENSION_EMBEDDINGS,
        valores_vector,
        None,
    ));

    RecordBatch::try_new(
        schema_doc_chunks(),
        vec![
            Arc::new(StringArray::from(vec!["CHK-ADV-01"])),
            Arc::new(StringArray::from(vec!["DOC-ADV-01"])),
            Arc::new(StringArray::from(vec![centro])),
            Arc::new(StringArray::from(vec!["Sección Segura"])),
            Arc::new(StringArray::from(vec!["Contenido técnico verificado"])),
            array_vector,
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .expect("RecordBatch mínimo")
}

#[tokio::test]
async fn test_adv_rechazo_centro_id_vacio_o_blancos() {
    let tmp = tempdir().expect("Crear tempdir");
    let store = Arc::new(
        LanceDbStore::conectar(tmp.path())
            .await
            .expect("Conectar store"),
    );
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar tablas");
    let engine = Arc::new(MockEmbeddingEngine::default());
    let service = HybridSearchService::new(store, engine);

    // Centro ID vacío
    let params_vacio = HybridSearchParams {
        query: "prueba".to_string(),
        centro_id: "".to_string(),
        ..Default::default()
    };
    let res = service.hybrid_search(&params_vacio).await;
    assert!(
        matches!(res, Err(DomainError::SearchError(_))),
        "Debe rechazar centro_id vacío"
    );

    // Centro ID solo espacios
    let params_espacios = HybridSearchParams {
        query: "prueba".to_string(),
        centro_id: "   \t  ".to_string(),
        ..Default::default()
    };
    let res2 = service.hybrid_search(&params_espacios).await;
    assert!(
        matches!(res2, Err(DomainError::SearchError(_))),
        "Debe rechazar centro_id con solo espacios"
    );
}

#[tokio::test]
async fn test_adv_query_vacia_retorna_vector_vacio_sin_error() {
    let tmp = tempdir().expect("Crear tempdir");
    let store = Arc::new(
        LanceDbStore::conectar(tmp.path())
            .await
            .expect("Conectar store"),
    );
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar tablas");
    let engine = Arc::new(MockEmbeddingEngine::default());
    let service = HybridSearchService::new(store, engine);

    let params_blanco = HybridSearchParams {
        query: "   \n\t  ".to_string(),
        centro_id: "centro_valido".to_string(),
        ..Default::default()
    };

    let hits = service
        .hybrid_search(&params_blanco)
        .await
        .expect("Debe manejar query vacía con éxito");
    assert!(
        hits.is_empty(),
        "Query en blanco debe retornar lista vacía sin pánico"
    );
}

#[tokio::test]
async fn test_adv_resistencia_a_inyecciones_en_query_y_centro() {
    let tmp = tempdir().expect("Crear tempdir");
    let store = Arc::new(
        LanceDbStore::conectar(tmp.path())
            .await
            .expect("Conectar store"),
    );
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar tablas");

    let batch = crear_lote_minimo("planta_segura");
    store
        .insertar_lote(TABLA_DOC_CHUNKS, batch)
        .await
        .expect("Insertar lote");

    let engine = Arc::new(MockEmbeddingEngine::default());
    let service = HybridSearchService::new(store, engine);

    let ataques = vec![
        "'; DROP TABLE \"02_doc_chunks\"; --",
        "' OR '1'='1",
        "\" OR 1=1 --",
        "admin' --",
        "UNION SELECT * FROM \"01_doc_catalog\"",
        "\0\0\0CARACTERES_NULOS",
        "../../../etc/passwd",
        "{{7*7}} <script>alert(1)</script>",
    ];

    for ataque in ataques {
        let params = HybridSearchParams {
            query: ataque.to_string(),
            centro_id: "planta_segura".to_string(),
            top_k: 5,
            ..Default::default()
        };

        // No debe causar pánico ni destruir datos
        let res = service.hybrid_search(&params).await;
        assert!(
            res.is_ok(),
            "La consulta con payload '{}' debe ser contenida de forma segura",
            ataque
        );
    }
}

#[tokio::test]
async fn test_adv_limites_extremos_top_k() {
    let tmp = tempdir().expect("Crear tempdir");
    let store = Arc::new(
        LanceDbStore::conectar(tmp.path())
            .await
            .expect("Conectar store"),
    );
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar tablas");

    let batch = crear_lote_minimo("planta_limites");
    store
        .insertar_lote(TABLA_DOC_CHUNKS, batch)
        .await
        .expect("Insertar lote");

    let engine = Arc::new(MockEmbeddingEngine::default());
    let service = HybridSearchService::new(store, engine);

    // top_k = 0
    let params_zero = HybridSearchParams {
        query: "contenido tecnico".to_string(),
        centro_id: "planta_limites".to_string(),
        top_k: 0,
        ..Default::default()
    };
    let hits_zero = service.hybrid_search(&params_zero).await.unwrap();
    assert_eq!(hits_zero.len(), 0);

    // top_k desmesurado (10,000 sobre 1 solo registro)
    let params_huge = HybridSearchParams {
        query: "contenido tecnico".to_string(),
        centro_id: "planta_limites".to_string(),
        top_k: 10_000,
        ..Default::default()
    };
    let hits_huge = service.hybrid_search(&params_huge).await.unwrap();
    assert_eq!(
        hits_huge.len(),
        1,
        "No debe exceder los registros existentes"
    );
}

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
