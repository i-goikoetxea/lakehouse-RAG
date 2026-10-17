//! Puerto de proyección (plan v2 §0): LanceDB es la única fuente de verdad y DataFusion,
//! Petgraph y TableLookup son vistas derivadas que se reconstruyen desde ella.

use async_trait::async_trait;

use crate::application::ports::center_workspace::CentroAbierto;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

/// Una tabla registrada en DataFusion para un centro
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VistaProyectada {
    /// Nombre canónico de la tabla (`03_matriz_operativa`)
    pub tabla: String,
    /// Nombre con prefijo de centro registrado en DataFusion (`obra-a_03_matriz_operativa`)
    pub vista: String,
    /// Filas `COMMITTED` (vigentes e históricas)
    pub filas: usize,
    pub filas_vigentes: usize,
}

/// Resultado de `reconstruir_vistas`
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResumenProyeccion {
    pub centro_id: String,
    pub vistas: Vec<VistaProyectada>,
    /// Aristas vigentes cargadas en Petgraph (de todos los centros activos)
    pub aristas_grafo: usize,
    /// Celdas vigentes cargadas en TableLookup (de todos los centros activos)
    pub celdas_lookup: usize,
}

/// Nombre de la vista de DataFusion de una tabla de un centro (prefijo de centro)
pub fn nombre_vista(centro_id: &CenterId, tabla: &str) -> String {
    format!("{}_{}", centro_id.as_str(), tabla)
}

/// Contrato de reconstrucción de las vistas derivadas de un centro
#[async_trait]
pub trait LakehouseProjectionPort: Send + Sync {
    /// Lee de LanceDB las filas `COMMITTED` de las 6 tablas y las registra en DataFusion con
    /// prefijo de centro; recarga Petgraph y TableLookup con lo vigente de los centros
    /// proyectados y exporta el inventario a `glue_catalog/`.
    async fn reconstruir_vistas(
        &self,
        centro: &CentroAbierto,
    ) -> Result<ResumenProyeccion, DomainError>;

    /// Desregistra todas las vistas del centro (cero acceso) sin tocar el disco
    async fn retirar_vistas(&self, centro_id: &CenterId) -> Result<(), DomainError>;

    /// Centros con vistas registradas ahora mismo
    async fn centros_proyectados(&self) -> Vec<CenterId>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nombre_vista_lleva_prefijo_de_centro() {
        let centro = CenterId::new("Obra-A").unwrap();
        assert_eq!(
            nombre_vista(&centro, "03_matriz_operativa"),
            "obra-a_03_matriz_operativa"
        );
    }
}
