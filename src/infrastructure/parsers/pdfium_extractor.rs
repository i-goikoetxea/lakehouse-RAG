//! Lado del proceso hijo de la extracción de PDF con pdfium (`ssd_generalidad_4_piezas.md` §1.2).
//!
//! `lakehouse_rag_cli --extraer-texto-pdf` lee el PDF por stdin y escribe por stdout un JSON con el
//! texto crudo de cada página. pdfium es C++: un PDF hostil que explote un fallo solo compromete
//! este proceso, que no tiene acceso al estado de la app. El padre (`pdf_parser.rs`) limita su
//! tiempo y su memoria.
//!
//! - La DLL se carga solo por **ruta absoluta** (junto al ejecutable o en `LAKEHOUSE_RAG_PDFIUM`),
//!   nunca por la ruta de búsqueda del sistema, y solo si su sha256 es el fijado aquí.
//! - La versión es la de `pdfium-binaries` fijada en `VERSION_PDFIUM`; la descarga
//!   `scripts/obtener_pdfium.ps1`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Argumento oculto del binario de consola que arranca el proceso hijo
pub const ARGUMENTO_EXTRAER_TEXTO_PDF: &str = "--extraer-texto-pdf";
/// Ruta absoluta alternativa de la DLL (por ejemplo, para los tests)
pub const VARIABLE_RUTA_PDFIUM: &str = "LAKEHOUSE_RAG_PDFIUM";
/// Publicación de `bblanchon/pdfium-binaries`, la misma que los bindings `pdfium_8125`
pub const VERSION_PDFIUM: &str = "chromium/8125";
pub const NOMBRE_BIBLIOTECA_PDFIUM: &str = "pdfium.dll";
/// sha256 de `bin/pdfium.dll` de `pdfium-win-x64.tgz` (`VERSION_PDFIUM`)
pub const SHA256_PDFIUM: &str = "4b0dec92555d69437bef0df37be630181ab3e83a6823ca264ede355a4f2ef994";
/// Un PDF más grande se rechaza antes de leerlo entero
pub const MAX_BYTES_PDF: u64 = 1 << 30;
pub const INSTRUCCION_OBTENER_PDFIUM: &str =
    "ejecuta scripts/obtener_pdfium.ps1 y vuelve a compilar, o fija LAKEHOUSE_RAG_PDFIUM";

/// Respuesta del proceso hijo, en una sola línea JSON por stdout
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RespuestaExtraccion {
    Paginas(Vec<PaginaExtraida>),
    Error(String),
}

/// Texto crudo de una página (base 1); `None` si pdfium no pudo leerla
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaginaExtraida {
    pub pagina: u32,
    pub texto: Option<String>,
}

/// Punto de entrada del subcomando oculto. Devuelve el código de salida del proceso
pub fn ejecutar_extraccion_por_stdio() -> i32 {
    let respuesta = match leer_pdf_de_stdin() {
        Ok(bytes) => extraer_con_pdfium(&bytes),
        Err(motivo) => RespuestaExtraccion::Error(motivo),
    };
    let codigo = i32::from(matches!(respuesta, RespuestaExtraccion::Error(_)));
    let linea = serde_json::to_string(&respuesta)
        .unwrap_or_else(|_| r#"{"error":"no se pudo serializar la respuesta"}"#.to_string());
    let mut salida = std::io::stdout().lock();
    if writeln!(salida, "{linea}")
        .and_then(|()| salida.flush())
        .is_err()
    {
        return 2;
    }
    codigo
}

fn leer_pdf_de_stdin() -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_BYTES_PDF + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("no se pudo leer el PDF: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES_PDF {
        return Err("el PDF supera el tamaño máximo".to_string());
    }
    Ok(bytes)
}

/// Ruta absoluta de la DLL: `LAKEHOUSE_RAG_PDFIUM` o junto al ejecutable. Nunca un nombre suelto,
/// que Windows buscaría en la ruta de búsqueda del sistema
pub fn ruta_biblioteca_pdfium() -> Result<PathBuf, String> {
    let ruta = match std::env::var_os(VARIABLE_RUTA_PDFIUM) {
        Some(valor) if !valor.is_empty() => PathBuf::from(valor),
        _ => std::env::current_exe()
            .map_err(|e| format!("no se pudo localizar el ejecutable: {e}"))?
            .parent()
            .ok_or("el ejecutable no tiene carpeta")?
            .join(NOMBRE_BIBLIOTECA_PDFIUM),
    };
    if !ruta.is_absolute() {
        return Err(format!(
            "{VARIABLE_RUTA_PDFIUM} debe ser una ruta absoluta: {}",
            ruta.display()
        ));
    }
    Ok(ruta)
}

/// Comprueba que la DLL existe y que su sha256 es el fijado
pub fn verificar_biblioteca(ruta: &Path) -> Result<(), String> {
    let bytes = std::fs::read(ruta).map_err(|_| {
        format!(
            "falta la biblioteca pdfium ({VERSION_PDFIUM}) en {}: {INSTRUCCION_OBTENER_PDFIUM}",
            ruta.display()
        )
    })?;
    let huella = format!("{:x}", Sha256::digest(&bytes));
    if huella != SHA256_PDFIUM {
        return Err(format!(
            "la biblioteca pdfium de {} no es la versión fijada {VERSION_PDFIUM} (sha256 {huella}): \
             {INSTRUCCION_OBTENER_PDFIUM}",
            ruta.display()
        ));
    }
    Ok(())
}

fn extraer_con_pdfium(bytes: &[u8]) -> RespuestaExtraccion {
    use pdfium_render::prelude::Pdfium;

    let ruta = match ruta_biblioteca_pdfium() {
        Ok(ruta) => ruta,
        Err(motivo) => return RespuestaExtraccion::Error(motivo),
    };
    if let Err(motivo) = verificar_biblioteca(&ruta) {
        return RespuestaExtraccion::Error(motivo);
    }
    let enlaces = match Pdfium::bind_to_library(&ruta) {
        Ok(enlaces) => enlaces,
        Err(e) => {
            return RespuestaExtraccion::Error(format!(
                "no se pudo cargar pdfium de {}: {e}",
                ruta.display()
            ));
        }
    };
    let pdfium = Pdfium::new(enlaces);
    let documento = match pdfium.load_pdf_from_byte_slice(bytes, None) {
        Ok(documento) => documento,
        Err(e) => return RespuestaExtraccion::Error(format!("pdfium no pudo abrirlo ({e})")),
    };
    let paginas = documento.pages();
    let total = paginas.len().max(0);
    RespuestaExtraccion::Paginas(
        (0..total)
            .map(|indice| PaginaExtraida {
                pagina: u32::try_from(indice + 1).unwrap_or(u32::MAX),
                texto: paginas
                    .get(indice)
                    .and_then(|pagina| pagina.text().map(|texto| texto.all()))
                    .ok()
                    .map(|texto| traducir_artefactos_pdfium(&texto)),
            })
            .collect(),
    )
}

/// Convenciones propias de la salida de pdfium, que no son texto del documento:
/// - los saltos de línea son `\r\n`;
/// - un guion de corte de palabra sale como `U+0002`, y pdfium une a esa línea la siguiente.
///   Se restauran el guion y el salto, como en la página: si no, un encabezado numerado seguido
///   de un párrafo cortado se convierte en una línea larga que ya no parece un encabezado.
///
/// El resto de controles se conserva: la compuerta de legibilidad los mide.
pub fn traducir_artefactos_pdfium(texto: &str) -> String {
    texto
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace("\u{0002}\n", "\u{0002}")
        .replace('\u{0002}', "-\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_los_saltos_de_pdfium_pasan_a_saltos_de_linea() {
        assert_eq!(
            traducir_artefactos_pdfium("uno\r\ndos\rtres\n"),
            "uno\ndos\ntres\n"
        );
    }

    #[test]
    fn test_el_guion_de_corte_de_pdfium_vuelve_a_ser_un_guion_al_final_de_su_linea() {
        // pdfium une la línea siguiente a la cortada: se restaura el salto, como en la página
        assert_eq!(
            traducir_artefactos_pdfium("8.2 Theorem. fol\u{0002}lowing property.\r\nNext"),
            "8.2 Theorem. fol-\nlowing property.\nNext"
        );
        assert_eq!(
            traducir_artefactos_pdfium("subse\u{0002}\r\nquent"),
            "subse-\nquent"
        );
    }

    #[test]
    fn test_los_demas_controles_se_conservan_para_la_compuerta() {
        assert_eq!(
            traducir_artefactos_pdfium("de\u{000C}ne\u{FFFD}"),
            "de\u{000C}ne\u{FFFD}"
        );
    }

    #[test]
    fn test_una_dll_inexistente_da_un_error_accionable() {
        let motivo = verificar_biblioteca(Path::new("C:/no/existe/pdfium.dll")).unwrap_err();
        assert!(motivo.contains("obtener_pdfium.ps1"), "{motivo}");
        assert!(motivo.contains(VERSION_PDFIUM), "{motivo}");
    }

    #[test]
    fn test_una_dll_con_otro_sha256_se_rechaza() {
        let carpeta = tempfile::tempdir().unwrap();
        let falsa = carpeta.path().join(NOMBRE_BIBLIOTECA_PDFIUM);
        std::fs::write(&falsa, b"MZ no es pdfium").unwrap();
        let motivo = verificar_biblioteca(&falsa).unwrap_err();
        assert!(motivo.contains("no es la versión fijada"), "{motivo}");
    }

    #[test]
    fn test_la_respuesta_del_hijo_viaja_en_json() {
        let respuesta = RespuestaExtraccion::Paginas(vec![
            PaginaExtraida {
                pagina: 1,
                texto: Some("α\u{000C}".to_string()),
            },
            PaginaExtraida {
                pagina: 2,
                texto: None,
            },
        ]);
        let linea = serde_json::to_string(&respuesta).unwrap();
        assert_eq!(
            serde_json::from_str::<RespuestaExtraccion>(&linea).unwrap(),
            respuesta
        );
        let error: RespuestaExtraccion = serde_json::from_str(r#"{"error":"x"}"#).unwrap();
        assert_eq!(error, RespuestaExtraccion::Error("x".to_string()));
    }
}
