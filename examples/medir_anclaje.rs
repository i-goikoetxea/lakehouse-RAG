//! Medición N2 del anclaje de tablas (`docs/arquitectura/ssd_generalidad_4_piezas.md` §6.3).
//!
//! - `sinteticos <casos.json> <salida.jsonl>`: ejecuta `anclar_tabla` sobre casos escritos por
//!   `benchmarks/anclaje/generar_sinteticos.py`, con la página real de cada fila. Sin Claude.
//! - `pdf <carpeta_pdf> <carpeta_cache> <salida.jsonl>`: parsea cada PDF con el `PdfParser` de la
//!   app, lo destila **una sola vez** con el destilador de la app (Sonnet, guardado en
//!   `<carpeta_cache>/<pdf>.paquete.json`) y mapea la revisión como la ingesta: escribe la página
//!   de cada celda. Las ejecuciones siguientes reutilizan el paquete y no gastan Claude. El oráculo
//!   es `benchmarks/anclaje/oraculo_pypdf.py`.
//!
//! La salida contiene texto de documentos reales: debe ir a una carpeta ignorada por git.

use std::path::{Path, PathBuf};

use lakehouse_rag::application::dtos::ingestion::DistilledDocumentPackage;
use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::application::ports::llm_distiller::LlmDistillerPort;
use lakehouse_rag::application::use_cases::mapeo_destilacion::{clave_documental, mapear_revision};
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::services::anclaje_tabla::{
    FilaParaAnclar, LineaPaginada, TablaParaAnclar, anclar_tabla,
};
use lakehouse_rag::infrastructure::llm::claude_headless_distiller::{
    ClaudeHeadlessConfig, ClaudeHeadlessDistiller,
};
use lakehouse_rag::infrastructure::parsers::PdfParser;
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct CasoSintetico {
    id: String,
    lineas: Vec<(u32, String)>,
    cabeceras: Vec<String>,
    filas: Vec<FilaSintetica>,
}

#[derive(Deserialize)]
struct FilaSintetica {
    clave: Vec<String>,
    valores: Vec<String>,
    paginas_validas: Vec<u32>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    match argumentos.as_slice() {
        [modo, casos, salida] if modo == "sinteticos" => medir_sinteticos(casos, salida),
        [modo, carpeta, cache, salida] if modo == "pdf" => medir_pdf(carpeta, cache, salida).await,
        _ => anyhow::bail!(
            "uso: medir_anclaje sinteticos <casos.json> <salida.jsonl> | pdf <carpeta_pdf> <cache> <salida.jsonl>"
        ),
    }
}

fn medir_sinteticos(casos: &str, salida: &str) -> anyhow::Result<()> {
    let casos: Vec<CasoSintetico> = serde_json::from_slice(&std::fs::read(casos)?)?;
    let mut lineas_salida = Vec::new();
    for caso in &casos {
        let lineas: Vec<LineaPaginada> = caso
            .lineas
            .iter()
            .map(|(pagina, texto)| LineaPaginada {
                pagina: *pagina,
                texto,
            })
            .collect();
        let tabla = TablaParaAnclar {
            cabeceras: caso.cabeceras.clone(),
            filas: caso
                .filas
                .iter()
                .map(|f| FilaParaAnclar {
                    clave: f.clave.clone(),
                    valores: f.valores.clone(),
                })
                .collect(),
        };
        let anclaje = anclar_tabla(&lineas, &tabla);
        for (indice, (fila, calculada)) in caso.filas.iter().zip(&anclaje.por_fila).enumerate() {
            lineas_salida.push(
                json!({
                    "caso": caso.id, "fila": indice, "validas": fila.paginas_validas,
                    "calculada": calculada, "bloque": anclaje.bloque,
                })
                .to_string(),
            );
        }
    }
    std::fs::write(salida, lineas_salida.join("\n") + "\n")?;
    println!("{} casos, {} filas", casos.len(), lineas_salida.len());
    Ok(())
}

async fn medir_pdf(carpeta: &str, cache: &str, salida: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(cache)?;
    let configuracion = ClaudeHeadlessConfig::default();
    anyhow::ensure!(
        configuracion.model == "sonnet",
        "la destilación de medición debe usar Sonnet, no {}",
        configuracion.model
    );
    let destilador = ClaudeHeadlessDistiller::new(configuracion);
    let parser = PdfParser::new();
    let centro = CenterId::new("medicion-anclaje")?;
    let mut lineas_salida = Vec::new();
    for ruta in pdfs_de(Path::new(carpeta))? {
        let nombre = ruta.file_name().unwrap().to_string_lossy().to_string();
        let parsed = parser.parse_bytes(&nombre, &std::fs::read(&ruta)?).await?;
        let ruta_paquete = Path::new(cache).join(format!("{nombre}.paquete.json"));
        let paquete: DistilledDocumentPackage = if ruta_paquete.is_file() {
            serde_json::from_slice(&std::fs::read(&ruta_paquete)?)?
        } else {
            println!("{nombre}: destilando con Sonnet (una sola vez)…");
            let paquete = destilador.distill(&parsed).await?;
            std::fs::write(&ruta_paquete, serde_json::to_vec_pretty(&paquete)?)?;
            paquete
        };
        let clave = clave_documental(&centro, &paquete.catalogo)?;
        let revision = mapear_revision(&clave, &parsed, &paquete, "medicion")?;
        for tabla in &revision.tablas {
            for celda in revision
                .celdas
                .iter()
                .filter(|c| c.tabla_id == tabla.tabla_id)
            {
                lineas_salida.push(
                    json!({
                        "archivo": nombre,
                        "tabla": tabla.titulo_tabla,
                        "bloque": tabla.paginas,
                        "fila": [celda.eje_y.categoria, celda.eje_y.minimo, celda.eje_y.maximo],
                        "columna": [celda.eje_x.categoria, celda.eje_x.minimo, celda.eje_x.maximo],
                        "valor": celda.valor_resultado_texto,
                        "pagina": celda.pagina,
                    })
                    .to_string(),
                );
            }
        }
        println!(
            "{nombre}: {} tablas, {} celdas, {} con página",
            revision.tablas.len(),
            revision.celdas.len(),
            revision
                .celdas
                .iter()
                .filter(|c| c.pagina.is_some())
                .count()
        );
    }
    std::fs::write(salida, lineas_salida.join("\n") + "\n")?;
    Ok(())
}

fn pdfs_de(carpeta: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut rutas: Vec<PathBuf> = std::fs::read_dir(carpeta)?
        .filter_map(Result::ok)
        .map(|entrada| entrada.path())
        .filter(|ruta| {
            ruta.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        })
        .collect();
    rutas.sort();
    Ok(rutas)
}
