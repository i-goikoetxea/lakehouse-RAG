//! Comandos de la API key para la ventana (Fase 5.2). El WebView solo escribe la key y lee un
//! estado con máscara: nunca la recibe de vuelta, ni en el estado ni en un error.

use serde::Serialize;

use crate::infrastructure::llm::anthropic_api::{AlmacenSecreto, SecretApiKey};
use crate::infrastructure::llm::modo_llm::ModoLlm;

/// Una key de la consola es corta y sin espacios: lo demás es un pegado erróneo
const LONGITUD_MAXIMA_KEY: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoApiKeyDto {
    pub configurada: bool,
    /// `sk-ant-…` y los 4 últimos caracteres
    pub mascara: Option<String>,
    /// De dónde sale la key que usará la app: `entorno`, `almacen` o ninguno
    pub origen: Option<String>,
    /// Modo del LLM con que arrancó la app (`api` o `cli`); se lee al arrancar
    pub modo_llm: String,
}

fn nombre_de_modo(modo: ModoLlm) -> String {
    match modo {
        ModoLlm::Api => "api",
        ModoLlm::Cli => "cli",
    }
    .to_string()
}

/// Estado de la key: la variable de entorno manda sobre el almacén, como en la composición
pub fn estado_api_key_en(
    valor_entorno: Option<String>,
    almacen: &dyn AlmacenSecreto,
    modo: ModoLlm,
) -> EstadoApiKeyDto {
    let (key, origen) = match SecretApiKey::desde_valor_de_entorno(valor_entorno) {
        Ok(key) => (Some(key), Some("entorno")),
        Err(_) => match almacen.leer() {
            Ok(Some(key)) => (Some(key), Some("almacen")),
            _ => (None, None),
        },
    };
    EstadoApiKeyDto {
        configurada: key.is_some(),
        mascara: key.map(|k| k.mascara_con_prefijo()),
        origen: origen.map(str::to_string),
        modo_llm: nombre_de_modo(modo),
    }
}

/// Guarda la key en el almacén y devuelve el estado. Los errores son fijos: nunca la repiten
pub fn guardar_api_key_en(
    key: String,
    valor_entorno: Option<String>,
    almacen: &dyn AlmacenSecreto,
    modo: ModoLlm,
) -> Result<EstadoApiKeyDto, String> {
    let limpia = key.trim();
    if limpia.chars().count() > LONGITUD_MAXIMA_KEY || limpia.chars().any(char::is_whitespace) {
        return Err(
            "La API key no tiene un formato válido: pégala sin espacios ni saltos de línea.".into(),
        );
    }
    let secreto = SecretApiKey::new(limpia).map_err(|_| "La API key está vacía.".to_string())?;
    drop(key);
    almacen.guardar(&secreto).map_err(|_| {
        "No se pudo guardar la API key en el almacén de credenciales del sistema.".to_string()
    })?;
    tracing::info!(mascara = %secreto.mascara(), "API key guardada en el almacén del sistema");
    Ok(estado_api_key_en(valor_entorno, almacen, modo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::llm::anthropic_api::AlmacenEnMemoria;

    const KEY_FALSA: &str = "sk-ant-api03-PRUEBA-no-real-9z8y";

    #[test]
    fn test_sin_key_el_estado_no_esta_configurado() {
        let estado = estado_api_key_en(None, &AlmacenEnMemoria::default(), ModoLlm::Cli);
        assert!(!estado.configurada);
        assert_eq!(estado.mascara, None);
        assert_eq!(estado.modo_llm, "cli");
    }

    #[test]
    fn test_guardar_devuelve_solo_la_mascara() {
        let almacen = AlmacenEnMemoria::default();
        let estado =
            guardar_api_key_en(format!(" {KEY_FALSA}\n"), None, &almacen, ModoLlm::Api).unwrap();
        assert_eq!(estado.mascara.as_deref(), Some("sk-ant-…9z8y"));
        assert_eq!(estado.origen.as_deref(), Some("almacen"));
        let payload = serde_json::to_string(&estado).unwrap();
        assert!(!payload.contains("PRUEBA"), "{payload}");
        assert_eq!(
            almacen.leer().unwrap().map(|k| k.mascara()),
            Some("…9z8y".into())
        );
    }

    #[test]
    fn test_la_variable_de_entorno_manda_en_el_estado() {
        let almacen = AlmacenEnMemoria::default();
        let estado = estado_api_key_en(Some(KEY_FALSA.into()), &almacen, ModoLlm::Api);
        assert_eq!(estado.origen.as_deref(), Some("entorno"));
    }

    #[test]
    fn test_los_errores_nunca_repiten_la_key() {
        let almacen = AlmacenEnMemoria::default();
        for mala in [
            String::new(),
            format!("{KEY_FALSA} {KEY_FALSA}"),
            "x".repeat(600),
        ] {
            let error = guardar_api_key_en(mala.clone(), None, &almacen, ModoLlm::Cli).unwrap_err();
            assert!(
                !error.contains("PRUEBA") && !error.contains("xxxx"),
                "{error}"
            );
        }
        assert_eq!(almacen.leer().unwrap(), None);
    }
}
