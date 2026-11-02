//! Integración de la Fase 4 de STAIR sobre un centro real en disco: búsqueda evolucionada,
//! `stair_navigate_toc`, `stair_get_subtree_context` y reintento de destilación.

#[path = "comun_centros/mod.rs"]
mod comun_centros;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;
#[path = "comun_store_v2/mod.rs"]
mod comun_store_v2;
#[path = "comun_tools_stair/mod.rs"]
mod comun_tools_stair;

use std::path::Path;

use comun_tools_stair::*;
use lakehouse_rag::presentation::cli::ejecutor::{CODIGO_ERROR, CODIGO_OK, ejecutar};
use lakehouse_rag::presentation::cli::{ComandoCli, FormatoSalida};
use lakehouse_rag::presentation::gui::comandos;
use serde_json::json;

const BUSCAR: &str = "search_normative_context";
const TOC: &str = "stair_navigate_toc";
const SUBARBOL: &str = "stair_get_subtree_context";

// --- Búsqueda ---

#[tokio::test]
async fn test_modos_de_busqueda_lexico_semantico_e_hibrido() {
    let e = entorno_stair().await;
    let sin_lexico = "zzzqqqxxx";
    let bm25 = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": sin_lexico, "modo": "EXACT_BM25" })),
    )
    .await;
    assert_eq!(bm25["total"], 0, "BM25 no inventa coincidencias");
    let semantico = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": sin_lexico, "modo": "SEMANTIC_ONLY" })),
    )
    .await;
    assert!(
        semantico["total"].as_u64().unwrap() > 0,
        "el modo vectorial siempre ordena"
    );
    assert_eq!(semantico["modo"], "SEMANTIC_ONLY");

    let exacta = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "durmientes", "modo": "EXACT_BM25" })),
    )
    .await;
    assert!(exacta["total"].as_u64().unwrap() >= 1);
    assert!(resultados(&exacta).iter().all(|r| {
        r["texto"].as_str().unwrap().contains("durmientes")
            || r["breadcrumb_jerarquia"]
                .as_str()
                .unwrap()
                .contains("Durmientes")
    }));

    let por_defecto = ok(&e, BUSCAR, args(json!({ "consulta": "durmientes" }))).await;
    assert_eq!(por_defecto["modo"], "HYBRID");

    let invalido = error(
        &e,
        BUSCAR,
        args(json!({ "consulta": "x", "modo": "MAGICO" })),
    )
    .await;
    assert!(invalido.contains("Argumentos inválidos"), "{invalido}");
}

#[tokio::test]
async fn test_busqueda_cita_doc_revision_breadcrumb_y_estado_completo() {
    let e = entorno_stair().await;
    let salida = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "durmientes", "modo": "EXACT_BM25" })),
    )
    .await;
    let r = &resultados(&salida)[0];
    assert_eq!(r["cita"]["doc_id"], DOC);
    assert_eq!(r["cita"]["revision"], "Rev. 02");
    assert!(
        r["cita"]["breadcrumb"]
            .as_str()
            .unwrap()
            .contains("Durmientes")
    );
    assert!(
        r["cita"]["nodo_id"]
            .as_str()
            .unwrap()
            .starts_with("PR-STAIR-01#sec-")
    );
    assert_eq!(r["estado_contenido"], "COMPLETO");
    assert_eq!(r["tiene_capa_2"], true);
    assert!(
        r["condicion_aplicacion"]
            .as_str()
            .unwrap()
            .contains("7 días")
    );
}

#[tokio::test]
async fn test_busqueda_filtra_por_subarbol_con_nodo_y_prefijo_seguro() {
    let e = entorno_stair().await;
    let custodia = capitulo(&e, "Custodia").await;
    let salida = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "tubería", "modo": "EXACT_BM25", "nodo_id": custodia })),
    )
    .await;
    assert!(salida["total"].as_u64().unwrap() >= 1);
    for r in resultados(&salida) {
        assert!(
            r["breadcrumb_jerarquia"]
                .as_str()
                .unwrap()
                .contains("Custodia"),
            "{r}"
        );
    }

    // El prefijo /…/2.1 no captura /…/2.10
    let hijos = toc(&e, Some(&custodia), None).await;
    let acopio = hijos["nodos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["titulo_seccion"].as_str().unwrap().contains("Acopio"))
        .unwrap()
        .clone();
    let prefijo = acopio["ruta_linaje"].as_str().unwrap();
    let filtrada = ok(
        &e,
        BUSCAR,
        args(
            json!({ "consulta": "devolución durmientes", "modo": "EXACT_BM25",
                     "doc_id": DOC, "ruta_linaje_prefijo": prefijo }),
        ),
    )
    .await;
    let textos = resultados(&filtrada);
    assert!(!textos.is_empty());
    assert!(
        textos
            .iter()
            .all(|r| !r["texto"].as_str().unwrap().contains("devolución"))
    );
}

#[tokio::test]
async fn test_busqueda_con_seccion_inexistente_es_error_y_no_inventa() {
    let e = entorno_stair().await;
    let nodo = error(
        &e,
        BUSCAR,
        args(json!({ "consulta": "tubería", "nodo_id": "PR-STAIR-01#sec-99.99" })),
    )
    .await;
    assert!(nodo.contains("no existe"), "{nodo}");
    let ruta = error(
        &e,
        BUSCAR,
        args(json!({ "consulta": "tubería", "doc_id": DOC, "ruta_linaje_prefijo": "/99" })),
    )
    .await;
    assert!(ruta.contains("no existe"), "{ruta}");
    let sin_doc = error(
        &e,
        BUSCAR,
        args(json!({ "consulta": "tubería", "ruta_linaje_prefijo": "/1" })),
    )
    .await;
    assert!(sin_doc.contains("doc_id"), "{sin_doc}");
}

#[tokio::test]
async fn test_provisional_se_distingue_y_queda_visible_aunque_no_coincida() {
    let e = entorno_stair().await;
    let salida = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "carretillas bocina", "modo": "EXACT_BM25" })),
    )
    .await;
    let r = resultados(&salida)
        .into_iter()
        .find(|r| r["texto"].as_str().unwrap().contains("bocina"))
        .unwrap();
    assert_eq!(r["estado_contenido"], "PARCIAL_UNIVERSAL");
    assert_eq!(r["tiene_capa_2"], false);
    assert_eq!(r["provisional"], true);
    assert_eq!(
        r["prevalece"], false,
        "un provisional nunca se impone a lo vigente sin validar: {r}"
    );
    assert!(r["aviso_contenido"].as_str().unwrap().contains("Capa 2"));

    // Una consulta sobre otro tema también informa de que hay documentos sin destilar
    let otra = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "durmientes", "modo": "EXACT_BM25" })),
    )
    .await;
    let lista = otra["documentos_no_definitivos"].as_array().unwrap();
    assert!(lista.iter().any(|d| {
        d["estado_contenido"] == "PARCIAL_UNIVERSAL"
            && d["doc_id"]
                .as_str()
                .unwrap()
                .starts_with("MANUAL-SIN-DESTILAR")
    }));
}

#[tokio::test]
async fn test_requiere_ocr_se_explica_en_busqueda_y_toc() {
    let e = entorno_stair().await;
    let otra = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "durmientes", "modo": "EXACT_BM25" })),
    )
    .await;
    let lista = otra["documentos_no_definitivos"].as_array().unwrap();
    let escaneado = lista
        .iter()
        .find(|d| d["estado_contenido"] == "REQUIERE_OCR")
        .unwrap();
    assert!(escaneado["explicacion"].as_str().unwrap().contains("OCR"));

    let doc = escaneado["doc_id"].as_str().unwrap();
    let indice = ok(&e, TOC, args(json!({ "doc_id": doc }))).await;
    assert_eq!(indice["estado_contenido"], "REQUIERE_OCR");
    assert!(indice["aviso_contenido"].as_str().unwrap().contains("OCR"));
}

#[tokio::test]
async fn test_pdf_cita_paginas_en_busqueda_y_subarbol() {
    let e = entorno_stair().await;
    let relleno = "Texto suficiente para que la página cuente como extraíble por el extractor";
    let bytes = comun_pdf::pdf(&[
        &["1 Alcance general", relleno],
        &[
            relleno,
            "Frase con el termino zanahoria en la segunda pagina",
        ],
        &["2 Ensayos finales", relleno],
    ]);
    let ruta = e.escribir("informe_paginado.pdf", &bytes);
    let informe = e.app.ingestar(&ruta, CENTRO).await.unwrap().informe;
    let salida = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "zanahoria", "modo": "EXACT_BM25", "doc_id": informe.doc_id })),
    )
    .await;
    let r = &resultados(&salida)[0];
    assert_eq!(r["cita"]["pagina_inicio"], 1, "{r}");
    assert_eq!(r["cita"]["paginas"], "1-2", "{r}");

    let nodo = r["cita"]["nodo_id"].as_str().unwrap();
    let seccion = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": informe.doc_id, "nodo_id": nodo })),
    )
    .await;
    assert_eq!(seccion["secciones"][0]["cita"]["paginas"], "1-2");
    assert_eq!(seccion["cita"]["pagina_inicio"], 1);
}

// --- ToC ---

#[tokio::test]
async fn test_toc_raices_e_hijos_en_orden_natural_con_datos_de_cada_nodo() {
    let e = entorno_stair().await;
    let raices = toc(&e, None, None).await;
    let primera = &raices["nodos"][0];
    assert_eq!(primera["codigo_seccion"], "0.0", "el preámbulo va primero");
    assert_eq!(raices["revision"], "Rev. 02");

    let custodia = capitulo(&e, "Custodia").await;
    let hijos = toc(&e, Some(&custodia), None).await;
    let titulos: Vec<&str> = hijos["nodos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["titulo_seccion"].as_str().unwrap())
        .collect();
    assert_eq!(titulos.len(), 3);
    assert!(
        titulos[0].contains("Acopio")
            && titulos[1].contains("Traslado")
            && titulos[2].contains("Devolución"),
        "2.2 va antes que 2.10: {titulos:?}"
    );
    let acopio = &hijos["nodos"][0];
    assert_eq!(acopio["tiene_hijos"], true);
    assert_eq!(hijos["nodos"][1]["tiene_hijos"], false);
    assert!(
        acopio["condiciones_alcance"]
            .as_str()
            .unwrap()
            .contains("7 días")
    );
    assert!(!acopio["resumen_seccion"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_toc_viaja_en_el_tiempo_sobre_07() {
    let e = entorno_stair().await;
    let raiz = raiz_procedimiento(&e).await;
    let actual = toc(&e, Some(&raiz), None).await;
    assert!(actual.to_string().contains("Archivo"));
    let pasada = toc(&e, Some(&raiz), Some(FECHA_V1)).await;
    assert_eq!(pasada["revision"], "Rev. 01");
    assert!(
        !pasada.to_string().contains("Archivo"),
        "la sección 8 no existía en la Rev. 01"
    );
    let antes = error(
        &e,
        TOC,
        args(json!({ "doc_id": DOC, "fecha_referencia": "2020-01-01" })),
    )
    .await;
    assert!(antes.contains("2020-01-01"), "{antes}");
}

#[tokio::test]
async fn test_toc_aislado_por_centro_y_doc_inexistente_util() {
    let e = entorno_stair().await;
    let mensaje = error(&e, TOC, args(json!({ "doc_id": DOC_AJENO }))).await;
    assert!(mensaje.contains("inspect_lakehouse_catalog"), "{mensaje}");
    assert!(!mensaje.contains("reservado"));
    let suyo = ok(
        &e,
        TOC,
        json!({ "centro_id": CENTRO_AJENO, "doc_id": DOC_AJENO }),
    )
    .await;
    assert_eq!(suyo["doc_id"], DOC_AJENO);
}

#[tokio::test]
async fn test_toc_acota_la_respuesta_y_permite_seguir() {
    let e = entorno_stair().await;
    // El límite se prueba con desplazamiento: pedir más allá del final no inventa nodos
    let raiz = raiz_procedimiento(&e).await;
    let fuera = ok(
        &e,
        TOC,
        args(json!({ "doc_id": DOC, "nodo_padre_id": raiz, "desplazamiento": 500 })),
    )
    .await;
    assert_eq!(fuera["nodos"].as_array().unwrap().len(), 0);
    assert_eq!(fuera["truncado"], false);
    assert_eq!(fuera["total"], 8);
}

// --- Subárbol ---

#[tokio::test]
async fn test_subarbol_une_descendientes_en_orden_con_condicion_heredada() {
    let e = entorno_stair().await;
    let custodia = capitulo(&e, "Custodia").await;
    let hijos = toc(&e, Some(&custodia), None).await;
    let acopio = nodo_titulado(&hijos, "Acopio");
    let salida = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": acopio })),
    )
    .await;
    let secciones = salida["secciones"].as_array().unwrap();
    assert_eq!(secciones.len(), 2, "el nodo y su hijo: {salida}");
    assert!(
        secciones[1]["titulo_seccion"]
            .as_str()
            .unwrap()
            .contains("Durmientes")
    );
    assert!(
        secciones[1]["texto"]
            .as_str()
            .unwrap()
            .contains(FRASE_ACOPIO)
    );
    assert!(
        secciones[1]["condiciones_heredadas"]
            .to_string()
            .contains("7 días"),
        "{}",
        secciones[1]
    );
    assert_eq!(salida["truncado"], false);
    assert_eq!(salida["cita"]["doc_id"], DOC);
    assert_eq!(salida["cita"]["revision"], "Rev. 02");
    assert!(
        salida["nota_seguridad"]
            .as_str()
            .unwrap()
            .contains("instrucciones")
    );

    // Desde el padre, la condición del hijo no se atribuye a la sección padre
    let desde_custodia = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": custodia })),
    )
    .await;
    let padre = &desde_custodia["secciones"][0];
    assert!(
        !padre["condiciones_heredadas"]
            .to_string()
            .contains("7 días"),
        "{padre}"
    );
}

#[tokio::test]
async fn test_subarbol_reconstruye_sin_duplicar_el_solape_y_sin_perder_texto() {
    let e = entorno_stair().await;
    let clausulas = capitulo(&e, "Cláusulas").await;
    let salida = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": clausulas })),
    )
    .await;
    let seccion = &salida["secciones"][0];
    assert!(
        seccion["chunks"].as_array().unwrap().len() > 1,
        "debe haber varias ventanas"
    );
    let texto = seccion["texto"].as_str().unwrap();
    assert_eq!(texto.trim(), clausula_extensa(), "ni duplicado ni pérdida");
}

#[tokio::test]
async fn test_subarbol_sin_solape_titulos_repetidos_y_nodo_sin_texto() {
    let e = entorno_stair().await;
    let alfa = capitulo(&e, "Anexo alfa").await;
    let salida = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": alfa })),
    )
    .await;
    let texto = salida["secciones"].to_string();
    assert!(texto.contains(FRASE_ANEXO_A));
    assert!(
        !texto.contains(FRASE_ANEXO_B),
        "dos «Alcance» no se mezclan por breadcrumb"
    );

    let soldadura = capitulo(&e, "Soldadura").await;
    let sin_texto = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": soldadura })),
    )
    .await;
    assert_eq!(
        sin_texto["secciones"][0]["texto"], "",
        "un título sin texto no inventa contenido"
    );
}

#[tokio::test]
async fn test_subarbol_time_travel_y_nodo_inexistente_util() {
    let e = entorno_stair().await;
    let archivo = capitulo(&e, "Archivo").await;
    let actual = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": archivo })),
    )
    .await;
    assert!(actual.to_string().contains("cinco años"));
    let pasado = error(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": archivo, "fecha_referencia": FECHA_V1 })),
    )
    .await;
    assert!(
        pasado.contains("no existe") && pasado.contains("stair_navigate_toc"),
        "{pasado}"
    );
}

// --- Alias provisional y reintento ---

#[tokio::test]
async fn test_reintento_por_cli_resuelve_alias_y_gui_refleja_el_estado() {
    let e = entorno_stair().await;
    let catalogo = comandos::catalogo_centro(&e.app, CENTRO).await.unwrap();
    let manual = catalogo
        .iter()
        .find(|d| d.doc_id.starts_with("MANUAL-SIN-DESTILAR"))
        .unwrap();
    assert_eq!(
        manual.estado_contenido.as_deref(),
        Some("PARCIAL_UNIVERSAL")
    );
    assert!(manual.pendiente_destilacion);
    let provisional = manual.doc_id.clone();
    let vigente = catalogo
        .iter()
        .find(|d| d.doc_id == DOC && d.vigente)
        .unwrap();
    assert_eq!(vigente.estado_contenido.as_deref(), Some("COMPLETO"));
    assert!(!vigente.pendiente_destilacion);

    // Claude sigue caído: el error llega y el provisional queda intacto
    let fallo = ejecutar(
        &e.app,
        ComandoCli::Reintentar {
            doc_id: provisional.clone(),
            centro: CENTRO.to_string(),
        },
        FormatoSalida::Json,
    )
    .await;
    assert_eq!(fallo.codigo, CODIGO_ERROR, "{fallo:?}");
    assert!(ok(&e, TOC, args(json!({ "doc_id": provisional }))).await["provisional"] == true);

    e.restaurar_claude();
    let hecho = ejecutar(
        &e.app,
        ComandoCli::Reintentar {
            doc_id: provisional.clone(),
            centro: CENTRO.to_string(),
        },
        FormatoSalida::Json,
    )
    .await;
    assert_eq!(hecho.codigo, CODIGO_OK, "{hecho:?}");
    let informe: serde_json::Value = serde_json::from_str(&hecho.stdout).unwrap();
    assert_eq!(informe["doc_id"], "MAN-CARR-07");
    assert_eq!(informe["estado_contenido"], "COMPLETO");

    // El identificador antiguo sigue resolviendo como alias, también en nodo_id
    let alias = ok(&e, TOC, args(json!({ "doc_id": provisional }))).await;
    assert_eq!(alias["doc_id"], "MAN-CARR-07");
    assert_eq!(
        alias["alias_resuelto_de"].as_str(),
        Some(provisional.as_str())
    );
    assert_eq!(alias["provisional"], false);
    let nodo_antiguo = format!("{provisional}#sec-preambulo");
    let raiz = alias["nodos"][0]["nodo_id"].as_str().unwrap().to_string();
    let segmento = raiz.split_once('#').unwrap().1;
    let por_alias = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": provisional, "nodo_id": format!("{provisional}#{segmento}") })),
    )
    .await;
    assert_eq!(por_alias["nodo"]["nodo_id"], raiz, "{nodo_antiguo}");
    let busqueda = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "bocina", "doc_id": provisional, "modo": "EXACT_BM25" })),
    )
    .await;
    assert_eq!(busqueda["total"], 1);
    assert_eq!(resultados(&busqueda)[0]["cita"]["doc_id"], "MAN-CARR-07");
}

#[tokio::test]
async fn test_reintento_por_comando_gui_y_doc_definitivo_es_idempotente() {
    let e = entorno_stair().await;
    let definitivo = comandos::reintentar_destilacion(&e.app, DOC, CENTRO)
        .await
        .unwrap();
    assert!(
        definitivo.ya_ingerido,
        "un documento definitivo no vuelve a Claude"
    );

    let desconocido = comandos::reintentar_destilacion(&e.app, "NO-EXISTE", CENTRO)
        .await
        .unwrap_err();
    assert!(desconocido.contains("NO-EXISTE"), "{desconocido}");

    let catalogo = comandos::catalogo_centro(&e.app, CENTRO).await.unwrap();
    let manual = catalogo
        .iter()
        .find(|d| d.doc_id.starts_with("MANUAL-SIN-DESTILAR"))
        .unwrap()
        .doc_id
        .clone();
    e.restaurar_claude();
    let hecho = comandos::reintentar_destilacion(&e.app, &manual, CENTRO)
        .await
        .unwrap();
    assert_eq!(hecho.doc_id, "MAN-CARR-07");
    assert_eq!(hecho.estado_contenido, "COMPLETO");
    assert!(!hecho.pendiente_destilacion);
    assert!(hecho.proyectado);
}

#[tokio::test]
async fn test_informe_de_ingesta_de_la_gui_lleva_estado_y_pendiente() {
    let e = entorno_stair().await;
    let ruta = e.escribir(
        "otro_manual.md",
        b"# Otro\n\n## 1. Uno\nTexto de otro manual.\n",
    );
    let informe = comandos::ingestar_documento(&e.app, ruta.to_str().unwrap(), CENTRO, None)
        .await
        .unwrap();
    assert_eq!(informe.estado_contenido, "PARCIAL_UNIVERSAL");
    assert!(informe.pendiente_destilacion);
    assert!(informe.aviso.is_some());
    assert!(Path::new(&ruta).exists());
}

#[tokio::test]
async fn test_inspeccion_del_catalogo_muestra_estado_y_provisional() {
    let e = entorno_stair().await;
    let salida = ok(&e, "inspect_lakehouse_catalog", args(json!({}))).await;
    let documentos = salida["documentos"].as_array().unwrap();
    let manual = documentos
        .iter()
        .find(|d| {
            d["doc_id"]
                .as_str()
                .unwrap()
                .starts_with("MANUAL-SIN-DESTILAR")
        })
        .unwrap();
    assert_eq!(manual["estado_contenido"], "PARCIAL_UNIVERSAL");
    assert_eq!(manual["provisional"], true);
    assert!(
        manual["aviso_contenido"]
            .as_str()
            .unwrap()
            .contains("Capa 2")
    );
    let vigente = documentos.iter().find(|d| d["doc_id"] == DOC).unwrap();
    assert_eq!(vigente["estado_contenido"], "COMPLETO");
    assert_eq!(vigente["provisional"], false);
}
