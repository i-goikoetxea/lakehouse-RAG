//! Chat en vivo de la ventana (Fase 15, rediseño del chat): `GuiChatService` reenvía cada
//! fragmento de texto y cada herramienta como `ChatStreamEvento` mientras el agente trabaja,
//! y `detener` corta un turno en curso sin dejar rastro en el historial.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use lakehouse_rag::application::dtos::agent::{
    AgentMessage, AgentTurnOutput, ClaudeModel, ModelDescriptor,
};
use lakehouse_rag::application::dtos::tools::{ToolCall, ToolDefinition, ToolResult};
use lakehouse_rag::application::ports::agent_runner::{AgentRunnerPort, ObservadorTexto};
use lakehouse_rag::application::ports::model_catalog::ModelCatalogPort;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::presentation::gui::chat_service::{
    ChatStreamEvento, EmisorChat, GuiChatService,
};
use serde_json::json;

struct Herramientas;

#[async_trait]
impl ToolServerPort for Herramientas {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        ToolDefinition::lakehouse_tools()
    }
    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            output: json!({ "filas": [] }),
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

/// Primer turno: pide una tool. Segundo: responde en fragmentos (con una pausa opcional)
struct RunnerGuionizado {
    turnos: Mutex<usize>,
    pausa_tras_primer_fragmento: Duration,
}

#[async_trait]
impl AgentRunnerPort for RunnerGuionizado {
    async fn ejecutar_turno(
        &self,
        _modelo: &ClaudeModel,
        _system_prompt: &str,
        _historial: &[AgentMessage],
        _herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        unreachable!("la ventana siempre usa el turno en vivo")
    }

    async fn ejecutar_turno_en_vivo(
        &self,
        _modelo: &ClaudeModel,
        _system_prompt: &str,
        _historial: &[AgentMessage],
        _herramientas: &[ToolDefinition],
        observador: ObservadorTexto,
    ) -> Result<AgentTurnOutput, DomainError> {
        let turno = {
            let mut t = self.turnos.lock().unwrap();
            *t += 1;
            *t
        };
        if turno == 1 {
            return Ok(AgentTurnOutput::ToolCalls(vec![ToolCall {
                id: "call_1".into(),
                name: "query_operational_matrix".into(),
                arguments: json!({ "centro_id": "obra", "fase": "CUSTODIA" }),
            }]));
        }
        observador("**Jefe de ");
        tokio::time::sleep(self.pausa_tras_primer_fragmento).await;
        observador("Producción**");
        Ok(AgentTurnOutput::FinalMessage(
            "**Jefe de Producción**".to_string(),
        ))
    }
}

fn servicio(pausa: Duration) -> Arc<GuiChatService> {
    let runner = Arc::new(RunnerGuionizado {
        turnos: Mutex::new(0),
        pausa_tras_primer_fragmento: pausa,
    });
    let orquestador = AgentOrchestrator::new(runner, Arc::new(Herramientas));
    Arc::new(GuiChatService::new(orquestador, Arc::new(SinCatalogo)))
}

type EventosRecogidos = Arc<Mutex<Vec<ChatStreamEvento>>>;

fn recolector() -> (EventosRecogidos, EmisorChat) {
    let eventos: EventosRecogidos = Arc::default();
    let destino = eventos.clone();
    (eventos, Arc::new(move |e| destino.lock().unwrap().push(e)))
}

#[tokio::test]
async fn test_el_turno_emite_herramientas_y_texto_en_orden_y_devuelve_el_payload_final() {
    let chat = servicio(Duration::ZERO);
    let (eventos, emisor) = recolector();
    let payload = chat
        .enviar_mensaje_en_vivo("¿Custodia?", "obra", "sonnet", "turno-1", emisor)
        .await
        .unwrap();
    assert!(
        payload
            .reply_html
            .contains("<strong>Jefe de Producción</strong>")
    );
    assert_eq!(
        *eventos.lock().unwrap(),
        vec![
            ChatStreamEvento::HerramientaInicio {
                turno_id: "turno-1".into(),
                nombre: "query_operational_matrix".into(),
            },
            ChatStreamEvento::HerramientaFin {
                turno_id: "turno-1".into(),
                nombre: "query_operational_matrix".into(),
                es_error: false,
            },
            ChatStreamEvento::Texto {
                turno_id: "turno-1".into(),
                texto: "**Jefe de ".into(),
            },
            ChatStreamEvento::Texto {
                turno_id: "turno-1".into(),
                texto: "Producción**".into(),
            },
        ]
    );
    assert_eq!(chat.historial("obra").await.len(), 4);
}

#[test]
fn test_el_evento_se_serializa_en_camel_case_con_su_tipo() {
    let evento = ChatStreamEvento::HerramientaFin {
        turno_id: "t".into(),
        nombre: "lookup_double_entry_table".into(),
        es_error: true,
    };
    assert_eq!(
        serde_json::to_value(&evento).unwrap(),
        json!({ "tipo": "herramientaFin", "turnoId": "t", "nombre": "lookup_double_entry_table", "esError": true })
    );
}

#[tokio::test]
async fn test_detener_corta_el_turno_en_curso_y_no_deja_rastro_en_el_historial() {
    let chat = servicio(Duration::from_secs(20));
    let (eventos, emisor) = recolector();
    let inicio = Instant::now();
    let en_curso = {
        let chat = chat.clone();
        tokio::spawn(async move {
            chat.enviar_mensaje_en_vivo("¿Custodia?", "obra", "sonnet", "turno-2", emisor)
                .await
        })
    };
    while !eventos
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, ChatStreamEvento::Texto { .. }))
    {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(chat.detener("turno-2"));
    let error = en_curso.await.unwrap().expect_err("detenido");
    assert!(error.contains("detenida"), "{error}");
    assert!(inicio.elapsed() < Duration::from_secs(5));
    assert!(chat.historial("obra").await.is_empty());
    assert!(!chat.detener("turno-2"), "el turno ya no existe");
    assert!(!chat.detener("desconocido"));
}
