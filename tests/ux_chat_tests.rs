//! Fase 15 — rediseño del chat (referencia: `docs/validacion/fase15/informe_chat_referencia.md`).
//! Comprobaciones estáticas del frontend: streaming por evento, render progresivo seguro,
//! botón Detener, scroll pegado al final con umbral de 24 px, columna de 768 px, burbuja del
//! usuario al 70 %, composer que crece hasta el 25 % de la altura y acciones al pasar el ratón.

const APP_JS: &str = include_str!("../src/presentation/gui/web/app.js");
const CSS: &str = include_str!("../src/presentation/gui/web/styles.css");
const HTML: &str = include_str!("../src/presentation/gui/web/index.html");

#[test]
fn test_el_chat_escucha_el_stream_renderiza_en_rust_y_puede_detenerse() {
    for necesario in [
        "listen('chat-stream'",
        "invoke('renderizar_markdown'",
        "invoke('detener_respuesta'",
        "turnoId",
    ] {
        assert!(APP_JS.contains(necesario), "app.js no contiene {necesario}");
    }
}

#[test]
fn test_scroll_pegado_al_final_con_umbral_de_24_px_y_ancla_del_ultimo_mensaje() {
    assert!(APP_JS.contains("UMBRAL_PEGADO_PX = 24"));
    assert!(
        APP_JS.contains("espaciador-ancla"),
        "falta el espaciador que ancla la pregunta"
    );
    let regla_mensajes = CSS
        .lines()
        .find(|l| l.starts_with(".chat-messages {"))
        .expect("regla .chat-messages");
    assert!(
        !regla_mensajes.contains("smooth"),
        "el auto-scroll del streaming no debe ser animado: {regla_mensajes}"
    );
}

#[test]
fn test_maqueta_columna_burbuja_composer_y_acciones() {
    for regla in [
        "--ancho-chat: 768px",
        "max-width: 70%",
        "border-radius: 22px",
        "max-height: 25vh",
        ".acciones-respuesta { opacity: 0",
    ] {
        assert!(CSS.contains(regla), "styles.css no contiene «{regla}»");
    }
    assert!(HTML.contains("id=\"btn-send\""));
    assert!(
        HTML.contains("class=\"composer-pie\""),
        "el selector de modelo y el botón viven en el pie del composer"
    );
}

/// Regresión (verificación real): poner el espaciador a 0 para medirlo recorta el scrollTop y
/// suelta la vista del final durante el streaming; se mide sin colapsarlo
#[test]
fn test_el_espaciador_se_mide_sin_colapsarlo() {
    assert!(
        !APP_JS.contains("espaciador.style.height = '0px'"),
        "colapsar el espaciador recorta el scroll"
    );
}

/// Regresión (verificación real): el mensaje del asistente se insertaba delante del espaciador
/// del turno ANTERIOR, es decir, encima de la nueva pregunta
#[test]
fn test_la_respuesta_se_anade_despues_de_su_pregunta() {
    assert!(
        !APP_JS.contains("insertBefore(mensaje"),
        "la respuesta quedaría encima de la pregunta"
    );
}
