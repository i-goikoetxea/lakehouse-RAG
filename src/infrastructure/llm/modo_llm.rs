//! Selección del transporte del LLM (`claude -p` o la API de Messages) por configuración.
//! **Por defecto `Cli`** hasta que el A/B de la Fase 5.2 valide la API. El modo se lee al arrancar.

use std::sync::Arc;

use crate::application::ports::agent_runner::AgentRunnerPort;
use crate::application::ports::verificador_procedencia::VerificadorProcedenciaPort;
use crate::infrastructure::llm::anthropic_api::secreto::VARIABLE_API_KEY;
use crate::infrastructure::llm::anthropic_api::{
    AlmacenKeyring, AnthropicHttpClient, AnthropicMessagesRunner, AnthropicProcedenciaVerifier,
    ConfigClienteAnthropic, ErrorApiAnthropic, SecretApiKey, resolver_api_key,
};
use crate::infrastructure::llm::{ClaudeAgentConfig, ClaudeAgentRunner, ClaudeHeadlessProcedencia};

pub const VARIABLE_MODO_LLM: &str = "LAKEHOUSE_RAG_MODO_LLM";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModoLlm {
    #[default]
    Cli,
    Api,
}

impl ModoLlm {
    /// `api` o `cli`, sin distinguir mayúsculas. Ausente o desconocido: `Cli`
    pub fn desde_valor(valor: Option<&str>) -> Self {
        match valor.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            Some("api") => Self::Api,
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
}

/// Verificador de procedencia según el modo. Con `Api` y sin key, o si el cliente no se puede
/// construir, vuelve a `Cli` con un aviso: la app nunca se cae por la key
pub fn verificador_procedencia_para(
    modo: ModoLlm,
    binario_cli: std::path::PathBuf,
) -> Arc<dyn VerificadorProcedenciaPort> {
    if modo == ModoLlm::Api {
        let cliente = resolver_api_key(
            std::env::var(VARIABLE_API_KEY).ok(),
            &AlmacenKeyring::default(),
        )
        .and_then(|key| AnthropicHttpClient::new(key, ConfigClienteAnthropic::procedencia()));
        match cliente {
            Ok(cliente) => return Arc::new(AnthropicProcedenciaVerifier::new(cliente)),
            Err(error) => {
                tracing::warn!(%error, "modo_llm api sin cliente: la procedencia usa claude -p")
            }
        }
    }
    Arc::new(ClaudeHeadlessProcedencia::new(binario_cli))
}

/// El agente según el modo, y el runner de la API aparte para leer sus métricas (A/B)
pub struct AgenteSegunModo {
    pub runner: Arc<dyn AgentRunnerPort>,
    pub api: Option<Arc<AnthropicMessagesRunner>>,
}

/// Con `Api` y la key resuelta, `AnthropicMessagesRunner`; si no, `claude -p` con un aviso
pub fn agente_para(modo: ModoLlm, config_cli: ClaudeAgentConfig) -> AgenteSegunModo {
    let key = resolver_api_key(
        std::env::var(VARIABLE_API_KEY).ok(),
        &AlmacenKeyring::default(),
    );
    agente_con_key(modo, key, ConfigClienteAnthropic::agente(), config_cli)
}

/// `agente_para` con la key y la configuración del cliente inyectadas (tests y herramientas)
pub fn agente_con_key(
    modo: ModoLlm,
    key: Result<SecretApiKey, ErrorApiAnthropic>,
    config_api: ConfigClienteAnthropic,
    config_cli: ClaudeAgentConfig,
) -> AgenteSegunModo {
    if modo == ModoLlm::Api {
        match key.and_then(|key| AnthropicHttpClient::new(key, config_api)) {
            Ok(cliente) => {
                let api = Arc::new(AnthropicMessagesRunner::new(cliente));
                return AgenteSegunModo {
                    runner: api.clone(),
                    api: Some(api),
                };
            }
            Err(error) => {
                tracing::warn!(%error, "modo_llm api sin cliente: el agente usa claude -p")
            }
        }
    }
    AgenteSegunModo {
        runner: Arc::new(ClaudeAgentRunner::new(config_cli)),
        api: None,
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
    }

    #[test]
    fn test_api_sin_distinguir_mayusculas_y_desconocido_vuelve_a_cli() {
        assert_eq!(ModoLlm::desde_valor(Some("api")), ModoLlm::Api);
        assert_eq!(ModoLlm::desde_valor(Some(" API ")), ModoLlm::Api);
        assert_eq!(ModoLlm::desde_valor(Some("openai")), ModoLlm::Cli);
    }
}
