//! `cargo tauri dev` y `cargo tauri build` sirven el mismo frontend: la carpeta estática que se
//! incrusta al compilar. Sin servidor de desarrollo ni paso de build propio (no hay `npm`), no
//! hay dos copias que puedan desincronizarse.

use serde_json::Value;

fn configuracion() -> Value {
    serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json válido")
}

#[test]
fn test_dev_y_build_sirven_la_misma_carpeta_estatica() {
    let build = &configuracion()["build"];
    assert_eq!(build["frontendDist"], "src/presentation/gui/web");
    for clave in ["devUrl", "beforeDevCommand", "beforeBuildCommand"] {
        assert!(
            build.get(clave).is_none(),
            "{clave} separaría el frontend de dev del de producción"
        );
    }
    let raiz = env!("CARGO_MANIFEST_DIR");
    assert!(
        !std::path::Path::new(raiz).join("package.json").exists(),
        "un paso de npm exigiría sincronizar su salida con el binario"
    );
    for fichero in ["index.html", "app.js", "styles.css"] {
        assert!(
            std::path::Path::new(raiz)
                .join("src/presentation/gui/web")
                .join(fichero)
                .is_file(),
            "{fichero}"
        );
    }
}

#[test]
fn test_la_version_de_la_ventana_es_la_del_paquete() {
    assert_eq!(
        configuracion()["version"],
        env!("CARGO_PKG_VERSION"),
        "tauri.conf.json y Cargo.toml declaran la misma versión"
    );
}
