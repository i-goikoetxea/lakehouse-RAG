//! Pruebas de Integración para Búsqueda Híbrida (Vectorial + BM25 + Filtros Arrow + RRF)

use std::sync::Arc;

use tempfile::tempdir;

use lakehouse_rag::application::dtos::lakehouse_filas::{ClaveDocumental, FilaChunk, LoteIngesta};
use lakehouse_rag::application::ports::embedding_engine::EmbeddingEnginePort;
use lakehouse_rag::application::ports::hybrid_search::{HybridSearchParams, HybridSearchPort};
use lakehouse_rag::application::ports::lakehouse_store::{EstadoIngesta, LakehouseStorePort};
use lakehouse_rag::application::use_cases::hybrid_search_service::HybridSearchService;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::{DocId, TipoDocumento};
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::lotes_arrow::ColumnasTransaccionales;
use lakehouse_rag::infrastructure::storage::schemas::DIMENSION_EMBEDDINGS;

/// Datos de prueba para sembrar un chunk en el almacén v2
struct TestChunkFixture<'a> {
    chunk_id: &'a str,
    doc_id: &'a str,
    centro_id: &'a str,
    seccion: &'a str,
    contenido: &'a str,
    vector: Vec<f32>,
    vigente: bool,
}

impl TestChunkFixture<'_> {
    fn a_fila(&self) -> FilaChunk {
        FilaChunk {
            clave: ClaveDocumental {
                centro_id: CenterId::new(self.centro_id).expect("centro_id válido"),
                doc_id: DocId::new(self.doc_id).expect("doc_id válido"),
                version_num: 1,
            },
            chunk_id: self.chunk_id.to_string(),
            nivel_prelacion: TipoDocumento::ProcedimientoObra.nivel_prelacion(),
            breadcrumb_jerarquia: self.seccion.to_string(),
            condicion_aplicacion: None,
            texto_contenido: self.contenido.to_string(),
            tablas_vinculadas: Vec::new(),
            hitos_vinculados: Vec::new(),
            vector: self.vector.clone(),
        }
    }
}

/// Siembra los chunks con el estado indicado, agrupando por el valor de `vigente`
async fn sembrar_chunks(
    store: &LanceDbStore,
    chunks: Vec<TestChunkFixture<'_>>,
    estado: EstadoIngesta,
) {
    let ingest_id = IngestId::new("ingesta-prueba-busqueda").expect("ingest_id válido");
    for vigente in [true, false] {
        let filas: Vec<FilaChunk> = chunks
            .iter()
            .filter(|c| c.vigente == vigente)
            .map(TestChunkFixture::a_fila)
            .collect();
        if filas.is_empty() {
            continue;
        }
        let transaccion = ColumnasTransaccionales {
            ingest_id: ingest_id.clone(),
            estado,
            vigente,
        };
        store
            .sembrar_filas(&LoteIngesta::Chunks(filas), &transaccion)
            .await
            .expect("Sembrar chunks");
    }
}

#[tokio::test]
async fn test_integration_busqueda_hibrida_recupera_chunk_relevante_por_rrf() {
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

    let engine = Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )));

    let t1 = "Procedimiento de inspección y calibración de válvula de alivio de presión en calderas de vapor";
    let t2 = "Plan de mantenimiento preventivo y lubricación de cintas transportadoras de mineral";
    let t3 = "Protocolo de seguridad operacional en reactores químicos de alta temperatura";

    let v1 = engine.generate_embedding(t1).await.unwrap();
    let v2 = engine.generate_embedding(t2).await.unwrap();
    let v3 = engine.generate_embedding(t3).await.unwrap();

    let chunks = vec![
        TestChunkFixture {
            chunk_id: "CHK-001",
            doc_id: "DOC-CAL-01",
            centro_id: "planta_central",
            seccion: "Sección 4.2: Válvulas de Alivio",
            contenido: t1,
            vector: v1,
            vigente: true,
        },
        TestChunkFixture {
            chunk_id: "CHK-002",
            doc_id: "DOC-MEC-02",
            centro_id: "planta_central",
            seccion: "Sección 1.1: Cintas",
            contenido: t2,
            vector: v2,
            vigente: true,
        },
        TestChunkFixture {
            chunk_id: "CHK-003",
            doc_id: "DOC-QUI-03",
            centro_id: "planta_central",
            seccion: "Sección 3.5: Reactores",
            contenido: t3,
            vector: v3,
            vigente: true,
        },
    ];

    sembrar_chunks(&store, chunks, EstadoIngesta::Committed).await;

    let hybrid_service = HybridSearchService::new(store.clone(), engine.clone());

    let params = HybridSearchParams {
        query: "calibracion valvula alivio presion caldera".to_string(),
        centro_id: "planta_central".to_string(),
        top_k: 3,
        solo_vigentes: true,
        peso_vectorial: 0.6,
        peso_bm25: 0.4,
        rrf_k: 60.0,
    };

    let hits = hybrid_service
        .hybrid_search(&params)
        .await
        .expect("Ejecutar búsqueda híbrida");

    assert!(!hits.is_empty(), "Debe retornar al menos 1 resultado");
    assert_eq!(hits[0].chunk_id, "CHK-001");
    assert_eq!(hits[0].doc_id, "DOC-CAL-01");
    assert!(
        hits[0].score_hibrido > 0.0,
        "Score híbrido debe ser positivo"
    );
    assert!(
        hits[0].score_vectorial > 0.0,
        "Score vectorial debe ser positivo"
    );
    assert!(hits[0].score_bm25 > 0.0, "Score BM25 debe ser positivo");
}

#[tokio::test]
async fn test_integration_aislamiento_estricto_multi_centro() {
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

    let engine = Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )));

    let texto = "Manual de contingencias ante derrames de sustancias peligrosas";
    let v_norte = engine.generate_embedding(texto).await.unwrap();
    let v_sur = engine.generate_embedding(texto).await.unwrap();

    let chunks = vec![
        TestChunkFixture {
            chunk_id: "CHK-NORTE-1",
            doc_id: "DOC-NORTE",
            centro_id: "planta_norte",
            seccion: "Emergencias",
            contenido: texto,
            vector: v_norte,
            vigente: true,
        },
        TestChunkFixture {
            chunk_id: "CHK-SUR-1",
            doc_id: "DOC-SUR",
            centro_id: "planta_sur",
            seccion: "Emergencias",
            contenido: texto,
            vector: v_sur,
            vigente: true,
        },
    ];

    sembrar_chunks(&store, chunks, EstadoIngesta::Committed).await;

    let hybrid_service = HybridSearchService::new(store.clone(), engine.clone());

    // Búsqueda orientada a planta_norte
    let params_norte = HybridSearchParams {
        query: "contingencias sustancias peligrosas".to_string(),
        centro_id: "planta_norte".to_string(),
        top_k: 10,
        solo_vigentes: true,
        ..Default::default()
    };

    let hits_norte = hybrid_service
        .hybrid_search(&params_norte)
        .await
        .expect("Búsqueda norte");

    assert_eq!(hits_norte.len(), 1);
    assert_eq!(hits_norte[0].centro_id, "planta_norte");
    assert_eq!(hits_norte[0].chunk_id, "CHK-NORTE-1");

    // Verificar que ninguna fila de planta_sur se filtre
    assert!(hits_norte.iter().all(|h| h.centro_id == "planta_norte"));
}

#[tokio::test]
async fn test_integration_filtrado_estricto_por_vigencia() {
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

    let engine = Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )));

    let texto_rev1 = "Especificación técnica de motores eléctricos versión preliminar";
    let texto_rev2 = "Especificación técnica de motores eléctricos versión vigente definitiva";

    let v1 = engine.generate_embedding(texto_rev1).await.unwrap();
    let v2 = engine.generate_embedding(texto_rev2).await.unwrap();

    let chunks = vec![
        TestChunkFixture {
            chunk_id: "CHK-REV1",
            doc_id: "DOC-MOT-V1",
            centro_id: "planta_alfa",
            seccion: "Motores",
            contenido: texto_rev1,
            vector: v1,
            vigente: false, // NO VIGENTE (derogado)
        },
        TestChunkFixture {
            chunk_id: "CHK-REV2",
            doc_id: "DOC-MOT-V2",
            centro_id: "planta_alfa",
            seccion: "Motores",
            contenido: texto_rev2,
            vector: v2,
            vigente: true, // VIGENTE
        },
    ];

    sembrar_chunks(&store, chunks, EstadoIngesta::Committed).await;

    let hybrid_service = HybridSearchService::new(store.clone(), engine.clone());

    // Consulta con solo_vigentes = true
    let params_vigentes = HybridSearchParams {
        query: "motores electricos especificacion".to_string(),
        centro_id: "planta_alfa".to_string(),
        top_k: 10,
        solo_vigentes: true,
        ..Default::default()
    };

    let hits_vigentes = hybrid_service
        .hybrid_search(&params_vigentes)
        .await
        .expect("Búsqueda solo vigentes");

    assert_eq!(hits_vigentes.len(), 1);
    assert_eq!(hits_vigentes[0].chunk_id, "CHK-REV2");
    assert!(hits_vigentes[0].vigente);

    // Consulta con solo_vigentes = false
    let params_todos = HybridSearchParams {
        query: "motores electricos especificacion".to_string(),
        centro_id: "planta_alfa".to_string(),
        top_k: 10,
        solo_vigentes: false,
        ..Default::default()
    };

    let hits_todos = hybrid_service
        .hybrid_search(&params_todos)
        .await
        .expect("Búsqueda todos");

    assert_eq!(hits_todos.len(), 2);
}

#[tokio::test]
async fn test_integration_ponderacion_pesos_rrf() {
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

    let engine = Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )));

    let t1 = "Procedimiento especial código ISO-9001 calidad total";
    let v1 = engine.generate_embedding(t1).await.unwrap();

    let chunks = vec![TestChunkFixture {
        chunk_id: "CHK-W1",
        doc_id: "DOC-W",
        centro_id: "planta_peso",
        seccion: "Calidad",
        contenido: t1,
        vector: v1,
        vigente: true,
    }];

    sembrar_chunks(&store, chunks, EstadoIngesta::Committed).await;

    let hybrid_service = HybridSearchService::new(store.clone(), engine.clone());

    // 100% vectorial, 0% BM25
    let params_vec_only = HybridSearchParams {
        query: "calidad total ISO-9001".to_string(),
        centro_id: "planta_peso".to_string(),
        top_k: 1,
        solo_vigentes: true,
        peso_vectorial: 1.0,
        peso_bm25: 0.0,
        rrf_k: 60.0,
    };
    let hits_vec = hybrid_service
        .hybrid_search(&params_vec_only)
        .await
        .unwrap();
    assert_eq!(hits_vec.len(), 1);

    // 0% vectorial, 100% BM25
    let params_bm25_only = HybridSearchParams {
        query: "calidad total ISO-9001".to_string(),
        centro_id: "planta_peso".to_string(),
        top_k: 1,
        solo_vigentes: true,
        peso_vectorial: 0.0,
        peso_bm25: 1.0,
        rrf_k: 60.0,
    };
    let hits_bm25 = hybrid_service
        .hybrid_search(&params_bm25_only)
        .await
        .unwrap();
    assert_eq!(hits_bm25.len(), 1);
}

#[tokio::test]
async fn test_integration_chunk_staged_nunca_aparece_en_resultados() {
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

    let engine = Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )));

    let texto = "Instrucción de trabajo para soldadura de tuberías de acero al carbono";
    let vector = engine.generate_embedding(texto).await.unwrap();

    let fixture = |chunk_id, doc_id| TestChunkFixture {
        chunk_id,
        doc_id,
        centro_id: "planta_staged",
        seccion: "Soldadura",
        contenido: texto,
        vector: vector.clone(),
        vigente: true,
    };

    sembrar_chunks(
        &store,
        vec![fixture("CHK-COMMITTED", "DOC-COMMITTED")],
        EstadoIngesta::Committed,
    )
    .await;
    // Fila vigente pero aún STAGED: la ingesta no se ha confirmado
    sembrar_chunks(
        &store,
        vec![fixture("CHK-STAGED", "DOC-STAGED")],
        EstadoIngesta::Staged,
    )
    .await;

    let hybrid_service = HybridSearchService::new(store.clone(), engine.clone());

    for solo_vigentes in [true, false] {
        let params = HybridSearchParams {
            query: "soldadura tuberías acero carbono".to_string(),
            centro_id: "planta_staged".to_string(),
            top_k: 10,
            solo_vigentes,
            ..Default::default()
        };
        let hits = hybrid_service
            .hybrid_search(&params)
            .await
            .expect("Búsqueda con fila staged");

        assert_eq!(hits.len(), 1, "Solo debe verse la fila COMMITTED");
        assert_eq!(hits[0].chunk_id, "CHK-COMMITTED");
        assert!(hits.iter().all(|h| h.chunk_id != "CHK-STAGED"));
    }
}
