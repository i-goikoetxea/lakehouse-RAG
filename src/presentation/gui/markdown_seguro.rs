//! Markdown del modelo → HTML seguro para `innerHTML` (CommonMark + tablas GFM con
//! `pulldown-cmark`).
//!
//! Invariantes de seguridad:
//! * Ningún HTML crudo del modelo o de los documentos llega vivo: los eventos `Html` e
//!   `InlineHtml` se convierten en texto, que el escritor escapa.
//! * Los enlaces **no navegan** la ventana: se pintan como `<span class="md-enlace">` con la
//!   URL (solo `http`/`https`) en `title`; las imágenes se reducen a su texto alternativo.
//! * Los únicos atributos emitidos son `class`, `title` (escapado) y la alineación de tabla.
//!
//! Decisiones de lectura: un salto de línea simple es salto visible (el modelo escribe
//! «**Campo:** valor» línea a línea) y las tablas van envueltas para desplazarse en horizontal.

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

/// Convierte el Markdown del modelo en HTML apto para `innerHTML`
pub fn markdown_a_html_seguro(markdown: &str) -> String {
    let opciones = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let eventos = Parser::new_ext(markdown, opciones).flat_map(neutralizar);
    let mut salida = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut salida, eventos);
    salida
        .replace(
            "<table>",
            "<div class=\"md-tabla-envoltorio\"><table class=\"md-tabla\">",
        )
        .replace("</table>", "</table></div>")
}

/// Sustituye todo lo que podría ejecutar, navegar o cargar recursos
fn neutralizar(evento: Event<'_>) -> Vec<Event<'_>> {
    match evento {
        // Un bloque HTML llega línea a línea con su `\n`: texto escapado + salto visible
        Event::Html(crudo) => match crudo.strip_suffix('\n') {
            Some(linea) => vec![
                Event::Text(CowStr::from(linea.to_string())),
                Event::HardBreak,
            ],
            None => vec![Event::Text(crudo)],
        },
        otro => vec![neutralizar_en_linea(otro)],
    }
}

fn neutralizar_en_linea(evento: Event<'_>) -> Event<'_> {
    match evento {
        Event::InlineHtml(crudo) => Event::Text(crudo),
        Event::SoftBreak => Event::HardBreak,
        Event::Start(Tag::Link { dest_url, .. }) => Event::InlineHtml(apertura_enlace(&dest_url)),
        Event::End(TagEnd::Link) => Event::InlineHtml(CowStr::Borrowed("</span>")),
        Event::Start(Tag::Image { .. }) => {
            Event::InlineHtml(CowStr::Borrowed("<span class=\"md-imagen\">[imagen: "))
        }
        Event::End(TagEnd::Image) => Event::InlineHtml(CowStr::Borrowed("]</span>")),
        otro => otro,
    }
}

fn apertura_enlace(destino: &str) -> CowStr<'static> {
    let destino = destino.trim();
    let es_web = ["http://", "https://"]
        .iter()
        .any(|esquema| destino.to_ascii_lowercase().starts_with(esquema));
    if es_web {
        CowStr::from(format!(
            "<span class=\"md-enlace\" title=\"{}\">",
            escapar_atributo(destino)
        ))
    } else {
        CowStr::Borrowed("<span class=\"md-enlace\">")
    }
}

fn escapar_atributo(valor: &str) -> String {
    let mut escapado = String::with_capacity(valor.len());
    for c in valor.chars() {
        match c {
            '&' => escapado.push_str("&amp;"),
            '<' => escapado.push_str("&lt;"),
            '>' => escapado.push_str("&gt;"),
            '"' => escapado.push_str("&quot;"),
            '\'' => escapado.push_str("&#39;"),
            otro => escapado.push(otro),
        }
    }
    escapado
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enlace_web_queda_como_texto_con_la_url_escapada_en_title() {
        assert_eq!(
            markdown_a_html_seguro("[norma](https://x.y/a?b=\"c\")"),
            "<p><span class=\"md-enlace\" title=\"https://x.y/a?b=&quot;c&quot;\">norma</span></p>\n"
        );
    }

    #[test]
    fn test_salto_simple_es_visible_y_la_tabla_va_envuelta() {
        let html = markdown_a_html_seguro("**A:** 1\n**B:** 2\n\n| x |\n|---|\n| 1 |");
        assert!(html.contains("<strong>A:</strong> 1<br />"), "{html}");
        assert!(
            html.contains("<div class=\"md-tabla-envoltorio\"><table class=\"md-tabla\">"),
            "{html}"
        );
    }
}
