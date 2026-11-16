//! Fase 5.2: la API key no llega al WebView (carga útil de los comandos), ni a los logs, ni a la
//! telemetría del runner del agente, aunque el servidor la repita en sus errores. Binario propio:
//! el suscriptor de `tracing` no compite con otros tests.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lakehouse_rag::application::dtos::agent::{AgentMessage, ClaudeModel};
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::infrastructure::llm::anthropic_api::{
    AlmacenEnMemoria, AnthropicHttpClient, AnthropicMessagesRunner, ConfigClienteAnthropic,
    PoliticaReintentos, SecretApiKey,
};
use lakehouse_rag::infrastructure::llm::modo_llm::ModoLlm;
use lakehouse_rag::presentation::gui::api_key::{estado_api_key_en, guardar_api_key_en};
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_FALSA: &str = "sk-ant-api03-SECRETO-NO-REAL-q7w8e9r0";
const NUCLEO: &str = "SECRETO-NO-REAL";

#[derive(Clone, Default)]
struct LogsCapturados(Arc<Mutex<Vec<u8>>>);

impl Write for LogsCapturados {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn test_la_key_no_sale_por_el_webview_los_logs_ni_la_telemetria() {
    let logs = LogsCapturados::default();
    let escritor = logs.clone();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || escritor.clone())
        .init();

    // 1. Comandos de la ventana: lo que vuelve al WebView
    let almacen = AlmacenEnMemoria::default();
    let guardado = guardar_api_key_en(KEY_FALSA.into(), None, &almacen, ModoLlm::Api).unwrap();
    let estado = estado_api_key_en(None, &almacen, ModoLlm::Api);
    let error =
        guardar_api_key_en(format!("{KEY_FALSA} x"), None, &almacen, ModoLlm::Api).unwrap_err();
    for carga in [
        serde_json::to_string(&guardado).unwrap(),
        serde_json::to_string(&estado).unwrap(),
        error,
    ] {
        assert!(!carga.contains(NUCLEO), "{carga}");
    }
    assert!(estado.configurada);
    assert!(estado.mascara.unwrap().starts_with("sk-ant-…"));

    // 2. El runner del agente contra un servidor hostil que repite la key
    let servidor = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(529).set_body_json(json!({
            "type": "error", "error": {"type": "overloaded_error", "message": format!("eco {KEY_FALSA}")}
        })))
        .up_to_n_times(1)
        .mount(&servidor)
        .await;
    let flujo_hostil = format!(
        "event: message_start\ndata: {}\n\nevent: error\ndata: {}\n\n",
        json!({"type":"message_start","message":{"id":"m","model":"x","usage":{}}}),
        json!({"type":"error","error":{"type":"api_error","message":format!("eco {KEY_FALSA}")}})
    );
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(flujo_hostil, "text/event-stream"))
        .up_to_n_times(1)
        .mount(&servidor)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "type": "error", "error": {"type": "authentication_error", "message": format!("eco {KEY_FALSA}")}
        })))
        .mount(&servidor)
        .await;
    let config = ConfigClienteAnthropic {
        reintentos: PoliticaReintentos {
            espera_base: Duration::from_millis(1),
            espera_maxima: Duration::from_millis(2),
            ..PoliticaReintentos::default()
        },
        ..ConfigClienteAnthropic::agente().con_base_url(servidor.uri())
    };
    let runner = AnthropicMessagesRunner::new(
        AnthropicHttpClient::new(SecretApiKey::new(KEY_FALSA).unwrap(), config).unwrap(),
    );
    let error = runner
        .ejecutar_turno(
            &ClaudeModel::sonnet(),
            "sistema",
            &[AgentMessage::user("hola")],
            &[],
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.contains(NUCLEO), "{error}");
    let telemetria = serde_json::to_string(&runner.tomar_metricas()).unwrap();
    assert!(!telemetria.contains(NUCLEO));

    // 3. Nada de lo anterior dejó la key en los logs (sí la máscara)
    let texto_logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(!texto_logs.contains(NUCLEO), "{texto_logs}");
    assert!(
        texto_logs.contains("reintento"),
        "se registraron los reintentos"
    );
}
