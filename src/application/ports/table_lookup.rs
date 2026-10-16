//! Puerto de aplicación para la búsqueda determinista en tablas de doble entrada (Tabla 04 y 05)

use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::domain::models::double_entry_table::CeldaDobleEntrada;

/// Puerto para consultas y resolución de celdas cruzadas en matrices técnicas
#[async_trait]
pub trait TableLookupPort: Send + Sync {
    /// Resuelve de manera determinista una celda en una tabla técnica cruzando fila y columna
    /// ya sea por etiqueta categórica o por valor numérico evaluado en rango continuo.
    async fn buscar_celda(
        &self,
        nombre_tabla: &str,
        etiqueta_fila: Option<&str>,
        valor_fila: Option<f64>,
        etiqueta_columna: Option<&str>,
        valor_columna: Option<f64>,
    ) -> Result<Option<CeldaDobleEntrada>, DomainError>;

    /// Lista todas las celdas vigentes pertenecientes a una tabla técnica específica
    async fn listar_celdas_por_tabla(
        &self,
        nombre_tabla: &str,
    ) -> Result<Vec<CeldaDobleEntrada>, DomainError>;
}
