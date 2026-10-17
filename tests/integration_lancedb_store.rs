//! Pruebas de integración sobre LanceDB (.lance) y esquemas Apache Arrow v2

mod comun_store_v2;

use arrow_array::RecordBatch;
use comun_store_v2::*;
use lakehouse_rag::application::dtos::lakehouse_filas::LoteIngesta;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, LakehouseStorePort, TABLA_DOCUMENTOS_CATALOGO, TODAS_LAS_TABLAS,
};
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;

fn total_filas(lotes: &[RecordBatch]) -> usize {
    lotes.iter().map(RecordBatch::num_rows).sum()
}

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
            "Falta la tabla {tabla_esperada}"
        );
        assert_eq!(
            store.contar_registros(tabla_esperada).await.unwrap(),
            0,
            "La tabla {tabla_esperada} debe nacer vacía"
        );
    }

    // 2. Segunda llamada debe ser idempotente y no fallar
    store
        .inicializar_tablas()
        .await
        .expect("Segunda inicialización idempotente debe ser exitosa");
    assert_eq!(store.listar_tablas().await.unwrap().len(), 6);
}

#[tokio::test]
async fn test_insercion_y_conteo_en_tabla_doc_catalog() {
    let (_dir, store) = store_temporal().await;

    for (ingest, doc_id, hash) in [
        ("ing-cat-001", "DOC-001", HASH_A),
        ("ing-cat-002", "DOC-002", HASH_B),
    ] {
        let ingest_id = IngestId::new(ingest).unwrap();
        let escritas = store
            .escribir_staged(
                &ingest_id,
                vec![LoteIngesta::Catalogo(vec![fila_catalogo(
                    doc_id,
                    1,
                    fecha(2026, 1, 1),
                    hash,
                )])],
            )
            .await
            .expect("Escribir catálogo staged");
        assert_eq!(escritas, 1);
        store
            .confirmar_ingesta(&ingest_id, &DocId::new(doc_id).unwrap())
            .await
            .expect("Confirmar ingesta");
    }

    let total = store
        .contar_registros(TABLA_DOCUMENTOS_CATALOGO)
        .await
        .expect("Contar registros");
    assert_eq!(total, 2);

    let vigentes = store
        .leer_tabla(
            TABLA_DOCUMENTOS_CATALOGO,
            &ConsultaTabla::confirmadas(100).solo_vigentes(),
        )
        .await
        .expect("Leer vigentes");
    assert_eq!(total_filas(&vigentes), 2);
}

#[tokio::test]
async fn test_mecanismo_control_versiones_retirar_vigencia() {
    let (_dir, store) = store_temporal().await;

    // Documento ajeno que no debe verse afectado
    let ingest_ajeno = IngestId::new("ing-ajeno-001").unwrap();
    store
        .escribir_staged(
            &ingest_ajeno,
            revision("DOC-REV-B", 1, fecha(2026, 1, 1), HASH_C),
        )
        .await
        .expect("Staged documento ajeno");
    store
        .confirmar_ingesta(&ingest_ajeno, &DocId::new("DOC-REV-B").unwrap())
        .await
        .expect("Confirmar documento ajeno");

    // Dos revisiones del mismo documento
    let primera = confirmar_revision(&store, "ing-rev-003", 3, fecha(2026, 3, 1), HASH_A).await;
    assert!(primera.versiones_derogadas.is_empty());
    assert_eq!(primera.filas_retiradas, 0);

    let segunda = confirmar_revision(&store, "ing-rev-004", 4, fecha(2026, 9, 15), HASH_B).await;
    assert_eq!(segunda.versiones_derogadas, vec![3]);
    assert!(
        segunda.filas_retiradas >= 1,
        "La revisión 3 debe retirarse en al menos una fila"
    );

    // Solo la revisión 4 queda vigente para el documento
    let vigentes_doc = store
        .leer_tabla(
            TABLA_DOCUMENTOS_CATALOGO,
            &ConsultaTabla::confirmadas(100)
                .solo_vigentes()
                .del_documento(&doc()),
        )
        .await
        .expect("Leer vigentes del documento");
    assert_eq!(total_filas(&vigentes_doc), 1, "Solo una revisión vigente");

    // El histórico conserva ambas revisiones
    let historico = store
        .leer_tabla(
            TABLA_DOCUMENTOS_CATALOGO,
            &ConsultaTabla::confirmadas(100).del_documento(&doc()),
        )
        .await
        .expect("Leer histórico del documento");
    assert_eq!(total_filas(&historico), 2);

    let filtro_v3 = format!("doc_id = '{DOC}' AND version_num = 3 AND vigente = true");
    let v3_vigentes = store
        .leer_filas_crudas(TABLA_DOCUMENTOS_CATALOGO, Some(&filtro_v3))
        .await
        .expect("Leer v3");
    assert_eq!(
        total_filas(&v3_vigentes),
        0,
        "La revisión 3 ya no es vigente"
    );

    let filtro_v4 = format!("doc_id = '{DOC}' AND version_num = 4 AND vigente = true");
    let v4_vigentes = store
        .leer_filas_crudas(TABLA_DOCUMENTOS_CATALOGO, Some(&filtro_v4))
        .await
        .expect("Leer v4");
    assert_eq!(total_filas(&v4_vigentes), 1, "La revisión 4 es la vigente");

    // El otro documento sigue vigente
    let ajeno = store
        .leer_tabla(
            TABLA_DOCUMENTOS_CATALOGO,
            &ConsultaTabla::confirmadas(100)
                .solo_vigentes()
                .del_documento(&DocId::new("DOC-REV-B").unwrap()),
        )
        .await
        .expect("Leer vigentes del documento ajeno");
    assert_eq!(total_filas(&ajeno), 1, "DOC-REV-B debe permanecer vigente");

    assert!(
        store
            .verificar_integridad()
            .await
            .expect("Verificar integridad")
            .es_integro(),
        "El almacén debe quedar íntegro"
    );
}

#[tokio::test]
async fn test_insercion_en_las_6_tablas_del_lakehouse() {
    let (_dir, store) = store_temporal().await;

    let ingest_id = IngestId::new("ing-seis-tablas").unwrap();
    let escritas = store
        .escribir_staged(&ingest_id, revision(DOC, 3, fecha(2026, 3, 1), HASH_A))
        .await
        .expect("Escribir revisión completa");
    assert_eq!(escritas, FILAS_POR_REVISION.iter().sum::<usize>());

    // Staged: las 6 tablas tienen filas físicas pero las lecturas normales no las ven
    for (tabla, esperadas) in TODAS_LAS_TABLAS.iter().zip(FILAS_POR_REVISION) {
        assert_eq!(
            store.contar_registros(tabla).await.unwrap(),
            esperadas,
            "La tabla {tabla} debe tener {esperadas} filas"
        );
        let visibles = store
            .leer_tabla(tabla, &ConsultaTabla::confirmadas(100))
            .await
            .unwrap();
        assert_eq!(total_filas(&visibles), 0, "{tabla}: STAGED no es visible");
    }

    store
        .confirmar_ingesta(&ingest_id, &doc())
        .await
        .expect("Confirmar");

    for (tabla, esperadas) in TODAS_LAS_TABLAS.iter().zip(FILAS_POR_REVISION) {
        let visibles = store
            .leer_tabla(tabla, &ConsultaTabla::confirmadas(100).solo_vigentes())
            .await
            .unwrap();
        assert_eq!(
            total_filas(&visibles),
            esperadas,
            "La tabla {tabla} debe exponer {esperadas} filas vigentes"
        );
    }
}
