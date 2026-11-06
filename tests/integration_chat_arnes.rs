//! El arnés visto desde la ventana (Fase 3): `GuiChatService` avisa de la retención, traduce las
//! intervenciones a lenguaje del operador y no conserva ni el recordatorio ni lo rechazado.

use std::collections::VecDeque;
use std::future::pending;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use lakehouse_rag::application::dtos::agent::{
    AgentMessage, AgentRole, AgentTurnOutput, ClaudeModel, EtapaArnes, ModelDescriptor,
    TurnoDirigido,
};
use lakehouse_rag::application::dtos::tools::{
    LakehouseToolName, ToolCall, ToolDefinition, ToolResult,
};
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::application::ports::model_catalog::ModelCatalogPort;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::agent_orchestrator::{
    AgentOrchestrator, MENSAJE_RESPUESTA_NO_VALIDADA,
};
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::presentation::gui::chat_service::{
    ChatStreamEvento, EmisorChat, GuiChatService,
};
use serde_json::json;

const PREGUNTA_CRUZABLE: &str = "¿Qué temperatura da la Tabla 4-A para 30 mm y CE 0,42 %?";
const RESPUESTA_RECHAZADA: &str = "Son 999 grados";

struct LookupResuelto;

#[async_trait]
impl ToolServerPort for LookupResuelto {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        ToolDefinition::lakehouse_tools()
    }
    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            output: json!({ "estado": "resuelto", "valor_resultado_texto": "125" }),
            is_error: false,
        })
    }
}

struct SinCatalogo;

#[async_trait]
impl ModelCatalogPort for SinCatalogo {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        Ok(Vec::new())
    }
}

/// `None` en la lista de respuestas deja el paso colgado, como un proceso que no termina
#[derive(Default)]
struct RunnerRetenido {
    dirigido_colgado: bool,
    respuestas: Mutex<VecDeque<Option<&'static str>>>,
    vistas: Mutex<Vec<Vec<AgentMessage>>>,
}

impl RunnerRetenido {
    fn con_respuestas(respuestas: &[Option<&'static str>]) -> Self {
        Self {
            respuestas: Mutex::new(respuestas.iter().copied().collect()),
            ..Self::default()
        }
    }
}

#[async_trait]
impl AgentRunnerPort for RunnerRetenido {
    async fn ejecutar_turno(
        &self,
        _modelo: &ClaudeModel,
        _system_prompt: &str,
        historial: &[AgentMessage],
        _herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        self.vistas.lock().unwrap().push(historial.to_vec());
        let siguiente = self.respuestas.lock().unwrap().pop_front().flatten();
        match siguiente {
            Some(texto) => Ok(AgentTurnOutput::FinalMessage(texto.to_string())),
            None => pending().await,
        }
    }

    async fn ejecutar_turno_dirigido(
        &self,
        _modelo: &ClaudeModel,
        _system_prompt: &str,
        _historial: &[AgentMessage],
        _herramientas: &[ToolDefinition],
        herramienta: LakehouseToolName,
    ) -> Result<TurnoDirigido, DomainError> {
        if self.dirigido_colgado {
            return pending().await;
        }
        Ok(TurnoDirigido::Llamada(ToolCall {
            id: "dirigida".into(),
            name: herramienta.as_str().into(),
            arguments: json!({}),
        }))
    }
}

fn servicio(runner: Arc<RunnerRetenido>) -> Arc<GuiChatService> {
    let orquestador = AgentOrchestrator::new(runner, Arc::new(LookupResuelto));
    Arc::new(GuiChatService::new(orquestador, Arc::new(SinCatalogo)))
}

type EventosRecogidos = Arc<Mutex<Vec<ChatStreamEvento>>>;

fn recolector() -> (EventosRecogidos, EmisorChat) {
    let eventos: EventosRecogidos = Arc::default();
    let destino = eventos.clone();
    (eventos, Arc::new(move |e| destino.lock().unwrap().push(e)))
}

fn historial_sin_rastro_del_arnes(historial: &[AgentMessage]) {
    for mensaje in historial {
        assert!(
            !mensaje.content.contains("RECORDATORIO"),
            "{}",
            mensaje.content
        );
        assert!(
            !mensaje.content.contains(RESPUESTA_RECHAZADA),
            "{}",
            mensaje.content
        );
    }
}

#[tokio::test]
async fn test_turno_retenido_avisa_antes_de_todo_y_el_texto_llega_al_final_y_de_una_vez() {
    let chat = servicio(Arc::new(RunnerRetenido::con_respuestas(&[Some("Son 125")])));
    let (eventos, emisor) = recolector();
    chat.enviar_mensaje_en_vivo(PREGUNTA_CRUZABLE, "obra", "sonnet", "t1", emisor)
        .await
        .unwrap();
    let eventos = eventos.lock().unwrap();
    assert_eq!(
        eventos[0],
        ChatStreamEvento::TurnoRetenido {
            turno_id: "t1".into()
        }
    );
    let textos: Vec<usize> = eventos
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, ChatStreamEvento::Texto { .. }))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(textos, vec![eventos.len() - 1], "{eventos:?}");
}

#[tokio::test]
async fn test_la_intervencion_llega_en_vivo_en_lenguaje_del_operador() {
    let chat = servicio(Arc::new(RunnerRetenido::con_respuestas(&[Some("Son 125")])));
    let (eventos, emisor) = recolector();
    chat.enviar_mensaje_en_vivo(PREGUNTA_CRUZABLE, "obra", "sonnet", "t1", emisor)
        .await
        .unwrap();
    let eventos = eventos.lock().unwrap();
    let mensaje = eventos
        .iter()
        .find_map(|e| match e {
            ChatStreamEvento::Arnes {
                etapa: EtapaArnes::Directiva,
                mensaje,
                ..
            } => Some(mensaje.clone()),
            _ => None,
        })
        .expect("directiva en vivo");
    assert!(!mensaje.is_empty());
    assert!(!mensaje.contains("lookup_double_entry_table"), "{mensaje}");
}

#[tokio::test]
async fn test_reintento_valido_se_refleja_en_el_payload_sin_salida_segura() {
    let chat = servicio(Arc::new(RunnerRetenido::con_respuestas(&[
        Some(RESPUESTA_RECHAZADA),
        Some("Son 125"),
    ])));
    let payload = chat
        .enviar_mensaje(PREGUNTA_CRUZABLE, "obra", "sonnet")
        .await
        .unwrap();
    let etapas: Vec<EtapaArnes> = payload.arnes.iter().map(|i| i.etapa).collect();
    assert_eq!(etapas, vec![EtapaArnes::Directiva, EtapaArnes::Reintento]);
    assert!(!payload.salida_segura);
    assert!(payload.reply_html.contains("Son 125"));
    assert!(!payload.reply_html.contains("999"));
}

#[tokio::test]
async fn test_salida_segura_entrega_el_mensaje_fijo_marcado_y_conserva_las_tools() {
    let chat = servicio(Arc::new(RunnerRetenido::con_respuestas(&[
        Some(RESPUESTA_RECHAZADA),
        Some(RESPUESTA_RECHAZADA),
    ])));
    let (eventos, emisor) = recolector();
    let payload = chat
        .enviar_mensaje_en_vivo(PREGUNTA_CRUZABLE, "obra", "sonnet", "t1", emisor)
        .await
        .unwrap();
    assert!(payload.salida_segura);
    assert!(payload.reply_html.contains(MENSAJE_RESPUESTA_NO_VALIDADA));
    assert!(!payload.reply_html.contains("999"));
    assert_eq!(payload.tools_used.len(), 1, "los resultados siguen a mano");
    assert!(payload.matrix_result.is_some());
    assert!(
        !eventos.lock().unwrap().iter().any(|e| matches!(
            e,
            ChatStreamEvento::Texto { texto, .. } if texto.contains("999")
        )),
        "lo rechazado nunca sale en vivo"
    );
}

#[tokio::test]
async fn test_el_historial_no_conserva_recordatorio_ni_respuesta_rechazada() {
    let runner = Arc::new(RunnerRetenido::con_respuestas(&[
        Some(RESPUESTA_RECHAZADA),
        Some("Son 125"),
        Some("Son 125"),
    ]));
    let chat = servicio(runner.clone());
    chat.enviar_mensaje(PREGUNTA_CRUZABLE, "obra", "sonnet")
        .await
        .unwrap();
    historial_sin_rastro_del_arnes(&chat.historial("obra").await);

    chat.enviar_mensaje("Resume el resultado", "obra", "sonnet")
        .await
        .ok();
    historial_sin_rastro_del_arnes(&chat.historial("obra").await);
    assert!(
        runner.vistas.lock().unwrap()[1]
            .iter()
            .any(|m| m.role == AgentRole::User && m.content.contains("RECORDATORIO")),
        "el recordatorio sí llega al modelo durante el reintento"
    );
}

#[tokio::test]
async fn test_el_historial_de_la_salida_segura_solo_guarda_el_mensaje_fijo() {
    let chat = servicio(Arc::new(RunnerRetenido::con_respuestas(&[
        Some(RESPUESTA_RECHAZADA),
        Some(RESPUESTA_RECHAZADA),
    ])));
    chat.enviar_mensaje(PREGUNTA_CRUZABLE, "obra", "sonnet")
        .await
        .unwrap();
    let historial = chat.historial("obra").await;
    historial_sin_rastro_del_arnes(&historial);
    assert_eq!(
        historial.last().unwrap().content,
        MENSAJE_RESPUESTA_NO_VALIDADA
    );
}

async fn detener_cuando<F>(chat: &Arc<GuiChatService>, eventos: &EventosRecogidos, listo: F)
where
    F: Fn(&[ChatStreamEvento]) -> bool,
{
    let limite = Instant::now() + Duration::from_secs(10);
    while !listo(&eventos.lock().unwrap()) {
        assert!(Instant::now() < limite, "el evento esperado no llegó");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(chat.detener("t1"));
}

#[tokio::test]
async fn test_detener_corta_el_paso_dirigido_y_no_deja_rastro() {
    let chat = servicio(Arc::new(RunnerRetenido {
        dirigido_colgado: true,
        ..RunnerRetenido::default()
    }));
    let (eventos, emisor) = recolector();
    let inicio = Instant::now();
    let en_curso = {
        let chat = chat.clone();
        tokio::spawn(async move {
            chat.enviar_mensaje_en_vivo(PREGUNTA_CRUZABLE, "obra", "sonnet", "t1", emisor)
                .await
        })
    };
    detener_cuando(&chat, &eventos, |e| {
        e.iter()
            .any(|e| matches!(e, ChatStreamEvento::Arnes { .. }))
    })
    .await;
    let error = en_curso.await.unwrap().expect_err("detenido");
    assert!(error.contains("detenida"), "{error}");
    assert!(inicio.elapsed() < Duration::from_secs(5));
    assert!(chat.historial("obra").await.is_empty());
}

#[tokio::test]
async fn test_detener_corta_el_reintento_y_no_deja_rastro() {
    let chat = servicio(Arc::new(RunnerRetenido::con_respuestas(&[
        Some(RESPUESTA_RECHAZADA),
        None,
    ])));
    let (eventos, emisor) = recolector();
    let en_curso = {
        let chat = chat.clone();
        tokio::spawn(async move {
            chat.enviar_mensaje_en_vivo(PREGUNTA_CRUZABLE, "obra", "sonnet", "t1", emisor)
                .await
        })
    };
    detener_cuando(&chat, &eventos, |e| {
        e.iter().any(|e| {
            matches!(
                e,
                ChatStreamEvento::Arnes {
                    etapa: EtapaArnes::Reintento,
                    ..
                }
            )
        })
    })
    .await;
    assert!(en_curso.await.unwrap().is_err());
    assert!(chat.historial("obra").await.is_empty());
    assert!(
        !eventos
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, ChatStreamEvento::Texto { .. }))
    );
}

#[test]
fn test_los_eventos_nuevos_se_serializan_en_camel_case() {
    assert_eq!(
        serde_json::to_value(ChatStreamEvento::TurnoRetenido {
            turno_id: "t".into()
        })
        .unwrap(),
        json!({ "tipo": "turnoRetenido", "turnoId": "t" })
    );
    assert_eq!(
        serde_json::to_value(ChatStreamEvento::Arnes {
            turno_id: "t".into(),
            etapa: EtapaArnes::SalidaSegura,
            mensaje: "m".into(),
        })
        .unwrap(),
        json!({ "tipo": "arnes", "turnoId": "t", "etapa": "salida_segura", "mensaje": "m" })
    );
}
