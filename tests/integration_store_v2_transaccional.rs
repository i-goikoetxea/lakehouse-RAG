//! Integración del almacén v2 transaccional (Fase 10): staged → commit, rollback, idempotencia
//! por hash, retirada de vigencia en las 6 tablas, IDs versionados y `verificar_integridad`.

mod comun_store_v2;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow_array::{Array, Date32Array, RecordBatch, StringArray};
use async_trait::async_trait;

use comun_store_v2::*;
use lakehouse_rag::application::dtos::ingestion::{DistilledDocumentPackage, ParsedDocument};
use lakehouse_rag::application::dtos::lakehouse_filas::LoteIngesta;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, EstadoIngesta, InformeIntegridad, LakehouseStorePort, TABLA_CHUNKS_NORMATIVA,
    TABLA_DOCUMENTOS_CATALOGO, TODAS_LAS_TABLAS,
};
use lakehouse_rag::application::ports::llm_distiller::LlmDistillerPort;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::lotes_arrow::ColumnasTransaccionales;

/// Decorador del almacén que falla al llegar al lote de la tabla 05 (tras escribir 01-04)
struct AlmacenConFalloEnCeldas {
    interno: Arc<LanceDbStore>,
}

#[async_trait]
impl LakehouseStorePort for AlmacenConFalloEnCeldas {
    async fn inicializar_tablas(&self) -> Result<(), DomainError> {
        self.interno.inicializar_tablas().await
    }
    async fn existe_hash(
        &self,
        centro_id: &CenterId,
        hash_sha256: &str,
    ) -> Result<
        Option<lakehouse_rag::application::ports::lakehouse_store::HuellaIngerida>,
        DomainError,
    > {
        self.interno.existe_hash(centro_id, hash_sha256).await
    }
    async fn escribir_staged(
        &self,
        ingest_id: &IngestId,
        lotes: Vec<LoteIngesta>,
    ) -> Result<usize, DomainError> {
        let mut escritas = 0;
        for lote in lotes {
            if matches!(lote, LoteIngesta::Celdas(_)) {
                return Err(DomainError::StorageError(
                    "Fallo inyectado al escribir 05_celdas_doble_entrada".to_string(),
                ));
            }
            escritas += self.interno.escribir_staged(ingest_id, vec![lote]).await?;
        }
        Ok(escritas)
    }
    async fn confirmar_ingesta(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
    ) -> Result<lakehouse_rag::application::ports::lakehouse_store::ResumenConfirmacion, DomainError>
    {
        self.interno.confirmar_ingesta(ingest_id, doc_id).await
    }
    async fn revertir_ingesta(&self, ingest_id: &IngestId) -> Result<u64, DomainError> {
        self.interno.revertir_ingesta(ingest_id).await
    }
    async fn leer_tabla(
        &self,
        nombre_tabla: &str,
        consulta: &ConsultaTabla,
    ) -> Result<Vec<RecordBatch>, DomainError> {
        self.interno.leer_tabla(nombre_tabla, consulta).await
    }
    async fn verificar_integridad(&self) -> Result<InformeIntegridad, DomainError> {
        self.interno.verificar_integridad().await
    }
    async fn contar_registros(&self, nombre_tabla: &str) -> Result<usize, DomainError> {
        self.interno.contar_registros(nombre_tabla).await
    }
    async fn listar_tablas(&self) -> Result<Vec<String>, DomainError> {
        self.interno.listar_tablas().await
    }
}

/// Destilador que cuenta las llamadas (para comprobar la idempotencia sin LLM)
struct DestiladorContador {
    interno: Arc<dyn LlmDistillerPort>,
    llamadas: AtomicUsize,
}

#[async_trait]
impl LlmDistillerPort for DestiladorContador {
    async fn distill(
        &self,
        parsed: &ParsedDocument,
    ) -> Result<DistilledDocumentPackage, DomainError> {
        self.llamadas.fetch_add(1, Ordering::SeqCst);
        self.interno.distill(parsed).await
    }
    fn json_schema(&self) -> String {
        self.interno.json_schema()
    }
}

fn filas_totales(lotes: &[RecordBatch]) -> usize {
    lotes.iter().map(RecordBatch::num_rows).sum()
}

async fn filas_con(store: &LanceDbStore, tabla: &str, filtro: &str) -> usize {
    filas_totales(&store.leer_filas_crudas(tabla, Some(filtro)).await.unwrap())
}

#[tokio::test]
async fn staged_no_es_visible_hasta_confirmar_y_commit_activa_las_6_tablas() {
    let (_dir, store) = store_temporal().await;
    let ingest = IngestId::new("ING-STAGED-01").unwrap();
    let escritas = store
        .escribir_staged(&ingest, revision(DOC, 3, fecha(2026, 3, 1), HASH_A))
        .await
        .unwrap();
    assert_eq!(escritas, FILAS_POR_REVISION.iter().sum::<usize>());

    for tabla in TODAS_LAS_TABLAS {
        let confirmadas = store
            .leer_tabla(tabla, &ConsultaTabla::confirmadas(1000))
            .await
            .unwrap();
        assert_eq!(
            filas_totales(&confirmadas),
            0,
            "{tabla} expone filas STAGED"
        );
    }
    let informe = store.verificar_integridad().await.unwrap();
    assert_eq!(informe.ingestas_staged, vec!["ING-STAGED-01".to_string()]);
    assert!(
        store
            .existe_hash(&centro(), HASH_A)
            .await
            .unwrap()
            .is_none()
    );

    let resumen = store.confirmar_ingesta(&ingest, &doc()).await.unwrap();
    assert_eq!(resumen.filas_activadas as usize, escritas);
    assert!(resumen.versiones_derogadas.is_empty());

    for (tabla, esperadas) in TODAS_LAS_TABLAS.iter().zip(FILAS_POR_REVISION) {
        let vigentes = store
            .leer_tabla(
                tabla,
                &ConsultaTabla::confirmadas(1000)
                    .solo_vigentes()
                    .del_documento(&doc()),
            )
            .await
            .unwrap();
        assert_eq!(filas_totales(&vigentes), esperadas, "{tabla}");
    }
    let huella = store.existe_hash(&centro(), HASH_A).await.unwrap().unwrap();
    assert_eq!((huella.version_num, huella.ingest_id), (3, ingest));
    assert!(store.verificar_integridad().await.unwrap().es_integro());
}

#[tokio::test]
async fn rev03_a_rev04_deja_cero_filas_vigentes_de_la_v3_en_las_6_tablas() {
    let (_dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 3, 1), HASH_A).await;
    let resumen = confirmar_revision(&store, "ING-V4", 4, fecha(2026, 9, 15), HASH_B).await;
    assert_eq!(resumen.versiones_derogadas, vec![3]);

    for (tabla, esperadas) in TODAS_LAS_TABLAS.iter().zip(FILAS_POR_REVISION) {
        let v3_vigentes = filas_con(
            &store,
            tabla,
            &format!("doc_id = '{DOC}' AND version_num = 3 AND vigente = true"),
        )
        .await;
        let v4_vigentes = filas_con(
            &store,
            tabla,
            &format!("doc_id = '{DOC}' AND version_num = 4 AND vigente = true"),
        )
        .await;
        assert_eq!(
            v3_vigentes, 0,
            "{tabla} conserva filas vigentes de la Rev. 03 (H3)"
        );
        assert_eq!(v4_vigentes, esperadas, "{tabla}");
    }

    let catalogo_v3 = store
        .leer_filas_crudas(TABLA_DOCUMENTOS_CATALOGO, Some("version_num = 3"))
        .await
        .unwrap();
    let derogacion = catalogo_v3[0]
        .column_by_name("fecha_derogacion")
        .and_then(|c| c.as_any().downcast_ref::<Date32Array>())
        .unwrap();
    assert_eq!(derogacion.value_as_date(0), Some(fecha(2026, 9, 15)));
    let catalogo_v4 = store
        .leer_filas_crudas(TABLA_DOCUMENTOS_CATALOGO, Some("version_num = 4"))
        .await
        .unwrap();
    assert!(
        catalogo_v4[0]
            .column_by_name("fecha_derogacion")
            .unwrap()
            .is_null(0)
    );
    assert!(store.verificar_integridad().await.unwrap().es_integro());
}

#[tokio::test]
async fn ids_primarios_unicos_entre_revisiones_en_las_6_tablas() {
    let (_dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 3, 1), HASH_A).await;
    confirmar_revision(&store, "ING-V4", 4, fecha(2026, 9, 15), HASH_B).await;

    let claves_primarias = [
        (TABLA_DOCUMENTOS_CATALOGO, "ingest_id"),
        (TABLA_CHUNKS_NORMATIVA, "chunk_id"),
        ("03_matriz_operativa", "hito_id"),
        ("04_catalogo_tablas", "tabla_id"),
        ("05_celdas_doble_entrada", "celda_id"),
        ("06_grafo_referencias", "edge_id"),
    ];
    for (tabla, columna) in claves_primarias {
        let lotes = store.leer_filas_crudas(tabla, None).await.unwrap();
        let ids: Vec<String> = lotes
            .iter()
            .flat_map(|l| {
                let col = l
                    .column_by_name(columna)
                    .unwrap()
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap();
                (0..col.len())
                    .map(|i| col.value(i).to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        let distintos: BTreeSet<&String> = ids.iter().collect();
        assert_eq!(
            distintos.len(),
            ids.len(),
            "{tabla}.{columna} repetidos: {ids:?}"
        );
    }
}

#[tokio::test]
async fn servicio_revierte_todo_ante_fallo_inyectado_en_la_tabla_05() {
    let (_dir, store) = store_temporal().await;
    let distiller = distiller_revisiones();

    let servicio_ok = servicio(distiller.clone(), store.clone());
    servicio_ok
        .ingestar_documento(&centro(), ARCHIVO_V3, CONTENIDO_V3)
        .await
        .unwrap();

    let con_fallo = servicio(
        distiller,
        Arc::new(AlmacenConFalloEnCeldas {
            interno: store.clone(),
        }),
    );
    let error = con_fallo
        .ingestar_documento(&centro(), ARCHIVO_V4, CONTENIDO_V4)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("Fallo inyectado"), "{error}");

    for tabla in TODAS_LAS_TABLAS {
        let residuales = filas_con(&store, tabla, "version_num = 4").await;
        assert_eq!(
            residuales, 0,
            "{tabla} conserva filas de la ingesta fallida"
        );
        let v3_vigentes = filas_con(&store, tabla, "version_num = 3 AND vigente = true").await;
        assert!(v3_vigentes > 0, "{tabla}: la Rev. 03 dejó de estar vigente");
    }
    let informe = store.verificar_integridad().await.unwrap();
    assert!(informe.es_integro(), "{informe:?}");
}

#[tokio::test]
async fn reingesta_del_mismo_hash_no_llama_al_destilador_ni_duplica_filas() {
    let (_dir, store) = store_temporal().await;
    let contador = Arc::new(DestiladorContador {
        interno: distiller_revisiones(),
        llamadas: AtomicUsize::new(0),
    });
    let servicio = servicio(contador.clone(), store.clone());

    let primero = servicio
        .ingestar_documento(&centro(), ARCHIVO_V3, CONTENIDO_V3)
        .await
        .unwrap();
    let mut conteos = Vec::new();
    for tabla in TODAS_LAS_TABLAS {
        conteos.push(store.contar_registros(tabla).await.unwrap());
    }

    let segundo = servicio
        .ingestar_documento(&centro(), ARCHIVO_V3, CONTENIDO_V3)
        .await
        .unwrap();
    assert!(segundo.ya_ingerido && !primero.ya_ingerido);
    assert_eq!(segundo.ingest_id, primero.ingest_id);
    assert_eq!((segundo.doc_id.as_str(), segundo.version), (DOC, 3));
    assert_eq!(
        contador.llamadas.load(Ordering::SeqCst),
        1,
        "se volvió a llamar al LLM"
    );
    for (tabla, previas) in TODAS_LAS_TABLAS.iter().zip(conteos) {
        assert_eq!(
            store.contar_registros(tabla).await.unwrap(),
            previas,
            "{tabla}"
        );
    }

    let otro_centro = CenterId::new("otra-planta").unwrap();
    let en_otro = servicio
        .ingestar_documento(&otro_centro, ARCHIVO_V3, CONTENIDO_V3)
        .await
        .unwrap();
    assert!(!en_otro.ya_ingerido, "la idempotencia es por centro");
}

#[tokio::test]
async fn servicio_rev03_a_rev04_usa_ids_versionados_y_retira_el_grafo() {
    let (_dir, store) = store_temporal().await;
    let servicio = servicio(distiller_revisiones(), store.clone());
    let v3 = servicio
        .ingestar_documento(&centro(), ARCHIVO_V3, CONTENIDO_V3)
        .await
        .unwrap();
    let v4 = servicio
        .ingestar_documento(&centro(), ARCHIVO_V4, CONTENIDO_V4)
        .await
        .unwrap();
    assert_eq!(v4.versiones_derogadas, vec![3]);
    assert_ne!(v3.ingest_id, v4.ingest_id);

    let aristas_v3_vigentes = filas_con(
        &store,
        "06_grafo_referencias",
        "version_num = 3 AND vigente = true",
    )
    .await;
    assert_eq!(aristas_v3_vigentes, 0);
    let chunk_ids = store
        .leer_filas_crudas(TABLA_CHUNKS_NORMATIVA, None)
        .await
        .unwrap();
    let ids = chunk_ids[0]
        .column_by_name("chunk_id")
        .unwrap()
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap()
        .clone();
    assert!(ids.value(0).contains("@v3-CHK-") || ids.value(0).contains("@v4-CHK-"));
    assert!(store.verificar_integridad().await.unwrap().es_integro());
}

#[tokio::test]
async fn verificar_integridad_detecta_huerfanos_staged_y_versiones_vigentes_multiples() {
    let (_dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 3, 1), HASH_A).await;

    // Huérfano: chunk confirmado de una versión que no existe en 01
    let confirmada_vigente = ColumnasTransaccionales {
        ingest_id: IngestId::new("ING-HUERFANO").unwrap(),
        estado: EstadoIngesta::Committed,
        vigente: true,
    };
    let huerfano = revision(DOC, 9, fecha(2026, 5, 1), HASH_B)
        .into_iter()
        .find(|l| matches!(l, LoteIngesta::Chunks(_)))
        .unwrap();
    store
        .sembrar_filas(&huerfano, &confirmada_vigente)
        .await
        .unwrap();

    // STAGED colgado de otra ingesta
    store
        .escribir_staged(
            &IngestId::new("ING-COLGADA").unwrap(),
            revision("PO-ALM-99", 1, fecha(2026, 1, 1), HASH_C),
        )
        .await
        .unwrap();

    let informe = store.verificar_integridad().await.unwrap();
    assert!(!informe.es_integro());
    assert!(
        informe
            .huerfanos
            .iter()
            .any(|h| h.tabla == TABLA_CHUNKS_NORMATIVA
                && h.doc_id == DOC
                && h.version_num == 9
                && h.filas == FILAS_POR_REVISION[1]),
        "{:?}",
        informe.huerfanos
    );
    assert_eq!(informe.ingestas_staged, vec!["ING-COLGADA".to_string()]);
    assert!(
        informe
            .versiones_vigentes_multiples
            .iter()
            .any(|v| v.tabla == TABLA_CHUNKS_NORMATIVA
                && v.doc_id == DOC
                && v.versiones == vec![3, 9]),
        "{:?}",
        informe.versiones_vigentes_multiples
    );
}

#[tokio::test]
async fn leer_tabla_respeta_limite_explicito_por_encima_de_10_filas() {
    let (_dir, store) = store_temporal().await;
    let ingest = IngestId::new("ING-MUCHAS").unwrap();
    store
        .escribir_staged(&ingest, revision_con_chunks(DOC, 1, 25))
        .await
        .unwrap();
    store.confirmar_ingesta(&ingest, &doc()).await.unwrap();

    let todas = store
        .leer_tabla(TABLA_CHUNKS_NORMATIVA, &ConsultaTabla::confirmadas(1000))
        .await
        .unwrap();
    assert_eq!(
        filas_totales(&todas),
        25,
        "trampa O4: LanceDB limita a 10 sin .limit()"
    );
    let acotadas = store
        .leer_tabla(TABLA_CHUNKS_NORMATIVA, &ConsultaTabla::confirmadas(7))
        .await
        .unwrap();
    assert_eq!(filas_totales(&acotadas), 7);
}
