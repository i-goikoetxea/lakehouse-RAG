//! Fase 15: sesión de Claude Code ausente o caducada. Un `claude` falso (compilado al vuelo con
//! `rustc` en un directorio temporal) reproduce la salida REAL del CLI 2.1.302 sin sesión,
//! capturada con un `CLAUDE_CONFIG_DIR` vacío: así no se toca la sesión real del usuario.
//! El agente y el destilador deben traducirla a un mensaje claro y accionable, nunca a la salida
//! cruda ni al JSON del CLI. (El catálogo de modelos no necesita sesión: el CLI real lo sirve.)

mod comun_claude_falso;

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use comun_claude_falso::ClaudeFalso;
use lakehouse_rag::application::dtos::agent::{AgentMessage, ClaudeModel};
use lakehouse_rag::application::dtos::ingestion::ParsedDocument;
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::application::ports::llm_distiller::LlmDistillerPort;
use lakehouse_rag::infrastructure::llm::sesion_claude::MENSAJE_SESION_CLAUDE;
use lakehouse_rag::infrastructure::llm::{
    ClaudeAgentConfig, ClaudeAgentRunner, ClaudeHeadlessConfig, ClaudeHeadlessDistiller,
};

/// Fuente del `claude` falso: salida real del CLI sin sesión (texto o sobre JSON), código 1
const FUENTE_CLAUDE_FALSO: &str = r##"
use std::io::Read;
fn main() {
    let mut entrada = String::new();
    let _ = std::io::stdin().read_to_string(&mut entrada);
    let json = std::env::args().any(|a| a == "json" || a == "stream-json");
    if json {
        println!("{}", r#"{"type":"result","subtype":"success","is_error":true,"terminal_reason":"api_error","num_turns":1,"result":"Not logged in · Please run /login","total_cost_usd":0}"#);
    } else {
        println!("Not logged in · Please run /login");
    }
    std::process::exit(1);
}
"##;

/// `claude` falso compilado una vez por proceso de test
fn claude_falso() -> &'static Path {
    static BINARIO: OnceLock<ClaudeFalso> = OnceLock::new();
    &BINARIO
        .get_or_init(|| ClaudeFalso::compilar(FUENTE_CLAUDE_FALSO, &[]))
        .binario
}

fn assert_mensaje_claro(error: &str) {
    assert!(error.contains(MENSAJE_SESION_CLAUDE), "mensaje: {error}");
    assert!(
        !error.contains("is_error") && !error.contains("terminal_reason"),
        "no debe mostrar el JSON del CLI: {error}"
    );
}

#[tokio::test]
async fn test_agente_sin_sesion_devuelve_mensaje_accionable() {
    let runner = ClaudeAgentRunner::new(ClaudeAgentConfig {
        binary_path: claude_falso().to_path_buf(),
        timeout_duration: Duration::from_secs(30),
        extra_args: Vec::new(),
    });
    let error = runner
        .ejecutar_turno(
            &ClaudeModel::sonnet(),
            "Eres el asistente",
            &[AgentMessage::user("¿Responsable de la custodia?")],
            &[],
        )
        .await
        .expect_err("sin sesión debe fallar");
    assert_mensaje_claro(&error.to_string());
}

#[tokio::test]
async fn test_destilador_sin_sesion_devuelve_mensaje_accionable_sin_json() {
    let destilador = ClaudeHeadlessDistiller::new(ClaudeHeadlessConfig {
        binary_path: claude_falso().to_string_lossy().to_string(),
        ..ClaudeHeadlessConfig::default()
    });
    let documento = ParsedDocument {
        nombre_archivo: "po_alm_03.md".into(),
        titulo: "PO-ALM-03".into(),
        secciones: Vec::new(),
        tablas: Vec::new(),
        contenido_completo: "# PO-ALM-03\nRecepción de tubería.".into(),
    };
    let error = destilador
        .distill(&documento)
        .await
        .expect_err("sin sesión debe fallar");
    assert_mensaje_claro(&error.to_string());
}

#[test]
fn test_sobre_json_sin_sesion_con_codigo_cero_tambien_se_traduce() {
    let sobre = r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#;
    let error = ClaudeHeadlessDistiller::extraer_paquete(sobre).expect_err("is_error");
    assert_mensaje_claro(&error.to_string());
}
