//! Preferencia persistida del modo del LLM (`llm_config.json` en el directorio de configuración).
//!
//! * Prelación al arrancar: `LAKEHOUSE_RAG_MODO_LLM` (si está definida y no vacía) > fichero >
//!   `Cli`. Sin configuración, el modo sigue siendo `Cli`.
//! * El fichero solo guarda el modo y la URL de FreeLLMAPI (no es secreta). Las keys viven en el
//!   almacén del sistema, nunca aquí.
//! * Un fichero ausente, corrupto o desmesurado no tumba la app: se ignora con un aviso.
//! * Escritura atómica: archivo temporal + `rename`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::infrastructure::llm::modo_llm::{ModoLlm, VARIABLE_MODO_LLM};

pub const ARCHIVO_PREFERENCIAS_LLM: &str = "llm_config.json";
/// Un fichero legítimo ocupa unos bytes; más es un error o un ataque
const TAMANO_MAXIMO_PREFERENCIAS: u64 = 4 * 1024;
/// La app de escritorio de FreeLLMAPI escucha aquí (su documentación oficial)
pub const URL_FREELLMAPI_POR_DEFECTO: &str = "http://127.0.0.1:31415";
const LONGITUD_MAXIMA_URL: usize = 256;

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

#[derive(Debug, Default, Serialize, Deserialize)]
struct ArchivoPreferenciasLlm {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modo_llm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    freellmapi_url: Option<String>,
}

/// Lo guardado en `llm_config.json`, ya validado: lo que no es válido queda en `None`
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreferenciasLlm {
    pub modo: Option<ModoLlm>,
    pub freellmapi_url: Option<String>,
}

impl PreferenciasLlm {
    /// La URL del proxy guardada o la de la app de escritorio
    pub fn url_freellmapi(&self) -> &str {
        self.freellmapi_url
            .as_deref()
            .unwrap_or(URL_FREELLMAPI_POR_DEFECTO)
    }
}

/// `http` o `https` con host, sin credenciales, query, fragmento ni más ruta que `/`. Devuelve la
/// URL normalizada sin `/` final, o `None` (el llamante nunca repite la entrada)
pub fn validar_url_freellmapi(valor: &str) -> Option<String> {
    let limpia = valor.trim();
    if limpia.is_empty() || limpia.chars().count() > LONGITUD_MAXIMA_URL {
        return None;
    }
    let url = reqwest::Url::parse(limpia).ok()?;
    let valida = matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some_and(|h| !h.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == "/";
    valida.then(|| url.as_str().trim_end_matches('/').to_string())
}

/// La URL apunta a este equipo: la key y los prompts no salen por la red hasta el proxy
pub fn es_url_local(url: &str) -> bool {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        })
}

pub fn ruta_preferencias(directorio_config: &Path) -> PathBuf {
    directorio_config.join(ARCHIVO_PREFERENCIAS_LLM)
}

/// El modo guardado, o `None` si no hay fichero o no es válido
pub fn leer_modo_guardado(directorio_config: &Path) -> Option<ModoLlm> {
    leer_preferencias(directorio_config).modo
}

/// Las preferencias guardadas. Lo ausente o inválido queda en `None`, con un aviso que no
/// repite el contenido: podría ser cualquier cosa pegada ahí
pub fn leer_preferencias(directorio_config: &Path) -> PreferenciasLlm {
    let ruta = ruta_preferencias(directorio_config);
    let Ok(metadatos) = std::fs::metadata(&ruta) else {
        return PreferenciasLlm::default();
    };
    if !metadatos.is_file() || metadatos.len() > TAMANO_MAXIMO_PREFERENCIAS {
        tracing::warn!(ruta = %ruta.display(), "preferencias del LLM no válidas: se ignoran");
        return PreferenciasLlm::default();
    }
    let archivo = std::fs::read_to_string(&ruta)
        .ok()
        .and_then(|texto| serde_json::from_str::<ArchivoPreferenciasLlm>(&texto).ok());
    let Some(archivo) = archivo else {
        tracing::warn!(ruta = %ruta.display(), "preferencias del LLM ilegibles: se ignoran");
        return PreferenciasLlm::default();
    };
    let modo = archivo.modo_llm.as_deref().and_then(modo_estricto);
    if archivo.modo_llm.is_some() && modo.is_none() {
        tracing::warn!(ruta = %ruta.display(), "modo del LLM guardado desconocido: se ignora");
    }
    let freellmapi_url = archivo
        .freellmapi_url
        .as_deref()
        .and_then(validar_url_freellmapi);
    if archivo.freellmapi_url.is_some() && freellmapi_url.is_none() {
        tracing::warn!(ruta = %ruta.display(), "URL de FreeLLMAPI guardada no válida: se ignora");
    }
    PreferenciasLlm {
        modo,
        freellmapi_url,
    }
}

/// Solo `cli`, `api` o `freellmapi`, sin distinguir mayúsculas; cualquier otra cosa se rechaza
pub fn modo_estricto(valor: &str) -> Option<ModoLlm> {
    match valor.trim().to_ascii_lowercase().as_str() {
        "cli" => Some(ModoLlm::Cli),
        "api" => Some(ModoLlm::Api),
        "freellmapi" => Some(ModoLlm::FreeLlmApi),
        _ => None,
    }
}

pub fn nombre_de_modo(modo: ModoLlm) -> &'static str {
    modo.nombre()
}

/// Guarda el modo elegido y conserva la URL de FreeLLMAPI válida. El fichero se reescribe entero
/// con esos dos campos: cualquier otro desaparece
pub fn guardar_modo(directorio_config: &Path, modo: ModoLlm) -> std::io::Result<()> {
    let actuales = leer_preferencias(directorio_config);
    escribir_preferencias(
        directorio_config,
        &PreferenciasLlm {
            modo: Some(modo),
            ..actuales
        },
    )
}

/// Guarda la URL de FreeLLMAPI (validada) y conserva el modo. Devuelve la URL normalizada, o
/// `None` si no es válida (y entonces no toca el fichero)
pub fn guardar_url_freellmapi(
    directorio_config: &Path,
    url: &str,
) -> std::io::Result<Option<String>> {
    let Some(url) = validar_url_freellmapi(url) else {
        return Ok(None);
    };
    let actuales = leer_preferencias(directorio_config);
    escribir_preferencias(
        directorio_config,
        &PreferenciasLlm {
            freellmapi_url: Some(url.clone()),
            ..actuales
        },
    )?;
    Ok(Some(url))
}

fn escribir_preferencias(
    directorio_config: &Path,
    preferencias: &PreferenciasLlm,
) -> std::io::Result<()> {
    std::fs::create_dir_all(directorio_config)?;
    let contenido = serde_json::to_string_pretty(&ArchivoPreferenciasLlm {
        modo_llm: preferencias.modo.map(|m| nombre_de_modo(m).to_string()),
        freellmapi_url: preferencias.freellmapi_url.clone(),
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
    fn test_la_url_y_el_modo_se_conservan_al_guardar_el_otro() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            leer_preferencias(dir.path()).url_freellmapi(),
            "http://127.0.0.1:31415"
        );
        guardar_modo(dir.path(), ModoLlm::FreeLlmApi).unwrap();
        let guardada = guardar_url_freellmapi(dir.path(), " http://localhost:3001/ ").unwrap();
        assert_eq!(guardada.as_deref(), Some("http://localhost:3001"));
        let leidas = leer_preferencias(dir.path());
        assert_eq!(leidas.modo, Some(ModoLlm::FreeLlmApi));
        assert_eq!(leidas.url_freellmapi(), "http://localhost:3001");
        guardar_modo(dir.path(), ModoLlm::Cli).unwrap();
        assert_eq!(
            leer_preferencias(dir.path()).url_freellmapi(),
            "http://localhost:3001"
        );
    }

    #[test]
    fn test_una_url_invalida_no_se_guarda_ni_borra_la_anterior() {
        let dir = tempfile::tempdir().unwrap();
        guardar_url_freellmapi(dir.path(), "http://127.0.0.1:31415").unwrap();
        for mala in [
            "",
            "127.0.0.1:31415",
            "ftp://127.0.0.1",
            "file:///C:/x",
            "javascript:alert(1)",
            "http://usuario:clave@127.0.0.1:31415",
            "http://127.0.0.1:31415/v1",
            "http://127.0.0.1:31415/?k=freellmapi-x",
            "http://127.0.0.1:31415/#x",
            "freellmapi-NO-ES-REAL-key",
        ] {
            assert_eq!(
                guardar_url_freellmapi(dir.path(), mala).unwrap(),
                None,
                "{mala}"
            );
        }
        let larga = format!("http://{}.com", "a".repeat(300));
        assert_eq!(validar_url_freellmapi(&larga), None);
        assert_eq!(
            leer_preferencias(dir.path()).freellmapi_url.as_deref(),
            Some("http://127.0.0.1:31415")
        );
    }

    #[test]
    fn test_un_fichero_antiguo_sin_url_sigue_valiendo_y_una_url_envenenada_se_ignora() {
        let dir = tempfile::tempdir().unwrap();
        let ruta = ruta_preferencias(dir.path());
        std::fs::write(&ruta, r#"{"modo_llm":"api"}"#).unwrap();
        assert_eq!(leer_modo_guardado(dir.path()), Some(ModoLlm::Api));
        std::fs::write(
            &ruta,
            r#"{"modo_llm":"freellmapi","freellmapi_url":"javascript:alert(1)"}"#,
        )
        .unwrap();
        let leidas = leer_preferencias(dir.path());
        assert_eq!(leidas.modo, Some(ModoLlm::FreeLlmApi));
        assert_eq!(leidas.freellmapi_url, None);
    }

    #[test]
    fn test_url_local_solo_para_loopback() {
        for local in [
            "http://127.0.0.1:31415",
            "http://localhost:3001",
            "http://[::1]:8",
        ] {
            assert!(es_url_local(local), "{local}");
        }
        for remota in ["http://192.168.1.10:3001", "https://freellmapi.example.com"] {
            assert!(!es_url_local(remota), "{remota}");
        }
    }

    #[test]
    fn test_modo_estricto_solo_acepta_cli_api_o_freellmapi() {
        assert_eq!(modo_estricto(" API "), Some(ModoLlm::Api));
        assert_eq!(modo_estricto("cli"), Some(ModoLlm::Cli));
        assert_eq!(modo_estricto("FreeLLMAPI"), Some(ModoLlm::FreeLlmApi));
        for malo in ["", "apí", "api;rm", "claude", "1"] {
            assert_eq!(modo_estricto(malo), None, "{malo}");
        }
    }
}
