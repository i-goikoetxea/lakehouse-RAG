//! Adaptadores sobre la API de Messages de Anthropic (Pieza 5): procedencia (5.1) y agente (5.2).
//! La destilación (5.3) reutilizará el mismo cliente.
//! Contrato: `docs/arquitectura/ssd_pieza5_fase51_procedencia_api.md`.

pub mod agente;
pub mod cliente;
pub mod error;
pub mod flujo;
pub mod mensajes;
pub mod procedencia;
pub mod secreto;

pub use agente::{AnthropicMessagesRunner, MODELO_AGENTE_API, MetricaPasoApi};
pub use cliente::{
    AnthropicHttpClient, ConfigClienteAnthropic, PoliticaReintentos, RespuestaConMetricas,
};
pub use error::ErrorApiAnthropic;
pub use procedencia::{AnthropicProcedenciaVerifier, MODELO_PROCEDENCIA_API};
pub use secreto::{
    AlmacenEnMemoria, AlmacenKeyring, AlmacenSecreto, SecretApiKey, resolver_api_key,
};
