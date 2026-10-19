//! `claude` falso para los tests: un ejecutable compilado al vuelo con `rustc` en un directorio
//! temporal a partir de una fuente Rust. Reproduce salidas REALES del CLI sin tocar la sesión
//! del usuario.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

pub struct ClaudeFalso {
    _directorio: tempfile::TempDir,
    pub binario: PathBuf,
}

impl ClaudeFalso {
    /// Compila `fuente` (un `fn main`) y deja `claude[.exe]` en un directorio temporal, junto a
    /// los `archivos` (nombre, contenido) que la fuente pueda leer desde su propio directorio
    pub fn compilar(fuente: &str, archivos: &[(&str, &str)]) -> Self {
        let directorio = tempfile::tempdir().unwrap();
        let ruta_fuente = directorio.path().join("claude_falso.rs");
        std::fs::write(&ruta_fuente, fuente).unwrap();
        for (nombre, contenido) in archivos {
            std::fs::write(directorio.path().join(nombre), contenido).unwrap();
        }
        let binario = directorio
            .path()
            .join(format!("claude{}", std::env::consts::EXE_SUFFIX));
        let estado = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
            .arg("--edition=2021")
            .arg("-o")
            .arg(&binario)
            .arg(&ruta_fuente)
            .status()
            .expect("rustc disponible para compilar el claude falso");
        assert!(estado.success(), "no compiló el claude falso");
        Self {
            _directorio: directorio,
            binario,
        }
    }
}
