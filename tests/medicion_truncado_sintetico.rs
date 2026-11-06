//! Medición del recorte a 512 tokens del embedder real sobre un documento Markdown sintético,
//! sin Claude: fragmenta como la ingesta provisional y cuenta los textos vectorizables que el
//! tokenizer real de E5 trunca. Ningún texto procede de documentos reales.
//! `FASTEMBED_CACHE_DIR=<caché> cargo test --test medicion_truncado_sintetico -- --ignored --nocapture`

use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::application::ports::embedding_engine::EmbeddingEnginePort;
use lakehouse_rag::application::use_cases::mapeo_destilacion::mapear_revision_provisional;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::services::fragmentacion::estimar_tokens;
use lakehouse_rag::infrastructure::embeddings::fastembed_engine::FastEmbedEngine;
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;

/// Vocabulario inventado de prosa técnica en español
const VOCABULARIO: [&str; 24] = [
    "el",
    "operario",
    "verificará",
    "que",
    "la",
    "estructura",
    "auxiliar",
    "cumple",
    "con",
    "los",
    "requisitos",
    "de",
    "estanqueidad",
    "y",
    "resistencia",
    "antes",
    "del",
    "montaje",
    "definitivo",
    "en",
    "cada",
    "tramo",
    "inspeccionado",
    "durante",
];
/// Tramos con cifras, unidades y códigos, más densos en tokens reales que la prosa
const CODIGOS: [&str; 6] = [
    "REQ-12.4.7",
    "350 mm",
    "±0,5 %",
    "ISO-9001:2015",
    "4.18.3.b)",
    "1.250 kN/m²",
];
/// Tamaños de sección en tokens estimados, de 400 a 700 con las fronteras del umbral
const TAMANOS: [usize; 14] = [
    400, 450, 480, 500, 511, 512, 513, 540, 580, 600, 601, 640, 680, 700,
];

/// Texto de una sección hasta alcanzar `objetivo` tokens estimados; `con_codigos` intercala cifras
fn texto_seccion(objetivo: usize, semilla: usize, con_codigos: bool) -> String {
    let mut texto = String::new();
    let mut paso = semilla;
    while estimar_tokens(&texto) < objetivo {
        let palabra = if con_codigos && paso.is_multiple_of(5) {
            CODIGOS[paso % CODIGOS.len()]
        } else {
            VOCABULARIO[paso % VOCABULARIO.len()]
        };
        let separador = if paso % 17 == 16 { ". " } else { " " };
        let candidato = format!("{texto}{palabra}{separador}");
        if estimar_tokens(&candidato) > objetivo {
            break;
        }
        texto = candidato;
        paso += 7;
    }
    texto.trim_end().to_string()
}

/// Documento con seis niveles de encabezado de títulos largos (breadcrumbs profundos) y una
/// condición heredada, y hojas con todos los `TAMANOS`, en prosa y con códigos
fn documento_sintetico() -> String {
    let mut documento = String::new();
    for capitulo in 1..=2 {
        documento.push_str(&format!(
            "# {capitulo} Requisitos generales de la instalación auxiliar del tramo {capitulo}\n\n"
        ));
        documento.push_str(&format!(
            "## {capitulo}.1 Condiciones de montaje en ambiente húmedo y temperatura baja\n\n"
        ));
        documento.push_str(&format!(
            "### {capitulo}.1.1 Verificación previa de los elementos de anclaje y soporte\n\n"
        ));
        documento.push_str(&format!(
            "#### {capitulo}.1.1.1 Control documental de los certificados del suministrador\n\n"
        ));
        documento.push_str(&format!(
            "##### {capitulo}.1.1.1.1 Registro de las inspecciones intermedias por tramo\n\n"
        ));
        for (indice, tamano) in TAMANOS.iter().enumerate() {
            documento.push_str(&format!(
                "###### {capitulo}.1.1.1.1.{} Comprobación del tramo número {} del recorrido\n\n{}\n\n",
                indice + 1,
                indice + 1,
                texto_seccion(*tamano, indice, capitulo == 2)
            ));
        }
    }
    documento
}

/// Títulos del documento unidos como un breadcrumb
fn titulos_como_breadcrumb() -> String {
    documento_sintetico()
        .lines()
        .filter_map(|linea| linea.strip_prefix('#'))
        .map(|titulo| titulo.trim_start_matches('#').trim())
        .take(6)
        .collect::<Vec<_>>()
        .join(" > ")
}

#[tokio::test]
#[ignore = "requiere el modelo real de FastEmbed"]
async fn medir_truncado_de_un_documento_sintetico_largo() {
    let documento = documento_sintetico();
    let parsed = MarkdownParser
        .parse_bytes("sintetico.md", documento.as_bytes())
        .await
        .unwrap();
    let centro = CenterId::new("medicion").unwrap();
    let (_, revision) =
        mapear_revision_provisional(&centro, &parsed, "0".repeat(64).as_str()).unwrap();
    let motor = FastEmbedEngine::try_new(None).unwrap();

    let textos = &revision.textos_chunks;
    let contenidos: Vec<String> = revision
        .chunks
        .iter()
        .map(|chunk| chunk.texto_contenido.clone())
        .collect();
    let truncados = motor.contar_truncados(textos).await.unwrap();
    let truncados_sin_miga = motor.contar_truncados(&contenidos).await.unwrap();
    let estimados: Vec<usize> = textos.iter().map(|t| estimar_tokens(t)).collect();
    println!(
        "secciones={} chunks={} truncados={} truncados_sin_miga={} estimado_max={} estimado_min={}",
        parsed.secciones.len(),
        textos.len(),
        truncados,
        truncados_sin_miga,
        estimados.iter().max().unwrap(),
        estimados.iter().min().unwrap()
    );
    for (texto, estimado) in textos.iter().zip(&estimados) {
        let truncado = motor
            .contar_truncados(std::slice::from_ref(texto))
            .await
            .unwrap();
        println!("chunk estimado={estimado} truncado={truncado}");
    }
}

/// Tokens estimados del prefijo más largo, en caracteres, que el tokenizer real no trunca
async fn estimado_en_el_limite_real(motor: &FastEmbedEngine, texto: &str) -> usize {
    let cortes: Vec<usize> = texto
        .char_indices()
        .map(|(posicion, _)| posicion)
        .chain(std::iter::once(texto.len()))
        .collect();
    let (mut cabe, mut no_cabe) = (0, cortes.len());
    while no_cabe - cabe > 1 {
        let medio = (cabe + no_cabe) / 2;
        let prefijo = texto[..cortes[medio]].to_string();
        if motor.contar_truncados(&[prefijo]).await.unwrap() == 0 {
            cabe = medio;
        } else {
            no_cabe = medio;
        }
    }
    estimar_tokens(&texto[..cortes[cabe]])
}

/// Cuántos tokens estimados caben en los 512 reales según el tipo de texto: mide cuánto se
/// desvía la estimación determinista del tokenizer de E5
#[tokio::test]
#[ignore = "requiere el modelo real de FastEmbed"]
async fn medir_desvio_de_la_estimacion_por_tipo_de_texto() {
    let motor = FastEmbedEngine::try_new(None).unwrap();
    let casos = [
        ("prosa", texto_seccion(1500, 0, false)),
        ("prosa_con_codigos", texto_seccion(1500, 0, true)),
        ("sin_espacios", "ñandúcontrol".repeat(400)),
        ("ideogramas", "日本語の技術文書 ".repeat(300)),
        ("breadcrumb", titulos_como_breadcrumb().repeat(8)),
    ];
    for (tipo, texto) in casos {
        println!(
            "{tipo}: estimado_total={} estimado_en_limite_real={}",
            estimar_tokens(&texto),
            estimado_en_el_limite_real(&motor, &texto).await
        );
    }
}
