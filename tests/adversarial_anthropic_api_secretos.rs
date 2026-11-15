//! La API key nunca sale por `tracing`, ni por los errores, ni por `Debug`, aunque el servidor la
//! repita en sus respuestas. Binario propio: el suscriptor de logs no compite con otros tests.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lakehouse_rag::infrastructure::llm::anthropic_api::mensajes::{
    MensajeEntrada, PeticionMensajes,
};
use lakehouse_rag::infrastructure::llm::anthropic_api::{
    AnthropicHttpClient, ConfigClienteAnthropic, PoliticaReintentos, SecretApiKey,
};
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_FALSA: &str = "sk-ant-test-NO-ES-REAL-7f3a9c";

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

fn error_api(tipo: &str, mensaje: &str) -> serde_json::Value {
    json!({ "type": "error", "error": { "type": tipo, "message": mensaje } })
}

#[tokio::test]
async fn test_la_key_no_aparece_en_logs_ni_en_errores() {
    let logs = LogsCapturados::default();
    let escritor = logs.clone();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || escritor.clone())
        .init();

    let servidor = MockServer::start().await;
    // Un servidor hostil que repite la key en sus errores
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(529)
                .set_body_json(error_api("overloaded_error", &format!("eco {KEY_FALSA}"))),
        )
        .up_to_n_times(2)
        .mount(&servidor)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(error_api(
            "invalid_request_error",
            &format!("eco {KEY_FALSA}"),
        )))
        .mount(&servidor)
        .await;
    let key = SecretApiKey::new(KEY_FALSA).unwrap();
    let config = ConfigClienteAnthropic {
        reintentos: PoliticaReintentos {
            espera_base: Duration::from_millis(1),
            espera_maxima: Duration::from_millis(2),
            ..PoliticaReintentos::default()
        },
        ..ConfigClienteAnthropic::procedencia().con_base_url(servidor.uri())
    };
    let cliente = AnthropicHttpClient::new(key.clone(), config.clone()).unwrap();
    let peticion = PeticionMensajes {
        model: "claude-haiku-5-5".into(),
        max_tokens: 16,
        system: Vec::new(),
        messages: vec![MensajeEntrada::usuario("hola")],
        tools: Vec::new(),
        tool_choice: None,
    };
    let error = cliente.crear_mensaje(&peticion).await.unwrap_err();

    let texto_logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(
        texto_logs.contains("reintento de transporte"),
        "se esperaban logs: {texto_logs}"
    );
    assert!(
        texto_logs.contains("[REDACTED]"),
        "el eco del servidor debe salir enmascarado"
    );
    for texto in [
        texto_logs,
        format!("{error}"),
        format!("{error:?}"),
        format!("{key:?}"),
        format!("{key}"),
        format!("{config:?}"),
    ] {
        assert!(!texto.contains(KEY_FALSA), "fuga de la key en: {texto}");
    }
}
