//! Suite adversarial del umbral de chunk único a 512 tokens estimados: secciones enormes,
//! breadcrumbs muy largos, texto sin espacios e ideogramas. Todo el texto es sintético.

use lakehouse_rag::application::dtos::ingestion::ParsedDocument;
use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::application::use_cases::mapeo_destilacion::mapear_revision_provisional;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::services::fragmentacion::{
    TOKENS_SOLAPE, TOKENS_VENTANA, UMBRAL_CHUNK_UNICO, estimar_tokens, fragmentar_seccion,
    reconstruir_sin_solape,
};
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;

fn palabras(total: usize) -> String {
    (0..total)
        .map(|i| format!("w{:03}", i % 1000))
        .collect::<Vec<_>>()
        .join(" ")
}

fn sin_perdidas_y_dentro_de_la_ventana(original: &str) -> Vec<String> {
    let ventanas = fragmentar_seccion(original);
    assert!(
        ventanas.iter().all(|v| estimar_tokens(v) <= TOKENS_VENTANA),
        "una ventana supera {TOKENS_VENTANA} tokens estimados"
    );
    assert_eq!(reconstruir_sin_solape(&ventanas), original.trim());
    ventanas
}

#[test]
fn test_el_umbral_de_chunk_unico_coincide_con_la_ventana_del_modelo() {
    assert_eq!(UMBRAL_CHUNK_UNICO, 512);
    assert_eq!(UMBRAL_CHUNK_UNICO, TOKENS_VENTANA);
}

#[test]
fn test_seccion_enorme_se_divide_en_ventanas_acotadas_sin_perdidas() {
    let original = palabras(200_000);
    let ventanas = sin_perdidas_y_dentro_de_la_ventana(&original);
    let avance = TOKENS_VENTANA - TOKENS_SOLAPE;
    assert!(
        ventanas.len() <= 200_000 / (avance / 2) + 1,
        "{} ventanas: cada una avanza al menos media ventana menos el solape",
        ventanas.len()
    );
}

#[test]
fn test_texto_sin_espacios_en_la_frontera_de_512() {
    let justo = "a".repeat(512 * 4);
    assert_eq!(estimar_tokens(&justo), 512);
    assert_eq!(fragmentar_seccion(&justo), vec![justo.clone()]);

    let encima = "a".repeat(512 * 4 + 1);
    assert_eq!(estimar_tokens(&encima), 513);
    let ventanas = sin_perdidas_y_dentro_de_la_ventana(&encima);
    assert_eq!(ventanas.len(), 2);
    assert!(
        encima.ends_with(ventanas[1].as_str()),
        "la cola queda cubierta"
    );
}

#[test]
fn test_ideogramas_y_utf8_multibyte_en_la_frontera_de_512() {
    let justo = "語".repeat(512);
    assert_eq!(fragmentar_seccion(&justo), vec![justo.clone()]);

    let encima = "語".repeat(513);
    let ventanas = sin_perdidas_y_dentro_de_la_ventana(&encima);
    assert_eq!(ventanas.len(), 2);
    assert_eq!(ventanas[1].chars().count(), TOKENS_SOLAPE + 1);

    let ene = "ñ".repeat(513 * 4);
    assert_eq!(sin_perdidas_y_dentro_de_la_ventana(&ene).len(), 2);
}

async fn parsear(markdown: &str) -> ParsedDocument {
    MarkdownParser
        .parse_bytes("sintetico.md", markdown.as_bytes())
        .await
        .unwrap()
}

/// Seis niveles de encabezado con títulos cercanos al máximo de la pila y una hoja de `tokens`
fn documento_con_breadcrumb_enorme(tokens: usize) -> String {
    let titulo = |nivel: usize| format!("{nivel} {}", "requisito auxiliar ".repeat(25));
    let mut documento = String::new();
    for nivel in 1..=6 {
        documento.push_str(&format!("{} {}\n\n", "#".repeat(nivel), titulo(nivel)));
    }
    documento.push_str(&palabras(tokens));
    documento.push('\n');
    documento
}

#[tokio::test]
async fn test_breadcrumb_muy_largo_no_altera_los_cortes_ni_pierde_palabras() {
    for (tokens, chunks_esperados) in [(512, 1), (513, 2)] {
        let parsed = parsear(&documento_con_breadcrumb_enorme(tokens)).await;
        let centro = CenterId::new("umbral").unwrap();
        let (_, revision) = mapear_revision_provisional(&centro, &parsed, &"0".repeat(64)).unwrap();
        let hoja: Vec<_> = revision
            .chunks
            .iter()
            .filter(|chunk| chunk.texto_contenido.starts_with('w'))
            .collect();
        assert_eq!(hoja.len(), chunks_esperados, "{tokens} tokens");
        assert!(estimar_tokens(&hoja[0].breadcrumb_jerarquia) > TOKENS_VENTANA / 2);

        let union: String = revision
            .chunks
            .iter()
            .map(|c| format!("{} {}", c.breadcrumb_jerarquia, c.texto_contenido))
            .collect::<Vec<_>>()
            .join(" ");
        for palabra in documento_con_breadcrumb_enorme(tokens).split_whitespace() {
            if palabra.chars().all(|c| c == '#') {
                continue;
            }
            assert!(union.contains(palabra), "falta {palabra}");
        }
    }
}
