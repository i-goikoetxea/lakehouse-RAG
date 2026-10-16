//! Pruebas Adversariales de Seguridad, Resiliencia y Caos para Almacenamiento LanceDB y Esquemas Arrow

use std::sync::Arc;

use arrow_array::{
    BooleanArray, FixedSizeListArray, Float32Array, Int32Array, RecordBatch, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lakehouse_rag::application::ports::lakehouse_store::{
    LakehouseStorePort, TABLA_DOC_CATALOG, TABLA_DOC_CHUNKS,
};
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::schemas::schema_doc_catalog;

#[tokio::test]
async fn test_adv_rechazo_lote_con_esquema_corrupto_o_incompatible() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();
    store.inicializar_tablas().await.unwrap();

    // Intentar insertar un lote con tipo de dato incompatible (ej. String donde se espera Int32 para nivel_prelacion)
    let esquema_corrupto = Arc::new(Schema::new(vec![
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("titulo", DataType::Utf8, false),
        Field::new("tipo_documental", DataType::Utf8, false),
        Field::new("emisor", DataType::Utf8, false),
        Field::new("fecha_emision", DataType::Utf8, false),
        Field::new("nivel_prelacion", DataType::Utf8, false), // Corrupción intencional: Utf8 en vez de Int32
        Field::new("centro_id", DataType::Utf8, false),
        Field::new("vigente", DataType::Boolean, false),
        Field::new("version", DataType::Int32, false),
        Field::new("hash_sha256", DataType::Utf8, false),
    ]));

    let batch_corrupto = RecordBatch::try_new(
        esquema_corrupto,
        vec![
            Arc::new(StringArray::from(vec!["DOC-001"])),
            Arc::new(StringArray::from(vec!["Título"])),
            Arc::new(StringArray::from(vec!["Norma"])),
            Arc::new(StringArray::from(vec!["Emisor"])),
            Arc::new(StringArray::from(vec!["2026-01-01"])),
            Arc::new(StringArray::from(vec!["NIVEL_TEXTUAL_INVALIDO"])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(BooleanArray::from(vec![true])),
            Arc::new(Int32Array::from(vec![1])),
            Arc::new(StringArray::from(vec!["hash"])),
        ],
    )
    .unwrap();

    let resultado = store.insertar_lote(TABLA_DOC_CATALOG, batch_corrupto).await;
    assert!(
        resultado.is_err(),
        "LanceDB debe rechazar un lote cuyo esquema no coincide exactamente con la tabla"
    );
}

#[tokio::test]
async fn test_adv_rechazo_dimension_incompatible_en_vector_embeddings() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();
    store.inicializar_tablas().await.unwrap();

    // Intentar insertar un vector con dimensión 512 en lugar de 1024
    let dimension_invalida: i32 = 512;
    let valores_vector = Arc::new(Float32Array::from(vec![
        0.5f32;
        dimension_invalida as usize
    ]));
    let item_field = Arc::new(Field::new("item", DataType::Float32, true));
    let array_vector = Arc::new(FixedSizeListArray::new(
        item_field.clone(),
        dimension_invalida,
        valores_vector,
        None,
    ));

    let esquema_invalido = Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("centro_id", DataType::Utf8, false),
        Field::new("seccion", DataType::Utf8, false),
        Field::new("contenido", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(item_field, dimension_invalida),
            true,
        ),
        Field::new("vigente", DataType::Boolean, false),
    ]));

    let batch = RecordBatch::try_new(
        esquema_invalido,
        vec![
            Arc::new(StringArray::from(vec!["CHUNK-001"])),
            Arc::new(StringArray::from(vec!["DOC-001"])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(StringArray::from(vec!["Sec 1"])),
            Arc::new(StringArray::from(vec!["Texto"])),
            array_vector,
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .unwrap();

    let resultado = store.insertar_lote(TABLA_DOC_CHUNKS, batch).await;
    assert!(
        resultado.is_err(),
        "Debe rechazar un vector cuya dimensión no coincide con 1024"
    );
}

#[tokio::test]
async fn test_adv_operaciones_sobre_tabla_inexistente_retornan_error_controlado() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();
    store.inicializar_tablas().await.unwrap();

    let resultado_conteo = store.contar_registros("tabla_fantasma_no_existe").await;
    assert!(
        resultado_conteo.is_err(),
        "Contar sobre tabla inexistente debe devolver error"
    );

    let batch_dummy = RecordBatch::new_empty(schema_doc_catalog());
    let resultado_insercion = store
        .insertar_lote("tabla_fantasma_no_existe", batch_dummy)
        .await;
    assert!(
        resultado_insercion.is_err(),
        "Insertar en tabla inexistente debe devolver error"
    );
}

#[tokio::test]
async fn test_adv_idempotencia_y_resistencia_a_doble_retiro_de_vigencia() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();
    store.inicializar_tablas().await.unwrap();

    // Insertar un documento
    let batch = RecordBatch::try_new(
        schema_doc_catalog(),
        vec![
            Arc::new(StringArray::from(vec!["DOC-IDEMP-01"])),
            Arc::new(StringArray::from(vec!["Doc Idempotente"])),
            Arc::new(StringArray::from(vec!["Norma"])),
            Arc::new(StringArray::from(vec!["Emisor"])),
            Arc::new(StringArray::from(vec!["2026-01-01"])),
            Arc::new(Int32Array::from(vec![1])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(BooleanArray::from(vec![true])),
            Arc::new(Int32Array::from(vec![1])),
            Arc::new(StringArray::from(vec!["h1"])),
        ],
    )
    .unwrap();
    store.insertar_lote(TABLA_DOC_CATALOG, batch).await.unwrap();

    let doc_id = DocId::new("DOC-IDEMP-01").unwrap();

    // 1. Primer retiro: invalida la fila
    let primera_vez = store.retirar_vigencia_documento(&doc_id).await.unwrap();
    assert_eq!(primera_vez, 1);

    // 2. Segundo retiro inmediato: debe ser idempotente y reportar 0 filas afectadas sin error
    let segunda_vez = store.retirar_vigencia_documento(&doc_id).await.unwrap();
    assert_eq!(segunda_vez, 0);

    // 3. Retiro de documento inexistente: debe retornar 0 sin error
    let doc_inexistente = DocId::new("DOC-FANTASMA-99").unwrap();
    let retiro_fantasma = store
        .retirar_vigencia_documento(&doc_inexistente)
        .await
        .unwrap();
    assert_eq!(retiro_fantasma, 0);
}

#[tokio::test]
async fn test_adv_inicializacion_concurrente_multithread_sin_condiciones_de_carrera() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = Arc::new(LanceDbStore::conectar(temp_dir.path()).await.unwrap());

    // Disparar 8 llamadas concurrentes a `inicializar_tablas()`
    let mut handles = Vec::new();
    for _ in 0..8 {
        let store_clon = store.clone();
        handles.push(tokio::spawn(async move {
            store_clon.inicializar_tablas().await
        }));
    }

    for handle in handles {
        let res = handle.await.expect("Join handle");
        assert!(
            res.is_ok(),
            "La inicialización concurrente debe ser idempotente y tolerante a carreras"
        );
    }

    let tablas = store.listar_tablas().await.unwrap();
    assert_eq!(tablas.len(), 6, "Deben existir exactamente 6 tablas");
}
