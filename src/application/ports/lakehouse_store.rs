//! Puerto para el almacenamiento columnar en el Lakehouse (LanceDB / Apache Arrow)

use arrow_array::RecordBatch;
use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;

/// Nombres constantes inmutables para las 6 tablas del Lakehouse
pub const TABLA_DOC_CATALOG: &str = "01_doc_catalog";
pub const TABLA_DOC_CHUNKS: &str = "02_doc_chunks";
pub const TABLA_OPERATIONAL_MATRIX: &str = "03_operational_matrix";
pub const TABLA_DOUBLE_ENTRY_TABLES: &str = "04_double_entry_tables";
pub const TABLA_DOUBLE_ENTRY_CELLS: &str = "05_double_entry_cells";
pub const TABLA_REFERENCE_GRAPH: &str = "06_reference_graph";

/// Arreglo con los nombres de las 6 tablas del sistema
pub const TODAS_LAS_TABLAS: [&str; 6] = [
    TABLA_DOC_CATALOG,
    TABLA_DOC_CHUNKS,
    TABLA_OPERATIONAL_MATRIX,
    TABLA_DOUBLE_ENTRY_TABLES,
    TABLA_DOUBLE_ENTRY_CELLS,
    TABLA_REFERENCE_GRAPH,
];

/// Puerto de salida que define las operaciones del almacén columnar
#[async_trait]
pub trait LakehouseStorePort: Send + Sync {
    /// Inicializa de forma idempotente las 6 tablas del Lakehouse si no existen
    async fn inicializar_tablas(&self) -> Result<(), DomainError>;

    /// Inserta un lote de registros (RecordBatch) en la tabla especificada
    async fn insertar_lote(&self, nombre_tabla: &str, lote: RecordBatch)
    -> Result<(), DomainError>;

    /// Marca como no vigentes (`vigente = false`) todos los registros vinculados al documento indicado
    /// en las tablas aplicables (catálogo, chunks, matriz, tablas, celdas y referencias).
    /// Devuelve el total de registros invalidados.
    async fn retirar_vigencia_documento(&self, doc_id: &DocId) -> Result<u64, DomainError>;

    /// Cuenta la cantidad de registros actualmente almacenados en una tabla
    async fn contar_registros(&self, nombre_tabla: &str) -> Result<usize, DomainError>;

    /// Devuelve la lista de tablas existentes en el Lakehouse
    async fn listar_tablas(&self) -> Result<Vec<String>, DomainError>;
}
