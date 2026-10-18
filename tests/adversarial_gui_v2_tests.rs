//! Suite adversarial de la Fase 14 (ventana cableada a `LakehouseApp`): rutas hostiles,
//! archivos inexistentes o binarios, extensiones no soportadas, centro inactivo, HTML/JS
//! malicioso en documentos y respuestas, e ingestas concurrentes.

mod comun_centros;
mod comun_store_v2;

use std::sync::Arc;

use comun_centros::*;
use comun_store_v2::{ARCHIVO_V3, paquete_revision};
use lakehouse_rag::application::dtos::agent::AgentTurnOutput;
use lakehouse_rag::application::dtos::centros::DIRECTORIO_RAW_DOCS;
use lakehouse_rag::application::dtos::tools::ToolCall;
use lakehouse_rag::infrastructure::llm::{MockAgentRunner, MockDistiller};
use lakehouse_rag::presentation::gui::chat_service::GuiChatService;
use lakehouse_rag::presentation::gui::comandos;
use serde_json::json;

const CENTRO: &str = "obra-adversarial";
const XSS: &str = "<img src=x onerror=alert('xss')><script>alert(1)</script>";

async fn centro_creado(entorno: &Entorno, activo: bool) {
    comandos::crear_centro(&entorno.app, CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    if activo {
        comandos::activar_centro(&entorno.app, CENTRO)
            .await
            .unwrap();
    }
}

fn archivos_en_raw_docs(entorno: &Entorno) -> usize {
    std::fs::read_dir(entorno.ruta_centro("a").join(DIRECTORIO_RAW_DOCS))
        .map(|d| d.count())
        .unwrap_or(0)
}

/// HTML que el backend puede emitir en `reply_html`: etiquetas de formato y, como
/// atributos, solo `class`, `title` o `style` de alineación (nunca `on…`, `href` ni `src`)
fn sin_html_vivo(html: &str) -> bool {
    const PERMITIDAS: [&str; 24] = [
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
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "div",
        "span",
        "del",
    ];
    html.split('<').skip(1).all(|resto| {
        let Some((etiqueta, _)) = resto.split_once('>') else {
            return false;
        };
        let etiqueta = etiqueta
            .trim_start_matches('/')
            .trim_end_matches('/')
            .to_lowercase();
        let mut partes = etiqueta.split_whitespace();
        let nombre = partes.next().unwrap_or_default();
        PERMITIDAS.contains(&nombre)
            && partes.all(|a| {
                a.starts_with("class=")
                    || a.starts_with("title=")
                    || a.starts_with("style=")
                    || !a.contains('=')
            })
            && !etiqueta.contains(" on")
            && !etiqueta.contains("href")
            && !etiqueta.contains("src")
    })
}

#[tokio::test]
async fn test_rutas_con_salto_al_padre_o_nulas_se_rechazan_sin_copiar_nada() {
    let entorno = entorno();
    centro_creado(&entorno, true).await;
    let (v3, _) = entorno.escribir_revisiones();
    let con_salto = format!(
        "{}/../entrada/{ARCHIVO_V3}",
        entorno.raiz.path().join("entrada").display()
    );
    for ruta in [con_salto.as_str(), "..\\..\\Windows\\win.ini.md", "", "   "] {
        let error = comandos::ingestar_documento(&entorno.app, ruta, CENTRO, None)
            .await
            .unwrap_err();
        assert!(
            error.contains("..") || error.contains("vacía"),
            "{ruta:?}: {error}"
        );
    }
    assert_eq!(archivos_en_raw_docs(&entorno), 0);
    // La ruta directa al mismo archivo sí funciona
    comandos::ingestar_documento(&entorno.app, &v3.to_string_lossy(), CENTRO, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_archivo_inexistente_directorio_o_binario_se_rechazan_con_mensaje_claro() {
    let entorno = entorno();
    centro_creado(&entorno, true).await;
    let entrada = entorno.raiz.path().join("entrada");
    std::fs::create_dir_all(entrada.join("carpeta.md")).unwrap();
    let binario = entrada.join("plano.md");
    std::fs::write(&binario, [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46]).unwrap();

    let casos = [
        (entrada.join("no_existe.md"), "No se pudo abrir"),
        (entrada.join("carpeta.md"), "no es un archivo"),
        (binario, "no es texto UTF-8"),
    ];
    for (ruta, esperado) in casos {
        let error =
            comandos::ingestar_documento(&entorno.app, &ruta.to_string_lossy(), CENTRO, None)
                .await
                .unwrap_err();
        assert!(error.contains(esperado), "{}: {error}", ruta.display());
    }
    assert_eq!(archivos_en_raw_docs(&entorno), 0);
    let catalogo = comandos::catalogo_centro(&entorno.app, CENTRO)
        .await
        .unwrap();
    assert!(catalogo.is_empty());
}

#[tokio::test]
async fn test_extensiones_no_soportadas_se_rechazan_antes_de_leer_el_archivo() {
    let entorno = entorno();
    centro_creado(&entorno, true).await;
    let entrada = entorno.raiz.path().join("entrada");
    std::fs::create_dir_all(&entrada).unwrap();
    for nombre in [
        "manual.pdf",
        "macro.exe",
        "datos.json",
        "sin_extension",
        "doc.md.exe",
    ] {
        let ruta = entrada.join(nombre);
        std::fs::write(&ruta, "# Texto válido\n").unwrap();
        let error =
            comandos::ingestar_documento(&entorno.app, &ruta.to_string_lossy(), CENTRO, None)
                .await
                .unwrap_err();
        assert!(
            error.contains("Extensión no soportada"),
            "{nombre}: {error}"
        );
    }
    assert_eq!(archivos_en_raw_docs(&entorno), 0);
}

#[tokio::test]
async fn test_centro_inactivo_guarda_la_ingesta_sin_exponerla_hasta_activarlo() {
    let entorno = entorno_con_agente(Arc::new(MockAgentRunner::new_scripted(vec![
        AgentTurnOutput::FinalMessage("no debería llegar".into()),
    ])));
    centro_creado(&entorno, false).await;
    let (v3, _) = entorno.escribir_revisiones();

    let ingesta = comandos::ingestar_documento(&entorno.app, &v3.to_string_lossy(), CENTRO, None)
        .await
        .unwrap();
    assert!(!ingesta.proyectado);
    assert!(
        ingesta
            .aviso
            .as_deref()
            .is_some_and(|a| a.contains("inactivo")),
        "{ingesta:?}"
    );
    assert!(
        comandos::catalogo_centro(&entorno.app, CENTRO)
            .await
            .is_err()
    );

    let app = Arc::new(entorno.app);
    let chat = GuiChatService::desde_asistente(app.clone());
    let error = chat
        .enviar_mensaje("¿Quién custodia?", CENTRO, "sonnet")
        .await
        .unwrap_err();
    assert!(error.contains("inactivo"), "{error}");
    assert!(
        chat.historial(CENTRO).await.is_empty(),
        "el turno fallido no queda"
    );

    comandos::activar_centro(&app, CENTRO).await.unwrap();
    let catalogo = comandos::catalogo_centro(&app, CENTRO).await.unwrap();
    assert_eq!(
        catalogo.len(),
        1,
        "al activarlo, el documento ya es visible"
    );
}

#[tokio::test]
async fn test_html_y_js_malicioso_en_documento_y_respuesta_nunca_llega_vivo() {
    let distiller = MockDistiller::new();
    let mut paquete = paquete_revision(3, "2026-03-01", "FOR-ALM-02");
    paquete.catalogo.titulo = XSS.to_string();
    paquete.hitos[1].condicion_aplicacion = Some(format!("cuando {XSS}"));
    paquete.tablas[0].celdas[1].nota_pie_condicion = Some(XSS.to_string());
    distiller.registrar_respuesta(ARCHIVO_V3, paquete);
    let agente = Arc::new(MockAgentRunner::new_scripted(vec![
        AgentTurnOutput::ToolCalls(vec![
            ToolCall {
                id: "m".into(),
                name: "query_operational_matrix".into(),
                arguments: json!({ "centro_id": CENTRO, "fase": "CUSTODIA" }),
            },
            ToolCall {
                id: "l".into(),
                name: "lookup_double_entry_table".into(),
                arguments: json!({ "centro_id": CENTRO, "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42 }),
            },
        ]),
        AgentTurnOutput::FinalMessage(format!(
            "**Resultado** {XSS} `<b onmouseover=x>` \"comillas\" & 'apóstrofos'
## {XSS}
| {XSS} | b |
|---|---|
| <svg onload=x> | c |"
        )),
    ]));
    let entorno = entorno();
    let (app, _) = construir_app(&entorno.config(), Arc::new(distiller), agente, modelo_e5());
    let (v3, _) = entorno.escribir_revisiones();
    comandos::crear_centro(&app, CENTRO, &entorno.ruta_centro_texto("x"), None)
        .await
        .unwrap();
    comandos::activar_centro(&app, CENTRO).await.unwrap();
    comandos::ingestar_documento(&app, &v3.to_string_lossy(), CENTRO, None)
        .await
        .unwrap();

    // El catálogo devuelve el texto como DATO (el frontend lo pinta con textContent)
    let catalogo = comandos::catalogo_centro(&app, CENTRO).await.unwrap();
    assert_eq!(catalogo[0].titulo, XSS);

    let chat = GuiChatService::desde_asistente(Arc::new(app));
    let respuesta = chat
        .enviar_mensaje("<script>alert(2)</script>", CENTRO, "sonnet")
        .await
        .unwrap();
    assert!(
        sin_html_vivo(&respuesta.reply_html),
        "{}",
        respuesta.reply_html
    );
    assert!(
        respuesta
            .reply_html
            .starts_with("<p><strong>Resultado</strong>")
    );
    assert!(
        respuesta
            .reply_html
            .contains("<code>&lt;b onmouseover=x&gt;</code>")
    );
    assert!(
        respuesta
            .citations
            .iter()
            .any(|c| c.condicion_aplicacion.as_deref() == Some(&format!("cuando {XSS}"))),
        "la condición viaja como dato literal: {:?}",
        respuesta.citations
    );
    let lookup = respuesta.matrix_result.unwrap();
    assert_eq!(lookup["nota_pie_condicion"], XSS);
}

#[test]
fn test_el_frontend_solo_usa_inner_html_para_la_respuesta_ya_escapada() {
    let app_js = include_str!("../src/presentation/gui/web/app.js");
    let usos: Vec<&str> = app_js
        .lines()
        .filter(|l| l.contains("innerHTML") && !l.trim_start().starts_with('*'))
        .collect();
    assert_eq!(usos.len(), 1, "{usos:?}");
    assert!(usos[0].contains("= data.replyHtml"), "{usos:?}");
    for prohibido in [
        "insertAdjacentHTML",
        "outerHTML",
        "document.write",
        "eval(",
        "onclick=",
        "new Function",
    ] {
        assert!(!app_js.contains(prohibido), "app.js usa {prohibido}");
    }
    for simulacion in [
        "documentsCatalog",
        "simulateProgress",
        "simulateToolUsePipeline",
        "Math.random",
    ] {
        assert!(
            !app_js.contains(simulacion),
            "queda la simulación {simulacion}"
        );
    }
}

#[tokio::test]
async fn test_ingestas_concurrentes_se_serializan_sin_corromper_el_centro() {
    let entorno = entorno();
    centro_creado(&entorno, true).await;
    let (v3, v4) = entorno.escribir_revisiones();
    let app = Arc::new(entorno.app);
    let lanzar = |ruta: std::path::PathBuf| {
        let app = Arc::clone(&app);
        tokio::spawn(async move {
            comandos::ingestar_documento(&app, &ruta.to_string_lossy(), CENTRO, None).await
        })
    };
    // La misma revisión dos veces y la siguiente a la vez
    let tareas = vec![lanzar(v3.clone()), lanzar(v3), lanzar(v4)];
    let mut resultados = Vec::new();
    for tarea in tareas {
        resultados.push(tarea.await.unwrap().unwrap());
    }
    let v3_ya = resultados[..2].iter().filter(|r| r.ya_ingerido).count();
    assert_eq!(
        v3_ya, 1,
        "una de las dos copias de la Rev. 03 es idempotente"
    );

    let catalogo = comandos::catalogo_centro(&app, CENTRO).await.unwrap();
    assert_eq!(catalogo.len(), 2, "{catalogo:?}");
    assert_eq!(catalogo.iter().filter(|d| d.vigente).count(), 1);
    let integridad = comandos::integridad(&app, CENTRO).await.unwrap();
    assert!(integridad.integro, "{integridad:?}");
}

#[tokio::test]
async fn test_ids_y_rutas_de_centro_hostiles_desde_la_ventana() {
    let entorno = entorno();
    for (id, ruta) in [
        ("../fuera", entorno.ruta_centro_texto("h1")),
        ("obra'; DROP TABLE x;--", entorno.ruta_centro_texto("h2")),
        (
            "obra-ok",
            format!("{}/../../escape", entorno.ruta_centro_texto("h3")),
        ),
        ("obra-ok", String::new()),
    ] {
        assert!(
            comandos::crear_centro(&entorno.app, id, &ruta, Some(XSS))
                .await
                .is_err(),
            "{id} / {ruta}"
        );
    }
    assert!(
        comandos::listar_centros(&entorno.app)
            .await
            .unwrap()
            .is_empty()
    );
    for operacion in [
        "activar",
        "desactivar",
        "eliminar",
        "catalogo",
        "integridad",
    ] {
        let resultado = match operacion {
            "activar" => comandos::activar_centro(&entorno.app, "no-existe")
                .await
                .map(|_| ()),
            "desactivar" => comandos::desactivar_centro(&entorno.app, "no-existe").await,
            "eliminar" => comandos::eliminar_centro(&entorno.app, "no-existe")
                .await
                .map(|_| ()),
            "catalogo" => comandos::catalogo_centro(&entorno.app, "no-existe")
                .await
                .map(|_| ()),
            _ => comandos::integridad(&entorno.app, "no-existe")
                .await
                .map(|_| ()),
        };
        assert!(
            resultado.is_err(),
            "{operacion} sobre un centro inexistente"
        );
    }
}
