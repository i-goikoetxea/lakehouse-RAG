//! Puerto de Repositorio para la Gestión y Registro de Centros de Conocimiento
//!
//! El adaptador real persiste `centers_registry.json` en el directorio de configuración de la
//! app (especificación §3.1). Eliminar un centro solo lo quita del registro: nunca borra el disco.

use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::domain::models::center::{CenterId, CentroConocimiento};

/// Contrato para adaptadores que persisten y consultan el registro maestro de centros
#[async_trait]
pub trait CenterRegistryPort: Send + Sync {
    /// Registra un nuevo centro de conocimiento en el catálogo
    async fn registrar(&self, centro: &CentroConocimiento) -> Result<(), DomainError>;

    /// Obtiene un centro por su identificador único
    async fn obtener(&self, id: &CenterId) -> Result<Option<CentroConocimiento>, DomainError>;

    /// Lista todos los centros registrados en el sistema
    async fn listar_todos(&self) -> Result<Vec<CentroConocimiento>, DomainError>;

    /// Lista únicamente los centros que están marcados como activos
    async fn listar_activos(&self) -> Result<Vec<CentroConocimiento>, DomainError>;

    /// Modifica el estado activo/inactivo de un centro existente
    async fn cambiar_estado(&self, id: &CenterId, activo: bool) -> Result<(), DomainError>;

    /// Quita el centro del registro (sin tocar su directorio) y lo devuelve
    async fn eliminar(&self, id: &CenterId) -> Result<CentroConocimiento, DomainError>;
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::domain::models::center::CenterManifest;

    /// Implementación de prueba en memoria para verificar el contrato del puerto
    struct InMemoryCenterRegistry {
        centros: Mutex<HashMap<CenterId, CentroConocimiento>>,
    }

    impl InMemoryCenterRegistry {
        fn new() -> Self {
            Self {
                centros: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl CenterRegistryPort for InMemoryCenterRegistry {
        async fn registrar(&self, centro: &CentroConocimiento) -> Result<(), DomainError> {
            let mut lock = self.centros.lock().unwrap();
            if lock.contains_key(centro.id()) {
                return Err(DomainError::ValidationError(format!(
                    "El centro '{}' ya se encuentra registrado",
                    centro.id()
                )));
            }
            lock.insert(centro.id().clone(), centro.clone());
            Ok(())
        }

        async fn obtener(&self, id: &CenterId) -> Result<Option<CentroConocimiento>, DomainError> {
            let lock = self.centros.lock().unwrap();
            Ok(lock.get(id).cloned())
        }

        async fn listar_todos(&self) -> Result<Vec<CentroConocimiento>, DomainError> {
            let lock = self.centros.lock().unwrap();
            Ok(lock.values().cloned().collect())
        }

        async fn listar_activos(&self) -> Result<Vec<CentroConocimiento>, DomainError> {
            let lock = self.centros.lock().unwrap();
            Ok(lock.values().filter(|c| c.esta_activo()).cloned().collect())
        }

        async fn cambiar_estado(&self, id: &CenterId, activo: bool) -> Result<(), DomainError> {
            let mut lock = self.centros.lock().unwrap();
            if let Some(centro) = lock.get_mut(id) {
                if activo {
                    centro.activar();
                } else {
                    centro.desactivar();
                }
                Ok(())
            } else {
                Err(DomainError::InvalidCenterId(format!(
                    "Centro no encontrado: '{id}'"
                )))
            }
        }

        async fn eliminar(&self, id: &CenterId) -> Result<CentroConocimiento, DomainError> {
            self.centros.lock().unwrap().remove(id).ok_or_else(|| {
                DomainError::InvalidCenterId(format!("Centro no encontrado: '{id}'"))
            })
        }
    }

    #[tokio::test]
    async fn test_in_memory_registry_crud_ciclo_completo() {
        let registry = InMemoryCenterRegistry::new();
        let id1 = CenterId::new("planta-norte").unwrap();
        let manifest1 = CenterManifest::new(
            id1.clone(),
            "Planta Norte",
            "Operaciones Norte",
            "data/centers/norte",
        )
        .unwrap();
        let centro1 = CentroConocimiento::new(manifest1);

        let id2 = CenterId::new("planta-sur").unwrap();
        let manifest2 = CenterManifest::new(
            id2.clone(),
            "Planta Sur",
            "Operaciones Sur",
            "data/centers/sur",
        )
        .unwrap();
        let mut centro2 = CentroConocimiento::new(manifest2);
        centro2.desactivar();

        // 1. Registro
        registry.registrar(&centro1).await.expect("Debe registrar");
        registry.registrar(&centro2).await.expect("Debe registrar");

        // Registro duplicado falla
        assert!(registry.registrar(&centro1).await.is_err());

        // 2. Obtener
        let obtenido = registry.obtener(&id1).await.unwrap();
        assert!(obtenido.is_some());
        assert_eq!(obtenido.unwrap().id(), &id1);

        // 3. Listar todos
        let todos = registry.listar_todos().await.unwrap();
        assert_eq!(todos.len(), 2);

        // 4. Listar activos
        let activos = registry.listar_activos().await.unwrap();
        assert_eq!(activos.len(), 1);
        assert_eq!(activos[0].id(), &id1);

        // 5. Cambiar estado
        registry.cambiar_estado(&id2, true).await.unwrap();
        let activos_tras_activacion = registry.listar_activos().await.unwrap();
        assert_eq!(activos_tras_activacion.len(), 2);

        // 6. Eliminar del registro
        assert_eq!(registry.eliminar(&id2).await.unwrap().id(), &id2);
        assert!(registry.obtener(&id2).await.unwrap().is_none());
        assert!(registry.eliminar(&id2).await.is_err());
    }
}
