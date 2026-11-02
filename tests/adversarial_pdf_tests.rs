//! Adversarial del extractor de PDF (STAIR Fase 3): archivos hostiles, marcadores cíclicos y
//! numeración engañosa. Ningún caso puede provocar un pánico ni perder texto en silencio.

#[path = "comun_pdf/mod.rs"]
mod comun_pdf;

use comun_pdf::*;

use lakehouse_rag::application::dtos::ingestion::ParsedDocument;
use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::infrastructure::parsers::PdfParser;
use lakehouse_rag::infrastructure::parsers::pdf_estructura::estructurar_por_numeracion;
use lopdf::{EncryptionState, EncryptionVersion, Object, Permissions, dictionary};

const RELLENO: &str = "Texto corriente de relleno para superar el umbral de caracteres por pagina.";

async fn resultado(bytes: &[u8]) -> Result<ParsedDocument, DomainError> {
    PdfParser::new().parse_bytes("hostil.pdf", bytes).await
}

async fn exigir_error_con(bytes: &[u8], fragmento: &str) {
    let error = resultado(bytes)
        .await
        .expect_err("debía fallar")
        .to_string();
    assert!(error.contains(fragmento), "mensaje inesperado: {error}");
    assert!(
        error.contains("hostil.pdf"),
        "debe nombrar el archivo: {error}"
    );
}

#[tokio::test]
async fn test_cero_bytes_error_controlado() {
    exigir_error_con(&[], "vacío").await;
}

#[tokio::test]
async fn test_texto_renombrado_a_pdf_error_controlado() {
    exigir_error_con(b"# Soy un Markdown\nrenombrado", "no es un PDF").await;
}

#[tokio::test]
async fn test_corrupto_con_cabecera_valida_error_controlado() {
    exigir_error_con(b"%PDF-1.7\nbasura sin estructura \xFF\xFE", "corrupto").await;
}

#[tokio::test]
async fn test_truncado_en_cualquier_punto_nunca_entra_en_panico() {
    let bytes = pdf(&[&["1. ALCANCE", RELLENO], &["2. OTRO", RELLENO]]);
    for corte in (0..bytes.len()).step_by(bytes.len() / 40 + 1) {
        // Puede fallar o recuperar algo; lo inaceptable es un pánico sin capturar
        let _ = resultado(&bytes[..corte]).await;
    }
}

#[tokio::test]
async fn test_binario_aleatorio_tras_la_cabecera_error_controlado() {
    let mut bytes = b"%PDF-1.4\n".to_vec();
    bytes.extend((0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8));
    assert!(resultado(&bytes).await.is_err());
}

fn cifrado(clave_usuario: &str) -> Vec<u8> {
    let mut doc = documento(&[&["Contenido secreto", RELLENO]]);
    doc.trailer.set(
        "ID",
        vec![
            Object::string_literal("identificador-a"),
            Object::string_literal("identificador-a"),
        ],
    );
    let estado = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &doc,
        owner_password: "propietario",
        user_password: clave_usuario,
        key_length: 128,
        permissions: Permissions::all(),
    })
    .unwrap();
    doc.encrypt(&estado).unwrap();
    bytes_de(doc)
}

#[tokio::test]
async fn test_cifrado_con_contrasena_error_claro_y_sin_texto() {
    let error = resultado(&cifrado("usuario"))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("protegido"), "{error}");
    assert!(!error.contains("secreto"));
}

#[tokio::test]
async fn test_cifrado_con_contrasena_vacia_tampoco_se_abre() {
    // Solo restringe permisos, pero la decisión es no sortear ninguna protección
    let error = resultado(&cifrado("")).await.unwrap_err().to_string();
    assert!(error.contains("protegido"), "{error}");
}

fn con_arbol_a_medida(construir: impl FnOnce(&mut lopdf::Document, lopdf::ObjectId)) -> Vec<u8> {
    let mut doc = documento(&[&["1. ALCANCE", RELLENO], &["2. OTRO", RELLENO]]);
    let raiz = doc.new_object_id();
    construir(&mut doc, raiz);
    fijar_raiz_de_marcadores(&mut doc, raiz);
    bytes_de(doc)
}

#[tokio::test]
async fn test_marcadores_con_next_ciclico_terminan() {
    let bytes = con_arbol_a_medida(|doc, raiz| {
        let pagina = doc.get_pages()[&1];
        let a = doc.new_object_id();
        let b = doc.new_object_id();
        doc.objects.insert(
            a,
            Object::Dictionary(dictionary! {
                "Title" => Object::string_literal("Bucle A"),
                "Dest" => vec![pagina.into(), "Fit".into()],
                "Next" => b,
            }),
        );
        doc.objects.insert(
            b,
            Object::Dictionary(dictionary! {
                "Title" => Object::string_literal("Bucle B"),
                "Dest" => vec![pagina.into(), "Fit".into()],
                "Next" => a,
            }),
        );
        doc.objects
            .insert(raiz, Object::Dictionary(dictionary! { "First" => a }));
    });
    let documento = resultado(&bytes).await.unwrap();
    let titulos: Vec<&str> = documento
        .secciones
        .iter()
        .map(|s| s.titulo.as_str())
        .collect();
    assert_eq!(titulos, ["Bucle A", "Bucle B"]);
}

#[tokio::test]
async fn test_marcadores_con_first_ciclico_terminan() {
    let bytes = con_arbol_a_medida(|doc, raiz| {
        let pagina = doc.get_pages()[&2];
        let a = doc.new_object_id();
        doc.objects.insert(
            a,
            Object::Dictionary(dictionary! {
                "Title" => Object::string_literal("Autohijo"),
                "Dest" => vec![pagina.into(), "Fit".into()],
                "First" => a,
            }),
        );
        doc.objects
            .insert(raiz, Object::Dictionary(dictionary! { "First" => a }));
    });
    let documento = resultado(&bytes).await.unwrap();
    assert!(documento.secciones.iter().any(|s| s.titulo == "Autohijo"));
    assert!(documento.contenido_completo.contains("ALCANCE"));
}

#[tokio::test]
async fn test_destino_invalido_conserva_el_nodo_y_el_texto() {
    let bytes = con_arbol_a_medida(|doc, raiz| {
        let a = doc.new_object_id();
        let b = doc.new_object_id();
        let c = doc.new_object_id();
        doc.objects.insert(
            a,
            Object::Dictionary(dictionary! {
                "Title" => Object::string_literal("Destino roto"),
                "Dest" => vec![Object::Reference((9999, 0)), "Fit".into()],
                "Next" => b,
            }),
        );
        doc.objects.insert(
            b,
            Object::Dictionary(dictionary! {
                "Title" => Object::string_literal("Nombre inexistente"),
                "Dest" => Object::string_literal("no-existe"),
                "Next" => c,
            }),
        );
        doc.objects.insert(
            c,
            Object::Dictionary(dictionary! { "Title" => Object::string_literal("Sin destino") }),
        );
        doc.objects
            .insert(raiz, Object::Dictionary(dictionary! { "First" => a }));
    });
    let documento = resultado(&bytes).await.unwrap();
    let total: String = documento
        .secciones
        .iter()
        .map(|s| s.contenido.clone())
        .collect();
    assert!(total.contains("ALCANCE") && total.contains("OTRO"));
    assert!(
        documento
            .secciones
            .iter()
            .any(|s| s.titulo == "Destino roto")
    );
    assert!(
        documento
            .secciones
            .iter()
            .any(|s| s.titulo == "Sin destino")
    );
}

#[tokio::test]
async fn test_raiz_de_marcadores_sin_entradas_recurre_a_la_numeracion() {
    let bytes = con_arbol_a_medida(|doc, raiz| {
        doc.objects
            .insert(raiz, Object::Dictionary(dictionary! { "Count" => 0 }));
    });
    let documento = resultado(&bytes).await.unwrap();
    assert!(documento.secciones.iter().any(|s| s.titulo == "OTRO"));
}

#[tokio::test]
async fn test_titulos_con_caracteres_de_control_se_sanean() {
    let bytes = con_marcadores(
        &[&["Primero", RELLENO], &["Segundo", RELLENO]],
        &[
            marcador("Prim\u{1}ero\u{7}\n\t", 1, vec![]),
            marcador("\u{0}\u{1}\u{2}", 2, vec![]),
        ],
    );
    let documento = resultado(&bytes).await.unwrap();
    assert_eq!(documento.secciones[0].titulo, "Primero");
    for seccion in &documento.secciones {
        assert!(!seccion.titulo.chars().any(char::is_control));
        assert!(!seccion.breadcrumb.chars().any(char::is_control));
    }
    assert_eq!(documento.secciones[1].titulo, "(Sin título)");
}

#[tokio::test]
async fn test_arbol_de_paginas_ciclico_no_tumba_el_proceso() {
    let mut doc = documento(&[&["1. ALCANCE", RELLENO]]);
    let id_catalogo = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let id_paginas = doc
        .get_object(id_catalogo)
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_object_mut(id_paginas)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Kids", vec![Object::Reference(id_paginas)]);
    let _ = resultado(&bytes_de(doc)).await;
}

fn paginas(lineas: &[&[&str]]) -> Vec<String> {
    lineas.iter().map(|p| p.join("\n")).collect()
}

fn codigos(lineas: &[&[&str]]) -> Vec<String> {
    estructurar_por_numeracion(&paginas(lineas))
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.nivel > 0)
        .map(|s| s.codigo)
        .collect()
}

#[test]
fn test_lista_numerada_dentro_de_seccion_no_abre_nodos() {
    assert_eq!(
        codigos(&[&[
            "1. PRIMERO",
            "1.1 Interior",
            "1.2 Otro interior",
            "1. Primer paso de la lista",
            "2. Segundo paso de la lista",
            "3. Tercer paso de la lista",
            "Parrafo a",
            "Parrafo b",
            "Parrafo c",
            "Parrafo d",
            "Parrafo e",
            "Parrafo f",
            "Parrafo g",
            "Parrafo h",
            "Parrafo i",
            "Parrafo j",
            "Parrafo k",
            "Parrafo l",
            "Parrafo m",
            "Parrafo n",
            "Parrafo o",
            "Parrafo p",
            "2. SEGUNDO",
        ]]),
        ["1", "1.1", "1.2", "2"]
    );
}

#[test]
fn test_notas_al_pie_y_frases_numeradas_no_abren_nodos() {
    assert_eq!(
        codigos(&[&[
            "1. PRIMERO",
            "1 Nota al pie: vease la norma citada en el apartado anterior.",
            "2 Se aplicara el criterio del fabricante,",
            "2024 Informe anual",
            "77.88.99 Control de piezas",
            "7 Tubos",
            "1.1 Dentro",
            "2. SEGUNDO",
        ]]),
        ["1", "1.1", "2"]
    );
}

#[test]
fn test_item_de_lista_sin_punto_no_bloquea_los_subapartados_reales() {
    assert_eq!(
        codigos(&[&[
            "1. UNO",
            "1.1 Uno primero",
            "2 Frase de una lista partida en dos lineas",
            "3 Otra frase de la misma lista",
            "1.2 Uno segundo",
            "2. DOS",
            "2.1 Dos primero",
        ]]),
        ["1", "1.1", "1.2", "2", "2.1"]
    );
}

#[test]
fn test_numeracion_decreciente_o_salto_absurdo_no_abre_nodos() {
    assert_eq!(
        codigos(&[&[
            "1. UNO",
            "2. DOS",
            "2.1 Hijo",
            "1.5 Atras",
            "9.1 Salto",
            "3. TRES"
        ]]),
        ["1", "2", "2.1", "3"]
    );
}

#[test]
fn test_un_solo_encabezado_no_es_estructura() {
    assert!(estructurar_por_numeracion(&paginas(&[&["1. SOLO", "texto"]])).is_none());
}

#[test]
fn test_indice_al_principio_no_rompe_la_numeracion_real() {
    assert_eq!(
        codigos(&[
            &[
                "INDICE",
                "1. ALCANCE ....... 2",
                "2. DETALLE ....... 3",
                "3. CIERRE"
            ],
            &["1. ALCANCE", "texto", "1.1 Interior"],
            &["2. DETALLE", "texto", "3. CIERRE", "texto"],
        ]),
        ["1", "1.1", "2", "3"]
    );
}

#[test]
fn test_cabecera_repetida_no_es_encabezado_pero_sigue_en_el_texto() {
    let secciones = estructurar_por_numeracion(&paginas(&[
        &["1. Manual Corporativo", "1. ALCANCE", "texto a"],
        &["1. Manual Corporativo", "texto b"],
        &["1. Manual Corporativo", "2. DETALLE", "texto c"],
    ]))
    .unwrap();
    let titulos: Vec<&str> = secciones.iter().map(|s| s.titulo.as_str()).collect();
    assert!(!titulos.contains(&"Manual Corporativo"), "{titulos:?}");
    assert!(titulos.contains(&"ALCANCE") && titulos.contains(&"DETALLE"));
    let total: String = secciones.iter().map(|s| s.contenido.clone()).collect();
    assert_eq!(total.matches("Manual Corporativo").count(), 3);
}

#[test]
fn test_lineas_extremas_no_abren_nodos_ni_entran_en_panico() {
    let larga = format!("1. {}", "A".repeat(100_000));
    let profunda = format!("{} Titulo", "1.".repeat(5_000));
    let enorme = "99999999999999999999999 Titulo";
    assert!(
        codigos(&[&[
            larga.as_str(),
            profunda.as_str(),
            enorme,
            "\u{0}\u{1}",
            "١. عربي"
        ]])
        .is_empty()
    );
}
