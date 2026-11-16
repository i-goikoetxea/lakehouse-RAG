//! Configuración del motor del LLM desde la ventana: qué motor está en uso, cuál se ha elegido
//! y cuál se usará al reiniciar. El runner se compone al arrancar y no se cambia en caliente
//! (`docs/arquitectura/ssd_gui_config_llm.md`): la elección se persiste y se aplica al reiniciar.

use std::path::PathBuf;

use serde::Serialize;

use crate::infrastructure::llm::modo_llm::{ModoLlm, VARIABLE_MODO_LLM};
use crate::infrastructure::llm::preferencia_modo::{
    ModoResuelto, OrigenModo, guardar_modo, leer_modo_guardado, modo_estricto, nombre_de_modo,
    resolver_modo,
};

/// Motor con que se compuso esta sesión de la app; se registra como estado de Tauri
#[derive(Debug, Clone)]
pub struct MotorLlmSesion {
    pub directorio_config: PathBuf,
    pub pedido_al_arrancar: ModoResuelto,
    pub en_uso: ModoLlm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoModoLlmDto {
    /// Motor que atiende el chat en esta sesión de la app (`cli` o `api`)
    pub en_uso: String,
    /// Lo guardado en `llm_config.json`, si hay algo válido
    pub elegido: Option<String>,
    /// Motor que se pedirá en el próximo arranque, con la prelación entorno > fichero > `cli`
    pub al_reiniciar: String,
    /// `entorno`, `fichero` o `por_defecto`
    pub origen_al_reiniciar: String,
    /// `LAKEHOUSE_RAG_MODO_LLM` está definida y manda sobre lo que se elija en la ventana
    pub entorno_manda: bool,
    /// Se pidió `api` al arrancar, pero no había cliente: esta sesión usa `cli`
    pub respaldo_cli: bool,
    /// El motor del próximo arranque no es el que está en uso
    pub pendiente_de_reinicio: bool,
    pub avisos: Vec<String>,
}

/// Todo lo que el modal necesita saber del modo, sin tocar disco ni entorno
pub fn estado_modo_llm_en(
    sesion: &MotorLlmSesion,
    valor_entorno: Option<&str>,
    guardado: Option<ModoLlm>,
    key_configurada: bool,
) -> EstadoModoLlmDto {
    let siguiente = resolver_modo(valor_entorno, guardado);
    let entorno_manda = siguiente.origen == OrigenModo::Entorno;
    let respaldo_cli =
        sesion.pedido_al_arrancar.modo == ModoLlm::Api && sesion.en_uso == ModoLlm::Cli;
    // Sin key, pedir `api` acaba en `cli`: reiniciar solo cambia algo si cambia el motor efectivo
    let efectivo_al_reiniciar = if siguiente.modo == ModoLlm::Api && !key_configurada {
        ModoLlm::Cli
    } else {
        siguiente.modo
    };
    let pendiente_de_reinicio = efectivo_al_reiniciar != sesion.en_uso;
    let mut avisos = Vec::new();
    if entorno_manda {
        avisos.push(format!(
            "La variable {VARIABLE_MODO_LLM} está definida y manda sobre lo que elijas aquí."
        ));
    }
    if respaldo_cli {
        avisos.push(
            "Se pidió el modo API al arrancar, pero no había una API key válida: esta sesión usa CLI."
                .to_string(),
        );
    }
    if siguiente.modo == ModoLlm::Api && !key_configurada {
        avisos.push(
            "El modo API necesita una API key. Sin ella, la app seguirá usando CLI.".to_string(),
        );
    }
    if pendiente_de_reinicio {
        avisos.push("El cambio de motor se aplica al reiniciar la app.".to_string());
    }
    EstadoModoLlmDto {
        en_uso: nombre_de_modo(sesion.en_uso).to_string(),
        elegido: guardado.map(|m| nombre_de_modo(m).to_string()),
        al_reiniciar: nombre_de_modo(siguiente.modo).to_string(),
        origen_al_reiniciar: siguiente.origen.nombre().to_string(),
        entorno_manda,
        respaldo_cli,
        pendiente_de_reinicio,
        avisos,
    }
}

/// Persiste la elección (`cli` o `api`) y devuelve el estado. El error es fijo: no repite la
/// entrada, que llega de la ventana
pub fn guardar_modo_llm_en(
    sesion: &MotorLlmSesion,
    modo: &str,
    valor_entorno: Option<&str>,
    key_configurada: bool,
) -> Result<EstadoModoLlmDto, String> {
    let elegido =
        modo_estricto(modo).ok_or_else(|| "Modo desconocido: elige CLI o API.".to_string())?;
    guardar_modo(&sesion.directorio_config, elegido).map_err(|_| {
        "No se pudo guardar el modo en el directorio de configuración de la app.".to_string()
    })?;
    tracing::info!(
        modo = nombre_de_modo(elegido),
        "modo del LLM guardado; se aplica al reiniciar"
    );
    Ok(estado_modo_llm_en(
        sesion,
        valor_entorno,
        leer_modo_guardado(&sesion.directorio_config),
        key_configurada,
    ))
}

/// `estado_modo_llm_en` leyendo el fichero de la sesión
pub fn estado_modo_llm_desde_disco(
    sesion: &MotorLlmSesion,
    valor_entorno: Option<&str>,
    key_configurada: bool,
) -> EstadoModoLlmDto {
    estado_modo_llm_en(
        sesion,
        valor_entorno,
        leer_modo_guardado(&sesion.directorio_config),
        key_configurada,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sesion(dir: &Path, pedido: ModoLlm, origen: OrigenModo, en_uso: ModoLlm) -> MotorLlmSesion {
        MotorLlmSesion {
            directorio_config: dir.to_path_buf(),
            pedido_al_arrancar: ModoResuelto {
                modo: pedido,
                origen,
            },
            en_uso,
        }
    }

    fn por_defecto(dir: &Path) -> MotorLlmSesion {
        sesion(dir, ModoLlm::Cli, OrigenModo::PorDefecto, ModoLlm::Cli)
    }

    #[test]
    fn test_sin_configuracion_todo_es_cli_y_sin_avisos() {
        let dir = tempfile::tempdir().unwrap();
        let estado = estado_modo_llm_desde_disco(&por_defecto(dir.path()), None, false);
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
        let estado = guardar_modo_llm_en(&por_defecto(dir.path()), "api", None, true).unwrap();
        assert_eq!(
            estado.en_uso, "cli",
            "el motor en uso no cambia en caliente"
        );
        assert_eq!(estado.elegido.as_deref(), Some("api"));
        assert_eq!(estado.al_reiniciar, "api");
        assert_eq!(estado.origen_al_reiniciar, "fichero");
        assert!(estado.pendiente_de_reinicio);
        assert!(estado.avisos.iter().any(|a| a.contains("reiniciar")));
    }

    #[test]
    fn test_elegir_api_sin_key_avisa_del_respaldo_a_cli() {
        let dir = tempfile::tempdir().unwrap();
        let estado = guardar_modo_llm_en(&por_defecto(dir.path()), "api", None, false).unwrap();
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
            guardar_modo_llm_en(&por_defecto(dir.path()), "api", Some("cli"), true).unwrap();
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
        let sin_key = estado_modo_llm_desde_disco(&sesion, None, false);
        assert!(sin_key.respaldo_cli);
        assert_eq!(sin_key.en_uso, "cli");
        assert!(
            !sin_key.pendiente_de_reinicio,
            "sin key, reiniciar no cambia nada"
        );
        let con_key = estado_modo_llm_desde_disco(&sesion, None, true);
        assert!(
            con_key.pendiente_de_reinicio,
            "con la key guardada, reiniciar activa la API"
        );
    }

    #[test]
    fn test_volver_a_cli_desde_api_en_uso() {
        let dir = tempfile::tempdir().unwrap();
        let sesion = sesion(dir.path(), ModoLlm::Api, OrigenModo::Fichero, ModoLlm::Api);
        let estado = guardar_modo_llm_en(&sesion, "CLI", None, true).unwrap();
        assert_eq!(estado.en_uso, "api");
        assert_eq!(estado.al_reiniciar, "cli");
        assert!(estado.pendiente_de_reinicio);
    }

    #[test]
    fn test_un_modo_desconocido_no_se_guarda_ni_se_repite() {
        let dir = tempfile::tempdir().unwrap();
        for malo in ["openai", "<script>", "api\n{\"x\":1}", ""] {
            let error =
                guardar_modo_llm_en(&por_defecto(dir.path()), malo, None, true).unwrap_err();
            assert!(
                !error.contains("openai") && !error.contains("script"),
                "{error}"
            );
        }
        assert_eq!(leer_modo_guardado(dir.path()), None);
    }
}
