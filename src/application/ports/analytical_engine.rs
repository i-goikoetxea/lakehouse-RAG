//! Puerto de aplicación para el motor analítico de consultas (Apache DataFusion)

use std::time::Duration;

use arrow_array::RecordBatch;
use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

/// Límites de una consulta SQL de solo lectura emitida por el agente (plan v2, problema 6)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoliticaConsultaLectura {
    /// Únicas tablas que la consulta puede leer (nombres registrados, sin comillas)
    pub tablas_permitidas: Vec<String>,
    /// Filas máximas devueltas; si hay más, el resultado se marca como truncado
    pub limite_filas: usize,
    /// Tiempo máximo de planificación y ejecución
    pub timeout: Duration,
}

/// Resultado acotado de `ejecutar_consulta_lectura`
#[derive(Debug, Clone)]
pub struct ResultadoConsultaLectura {
    pub lotes: Vec<RecordBatch>,
    pub filas: usize,
    /// Verdadero si la consulta producía más filas que `limite_filas`
    pub truncado: bool,
}

/// Puerto que abstrae el motor analítico SQL en memoria y el aislamiento de centros
#[async_trait]
pub trait AnalyticalEnginePort: Send + Sync {
    /// Registra una tabla o vista en memoria a partir de lotes de Apache Arrow
    async fn registrar_tabla(
        &self,
        nombre_tabla: &str,
        lotes: Vec<RecordBatch>,
    ) -> Result<(), DomainError>;

    /// Desregistra una tabla o vista del motor analítico
    async fn desregistrar_tabla(&self, nombre_tabla: &str) -> Result<(), DomainError>;

    /// Registra las tablas de un centro de conocimiento aislando su catálogo
    async fn registrar_centro(
        &self,
        center_id: &CenterId,
        nombre_tabla: &str,
        lotes: Vec<RecordBatch>,
    ) -> Result<(), DomainError>;

    /// Desregistra todas las tablas asociadas a un centro de conocimiento
    async fn desregistrar_centro(&self, center_id: &CenterId) -> Result<(), DomainError>;

    /// Ejecuta una consulta analítica SQL determinista de solo lectura (SELECT)
    async fn ejecutar_sql(&self, sql: &str) -> Result<Vec<RecordBatch>, DomainError>;

    /// Ejecuta una única sentencia `SELECT`/`WITH` validada con el parser SQL del motor:
    /// rechaza DDL, DML, `COPY`, varias sentencias y cualquier tabla (o función de tabla)
    /// fuera de `tablas_permitidas`; aplica el límite de filas y el timeout.
    async fn ejecutar_consulta_lectura(
        &self,
        sql: &str,
        politica: &PoliticaConsultaLectura,
    ) -> Result<ResultadoConsultaLectura, DomainError>;

    /// Retorna la lista de tablas actualmente registradas en el contexto de sesión
    async fn listar_tablas_registradas(&self) -> Result<Vec<String>, DomainError>;
}
