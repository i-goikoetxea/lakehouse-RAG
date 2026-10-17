//! Servicio de las 6 herramientas v2 del Lakehouse (plan v2 §2.4, Fase 13).
//!
//! Todas las tools leen las **vistas DataFusion del centro activo**, que contienen todas las
//! revisiones confirmadas (Fase 12): así el *time travel* se resuelve con la ventana de
//! `01_documentos_catalogo` y el aislamiento de centro con la lista de vistas permitidas.
//! Los filtros que escribe el agente nunca se interpolan en SQL: se aplican en Rust.

mod busqueda;
mod catalogo;
mod grafo;
mod lookup;
mod matriz;
mod sql_lectura;

use std::sync::Arc;
use std::time::Duration;

use arrow_array::RecordBatch;
use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::{Map, Value, json};

use crate::application::dtos::tools::*;
use crate::application::ports::analytical_engine::{AnalyticalEnginePort, PoliticaConsultaLectura};
use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::lakehouse_projection::nombre_vista;
use crate::application::ports::lakehouse_store::{TABLA_DOCUMENTOS_CATALOGO, TODAS_LAS_TABLAS};
use crate::application::ports::semantic_index::IndiceSemanticoPort;
use crate::application::ports::tool_server::ToolServerPort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

pub use sql_lectura::reescribir_tablas_canonicas;

/// Límites de las tools (problemas 4 y 6)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfiguracionHerramientas {
    /// Filas máximas de `execute_athena_sql`
    pub limite_filas_sql: usize,
    /// Tiempo máximo de cada consulta SQL
    pub timeout_sql: Duration,
    /// Filas máximas que leen las tools internas (matriz, lookup, grafo…)
    pub limite_filas_internas: usize,
    /// Vecinos por nodo en el grafo (por defecto)
    pub max_vecinos: usize,
    /// Grado a partir del cual un nodo (que no es el origen) se resume como hub
    pub umbral_hub: usize,
    /// Nodos máximos de una respuesta del grafo
    pub max_nodos: usize,
    /// Resultados por defecto de `search_normative_context`
    pub top_k: usize,
}

impl Default for ConfiguracionHerramientas {
    fn default() -> Self {
        Self {
            limite_filas_sql: 1000,
            timeout_sql: Duration::from_secs(30),
            limite_filas_internas: 100_000,
            max_vecinos: 15,
            umbral_hub: 25,
            max_nodos: 50,
            top_k: 8,
        }
    }
}

/// Servidor de herramientas del Lakehouse sobre las vistas proyectadas
pub struct LakehouseToolService {
    motor: Arc<dyn AnalyticalEnginePort>,
    indice: Arc<dyn IndiceSemanticoPort>,
    embeddings: Arc<dyn EmbeddingEnginePort>,
    config: ConfiguracionHerramientas,
}

/// Contexto común de una invocación: centro validado y fecha de referencia
pub(crate) struct ContextoConsulta {
    pub centro: CenterId,
    pub fecha: Option<NaiveDate>,
}

impl ContextoConsulta {
    pub fn vista(&self, tabla: &str) -> String {
        format!("\"{}\"", nombre_vista(&self.centro, tabla))
    }

    /// Predicado de vigencia sobre el alias de la tabla (`t`) y el de su catálogo (`d`):
    /// sin fecha → `t.vigente = true`; con fecha D → ventana `[fecha_vigor, fecha_derogacion)`
    pub fn predicado_vigencia(&self, alias_tabla: &str, alias_catalogo: &str) -> String {
        match self.fecha {
            None => format!("{alias_tabla}.vigente = true"),
            Some(fecha) => predicado_ventana(alias_catalogo, fecha),
        }
    }

    /// `JOIN` de una tabla dependiente con su revisión en `01_documentos_catalogo`
    pub fn union_catalogo(&self, alias_tabla: &str, alias_catalogo: &str) -> String {
        format!(
            "JOIN {} {alias_catalogo} ON {alias_tabla}.doc_id = {alias_catalogo}.doc_id AND {alias_tabla}.version_num = {alias_catalogo}.version_num",
            self.vista(TABLA_DOCUMENTOS_CATALOGO)
        )
    }

    pub fn fecha_json(&self) -> Value {
        self.fecha.map_or(Value::Null, |f| {
            Value::String(f.format("%Y-%m-%d").to_string())
        })
    }
}

/// `fecha_vigor <= D AND (fecha_derogacion IS NULL OR fecha_derogacion > D)`
pub(crate) fn predicado_ventana(alias_catalogo: &str, fecha: NaiveDate) -> String {
    let d = fecha.format("%Y-%m-%d");
    format!(
        "{alias_catalogo}.fecha_vigor <= DATE '{d}' AND ({alias_catalogo}.fecha_derogacion IS NULL OR {alias_catalogo}.fecha_derogacion > DATE '{d}')"
    )
}

/// Literal SQL escapado (solo para valores que ya vienen de las vistas o validados)
pub(crate) fn literal_sql(valor: &str) -> String {
    format!("'{}'", valor.replace('\'', "''"))
}

/// Lotes Arrow → filas JSON (las columnas nulas se omiten)
pub(crate) fn lotes_a_filas(lotes: &[RecordBatch]) -> Result<Vec<Map<String, Value>>, DomainError> {
    if lotes.iter().all(|l| l.num_rows() == 0) {
        return Ok(Vec::new());
    }
    let mut buffer = Vec::new();
    {
        let mut escritor = arrow::json::ArrayWriter::new(&mut buffer);
        for lote in lotes {
            escritor.write(lote).map_err(|e| {
                DomainError::ToolExecutionError(format!("Error escribiendo Arrow a JSON: {e}"))
            })?;
        }
        escritor
            .finish()
            .map_err(|e| DomainError::ToolExecutionError(format!("Error finalizando JSON: {e}")))?;
    }
    serde_json::from_slice(&buffer)
        .map_err(|e| DomainError::ToolExecutionError(format!("Error leyendo JSON de Arrow: {e}")))
}

/// Texto de una columna (vacío si es nula)
pub(crate) fn texto(fila: &Map<String, Value>, columna: &str) -> String {
    match fila.get(columna) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(otro) => otro.to_string(),
    }
}

pub(crate) fn texto_opcional(fila: &Map<String, Value>, columna: &str) -> Option<String> {
    Some(texto(fila, columna)).filter(|t| !t.trim().is_empty())
}

pub(crate) fn entero(fila: &Map<String, Value>, columna: &str) -> Option<i64> {
    fila.get(columna).and_then(Value::as_i64)
}

pub(crate) fn decimal(fila: &Map<String, Value>, columna: &str) -> Option<f64> {
    fila.get(columna).and_then(Value::as_f64)
}

pub(crate) fn booleano(fila: &Map<String, Value>, columna: &str) -> bool {
    fila.get(columna).and_then(Value::as_bool).unwrap_or(false)
}

fn resultado(
    call_id: &str,
    herramienta: LakehouseToolName,
    salida: Result<Value, DomainError>,
) -> ToolResult {
    match salida {
        Ok(output) => ToolResult {
            call_id: call_id.to_string(),
            tool_name: herramienta.as_str().to_string(),
            output,
            is_error: false,
        },
        Err(e) => ToolResult {
            call_id: call_id.to_string(),
            tool_name: herramienta.as_str().to_string(),
            output: json!({ "error": e.to_string() }),
            is_error: true,
        },
    }
}

impl LakehouseToolService {
    pub fn new(
        motor: Arc<dyn AnalyticalEnginePort>,
        indice: Arc<dyn IndiceSemanticoPort>,
        embeddings: Arc<dyn EmbeddingEnginePort>,
    ) -> Self {
        Self {
            motor,
            indice,
            embeddings,
            config: ConfiguracionHerramientas::default(),
        }
    }

    pub fn con_configuracion(mut self, config: ConfiguracionHerramientas) -> Self {
        self.config = config;
        self
    }

    pub fn configuracion(&self) -> &ConfiguracionHerramientas {
        &self.config
    }

    /// Valida el centro y la fecha, y exige que las vistas del centro estén proyectadas
    pub(crate) async fn contexto(
        &self,
        centro_id: &str,
        fecha_referencia: Option<&str>,
    ) -> Result<ContextoConsulta, DomainError> {
        let centro = CenterId::new(centro_id)?;
        let fecha = parsear_fecha_referencia(fecha_referencia)?;
        let registradas = self.motor.listar_tablas_registradas().await?;
        let catalogo = nombre_vista(&centro, TABLA_DOCUMENTOS_CATALOGO);
        if !registradas.contains(&catalogo) {
            return Err(DomainError::ValidationError(format!(
                "El centro '{centro}' no está activo o no tiene vistas proyectadas"
            )));
        }
        Ok(ContextoConsulta { centro, fecha })
    }

    /// Las 6 vistas del centro: únicas tablas que puede leer una consulta
    pub(crate) fn politica(
        &self,
        contexto: &ContextoConsulta,
        limite: usize,
    ) -> PoliticaConsultaLectura {
        PoliticaConsultaLectura {
            tablas_permitidas: TODAS_LAS_TABLAS
                .iter()
                .map(|t| nombre_vista(&contexto.centro, t))
                .collect(),
            limite_filas: limite,
            timeout: self.config.timeout_sql,
        }
    }

    /// Consulta interna (generada por la tool, sin texto del agente) restringida al centro
    pub(crate) async fn consultar(
        &self,
        contexto: &ContextoConsulta,
        sql: &str,
    ) -> Result<Vec<Map<String, Value>>, DomainError> {
        let politica = self.politica(contexto, self.config.limite_filas_internas);
        let resultado = self.motor.ejecutar_consulta_lectura(sql, &politica).await?;
        lotes_a_filas(&resultado.lotes)
    }
}

#[async_trait]
impl ToolServerPort for LakehouseToolService {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        ToolDefinition::lakehouse_tools()
    }

    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
        let herramienta = match LakehouseToolName::from_str_name(&call.name) {
            Ok(h) => h,
            Err(e) => {
                return Ok(ToolResult {
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    output: json!({ "error": e.to_string() }),
                    is_error: true,
                });
            }
        };
        let argumentos = &call.arguments;
        let salida = match herramienta {
            LakehouseToolName::InspectLakehouseCatalog => {
                self.inspeccionar_catalogo(argumentos).await
            }
            LakehouseToolName::QueryOperationalMatrix => self.consultar_matriz(argumentos).await,
            LakehouseToolName::LookupDoubleEntryTable => self.cruzar_tabla(argumentos).await,
            LakehouseToolName::ExecuteAthenaSql => self.ejecutar_sql_agente(argumentos).await,
            LakehouseToolName::TraverseNormativeGraph => self.recorrer_grafo(argumentos).await,
            LakehouseToolName::SearchNormativeContext => self.buscar_contexto(argumentos).await,
        };
        Ok(resultado(&call.id, herramienta, salida))
    }
}
