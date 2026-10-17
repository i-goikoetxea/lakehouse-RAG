//! Infraestructura compartida por las puertas E2E con el binario real (Fases 12 y 13)
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::Value;

pub const BINARIO: &str = env!("CARGO_BIN_EXE_lakehouse_rag");

pub struct Sesion {
    _raiz: tempfile::TempDir,
    config: PathBuf,
    datos: PathBuf,
}

impl Sesion {
    pub fn nueva() -> Self {
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
    pub fn ejecutar(&self, argumentos: &[&str]) -> (i32, Value) {
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

    pub fn ruta_centro(&self, nombre: &str) -> String {
        self.datos.join(nombre).to_string_lossy().to_string()
    }
}

pub fn cache_fastembed() -> PathBuf {
    std::env::var_os("FASTEMBED_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("fastembed_cache")
        })
}

pub fn fixture(nombre: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(nombre)
        .to_string_lossy()
        .to_string()
}

pub fn recortar(texto: &str, maximo: usize) -> String {
    if texto.chars().count() <= maximo {
        texto.to_string()
    } else {
        format!("{}…", texto.chars().take(maximo).collect::<String>())
    }
}

pub fn imprimir_respuesta(respuesta: &Value) {
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

pub fn herramientas_ok(respuesta: &Value) -> Vec<&Value> {
    respuesta["herramientas"]
        .as_array()
        .map(|h| h.iter().filter(|t| t["es_error"] == false).collect())
        .unwrap_or_default()
}
