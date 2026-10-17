//! Puerta E2E de la Fase 13 (tools v2): binario real con Claude Code headless y FastEmbed
//! reales. Ingiere la Rev. 03 y la Rev. 04 de `PO-ALM-03`, el pliego PCC-01 y la norma
//! NT-SOL-01, y comprueba con `--ask --json` qué tools se usan y que las respuestas
//! coinciden con el oráculo de `tests/fixtures/README.md`. Ignorada por defecto:
//!
//! ```text
//! cargo test --test e2e_binario_fase13 -- --ignored --nocapture --test-threads=1
//! ```

mod comun_e2e;

use comun_e2e::*;
use serde_json::Value;

const CENTRO: &str = "obra-planta-solar-13";

fn usadas(respuesta: &Value) -> Vec<String> {
    herramientas_ok(respuesta)
        .iter()
        .map(|t| t["nombre"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn preguntar(sesion: &Sesion, pregunta: &str) -> (String, Value) {
    let (codigo, respuesta) = sesion.ejecutar(&["--ask", pregunta, "--centro", CENTRO]);
    imprimir_respuesta(&respuesta);
    assert_eq!(codigo, 0, "{respuesta}");
    println!("  tools usadas: {:?}", usadas(&respuesta));
    (
        respuesta["respuesta"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        respuesta,
    )
}

/// Alguna invocación sin error de `tool` cuyo resultado cumple `condicion`
fn tool_con(respuesta: &Value, tool: &str, condicion: impl Fn(&Value) -> bool) -> bool {
    herramientas_ok(respuesta)
        .into_iter()
        .any(|t| t["nombre"] == tool && condicion(t))
}

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large + binario"]
fn e2e_binario_fase13_tools_v2() {
    let sesion = Sesion::nueva();
    let (codigo, creado) = sesion.ejecutar(&[
        "--crear-centro",
        CENTRO,
        &sesion.ruta_centro("obra13"),
        "Planta Solar 13",
    ]);
    assert_eq!(codigo, 0, "{creado}");
    let (codigo, activado) = sesion.ejecutar(&["--activar", CENTRO]);
    assert_eq!(codigo, 0, "{activado}");

    for archivo in [
        "procedimiento_recepcion_rev03.md",
        "procedimiento_recepcion_rev04.md",
        "pliego_cliente_pcc01.md",
        "norma_soldeo_nt_sol_01.md",
    ] {
        let (codigo, ingesta) =
            sesion.ejecutar(&["--ingest", &fixture(archivo), "--centro", CENTRO]);
        assert_eq!(codigo, 0, "{ingesta}");
        println!(
            "  {} v{} · totales {} · aviso {}",
            ingesta["doc_id"], ingesta["version"], ingesta["totales"], ingesta["aviso_proyeccion"]
        );
    }

    // (a) Matriz: responsable y registro de la custodia
    let (texto, a) = preguntar(
        &sesion,
        "¿Quién es el responsable de la custodia y qué registro llena?",
    );
    assert!(texto.contains("Jefe de Producción"), "(a) {texto}");
    assert!(texto.contains("FOR-ALM-05"), "(a) {texto}");
    assert!(
        usadas(&a)
            .iter()
            .any(|t| t == "query_operational_matrix" || t == "execute_athena_sql"),
        "(a)"
    );

    // (b) Lookup: tabla 4-A con su nota
    let (texto, b) = preguntar(
        &sesion,
        "Según la Tabla 4-A, ¿cuál es la temperatura mínima de precalentamiento para un espesor de 30 mm y un carbono equivalente de 0,42 %?",
    );
    assert!(texto.contains("125"), "(b) {texto}");
    assert!(
        texto.contains("(*)") || texto.contains("HV10") || texto.contains("QA/QC"),
        "(b) falta la nota: {texto}"
    );
    assert!(
        tool_con(
            &b,
            "lookup_double_entry_table",
            |t| t["resultado"]["estado"] == "resuelto"
        ),
        "(b) debe resolver con lookup_double_entry_table"
    );

    // (c) Time travel: la misma consulta antes de la Rev. 04
    let (texto, c) = preguntar(
        &sesion,
        "¿Y cuál era esa temperatura mínima de la Tabla 4-A (30 mm, CE 0,42 %) el 1 de junio de 2026, antes de que entrara en vigor la Rev. 04?",
    );
    assert!(texto.contains("100"), "(c) {texto}");
    assert!(
        tool_con(
            &c,
            "lookup_double_entry_table",
            |t| t["argumentos"]["fecha_referencia"].is_string()
                && t["resultado"]["valor_resultado_texto"]
                    .as_str()
                    .is_some_and(|v| v.contains("100"))
        ) || tool_con(&c, "execute_athena_sql", |t| {
            t["argumentos"]["fecha_referencia"].is_string()
        }),
        "(c) debe consultar con fecha_referencia"
    );

    // (d) Listado exhaustivo de los registros que firma el Jefe de Producción
    let (texto, d) = preguntar(
        &sesion,
        "Dame el listado exhaustivo de todos los registros que firma (como responsable) el Jefe de Producción en los procedimientos vigentes.",
    );
    assert!(texto.contains("FOR-ALM-05"), "(d) {texto}");
    assert!(
        !texto.contains("FOR-ALM-02"),
        "(d) cita un registro derogado: {texto}"
    );
    assert!(
        usadas(&d)
            .iter()
            .any(|t| t == "execute_athena_sql" || t == "query_operational_matrix"),
        "(d)"
    );

    // (e) Contexto con condición heredada del encabezado
    let (texto, e) = preguntar(
        &sesion,
        "Según la norma NT-SOL-01, ¿a qué temperatura hay que calentar la zona de soldadura antes de soldar y en qué situación se aplica?",
    );
    assert!(texto.contains("80"), "(e) {texto}");
    assert!(
        texto.contains("< 0") || texto.to_lowercase().contains("bajo cero"),
        "(e) falta la condición: {texto}"
    );
    assert!(
        tool_con(&e, "search_normative_context", |t| t["resultado"]
            .to_string()
            .contains("< 0")),
        "(e) debe usar search_normative_context y recibir la condición"
    );

    // (f) Prelación: pliego del cliente (nivel 1) frente a norma (nivel 5)
    let (texto, f) = preguntar(
        &sesion,
        "¿Qué porcentaje de las uniones soldadas a tope hay que ensayar con radiografía en esta obra?",
    );
    assert!(texto.contains("100"), "(f) {texto}");
    assert!(
        texto.contains("PCC-01"),
        "(f) debe citar el pliego: {texto}"
    );
    assert!(
        texto.contains("NT-SOL-01") || texto.contains("10 %"),
        "(f) debe mencionar el supletorio: {texto}"
    );
    println!("  tools (f): {:?}", usadas(&f));
}
