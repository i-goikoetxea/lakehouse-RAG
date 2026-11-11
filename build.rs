use std::path::{Path, PathBuf};

/// DLL de pdfium que descarga `scripts/obtener_pdfium.ps1` (versión y sha256 fijados)
const DLL_PDFIUM: &str = "vendor/pdfium/pdfium.dll";

fn main() {
    copiar_pdfium_junto_a_los_binarios();
    tauri_build::build();
}

/// Copia la DLL a la carpeta de los binarios (`target/<perfil>`), donde el proceso hijo la carga
/// por ruta absoluta. Si falta, la compilación sigue: la ingesta de PDF fallará con un mensaje que
/// pide ejecutar el script (`ssd_generalidad_4_piezas.md` §1.5).
fn copiar_pdfium_junto_a_los_binarios() {
    println!("cargo:rerun-if-changed={DLL_PDFIUM}");
    let origen = Path::new(DLL_PDFIUM);
    let Some(destino) = carpeta_de_binarios() else {
        return;
    };
    if !origen.is_file() {
        println!(
            "cargo:warning=Falta {DLL_PDFIUM}: ejecuta scripts/obtener_pdfium.ps1 para poder ingerir PDF"
        );
        return;
    }
    if let Err(error) = std::fs::copy(origen, destino.join("pdfium.dll")) {
        println!("cargo:warning=No se pudo copiar {DLL_PDFIUM}: {error}");
    }
}

/// `OUT_DIR` es `target/<perfil>/build/<paquete>-<hash>/out`
fn carpeta_de_binarios() -> Option<PathBuf> {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR")?);
    out_dir.ancestors().nth(3).map(Path::to_path_buf)
}
