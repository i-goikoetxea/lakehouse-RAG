//! Integración de las citas de página exacta (`docs/arquitectura/ssd_citas_pagina.md`): los saltos
//! viajan PDF → LanceDB → proyección → tools, y las marcas solo existen en la respuesta.

#[path = "comun_centros/mod.rs"]
mod comun_centros;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;
#[path = "comun_store_v2/mod.rs"]
mod comun_store_v2;
#[path = "comun_tools_stair/mod.rs"]
mod comun_tools_stair;

use comun_tools_stair::*;
use lakehouse_rag::domain::services::marcas_pagina::{quitar_marcas, tramos_paginados};
use serde_json::{Value, json};

const BUSCAR: &str = "search_normative_context";
const SUBARBOL: &str = "stair_get_subtree_context";
const SQL: &str = "execute_athena_sql";
const RELLENO: &str = "Texto suficiente para que la página cuente como extraíble por el extractor";
const FRASE_PAGINA_2: &str = "Frase con el termino zanahoria en la segunda pagina";

async fn con_pdf_paginado() -> (EntornoStair, String) {
    let e = entorno_stair().await;
    let bytes = comun_pdf::pdf(&[
        &["1 Alcance general", RELLENO],
        &[RELLENO, FRASE_PAGINA_2],
        &["2 Ensayos finales", RELLENO],
    ]);
    let ruta = e.escribir("informe_paginado.pdf", &bytes);
    let doc_id = e.app.ingestar(&ruta, CENTRO).await.unwrap().informe.doc_id;
    (e, doc_id)
}

async fn buscar_zanahoria(e: &EntornoStair, doc_id: &str) -> Value {
    let salida = ok(
        e,
        BUSCAR,
        args(json!({ "consulta": "zanahoria", "modo": "EXACT_BM25", "doc_id": doc_id })),
    )
    .await;
    resultados(&salida)[0].clone()
}

async fn fila_de_chunk(e: &EntornoStair, chunk_id: &str) -> Value {
    let sql = format!(
        "SELECT texto_contenido, saltos_pagina FROM \"{CENTRO}_02_chunks_normativa\" WHERE chunk_id = '{chunk_id}'"
    );
    ok(e, SQL, args(json!({ "sql_query": sql }))).await["resultado"][0].clone()
}

#[tokio::test]
async fn test_busqueda_marca_la_pagina_del_dato_y_no_toca_lo_guardado() {
    let (e, doc_id) = con_pdf_paginado().await;
    let resultado = buscar_zanahoria(&e, &doc_id).await;
    let texto = resultado["texto"].as_str().unwrap();
    assert!(texto.starts_with("⟦p. 1⟧ "), "{texto}");
    let marca = texto.find("⟦p. 2⟧").expect("marca de la página 2");
    assert!(marca < texto.find("zanahoria").unwrap(), "{texto}");
    assert_eq!(resultado["cita"]["paginas"], "1-2");

    let tramos = tramos_paginados(texto);
    let del_dato = tramos
        .iter()
        .find(|t| t.texto.contains("zanahoria"))
        .unwrap();
    assert_eq!(del_dato.pagina, Some(2));
    assert!(tramos[0].texto.contains("Alcance") || tramos[0].texto.contains("Texto"));
    assert_eq!(tramos[0].pagina, Some(1));

    let fila = fila_de_chunk(&e, resultado["id"].as_str().unwrap()).await;
    let guardado = fila["texto_contenido"].as_str().unwrap();
    assert!(
        !guardado.contains('⟦'),
        "las marcas no se guardan: {guardado}"
    );
    assert_eq!(quitar_marcas(texto), guardado);
    let saltos = fila["saltos_pagina"].as_array().unwrap();
    assert_eq!(saltos[0], json!({ "desplazamiento": 0, "pagina": 1 }));
    assert_eq!(saltos[1]["pagina"], 2);
    let desplazamiento = saltos[1]["desplazamiento"].as_u64().unwrap() as usize;
    assert!(
        guardado[desplazamiento..].starts_with(RELLENO),
        "{guardado}"
    );
}

#[tokio::test]
async fn test_subarbol_marca_cada_tramo_y_las_marcas_no_cuentan_como_caracteres() {
    let (e, doc_id) = con_pdf_paginado().await;
    let nodo = buscar_zanahoria(&e, &doc_id).await["cita"]["nodo_id"]
        .as_str()
        .unwrap()
        .to_string();
    let seccion = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": doc_id, "nodo_id": nodo })),
    )
    .await;
    let texto = seccion["secciones"][0]["texto"].as_str().unwrap();
    assert!(texto.starts_with("⟦p. 1⟧ "), "{texto}");
    assert!(texto.contains("⟦p. 2⟧ "), "{texto}");
    let limpio = quitar_marcas(texto);
    assert_eq!(
        seccion["caracteres_entregados"].as_u64().unwrap() as usize,
        limpio.chars().count()
    );
    assert!(seccion["nota"].as_str().unwrap().contains("⟦p. N⟧"));
    assert!(seccion["cita"]["titulo"].is_string());

    // A mitad de la sección, el tramo empieza con la página de su primer carácter
    let desde = limpio.chars().count() - FRASE_PAGINA_2.chars().count();
    let resto = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": doc_id, "nodo_id": nodo, "desde_caracter": desde })),
    )
    .await;
    assert_eq!(
        resto["secciones"][0]["texto"],
        format!("⟦p. 2⟧ {FRASE_PAGINA_2}")
    );
}

#[tokio::test]
async fn test_markdown_no_lleva_marcas_y_la_cita_trae_titulo_y_fecha_si_consta() {
    let e = entorno_stair().await;
    let salida = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "durmientes de madera tratada", "modo": "EXACT_BM25" })),
    )
    .await;
    assert!(salida["nota"].as_str().unwrap().contains("⟦p. N⟧"));
    let todos = resultados(&salida);
    assert!(!todos.is_empty());
    assert!(
        todos
            .iter()
            .all(|r| !r["texto"].as_str().unwrap().contains('⟦')),
        "Markdown no tiene páginas"
    );
    let del_procedimiento = todos.iter().find(|r| r["cita"]["doc_id"] == DOC).unwrap();
    assert_eq!(
        del_procedimiento["cita"]["titulo"],
        format!("Documento {DOC}")
    );
    assert_eq!(del_procedimiento["cita"]["fecha_vigor"], "2026-09-15");
    assert_eq!(del_procedimiento["cita"]["paginas"], Value::Null);

    // El provisional no declara fecha: la clave no aparece
    let provisional = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "bocina al girar", "modo": "EXACT_BM25" })),
    )
    .await;
    let cita = &resultados(&provisional)[0]["cita"];
    assert!(cita["titulo"].is_string(), "{cita}");
    assert!(
        cita.as_object().unwrap().get("fecha_vigor").is_none(),
        "{cita}"
    );
}

#[tokio::test]
async fn test_el_orden_de_resultados_no_depende_de_las_marcas() {
    let (e, doc_id) = con_pdf_paginado().await;
    // «p» y los números de página no son términos del texto guardado: no puntúan
    let salida = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "⟦p. 2⟧", "modo": "EXACT_BM25", "doc_id": doc_id })),
    )
    .await;
    assert_eq!(salida["total"], 0, "{salida}");
}
