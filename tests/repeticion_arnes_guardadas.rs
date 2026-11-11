//! Herramienta de evaluación sin Claude (ignorada): repite respuestas guardadas por el arnés
//! (`PostInferenceGuard` y `VerificadorPaginas`) y vuelca, por respuesta, si el guard la
//! rechazaría (reintento) y qué avisos de página añadiría, con el estado de cada cita revisada.
//!
//! Admite dos formatos de crudas:
//! - Benchmark (`benchmark_crudas_*.jsonl`): una línea por respuesta con `pregunta`,
//!   `respuesta` y `herramientas` (`nombre`, `argumentos`, `resultado`, `es_error`).
//! - Aguja (`aguja_crudas_*.jsonl`): una línea por caso con `consultas`. No guardan la salida de
//!   las tools, así que la fuente se reconstruye con la frase de cada aguja tras la marca de su
//!   página esperada. Un extracto ajeno a la aguja queda «no encontrado» por la reconstrucción.
//!
//! La respuesta guardada ya lleva el bloque de verificación si lo hubo: se quita antes de repetir.
//! No versiona datos: las crudas viven fuera del repo.
//!
//! Uso (PowerShell):
//! `$env:CRUDAS_GUARDADAS="a.jsonl;b.jsonl"; $env:SALIDA_REPETICION="salida.json";
//!  cargo test --test repeticion_arnes_guardadas -- --ignored --nocapture`

use chrono::NaiveDate;
use lakehouse_rag::application::dtos::tools::ToolResult;
use lakehouse_rag::application::use_cases::arnes_turno::{
    CitaRevisada, ENCABEZADO_VERIFICACION_PAGINAS, IntentDetector, PostInferenceGuard,
    RegistroTurno, ResultadoCita, VerificadorPaginas,
};
use serde_json::{Value, json};

struct RespuestaGuardada {
    origen: String,
    id: String,
    pregunta: String,
    respuesta: String,
    registro: RegistroTurno,
    puntuacion: Value,
}

fn sin_bloque_de_verificacion(respuesta: &str) -> (String, usize) {
    let mut partes = respuesta.split(ENCABEZADO_VERIFICACION_PAGINAS);
    let modelo = partes.next().unwrap_or_default().trim_end().to_string();
    let avisos_guardados = partes
        .next()
        .map(|bloque| {
            bloque
                .lines()
                .filter(|l| l.trim_start().starts_with("- "))
                .count()
        })
        .unwrap_or(0);
    (modelo, avisos_guardados)
}

fn registro_de_herramientas(herramientas: &[Value]) -> RegistroTurno {
    let mut registro = RegistroTurno::default();
    for (indice, herramienta) in herramientas.iter().enumerate() {
        registro.registrar(ToolResult {
            call_id: format!("guardada_{indice}"),
            tool_name: herramienta["nombre"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            output: herramienta["resultado"].clone(),
            is_error: herramienta["es_error"].as_bool().unwrap_or(false),
        });
    }
    registro
}

fn registro_aguja(caso: &Value) -> RegistroTurno {
    let mut agujas: Vec<(String, u64)> = Vec::new();
    if let Some(aguja) = caso.get("aguja").filter(|a| a.is_object()) {
        agujas.push((
            aguja["frase"].as_str().unwrap_or_default().to_string(),
            caso["pagina_esperada"].as_u64().unwrap_or_default(),
        ));
    }
    if let (Some(lista), Some(paginas)) = (
        caso["agujas"].as_array(),
        caso["paginas_esperadas"].as_array(),
    ) {
        for (aguja, pagina) in lista.iter().zip(paginas) {
            agujas.push((
                aguja["frase"].as_str().unwrap_or_default().to_string(),
                pagina.as_u64().unwrap_or_default(),
            ));
        }
    }
    let resultados: Vec<Value> = agujas
        .iter()
        .map(|(frase, pagina)| json!({ "texto": format!("⟦p. {pagina}⟧ {frase}") }))
        .collect();
    let mut registro = RegistroTurno::default();
    registro.registrar(ToolResult {
        call_id: "reconstruido".into(),
        tool_name: "search_normative_context".into(),
        output: json!({ "grupos": [{ "resultados": resultados }] }),
        is_error: false,
    });
    registro
}

/// Fecha de la ejecución, del sello `_AAAAMMDDT` del nombre de las crudas: el reloj de la app la
/// inyectó entonces como fecha de hoy
fn fecha_de_la_ejecucion(ruta: &str) -> Option<NaiveDate> {
    let nombre = ruta.rsplit(['/', '\\']).next()?;
    let sello = nombre
        .split('_')
        .find(|parte| parte.len() > 8 && parte.as_bytes()[8] == b'T')?;
    NaiveDate::parse_from_str(&sello[..8], "%Y%m%d").ok()
}

fn leer(ruta: &str) -> Vec<RespuestaGuardada> {
    let hoy = fecha_de_la_ejecucion(ruta);
    let contenido = std::fs::read_to_string(ruta).expect("crudas");
    let mut respuestas = Vec::new();
    for (linea, texto) in contenido.lines().enumerate() {
        if texto.trim().is_empty() {
            continue;
        }
        let fila: Value = serde_json::from_str(texto).expect("línea JSON");
        if let Some(consultas) = fila["consultas"].as_array() {
            let mut registro = registro_aguja(&fila);
            if let Some(hoy) = hoy {
                registro.fijar_fecha_de_hoy(hoy);
            }
            for (indice, consulta) in consultas.iter().enumerate() {
                if !consulta["fallo"].is_null() {
                    continue;
                }
                respuestas.push(RespuestaGuardada {
                    origen: ruta.to_string(),
                    id: format!("caso{linea}-consulta{indice}"),
                    pregunta: consulta["pregunta"].as_str().unwrap_or_default().into(),
                    respuesta: consulta["respuesta"].as_str().unwrap_or_default().into(),
                    registro: registro.clone(),
                    puntuacion: consulta["pagina_exacta_estricta"].clone(),
                });
            }
        } else {
            if !fila["fallo"].is_null() || fila["respuesta"].is_null() {
                continue;
            }
            let herramientas = fila["herramientas"].as_array().cloned().unwrap_or_default();
            let mut registro = registro_de_herramientas(&herramientas);
            if let Some(hoy) = hoy {
                registro.fijar_fecha_de_hoy(hoy);
            }
            respuestas.push(RespuestaGuardada {
                origen: ruta.to_string(),
                id: format!(
                    "{}-r{}",
                    fila["id"].as_str().unwrap_or("?"),
                    fila["repeticion"]
                ),
                pregunta: fila["pregunta"].as_str().unwrap_or_default().into(),
                respuesta: fila["respuesta"].as_str().unwrap_or_default().into(),
                registro,
                puntuacion: fila["puntuacion"].clone(),
            });
        }
    }
    respuestas
}

#[test]
#[ignore = "herramienta de evaluación: necesita crudas guardadas fuera del repo"]
fn repetir_respuestas_guardadas() {
    let rutas = std::env::var("CRUDAS_GUARDADAS").expect("CRUDAS_GUARDADAS");
    let salida = std::env::var("SALIDA_REPETICION").expect("SALIDA_REPETICION");
    let mut filas = Vec::new();
    let (mut rechazos, mut avisos, mut con_bloque) = (0, 0, 0);
    let (mut mutadas_total, mut mutadas_con_nota) = (0, 0);
    for ruta in rutas.split(';').filter(|r| !r.is_empty()) {
        for guardada in leer(ruta) {
            let (modelo, avisos_guardados) = sin_bloque_de_verificacion(&guardada.respuesta);
            let intencion = IntentDetector::clasificar(&guardada.pregunta);
            let veredicto = PostInferenceGuard::validar(
                &modelo,
                &guardada.pregunta,
                &intencion,
                &guardada.registro,
            );
            let revisadas = VerificadorPaginas::revisar(&modelo, &guardada.registro);
            let motivos: Vec<String> = revisadas.iter().filter_map(|r| r.motivo()).collect();
            rechazos += usize::from(veredicto.is_err());
            avisos += motivos.len();
            con_bloque += usize::from(!motivos.is_empty());
            let citas: Vec<Value> = revisadas
                .iter()
                .map(|r| {
                    json!({
                        "extracto": r.extracto,
                        "citadas": r.citadas.iter().collect::<Vec<_>>(),
                        "resultado": format!("{:?}", r.resultado),
                        "motivo": r.motivo(),
                    })
                })
                .collect();
            let mutadas: Vec<Value> = revisadas
                .iter()
                .filter(|r| r.resultado == ResultadoCita::Confirmada)
                .map(|r| mutar_pagina(r, &guardada.registro))
                .collect();
            mutadas_con_nota += mutadas.iter().filter(|m| !m["motivo"].is_null()).count();
            mutadas_total += mutadas.len();
            filas.push(json!({
                "origen": guardada.origen,
                "id": guardada.id,
                "puntuacion_guardada": guardada.puntuacion,
                "intencion": format!("{intencion:?}"),
                "rechazo": veredicto.err().map(|v| v.motivo()),
                "avisos_pagina": motivos,
                "citas": citas,
                "avisos_guardados": avisos_guardados,
                "mutadas": mutadas,
            }));
        }
    }
    let resumen = json!({
        "respuestas": filas.len(),
        "rechazos_del_guard": rechazos,
        "avisos_de_pagina": avisos,
        "respuestas_con_bloque": con_bloque,
        "citas_mutadas": mutadas_total,
        "citas_mutadas_con_nota": mutadas_con_nota,
        "filas": filas,
    });
    std::fs::write(&salida, serde_json::to_string_pretty(&resumen).unwrap()).unwrap();
    println!(
        "{} respuestas; {rechazos} rechazos del guard; {avisos} avisos de página en {con_bloque} respuestas; {mutadas_con_nota}/{mutadas_total} citas mutadas con nota",
        resumen["respuestas"]
    );
}

/// Desplazamiento de la página mutada: ajena a cualquier página citada
const DESPLAZAMIENTO_MUTACION: u32 = 1000;

/// Error sembrado: la misma cita confirmada, con una página que no la contiene. Mide que el
/// verificador sigue avisando de los errores reales con extractos de respuestas reales.
fn mutar_pagina(revisada: &CitaRevisada, registro: &RegistroTurno) -> Value {
    let ajena =
        revisada.citadas.iter().max().copied().unwrap_or_default() + DESPLAZAMIENTO_MUTACION;
    let respuesta = format!("«{}» (p. {ajena})", revisada.extracto);
    let mutada = VerificadorPaginas::revisar(&respuesta, registro);
    json!({
        "extracto": revisada.extracto,
        "pagina_ajena": ajena,
        "resultado": mutada.first().map(|m| format!("{:?}", m.resultado)),
        "motivo": mutada.first().and_then(CitaRevisada::motivo),
    })
}
