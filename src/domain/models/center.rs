//! Entidad de Centro de Conocimiento y sus Value Objects
//!
//! Representa un centro industrial independiente (e.g., planta, refinería, terminal)
//! que aísla sus propias normas, matrices operacionales y tablas.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;

/// Identificador inmutable y validado de un Centro de Conocimiento
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CenterId(String);

impl CenterId {
    pub fn new(id: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = id.into().trim().to_lowercase();
        if trimmed.is_empty() {
            return Err(DomainError::InvalidCenterId(
                "El identificador no puede estar vacío".to_string(),
            ));
        }

        // Solo caracteres alfanuméricos, guiones y guiones bajos
        let is_valid = trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');

        if !is_valid {
            return Err(DomainError::InvalidCenterId(format!(
                "El identificador '{trimmed}' contiene caracteres inválidos. Solo se permiten letras, números, guiones y guiones bajos."
            )));
        }

        Ok(Self(trimmed))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CenterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Deref for CenterId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Manifiesto con los metadatos de configuración y almacenamiento de un centro
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CenterManifest {
    pub id: CenterId,
    pub nombre: String,
    pub descripcion: String,
    pub ruta_almacenamiento: String,
    pub activo: bool,
    pub version: u32,
    pub fecha_creacion: DateTime<Utc>,
}

impl CenterManifest {
    pub fn new(
        id: CenterId,
        nombre: impl Into<String>,
        descripcion: impl Into<String>,
        ruta_almacenamiento: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let nombre = nombre.into().trim().to_string();
        if nombre.is_empty() {
            return Err(DomainError::ValidationError(
                "El nombre del centro no puede estar vacío".to_string(),
            ));
        }

        let ruta = ruta_almacenamiento.into().trim().to_string();
        if ruta.is_empty() {
            return Err(DomainError::ValidationError(
                "La ruta de almacenamiento no puede estar vacía".to_string(),
            ));
        }

        Ok(Self {
            id,
            nombre,
            descripcion: descripcion.into().trim().to_string(),
            ruta_almacenamiento: ruta,
            activo: true,
            version: 1,
            fecha_creacion: Utc::now(),
        })
    }
}

/// Entidad raíz que gestiona el ciclo de vida del Centro de Conocimiento
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CentroConocimiento {
    manifiesto: CenterManifest,
}

impl CentroConocimiento {
    pub fn new(manifiesto: CenterManifest) -> Self {
        Self { manifiesto }
    }

    pub fn id(&self) -> &CenterId {
        &self.manifiesto.id
    }

    pub fn manifiesto(&self) -> &CenterManifest {
        &self.manifiesto
    }

    pub fn esta_activo(&self) -> bool {
        self.manifiesto.activo
    }

    pub fn activar(&mut self) {
        self.manifiesto.activo = true;
    }

    pub fn desactivar(&mut self) {
        self.manifiesto.activo = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_center_id_valido() {
        let center_id = CenterId::new("planta-norte_01").expect("Debe ser válido");
        assert_eq!(center_id.as_str(), "planta-norte_01");
        assert_eq!(center_id.to_string(), "planta-norte_01");
    }

    #[test]
    fn test_center_id_invalido_vacio() {
        let err = CenterId::new("   ").unwrap_err();
        match err {
            DomainError::InvalidCenterId(msg) => {
                assert!(msg.contains("no puede estar vacío"));
            }
            other => panic!("Error inesperado: {other:?}"),
        }
    }

    #[test]
    fn test_center_id_invalido_caracteres_especiales() {
        let err = CenterId::new("centro/invalido!").unwrap_err();
        match err {
            DomainError::InvalidCenterId(msg) => {
                assert!(msg.contains("caracteres inválidos"));
            }
            other => panic!("Error inesperado: {other:?}"),
        }
    }

    #[test]
    fn test_creacion_manifiesto_y_centro() {
        let id = CenterId::new("refineria-central").unwrap();
        let manifest = CenterManifest::new(
            id.clone(),
            "Refinería Central",
            "Centro para procesamiento de hidrocarburos",
            "data/centers/refineria-central",
        )
        .expect("Manifiesto válido");

        let mut centro = CentroConocimiento::new(manifest);
        assert_eq!(centro.id(), &id);
        assert!(centro.esta_activo());

        centro.desactivar();
        assert!(!centro.esta_activo());

        centro.activar();
        assert!(centro.esta_activo());
    }

    #[test]
    fn test_manifiesto_invalido_nombre_vacio() {
        let id = CenterId::new("centro-test").unwrap();
        let err = CenterManifest::new(id, "", "desc", "path").unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }
}
