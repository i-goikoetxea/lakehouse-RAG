//! Servicio de chat de la GUI: valida la entrada del operador, mantiene el historial por
//! centro y traduce la ejecución del agente a los DTOs que consume el frontend web.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;

use tokio::sync::Mutex;

use crate::application::dtos::agent::{
    AgentExecutionEvent, AgentMessage, AgentRole, ClaudeModel, ModelDescriptor,
};
use crate::application::ports::model_catalog::ModelCatalogPort;
use crate::application::use_cases::agent_orchestrator::{AgentOrchestrator, EventCallback};
use crate::domain::models::center::CenterId;
use crate::presentation::gui::{ChatResponsePayload, ToolCardDto};

type EventosRegistrados = Arc<StdMutex<Vec<(AgentExecutionEvent, Instant)>>>;

/// Fachada de la GUI sobre el orquestador del agente y el catálogo de modelos
pub struct GuiChatService {
    orquestador: AgentOrchestrator,
    catalogo: Arc<dyn ModelCatalogPort>,
    historiales: Mutex<HashMap<String, Vec<AgentMessage>>>,
}

impl GuiChatService {
    /// Longitud máxima (en caracteres) de un mensaje del operador
    pub const MAX_MENSAJE_LEN: usize = 8_000;
    /// Número máximo de mensajes conservados por centro
    pub const MAX_HISTORIAL: usize = 40;

    pub fn new(orquestador: AgentOrchestrator, catalogo: Arc<dyn ModelCatalogPort>) -> Self {
        Self {
            orquestador,
            catalogo,
            historiales: Mutex::new(HashMap::new()),
        }
    }

    /// Catálogo de modelos disponibles en la instalación de Claude Code
    pub async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, String> {
        self.catalogo
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
        let mensaje = Self::validar_mensaje(mensaje)?;
        let centro = CenterId::new(centro_id).map_err(|e| e.to_string())?;
        let modelo = ClaudeModel::new(modelo).map_err(|e| e.to_string())?;

        let mut historiales = self.historiales.lock().await;
        let historial = historiales.entry(centro.as_str().to_string()).or_default();
        let longitud_previa = historial.len();
        historial.push(AgentMessage::user(mensaje));

        let eventos: EventosRegistrados = Arc::default();
        let resultado = self
            .orquestador
            .procesar_turno(
                &modelo,
                &centro,
                historial,
                Some(Self::registrador(&eventos)),
            )
            .await;

        match resultado {
            Ok(respuesta) => {
                Self::acotar_historial(historial);
                let eventos = eventos.lock().map(|e| e.clone()).unwrap_or_default();
                Ok(ChatResponsePayload {
                    reply_html: texto_a_html_seguro(&respuesta),
                    tools_used: Self::tarjetas_de_herramientas(&eventos),
                    citations: Vec::new(),
                    matrix_result: None,
                    graph_result: None,
                })
            }
            Err(e) => {
                historial.truncate(longitud_previa);
                Err(e.to_string())
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

    fn registrador(eventos: &EventosRegistrados) -> EventCallback {
        let destino = Arc::clone(eventos);
        Arc::new(move |evento| {
            if let Ok(mut registro) = destino.lock() {
                registro.push((evento, Instant::now()));
            }
        })
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
                    tarjetas.push(ToolCardDto {
                        name: name.clone(),
                        args,
                        result: result.clone(),
                        status: if *is_error { "error" } else { "ok" }.to_string(),
                        duration: format!("{}ms", instante.duration_since(inicio).as_millis()),
                    });
                }
                _ => {}
            }
        }
        tarjetas
    }
}

/// Escapa HTML y convierte saltos de línea en `<br>` para inyectar en `innerHTML`
pub fn texto_a_html_seguro(texto: &str) -> String {
    let mut html = String::with_capacity(texto.len());
    for c in texto.chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '"' => html.push_str("&quot;"),
            '\'' => html.push_str("&#39;"),
            '\n' => html.push_str("<br>"),
            '\r' => {}
            otro => html.push(otro),
        }
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texto_a_html_seguro_escapa_metacaracteres() {
        assert_eq!(
            texto_a_html_seguro("<b onclick='x'>\"a\" & b</b>\r\nfin"),
            "&lt;b onclick=&#39;x&#39;&gt;&quot;a&quot; &amp; b&lt;/b&gt;<br>fin"
        );
    }
}
