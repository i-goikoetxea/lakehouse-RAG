//! Selección del transporte del LLM (`claude -p` o la API de Messages) por configuración.
//! **Por defecto `Cli`** hasta que el A/B de la Fase 5.2 valide la API.

use std::sync::Arc;

use crate::application::ports::verificador_procedencia::VerificadorProcedenciaPort;
use crate::infrastructure::llm::ClaudeHeadlessProcedencia;
use crate::infrastructure::llm::anthropic_api::secreto::VARIABLE_API_KEY;
use crate::infrastructure::llm::anthropic_api::{
    AlmacenKeyring, AnthropicHttpClient, AnthropicProcedenciaVerifier, ConfigClienteAnthropic,
    resolver_api_key,
};

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
            Some(otro) => {
                tracing::warn!(valor = otro, "{VARIABLE_MODO_LLM} desconocido: se usa cli");
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
    fn test_api_sin_distinguir_mayusculas_y_desconocido_vuelve_a_cli() {
        assert_eq!(ModoLlm::desde_valor(Some("api")), ModoLlm::Api);
        assert_eq!(ModoLlm::desde_valor(Some(" API ")), ModoLlm::Api);
        assert_eq!(ModoLlm::desde_valor(Some("openai")), ModoLlm::Cli);
    }
}
