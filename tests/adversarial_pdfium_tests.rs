//! Pieza 1, adversarial y de seguridad (`ssd_generalidad_4_piezas.md` §1.6): el proceso hijo se
//! cuelga o muere, la DLL falta o no es la fijada, una DLL falsa en la ruta de búsqueda y el
//! límite de memoria del job object.

#[path = "comun_pdf/mod.rs"]
mod comun_pdf;

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use comun_pdf::*;

use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::infrastructure::parsers::PdfParser;
use lakehouse_rag::infrastructure::parsers::pdfium_extractor::{
    ARGUMENTO_EXTRAER_TEXTO_PDF, RespuestaExtraccion, VARIABLE_RUTA_PDFIUM,
};
use lakehouse_rag::infrastructure::parsers::proceso_aislado::ProcesoExtractor;

const EXTRACTOR: &str = env!("CARGO_BIN_EXE_lakehouse_rag_cli");
const RELLENO: &str = "Texto corriente de relleno para superar el umbral de caracteres por pagina.";

/// Intérprete de órdenes de Windows por ruta absoluta, para simular hijos que fallan
fn cmd() -> String {
    std::env::var("ComSpec").unwrap_or_else(|_| "C:\\Windows\\System32\\cmd.exe".to_string())
}

fn pdf_corriente() -> Vec<u8> {
    pdf(&[&[RELLENO, RELLENO], &[RELLENO]])
}

/// Ejecuta el hijo real con el PDF por stdin y un entorno controlado
fn ejecutar_hijo(pdf: &[u8], configurar: impl FnOnce(&mut Command)) -> RespuestaExtraccion {
    let mut comando = Command::new(EXTRACTOR);
    comando
        .arg(ARGUMENTO_EXTRAER_TEXTO_PDF)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configurar(&mut comando);
    let mut hijo = comando.spawn().unwrap();
    hijo.stdin.take().unwrap().write_all(pdf).unwrap();
    let salida = hijo.wait_with_output().unwrap();
    serde_json::from_slice(&salida.stdout).unwrap()
}

#[tokio::test]
async fn test_el_hijo_colgado_se_mata_al_agotar_el_tiempo_y_la_ingesta_falla_con_mensaje() {
    let colgado = ProcesoExtractor::con_comando(cmd(), ["/c", "ping -n 60 127.0.0.1 >NUL"])
        .con_tiempo_maximo(Duration::from_secs(2));
    let inicio = Instant::now();
    let error = PdfParser::con_extractor(colgado)
        .parse_bytes("colgado.pdf", &pdf_corriente())
        .await
        .unwrap_err()
        .to_string();
    assert!(inicio.elapsed() < Duration::from_secs(20));
    assert!(error.contains("tiempo máximo"), "{error}");
}

#[tokio::test]
async fn test_el_hijo_que_muere_de_golpe_da_un_error_controlado() {
    let muerto = ProcesoExtractor::con_comando(cmd(), ["/c", "exit 3"]);
    let error = PdfParser::con_extractor(muerto)
        .parse_bytes("muerto.pdf", &pdf_corriente())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("anómala") && error.contains('3'), "{error}");
}

#[tokio::test]
async fn test_el_hijo_que_escribe_basura_da_un_error_controlado() {
    let basura = ProcesoExtractor::con_comando(cmd(), ["/c", "echo {\"paginas\":[{"]);
    let error = PdfParser::con_extractor(basura)
        .parse_bytes("basura.pdf", &pdf_corriente())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("anómala"), "{error}");
}

#[tokio::test]
async fn test_sin_extractor_la_ingesta_falla_con_un_mensaje_accionable() {
    let ausente = ProcesoExtractor::con_comando("C:/no/existe/lakehouse_rag_cli.exe", ["x"]);
    let error = PdfParser::con_extractor(ausente)
        .parse_bytes("doc.pdf", &pdf_corriente())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("falta el extractor"), "{error}");
}

#[tokio::test]
async fn test_el_limite_de_memoria_del_job_object_detiene_al_hijo() {
    let estrecho = ProcesoExtractor::junto_al_ejecutable().con_limite_memoria(8 << 20);
    let error = PdfParser::con_extractor(estrecho)
        .parse_bytes("doc.pdf", &pdf_corriente())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("PDF"), "{error}");
    // Con el límite por defecto, el mismo PDF se lee
    let documento = PdfParser::new()
        .parse_bytes("doc.pdf", &pdf_corriente())
        .await
        .unwrap();
    assert!(documento.contenido_completo.contains("relleno"));
}

#[test]
fn test_una_dll_inexistente_da_un_error_accionable() {
    let respuesta = ejecutar_hijo(&pdf_corriente(), |c| {
        c.env(VARIABLE_RUTA_PDFIUM, "C:\\no\\existe\\pdfium.dll");
    });
    let RespuestaExtraccion::Error(motivo) = respuesta else {
        panic!("se esperaba un error");
    };
    assert!(motivo.contains("obtener_pdfium.ps1"), "{motivo}");
}

#[test]
fn test_una_dll_con_otro_sha256_no_se_carga() {
    let carpeta = tempfile::tempdir().unwrap();
    let falsa = carpeta.path().join("pdfium.dll");
    std::fs::write(&falsa, b"MZ esto no es pdfium").unwrap();
    let respuesta = ejecutar_hijo(&pdf_corriente(), |c| {
        c.env(VARIABLE_RUTA_PDFIUM, &falsa);
    });
    let RespuestaExtraccion::Error(motivo) = respuesta else {
        panic!("se esperaba un error");
    };
    assert!(motivo.contains("no es la versión fijada"), "{motivo}");
}

#[test]
fn test_una_ruta_relativa_de_la_dll_se_rechaza() {
    let respuesta = ejecutar_hijo(&pdf_corriente(), |c| {
        c.env(VARIABLE_RUTA_PDFIUM, "pdfium.dll");
    });
    let RespuestaExtraccion::Error(motivo) = respuesta else {
        panic!("se esperaba un error");
    };
    assert!(motivo.contains("ruta absoluta"), "{motivo}");
}

#[test]
fn test_una_dll_falsa_en_la_ruta_de_busqueda_y_en_la_carpeta_actual_no_se_carga() {
    let carpeta = tempfile::tempdir().unwrap();
    std::fs::write(carpeta.path().join("pdfium.dll"), b"MZ trampa").unwrap();
    let ruta_busqueda = format!(
        "{};{}",
        carpeta.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let respuesta = ejecutar_hijo(&pdf_corriente(), |c| {
        c.env_remove(VARIABLE_RUTA_PDFIUM)
            .env("PATH", ruta_busqueda)
            .current_dir(carpeta.path());
    });
    let RespuestaExtraccion::Paginas(paginas) = respuesta else {
        panic!("la DLL fijada junto al ejecutable debía cargarse: {respuesta:?}");
    };
    assert_eq!(paginas.len(), 2);
}

#[test]
fn test_bytes_que_no_son_un_pdf_dan_error_en_el_hijo_sin_pánico() {
    let respuesta = ejecutar_hijo(b"%PDF-1.7 basura sin estructura", |_| {});
    assert!(matches!(respuesta, RespuestaExtraccion::Error(_)));
}
