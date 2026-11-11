//! Calibración N1 de la compuerta de legibilidad (`ssd_generalidad_4_piezas.md` §1.3 y §6.2), sin
//! Claude. Extrae cada PDF con el proceso hijo de pdfium y escribe, por documento, la clase y las
//! señales de cada página. Solo números: ningún texto del documento.
//!
//! Uso: `cargo build --bin lakehouse_rag_cli --example metricas_legibilidad` y
//! `target/debug/examples/metricas_legibilidad <carpeta_pdf> <salida.jsonl>`

use std::io::Write;
use std::path::Path;

use lakehouse_rag::infrastructure::parsers::legibilidad::{ClasePagina, clasificar_pagina};
use lakehouse_rag::infrastructure::parsers::proceso_aislado::ProcesoExtractor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    let [carpeta, salida] = argumentos.as_slice() else {
        anyhow::bail!("uso: metricas_legibilidad <carpeta_pdf> <salida.jsonl>");
    };
    let mut rutas: Vec<_> = std::fs::read_dir(carpeta)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|r| r.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
        .collect();
    rutas.sort();
    let extractor = ProcesoExtractor::junto_al_ejecutable();
    let mut fichero = std::fs::File::create(salida)?;
    for ruta in rutas {
        let nombre = Path::new(&ruta)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let paginas = match extractor.extraer(&std::fs::read(&ruta)?).await {
            Ok(paginas) => paginas,
            Err(motivo) => {
                println!("{nombre}: ERROR {motivo}");
                continue;
            }
        };
        let mut resumen = Vec::new();
        for pagina in &paginas {
            let (clase, m) = clasificar_pagina(pagina.texto.as_deref().unwrap_or(""));
            let clase = match clase {
                ClasePagina::Legible => "LEGIBLE".to_string(),
                ClasePagina::SinTexto => "SIN_TEXTO".to_string(),
                ClasePagina::Degradada { motivos } => format!("DEGRADADA {motivos:?}"),
            };
            let no_blancos = m.no_blancos.max(1) as f32;
            let fila = serde_json::json!({
                "archivo": nombre,
                "pagina": pagina.pagina,
                "clase": clase,
                "no_blancos": m.no_blancos,
                "reemplazo_o_control": m.reemplazo_o_control as f32 / no_blancos,
                "uso_privado": m.uso_privado as f32 / no_blancos,
                "uso_privado_dentro_de_palabra": m.uso_privado_dentro_de_palabra,
                "tokens_evaluables": m.tokens_evaluables,
                "mojibake": m.tokens_sospechosos as f32 / m.tokens_evaluables.max(1) as f32,
                "densidad_alfanumerica": m.densidad_alfanumerica,
                "simbolos_en_palabras": m.tokens_con_simbolos as f32 / m.tokens_con_letras.max(1) as f32,
                "tokens_con_letras": m.tokens_con_letras,
                "coherencia": m.letras_en_rachas as f32 / m.letras_no_cjk.max(1) as f32,
                "letras_no_cjk": m.letras_no_cjk,
                "digitos": m.digitos as f32 / no_blancos,
                "cjk": m.cjk as f32 / no_blancos,
                "ruido": (m.simbolos_anomalos.max(m.ruido_latin1)) as f32 / no_blancos,
            });
            writeln!(fichero, "{fila}")?;
            resumen.push(fila);
        }
        let maximo = |clave: &str, minimo_tokens: usize| {
            resumen
                .iter()
                .filter(|f| f["tokens_evaluables"].as_u64().unwrap_or(0) as usize >= minimo_tokens)
                .filter(|f| f["clase"] != "SIN_TEXTO")
                .map(|f| f[clave].as_f64().unwrap_or(0.0))
                .fold(0.0_f64, f64::max)
        };
        let cuenta = |prefijo: &str| {
            resumen
                .iter()
                .filter(|f| f["clase"].as_str().unwrap_or("").starts_with(prefijo))
                .count()
        };
        println!(
            "{nombre}: {} págs; legibles {}, degradadas {}, sin texto {}; máx control {:.4}, PUA {:.4}, PUA en palabra {}, mojibake {:.3} ({} págs evaluables)",
            resumen.len(),
            cuenta("LEGIBLE"),
            cuenta("DEGRADADA"),
            cuenta("SIN_TEXTO"),
            maximo("reemplazo_o_control", 0),
            maximo("uso_privado", 0),
            resumen
                .iter()
                .map(|f| f["uso_privado_dentro_de_palabra"].as_u64().unwrap_or(0))
                .max()
                .unwrap_or(0),
            maximo("mojibake", 80),
            resumen
                .iter()
                .filter(|f| f["tokens_evaluables"].as_u64().unwrap_or(0) >= 80)
                .count(),
        );
    }
    Ok(())
}
