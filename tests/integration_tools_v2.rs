//! Integración de las 6 tools v2 (Fase 13) sobre un centro real en disco: LanceDB →
//! proyección → tools. Oráculo en `tests/fixtures/README.md` (sección «Tools v2»).

mod comun_centros;
mod comun_store_v2;
mod comun_tools_v2;

use comun_tools_v2::*;
use serde_json::{Value, json};

fn textos(valor: &Value, clave: &str) -> Vec<String> {
    valor
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f[clave].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn test_lookup_fronteras_nota_obligatoria_y_time_travel() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    let cruce = |x: Value, y: Value, fecha: Option<&str>| {
        let mut a = args(json!({ "tabla_id": "4-A", "valor_x": x, "valor_y": y }));
        if let Some(f) = fecha {
            a["fecha_referencia"] = json!(f);
        }
        a
    };

    let vigente = ok(
        h,
        "lookup_double_entry_table",
        cruce(json!(30), json!("0,42"), None),
    )
    .await;
    assert_eq!(vigente["estado"], "resuelto", "{vigente}");
    assert_eq!(vigente["valor_resultado_texto"], "125 (*)");
    assert_eq!(vigente["valor_resultado_num"], 125.0);
    assert_eq!(vigente["nota_pie_condicion"], NOTA_ASTERISCO);
    assert_eq!(vigente["nota_obligatoria"], true);
    assert_eq!(vigente["cita"]["doc_id"], "PO-ALM-03");
    assert_eq!(vigente["cita"]["revision"], "Rev. 04");
    assert_eq!(vigente["nivel_prelacion"], 2);

    let historico = ok(
        h,
        "lookup_double_entry_table",
        cruce(json!(30), json!(0.42), Some(FECHA_ANTERIOR_REV04)),
    )
    .await;
    assert_eq!(historico["valor_resultado_texto"], "100 (*)", "{historico}");
    assert_eq!(historico["cita"]["revision"], "Rev. 03");
    assert_eq!(historico["nota_pie_condicion"], NOTA_ASTERISCO);

    let frontera = ok(
        h,
        "lookup_double_entry_table",
        cruce(json!(20), json!(0.42), None),
    )
    .await;
    assert_eq!(
        frontera["valor_resultado_texto"], "50",
        "e = 20 cae en e <= 20"
    );
    assert_eq!(frontera["rango_x"], "x <= 20");
    let justo_encima = ok(
        h,
        "lookup_double_entry_table",
        cruce(json!(20.0001), json!(0.42), None),
    )
    .await;
    assert_eq!(justo_encima["valor_resultado_texto"], "125 (*)");
    assert_eq!(justo_encima["rango_x"], "20 < x <= 40");

    let no_admisible = ok(
        h,
        "lookup_double_entry_table",
        cruce(json!(50), json!(0.5), None),
    )
    .await;
    assert_eq!(no_admisible["valor_resultado_texto"], "No admisible (*)");
    assert_eq!(no_admisible["nota_pie_condicion"], NOTA_ASTERISCO);

    let por_busqueda = ok(
        h,
        "lookup_double_entry_table",
        args(json!({ "busqueda_tabla": "precalentamiento", "valor_x": 30, "valor_y": 0.42 })),
    )
    .await;
    assert_eq!(por_busqueda["valor_resultado_texto"], "125 (*)");

    let inexistente = ok(
        h,
        "lookup_double_entry_table",
        args(json!({ "tabla_id": "9-Z", "valor_x": 1, "valor_y": 1 })),
    )
    .await;
    assert_eq!(inexistente["estado"], "tabla_no_encontrada");
    assert_eq!(textos(&inexistente["tablas"], "doc_id"), vec!["PO-ALM-03"]);
}

#[tokio::test]
async fn test_matriz_filas_atomicas_filtros_exactos_y_time_travel() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;

    let custodia = ok(
        h,
        "query_operational_matrix",
        args(json!({ "fase": "Custodia" })),
    )
    .await;
    assert_eq!(custodia["total"], 1, "{custodia}");
    let fila = &custodia["filas"][0];
    assert_eq!(fila["rol_responsable"], "Jefe de Producción");
    assert_eq!(fila["rol_ejecutor"], "Auxiliar de Almacén");
    assert_eq!(fila["registro_codigo"], "FOR-ALM-05");
    assert_eq!(fila["cita"]["revision"], "Rev. 04");
    assert_eq!(fila["nivel_prelacion"], 2);
    assert!(
        fila["condicion_aplicacion"]
            .as_str()
            .unwrap()
            .contains("7 días")
    );

    let recepcion = ok(
        h,
        "query_operational_matrix",
        args(json!({ "fase": "Recepción" })),
    )
    .await;
    assert_eq!(recepcion["total"], 1);
    assert_eq!(recepcion["filas"][0]["rol_responsable"], "Jefe de Almacén");

    let antes = ok(
        h,
        "query_operational_matrix",
        args(json!({ "fase": "CUSTODIA", "fecha_referencia": FECHA_ANTERIOR_REV04 })),
    )
    .await;
    assert_eq!(antes["filas"][0]["registro_codigo"], "FOR-ALM-02");
    assert_eq!(antes["filas"][0]["cita"]["revision"], "Rev. 03");

    let firma = ok(
        h,
        "query_operational_matrix",
        args(json!({ "rol": "jefe de produccion", "rol_en": "responsable" })),
    )
    .await;
    assert_eq!(
        textos(&firma["filas"], "registro_codigo"),
        vec!["FOR-ALM-05"]
    );
    let como_ejecutor = ok(
        h,
        "query_operational_matrix",
        args(json!({ "rol": "Jefe de Producción", "rol_en": "ejecutor" })),
    )
    .await;
    assert_eq!(como_ejecutor["total"], 0);

    let por_registro = ok(
        h,
        "query_operational_matrix",
        args(json!({ "registro": "for-alm-01" })),
    )
    .await;
    assert_eq!(
        textos(&por_registro["filas"], "fase_operativa"),
        vec!["RECEPCION"]
    );
}

#[tokio::test]
async fn test_sql_exhaustiva_limite_y_time_travel() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;

    let registros = ok(
        h,
        "execute_athena_sql",
        args(json!({ "sql_query": "SELECT DISTINCT registro_codigo FROM 03_matriz_operativa WHERE vigente = true AND rol_responsable = 'Jefe de Producción' ORDER BY 1" })),
    )
    .await;
    assert_eq!(
        textos(&registros["resultado"], "registro_codigo"),
        vec!["FOR-ALM-05"]
    );
    assert_eq!(registros["truncado"], false);

    let por_fase = ok(
        h,
        "execute_athena_sql",
        args(json!({ "sql_query": "SELECT fase_operativa, COUNT(*) AS n FROM 03_matriz_operativa WHERE vigente = true GROUP BY fase_operativa ORDER BY 1" })),
    )
    .await;
    assert_eq!(
        textos(&por_fase["resultado"], "fase_operativa"),
        vec!["CUSTODIA", "INSPECCION", "RECEPCION"]
    );

    let historico = ok(
        h,
        "execute_athena_sql",
        args(json!({ "sql_query": "SELECT registro_codigo FROM 03_matriz_operativa WHERE fase_operativa = 'CUSTODIA'", "fecha_referencia": FECHA_ANTERIOR_REV04 })),
    )
    .await;
    assert_eq!(
        textos(&historico["resultado"], "registro_codigo"),
        vec!["FOR-ALM-02"]
    );

    let cartesiano = ok(
        h,
        "execute_athena_sql",
        args(json!({ "sql_query": "SELECT a.chunk_id FROM 02_chunks_normativa a, 02_chunks_normativa b, 02_chunks_normativa c" })),
    )
    .await;
    assert_eq!(cartesiano["truncado"], true, "{}", cartesiano["filas"]);
    assert_eq!(cartesiano["filas"], 1000);
    assert_eq!(cartesiano["resultado"].as_array().unwrap().len(), 1000);
}

#[tokio::test]
async fn test_grafo_vigente_acotado_con_hub_y_versiones() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;

    let grafo = ok(
        h,
        "traverse_normative_graph",
        args(json!({ "nodo_origen": "PO-ALM-03" })),
    )
    .await;
    let nodos = grafo["nodos"].as_array().unwrap();
    assert!(nodos.len() <= 50, "{}", nodos.len());
    let hub = nodos
        .iter()
        .find(|n| n["id"] == "NORMA-HUB")
        .expect("hub presente");
    assert_eq!(hub["hub_resumido"], true);
    assert_eq!(hub["grado"], ARISTAS_HUB + 1);
    assert_eq!(
        hub["resumen_hub"]["top_relaciones"][0]["tipo_relacion"],
        "REFERENCIA_A"
    );
    assert!(
        nodos
            .iter()
            .all(|n| !n["id"].as_str().unwrap().starts_with("REG-")),
        "el hub no se expande"
    );
    assert_eq!(
        nodos[0]["id"], "PCC-01",
        "ordenado por prelación (nivel 1 primero)"
    );
    let aristas = grafo["aristas"].as_array().unwrap();
    assert!(
        aristas.iter().all(|a| a["cita"]["revision"] == "Rev. 04"),
        "0 aristas de la Rev. 03 vigentes"
    );
    assert!(aristas.iter().any(|a| a["destino"] == "FOR-ALM-05"));
    assert!(aristas.iter().any(|a| a["tipo_relacion"] == "DEROGA_A"));

    let antes = ok(
        h,
        "traverse_normative_graph",
        args(json!({ "nodo_origen": "PO-ALM-03", "max_saltos": 1, "fecha_referencia": FECHA_ANTERIOR_REV04 })),
    )
    .await;
    let destinos: Vec<String> = antes["aristas"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["destino"].as_str().unwrap().to_string())
        .collect();
    assert!(
        destinos.contains(&"FOR-ALM-02".to_string())
            && !destinos.contains(&"FOR-ALM-05".to_string())
    );

    let solo_registros = ok(
        h,
        "traverse_normative_graph",
        args(json!({ "nodo_origen": "PO-ALM-03", "tipo_relacion": ["EXIGE_LLENAR_REGISTRO"] })),
    )
    .await;
    let ids: Vec<String> = textos(&solo_registros["nodos"], "id");
    assert!(ids.contains(&"FOR-ALM-05".to_string()), "{ids:?}");
    assert!(
        !ids.iter()
            .any(|i| i == "NORMA-HUB" || i == "PCC-01" || i == "FOR-ALM-02"),
        "{ids:?}"
    );

    let desde_hub = ok(
        h,
        "traverse_normative_graph",
        args(json!({ "nodo_origen": "NORMA-HUB" })),
    )
    .await;
    assert!(desde_hub["nodos"].as_array().unwrap().len() <= 50);
}

#[tokio::test]
async fn test_busqueda_prelacion_condicion_heredada_y_catalogo() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;

    let ensayos = ok(
        h,
        "search_normative_context",
        args(json!({ "consulta": "porcentaje de soldaduras a tope que se deben ensayar" })),
    )
    .await;
    let grupo = ensayos["grupos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["tema"] == "ENSAYOS NO DESTRUCTIVOS")
        .unwrap_or_else(|| panic!("{ensayos}"))
        .clone();
    assert_eq!(grupo["conflicto_prelacion"], true);
    let resultados = grupo["resultados"].as_array().unwrap();
    let pliego = resultados
        .iter()
        .find(|r| r["cita"]["doc_id"] == "PCC-01")
        .unwrap();
    let norma = resultados
        .iter()
        .find(|r| r["cita"]["doc_id"] == "UNE-EN-1090-2")
        .unwrap();
    assert_eq!(pliego["prevalece"], true);
    assert!(pliego["texto"].as_str().unwrap().contains("100 %"));
    assert_eq!(norma["supletorio"], true);
    assert!(norma["texto"].as_str().unwrap().contains("10 %"));

    let calentar = ok(
        h,
        "search_normative_context",
        args(json!({ "consulta": "calentar la zona de soldadura a 80 °C", "top_k": 3 })),
    )
    .await;
    let primero = &calentar["grupos"][0]["resultados"][0];
    assert!(
        primero["texto"].as_str().unwrap().contains("80 °C"),
        "{calentar}"
    );
    assert!(
        primero["condicion_aplicacion"]
            .as_str()
            .unwrap()
            .contains("< 0 °C")
    );
    assert!(
        primero["breadcrumb_jerarquia"]
            .as_str()
            .unwrap()
            .contains("Soldadura bajo cero")
    );
    assert_eq!(primero["cita"]["revision"], "Rev. 04");

    let solo_cliente = ok(
        h,
        "search_normative_context",
        args(json!({ "consulta": "ensayar soldaduras", "nivel_prelacion_max": 1 })),
    )
    .await;
    for grupo in solo_cliente["grupos"].as_array().unwrap() {
        for r in grupo["resultados"].as_array().unwrap() {
            assert_eq!(r["cita"]["doc_id"], "PCC-01");
        }
    }

    let catalogo = ok(h, "inspect_lakehouse_catalog", args(json!({}))).await;
    let docs = textos(&catalogo["documentos"], "doc_id");
    assert_eq!(
        docs.first().map(String::as_str),
        Some("PCC-01"),
        "prelación 1 primero"
    );
    assert_eq!(
        docs.iter().filter(|d| *d == "PO-ALM-03").count(),
        1,
        "solo la revisión vigente"
    );
    let tabla = &catalogo["tablas_doble_entrada"][0];
    assert!(tabla["titulo_tabla"].as_str().unwrap().contains("4-A"));
    assert_eq!(tabla["eje_x_unidad"], "mm");
    assert!(tabla["instrucciones_uso_sql"].is_string());
    let antes = ok(
        h,
        "inspect_lakehouse_catalog",
        args(json!({ "fecha_referencia": FECHA_ANTERIOR_REV04 })),
    )
    .await;
    let revision = antes["documentos"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["doc_id"] == "PO-ALM-03")
        .unwrap()["revision"]
        .clone();
    assert_eq!(revision, "Rev. 03");
}

#[tokio::test]
async fn test_aislamiento_de_centro_en_las_6_tools() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    let vacio = |extra: Value| {
        let mut a = extra;
        a["centro_id"] = json!(CENTRO_VACIO);
        a
    };
    let catalogo = ok(h, "inspect_lakehouse_catalog", vacio(json!({}))).await;
    assert_eq!(catalogo["documentos"], json!([]));
    let matriz = ok(
        h,
        "query_operational_matrix",
        vacio(json!({ "fase": "CUSTODIA" })),
    )
    .await;
    assert_eq!(matriz["total"], 0);
    let lookup = ok(
        h,
        "lookup_double_entry_table",
        vacio(json!({ "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42 })),
    )
    .await;
    assert_eq!(lookup["estado"], "tabla_no_encontrada");
    let grafo = ok(
        h,
        "traverse_normative_graph",
        vacio(json!({ "nodo_origen": "PO-ALM-03" })),
    )
    .await;
    assert_eq!(grafo["estado"], "nodo_no_encontrado");
    let busqueda = ok(
        h,
        "search_normative_context",
        vacio(json!({ "consulta": "soldadura" })),
    )
    .await;
    assert_eq!(busqueda["total"], 0);
    let ajena = invocar(
        h,
        "execute_athena_sql",
        vacio(json!({ "sql_query": format!("SELECT * FROM \"{CENTRO}_03_matriz_operativa\"") })),
    )
    .await;
    assert!(ajena.is_error, "{}", ajena.output);
    assert!(
        ajena.output["error"]
            .as_str()
            .unwrap()
            .contains("Acceso denegado")
    );
}
