//! Caso de uso: ingesta documental transaccional hacia el Lakehouse v2.
//!
//! Flujo (plan v2, problema 8): hash SHA-256 → idempotencia (`existe_hash`) → parseo →
//! destilación → filas tipadas con IDs versionados → `escribir_staged` → `confirmar_ingesta`.
//! Cualquier error posterior a la primera escritura dispara `revertir_ingesta`.
//!
//! El DTO de destilación sigue siendo el v1 (Fase 11 lo sustituye): las columnas v2 que el LLM
//! aún no entrega se dejan vacías o nulas (`objeto_alcance`, `rol_ejecutor`, `registro_*`,
//! `chunk_origen_id`, `vector_hito`, `vector_tabla`, `valor_salida_nombre`, vínculos de chunks)
//! y los hitos se marcan `calidad_extraccion = REVISAR`.

use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use sha2::{Digest, Sha256};

use crate::application::dtos::ingestion::{
    DistilledCatalog, DistilledCell, DistilledDocumentPackage, DistilledDoubleEntryTable,
    DistilledOperationalTuple, DistilledReferenceEdge, ParsedDocument,
};
use crate::application::dtos::lakehouse_filas::{
    ClaveDocumental, EjeTabla, FilaArista, FilaCatalogo, FilaCelda, FilaChunk, FilaHito, FilaTabla,
    LimitesEje, LoteIngesta, TipoEjeTabla, TipoNodoGrafo,
};
use crate::application::ports::document_parser::DocumentParserPort;
use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::lakehouse_store::{HuellaIngerida, LakehouseStorePort};
use crate::application::ports::llm_distiller::LlmDistillerPort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::document::{DocId, NivelPrelacion, TipoDocumento};
use crate::domain::models::identificadores::{IngestId, TipoEntidad, id_versionado};
use crate::domain::models::operational_matrix::{CalidadExtraccion, FaseOperativa};
use crate::domain::models::reference_graph::TipoRelacionNormativa;
use crate::domain::models::vigencia::VentanaVigencia;
use crate::domain::services::normalizacion::codigo_canonico;

/// Tipo asumido cuando el LLM devuelve un `tipo_documental` fuera de vocabulario (Fase 11 lo fija por enum)
const TIPO_DOCUMENTO_POR_DEFECTO: TipoDocumento = TipoDocumento::ProcedimientoGeneral;
/// Fase asignada a una tupla sin fase interpretable
const FASE_NO_ESPECIFICADA: &str = "NO_ESPECIFICADA";
/// Separador de las piezas del criterio de aceptación compuesto desde el DTO v1
const SEPARADOR_CRITERIO: &str = " | ";

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

        let parsed = self.parser.parse(nombre_archivo, contenido).await?;
        let paquete = self.distiller.distill(&parsed).await?;
        let clave = clave_documental(centro_id, &paquete.catalogo)?;
        let filas = self
            .construir_filas(&clave, &parsed, &paquete, &hash_sha256)
            .await?;

        let ingest_id = IngestId::new(format!("ING-{}", uuid::Uuid::new_v4().simple()))?;
        let versiones_derogadas = self
            .escribir_y_confirmar(&ingest_id, &clave, filas.lotes)
            .await?;

        Ok(IngestionReport {
            doc_id: clave.doc_id.to_string(),
            centro_id: centro_id.to_string(),
            version: paquete.catalogo.version,
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

    async fn construir_filas(
        &self,
        clave: &ClaveDocumental,
        parsed: &ParsedDocument,
        paquete: &DistilledDocumentPackage,
        hash_sha256: &str,
    ) -> Result<FilasRevision, DomainError> {
        let tipo_documento = interpretar_tipo_documento(&paquete.catalogo);
        let nivel = tipo_documento.nivel_prelacion();

        let catalogo = fila_catalogo(clave, &paquete.catalogo, tipo_documento, hash_sha256)?;
        let chunks = self.filas_chunks(clave, nivel, parsed).await?;
        let hitos = filas_hitos(clave, nivel, &paquete.tuplas_operativas);
        let (tablas, celdas) = filas_tablas_y_celdas(clave, nivel, &paquete.tablas_doble_entrada);
        let aristas = filas_aristas(clave, &paquete.aristas_referencia);

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

    async fn filas_chunks(
        &self,
        clave: &ClaveDocumental,
        nivel: NivelPrelacion,
        parsed: &ParsedDocument,
    ) -> Result<Vec<FilaChunk>, DomainError> {
        let secciones = secciones_con_contenido(parsed);
        let textos: Vec<String> = secciones.iter().map(|(_, texto)| texto.clone()).collect();
        let vectores = self
            .embedding_engine
            .generate_batch_embeddings(&textos)
            .await?;
        if vectores.len() != secciones.len() {
            return Err(DomainError::EmbeddingError(format!(
                "Se esperaban {} embeddings y se obtuvieron {}",
                secciones.len(),
                vectores.len()
            )));
        }

        Ok(secciones
            .into_iter()
            .zip(vectores)
            .enumerate()
            .map(|(idx, ((breadcrumb, texto), vector))| FilaChunk {
                chunk_id: id_versionado(&clave.doc_id, clave.version_num, TipoEntidad::Chunk, idx),
                clave: clave.clone(),
                nivel_prelacion: nivel,
                // Fase 11: breadcrumb completo H1 > H2 > H3 y condición heredada (H7)
                breadcrumb_jerarquia: breadcrumb,
                condicion_aplicacion: None,
                texto_contenido: texto,
                tablas_vinculadas: Vec::new(),
                hitos_vinculados: Vec::new(),
                vector,
            })
            .collect())
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

/// `DocId` estricto (rechaza comillas antes de escribir nada, H9) y versión no negativa
fn clave_documental(
    centro_id: &CenterId,
    catalogo: &DistilledCatalog,
) -> Result<ClaveDocumental, DomainError> {
    let doc_id = DocId::new(&catalogo.doc_id)?;
    let version_num = u32::try_from(catalogo.version).map_err(|_| {
        DomainError::IngestionError(format!(
            "La versión destilada debe ser ≥ 0 (recibida {})",
            catalogo.version
        ))
    })?;
    Ok(ClaveDocumental {
        centro_id: centro_id.clone(),
        doc_id,
        version_num,
    })
}

fn interpretar_tipo_documento(catalogo: &DistilledCatalog) -> TipoDocumento {
    TipoDocumento::parse(&catalogo.tipo_documental).unwrap_or(TIPO_DOCUMENTO_POR_DEFECTO)
}

/// `fecha_emision` en ISO (`AAAA-MM-DD…`); si no se interpreta, la fecha de la ingesta
fn interpretar_fecha_vigor(fecha_emision: &str) -> NaiveDate {
    let prefijo: String = fecha_emision.trim().chars().take(10).collect();
    NaiveDate::parse_from_str(&prefijo, "%Y-%m-%d").unwrap_or_else(|_| Utc::now().date_naive())
}

fn fila_catalogo(
    clave: &ClaveDocumental,
    catalogo: &DistilledCatalog,
    tipo_documento: TipoDocumento,
    hash_sha256: &str,
) -> Result<FilaCatalogo, DomainError> {
    Ok(FilaCatalogo {
        clave: clave.clone(),
        titulo: catalogo.titulo.clone(),
        tipo_documento,
        revision: format!("Rev. {:02}", clave.version_num),
        vigencia: VentanaVigencia::new(interpretar_fecha_vigor(&catalogo.fecha_emision), None)?,
        hash_sha256: hash_sha256.to_string(),
    })
}

/// Pares (encabezado, texto) de las secciones no vacías; si no hay ninguna, el documento entero
fn secciones_con_contenido(parsed: &ParsedDocument) -> Vec<(String, String)> {
    let secciones: Vec<(String, String)> = parsed
        .secciones
        .iter()
        .filter(|sec| !sec.contenido.trim().is_empty())
        .map(|sec| {
            (
                format!("{} {}", sec.codigo, sec.titulo).trim().to_string(),
                sec.contenido.trim().to_string(),
            )
        })
        .collect();
    if secciones.is_empty() {
        vec![(parsed.titulo.clone(), parsed.contenido_completo.clone())]
    } else {
        secciones
    }
}

fn interpretar_fase(fase: &str) -> FaseOperativa {
    FaseOperativa::parse(fase).unwrap_or_else(|_| FaseOperativa::Otra(FASE_NO_ESPECIFICADA.into()))
}

fn texto_opcional(texto: &str) -> Option<String> {
    let limpio = texto.trim();
    (!limpio.is_empty()).then(|| limpio.to_string())
}

/// El DTO v1 no separa criterio, parámetro ni registro: se conservan juntos para no perder datos
fn criterio_compuesto(tupla: &DistilledOperationalTuple) -> String {
    [
        tupla.accion.as_str(),
        tupla.parametro_control.as_str(),
        tupla.tolerancia_criterio.as_str(),
        tupla.referencia_normativa.as_str(),
    ]
    .into_iter()
    .map(str::trim)
    .filter(|pieza| !pieza.is_empty())
    .collect::<Vec<_>>()
    .join(SEPARADOR_CRITERIO)
}

fn filas_hitos(
    clave: &ClaveDocumental,
    nivel: NivelPrelacion,
    tuplas: &[DistilledOperationalTuple],
) -> Vec<FilaHito> {
    tuplas
        .iter()
        .enumerate()
        .map(|(idx, tupla)| FilaHito {
            hito_id: id_versionado(&clave.doc_id, clave.version_num, TipoEntidad::Hito, idx),
            clave: clave.clone(),
            orden_secuencia: (idx + 1) as u32,
            fase_operativa: interpretar_fase(&tupla.fase_operativa),
            // Fase 11: el DTO v2 entregará objeto, ejecutor y registro separados (H1)
            objeto_alcance: String::new(),
            rol_ejecutor: String::new(),
            rol_responsable: tupla.rol_responsable.trim().to_string(),
            registro_codigo: None,
            registro_nombre: None,
            criterio_aceptacion: criterio_compuesto(tupla),
            condicion_aplicacion: texto_opcional(&tupla.condicion_aplicacion),
            nivel_prelacion: nivel,
            calidad_extraccion: CalidadExtraccion::Revisar,
            chunk_origen_id: None,
            vector_hito: None,
        })
        .collect()
}

fn tipo_eje(celdas: &[DistilledCell], es_eje_x: bool) -> TipoEjeTabla {
    let tiene_rango = celdas.iter().any(|c| {
        if es_eje_x {
            c.rango_x_min.is_some() || c.rango_x_max.is_some()
        } else {
            c.rango_y_min.is_some() || c.rango_y_max.is_some()
        }
    });
    if tiene_rango {
        TipoEjeTabla::RangoNumerico
    } else {
        TipoEjeTabla::CategoriaExacta
    }
}

fn instrucciones_sql(tabla_id: &str, tabla: &DistilledDoubleEntryTable) -> String {
    format!(
        "SELECT valor_resultado_texto, valor_resultado_num, nota_pie_condicion FROM \"05_celdas_doble_entrada\" \
         WHERE tabla_id = '{tabla_id}' AND vigente = true AND <eje X «{}» dentro de eje_x_min/eje_x_max \
         según eje_x_*_inclusivo, o eje_x_categoria exacta> AND <ídem para el eje Y «{}»>",
        tabla.coordenada_x_nombre, tabla.coordenada_y_nombre
    )
}

fn fila_tabla(
    clave: &ClaveDocumental,
    tabla_id: &str,
    tabla: &DistilledDoubleEntryTable,
) -> FilaTabla {
    FilaTabla {
        tabla_id: tabla_id.to_string(),
        clave: clave.clone(),
        titulo_tabla: tabla.nombre_tabla.clone(),
        eje_x: EjeTabla {
            nombre: tabla.coordenada_x_nombre.clone(),
            tipo: tipo_eje(&tabla.celdas, true),
            unidad: None,
        },
        eje_y: EjeTabla {
            nombre: tabla.coordenada_y_nombre.clone(),
            tipo: tipo_eje(&tabla.celdas, false),
            unidad: None,
        },
        // Fase 11: nombre de la magnitud de salida destilado por el LLM
        valor_salida_nombre: String::new(),
        instrucciones_uso_sql: instrucciones_sql(tabla_id, tabla),
        vector_tabla: None,
    }
}

/// Valor numérico inicial de un resultado (`"100 (*)"` → 100, `"1,5 h"` → 1,5, `"No admisible"` → None)
fn interpretar_valor_numerico(valor: &str) -> Option<f64> {
    let numero: String = valor
        .trim()
        .chars()
        .enumerate()
        .take_while(|(i, c)| c.is_ascii_digit() || matches!(c, '.' | ',') || (*i == 0 && *c == '-'))
        .map(|(_, c)| if c == ',' { '.' } else { c })
        .collect();
    numero.parse::<f64>().ok()
}

fn fila_celda(
    clave: &ClaveDocumental,
    nivel: NivelPrelacion,
    tabla_id: &str,
    celda_id: String,
    celda: &DistilledCell,
) -> FilaCelda {
    FilaCelda {
        celda_id,
        tabla_id: tabla_id.to_string(),
        clave: clave.clone(),
        eje_x: LimitesEje {
            categoria: celda.categoria_x.clone(),
            minimo: celda.rango_x_min,
            minimo_inclusivo: celda.x_inclusivo_min,
            maximo: celda.rango_x_max,
            maximo_inclusivo: celda.x_inclusivo_max,
        },
        eje_y: LimitesEje {
            categoria: celda.categoria_y.clone(),
            minimo: celda.rango_y_min,
            minimo_inclusivo: celda.y_inclusivo_min,
            maximo: celda.rango_y_max,
            maximo_inclusivo: celda.y_inclusivo_max,
        },
        eje_z_filtro_adicional: None,
        valor_resultado_texto: celda.valor_resultado.clone(),
        valor_resultado_num: interpretar_valor_numerico(&celda.valor_resultado),
        unidad_medida: texto_opcional(&celda.unidad_medida),
        nota_pie_condicion: celda.notas_especificas.clone(),
        nivel_prelacion: nivel,
    }
}

fn filas_tablas_y_celdas(
    clave: &ClaveDocumental,
    nivel: NivelPrelacion,
    tablas: &[DistilledDoubleEntryTable],
) -> (Vec<FilaTabla>, Vec<FilaCelda>) {
    let mut filas_tabla = Vec::with_capacity(tablas.len());
    let mut filas_celda = Vec::new();
    for (idx_tabla, tabla) in tablas.iter().enumerate() {
        let tabla_id = id_versionado(
            &clave.doc_id,
            clave.version_num,
            TipoEntidad::Tabla,
            idx_tabla,
        );
        for celda in &tabla.celdas {
            let celda_id = id_versionado(
                &clave.doc_id,
                clave.version_num,
                TipoEntidad::Celda,
                filas_celda.len(),
            );
            filas_celda.push(fila_celda(clave, nivel, &tabla_id, celda_id, celda));
        }
        filas_tabla.push(fila_tabla(clave, &tabla_id, tabla));
    }
    (filas_tabla, filas_celda)
}

/// Código canónico del destino (`UNE-EN 1090-2` → `UNE-EN-1090-2`); se conserva el texto si queda vacío
fn destino_canonico(destino: &str) -> String {
    let canonico = codigo_canonico(destino);
    if canonico.is_empty() {
        destino.trim().to_string()
    } else {
        canonico
    }
}

fn filas_aristas(clave: &ClaveDocumental, aristas: &[DistilledReferenceEdge]) -> Vec<FilaArista> {
    aristas
        .iter()
        .enumerate()
        .map(|(idx, arista)| {
            let (tipo_relacion, _fue_respaldo) =
                TipoRelacionNormativa::parse_con_respaldo(&arista.tipo_relacion);
            FilaArista {
                edge_id: id_versionado(&clave.doc_id, clave.version_num, TipoEntidad::Arista, idx),
                clave: clave.clone(),
                origen_id: clave.doc_id.to_string(),
                origen_tipo: TipoNodoGrafo::Documento,
                tipo_relacion,
                destino_id: destino_canonico(&arista.doc_destino_id),
                destino_tipo: TipoNodoGrafo::destino_de(tipo_relacion),
                contexto_relacion: arista.contexto_cita.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
    use crate::infrastructure::llm::mock_distiller::MockDistiller;
    use crate::infrastructure::parsers::markdown_parser::MarkdownParser;
    use crate::infrastructure::storage::lancedb_store::LanceDbStore;
    use crate::infrastructure::storage::schemas::DIMENSION_EMBEDDINGS;

    #[test]
    fn test_interpreta_valor_numerico_de_la_celda() {
        assert_eq!(interpretar_valor_numerico("100 (*)"), Some(100.0));
        assert_eq!(interpretar_valor_numerico(" 1,5 h"), Some(1.5));
        assert_eq!(interpretar_valor_numerico("-10 °C"), Some(-10.0));
        assert_eq!(interpretar_valor_numerico("No admisible"), None);
        assert_eq!(interpretar_valor_numerico(""), None);
    }

    #[test]
    fn test_fecha_vigor_iso_y_respaldo() {
        assert_eq!(
            interpretar_fecha_vigor("2026-09-15T00:00:00Z"),
            NaiveDate::from_ymd_opt(2026, 9, 15).unwrap()
        );
        assert_eq!(
            interpretar_fecha_vigor("sin fecha"),
            Utc::now().date_naive()
        );
    }

    #[test]
    fn test_aristas_canonicas_con_ids_versionados() {
        let clave = ClaveDocumental {
            centro_id: CenterId::new("planta").unwrap(),
            doc_id: DocId::new("PO-ALM-03").unwrap(),
            version_num: 4,
        };
        let aristas = filas_aristas(
            &clave,
            &[DistilledReferenceEdge {
                doc_destino_id: "UNE-EN 1090-2".to_string(),
                tipo_relacion: "Exige llenar registro".to_string(),
                peso_prelacion: 1.0,
                contexto_cita: None,
            }],
        );
        assert_eq!(aristas[0].edge_id, "PO-ALM-03@v4-ARI-0000");
        assert_eq!(aristas[0].destino_id, "UNE-EN-1090-2");
        assert_eq!(aristas[0].tipo_relacion.as_str(), "EXIGE_LLENAR_REGISTRO");
        assert_eq!(aristas[0].destino_tipo, TipoNodoGrafo::Registro);
    }

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
