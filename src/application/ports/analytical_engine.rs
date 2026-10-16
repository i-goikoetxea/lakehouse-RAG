//! Puerto de aplicación para el motor analítico de consultas (Apache DataFusion)

use arrow_array::RecordBatch;
use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

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

    /// Retorna la lista de tablas actualmente registradas en el contexto de sesión
    async fn listar_tablas_registradas(&self) -> Result<Vec<String>, DomainError>;
}
