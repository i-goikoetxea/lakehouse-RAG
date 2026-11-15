//! Adaptadores sobre la API de Messages de Anthropic (Pieza 5). La Fase 5.1 solo cubre la
//! procedencia; el agente (5.2) y la destilación (5.3) reutilizarán el mismo cliente.
//! Contrato: `docs/arquitectura/ssd_pieza5_fase51_procedencia_api.md`.

pub mod cliente;
pub mod error;
pub mod mensajes;
pub mod procedencia;
pub mod secreto;

pub use cliente::{
    AnthropicHttpClient, ConfigClienteAnthropic, PoliticaReintentos, RespuestaConMetricas,
};
pub use error::ErrorApiAnthropic;
pub use procedencia::{AnthropicProcedenciaVerifier, MODELO_PROCEDENCIA_API};
pub use secreto::{AlmacenKeyring, SecretApiKey, resolver_api_key};
