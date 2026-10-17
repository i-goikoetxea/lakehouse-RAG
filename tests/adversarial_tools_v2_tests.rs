//! Suite adversarial de las tools v2 (Fase 13): inyección, escape de centro, SQL mutante o de
//! ficheros, consultas enormes, fechas hostiles, tipos erróneos y valores no finitos.

mod comun_centros;
mod comun_store_v2;
mod comun_tools_v2;

use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow_array::{Int64Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use comun_tools_v2::*;
use lakehouse_rag::application::ports::analytical_engine::{
    AnalyticalEnginePort, PoliticaConsultaLectura,
};
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use serde_json::{Value, json};

fn error_de(resultado: &lakehouse_rag::application::dtos::tools::ToolResult) -> String {
    assert!(
        resultado.is_error,
        "se esperaba error: {}",
        resultado.output
    );
    resultado.output["error"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn test_inyeccion_sql_en_filtros_no_altera_la_consulta() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    for filtros in [
        json!({ "rol": "x' OR '1'='1" }),
        json!({ "registro": "' OR 1=1 --" }),
        json!({ "doc_id": "PO-ALM-03' OR '1'='1" }),
        json!({ "objeto": "%' UNION SELECT * FROM \"obra-vacia_01_documentos_catalogo\" --" }),
        json!({ "fase": "CUSTODIA'; DROP TABLE x; --", "rol": "nadie" }),
    ] {
        let salida = ok(h, "query_operational_matrix", args(filtros.clone())).await;
        assert_eq!(salida["total"], 0, "{filtros}: {salida}");
    }
    let lookup = ok(
        h,
        "lookup_double_entry_table",
        args(json!({ "tabla_id": "4-A' OR '1'='1", "valor_x": 30, "valor_y": 0.42 })),
    )
    .await;
    assert_eq!(lookup["estado"], "tabla_no_encontrada");
    let grafo = ok(
        h,
        "traverse_normative_graph",
        args(json!({ "nodo_origen": "PO-ALM-03'; DROP TABLE x; --" })),
    )
    .await;
    assert_eq!(grafo["estado"], "nodo_no_encontrado");
    let busqueda = ok(
        h,
        "search_normative_context",
        args(json!({ "consulta": "'; DROP TABLE \"02_chunks_normativa\"; --" })),
    )
    .await;
    assert!(busqueda["grupos"].is_array());
    // Los datos siguen intactos
    let matriz = ok(
        h,
        "query_operational_matrix",
        args(json!({ "fase": "CUSTODIA" })),
    )
    .await;
    assert_eq!(matriz["total"], 1);
}

#[tokio::test]
async fn test_sql_no_escapa_del_centro_activo() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    for sql in [
        format!("SELECT * FROM \"{CENTRO_VACIO}_01_documentos_catalogo\""),
        format!(
            "SELECT COUNT(*) FROM 01_documentos_catalogo UNION ALL SELECT COUNT(*) FROM \"{CENTRO_VACIO}_02_chunks_normativa\""
        ),
        format!(
            "SELECT * FROM 03_matriz_operativa WHERE doc_id IN (SELECT doc_id FROM \"{CENTRO_VACIO}_01_documentos_catalogo\")"
        ),
        format!(
            "WITH x AS (SELECT * FROM \"{CENTRO_VACIO}_06_grafo_referencias\") SELECT * FROM x"
        ),
    ] {
        let resultado = invocar(h, "execute_athena_sql", args(json!({ "sql_query": sql }))).await;
        assert!(
            error_de(&resultado).contains("Acceso denegado"),
            "{sql}: {}",
            resultado.output
        );
    }
    let otro = invocar(
        h,
        "execute_athena_sql",
        args(json!({ "sql_query": "SELECT * FROM otro_centro_03_matriz_operativa" })),
    )
    .await;
    assert!(otro.is_error);
}

#[tokio::test]
async fn test_sql_rechaza_ddl_dml_copy_y_varias_sentencias() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    for sql in [
        "DROP TABLE 03_matriz_operativa",
        "DELETE FROM 03_matriz_operativa",
        "INSERT INTO 03_matriz_operativa VALUES (1)",
        "UPDATE 03_matriz_operativa SET vigente = false",
        "CREATE TABLE robada AS SELECT * FROM 03_matriz_operativa",
        "CREATE EXTERNAL TABLE t STORED AS CSV LOCATION 'C:/Windows/win.ini'",
        "COPY (SELECT * FROM 03_matriz_operativa) TO 'C:/fuga.csv'",
        "COPY 03_matriz_operativa TO 'fuga.parquet'",
        "SELECT 1; DROP TABLE 03_matriz_operativa",
        "SELECT 1; SELECT 2",
        "EXPLAIN SELECT * FROM 03_matriz_operativa",
        "SET datafusion.execution.batch_size = 1",
        "SHOW TABLES",
        "",
        "   ;  ",
    ] {
        let resultado = invocar(h, "execute_athena_sql", args(json!({ "sql_query": sql }))).await;
        assert!(
            resultado.is_error,
            "debió rechazarse: {sql} → {}",
            resultado.output
        );
    }
    let con_punto_y_coma_final = ok(
        h,
        "execute_athena_sql",
        args(json!({ "sql_query": "SELECT COUNT(*) AS n FROM 01_documentos_catalogo;" })),
    )
    .await;
    assert_eq!(con_punto_y_coma_final["filas"], 1);
    let matriz = ok(h, "query_operational_matrix", args(json!({}))).await;
    assert_eq!(matriz["total"], 3, "las vistas siguen intactas");
}

#[tokio::test]
async fn test_sql_rechaza_funciones_de_fichero_y_de_tabla() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    for sql in [
        "SELECT * FROM read_csv('C:/Windows/win.ini')",
        "SELECT * FROM read_parquet('datos.parquet')",
        "SELECT * FROM read_json('x.json')",
        "SELECT * FROM 'C:/Windows/win.ini'",
        "SELECT * FROM generate_series(1, 100000000000)",
        "SELECT * FROM range(1, 100000000000)",
    ] {
        let resultado = invocar(h, "execute_athena_sql", args(json!({ "sql_query": sql }))).await;
        assert!(
            resultado.is_error,
            "debió rechazarse: {sql} → {}",
            resultado.output
        );
    }
}

#[tokio::test]
async fn test_sql_enorme_o_cartesiana_queda_acotada() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    let inicio = Instant::now();
    let cartesiano = ok(
        h,
        "execute_athena_sql",
        args(json!({ "sql_query": "SELECT * FROM 02_chunks_normativa a CROSS JOIN 02_chunks_normativa b CROSS JOIN 02_chunks_normativa c CROSS JOIN 02_chunks_normativa d" })),
    )
    .await;
    assert_eq!(cartesiano["truncado"], true);
    assert_eq!(cartesiano["resultado"].as_array().unwrap().len(), 1000);
    assert!(
        inicio.elapsed() < Duration::from_secs(20),
        "{:?}",
        inicio.elapsed()
    );
    let enorme = "SELECT 1 AS x".to_string() + &" UNION ALL SELECT 1".repeat(2000);
    let resultado = invocar(
        h,
        "execute_athena_sql",
        args(json!({ "sql_query": enorme })),
    )
    .await;
    assert!(error_de(&resultado).contains("caracteres"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_timeout_cancela_una_agregacion_cartesiana() {
    let motor = DataFusionEngine::new();
    let esquema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
    let lote = RecordBatch::try_new(
        esquema,
        vec![Arc::new(Int64Array::from_iter_values(0..2_000))],
    )
    .unwrap();
    motor.registrar_tabla("numeros", vec![lote]).await.unwrap();
    let politica = PoliticaConsultaLectura {
        tablas_permitidas: vec!["numeros".to_string()],
        limite_filas: 10,
        timeout: Duration::from_millis(500),
    };
    let inicio = Instant::now();
    let resultado = motor
        .ejecutar_consulta_lectura(
            "SELECT SUM(a.n * b.n + c.n) FROM numeros a, numeros b, numeros c",
            &politica,
        )
        .await;
    let error = resultado
        .expect_err("8.000 millones de filas no caben en 500 ms")
        .to_string();
    assert!(error.contains("tiempo máximo"), "{error}");
    assert!(
        inicio.elapsed() < Duration::from_secs(5),
        "{:?}",
        inicio.elapsed()
    );
}

#[tokio::test]
async fn test_fecha_referencia_invalida_o_extrema() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    for fecha in [
        "2026-02-30",
        "mañana",
        "01/06/2026",
        "2026-06-01' OR '1'='1",
        "+99999-01-01",
        "1899-12-31",
        "2026-06-01T00:00:00",
    ] {
        for (tool, extra) in [
            ("query_operational_matrix", json!({})),
            ("execute_athena_sql", json!({ "sql_query": "SELECT 1" })),
            ("search_normative_context", json!({ "consulta": "x" })),
        ] {
            let mut a = args(extra);
            a["fecha_referencia"] = json!(fecha);
            let resultado = invocar(h, tool, a).await;
            assert!(
                error_de(&resultado).contains("fecha_referencia"),
                "{tool} {fecha}"
            );
        }
    }
    let lejana = ok(
        h,
        "lookup_double_entry_table",
        args(json!({ "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42, "fecha_referencia": "9999-12-31" })),
    )
    .await;
    assert_eq!(
        lejana["valor_resultado_texto"], "125 (*)",
        "en 9999 sigue vigente la Rev. 04"
    );
    let remota = ok(
        h,
        "inspect_lakehouse_catalog",
        args(json!({ "fecha_referencia": "1900-01-01" })),
    )
    .await;
    assert_eq!(
        remota["documentos"],
        json!([]),
        "nada estaba en vigor en 1900"
    );
}

#[tokio::test]
async fn test_argumentos_con_tipos_erroneos_o_campos_desconocidos() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    let casos: Vec<(&str, Value)> = vec![
        ("inspect_lakehouse_catalog", json!({ "centro_id": 123 })),
        ("inspect_lakehouse_catalog", json!({})),
        (
            "inspect_lakehouse_catalog",
            args(json!({ "solo_vigentes": true })),
        ),
        (
            "query_operational_matrix",
            args(json!({ "fase": ["CUSTODIA"] })),
        ),
        (
            "query_operational_matrix",
            args(json!({ "rol_en": "firmante" })),
        ),
        (
            "lookup_double_entry_table",
            args(json!({ "tabla_id": "4-A", "valor_x": [30], "valor_y": 0.42 })),
        ),
        (
            "lookup_double_entry_table",
            args(json!({ "tabla_id": "4-A", "valor_x": { "v": 30 }, "valor_y": 0.42 })),
        ),
        (
            "lookup_double_entry_table",
            args(json!({ "tabla_id": "4-A", "valor_x": true, "valor_y": 0.42 })),
        ),
        (
            "lookup_double_entry_table",
            args(json!({ "tabla_id": "4-A", "valor_x": null, "valor_y": 0.42 })),
        ),
        (
            "lookup_double_entry_table",
            args(json!({ "tabla_id": "4-A", "valor_y": 0.42 })),
        ),
        (
            "lookup_double_entry_table",
            args(json!({ "valor_x": 30, "valor_y": 0.42 })),
        ),
        ("execute_athena_sql", args(json!({ "sql_query": 42 }))),
        (
            "traverse_normative_graph",
            args(json!({ "nodo_origen": "PO-ALM-03", "tipo_relacion": "DEROGA_A" })),
        ),
        (
            "traverse_normative_graph",
            args(json!({ "nodo_origen": "PO-ALM-03", "max_saltos": -1 })),
        ),
        (
            "traverse_normative_graph",
            args(json!({ "nodo_origen": "PO-ALM-03", "max_vecinos": 70000 })),
        ),
        (
            "traverse_normative_graph",
            args(json!({ "nodo_origen": "   " })),
        ),
        ("search_normative_context", args(json!({ "consulta": "" }))),
        (
            "search_normative_context",
            args(json!({ "consulta": "x".repeat(5000) })),
        ),
        (
            "search_normative_context",
            args(json!({ "consulta": "x", "top_k": "muchos" })),
        ),
        ("herramienta_inexistente", args(json!({}))),
    ];
    for (tool, argumentos) in casos {
        let resultado = invocar(h, tool, argumentos.clone()).await;
        assert!(
            resultado.is_error,
            "{tool} {argumentos} → {}",
            resultado.output
        );
    }
    // Los valores extremos admitidos se acotan en vez de desbordar
    let acotado = ok(
        h,
        "traverse_normative_graph",
        args(json!({ "nodo_origen": "PO-ALM-03", "max_saltos": 200, "max_vecinos": 60000 })),
    )
    .await;
    assert_eq!(acotado["limites"]["max_saltos"], 2);
    assert_eq!(acotado["limites"]["max_vecinos"], 50);
    assert!(acotado["nodos"].as_array().unwrap().len() <= 50);
}

#[tokio::test]
async fn test_lookup_rechaza_nan_e_infinitos() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    for hostil in ["NaN", "nan", "inf", "-inf", "Infinity", "1e400", "-1e999"] {
        for (x, y) in [(json!(hostil), json!(0.42)), (json!(30), json!(hostil))] {
            let resultado = invocar(
                h,
                "lookup_double_entry_table",
                args(json!({ "tabla_id": "4-A", "valor_x": x, "valor_y": y })),
            )
            .await;
            assert!(error_de(&resultado).contains("finito"), "{hostil}");
        }
    }
    let extremo = ok(
        h,
        "lookup_double_entry_table",
        args(json!({ "tabla_id": "4-A", "valor_x": f64::MAX, "valor_y": -f64::MAX })),
    )
    .await;
    assert_eq!(extremo["estado"], "resuelto", "{extremo}");
    assert_eq!(extremo["valor_resultado_texto"], "75", "x > 40 y CE < 0,40");
}

#[tokio::test]
async fn test_centro_inexistente_inactivo_u_hostil() {
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    entorno.app.desactivar_centro(CENTRO_VACIO).await.unwrap();
    for centro in ["no-existe", CENTRO_VACIO, "../etc", "obra\"; DROP", ""] {
        let resultado = invocar(
            h,
            "inspect_lakehouse_catalog",
            json!({ "centro_id": centro }),
        )
        .await;
        assert!(resultado.is_error, "{centro}: {}", resultado.output);
    }
}
