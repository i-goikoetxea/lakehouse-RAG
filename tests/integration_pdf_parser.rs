//! Extractor de PDF digital (STAIR Fase 3): árbol por marcadores o numeración, cobertura sin
//! pérdidas, `REQUIERE_OCR` y paso por el servicio de ingesta.

#[path = "comun_ingesta_universal/mod.rs"]
mod comun;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;

use std::sync::Arc;

use arrow_array::{Array, Int32Array};
use comun::*;
use comun_pdf::*;

use lakehouse_rag::application::dtos::centros::DocumentoOriginal;
use lakehouse_rag::application::dtos::ingestion::ParsedDocument;
use lakehouse_rag::application::ports::center_workspace::CenterWorkspacePort;
use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, LakehouseStorePort, TABLA_CHUNKS_NORMATIVA, TABLA_DOCUMENTOS_CATALOGO,
    TABLA_INDICE_JERARQUICO,
};
use lakehouse_rag::application::use_cases::document_ingestion_service::DocumentIngestionService;
use lakehouse_rag::infrastructure::parsers::{ParserPorFormato, PdfParser};
use lakehouse_rag::infrastructure::storage::center_workspace_disk::DiskCenterWorkspace;
use sha2::{Digest, Sha256};

const RELLENO: &str = "Texto corriente de relleno para superar el umbral de caracteres por pagina.";

async fn parsear(bytes: &[u8]) -> ParsedDocument {
    PdfParser::new()
        .parse_bytes("doc.pdf", bytes)
        .await
        .unwrap()
}

fn resumen(documento: &ParsedDocument) -> Vec<(String, String, u8)> {
    documento
        .secciones
        .iter()
        .map(|s| (s.codigo.clone(), s.titulo.clone(), s.nivel))
        .collect()
}

fn tripleta(codigo: &str, titulo: &str, nivel: u8) -> (String, String, u8) {
    (codigo.to_string(), titulo.to_string(), nivel)
}

fn exigir_sin_perdidas(documento: &ParsedDocument, palabras_esperadas: &str) {
    let corpus: String = documento
        .secciones
        .iter()
        .map(|s| format!("{} {} {}", s.breadcrumb, s.titulo, s.contenido))
        .collect::<Vec<_>>()
        .join(" ");
    let disponibles: std::collections::HashSet<String> = palabras(&corpus).into_iter().collect();
    for palabra in palabras(palabras_esperadas) {
        assert!(disponibles.contains(&palabra), "se perdió '{palabra}'");
    }
}

#[tokio::test]
async fn test_marcadores_dan_jerarquia_y_paginas() {
    let bytes = con_marcadores(
        &[
            &["Portada del manual", RELLENO],
            &["Capitulo Uno", "Texto del uno", RELLENO],
            &["Apartado Dos", "Texto del apartado", RELLENO],
        ],
        &[marcador(
            "Capitulo Uno",
            2,
            vec![marcador("Apartado Dos", 3, vec![])],
        )],
    );
    let documento = parsear(&bytes).await;
    let titulos: Vec<(String, u8)> = documento
        .secciones
        .iter()
        .map(|s| (s.titulo.clone(), s.nivel))
        .collect();
    assert_eq!(titulos[1], ("Capitulo Uno".to_string(), 1));
    assert_eq!(titulos[2], ("Apartado Dos".to_string(), 2));
    assert_eq!(documento.secciones[0].nivel, 0, "la portada es preámbulo");
    assert!(documento.secciones[0].contenido.contains("Portada"));
    assert_eq!(documento.secciones[1].pagina_inicio, Some(2));
    assert_eq!(documento.secciones[2].pagina_fin, Some(3));
    assert!(documento.secciones[2].breadcrumb.contains("Capitulo Uno"));
    assert!(documento.secciones[1].contenido.contains("Texto del uno"));
    assert!(!documento.requiere_ocr);
}

#[tokio::test]
async fn test_marcador_a_media_pagina_corta_en_su_linea_sin_perder_texto() {
    let bytes = con_marcadores(
        &[&[
            "Introduccion",
            "Antes del corte",
            "Seccion Central",
            "Despues del corte",
            RELLENO,
        ]],
        &[
            marcador("Introduccion", 1, vec![]),
            marcador("Seccion Central", 1, vec![]),
        ],
    );
    let documento = parsear(&bytes).await;
    assert_eq!(documento.secciones.len(), 2);
    assert!(documento.secciones[0].contenido.contains("Antes del corte"));
    assert!(
        documento.secciones[1]
            .contenido
            .contains("Despues del corte")
    );
    assert!(!documento.secciones[1].contenido.contains("Antes del corte"));
}

#[tokio::test]
async fn test_marcador_ausente_del_texto_abre_al_inicio_de_su_pagina() {
    let bytes = con_marcadores(
        &[
            &["Primera pagina", RELLENO],
            &["Linea previa de la dos", RELLENO],
        ],
        &[
            marcador("Primera pagina", 1, vec![]),
            marcador("Titulo que no aparece", 2, vec![]),
        ],
    );
    let documento = parsear(&bytes).await;
    let ultima = documento.secciones.last().unwrap();
    assert_eq!(ultima.titulo, "Titulo que no aparece");
    assert_eq!(ultima.pagina_inicio, Some(2));
    assert!(ultima.contenido.contains("Linea previa de la dos"));
}

#[tokio::test]
async fn test_sin_marcadores_detecta_numeracion_creciente() {
    let bytes = pdf(&[
        &[
            "Texto inicial del documento",
            "1. ALCANCE",
            RELLENO,
            "1.1 Objeto",
            RELLENO,
        ],
        &["1.2 Limites", RELLENO, "2. RESPONSABLES", RELLENO],
    ]);
    let documento = parsear(&bytes).await;
    assert_eq!(
        resumen(&documento)[1..],
        [
            tripleta("1", "ALCANCE", 1),
            tripleta("1.1", "Objeto", 2),
            tripleta("1.2", "Limites", 2),
            tripleta("2", "RESPONSABLES", 1),
        ]
    );
    assert_eq!(documento.secciones[0].nivel, 0);
    assert_eq!(documento.secciones[3].pagina_inicio, Some(2));
}

#[tokio::test]
async fn test_sin_encabezados_da_un_unico_contenido_general() {
    let bytes = pdf(&[
        &["Solo parrafos sin numerar", RELLENO],
        &["Mas texto corrido", RELLENO],
    ]);
    let documento = parsear(&bytes).await;
    assert_eq!(
        resumen(&documento),
        [tripleta("cuerpo", "Contenido General", 1)]
    );
    assert_eq!(documento.secciones[0].pagina_inicio, Some(1));
    assert_eq!(documento.secciones[0].pagina_fin, Some(2));
    exigir_sin_perdidas(&documento, "Solo parrafos sin numerar Mas texto corrido");
}

#[tokio::test]
async fn test_casi_vacio_requiere_ocr_e_indica_marcador() {
    let documento = parsear(&pdf(&[&["ab"], &[]])).await;
    assert!(documento.requiere_ocr);
    assert_eq!(documento.secciones.len(), 1);
    assert_eq!(documento.secciones[0].titulo, "Contenido General");
    assert!(documento.secciones[0].contenido.contains("ab"));
    assert!(!documento.avisos_extraccion.is_empty());

    let sin_texto = parsear(&pdf(&[&[], &[]])).await;
    assert!(sin_texto.requiere_ocr);
    assert!(
        sin_texto.secciones[0].contenido.contains("doc.pdf"),
        "sin texto queda localizable por su nombre"
    );
}

#[tokio::test]
async fn test_cobertura_sin_perdidas_en_todas_las_estrategias() {
    let lineas: [&[&str]; 2] = [
        &[
            "Prologo Alfa",
            "1. PRIMERO",
            "Beta gamma delta",
            "1.1 Interior",
            "Epsilon",
        ],
        &[
            "Zeta eta",
            "Theta iota",
            "2. SEGUNDO",
            "Kappa lambda mu",
            RELLENO,
        ],
    ];
    let esperado = lineas.concat().join(" ");
    exigir_sin_perdidas(&parsear(&pdf(&lineas)).await, &esperado);
    exigir_sin_perdidas(
        &parsear(&con_marcadores(
            &lineas,
            &[
                marcador("PRIMERO", 1, vec![]),
                marcador("No esta", 2, vec![]),
            ],
        ))
        .await,
        &esperado,
    );
}

#[tokio::test]
async fn test_el_selector_elige_por_extension() {
    let parser = ParserPorFormato::new();
    let bytes = pdf(&[&["Contenido suficiente de un PDF", RELLENO]]);
    let documento = parser.parse_bytes("MANUAL.PDF", &bytes).await.unwrap();
    assert_eq!(documento.secciones[0].titulo, "Contenido General");
    assert_eq!(documento.titulo, "MANUAL");

    let markdown = parser
        .parse_bytes("a.md", "# Hola\ntexto".as_bytes())
        .await
        .unwrap();
    assert_eq!(markdown.titulo, "Hola");
    let error = parser
        .parse_bytes("a.md", &[0xFF, 0xFE, 0x00])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("UTF-8"));
}

#[tokio::test]
async fn test_guardar_original_conserva_los_bytes_del_pdf() {
    let dir = tempfile::tempdir().unwrap();
    let ruta = dir.path().join("Manual.pdf");
    let bytes = pdf(&[&["Texto", RELLENO]]);
    std::fs::write(&ruta, &bytes).unwrap();
    let centro = dir.path().join("centro");
    let DocumentoOriginal {
        contenido,
        sha256,
        ruta_copia,
        ..
    } = DiskCenterWorkspace::new()
        .guardar_original(&centro, &ruta)
        .await
        .unwrap();
    assert_eq!(contenido, bytes);
    assert_eq!(sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(std::fs::read(ruta_copia).unwrap(), bytes);

    let ruta_md = dir.path().join("binario.md");
    std::fs::write(&ruta_md, [0xFF, 0xFE, 0x00]).unwrap();
    assert!(
        DiskCenterWorkspace::new()
            .guardar_original(&centro, &ruta_md)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_ingesta_de_pdf_escribe_arbol_y_paginas_y_calcula_el_hash_sobre_los_bytes() {
    let (_dir, store) = almacen().await;
    let destilador = DestiladorFijo::nuevo(paquete("PO-PDF-01", 1));
    let servicio = DocumentIngestionService::new(
        Arc::new(ParserPorFormato::new()),
        destilador.clone(),
        motor_mock(),
        store.clone(),
    );
    let bytes = pdf(&[&["1. ALCANCE", RELLENO], &["2. RESPONSABLES", RELLENO]]);
    let informe = servicio
        .ingestar_bytes(&centro(), "manual.pdf", &bytes)
        .await
        .unwrap();
    assert_eq!(informe.estado_contenido, "COMPLETO");
    assert_eq!(informe.sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(destilador.total_llamadas(), 1);
    let nodos = textos(&store, TABLA_INDICE_JERARQUICO, "nodo_id").await;
    assert!(nodos.contains(&"PO-PDF-01#sec-2".to_string()));
    exigir_integridad(&store).await;

    let lotes = store
        .leer_tabla(TABLA_CHUNKS_NORMATIVA, &ConsultaTabla::confirmadas(100))
        .await
        .unwrap();
    let inicios: Vec<i32> = lotes
        .iter()
        .flat_map(|lote| {
            let columna = lote
                .column_by_name("pagina_inicio")
                .unwrap()
                .as_any()
                .downcast_ref::<Int32Array>()
                .unwrap();
            (0..columna.len())
                .map(|i| columna.value(i))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(inicios, [1, 2]);

    let repetido = servicio
        .ingestar_bytes(&centro(), "manual.pdf", &bytes)
        .await
        .unwrap();
    assert!(repetido.ya_ingerido);
}

#[tokio::test]
async fn test_pdf_sin_texto_se_indexa_como_requiere_ocr_sin_llamar_a_claude() {
    let (_dir, store) = almacen().await;
    let destilador = DestiladorFijo::nuevo(paquete("PO-PDF-01", 1));
    let servicio = DocumentIngestionService::new(
        Arc::new(ParserPorFormato::new()),
        destilador.clone(),
        motor_mock(),
        store.clone(),
    );
    let bytes = pdf(&[&[], &[]]);
    let informe = servicio
        .ingestar_bytes(&centro(), "escaneado.pdf", &bytes)
        .await
        .unwrap();
    assert_eq!(informe.estado_contenido, "REQUIERE_OCR");
    assert!(!informe.pendiente_destilacion);
    assert!(informe.avisos.iter().any(|a| a.contains("OCR")));
    assert_eq!(destilador.total_llamadas(), 0, "no se destila sin texto");
    assert_eq!(informe.total_nodos, 1);
    let estados = textos(&store, TABLA_CHUNKS_NORMATIVA, "estado_contenido").await;
    assert_eq!(estados, ["REQUIERE_OCR"]);
    assert_eq!(
        textos(&store, TABLA_INDICE_JERARQUICO, "nodo_id").await,
        [format!("{}#sec-cuerpo", informe.doc_id)]
    );
    assert_eq!(
        textos(&store, TABLA_DOCUMENTOS_CATALOGO, "doc_id")
            .await
            .len(),
        1
    );
    exigir_integridad(&store).await;

    let repetido = servicio
        .ingestar_bytes(&centro(), "escaneado.pdf", &bytes)
        .await
        .unwrap();
    assert!(repetido.ya_ingerido);
    assert_eq!(repetido.estado_contenido, "REQUIERE_OCR");
    assert!(!repetido.pendiente_destilacion);

    let reintento = servicio
        .reintentar_destilacion_bytes(&centro(), "escaneado.pdf", &bytes)
        .await
        .unwrap();
    assert_eq!(reintento.estado_contenido, "REQUIERE_OCR");
    assert_eq!(
        destilador.total_llamadas(),
        0,
        "el reintento tampoco destila"
    );
}

/// PDF reales, por variables de entorno (sin rutas fijas): `LAKEHOUSE_PDF_REALES` lista rutas
/// separadas por `;`. Muestra árbol y cobertura; `cargo test --test integration_pdf_parser
/// pdf_reales -- --ignored --nocapture`.
#[tokio::test]
#[ignore = "requiere PDF reales por variable de entorno"]
async fn test_pdf_reales_arbol_y_cobertura() {
    let rutas = std::env::var("LAKEHOUSE_PDF_REALES").expect("LAKEHOUSE_PDF_REALES");
    for ruta in rutas.split(';').filter(|r| !r.trim().is_empty()) {
        let nombre = std::path::Path::new(ruta)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(ruta).unwrap();
        let parser = PdfParser::new();
        match parser.parse_bytes(&nombre, &bytes).await {
            Err(error) => println!("\n## {nombre}\nERROR: {error}"),
            Ok(documento) => {
                let corpus: Vec<String> = documento
                    .secciones
                    .iter()
                    .map(|s| format!("{} {}", s.breadcrumb, s.contenido))
                    .collect();
                let disponibles: std::collections::HashSet<String> =
                    palabras(&corpus.join(" ")).into_iter().collect();
                let todas = palabras(&documento.contenido_completo);
                let cubiertas = todas.iter().filter(|p| disponibles.contains(*p)).count();
                let cobertura = 100.0 * cubiertas as f64 / todas.len().max(1) as f64;
                println!(
                    "\n## {nombre}\nsecciones={} ocr={} cobertura={cobertura:.2}% avisos={:?}",
                    documento.secciones.len(),
                    documento.requiere_ocr,
                    documento.avisos_extraccion
                );
                for s in documento.secciones.iter().take(60) {
                    let sangria = "  ".repeat(usize::from(s.nivel));
                    println!(
                        "{sangria}{} {} [p{:?}-{:?}] {} car.",
                        s.codigo,
                        s.titulo.chars().take(70).collect::<String>(),
                        s.pagina_inicio,
                        s.pagina_fin,
                        s.contenido.chars().count()
                    );
                }
                if !documento.requiere_ocr {
                    assert!(cobertura > 99.99, "{nombre}: cobertura {cobertura}");
                }
            }
        }
    }
}

/// Oráculo privado: `LAKEHOUSE_PDF_ORACULO` = ruta del PDF y `LAKEHOUSE_CODIGOS_ORACULO` = códigos
/// que deben tener nodo propio, separados por comas. Solo imprime códigos, nunca texto.
#[tokio::test]
#[ignore = "requiere un PDF real indicado por variable de entorno"]
async fn oraculo_de_apartados_con_pdf_privado() {
    let ruta = std::env::var("LAKEHOUSE_PDF_ORACULO").expect("LAKEHOUSE_PDF_ORACULO");
    let codigos = std::env::var("LAKEHOUSE_CODIGOS_ORACULO").expect("LAKEHOUSE_CODIGOS_ORACULO");
    let parsed = PdfParser::new()
        .parse_bytes("oraculo.pdf", &std::fs::read(ruta).unwrap())
        .await
        .unwrap();
    let presentes: Vec<&str> = parsed.secciones.iter().map(|s| s.codigo.as_str()).collect();
    println!("{} secciones", presentes.len());
    let faltan: Vec<&str> = codigos
        .split(',')
        .filter(|c| !presentes.contains(c))
        .collect();
    for codigo in &faltan {
        for linea in parsed.contenido_completo.lines() {
            let linea = linea.trim();
            if linea
                .split_whitespace()
                .next()
                .map(|f| f.trim_end_matches('.'))
                == Some(*codigo)
            {
                println!(
                    "{codigo}: {} palabras, {} car., inicial mayúscula={}, final={:?}",
                    linea.split_whitespace().count() - 1,
                    linea.chars().count(),
                    linea
                        .split_whitespace()
                        .nth(1)
                        .and_then(|p| p.chars().next())
                        .is_some_and(char::is_uppercase),
                    linea.chars().last().filter(|c| !c.is_alphanumeric()),
                );
            }
        }
    }
    assert!(faltan.is_empty(), "sin nodo propio: {faltan:?}");
}
