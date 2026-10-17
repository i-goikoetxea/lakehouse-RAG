//! Suite adversarial del almacén v2 transaccional (Fase 10): ingest_id malicioso, doc_id con
//! comilla, confirmación doble, rollback de un ingest_id inexistente, lotes vacíos y otros
//! intentos de romper el ciclo staged → commit → rollback.

mod comun_store_v2;

use std::sync::Arc;

use arrow_array::RecordBatch;

use comun_store_v2::*;
use lakehouse_rag::application::dtos::lakehouse_filas::LoteIngesta;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, LIMITE_MAXIMO_LECTURA, LakehouseStorePort, TABLA_CHUNKS_NORMATIVA,
    TABLA_DOCUMENTOS_CATALOGO, TODAS_LAS_TABLAS,
};
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::infrastructure::llm::MockDistiller;
use lakehouse_rag::infrastructure::storage::lancedb_store::{LanceDbStore, literal_sql};

fn filas_totales(lotes: &[RecordBatch]) -> usize {
    lotes.iter().map(RecordBatch::num_rows).sum()
}

async fn total_fisico(store: &LanceDbStore) -> usize {
    let mut total = 0;
    for tabla in TODAS_LAS_TABLAS {
        total += store.contar_registros(tabla).await.unwrap();
    }
    total
}

async fn vigentes_de(store: &LanceDbStore, version: u32) -> usize {
    let mut total = 0;
    for tabla in TODAS_LAS_TABLAS {
        let filtro = format!("version_num = {version} AND vigente = true");
        total += filas_totales(&store.leer_filas_crudas(tabla, Some(&filtro)).await.unwrap());
    }
    total
}

#[tokio::test]
async fn adv_ingest_id_malicioso_se_rechaza_y_el_escape_neutraliza_comillas() {
    for malicioso in [
        "x' OR '1'='1",
        "a'; DELETE FROM t; --",
        "ING 01",
        "ING\0NULO",
        "ING-ñ",
        "../../etc",
        "",
        &"A".repeat(65),
    ] {
        assert!(
            matches!(
                IngestId::new(malicioso),
                Err(DomainError::ValidationError(_))
            ),
            "IngestId aceptó {malicioso:?}"
        );
    }
    assert_eq!(literal_sql("x' OR '1'='1"), "'x'' OR ''1''=''1'");

    // Un ingest_id válido pero «parecido» no toca las filas de otra ingesta
    let (_dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 3, 1), HASH_A).await;
    let antes = total_fisico(&store).await;
    let parecido = IngestId::new("ING-V3-").unwrap();
    assert_eq!(store.revertir_ingesta(&parecido).await.unwrap(), 0);
    assert_eq!(total_fisico(&store).await, antes);
}

#[tokio::test]
async fn adv_doc_id_sin_alfanumericos_se_rechaza_antes_de_escribir() {
    let (_dir, store) = store_temporal().await;
    let distiller = MockDistiller::new();
    let mut paquete = distiller_paquete_v3();
    // Fase 11: el doc_id libre se canonicaliza (`PO-ALM-03'B` → `PO-ALM-03-B`); solo uno sin
    // ningún alfanumérico queda vacío y se rechaza.
    paquete.catalogo.doc_id = "' ; -- '".to_string();
    distiller.registrar_respuesta(ARCHIVO_V3, paquete);

    let error = servicio(Arc::new(distiller), store.clone())
        .ingestar_documento(&centro(), ARCHIVO_V3, CONTENIDO_V3)
        .await
        .unwrap_err();
    assert!(matches!(error, DomainError::InvalidDocId(_)), "{error:?}");
    assert_eq!(total_fisico(&store).await, 0, "no debe quedar ninguna fila");

    // Filtros hostiles en hash: se rechazan antes de llegar a LanceDB
    let hostil = format!("{}' OR '1'='1", &HASH_A[..50]);
    assert!(matches!(
        store.existe_hash(&centro(), &hostil).await,
        Err(DomainError::ValidationError(_))
    ));
}

#[tokio::test]
async fn adv_confirmacion_doble_falla_sin_alterar_el_estado() {
    let (_dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 3, 1), HASH_A).await;
    confirmar_revision(&store, "ING-V4", 4, fecha(2026, 9, 15), HASH_B).await;
    let vigentes_v4 = vigentes_de(&store, 4).await;

    for repetida in ["ING-V4", "ING-V3"] {
        let error = store
            .confirmar_ingesta(&IngestId::new(repetida).unwrap(), &doc())
            .await
            .unwrap_err();
        assert!(
            matches!(error, DomainError::ValidationError(_)),
            "{error:?}"
        );
    }
    assert_eq!(vigentes_de(&store, 4).await, vigentes_v4);
    assert_eq!(
        vigentes_de(&store, 3).await,
        0,
        "reconfirmar la v3 no puede reactivarla"
    );

    // Una ingesta confirmada no puede revertirse (dejaría el documento sin versión vigente)
    assert!(
        store
            .revertir_ingesta(&IngestId::new("ING-V4").unwrap())
            .await
            .is_err()
    );
    assert!(store.verificar_integridad().await.unwrap().es_integro());
}

#[tokio::test]
async fn adv_rollback_de_ingest_id_inexistente_es_idempotente() {
    let (_dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 3, 1), HASH_A).await;
    let antes = total_fisico(&store).await;
    let fantasma = IngestId::new("ING-NO-EXISTE").unwrap();
    for _ in 0..3 {
        assert_eq!(store.revertir_ingesta(&fantasma).await.unwrap(), 0);
    }
    assert_eq!(total_fisico(&store).await, antes);

    // Confirmar una ingesta inexistente es un error controlado
    assert!(matches!(
        store.confirmar_ingesta(&fantasma, &doc()).await,
        Err(DomainError::ValidationError(_))
    ));
}

#[tokio::test]
async fn adv_lotes_vacios_o_mezclados_se_rechazan() {
    let (_dir, store) = store_temporal().await;
    let ingest = IngestId::new("ING-VACIA").unwrap();

    for lotes in [
        Vec::new(),
        vec![LoteIngesta::Chunks(Vec::new())],
        TODAS_LAS_TABLAS
            .iter()
            .map(|_| LoteIngesta::Celdas(Vec::new()))
            .collect(),
    ] {
        assert!(matches!(
            store.escribir_staged(&ingest, lotes).await,
            Err(DomainError::ValidationError(_))
        ));
    }

    let mut mezclados = revision(DOC, 3, fecha(2026, 3, 1), HASH_A);
    mezclados.extend(revision("PO-ALM-04", 1, fecha(2026, 3, 1), HASH_B));
    assert!(matches!(
        store.escribir_staged(&ingest, mezclados).await,
        Err(DomainError::ValidationError(_))
    ));
    assert_eq!(total_fisico(&store).await, 0);

    // Sin fila de catálogo no hay nada que confirmar
    let solo_chunks: Vec<LoteIngesta> = revision(DOC, 3, fecha(2026, 3, 1), HASH_A)
        .into_iter()
        .filter(|l| matches!(l, LoteIngesta::Chunks(_)))
        .collect();
    store.escribir_staged(&ingest, solo_chunks).await.unwrap();
    assert!(store.confirmar_ingesta(&ingest, &doc()).await.is_err());
    assert_eq!(store.revertir_ingesta(&ingest).await.unwrap(), 2);
    assert!(store.verificar_integridad().await.unwrap().es_integro());
}

#[tokio::test]
async fn adv_regresion_de_version_o_de_fecha_no_deroga_la_vigente() {
    let (_dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 3, 1), HASH_A).await;

    for (ingest, version, vigor) in [
        ("ING-V2", 2, fecha(2026, 6, 1)),
        ("ING-V3-BIS", 3, fecha(2026, 6, 1)),
        ("ING-V5-ANTIGUA", 5, fecha(2025, 1, 1)),
    ] {
        let id = IngestId::new(ingest).unwrap();
        store
            .escribir_staged(&id, revision(DOC, version, vigor, HASH_B))
            .await
            .unwrap();
        let error = store.confirmar_ingesta(&id, &doc()).await.unwrap_err();
        assert!(
            matches!(
                error,
                DomainError::VersionConflict(_) | DomainError::ValidationError(_)
            ),
            "{ingest}: {error:?}"
        );
        store.revertir_ingesta(&id).await.unwrap();
        assert_eq!(
            vigentes_de(&store, 3).await,
            FILAS_POR_REVISION.iter().sum::<usize>(),
            "{ingest} alteró la versión vigente"
        );
    }
    assert!(store.verificar_integridad().await.unwrap().es_integro());
}

#[tokio::test]
async fn adv_confirmar_con_doc_id_ajeno_falla() {
    let (_dir, store) = store_temporal().await;
    let id = IngestId::new("ING-AJENA").unwrap();
    store
        .escribir_staged(&id, revision(DOC, 1, fecha(2026, 1, 1), HASH_A))
        .await
        .unwrap();
    let otro = lakehouse_rag::domain::models::document::DocId::new("PO-ALM-99").unwrap();
    assert!(matches!(
        store.confirmar_ingesta(&id, &otro).await,
        Err(DomainError::ValidationError(_))
    ));
    assert!(
        store
            .leer_tabla(TABLA_DOCUMENTOS_CATALOGO, &ConsultaTabla::confirmadas(10))
            .await
            .unwrap()
            .iter()
            .all(|l| l.num_rows() == 0)
    );
}

#[tokio::test]
async fn adv_lecturas_con_limites_frontera_y_tablas_desconocidas() {
    let (_dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 3, 1), HASH_A).await;

    let cero = store
        .leer_tabla(TABLA_CHUNKS_NORMATIVA, &ConsultaTabla::confirmadas(0))
        .await
        .unwrap();
    assert_eq!(filas_totales(&cero), 0);
    let maximo = store
        .leer_tabla(
            TABLA_CHUNKS_NORMATIVA,
            &ConsultaTabla::confirmadas(LIMITE_MAXIMO_LECTURA),
        )
        .await
        .unwrap();
    assert_eq!(filas_totales(&maximo), FILAS_POR_REVISION[1]);
    assert!(
        store
            .leer_tabla(
                TABLA_CHUNKS_NORMATIVA,
                &ConsultaTabla::confirmadas(LIMITE_MAXIMO_LECTURA + 1)
            )
            .await
            .is_err()
    );
    for desconocida in ["01_doc_catalog", "02_chunks_normativa; DROP", ""] {
        assert!(matches!(
            store
                .leer_tabla(desconocida, &ConsultaTabla::confirmadas(10))
                .await,
            Err(DomainError::ValidationError(_))
        ));
    }
}

#[tokio::test]
async fn adv_ingestas_concurrentes_del_mismo_documento_dejan_una_sola_version_vigente() {
    let (_dir, store) = store_temporal().await;
    let mut tareas = Vec::new();
    for (ingest, version, mes, hash) in [
        ("ING-C1", 1, 1, HASH_A),
        ("ING-C2", 2, 2, HASH_B),
        ("ING-C3", 3, 3, HASH_C),
    ] {
        let store = store.clone();
        tareas.push(tokio::spawn(async move {
            let id = IngestId::new(ingest).unwrap();
            store
                .escribir_staged(&id, revision(DOC, version, fecha(2026, mes, 1), hash))
                .await?;
            match store.confirmar_ingesta(&id, &doc()).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    store.revertir_ingesta(&id).await?;
                    Err(e)
                }
            }
        }));
    }
    let mut confirmadas = 0;
    for tarea in tareas {
        if tarea.await.unwrap().is_ok() {
            confirmadas += 1;
        }
    }
    assert!(confirmadas >= 1);
    let informe = store.verificar_integridad().await.unwrap();
    assert!(informe.es_integro(), "{informe:?}");
}

#[tokio::test]
async fn adv_doc_id_con_comilla_se_canonicaliza_sin_inyectar_la_comilla() {
    let (_dir, store) = store_temporal().await;
    let distiller = MockDistiller::new();
    let mut paquete = distiller_paquete_v3();
    paquete.catalogo.doc_id = "PO-ALM-03'B".to_string();
    distiller.registrar_respuesta(ARCHIVO_V3, paquete);

    let reporte = servicio(Arc::new(distiller), store.clone())
        .ingestar_documento(&centro(), ARCHIVO_V3, CONTENIDO_V3)
        .await
        .unwrap();
    assert_eq!(reporte.doc_id, "PO-ALM-03-B");
    assert!(store.verificar_integridad().await.unwrap().es_integro());
}

fn distiller_paquete_v3() -> lakehouse_rag::application::dtos::ingestion::DistilledDocumentPackage {
    paquete_revision(3, "2026-03-01", "FOR-ALM-02")
}
