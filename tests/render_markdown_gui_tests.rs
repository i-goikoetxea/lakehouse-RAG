//! Corpus de render del Markdown que emite el modelo en la ventana (`texto_a_html_seguro`).
//! Los casos salen de respuestas reales de Sonnet y Haiku sobre los fixtures (capturas de la
//! Fase 14). Las aserciones son de comportamiento (qué etiqueta aparece y qué marcador NO debe
//! quedar literal), no de la forma exacta del HTML, para no atarse a un renderizador.

use lakehouse_rag::presentation::gui::chat_service::texto_a_html_seguro;

fn render(md: &str) -> String {
    texto_a_html_seguro(md)
}

/// Texto visible aproximado (sin etiquetas) para comprobar marcadores literales
fn visible(html: &str) -> String {
    let mut fuera = String::new();
    let mut dentro = false;
    for c in html.chars() {
        match c {
            '<' => dentro = true,
            '>' => dentro = false,
            _ if !dentro => fuera.push(c),
            _ => {}
        }
    }
    fuera
}

#[test]
fn test_cursiva_con_un_asterisco_o_guion_bajo() {
    let html = render("apartado *1. Objeto y Alcance* y _Registro de Custodia_");
    assert!(html.contains("<em>1. Objeto y Alcance</em>"), "{html}");
    assert!(html.contains("<em>Registro de Custodia</em>"), "{html}");
}

#[test]
fn test_negrita_y_cursiva_anidadas() {
    let html = render("**Registro:** FOR-ALM-01, *Parte de Recepción de Materiales*.");
    assert!(html.contains("<strong>Registro:</strong>"), "{html}");
    assert!(
        html.contains("<em>Parte de Recepción de Materiales</em>"),
        "{html}"
    );
    assert!(!visible(&html).contains('*'), "{html}");
}

#[test]
fn test_cita_en_bloque() {
    let html = render("> «Este procedimiento establece la secuencia»");
    assert!(html.contains("<blockquote"), "{html}");
    assert!(
        !visible(&html).contains('>'),
        "el marcador > no queda literal: {html}"
    );
}

#[test]
fn test_listas_ordenadas_y_anidadas() {
    let html = render(
        "**Criterio:**\n1. El albarán coincide.\n2. Certificado 3.1.\n   - sublote A\n   - sublote B\n3. Sin certificado, retenido.",
    );
    assert!(html.contains("<ol"), "{html}");
    assert_eq!(html.matches("<li").count(), 5, "{html}");
    assert!(html.contains("<ul"), "lista anidada: {html}");
}

#[test]
fn test_lista_con_asterisco_y_guion() {
    let html = render("- uno\n- dos\n\n* tres\n* cuatro");
    assert_eq!(html.matches("<li").count(), 4, "{html}");
    assert!(!visible(&html).contains('*'), "{html}");
}

#[test]
fn test_bloque_de_codigo_con_vallas() {
    let html = render("```sql\nSELECT * FROM \"obra_03\" WHERE a < 3\n```");
    assert!(html.contains("<pre"), "{html}");
    assert!(
        html.contains("SELECT * FROM"),
        "el asterisco del SQL se respeta: {html}"
    );
    assert!(html.contains("&lt; 3"), "{html}");
    assert!(!visible(&html).contains("```"), "{html}");
}

#[test]
fn test_parrafos_y_regla_horizontal() {
    let html = render("Primer párrafo\ncontinúa.\n\nSegundo párrafo.\n\n---\n\nTercero.");
    assert!(html.matches("<p").count() >= 3, "{html}");
    assert!(html.contains("<hr"), "{html}");
}

#[test]
fn test_tabla_con_alineacion_y_formato_en_celdas() {
    let html = render(
        "| Revisión | vigente |\n|:---|---:|\n| Rev. **04** | `true` |\n| Rev. 03 | false |",
    );
    assert!(html.contains("<table"), "{html}");
    assert!(html.contains("<th"), "{html}");
    assert!(html.contains("<strong>04</strong>"), "{html}");
    assert!(html.contains("<code>true</code>"), "{html}");
}

#[test]
fn test_encabezados_de_varios_niveles() {
    let html = render("# Uno\n## Dos\n### Tres\n#### Cuatro");
    assert!(!visible(&html).contains('#'), "{html}");
    assert!(html.contains("<h"), "{html}");
}

#[test]
fn test_asterisco_escapado_y_nota_al_pie() {
    let html = render("**125 °C (\\*)** y nota (\\*): requiere QA/QC");
    assert!(visible(&html).contains("125 °C (*)"), "{html}");
    assert!(!visible(&html).contains('\\'), "{html}");
}

/// Etiquetas HTML vivas del resultado (lo escapado como `&lt;` no cuenta)
fn etiquetas_vivas(html: &str) -> Vec<String> {
    html.split('<')
        .skip(1)
        .filter_map(|resto| {
            resto
                .split_once('>')
                .map(|(etiqueta, _)| etiqueta.to_lowercase())
        })
        .collect()
}

/// Solo etiquetas de formato y, como atributos, `class`/`align`/`style="text-align…"`
fn solo_html_de_formato(html: &str) -> Result<(), String> {
    const PERMITIDAS: [&str; 27] = [
        "div",
        "p",
        "br",
        "strong",
        "em",
        "code",
        "pre",
        "blockquote",
        "ul",
        "ol",
        "li",
        "hr",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "del",
        "span",
        "a",
    ];
    for etiqueta in etiquetas_vivas(html) {
        let etiqueta = etiqueta
            .trim_start_matches('/')
            .trim_end_matches('/')
            .trim();
        let mut partes = etiqueta.split_whitespace();
        let nombre = partes.next().unwrap_or_default();
        if !PERMITIDAS.contains(&nombre) {
            return Err(format!("etiqueta no permitida <{etiqueta}>"));
        }
        let atributos: String = partes.collect::<Vec<_>>().join(" ");
        if atributos.contains("on") && atributos.contains('=') && !atributos.starts_with("class=") {
            return Err(format!("atributo sospechoso en <{etiqueta}>"));
        }
        if nombre == "a"
            && !atributos.is_empty()
            && !atributos.contains("data-url=")
            && !atributos.starts_with("class=")
        {
            return Err(format!("enlace con atributos no permitidos <{etiqueta}>"));
        }
        if atributos.contains("href") || atributos.contains("src") {
            return Err(format!("href/src vivo en <{etiqueta}>"));
        }
    }
    Ok(())
}

#[test]
fn test_enlaces_no_navegan_la_ventana() {
    let html = render(
        "Ver [norma](https://example.com/une) y [local](javascript:alert(1)) y <https://x.y>",
    );
    solo_html_de_formato(&html).unwrap_or_else(|e| panic!("{e}: {html}"));
    assert!(visible(&html).contains("norma"), "{html}");
    assert!(!html.contains("data-url=\"javascript"), "{html}");
}

#[test]
fn test_html_crudo_del_modelo_siempre_escapado() {
    for hostil in [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<a href=\"javascript:alert(1)\">x</a>",
        "<details open ontoggle=alert(1)>",
        "texto <b onmouseover=x>negrita</b>",
        "[x](javascript:alert(1))",
        "![img](https://x/y.png)",
        "<div>

**bloque html**

</div>",
    ] {
        let html = render(hostil);
        solo_html_de_formato(&html).unwrap_or_else(|e| panic!("{hostil} → {e}: {html}"));
    }
}

#[test]
fn test_identificadores_con_guion_bajo_no_se_vuelven_cursiva() {
    let html = render("La columna condicion_aplicacion y fecha_referencia_vigente");
    assert!(!html.contains("<em>"), "{html}");
    assert!(visible(&html).contains("condicion_aplicacion"), "{html}");
}

/// Respuestas reales (Sonnet/Haiku, Fase 14) para el banco visual
const RESPUESTAS_REALES: [(&str, &str); 3] = [
    (
        "Custodia (Sonnet)",
        "## Fase CUSTODIA: responsable y registro\n\n**Responsable:** Jefe de Producción\n**Ejecutor:** Auxiliar de Almacén\n**Registro que se llena:** FOR-ALM-05 — *Registro de Custodia, Preservación y Trazabilidad*\n\n> «Tubería almacenada sobre durmientes a más de 15 cm del suelo»\n\n**Condición de aplicación:** solo cuando el acopio supera 7 días naturales.\n\n| Revisión | vigente | fecha_vigor | fecha_derogacion |\n|---|---|---|---|\n| Rev. 03 (versión 3) | false | 2026-03-01 | 2026-09-15 |\n| Rev. 04 (versión 4) | true | 2026-09-15 | — |\n\n**Cita:** PO-ALM-03, Rev. 04, *2. Secuencia Operativa > 2.2 Fase 2 — CUSTODIA*. El hito `PO-ALM-03@v4-HIT-0001` usa `condicion_aplicacion`.",
    ),
    (
        "Recepción (Sonnet)",
        "### Controles en la fase de RECEPCIÓN\n\n**Fuente:** PO-ALM-03, Rev. 04, apartado *2.1 Fase 1 — RECEPCIÓN*.\n\n- **Objeto/alcance:** tubería de acero al carbono (S275JR y S355J2).\n- **Criterio de aceptación (controles):**\n  1. El albarán coincide con el pedido.\n  2. Cada lote dispone de certificado de material 3.1 según UNE-EN 10204.\n  3. Sin certificado 3.1, el lote queda retenido.\n- **Responsable:** Jefe de Almacén.\n\n---\n\nSi lo necesita, puedo consultar el texto del apartado con `search_normative_context`.",
    ),
    (
        "Tabla 4-A (Haiku)",
        "**Temperatura mínima de precalentamiento: 125 °C (\\*)**\n\nNota de pie (literal): «125 °C (\\*): Requiere aprobación previa del Inspector QA/QC».\n\n1. Eje X (espesor_nominal_mm): 20 < e ≤ 40 mm.\n2. Eje Y (carbono_equivalente_pct): 0,4 ≤ CE ≤ 0,45 %.\n\n```sql\nSELECT valor_resultado_texto FROM \"obra_05_celdas_doble_entrada\" WHERE eje_x_min < 30\n```\n\nVer [UNE-EN ISO 15614-1](https://www.une.org) y <script>alert(1)</script>.",
    ),
];

#[test]
#[ignore = "genera docs/validacion/fase14/banco_render.html para revisión visual"]
fn generar_banco_de_render() {
    let estilos = include_str!("../src/presentation/gui/web/styles.css");
    let mut cuerpo = String::new();
    for (titulo, md) in RESPUESTAS_REALES {
        cuerpo.push_str(&format!(
            "<section class=\"caso\"><h6 class=\"caso-titulo\">{titulo}</h6><div class=\"respuesta\">{}</div></section>",
            render(md)
        ));
    }
    let pagina = format!(
        "<!DOCTYPE html><html lang=\"es\" data-theme=\"dark\"><head><meta charset=\"UTF-8\"><title>Banco de render</title><style>{estilos}\nbody{{overflow:auto;display:block;padding:24px}} .caso{{max-width:780px;margin:0 auto 28px;padding:16px 20px;border:1px solid var(--color-border);border-radius:10px;background:var(--color-surface-1)}} .caso-titulo{{font-size:11px;color:var(--color-ink-subtle);margin-bottom:10px;text-transform:uppercase}}</style></head><body>{cuerpo}</body></html>"
    );
    std::fs::write("docs/validacion/fase14/banco_render.html", pagina).unwrap();
}
