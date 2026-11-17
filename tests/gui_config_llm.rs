//! Botón de configuración y modal del motor del LLM y de la API key (estático: la ventana no se
//! abre en los tests). La lógica vive en `presentation::gui::config_llm` y tiene sus tests.

/// Sin retornos de carro: con `core.autocrlf` el checkout de Windows trae CRLF
fn sin_cr(texto: &str) -> String {
    texto.replace(char::from(13), "")
}

fn html() -> String {
    sin_cr(include_str!("../src/presentation/gui/web/index.html"))
}

fn app_js() -> String {
    sin_cr(include_str!("../src/presentation/gui/web/app.js"))
}

fn gui_rs() -> String {
    sin_cr(include_str!("../src/presentation/gui/mod.rs"))
}

fn bloque<'a>(texto: &'a str, desde: &str, hasta: &str) -> &'a str {
    let inicio = texto.find(desde).unwrap_or_else(|| panic!("falta {desde}"));
    let fin = inicio
        + texto[inicio..]
            .find(hasta)
            .unwrap_or_else(|| panic!("falta {hasta}"));
    &texto[inicio..fin]
}

#[test]
fn test_el_engranaje_va_tras_el_tema_y_antes_de_los_controles_de_ventana() {
    let html = html();
    let acciones = bloque(&html, "class=\"titlebar-actions\"", "</header>");
    let tema = acciones
        .find("id=\"btn-theme-toggle\"")
        .expect("botón de tema");
    let engranaje = acciones
        .find("id=\"btn-config\"")
        .expect("botón de configuración");
    let ventana = acciones
        .find("class=\"window-controls\"")
        .expect("controles de ventana");
    assert!(tema < engranaje && engranaje < ventana);
}

#[test]
fn test_el_engranaje_es_un_svg_monocromo_con_nombre_accesible() {
    let html = html();
    let boton = bloque(&html, "id=\"btn-config\"", "</button>");
    assert!(
        boton.contains("<svg") && boton.contains("currentColor"),
        "{boton}"
    );
    assert!(boton.contains("aria-label=\"Configuración\""), "{boton}");
    assert!(
        !boton.chars().any(|c| c as u32 >= 0x2600),
        "emoji en el botón: {boton}"
    );
}

#[test]
fn test_el_modal_tiene_selector_de_modo_y_campo_password() {
    let html = html();
    let modal = bloque(&html, "id=\"dialogo-config\"", "</dialog>");
    assert!(modal.contains("name=\"config-modo\" value=\"cli\""));
    assert!(modal.contains("name=\"config-modo\" value=\"api\""));
    assert!(
        modal.contains("id=\"config-en-uso\""),
        "muestra el motor en uso"
    );
    let campo = bloque(modal, "id=\"config-key-input\"", ">");
    assert!(campo.contains("type=\"password\""), "{campo}");
    assert!(campo.contains("autocomplete=\"off\""), "{campo}");
    assert!(
        !modal.contains("value=\"sk-ant"),
        "el HTML no trae ninguna key"
    );
}

#[test]
fn test_la_key_no_se_retiene_en_js_y_el_campo_se_vacia() {
    let app_js = app_js();
    let config = bloque(&app_js, "// 14b. Configuración", "// 15. Arranque");
    assert!(
        config.contains("invoke('guardar_api_key', { key: campoKey.value })"),
        "la key va directa del campo al comando, sin variable intermedia"
    );
    let envio = bloque(config, "$('form-api-key')", "await Promise.all([cargarConfig(), cargarModelos()]);\n  });");
    assert!(
        envio.contains("finally {\n      vaciarCampoKey();"),
        "{envio}"
    );
    assert!(config.contains("dialogoConfig.addEventListener('close', vaciarCampoKey)"));
    for prohibido in [
        "console.",
        "localStorage",
        "guardar(",
        "innerHTML",
        "sessionStorage",
    ] {
        assert!(
            !config.contains(prohibido),
            "el bloque de configuración usa {prohibido}"
        );
    }
    assert_eq!(
        config.matches("campoKey.value").count(),
        3,
        "solo se lee para validar y enviar, y se vacía"
    );
}

#[test]
fn test_los_comandos_de_configuracion_estan_registrados() {
    let gui_rs = gui_rs();
    let registro = bloque(&gui_rs, "generate_handler![", "]");
    for comando in [
        "estado_api_key",
        "guardar_api_key",
        "estado_modo_llm",
        "guardar_modo_llm",
        "guardar_url_freellmapi",
        "estado_freellmapi_key",
        "guardar_freellmapi_key",
    ] {
        assert!(
            registro.contains(comando),
            "falta {comando} en el invoke_handler"
        );
        assert!(
            gui_rs.contains(&format!("fn {comando}(")),
            "falta el comando {comando}"
        );
    }
}

#[test]
fn test_el_modal_ofrece_freellmapi_con_su_aviso_url_y_campo_password() {
    let html = html();
    let modal = bloque(&html, "id=\"dialogo-config\"", "</dialog>");
    assert!(modal.contains("name=\"config-modo\" value=\"freellmapi\""));
    assert!(modal.contains(
        "Modelos gratuitos de terceros; los prompts y extractos se envían a esos proveedores; calidad no medida"
    ));
    assert!(modal.contains("id=\"config-freellmapi-url\""));
    let campo = bloque(modal, "id=\"config-freellmapi-key-input\"", ">");
    assert!(campo.contains("type=\"password\""), "{campo}");
    assert!(campo.contains("autocomplete=\"off\""), "{campo}");
    assert!(
        !modal.contains("value=\"freellmapi-"),
        "el HTML no trae ninguna key"
    );
}

#[test]
fn test_la_key_del_proxy_no_se_retiene_en_js_y_el_campo_se_vacia() {
    let app_js = app_js();
    let config = bloque(&app_js, "// 14b. Configuración", "// 15. Arranque");
    assert!(
        config.contains("invoke('guardar_freellmapi_key', { key: campoKeyProxy.value })"),
        "la key va directa del campo al comando"
    );
    let envio = bloque(
        config,
        "$('form-freellmapi-key')",
        "await Promise.all([cargarConfig(), cargarModelos()]);
  });",
    );
    assert!(
        envio.contains(
            "finally {
      campoKeyProxy.value = '';"
        ),
        "{envio}"
    );
    assert_eq!(
        config.matches("campoKeyProxy.value").count(),
        4,
        "solo se lee para validar y enviar, y se vacía (al enviar y al cerrar)"
    );
}

#[test]
fn test_el_desplegable_de_modelos_pinta_con_textcontent_y_aguanta_un_proxy_caido() {
    let app_js = app_js();
    let modelos = bloque(
        &app_js,
        "async function cargarModelos()",
        "function etiquetaModelo()",
    );
    assert!(!modelos.contains("innerHTML"), "{modelos}");
    assert!(modelos.contains("el('option', null, m.displayName)"));
    assert!(modelos.contains("Catálogo de modelos no disponible"));
    assert!(modelos.contains("selectModelo.disabled = true"));
}
