//! Pieza 1 (`docs/arquitectura/ssd_generalidad_4_piezas.md` §1): extracción con pdfium en un
//! proceso hijo, compuerta de legibilidad por página y estado `LECTURA_DEGRADADA` en la ingesta.
//! Todos los PDF son sintéticos (lopdf); el texto se redacta en el test.

#[path = "comun_ingesta_universal/mod.rs"]
mod comun;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;
#[path = "comun_pdf/fuentes.rs"]
mod fuentes;

use std::sync::Arc;

use arrow_array::{Array, Int32Array, ListArray, RecordBatch, StringArray};
use comun::*;
use comun_pdf::*;
use fuentes::*;
use lopdf::Object;

use lakehouse_rag::application::dtos::ingestion::{MotivoDegradacion, ParsedDocument};
use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, LakehouseStorePort, TABLA_CHUNKS_NORMATIVA, TABLA_DOCUMENTOS_CATALOGO,
    TABLA_INDICE_JERARQUICO,
};
use lakehouse_rag::application::use_cases::document_ingestion_service::DocumentIngestionService;
use lakehouse_rag::infrastructure::parsers::{ParserPorFormato, PdfParser};

const RELLENO: &str = "Texto corriente de relleno para superar el umbral de caracteres por pagina.";

async fn parsear(bytes: &[u8]) -> ParsedDocument {
    PdfParser::new()
        .parse_bytes("doc.pdf", bytes)
        .await
        .unwrap()
}

/// Líneas de prosa redactada para el test: unas 120 palabras evaluables
fn prosa(semilla: &str) -> Vec<String> {
    (0..12)
        .map(|i| {
            format!(
                "{semilla} {i}: la comision revisara cada solicitud antes de aprobar el \
                 presupuesto anual y publicara una nota breve"
            )
        })
        .collect()
}

/// La misma prosa con las vocales sustituidas, como una fuente con la codificación cambiada
fn prosa_cifrada() -> Vec<String> {
    prosa("Apartado")
        .into_iter()
        .map(|linea| {
            linea
                .chars()
                .map(|c| match c {
                    'a' => 'q',
                    'e' => 'x',
                    'i' => 'z',
                    'o' => 'k',
                    'u' => 'w',
                    otro => otro,
                })
                .collect()
        })
        .collect()
}

fn como_str(lineas: &[String]) -> Vec<&str> {
    lineas.iter().map(String::as_str).collect()
}

#[tokio::test]
async fn test_pdfium_aplica_differences_con_nombres_uni_afii_y_ligaduras() {
    let bytes = pdf_con_diferencias(
        vec![
            65.into(),
            Object::Name(b"uni0394".to_vec()),
            Object::Name(b"afii10017".to_vec()),
            Object::Name(b"fi".to_vec()),
        ],
        &[b"ABC", b"ABCABC"],
    );
    let documento = parsear(&bytes).await;
    let texto = &documento.contenido_completo;
    assert!(texto.contains('Δ'), "{texto:?}");
    assert!(texto.contains('А'), "{texto:?}");
    assert!(texto.contains("fi") || texto.contains('ﬁ'), "{texto:?}");
    assert!(
        !texto.contains("ABC"),
        "las letras de la base no deben quedar: {texto:?}"
    );
}

#[tokio::test]
async fn test_pdfium_usa_el_to_unicode_de_la_fuente() {
    let bytes = pdf_con_to_unicode(&[(b'a', 'α'), (b'b', 'β'), (b'c', 'γ')], &[b"abc cab"]);
    let documento = parsear(&bytes).await;
    assert!(
        documento.contenido_completo.contains("αβγ γαβ"),
        "{:?}",
        documento.contenido_completo
    );
}

#[tokio::test]
async fn test_el_guion_de_corte_de_pdfium_no_es_un_control() {
    let bytes = pdf(&[&[RELLENO, "una palabra par-", "tida al final de la linea"]]);
    let documento = parsear(&bytes).await;
    assert!(documento.contenido_completo.contains("par-"));
    assert!(documento.paginas_degradadas.is_empty());
}

#[tokio::test]
async fn test_una_pagina_con_letras_sustituidas_se_marca_degradada_y_las_demas_no() {
    let legible = prosa("Apartado");
    let cifrada = prosa_cifrada();
    let bytes = pdf(&[
        &como_str(&legible),
        &como_str(&cifrada),
        &como_str(&legible),
    ]);
    let documento = parsear(&bytes).await;
    let paginas: Vec<u32> = documento
        .paginas_degradadas
        .iter()
        .map(|p| p.pagina)
        .collect();
    assert_eq!(paginas, [2]);
    assert!(matches!(
        documento.paginas_degradadas[0].motivos.as_slice(),
        [MotivoDegradacion::Mojibake { .. }]
    ));
    assert!(documento.paginas_sin_texto.is_empty());
}

#[tokio::test]
async fn test_una_pagina_casi_vacia_se_informa_sin_texto_y_no_degradada() {
    let legible = prosa("Apartado");
    let bytes = pdf(&[&como_str(&legible), &["Pagina 2"], &como_str(&legible)]);
    let documento = parsear(&bytes).await;
    assert_eq!(documento.paginas_sin_texto, [2]);
    assert!(documento.paginas_degradadas.is_empty());
    assert!(
        documento
            .avisos_extraccion
            .iter()
            .any(|a| a.contains("sin texto") && a.contains('2')),
        "{:?}",
        documento.avisos_extraccion
    );
}

fn enteros(lotes: &[RecordBatch], nombre: &str) -> Vec<i32> {
    lotes
        .iter()
        .flat_map(|lote| {
            let columna = lote
                .column_by_name(nombre)
                .unwrap()
                .as_any()
                .downcast_ref::<Int32Array>()
                .unwrap();
            (0..columna.len())
                .map(|i| columna.value(i))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn cadenas(lotes: &[RecordBatch], nombre: &str) -> Vec<String> {
    lotes
        .iter()
        .flat_map(|lote| {
            let columna = lote
                .column_by_name(nombre)
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            (0..columna.len())
                .map(|i| columna.value(i).to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[tokio::test]
async fn test_la_ingesta_marca_lectura_degradada_solo_en_las_filas_de_la_pagina_degradada() {
    let (_dir, store) = almacen().await;
    let destilador = DestiladorFijo::nuevo(paquete("PO-PDF-07", 1));
    let servicio = DocumentIngestionService::new(
        Arc::new(ParserPorFormato::new()),
        destilador.clone(),
        motor_mock(),
        store.clone(),
    );
    let mut uno = vec!["1. ALCANCE".to_string()];
    uno.extend(prosa("Alcance"));
    let mut dos = vec!["2. RESPONSABLES".to_string()];
    dos.extend(prosa_cifrada());
    let mut tres = vec!["3. REGISTROS".to_string()];
    tres.extend(prosa("Registros"));
    let bytes = pdf(&[&como_str(&uno), &como_str(&dos), &como_str(&tres)]);

    let informe = servicio
        .ingestar_bytes(&centro(), "manual.pdf", &bytes)
        .await
        .unwrap();
    assert_eq!(informe.estado_contenido, "LECTURA_DEGRADADA");
    assert!(!informe.pendiente_destilacion);
    assert!(
        informe
            .avisos
            .iter()
            .any(|a| a.contains("Página 2") && a.contains("degradad")),
        "{:?}",
        informe.avisos
    );

    let chunks = store
        .leer_tabla(TABLA_CHUNKS_NORMATIVA, &ConsultaTabla::confirmadas(100))
        .await
        .unwrap();
    let por_pagina: Vec<(i32, String)> = enteros(&chunks, "pagina_inicio")
        .into_iter()
        .zip(cadenas(&chunks, "estado_contenido"))
        .collect();
    assert_eq!(
        por_pagina,
        [
            (1, "COMPLETO".to_string()),
            (2, "LECTURA_DEGRADADA".to_string()),
            (3, "COMPLETO".to_string())
        ]
    );
    let nodos = textos(&store, TABLA_INDICE_JERARQUICO, "estado_contenido").await;
    assert_eq!(
        nodos.iter().filter(|e| *e == "LECTURA_DEGRADADA").count(),
        1
    );

    let catalogo = store
        .leer_tabla(TABLA_DOCUMENTOS_CATALOGO, &ConsultaTabla::confirmadas(10))
        .await
        .unwrap();
    let lista = catalogo[0]
        .column_by_name("paginas_degradadas")
        .unwrap()
        .as_any()
        .downcast_ref::<ListArray>()
        .unwrap()
        .value(0);
    let paginas = lista.as_any().downcast_ref::<Int32Array>().unwrap();
    assert_eq!(paginas.values().to_vec(), [2]);
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_un_documento_legible_sigue_completo_y_sin_paginas_degradadas_en_el_catalogo() {
    let (_dir, store) = almacen().await;
    let servicio = DocumentIngestionService::new(
        Arc::new(ParserPorFormato::new()),
        DestiladorFijo::nuevo(paquete("PO-PDF-08", 1)),
        motor_mock(),
        store.clone(),
    );
    let legible = prosa("Apartado");
    let bytes = pdf(&[&como_str(&legible), &como_str(&legible)]);
    let informe = servicio
        .ingestar_bytes(&centro(), "limpio.pdf", &bytes)
        .await
        .unwrap();
    assert_eq!(informe.estado_contenido, "COMPLETO");
    assert!(
        !informe.avisos.iter().any(|a| a.contains("degradad")),
        "sin avisos falsos: {:?}",
        informe.avisos
    );
    let catalogo = store
        .leer_tabla(TABLA_DOCUMENTOS_CATALOGO, &ConsultaTabla::confirmadas(10))
        .await
        .unwrap();
    assert!(
        catalogo[0]
            .column_by_name("paginas_degradadas")
            .unwrap()
            .is_null(0)
    );
}
