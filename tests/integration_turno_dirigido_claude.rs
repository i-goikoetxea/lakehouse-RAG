//! Turno dirigido del arnés: `ClaudeAgentRunner::ejecutar_turno_dirigido` sobre un `claude` falso.
//! Los sobres reproducen los dos desenlaces observados en el spike de la Fase 0
//! (`docs/arquitectura/ssd_arnes_fase0_spike.md` §3).

mod comun_claude_falso;

use std::time::Duration;

use comun_claude_falso::ClaudeFalso;
use lakehouse_rag::application::dtos::agent::{AgentMessage, ClaudeModel, TurnoDirigido};
use lakehouse_rag::application::dtos::tools::{LakehouseToolName, ToolDefinition};
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::infrastructure::llm::sesion_claude::MENSAJE_SESION_CLAUDE;
use lakehouse_rag::infrastructure::llm::{ClaudeAgentConfig, ClaudeAgentRunner};
use serde_json::{Value, json};

/// Guarda argumentos y entrada, emite `sobre.json` y termina con el código de `codigo.txt`
const FUENTE: &str = r##"
use std::io::Read;
fn main() {
    let mut entrada = String::new();
    let _ = std::io::stdin().read_to_string(&mut entrada);
    let dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::fs::write(dir.join("args.txt"), args.join("\u{1f}")).unwrap();
    std::fs::write(dir.join("entrada.txt"), entrada).unwrap();
    print!("{}", std::fs::read_to_string(dir.join("sobre.json")).unwrap());
    let codigo = std::fs::read_to_string(dir.join("codigo.txt")).ok().and_then(|c| c.trim().parse().ok()).unwrap_or(0);
    std::process::exit(codigo);
}
"##;

const LOOKUP: LakehouseToolName = LakehouseToolName::LookupDoubleEntryTable;

async fn dirigir(falso: &ClaudeFalso) -> Result<TurnoDirigido, DomainError> {
    let runner = ClaudeAgentRunner::new(ClaudeAgentConfig {
        binary_path: falso.binario.clone(),
        timeout_duration: Duration::from_secs(30),
        extra_args: Vec::new(),
    });
    runner
        .ejecutar_turno_dirigido(
            &ClaudeModel::new("haiku").unwrap(),
            "Eres el asistente",
            &[AgentMessage::user("Tabla 4-A, 30 mm y CE 0,42 %")],
            &ToolDefinition::lakehouse_tools(),
            LOOKUP,
        )
        .await
}

fn falso_con_sobre(sobre: &Value) -> ClaudeFalso {
    ClaudeFalso::compilar(FUENTE, &[("sobre.json", &sobre.to_string())])
}

fn sobre_exito(structured_output: Option<Value>, result: &str) -> Value {
    let mut sobre = json!({
        "type": "result", "subtype": "success", "is_error": false, "result": result,
    });
    if let Some(salida) = structured_output {
        sobre["structured_output"] = salida;
    }
    sobre
}

fn leer_junto_al_binario(falso: &ClaudeFalso, nombre: &str) -> String {
    std::fs::read_to_string(falso.binario.parent().unwrap().join(nombre)).unwrap()
}

#[tokio::test]
async fn test_argumentos_piden_json_y_esquema_que_fija_nombre_y_argumentos_de_la_tool() {
    let falso = falso_con_sobre(&sobre_exito(None, ""));
    dirigir(&falso).await.unwrap();

    let crudos = leer_junto_al_binario(&falso, "args.txt");
    let args: Vec<&str> = crudos.split('\u{1f}').collect();
    let valor_de = |flag: &str| args[args.iter().position(|a| *a == flag).expect(flag) + 1];
    assert_eq!(valor_de("--output-format"), "json");
    assert_eq!(valor_de("--model"), "haiku");
    assert_eq!(valor_de("--tools"), "");
    assert!(args.contains(&"--no-session-persistence"));

    let esquema: Value = serde_json::from_str(valor_de("--json-schema")).unwrap();
    let definicion = ToolDefinition::lakehouse_tools()
        .into_iter()
        .find(|t| t.name == LOOKUP.as_str())
        .unwrap();
    assert_eq!(esquema["properties"]["name"]["const"], LOOKUP.as_str());
    assert_eq!(esquema["properties"]["arguments"], definicion.parameters);
    assert_eq!(esquema["required"], json!(["name", "arguments"]));
    assert_eq!(esquema["additionalProperties"], false);
}

#[tokio::test]
async fn test_la_entrada_lleva_el_historial_y_el_catalogo_como_un_turno_normal() {
    let falso = falso_con_sobre(&sobre_exito(None, ""));
    dirigir(&falso).await.unwrap();
    let entrada = leer_junto_al_binario(&falso, "entrada.txt");
    assert!(entrada.contains("Tabla 4-A, 30 mm y CE 0,42 %"));
    for herramienta in LakehouseToolName::TODAS {
        assert!(entrada.contains(herramienta.as_str()));
    }
}

#[tokio::test]
async fn test_structured_output_valido_es_una_llamada_a_la_tool() {
    let argumentos = json!({"centro_id": "c", "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42});
    let falso = falso_con_sobre(&sobre_exito(
        Some(json!({"name": LOOKUP.as_str(), "arguments": argumentos})),
        "ignorado",
    ));
    match dirigir(&falso).await.unwrap() {
        TurnoDirigido::Llamada(llamada) => {
            assert_eq!(llamada.name, LOOKUP.as_str());
            assert_eq!(llamada.arguments, argumentos);
            assert!(!llamada.id.is_empty());
        }
        otro => panic!("se esperaba una llamada: {otro:?}"),
    }
}

#[tokio::test]
async fn test_sin_structured_output_no_hay_llamada_aunque_el_sobre_indique_exito() {
    // El texto de `result` trae una llamada a otra tool: el spike vio este desenlace con Haiku.
    let falso = falso_con_sobre(&sobre_exito(
        None,
        r#"{"tool_calls":[{"name":"search_normative_context","arguments":{}}]}"#,
    ));
    assert_eq!(dirigir(&falso).await.unwrap(), TurnoDirigido::SinLlamada);
}

#[tokio::test]
async fn test_is_error_con_salida_cero_es_un_error_y_no_una_falta_de_llamada() {
    let falso = falso_con_sobre(&json!({
        "type": "result", "subtype": "error_during_execution", "is_error": true, "result": "fallo interno",
    }));
    let error = dirigir(&falso).await.unwrap_err();
    assert!(error.to_string().contains("fallo interno"), "{error}");
}

#[tokio::test]
async fn test_sesion_caducada_en_el_sobre_da_el_mensaje_accionable() {
    let falso = falso_con_sobre(&json!({
        "type": "result", "is_error": true,
        "result": "Failed to authenticate: OAuth session expired and could not be refreshed",
    }));
    let error = dirigir(&falso).await.unwrap_err();
    assert!(error.to_string().contains(MENSAJE_SESION_CLAUDE), "{error}");
}

#[tokio::test]
async fn test_codigo_de_salida_distinto_de_cero_es_un_error() {
    let falso = ClaudeFalso::compilar(FUENTE, &[("sobre.json", "{}"), ("codigo.txt", "3")]);
    assert!(dirigir(&falso).await.is_err());
}

#[tokio::test]
async fn test_salida_que_no_es_json_es_un_error_y_no_una_falta_de_llamada() {
    let falso = ClaudeFalso::compilar(
        FUENTE,
        &[("sobre.json", "formato inesperado de una versión futura")],
    );
    assert!(dirigir(&falso).await.is_err());
}

#[tokio::test]
async fn test_herramienta_fuera_del_catalogo_es_un_error() {
    let falso = falso_con_sobre(&sobre_exito(None, ""));
    let runner = ClaudeAgentRunner::new(ClaudeAgentConfig {
        binary_path: falso.binario.clone(),
        timeout_duration: Duration::from_secs(30),
        extra_args: Vec::new(),
    });
    let resultado = runner
        .ejecutar_turno_dirigido(&ClaudeModel::sonnet(), "s", &[], &[], LOOKUP)
        .await;
    assert!(resultado.is_err());
}
