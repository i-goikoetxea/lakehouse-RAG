//! Puerta E2E de la Fase 12 (plan v2 §3.1): ejecuta el **binario real**
//! (`target/debug/lakehouse_rag`) con un directorio de configuración temporal, Claude Code
//! headless y FastEmbed reales. Ignorados por defecto:
//!
//! ```text
//! cargo build && cargo test --test e2e_binario_fase12 -- --ignored --nocapture --test-threads=1
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::Value;

const BINARIO: &str = env!("CARGO_BIN_EXE_lakehouse_rag");

struct Sesion {
    _raiz: tempfile::TempDir,
    config: PathBuf,
    datos: PathBuf,
}

impl Sesion {
    fn nueva() -> Self {
        let raiz = tempfile::tempdir().unwrap();
        let config = raiz.path().join("config");
        let datos = raiz.path().join("centros");
        Self {
            _raiz: raiz,
            config,
            datos,
        }
    }

    /// Ejecuta el binario con `--json` y devuelve (código, JSON de stdout)
    fn ejecutar(&self, argumentos: &[&str]) -> (i32, Value) {
        let inicio = Instant::now();
        let salida = Command::new(BINARIO)
            .args(argumentos)
            .arg("--json")
            .env("LAKEHOUSE_RAG_CONFIG_DIR", &self.config)
            .env("FASTEMBED_CACHE_DIR", cache_fastembed())
            .output()
            .expect("no se pudo lanzar el binario");
        let stdout = String::from_utf8_lossy(&salida.stdout).to_string();
        let stderr = String::from_utf8_lossy(&salida.stderr).to_string();
        let codigo = salida.status.code().unwrap_or(-1);
        println!(
            "\n$ lakehouse_rag {} --json   [{:.1} s, código {codigo}]",
            argumentos.join(" "),
            inicio.elapsed().as_secs_f64()
        );
        if !stderr.trim().is_empty() {
            println!("  stderr: {}", recortar(stderr.trim(), 600));
        }
        let json: Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|e| panic!("stdout no es JSON ({e}): {stdout}"));
        (codigo, json)
    }

    fn ruta_centro(&self, nombre: &str) -> String {
        self.datos.join(nombre).to_string_lossy().to_string()
    }
}

fn cache_fastembed() -> PathBuf {
    std::env::var_os("FASTEMBED_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("fastembed_cache")
        })
}

fn fixture(nombre: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(nombre)
        .to_string_lossy()
        .to_string()
}

fn recortar(texto: &str, maximo: usize) -> String {
    if texto.chars().count() <= maximo {
        texto.to_string()
    } else {
        format!("{}…", texto.chars().take(maximo).collect::<String>())
    }
}

fn imprimir_respuesta(respuesta: &Value) {
    println!(
        "  respuesta: {}",
        recortar(respuesta["respuesta"].as_str().unwrap_or(""), 900)
    );
    for herramienta in respuesta["herramientas"].as_array().into_iter().flatten() {
        println!(
            "  tool {} [{}] args={} → {}",
            herramienta["nombre"],
            if herramienta["es_error"] == true {
                "error"
            } else {
                "ok"
            },
            herramienta["argumentos"],
            recortar(&herramienta["resultado"].to_string(), 300)
        );
    }
}

fn herramientas_ok(respuesta: &Value) -> Vec<&Value> {
    respuesta["herramientas"]
        .as_array()
        .map(|h| h.iter().filter(|t| t["es_error"] == false).collect())
        .unwrap_or_default()
}

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large + binario"]
fn e2e_binario_fase12_ingesta_integridad_y_preguntas() {
    let sesion = Sesion::nueva();
    let centro = "obra-planta-solar-01";

    let (codigo, creado) = sesion.ejecutar(&[
        "--crear-centro",
        centro,
        &sesion.ruta_centro("obra_solar"),
        "Planta Solar",
    ]);
    assert_eq!(codigo, 0, "{creado}");
    assert_eq!(creado["centro"]["activo"], false);

    let (codigo, activado) = sesion.ejecutar(&["--activar", centro]);
    assert_eq!(codigo, 0, "{activado}");
    assert_eq!(
        activado["proyeccion"]["vistas"].as_array().unwrap().len(),
        6
    );

    for (archivo, version) in [
        ("procedimiento_recepcion_rev03.md", 3),
        ("procedimiento_recepcion_rev04.md", 4),
    ] {
        let (codigo, ingesta) =
            sesion.ejecutar(&["--ingest", &fixture(archivo), "--centro", centro]);
        assert_eq!(codigo, 0, "{ingesta}");
        println!(
            "  {} v{} · totales {} · derogadas {} · vistas {}",
            ingesta["doc_id"],
            ingesta["version"],
            ingesta["totales"],
            ingesta["versiones_derogadas"],
            ingesta["proyeccion"]["vistas"]
        );
        assert_eq!(ingesta["version"], version);
        assert_eq!(ingesta["ya_ingerido"], false);
        assert!(
            ingesta["copia_original"]
                .as_str()
                .unwrap()
                .contains("raw_docs")
        );
    }

    let (codigo, integridad) = sesion.ejecutar(&["--integridad", centro]);
    println!("  integridad: {integridad}");
    assert_eq!(codigo, 0, "{integridad}");
    assert_eq!(integridad["integro"], true);

    let (codigo, custodia) = sesion.ejecutar(&[
        "--ask",
        "¿Quién es el responsable de la custodia y qué registro llena?",
        "--centro",
        centro,
    ]);
    imprimir_respuesta(&custodia);
    assert_eq!(codigo, 0, "{custodia}");
    let texto = custodia["respuesta"].as_str().unwrap();
    assert!(
        !herramientas_ok(&custodia).is_empty(),
        "debe invocar al menos una tool sin error"
    );
    assert!(texto.contains("Jefe de Producción"), "{texto}");
    assert!(texto.contains("FOR-ALM-05"), "{texto}");

    let (codigo, agregado) = sesion.ejecutar(&[
        "--ask",
        "Usa execute_athena_sql con una SELECT de agregación: cuenta los hitos vigentes de \
         03_matriz_operativa agrupados por fase_operativa y dame el total por fase.",
        "--centro",
        centro,
    ]);
    imprimir_respuesta(&agregado);
    assert_eq!(codigo, 0, "{agregado}");
    let con_datos = herramientas_ok(&agregado).into_iter().any(|t| {
        t["nombre"] == "execute_athena_sql"
            && t["resultado"]
                .as_array()
                .is_some_and(|filas| !filas.is_empty())
    });
    assert!(
        con_datos,
        "la agregación SQL debe devolver filas reales: {agregado}"
    );
}

/// Especificación §9.1: dos centros, solo uno activo; la consulta no ve el otro
#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large + binario"]
fn e2e_binario_fase12_multi_centro() {
    let sesion = Sesion::nueva();
    let (codigo, _) = sesion.ejecutar(&[
        "--crear-centro",
        "obra-a",
        &sesion.ruta_centro("obra_a"),
        "Centro Obra A",
    ]);
    assert_eq!(codigo, 0);
    let (codigo, _) = sesion.ejecutar(&[
        "--crear-centro",
        "obra-b",
        &sesion.ruta_centro("obra_b"),
        "Centro Obra B",
    ]);
    assert_eq!(codigo, 0);
    let (codigo, ingesta) = sesion.ejecutar(&[
        "--ingest",
        &fixture("procedimiento_recepcion_rev03.md"),
        "--centro",
        "obra-a",
    ]);
    assert_eq!(codigo, 0, "{ingesta}");
    let (codigo, _) = sesion.ejecutar(&["--activar", "obra-b"]);
    assert_eq!(codigo, 0);

    let (_, centros) = sesion.ejecutar(&["--centros"]);
    println!("  centros: {centros}");

    // A tiene datos pero está inactivo: preguntarle es un error explícito
    let (codigo, inactivo) =
        sesion.ejecutar(&["--ask", "¿Qué documentos hay?", "--centro", "obra-a"]);
    println!("  obra-a (inactivo): {inactivo}");
    assert_eq!(codigo, 1);
    assert!(inactivo["error"].as_str().unwrap().contains("inactivo"));

    // B (activo y vacío) no ve los documentos de A
    let (codigo, b) = sesion.ejecutar(&[
        "--ask",
        "Usa execute_athena_sql para contar las filas de 01_documentos_catalogo de este centro \
         (SELECT COUNT(*) AS n FROM 01_documentos_catalogo) y dime el número.",
        "--centro",
        "obra-b",
    ]);
    imprimir_respuesta(&b);
    assert_eq!(codigo, 0, "{b}");
    let conteos: Vec<i64> = herramientas_ok(&b)
        .into_iter()
        .filter(|t| t["nombre"] == "execute_athena_sql")
        .filter_map(|t| {
            let fila = t["resultado"].as_array()?.first()?.as_object()?.clone();
            fila.values().next()?.as_i64()
        })
        .collect();
    assert!(!conteos.is_empty(), "debe ejecutar la SELECT: {b}");
    assert!(
        conteos.iter().all(|&n| n == 0),
        "B no debe ver datos de A: {conteos:?}"
    );
    assert!(
        !b["herramientas"].to_string().contains("PO-ALM-03"),
        "ningún resultado de B contiene el documento de A"
    );
}
