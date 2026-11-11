//! Integración de la página exacta de la Capa 2 (`ssd_generalidad_4_piezas.md` §2): un PDF
//! sintético de 3 páginas con una tabla partida entre las pp. 2 y 3 llega con su bloque a 04, con
//! la página de cada celda a 05 y con `cita.paginas` a `lookup_double_entry_table`.

#[path = "comun_centros/mod.rs"]
mod comun_centros;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;
#[path = "comun_store_v2/mod.rs"]
mod comun_store_v2;
#[path = "comun_tools_stair/mod.rs"]
mod comun_tools_stair;

use comun_tools_stair::*;
use lakehouse_rag::application::dtos::ingestion::{
    CeldaDestilada, EjeDestilado, LimitesDestilados, TablaDestilada,
};
use serde_json::{Value, json};

const ARCHIVO: &str = "tabla_paginada.pdf";
const RELLENO: &str = "Texto suficiente para que la pagina cuente como extraible por el extractor";
const LOOKUP: &str = "lookup_double_entry_table";
const SQL: &str = "execute_athena_sql";

/// (grado, delgado, grueso, página)
const FILAS: [(&str, &str, &str, u32); 5] = [
    ("S275", "75", "125", 2),
    ("S355", "100", "150", 2),
    ("S460", "125", "175", 2),
    ("Al6061", "30", "45", 3),
    ("Cu110", "40", "60", 3),
];

fn categoria(texto: &str) -> LimitesDestilados {
    LimitesDestilados {
        categoria: Some(texto.to_string()),
        min: None,
        min_inclusivo: false,
        max: None,
        max_inclusivo: false,
    }
}

fn eje(nombre: &str) -> EjeDestilado {
    EjeDestilado {
        nombre: nombre.to_string(),
        tipo: "CATEGORIA_EXACTA".to_string(),
        unidad: None,
    }
}

fn tabla_destilada() -> TablaDestilada {
    let celdas = FILAS
        .iter()
        .flat_map(|(grado, delgado, grueso, _)| {
            [("Thin", *delgado), ("Thick", *grueso)].map(|(espesor, valor)| CeldaDestilada {
                eje_x: categoria(espesor),
                eje_y: categoria(grado),
                eje_z: None,
                valor_resultado: valor.to_string(),
                unidad_medida: Some("C".to_string()),
                nota_pie_condicion: None,
            })
        })
        .collect();
    TablaDestilada {
        tabla_id: "9-C".to_string(),
        titulo: "Preheat temperature".to_string(),
        eje_x: eje("espesor"),
        eje_y: eje("grado"),
        valor_salida_nombre: "temperatura_c".to_string(),
        seccion_origen: None,
        celdas,
    }
}

async fn con_tabla_paginada() -> EntornoStair {
    let e = entorno_stair().await;
    let mut paquete = paquete("PR-TABLA-01", 1, "2026-01-01");
    paquete.tablas.push(tabla_destilada());
    e.registrar_destilacion(ARCHIVO, paquete);
    let bytes = comun_pdf::pdf(&[
        &[
            "1 Alcance",
            RELLENO,
            "Grade S275 needs 75 degrees as shown in the annex",
        ],
        &[
            "2 Precalentamiento",
            RELLENO,
            "Grade Thin Thick",
            "S275 75 125",
            "S355 100 150",
            "S460 125 175",
        ],
        &[RELLENO, "Grade Thin Thick", "Al6061 30 45", "Cu110 40 60"],
    ]);
    let ruta = e.escribir(ARCHIVO, &bytes);
    let informe = e.app.ingestar(&ruta, CENTRO).await.unwrap().informe;
    assert_eq!(informe.doc_id, "PR-TABLA-01", "{informe:?}");
    e
}

async fn cruzar(e: &EntornoStair, grado: &str) -> Value {
    ok(
        e,
        LOOKUP,
        args(json!({ "tabla_id": "Table 9-C", "valor_x": "Thin", "valor_y": grado })),
    )
    .await
}

#[tokio::test]
async fn test_lookup_cita_la_pagina_de_la_fila_en_una_tabla_partida() {
    let e = con_tabla_paginada().await;
    for (grado, delgado, _, pagina) in FILAS {
        let salida = cruzar(&e, grado).await;
        assert_eq!(salida["estado"], "resuelto", "{salida}");
        assert_eq!(salida["valor_resultado_texto"], delgado, "{salida}");
        assert_eq!(
            salida["cita"]["paginas"],
            pagina.to_string(),
            "{grado}: {salida}"
        );
    }
}

#[tokio::test]
async fn test_catalogo_04_guarda_el_bloque_y_05_la_pagina_de_cada_celda() {
    let e = con_tabla_paginada().await;
    let sql = format!(
        "SELECT pagina_inicio, pagina_fin, titulo_tabla FROM \"{CENTRO}_04_catalogo_tablas\" WHERE doc_id = 'PR-TABLA-01'"
    );
    let tabla = ok(&e, SQL, args(json!({ "sql_query": sql }))).await["resultado"][0].clone();
    assert_eq!(tabla["pagina_inicio"], 2, "{tabla}");
    assert_eq!(tabla["pagina_fin"], 3, "{tabla}");
    assert_eq!(tabla["titulo_tabla"], "9-C — Preheat temperature");

    let sql = format!(
        "SELECT eje_y_categoria, pagina FROM \"{CENTRO}_05_celdas_doble_entrada\" WHERE doc_id = 'PR-TABLA-01'"
    );
    let celdas = ok(&e, SQL, args(json!({ "sql_query": sql }))).await["resultado"].clone();
    let celdas = celdas.as_array().unwrap();
    assert_eq!(celdas.len(), 10);
    for celda in celdas {
        let grado = celda["eje_y_categoria"].as_str().unwrap();
        let (_, _, _, pagina) = FILAS.iter().find(|f| f.0 == grado).unwrap();
        assert_eq!(celda["pagina"], *pagina, "{celda}");
    }
}

#[tokio::test]
async fn test_tabla_de_markdown_no_trae_paginas_en_la_cita() {
    let e = entorno_stair().await;
    let mut paquete = paquete("PR-TABLA-MD", 1, "2026-01-01");
    paquete.tablas.push(tabla_destilada());
    e.registrar_destilacion("tabla.md", paquete);
    let markdown = "# Tabla\n\n## 1 Datos\n\n| Grade | Thin | Thick |\n| --- | --- | --- |\n| S275 | 75 | 125 |\n| S355 | 100 | 150 |\n";
    let ruta = e.escribir("tabla.md", markdown.as_bytes());
    e.app.ingestar(&ruta, CENTRO).await.unwrap();
    let salida = cruzar(&e, "S355").await;
    assert_eq!(salida["estado"], "resuelto", "{salida}");
    assert!(salida["cita"].get("paginas").is_none(), "{salida}");
}
