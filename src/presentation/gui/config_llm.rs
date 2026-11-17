//! Configuración del motor del LLM desde la ventana: qué motor está en uso, cuál se ha elegido
//! y la URL del proxy FreeLLMAPI. Lo que se guarda se persiste y se aplica al instante, sin
//! reiniciar la app (`MotorLlmVivo`); una consulta en curso termina con el motor con que empezó.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::infrastructure::llm::modo_llm::{ModoLlm, VARIABLE_MODO_LLM};
use crate::infrastructure::llm::motor_conmutable::MotorConmutable;
use crate::infrastructure::llm::preferencia_modo::{
    ModoResuelto, OrigenModo, PreferenciasLlm, es_url_local, guardar_modo, guardar_url_freellmapi,
    leer_preferencias, modo_estricto, nombre_de_modo, resolver_modo,
};

/// Instantánea del motor compuesto ahora mismo (lo que leen las funciones puras del modal)
#[derive(Debug, Clone)]
pub struct MotorLlmSesion {
    pub directorio_config: PathBuf,
    /// Motor pedido en la composición vigente (al arrancar o al último guardado) y su origen
    pub pedido_al_arrancar: ModoResuelto,
    pub en_uso: ModoLlm,
    /// URL de FreeLLMAPI con que se compuso el motor vigente (la use o no)
    pub freellmapi_url: String,
}

/// Motor vivo de la ventana; se registra como estado de Tauri. Guardar un motor, una URL o una
/// key lo recompone al instante con la misma prelación que al arrancar (entorno > fichero > CLI)
pub struct MotorLlmVivo {
    pub directorio_config: PathBuf,
    pub motor: Arc<MotorConmutable>,
}

impl MotorLlmVivo {
    pub fn sesion(&self) -> MotorLlmSesion {
        let actual = self.motor.actual();
        MotorLlmSesion {
            directorio_config: self.directorio_config.clone(),
            pedido_al_arrancar: actual.pedido,
            en_uso: actual.en_uso,
            freellmapi_url: actual.freellmapi_url.clone(),
        }
    }

    /// Aplica las preferencias guardadas. `forzar`: se guardó la key de `modo_de_la_key`; si es
    /// el motor pedido, se recompone aunque no cambie nada más
    pub fn aplicar(&self, valor_entorno: Option<&str>, key_guardada: Option<ModoLlm>) {
        let guardadas = leer_preferencias(&self.directorio_config);
        let pedido = resolver_modo(valor_entorno, guardadas.modo);
        let forzar = key_guardada == Some(pedido.modo);
        self.motor
            .aplicar(pedido, guardadas.url_freellmapi(), forzar);
    }
}

/// Qué keys hay disponibles para cada motor (variable de entorno o almacén)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeysConfiguradas {
    pub anthropic: bool,
    pub freellmapi: bool,
}

impl KeysConfiguradas {
    /// El motor tiene lo que necesita para arrancar (la CLI no necesita key)
    pub fn tiene_key(self, modo: ModoLlm) -> bool {
        match modo {
            ModoLlm::Cli => true,
            ModoLlm::Api => self.anthropic,
            ModoLlm::FreeLlmApi => self.freellmapi,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoModoLlmDto {
    /// Motor que atiende el chat en esta sesión (`cli`, `api` o `freellmapi`)
    pub en_uso: String,
    /// Lo guardado en `llm_config.json`, si hay algo válido
    pub elegido: Option<String>,
    /// Motor pedido según lo guardado, con la prelación entorno > fichero > `cli`. Se aplica al
    /// guardar; el nombre del campo se conserva por compatibilidad
    pub al_reiniciar: String,
    /// `entorno`, `fichero` o `por_defecto`
    pub origen_al_reiniciar: String,
    /// `LAKEHOUSE_RAG_MODO_LLM` está definida y manda sobre lo que se elija en la ventana
    pub entorno_manda: bool,
    /// Se pidió un motor con API al arrancar, pero no había cliente: esta sesión usa `cli`
    pub respaldo_cli: bool,
    /// Lo guardado aún no es el motor (o la URL del proxy) en uso: el motor vivo lo aplica al
    /// guardar, así que en la ventana solo dura hasta la respuesta del comando
    pub pendiente_de_reinicio: bool,
    /// URL de FreeLLMAPI guardada (o la de la app de escritorio)
    pub freellmapi_url: String,
    /// La URL apunta a este equipo
    pub freellmapi_url_local: bool,
    pub avisos: Vec<String>,
}

fn nombre_visible(modo: ModoLlm) -> &'static str {
    match modo {
        ModoLlm::Cli => "CLI",
        ModoLlm::Api => "API",
        ModoLlm::FreeLlmApi => "FreeLLMAPI",
    }
}

/// Todo lo que el modal necesita saber del motor, sin tocar disco ni entorno
pub fn estado_modo_llm_en(
    sesion: &MotorLlmSesion,
    valor_entorno: Option<&str>,
    guardadas: &PreferenciasLlm,
    keys: KeysConfiguradas,
) -> EstadoModoLlmDto {
    let siguiente = resolver_modo(valor_entorno, guardadas.modo);
    let entorno_manda = siguiente.origen == OrigenModo::Entorno;
    let respaldo_cli =
        sesion.pedido_al_arrancar.modo != ModoLlm::Cli && sesion.en_uso == ModoLlm::Cli;
    let falta_key = !keys.tiene_key(siguiente.modo);
    // Sin key, pedir un motor con API acaba en `cli`: reiniciar solo cambia algo si cambia el
    // motor efectivo (o la URL del proxy en uso)
    let efectivo_al_reiniciar = if falta_key {
        ModoLlm::Cli
    } else {
        siguiente.modo
    };
    let url = guardadas.url_freellmapi().to_string();
    let url_local = es_url_local(&url);
    let cambia_url = efectivo_al_reiniciar == ModoLlm::FreeLlmApi
        && sesion.en_uso == ModoLlm::FreeLlmApi
        && url != sesion.freellmapi_url;
    let pendiente_de_reinicio = efectivo_al_reiniciar != sesion.en_uso || cambia_url;
    let mut avisos = Vec::new();
    if entorno_manda {
        avisos.push(format!(
            "La variable {VARIABLE_MODO_LLM} está definida y manda sobre lo que elijas aquí."
        ));
    }
    if respaldo_cli {
        avisos.push(format!(
            "Se pidió el modo {}, pero no hay una key válida: se usa CLI.",
            nombre_visible(sesion.pedido_al_arrancar.modo)
        ));
    }
    match siguiente.modo {
        ModoLlm::Api if falta_key => avisos.push(
            "El modo API necesita una API key. Sin ella, la app seguirá usando CLI.".to_string(),
        ),
        ModoLlm::FreeLlmApi if falta_key => avisos.push(
            "El modo FreeLLMAPI necesita su key (freellmapi-…). Sin ella, la app seguirá usando CLI."
                .to_string(),
        ),
        _ => {}
    }
    let usa_proxy = siguiente.modo == ModoLlm::FreeLlmApi || sesion.en_uso == ModoLlm::FreeLlmApi;
    if usa_proxy && !url_local {
        avisos.push(format!(
            "La URL de FreeLLMAPI no es de este equipo: la key y los prompts viajan por la red hasta {url}."
        ));
    }
    EstadoModoLlmDto {
        en_uso: nombre_de_modo(sesion.en_uso).to_string(),
        elegido: guardadas.modo.map(|m| nombre_de_modo(m).to_string()),
        al_reiniciar: nombre_de_modo(siguiente.modo).to_string(),
        origen_al_reiniciar: siguiente.origen.nombre().to_string(),
        entorno_manda,
        respaldo_cli,
        pendiente_de_reinicio,
        freellmapi_url: url,
        freellmapi_url_local: url_local,
        avisos,
    }
}

/// Persiste la elección (`cli`, `api` o `freellmapi`) y devuelve el estado. El error es fijo:
/// no repite la entrada, que llega de la ventana
pub fn guardar_modo_llm_en(
    sesion: &MotorLlmSesion,
    modo: &str,
    valor_entorno: Option<&str>,
    keys: KeysConfiguradas,
) -> Result<EstadoModoLlmDto, String> {
    let elegido = modo_estricto(modo)
        .ok_or_else(|| "Modo desconocido: elige CLI, API o FreeLLMAPI.".to_string())?;
    guardar_modo(&sesion.directorio_config, elegido).map_err(|_| {
        "No se pudo guardar el modo en el directorio de configuración de la app.".to_string()
    })?;
    tracing::info!(modo = nombre_de_modo(elegido), "modo del LLM guardado");
    Ok(estado_modo_llm_desde_disco(sesion, valor_entorno, keys))
}

/// Persiste la URL de FreeLLMAPI (validada) y devuelve el estado. El error es fijo
pub fn guardar_url_freellmapi_en(
    sesion: &MotorLlmSesion,
    url: &str,
    valor_entorno: Option<&str>,
    keys: KeysConfiguradas,
) -> Result<EstadoModoLlmDto, String> {
    let guardada = guardar_url_freellmapi(&sesion.directorio_config, url).map_err(|_| {
        "No se pudo guardar la URL en el directorio de configuración de la app.".to_string()
    })?;
    if guardada.is_none() {
        return Err(
            "URL no válida: usa http(s)://equipo:puerto, sin ruta, credenciales ni parámetros."
                .to_string(),
        );
    }
    tracing::info!("URL de FreeLLMAPI guardada");
    Ok(estado_modo_llm_desde_disco(sesion, valor_entorno, keys))
}

/// `estado_modo_llm_en` leyendo el fichero de la sesión
pub fn estado_modo_llm_desde_disco(
    sesion: &MotorLlmSesion,
    valor_entorno: Option<&str>,
    keys: KeysConfiguradas,
) -> EstadoModoLlmDto {
    estado_modo_llm_en(
        sesion,
        valor_entorno,
        &leer_preferencias(&sesion.directorio_config),
        keys,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::llm::preferencia_modo::leer_modo_guardado;
    use std::path::Path;

    fn sesion(dir: &Path, pedido: ModoLlm, origen: OrigenModo, en_uso: ModoLlm) -> MotorLlmSesion {
        MotorLlmSesion {
            directorio_config: dir.to_path_buf(),
            pedido_al_arrancar: ModoResuelto {
                modo: pedido,
                origen,
            },
            en_uso,
            freellmapi_url: "http://127.0.0.1:31415".into(),
        }
    }

    const CON_KEYS: KeysConfiguradas = KeysConfiguradas {
        anthropic: true,
        freellmapi: true,
    };
    const SIN_KEYS: KeysConfiguradas = KeysConfiguradas {
        anthropic: false,
        freellmapi: false,
    };

    fn por_defecto(dir: &Path) -> MotorLlmSesion {
        sesion(dir, ModoLlm::Cli, OrigenModo::PorDefecto, ModoLlm::Cli)
    }

    #[test]
    fn test_sin_configuracion_todo_es_cli_y_sin_avisos() {
        let dir = tempfile::tempdir().unwrap();
        let estado = estado_modo_llm_desde_disco(&por_defecto(dir.path()), None, SIN_KEYS);
        assert_eq!(estado.en_uso, "cli");
        assert_eq!(estado.al_reiniciar, "cli");
        assert_eq!(estado.elegido, None);
        assert_eq!(estado.origen_al_reiniciar, "por_defecto");
        assert!(!estado.pendiente_de_reinicio);
        assert!(estado.avisos.is_empty(), "{:?}", estado.avisos);
    }

    #[test]
    fn test_elegir_api_con_key_queda_pendiente_de_reinicio() {
        let dir = tempfile::tempdir().unwrap();
        let estado = guardar_modo_llm_en(&por_defecto(dir.path()), "api", None, CON_KEYS).unwrap();
        assert_eq!(
            estado.en_uso, "cli",
            "el motor en uso no cambia en caliente"
        );
        assert_eq!(estado.elegido.as_deref(), Some("api"));
        assert_eq!(estado.al_reiniciar, "api");
        assert_eq!(estado.origen_al_reiniciar, "fichero");
        assert!(estado.pendiente_de_reinicio);
        assert!(!estado.avisos.iter().any(|a| a.contains("reinici")));
    }

    #[test]
    fn test_elegir_api_sin_key_avisa_del_respaldo_a_cli() {
        let dir = tempfile::tempdir().unwrap();
        let estado = guardar_modo_llm_en(&por_defecto(dir.path()), "api", None, SIN_KEYS).unwrap();
        assert_eq!(estado.al_reiniciar, "api");
        assert!(
            estado
                .avisos
                .iter()
                .any(|a| a.contains("necesita una API key"))
        );
        assert!(
            !estado.pendiente_de_reinicio,
            "sin key, reiniciar sigue en CLI"
        );
    }

    #[test]
    fn test_el_entorno_manda_y_se_dice() {
        let dir = tempfile::tempdir().unwrap();
        let estado =
            guardar_modo_llm_en(&por_defecto(dir.path()), "api", Some("cli"), CON_KEYS).unwrap();
        assert_eq!(estado.elegido.as_deref(), Some("api"));
        assert_eq!(estado.al_reiniciar, "cli");
        assert!(estado.entorno_manda);
        assert!(!estado.pendiente_de_reinicio);
        assert!(estado.avisos.iter().any(|a| a.contains(VARIABLE_MODO_LLM)));
    }

    #[test]
    fn test_api_pedida_sin_cliente_muestra_respaldo_y_no_promete_un_cambio() {
        let dir = tempfile::tempdir().unwrap();
        let sesion = sesion(dir.path(), ModoLlm::Api, OrigenModo::Fichero, ModoLlm::Cli);
        guardar_modo(dir.path(), ModoLlm::Api).unwrap();
        let sin_key = estado_modo_llm_desde_disco(&sesion, None, SIN_KEYS);
        assert!(sin_key.respaldo_cli);
        assert_eq!(sin_key.en_uso, "cli");
        assert!(
            !sin_key.pendiente_de_reinicio,
            "sin key, reiniciar no cambia nada"
        );
        let con_key = estado_modo_llm_desde_disco(&sesion, None, CON_KEYS);
        assert!(
            con_key.pendiente_de_reinicio,
            "con la key guardada, reiniciar activa la API"
        );
    }

    #[test]
    fn test_volver_a_cli_desde_api_en_uso() {
        let dir = tempfile::tempdir().unwrap();
        let sesion = sesion(dir.path(), ModoLlm::Api, OrigenModo::Fichero, ModoLlm::Api);
        let estado = guardar_modo_llm_en(&sesion, "CLI", None, CON_KEYS).unwrap();
        assert_eq!(estado.en_uso, "api");
        assert_eq!(estado.al_reiniciar, "cli");
        assert!(estado.pendiente_de_reinicio);
    }

    #[test]
    fn test_un_modo_desconocido_no_se_guarda_ni_se_repite() {
        let dir = tempfile::tempdir().unwrap();
        for malo in ["openai", "<script>", "api\n{\"x\":1}", ""] {
            let error =
                guardar_modo_llm_en(&por_defecto(dir.path()), malo, None, CON_KEYS).unwrap_err();
            assert!(
                !error.contains("openai") && !error.contains("script"),
                "{error}"
            );
        }
        assert_eq!(leer_modo_guardado(dir.path()), None);
    }

    #[test]
    fn test_elegir_freellmapi_con_su_key_queda_pendiente_y_sin_ella_avisa() {
        let dir = tempfile::tempdir().unwrap();
        let solo_anthropic = KeysConfiguradas {
            anthropic: true,
            freellmapi: false,
        };
        let estado =
            guardar_modo_llm_en(&por_defecto(dir.path()), "freellmapi", None, solo_anthropic)
                .unwrap();
        assert_eq!(estado.al_reiniciar, "freellmapi");
        assert!(
            !estado.pendiente_de_reinicio,
            "la key de Anthropic no sirve para el proxy"
        );
        assert!(estado.avisos.iter().any(|a| a.contains("freellmapi-…")));
        let con_key = estado_modo_llm_desde_disco(&por_defecto(dir.path()), None, CON_KEYS);
        assert!(con_key.pendiente_de_reinicio);
        assert_eq!(con_key.freellmapi_url, "http://127.0.0.1:31415");
        assert!(con_key.freellmapi_url_local);
    }

    #[test]
    fn test_cambiar_la_url_del_proxy_en_uso_queda_pendiente_y_una_remota_avisa() {
        let dir = tempfile::tempdir().unwrap();
        let sesion = sesion(
            dir.path(),
            ModoLlm::FreeLlmApi,
            OrigenModo::Fichero,
            ModoLlm::FreeLlmApi,
        );
        guardar_modo(dir.path(), ModoLlm::FreeLlmApi).unwrap();
        let igual = estado_modo_llm_desde_disco(&sesion, None, CON_KEYS);
        assert!(!igual.pendiente_de_reinicio, "{:?}", igual.avisos);
        let estado =
            guardar_url_freellmapi_en(&sesion, "http://192.168.1.20:3001", None, CON_KEYS).unwrap();
        assert!(estado.pendiente_de_reinicio);
        assert!(!estado.freellmapi_url_local);
        assert!(
            estado
                .avisos
                .iter()
                .any(|a| a.contains("viajan por la red"))
        );
    }

    #[test]
    fn test_una_url_hostil_no_se_guarda_ni_se_repite() {
        let dir = tempfile::tempdir().unwrap();
        for mala in [
            "javascript:alert(1)",
            "freellmapi-SECRETO-1234",
            "http://a:b@h",
        ] {
            let error = guardar_url_freellmapi_en(&por_defecto(dir.path()), mala, None, CON_KEYS)
                .unwrap_err();
            assert!(
                !error.contains("SECRETO") && !error.contains("alert"),
                "{error}"
            );
        }
        assert_eq!(leer_preferencias(dir.path()).freellmapi_url, None);
    }

    #[test]
    fn test_freellmapi_pedido_sin_key_muestra_el_respaldo() {
        let dir = tempfile::tempdir().unwrap();
        let sesion = sesion(
            dir.path(),
            ModoLlm::FreeLlmApi,
            OrigenModo::Entorno,
            ModoLlm::Cli,
        );
        let estado = estado_modo_llm_desde_disco(&sesion, Some("freellmapi"), SIN_KEYS);
        assert!(estado.respaldo_cli);
        assert!(
            estado
                .avisos
                .iter()
                .any(|a| a.contains("Se pidió el modo FreeLLMAPI"))
        );
    }

    // --- Motor vivo: cambio sin reiniciar ---

    mod vivo {
        use super::*;
        use crate::application::dtos::agent::{
            AgentMessage, AgentTurnOutput, ClaudeModel, ModelDescriptor,
        };
        use crate::application::dtos::procedencia::{SolicitudProcedencia, TrazasPropuestas};
        use crate::application::dtos::tools::ToolDefinition;
        use crate::application::ports::agent_runner::AgentRunnerPort;
        use crate::application::ports::model_catalog::ModelCatalogPort;
        use crate::application::ports::verificador_procedencia::VerificadorProcedenciaPort;
        use crate::domain::errors::DomainError;
        use crate::infrastructure::llm::motor_conmutable::{Compositor, MotorCompuesto};
        use async_trait::async_trait;
        use std::sync::Mutex;

        struct Nulo;

        #[async_trait]
        impl AgentRunnerPort for Nulo {
            async fn ejecutar_turno(
                &self,
                _: &ClaudeModel,
                _: &str,
                _: &[AgentMessage],
                _: &[ToolDefinition],
            ) -> Result<AgentTurnOutput, DomainError> {
                Ok(AgentTurnOutput::FinalMessage(String::new()))
            }
        }

        #[async_trait]
        impl VerificadorProcedenciaPort for Nulo {
            async fn proponer_trazas(
                &self,
                _: &SolicitudProcedencia,
            ) -> Result<TrazasPropuestas, DomainError> {
                Err(DomainError::AgentError(String::new()))
            }
        }

        #[async_trait]
        impl ModelCatalogPort for Nulo {
            async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
                Ok(vec![])
            }
        }

        /// Compone siempre el motor pedido (como si hubiera key) y cuenta las composiciones
        fn compositor(composiciones: Arc<Mutex<usize>>) -> Compositor {
            Arc::new(move |pedido: ModoResuelto, url: &str| {
                *composiciones.lock().unwrap() += 1;
                MotorCompuesto {
                    pedido,
                    en_uso: pedido.modo,
                    freellmapi_url: url.to_string(),
                    runner: Arc::new(Nulo),
                    api: None,
                    verificador: Arc::new(Nulo),
                    catalogo: Arc::new(Nulo),
                }
            })
        }

        fn vivo(dir: &Path, composiciones: Arc<Mutex<usize>>) -> MotorLlmVivo {
            MotorLlmVivo {
                directorio_config: dir.to_path_buf(),
                motor: MotorConmutable::new(
                    compositor(composiciones),
                    ModoResuelto {
                        modo: ModoLlm::Cli,
                        origen: OrigenModo::PorDefecto,
                    },
                    "http://127.0.0.1:31415",
                ),
            }
        }

        #[test]
        fn test_guardar_otro_motor_lo_pone_en_uso_al_instante_sin_aviso_de_reinicio() {
            let dir = tempfile::tempdir().unwrap();
            let vivo = vivo(dir.path(), Arc::default());
            guardar_modo_llm_en(&vivo.sesion(), "freellmapi", None, CON_KEYS).unwrap();
            vivo.aplicar(None, None);
            let estado = estado_modo_llm_desde_disco(&vivo.sesion(), None, CON_KEYS);
            assert_eq!(estado.en_uso, "freellmapi");
            assert!(!estado.pendiente_de_reinicio);
            assert!(
                !estado.avisos.iter().any(|a| a.contains("reinici")),
                "{:?}",
                estado.avisos
            );
        }

        #[test]
        fn test_con_la_variable_de_entorno_guardar_no_cambia_el_motor() {
            let dir = tempfile::tempdir().unwrap();
            let vivo = vivo(dir.path(), Arc::default());
            guardar_modo_llm_en(&vivo.sesion(), "api", Some("cli"), CON_KEYS).unwrap();
            vivo.aplicar(Some("cli"), None);
            assert_eq!(vivo.sesion().en_uso, ModoLlm::Cli);
            assert_eq!(vivo.sesion().pedido_al_arrancar.origen, OrigenModo::Entorno);
        }

        #[test]
        fn test_guardar_la_key_del_motor_pedido_lo_recompone_y_la_de_otro_no() {
            let dir = tempfile::tempdir().unwrap();
            let composiciones: Arc<Mutex<usize>> = Arc::default();
            let vivo = vivo(dir.path(), Arc::clone(&composiciones));
            guardar_modo(dir.path(), ModoLlm::Api).unwrap();
            vivo.aplicar(None, None);
            assert_eq!(*composiciones.lock().unwrap(), 2);
            vivo.aplicar(None, Some(ModoLlm::FreeLlmApi));
            assert_eq!(*composiciones.lock().unwrap(), 2, "key de otro motor");
            vivo.aplicar(None, Some(ModoLlm::Api));
            assert_eq!(*composiciones.lock().unwrap(), 3, "key del motor pedido");
        }
    }
}
