//! Utilidades de las suites de ingesta universal: almacén temporal, destiladores simulados y
//! lectura de columnas. Nada llama a Claude ni carga modelos reales.
#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arrow_array::{Array, RecordBatch, StringArray};
use async_trait::async_trait;

use lakehouse_rag::application::dtos::ingestion::{
    CatalogoDestilado, DistilledDocumentPackage, HitoDestilado, ParsedDocument,
};
use lakehouse_rag::application::ports::lakehouse_store::{LakehouseStorePort, TODAS_LAS_TABLAS};
use lakehouse_rag::application::ports::llm_distiller::LlmDistillerPort;
use lakehouse_rag::application::use_cases::document_ingestion_service::DocumentIngestionService;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::llm::mock_distiller::MockDistiller;
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::schemas::DIMENSION_EMBEDDINGS;

pub fn centro() -> CenterId {
    CenterId::new("planta").unwrap()
}

pub async fn almacen() -> (tempfile::TempDir, Arc<LanceDbStore>) {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceDbStore::conectar(dir.path()).await.unwrap();
    store.inicializar_tablas().await.unwrap();
    (dir, Arc::new(store))
}

pub fn motor_mock() -> Arc<MockEmbeddingEngine> {
    Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )))
}

pub fn servicio_con(
    destilador: Arc<dyn LlmDistillerPort>,
    store: Arc<dyn LakehouseStorePort>,
) -> DocumentIngestionService {
    DocumentIngestionService::new(
        Arc::new(MarkdownParser::new()),
        destilador,
        motor_mock(),
        store,
    )
}

/// Destilador que siempre falla, como Claude ausente, lento o con JSON basura
pub fn destilador_caido(motivo: &str) -> Arc<MockDistiller> {
    let destilador = MockDistiller::new();
    destilador.programar_error(motivo);
    Arc::new(destilador)
}

pub fn catalogo(doc_id: &str, version_num: i64) -> CatalogoDestilado {
    CatalogoDestilado {
        doc_id: doc_id.to_string(),
        titulo: "Documento".to_string(),
        tipo_documento: "PROCEDIMIENTO_OBRA".to_string(),
        revision: format!("Rev. {version_num:02}"),
        version_num,
        fecha_vigor: Some("2026-03-01".to_string()),
        emisor: "Calidad".to_string(),
    }
}

/// Paquete con un hito, para comprobar que la Capa 2 se escribe
pub fn paquete(doc_id: &str, version_num: i64) -> DistilledDocumentPackage {
    DistilledDocumentPackage {
        catalogo: catalogo(doc_id, version_num),
        hitos: vec![HitoDestilado {
            orden_secuencia: 1,
            fase_operativa: "RECEPCION".to_string(),
            objeto_alcance: "Material".to_string(),
            rol_ejecutor: "Auxiliar".to_string(),
            rol_responsable: "Jefe".to_string(),
            origen_responsable: "INFERIDO".to_string(),
            registro_codigo: None,
            registro_nombre: None,
            criterio_aceptacion: "Cumple".to_string(),
            condicion_aplicacion: None,
            seccion_origen: "1".to_string(),
        }],
        tablas: Vec::new(),
        aristas: Vec::new(),
    }
}

/// Destilador de respuesta fija que cuenta sus llamadas
pub struct DestiladorFijo {
    pub paquete: DistilledDocumentPackage,
    pub llamadas: AtomicUsize,
}

impl DestiladorFijo {
    pub fn nuevo(paquete: DistilledDocumentPackage) -> Arc<Self> {
        Arc::new(Self {
            paquete,
            llamadas: AtomicUsize::new(0),
        })
    }

    pub fn total_llamadas(&self) -> usize {
        self.llamadas.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl LlmDistillerPort for DestiladorFijo {
    async fn distill(&self, _: &ParsedDocument) -> Result<DistilledDocumentPackage, DomainError> {
        self.llamadas.fetch_add(1, Ordering::SeqCst);
        Ok(self.paquete.clone())
    }
}

/// Valores de una columna de texto (nulos como `None`)
pub fn columna_texto(lotes: &[RecordBatch], nombre: &str) -> Vec<Option<String>> {
    lotes
        .iter()
        .flat_map(|lote| {
            let valores = lote
                .column_by_name(nombre)
                .unwrap_or_else(|| panic!("falta {nombre}"))
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            (0..lote.num_rows())
                .map(|i| valores.is_valid(i).then(|| valores.value(i).to_string()))
                .collect::<Vec<_>>()
        })
        .collect()
}

pub async fn columna(store: &LanceDbStore, tabla: &str, nombre: &str) -> Vec<Option<String>> {
    let lotes = store.leer_filas_crudas(tabla, None).await.unwrap();
    columna_texto(&lotes, nombre)
}

pub async fn textos(store: &LanceDbStore, tabla: &str, nombre: &str) -> Vec<String> {
    columna(store, tabla, nombre)
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// Filas físicas de todas las tablas, para comparar el estado antes y después de un fallo
pub async fn foto_de_filas(store: &LanceDbStore) -> Vec<(String, Vec<String>)> {
    let mut foto = Vec::new();
    for tabla in TODAS_LAS_TABLAS {
        let mut ingestas = textos(store, tabla, "ingest_id").await;
        ingestas.sort();
        foto.push((tabla.to_string(), ingestas));
    }
    foto
}

pub async fn exigir_integridad(store: &LanceDbStore) {
    let informe = store.verificar_integridad().await.unwrap();
    assert!(informe.es_integro(), "{informe:?}");
}
