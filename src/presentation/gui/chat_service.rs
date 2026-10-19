//! Servicio de chat de la GUI: valida la entrada del operador, mantiene el historial por
//! centro y traduce la ejecución del agente a los DTOs que consume el frontend web.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;

use serde::Serialize;
use tokio::sync::{Mutex, Notify};

use crate::application::dtos::agent::{
    AgentExecutionEvent, AgentMessage, AgentRole, ClaudeModel, ModelDescriptor,
};
use crate::application::ports::model_catalog::ModelCatalogPort;
use crate::application::use_cases::agent_orchestrator::{AgentOrchestrator, EventCallback};
use crate::application::use_cases::lakehouse_app::{AsistenteDirecto, AsistenteLakehousePort};
use crate::domain::models::center::CenterId;
use crate::presentation::gui::markdown_seguro::markdown_a_html_seguro;
use crate::presentation::gui::resultados_tools::{
    agrupar_fuentes, extraer_citas, extraer_graph_result, extraer_matrix_result, resumen_tool,
};
use crate::presentation::gui::{ChatResponsePayload, ToolCardDto};

type EventosRegistrados = Arc<StdMutex<Vec<(AgentExecutionEvent, Instant)>>>;

/// Evento que la ventana recibe en vivo durante un turno (evento Tauri `chat-stream`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "tipo",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatStreamEvento {
    /// Fragmento de texto de la respuesta (Markdown parcial, sin renderizar)
    Texto { turno_id: String, texto: String },
    /// El agente empieza a ejecutar una herramienta del Lakehouse
    HerramientaInicio { turno_id: String, nombre: String },
    /// La herramienta ha terminado
    HerramientaFin {
        turno_id: String,
        nombre: String,
        es_error: bool,
    },
}

/// Destino de los eventos en vivo (en la ventana, `AppHandle::emit`)
pub type EmisorChat = Arc<dyn Fn(ChatStreamEvento) + Send + Sync>;

/// Mensaje de error de un turno cortado con `detener`
pub const MENSAJE_RESPUESTA_DETENIDA: &str = "Respuesta detenida por el operador";

/// Fachada de la GUI sobre el asistente del Lakehouse (en producción, `LakehouseApp`)
pub struct GuiChatService {
    asistente: Arc<dyn AsistenteLakehousePort>,
    historiales: Mutex<HashMap<String, Vec<AgentMessage>>>,
    /// Turnos en curso que se pueden detener, por `turno_id`
    en_curso: StdMutex<HashMap<String, Arc<Notify>>>,
}

impl GuiChatService {
    /// Longitud máxima (en caracteres) de un mensaje del operador
    pub const MAX_MENSAJE_LEN: usize = 8_000;
    /// Número máximo de mensajes conservados por centro
    pub const MAX_HISTORIAL: usize = 40;

    /// Chat directo sobre un orquestador, sin gestión de centros (tests y usos embebidos)
    pub fn new(orquestador: AgentOrchestrator, catalogo: Arc<dyn ModelCatalogPort>) -> Self {
        Self::desde_asistente(Arc::new(AsistenteDirecto::new(orquestador, catalogo)))
    }

    /// Chat sobre la composición única (`LakehouseApp`): exige el centro activo y proyectado
    pub fn desde_asistente(asistente: Arc<dyn AsistenteLakehousePort>) -> Self {
        Self {
            asistente,
            historiales: Mutex::new(HashMap::new()),
            en_curso: StdMutex::new(HashMap::new()),
        }
    }

    /// Catálogo de modelos disponibles en la instalación de Claude Code
    pub async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, String> {
        self.asistente
            .listar_modelos()
            .await
            .map_err(|e| e.to_string())
    }

    /// Copia del historial conversacional de un centro
    pub async fn historial(&self, centro_id: &str) -> Vec<AgentMessage> {
        self.historiales
            .lock()
            .await
            .get(centro_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Ejecuta un turno de chat completo y devuelve la respuesta lista para renderizar
    pub async fn enviar_mensaje(
        &self,
        mensaje: &str,
        centro_id: &str,
        modelo: &str,
    ) -> Result<ChatResponsePayload, String> {
        self.enviar_mensaje_en_vivo(mensaje, centro_id, modelo, "", Arc::new(|_| {}))
            .await
    }

    /// Detiene el turno `turno_id` si sigue en curso. Devuelve si había algo que detener.
    pub fn detener(&self, turno_id: &str) -> bool {
        let aviso = self
            .en_curso
            .lock()
            .ok()
            .and_then(|mut turnos| turnos.remove(turno_id));
        match aviso {
            Some(aviso) => {
                aviso.notify_one();
                true
            }
            None => false,
        }
    }

    /// Como `enviar_mensaje`, emitiendo en vivo el texto y las herramientas del turno
    pub async fn enviar_mensaje_en_vivo(
        &self,
        mensaje: &str,
        centro_id: &str,
        modelo: &str,
        turno_id: &str,
        emisor: EmisorChat,
    ) -> Result<ChatResponsePayload, String> {
        let mensaje = Self::validar_mensaje(mensaje)?;
        let centro = CenterId::new(centro_id).map_err(|e| e.to_string())?;
        let modelo = ClaudeModel::new(modelo).map_err(|e| e.to_string())?;

        let mut historiales = self.historiales.lock().await;
        let historial = historiales.entry(centro.as_str().to_string()).or_default();
        let longitud_previa = historial.len();
        historial.push(AgentMessage::user(mensaje));

        let eventos: EventosRegistrados = Arc::default();
        let aviso_detener = Arc::new(Notify::new());
        if let Ok(mut turnos) = self.en_curso.lock() {
            turnos.insert(turno_id.to_string(), Arc::clone(&aviso_detener));
        }
        // Al detener se suelta el futuro del turno: el subproceso de Claude muere (kill_on_drop)
        let resultado = tokio::select! {
            resultado = self.asistente.conversar(
                &modelo,
                &centro,
                historial,
                Some(Self::registrador(&eventos, turno_id, emisor)),
            ) => resultado.map_err(|e| e.to_string()),
            _ = aviso_detener.notified() => Err(MENSAJE_RESPUESTA_DETENIDA.to_string()),
        };
        if let Ok(mut turnos) = self.en_curso.lock() {
            turnos.remove(turno_id);
        }

        match resultado {
            Ok(respuesta) => {
                Self::acotar_historial(historial);
                let eventos = eventos.lock().map(|e| e.clone()).unwrap_or_default();
                let tarjetas = Self::tarjetas_de_herramientas(&eventos);
                let citations = extraer_citas(&tarjetas);
                Ok(ChatResponsePayload {
                    reply_html: texto_a_html_seguro(&respuesta),
                    fuentes: agrupar_fuentes(&citations),
                    citations,
                    matrix_result: extraer_matrix_result(&tarjetas),
                    graph_result: extraer_graph_result(&tarjetas),
                    tools_used: tarjetas,
                })
            }
            Err(e) => {
                historial.truncate(longitud_previa);
                Err(e)
            }
        }
    }

    fn validar_mensaje(mensaje: &str) -> Result<&str, String> {
        let limpio = mensaje.trim();
        if limpio.is_empty() {
            return Err("El mensaje está vacío".to_string());
        }
        if limpio.chars().count() > Self::MAX_MENSAJE_LEN {
            return Err(format!(
                "El mensaje supera el máximo de {} caracteres",
                Self::MAX_MENSAJE_LEN
            ));
        }
        Ok(limpio)
    }

    /// Registra los eventos del turno (para el payload final) y reenvía los de la ventana
    fn registrador(
        eventos: &EventosRegistrados,
        turno_id: &str,
        emisor: EmisorChat,
    ) -> EventCallback {
        let destino = Arc::clone(eventos);
        let turno_id = turno_id.to_string();
        Arc::new(move |evento| {
            if let Some(en_vivo) = Self::evento_en_vivo(&evento, &turno_id) {
                emisor(en_vivo);
            }
            if let Ok(mut registro) = destino.lock() {
                registro.push((evento, Instant::now()));
            }
        })
    }

    fn evento_en_vivo(evento: &AgentExecutionEvent, turno_id: &str) -> Option<ChatStreamEvento> {
        let turno_id = turno_id.to_string();
        match evento {
            AgentExecutionEvent::TextDelta(texto) => Some(ChatStreamEvento::Texto {
                turno_id,
                texto: texto.clone(),
            }),
            AgentExecutionEvent::CallingTool { name, .. } => {
                Some(ChatStreamEvento::HerramientaInicio {
                    turno_id,
                    nombre: name.clone(),
                })
            }
            AgentExecutionEvent::ToolResultReceived { name, is_error, .. } => {
                Some(ChatStreamEvento::HerramientaFin {
                    turno_id,
                    nombre: name.clone(),
                    es_error: *is_error,
                })
            }
            _ => None,
        }
    }

    /// Descarta los mensajes más antiguos manteniendo que el historial empiece por el operador
    fn acotar_historial(historial: &mut Vec<AgentMessage>) {
        if historial.len() <= Self::MAX_HISTORIAL {
            return;
        }
        let mut inicio = historial.len() - Self::MAX_HISTORIAL;
        while inicio < historial.len() && historial[inicio].role != AgentRole::User {
            inicio += 1;
        }
        historial.drain(..inicio);
    }

    fn tarjetas_de_herramientas(eventos: &[(AgentExecutionEvent, Instant)]) -> Vec<ToolCardDto> {
        let mut pendientes: Vec<(String, serde_json::Value, Instant)> = Vec::new();
        let mut tarjetas = Vec::new();

        for (evento, instante) in eventos {
            match evento {
                AgentExecutionEvent::CallingTool { name, arguments } => {
                    pendientes.push((name.clone(), arguments.clone(), *instante));
                }
                AgentExecutionEvent::ToolResultReceived {
                    name,
                    result,
                    is_error,
                } => {
                    let (args, inicio) = match pendientes.iter().position(|(n, _, _)| n == name) {
                        Some(idx) => {
                            let (_, args, inicio) = pendientes.remove(idx);
                            (args, inicio)
                        }
                        None => (serde_json::Value::Null, *instante),
                    };
                    let status = if *is_error { "error" } else { "ok" };
                    tarjetas.push(ToolCardDto {
                        name: name.clone(),
                        args,
                        resumen: resumen_tool(name, result, status),
                        result: result.clone(),
                        status: status.to_string(),
                        duration: format!("{}ms", instante.duration_since(inicio).as_millis()),
                    });
                }
                _ => {}
            }
        }
        tarjetas
    }
}

/// HTML seguro de la respuesta del modelo: Markdown real con todo el HTML crudo escapado
/// (ver `markdown_seguro`)
pub fn texto_a_html_seguro(texto: &str) -> String {
    markdown_a_html_seguro(texto)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_del_modelo_se_escapa_y_el_markdown_se_pinta() {
        let html = texto_a_html_seguro("**Jefe** firma `FOR-ALM-05` <img src=x onerror=alert(1)>");
        assert_eq!(
            html,
            "<p><strong>Jefe</strong> firma <code>FOR-ALM-05</code> &lt;img src=x onerror=alert(1)&gt;</p>\n"
        );
    }
}
