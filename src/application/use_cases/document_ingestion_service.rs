//! Caso de uso: ingesta documental transaccional hacia el Lakehouse v2.
//!
//! Flujo (plan v2, problema 8): hash SHA-256 → idempotencia (`existe_hash`) → parseo →
//! destilación → mapeo con red de seguridad (`mapeo_destilacion`) → vectores de chunks, hitos y
//! tablas → `escribir_staged` → `confirmar_ingesta`. Cualquier error posterior a la primera
//! escritura dispara `revertir_ingesta`.

use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::application::dtos::ingestion::{EtapaIngesta, ObservadorIngesta};
use crate::application::dtos::lakehouse_filas::{ClaveDocumental, LoteIngesta};
use crate::application::ports::document_parser::DocumentParserPort;
use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::lakehouse_store::{HuellaIngerida, LakehouseStorePort};
use crate::application::ports::llm_distiller::LlmDistillerPort;
use crate::application::use_cases::mapeo_destilacion::{
    RevisionMapeada, clave_documental, mapear_revision,
};
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::identificadores::IngestId;

/// Reporte consolidado de la ingesta documental
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionReport {
    pub doc_id: String,
    pub centro_id: String,
    pub version: i32,
    pub ingest_id: String,
    /// `true` si el hash ya estaba confirmado: no se llamó al destilador ni se escribió nada
    pub ya_ingerido: bool,
    pub total_chunks: usize,
    pub total_tuplas: usize,
    pub total_tablas: usize,
    pub total_celdas: usize,
    pub total_aristas: usize,
    pub versiones_derogadas: Vec<u32>,
    pub sha256: String,
}

/// Filas v2 de una revisión, listas para escribir como STAGED
struct FilasRevision {
    lotes: Vec<LoteIngesta>,
    total_chunks: usize,
    total_tuplas: usize,
    total_tablas: usize,
    total_celdas: usize,
    total_aristas: usize,
}

/// Orquestador del pipeline de ingesta
pub struct DocumentIngestionService {
    parser: Arc<dyn DocumentParserPort>,
    distiller: Arc<dyn LlmDistillerPort>,
    embedding_engine: Arc<dyn EmbeddingEnginePort>,
    store: Arc<dyn LakehouseStorePort>,
    observador: Option<ObservadorIngesta>,
}

impl DocumentIngestionService {
    pub fn new(
        parser: Arc<dyn DocumentParserPort>,
        distiller: Arc<dyn LlmDistillerPort>,
        embedding_engine: Arc<dyn EmbeddingEnginePort>,
        store: Arc<dyn LakehouseStorePort>,
    ) -> Self {
        Self {
            parser,
            distiller,
            embedding_engine,
            store,
            observador: None,
        }
    }

    /// Notifica cada etapa del pipeline (progreso en la GUI)
    pub fn con_observador(mut self, observador: Option<ObservadorIngesta>) -> Self {
        self.observador = observador;
        self
    }

    fn notificar(&self, etapa: EtapaIngesta) {
        if let Some(observador) = &self.observador {
            observador(etapa);
        }
    }

    /// Ejecuta el pipeline completo; idempotente por hash y atómico por `ingest_id`
    pub async fn ingestar_documento(
        &self,
        centro_id: &CenterId,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<IngestionReport, DomainError> {
        if contenido.trim().is_empty() {
            return Err(DomainError::IngestionError(
                "No se puede ingestar un documento con contenido vacío".to_string(),
            ));
        }

        let hash_sha256 = calcular_sha256(contenido);
        if let Some(huella) = self.store.existe_hash(centro_id, &hash_sha256).await? {
            return Ok(reporte_ya_ingerido(centro_id, huella, hash_sha256));
        }

        self.notificar(EtapaIngesta::Parseo);
        let parsed = self.parser.parse(nombre_archivo, contenido).await?;
        self.notificar(EtapaIngesta::Destilacion);
        let paquete = self.distiller.distill(&parsed).await?;
        let clave = clave_documental(centro_id, &paquete.catalogo)?;
        let revision = mapear_revision(&clave, &parsed, &paquete, &hash_sha256)?;
        self.notificar(EtapaIngesta::Embeddings);
        let filas = self.vectorizar(revision).await?;
        self.notificar(EtapaIngesta::Escritura);

        let ingest_id = IngestId::new(format!("ING-{}", uuid::Uuid::new_v4().simple()))?;
        let versiones_derogadas = self
            .escribir_y_confirmar(&ingest_id, &clave, filas.lotes)
            .await?;

        Ok(IngestionReport {
            doc_id: clave.doc_id.to_string(),
            centro_id: centro_id.to_string(),
            version: i32::try_from(clave.version_num).unwrap_or(i32::MAX),
            ingest_id: ingest_id.to_string(),
            ya_ingerido: false,
            total_chunks: filas.total_chunks,
            total_tuplas: filas.total_tuplas,
            total_tablas: filas.total_tablas,
            total_celdas: filas.total_celdas,
            total_aristas: filas.total_aristas,
            versiones_derogadas,
            sha256: hash_sha256,
        })
    }

    /// Escritura STAGED + commit; ante cualquier fallo revierte todo lo del `ingest_id`
    async fn escribir_y_confirmar(
        &self,
        ingest_id: &IngestId,
        clave: &ClaveDocumental,
        lotes: Vec<LoteIngesta>,
    ) -> Result<Vec<u32>, DomainError> {
        if let Err(error) = self.store.escribir_staged(ingest_id, lotes).await {
            return Err(self.revertir_tras_error(ingest_id, error).await);
        }
        match self.store.confirmar_ingesta(ingest_id, &clave.doc_id).await {
            Ok(resumen) => Ok(resumen.versiones_derogadas),
            Err(error) => Err(self.revertir_tras_error(ingest_id, error).await),
        }
    }

    async fn revertir_tras_error(&self, ingest_id: &IngestId, error: DomainError) -> DomainError {
        match self.store.revertir_ingesta(ingest_id).await {
            Ok(_) => error,
            Err(fallo_rollback) => DomainError::IngestionError(format!(
                "{error}; además falló el rollback de '{ingest_id}': {fallo_rollback}"
            )),
        }
    }

    /// Rellena `vector`, `vector_hito` y `vector_tabla` con una sola llamada al motor
    async fn vectorizar(&self, revision: RevisionMapeada) -> Result<FilasRevision, DomainError> {
        let RevisionMapeada {
            catalogo,
            mut chunks,
            mut hitos,
            mut tablas,
            celdas,
            aristas,
            textos_chunks,
            textos_hitos,
            textos_tablas,
        } = revision;
        let textos: Vec<String> = textos_chunks
            .into_iter()
            .chain(textos_hitos)
            .chain(textos_tablas)
            .collect();
        let esperados = textos.len();
        let vectores = self
            .embedding_engine
            .generate_batch_embeddings(&textos)
            .await?;
        if vectores.len() != esperados {
            return Err(DomainError::EmbeddingError(format!(
                "Se esperaban {esperados} embeddings y se obtuvieron {}",
                vectores.len()
            )));
        }
        let mut vectores = vectores.into_iter();
        for chunk in &mut chunks {
            chunk.vector = vectores.next().unwrap_or_default();
        }
        for hito in &mut hitos {
            hito.vector_hito = vectores.next();
        }
        for tabla in &mut tablas {
            tabla.vector_tabla = vectores.next();
        }

        Ok(FilasRevision {
            total_chunks: chunks.len(),
            total_tuplas: hitos.len(),
            total_tablas: tablas.len(),
            total_celdas: celdas.len(),
            total_aristas: aristas.len(),
            lotes: vec![
                LoteIngesta::Catalogo(vec![catalogo]),
                LoteIngesta::Chunks(chunks),
                LoteIngesta::Hitos(hitos),
                LoteIngesta::Tablas(tablas),
                LoteIngesta::Celdas(celdas),
                LoteIngesta::Aristas(aristas),
            ],
        })
    }
}

fn calcular_sha256(contenido: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contenido.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn reporte_ya_ingerido(
    centro_id: &CenterId,
    huella: HuellaIngerida,
    hash_sha256: String,
) -> IngestionReport {
    IngestionReport {
        doc_id: huella.doc_id.to_string(),
        centro_id: centro_id.to_string(),
        version: i32::try_from(huella.version_num).unwrap_or(i32::MAX),
        ingest_id: huella.ingest_id.to_string(),
        ya_ingerido: true,
        total_chunks: 0,
        total_tuplas: 0,
        total_tablas: 0,
        total_celdas: 0,
        total_aristas: 0,
        versiones_derogadas: Vec::new(),
        sha256: hash_sha256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
    use crate::infrastructure::llm::mock_distiller::MockDistiller;
    use crate::infrastructure::parsers::markdown_parser::MarkdownParser;
    use crate::infrastructure::storage::lancedb_store::LanceDbStore;
    use crate::infrastructure::storage::schemas::DIMENSION_EMBEDDINGS;

    #[tokio::test]
    async fn test_ingestion_service_pipeline_completo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();
        store.inicializar_tablas().await.unwrap();

        let parser = Arc::new(MarkdownParser::new());
        let distiller = Arc::new(MockDistiller::new());
        let embedding_engine = Arc::new(MockEmbeddingEngine::new(Some(
            DIMENSION_EMBEDDINGS as usize,
        )));

        let service =
            DocumentIngestionService::new(parser, distiller, embedding_engine, Arc::new(store));

        let centro = CenterId::new("planta-central").unwrap();
        let markdown = r#"
# Manual de Mantenimiento Mayor

## 1. Alcance Técnico
Este manual cubre los procedimientos para inspección de válvulas de alivio.

## 2. Inspección
Verificar presión de apertura con manómetro patrón calibrado.

**Tabla de Presiones de Disparo**
| Diámetro (pulg) | Presión Disparo (psi) | Clase Brida |
| --- | --- | --- |
| 1.0 - 2.0 | 150 psi | Clase 300 |
| 2.1 - 4.0 | 300 psi | Clase 600 |
*Nota: Prueba hidrostática anual obligatoria.*

Cumple con lineamientos de NOM-STPS-001.
"#;

        let reporte = service
            .ingestar_documento(&centro, "manual_valvulas.md", markdown)
            .await
            .expect("Ingesta exitosa");

        assert_eq!(reporte.centro_id, "planta-central");
        assert_eq!(reporte.doc_id, "MANUAL-VALVULAS");
        assert!(reporte.total_chunks >= 2);
        assert!(reporte.total_tuplas >= 1);
        assert_eq!(reporte.total_tablas, 1);
        assert_eq!(reporte.total_celdas, 2);
        assert_eq!(reporte.total_aristas, 1);
        assert!(!reporte.sha256.is_empty());
    }

    #[tokio::test]
    async fn test_observador_recibe_las_etapas_en_orden_y_nada_si_ya_estaba_ingerido() {
        use crate::application::dtos::ingestion::EtapaIngesta;
        use std::sync::Mutex as StdMutex;

        let temp_dir = tempfile::tempdir().unwrap();
        let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();
        store.inicializar_tablas().await.unwrap();
        let etapas: Arc<StdMutex<Vec<EtapaIngesta>>> = Arc::default();
        let destino = Arc::clone(&etapas);
        let service = DocumentIngestionService::new(
            Arc::new(MarkdownParser::new()),
            Arc::new(MockDistiller::new()),
            Arc::new(MockEmbeddingEngine::new(Some(
                DIMENSION_EMBEDDINGS as usize,
            ))),
            Arc::new(store),
        )
        .con_observador(Some(Arc::new(move |etapa| {
            destino.lock().unwrap().push(etapa)
        })));

        let centro = CenterId::new("planta-central").unwrap();
        let markdown = "# Manual

## 1. Alcance
Texto del alcance.
";
        service
            .ingestar_documento(&centro, "manual.md", markdown)
            .await
            .unwrap();
        assert_eq!(
            *etapas.lock().unwrap(),
            vec![
                EtapaIngesta::Parseo,
                EtapaIngesta::Destilacion,
                EtapaIngesta::Embeddings,
                EtapaIngesta::Escritura
            ]
        );

        etapas.lock().unwrap().clear();
        let repetida = service
            .ingestar_documento(&centro, "manual.md", markdown)
            .await
            .unwrap();
        assert!(repetida.ya_ingerido);
        assert!(etapas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_ingestion_service_rechaza_contenido_vacio() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();

        let parser = Arc::new(MarkdownParser::new());
        let distiller = Arc::new(MockDistiller::new());
        let embedding_engine = Arc::new(MockEmbeddingEngine::new(Some(
            DIMENSION_EMBEDDINGS as usize,
        )));

        let service =
            DocumentIngestionService::new(parser, distiller, embedding_engine, Arc::new(store));

        let centro = CenterId::new("planta-central").unwrap();
        let err = service
            .ingestar_documento(&centro, "vacio.md", "   \n\t  ")
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::IngestionError(_)));
    }
}
