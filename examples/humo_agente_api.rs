//! Humo del agente por la API real (Pieza 5, Fase 5.2), con datos **sintéticos**: el orquestador
//! real, `AnthropicMessagesRunner` (Sonnet 5.5) y la procedencia con Haiku 5.5 sobre un servidor
//! de tools inventado. No toca centros. Gasta API (unas 15-25 llamadas): necesita
//! `ANTHROPIC_API_KEY`.
//!
//! ```text
//! cargo run --example humo_agente_api -- [salida.jsonl]
//! ```

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use lakehouse_rag::application::dtos::agent::{
    AgentExecutionEvent, AgentMessage, ClaudeModel, EtapaArnes,
};
use lakehouse_rag::application::dtos::tools::{ToolCall, ToolDefinition, ToolResult};
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::infrastructure::llm::anthropic_api::{
    AnthropicHttpClient, AnthropicMessagesRunner, AnthropicProcedenciaVerifier,
    ConfigClienteAnthropic, SecretApiKey,
};
use serde_json::{Value, json};

/// Documento inventado: ningún texto de documentos reales
fn salida_sintetica(llamada: &ToolCall) -> Value {
    match llamada.name.as_str() {
        "search_normative_context" | "stair_get_subtree_context" => json!({
            "resultados": [{
                "doc_id": "DOC-SINT-01", "revision": "2", "pagina": 3,
                "breadcrumb": "4. Pruebas > 4.2 Prueba hidrostática",
                "texto": "4.2 Prueba hidrostática. La línea L-999 se prueba a 16 bar durante 30 min. Test duration: 30 min at 16 bar."
            }]
        }),
        "lookup_double_entry_table" => json!({
            "estado": "resuelto", "tabla_id": "4-A", "doc_id": "DOC-SINT-01",
            "eje_x": "espesor (mm)", "valor_x": 30, "eje_y": "CE (%)", "valor_y": 0.42,
            "valor_resultado_texto": "125", "unidad": "°C", "pagina": 5
        }),
        "query_operational_matrix" => json!({
            "filas": [{
                "doc_id": "DOC-SINT-01", "fase": "Recepción", "registro": "REG-07",
                "rol_responsable": "Jefe de calidad", "origen_responsable": "DECLARADO", "pagina": 7
            }],
            "excluidos_no_declarados": 0
        }),
        "inspect_lakehouse_catalog" => json!({
            "documentos": [{"doc_id": "DOC-SINT-01", "titulo": "Procedimiento sintético de pruebas", "revision": "2", "vigente": true}],
            "tablas": [{"tabla_id": "4-A", "ejes": ["espesor (mm)", "CE (%)"], "unidad": "°C"}]
        }),
        _ => json!({ "estado": "sin_coincidencia" }),
    }
}

struct ToolsSinteticas;

#[async_trait]
impl ToolServerPort for ToolsSinteticas {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        ToolDefinition::lakehouse_tools()
    }

    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
        Ok(ToolResult {
            call_id: call.id.clone(),
            tool_name: call.name.clone(),
            output: salida_sintetica(call),
            is_error: false,
        })
    }
}

/// (pregunta, fragmentos que la respuesta correcta debe contener)
const PREGUNTAS: &[(&str, &[&str])] = &[
    ("¿Qué es una prueba hidrostática?", &[]),
    ("¿A qué presión se prueba la línea L-999?", &["16"]),
    (
        "Según la Tabla 4-A, ¿qué temperatura de precalentamiento corresponde a 30 mm y CE 0,42 %?",
        &["125"],
    ),
    (
        "¿Quién firma el registro REG-07 de recepción?",
        &["Jefe de calidad"],
    ),
    (
        "What is the test duration for line L-999 in DOC-SINT-01?",
        &["30"],
    ),
    (
        "¿Qué establece el apartado 4.2 del DOC-SINT-01?",
        &["16", "4.2"],
    ),
];

#[tokio::main]
async fn main() -> Result<(), String> {
    let salida = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "humo_agente_api.jsonl".into());
    let key = || SecretApiKey::desde_entorno().map_err(|e| e.to_string());
    let runner = Arc::new(AnthropicMessagesRunner::new(
        AnthropicHttpClient::new(key()?, ConfigClienteAnthropic::agente())
            .map_err(|e| e.to_string())?,
    ));
    let verificador = Arc::new(AnthropicProcedenciaVerifier::new(
        AnthropicHttpClient::new(key()?, ConfigClienteAnthropic::procedencia())
            .map_err(|e| e.to_string())?,
    ));
    let orquestador = AgentOrchestrator::new(runner.clone(), Arc::new(ToolsSinteticas))
        .with_verificador_procedencia(verificador);
    let centro = CenterId::new("centro_sintetico").map_err(|e| e.to_string())?;
    let mut archivo = std::fs::File::create(&salida).map_err(|e| e.to_string())?;
    let (mut correctas, mut coste_total) = (0, 0.0);

    for (pregunta, esperados) in PREGUNTAS {
        let eventos = Arc::new(Mutex::new(Vec::new()));
        let destino = Arc::clone(&eventos);
        let mut historial = vec![AgentMessage::user(*pregunta)];
        let inicio = Instant::now();
        let resultado = orquestador
            .procesar_turno(
                &ClaudeModel::sonnet(),
                &centro,
                &mut historial,
                Some(Arc::new(move |e| destino.lock().unwrap().push(e))),
            )
            .await;
        let latencia_ms = inicio.elapsed().as_millis() as u64;
        let pasos = runner.tomar_metricas();
        let eventos = eventos.lock().unwrap().clone();
        let etapas: Vec<EtapaArnes> = eventos
            .iter()
            .filter_map(|e| match e {
                AgentExecutionEvent::ArnesIntervino { etapa, .. } => Some(*etapa),
                _ => None,
            })
            .collect();
        let herramientas: Vec<String> = eventos
            .iter()
            .filter_map(|e| match e {
                AgentExecutionEvent::CallingTool { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        let respuesta = resultado.as_ref().map(String::as_str).unwrap_or_default();
        let correcta = resultado.is_ok() && esperados.iter().all(|f| respuesta.contains(f));
        correctas += usize::from(correcta);
        let coste: f64 = pasos.iter().filter_map(|p| p.coste_usd).sum();
        coste_total += coste;
        let linea = json!({
            "pregunta": pregunta,
            "entregada": resultado.is_ok(),
            "correcta_por_fragmentos": correcta,
            "error": resultado.as_ref().err().map(|e| e.to_string()),
            "respuesta": respuesta,
            "latencia_ms": latencia_ms,
            "herramientas": herramientas,
            "etapas_arnes": etapas,
            "pasos_api": pasos,
            "coste_usd_agente": coste,
        });
        writeln!(archivo, "{linea}").map_err(|e| e.to_string())?;
        println!(
            "{} | {} | {latencia_ms} ms | {} pasos | {coste:.4} USD | tools {herramientas:?} | arnés {etapas:?}",
            if correcta { "OK " } else { "MAL" },
            pregunta,
            pasos.len(),
        );
    }
    println!(
        "{correctas}/{} correctas por fragmentos; coste del agente {coste_total:.4} USD (sin la procedencia)",
        PREGUNTAS.len()
    );
    Ok(())
}
