//! Adaptador de la estructura física de un centro (especificación §3.2):
//!
//! ```text
//! <ruta_disco>/
//! ├── center_manifest.json   (schema_version = 2, modelo y dimensión de embeddings)
//! ├── raw_docs/              (<sha256>_<nombre> + <sha256>_<nombre>.sha256)
//! ├── glue_catalog/          (inventario de tablas exportado por la proyección)
//! └── lakehouse_data/        (las 6 tablas .lance)
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::application::dtos::centros::{
    ARCHIVO_MANIFIESTO, DIRECTORIO_GLUE_CATALOG, DIRECTORIO_LAKEHOUSE_DATA, DIRECTORIO_RAW_DOCS,
    DocumentoOriginal, ManifiestoCentroDisco, ModeloEmbeddingsActivo,
};
use crate::application::ports::center_workspace::{CenterWorkspacePort, CentroAbierto};
use crate::application::ports::lakehouse_store::LakehouseStorePort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::infrastructure::storage::lancedb_store::LanceDbStore;

/// Tamaño máximo de un original admitido en `raw_docs/`
pub const TAMANO_MAXIMO_ORIGINAL: u64 = 64 * 1024 * 1024;
/// Tamaño máximo del manifiesto (un manifiesto legítimo ocupa < 1 KB)
const TAMANO_MAXIMO_MANIFIESTO: u64 = 64 * 1024;

/// Espacio de centros en el sistema de archivos local
#[derive(Debug, Default, Clone)]
pub struct DiskCenterWorkspace;

impl DiskCenterWorkspace {
    pub fn new() -> Self {
        Self
    }
}

fn error_io(contexto: &str, ruta: &Path, e: std::io::Error) -> DomainError {
    DomainError::StorageError(format!("{contexto} '{}': {e}", ruta.display()))
}

fn leer_manifiesto(ruta_centro: &Path) -> Result<Option<ManifiestoCentroDisco>, DomainError> {
    let ruta = ruta_centro.join(ARCHIVO_MANIFIESTO);
    let metadatos = match std::fs::metadata(&ruta) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(error_io("No se pudo leer el manifiesto", &ruta, e)),
    };
    if metadatos.len() > TAMANO_MAXIMO_MANIFIESTO {
        return Err(DomainError::StorageError(format!(
            "El manifiesto '{}' supera el tamaño máximo admitido",
            ruta.display()
        )));
    }
    let texto = std::fs::read_to_string(&ruta)
        .map_err(|e| error_io("No se pudo leer el manifiesto", &ruta, e))?;
    serde_json::from_str(&texto).map(Some).map_err(|e| {
        DomainError::StorageError(format!(
            "El manifiesto '{}' está corrupto: {e}",
            ruta.display()
        ))
    })
}

fn escribir_atomico(destino: &Path, contenido: &[u8]) -> Result<(), DomainError> {
    let temporal = destino.with_extension("tmp");
    std::fs::write(&temporal, contenido)
        .map_err(|e| error_io("No se pudo escribir", &temporal, e))?;
    std::fs::rename(&temporal, destino).map_err(|e| error_io("No se pudo renombrar", destino, e))
}

fn directorio_vacio_o_inexistente(ruta: &Path) -> Result<bool, DomainError> {
    match std::fs::read_dir(ruta) {
        Ok(mut entradas) => Ok(entradas.next().is_none()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(error_io("No se pudo inspeccionar", ruta, e)),
    }
}

/// Nombre de archivo seguro: solo `[A-Za-z0-9._-]`, sin separadores ni `..`
fn nombre_seguro(nombre: &str) -> String {
    let limpio: String = nombre
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let limpio = limpio.trim_matches('.').replace("..", "_");
    if limpio.is_empty() {
        "documento".to_string()
    } else {
        limpio.chars().take(120).collect()
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[async_trait]
impl CenterWorkspacePort for DiskCenterWorkspace {
    fn ruta_absoluta(&self, ruta: &Path) -> Result<PathBuf, DomainError> {
        std::path::absolute(ruta).map_err(|e| error_io("Ruta de centro inválida", ruta, e))
    }

    async fn preparar(
        &self,
        ruta: &Path,
        manifiesto: &ManifiestoCentroDisco,
        modelo_activo: &ModeloEmbeddingsActivo,
    ) -> Result<ManifiestoCentroDisco, DomainError> {
        if ruta.exists() && !ruta.is_dir() {
            return Err(DomainError::ValidationError(format!(
                "La ruta del centro '{}' existe y no es un directorio",
                ruta.display()
            )));
        }
        let existente = leer_manifiesto(ruta)?;
        if existente.is_none() && !directorio_vacio_o_inexistente(ruta)? {
            return Err(DomainError::ValidationError(format!(
                "El directorio '{}' no está vacío y no contiene un {ARCHIVO_MANIFIESTO}: elige un \
                 directorio vacío o uno que ya sea un centro",
                ruta.display()
            )));
        }
        if let Some(previo) = &existente {
            previo.validar_compatibilidad(modelo_activo)?;
        }

        for subdirectorio in [
            DIRECTORIO_RAW_DOCS,
            DIRECTORIO_GLUE_CATALOG,
            DIRECTORIO_LAKEHOUSE_DATA,
        ] {
            let destino = ruta.join(subdirectorio);
            std::fs::create_dir_all(&destino)
                .map_err(|e| error_io("No se pudo crear", &destino, e))?;
        }

        let manifiesto = match existente {
            Some(previo) => previo,
            None => {
                let json = serde_json::to_vec_pretty(manifiesto).map_err(|e| {
                    DomainError::StorageError(format!("Error serializando el manifiesto: {e}"))
                })?;
                escribir_atomico(&ruta.join(ARCHIVO_MANIFIESTO), &json)?;
                manifiesto.clone()
            }
        };

        let store = LanceDbStore::conectar(ruta.join(DIRECTORIO_LAKEHOUSE_DATA)).await?;
        store.inicializar_tablas().await?;
        Ok(manifiesto)
    }

    async fn abrir(
        &self,
        id: &CenterId,
        ruta: &Path,
        modelo_activo: &ModeloEmbeddingsActivo,
    ) -> Result<CentroAbierto, DomainError> {
        if !ruta.is_dir() {
            return Err(DomainError::StorageError(format!(
                "El directorio del centro '{id}' no existe: '{}'",
                ruta.display()
            )));
        }
        let manifiesto = leer_manifiesto(ruta)?.ok_or_else(|| {
            DomainError::StorageError(format!(
                "'{}' no es un centro: falta {ARCHIVO_MANIFIESTO}",
                ruta.display()
            ))
        })?;
        if manifiesto.centro_id != id.as_str() {
            return Err(DomainError::ValidationError(format!(
                "El manifiesto de '{}' pertenece al centro '{}', no a '{id}'",
                ruta.display(),
                manifiesto.centro_id
            )));
        }
        manifiesto.validar_compatibilidad(modelo_activo)?;

        let store = LanceDbStore::conectar(ruta.join(DIRECTORIO_LAKEHOUSE_DATA)).await?;
        Ok(CentroAbierto {
            id: id.clone(),
            ruta: ruta.to_path_buf(),
            manifiesto,
            store: Arc::new(store),
        })
    }

    async fn guardar_original(
        &self,
        ruta_centro: &Path,
        ruta_archivo: &Path,
    ) -> Result<DocumentoOriginal, DomainError> {
        let metadatos = std::fs::metadata(ruta_archivo)
            .map_err(|e| error_io("No se pudo abrir el documento", ruta_archivo, e))?;
        if !metadatos.is_file() {
            return Err(DomainError::IngestionError(format!(
                "'{}' no es un archivo",
                ruta_archivo.display()
            )));
        }
        if metadatos.len() > TAMANO_MAXIMO_ORIGINAL {
            return Err(DomainError::IngestionError(format!(
                "'{}' supera el tamaño máximo de {TAMANO_MAXIMO_ORIGINAL} bytes",
                ruta_archivo.display()
            )));
        }
        let bytes = std::fs::read(ruta_archivo)
            .map_err(|e| error_io("No se pudo leer el documento", ruta_archivo, e))?;
        let contenido = String::from_utf8(bytes).map_err(|_| {
            DomainError::IngestionError(format!(
                "'{}' no es texto UTF-8 (por ahora solo se ingiere Markdown)",
                ruta_archivo.display()
            ))
        })?;

        let sha256 = sha256_hex(contenido.as_bytes());
        let nombre_archivo = ruta_archivo
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "documento".to_string());
        let raw_docs = ruta_centro.join(DIRECTORIO_RAW_DOCS);
        std::fs::create_dir_all(&raw_docs)
            .map_err(|e| error_io("No se pudo crear", &raw_docs, e))?;
        let nombre_copia = format!("{sha256}_{}", nombre_seguro(&nombre_archivo));
        let ruta_copia = raw_docs.join(&nombre_copia);

        // Copia inmutable: si ya existe con ese hash, no se reescribe
        if !ruta_copia.exists() {
            escribir_atomico(&ruta_copia, contenido.as_bytes())?;
            let huella = format!("{sha256}  {nombre_archivo}\n");
            escribir_atomico(
                &raw_docs.join(format!("{nombre_copia}.sha256")),
                huella.as_bytes(),
            )?;
        }

        Ok(DocumentoOriginal {
            nombre_archivo,
            contenido,
            sha256,
            ruta_copia,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nombre_seguro_neutraliza_separadores_y_saltos() {
        assert_eq!(nombre_seguro("rev03.md"), "rev03.md");
        assert_eq!(nombre_seguro("../../etc/passwd"), "___etc_passwd");
        assert!(!nombre_seguro("a/../b").contains(".."));
        assert_eq!(nombre_seguro("..."), "documento");
        assert_eq!(nombre_seguro("recepción v2.md"), "recepci_n_v2.md");
    }

    #[test]
    fn test_sha256_hex_es_el_del_contenido() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
