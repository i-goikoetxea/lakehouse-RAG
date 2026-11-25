//! Un historial compactado (F-2) sigue siendo una conversación válida para los dos motores: el
//! adaptador real de la API de Messages lo traduce sin bloques a medias y la entrada del CLI no
//! lleva resultados huérfanos. Servidor local, sin red, sin keys reales y sin Claude.

use std::collections::HashSet;
use std::time::Duration;

use lakehouse_rag::application::dtos::agent::{
    AgentMessage, AgentRole, AgentTurnOutput, ClaudeModel,
};
use lakehouse_rag::application::dtos::tools::{ToolCall, ToolDefinition};
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::application::use_cases::historial_turnos::acotar_historial_por_turnos;
use lakehouse_rag::infrastructure::llm::ClaudeAgentRunner;
use lakehouse_rag::infrastructure::llm::anthropic_api::{
    AnthropicHttpClient, AnthropicMessagesRunner, ConfigClienteAnthropic, PerfilApi,
    PoliticaReintentos, SecretApiKey,
};
use lakehouse_rag::presentation::gui::chat_service::GuiChatService;
use serde_json::{Value, json};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_FALSA: &str = "sk-ant-test-NO-ES-REAL-compactado-1";
const SISTEMA: &str = "Eres el asistente de un centro de prueba.";
const FIRMA_ANTIGUA: &str = "FIRMA-DEL-TURNO-ANTIGUO";
const FIRMA_RECIENTE: &str = "FIRMA-DEL-TURNO-RECIENTE";

fn runner(base_url: &str) -> AnthropicMessagesRunner {
    let config = ConfigClienteAnthropic {
        base_url: base_url.into(),
        timeout_conexion: Duration::from_millis(500),
        timeout_lectura: Duration::from_secs(5),
        reintentos: PoliticaReintentos {
            max_reintentos: 1,
            espera_base: Duration::from_millis(1),
            espera_maxima: Duration::from_millis(2),
            tope_retry_after: Duration::from_millis(5),
        },
        perfil: PerfilApi::ANTHROPIC,
    };
    AnthropicMessagesRunner::new(
        AnthropicHttpClient::new(SecretApiKey::new(KEY_FALSA).unwrap(), config).unwrap(),
    )
}

fn herramientas() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "search_normative_context".into(),
        description: "tool de búsqueda".into(),
        parameters: json!({"type": "object", "properties": {"consulta": {"type": "string"}}}),
    }]
}

fn sse(eventos: &[Value]) -> String {
    eventos
        .iter()
        .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
        .collect()
}

fn inicio() -> Value {
    json!({"type":"message_start","message":{"id":"msg","model":"claude-sonnet-5-5","content":[],
        "usage":{"input_tokens":10,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":1}}})
}

fn fin(stop_reason: &str) -> [Value; 2] {
    [
        json!({"type":"message_delta","delta":{"stop_reason":stop_reason},"usage":{"output_tokens":5}}),
        json!({"type":"message_stop"}),
    ]
}

/// Pensamiento firmado con `firma` y un `tool_use` por cada id
fn flujo_de_tools(firma: &str, ids: &[&str]) -> String {
    let mut eventos = vec![
        inicio(),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":firma}}),
        json!({"type":"content_block_stop","index":0}),
    ];
    for (i, id) in ids.iter().enumerate() {
        let indice = i + 1;
        eventos.push(json!({"type":"content_block_start","index":indice,"content_block":{"type":"tool_use","id":id,"name":"search_normative_context","input":{}}}));
        eventos.push(json!({"type":"content_block_delta","index":indice,"delta":{"type":"input_json_delta","partial_json":"{\"consulta\":\"plazo\"}"}}));
        eventos.push(json!({"type":"content_block_stop","index":indice}));
    }
    eventos.extend(fin("tool_use"));
    sse(&eventos)
}

fn flujo_de_texto(texto: &str) -> String {
    let mut eventos = vec![
        inicio(),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":texto}}),
        json!({"type":"content_block_stop","index":0}),
    ];
    eventos.extend(fin("end_turn"));
    sse(&eventos)
}

async fn servidor_con(cuerpos: &[String]) -> MockServer {
    let servidor = MockServer::start().await;
    for cuerpo in cuerpos {
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(cuerpo.clone(), "text/event-stream"),
            )
            .up_to_n_times(1)
            .mount(&servidor)
            .await;
    }
    servidor
}

/// Un turno completo contra el adaptador real, como lo haría el orquestador: pregunta, pasos de
/// herramientas con sus resultados y respuesta final
async fn turno_por_la_api(
    runner: &AnthropicMessagesRunner,
    historial: &mut Vec<AgentMessage>,
    pregunta: &str,
) {
    historial.push(AgentMessage::user(pregunta));
    loop {
        let salida = runner
            .ejecutar_turno(&ClaudeModel::sonnet(), SISTEMA, historial, &herramientas())
            .await
            .unwrap();
        match salida {
            AgentTurnOutput::ToolCalls(llamadas) => {
                historial.push(AgentMessage::assistant_with_tools(llamadas.clone()));
                for llamada in &llamadas {
                    historial.push(AgentMessage::tool_result(
                        &llamada.id,
                        format!("{{\"resultado\":\"{}\"}}", llamada.id),
                    ));
                }
            }
            AgentTurnOutput::FinalMessage(texto) => {
                historial.push(AgentMessage::assistant(texto));
                return;
            }
        }
    }
}

fn mensajes_de_la_peticion(
    runner: &AnthropicMessagesRunner,
    historial: &[AgentMessage],
) -> Vec<Value> {
    let peticion = runner.construir_peticion(
        &ClaudeModel::sonnet(),
        SISTEMA,
        historial,
        &herramientas(),
        None,
    );
    serde_json::to_value(&peticion).unwrap()["messages"]
        .as_array()
        .unwrap()
        .clone()
}

fn ids_de(mensaje: &Value, tipo: &str, campo: &str) -> Vec<String> {
    mensaje["content"]
        .as_array()
        .expect("el contenido va en bloques")
        .iter()
        .filter(|b| b["type"] == tipo)
        .map(|b| b[campo].as_str().unwrap().to_string())
        .collect()
}

fn firmas_de(mensajes: &[Value]) -> Vec<String> {
    mensajes
        .iter()
        .flat_map(|m| ids_de(m, "thinking", "signature"))
        .collect()
}

/// Lo que la API de Messages exige de la lista: empieza y acaba en `user`, alterna roles, cada
/// `tool_use` tiene su `tool_result` en el mensaje siguiente y no hay `tool_result` de más, y
/// un bloque `thinking` solo acompaña a un mensaje que pide herramientas
fn assert_secuencia_valida(mensajes: &[Value]) {
    assert!(!mensajes.is_empty());
    for (i, mensaje) in mensajes.iter().enumerate() {
        let esperado = if i % 2 == 0 { "user" } else { "assistant" };
        assert_eq!(mensaje["role"], esperado, "mensaje {i}: {mensaje}");
        assert!(
            !mensaje["content"].as_array().unwrap().is_empty(),
            "mensaje {i} vacío"
        );
        let anterior_pide = if i == 0 {
            Vec::new()
        } else {
            ids_de(&mensajes[i - 1], "tool_use", "id")
        };
        if esperado == "user" {
            assert_eq!(
                ids_de(mensaje, "tool_result", "tool_use_id"),
                anterior_pide,
                "mensaje {i}: resultados que no casan con las peticiones"
            );
        } else {
            assert!(ids_de(mensaje, "tool_result", "tool_use_id").is_empty());
            if ids_de(mensaje, "tool_use", "id").is_empty() {
                assert!(
                    ids_de(mensaje, "thinking", "signature").is_empty(),
                    "mensaje {i}: thinking sin petición de herramienta"
                );
            }
        }
    }
    let ultimo = mensajes.last().unwrap();
    assert_eq!(ultimo["role"], "user");
}

#[tokio::test]
async fn test_historial_compactado_se_traduce_a_una_secuencia_valida_sin_thinking_retirado() {
    let servidor = servidor_con(&[
        flujo_de_tools(FIRMA_ANTIGUA, &["toolu_a1", "toolu_a2"]),
        flujo_de_texto("Respuesta uno."),
        flujo_de_tools(FIRMA_RECIENTE, &["toolu_b1", "toolu_b2"]),
        flujo_de_texto("Respuesta dos."),
    ])
    .await;
    let runner = runner(&servidor.uri());
    let mut historial = Vec::new();
    turno_por_la_api(&runner, &mut historial, "Primera pregunta").await;
    turno_por_la_api(&runner, &mut historial, "Segunda pregunta").await;
    assert_eq!(historial.len(), 10);

    // Control: sin compactar, el adaptador reinserta el pensamiento firmado de los dos turnos
    let sin_compactar = [historial.clone(), vec![AgentMessage::user("Tercera")]].concat();
    let mensajes = mensajes_de_la_peticion(&runner, &sin_compactar);
    assert_secuencia_valida(&mensajes);
    assert_eq!(firmas_de(&mensajes), [FIRMA_ANTIGUA, FIRMA_RECIENTE]);

    // Con un tope de 8 solo hace falta compactar el primer turno
    acotar_historial_por_turnos(&mut historial, 8);
    assert_eq!(historial.len(), 7);
    historial.push(AgentMessage::user("Tercera"));

    let mensajes = mensajes_de_la_peticion(&runner, &historial);
    assert_secuencia_valida(&mensajes);
    assert_eq!(mensajes.len(), 7, "{mensajes:?}");
    // El turno compactado son dos mensajes de texto plano
    assert_eq!(
        mensajes[0]["content"],
        json!([{"type": "text", "text": "Primera pregunta"}])
    );
    assert_eq!(
        mensajes[1]["content"],
        json!([{"type": "text", "text": "Respuesta uno."}])
    );
    // Del ciclo retirado no queda ni la petición, ni el resultado, ni su pensamiento
    let cuerpo = Value::Array(mensajes.clone()).to_string();
    for retirado in ["toolu_a1", "toolu_a2", FIRMA_ANTIGUA] {
        assert!(!cuerpo.contains(retirado), "queda {retirado}");
    }
    // El turno que no se compactó conserva su ciclo entero, con el pensamiento firmado
    assert_eq!(firmas_de(&mensajes), [FIRMA_RECIENTE]);
    assert_eq!(
        ids_de(&mensajes[3], "tool_use", "id"),
        ["toolu_b1", "toolu_b2"]
    );
    assert_eq!(
        ids_de(&mensajes[4], "tool_result", "tool_use_id"),
        ["toolu_b1", "toolu_b2"]
    );
}

fn llamada(id: &str) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        name: "search_normative_context".to_string(),
        arguments: json!({ "consulta": id }),
    }
}

/// Turno con `pasos` peticiones de `llamadas` herramientas cada una
fn turno(etiqueta: &str, pasos: usize, llamadas: usize) -> Vec<AgentMessage> {
    let mut turno = vec![AgentMessage::user(format!("pregunta {etiqueta}"))];
    for paso in 0..pasos {
        let ids: Vec<String> = (0..llamadas)
            .map(|i| format!("{etiqueta}_{paso}_{i}"))
            .collect();
        turno.push(AgentMessage::assistant_with_tools(
            ids.iter().map(|id| llamada(id)).collect(),
        ));
        turno.extend(
            ids.iter()
                .map(|id| AgentMessage::tool_result(id, format!("resultado {id}"))),
        );
    }
    turno.push(AgentMessage::assistant(format!("respuesta {etiqueta}")));
    turno
}

/// Historial con el tope real del chat: un turno de 50 mensajes compactado, un seguimiento con
/// herramientas entero y la pregunta siguiente
fn historial_compactado_con_pregunta_nueva() -> (Vec<AgentMessage>, Vec<AgentMessage>) {
    let largo = turno("largo", 3, 15);
    assert_eq!(largo.len(), 50);
    let seguimiento = turno("seguimiento", 1, 3);
    let mut historial = [largo.clone(), seguimiento.clone()].concat();
    acotar_historial_por_turnos(&mut historial, GuiChatService::MAX_HISTORIAL);
    assert_eq!(
        historial,
        [
            vec![largo[0].clone(), largo[49].clone()],
            seguimiento.clone()
        ]
        .concat()
    );
    historial.push(AgentMessage::user("pregunta nueva"));
    (largo, historial)
}

#[test]
fn test_historial_compactado_con_el_tope_real_es_valido_para_la_api() {
    let (_, historial) = historial_compactado_con_pregunta_nueva();
    // Un adaptador que no recuerda ningún turno (otra sesión de la app): reconstruye las
    // peticiones desde el historial. No se envía nada
    let mensajes = mensajes_de_la_peticion(&runner("http://127.0.0.1:9"), &historial);
    assert_secuencia_valida(&mensajes);
    assert_eq!(mensajes.len(), 7);
    assert!(firmas_de(&mensajes).is_empty());
}

#[test]
fn test_entrada_del_cli_con_historial_compactado_no_lleva_resultados_huerfanos() {
    let (largo, historial) = historial_compactado_con_pregunta_nueva();
    let entrada =
        ClaudeAgentRunner::construir_entrada_conversacion(&historial, &herramientas()).unwrap();

    // El documento JSON va tras el protocolo (que también lleva llaves) y antes de la consigna
    let marca = "DISPONIBLES:\n";
    let inicio = entrada.find(marca).expect("falta la conversación") + marca.len();
    let fin = entrada.rfind('}').unwrap();
    let documento: Value = serde_json::from_str(&entrada[inicio..=fin]).unwrap();
    let mensajes: Vec<AgentMessage> = serde_json::from_value(documento["messages"].clone())
        .expect("los mensajes conservan su forma");
    assert_eq!(mensajes, historial);

    // Cada resultado sigue a su petición dentro del mismo turno, y cada petición tiene resultado
    let mut pedidas: HashSet<String> = HashSet::new();
    let mut resueltas: HashSet<String> = HashSet::new();
    for mensaje in &mensajes {
        match mensaje.role {
            AgentRole::User => {
                assert_eq!(pedidas, resueltas, "peticiones sin resultado");
                pedidas.clear();
                resueltas.clear();
            }
            AgentRole::Assistant => {
                for llamada in mensaje.tool_calls.iter().flatten() {
                    pedidas.insert(llamada.id.clone());
                }
            }
            AgentRole::Tool => {
                let id = mensaje.tool_call_id.clone().expect("resultado sin id");
                assert!(pedidas.contains(&id), "resultado {id} sin petición");
                resueltas.insert(id);
            }
        }
    }
    let resultados = mensajes.iter().filter(|m| m.role == AgentRole::Tool);
    assert_eq!(resultados.count(), 3, "solo los del turno no compactado");

    // Del turno compactado no viaja ninguna petición ni resultado
    for retirado in largo.iter().filter_map(|m| m.tool_call_id.as_deref()) {
        assert!(
            !entrada.contains(&format!("\"{retirado}\"")),
            "queda {retirado}"
        );
    }
    assert!(entrada.contains("respuesta largo"));
}
