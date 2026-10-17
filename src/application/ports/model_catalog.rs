//! Puerto de aplicación para descubrir los modelos Claude disponibles en el entorno

use async_trait::async_trait;

use crate::application::dtos::agent::ModelDescriptor;
use crate::domain::errors::DomainError;

/// Puerto que provee el catálogo dinámico de modelos seleccionables por el operador
#[async_trait]
pub trait ModelCatalogPort: Send + Sync {
    /// Retorna los modelos disponibles para la cuenta/instalación activa
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError>;
}
