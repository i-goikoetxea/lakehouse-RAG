//! A/B del agente, API frente a CLI (Pieza 5, Fase 5.2). Pregunta sobre un centro **ya ingerido y
//! activo**, sin crear, activar ni reingerir nada, y escribe una línea JSONL por pregunta y
//! repetición con la respuesta y sus trazas.
//!
//! Con `--modo api` gasta API: necesita `ANTHROPIC_API_KEY` (o la key en el almacén del sistema).
//! Agente con `claude-sonnet-5-5` y procedencia con Haiku 5.5, los dos por la API. Con `--modo cli`
//! usa `claude -p` en los dos casos, como la app por defecto.
//!
//! ```text
//! cargo run --release --example ab_agente -- --centro <id> --preguntas <entrada.jsonl> \
//!     --modo api|cli --salida <salida.jsonl> [--modelo sonnet] [--repeticiones 1]
//! ```
//!
//! - **Entrada:** una línea `{"id": "...", "pregunta": "..."}` por pregunta. Los demás campos se
//!   ignoran y no se copian a la salida.
//! - **Salida:** los campos de `benchmark_crudas` (`pregunta`, `respuesta`, `herramientas` con
//!   `nombre`, `argumentos`, `resultado` y `es_error`), así que se puede repetir con
//!   `tests/repeticion_arnes_guardadas.rs`. Además, por línea:
//!   - `entregada`, `error`, `salida_segura`, `latencia_ms`, `arnes` y `omisiones_de_herramienta`;
//!   - en modo API, `pasos_api` (`ttft_ms`, tokens y coste por paso) y sus totales.

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::Instant;

use lakehouse_rag::application::dtos::agent::EtapaArnes;
use lakehouse_rag::application::use_cases::lakehouse_app::LakehouseApp;
use lakehouse_rag::infrastructure::bootstrap::{ConfiguracionComposicion, composicion_real};
use lakehouse_rag::infrastructure::llm::ClaudeAgentConfig;
use lakehouse_rag::infrastructure::llm::anthropic_api::MetricaPasoApi;
use lakehouse_rag::infrastructure::llm::modo_llm::{
    ModoLlm, agente_para, verificador_procedencia_para,
};
use serde_json::{Value, json};

struct Argumentos {
    centro: String,
    preguntas: PathBuf,
    salida: PathBuf,
    modo: ModoLlm,
    modelo: String,
    repeticiones: usize,
}

fn valor(args: &[String], bandera: &str) -> Option<String> {
    args.iter()
        .position(|a| a == bandera)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn argumentos() -> Result<Argumentos, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let uso = "uso: --centro <id> --preguntas <entrada.jsonl> --modo api|cli --salida <salida.jsonl> [--modelo sonnet] [--repeticiones N]";
    let requerido =
        |bandera: &str| valor(&args, bandera).ok_or_else(|| format!("falta {bandera}; {uso}"));
    let modo = match requerido("--modo")?.to_ascii_lowercase().as_str() {
        "api" => ModoLlm::Api,
        "cli" => ModoLlm::Cli,
        otro => return Err(format!("modo desconocido «{otro}»; {uso}")),
    };
    Ok(Argumentos {
        centro: requerido("--centro")?,
        preguntas: PathBuf::from(requerido("--preguntas")?),
        salida: PathBuf::from(requerido("--salida")?),
        modo,
        modelo: valor(&args, "--modelo").unwrap_or_else(|| "sonnet".into()),
        repeticiones: valor(&args, "--repeticiones")
            .map(|n| {
                n.parse()
                    .map_err(|_| format!("--repeticiones no es un número; {uso}"))
            })
            .transpose()?
            .unwrap_or(1),
    })
}

fn leer_preguntas(ruta: &PathBuf) -> Result<Vec<(String, String)>, String> {
    let archivo = std::fs::File::open(ruta).map_err(|e| format!("{}: {e}", ruta.display()))?;
    let mut preguntas = Vec::new();
    for (numero, linea) in std::io::BufReader::new(archivo).lines().enumerate() {
        let linea = linea.map_err(|e| e.to_string())?;
        if linea.trim().is_empty() {
            continue;
        }
        let valor: Value = serde_json::from_str(&linea)
            .map_err(|e| format!("línea {}: no es JSON ({e})", numero + 1))?;
        let id = valor["id"]
            .as_str()
            .map(str::to_string)
            .or_else(|| valor["id"].as_u64().map(|n| n.to_string()))
            .ok_or_else(|| format!("línea {}: falta `id`", numero + 1))?;
        let pregunta = valor["pregunta"]
            .as_str()
            .ok_or_else(|| format!("línea {}: falta `pregunta`", numero + 1))?;
        preguntas.push((id, pregunta.to_string()));
    }
    Ok(preguntas)
}

fn totales(pasos: &[MetricaPasoApi]) -> Value {
    let suma = |f: fn(&MetricaPasoApi) -> u64| pasos.iter().map(f).sum::<u64>();
    let coste: Option<f64> = pasos.iter().map(|p| p.coste_usd).sum();
    json!({
        "pasos": pasos.len(),
        "ttft_ms_primer_paso": pasos.first().and_then(|p| p.ttft_ms),
        "latencia_llm_ms": suma(|p| p.latencia_ms),
        "input_tokens": suma(|p| p.input_tokens),
        "cache_creation_input_tokens": suma(|p| p.cache_creation_input_tokens),
        "cache_read_input_tokens": suma(|p| p.cache_read_input_tokens),
        "output_tokens": suma(|p| p.output_tokens),
        "coste_usd": coste,
        "reintentos_de_transporte": pasos.iter().map(|p| p.intentos.saturating_sub(1)).sum::<u32>(),
        "bloques_thinking_descartados": pasos.iter().map(|p| p.bloques_thinking_descartados).sum::<usize>(),
    })
}

fn etapa(etapa: EtapaArnes) -> Value {
    serde_json::to_value(etapa).unwrap_or(Value::Null)
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let args = argumentos()?;
    let preguntas = leer_preguntas(&args.preguntas)?;
    let mut composicion = composicion_real(&ConfiguracionComposicion::desde_entorno());
    let agente = agente_para(args.modo, ClaudeAgentConfig::default());
    if args.modo == ModoLlm::Api && agente.api.is_none() {
        return Err(
            "modo api sin cliente: define ANTHROPIC_API_KEY o guárdala en el almacén del sistema"
                .into(),
        );
    }
    composicion.dependencias.agente = agente.runner;
    composicion.dependencias.verificador_procedencia = Some(verificador_procedencia_para(
        args.modo,
        ClaudeAgentConfig::default().binary_path,
    ));
    let app = LakehouseApp::new(composicion.dependencias);
    let modo = match args.modo {
        ModoLlm::Api => "api",
        ModoLlm::Cli => "cli",
    };

    let mut salida = std::fs::File::create(&args.salida)
        .map_err(|e| format!("{}: {e}", args.salida.display()))?;
    let total = preguntas.len() * args.repeticiones;
    let mut hechas = 0;
    for repeticion in 1..=args.repeticiones {
        for (id, pregunta) in &preguntas {
            hechas += 1;
            let inicio = Instant::now();
            let resultado = app
                .preguntar(pregunta, &args.centro, Some(&args.modelo))
                .await;
            let latencia_ms = inicio.elapsed().as_millis() as u64;
            let pasos = agente
                .api
                .as_ref()
                .map(|a| a.tomar_metricas())
                .unwrap_or_default();
            let mut linea = json!({
                "id": id,
                "repeticion": repeticion,
                "modo": modo,
                "modelo": args.modelo,
                "pregunta": pregunta,
                "latencia_ms": latencia_ms,
                "pasos_api": pasos,
                "totales_api": totales(&pasos),
            });
            match resultado {
                Ok(respuesta) => {
                    let omisiones = respuesta
                        .arnes
                        .iter()
                        .filter(|i| i.etapa == EtapaArnes::OmisionDeHerramienta)
                        .count();
                    let campos = json!({
                        "entregada": true,
                        "error": null,
                        "respuesta": respuesta.respuesta,
                        "salida_segura": respuesta.arnes.iter().any(|i| i.etapa == EtapaArnes::SalidaSegura),
                        "herramientas": respuesta.herramientas.iter().map(|h| json!({
                            "nombre": h.nombre, "argumentos": h.argumentos,
                            "resultado": h.resultado, "es_error": h.es_error,
                        })).collect::<Vec<_>>(),
                        "tools_invocadas": respuesta.herramientas.len(),
                        "arnes": respuesta.arnes.iter().map(|i| json!({
                            "etapa": etapa(i.etapa), "motivo": i.motivo,
                        })).collect::<Vec<_>>(),
                        "omisiones_de_herramienta": omisiones,
                    });
                    linea
                        .as_object_mut()
                        .unwrap()
                        .extend(campos.as_object().unwrap().clone());
                }
                Err(error) => {
                    let campos = json!({
                        "entregada": false, "error": error.to_string(), "respuesta": "",
                        "salida_segura": false, "herramientas": [], "tools_invocadas": 0,
                        "arnes": [], "omisiones_de_herramienta": 0,
                    });
                    linea
                        .as_object_mut()
                        .unwrap()
                        .extend(campos.as_object().unwrap().clone());
                }
            }
            writeln!(salida, "{linea}").map_err(|e| e.to_string())?;
            salida.flush().map_err(|e| e.to_string())?;
            eprintln!(
                "[{hechas}/{total}] {id} (rep. {repeticion}): {} en {latencia_ms} ms",
                if linea["entregada"] == true {
                    "entregada"
                } else {
                    "error"
                }
            );
        }
    }
    Ok(())
}
