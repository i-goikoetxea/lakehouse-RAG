//! Adaptador de embeddings locales utilizando FastEmbed (ONNX Runtime).
//!
//! Modelo por defecto: `MultilingualE5Large` (1024 dims). Es el único modelo multilingüe de
//! 1024 dimensiones que ofrece fastembed 4.9.1 (BGE-M3 no está en esa versión) y los
//! documentos están en español (O3). La familia E5 espera los prefijos `query: ` (consultas,
//! `generate_embedding`) y `passage: ` (documentos, `generate_batch_embeddings`).
//!
//! Caché del modelo (≈ 2,2 GB): parámetro explícito > variable `FASTEMBED_CACHE_DIR` >
//! directorio de datos de la app (`%LOCALAPPDATA%\lakehouse_rag\fastembed_cache` en Windows,
//! `$XDG_DATA_HOME` o `~/.local/share` en Unix). Nunca el directorio de trabajo.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::domain::errors::DomainError;

/// Modelo multilingüe de 1024 dimensiones usado si no se indica otro
pub const MODELO_POR_DEFECTO: EmbeddingModel = EmbeddingModel::MultilingualE5Large;
/// Variable de entorno que fija el directorio de caché del modelo
pub const VARIABLE_CACHE: &str = "FASTEMBED_CACHE_DIR";
/// Subdirectorio de la app dentro del directorio de datos del usuario
const SUBDIRECTORIO_APP: &str = "lakehouse_rag";
/// Subdirectorio de la caché de modelos dentro del directorio de la app
const SUBDIRECTORIO_CACHE: &str = "fastembed_cache";
/// Prefijo E5 para los textos de consulta
const PREFIJO_CONSULTA_E5: &str = "query: ";
/// Prefijo E5 para los pasajes indexados
const PREFIJO_PASAJE_E5: &str = "passage: ";

/// Directorio de datos del usuario según la plataforma (sin dependencias externas)
fn directorio_datos_usuario() -> Option<PathBuf> {
    let candidatas: &[&str] = if cfg!(windows) {
        &["LOCALAPPDATA", "APPDATA"]
    } else {
        &["XDG_DATA_HOME"]
    };
    candidatas
        .iter()
        .find_map(|var| std::env::var_os(var).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|home| PathBuf::from(home).join(".local").join("share"))
        })
}

/// Decide el directorio de caché: explícito > variable de entorno > datos de la app.
/// Sin directorio de datos se usa el temporal del sistema (nunca el cwd).
pub fn resolver_directorio_cache(
    explicito: Option<&Path>,
    variable_entorno: Option<OsString>,
    directorio_datos: Option<PathBuf>,
) -> PathBuf {
    if let Some(ruta) = explicito.filter(|r| !r.as_os_str().is_empty()) {
        return ruta.to_path_buf();
    }
    if let Some(valor) = variable_entorno.filter(|v| !v.is_empty()) {
        return PathBuf::from(valor);
    }
    directorio_datos
        .unwrap_or_else(std::env::temp_dir)
        .join(SUBDIRECTORIO_APP)
        .join(SUBDIRECTORIO_CACHE)
}

/// Directorio de caché efectivo con el entorno actual del proceso
pub fn directorio_cache_efectivo(explicito: Option<&Path>) -> PathBuf {
    resolver_directorio_cache(
        explicito,
        std::env::var_os(VARIABLE_CACHE),
        directorio_datos_usuario(),
    )
}

/// Dimensión del vector que produce cada modelo
pub fn dimension_del_modelo(modelo: &EmbeddingModel) -> usize {
    match modelo {
        EmbeddingModel::BGELargeENV15
        | EmbeddingModel::BGELargeENV15Q
        | EmbeddingModel::BGELargeZHV15
        | EmbeddingModel::MultilingualE5Large
        | EmbeddingModel::ModernBertEmbedLarge
        | EmbeddingModel::MxbaiEmbedLargeV1
        | EmbeddingModel::MxbaiEmbedLargeV1Q => 1024,
        EmbeddingModel::BGEBaseENV15
        | EmbeddingModel::BGEBaseENV15Q
        | EmbeddingModel::MultilingualE5Base
        | EmbeddingModel::NomicEmbedTextV1
        | EmbeddingModel::NomicEmbedTextV15
        | EmbeddingModel::NomicEmbedTextV15Q
        | EmbeddingModel::GTEBaseENV15
        | EmbeddingModel::GTEBaseENV15Q
        | EmbeddingModel::ParaphraseMLMpnetBaseV2 => 768,
        _ => 384,
    }
}

/// Prefijos de consulta y de pasaje que exige el modelo (vacíos si no los usa)
fn prefijos_del_modelo(modelo: &EmbeddingModel) -> (&'static str, &'static str) {
    match modelo {
        EmbeddingModel::MultilingualE5Small
        | EmbeddingModel::MultilingualE5Base
        | EmbeddingModel::MultilingualE5Large => (PREFIJO_CONSULTA_E5, PREFIJO_PASAJE_E5),
        _ => ("", ""),
    }
}

/// Adaptador de infraestructura que ejecuta inferencia ONNX local mediante FastEmbed
pub struct FastEmbedEngine {
    model: Arc<tokio::sync::Mutex<TextEmbedding>>,
    dimension: usize,
    prefijo_consulta: &'static str,
    prefijo_pasaje: &'static str,
}

impl FastEmbedEngine {
    /// Inicializa el modelo indicado (o el multilingüe por defecto) con la caché por defecto
    pub fn try_new(model_variant: Option<EmbeddingModel>) -> Result<Self, DomainError> {
        Self::con_cache(model_variant, None)
    }

    /// Inicializa el modelo con un directorio de caché explícito (o el efectivo si es `None`)
    pub fn con_cache(
        model_variant: Option<EmbeddingModel>,
        directorio_cache: Option<&Path>,
    ) -> Result<Self, DomainError> {
        let modelo = model_variant.unwrap_or(MODELO_POR_DEFECTO);
        let dimension = dimension_del_modelo(&modelo);
        let (prefijo_consulta, prefijo_pasaje) = prefijos_del_modelo(&modelo);
        let cache = directorio_cache_efectivo(directorio_cache);

        let options = InitOptions::new(modelo)
            .with_cache_dir(cache)
            .with_show_download_progress(false);
        let embedding_model = TextEmbedding::try_new(options).map_err(|e| {
            DomainError::EmbeddingError(format!("Error inicializando FastEmbed: {e}"))
        })?;

        Ok(Self {
            model: Arc::new(tokio::sync::Mutex::new(embedding_model)),
            dimension,
            prefijo_consulta,
            prefijo_pasaje,
        })
    }

    async fn inferir(&self, textos: Vec<String>) -> Result<Vec<Vec<f32>>, DomainError> {
        if textos.is_empty() {
            return Ok(Vec::new());
        }
        let model_arc = Arc::clone(&self.model);
        // Inferencia en el pool bloqueante de Tokio para no detener el reactor asíncrono
        tokio::task::spawn_blocking(move || {
            let model = model_arc.blocking_lock();
            model.embed(textos, None)
        })
        .await
        .map_err(|e| DomainError::EmbeddingError(format!("Error en tarea de inferencia: {e}")))?
        .map_err(|e| DomainError::EmbeddingError(format!("Error generando embeddings: {e}")))
    }
}

fn con_prefijo(prefijo: &str, textos: &[String]) -> Vec<String> {
    textos.iter().map(|t| format!("{prefijo}{t}")).collect()
}

#[async_trait]
impl EmbeddingEnginePort for FastEmbedEngine {
    /// Vector de una consulta (prefijo `query: ` en E5)
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, DomainError> {
        let consulta = con_prefijo(self.prefijo_consulta, &[text.to_string()]);
        self.inferir(consulta)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                DomainError::EmbeddingError("No se generó ningún vector de salida".to_string())
            })
    }

    /// Vectores de pasajes indexables (prefijo `passage: ` en E5)
    async fn generate_batch_embeddings(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, DomainError> {
        self.inferir(con_prefijo(self.prefijo_pasaje, texts)).await
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_prioriza_parametro_luego_variable_y_nunca_el_cwd() {
        let datos = Some(PathBuf::from("C:/datos"));
        assert_eq!(
            resolver_directorio_cache(
                Some(Path::new("D:/explicita")),
                Some("E:/env".into()),
                datos.clone()
            ),
            PathBuf::from("D:/explicita")
        );
        assert_eq!(
            resolver_directorio_cache(None, Some("E:/env".into()), datos.clone()),
            PathBuf::from("E:/env")
        );
        assert_eq!(
            resolver_directorio_cache(None, Some("".into()), datos),
            PathBuf::from("C:/datos")
                .join("lakehouse_rag")
                .join("fastembed_cache")
        );
        let sin_datos = resolver_directorio_cache(None, None, None);
        assert!(sin_datos.is_absolute(), "{sin_datos:?}");
        assert!(sin_datos.ends_with("lakehouse_rag/fastembed_cache"));
    }

    #[test]
    fn test_modelo_por_defecto_es_multilingue_1024_con_prefijos_e5() {
        assert_eq!(dimension_del_modelo(&MODELO_POR_DEFECTO), 1024);
        assert_eq!(
            prefijos_del_modelo(&MODELO_POR_DEFECTO),
            ("query: ", "passage: ")
        );
        assert_eq!(
            prefijos_del_modelo(&EmbeddingModel::BGELargeENV15),
            ("", "")
        );
    }
}
