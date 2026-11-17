//! La API key de Anthropic como tipo opaco. Nunca se escribe en logs, errores ni disco: `Debug` y
//! `Display` dan `[REDACTED]` (SSD de la Fase 5.1 §3).

use std::fmt;

use super::error::ErrorApiAnthropic;

/// Variable de entorno de la key en la CLI, los benchmarks y los tests
pub const VARIABLE_API_KEY: &str = "ANTHROPIC_API_KEY";
/// Servicio y usuario de la entrada del almacén de credenciales del sistema
pub const SERVICIO_KEYRING: &str = "lakehouse_rag";
pub const USUARIO_KEYRING: &str = "anthropic_api_key";
/// Variable de entorno de la key unificada del proxy FreeLLMAPI: manda sobre el almacén
pub const VARIABLE_FREELLMAPI_KEY: &str = "FREELLMAPI_API_KEY";
/// Entrada propia del almacén para la key de FreeLLMAPI, distinta de la de Anthropic
pub const USUARIO_KEYRING_FREELLMAPI: &str = "freellmapi_api_key";
const OCULTA: &str = "[REDACTED]";

#[derive(Clone, PartialEq, Eq)]
pub struct SecretApiKey(String);

impl SecretApiKey {
    /// Una key vacía o solo con espacios no es una key
    pub fn new(valor: impl Into<String>) -> Result<Self, ErrorApiAnthropic> {
        let valor = valor.into().trim().to_string();
        if valor.is_empty() {
            return Err(ErrorApiAnthropic::KeyAusente);
        }
        Ok(Self(valor))
    }

    /// Lee la key del valor de una variable de entorno (`None` si no está definida)
    pub fn desde_valor_de_entorno(valor: Option<String>) -> Result<Self, ErrorApiAnthropic> {
        valor.map_or(Err(ErrorApiAnthropic::KeyAusente), Self::new)
    }

    /// `ANTHROPIC_API_KEY` del proceso
    pub fn desde_entorno() -> Result<Self, ErrorApiAnthropic> {
        Self::desde_valor_de_entorno(std::env::var(VARIABLE_API_KEY).ok())
    }

    /// Solo el cliente HTTP la lee, para la cabecera `x-api-key`
    pub(crate) fn exponer(&self) -> &str {
        &self.0
    }

    /// Máscara para mostrar que hay key sin revelarla: los 4 últimos caracteres
    pub fn mascara(&self) -> String {
        let cola: String = self
            .0
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("…{cola}")
    }
}

impl SecretApiKey {
    /// Máscara para la GUI: el prefijo público (`sk-ant-` o `freellmapi-`, si lo tiene) y los 4
    /// últimos caracteres
    pub fn mascara_con_prefijo(&self) -> String {
        let prefijo = PREFIJOS_PUBLICOS
            .iter()
            .find(|p| self.0.starts_with(**p))
            .copied()
            .unwrap_or("");
        format!("{prefijo}{}", self.mascara())
    }
}

/// Prefijos comunes a todas las keys de la consola de Anthropic y de FreeLLMAPI: no identifican
/// ninguna
const PREFIJOS_PUBLICOS: [&str; 2] = ["sk-ant-", "freellmapi-"];

/// Dónde se guarda la key de la GUI. El de producción es el almacén del sistema; los tests usan
/// uno en memoria y nunca tocan el del usuario
pub trait AlmacenSecreto: Send + Sync {
    fn guardar(&self, key: &SecretApiKey) -> Result<(), ErrorApiAnthropic>;
    fn leer(&self) -> Result<Option<SecretApiKey>, ErrorApiAnthropic>;
}

impl AlmacenSecreto for AlmacenKeyring {
    fn guardar(&self, key: &SecretApiKey) -> Result<(), ErrorApiAnthropic> {
        AlmacenKeyring::guardar(self, key)
    }

    fn leer(&self) -> Result<Option<SecretApiKey>, ErrorApiAnthropic> {
        AlmacenKeyring::leer(self)
    }
}

/// Almacén en memoria (tests y herramientas)
#[derive(Default)]
pub struct AlmacenEnMemoria(std::sync::Mutex<Option<SecretApiKey>>);

impl AlmacenSecreto for AlmacenEnMemoria {
    fn guardar(&self, key: &SecretApiKey) -> Result<(), ErrorApiAnthropic> {
        *self
            .0
            .lock()
            .map_err(|_| ErrorApiAnthropic::AlmacenCredenciales("bloqueo envenenado".into()))? =
            Some(key.clone());
        Ok(())
    }

    fn leer(&self) -> Result<Option<SecretApiKey>, ErrorApiAnthropic> {
        Ok(self.0.lock().ok().and_then(|k| k.clone()))
    }
}

impl fmt::Debug for SecretApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(OCULTA)
    }
}

impl fmt::Display for SecretApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(OCULTA)
    }
}

/// Almacén de credenciales del sistema (Windows: Credential Manager). Sin comandos de GUI en la 5.1
pub struct AlmacenKeyring {
    servicio: String,
    usuario: String,
}

impl Default for AlmacenKeyring {
    fn default() -> Self {
        Self::new(SERVICIO_KEYRING, USUARIO_KEYRING)
    }
}

impl AlmacenKeyring {
    /// La entrada de la key de FreeLLMAPI
    pub fn freellmapi() -> Self {
        Self::new(SERVICIO_KEYRING, USUARIO_KEYRING_FREELLMAPI)
    }

    pub fn usuario(&self) -> &str {
        &self.usuario
    }

    pub fn new(servicio: impl Into<String>, usuario: impl Into<String>) -> Self {
        Self {
            servicio: servicio.into(),
            usuario: usuario.into(),
        }
    }

    fn entrada(&self) -> Result<keyring::Entry, ErrorApiAnthropic> {
        keyring::Entry::new(&self.servicio, &self.usuario)
            .map_err(|e| ErrorApiAnthropic::AlmacenCredenciales(e.to_string()))
    }

    pub fn guardar(&self, key: &SecretApiKey) -> Result<(), ErrorApiAnthropic> {
        self.entrada()?
            .set_password(key.exponer())
            .map_err(|e| ErrorApiAnthropic::AlmacenCredenciales(e.to_string()))
    }

    /// `Ok(None)` si no hay key guardada
    pub fn leer(&self) -> Result<Option<SecretApiKey>, ErrorApiAnthropic> {
        match self.entrada()?.get_password() {
            Ok(valor) => SecretApiKey::new(valor).map(Some),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(ErrorApiAnthropic::AlmacenCredenciales(e.to_string())),
        }
    }

    pub fn borrar(&self) -> Result<(), ErrorApiAnthropic> {
        match self.entrada()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(ErrorApiAnthropic::AlmacenCredenciales(e.to_string())),
        }
    }
}

/// Orden de búsqueda: la variable de entorno y, si no está, el almacén del sistema
pub fn resolver_api_key(
    valor_entorno: Option<String>,
    almacen: &AlmacenKeyring,
) -> Result<SecretApiKey, ErrorApiAnthropic> {
    match SecretApiKey::desde_valor_de_entorno(valor_entorno) {
        Ok(key) => Ok(key),
        Err(_) => almacen.leer()?.ok_or(ErrorApiAnthropic::KeyAusente),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_FALSA: &str = "sk-ant-prueba-0123456789abcdef";

    #[test]
    fn test_debug_y_display_ocultan_la_key() {
        let key = SecretApiKey::new(KEY_FALSA).unwrap();
        assert_eq!(format!("{key:?}"), "[REDACTED]");
        assert_eq!(format!("{key}"), "[REDACTED]");
        assert!(!format!("{:?}", Some(key.clone())).contains("sk-ant"));
        assert_eq!(key.exponer(), KEY_FALSA);
    }

    #[test]
    fn test_key_vacia_o_ausente_es_un_error() {
        assert!(SecretApiKey::new("").is_err());
        assert!(SecretApiKey::new("   \n").is_err());
        assert!(matches!(
            SecretApiKey::desde_valor_de_entorno(None),
            Err(ErrorApiAnthropic::KeyAusente)
        ));
        assert!(SecretApiKey::desde_valor_de_entorno(Some(format!(" {KEY_FALSA} "))).is_ok());
    }

    #[test]
    fn test_la_mascara_solo_muestra_la_cola() {
        let key = SecretApiKey::new(KEY_FALSA).unwrap();
        assert_eq!(key.mascara(), "…cdef");
        assert!(!key.mascara().contains("sk-ant"));
    }

    #[test]
    fn test_la_mascara_de_la_gui_lleva_solo_el_prefijo_publico_y_la_cola() {
        let key = SecretApiKey::new(KEY_FALSA).unwrap();
        assert_eq!(key.mascara_con_prefijo(), "sk-ant-…cdef");
        let otra = SecretApiKey::new("clave-sin-prefijo-1234").unwrap();
        assert_eq!(otra.mascara_con_prefijo(), "…1234");
        let proxy = SecretApiKey::new("freellmapi-NO-ES-REAL-abcd9876").unwrap();
        assert_eq!(proxy.mascara_con_prefijo(), "freellmapi-…9876");
    }

    #[test]
    fn test_la_key_de_freellmapi_tiene_su_propia_entrada_en_el_almacen() {
        assert_eq!(AlmacenKeyring::freellmapi().usuario(), "freellmapi_api_key");
        assert_ne!(
            AlmacenKeyring::freellmapi().usuario(),
            AlmacenKeyring::default().usuario()
        );
    }

    #[test]
    fn test_la_variable_de_entorno_tiene_prioridad_sobre_el_almacen() {
        let almacen = AlmacenKeyring::new("lakehouse_rag_test_inexistente", "sin_usuario");
        let key = resolver_api_key(Some(KEY_FALSA.into()), &almacen).unwrap();
        assert_eq!(key.exponer(), KEY_FALSA);
    }

    /// Toca el almacén real del sistema: se ejecuta a mano
    #[test]
    #[ignore]
    fn test_keyring_guarda_lee_y_borra() {
        let almacen = AlmacenKeyring::new("lakehouse_rag_test", "key_de_prueba");
        let key = SecretApiKey::new(KEY_FALSA).unwrap();
        almacen.guardar(&key).unwrap();
        assert_eq!(almacen.leer().unwrap(), Some(key));
        almacen.borrar().unwrap();
        assert_eq!(almacen.leer().unwrap(), None);
    }
}
