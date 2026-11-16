//! Calibración del Nivel 2 de procedencia con Claude Haiku 5.5 real (Pieza 5, Fase 5.1).
//!
//! Gasta API: necesita `ANTHROPIC_API_KEY` en el entorno. Lee la batería sintética, pide trazas
//! para cada caso, las comprueba con la misma función de Rust que el guard y mide latencia, tokens
//! y coste. **Puerta:** ninguna cifra falsa aprobada.
//!
//! ```text
//! cargo run --example calibrar_procedencia_api -- [pasadas] [salida.jsonl] [forzada]
//! ```

use std::sync::{Arc, Mutex};
use std::time::Duration;

use lakehouse_rag::application::dtos::procedencia::{
    SalidaTurno, SolicitudProcedencia, Traza, TrazasPropuestas,
};
use lakehouse_rag::application::ports::verificador_procedencia::VerificadorProcedenciaPort;
use lakehouse_rag::application::use_cases::arnes_turno::procedencia::{
    CifraPendiente, cifras_verificadas,
};
use lakehouse_rag::infrastructure::llm::anthropic_api::mensajes::{TARIFA_HAIKU_5_5, Uso};
use lakehouse_rag::infrastructure::llm::anthropic_api::{
    AnthropicHttpClient, AnthropicProcedenciaVerifier, ConfigClienteAnthropic, SecretApiKey,
};
use serde::Deserialize;
use serde_json::json;

const BATERIA: &str = include_str!("../benchmarks/calibracion_procedencia/bateria_sintetica.json");

#[derive(Deserialize)]
struct Bateria {
    casos: Vec<Caso>,
}

#[derive(Deserialize)]
struct Caso {
    id: String,
    idioma: String,
    pregunta: String,
    salidas: Vec<SalidaTurno>,
    borrador: String,
    cifras: Vec<CifraCaso>,
}

#[derive(Deserialize)]
struct CifraCaso {
    literal: String,
    legitima: bool,
}

#[derive(Clone, Copy, Default)]
struct Medida {
    latencia: Duration,
    intentos: u32,
    uso: Uso,
}

fn percentil(valores: &mut [f64], p: f64) -> f64 {
    if valores.is_empty() {
        return 0.0;
    }
    valores.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rango = (p * (valores.len() - 1) as f64).round() as usize;
    valores[rango]
}

/// Por qué no pasó una cifra legítima
fn diagnostico(
    solicitud: &SolicitudProcedencia,
    pendiente: &CifraPendiente,
    trazas: &TrazasPropuestas,
) -> &'static str {
    let con_etiqueta = trazas.trazas.iter().any(|t| t.cifra == pendiente.literal);
    let reetiquetadas = TrazasPropuestas {
        trazas: trazas
            .trazas
            .iter()
            .map(|t| Traza {
                cifra: pendiente.literal.clone(),
                tipo: t.tipo.clone(),
            })
            .collect(),
    };
    let valida_reetiquetada =
        cifras_verificadas(solicitud, std::slice::from_ref(pendiente), &reetiquetadas)
            .contains(&pendiente.literal);
    match (con_etiqueta, valida_reetiquetada) {
        (false, true) => "etiqueta_cifra_distinta",
        (false, false) if trazas.trazas.is_empty() => "sin_trazas",
        (false, false) => "sin_traza_para_la_cifra",
        (true, _) => "traza_no_valida_en_rust",
    }
}

#[tokio::main]
async fn main() {
    let argumentos: Vec<String> = std::env::args().collect();
    let pasadas: usize = argumentos.get(1).and_then(|a| a.parse().ok()).unwrap_or(2);
    let salida = argumentos.get(2).cloned();
    let key = match SecretApiKey::desde_entorno() {
        Ok(key) => key,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let medidas: Arc<Mutex<Vec<Medida>>> = Arc::default();
    let registro = medidas.clone();
    let cliente =
        AnthropicHttpClient::new(key, ConfigClienteAnthropic::procedencia()).expect("cliente");
    let verificador =
        AnthropicProcedenciaVerifier::new(cliente).con_observador(Box::new(move |r| {
            registro.lock().unwrap().push(Medida {
                latencia: r.latencia,
                intentos: r.intentos,
                uso: r.respuesta.usage,
            });
        }));
    // Por defecto el adaptador razona antes de llamar; `forzada` mide la tool forzada
    let razonar = argumentos.get(3).is_none_or(|a| a != "forzada");
    let verificador = if razonar {
        verificador
    } else {
        verificador.con_tool_forzada()
    };

    let bateria: Bateria = serde_json::from_str(BATERIA).expect("batería");
    let mut lineas = Vec::new();
    let (mut legitimas, mut legitimas_ok, mut falsas, mut falsas_aprobadas, mut errores) =
        (0, 0, 0, 0, 0);
    for pasada in 1..=pasadas {
        for caso in &bateria.casos {
            let solicitud = SolicitudProcedencia {
                pregunta: caso.pregunta.clone(),
                salidas: caso.salidas.clone(),
                borrador: caso.borrador.clone(),
                cifras: caso.cifras.iter().map(|c| c.literal.clone()).collect(),
            };
            let pendientes: Vec<CifraPendiente> = caso
                .cifras
                .iter()
                .map(|c| CifraPendiente {
                    literal: c.literal.clone(),
                    solo_estructural: false,
                })
                .collect();
            let resultado = verificador.proponer_trazas(&solicitud).await;
            let (trazas, error) = match resultado {
                Ok(trazas) => (trazas, None),
                Err(e) => {
                    errores += 1;
                    (TrazasPropuestas::default(), Some(e.to_string()))
                }
            };
            let verificadas = cifras_verificadas(&solicitud, &pendientes, &trazas);
            for (cifra, pendiente) in caso.cifras.iter().zip(&pendientes) {
                let aprobada = verificadas.contains(&cifra.literal);
                if cifra.legitima {
                    legitimas += 1;
                    legitimas_ok += usize::from(aprobada);
                } else {
                    falsas += 1;
                    falsas_aprobadas += usize::from(aprobada);
                }
                let motivo = (cifra.legitima && !aprobada && error.is_none())
                    .then(|| diagnostico(&solicitud, pendiente, &trazas));
                lineas.push(json!({
                    "pasada": pasada, "caso": caso.id, "idioma": caso.idioma, "cifra": cifra.literal,
                    "legitima": cifra.legitima, "aprobada": aprobada, "diagnostico": motivo,
                    "error": error, "trazas": trazas,
                }));
            }
        }
    }

    let medidas = medidas.lock().unwrap().clone();
    let mut latencias: Vec<f64> = medidas
        .iter()
        .map(|m| m.latencia.as_secs_f64() * 1000.0)
        .collect();
    let total = medidas.iter().fold(Uso::default(), |a, m| Uso {
        input_tokens: a.input_tokens + m.uso.input_tokens,
        cache_creation_input_tokens: a.cache_creation_input_tokens
            + m.uso.cache_creation_input_tokens,
        cache_read_input_tokens: a.cache_read_input_tokens + m.uso.cache_read_input_tokens,
        output_tokens: a.output_tokens + m.uso.output_tokens,
    });
    let reintentos: u32 = medidas.iter().map(|m| m.intentos - 1).sum();
    let resumen = json!({
        "pasadas": pasadas, "razonamiento_previo": razonar, "llamadas_validas": medidas.len(), "llamadas_con_error": errores,
        "falsas": falsas, "falsas_aprobadas": falsas_aprobadas,
        "legitimas": legitimas, "legitimas_aprobadas": legitimas_ok,
        "latencia_ms_p50": percentil(&mut latencias.clone(), 0.5),
        "latencia_ms_p95": percentil(&mut latencias, 0.95),
        "reintentos_de_transporte": reintentos,
        "tokens": { "input": total.input_tokens, "cache_creation": total.cache_creation_input_tokens,
                    "cache_read": total.cache_read_input_tokens, "output": total.output_tokens },
        "coste_usd": total.coste_estimado_usd(&TARIFA_HAIKU_5_5),
        "puerta_ninguna_falsa_aprobada": falsas_aprobadas == 0,
    });
    if let Some(ruta) = salida {
        let texto: String = lineas.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&ruta, texto).expect("salida");
    }
    println!("{}", serde_json::to_string_pretty(&resumen).unwrap());
    if falsas_aprobadas > 0 {
        std::process::exit(1);
    }
}
