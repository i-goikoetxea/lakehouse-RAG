//! Pruebas Adversariales de Seguridad, Resiliencia y Caos para Almacenamiento LanceDB y Esquemas Arrow v2

mod comun_store_v2;

use std::sync::Arc;

use arrow_array::{Int32Array, RecordBatch, RecordBatchIterator};
use arrow_schema::{DataType, Field, Schema};
use comun_store_v2::*;
use lakehouse_rag::application::dtos::lakehouse_filas::LoteIngesta;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, LakehouseStorePort, TABLA_CHUNKS_NORMATIVA, TABLA_DOCUMENTOS_CATALOGO,
    TODAS_LAS_TABLAS,
};
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;

#[tokio::test]
async fn test_adv_rechazo_lote_con_esquema_corrupto_o_incompatible() {
    let (_dir, store) = store_temporal().await;

    // 1. Lotes vacíos: rechazados
    let ingest_vacio = IngestId::new("ing-vacio").unwrap();
    assert!(
        store
            .escribir_staged(&ingest_vacio, Vec::new())
            .await
            .is_err(),
        "Un Vec de lotes vacío debe rechazarse"
    );
    assert!(
        store
            .escribir_staged(&ingest_vacio, vec![LoteIngesta::Catalogo(Vec::new())])
            .await
            .is_err(),
        "Un lote sin filas debe rechazarse"
    );

    // 2. Claves documentales mezcladas (otro documento / otra versión): rechazado
    let ingest_mezcla = IngestId::new("ing-mezcla").unwrap();
    let mezcla_documentos = vec![
        LoteIngesta::Catalogo(vec![fila_catalogo("DOC-A", 1, fecha(2026, 1, 1), HASH_A)]),
        LoteIngesta::Chunks(vec![fila_chunk("DOC-B", 1, 0)]),
    ];
    assert!(
        store
            .escribir_staged(&ingest_mezcla, mezcla_documentos)
            .await
            .is_err(),
        "Claves de documentos distintos deben rechazarse"
    );
    let mezcla_versiones = vec![
        LoteIngesta::Catalogo(vec![fila_catalogo("DOC-A", 1, fecha(2026, 1, 1), HASH_A)]),
        LoteIngesta::Chunks(vec![fila_chunk("DOC-A", 2, 0)]),
    ];
    assert!(
        store
            .escribir_staged(&ingest_mezcla, mezcla_versiones)
            .await
            .is_err(),
        "Versiones distintas en la misma ingesta deben rechazarse"
    );
    for tabla in TODAS_LAS_TABLAS {
        assert_eq!(
            store.contar_registros(tabla).await.unwrap(),
            0,
            "Los rechazos no deben dejar filas en {tabla}"
        );
    }

    // 3. Añadir directamente un RecordBatch de esquema incompatible: LanceDB lo rechaza
    let esquema_corrupto = Arc::new(Schema::new(vec![Field::new(
        "doc_id",
        DataType::Int32, // Corrupción intencional: Int32 en vez de Utf8
        false,
    )]));
    let batch_corrupto = RecordBatch::try_new(
        esquema_corrupto.clone(),
        vec![Arc::new(Int32Array::from(vec![1]))],
    )
    .unwrap();
    let tabla = store.abrir_tabla(TABLA_DOCUMENTOS_CATALOGO).await.unwrap();
    let lector = RecordBatchIterator::new(vec![Ok(batch_corrupto)], esquema_corrupto);
    let resultado = tabla.add(Box::new(lector)).execute().await;
    assert!(
        resultado.is_err(),
        "LanceDB debe rechazar un lote cuyo esquema no coincide con la tabla"
    );
    assert_eq!(
        store
            .contar_registros(TABLA_DOCUMENTOS_CATALOGO)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn test_adv_rechazo_dimension_incompatible_en_vector_embeddings() {
    let (_dir, store) = store_temporal().await;

    // FilaChunk con un vector de 512 dimensiones en lugar de 1024
    let mut chunk = fila_chunk("DOC-001", 1, 0);
    chunk.vector = vec![0.5f32; 512];

    let ingest_id = IngestId::new("ing-dim-invalida").unwrap();
    let resultado = store
        .escribir_staged(
            &ingest_id,
            vec![
                LoteIngesta::Catalogo(vec![fila_catalogo("DOC-001", 1, fecha(2026, 1, 1), HASH_A)]),
                LoteIngesta::Chunks(vec![chunk]),
            ],
        )
        .await;
    assert!(
        resultado.is_err(),
        "Debe rechazar un vector cuya dimensión no coincide con 1024"
    );

    // Lo escrito antes del fallo se revierte sin dejar rastro
    store
        .revertir_ingesta(&ingest_id)
        .await
        .expect("Revertir tras fallo");
    assert_eq!(
        store
            .contar_registros(TABLA_CHUNKS_NORMATIVA)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        store
            .contar_registros(TABLA_DOCUMENTOS_CATALOGO)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn test_adv_operaciones_sobre_tabla_inexistente_retornan_error_controlado() {
    let (_dir, store) = store_temporal().await;

    assert!(
        store
            .contar_registros("tabla_fantasma_no_existe")
            .await
            .is_err(),
        "Contar sobre tabla inexistente debe devolver error"
    );
    assert!(
        store
            .leer_tabla("tabla_fantasma_no_existe", &ConsultaTabla::confirmadas(10))
            .await
            .is_err(),
        "Leer una tabla inexistente debe devolver error"
    );
    assert!(
        store.abrir_tabla("tabla_fantasma_no_existe").await.is_err(),
        "Abrir una tabla inexistente debe devolver error"
    );
}

#[tokio::test]
async fn test_adv_idempotencia_y_resistencia_a_doble_retiro_de_vigencia() {
    let (_dir, store) = store_temporal().await;

    let ingest_id = IngestId::new("ing-idemp-01").unwrap();
    let escritas = store
        .escribir_staged(&ingest_id, revision(DOC, 3, fecha(2026, 3, 1), HASH_A))
        .await
        .unwrap();
    assert_eq!(escritas, FILAS_POR_REVISION.iter().sum::<usize>());

    // 1. Primera reversión: borra todas las filas staged
    let primera_vez = store.revertir_ingesta(&ingest_id).await.unwrap();
    assert_eq!(primera_vez as usize, escritas);

    // 2. Segunda reversión inmediata: idempotente, 0 filas y sin error
    let segunda_vez = store.revertir_ingesta(&ingest_id).await.unwrap();
    assert_eq!(segunda_vez, 0);

    // 3. Revertir una ingesta inexistente: Ok(0)
    let fantasma = IngestId::new("ing-fantasma-99").unwrap();
    assert_eq!(store.revertir_ingesta(&fantasma).await.unwrap(), 0);

    for tabla in TODAS_LAS_TABLAS {
        assert_eq!(store.contar_registros(tabla).await.unwrap(), 0);
    }

    // 4. Una ingesta ya confirmada no puede revertirse
    confirmar_revision(&store, "ing-idemp-02", 4, fecha(2026, 9, 15), HASH_B).await;
    let confirmada = IngestId::new("ing-idemp-02").unwrap();
    assert!(
        store.revertir_ingesta(&confirmada).await.is_err(),
        "Revertir una ingesta confirmada debe devolver error"
    );
    assert!(store.verificar_integridad().await.unwrap().es_integro());
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
