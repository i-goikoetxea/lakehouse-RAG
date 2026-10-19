//! Streaming del agente (Fase 15, rediseño del chat): `ClaudeAgentRunner::ejecutar_turno_en_vivo`
//! sobre un `claude` falso que reproduce, con pausas, la salida REAL de
//! `claude -p --output-format stream-json --include-partial-messages`
//! (`tests/fixtures/claude_stream_json/`).

mod comun_claude_falso;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use comun_claude_falso::ClaudeFalso;
use lakehouse_rag::application::dtos::agent::{AgentMessage, AgentTurnOutput, ClaudeModel};
use lakehouse_rag::application::ports::agent_runner::{AgentRunnerPort, ObservadorTexto};
use lakehouse_rag::infrastructure::llm::sesion_claude::MENSAJE_SESION_CLAUDE;
use lakehouse_rag::infrastructure::llm::{ClaudeAgentConfig, ClaudeAgentRunner};

const TEXTO: &str = include_str!("fixtures/claude_stream_json/texto.jsonl");
const HERRAMIENTA: &str = include_str!("fixtures/claude_stream_json/herramienta.jsonl");
const SIN_SESION: &str = r#"{"type":"system","subtype":"init"}
{"type":"result","subtype":"success","is_error":true,"result":"Failed to authenticate: OAuth session expired and could not be refreshed"}
"#;

/// Emite `stream.jsonl` (junto al ejecutable) línea a línea con 120 ms entre líneas, solo si
/// se pidió `stream-json`; termina con el código de `codigo.txt` (0 si no existe)
const FUENTE: &str = r##"
use std::io::{Read, Write};
fn main() {
    let mut entrada = String::new();
    let _ = std::io::stdin().read_to_string(&mut entrada);
    let dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "stream-json") || !args.iter().any(|a| a == "--include-partial-messages") {
        println!("faltan los argumentos de streaming");
        std::process::exit(9);
    }
    let stream = std::fs::read_to_string(dir.join("stream.jsonl")).unwrap();
    let mut salida = std::io::stdout();
    for linea in stream.lines() {
        writeln!(salida, "{linea}").unwrap();
        salida.flush().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    let codigo = std::fs::read_to_string(dir.join("codigo.txt")).ok().and_then(|c| c.trim().parse().ok()).unwrap_or(0);
    std::process::exit(codigo);
}
"##;

fn runner(falso: &ClaudeFalso) -> ClaudeAgentRunner {
    ClaudeAgentRunner::new(ClaudeAgentConfig {
        binary_path: falso.binario.clone(),
        timeout_duration: Duration::from_secs(30),
        extra_args: Vec::new(),
    })
}

/// Ejecuta un turno en vivo y devuelve (salida, fragmentos con su instante, duración total)
async fn turno(
    falso: &ClaudeFalso,
) -> (
    Result<AgentTurnOutput, lakehouse_rag::domain::errors::DomainError>,
    Vec<(String, Duration)>,
    Duration,
) {
    let inicio = Instant::now();
    let fragmentos: Arc<Mutex<Vec<(String, Duration)>>> = Arc::default();
    let destino = fragmentos.clone();
    let observador: ObservadorTexto = Arc::new(move |f: &str| {
        destino
            .lock()
            .unwrap()
            .push((f.to_string(), inicio.elapsed()))
    });
    let salida = runner(falso)
        .ejecutar_turno_en_vivo(
            &ClaudeModel::new("haiku").unwrap(),
            "Eres el asistente",
            &[AgentMessage::user("¿Qué es un HA-30?")],
            &[],
            observador,
        )
        .await;
    let total = inicio.elapsed();
    let fragmentos = fragmentos.lock().unwrap().clone();
    (salida, fragmentos, total)
}

#[tokio::test]
async fn test_la_respuesta_llega_fragmento_a_fragmento_antes_de_terminar_el_proceso() {
    let falso = ClaudeFalso::compilar(FUENTE, &[("stream.jsonl", TEXTO)]);
    let (salida, fragmentos, total) = turno(&falso).await;
    let esperado = "El HA-30 es un hormigón en masa o armado de res";
    assert_eq!(
        salida.unwrap(),
        AgentTurnOutput::FinalMessage(esperado.to_string())
    );
    assert!(fragmentos.len() >= 6, "{fragmentos:?}");
    let concatenado: String = fragmentos.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(concatenado, esperado);
    let primero = fragmentos[0].1;
    assert!(
        total - primero >= Duration::from_millis(500),
        "el primer fragmento llegó a {primero:?} de {total:?}: no es streaming"
    );
}

#[tokio::test]
async fn test_una_peticion_de_herramienta_no_emite_texto_y_se_interpreta_como_tool_calls() {
    let falso = ClaudeFalso::compilar(FUENTE, &[("stream.jsonl", HERRAMIENTA)]);
    let (salida, fragmentos, _) = turno(&falso).await;
    assert!(fragmentos.is_empty(), "{fragmentos:?}");
    match salida.unwrap() {
        AgentTurnOutput::ToolCalls(llamadas) => {
            assert_eq!(llamadas[0].name, "query_operational_matrix");
            assert_eq!(llamadas[0].arguments["fase"], "CUSTODIA");
        }
        otra => panic!("se esperaba ToolCalls: {otra:?}"),
    }
}

#[tokio::test]
async fn test_sesion_caducada_en_streaming_da_el_mensaje_accionable() {
    let falso = ClaudeFalso::compilar(FUENTE, &[("stream.jsonl", SIN_SESION), ("codigo.txt", "1")]);
    let (salida, fragmentos, _) = turno(&falso).await;
    assert!(fragmentos.is_empty());
    let error = salida.expect_err("sin sesión").to_string();
    assert!(error.contains(MENSAJE_SESION_CLAUDE), "{error}");
}
