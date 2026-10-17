//! Etapa 1 de validación E2E: ingesta real de un documento de punta a punta.
//!
//! Flujo: Markdown → `MarkdownParser` → `ClaudeHeadlessDistiller` (Claude real) →
//! `FastEmbedEngine` (ONNX real) → `LanceDbStore` en disco (tempdir).
//!
//! Fixtures: `tests/fixtures/procedimiento_recepcion_rev03.md` y `_rev04.md`.
//! Respuestas correctas esperadas (detalle en `tests/fixtures/README.md`):
//! - Matriz operativa: 3 filas separadas (RECEPCION, CUSTODIA, INSPECCION).
//!   Responsable de la CUSTODIA: «Jefe de Producción»; registro: FOR-ALM-02 (Rev. 03)
//!   y FOR-ALM-05 (Rev. 04). El Jefe de Almacén firma la RECEPCIÓN, no la CUSTODIA.
//! - Tabla 4-A (9 celdas): cruce e = 30 mm, CE = 0,42 % → 100 °C con nota (*)
//!   «aprobación del Inspector QA/QC y ensayo de dureza HV10» en la Rev. 03; 125 °C en la Rev. 04.
//! - Al ingerir la Rev. 04 sobre la Rev. 03, todas las filas de la Rev. 03 pasan a `vigente = false`.
//!
//! Ejecución:
//! - Suite normal (mocks, sin red): `cargo test --test acceptance_e2e_etapa1_ingesta`
//! - Reales (Claude + FastEmbed): `cargo test --test acceptance_e2e_etapa1_ingesta -- --ignored --nocapture --test-threads=1`
//!   Variables opcionales: `ETAPA1_CLAUDE_TIMEOUT_S` (por defecto 600) y `ETAPA1_CLAUDE_MODEL` (por defecto `sonnet`).
//!   Los paquetes JSON destilados por Claude se vuelcan en `target/validacion_etapa1/`.
//!
//! Los tests `hallazgo_*` describen el comportamiento correcto. Los que la Fase 10 arregló
//! (H1 esquemas, H3, H4, H5 y H9) ya corren en la suite normal; los pendientes (H7, H8) siguen
//! con `#[ignore]` y en rojo a propósito.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use arrow::compute::concat_batches;
use arrow_array::{
    Array, BooleanArray, FixedSizeListArray, Float32Array, Float64Array, Int32Array, RecordBatch,
    StringArray,
};
use async_trait::async_trait;
use futures::StreamExt;
use lakehouse_rag::application::dtos::ingestion::{
    DistilledCatalog, DistilledCell, DistilledDocumentPackage, DistilledDoubleEntryTable,
    DistilledOperationalTuple, DistilledReferenceEdge, ParsedDocument,
};
use lakehouse_rag::application::dtos::lakehouse_filas::LoteIngesta;
use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::application::ports::embedding_engine::EmbeddingEnginePort;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, HuellaIngerida, InformeIntegridad, LakehouseStorePort, ResumenConfirmacion,
    TABLA_CATALOGO_TABLAS, TABLA_CELDAS_DOBLE_ENTRADA, TABLA_CHUNKS_NORMATIVA,
    TABLA_DOCUMENTOS_CATALOGO, TABLA_GRAFO_REFERENCIAS, TABLA_MATRIZ_OPERATIVA, TODAS_LAS_TABLAS,
};
use lakehouse_rag::application::ports::llm_distiller::LlmDistillerPort;
use lakehouse_rag::application::use_cases::document_ingestion_service::{
    DocumentIngestionService, IngestionReport,
};
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::domain::models::reference_graph::TipoRelacionNormativa;
use lakehouse_rag::infrastructure::embeddings::fastembed_engine::FastEmbedEngine;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::llm::{
    ClaudeHeadlessConfig, ClaudeHeadlessDistiller, MockDistiller,
};
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::schemas::{
    DIMENSION_EMBEDDINGS, schema_catalogo_tablas, schema_documentos_catalogo,
    schema_matriz_operativa,
};
use lancedb::query::{ExecutableQuery, QueryBase};

const FIXTURE_REV03: &str = "procedimiento_recepcion_rev03.md";
const FIXTURE_REV04: &str = "procedimiento_recepcion_rev04.md";
const DOC_ID_ESPERADO: &str = "PO-ALM-03";
const CENTRO_PRUEBA: &str = "obra-planta-solar-01";

/// Cruce de control de la Tabla 4-A: espesor 30 mm, carbono equivalente 0,42 %.
const CRUCE_ESPESOR_MM: f64 = 30.0;
const CRUCE_CE_PORCENTAJE: f64 = 0.42;

const LIMITE_LECTURA_FILAS: usize = 100_000;

/// Vocabulario de `tipo_relacion` de la Tabla 6 (§4 de la especificación + `REFERENCIA_A` del plan v2).
const RELACIONES_SPEC: [&str; 6] = TipoRelacionNormativa::VALORES_CANONICOS;

/// Comprobaciones que fallan por hallazgos ya documentados (ver tests `hallazgo_*`).
/// La variante con mocks las excluye para no romper la suite normal.
/// Fase 10: H1 (esquemas), H3 (vigencia del grafo) y H4 (IDs) ya no fallan.
const HALLAZGOS_CONOCIDOS: [&str; 0] = [];

// ─────────────────────────────────────────────────────────────────────────────
// Cronometraje por puerto (decoradores que miden cada paso del pipeline real)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct CronometroPipeline {
    marcas: Mutex<Vec<(String, Duration)>>,
}

impl CronometroPipeline {
    fn registrar(&self, paso: impl Into<String>, duracion: Duration) {
        self.marcas.lock().unwrap().push((paso.into(), duracion));
    }

    fn imprimir(&self, titulo: &str) {
        println!("\n⏱  Tiempos — {titulo}");
        for (paso, duracion) in self.marcas.lock().unwrap().iter() {
            println!("   {paso:<48} {:>10.3} s", duracion.as_secs_f64());
        }
    }

    fn reiniciar(&self) {
        self.marcas.lock().unwrap().clear();
    }
}

struct ParserCronometrado {
    interno: MarkdownParser,
    cronometro: Arc<CronometroPipeline>,
}

#[async_trait]
impl DocumentParserPort for ParserCronometrado {
    async fn parse(
        &self,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<ParsedDocument, DomainError> {
        let inicio = Instant::now();
        let resultado = self.interno.parse(nombre_archivo, contenido).await;
        self.cronometro
            .registrar("1. parseo Markdown", inicio.elapsed());
        resultado
    }
}

struct DestiladorCronometrado {
    interno: Arc<dyn LlmDistillerPort>,
    cronometro: Arc<CronometroPipeline>,
    volcado_json: Option<PathBuf>,
}

#[async_trait]
impl LlmDistillerPort for DestiladorCronometrado {
    async fn distill(
        &self,
        parsed: &ParsedDocument,
    ) -> Result<DistilledDocumentPackage, DomainError> {
        let inicio = Instant::now();
        let resultado = self.interno.distill(parsed).await;
        self.cronometro
            .registrar("2. destilación LLM", inicio.elapsed());
        if let (Some(directorio), Ok(paquete)) = (&self.volcado_json, &resultado) {
            volcar_paquete_json(directorio, &parsed.nombre_archivo, paquete);
        }
        resultado
    }
}

struct EmbeddingsCronometrados {
    interno: Arc<dyn EmbeddingEnginePort>,
    cronometro: Arc<CronometroPipeline>,
}

#[async_trait]
impl EmbeddingEnginePort for EmbeddingsCronometrados {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, DomainError> {
        self.interno.generate_embedding(text).await
    }

    async fn generate_batch_embeddings(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, DomainError> {
        let inicio = Instant::now();
        let resultado = self.interno.generate_batch_embeddings(texts).await;
        self.cronometro.registrar(
            format!("3. embeddings ({} textos)", texts.len()),
            inicio.elapsed(),
        );
        resultado
    }

    fn dimension(&self) -> usize {
        self.interno.dimension()
    }
}

struct AlmacenCronometrado {
    interno: Arc<LanceDbStore>,
    cronometro: Arc<CronometroPipeline>,
}

#[async_trait]
impl LakehouseStorePort for AlmacenCronometrado {
    async fn inicializar_tablas(&self) -> Result<(), DomainError> {
        self.interno.inicializar_tablas().await
    }

    async fn existe_hash(
        &self,
        centro_id: &CenterId,
        hash_sha256: &str,
    ) -> Result<Option<HuellaIngerida>, DomainError> {
        let inicio = Instant::now();
        let resultado = self.interno.existe_hash(centro_id, hash_sha256).await;
        self.cronometro
            .registrar("0. comprobación de hash", inicio.elapsed());
        resultado
    }

    async fn escribir_staged(
        &self,
        ingest_id: &IngestId,
        lotes: Vec<LoteIngesta>,
    ) -> Result<usize, DomainError> {
        let inicio = Instant::now();
        let resultado = self.interno.escribir_staged(ingest_id, lotes).await;
        self.cronometro
            .registrar("4. escritura STAGED (6 tablas)", inicio.elapsed());
        resultado
    }

    async fn confirmar_ingesta(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
    ) -> Result<ResumenConfirmacion, DomainError> {
        let inicio = Instant::now();
        let resultado = self.interno.confirmar_ingesta(ingest_id, doc_id).await;
        self.cronometro.registrar(
            "5. commit (activar y retirar vigencia previa)",
            inicio.elapsed(),
        );
        resultado
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

// ─────────────────────────────────────────────────────────────────────────────
// Verificador de comprobaciones (acumula resultados en lugar de abortar al primero)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EstadoComprobacion {
    Ok,
    Falla,
    NoExiste,
}

struct ResultadoComprobacion {
    clave: &'static str,
    estado: EstadoComprobacion,
    detalle: String,
}

#[derive(Default)]
struct InformeVerificacion {
    resultados: Vec<ResultadoComprobacion>,
}

impl InformeVerificacion {
    fn anotar(&mut self, clave: &'static str, cumple: bool, detalle: impl Into<String>) {
        let estado = if cumple {
            EstadoComprobacion::Ok
        } else {
            EstadoComprobacion::Falla
        };
        self.resultados.push(ResultadoComprobacion {
            clave,
            estado,
            detalle: detalle.into(),
        });
    }

    fn anotar_inexistente(&mut self, clave: &'static str, detalle: impl Into<String>) {
        self.resultados.push(ResultadoComprobacion {
            clave,
            estado: EstadoComprobacion::NoExiste,
            detalle: detalle.into(),
        });
    }

    fn imprimir(&self, titulo: &str) {
        println!("\n📋 Comprobaciones — {titulo}");
        for r in &self.resultados {
            let etiqueta = match r.estado {
                EstadoComprobacion::Ok => "OK       ",
                EstadoComprobacion::Falla => "FALLA    ",
                EstadoComprobacion::NoExiste => "NO EXISTE",
            };
            println!("   [{etiqueta}] {:<34} {}", r.clave, r.detalle);
        }
    }

    fn fallidas_excepto(&self, excluidas: &[&str]) -> Vec<&'static str> {
        self.resultados
            .iter()
            .filter(|r| r.estado != EstadoComprobacion::Ok && !excluidas.contains(&r.clave))
            .map(|r| r.clave)
            .collect()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Utilidades de lectura de LanceDB y normalización
// ─────────────────────────────────────────────────────────────────────────────

fn ruta_fixture(nombre: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(nombre)
}

fn leer_fixture(nombre: &str) -> String {
    std::fs::read_to_string(ruta_fixture(nombre))
        .unwrap_or_else(|e| panic!("No se pudo leer el fixture {nombre}: {e}"))
}

fn directorio_volcados() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("validacion_etapa1")
}

fn volcar_paquete_json(
    directorio: &PathBuf,
    nombre_archivo: &str,
    paquete: &DistilledDocumentPackage,
) {
    let _ = std::fs::create_dir_all(directorio);
    let destino = directorio.join(format!("{nombre_archivo}.destilado.json"));
    if let Ok(json) = serde_json::to_string_pretty(paquete) {
        let _ = std::fs::write(destino, json);
    }
}

/// Lee todas las filas de una tabla que cumplen el filtro, concatenadas en un único lote.
async fn leer_filas(store: &LanceDbStore, tabla: &str, filtro: Option<&str>) -> RecordBatch {
    let tabla_abierta = store.abrir_tabla(tabla).await.expect("Abrir tabla");
    // OJO: `Query::new` de lancedb 0.15 fija `limit = Some(10)` aunque la doc diga
    // "sin límite"; sin este `limit` explícito solo se leen 10 filas.
    let consulta_base = tabla_abierta.query().limit(LIMITE_LECTURA_FILAS);
    let consulta = match filtro {
        Some(f) => consulta_base.only_if(f),
        None => consulta_base,
    };
    let mut flujo = consulta.execute().await.expect("Ejecutar consulta LanceDB");
    let mut lotes = Vec::new();
    while let Some(lote) = flujo.next().await {
        lotes.push(lote.expect("Leer lote"));
    }
    let esquema = tabla_abierta.schema().await.expect("Esquema de la tabla");
    concat_batches(&esquema, &lotes).expect("Concatenar lotes")
}

async fn contar_filas(store: &LanceDbStore, tabla: &str, filtro: &str) -> usize {
    let tabla_abierta = store.abrir_tabla(tabla).await.expect("Abrir tabla");
    tabla_abierta
        .count_rows(Some(filtro.to_string()))
        .await
        .expect("Contar filas con filtro")
}

fn columna_texto(lote: &RecordBatch, nombre: &str) -> Vec<Option<String>> {
    let Some(columna) = lote.column_by_name(nombre) else {
        return Vec::new();
    };
    let textos = columna
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap_or_else(|| panic!("La columna {nombre} no es Utf8"));
    (0..textos.len())
        .map(|i| (!textos.is_null(i)).then(|| textos.value(i).to_string()))
        .collect()
}

fn columna_f64(lote: &RecordBatch, nombre: &str) -> Vec<Option<f64>> {
    let valores = lote
        .column_by_name(nombre)
        .and_then(|c| c.as_any().downcast_ref::<Float64Array>().cloned())
        .unwrap_or_else(|| panic!("Columna Float64 {nombre} ausente"));
    (0..valores.len())
        .map(|i| (!valores.is_null(i)).then(|| valores.value(i)))
        .collect()
}

fn columna_bool(lote: &RecordBatch, nombre: &str) -> Vec<bool> {
    let valores = lote
        .column_by_name(nombre)
        .and_then(|c| c.as_any().downcast_ref::<BooleanArray>().cloned())
        .unwrap_or_else(|| panic!("Columna Boolean {nombre} ausente"));
    (0..valores.len()).map(|i| valores.value(i)).collect()
}

/// Mayúsculas y sin tildes, para comparar fases como `RECEPCIÓN` y `RECEPCION`.
fn normalizar(texto: &str) -> String {
    texto
        .to_uppercase()
        .chars()
        .map(|c| match c {
            'Á' => 'A',
            'É' => 'E',
            'Í' => 'I',
            'Ó' => 'O',
            'Ú' => 'U',
            otro => otro,
        })
        .collect()
}

/// Concatena todas las columnas de texto de una fila (para buscar un código donde quiera que esté).
fn texto_de_fila(lote: &RecordBatch, fila: usize) -> String {
    let mut partes = Vec::new();
    for columna in lote.columns() {
        if let Some(textos) = columna.as_any().downcast_ref::<StringArray>()
            && !textos.is_null(fila)
        {
            partes.push(textos.value(fila).to_string());
        }
    }
    partes.join(" | ")
}

struct RangoCelda {
    minimo: Option<f64>,
    maximo: Option<f64>,
    minimo_inclusivo: bool,
    maximo_inclusivo: bool,
}

impl RangoCelda {
    fn contiene(&self, valor: f64) -> bool {
        let cumple_minimo = match self.minimo {
            None => true,
            Some(m) => valor > m || (self.minimo_inclusivo && (valor - m).abs() < f64::EPSILON),
        };
        let cumple_maximo = match self.maximo {
            None => true,
            Some(m) => valor < m || (self.maximo_inclusivo && (valor - m).abs() < f64::EPSILON),
        };
        let tiene_algun_limite = self.minimo.is_some() || self.maximo.is_some();
        tiene_algun_limite && cumple_minimo && cumple_maximo
    }
}

struct CeldaLeida {
    rango_x: RangoCelda,
    rango_y: RangoCelda,
    valor: String,
    notas: String,
}

fn leer_celdas(lote: &RecordBatch) -> Vec<CeldaLeida> {
    let x_min = columna_f64(lote, "eje_x_min");
    let x_max = columna_f64(lote, "eje_x_max");
    let x_inc_min = columna_bool(lote, "eje_x_min_inclusivo");
    let x_inc_max = columna_bool(lote, "eje_x_max_inclusivo");
    let y_min = columna_f64(lote, "eje_y_min");
    let y_max = columna_f64(lote, "eje_y_max");
    let y_inc_min = columna_bool(lote, "eje_y_min_inclusivo");
    let y_inc_max = columna_bool(lote, "eje_y_max_inclusivo");
    let valores = columna_texto(lote, "valor_resultado_texto");
    let notas = columna_texto(lote, "nota_pie_condicion");

    (0..lote.num_rows())
        .map(|i| CeldaLeida {
            rango_x: RangoCelda {
                minimo: x_min[i],
                maximo: x_max[i],
                minimo_inclusivo: x_inc_min[i],
                maximo_inclusivo: x_inc_max[i],
            },
            rango_y: RangoCelda {
                minimo: y_min[i],
                maximo: y_max[i],
                minimo_inclusivo: y_inc_min[i],
                maximo_inclusivo: y_inc_max[i],
            },
            valor: valores[i].clone().unwrap_or_default(),
            notas: notas[i].clone().unwrap_or_default(),
        })
        .collect()
}

/// Busca la celda del cruce de control. Si ninguna encaja con X = espesor e Y = CE,
/// prueba la orientación invertida por si el LLM intercambió los ejes.
fn buscar_celda_cruce(celdas: &[CeldaLeida]) -> Vec<&CeldaLeida> {
    let filtrar = |valor_x: f64, valor_y: f64| -> Vec<&CeldaLeida> {
        celdas
            .iter()
            .filter(|c| c.rango_x.contiene(valor_x) && c.rango_y.contiene(valor_y))
            .collect()
    };
    let orientacion_directa = filtrar(CRUCE_ESPESOR_MM, CRUCE_CE_PORCENTAJE);
    if orientacion_directa.is_empty() {
        filtrar(CRUCE_CE_PORCENTAJE, CRUCE_ESPESOR_MM)
    } else {
        orientacion_directa
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Comprobaciones de la Etapa 1
// ─────────────────────────────────────────────────────────────────────────────

fn verificar_esquemas_contra_spec(informe: &mut InformeVerificacion) {
    let matriz = schema_matriz_operativa();
    let faltantes_03: Vec<&str> = [
        "hito_id",
        "orden_secuencia",
        "objeto_alcance",
        "rol_ejecutor",
        "rol_responsable",
        "registro_codigo",
        "registro_nombre",
        "criterio_aceptacion",
        "chunk_origen_id",
    ]
    .into_iter()
    .filter(|campo| matriz.field_with_name(campo).is_err())
    .collect();
    if faltantes_03.is_empty() {
        informe.anotar(
            "esquema_03_campos_spec",
            true,
            "todos los campos de §4 presentes",
        );
    } else {
        informe.anotar_inexistente(
            "esquema_03_campos_spec",
            format!("faltan en 03_matriz_operativa: {faltantes_03:?}"),
        );
    }

    let tiene_vector_hito = matriz.field_with_name("vector_hito").is_ok();
    let tiene_vector_tabla = schema_catalogo_tablas()
        .field_with_name("vector_tabla")
        .is_ok();
    if tiene_vector_hito && tiene_vector_tabla {
        informe.anotar(
            "esquema_vectores_hito_tabla",
            true,
            "vector_hito y vector_tabla presentes",
        );
    } else {
        informe.anotar_inexistente(
            "esquema_vectores_hito_tabla",
            format!(
                "vector_hito presente={tiene_vector_hito}, vector_tabla presente={tiene_vector_tabla} (solo 02 tiene vector)"
            ),
        );
    }

    let catalogo = schema_documentos_catalogo();
    let faltantes_01: Vec<&str> = ["revision", "fecha_vigor", "tipo_documento"]
        .into_iter()
        .filter(|campo| catalogo.field_with_name(campo).is_err())
        .collect();
    if faltantes_01.is_empty() {
        informe.anotar("esquema_01_campos_spec", true, "campos de §4 presentes");
    } else {
        informe.anotar_inexistente(
            "esquema_01_campos_spec",
            format!("faltan en 01_documentos_catalogo: {faltantes_01:?}"),
        );
    }
}

async fn verificar_tablas_creadas(store: &LanceDbStore, informe: &mut InformeVerificacion) {
    let existentes = store.listar_tablas().await.expect("Listar tablas");
    let faltantes: Vec<&str> = TODAS_LAS_TABLAS
        .into_iter()
        .filter(|t| !existentes.contains(&t.to_string()))
        .collect();
    informe.anotar(
        "seis_tablas_creadas",
        faltantes.is_empty(),
        format!("existentes={existentes:?}"),
    );
}

async fn verificar_conteos(
    store: &LanceDbStore,
    reporte: &IngestionReport,
    informe: &mut InformeVerificacion,
) {
    let filtro = format!("doc_id = '{}' AND vigente = true", reporte.doc_id);
    let esperados = [
        (TABLA_DOCUMENTOS_CATALOGO, 1, filtro.as_str()),
        (
            TABLA_CHUNKS_NORMATIVA,
            reporte.total_chunks,
            filtro.as_str(),
        ),
        (
            TABLA_MATRIZ_OPERATIVA,
            reporte.total_tuplas,
            filtro.as_str(),
        ),
        (TABLA_CATALOGO_TABLAS, reporte.total_tablas, filtro.as_str()),
        (
            TABLA_CELDAS_DOBLE_ENTRADA,
            reporte.total_celdas,
            filtro.as_str(),
        ),
        (
            TABLA_GRAFO_REFERENCIAS,
            reporte.total_aristas,
            filtro.as_str(),
        ),
    ];
    let mut detalle = Vec::new();
    let mut todo_cuadra = true;
    for (tabla, esperado, filtro_tabla) in esperados {
        let reales = contar_filas(store, tabla, filtro_tabla).await;
        todo_cuadra &= reales == esperado && reales > 0;
        detalle.push(format!("{tabla}={reales}/{esperado}"));
    }
    informe.anotar("filas_por_tabla", todo_cuadra, detalle.join(", "));
}

async fn verificar_matriz_anti_fusion(
    store: &LanceDbStore,
    doc_id: &str,
    registro_custodia_esperado: &str,
    informe: &mut InformeVerificacion,
) {
    let lote = leer_filas(
        store,
        TABLA_MATRIZ_OPERATIVA,
        Some(&format!("doc_id = '{doc_id}' AND vigente = true")),
    )
    .await;
    let fases: Vec<String> = columna_texto(&lote, "fase_operativa")
        .into_iter()
        .map(|f| normalizar(&f.unwrap_or_default()))
        .collect();
    let fases_distintas: BTreeSet<&String> = fases.iter().collect();

    informe.anotar(
        "matriz_min_3_filas",
        lote.num_rows() >= 3,
        format!("{} filas vigentes, fases={fases:?}", lote.num_rows()),
    );

    let objetivo = ["RECEPCION", "CUSTODIA", "INSPECCION"];
    let cada_fase_en_su_fila = objetivo.iter().all(|f| fases.iter().any(|x| x.contains(f)));
    let alguna_fila_fusionada = fases
        .iter()
        .any(|x| objetivo.iter().filter(|f| x.contains(**f)).count() > 1);
    informe.anotar(
        "matriz_anti_fusion",
        cada_fase_en_su_fila && !alguna_fila_fusionada && fases_distintas.len() >= 3,
        format!(
            "las 3 fases presentes={cada_fase_en_su_fila}, fila fusionada={alguna_fila_fusionada}"
        ),
    );

    let fases_crudas: Vec<String> = columna_texto(&lote, "fase_operativa")
        .into_iter()
        .flatten()
        .collect();
    let fases_sql_exactas = objetivo
        .iter()
        .all(|f| fases_crudas.iter().any(|cruda| cruda == f));
    informe.anotar(
        "fases_sql_exactas",
        fases_sql_exactas,
        format!(
            "un WHERE fase_operativa = 'RECEPCION' necesita el valor canónico sin tilde; valores={fases_crudas:?}"
        ),
    );

    let responsables = columna_texto(&lote, "rol_responsable");
    let filas_custodia: Vec<usize> = (0..lote.num_rows())
        .filter(|i| fases[*i].contains("CUSTODIA"))
        .collect();
    let responsables_custodia: Vec<String> = filas_custodia
        .iter()
        .map(|i| responsables[*i].clone().unwrap_or_default())
        .collect();
    let responsable_correcto = !responsables_custodia.is_empty()
        && responsables_custodia
            .iter()
            .all(|r| normalizar(r).contains("PRODUCCION"));
    informe.anotar(
        "custodia_responsable",
        responsable_correcto,
        format!(
            "CUSTODIA → rol_responsable={responsables_custodia:?} (esperado Jefe de Producción)"
        ),
    );

    let responsable_sin_ejecutor = !responsables_custodia.is_empty()
        && responsables_custodia
            .iter()
            .all(|r| !normalizar(r).contains("AUXILIAR"));
    informe.anotar(
        "custodia_responsable_sin_ejecutor",
        responsable_sin_ejecutor,
        "rol_responsable no debe mezclar al ejecutor (Auxiliar de Almacén)",
    );

    let textos_custodia: Vec<String> = filas_custodia
        .iter()
        .map(|i| texto_de_fila(&lote, *i))
        .collect();
    let registro_presente = textos_custodia
        .iter()
        .any(|t| t.contains(registro_custodia_esperado));
    informe.anotar(
        "custodia_registro",
        registro_presente,
        format!(
            "{registro_custodia_esperado} en fila CUSTODIA={registro_presente} (registro_codigo llega en la Fase 11; se busca en cualquier columna)"
        ),
    );
}

async fn verificar_celdas_doble_entrada(
    store: &LanceDbStore,
    doc_id: &str,
    valor_esperado: &str,
    informe: &mut InformeVerificacion,
) {
    let lote = leer_filas(
        store,
        TABLA_CELDAS_DOBLE_ENTRADA,
        Some(&format!("doc_id = '{doc_id}' AND vigente = true")),
    )
    .await;
    let celdas = leer_celdas(&lote);
    let con_rangos = celdas
        .iter()
        .filter(|c| {
            (c.rango_x.minimo.is_some() || c.rango_x.maximo.is_some())
                && (c.rango_y.minimo.is_some() || c.rango_y.maximo.is_some())
        })
        .count();
    informe.anotar(
        "celdas_9_con_rangos_xy",
        celdas.len() >= 9 && con_rangos >= 9,
        format!(
            "{} celdas vigentes, {con_rangos} con rangos numéricos en X e Y",
            celdas.len()
        ),
    );

    let coincidencias = buscar_celda_cruce(&celdas);
    let valor_correcto =
        coincidencias.len() == 1 && coincidencias[0].valor.contains(valor_esperado);
    informe.anotar(
        "celda_cruce_valor",
        valor_correcto,
        format!(
            "cruce e=30 mm, CE=0,42 → {:?} (esperado {valor_esperado}, 1 sola celda)",
            coincidencias
                .iter()
                .map(|c| c.valor.as_str())
                .collect::<Vec<_>>()
        ),
    );

    let nota_correcta = coincidencias.len() == 1 && {
        let nota = normalizar(&coincidencias[0].notas);
        nota.contains("QA/QC") || nota.contains("DUREZA")
    };
    informe.anotar(
        "celda_cruce_nota_pie",
        nota_correcta,
        format!(
            "notas={:?} (esperada nota (*) QA/QC + dureza)",
            coincidencias
                .iter()
                .map(|c| c.notas.as_str())
                .collect::<Vec<_>>()
        ),
    );

    let frontera_20_inclusiva = celdas.iter().any(|c| {
        (c.rango_x.contiene(20.0) && c.rango_x.maximo == Some(20.0))
            || (c.rango_y.contiene(20.0) && c.rango_y.maximo == Some(20.0))
    });
    informe.anotar(
        "celda_frontera_inclusiva",
        frontera_20_inclusiva,
        "e = 20 mm cae en la columna e ≤ 20 (máximo inclusivo)",
    );
}

async fn verificar_grafo(store: &LanceDbStore, doc_id: &str, informe: &mut InformeVerificacion) {
    let lote = leer_filas(
        store,
        TABLA_GRAFO_REFERENCIAS,
        Some(&format!("doc_id = '{doc_id}' AND vigente = true")),
    )
    .await;
    let destinos: Vec<String> = columna_texto(&lote, "destino_id")
        .into_iter()
        .flatten()
        .collect();
    let menciona_norma = destinos
        .iter()
        .any(|d| d.contains("1090") || d.contains("15614"));
    informe.anotar(
        "grafo_aristas",
        lote.num_rows() >= 3 && menciona_norma,
        format!("{} aristas → {destinos:?}", lote.num_rows()),
    );

    let relaciones: Vec<String> = columna_texto(&lote, "tipo_relacion")
        .into_iter()
        .flatten()
        .collect();
    let fuera_de_vocabulario: BTreeSet<&String> = relaciones
        .iter()
        .filter(|r| !RELACIONES_SPEC.contains(&r.as_str()))
        .collect();
    informe.anotar(
        "grafo_vocabulario_spec",
        fuera_de_vocabulario.is_empty(),
        format!("tipo_relacion fuera de §4: {fuera_de_vocabulario:?}"),
    );

    let registros_enlazados: Vec<&String> = destinos
        .iter()
        .zip(relaciones.iter())
        .filter(|(_, relacion)| relacion.as_str() == "EXIGE_LLENAR_REGISTRO")
        .map(|(destino, _)| destino)
        .collect();
    informe.anotar(
        "grafo_registros",
        registros_enlazados.len() >= 3,
        format!("aristas EXIGE_LLENAR_REGISTRO → {registros_enlazados:?} (esperados FOR-ALM-01, FOR-ALM-0x, PPI-MAT-09)"),
    );
}

async fn verificar_derogacion(
    store: &LanceDbStore,
    doc_id: &str,
    informe: &mut InformeVerificacion,
) {
    let derogaciones = contar_filas(
        store,
        TABLA_GRAFO_REFERENCIAS,
        &format!("doc_id = '{doc_id}' AND tipo_relacion = 'DEROGA_A' AND vigente = true"),
    )
    .await;
    informe.anotar(
        "grafo_deroga_a_rev03",
        derogaciones >= 1,
        format!("{derogaciones} aristas DEROGA_A vigentes"),
    );
}

async fn verificar_vectores(
    store: &LanceDbStore,
    doc_id: &str,
    dimension_motor: usize,
    informe: &mut InformeVerificacion,
) {
    let lote = leer_filas(
        store,
        TABLA_CHUNKS_NORMATIVA,
        Some(&format!("doc_id = '{doc_id}' AND vigente = true")),
    )
    .await;
    let vectores = lote
        .column_by_name("vector")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>().cloned())
        .expect("Columna vector FixedSizeList");
    let dimension_columna = vectores.value_length() as usize;
    let mut sin_nulos_ni_ceros = vectores.null_count() == 0;
    for fila in 0..vectores.len() {
        let valores = vectores.value(fila);
        let flotantes = valores
            .as_any()
            .downcast_ref::<Float32Array>()
            .expect("Float32");
        let norma: f32 = flotantes.values().iter().map(|v| v * v).sum::<f32>().sqrt();
        sin_nulos_ni_ceros &= norma > 0.0;
    }
    informe.anotar(
        "vectores_dimension",
        dimension_columna == DIMENSION_EMBEDDINGS as usize
            && dimension_motor == dimension_columna
            && sin_nulos_ni_ceros
            && vectores.len() > 0,
        format!(
            "{} vectores, columna={dimension_columna}, motor={dimension_motor}, sin nulos/ceros={sin_nulos_ni_ceros}",
            vectores.len()
        ),
    );
}

async fn verificar_versionado(
    store: &LanceDbStore,
    reporte_previo: &IngestionReport,
    reporte_nuevo: &IngestionReport,
    informe: &mut InformeVerificacion,
) {
    informe.anotar(
        "doc_id_estable_entre_revisiones",
        reporte_previo.doc_id == reporte_nuevo.doc_id,
        format!("{} → {}", reporte_previo.doc_id, reporte_nuevo.doc_id),
    );
    let doc_id = &reporte_nuevo.doc_id;

    let catalogo = leer_filas(
        store,
        TABLA_DOCUMENTOS_CATALOGO,
        Some(&format!("doc_id = '{doc_id}'")),
    )
    .await;
    let versiones = catalogo
        .column_by_name("version_num")
        .and_then(|c| c.as_any().downcast_ref::<Int32Array>().cloned())
        .expect("Columna version_num");
    let vigentes = columna_bool(&catalogo, "vigente");
    let pares: Vec<(i32, bool)> = (0..catalogo.num_rows())
        .map(|i| (versiones.value(i), vigentes[i]))
        .collect();
    let solo_ultima_vigente = pares.iter().filter(|(_, v)| *v).count() == 1
        && pares
            .iter()
            .all(|(ver, vig)| *vig == (*ver == reporte_nuevo.version));
    informe.anotar(
        "vigencia_01_documentos_catalogo",
        solo_ultima_vigente && reporte_nuevo.version > reporte_previo.version,
        format!("(version, vigente)={pares:?}"),
    );

    let filtro_doc = format!("doc_id = '{doc_id}'");
    let casos: [(&'static str, &str, &str, usize, usize); 5] = [
        (
            "vigencia_02_chunks_normativa",
            TABLA_CHUNKS_NORMATIVA,
            &filtro_doc,
            reporte_previo.total_chunks,
            reporte_nuevo.total_chunks,
        ),
        (
            "vigencia_03_matriz_operativa",
            TABLA_MATRIZ_OPERATIVA,
            &filtro_doc,
            reporte_previo.total_tuplas,
            reporte_nuevo.total_tuplas,
        ),
        (
            "vigencia_04_catalogo_tablas",
            TABLA_CATALOGO_TABLAS,
            &filtro_doc,
            reporte_previo.total_tablas,
            reporte_nuevo.total_tablas,
        ),
        (
            "vigencia_05_celdas_doble_entrada",
            TABLA_CELDAS_DOBLE_ENTRADA,
            &filtro_doc,
            reporte_previo.total_celdas,
            reporte_nuevo.total_celdas,
        ),
        (
            "vigencia_06_grafo_referencias",
            TABLA_GRAFO_REFERENCIAS,
            &filtro_doc,
            reporte_previo.total_aristas,
            reporte_nuevo.total_aristas,
        ),
    ];
    for (clave, tabla, filtro, previas, nuevas) in casos {
        let historicas = contar_filas(store, tabla, &format!("{filtro} AND vigente = false")).await;
        let activas = contar_filas(store, tabla, &format!("{filtro} AND vigente = true")).await;
        informe.anotar(
            clave,
            historicas == previas && activas == nuevas,
            format!("vigente=false {historicas}/{previas}, vigente=true {activas}/{nuevas}"),
        );
    }

    let chunks = leer_filas(store, TABLA_CHUNKS_NORMATIVA, Some(&filtro_doc)).await;
    let ids = columna_texto(&chunks, "chunk_id");
    let unicos: BTreeSet<&Option<String>> = ids.iter().collect();
    informe.anotar(
        "ids_unicos_entre_revisiones",
        unicos.len() == ids.len(),
        format!("chunk_id: {} filas, {} distintos", ids.len(), unicos.len()),
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Montaje del pipeline
// ─────────────────────────────────────────────────────────────────────────────

struct PipelineEtapa1 {
    servicio: DocumentIngestionService,
    store: Arc<LanceDbStore>,
    cronometro: Arc<CronometroPipeline>,
    dimension_motor: usize,
    _directorio_temporal: tempfile::TempDir,
}

async fn montar_pipeline(
    distiller: Arc<dyn LlmDistillerPort>,
    embeddings: Arc<dyn EmbeddingEnginePort>,
    volcado_json: Option<PathBuf>,
) -> PipelineEtapa1 {
    let directorio_temporal = tempfile::tempdir().expect("Crear tempdir");
    let store = Arc::new(
        LanceDbStore::conectar(directorio_temporal.path())
            .await
            .expect("Conectar LanceDB"),
    );
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar 6 tablas");
    let cronometro = Arc::new(CronometroPipeline::default());
    let dimension_motor = embeddings.dimension();

    let servicio = DocumentIngestionService::new(
        Arc::new(ParserCronometrado {
            interno: MarkdownParser::new(),
            cronometro: cronometro.clone(),
        }),
        Arc::new(DestiladorCronometrado {
            interno: distiller,
            cronometro: cronometro.clone(),
            volcado_json,
        }),
        Arc::new(EmbeddingsCronometrados {
            interno: embeddings,
            cronometro: cronometro.clone(),
        }),
        Arc::new(AlmacenCronometrado {
            interno: store.clone(),
            cronometro: cronometro.clone(),
        }),
    );

    PipelineEtapa1 {
        servicio,
        store,
        cronometro,
        dimension_motor,
        _directorio_temporal: directorio_temporal,
    }
}

async fn ingerir_cronometrado(
    pipeline: &PipelineEtapa1,
    nombre_fixture: &str,
) -> Result<IngestionReport, DomainError> {
    pipeline.cronometro.reiniciar();
    let centro = CenterId::new(CENTRO_PRUEBA).unwrap();
    let contenido = leer_fixture(nombre_fixture);
    let inicio = Instant::now();
    let resultado = pipeline
        .servicio
        .ingestar_documento(&centro, nombre_fixture, &contenido)
        .await;
    pipeline
        .cronometro
        .registrar("TOTAL ingestar_documento", inicio.elapsed());
    pipeline.cronometro.imprimir(nombre_fixture);
    resultado
}

/// Ejecuta la batería completa: ingesta Rev. 03, comprobaciones, ingesta Rev. 04 y versionado.
async fn ejecutar_bateria_completa(pipeline: &PipelineEtapa1) -> InformeVerificacion {
    let mut informe = InformeVerificacion::default();
    verificar_esquemas_contra_spec(&mut informe);
    verificar_tablas_creadas(&pipeline.store, &mut informe).await;

    let reporte_rev03 = match ingerir_cronometrado(pipeline, FIXTURE_REV03).await {
        Ok(r) => r,
        Err(e) => {
            informe.anotar("ingesta_rev03", false, format!("error: {e}"));
            return informe;
        }
    };
    println!("\n📦 Reporte Rev. 03: {reporte_rev03:?}");
    informe.anotar(
        "ingesta_rev03",
        true,
        format!(
            "doc_id={}, version={}, chunks={}, tuplas={}, tablas={}, celdas={}, aristas={}",
            reporte_rev03.doc_id,
            reporte_rev03.version,
            reporte_rev03.total_chunks,
            reporte_rev03.total_tuplas,
            reporte_rev03.total_tablas,
            reporte_rev03.total_celdas,
            reporte_rev03.total_aristas
        ),
    );
    informe.anotar(
        "doc_id_canonico",
        reporte_rev03.doc_id == DOC_ID_ESPERADO,
        format!(
            "doc_id={} (esperado {DOC_ID_ESPERADO})",
            reporte_rev03.doc_id
        ),
    );

    let doc_id = reporte_rev03.doc_id.clone();
    verificar_conteos(&pipeline.store, &reporte_rev03, &mut informe).await;
    verificar_matriz_anti_fusion(&pipeline.store, &doc_id, "FOR-ALM-02", &mut informe).await;
    verificar_celdas_doble_entrada(&pipeline.store, &doc_id, "100", &mut informe).await;
    verificar_grafo(&pipeline.store, &doc_id, &mut informe).await;
    verificar_vectores(
        &pipeline.store,
        &doc_id,
        pipeline.dimension_motor,
        &mut informe,
    )
    .await;

    let reporte_rev04 = match ingerir_cronometrado(pipeline, FIXTURE_REV04).await {
        Ok(r) => r,
        Err(e) => {
            informe.anotar("ingesta_rev04", false, format!("error: {e}"));
            return informe;
        }
    };
    println!("\n📦 Reporte Rev. 04: {reporte_rev04:?}");
    informe.anotar(
        "ingesta_rev04",
        true,
        format!("version={}", reporte_rev04.version),
    );
    verificar_versionado(
        &pipeline.store,
        &reporte_rev03,
        &reporte_rev04,
        &mut informe,
    )
    .await;
    verificar_derogacion(&pipeline.store, &reporte_rev04.doc_id, &mut informe).await;
    verificar_matriz_anti_fusion(
        &pipeline.store,
        &reporte_rev04.doc_id,
        "FOR-ALM-05",
        &mut informe,
    )
    .await;
    verificar_celdas_doble_entrada(&pipeline.store, &reporte_rev04.doc_id, "125", &mut informe)
        .await;
    informe
}

// ─────────────────────────────────────────────────────────────────────────────
// Paquetes "oro" para la variante con mocks (lo que Claude debería devolver)
// ─────────────────────────────────────────────────────────────────────────────

fn tupla_oro(
    fase: &str,
    ejecutor: &str,
    responsable: &str,
    registro: &str,
) -> DistilledOperationalTuple {
    DistilledOperationalTuple {
        seccion: "2".to_string(),
        fase_operativa: fase.to_string(),
        rol_responsable: responsable.to_string(),
        accion: format!("Ejecuta: {ejecutor}"),
        parametro_control: "Criterio de aceptación de la fase".to_string(),
        tolerancia_criterio: "Según PO-ALM-03".to_string(),
        condicion_aplicacion: "Siempre".to_string(),
        // El DTO v1 no tiene `registro_codigo`: el código del registro solo cabe aquí (Fase 11).
        referencia_normativa: registro.to_string(),
    }
}

fn celda_oro(
    rango_espesor: (Option<f64>, Option<f64>, bool, bool),
    rango_ce: (Option<f64>, Option<f64>, bool, bool),
    valor: &str,
    nota: Option<&str>,
) -> DistilledCell {
    DistilledCell {
        rango_x_min: rango_espesor.0,
        rango_x_max: rango_espesor.1,
        x_inclusivo_min: rango_espesor.2,
        x_inclusivo_max: rango_espesor.3,
        categoria_x: None,
        rango_y_min: rango_ce.0,
        rango_y_max: rango_ce.1,
        y_inclusivo_min: rango_ce.2,
        y_inclusivo_max: rango_ce.3,
        categoria_y: None,
        valor_resultado: valor.to_string(),
        unidad_medida: "°C".to_string(),
        notas_especificas: nota.map(str::to_string),
    }
}

fn paquete_oro(
    version: i32,
    registro_custodia: &str,
    valor_cruce: &str,
) -> DistilledDocumentPackage {
    let nota_asterisco =
        "(*) Requiere aprobación previa del Inspector QA/QC y ensayo de dureza HV10";
    let nota_uno = "(1) Mantener la temperatura de interpaso por debajo de 250 °C (Anexo II)";
    let espesores = [
        (None, Some(20.0), false, true),
        (Some(20.0), Some(40.0), false, true),
        (Some(40.0), None, false, false),
    ];
    let carbonos = [
        (None, Some(0.40), false, false),
        (Some(0.40), Some(0.45), true, true),
        (Some(0.45), None, false, false),
    ];
    let valores = [
        [("10", None), ("50", None), ("100", None)],
        [
            ("50", None),
            (valor_cruce, Some(nota_asterisco)),
            ("150", Some(nota_uno)),
        ],
        [
            ("100", None),
            ("150", Some(nota_uno)),
            ("No admisible", Some(nota_asterisco)),
        ],
    ];
    let mut celdas = Vec::new();
    for (fila, ce) in carbonos.iter().enumerate() {
        for (columna, espesor) in espesores.iter().enumerate() {
            let (valor, nota) = valores[fila][columna];
            celdas.push(celda_oro(*espesor, *ce, valor, nota));
        }
    }

    let mut aristas: Vec<DistilledReferenceEdge> = [
        ("UNE-EN 1090-2", "SUBORDINADO_A_NORMA"),
        ("UNE-EN ISO 15614-1", "SUBORDINADO_A_NORMA"),
        ("PO-CAL-01", "SUBORDINADO_A_NORMA"),
        ("PCC-01", "SUBORDINADO_A_NORMA"),
        ("FOR-ALM-01", "EXIGE_LLENAR_REGISTRO"),
        (registro_custodia, "EXIGE_LLENAR_REGISTRO"),
        ("PPI-MAT-09", "EXIGE_LLENAR_REGISTRO"),
    ]
    .into_iter()
    .map(|(destino, relacion)| DistilledReferenceEdge {
        doc_destino_id: destino.to_string(),
        tipo_relacion: relacion.to_string(),
        peso_prelacion: 1.0,
        contexto_cita: None,
    })
    .collect();
    if version == 4 {
        aristas.push(DistilledReferenceEdge {
            doc_destino_id: DOC_ID_ESPERADO.to_string(),
            tipo_relacion: "DEROGA_A".to_string(),
            peso_prelacion: 1.0,
            contexto_cita: Some("Sustitución íntegra".to_string()),
        });
    }

    DistilledDocumentPackage {
        catalogo: DistilledCatalog {
            doc_id: DOC_ID_ESPERADO.to_string(),
            titulo:
                "Procedimiento de Recepción, Custodia e Inspección de Tubería de Acero al Carbono"
                    .to_string(),
            tipo_documental: "PROCEDIMIENTO_OBRA".to_string(),
            emisor: "Dirección de Calidad de Obra".to_string(),
            fecha_emision: if version == 3 {
                "2026-03-01"
            } else {
                "2026-09-15"
            }
            .to_string(),
            nivel_prelacion: 2,
            version,
        },
        tuplas_operativas: vec![
            tupla_oro("RECEPCION", "Almacenero", "Jefe de Almacén", "FOR-ALM-01"),
            tupla_oro(
                "CUSTODIA",
                "Auxiliar de Almacén",
                "Jefe de Producción",
                registro_custodia,
            ),
            tupla_oro(
                "INSPECCION",
                "Técnico de Calidad",
                "Inspector QA/QC",
                "PPI-MAT-09",
            ),
        ],
        tablas_doble_entrada: vec![DistilledDoubleEntryTable {
            nombre_tabla: "Tabla 4-A Temperatura mínima de precalentamiento (°C)".to_string(),
            coordenada_x_nombre: "espesor_nominal_mm".to_string(),
            coordenada_y_nombre: "carbono_equivalente_pct".to_string(),
            nota_pie: Some(format!("{nota_asterisco}. {nota_uno}")),
            celdas,
        }],
        aristas_referencia: aristas,
    }
}

fn distiller_oro() -> Arc<MockDistiller> {
    let distiller = MockDistiller::new();
    distiller.registrar_respuesta(FIXTURE_REV03, paquete_oro(3, "FOR-ALM-02", "100"));
    distiller.registrar_respuesta(FIXTURE_REV04, paquete_oro(4, "FOR-ALM-05", "125"));
    Arc::new(distiller)
}

fn embeddings_mock() -> Arc<MockEmbeddingEngine> {
    Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )))
}

fn configurar_cache_fastembed() {
    static CONFIGURAR: Once = Once::new();
    CONFIGURAR.call_once(|| {
        if std::env::var_os("FASTEMBED_CACHE_DIR").is_none() {
            let destino = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("fastembed_cache");
            // SAFETY: se ejecuta una sola vez antes de inicializar FastEmbed; los tests reales
            // se lanzan con --test-threads=1, así que ningún otro hilo lee el entorno a la vez.
            unsafe { std::env::set_var("FASTEMBED_CACHE_DIR", destino) };
        }
    });
}

fn config_claude_validacion() -> ClaudeHeadlessConfig {
    let timeout_seconds = std::env::var("ETAPA1_CLAUDE_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(600);
    let model = std::env::var("ETAPA1_CLAUDE_MODEL").unwrap_or_else(|_| "sonnet".to_string());
    ClaudeHeadlessConfig {
        binary_path: "claude".to_string(),
        model,
        timeout_seconds,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Variante con mocks (suite normal)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn mock_parser_conserva_jerarquia_y_tabla_del_fixture() {
    let parsed = MarkdownParser::new()
        .parse(FIXTURE_REV03, &leer_fixture(FIXTURE_REV03))
        .await
        .expect("Parseo del fixture");

    let niveles: BTreeSet<u8> = parsed.secciones.iter().map(|s| s.nivel).collect();
    assert!(niveles.contains(&1) && niveles.contains(&2) && niveles.contains(&3));
    assert!(parsed.titulo.contains("PO-ALM-03"));
    assert!(
        parsed
            .secciones
            .iter()
            .any(|s| s.titulo.contains("CUSTODIA"))
    );

    assert_eq!(parsed.tablas.len(), 1, "Una sola tabla de doble entrada");
    let tabla = &parsed.tablas[0];
    assert_eq!(tabla.encabezados.len(), 4);
    assert_eq!(tabla.filas.len(), 3);
    assert_eq!(tabla.filas[1][2], "100 (*)");
}

#[tokio::test]
async fn mock_pipeline_rev03_y_rev04_sobre_lancedb_en_disco() {
    let pipeline = montar_pipeline(distiller_oro(), embeddings_mock(), None).await;
    let informe = ejecutar_bateria_completa(&pipeline).await;
    informe.imprimir("pipeline con mocks (destilación oro + embeddings mock)");

    let fallidas = informe.fallidas_excepto(&HALLAZGOS_CONOCIDOS);
    assert!(
        fallidas.is_empty(),
        "Comprobaciones fallidas no documentadas como hallazgo: {fallidas:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Variante real (Claude + FastEmbed): ejecutar con --ignored --nocapture --test-threads=1
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requiere descargar el modelo ONNX de FastEmbed (~1,3 GB la primera vez)"]
async fn real_fastembed_genera_vectores_de_dimension_1024() {
    configurar_cache_fastembed();
    let inicio = Instant::now();
    let motor = tokio::task::spawn_blocking(|| FastEmbedEngine::try_new(None))
        .await
        .unwrap()
        .expect("Inicializar FastEmbed");
    println!(
        "⏱  carga del modelo FastEmbed: {:.3} s",
        inicio.elapsed().as_secs_f64()
    );

    let textos = vec![
        "La custodia la firma el Jefe de Producción con el registro FOR-ALM-02".to_string(),
        "Temperatura mínima de precalentamiento para espesor 30 mm".to_string(),
    ];
    let inicio = Instant::now();
    let vectores = motor
        .generate_batch_embeddings(&textos)
        .await
        .expect("Embeddings");
    println!(
        "⏱  embeddings de 2 textos: {:.3} s",
        inicio.elapsed().as_secs_f64()
    );

    assert_eq!(motor.dimension(), DIMENSION_EMBEDDINGS as usize);
    assert!(
        vectores
            .iter()
            .all(|v| v.len() == DIMENSION_EMBEDDINGS as usize)
    );
}

#[tokio::test]
#[ignore = "invoca la CLI real de Claude con la configuración por defecto de producción (timeout 60 s)"]
async fn real_claude_destilador_con_configuracion_por_defecto() {
    let distiller = ClaudeHeadlessDistiller::new(ClaudeHeadlessConfig::default());
    let parsed = MarkdownParser::new()
        .parse(FIXTURE_REV03, &leer_fixture(FIXTURE_REV03))
        .await
        .unwrap();
    println!(
        "Longitud del prompt (aprox.): {} bytes",
        parsed.contenido_completo.len() + distiller.json_schema().len()
    );

    let inicio = Instant::now();
    let resultado = distiller.distill(&parsed).await;
    println!(
        "⏱  destilación con config por defecto: {:.3} s",
        inicio.elapsed().as_secs_f64()
    );

    match &resultado {
        Ok(paquete) => {
            volcar_paquete_json(&directorio_volcados(), "default_config_rev03", paquete);
            println!(
                "✅ doc_id={}, version={}, tuplas={}, tablas={}, aristas={}",
                paquete.catalogo.doc_id,
                paquete.catalogo.version,
                paquete.tuplas_operativas.len(),
                paquete.tablas_doble_entrada.len(),
                paquete.aristas_referencia.len()
            );
        }
        Err(e) => {
            let mensaje = e.to_string();
            let recorte: String = mensaje.chars().take(1500).collect();
            println!("❌ {recorte}");
        }
    }
    assert!(
        resultado.is_ok(),
        "La destilación con la configuración por defecto falló"
    );
}

#[tokio::test]
#[ignore = "E2E real: Claude headless + FastEmbed + LanceDB en disco (varios minutos)"]
async fn real_e2e_ingesta_rev03_y_rev04_claude_fastembed_lancedb() {
    configurar_cache_fastembed();
    let inicio = Instant::now();
    let motor = tokio::task::spawn_blocking(|| FastEmbedEngine::try_new(None))
        .await
        .unwrap()
        .expect("Inicializar FastEmbed");
    println!(
        "⏱  carga del modelo FastEmbed: {:.3} s",
        inicio.elapsed().as_secs_f64()
    );

    let config = config_claude_validacion();
    println!(
        "Claude: modelo={}, timeout={} s",
        config.model, config.timeout_seconds
    );
    let pipeline = montar_pipeline(
        Arc::new(ClaudeHeadlessDistiller::new(config)),
        Arc::new(motor),
        Some(directorio_volcados()),
    )
    .await;

    let informe = ejecutar_bateria_completa(&pipeline).await;
    informe.imprimir("pipeline REAL (Claude headless + FastEmbed + LanceDB)");

    let fallidas = informe.fallidas_excepto(&[]);
    assert!(
        fallidas.is_empty(),
        "Comprobaciones no superadas: {fallidas:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Hallazgos: describen el comportamiento correcto y están en rojo hasta que se arreglen
// ─────────────────────────────────────────────────────────────────────────────

/// H3 arreglado en la Fase 10: la 06 tiene `doc_id` y la confirmación retira la vigencia en las 6 tablas.
#[tokio::test]
async fn hallazgo_rev04_retira_vigencia_de_las_aristas_del_grafo() {
    let pipeline = montar_pipeline(distiller_oro(), embeddings_mock(), None).await;
    ingerir_cronometrado(&pipeline, FIXTURE_REV03)
        .await
        .unwrap();
    ingerir_cronometrado(&pipeline, FIXTURE_REV04)
        .await
        .unwrap();

    let aristas_rev03_vigentes = contar_filas(
        &pipeline.store,
        TABLA_GRAFO_REFERENCIAS,
        &format!("doc_id = '{DOC_ID_ESPERADO}' AND vigente = true"),
    )
    .await;
    // Rev. 03 aporta 7 aristas y Rev. 04 aporta 8: solo deberían quedar vigentes las 8 nuevas.
    assert_eq!(
        aristas_rev03_vigentes, 8,
        "Las aristas de la Rev. 03 siguen vigentes"
    );
}

/// H4 arreglado en la Fase 10: IDs versionados (`PO-ALM-03@v3-HIT-0000`).
#[tokio::test]
async fn hallazgo_ids_no_colisionan_entre_revisiones() {
    let pipeline = montar_pipeline(distiller_oro(), embeddings_mock(), None).await;
    ingerir_cronometrado(&pipeline, FIXTURE_REV03)
        .await
        .unwrap();
    ingerir_cronometrado(&pipeline, FIXTURE_REV04)
        .await
        .unwrap();

    let claves_primarias = [
        (TABLA_CHUNKS_NORMATIVA, "chunk_id"),
        (TABLA_MATRIZ_OPERATIVA, "hito_id"),
        (TABLA_CATALOGO_TABLAS, "tabla_id"),
        (TABLA_CELDAS_DOBLE_ENTRADA, "celda_id"),
        (TABLA_GRAFO_REFERENCIAS, "edge_id"),
    ];
    for (tabla, columna) in claves_primarias {
        let lote = leer_filas(&pipeline.store, tabla, None).await;
        let ids = columna_texto(&lote, columna);
        let distintos: BTreeSet<&Option<String>> = ids.iter().collect();
        assert!(!ids.is_empty(), "{tabla} vacía");
        assert_eq!(distintos.len(), ids.len(), "{columna} duplicados: {ids:?}");
    }
}

/// H5 arreglado en la Fase 10: `existe_hash` antes de destilar.
#[tokio::test]
async fn hallazgo_reingesta_del_mismo_documento_no_duplica_filas() {
    let pipeline = montar_pipeline(distiller_oro(), embeddings_mock(), None).await;
    let primero = ingerir_cronometrado(&pipeline, FIXTURE_REV03)
        .await
        .unwrap();
    let segundo = ingerir_cronometrado(&pipeline, FIXTURE_REV03)
        .await
        .unwrap();
    assert!(
        segundo.ya_ingerido,
        "La re-ingesta debería devolver «ya ingerido»"
    );
    assert_eq!(segundo.ingest_id, primero.ingest_id);

    let filas_catalogo = contar_filas(
        &pipeline.store,
        TABLA_DOCUMENTOS_CATALOGO,
        &format!("hash_sha256 = '{}'", primero.sha256),
    )
    .await;
    assert_eq!(filas_catalogo, 1, "El mismo hash se ingirió dos veces");
}

/// H1 (esquemas) arreglado en la Fase 10: las 6 tablas siguen §4.
#[tokio::test]
async fn hallazgo_esquemas_cumplen_la_especificacion() {
    let mut informe = InformeVerificacion::default();
    verificar_esquemas_contra_spec(&mut informe);
    informe.imprimir("esquemas contra §4");
    assert!(informe.fallidas_excepto(&[]).is_empty());
}

#[tokio::test]
#[ignore = "HALLAZGO H7 (Fase 11): breadcrumb_jerarquia guarda solo el título de su sección, no H1 > H2 > H3"]
async fn hallazgo_chunks_guardan_breadcrumb_jerarquico() {
    let pipeline = montar_pipeline(distiller_oro(), embeddings_mock(), None).await;
    ingerir_cronometrado(&pipeline, FIXTURE_REV03)
        .await
        .unwrap();

    let chunks = leer_filas(&pipeline.store, TABLA_CHUNKS_NORMATIVA, None).await;
    let secciones: Vec<String> = columna_texto(&chunks, "breadcrumb_jerarquia")
        .into_iter()
        .flatten()
        .collect();
    let custodia = secciones
        .iter()
        .find(|s| s.contains("CUSTODIA"))
        .expect("Chunk de custodia");
    assert!(
        custodia.contains("Secuencia Operativa"),
        "El chunk de custodia pierde su sección padre: {custodia:?}"
    );
}

#[tokio::test]
#[ignore = "HALLAZGO H8: el parser pierde el título de la tabla si hay línea en blanco y solo capta notas que empiezan por '*' o 'Nota:'"]
async fn hallazgo_parser_captura_notas_al_pie_de_la_tabla() {
    let parsed = MarkdownParser::new()
        .parse(FIXTURE_REV03, &leer_fixture(FIXTURE_REV03))
        .await
        .unwrap();
    let notas = parsed.tablas[0].notas.clone().unwrap_or_default();
    assert!(
        notas.contains("(*)") && notas.contains("(1)"),
        "notas={notas:?}"
    );
}

/// H9 arreglado en las Fases 9 y 10: `DocId` con charset estricto rechaza la comilla antes de
/// escribir nada, y además todos los predicados del almacén escapan sus literales.
#[tokio::test]
async fn hallazgo_doc_id_con_comilla_se_rechaza_sin_escribir_nada() {
    let distiller = MockDistiller::new();
    let mut rev_a = paquete_oro(3, "FOR-ALM-02", "100");
    rev_a.catalogo.doc_id = "PO-ALM-03'B".to_string();
    distiller.registrar_respuesta(FIXTURE_REV03, rev_a);

    let pipeline = montar_pipeline(Arc::new(distiller), embeddings_mock(), None).await;
    let error = ingerir_cronometrado(&pipeline, FIXTURE_REV03)
        .await
        .unwrap_err();
    assert!(matches!(error, DomainError::InvalidDocId(_)), "{error:?}");
    for tabla in TODAS_LAS_TABLAS {
        let filas = pipeline.store.contar_registros(tabla).await.unwrap();
        assert_eq!(filas, 0, "{tabla} tiene filas tras rechazar el doc_id");
    }
}
