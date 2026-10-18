//! Formateo de la salida de los subcomandos: texto legible para personas o JSON estable para
//! scripts y la puerta E2E (`--json`).

use serde_json::{Value, json};

use crate::application::ports::lakehouse_projection::ResumenProyeccion;
use crate::application::use_cases::lakehouse_app::{
    InformeIntegridadCentro, RespuestaLakehouse, ResultadoIngesta,
};
use crate::domain::models::center::CentroConocimiento;

/// Longitud máxima del extracto de resultado de cada herramienta en modo legible
const EXTRACTO_RESULTADO: usize = 400;

fn recortar(texto: &str, maximo: usize) -> String {
    if texto.chars().count() <= maximo {
        return texto.to_string();
    }
    let corte: String = texto.chars().take(maximo).collect();
    format!("{corte}…")
}

pub fn centro_json(centro: &CentroConocimiento) -> Value {
    json!({
        "id": centro.id().as_str(),
        "nombre": centro.manifiesto().nombre,
        "ruta_disco": centro.manifiesto().ruta_almacenamiento,
        "activo": centro.esta_activo(),
        "fecha_creacion": centro.manifiesto().fecha_creacion.to_rfc3339(),
    })
}

pub fn centros_json(centros: &[CentroConocimiento]) -> Value {
    json!({ "ok": true, "centros": centros.iter().map(centro_json).collect::<Vec<_>>() })
}

pub fn centros_legible(centros: &[CentroConocimiento]) -> String {
    if centros.is_empty() {
        return "No hay centros registrados. Crea uno con --crear-centro <id> <ruta> [nombre]."
            .to_string();
    }
    let mut lineas = vec![format!("Centros registrados ({}):", centros.len())];
    for centro in centros {
        lineas.push(format!(
            "  [{}] {} — {} ({})",
            if centro.esta_activo() {
                "ACTIVO"
            } else {
                "inactivo"
            },
            centro.id(),
            centro.manifiesto().nombre,
            centro.manifiesto().ruta_almacenamiento
        ));
    }
    lineas.join("\n")
}

pub fn proyeccion_json(resumen: &ResumenProyeccion) -> Value {
    json!({
        "centro_id": resumen.centro_id,
        "vistas": resumen.vistas.iter().map(|v| json!({
            "tabla": v.tabla,
            "vista": v.vista,
            "filas": v.filas,
            "filas_vigentes": v.filas_vigentes,
        })).collect::<Vec<_>>(),
    })
}

pub fn proyeccion_legible(resumen: &ResumenProyeccion) -> String {
    let mut lineas = vec![format!(
        "Centro '{}' activo. Vistas reconstruidas en DataFusion:",
        resumen.centro_id
    )];
    for vista in &resumen.vistas {
        lineas.push(format!(
            "  {} → {} filas ({} vigentes)",
            vista.vista, vista.filas, vista.filas_vigentes
        ));
    }
    lineas.join("\n")
}

pub fn ingesta_json(resultado: &ResultadoIngesta) -> Value {
    let informe = &resultado.informe;
    json!({
        "ok": true,
        "doc_id": informe.doc_id,
        "centro_id": informe.centro_id,
        "version": informe.version,
        "ingest_id": informe.ingest_id,
        "ya_ingerido": informe.ya_ingerido,
        "sha256": informe.sha256,
        "copia_original": resultado.copia_original,
        "totales": {
            "chunks": informe.total_chunks,
            "hitos": informe.total_tuplas,
            "tablas": informe.total_tablas,
            "celdas": informe.total_celdas,
            "aristas": informe.total_aristas,
        },
        "versiones_derogadas": informe.versiones_derogadas,
        "proyeccion": resultado.proyeccion.as_ref().map(proyeccion_json),
        "aviso_proyeccion": resultado.aviso_proyeccion,
    })
}

pub fn ingesta_legible(resultado: &ResultadoIngesta) -> String {
    let informe = &resultado.informe;
    if informe.ya_ingerido {
        return format!(
            "Ya ingerido: {} v{} (sha256 {}). No se ha llamado a Claude ni se ha escrito nada.",
            informe.doc_id, informe.version, informe.sha256
        );
    }
    let mut texto = format!(
        "Ingerido {} v{} en '{}' ({})\n  chunks {} · hitos {} · tablas {} · celdas {} · aristas {}\n  original: {}",
        informe.doc_id,
        informe.version,
        informe.centro_id,
        informe.ingest_id,
        informe.total_chunks,
        informe.total_tuplas,
        informe.total_tablas,
        informe.total_celdas,
        informe.total_aristas,
        resultado.copia_original
    );
    if !informe.versiones_derogadas.is_empty() {
        texto.push_str(&format!(
            "\n  versiones derogadas: {:?}",
            informe.versiones_derogadas
        ));
    }
    if resultado.proyeccion.is_some() {
        texto.push_str("\n  vistas del centro reconstruidas");
    }
    if let Some(aviso) = &resultado.aviso_proyeccion {
        texto.push_str(&format!("\n  AVISO: {aviso}"));
    }
    texto
}

pub fn respuesta_json(respuesta: &RespuestaLakehouse) -> Value {
    json!({
        "ok": true,
        "centro_id": respuesta.centro_id,
        "modelo": respuesta.modelo,
        "respuesta": respuesta.respuesta,
        "herramientas": respuesta.herramientas.iter().map(|h| json!({
            "nombre": h.nombre,
            "argumentos": h.argumentos,
            "es_error": h.es_error,
            "resultado": h.resultado,
        })).collect::<Vec<_>>(),
    })
}

pub fn respuesta_legible(respuesta: &RespuestaLakehouse) -> String {
    let mut lineas = vec![respuesta.respuesta.trim().to_string(), String::new()];
    if respuesta.herramientas.is_empty() {
        lineas.push(format!(
            "Herramientas usadas: ninguna (modelo {}, centro {})",
            respuesta.modelo, respuesta.centro_id
        ));
    } else {
        lineas.push(format!(
            "Herramientas usadas ({}; modelo {}, centro {}):",
            respuesta.herramientas.len(),
            respuesta.modelo,
            respuesta.centro_id
        ));
        for (i, uso) in respuesta.herramientas.iter().enumerate() {
            lineas.push(format!(
                "  {}. {} [{}] {}",
                i + 1,
                uso.nombre,
                if uso.es_error { "error" } else { "ok" },
                uso.argumentos
            ));
            lineas.push(format!(
                "     → {}",
                recortar(&uso.resultado.to_string(), EXTRACTO_RESULTADO)
            ));
        }
    }
    lineas.join("\n")
}

pub fn integridad_json(informe: &InformeIntegridadCentro) -> Value {
    let i = &informe.informe;
    json!({
        "ok": true,
        "centro_id": informe.centro_id,
        "integro": informe.es_integro(),
        "tablas_ausentes": informe.tablas_ausentes,
        "ingestas_staged": i.ingestas_staged,
        "huerfanos": i.huerfanos.iter().map(|h| json!({
            "tabla": h.tabla, "doc_id": h.doc_id, "version_num": h.version_num, "filas": h.filas,
        })).collect::<Vec<_>>(),
        "versiones_vigentes_multiples": i.versiones_vigentes_multiples.iter().map(|v| json!({
            "tabla": v.tabla, "doc_id": v.doc_id, "versiones": v.versiones,
        })).collect::<Vec<_>>(),
    })
}

pub fn integridad_legible(informe: &InformeIntegridadCentro) -> String {
    if informe.es_integro() {
        return format!(
            "Centro '{}' ÍNTEGRO: 6/6 tablas, sin huérfanos, sin ingestas STAGED y una sola versión vigente por documento.",
            informe.centro_id
        );
    }
    let i = &informe.informe;
    let mut lineas = vec![format!("Centro '{}' NO íntegro:", informe.centro_id)];
    if !informe.tablas_ausentes.is_empty() {
        lineas.push(format!(
            "  tablas ausentes: {}",
            informe.tablas_ausentes.join(", ")
        ));
    }
    if !i.ingestas_staged.is_empty() {
        lineas.push(format!(
            "  ingestas STAGED: {}",
            i.ingestas_staged.join(", ")
        ));
    }
    for h in &i.huerfanos {
        lineas.push(format!(
            "  huérfanos en {}: {} v{} ({} filas)",
            h.tabla, h.doc_id, h.version_num, h.filas
        ));
    }
    for v in &i.versiones_vigentes_multiples {
        lineas.push(format!(
            "  varias versiones vigentes en {}: {} {:?}",
            v.tabla, v.doc_id, v.versiones
        ));
    }
    lineas.join("\n")
}

pub fn error_json(mensaje: &str) -> Value {
    json!({ "ok": false, "error": mensaje })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::use_cases::lakehouse_app::UsoHerramienta;

    #[test]
    fn test_respuesta_legible_lista_herramientas_con_extracto() {
        let respuesta = RespuestaLakehouse {
            centro_id: "obra-a".into(),
            modelo: "sonnet".into(),
            respuesta: "Jefe de Producción, FOR-ALM-05".into(),
            herramientas: vec![UsoHerramienta {
                nombre: "query_operational_matrix".into(),
                argumentos: json!({"fase": "CUSTODIA"}),
                es_error: false,
                resultado: json!([{ "texto": "x".repeat(1000) }]),
            }],
        };
        let texto = respuesta_legible(&respuesta);
        assert!(texto.starts_with("Jefe de Producción"));
        assert!(texto.contains("1. query_operational_matrix [ok]"));
        assert!(texto.contains('…'));
        let json = respuesta_json(&respuesta);
        assert_eq!(
            json["herramientas"][0]["nombre"],
            "query_operational_matrix"
        );
        assert_eq!(json["ok"], true);
    }

    #[test]
    fn test_centros_vacios_sugieren_crear_uno() {
        assert!(centros_legible(&[]).contains("--crear-centro"));
        assert_eq!(centros_json(&[])["centros"], json!([]));
    }
}
