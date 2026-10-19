//! Sesión de Claude Code ausente o caducada: el CLI responde con un aviso de autenticación
//! (texto plano o un sobre JSON con `is_error`). Se traduce a un mensaje claro y accionable
//! en lugar de mostrar al usuario la salida cruda del CLI.

/// Mensaje que ven la GUI, la CLI y el MCP cuando Claude Code no tiene sesión válida
pub const MENSAJE_SESION_CLAUDE: &str = "Claude Code no tiene una sesión válida: no has \
    iniciado sesión o la sesión ha caducado. Para arreglarlo, abre una terminal, ejecuta \
    `claude` y escribe `/login` (o ejecuta `claude auth login`); cuando termine, repite la \
    operación. No hace falta reiniciar la aplicación.";

/// Señales (en minúsculas) con las que el CLI de Claude Code informa de un fallo de sesión
const SENALES_FALLO_SESION: &[&str] = &[
    "not logged in",
    "please run /login",
    "invalid api key",
    "oauth token has expired",
    "oauth token revoked",
    "authentication_error",
    "invalid bearer token",
    "failed to authenticate",
    "oauth session expired",
];

/// ¿La salida del CLI (stdout y stderr) indica una sesión ausente o caducada?
pub fn es_fallo_de_sesion(salida_cli: &str) -> bool {
    let normalizada = salida_cli.to_lowercase();
    SENALES_FALLO_SESION
        .iter()
        .any(|senal| normalizada.contains(senal))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Salidas reales de `claude` 2.1.302 con un directorio de configuración vacío
    const TEXTO_REAL: &str = "Not logged in · Please run /login";
    const SOBRE_REAL: &str = r#"{"type":"result","subtype":"success","is_error":true,"terminal_reason":"api_error","result":"Not logged in · Please run /login"}"#;

    #[test]
    fn test_detecta_las_salidas_reales_y_las_variantes_de_caducidad() {
        for salida in [
            TEXTO_REAL,
            SOBRE_REAL,
            "Invalid API key · Please run /login",
            "API Error: 401 {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"OAuth token has expired. Please obtain a new token or refresh your existing token.\"}}",
            "OAuth token revoked · Please run /login",
            // Salida real capturada el 2026-10-19 con la sesión OAuth caducada
            "Failed to authenticate: OAuth session expired and could not be refreshed",
        ] {
            assert!(es_fallo_de_sesion(salida), "{salida}");
        }
    }

    #[test]
    fn test_no_confunde_otros_errores_ni_respuestas_normales() {
        for salida in [
            "rate limit exceeded",
            "API Error: 529 Overloaded",
            "El operador inicia sesión en el terminal de báscula (login) antes de pesar.",
            r#"{"type":"result","is_error":false,"result":"FOR-ALM-05"}"#,
            "",
        ] {
            assert!(!es_fallo_de_sesion(salida), "{salida}");
        }
    }

    #[test]
    fn test_el_mensaje_es_accionable_y_no_es_json() {
        assert!(MENSAJE_SESION_CLAUDE.contains("/login"));
        assert!(MENSAJE_SESION_CLAUDE.contains("caducado"));
        assert!(!MENSAJE_SESION_CLAUDE.contains('{'));
    }
}
