//! Servicio de Búsqueda Híbrida combinando similitud vectorial, BM25 y Reciprocal Rank Fusion (RRF)

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use arrow_array::{Array, BooleanArray, Float32Array, RecordBatch, StringArray};
use async_trait::async_trait;
use futures::StreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};

use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::hybrid_search::{HybridSearchParams, HybridSearchPort, SearchHit};
use crate::application::ports::lakehouse_store::TABLA_DOC_CHUNKS;
use crate::domain::errors::DomainError;
use crate::infrastructure::storage::lancedb_store::LanceDbStore;

/// Orquestador de búsqueda híbrida en el Lakehouse
pub struct HybridSearchService {
    store: Arc<LanceDbStore>,
    embedding_engine: Arc<dyn EmbeddingEnginePort>,
}

#[derive(Debug, Clone)]
struct ChunkRecord {
    chunk_id: String,
    doc_id: String,
    centro_id: String,
    seccion: String,
    contenido: String,
    vigente: bool,
    score_vectorial: f32,
    score_bm25: f32,
}

impl HybridSearchService {
    /// Crea una nueva instancia del servicio de búsqueda híbrida
    pub fn new(store: Arc<LanceDbStore>, embedding_engine: Arc<dyn EmbeddingEnginePort>) -> Self {
        Self {
            store,
            embedding_engine,
        }
    }

    /// Tokeniza una cadena de texto extrayendo términos en minúsculas sin puntuación
    fn tokenizar(texto: &str) -> Vec<String> {
        texto
            .split_whitespace()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>()
                    .to_lowercase()
            })
            .filter(|w| !w.is_empty())
            .collect()
    }

    /// Extrae registros `ChunkRecord` desde un `RecordBatch` de LanceDB
    fn extraer_records_de_batch(
        batch: &RecordBatch,
        distancia_col: Option<&Float32Array>,
    ) -> Vec<ChunkRecord> {
        let mut records = Vec::with_capacity(batch.num_rows());

        let chunk_id_arr = batch
            .column_by_name("chunk_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let doc_id_arr = batch
            .column_by_name("doc_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let centro_id_arr = batch
            .column_by_name("centro_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let seccion_arr = batch
            .column_by_name("seccion")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let contenido_arr = batch
            .column_by_name("contenido")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let vigente_arr = batch
            .column_by_name("vigente")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>());

        if let (
            Some(chunk_ids),
            Some(doc_ids),
            Some(centro_ids),
            Some(secciones),
            Some(contenidos),
            Some(vigentes),
        ) = (
            chunk_id_arr,
            doc_id_arr,
            centro_id_arr,
            seccion_arr,
            contenido_arr,
            vigente_arr,
        ) {
            for row in 0..batch.num_rows() {
                if chunk_ids.is_null(row) {
                    continue;
                }

                let score_vec = if let Some(dist_arr) = distancia_col {
                    if dist_arr.is_null(row) {
                        0.0
                    } else {
                        let dist = dist_arr.value(row);
                        1.0 / (1.0 + dist.max(0.0))
                    }
                } else {
                    0.0
                };

                records.push(ChunkRecord {
                    chunk_id: chunk_ids.value(row).to_string(),
                    doc_id: doc_ids.value(row).to_string(),
                    centro_id: centro_ids.value(row).to_string(),
                    seccion: secciones.value(row).to_string(),
                    contenido: contenidos.value(row).to_string(),
                    vigente: vigentes.value(row),
                    score_vectorial: score_vec,
                    score_bm25: 0.0,
                });
            }
        }

        records
    }

    /// Calcula puntuaciones BM25 sobre una colección de chunks para una consulta dada
    fn calcular_bm25(query_tokens: &[String], chunks: &mut [ChunkRecord]) {
        if chunks.is_empty() || query_tokens.is_empty() {
            return;
        }

        let n_docs = chunks.len() as f32;
        let mut doc_lengths = Vec::with_capacity(chunks.len());
        let mut tokenized_docs = Vec::with_capacity(chunks.len());

        for chunk in chunks.iter() {
            let tokens = Self::tokenizar(&chunk.contenido);
            doc_lengths.push(tokens.len() as f32);
            tokenized_docs.push(tokens);
        }

        let total_length: f32 = doc_lengths.iter().sum();
        let avgdl = if total_length > 0.0 {
            total_length / n_docs
        } else {
            1.0
        };

        let k1: f32 = 1.2;
        let b: f32 = 0.75;

        // Document frequency por cada término de la consulta
        let mut df_map: HashMap<&str, usize> = HashMap::new();
        for q_tok in query_tokens {
            let mut count = 0;
            for (idx, doc_toks) in tokenized_docs.iter().enumerate() {
                let in_content = doc_toks.iter().any(|t| t == q_tok);
                let in_section = Self::tokenizar(&chunks[idx].seccion)
                    .iter()
                    .any(|t| t == q_tok);
                if in_content || in_section {
                    count += 1;
                }
            }
            df_map.insert(q_tok.as_str(), count);
        }

        for (idx, chunk) in chunks.iter_mut().enumerate() {
            let doc_len = doc_lengths[idx];
            let doc_toks = &tokenized_docs[idx];
            let seccion_toks = Self::tokenizar(&chunk.seccion);

            let mut bm25_sum = 0.0f32;

            for q_tok in query_tokens {
                let df = *df_map.get(q_tok.as_str()).unwrap_or(&0) as f32;
                if df == 0.0 {
                    continue;
                }

                // Inverted Document Frequency (IDF) Robertson-Spärck Jones
                let idf = ((n_docs - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0);

                let content_tf = doc_toks.iter().filter(|&t| t == q_tok).count() as f32;
                let section_tf = seccion_toks.iter().filter(|&t| t == q_tok).count() as f32;
                // Boost de coincidencia en el título de sección
                let tf = content_tf + (section_tf * 1.5);

                if tf > 0.0 {
                    let num = tf * (k1 + 1.0);
                    let den = tf + k1 * (1.0 - b + b * (doc_len / avgdl));
                    bm25_sum += idf * (num / den);
                }
            }

            chunk.score_bm25 = bm25_sum;
        }

        // Normalización min-max de BM25 a escala [0.0, 1.0] para balance armónico
        let max_bm25 = chunks
            .iter()
            .map(|c| c.score_bm25)
            .fold(0.0f32, |acc, x| acc.max(x));
        if max_bm25 > 1e-6 {
            for chunk in chunks.iter_mut() {
                chunk.score_bm25 /= max_bm25;
            }
        }
    }
}

#[async_trait]
impl HybridSearchPort for HybridSearchService {
    async fn hybrid_search(
        &self,
        params: &HybridSearchParams,
    ) -> Result<Vec<SearchHit>, DomainError> {
        let query_trimmed = params.query.trim();
        if query_trimmed.is_empty() {
            return Ok(Vec::new());
        }

        if params.centro_id.trim().is_empty() {
            return Err(DomainError::SearchError(
                "El centro_id no puede estar vacío para la búsqueda híbrida".to_string(),
            ));
        }

        let table = self.store.abrir_tabla(TABLA_DOC_CHUNKS).await?;

        // 1. Construir expresión de filtro SQL para LanceDB
        let mut filter_expr = format!("centro_id = '{}'", params.centro_id.trim());
        if params.solo_vigentes {
            filter_expr.push_str(" AND vigente = true");
        }

        let candidate_limit = (params.top_k * 4).max(50);

        // 2. Búsqueda Vectorial Semántica
        let query_vector = self
            .embedding_engine
            .generate_embedding(query_trimmed)
            .await?;

        let mut vector_results: Vec<ChunkRecord> = Vec::new();
        let vector_query_res = table
            .query()
            .nearest_to(query_vector)
            .map_err(|e| DomainError::SearchError(format!("Error en consulta vectorial: {e}")))?
            .only_if(&filter_expr)
            .limit(candidate_limit)
            .execute()
            .await;

        if let Ok(mut stream) = vector_query_res {
            while let Some(batch_res) = stream.next().await {
                if let Ok(batch) = batch_res {
                    let dist_arr = batch
                        .column_by_name("_distance")
                        .and_then(|c| c.as_any().downcast_ref::<Float32Array>());
                    let records = Self::extraer_records_de_batch(&batch, dist_arr);
                    vector_results.extend(records);
                }
            }
        }

        // 3. Extracción de Candidatos Léxicos (BM25)
        let mut lexical_results: Vec<ChunkRecord> = Vec::new();
        let lexical_query_res = table
            .query()
            .only_if(&filter_expr)
            .limit(candidate_limit * 2)
            .execute()
            .await;

        if let Ok(mut stream) = lexical_query_res {
            while let Some(batch_res) = stream.next().await {
                if let Ok(batch) = batch_res {
                    let records = Self::extraer_records_de_batch(&batch, None);
                    lexical_results.extend(records);
                }
            }
        }

        // 4. Calcular BM25 sobre los candidatos léxicos
        let query_tokens = Self::tokenizar(query_trimmed);
        Self::calcular_bm25(&query_tokens, &mut lexical_results);

        // 5. Unificación y Reciprocal Rank Fusion (RRF)
        // Mapear rankings vectoriales
        vector_results.sort_by(|a, b| {
            b.score_vectorial
                .partial_cmp(&a.score_vectorial)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut vector_ranks: HashMap<String, usize> = HashMap::new();
        let mut vector_scores: HashMap<String, f32> = HashMap::new();
        for (rank, rec) in vector_results.iter().enumerate() {
            vector_ranks.insert(rec.chunk_id.clone(), rank + 1);
            vector_scores.insert(rec.chunk_id.clone(), rec.score_vectorial);
        }

        // Mapear rankings BM25 (filtrando scores mayores a 0)
        lexical_results.sort_by(|a, b| {
            b.score_bm25
                .partial_cmp(&a.score_bm25)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut bm25_ranks: HashMap<String, usize> = HashMap::new();
        let mut bm25_scores: HashMap<String, f32> = HashMap::new();
        for (rank, rec) in lexical_results.iter().enumerate() {
            if rec.score_bm25 > 0.0 {
                bm25_ranks.insert(rec.chunk_id.clone(), rank + 1);
                bm25_scores.insert(rec.chunk_id.clone(), rec.score_bm25);
            }
        }

        // Combinar todos los chunk_ids únicos encontrados
        let mut todos_chunks_map: HashMap<String, ChunkRecord> = HashMap::new();
        for rec in vector_results {
            todos_chunks_map.insert(rec.chunk_id.clone(), rec);
        }
        for rec in lexical_results {
            todos_chunks_map.entry(rec.chunk_id.clone()).or_insert(rec);
        }

        let k_rrf = params.rrf_k.max(1.0);
        let default_rank = (candidate_limit * 2) + 1;

        let mut hits: Vec<SearchHit> = Vec::new();
        let mut seen_chunks: HashSet<String> = HashSet::new();

        for (chunk_id, chunk_rec) in todos_chunks_map {
            if seen_chunks.contains(&chunk_id) {
                continue;
            }
            seen_chunks.insert(chunk_id.clone());

            let rank_vec = *vector_ranks.get(&chunk_id).unwrap_or(&default_rank);
            let rank_bm25 = *bm25_ranks.get(&chunk_id).unwrap_or(&default_rank);

            let rrf_vec = params.peso_vectorial / (k_rrf + rank_vec as f32);
            let rrf_bm25 = params.peso_bm25 / (k_rrf + rank_bm25 as f32);
            let score_hibrido = rrf_vec + rrf_bm25;

            let score_vectorial = *vector_scores.get(&chunk_id).unwrap_or(&0.0);
            let score_bm25 = *bm25_scores.get(&chunk_id).unwrap_or(&0.0);

            hits.push(SearchHit {
                chunk_id,
                doc_id: chunk_rec.doc_id,
                centro_id: chunk_rec.centro_id,
                seccion: chunk_rec.seccion,
                contenido: chunk_rec.contenido,
                score_vectorial,
                score_bm25,
                score_hibrido,
                vigente: chunk_rec.vigente,
            });
        }

        // Ordenar por score_hibrido descendente
        hits.sort_by(|a, b| {
            b.score_hibrido
                .partial_cmp(&a.score_hibrido)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Aplicar límite top_k
        if hits.len() > params.top_k {
            hits.truncate(params.top_k);
        }

        Ok(hits)
    }
}
