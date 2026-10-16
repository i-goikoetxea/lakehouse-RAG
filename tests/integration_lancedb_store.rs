//! Pruebas de integración sobre LanceDB (.lance) y esquemas Apache Arrow

use std::sync::Arc;

use arrow_array::{
    BooleanArray, FixedSizeListArray, Float32Array, Float64Array, Int32Array, RecordBatch,
    StringArray,
};
use arrow_schema::{DataType, Field};
use lakehouse_rag::application::ports::lakehouse_store::{
    LakehouseStorePort, TABLA_DOC_CATALOG, TABLA_DOC_CHUNKS, TABLA_DOUBLE_ENTRY_CELLS,
    TABLA_DOUBLE_ENTRY_TABLES, TABLA_OPERATIONAL_MATRIX, TABLA_REFERENCE_GRAPH, TODAS_LAS_TABLAS,
};
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::schemas::{
    DIMENSION_EMBEDDINGS, schema_doc_catalog, schema_doc_chunks, schema_double_entry_cells,
    schema_double_entry_tables, schema_operational_matrix, schema_reference_graph,
};
use lancedb::query::{ExecutableQuery, QueryBase};

#[tokio::test]
async fn test_inicializar_tablas_crea_las_6_tablas_en_lancedb() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path())
        .await
        .expect("Conectar store");

    // 1. Inicialización idempotente
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar tablas");

    let tablas = store.listar_tablas().await.expect("Listar tablas");
    assert_eq!(tablas.len(), 6);

    for tabla_esperada in TODAS_LAS_TABLAS {
        assert!(
            tablas.contains(&tabla_esperada.to_string()),
            "Falta la tabla {}",
            tabla_esperada
        );
    }

    // 2. Segunda llamada debe ser idempotente y no fallar
    store
        .inicializar_tablas()
        .await
        .expect("Segunda inicialización idempotente debe ser exitosa");
}

#[tokio::test]
async fn test_insercion_y_conteo_en_tabla_doc_catalog() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path())
        .await
        .expect("Conectar store");
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar tablas");

    let schema = schema_doc_catalog();

    let doc_ids = Arc::new(StringArray::from(vec!["DOC-001", "DOC-002"]));
    let titulos = Arc::new(StringArray::from(vec!["Manual A", "Manual B"]));
    let tipos = Arc::new(StringArray::from(vec!["Normativa", "Instructivo"]));
    let emisores = Arc::new(StringArray::from(vec!["Gerencia Calidad", "Operaciones"]));
    let fechas = Arc::new(StringArray::from(vec!["2026-01-01", "2026-02-01"]));
    let niveles = Arc::new(Int32Array::from(vec![1, 2]));
    let centros = Arc::new(StringArray::from(vec!["planta-norte", "planta-norte"]));
    let vigentes = Arc::new(BooleanArray::from(vec![true, true]));
    let versiones = Arc::new(Int32Array::from(vec![1, 1]));
    let hashes = Arc::new(StringArray::from(vec!["hash1", "hash2"]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            doc_ids, titulos, tipos, emisores, fechas, niveles, centros, vigentes, versiones,
            hashes,
        ],
    )
    .expect("Construir RecordBatch");

    store
        .insertar_lote(TABLA_DOC_CATALOG, batch)
        .await
        .expect("Insertar lote");

    let total = store
        .contar_registros(TABLA_DOC_CATALOG)
        .await
        .expect("Contar registros");
    assert_eq!(total, 2);
}

#[tokio::test]
async fn test_mecanismo_control_versiones_retirar_vigencia() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path())
        .await
        .expect("Conectar store");
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar tablas");

    // Insertar registros en catálogo con 2 documentos distintos
    let schema = schema_doc_catalog();
    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(vec!["DOC-REV-A", "DOC-REV-B"])),
            Arc::new(StringArray::from(vec![
                "Procedimiento A",
                "Procedimiento B",
            ])),
            Arc::new(StringArray::from(vec!["Norma", "Norma"])),
            Arc::new(StringArray::from(vec!["ISO", "ISO"])),
            Arc::new(StringArray::from(vec!["2026-01-01", "2026-01-01"])),
            Arc::new(Int32Array::from(vec![1, 1])),
            Arc::new(StringArray::from(vec!["centro-central", "centro-central"])),
            Arc::new(BooleanArray::from(vec![true, true])),
            Arc::new(Int32Array::from(vec![1, 1])),
            Arc::new(StringArray::from(vec!["h1", "h2"])),
        ],
    )
    .expect("Batch catálogo");

    store
        .insertar_lote(TABLA_DOC_CATALOG, batch)
        .await
        .expect("Insertar catálogo");

    // Retirar vigencia únicamente de DOC-REV-A
    let doc_a_id = DocId::new("DOC-REV-A").unwrap();
    let invalidados = store
        .retirar_vigencia_documento(&doc_a_id)
        .await
        .expect("Retirar vigencia");

    assert!(invalidados >= 1);

    // Verificar en la tabla que DOC-REV-A ya no tiene vigente = true
    let tabla = store
        .abrir_tabla(TABLA_DOC_CATALOG)
        .await
        .expect("Abrir tabla");
    let scanner = tabla
        .query()
        .only_if("doc_id = 'DOC-REV-A' AND vigente = true")
        .execute()
        .await
        .expect("Query activos");

    // Al escanear el stream no debe haber filas vigentes para DOC-REV-A
    use futures::StreamExt;
    let mut stream = scanner;
    let mut vigentes_encontrados = 0;
    while let Some(batch_res) = stream.next().await {
        let b = batch_res.expect("Batch de lectura");
        vigentes_encontrados += b.num_rows();
    }
    assert_eq!(vigentes_encontrados, 0, "DOC-REV-A no debe estar vigente");

    // Verificar que DOC-REV-B sigue vigente
    let scanner_b = tabla
        .query()
        .only_if("doc_id = 'DOC-REV-B' AND vigente = true")
        .execute()
        .await
        .expect("Query activos DOC-REV-B");
    let mut stream_b = scanner_b;
    let mut vigentes_b = 0;
    while let Some(batch_res) = stream_b.next().await {
        let b = batch_res.expect("Batch de lectura");
        vigentes_b += b.num_rows();
    }
    assert_eq!(vigentes_b, 1, "DOC-REV-B debe permanecer vigente");
}

#[tokio::test]
async fn test_insercion_en_las_6_tablas_del_lakehouse() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path())
        .await
        .expect("Conectar store");
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar tablas");

    // 1. Catálogo
    let batch_cat = RecordBatch::try_new(
        schema_doc_catalog(),
        vec![
            Arc::new(StringArray::from(vec!["DOC-001"])),
            Arc::new(StringArray::from(vec!["Título 1"])),
            Arc::new(StringArray::from(vec!["Norma"])),
            Arc::new(StringArray::from(vec!["Emisor 1"])),
            Arc::new(StringArray::from(vec!["2026-01-01"])),
            Arc::new(Int32Array::from(vec![1])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(BooleanArray::from(vec![true])),
            Arc::new(Int32Array::from(vec![1])),
            Arc::new(StringArray::from(vec!["h1"])),
        ],
    )
    .unwrap();
    store
        .insertar_lote(TABLA_DOC_CATALOG, batch_cat)
        .await
        .unwrap();

    // 2. Chunks
    let valores_vector = Arc::new(Float32Array::from(vec![
        0.1f32;
        DIMENSION_EMBEDDINGS as usize
    ]));
    let item_field = Arc::new(Field::new("item", DataType::Float32, true));
    let array_vector = Arc::new(FixedSizeListArray::new(
        item_field,
        DIMENSION_EMBEDDINGS,
        valores_vector,
        None,
    ));

    let batch_chunks = RecordBatch::try_new(
        schema_doc_chunks(),
        vec![
            Arc::new(StringArray::from(vec!["CHUNK-001"])),
            Arc::new(StringArray::from(vec!["DOC-001"])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(StringArray::from(vec!["Sec 1.1"])),
            Arc::new(StringArray::from(vec!["Contenido textual técnico"])),
            array_vector,
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .unwrap();
    store
        .insertar_lote(TABLA_DOC_CHUNKS, batch_chunks)
        .await
        .unwrap();

    // 3. Matriz Operativa
    let batch_matriz = RecordBatch::try_new(
        schema_operational_matrix(),
        vec![
            Arc::new(StringArray::from(vec!["TUPLA-001"])),
            Arc::new(StringArray::from(vec!["DOC-001"])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(StringArray::from(vec!["Recepcion"])),
            Arc::new(StringArray::from(vec!["Inspector Calidad"])),
            Arc::new(StringArray::from(vec!["Medir"])),
            Arc::new(StringArray::from(vec!["Presión"])),
            Arc::new(StringArray::from(vec!["+/- 0.5 bar"])),
            Arc::new(StringArray::from(vec!["En frío"])),
            Arc::new(StringArray::from(vec!["Sec. 3.2"])),
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .unwrap();
    store
        .insertar_lote(TABLA_OPERATIONAL_MATRIX, batch_matriz)
        .await
        .unwrap();

    // 4. Catálogo de Tablas
    let batch_tablas = RecordBatch::try_new(
        schema_double_entry_tables(),
        vec![
            Arc::new(StringArray::from(vec!["TABLA-001"])),
            Arc::new(StringArray::from(vec!["DOC-001"])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(StringArray::from(vec!["Tabla de Tolerancias"])),
            Arc::new(StringArray::from(vec!["Espesor"])),
            Arc::new(StringArray::from(vec!["Temperatura"])),
            Arc::new(StringArray::from(vec![Some("Valores en mm")])),
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .unwrap();
    store
        .insertar_lote(TABLA_DOUBLE_ENTRY_TABLES, batch_tablas)
        .await
        .unwrap();

    // 5. Celdas de Doble Entrada
    let batch_celdas = RecordBatch::try_new(
        schema_double_entry_cells(),
        vec![
            Arc::new(StringArray::from(vec!["CELDA-001"])),
            Arc::new(StringArray::from(vec!["TABLA-001"])),
            Arc::new(StringArray::from(vec!["DOC-001"])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(Float64Array::from(vec![Some(10.0)])),
            Arc::new(Float64Array::from(vec![Some(20.0)])),
            Arc::new(BooleanArray::from(vec![true])),
            Arc::new(BooleanArray::from(vec![false])),
            Arc::new(StringArray::from(vec![None::<&str>])),
            Arc::new(Float64Array::from(vec![Some(100.0)])),
            Arc::new(Float64Array::from(vec![Some(150.0)])),
            Arc::new(BooleanArray::from(vec![true])),
            Arc::new(BooleanArray::from(vec![true])),
            Arc::new(StringArray::from(vec![None::<&str>])),
            Arc::new(StringArray::from(vec!["0.25"])),
            Arc::new(StringArray::from(vec!["mm"])),
            Arc::new(StringArray::from(vec![Some("Nota de celda")])),
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .unwrap();
    store
        .insertar_lote(TABLA_DOUBLE_ENTRY_CELLS, batch_celdas)
        .await
        .unwrap();

    // 6. Grafo de Referencias
    let batch_grafo = RecordBatch::try_new(
        schema_reference_graph(),
        vec![
            Arc::new(StringArray::from(vec!["ARISTA-001"])),
            Arc::new(StringArray::from(vec!["c1"])),
            Arc::new(StringArray::from(vec!["DOC-001"])),
            Arc::new(StringArray::from(vec!["DOC-002"])),
            Arc::new(StringArray::from(vec!["Complementa"])),
            Arc::new(Float32Array::from(vec![0.8f32])),
            Arc::new(StringArray::from(vec![Some("Conforme al anexo 2")])),
            Arc::new(BooleanArray::from(vec![true])),
        ],
    )
    .unwrap();
    store
        .insertar_lote(TABLA_REFERENCE_GRAPH, batch_grafo)
        .await
        .unwrap();

    // Verificación de conteos
    for tabla in TODAS_LAS_TABLAS {
        let count = store.contar_registros(tabla).await.unwrap();
        assert_eq!(count, 1, "La tabla {} debe tener exactamente 1 fila", tabla);
    }
}
