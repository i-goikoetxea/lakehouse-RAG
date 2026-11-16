//! `AnthropicMessagesRunner` contra un servidor local (wiremock y TCP en crudo), sin red: forma
//! de la petición, traducción del historial, bloques `thinking` devueltos intactos, turno
//! dirigido, streaming, métricas y aborto.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use lakehouse_rag::application::dtos::agent::{
    AgentMessage, AgentTurnOutput, ClaudeModel, TurnoDirigido,
};
use lakehouse_rag::application::dtos::tools::{LakehouseToolName, ToolCall, ToolDefinition};
use lakehouse_rag::application::ports::agent_runner::{AgentRunnerPort, ObservadorTexto};
use lakehouse_rag::infrastructure::llm::anthropic_api::{
    AnthropicHttpClient, AnthropicMessagesRunner, ConfigClienteAnthropic, PoliticaReintentos,
    SecretApiKey,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_FALSA: &str = "sk-ant-test-NO-ES-REAL-agente-1";
const SISTEMA: &str = "Eres el asistente de un centro de prueba.";

fn config(base_url: &str) -> ConfigClienteAnthropic {
    ConfigClienteAnthropic {
        base_url: base_url.into(),
        timeout_conexion: Duration::from_millis(500),
        timeout_lectura: Duration::from_secs(5),
        reintentos: PoliticaReintentos {
            max_reintentos: 1,
            espera_base: Duration::from_millis(1),
            espera_maxima: Duration::from_millis(2),
            tope_retry_after: Duration::from_millis(5),
        },
    }
}

fn runner(base_url: &str) -> AnthropicMessagesRunner {
    AnthropicMessagesRunner::new(
        AnthropicHttpClient::new(SecretApiKey::new(KEY_FALSA).unwrap(), config(base_url)).unwrap(),
    )
}

fn herramientas() -> Vec<ToolDefinition> {
    ["search_normative_context", "lookup_double_entry_table"]
        .into_iter()
        .map(|nombre| ToolDefinition {
            name: nombre.into(),
            description: format!("tool {nombre}"),
            parameters: json!({"type": "object", "properties": {"consulta": {"type": "string"}}}),
        })
        .collect()
}

fn sse(eventos: &[Value]) -> String {
    eventos
        .iter()
        .map(|e| format!("event: {}\ndata: {e}\n\n", e["type"].as_str().unwrap()))
        .collect()
}

fn inicio(input: u64, escritura: u64, lectura: u64) -> Value {
    json!({"type":"message_start","message":{"id":"msg","model":"claude-sonnet-5-5","content":[],
        "usage":{"input_tokens":input,"cache_creation_input_tokens":escritura,"cache_read_input_tokens":lectura,"output_tokens":1}}})
}

fn fin(stop_reason: &str, salida: u64) -> [Value; 2] {
    [
        json!({"type":"message_delta","delta":{"stop_reason":stop_reason},"usage":{"output_tokens":salida}}),
        json!({"type":"message_stop"}),
    ]
}

/// Pensamiento firmado + dos `tool_use` en paralelo
fn flujo_de_tools(ids: [&str; 2], nombres: [&str; 2]) -> String {
    let mut eventos = vec![
        inicio(100, 4000, 0),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"FIRMA-1"}}),
        json!({"type":"content_block_stop","index":0}),
    ];
    for (i, (id, nombre)) in ids.iter().zip(nombres).enumerate() {
        let indice = i + 1;
        eventos.push(json!({"type":"content_block_start","index":indice,"content_block":{"type":"tool_use","id":id,"name":nombre,"input":{}}}));
        eventos.push(json!({"type":"content_block_delta","index":indice,"delta":{"type":"input_json_delta","partial_json":"{\"consulta\":\"plazo\"}"}}));
        eventos.push(json!({"type":"content_block_stop","index":indice}));
    }
    eventos.extend(fin("tool_use", 80));
    sse(&eventos)
}

fn flujo_de_texto(fragmentos: &[&str]) -> String {
    let mut eventos = vec![
        inicio(20, 0, 4000),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
    ];
    for fragmento in fragmentos {
        eventos.push(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":fragmento}}));
    }
    eventos.push(json!({"type":"content_block_stop","index":0}));
    eventos.extend(fin("end_turn", 30));
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

async fn cuerpos_recibidos(servidor: &MockServer) -> Vec<Value> {
    servidor
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

#[tokio::test]
async fn test_la_peticion_lleva_auto_cache_thinking_y_sin_temperature() {
    let servidor = servidor_con(&[flujo_de_texto(&["Hola"])]).await;
    let runner = runner(&servidor.uri());
    let salida = runner
        .ejecutar_turno(
            &ClaudeModel::sonnet(),
            SISTEMA,
            &[AgentMessage::user("¿Qué es una junta?")],
            &herramientas(),
        )
        .await
        .unwrap();
    assert_eq!(salida, AgentTurnOutput::FinalMessage("Hola".into()));

    let recibidas = servidor.received_requests().await.unwrap();
    assert_eq!(
        recibidas[0].headers.get("anthropic-beta").unwrap(),
        "thinking-binding-controls-2026-08-01"
    );
    let cuerpo: Value = serde_json::from_slice(&recibidas[0].body).unwrap();
    assert_eq!(cuerpo["model"], "claude-sonnet-5-5");
    assert_eq!(cuerpo["tool_choice"], json!({"type": "auto"}));
    assert_eq!(cuerpo["stream"], true);
    assert!(cuerpo.get("temperature").is_none());
    assert_eq!(cuerpo["thinking"]["type"], "adaptive");
    assert_eq!(
        cuerpo["thinking"]["block_binding"]["prefix_mismatch_behavior"],
        "drop_block"
    );
    assert_eq!(cuerpo["system"][0]["text"], SISTEMA);
    assert_eq!(cuerpo["system"][0]["cache_control"]["type"], "ephemeral");
    assert!(cuerpo["tools"][0].get("cache_control").is_none());
    assert_eq!(cuerpo["tools"][1]["cache_control"]["type"], "ephemeral");
    assert_eq!(
        cuerpo["messages"],
        json!([{"role": "user", "content": [
            {"type": "text", "text": "¿Qué es una junta?", "cache_control": {"type": "ephemeral"}}
        ]}])
    );
}

#[tokio::test]
async fn test_tool_use_en_paralelo_y_el_turno_se_devuelve_con_su_thinking() {
    let servidor = servidor_con(&[
        flujo_de_tools(
            ["toolu_a", "toolu_b"],
            ["search_normative_context", "lookup_double_entry_table"],
        ),
        flujo_de_texto(&["Listo"]),
    ])
    .await;
    let runner = runner(&servidor.uri());
    let modelo = ClaudeModel::sonnet();
    let mut historial = vec![AgentMessage::user("Compara A y B")];
    let AgentTurnOutput::ToolCalls(llamadas) = runner
        .ejecutar_turno(&modelo, SISTEMA, &historial, &herramientas())
        .await
        .unwrap()
    else {
        panic!("se esperaban llamadas");
    };
    assert_eq!(llamadas.len(), 2);
    assert_eq!(llamadas[0].arguments, json!({"consulta": "plazo"}));

    historial.push(AgentMessage::assistant_with_tools(llamadas.clone()));
    historial.push(AgentMessage::tool_result("toolu_a", "{\"r\":1}"));
    historial.push(AgentMessage::tool_result("toolu_b", "{\"r\":2}"));
    runner
        .ejecutar_turno(&modelo, SISTEMA, &historial, &herramientas())
        .await
        .unwrap();

    let segunda = &cuerpos_recibidos(&servidor).await[1];
    let mensajes = segunda["messages"].as_array().unwrap();
    assert_eq!(mensajes.len(), 3, "{mensajes:?}");
    assert_eq!(mensajes[1]["role"], "assistant");
    assert_eq!(
        mensajes[1]["content"][0],
        json!({"type": "thinking", "thinking": "", "signature": "FIRMA-1"})
    );
    assert_eq!(mensajes[1]["content"][1]["id"], "toolu_a");
    assert_eq!(mensajes[2]["role"], "user");
    let resultados = mensajes[2]["content"].as_array().unwrap();
    assert_eq!(
        resultados.len(),
        2,
        "los tool_result van en un solo mensaje"
    );
    assert_eq!(resultados[0]["tool_use_id"], "toolu_a");
    assert_eq!(resultados[1]["tool_use_id"], "toolu_b");
    assert_eq!(resultados[1]["cache_control"]["type"], "ephemeral");
}

#[tokio::test]
async fn test_un_recordatorio_tras_los_resultados_va_en_el_mismo_mensaje() {
    let servidor = servidor_con(&[flujo_de_texto(&["Ok"])]).await;
    let runner = runner(&servidor.uri());
    let llamada = ToolCall {
        id: "call_x".into(),
        name: "search_normative_context".into(),
        arguments: json!({}),
    };
    let historial = vec![
        AgentMessage::user("¿Plazo?"),
        AgentMessage::assistant_with_tools(vec![llamada]),
        AgentMessage::tool_result("call_x", ""),
        AgentMessage::user("RECORDATORIO DEL SISTEMA: usa solo cifras de las salidas."),
    ];
    runner
        .ejecutar_turno(&ClaudeModel::sonnet(), SISTEMA, &historial, &herramientas())
        .await
        .unwrap();
    let cuerpo = &cuerpos_recibidos(&servidor).await[0];
    let mensajes = cuerpo["messages"].as_array().unwrap();
    assert_eq!(mensajes.len(), 3);
    // Un turno que el runner no recuerda se reconstruye con `tool_use`
    assert_eq!(mensajes[1]["content"][0]["type"], "tool_use");
    assert_eq!(mensajes[2]["content"][0]["type"], "tool_result");
    assert_eq!(mensajes[2]["content"][0]["content"], "(sin contenido)");
    assert_eq!(mensajes[2]["content"][1]["type"], "text");
}

#[tokio::test]
async fn test_turno_dirigido_con_directiva_que_persiste_en_su_sitio() {
    let servidor = servidor_con(&[
        flujo_de_tools(
            ["toolu_d", "toolu_otra"],
            ["lookup_double_entry_table", "search_normative_context"],
        ),
        flujo_de_texto(&["Celda"]),
        flujo_de_texto(&["Sin llamada"]),
    ])
    .await;
    let runner = runner(&servidor.uri());
    let modelo = ClaudeModel::sonnet();
    let mut historial = vec![AgentMessage::user("Tabla 4-A, ¿30 mm y 0,42 %?")];
    let salida = runner
        .ejecutar_turno_dirigido(
            &modelo,
            SISTEMA,
            &historial,
            &herramientas(),
            LakehouseToolName::LookupDoubleEntryTable,
        )
        .await
        .unwrap();
    let TurnoDirigido::Llamada(llamada) = salida else {
        panic!("se esperaba la llamada dirigida");
    };
    assert_eq!(llamada.id, "toolu_d");

    let primera = &cuerpos_recibidos(&servidor).await[0];
    let ultimo = primera["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(ultimo["role"], "system");
    assert!(
        ultimo["content"]
            .as_str()
            .unwrap()
            .contains("lookup_double_entry_table")
    );
    assert_eq!(primera["tool_choice"], json!({"type": "auto"}));

    historial.push(AgentMessage::assistant_with_tools(vec![llamada]));
    historial.push(AgentMessage::tool_result("toolu_d", "{\"valor\":125}"));
    runner
        .ejecutar_turno(&modelo, SISTEMA, &historial, &herramientas())
        .await
        .unwrap();
    let segunda = &cuerpos_recibidos(&servidor).await[1];
    let roles: Vec<&str> = segunda["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, vec!["user", "system", "assistant", "user"]);
    let asistente = &segunda["messages"][2]["content"];
    assert_eq!(
        asistente.as_array().unwrap().len(),
        2,
        "solo thinking y la dirigida"
    );

    let sin_llamada = runner
        .ejecutar_turno_dirigido(
            &modelo,
            SISTEMA,
            &[AgentMessage::user("¿Quién firma?")],
            &herramientas(),
            LakehouseToolName::QueryOperationalMatrix,
        )
        .await
        .unwrap();
    assert_eq!(sin_llamada, TurnoDirigido::SinLlamada);
}

#[tokio::test]
async fn test_en_vivo_emite_los_fragmentos_y_sin_observador_no() {
    let servidor = servidor_con(&[flujo_de_texto(&["Es ", "un ", "cordón."])]).await;
    let runner = runner(&servidor.uri());
    let vistos = Arc::new(Mutex::new(Vec::<String>::new()));
    let destino = Arc::clone(&vistos);
    let observador: ObservadorTexto =
        Arc::new(move |f: &str| destino.lock().unwrap().push(f.into()));
    let salida = runner
        .ejecutar_turno_en_vivo(
            &ClaudeModel::sonnet(),
            SISTEMA,
            &[AgentMessage::user("¿Qué es?")],
            &herramientas(),
            observador,
        )
        .await
        .unwrap();
    assert_eq!(
        salida,
        AgentTurnOutput::FinalMessage("Es un cordón.".into())
    );
    assert_eq!(*vistos.lock().unwrap(), vec!["Es ", "un ", "cordón."]);
}

#[tokio::test]
async fn test_metricas_por_paso_con_tokens_coste_y_herramientas() {
    let servidor = servidor_con(&[
        flujo_de_tools(
            ["toolu_a", "toolu_b"],
            ["search_normative_context", "search_normative_context"],
        ),
        flujo_de_texto(&["Ok"]),
    ])
    .await;
    let runner = runner(&servidor.uri());
    let modelo = ClaudeModel::new("claude-sonnet-5-5").unwrap();
    for _ in 0..2 {
        runner
            .ejecutar_turno(
                &modelo,
                SISTEMA,
                &[AgentMessage::user("x")],
                &herramientas(),
            )
            .await
            .unwrap();
    }
    let metricas = runner.tomar_metricas();
    assert_eq!(metricas.len(), 2);
    assert_eq!(metricas[0].herramientas.len(), 2);
    assert_eq!(metricas[0].cache_creation_input_tokens, 4000);
    assert_eq!(metricas[0].output_tokens, 80);
    // 100·2 + 4000·2,5 + 80·10 por millón
    let esperado = (100.0 * 2.0 + 4000.0 * 2.5 + 80.0 * 10.0) / 1e6;
    assert!((metricas[0].coste_usd.unwrap() - esperado).abs() < 1e-12);
    assert_eq!(metricas[1].cache_read_input_tokens, 4000);
    assert!(metricas[1].ttft_ms.is_some());
    assert!(
        runner.tomar_metricas().is_empty(),
        "tomar vacía el registro"
    );
    let telemetria = serde_json::to_string(&metricas).unwrap();
    assert!(!telemetria.contains(KEY_FALSA));
}

#[tokio::test]
async fn test_max_tokens_nunca_llega_como_respuesta() {
    let mut eventos = vec![
        inicio(1, 0, 0),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"El valor es 12"}}),
        json!({"type":"content_block_stop","index":0}),
    ];
    eventos.extend(fin("max_tokens", 32000));
    let servidor = servidor_con(&[sse(&eventos)]).await;
    let error = runner(&servidor.uri())
        .ejecutar_turno(
            &ClaudeModel::sonnet(),
            SISTEMA,
            &[AgentMessage::user("x")],
            &[],
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("max_tokens"), "{error}");
}

#[test]
fn test_modelos_de_la_cli_se_traducen_al_id_de_la_api() {
    for (alias, id) in [
        ("sonnet", "claude-sonnet-5-5"),
        ("default", "claude-sonnet-5-5"),
        ("haiku", "claude-haiku-5-5"),
        ("claude-sonnet-5", "claude-sonnet-5"),
    ] {
        assert_eq!(
            AnthropicMessagesRunner::modelo_api(&ClaudeModel::new(alias).unwrap()),
            id
        );
    }
}

/// Servidor HTTP mínimo que envía el flujo en trozos con pausas y anota si el cliente cerró
async fn servidor_lento(trozos: Vec<String>, pausa: Duration) -> (String, Arc<Mutex<bool>>) {
    let oyente = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let direccion = format!("http://{}", oyente.local_addr().unwrap());
    let cerrado = Arc::new(Mutex::new(false));
    let marca = Arc::clone(&cerrado);
    tokio::spawn(async move {
        let (mut socket, _) = oyente.accept().await.unwrap();
        let mut peticion = vec![0u8; 64 * 1024];
        let _ = socket.read(&mut peticion).await;
        let cabecera =
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
        if socket.write_all(cabecera.as_bytes()).await.is_err() {
            return;
        }
        for trozo in trozos {
            if socket.write_all(trozo.as_bytes()).await.is_err() || socket.flush().await.is_err() {
                *marca.lock().unwrap() = true;
                return;
            }
            tokio::time::sleep(pausa).await;
        }
    });
    (direccion, cerrado)
}

#[tokio::test]
async fn test_abortar_suelta_el_flujo_y_no_emite_mas_texto() {
    let mut trozos = vec![
        sse(&[inicio(1, 0, 0)]),
        sse(&[
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        ]),
    ];
    for i in 0..40 {
        trozos.push(sse(&[json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":format!("f{i} ")}})]));
    }
    let (url, cerrado) = servidor_lento(trozos, Duration::from_millis(40)).await;
    let runner = runner(&url);
    let vistos = Arc::new(Mutex::new(Vec::<String>::new()));
    let destino = Arc::clone(&vistos);
    let detener = Arc::new(tokio::sync::Notify::new());
    let aviso = Arc::clone(&detener);
    let observador: ObservadorTexto = Arc::new(move |f: &str| {
        destino.lock().unwrap().push(f.into());
        aviso.notify_one();
    });
    let historial = [AgentMessage::user("¿Qué es?")];
    let herramientas = herramientas();
    let modelo = ClaudeModel::sonnet();
    // Como `chat_service`: al detener, `select!` suelta el futuro del turno
    let resultado = tokio::select! {
        r = runner.ejecutar_turno_en_vivo(&modelo, SISTEMA, &historial, &herramientas, observador) => Some(r),
        _ = detener.notified() => None,
    };
    assert!(resultado.is_none(), "el turno se detuvo");
    let al_detener = vistos.lock().unwrap().len();
    assert_eq!(al_detener, 1);
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(
        vistos.lock().unwrap().len(),
        al_detener,
        "tras abortar no sale ningún fragmento"
    );
    assert!(*cerrado.lock().unwrap(), "el socket se cerró");
}

/// Servidor de tools mínimo: devuelve siempre la misma salida
struct ToolsFijas;

#[async_trait::async_trait]
impl lakehouse_rag::application::ports::tool_server::ToolServerPort for ToolsFijas {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        herramientas()
    }

    async fn ejecutar_herramienta(
        &self,
        call: &ToolCall,
    ) -> Result<
        lakehouse_rag::application::dtos::tools::ToolResult,
        lakehouse_rag::domain::errors::DomainError,
    > {
        Ok(lakehouse_rag::application::dtos::tools::ToolResult {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            output: json!({ "texto": "La presión de la línea L-999 es 125 bar." }),
            is_error: false,
        })
    }
}

async fn turno_completo(
    url: &str,
    pregunta: &str,
) -> (
    String,
    Vec<lakehouse_rag::application::dtos::agent::AgentExecutionEvent>,
) {
    use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
    use lakehouse_rag::domain::models::center::CenterId;
    let orquestador = AgentOrchestrator::new(Arc::new(runner(url)), Arc::new(ToolsFijas));
    let eventos = Arc::new(Mutex::new(Vec::new()));
    let destino = Arc::clone(&eventos);
    let mut historial = vec![AgentMessage::user(pregunta)];
    let respuesta = orquestador
        .procesar_turno(
            &ClaudeModel::sonnet(),
            &CenterId::new("centro_prueba").unwrap(),
            &mut historial,
            Some(Arc::new(move |e| destino.lock().unwrap().push(e))),
        )
        .await
        .unwrap();
    let eventos = eventos.lock().unwrap().clone();
    (respuesta, eventos)
}

#[tokio::test]
async fn test_texto_en_vivo_rechazado_en_message_stop_se_descarta() {
    use lakehouse_rag::application::dtos::agent::AgentExecutionEvent;
    let crudo = r#"Busco. {"tool_calls": [{"id": "c"#;
    let servidor = servidor_con(&[
        flujo_de_texto(&["Busco. ", &crudo["Busco. ".len()..]]),
        flujo_de_texto(&["Es un cordón de refuerzo."]),
    ])
    .await;
    let (respuesta, eventos) =
        turno_completo(&servidor.uri(), "¿Qué es el refuerzo de soldadura?").await;
    assert_eq!(respuesta, "Es un cordón de refuerzo.");
    let primero_en_vivo = eventos
        .iter()
        .position(|e| matches!(e, AgentExecutionEvent::TextDelta(t) if t == "Busco. "))
        .expect("el texto salió en vivo");
    let descartado = eventos
        .iter()
        .position(|e| *e == AgentExecutionEvent::TextoEnVivoDescartado)
        .expect("se descarta lo emitido");
    assert!(primero_en_vivo < descartado);
}

#[tokio::test]
async fn test_omision_corregida_de_punta_a_punta_por_la_api() {
    let servidor = servidor_con(&[
        flujo_de_texto(&["La presión de diseño es 16 bar."]),
        flujo_de_tools(
            ["toolu_a", "toolu_b"],
            ["search_normative_context", "search_normative_context"],
        ),
        flujo_de_texto(&["La presión es 125 bar."]),
    ])
    .await;
    let (respuesta, eventos) =
        turno_completo(&servidor.uri(), "¿Qué presión tiene la línea L-999?").await;
    assert_eq!(respuesta, "La presión es 125 bar.");
    let omisiones = eventos
        .iter()
        .filter(|e| {
            matches!(
                e,
                lakehouse_rag::application::dtos::agent::AgentExecutionEvent::ArnesIntervino {
                    etapa:
                        lakehouse_rag::application::dtos::agent::EtapaArnes::OmisionDeHerramienta,
                    ..
                }
            )
        })
        .count();
    assert_eq!(omisiones, 1);
    let cuerpos = cuerpos_recibidos(&servidor).await;
    let segunda = cuerpos[1]["messages"].as_array().unwrap();
    assert_eq!(
        segunda.len(),
        1,
        "la corrección va en el mensaje del operador"
    );
    let bloques = segunda[0]["content"].as_array().unwrap();
    assert!(
        bloques[1]["text"]
            .as_str()
            .unwrap()
            .contains("RECORDATORIO")
    );
    // La corrección sigue en su sitio en el paso siguiente (historial de solo añadir)
    let tercera = cuerpos[2]["messages"].as_array().unwrap();
    assert_eq!(tercera[0]["content"][1]["text"], bloques[1]["text"]);
}
