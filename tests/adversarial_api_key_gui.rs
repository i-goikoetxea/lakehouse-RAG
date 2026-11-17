//! Fase 5.2: la API key no llega al WebView (carga útil de los comandos), ni a los logs, ni a la
//! telemetría del runner del agente, aunque el servidor la repita en sus errores. Binario propio:
//! el suscriptor de `tracing` no compite con otros tests.
//!
//! Modal de configuración: la key tampoco llega al fichero de preferencias del modo
//! (`llm_config.json`), ni aunque se pegue en el selector de modo o el fichero venga envenenado.
//!
//! Motor FreeLLMAPI: su key (`freellmapi-…`) tampoco sale por el estado, los errores del proxy,
//! el catálogo de modelos, la URL pegada por error ni los logs.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lakehouse_rag::application::dtos::agent::{AgentMessage, ClaudeModel};
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::application::ports::model_catalog::ModelCatalogPort;
use lakehouse_rag::infrastructure::llm::anthropic_api::{
    AlmacenEnMemoria, AlmacenSecreto, AnthropicHttpClient, AnthropicMessagesRunner,
    ConfigClienteAnthropic, PerfilApi, PoliticaReintentos, SecretApiKey,
};
use lakehouse_rag::infrastructure::llm::freellmapi_catalogo::FreeLlmApiModelCatalog;
use lakehouse_rag::infrastructure::llm::modo_llm::ModoLlm;
use lakehouse_rag::infrastructure::llm::preferencia_modo::{
    ARCHIVO_PREFERENCIAS_LLM, ModoResuelto, OrigenModo, leer_modo_guardado, leer_preferencias,
};
use lakehouse_rag::presentation::gui::api_key::{estado_api_key_en, guardar_api_key_en};
use lakehouse_rag::presentation::gui::config_llm::{
    KeysConfiguradas, MotorLlmSesion, estado_modo_llm_desde_disco, guardar_modo_llm_en,
    guardar_url_freellmapi_en,
};
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY_FALSA: &str = "sk-ant-api03-SECRETO-NO-REAL-q7w8e9r0";
const NUCLEO: &str = "SECRETO-NO-REAL";
const KEY_PROXY_FALSA: &str = "freellmapi-PROXYSECRETO-NO-REAL-k4j5";
const NUCLEO_PROXY: &str = "PROXYSECRETO";

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

    // 3. El flujo del modal: guardar la key y el modo deja el fichero de configuración sin key,
    // aunque la key se pegue en el selector de modo o el fichero traiga una de antes
    let dir = tempfile::tempdir().unwrap();
    let sesion = MotorLlmSesion {
        directorio_config: dir.path().to_path_buf(),
        pedido_al_arrancar: ModoResuelto {
            modo: ModoLlm::Cli,
            origen: OrigenModo::PorDefecto,
        },
        en_uso: ModoLlm::Cli,
        freellmapi_url: "http://127.0.0.1:31415".into(),
    };
    std::fs::write(
        dir.path().join(ARCHIVO_PREFERENCIAS_LLM),
        json!({"modo_llm": "api", "api_key": KEY_FALSA}).to_string(),
    )
    .unwrap();
    assert_eq!(leer_modo_guardado(dir.path()), Some(ModoLlm::Api));
    let almacen_modal = AlmacenEnMemoria::default();
    let key_guardada = KeysConfiguradas {
        anthropic: guardar_api_key_en(KEY_FALSA.into(), None, &almacen_modal, ModoLlm::Cli)
            .unwrap()
            .configurada,
        freellmapi: false,
    };
    let pegada_en_el_modo =
        guardar_modo_llm_en(&sesion, KEY_FALSA, None, key_guardada).unwrap_err();
    assert!(!pegada_en_el_modo.contains(NUCLEO), "{pegada_en_el_modo}");
    let elegido = guardar_modo_llm_en(&sesion, "api", None, key_guardada).unwrap();
    let estado_modo = estado_modo_llm_desde_disco(&sesion, Some(KEY_FALSA), key_guardada);
    for carga in [
        serde_json::to_string(&elegido).unwrap(),
        serde_json::to_string(&estado_modo).unwrap(),
    ] {
        assert!(!carga.contains(NUCLEO), "{carga}");
    }
    assert_eq!(elegido.al_reiniciar, "api");
    for entrada in std::fs::read_dir(dir.path()).unwrap() {
        let ruta = entrada.unwrap().path();
        let contenido = std::fs::read_to_string(&ruta).unwrap();
        assert!(
            !contenido.contains(NUCLEO),
            "{}: {contenido}",
            ruta.display()
        );
        assert!(
            !contenido.contains("api_key"),
            "{}: {contenido}",
            ruta.display()
        );
    }
    std::fs::write(
        dir.path().join(ARCHIVO_PREFERENCIAS_LLM),
        "x".repeat(64 * 1024),
    )
    .unwrap();
    assert_eq!(
        leer_modo_guardado(dir.path()),
        None,
        "un fichero desmesurado se ignora"
    );

    // 4. FreeLLMAPI: la key del proxy no vuelve al WebView, ni por los errores del proxy (que la
    // repiten), ni por el catálogo, ni pegada en el campo de la URL
    let almacen_proxy = AlmacenEnMemoria::default();
    let estado_proxy = guardar_api_key_en(
        KEY_PROXY_FALSA.into(),
        None,
        &almacen_proxy,
        ModoLlm::FreeLlmApi,
    )
    .unwrap();
    assert_eq!(estado_proxy.mascara.as_deref(), Some("freellmapi-…k4j5"));
    assert!(
        almacen_modal
            .leer()
            .unwrap()
            .is_some_and(|k| k.mascara_con_prefijo().starts_with("sk-ant-")),
        "cada key en su almacén"
    );
    let proxy = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "type": "error",
            "error": {"type": "invalid_request_error", "message": format!("clave {KEY_PROXY_FALSA} rechazada")}
        })))
        .mount(&proxy)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500).set_body_string(format!("eco {KEY_PROXY_FALSA}")))
        .mount(&proxy)
        .await;
    let config_proxy = ConfigClienteAnthropic::agente()
        .con_perfil(PerfilApi::FREELLMAPI)
        .con_base_url(proxy.uri());
    let cliente_proxy = || {
        AnthropicHttpClient::new(
            SecretApiKey::new(KEY_PROXY_FALSA).unwrap(),
            config_proxy.clone(),
        )
        .unwrap()
    };
    let runner_proxy = AnthropicMessagesRunner::new(cliente_proxy());
    let error_proxy = runner_proxy
        .ejecutar_turno(
            &ClaudeModel::new("auto").unwrap(),
            "sistema",
            &[AgentMessage::user("hola")],
            &[],
        )
        .await
        .unwrap_err()
        .to_string();
    let error_catalogo = FreeLlmApiModelCatalog::new(cliente_proxy(), proxy.uri())
        .listar_modelos()
        .await
        .unwrap_err()
        .to_string();
    let recibidas = proxy.received_requests().await.unwrap();
    assert!(
        recibidas
            .iter()
            .all(|r| r.headers.get("x-api-key").unwrap() == KEY_PROXY_FALSA),
        "al proxy solo va su key, nunca la de Anthropic"
    );
    let pegada_en_la_url =
        guardar_url_freellmapi_en(&sesion, KEY_PROXY_FALSA, None, key_guardada).unwrap_err();
    let elegido_proxy = guardar_modo_llm_en(
        &sesion,
        "freellmapi",
        None,
        KeysConfiguradas {
            anthropic: true,
            freellmapi: true,
        },
    )
    .unwrap();
    for carga in [
        serde_json::to_string(&estado_proxy).unwrap(),
        error_proxy,
        error_catalogo,
        pegada_en_la_url,
        serde_json::to_string(&elegido_proxy).unwrap(),
        serde_json::to_string(&runner_proxy.tomar_metricas()).unwrap(),
    ] {
        assert!(!carga.contains(NUCLEO_PROXY), "{carga}");
    }
    assert_eq!(leer_preferencias(dir.path()).freellmapi_url, None);
    let fichero = std::fs::read_to_string(dir.path().join(ARCHIVO_PREFERENCIAS_LLM)).unwrap();
    assert!(!fichero.contains(NUCLEO_PROXY), "{fichero}");

    // 5. Nada de lo anterior dejó ninguna de las dos keys en los logs (sí la máscara)
    let texto_logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(!texto_logs.contains(NUCLEO), "{texto_logs}");
    assert!(!texto_logs.contains(NUCLEO_PROXY), "{texto_logs}");
    assert!(
        texto_logs.contains("reintento"),
        "se registraron los reintentos"
    );
}
