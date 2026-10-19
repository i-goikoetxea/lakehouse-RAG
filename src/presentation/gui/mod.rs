//! Módulo de Presentación GUI (Aplicación de Escritorio Tauri 2)
//!
//! Ventana frameless 16:9 y comandos IPC finos sobre la composición única (`LakehouseApp`):
//! centros, ingesta por ruta con progreso, catálogo, integridad, modelos y chat. La lógica de
//! cada comando vive en `comandos` (probada sin ventana); aquí solo se extrae el estado.

pub mod chat_service;
pub mod comandos;
pub mod markdown_seguro;
pub mod resultados_tools;

use serde::{Deserialize, Serialize};

use std::sync::Arc;

use tauri::{DragDropEvent, Emitter, WindowEvent};
use tauri_plugin_dialog::DialogExt;

use crate::application::dtos::agent::ModelDescriptor;
use crate::application::dtos::centros::EXTENSIONES_INGERIBLES;
use crate::application::dtos::ingestion::ObservadorIngesta;
use crate::application::use_cases::lakehouse_app::LakehouseApp;
use chat_service::{ChatStreamEvento, GuiChatService, texto_a_html_seguro};
use comandos::{
    ActivacionDto, CentroDto, DocumentoCatalogoDto, EVENTO_ARRASTRE, EVENTO_DOCUMENTOS_SOLTADOS,
    EVENTO_PROGRESO_INGESTA, IngestaDto, IntegridadDto, ProgresoIngestaDto,
};
pub use resultados_tools::{CitationDto, FuenteCitadaDto};

/// Inicia la aplicación nativa Tauri 2.0 sobre la composición única (`LakehouseApp`)
pub fn iniciar_app_tauri(app: Arc<LakehouseApp>) -> Result<(), tauri::Error> {
    let servicio_chat = GuiChatService::desde_asistente(app.clone());
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(servicio_chat)
        .manage(app)
        .on_window_event(reemitir_arrastre_de_archivos)
        .invoke_handler(tauri::generate_handler![
            listar_modelos_claude,
            enviar_mensaje_chat,
            detener_respuesta,
            renderizar_markdown,
            listar_centros,
            crear_centro,
            activar_centro,
            desactivar_centro,
            eliminar_centro,
            seleccionar_documentos,
            seleccionar_directorio,
            ingestar_documento,
            catalogo_centro,
            integridad_centro
        ])
        .run(tauri::generate_context!())
}

/// En Tauri 2 el drop de archivos llega como evento de ventana con rutas (el `drop` HTML no
/// trae rutas en WebView2): se reemite al frontend, que pide la ingesta por ruta
fn reemitir_arrastre_de_archivos(ventana: &tauri::Window, evento: &WindowEvent) {
    let WindowEvent::DragDrop(arrastre) = evento else {
        return;
    };
    let resultado = match arrastre {
        DragDropEvent::Drop { paths, .. } => {
            let rutas: Vec<String> = paths
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            ventana.emit(EVENTO_DOCUMENTOS_SOLTADOS, rutas)
        }
        DragDropEvent::Enter { .. } => ventana.emit(EVENTO_ARRASTRE, "entrar"),
        DragDropEvent::Leave => ventana.emit(EVENTO_ARRASTRE, "salir"),
        _ => Ok(()),
    };
    if let Err(e) = resultado {
        tracing::warn!("No se pudo reemitir el arrastre de archivos: {e}");
    }
}

#[tauri::command]
async fn listar_centros(
    app: tauri::State<'_, Arc<LakehouseApp>>,
) -> Result<Vec<CentroDto>, String> {
    comandos::listar_centros(&app).await
}

#[tauri::command]
async fn crear_centro(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    centro_id: String,
    ruta: String,
    nombre: Option<String>,
) -> Result<CentroDto, String> {
    comandos::crear_centro(&app, &centro_id, &ruta, nombre.as_deref()).await
}

/// Activa un centro y reconstruye sus vistas
#[tauri::command]
async fn activar_centro(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    centro_id: String,
) -> Result<ActivacionDto, String> {
    comandos::activar_centro(&app, &centro_id).await
}

/// Desactiva un centro (cero acceso)
#[tauri::command]
async fn desactivar_centro(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    centro_id: String,
) -> Result<(), String> {
    comandos::desactivar_centro(&app, &centro_id).await
}

/// Quita el centro del registro sin borrar su directorio
#[tauri::command]
async fn eliminar_centro(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    centro_id: String,
) -> Result<CentroDto, String> {
    comandos::eliminar_centro(&app, &centro_id).await
}

/// Diálogo nativo de archivos: devuelve las rutas elegidas (el contenido no viaja por IPC)
#[tauri::command]
async fn seleccionar_documentos(app_handle: tauri::AppHandle) -> Result<Vec<String>, String> {
    let elegidos = app_handle
        .dialog()
        .file()
        .set_title("Documentos a ingerir")
        .add_filter("Markdown o texto", &EXTENSIONES_INGERIBLES)
        .blocking_pick_files()
        .unwrap_or_default();
    elegidos
        .into_iter()
        .map(|ruta| {
            ruta.into_path()
                .map(|p| p.to_string_lossy().to_string())
                .map_err(|e| e.to_string())
        })
        .collect()
}

/// Diálogo nativo de carpetas para la ruta de un centro nuevo
#[tauri::command]
async fn seleccionar_directorio(app_handle: tauri::AppHandle) -> Result<Option<String>, String> {
    app_handle
        .dialog()
        .file()
        .set_title("Carpeta del centro (vacía o inexistente)")
        .blocking_pick_folder()
        .map(|ruta| {
            ruta.into_path()
                .map(|p| p.to_string_lossy().to_string())
                .map_err(|e| e.to_string())
        })
        .transpose()
}

/// Ingesta por ruta; emite `ingesta-progreso` en cada etapa
#[tauri::command]
async fn ingestar_documento(
    app_handle: tauri::AppHandle,
    app: tauri::State<'_, Arc<LakehouseApp>>,
    ruta: String,
    centro_id: String,
) -> Result<IngestaDto, String> {
    let (centro, archivo) = (centro_id.clone(), ruta.clone());
    let observador: ObservadorIngesta = Arc::new(move |etapa| {
        let progreso = ProgresoIngestaDto::new(&centro, &archivo, etapa);
        if let Err(e) = app_handle.emit(EVENTO_PROGRESO_INGESTA, progreso) {
            tracing::warn!("No se pudo emitir el progreso de la ingesta: {e}");
        }
    });
    comandos::ingestar_documento(&app, &ruta, &centro_id, Some(observador)).await
}

/// Revisiones del catálogo del centro activo (vigentes y derogadas)
#[tauri::command]
async fn catalogo_centro(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    centro_id: String,
) -> Result<Vec<DocumentoCatalogoDto>, String> {
    comandos::catalogo_centro(&app, &centro_id).await
}

#[tauri::command]
async fn integridad_centro(
    app: tauri::State<'_, Arc<LakehouseApp>>,
    centro_id: String,
) -> Result<IntegridadDto, String> {
    comandos::integridad(&app, &centro_id).await
}

/// Comando IPC: catálogo dinámico de modelos de la instalación local de Claude Code
#[tauri::command]
async fn listar_modelos_claude(
    servicio: tauri::State<'_, GuiChatService>,
) -> Result<Vec<ModelDescriptor>, String> {
    servicio.listar_modelos().await
}

/// Evento con el texto y las herramientas de un turno de chat en curso
pub const EVENTO_CHAT_STREAM: &str = "chat-stream";

/// Comando IPC: turno de chat con el agente del Lakehouse vía Claude Code headless. Mientras
/// dura, emite `chat-stream` (texto en vivo y herramientas); al final devuelve el payload.
#[tauri::command]
async fn enviar_mensaje_chat(
    app_handle: tauri::AppHandle,
    servicio: tauri::State<'_, GuiChatService>,
    mensaje: String,
    centro_id: String,
    modelo: String,
    turno_id: String,
) -> Result<ChatResponsePayload, String> {
    let emisor = Arc::new(move |evento: ChatStreamEvento| {
        if let Err(e) = app_handle.emit(EVENTO_CHAT_STREAM, evento) {
            tracing::warn!("No se pudo emitir el evento del chat: {e}");
        }
    });
    servicio
        .enviar_mensaje_en_vivo(&mensaje, &centro_id, &modelo, &turno_id, emisor)
        .await
}

/// Comando IPC: detiene el turno de chat en curso (botón Detener)
#[tauri::command]
fn detener_respuesta(servicio: tauri::State<'_, GuiChatService>, turno_id: String) -> bool {
    servicio.detener(&turno_id)
}

/// Comando IPC: Markdown parcial → HTML seguro (render progresivo durante el streaming)
#[tauri::command]
fn renderizar_markdown(texto: String) -> String {
    texto_a_html_seguro(&texto)
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

/// Solicitud de consulta de chat con selección de modelo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatQueryPayload {
    pub mensaje: String,
    pub centro_id: String,
    pub modelo: String,
}

/// Registro de ejecución de herramienta para observabilidad en tiempo real (Tool Card)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCardDto {
    pub name: String,
    pub args: serde_json::Value,
    pub result: serde_json::Value,
    pub status: String,
    pub duration: String,
    /// Línea legible del resultado (p. ej. «resuelto · 125 (*) °C»)
    pub resumen: String,
}

/// Respuesta completa para el asistente del frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponsePayload {
    pub reply_html: String,
    pub tools_used: Vec<ToolCardDto>,
    pub citations: Vec<CitationDto>,
    /// Las mismas citas agrupadas por documento + revisión (un chip por fuente)
    pub fuentes: Vec<FuenteCitadaDto>,
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
