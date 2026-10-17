//! Adaptador del registro maestro de centros en `centers_registry.json` (especificación §3.1).
//!
//! * El directorio de configuración se inyecta (tests) o se resuelve con
//!   [`directorio_configuracion_efectivo`] (`LAKEHOUSE_RAG_CONFIG_DIR` > `%APPDATA%` / XDG).
//! * Escritura atómica: archivo temporal + `rename`.
//! * Un registro corrupto nunca se sobrescribe: se devuelve un error con la ruta.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::application::dtos::centros::VERSION_ESQUEMA_CENTRO;
use crate::application::ports::center_registry::CenterRegistryPort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::{CenterId, CenterManifest, CentroConocimiento};

pub const ARCHIVO_REGISTRO: &str = "centers_registry.json";
pub const VARIABLE_DIRECTORIO_CONFIG: &str = "LAKEHOUSE_RAG_CONFIG_DIR";
const SUBDIRECTORIO_APP: &str = "lakehouse_rag";
/// Tope defensivo del tamaño del registro (un registro legítimo ocupa unos KB)
const TAMANO_MAXIMO_REGISTRO: u64 = 16 * 1024 * 1024;

/// Resuelve el directorio de configuración: variable de entorno > directorio de usuario
pub fn resolver_directorio_configuracion(
    variable_entorno: Option<std::ffi::OsString>,
    directorio_usuario: Option<PathBuf>,
) -> PathBuf {
    if let Some(valor) = variable_entorno.filter(|v| !v.is_empty()) {
        return PathBuf::from(valor);
    }
    directorio_usuario
        .unwrap_or_else(std::env::temp_dir)
        .join(SUBDIRECTORIO_APP)
}

/// Directorio de configuración efectivo con el entorno del proceso
pub fn directorio_configuracion_efectivo() -> PathBuf {
    resolver_directorio_configuracion(
        std::env::var_os(VARIABLE_DIRECTORIO_CONFIG),
        directorio_configuracion_usuario(),
    )
}

fn directorio_configuracion_usuario() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").map(PathBuf::from);
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct EstadisticasCentro {
    #[serde(default)]
    documentos_procesados: u64,
    #[serde(default)]
    tuplas_operativas: u64,
    #[serde(default)]
    tablas_doble_entrada: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntradaRegistro {
    id: String,
    nombre: String,
    ruta_disco: String,
    activo: bool,
    fecha_creacion: DateTime<Utc>,
    #[serde(default)]
    estadisticas: EstadisticasCentro,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ArchivoRegistro {
    centros: Vec<EntradaRegistro>,
}

impl EntradaRegistro {
    fn desde_centro(centro: &CentroConocimiento) -> Self {
        let m = centro.manifiesto();
        Self {
            id: m.id.to_string(),
            nombre: m.nombre.clone(),
            ruta_disco: m.ruta_almacenamiento.clone(),
            activo: m.activo,
            fecha_creacion: m.fecha_creacion,
            estadisticas: EstadisticasCentro::default(),
        }
    }

    fn a_centro(&self) -> Result<CentroConocimiento, DomainError> {
        Ok(CentroConocimiento::new(CenterManifest {
            id: CenterId::new(&self.id)?,
            nombre: self.nombre.clone(),
            descripcion: String::new(),
            ruta_almacenamiento: self.ruta_disco.clone(),
            activo: self.activo,
            version: VERSION_ESQUEMA_CENTRO,
            fecha_creacion: self.fecha_creacion,
        }))
    }
}

/// Registro de centros persistido en JSON
pub struct JsonCenterRegistry {
    ruta_archivo: PathBuf,
    escritura: Mutex<()>,
}

impl JsonCenterRegistry {
    /// Registro en `<directorio_config>/centers_registry.json` (el directorio se crea al escribir)
    pub fn new(directorio_config: impl AsRef<Path>) -> Self {
        Self {
            ruta_archivo: directorio_config.as_ref().join(ARCHIVO_REGISTRO),
            escritura: Mutex::new(()),
        }
    }

    pub fn ruta_archivo(&self) -> &Path {
        &self.ruta_archivo
    }

    fn leer(&self) -> Result<ArchivoRegistro, DomainError> {
        let metadatos = match std::fs::metadata(&self.ruta_archivo) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ArchivoRegistro::default());
            }
            Err(e) => return Err(self.error_io("leer", e)),
        };
        if metadatos.len() > TAMANO_MAXIMO_REGISTRO {
            return Err(self.error_corrupto("supera el tamaño máximo admitido"));
        }
        let texto =
            std::fs::read_to_string(&self.ruta_archivo).map_err(|e| self.error_io("leer", e))?;
        if texto.trim().is_empty() {
            return Ok(ArchivoRegistro::default());
        }
        let archivo: ArchivoRegistro =
            serde_json::from_str(&texto).map_err(|e| self.error_corrupto(&e.to_string()))?;
        for entrada in &archivo.centros {
            CenterId::new(&entrada.id)
                .map_err(|e| self.error_corrupto(&format!("entrada inválida: {e}")))?;
        }
        Ok(archivo)
    }

    fn escribir(&self, archivo: &ArchivoRegistro) -> Result<(), DomainError> {
        if let Some(padre) = self.ruta_archivo.parent() {
            std::fs::create_dir_all(padre).map_err(|e| self.error_io("crear directorio", e))?;
        }
        let json = serde_json::to_string_pretty(archivo)
            .map_err(|e| DomainError::StorageError(format!("Error serializando registro: {e}")))?;
        let temporal = self.ruta_archivo.with_extension("json.tmp");
        std::fs::write(&temporal, json).map_err(|e| self.error_io("escribir", e))?;
        std::fs::rename(&temporal, &self.ruta_archivo).map_err(|e| self.error_io("renombrar", e))
    }

    fn error_io(&self, operacion: &str, e: std::io::Error) -> DomainError {
        DomainError::StorageError(format!(
            "No se pudo {operacion} el registro de centros '{}': {e}",
            self.ruta_archivo.display()
        ))
    }

    fn error_corrupto(&self, detalle: &str) -> DomainError {
        DomainError::StorageError(format!(
            "El registro de centros '{}' está corrupto ({detalle}). No se ha modificado: \
             corrígelo o muévelo para empezar con un registro vacío.",
            self.ruta_archivo.display()
        ))
    }

    async fn modificar<T>(
        &self,
        cambio: impl FnOnce(&mut ArchivoRegistro) -> Result<T, DomainError>,
    ) -> Result<T, DomainError> {
        let _guardia = self.escritura.lock().await;
        let mut archivo = self.leer()?;
        let resultado = cambio(&mut archivo)?;
        self.escribir(&archivo)?;
        Ok(resultado)
    }
}

fn no_encontrado(id: &CenterId) -> DomainError {
    DomainError::InvalidCenterId(format!("Centro no encontrado en el registro: '{id}'"))
}

#[async_trait]
impl CenterRegistryPort for JsonCenterRegistry {
    async fn registrar(&self, centro: &CentroConocimiento) -> Result<(), DomainError> {
        self.modificar(|archivo| {
            if archivo.centros.iter().any(|c| c.id == centro.id().as_str()) {
                return Err(DomainError::ValidationError(format!(
                    "El centro '{}' ya se encuentra registrado",
                    centro.id()
                )));
            }
            archivo.centros.push(EntradaRegistro::desde_centro(centro));
            Ok(())
        })
        .await
    }

    async fn obtener(&self, id: &CenterId) -> Result<Option<CentroConocimiento>, DomainError> {
        let archivo = self.leer()?;
        archivo
            .centros
            .iter()
            .find(|c| c.id == id.as_str())
            .map(EntradaRegistro::a_centro)
            .transpose()
    }

    async fn listar_todos(&self) -> Result<Vec<CentroConocimiento>, DomainError> {
        self.leer()?
            .centros
            .iter()
            .map(EntradaRegistro::a_centro)
            .collect()
    }

    async fn listar_activos(&self) -> Result<Vec<CentroConocimiento>, DomainError> {
        Ok(self
            .listar_todos()
            .await?
            .into_iter()
            .filter(CentroConocimiento::esta_activo)
            .collect())
    }

    async fn cambiar_estado(&self, id: &CenterId, activo: bool) -> Result<(), DomainError> {
        self.modificar(|archivo| {
            let entrada = archivo
                .centros
                .iter_mut()
                .find(|c| c.id == id.as_str())
                .ok_or_else(|| no_encontrado(id))?;
            entrada.activo = activo;
            Ok(())
        })
        .await
    }

    async fn eliminar(&self, id: &CenterId) -> Result<CentroConocimiento, DomainError> {
        self.modificar(|archivo| {
            let posicion = archivo
                .centros
                .iter()
                .position(|c| c.id == id.as_str())
                .ok_or_else(|| no_encontrado(id))?;
            archivo.centros.remove(posicion).a_centro()
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centro(id: &str, ruta: &str) -> CentroConocimiento {
        let manifiesto =
            CenterManifest::new(CenterId::new(id).unwrap(), id, "", ruta).expect("manifiesto");
        CentroConocimiento::new(manifiesto)
    }

    #[test]
    fn test_directorio_configuracion_prioriza_variable_de_entorno() {
        let elegido = resolver_directorio_configuracion(
            Some("D:/config_test".into()),
            Some(PathBuf::from("C:/Users/x/AppData/Roaming")),
        );
        assert_eq!(elegido, PathBuf::from("D:/config_test"));
        let por_defecto =
            resolver_directorio_configuracion(None, Some(PathBuf::from("C:/AppData")));
        assert_eq!(
            por_defecto,
            PathBuf::from("C:/AppData").join("lakehouse_rag")
        );
    }

    #[tokio::test]
    async fn test_registro_persiste_en_json_con_formato_de_la_especificacion() {
        let dir = tempfile::tempdir().unwrap();
        let registro = JsonCenterRegistry::new(dir.path());
        registro.registrar(&centro("obra-a", "D:/a")).await.unwrap();
        registro
            .cambiar_estado(&CenterId::new("obra-a").unwrap(), false)
            .await
            .unwrap();

        let texto = std::fs::read_to_string(dir.path().join(ARCHIVO_REGISTRO)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&texto).unwrap();
        let entrada = &json["centros"][0];
        assert_eq!(entrada["id"], "obra-a");
        assert_eq!(entrada["ruta_disco"], "D:/a");
        assert_eq!(entrada["activo"], false);
        assert!(entrada["fecha_creacion"].is_string());

        // Una instancia nueva (otro proceso) ve el mismo estado
        let otra = JsonCenterRegistry::new(dir.path());
        let todos = otra.listar_todos().await.unwrap();
        assert_eq!(todos.len(), 1);
        assert!(!todos[0].esta_activo());
        assert!(otra.listar_activos().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_eliminar_solo_quita_la_entrada() {
        let dir = tempfile::tempdir().unwrap();
        let registro = JsonCenterRegistry::new(dir.path());
        registro.registrar(&centro("obra-a", "D:/a")).await.unwrap();
        registro.registrar(&centro("obra-b", "D:/b")).await.unwrap();
        let id = CenterId::new("obra-a").unwrap();
        assert_eq!(registro.eliminar(&id).await.unwrap().id(), &id);
        assert!(registro.obtener(&id).await.unwrap().is_none());
        assert!(registro.eliminar(&id).await.is_err());
        assert_eq!(registro.listar_todos().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_registro_corrupto_da_error_y_no_se_sobrescribe() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = dir.path().join(ARCHIVO_REGISTRO);
        std::fs::write(&ruta, "{ esto no es json").unwrap();
        let registro = JsonCenterRegistry::new(dir.path());

        let error = registro.listar_todos().await.unwrap_err().to_string();
        assert!(error.contains("corrupto"), "{error}");
        assert!(registro.registrar(&centro("obra-a", "D:/a")).await.is_err());
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "{ esto no es json");
    }
}
