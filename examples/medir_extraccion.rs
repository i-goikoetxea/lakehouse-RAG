//! Medición N1 de la extracción de PDF, sin Claude (`docs/arquitectura/ssd_generalidad_4_piezas.md`
//! §6.2). Para cada PDF de la carpeta pasa el `PdfParser` de la app y escribe un JSON con el
//! documento parseado entero (secciones, saltos de página y, si existen, las páginas degradadas)
//! más el tiempo de parseo. La comparación antes y después la hace
//! `benchmarks/legibilidad/comparar.py`.
//!
//! Uso: `cargo run --release --example medir_extraccion -- <carpeta_pdf> <carpeta_salida>`
//!
//! La salida contiene texto de documentos reales: debe ir a una carpeta ignorada por git.

use std::path::{Path, PathBuf};
use std::time::Instant;

use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::infrastructure::parsers::PdfParser;
use sha2::{Digest, Sha256};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    let [carpeta_pdf, carpeta_salida] = argumentos.as_slice() else {
        anyhow::bail!("uso: medir_extraccion <carpeta_pdf> <carpeta_salida>");
    };
    std::fs::create_dir_all(carpeta_salida)?;
    let parser = PdfParser::new();
    for ruta in pdfs_de(Path::new(carpeta_pdf))? {
        let bytes = std::fs::read(&ruta)?;
        let nombre = ruta
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let inicio = Instant::now();
        let resultado = parser.parse_bytes(&nombre, &bytes).await;
        let segundos = inicio.elapsed().as_secs_f64();
        let medicion = serde_json::json!({
            "archivo": nombre,
            "sha256": format!("{:x}", Sha256::digest(&bytes)),
            "segundos": segundos,
            "documento": resultado.as_ref().ok(),
            "error": resultado.as_ref().err().map(ToString::to_string),
        });
        let destino = Path::new(carpeta_salida).join(format!("{nombre}.json"));
        std::fs::write(&destino, serde_json::to_vec(&medicion)?)?;
        println!(
            "{nombre}: {segundos:.2} s, {}",
            if resultado.is_ok() { "ok" } else { "error" }
        );
    }
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
