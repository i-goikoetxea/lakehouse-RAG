//! Caso de uso: Orquestador atómico de Ingesta Documental y Destilación hacia Lakehouse

use std::sync::Arc;

use arrow_array::{
    BooleanArray, FixedSizeListArray, Float32Array, Float64Array, Int32Array, RecordBatch,
    StringArray,
};
use arrow_schema::{DataType, Field};
use sha2::{Digest, Sha256};

use crate::application::dtos::ingestion::{ChunkToIngest, ParsedDocument};
use crate::application::ports::document_parser::DocumentParserPort;
use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::lakehouse_store::{
    LakehouseStorePort, TABLA_DOC_CATALOG, TABLA_DOC_CHUNKS, TABLA_DOUBLE_ENTRY_CELLS,
    TABLA_DOUBLE_ENTRY_TABLES, TABLA_OPERATIONAL_MATRIX, TABLA_REFERENCE_GRAPH,
};
use crate::application::ports::llm_distiller::LlmDistillerPort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::document::DocId;
use crate::infrastructure::storage::schemas::{
    DIMENSION_EMBEDDINGS, schema_doc_catalog, schema_doc_chunks, schema_double_entry_cells,
    schema_double_entry_tables, schema_operational_matrix, schema_reference_graph,
};

/// Reporte consolidado de la ingesta documental atómica
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionReport {
    pub doc_id: String,
    pub centro_id: String,
    pub version: i32,
    pub total_chunks: usize,
    pub total_tuplas: usize,
    pub total_tablas: usize,
    pub total_celdas: usize,
    pub total_aristas: usize,
    pub sha256: String,
}

/// Orquestador atómico del pipeline de ingesta
pub struct DocumentIngestionService {
    parser: Arc<dyn DocumentParserPort>,
    distiller: Arc<dyn LlmDistillerPort>,
    embedding_engine: Arc<dyn EmbeddingEnginePort>,
    store: Arc<dyn LakehouseStorePort>,
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
        }
    }

    /// Ejecuta el pipeline completo de ingesta atómica
    pub async fn ingestar_documento(
        &self,
        centro_id: &CenterId,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<IngestionReport, DomainError> {
        let contenido_limpio = contenido.trim();
        if contenido_limpio.is_empty() {
            return Err(DomainError::IngestionError(
                "No se puede ingestar un documento con contenido vacío".to_string(),
            ));
        }

        // 1. Parseo estructural
        let parsed = self.parser.parse(nombre_archivo, contenido).await?;

        // 2. Destilación LLM estructurada
        let paquete = self.distiller.distill(&parsed).await?;

        // 3. Validación de identificador de documento
        let doc_id = DocId::new(&paquete.catalogo.doc_id)?;

        // 4. Retirar vigencia de versiones previas en LanceDB
        self.store.retirar_vigencia_documento(&doc_id).await?;

        // 5. Calcular Hash SHA-256
        let mut hasher = Sha256::new();
        hasher.update(contenido.as_bytes());
        let hash_sha256 = format!("{:x}", hasher.finalize());

        // 6. Inserción Lote 01: Catálogo
        let batch_catalog =
            self.crear_batch_catalogo(centro_id, &paquete.catalogo, &hash_sha256)?;
        self.store
            .insertar_lote(TABLA_DOC_CATALOG, batch_catalog)
            .await?;

        // 7. Generación de Chunks y Embeddings (Lote 02)
        let chunks = self.extraer_chunks(&doc_id, &parsed);
        let total_chunks = chunks.len();
        if !chunks.is_empty() {
            let batch_chunks = self.crear_batch_chunks(centro_id, &chunks).await?;
            self.store
                .insertar_lote(TABLA_DOC_CHUNKS, batch_chunks)
                .await?;
        }

        // 8. Inserción Lote 03: Matriz Operativa
        let total_tuplas = paquete.tuplas_operativas.len();
        if total_tuplas > 0 {
            let batch_matriz =
                self.crear_batch_matriz(centro_id, &doc_id, &paquete.tuplas_operativas)?;
            self.store
                .insertar_lote(TABLA_OPERATIONAL_MATRIX, batch_matriz)
                .await?;
        }

        // 9. Inserción Lotes 04 y 05: Tablas de Doble Entrada y Celdas
        let total_tablas = paquete.tablas_doble_entrada.len();
        let mut total_celdas = 0;
        if total_tablas > 0 {
            let (batch_tablas, batch_celdas) =
                self.crear_batches_tablas(centro_id, &doc_id, &paquete.tablas_doble_entrada)?;
            self.store
                .insertar_lote(TABLA_DOUBLE_ENTRY_TABLES, batch_tablas)
                .await?;

            total_celdas = batch_celdas.num_rows();
            if total_celdas > 0 {
                self.store
                    .insertar_lote(TABLA_DOUBLE_ENTRY_CELLS, batch_celdas)
                    .await?;
            }
        }

        // 10. Inserción Lote 06: Grafo de Referencias
        let total_aristas = paquete.aristas_referencia.len();
        if total_aristas > 0 {
            let batch_grafo =
                self.crear_batch_grafo(centro_id, &doc_id, &paquete.aristas_referencia)?;
            self.store
                .insertar_lote(TABLA_REFERENCE_GRAPH, batch_grafo)
                .await?;
        }

        Ok(IngestionReport {
            doc_id: doc_id.to_string(),
            centro_id: centro_id.to_string(),
            version: paquete.catalogo.version,
            total_chunks,
            total_tuplas,
            total_tablas,
            total_celdas,
            total_aristas,
            sha256: hash_sha256,
        })
    }

    fn extraer_chunks(&self, doc_id: &DocId, parsed: &ParsedDocument) -> Vec<ChunkToIngest> {
        let mut chunks = Vec::new();

        for (idx, sec) in parsed.secciones.iter().enumerate() {
            let contenido = sec.contenido.trim();
            if !contenido.is_empty() {
                chunks.push(ChunkToIngest {
                    chunk_id: format!("{}-CHK-{idx}", doc_id.as_str()),
                    doc_id: doc_id.to_string(),
                    seccion: format!("{} {}", sec.codigo, sec.titulo).trim().to_string(),
                    contenido: contenido.to_string(),
                });
            }
        }

        if chunks.is_empty() {
            chunks.push(ChunkToIngest {
                chunk_id: format!("{}-CHK-0", doc_id.as_str()),
                doc_id: doc_id.to_string(),
                seccion: parsed.titulo.clone(),
                contenido: parsed.contenido_completo.clone(),
            });
        }

        chunks
    }

    fn crear_batch_catalogo(
        &self,
        centro_id: &CenterId,
        cat: &crate::application::dtos::ingestion::DistilledCatalog,
        hash_sha256: &str,
    ) -> Result<RecordBatch, DomainError> {
        let schema = schema_doc_catalog();
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(vec![cat.doc_id.as_str()])),
                Arc::new(StringArray::from(vec![cat.titulo.as_str()])),
                Arc::new(StringArray::from(vec![cat.tipo_documental.as_str()])),
                Arc::new(StringArray::from(vec![cat.emisor.as_str()])),
                Arc::new(StringArray::from(vec![cat.fecha_emision.as_str()])),
                Arc::new(Int32Array::from(vec![cat.nivel_prelacion])),
                Arc::new(StringArray::from(vec![centro_id.as_str()])),
                Arc::new(BooleanArray::from(vec![true])),
                Arc::new(Int32Array::from(vec![cat.version])),
                Arc::new(StringArray::from(vec![hash_sha256])),
            ],
        )
        .map_err(|e| DomainError::StorageError(format!("Error al armar RecordBatch catálogo: {e}")))
    }

    async fn crear_batch_chunks(
        &self,
        centro_id: &CenterId,
        chunks: &[ChunkToIngest],
    ) -> Result<RecordBatch, DomainError> {
        let textos: Vec<String> = chunks.iter().map(|c| c.contenido.clone()).collect();
        let vectores: Vec<Vec<f32>> = self
            .embedding_engine
            .generate_batch_embeddings(&textos)
            .await?;

        let dim = DIMENSION_EMBEDDINGS as usize;
        let mut floats_aplanados = Vec::with_capacity(chunks.len() * dim);
        for v in &vectores {
            if v.len() != dim {
                return Err(DomainError::StorageError(format!(
                    "Dimensión de embedding incompatible: esperada {dim}, obtenida {}",
                    v.len()
                )));
            }
            floats_aplanados.extend_from_slice(v);
        }

        let valores_vector = Arc::new(Float32Array::from(floats_aplanados));
        let item_field = Arc::new(Field::new("item", DataType::Float32, true));
        let array_vector = Arc::new(FixedSizeListArray::new(
            item_field,
            DIMENSION_EMBEDDINGS,
            valores_vector,
            None,
        ));

        let chunk_ids: Vec<&str> = chunks.iter().map(|c| c.chunk_id.as_str()).collect();
        let doc_ids: Vec<&str> = chunks.iter().map(|c| c.doc_id.as_str()).collect();
        let centros: Vec<&str> = vec![centro_id.as_str(); chunks.len()];
        let secciones: Vec<&str> = chunks.iter().map(|c| c.seccion.as_str()).collect();
        let contenidos: Vec<&str> = chunks.iter().map(|c| c.contenido.as_str()).collect();
        let vigentes: Vec<bool> = vec![true; chunks.len()];

        let schema = schema_doc_chunks();
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(chunk_ids)),
                Arc::new(StringArray::from(doc_ids)),
                Arc::new(StringArray::from(centros)),
                Arc::new(StringArray::from(secciones)),
                Arc::new(StringArray::from(contenidos)),
                array_vector,
                Arc::new(BooleanArray::from(vigentes)),
            ],
        )
        .map_err(|e| DomainError::StorageError(format!("Error al armar RecordBatch chunks: {e}")))
    }

    fn crear_batch_matriz(
        &self,
        centro_id: &CenterId,
        doc_id: &DocId,
        tuplas: &[crate::application::dtos::ingestion::DistilledOperationalTuple],
    ) -> Result<RecordBatch, DomainError> {
        let count = tuplas.len();
        let mut tupla_ids = Vec::with_capacity(count);
        let mut doc_ids = Vec::with_capacity(count);
        let mut centros = Vec::with_capacity(count);
        let mut fases = Vec::with_capacity(count);
        let mut roles = Vec::with_capacity(count);
        let mut acciones = Vec::with_capacity(count);
        let mut parametros = Vec::with_capacity(count);
        let mut tolerancias = Vec::with_capacity(count);
        let mut condiciones = Vec::with_capacity(count);
        let mut referencias = Vec::with_capacity(count);
        let vigentes = vec![true; count];

        for (idx, t) in tuplas.iter().enumerate() {
            tupla_ids.push(format!("{}-TPL-{idx}", doc_id.as_str()));
            doc_ids.push(doc_id.as_str());
            centros.push(centro_id.as_str());
            fases.push(t.fase_operativa.as_str());
            roles.push(t.rol_responsable.as_str());
            acciones.push(t.accion.as_str());
            parametros.push(t.parametro_control.as_str());
            tolerancias.push(t.tolerancia_criterio.as_str());
            condiciones.push(t.condicion_aplicacion.as_str());
            referencias.push(t.referencia_normativa.as_str());
        }

        let schema = schema_operational_matrix();
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(tupla_ids)),
                Arc::new(StringArray::from(doc_ids)),
                Arc::new(StringArray::from(centros)),
                Arc::new(StringArray::from(fases)),
                Arc::new(StringArray::from(roles)),
                Arc::new(StringArray::from(acciones)),
                Arc::new(StringArray::from(parametros)),
                Arc::new(StringArray::from(tolerancias)),
                Arc::new(StringArray::from(condiciones)),
                Arc::new(StringArray::from(referencias)),
                Arc::new(BooleanArray::from(vigentes)),
            ],
        )
        .map_err(|e| DomainError::StorageError(format!("Error al armar RecordBatch matriz: {e}")))
    }

    fn crear_batches_tablas(
        &self,
        centro_id: &CenterId,
        doc_id: &DocId,
        tablas: &[crate::application::dtos::ingestion::DistilledDoubleEntryTable],
    ) -> Result<(RecordBatch, RecordBatch), DomainError> {
        let mut tabla_ids = Vec::new();
        let mut doc_ids_t = Vec::new();
        let mut centros_t = Vec::new();
        let mut nombres = Vec::new();
        let mut coords_x = Vec::new();
        let mut coords_y = Vec::new();
        let mut notas_t = Vec::new();
        let mut vigentes_t = Vec::new();

        let mut celda_ids = Vec::new();
        let mut celda_tabla_ids = Vec::new();
        let mut celda_doc_ids = Vec::new();
        let mut celda_centros = Vec::new();
        let mut rx_min = Vec::new();
        let mut rx_max = Vec::new();
        let mut x_inc_min = Vec::new();
        let mut x_inc_max = Vec::new();
        let mut cat_x = Vec::new();
        let mut ry_min = Vec::new();
        let mut ry_max = Vec::new();
        let mut y_inc_min = Vec::new();
        let mut y_inc_max = Vec::new();
        let mut cat_y = Vec::new();
        let mut val_res = Vec::new();
        let mut unidades = Vec::new();
        let mut notas_c = Vec::new();
        let mut vigentes_c = Vec::new();

        for (t_idx, t) in tablas.iter().enumerate() {
            let t_id = format!("{}-TBL-{t_idx}", doc_id.as_str());
            tabla_ids.push(t_id.clone());
            doc_ids_t.push(doc_id.as_str());
            centros_t.push(centro_id.as_str());
            nombres.push(t.nombre_tabla.as_str());
            coords_x.push(t.coordenada_x_nombre.as_str());
            coords_y.push(t.coordenada_y_nombre.as_str());
            notas_t.push(t.nota_pie.as_deref());
            vigentes_t.push(true);

            for (c_idx, c) in t.celdas.iter().enumerate() {
                celda_ids.push(format!("{t_id}-CLD-{c_idx}"));
                celda_tabla_ids.push(t_id.clone());
                celda_doc_ids.push(doc_id.as_str());
                celda_centros.push(centro_id.as_str());

                rx_min.push(c.rango_x_min);
                rx_max.push(c.rango_x_max);
                x_inc_min.push(c.x_inclusivo_min);
                x_inc_max.push(c.x_inclusivo_max);
                cat_x.push(c.categoria_x.as_deref());

                ry_min.push(c.rango_y_min);
                ry_max.push(c.rango_y_max);
                y_inc_min.push(c.y_inclusivo_min);
                y_inc_max.push(c.y_inclusivo_max);
                cat_y.push(c.categoria_y.as_deref());

                val_res.push(c.valor_resultado.as_str());
                unidades.push(c.unidad_medida.as_str());
                notas_c.push(c.notas_especificas.as_deref());
                vigentes_c.push(true);
            }
        }

        let schema_t = schema_double_entry_tables();
        let batch_t = RecordBatch::try_new(
            schema_t,
            vec![
                Arc::new(StringArray::from(tabla_ids)),
                Arc::new(StringArray::from(doc_ids_t)),
                Arc::new(StringArray::from(centros_t)),
                Arc::new(StringArray::from(nombres)),
                Arc::new(StringArray::from(coords_x)),
                Arc::new(StringArray::from(coords_y)),
                Arc::new(StringArray::from(notas_t)),
                Arc::new(BooleanArray::from(vigentes_t)),
            ],
        )
        .map_err(|e| DomainError::StorageError(format!("Error en batch tablas: {e}")))?;

        let schema_c = schema_double_entry_cells();
        let batch_c = RecordBatch::try_new(
            schema_c,
            vec![
                Arc::new(StringArray::from(celda_ids)),
                Arc::new(StringArray::from(celda_tabla_ids)),
                Arc::new(StringArray::from(celda_doc_ids)),
                Arc::new(StringArray::from(celda_centros)),
                Arc::new(Float64Array::from(rx_min)),
                Arc::new(Float64Array::from(rx_max)),
                Arc::new(BooleanArray::from(x_inc_min)),
                Arc::new(BooleanArray::from(x_inc_max)),
                Arc::new(StringArray::from(cat_x)),
                Arc::new(Float64Array::from(ry_min)),
                Arc::new(Float64Array::from(ry_max)),
                Arc::new(BooleanArray::from(y_inc_min)),
                Arc::new(BooleanArray::from(y_inc_max)),
                Arc::new(StringArray::from(cat_y)),
                Arc::new(StringArray::from(val_res)),
                Arc::new(StringArray::from(unidades)),
                Arc::new(StringArray::from(notas_c)),
                Arc::new(BooleanArray::from(vigentes_c)),
            ],
        )
        .map_err(|e| DomainError::StorageError(format!("Error en batch celdas: {e}")))?;

        Ok((batch_t, batch_c))
    }

    fn crear_batch_grafo(
        &self,
        centro_id: &CenterId,
        doc_id: &DocId,
        aristas: &[crate::application::dtos::ingestion::DistilledReferenceEdge],
    ) -> Result<RecordBatch, DomainError> {
        let count = aristas.len();
        let mut arista_ids = Vec::with_capacity(count);
        let mut centros = Vec::with_capacity(count);
        let mut origenes = Vec::with_capacity(count);
        let mut destinos = Vec::with_capacity(count);
        let mut tipos = Vec::with_capacity(count);
        let mut pesos = Vec::with_capacity(count);
        let mut citas = Vec::with_capacity(count);
        let vigentes = vec![true; count];

        for (idx, a) in aristas.iter().enumerate() {
            arista_ids.push(format!("{}-REF-{idx}", doc_id.as_str()));
            centros.push(centro_id.as_str());
            origenes.push(doc_id.as_str());
            destinos.push(a.doc_destino_id.as_str());
            tipos.push(a.tipo_relacion.as_str());
            pesos.push(a.peso_prelacion);
            citas.push(a.contexto_cita.as_deref());
        }

        let schema = schema_reference_graph();
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(arista_ids)),
                Arc::new(StringArray::from(centros)),
                Arc::new(StringArray::from(origenes)),
                Arc::new(StringArray::from(destinos)),
                Arc::new(StringArray::from(tipos)),
                Arc::new(Float32Array::from(pesos)),
                Arc::new(StringArray::from(citas)),
                Arc::new(BooleanArray::from(vigentes)),
            ],
        )
        .map_err(|e| DomainError::StorageError(format!("Error en batch grafo: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
    use crate::infrastructure::llm::mock_distiller::MockDistiller;
    use crate::infrastructure::parsers::markdown_parser::MarkdownParser;
    use crate::infrastructure::storage::lancedb_store::LanceDbStore;

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
        assert_eq!(reporte.doc_id, "MANUAL_VALVULAS");
        assert!(reporte.total_chunks >= 2);
        assert!(reporte.total_tuplas >= 1);
        assert_eq!(reporte.total_tablas, 1);
        assert_eq!(reporte.total_celdas, 2);
        assert_eq!(reporte.total_aristas, 1);
        assert!(!reporte.sha256.is_empty());
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
