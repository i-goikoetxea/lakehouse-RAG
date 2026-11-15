//! Errores del cliente de la API de Messages. Ningún mensaje lleva la key, las cabeceras ni el
//! cuerpo de la petición; los de 400/401/403 dicen qué hacer.

use thiserror::Error;

use crate::domain::errors::DomainError;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum ErrorApiAnthropic {
    #[error(
        "No hay API key de Anthropic: define la variable ANTHROPIC_API_KEY o guárdala en el almacén de credenciales del sistema"
    )]
    KeyAusente,

    #[error("Almacén de credenciales del sistema: {0}")]
    AlmacenCredenciales(String),

    #[error(
        "La API de Anthropic rechazó la key (401): revisa que ANTHROPIC_API_KEY sea una key vigente de la consola de Anthropic"
    )]
    Autenticacion,

    #[error(
        "La API de Anthropic denegó el permiso (403): la key no tiene acceso a este modelo o a esta organización. {detalle}"
    )]
    Permiso { detalle: String },

    #[error("La API de Anthropic rechazó la petición (400): {detalle}")]
    PeticionInvalida { detalle: String },

    #[error("La API de Anthropic respondió {estado}: {detalle}")]
    EstadoNoReintentable { estado: u16, detalle: String },

    #[error(
        "La API de Anthropic sigue sin responder bien tras {intentos} intentos (último: {ultimo})"
    )]
    ReintentosAgotados { intentos: u32, ultimo: String },

    #[error("La respuesta se cortó por max_tokens: no se acepta como resultado")]
    RespuestaTruncada,

    #[error("El modelo rechazó la petición (stop_reason refusal)")]
    Rechazo,

    #[error("Respuesta de la API no válida: {0}")]
    RespuestaNoValida(String),
}

impl ErrorApiAnthropic {
    /// Un fallo de transporte o de servidor que merece otro intento (no gasta el del arnés)
    pub fn estado_reintentable(estado: u16) -> bool {
        matches!(estado, 429 | 529 | 500..=504)
    }
}

impl From<ErrorApiAnthropic> for DomainError {
    fn from(error: ErrorApiAnthropic) -> Self {
        DomainError::AgentError(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estados_reintentables_del_contrato() {
        for estado in [429, 529, 500, 501, 502, 503, 504] {
            assert!(ErrorApiAnthropic::estado_reintentable(estado), "{estado}");
        }
        for estado in [400, 401, 403, 404, 413, 505] {
            assert!(!ErrorApiAnthropic::estado_reintentable(estado), "{estado}");
        }
    }

    #[test]
    fn test_los_mensajes_de_400_401_403_son_accionables() {
        assert!(
            ErrorApiAnthropic::Autenticacion
                .to_string()
                .contains("ANTHROPIC_API_KEY")
        );
        assert!(
            ErrorApiAnthropic::Permiso {
                detalle: String::new()
            }
            .to_string()
            .contains("acceso")
        );
        assert!(
            ErrorApiAnthropic::PeticionInvalida {
                detalle: "campo x".into()
            }
            .to_string()
            .contains("campo x")
        );
    }
}
