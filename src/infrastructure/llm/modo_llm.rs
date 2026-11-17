//! Selección del transporte del LLM (`claude -p`, la API de Messages o el proxy FreeLLMAPI) por
//! configuración. **Por defecto `Cli`**. El modo se resuelve al arrancar: entorno >
//! `llm_config.json` > `Cli` (`preferencia_modo`). Contrato del tercer motor:
//! `docs/arquitectura/ssd_motor_freellmapi.md`.

use std::sync::Arc;

use crate::application::ports::agent_runner::AgentRunnerPort;
use crate::application::ports::model_catalog::ModelCatalogPort;
use crate::application::ports::verificador_procedencia::VerificadorProcedenciaPort;
use crate::infrastructure::llm::anthropic_api::secreto::{
    VARIABLE_API_KEY, VARIABLE_FREELLMAPI_KEY,
};
use crate::infrastructure::llm::anthropic_api::{
    AlmacenKeyring, AnthropicHttpClient, AnthropicMessagesRunner, AnthropicProcedenciaVerifier,
    ConfigClienteAnthropic, ErrorApiAnthropic, PerfilApi, SecretApiKey, resolver_api_key,
};
use crate::infrastructure::llm::freellmapi_catalogo::{CatalogoSinKey, FreeLlmApiModelCatalog};
use crate::infrastructure::llm::{
    ClaudeAgentConfig, ClaudeAgentRunner, ClaudeHeadlessModelCatalog, ClaudeHeadlessProcedencia,
    ClaudeModelCatalogConfig,
};

pub const VARIABLE_MODO_LLM: &str = "LAKEHOUSE_RAG_MODO_LLM";
/// Ruta del router de FreeLLMAPI para el Nivel 2 de procedencia: prima la exactitud (SSD §3.6)
pub const MODELO_PROCEDENCIA_FREELLMAPI: &str = "auto:smart";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModoLlm {
    #[default]
    Cli,
    Api,
    /// Proxy FreeLLMAPI: modelos gratuitos de terceros por la API de Messages
    FreeLlmApi,
}

impl ModoLlm {
    /// `api`, `freellmapi` o `cli`, sin distinguir mayúsculas. Ausente o desconocido: `Cli`
    pub fn desde_valor(valor: Option<&str>) -> Self {
        match valor.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("api") => Self::Api,
            Some("freellmapi") => Self::FreeLlmApi,
            None | Some("") | Some("cli") => Self::Cli,
            // Sin repetir el valor: podría ser una key pegada en la variable equivocada
            Some(otro) => {
                tracing::warn!(
                    longitud = otro.chars().count(),
                    "{VARIABLE_MODO_LLM} desconocido: se usa cli"
                );
                Self::Cli
            }
        }
    }

    pub fn desde_entorno() -> Self {
        Self::desde_valor(std::env::var(VARIABLE_MODO_LLM).ok().as_deref())
    }

    /// Nombre en el fichero, el entorno y la ventana
    pub fn nombre(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Api => "api",
            Self::FreeLlmApi => "freellmapi",
        }
    }

    /// Variable de entorno de la key del motor; `None` en `Cli`
    pub fn variable_de_key(self) -> Option<&'static str> {
        match self {
            Self::Cli => None,
            Self::Api => Some(VARIABLE_API_KEY),
            Self::FreeLlmApi => Some(VARIABLE_FREELLMAPI_KEY),
        }
    }

    /// Entrada del almacén del sistema con la key del motor; `None` en `Cli`
    pub fn almacen_de_key(self) -> Option<AlmacenKeyring> {
        match self {
            Self::Cli => None,
            Self::Api => Some(AlmacenKeyring::default()),
            Self::FreeLlmApi => Some(AlmacenKeyring::freellmapi()),
        }
    }
}

/// La key del motor: su variable de entorno y, si no está, su entrada del almacén. Cada motor
/// solo lee la suya: la key de Anthropic nunca va al proxy, ni al revés
pub fn key_del_motor(modo: ModoLlm) -> Result<SecretApiKey, ErrorApiAnthropic> {
    match (modo.variable_de_key(), modo.almacen_de_key()) {
        (Some(variable), Some(almacen)) => resolver_api_key(std::env::var(variable).ok(), &almacen),
        _ => Err(ErrorApiAnthropic::KeyAusente),
    }
}

/// Configuración del cliente HTTP de un motor con API; `None` en `Cli`
pub fn config_cliente_para(
    modo: ModoLlm,
    base: ConfigClienteAnthropic,
    url_freellmapi: &str,
) -> Option<ConfigClienteAnthropic> {
    match modo {
        ModoLlm::Cli => None,
        ModoLlm::Api => Some(base),
        ModoLlm::FreeLlmApi => Some(
            base.con_perfil(PerfilApi::FREELLMAPI)
                .con_base_url(url_freellmapi),
        ),
    }
}

/// Verificador de procedencia según el modo. Sin key, o si el cliente no se puede construir,
/// vuelve a `Cli` con un aviso: la app nunca se cae por la key
pub fn verificador_procedencia_para(
    modo: ModoLlm,
    url_freellmapi: &str,
    binario_cli: std::path::PathBuf,
) -> Arc<dyn VerificadorProcedenciaPort> {
    if let Some(config) =
        config_cliente_para(modo, ConfigClienteAnthropic::procedencia(), url_freellmapi)
    {
        match key_del_motor(modo).and_then(|key| AnthropicHttpClient::new(key, config)) {
            Ok(cliente) => {
                let verificador = AnthropicProcedenciaVerifier::new(cliente);
                return Arc::new(if modo == ModoLlm::FreeLlmApi {
                    verificador.con_modelo(MODELO_PROCEDENCIA_FREELLMAPI)
                } else {
                    verificador
                });
            }
            Err(error) => tracing::warn!(
                %error,
                modo = modo.nombre(),
                "modo_llm sin cliente: la procedencia usa claude -p"
            ),
        }
    }
    Arc::new(ClaudeHeadlessProcedencia::new(binario_cli))
}

/// Catálogo del desplegable según el motor **en uso**: el del proxy con FreeLLMAPI; si no, el de
/// la instalación de Claude Code (los modos CLI y API usan los mismos modelos)
pub fn catalogo_modelos_para(en_uso: ModoLlm, url_freellmapi: &str) -> Arc<dyn ModelCatalogPort> {
    if en_uso != ModoLlm::FreeLlmApi {
        return Arc::new(ClaudeHeadlessModelCatalog::new(
            ClaudeModelCatalogConfig::default(),
        ));
    }
    let config = config_cliente_para(
        en_uso,
        ConfigClienteAnthropic::procedencia(),
        url_freellmapi,
    );
    match config
        .map(|config| key_del_motor(en_uso).and_then(|key| AnthropicHttpClient::new(key, config)))
    {
        Some(Ok(cliente)) => Arc::new(FreeLlmApiModelCatalog::new(cliente, url_freellmapi)),
        _ => Arc::new(CatalogoSinKey),
    }
}

/// El agente según el modo, el runner de la API aparte para leer sus métricas (A/B) y el motor
/// que de verdad quedó en uso (`Cli` si no había cliente)
pub struct AgenteSegunModo {
    pub runner: Arc<dyn AgentRunnerPort>,
    pub api: Option<Arc<AnthropicMessagesRunner>>,
    pub en_uso: ModoLlm,
}

/// Con un motor de API y su key, `AnthropicMessagesRunner`; si no, `claude -p` con un aviso
pub fn agente_para(
    modo: ModoLlm,
    url_freellmapi: &str,
    config_cli: ClaudeAgentConfig,
) -> AgenteSegunModo {
    match config_cliente_para(modo, ConfigClienteAnthropic::agente(), url_freellmapi) {
        Some(config_api) => agente_con_key(modo, key_del_motor(modo), config_api, config_cli),
        None => agente_cli(config_cli),
    }
}

/// `agente_para` con la key y la configuración del cliente inyectadas (tests y herramientas)
pub fn agente_con_key(
    modo: ModoLlm,
    key: Result<SecretApiKey, ErrorApiAnthropic>,
    config_api: ConfigClienteAnthropic,
    config_cli: ClaudeAgentConfig,
) -> AgenteSegunModo {
    if modo != ModoLlm::Cli {
        match key.and_then(|key| AnthropicHttpClient::new(key, config_api)) {
            Ok(cliente) => {
                let api = Arc::new(AnthropicMessagesRunner::new(cliente));
                return AgenteSegunModo {
                    runner: api.clone(),
                    api: Some(api),
                    en_uso: modo,
                };
            }
            Err(error) => tracing::warn!(
                %error,
                modo = modo.nombre(),
                "modo_llm sin cliente: el agente usa claude -p"
            ),
        }
    }
    agente_cli(config_cli)
}

fn agente_cli(config_cli: ClaudeAgentConfig) -> AgenteSegunModo {
    AgenteSegunModo {
        runner: Arc::new(ClaudeAgentRunner::new(config_cli)),
        api: None,
        en_uso: ModoLlm::Cli,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_por_defecto_es_cli() {
        assert_eq!(ModoLlm::default(), ModoLlm::Cli);
        assert_eq!(ModoLlm::desde_valor(None), ModoLlm::Cli);
        assert_eq!(ModoLlm::desde_valor(Some("")), ModoLlm::Cli);
        assert_eq!(ModoLlm::desde_valor(Some("cli")), ModoLlm::Cli);
    }

    #[test]
    fn test_el_agente_sigue_al_modo_y_sin_key_vuelve_a_cli() {
        let key = || SecretApiKey::new("sk-ant-test-NO-ES-REAL-modo");
        let config = ConfigClienteAnthropic::agente;
        let api = agente_con_key(ModoLlm::Api, key(), config(), ClaudeAgentConfig::default());
        assert!(api.api.is_some());
        assert!(api.runner.necesita_correccion_de_omision());
        let cli = agente_con_key(ModoLlm::Cli, key(), config(), ClaudeAgentConfig::default());
        assert!(cli.api.is_none());
        assert!(!cli.runner.necesita_correccion_de_omision());
        let sin_key = agente_con_key(
            ModoLlm::Api,
            Err(ErrorApiAnthropic::KeyAusente),
            config(),
            ClaudeAgentConfig::default(),
        );
        assert!(sin_key.api.is_none());
        assert_eq!(sin_key.en_uso, ModoLlm::Cli);
        assert_eq!(api.en_uso, ModoLlm::Api);
    }

    #[test]
    fn test_freellmapi_compone_el_runner_con_el_perfil_y_la_url_del_proxy() {
        let config = config_cliente_para(
            ModoLlm::FreeLlmApi,
            ConfigClienteAnthropic::agente(),
            "http://127.0.0.1:31415",
        )
        .unwrap();
        assert_eq!(config.perfil, PerfilApi::FREELLMAPI);
        assert_eq!(config.base_url, "http://127.0.0.1:31415");
        let api = config_cliente_para(
            ModoLlm::Api,
            ConfigClienteAnthropic::agente(),
            "http://127.0.0.1:31415",
        )
        .unwrap();
        assert_eq!(api.perfil, PerfilApi::ANTHROPIC);
        assert!(api.base_url.contains("anthropic.com"), "{}", api.base_url);
        assert!(config_cliente_para(ModoLlm::Cli, ConfigClienteAnthropic::agente(), "x").is_none());

        let key = SecretApiKey::new("freellmapi-test-NO-ES-REAL-0000");
        let agente = agente_con_key(
            ModoLlm::FreeLlmApi,
            key,
            config,
            ClaudeAgentConfig::default(),
        );
        assert_eq!(agente.en_uso, ModoLlm::FreeLlmApi);
        assert!(agente.api.is_some());
        assert!(
            agente.runner.necesita_correccion_de_omision(),
            "la corrección de omisión sigue activa con modelos gratuitos"
        );
    }

    #[test]
    fn test_cada_motor_lee_solo_su_propia_key() {
        assert_eq!(ModoLlm::Api.variable_de_key(), Some("ANTHROPIC_API_KEY"));
        assert_eq!(
            ModoLlm::FreeLlmApi.variable_de_key(),
            Some("FREELLMAPI_API_KEY")
        );
        assert_eq!(ModoLlm::Cli.variable_de_key(), None);
        assert_ne!(
            ModoLlm::Api.almacen_de_key().unwrap().usuario(),
            ModoLlm::FreeLlmApi.almacen_de_key().unwrap().usuario()
        );
        assert!(matches!(
            key_del_motor(ModoLlm::Cli),
            Err(ErrorApiAnthropic::KeyAusente)
        ));
    }

    #[test]
    fn test_api_sin_distinguir_mayusculas_y_desconocido_vuelve_a_cli() {
        assert_eq!(ModoLlm::desde_valor(Some("api")), ModoLlm::Api);
        assert_eq!(ModoLlm::desde_valor(Some(" API ")), ModoLlm::Api);
        assert_eq!(ModoLlm::desde_valor(Some("openai")), ModoLlm::Cli);
        assert_eq!(
            ModoLlm::desde_valor(Some(" FreeLLMAPI ")),
            ModoLlm::FreeLlmApi
        );
        for modo in [ModoLlm::Cli, ModoLlm::Api, ModoLlm::FreeLlmApi] {
            assert_eq!(ModoLlm::desde_valor(Some(modo.nombre())), modo);
        }
    }
}
