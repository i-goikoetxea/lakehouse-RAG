//! Preferencia persistida del modo del LLM (`llm_config.json` en el directorio de configuración).
//!
//! * Prelación al arrancar: `LAKEHOUSE_RAG_MODO_LLM` (si está definida y no vacía) > fichero >
//!   `Cli`. Sin configuración, el modo sigue siendo `Cli`.
//! * El fichero solo guarda el modo. La API key vive en el almacén del sistema, nunca aquí.
//! * Un fichero ausente, corrupto o desmesurado no tumba la app: se ignora con un aviso.
//! * Escritura atómica: archivo temporal + `rename`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::infrastructure::llm::modo_llm::{ModoLlm, VARIABLE_MODO_LLM};

pub const ARCHIVO_PREFERENCIAS_LLM: &str = "llm_config.json";
/// Un fichero legítimo ocupa unos bytes; más es un error o un ataque
const TAMANO_MAXIMO_PREFERENCIAS: u64 = 4 * 1024;

/// De dónde sale el modo resuelto al arrancar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrigenModo {
    Entorno,
    Fichero,
    PorDefecto,
}

impl OrigenModo {
    pub fn nombre(self) -> &'static str {
        match self {
            Self::Entorno => "entorno",
            Self::Fichero => "fichero",
            Self::PorDefecto => "por_defecto",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModoResuelto {
    pub modo: ModoLlm,
    pub origen: OrigenModo,
}

/// La variable de entorno manda si está definida y no vacía; después, el fichero; si no, `Cli`
pub fn resolver_modo(valor_entorno: Option<&str>, guardado: Option<ModoLlm>) -> ModoResuelto {
    if let Some(valor) = valor_entorno.filter(|v| !v.trim().is_empty()) {
        return ModoResuelto {
            modo: ModoLlm::desde_valor(Some(valor)),
            origen: OrigenModo::Entorno,
        };
    }
    match guardado {
        Some(modo) => ModoResuelto {
            modo,
            origen: OrigenModo::Fichero,
        },
        None => ModoResuelto {
            modo: ModoLlm::Cli,
            origen: OrigenModo::PorDefecto,
        },
    }
}

/// `resolver_modo` con el entorno del proceso y el fichero de `directorio_config`
pub fn resolver_modo_al_arrancar(directorio_config: &Path) -> ModoResuelto {
    resolver_modo(
        std::env::var(VARIABLE_MODO_LLM).ok().as_deref(),
        leer_modo_guardado(directorio_config),
    )
}

#[derive(Debug, Serialize, Deserialize)]
struct ArchivoPreferenciasLlm {
    modo_llm: String,
}

pub fn ruta_preferencias(directorio_config: &Path) -> PathBuf {
    directorio_config.join(ARCHIVO_PREFERENCIAS_LLM)
}

/// El modo guardado, o `None` si no hay fichero o no es válido (con un aviso, sin repetir su
/// contenido: podría ser cualquier cosa pegada ahí)
pub fn leer_modo_guardado(directorio_config: &Path) -> Option<ModoLlm> {
    let ruta = ruta_preferencias(directorio_config);
    let metadatos = std::fs::metadata(&ruta).ok()?;
    if !metadatos.is_file() || metadatos.len() > TAMANO_MAXIMO_PREFERENCIAS {
        tracing::warn!(ruta = %ruta.display(), "preferencias del LLM no válidas: se ignoran");
        return None;
    }
    let texto = std::fs::read_to_string(&ruta).ok()?;
    match serde_json::from_str::<ArchivoPreferenciasLlm>(&texto)
        .ok()
        .and_then(|archivo| modo_estricto(&archivo.modo_llm))
    {
        Some(modo) => Some(modo),
        None => {
            tracing::warn!(ruta = %ruta.display(), "preferencias del LLM ilegibles: se ignoran");
            None
        }
    }
}

/// Solo `cli` o `api`, sin distinguir mayúsculas; cualquier otra cosa se rechaza
pub fn modo_estricto(valor: &str) -> Option<ModoLlm> {
    match valor.trim().to_ascii_lowercase().as_str() {
        "cli" => Some(ModoLlm::Cli),
        "api" => Some(ModoLlm::Api),
        _ => None,
    }
}

pub fn nombre_de_modo(modo: ModoLlm) -> &'static str {
    match modo {
        ModoLlm::Api => "api",
        ModoLlm::Cli => "cli",
    }
}

/// Guarda el modo elegido; el fichero se reescribe entero y solo con el modo
pub fn guardar_modo(directorio_config: &Path, modo: ModoLlm) -> std::io::Result<()> {
    std::fs::create_dir_all(directorio_config)?;
    let contenido = serde_json::to_string_pretty(&ArchivoPreferenciasLlm {
        modo_llm: nombre_de_modo(modo).to_string(),
    })
    .map_err(std::io::Error::other)?;
    let destino = ruta_preferencias(directorio_config);
    let temporal = directorio_config.join(format!("{ARCHIVO_PREFERENCIAS_LLM}.tmp"));
    std::fs::write(&temporal, contenido)?;
    std::fs::rename(&temporal, &destino)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sin_entorno_ni_fichero_es_cli_por_defecto() {
        let resuelto = resolver_modo(None, None);
        assert_eq!(resuelto.modo, ModoLlm::Cli);
        assert_eq!(resuelto.origen, OrigenModo::PorDefecto);
        assert_eq!(
            resolver_modo(Some("  "), None).origen,
            OrigenModo::PorDefecto
        );
    }

    #[test]
    fn test_el_entorno_manda_sobre_el_fichero() {
        let resuelto = resolver_modo(Some("cli"), Some(ModoLlm::Api));
        assert_eq!(resuelto.modo, ModoLlm::Cli);
        assert_eq!(resuelto.origen, OrigenModo::Entorno);
        let sin_entorno = resolver_modo(None, Some(ModoLlm::Api));
        assert_eq!(sin_entorno.modo, ModoLlm::Api);
        assert_eq!(sin_entorno.origen, OrigenModo::Fichero);
    }

    #[test]
    fn test_guardar_y_leer_el_modo() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(leer_modo_guardado(dir.path()), None);
        guardar_modo(dir.path(), ModoLlm::Api).unwrap();
        assert_eq!(leer_modo_guardado(dir.path()), Some(ModoLlm::Api));
        guardar_modo(dir.path(), ModoLlm::Cli).unwrap();
        assert_eq!(leer_modo_guardado(dir.path()), Some(ModoLlm::Cli));
        assert!(!dir.path().join("llm_config.json.tmp").exists());
    }

    #[test]
    fn test_guardar_crea_el_directorio_de_configuracion() {
        let dir = tempfile::tempdir().unwrap();
        let anidado = dir.path().join("a").join("b");
        guardar_modo(&anidado, ModoLlm::Api).unwrap();
        assert_eq!(leer_modo_guardado(&anidado), Some(ModoLlm::Api));
    }

    #[test]
    fn test_fichero_corrupto_o_con_modo_desconocido_se_ignora() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = ruta_preferencias(dir.path());
        for contenido in [
            "{",
            "[]",
            r#"{"modo_llm":"openai"}"#,
            r#"{"modo_llm":7}"#,
            "",
        ] {
            std::fs::write(&ruta, contenido).unwrap();
            assert_eq!(leer_modo_guardado(dir.path()), None, "{contenido}");
        }
    }

    #[test]
    fn test_modo_estricto_solo_acepta_cli_o_api() {
        assert_eq!(modo_estricto(" API "), Some(ModoLlm::Api));
        assert_eq!(modo_estricto("cli"), Some(ModoLlm::Cli));
        for malo in ["", "apí", "api;rm", "claude", "1"] {
            assert_eq!(modo_estricto(malo), None, "{malo}");
        }
    }
}
