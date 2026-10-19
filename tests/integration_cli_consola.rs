//! Fase 15: binario de consola `lakehouse_rag_cli` (subsistema consola). Sin Claude ni FastEmbed:
//! solo los subcomandos que no los cargan, con el registro aislado en un directorio temporal.

use std::process::{Command, Output};

const BINARIO_CLI: &str = env!("CARGO_BIN_EXE_lakehouse_rag_cli");

fn ejecutar(config: &std::path::Path, argumentos: &[&str]) -> Output {
    Command::new(BINARIO_CLI)
        .args(argumentos)
        .env("LAKEHOUSE_RAG_CONFIG_DIR", config)
        .output()
        .expect("se lanza lakehouse_rag_cli")
}

#[test]
fn test_cli_de_consola_gestiona_centros_con_codigos_de_salida_fiables() {
    let raiz = tempfile::tempdir().unwrap();
    let config = raiz.path().join("config");
    let ruta_centro = raiz.path().join("obra_cli").to_string_lossy().to_string();

    let ayuda = ejecutar(&config, &[]);
    assert_eq!(ayuda.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&ayuda.stdout).contains("lakehouse_rag_cli"));

    let creado = ejecutar(
        &config,
        &[
            "--crear-centro",
            "obra-cli",
            &ruta_centro,
            "Obra CLI",
            "--json",
        ],
    );
    assert_eq!(creado.status.code(), Some(0), "{creado:?}");

    let listado = ejecutar(&config, &["--centros", "--json"]);
    assert_eq!(listado.status.code(), Some(0));
    let centros: serde_json::Value =
        serde_json::from_slice(&listado.stdout).expect("stdout es JSON");
    assert!(centros.to_string().contains("obra-cli"), "{centros}");

    let eliminado = ejecutar(&config, &["--eliminar-centro", "obra-cli", "--json"]);
    assert_eq!(eliminado.status.code(), Some(0), "{eliminado:?}");
    assert!(
        std::path::Path::new(&ruta_centro).exists(),
        "quitar del registro no borra el disco"
    );
}

#[test]
fn test_cli_de_consola_rechaza_la_gui_y_los_argumentos_invalidos_con_codigo_2() {
    let raiz = tempfile::tempdir().unwrap();
    for argumentos in [&["--gui"][..], &["--ingest"][..], &["--activar"][..]] {
        let salida = ejecutar(raiz.path(), argumentos);
        assert_eq!(salida.status.code(), Some(2), "{argumentos:?}: {salida:?}");
        assert!(!String::from_utf8_lossy(&salida.stderr).trim().is_empty());
    }
}
