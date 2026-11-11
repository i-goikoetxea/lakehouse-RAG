//! Caso de uso: ingesta documental universal y transaccional hacia el Lakehouse.
//!
//! Flujo (`docs/arquitectura/decisiones_stair.md`): hash SHA-256 → idempotencia (`existe_hash`) →
//! parseo → destilación → mapeo → vectores → `escribir_staged` → `confirmar_ingesta`.
//! - El `doc_id` canónico depende de Claude, así que se destila antes de escribir y todo va en una
//!   sola transacción.
//! - Si la destilación o su mapeo fallan, el documento se indexa igual en 02 y 07 con `doc_id`
//!   provisional y `PARCIAL_UNIVERSAL`, y el informe lo avisa. Solo revierten los fallos de
//!   embeddings o de disco.
//! - `reintentar_destilacion` sustituye la revisión provisional por la definitiva reutilizando
//!   sus vectores; si falla, la revisión provisional queda intacta.

use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::{Array, FixedSizeListArray, Float32Array, RecordBatch, StringArray};
use sha2::{Digest, Sha256};

use crate::application::dtos::ingestion::{EtapaIngesta, ObservadorIngesta, ParsedDocument};
use crate::application::dtos::lakehouse_filas::{ClaveDocumental, LoteIngesta};
use crate::application::ports::document_parser::DocumentParserPort;
use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::lakehouse_store::{
    ConsultaTabla, HuellaIngerida, LIMITE_MAXIMO_LECTURA, LakehouseStorePort,
    TABLA_CHUNKS_NORMATIVA, TABLA_INDICE_JERARQUICO,
};
use crate::application::ports::llm_distiller::LlmDistillerPort;
use crate::application::use_cases::mapeo_destilacion::{
    RevisionMapeada, clave_documental, componer_texto_nodo, componer_texto_vectorizable,
    mapear_revision, mapear_revision_provisional,
};
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::document::DocId;
use crate::domain::models::estructura_documental::EstadoContenido;
use crate::domain::models::identificadores::IngestId;

/// Vectores ya calculados, por el texto exacto del que salieron
type VectoresPrevios = HashMap<String, Vec<f32>>;

/// Reporte consolidado de la ingesta documental
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionReport {
    pub doc_id: String,
    pub centro_id: String,
    pub version: i32,
    pub ingest_id: String,
    /// `true` si el hash ya estaba confirmado: no se llamó al destilador ni se escribió nada
    pub ya_ingerido: bool,
    /// `COMPLETO`, `PARCIAL_UNIVERSAL` o `REQUIERE_OCR`
    pub estado_contenido: String,
    /// `true` si el documento sigue provisional y admite `reintentar_destilacion`
    pub pendiente_destilacion: bool,
    /// Incidencias que el operador debe ver aunque la ingesta se haya confirmado
    pub avisos: Vec<String>,
    pub total_chunks: usize,
    pub total_nodos: usize,
    pub total_tuplas: usize,
    /// Hitos sin rol ni registro: no entran en la matriz y su texto sigue en la Capa 0
    pub hitos_derivados_capa0: usize,
    pub total_tablas: usize,
    pub total_celdas: usize,
    pub total_aristas: usize,
    pub versiones_derogadas: Vec<u32>,
    pub sha256: String,
}

/// Filas de una revisión, listas para escribir como STAGED
struct FilasRevision {
    lotes: Vec<LoteIngesta>,
    hitos_derivados_capa0: usize,
    total_chunks: usize,
    total_nodos: usize,
    total_tuplas: usize,
    total_tablas: usize,
    total_celdas: usize,
    total_aristas: usize,
    aviso_truncado: Option<String>,
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

    /// Ejecuta el pipeline completo; idempotente por hash y atómico por `ingest_id`.
    /// Un fallo de Claude no es un error: el documento queda indexado como provisional.
    pub async fn ingestar_documento(
        &self,
        centro_id: &CenterId,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<IngestionReport, DomainError> {
        self.ingestar_bytes(centro_id, nombre_archivo, contenido.as_bytes())
            .await
    }

    /// `ingestar_documento` sobre los bytes del original; el hash se calcula sobre ellos
    pub async fn ingestar_bytes(
        &self,
        centro_id: &CenterId,
        nombre_archivo: &str,
        contenido: &[u8],
    ) -> Result<IngestionReport, DomainError> {
        if contenido_en_blanco(contenido) {
            return Err(DomainError::IngestionError(
                "No se puede ingestar un documento con contenido vacío".to_string(),
            ));
        }

        let hash_sha256 = calcular_sha256(contenido);
        if let Some(huella) = self.store.existe_hash(centro_id, &hash_sha256).await? {
            let requiere_ocr = self.revision_requiere_ocr(centro_id, &huella).await?;
            return Ok(reporte_ya_ingerido(
                centro_id,
                huella,
                hash_sha256,
                requiere_ocr,
            ));
        }

        self.notificar(EtapaIngesta::Parseo);
        let parsed = self.parser.parse_bytes(nombre_archivo, contenido).await?;
        self.notificar(EtapaIngesta::Destilacion);
        let destilado = if parsed.requiere_ocr {
            Err(DomainError::IngestionError(
                "el PDF no tiene capa de texto".to_string(),
            ))
        } else {
            self.destilar_y_mapear(centro_id, &parsed, &hash_sha256)
                .await
        };
        let (clave, revision, estado, mut avisos) = match destilado {
            Ok((clave, revision)) => (clave, revision, estado_destilado(&parsed), Vec::new()),
            Err(fallo) => {
                let (clave, revision) =
                    mapear_revision_provisional(centro_id, &parsed, &hash_sha256)?;
                if parsed.requiere_ocr {
                    let aviso = aviso_requiere_ocr(&clave.doc_id);
                    (clave, revision, EstadoContenido::RequiereOcr, vec![aviso])
                } else {
                    let aviso = aviso_provisional(&clave.doc_id, &fallo);
                    (
                        clave,
                        revision,
                        EstadoContenido::ParcialUniversal,
                        vec![aviso],
                    )
                }
            }
        };
        avisos.extend(parsed.avisos_extraccion.iter().cloned());
        avisos.extend(revision.avisos.iter().cloned());
        self.notificar(EtapaIngesta::Embeddings);
        let filas = self.vectorizar(revision, &VectoresPrevios::new()).await?;
        avisos.extend(filas.aviso_truncado.clone());
        self.notificar(EtapaIngesta::Escritura);

        let ingest_id = nuevo_ingest_id()?;
        let lotes = filas.lotes.clone();
        let versiones_derogadas = self.escribir_y_confirmar(&ingest_id, &clave, lotes).await?;
        Ok(reporte_confirmado(
            &clave,
            &ingest_id,
            estado,
            filas,
            versiones_derogadas,
            avisos,
            hash_sha256,
        ))
    }

    /// Vuelve a destilar un documento provisional y lo sustituye en una transacción. Con código
    /// válido pasa a su `doc_id` canónico; sin él, el provisional se consolida como definitivo. Si
    /// algo falla, la revisión provisional queda intacta. Sobre un documento ya definitivo no hace
    /// nada (idempotente).
    pub async fn reintentar_destilacion(
        &self,
        centro_id: &CenterId,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<IngestionReport, DomainError> {
        self.reintentar_destilacion_bytes(centro_id, nombre_archivo, contenido.as_bytes())
            .await
    }

    /// `reintentar_destilacion` sobre los bytes del original. Un PDF sin texto no se reintenta:
    /// Claude no tendría nada que destilar, y pasarle OCR cambia los bytes y por tanto el hash.
    pub async fn reintentar_destilacion_bytes(
        &self,
        centro_id: &CenterId,
        nombre_archivo: &str,
        contenido: &[u8],
    ) -> Result<IngestionReport, DomainError> {
        let hash_sha256 = calcular_sha256(contenido);
        let huella = self
            .store
            .existe_hash(centro_id, &hash_sha256)
            .await?
            .ok_or_else(|| {
                DomainError::ValidationError(format!(
                    "'{nombre_archivo}' no está ingerido en '{centro_id}': no hay nada que reintentar"
                ))
            })?;
        let requiere_ocr = self.revision_requiere_ocr(centro_id, &huella).await?;
        if !huella.es_provisional || requiere_ocr {
            return Ok(reporte_ya_ingerido(
                centro_id,
                huella,
                hash_sha256,
                requiere_ocr,
            ));
        }

        self.notificar(EtapaIngesta::Parseo);
        let parsed = self.parser.parse_bytes(nombre_archivo, contenido).await?;
        self.notificar(EtapaIngesta::Destilacion);
        let paquete = self.distiller.distill(&parsed).await?;
        let (clave, doc_id_provisional) = match clave_documental(centro_id, &paquete.catalogo) {
            Ok(clave) if clave.doc_id != huella.doc_id => (clave, Some(huella.doc_id.clone())),
            _ => (
                ClaveDocumental {
                    centro_id: centro_id.clone(),
                    doc_id: huella.doc_id.clone(),
                    version_num: huella.version_num,
                },
                None,
            ),
        };
        let mut revision = mapear_revision(&clave, &parsed, &paquete, &hash_sha256)?;
        revision.catalogo.doc_id_provisional = doc_id_provisional;
        let avisos_mapeo = revision.avisos.clone();

        self.notificar(EtapaIngesta::Embeddings);
        let previos = self.vectores_previos(centro_id, &huella.doc_id).await?;
        let filas = self.vectorizar(revision, &previos).await?;
        self.notificar(EtapaIngesta::Escritura);

        let ingest_id = nuevo_ingest_id()?;
        let resumen = self
            .escribir_y_sustituir(&ingest_id, &clave, filas.lotes.clone(), &huella.ingest_id)
            .await?;
        let avisos = avisos_mapeo
            .into_iter()
            .chain(filas.aviso_truncado.clone())
            .chain(resumen.aviso_limpieza)
            .collect();
        Ok(reporte_confirmado(
            &clave,
            &ingest_id,
            estado_destilado(&parsed),
            filas,
            resumen.versiones_derogadas,
            avisos,
            hash_sha256,
        ))
    }

    /// Un PDF sin texto conserva la identidad provisional; solo 02 distingue su estado
    async fn revision_requiere_ocr(
        &self,
        centro_id: &CenterId,
        huella: &HuellaIngerida,
    ) -> Result<bool, DomainError> {
        if !huella.es_provisional {
            return Ok(false);
        }
        let consulta = ConsultaTabla::confirmadas(1)
            .solo_vigentes()
            .del_centro(centro_id)
            .del_documento(&huella.doc_id);
        for lote in self
            .store
            .leer_tabla(TABLA_CHUNKS_NORMATIVA, &consulta)
            .await?
        {
            let estados = columna_texto(&lote, "estado_contenido")?;
            if (0..lote.num_rows())
                .any(|fila| estados.value(fila) == EstadoContenido::RequiereOcr.as_str())
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Etapa 2: cualquier fallo aquí (Claude, JSON, código o Capa 2 inválidos) lleva al provisional
    async fn destilar_y_mapear(
        &self,
        centro_id: &CenterId,
        parsed: &ParsedDocument,
        hash_sha256: &str,
    ) -> Result<(ClaveDocumental, RevisionMapeada), DomainError> {
        let paquete = self.distiller.distill(parsed).await?;
        let clave = clave_documental(centro_id, &paquete.catalogo)?;
        let revision = mapear_revision(&clave, parsed, &paquete, hash_sha256)?;
        Ok((clave, revision))
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

    async fn escribir_y_sustituir(
        &self,
        ingest_id: &IngestId,
        clave: &ClaveDocumental,
        lotes: Vec<LoteIngesta>,
        ingest_sustituida: &IngestId,
    ) -> Result<crate::application::ports::lakehouse_store::ResumenConfirmacion, DomainError> {
        if let Err(error) = self.store.escribir_staged(ingest_id, lotes).await {
            return Err(self.revertir_tras_error(ingest_id, error).await);
        }
        match self
            .store
            .confirmar_sustitucion(ingest_id, &clave.doc_id, ingest_sustituida)
            .await
        {
            Ok(resumen) => Ok(resumen),
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

    /// Vectores vigentes de 02 y 07 de un documento, por el texto exacto que los generó
    async fn vectores_previos(
        &self,
        centro_id: &CenterId,
        doc_id: &DocId,
    ) -> Result<VectoresPrevios, DomainError> {
        let consulta = ConsultaTabla::confirmadas(LIMITE_MAXIMO_LECTURA)
            .solo_vigentes()
            .del_centro(centro_id)
            .del_documento(doc_id);
        let mut previos = VectoresPrevios::new();
        for lote in self
            .store
            .leer_tabla(TABLA_CHUNKS_NORMATIVA, &consulta)
            .await?
        {
            let breadcrumbs = columna_texto(&lote, "breadcrumb_jerarquia")?;
            let condiciones = columna_texto(&lote, "condicion_aplicacion")?;
            let textos = columna_texto(&lote, "texto_contenido")?;
            let vectores = columna_vectores(&lote, "vector")?;
            for fila in 0..lote.num_rows() {
                let condicion = condiciones.is_valid(fila).then(|| condiciones.value(fila));
                let texto = componer_texto_vectorizable(
                    breadcrumbs.value(fila),
                    condicion,
                    textos.value(fila),
                );
                previos.insert(texto, vector_de_fila(vectores, fila)?);
            }
        }
        for lote in self
            .store
            .leer_tabla(TABLA_INDICE_JERARQUICO, &consulta)
            .await?
        {
            let titulos = columna_texto(&lote, "titulo_seccion")?;
            let resumenes = columna_texto(&lote, "resumen_seccion")?;
            let vectores = columna_vectores(&lote, "vector_seccion")?;
            for fila in 0..lote.num_rows() {
                let texto = componer_texto_nodo(titulos.value(fila), resumenes.value(fila));
                previos.insert(texto, vector_de_fila(vectores, fila)?);
            }
        }
        Ok(previos)
    }

    /// Rellena los vectores de chunks, nodos, hitos y tablas con una sola llamada al motor; los
    /// textos presentes en `previos` reutilizan su vector
    async fn vectorizar(
        &self,
        revision: RevisionMapeada,
        previos: &VectoresPrevios,
    ) -> Result<FilasRevision, DomainError> {
        let RevisionMapeada {
            avisos: _,
            hitos_derivados_capa0,
            catalogo,
            mut chunks,
            mut hitos,
            mut tablas,
            celdas,
            aristas,
            mut nodos,
            textos_nodos,
            textos_chunks,
            textos_hitos,
            textos_tablas,
        } = revision;
        let textos: Vec<String> = textos_chunks
            .into_iter()
            .chain(textos_nodos)
            .chain(textos_hitos)
            .chain(textos_tablas)
            .collect();
        let pendientes: Vec<String> = textos
            .iter()
            .filter(|texto| !previos.contains_key(*texto))
            .cloned()
            .collect();
        let vectores = self
            .embedding_engine
            .generate_batch_embeddings(&pendientes)
            .await?;
        if vectores.len() != pendientes.len() {
            return Err(DomainError::EmbeddingError(format!(
                "Se esperaban {} embeddings y se obtuvieron {}",
                pendientes.len(),
                vectores.len()
            )));
        }
        let aviso_truncado = self.aviso_truncado(&pendientes).await;

        let mut nuevos = vectores.into_iter();
        let mut vectores = textos.iter().map(|texto| {
            previos
                .get(texto)
                .cloned()
                .or_else(|| nuevos.next())
                .unwrap_or_default()
        });
        for chunk in &mut chunks {
            chunk.vector = vectores.next().unwrap_or_default();
        }
        for nodo in &mut nodos {
            nodo.vector_seccion = vectores.next().unwrap_or_default();
        }
        for hito in &mut hitos {
            hito.vector_hito = vectores.next();
        }
        for tabla in &mut tablas {
            tabla.vector_tabla = vectores.next();
        }

        Ok(FilasRevision {
            total_chunks: chunks.len(),
            total_nodos: nodos.len(),
            total_tuplas: hitos.len(),
            hitos_derivados_capa0,
            total_tablas: tablas.len(),
            total_celdas: celdas.len(),
            total_aristas: aristas.len(),
            aviso_truncado,
            lotes: vec![
                LoteIngesta::Catalogo(vec![catalogo]),
                LoteIngesta::Chunks(chunks),
                LoteIngesta::Indice(nodos),
                LoteIngesta::Hitos(hitos),
                LoteIngesta::Tablas(tablas),
                LoteIngesta::Celdas(celdas),
                LoteIngesta::Aristas(aristas),
            ],
        })
    }

    /// El modelo recorta en silencio lo que excede su límite; aquí deja de ser silencioso. El
    /// texto íntegro sigue en 02, solo el vector representa menos. La comprobación es informativa
    /// y su fallo también se avisa, sin impedir la ingesta.
    async fn aviso_truncado(&self, textos: &[String]) -> Option<String> {
        match self.embedding_engine.contar_truncados(textos).await {
            Ok(0) => None,
            Ok(truncados) => Some(format!(
                "{truncados} de {} textos superan el límite del modelo de embeddings: su vector \
                 solo representa el inicio, aunque el texto íntegro sigue en el corpus",
                textos.len()
            )),
            Err(error) => Some(format!(
                "No se pudo comprobar el truncado de los embeddings: {error}"
            )),
        }
    }
}

/// Vacío o solo espacios; unos bytes que no son UTF-8 no están en blanco, los valida el parser
fn contenido_en_blanco(contenido: &[u8]) -> bool {
    std::str::from_utf8(contenido).is_ok_and(|texto| texto.trim().is_empty())
}

fn calcular_sha256(contenido: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contenido);
    format!("{:x}", hasher.finalize())
}

fn nuevo_ingest_id() -> Result<IngestId, DomainError> {
    IngestId::new(format!("ING-{}", uuid::Uuid::new_v4().simple()))
}

fn aviso_provisional(doc_id: &DocId, fallo: &DomainError) -> String {
    format!(
        "Destilación no disponible ({fallo}). El texto quedó indexado con el identificador \
         provisional '{doc_id}' y sin hitos ni tablas estructuradas; reinténtalo cuando Claude esté \
         disponible."
    )
}

/// Un documento destilado deja de ser COMPLETO si tiene 1 o más páginas degradadas
/// (`ssd_generalidad_4_piezas.md` §1.4)
fn estado_destilado(parsed: &ParsedDocument) -> EstadoContenido {
    if parsed.paginas_degradadas.is_empty() {
        EstadoContenido::Completo
    } else {
        EstadoContenido::LecturaDegradada
    }
}

fn aviso_requiere_ocr(doc_id: &DocId) -> String {
    format!(
        "PDF escaneado o sin capa de texto ('{doc_id}'): no se ha destilado y solo es localizable \
         por su nombre o por el poco texto que contiene. Pásalo por un OCR e ingiere el PDF \
         resultante."
    )
}

fn columna_texto<'a>(lote: &'a RecordBatch, nombre: &str) -> Result<&'a StringArray, DomainError> {
    lote.column_by_name(nombre)
        .and_then(|c| c.as_any().downcast_ref())
        .ok_or_else(|| DomainError::StorageError(format!("Columna '{nombre}' ausente")))
}

fn columna_vectores<'a>(
    lote: &'a RecordBatch,
    nombre: &str,
) -> Result<&'a FixedSizeListArray, DomainError> {
    lote.column_by_name(nombre)
        .and_then(|c| c.as_any().downcast_ref())
        .ok_or_else(|| DomainError::StorageError(format!("Columna '{nombre}' ausente")))
}

fn vector_de_fila(vectores: &FixedSizeListArray, fila: usize) -> Result<Vec<f32>, DomainError> {
    let valores = vectores.value(fila);
    let flotantes = valores
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or_else(|| DomainError::StorageError("Vector con tipo inesperado".to_string()))?;
    Ok(flotantes.values().to_vec())
}

fn reporte_confirmado(
    clave: &ClaveDocumental,
    ingest_id: &IngestId,
    estado: EstadoContenido,
    filas: FilasRevision,
    versiones_derogadas: Vec<u32>,
    avisos: Vec<String>,
    hash_sha256: String,
) -> IngestionReport {
    IngestionReport {
        doc_id: clave.doc_id.to_string(),
        centro_id: clave.centro_id.to_string(),
        version: i32::try_from(clave.version_num).unwrap_or(i32::MAX),
        ingest_id: ingest_id.to_string(),
        ya_ingerido: false,
        estado_contenido: estado.as_str().to_string(),
        pendiente_destilacion: estado == EstadoContenido::ParcialUniversal,
        avisos,
        total_chunks: filas.total_chunks,
        total_nodos: filas.total_nodos,
        total_tuplas: filas.total_tuplas,
        hitos_derivados_capa0: filas.hitos_derivados_capa0,
        total_tablas: filas.total_tablas,
        total_celdas: filas.total_celdas,
        total_aristas: filas.total_aristas,
        versiones_derogadas,
        sha256: hash_sha256,
    }
}

fn reporte_ya_ingerido(
    centro_id: &CenterId,
    huella: HuellaIngerida,
    hash_sha256: String,
    requiere_ocr: bool,
) -> IngestionReport {
    let (estado, avisos) = if requiere_ocr {
        (
            EstadoContenido::RequiereOcr,
            vec![aviso_requiere_ocr(&huella.doc_id)],
        )
    } else if huella.es_provisional {
        (
            EstadoContenido::ParcialUniversal,
            vec![format!(
                "'{}' sigue provisional, sin destilar: reintenta la destilación en lugar de reingerirlo",
                huella.doc_id
            )],
        )
    } else {
        (EstadoContenido::Completo, Vec::new())
    };
    IngestionReport {
        doc_id: huella.doc_id.to_string(),
        centro_id: centro_id.to_string(),
        version: i32::try_from(huella.version_num).unwrap_or(i32::MAX),
        ingest_id: huella.ingest_id.to_string(),
        ya_ingerido: true,
        estado_contenido: estado.as_str().to_string(),
        pendiente_destilacion: huella.es_provisional && !requiere_ocr,
        avisos,
        total_chunks: 0,
        total_nodos: 0,
        total_tuplas: 0,
        hitos_derivados_capa0: 0,
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
