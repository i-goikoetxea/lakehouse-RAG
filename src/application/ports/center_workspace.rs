//! Puerto del espacio en disco de un centro (especificación §3.2):
//! `center_manifest.json`, `raw_docs/`, `glue_catalog/` y `lakehouse_data/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use crate::application::dtos::centros::{
    DocumentoOriginal, ManifiestoCentroDisco, ModeloEmbeddingsActivo,
};
use crate::application::ports::lakehouse_store::LakehouseStorePort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

/// Centro abierto y validado: su manifiesto coincide con el modelo activo
#[derive(Clone)]
pub struct CentroAbierto {
    pub id: CenterId,
    pub ruta: PathBuf,
    pub manifiesto: ManifiestoCentroDisco,
    pub store: Arc<dyn LakehouseStorePort>,
}

/// Contrato del adaptador que gestiona la estructura física de un centro
#[async_trait]
pub trait CenterWorkspacePort: Send + Sync {
    /// Convierte una ruta ya validada léxicamente en absoluta (sin tocar el disco)
    fn ruta_absoluta(&self, ruta: &Path) -> Result<PathBuf, DomainError>;

    /// Crea (de forma idempotente) los subdirectorios y el manifiesto. Si ya existe un
    /// manifiesto, lo valida contra el modelo activo y lo devuelve sin sobrescribirlo.
    async fn preparar(
        &self,
        ruta: &Path,
        manifiesto: &ManifiestoCentroDisco,
        modelo_activo: &ModeloEmbeddingsActivo,
    ) -> Result<ManifiestoCentroDisco, DomainError>;

    /// Abre un centro existente: lee y valida el manifiesto y conecta su almacén LanceDB
    async fn abrir(
        &self,
        id: &CenterId,
        ruta: &Path,
        modelo_activo: &ModeloEmbeddingsActivo,
    ) -> Result<CentroAbierto, DomainError>;

    /// Lee un original y deja su copia inmutable en `raw_docs/` con su SHA-256
    async fn guardar_original(
        &self,
        ruta_centro: &Path,
        ruta_archivo: &Path,
    ) -> Result<DocumentoOriginal, DomainError>;
}
