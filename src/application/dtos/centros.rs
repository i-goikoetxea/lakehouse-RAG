//! DTOs de los centros de conocimiento en disco (especificación §3, plan v2 Fase 12).
//!
//! Tipos puros: el manifiesto del centro, el modelo de embeddings activo y la validación
//! léxica de rutas (sin tocar el disco).

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;

/// Versión del esquema de tablas que escribe esta build (`center_manifest.json`)
pub const VERSION_ESQUEMA_CENTRO: u32 = 2;

/// Nombre del manifiesto en la raíz de cada centro
pub const ARCHIVO_MANIFIESTO: &str = "center_manifest.json";
/// Subdirectorios del centro (especificación §3.2)
pub const DIRECTORIO_RAW_DOCS: &str = "raw_docs";
pub const DIRECTORIO_GLUE_CATALOG: &str = "glue_catalog";
pub const DIRECTORIO_LAKEHOUSE_DATA: &str = "lakehouse_data";

/// Modelo de embeddings con el que trabaja la aplicación (no se mezclan espacios vectoriales)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeloEmbeddingsActivo {
    pub nombre: String,
    pub dimension: usize,
}

impl ModeloEmbeddingsActivo {
    pub fn new(nombre: impl Into<String>, dimension: usize) -> Self {
        Self {
            nombre: nombre.into(),
            dimension,
        }
    }
}

/// Contenido de `center_manifest.json`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifiestoCentroDisco {
    pub schema_version: u32,
    pub centro_id: String,
    pub nombre: String,
    pub modelo_embeddings: String,
    pub dimension_embeddings: usize,
    pub fecha_creacion: String,
}

impl ManifiestoCentroDisco {
    pub fn nuevo(
        centro_id: &str,
        nombre: &str,
        modelo: &ModeloEmbeddingsActivo,
        fecha_creacion: String,
    ) -> Self {
        Self {
            schema_version: VERSION_ESQUEMA_CENTRO,
            centro_id: centro_id.to_string(),
            nombre: nombre.to_string(),
            modelo_embeddings: modelo.nombre.clone(),
            dimension_embeddings: modelo.dimension,
            fecha_creacion,
        }
    }

    /// Un centro solo se abre con la misma versión de esquema y el mismo modelo de embeddings
    pub fn validar_compatibilidad(
        &self,
        modelo_activo: &ModeloEmbeddingsActivo,
    ) -> Result<(), DomainError> {
        if self.schema_version != VERSION_ESQUEMA_CENTRO {
            return Err(DomainError::ValidationError(format!(
                "El centro '{}' usa schema_version = {} y esta aplicación solo abre la versión {}",
                self.centro_id, self.schema_version, VERSION_ESQUEMA_CENTRO
            )));
        }
        if self.modelo_embeddings != modelo_activo.nombre
            || self.dimension_embeddings != modelo_activo.dimension
        {
            return Err(DomainError::ValidationError(format!(
                "El centro '{}' se indexó con el modelo de embeddings '{}' ({} dims) y el modelo \
                 activo es '{}' ({} dims): sus vectores no son comparables. Reingiere el centro \
                 con el modelo activo o arranca la aplicación con el modelo del manifiesto.",
                self.centro_id,
                self.modelo_embeddings,
                self.dimension_embeddings,
                modelo_activo.nombre,
                modelo_activo.dimension
            )));
        }
        Ok(())
    }
}

/// Copia inmutable de un original en `raw_docs/`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentoOriginal {
    pub nombre_archivo: String,
    pub contenido: String,
    pub sha256: String,
    pub ruta_copia: PathBuf,
}

/// Valida léxicamente la ruta de un centro: no vacía, sin componentes `..` y sin `\0`
pub fn validar_ruta_centro(ruta: &str) -> Result<PathBuf, DomainError> {
    let limpia = ruta.trim();
    if limpia.is_empty() {
        return Err(DomainError::ValidationError(
            "La ruta del centro no puede estar vacía".to_string(),
        ));
    }
    if limpia.contains('\0') {
        return Err(DomainError::ValidationError(
            "La ruta del centro contiene un carácter nulo".to_string(),
        ));
    }
    let camino = Path::new(limpia);
    if camino
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(DomainError::ValidationError(format!(
            "La ruta del centro '{limpia}' contiene '..': indica una ruta directa, sin saltos al directorio padre"
        )));
    }
    Ok(camino.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e5() -> ModeloEmbeddingsActivo {
        ModeloEmbeddingsActivo::new("MultilingualE5Large", 1024)
    }

    #[test]
    fn test_manifiesto_nuevo_usa_schema_version_2() {
        let m = ManifiestoCentroDisco::nuevo("obra-a", "Obra A", &e5(), "2026-10-17".into());
        assert_eq!(m.schema_version, 2);
        assert_eq!(m.modelo_embeddings, "MultilingualE5Large");
        assert_eq!(m.dimension_embeddings, 1024);
        assert!(m.validar_compatibilidad(&e5()).is_ok());
    }

    #[test]
    fn test_manifiesto_de_otro_modelo_se_rechaza_con_mensaje_claro() {
        let bge = ModeloEmbeddingsActivo::new("BGELargeENV15", 1024);
        let m = ManifiestoCentroDisco::nuevo("obra-a", "Obra A", &bge, "2026-10-17".into());
        let error = m.validar_compatibilidad(&e5()).unwrap_err().to_string();
        assert!(error.contains("BGELargeENV15"), "{error}");
        assert!(error.contains("MultilingualE5Large"), "{error}");
        assert!(error.contains("no son comparables"), "{error}");
    }

    #[test]
    fn test_manifiesto_de_otra_version_de_esquema_se_rechaza() {
        let mut m = ManifiestoCentroDisco::nuevo("obra-a", "Obra A", &e5(), "x".into());
        m.schema_version = 1;
        let error = m.validar_compatibilidad(&e5()).unwrap_err().to_string();
        assert!(error.contains("schema_version = 1"), "{error}");
    }

    #[test]
    fn test_ruta_con_directorio_padre_se_rechaza() {
        for ruta in ["../fuera", "datos/../../etc", "C:\\datos\\..\\x", "  "] {
            assert!(validar_ruta_centro(ruta).is_err(), "{ruta}");
        }
        assert!(validar_ruta_centro("D:/datos/obra_a").is_ok());
        assert!(validar_ruta_centro("relativa/obra..a").is_ok());
    }
}
