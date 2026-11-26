//! El historial del chat se acota por turnos completos (F-2 de la auditoría de la v1.1.0):
//! un turno largo no vacía el historial, el turno siguiente recibe el anterior entero, los
//! turnos antiguos se compactan a pregunta y respuesta antes de descartarse y un turno fallido
//! o detenido no deja rastro. Sin red, sin keys y sin Claude.

use std::collections::{HashSet, VecDeque};
use std::future::pending;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Notify;

use lakehouse_rag::application::dtos::agent::{
    AgentMessage, AgentRole, AgentTurnOutput, ClaudeModel, ModelDescriptor, TurnoDirigido,
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
use lakehouse_rag::infrastructure::llm::MockAgentRunner;
use lakehouse_rag::presentation::gui::chat_service::{GuiChatService, MENSAJE_RESPUESTA_DETENIDA};

/// Pregunta con intención de cruce: el arnés dirige la primera llamada y valida las cifras
const PREGUNTA_CRUZABLE: &str = "¿Qué temperatura da la Tabla 4-A para 30 mm y CE 0,42 %?";
/// Cifra que ningún resultado respalda
const RESPUESTA_RECHAZADA: &str = "Son 999 grados";

/// Cada resultado lleva el identificador de su llamada: así se comprueba que llega íntegro
struct ToolServerEco;

fn salida_de(call_id: &str) -> serde_json::Value {
    json!({ "estado": "resuelto", "valor_resultado_texto": "125", "llamada": call_id })
}

#[async_trait]
impl ToolServerPort for ToolServerEco {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        ToolDefinition::lakehouse_tools()
    }

    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            output: salida_de(&call.id),
            is_error: false,
        })
    }
}

struct CatalogoVacio;

#[async_trait]
impl ModelCatalogPort for CatalogoVacio {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        Ok(vec![])
    }
}

enum Paso {
    Salida(AgentTurnOutput),
    Error(&'static str),
    /// El paso no termina nunca, como un proceso colgado: avisa por `colgado` y espera
    Colgar,
}

/// Runner guionizado que guarda lo que recibe en cada paso
struct RunnerQueRegistra {
    guion: Mutex<VecDeque<Paso>>,
    vistas: Mutex<Vec<Vec<AgentMessage>>>,
    colgado: Notify,
}

impl RunnerQueRegistra {
    fn con_guion(guion: Vec<Paso>) -> Arc<Self> {
        Arc::new(Self {
            guion: Mutex::new(guion.into()),
            vistas: Mutex::new(Vec::new()),
            colgado: Notify::new(),
        })
    }

    fn ultima_vista(&self) -> Vec<AgentMessage> {
        self.vistas
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("el runner no recibió ningún paso")
    }
}

#[async_trait]
impl AgentRunnerPort for RunnerQueRegistra {
    async fn ejecutar_turno(
        &self,
        _modelo: &ClaudeModel,
        _system_prompt: &str,
        historial: &[AgentMessage],
        _herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        self.vistas.lock().unwrap().push(historial.to_vec());
        // Un guion agotado es un fallo del test, no un paso colgado: así no se cuelga la suite
        let paso =
            self.guion.lock().unwrap().pop_front().expect(
                "guion del runner agotado: el orquestador pidió más pasos de los previstos",
            );
        match paso {
            Paso::Salida(salida) => Ok(salida),
            Paso::Error(mensaje) => Err(DomainError::AgentError(mensaje.to_string())),
            Paso::Colgar => {
                self.colgado.notify_one();
                pending().await
            }
        }
    }

    async fn ejecutar_turno_dirigido(
        &self,
        _modelo: &ClaudeModel,
        _system_prompt: &str,
        _historial: &[AgentMessage],
        _herramientas: &[ToolDefinition],
        dirigida: LakehouseToolName,
    ) -> Result<TurnoDirigido, DomainError> {
        Ok(TurnoDirigido::Llamada(ToolCall {
            id: "dirigida".to_string(),
            name: dirigida.as_str().to_string(),
            arguments: json!({}),
        }))
    }
}

/// Paso del modelo con `llamadas` peticiones en paralelo, todas distintas
fn paso_con_llamadas(prefijo: &str, llamadas: usize) -> AgentTurnOutput {
    AgentTurnOutput::ToolCalls(
        (0..llamadas)
            .map(|i| ToolCall {
                id: format!("{prefijo}_{i}"),
                name: "search_normative_context".to_string(),
                arguments: json!({ "centro_id": "c", "consulta": format!("termino {prefijo} {i}") }),
            })
            .collect(),
    )
}

fn final_con(texto: &str) -> Paso {
    Paso::Salida(AgentTurnOutput::FinalMessage(texto.to_string()))
}

/// Guion de un turno de 50 mensajes: pregunta, 3 pasos de 15 llamadas y respuesta final
fn guion_turno_largo(prefijo: &str) -> Vec<Paso> {
    let mut guion: Vec<Paso> = (0..3)
        .map(|paso| Paso::Salida(paso_con_llamadas(&format!("{prefijo}{paso}"), 15)))
        .collect();
    guion.push(final_con("Hecho."));
    guion
}

const MENSAJES_TURNO_LARGO: usize = 50;

fn servicio_con(runner: Arc<RunnerQueRegistra>) -> Arc<GuiChatService> {
    let orquestador = AgentOrchestrator::new(runner, Arc::new(ToolServerEco));
    Arc::new(GuiChatService::new(orquestador, Arc::new(CatalogoVacio)))
}

async fn enviar(servicio: &GuiChatService, mensaje: &str, centro: &str) {
    let respuesta = servicio.enviar_mensaje(mensaje, centro, "haiku").await;
    assert!(respuesta.is_ok(), "{respuesta:?}");
}

/// Cada resultado sigue a una petición que lo pidió, con la salida íntegra de la herramienta,
/// y ninguna petición se queda sin resultado
fn assert_resultados_integros_y_emparejados(mensajes: &[AgentMessage]) {
    let mut pedidas: HashSet<String> = HashSet::new();
    let mut resueltas: HashSet<String> = HashSet::new();
    for mensaje in mensajes {
        match mensaje.role {
            AgentRole::User => assert_eq!(pedidas, resueltas, "peticiones sin resultado"),
            AgentRole::Assistant => {
                for llamada in mensaje.tool_calls.iter().flatten() {
                    pedidas.insert(llamada.id.clone());
                }
            }
            AgentRole::Tool => {
                let id = mensaje.tool_call_id.clone().expect("resultado sin id");
                assert!(pedidas.contains(&id), "resultado {id} sin petición");
                assert_eq!(mensaje.content, salida_de(&id).to_string());
                resueltas.insert(id);
            }
        }
    }
    assert_eq!(pedidas, resueltas, "peticiones sin resultado");
}

/// Regresión de la prueba de concepto `poc_turno_largo_vacia_el_historial` de la auditoría, con
/// la aserción invertida: el turno de 50 mensajes se conserva entero
#[tokio::test]
async fn test_regresion_turno_largo_no_vacia_el_historial() {
    let mut guion: Vec<AgentTurnOutput> = (0..3)
        .map(|p| paso_con_llamadas(&format!("p{p}"), 15))
        .collect();
    guion.push(AgentTurnOutput::FinalMessage("Hecho.".to_string()));
    let orquestador = AgentOrchestrator::new(
        Arc::new(MockAgentRunner::new_scripted(guion)),
        Arc::new(ToolServerEco),
    );
    let servicio = GuiChatService::new(orquestador, Arc::new(CatalogoVacio));

    let respuesta = servicio.enviar_mensaje("hola", "c", "haiku").await;
    assert!(respuesta.is_ok(), "{respuesta:?}");

    let historial = servicio.historial("c").await;
    assert_eq!(historial.len(), MENSAJES_TURNO_LARGO);
    assert!(historial.len() > GuiChatService::MAX_HISTORIAL);
    assert_eq!(historial[0], AgentMessage::user("hola"));
    assert_eq!(historial.last(), Some(&AgentMessage::assistant("Hecho.")));
    assert_resultados_integros_y_emparejados(&historial);
}

#[tokio::test]
async fn test_el_turno_de_seguimiento_recibe_el_turno_largo_anterior() {
    let mut guion = guion_turno_largo("a");
    guion.push(final_con("De nada."));
    let runner = RunnerQueRegistra::con_guion(guion);
    let servicio = servicio_con(runner.clone());

    enviar(&servicio, "hola", "c").await;
    let tras_el_largo = servicio.historial("c").await;
    assert_eq!(tras_el_largo.len(), MENSAJES_TURNO_LARGO);

    enviar(&servicio, "gracias", "c").await;

    let vista = runner.ultima_vista();
    assert_eq!(vista.len(), MENSAJES_TURNO_LARGO + 1);
    assert_eq!(vista[..MENSAJES_TURNO_LARGO], tras_el_largo[..]);
    assert_eq!(vista[0], AgentMessage::user("hola"));
    assert_eq!(
        vista[MENSAJES_TURNO_LARGO - 1],
        AgentMessage::assistant("Hecho.")
    );
    assert_eq!(vista[MENSAJES_TURNO_LARGO], AgentMessage::user("gracias"));
    assert_resultados_integros_y_emparejados(&vista);

    // Los dos turnos juntos superan el tope: el antiguo se compacta a su pregunta y su
    // respuesta, y el reciente queda entero
    assert_eq!(
        servicio.historial("c").await,
        vec![
            AgentMessage::user("hola"),
            AgentMessage::assistant("Hecho."),
            AgentMessage::user("gracias"),
            AgentMessage::assistant("De nada.")
        ]
    );
}

/// Guion de tres turnos: uno de 50 mensajes, uno de 6 con herramientas y lo que siga
fn guion_largo_y_seguimiento_con_herramientas() -> Vec<Paso> {
    let mut guion = guion_turno_largo("a");
    guion.push(Paso::Salida(paso_con_llamadas("b0", 3)));
    guion.push(final_con("Segunda respuesta."));
    guion
}

/// Tras los dos primeros turnos del guion anterior: el largo compactado y el segundo entero
async fn assert_largo_compactado_y_segundo_entero(servicio: &GuiChatService) -> Vec<AgentMessage> {
    let historial = servicio.historial("c").await;
    assert_eq!(historial.len(), 8);
    assert_eq!(
        historial[..3],
        [
            AgentMessage::user("hola"),
            AgentMessage::assistant("Hecho."),
            AgentMessage::user("segunda"),
        ]
    );
    assert_eq!(historial[7], AgentMessage::assistant("Segunda respuesta."));
    assert_resultados_integros_y_emparejados(&historial);
    historial
}

#[tokio::test]
async fn test_el_tercer_turno_recibe_el_largo_compactado_y_el_segundo_entero() {
    let mut guion = guion_largo_y_seguimiento_con_herramientas();
    guion.push(final_con("Tercera respuesta."));
    let runner = RunnerQueRegistra::con_guion(guion);
    let servicio = servicio_con(runner.clone());

    enviar(&servicio, "hola", "c").await;
    assert_eq!(servicio.historial("c").await.len(), MENSAJES_TURNO_LARGO);
    enviar(&servicio, "segunda", "c").await;
    // Lo que el runner vio al cerrar el segundo turno: el largo entero y el segundo sin su respuesta
    let segundo_entero = runner.ultima_vista()[MENSAJES_TURNO_LARGO..].to_vec();
    assert_eq!(segundo_entero.len(), 5);
    let tras_el_segundo = assert_largo_compactado_y_segundo_entero(&servicio).await;
    assert_eq!(tras_el_segundo[2..7], segundo_entero[..]);

    enviar(&servicio, "tercera", "c").await;

    let vista = runner.ultima_vista();
    assert_eq!(vista.len(), 9);
    // Del turno largo, la pregunta y la respuesta, sin herramientas
    assert_eq!(vista[0], AgentMessage::user("hola"));
    assert_eq!(vista[1], AgentMessage::assistant("Hecho."));
    // El segundo, entero y con sus resultados íntegros
    assert_eq!(vista[2..8], tras_el_segundo[2..]);
    let resultados = vista.iter().filter(|m| m.role == AgentRole::Tool).count();
    assert_eq!(resultados, 3);
    assert!(vista.iter().all(|m| {
        m.tool_call_id
            .as_deref()
            .is_none_or(|id| id.starts_with("b0_"))
    }));
    assert_eq!(vista[8], AgentMessage::user("tercera"));
    assert_resultados_integros_y_emparejados(&vista);

    // Nada más que compactar: el historial solo crece con el tercer turno
    let historial = servicio.historial("c").await;
    assert_eq!(historial.len(), 10);
    assert_eq!(historial[..8], tras_el_segundo[..]);
}

#[tokio::test]
async fn test_turno_con_error_sobre_un_historial_compactado_lo_deja_como_estaba() {
    let mut guion = guion_largo_y_seguimiento_con_herramientas();
    guion.push(Paso::Salida(paso_con_llamadas("c0", 4)));
    guion.push(Paso::Error("fallo simulado del modelo"));
    let servicio = servicio_con(RunnerQueRegistra::con_guion(guion));

    enviar(&servicio, "hola", "c").await;
    enviar(&servicio, "segunda", "c").await;
    let antes = assert_largo_compactado_y_segundo_entero(&servicio).await;

    let error = servicio
        .enviar_mensaje("tercera", "c", "haiku")
        .await
        .expect_err("el turno falla");
    assert!(error.contains("fallo simulado del modelo"), "{error}");
    assert_eq!(servicio.historial("c").await, antes);
}

#[tokio::test]
async fn test_turno_detenido_sobre_un_historial_compactado_lo_deja_como_estaba() {
    let mut guion = guion_largo_y_seguimiento_con_herramientas();
    guion.push(Paso::Salida(paso_con_llamadas("c0", 4)));
    guion.push(Paso::Colgar);
    let runner = RunnerQueRegistra::con_guion(guion);
    let servicio = servicio_con(runner.clone());

    enviar(&servicio, "hola", "c").await;
    enviar(&servicio, "segunda", "c").await;
    let antes = assert_largo_compactado_y_segundo_entero(&servicio).await;

    let en_curso = {
        let servicio = servicio.clone();
        tokio::spawn(async move {
            servicio
                .enviar_mensaje_en_vivo("tercera", "c", "haiku", "t3", Arc::new(|_| {}))
                .await
        })
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        runner.colgado.notified(),
    )
    .await
    .expect("el turno no llegó al paso colgado");
    assert!(servicio.detener("t3"));

    let error = en_curso.await.unwrap().expect_err("detenido");
    assert_eq!(error, MENSAJE_RESPUESTA_DETENIDA);
    assert_eq!(servicio.historial("c").await, antes);
}

#[tokio::test]
async fn test_los_resultados_de_un_turno_que_cabe_llegan_integros_al_siguiente() {
    let runner = RunnerQueRegistra::con_guion(vec![
        Paso::Salida(paso_con_llamadas("a0", 3)),
        Paso::Salida(paso_con_llamadas("a1", 3)),
        final_con("Hecho."),
        final_con("De nada."),
    ]);
    let servicio = servicio_con(runner.clone());

    enviar(&servicio, "hola", "c").await;
    let primer_turno = servicio.historial("c").await;
    // Pregunta, 2 peticiones con 3 resultados cada una y respuesta
    assert_eq!(primer_turno.len(), 10);

    enviar(&servicio, "gracias", "c").await;

    let vista = runner.ultima_vista();
    assert_eq!(vista[..10], primer_turno[..]);
    assert_eq!(vista[10], AgentMessage::user("gracias"));
    let resultados = vista.iter().filter(|m| m.role == AgentRole::Tool).count();
    assert_eq!(resultados, 6);
    assert_resultados_integros_y_emparejados(&vista);

    // Los dos turnos caben en el tope: el historial los conserva tal cual
    let historial = servicio.historial("c").await;
    assert_eq!(historial.len(), 12);
    assert_eq!(historial[..10], primer_turno[..]);
}

#[tokio::test]
async fn test_turno_con_error_tras_un_turno_largo_deja_el_historial_como_estaba() {
    let mut guion = guion_turno_largo("a");
    guion.push(Paso::Salida(paso_con_llamadas("b0", 4)));
    guion.push(Paso::Error("fallo simulado del modelo"));
    let servicio = servicio_con(RunnerQueRegistra::con_guion(guion));

    enviar(&servicio, "hola", "c").await;
    let antes = servicio.historial("c").await;
    assert_eq!(antes.len(), MENSAJES_TURNO_LARGO);

    let error = servicio
        .enviar_mensaje("gracias", "c", "haiku")
        .await
        .expect_err("el turno falla");
    assert!(error.contains("fallo simulado del modelo"), "{error}");
    assert_eq!(servicio.historial("c").await, antes);
}

#[tokio::test]
async fn test_turno_detenido_tras_un_turno_largo_deja_el_historial_como_estaba() {
    let mut guion = guion_turno_largo("a");
    guion.push(Paso::Salida(paso_con_llamadas("b0", 4)));
    guion.push(Paso::Colgar);
    let runner = RunnerQueRegistra::con_guion(guion);
    let servicio = servicio_con(runner.clone());

    enviar(&servicio, "hola", "c").await;
    let antes = servicio.historial("c").await;
    assert_eq!(antes.len(), MENSAJES_TURNO_LARGO);

    let en_curso = {
        let servicio = servicio.clone();
        tokio::spawn(async move {
            servicio
                .enviar_mensaje_en_vivo("gracias", "c", "haiku", "t2", Arc::new(|_| {}))
                .await
        })
    };
    // El turno ya ejecutó sus 4 herramientas y espera al modelo: ahí se detiene
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        runner.colgado.notified(),
    )
    .await
    .expect("el turno no llegó al paso colgado");
    assert!(servicio.detener("t2"));

    let error = en_curso.await.unwrap().expect_err("detenido");
    assert_eq!(error, MENSAJE_RESPUESTA_DETENIDA);
    assert_eq!(servicio.historial("c").await, antes);
}

#[tokio::test]
async fn test_turno_largo_que_acaba_en_salida_segura_se_conserva_como_turno_completo() {
    // Llamada dirigida, 3 pasos de 15 llamadas y dos respuestas que el guard rechaza
    let mut guion: Vec<Paso> = (0..3)
        .map(|paso| Paso::Salida(paso_con_llamadas(&format!("a{paso}"), 15)))
        .collect();
    guion.push(final_con(RESPUESTA_RECHAZADA));
    guion.push(final_con(RESPUESTA_RECHAZADA));
    let servicio = servicio_con(RunnerQueRegistra::con_guion(guion));

    let payload = servicio
        .enviar_mensaje(PREGUNTA_CRUZABLE, "c", "haiku")
        .await
        .expect("la salida segura es una respuesta, no un error");
    assert!(payload.salida_segura);

    let historial = servicio.historial("c").await;
    // Pregunta, llamada dirigida con su resultado, 3 × (petición + 15 resultados) y mensaje fijo
    assert_eq!(historial.len(), 52);
    assert_eq!(historial[0], AgentMessage::user(PREGUNTA_CRUZABLE));
    assert_eq!(
        historial.last(),
        Some(&AgentMessage::assistant(MENSAJE_RESPUESTA_NO_VALIDADA))
    );
    // Ni el recordatorio del reintento ni lo rechazado quedan en el historial
    let del_operador = historial
        .iter()
        .filter(|m| m.role == AgentRole::User)
        .count();
    assert_eq!(del_operador, 1);
    assert!(historial.iter().all(|m| m.content != RESPUESTA_RECHAZADA));
    assert_resultados_integros_y_emparejados(&historial);
}

#[tokio::test]
async fn test_acotar_un_centro_no_toca_el_otro() {
    // Tres turnos de 18 mensajes en un centro: el tercero obliga a compactar el primero
    let mut guion = vec![final_con("Respuesta del otro centro.")];
    for turno in 0..3 {
        guion.push(Paso::Salida(paso_con_llamadas(&format!("t{turno}"), 15)));
        guion.push(final_con("Hecho."));
    }
    let servicio = servicio_con(RunnerQueRegistra::con_guion(guion));

    enviar(&servicio, "hola", "centro_b").await;
    let otro_centro = servicio.historial("centro_b").await;
    assert_eq!(otro_centro.len(), 2);

    enviar(&servicio, "primera", "centro_a").await;
    enviar(&servicio, "segunda", "centro_a").await;
    let antes_de_acotar = servicio.historial("centro_a").await;
    assert_eq!(antes_de_acotar.len(), 36);
    enviar(&servicio, "tercera", "centro_a").await;

    // 54 mensajes: el primer turno queda en pregunta y respuesta, y el segundo entero
    let acotado = servicio.historial("centro_a").await;
    assert_eq!(acotado.len(), 38);
    assert_eq!(
        acotado[..2],
        [
            AgentMessage::user("primera"),
            AgentMessage::assistant("Hecho.")
        ]
    );
    assert_eq!(acotado[2..20], antes_de_acotar[18..]);
    assert_eq!(acotado[20], AgentMessage::user("tercera"));
    assert_resultados_integros_y_emparejados(&acotado);

    assert_eq!(servicio.historial("centro_b").await, otro_centro);
}
