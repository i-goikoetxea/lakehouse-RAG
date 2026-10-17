//! Módulo de Presentación GUI (Aplicación de Escritorio Moderna)
//!
//! Define la configuración de la ventana frameless (relación 16:9, sin decoraciones nativas)
//! y los contratos DTO para los comandos IPC entre el cliente web (Linear + NotebookLM) y
//! los servicios del núcleo (LakehouseToolService y DocumentIngestionService).

pub mod chat_service;

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::application::dtos::agent::ModelDescriptor;
use crate::application::use_cases::lakehouse_app::LakehouseApp;
use crate::domain::models::center::CentroConocimiento;
use chat_service::GuiChatService;

/// Inicia la aplicación nativa Tauri 2.0 sobre la composición única (`LakehouseApp`)
pub fn iniciar_app_tauri(app: Arc<LakehouseApp>) -> Result<(), tauri::Error> {
    let servicio_chat = GuiChatService::desde_asistente(app.clone());
    tauri::Builder::default()
        .manage(servicio_chat)
        .manage(app)
        .invoke_handler(tauri::generate_handler![
            listar_modelos_claude,
            enviar_mensaje_chat,
            listar_centros,
            activar_centro,
            desactivar_centro,
            ingestar_documento
        ])
        .run(tauri::generate_context!())
}

/// Centro tal como lo consume el frontend
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CentroDto {
    pub id: String,
    pub nombre: String,
    pub ruta_disco: String,
    pub activo: bool,
}

impl From<&CentroConocimiento> for CentroDto {
    fn from(centro: &CentroConocimiento) -> Self {
        Self {
            id: centro.id().to_string(),
            nombre: centro.manifiesto().nombre.clone(),
            ruta_disco: centro.manifiesto().ruta_almacenamiento.clone(),
            activo: centro.esta_activo(),
        }
    }
}

/// Comando IPC: centros registrados
#[tauri::command]
async fn listar_centros(
    app: tauri::State<'_, Arc<LakehouseApp>>,
) -> Result<Vec<CentroDto>, String> {
    let centros = app.listar_centros().await.map_err(|e| e.to_string())?;
    Ok(centros.iter().map(CentroDto::from).collect())
}

/// Comando IPC: activa un centro y reconstruye sus vistas
#[tauri::command]
async fn activar_centro(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    centro_id: String,
) -> Result<usize, String> {
    let resumen = app
        .activar_centro(&centro_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(resumen.vistas.len())
}

/// Comando IPC: desactiva un centro (cero acceso)
#[tauri::command]
async fn desactivar_centro(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    centro_id: String,
) -> Result<(), String> {
    app.desactivar_centro(&centro_id)
        .await
        .map_err(|e| e.to_string())
}

/// Comando IPC: ingesta transaccional de un archivo local en un centro
#[tauri::command]
async fn ingestar_documento(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    ruta: String,
    centro_id: String,
) -> Result<String, String> {
    let resultado = app
        .ingestar(std::path::Path::new(&ruta), &centro_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "{} v{} ({})",
        resultado.informe.doc_id,
        resultado.informe.version,
        if resultado.informe.ya_ingerido {
            "ya ingerido"
        } else {
            "ingerido"
        }
    ))
}

/// Comando IPC: catálogo dinámico de modelos de la instalación local de Claude Code
#[tauri::command]
async fn listar_modelos_claude(
    servicio: tauri::State<'_, GuiChatService>,
) -> Result<Vec<ModelDescriptor>, String> {
    servicio.listar_modelos().await
}

/// Comando IPC: turno de chat con el agente del Lakehouse vía Claude Code headless
#[tauri::command]
async fn enviar_mensaje_chat(
    servicio: tauri::State<'_, GuiChatService>,
    mensaje: String,
    centro_id: String,
    modelo: String,
) -> Result<ChatResponsePayload, String> {
    servicio.enviar_mensaje(&mensaje, &centro_id, &modelo).await
}

/// Configuración de especificación de la ventana de escritorio
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowConfig {
    /// Título de la ventana
    pub title: String,
    /// Ancho inicial en píxeles (16:9)
    pub width: u32,
    /// Alto inicial en píxeles (16:9)
    pub height: u32,
    /// Sin decoraciones del sistema operativo (frameless)
    pub decorations: bool,
    /// Si la ventana puede redimensionarse
    pub resizable: bool,
    /// Centrar ventana al inicio
    pub center: bool,
    /// Fondo transparente para bordes redondeados
    pub transparent: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Lakehouse RAG Industrial".to_string(),
            width: 1280,
            height: 720,
            decorations: false,
            resizable: true,
            center: true,
            transparent: false,
        }
    }
}

/// Solicitud de ingesta enviada desde la zona Drag & Drop del frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestarDocPayload {
    pub nombre: String,
    pub centro_id: String,
}

/// Solicitud de consulta de chat con selección de modelo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatQueryPayload {
    pub mensaje: String,
    pub centro_id: String,
    pub modelo: String,
}

/// Cita bibliográfica verificada en los esquemas Arrow
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationDto {
    pub doc_id: String,
    pub title: String,
    pub page: u32,
    pub hash: String,
}

/// Registro de ejecución de herramienta para observabilidad en tiempo real (Tool Card)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCardDto {
    pub name: String,
    pub args: serde_json::Value,
    pub result: serde_json::Value,
    pub status: String,
    pub duration: String,
}

/// Respuesta completa para el asistente del frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponsePayload {
    pub reply_html: String,
    pub tools_used: Vec<ToolCardDto>,
    pub citations: Vec<CitationDto>,
    pub matrix_result: Option<serde_json::Value>,
    pub graph_result: Option<serde_json::Value>,
}

/// Controlador de la aplicación gráfica
pub struct GuiController;

impl GuiController {
    /// Retorna la configuración validada de la ventana de escritorio
    pub fn config_ventana() -> WindowConfig {
        WindowConfig::default()
    }

    /// Valida que la ventana cumpla la relación de aspecto 16:9 y modo frameless
    pub fn es_configuracion_valida(config: &WindowConfig) -> bool {
        let es_16_9 = (config.width * 9) == (config.height * 16);
        !config.decorations && es_16_9 && config.width >= 1280
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configuracion_por_defecto_es_frameless_16_9() {
        let config = GuiController::config_ventana();
        assert_eq!(config.width, 1280);
        assert_eq!(config.height, 720);
        assert!(
            !config.decorations,
            "Debe ser frameless sin decoraciones nativas"
        );
        assert!(config.center);
        assert!(GuiController::es_configuracion_valida(&config));
    }

    #[test]
    fn test_rechazo_configuraciones_con_decoraciones_o_aspect_ratio_incorrecto() {
        let mut config = GuiController::config_ventana();
        config.decorations = true;
        assert!(!GuiController::es_configuracion_valida(&config));

        let mut config_ratio = GuiController::config_ventana();
        config_ratio.height = 800; // 1280x800 es 16:10, no 16:9
        assert!(!GuiController::es_configuracion_valida(&config_ratio));
    }

    #[test]
    fn test_serializacion_payloads_gui() {
        let payload = ChatQueryPayload {
            mensaje: "¿Cuál es la temperatura máxima en Recepción?".to_string(),
            centro_id: "planta_alfa".to_string(),
            modelo: "sonnet".to_string(),
        };

        let json = serde_json::to_string(&payload).expect("Serialización fallida");
        assert!(json.contains("temperatura máxima"));
        assert!(json.contains("sonnet"));
    }
}
