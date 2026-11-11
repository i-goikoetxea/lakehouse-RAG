//! Lado del padre de la extracción aislada (`ssd_generalidad_4_piezas.md` §1.2).
//!
//! Lanza el proceso hijo (`lakehouse_rag_cli --extraer-texto-pdf`), lo asocia en Windows a un
//! *job object* con límite de memoria **antes** de pasarle el PDF por stdin y espera su respuesta
//! JSON con un límite de tiempo. Si el tiempo se agota, el hijo muere; si muere por su cuenta, el
//! padre devuelve un error sin pánico. Hay un proceso por documento, así que pdfium nunca recibe
//! llamadas concurrentes.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::infrastructure::parsers::pdfium_extractor::{
    ARGUMENTO_EXTRAER_TEXTO_PDF, PaginaExtraida, RespuestaExtraccion,
};

/// Igual que el límite histórico de la extracción en el propio proceso
pub const TIEMPO_MAXIMO_EXTRACCION: Duration = Duration::from_secs(300);
/// Memoria máxima del proceso hijo; al superarla, Windows lo termina
pub const LIMITE_MEMORIA_EXTRACCION: u64 = 2 << 30;
/// Salida máxima que se acepta del hijo (texto de todas las páginas en JSON)
const MAX_BYTES_RESPUESTA: u64 = 512 << 20;
const MAX_BYTES_ERRORES: u64 = 64 << 10;
#[cfg(windows)]
const NOMBRE_EXTRACTOR: &str = "lakehouse_rag_cli.exe";
#[cfg(not(windows))]
const NOMBRE_EXTRACTOR: &str = "lakehouse_rag_cli";

/// Cómo se lanza el proceso hijo
#[derive(Debug, Clone)]
pub struct ProcesoExtractor {
    programa: PathBuf,
    argumentos: Vec<String>,
    tiempo_maximo: Duration,
    limite_memoria: u64,
}

impl Default for ProcesoExtractor {
    fn default() -> Self {
        Self::junto_al_ejecutable()
    }
}

impl ProcesoExtractor {
    /// `lakehouse_rag_cli` junto al ejecutable actual. Los tests y los ejemplos de cargo viven en
    /// `deps/` y `examples/`, un nivel por debajo de los binarios
    pub fn junto_al_ejecutable() -> Self {
        Self::con_comando(ruta_extractor_por_defecto(), [ARGUMENTO_EXTRAER_TEXTO_PDF])
    }

    pub fn con_comando<I, S>(programa: impl Into<PathBuf>, argumentos: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            programa: programa.into(),
            argumentos: argumentos.into_iter().map(Into::into).collect(),
            tiempo_maximo: TIEMPO_MAXIMO_EXTRACCION,
            limite_memoria: LIMITE_MEMORIA_EXTRACCION,
        }
    }

    pub fn con_tiempo_maximo(mut self, tiempo_maximo: Duration) -> Self {
        self.tiempo_maximo = tiempo_maximo;
        self
    }

    pub fn con_limite_memoria(mut self, bytes: u64) -> Self {
        self.limite_memoria = bytes;
        self
    }

    pub fn programa(&self) -> &Path {
        &self.programa
    }

    /// Texto crudo de cada página según el hijo, o el motivo del fallo para el operador
    pub async fn extraer(&self, pdf: &[u8]) -> Result<Vec<PaginaExtraida>, String> {
        if !self.programa.is_file() {
            return Err(format!(
                "falta el extractor de PDF {} (compila el binario lakehouse_rag_cli o reinstala la \
                 app)",
                self.programa.display()
            ));
        }
        match tokio::time::timeout(self.tiempo_maximo, self.ejecutar(pdf)).await {
            Ok(resultado) => resultado,
            Err(_) => {
                Err("la lectura superó el tiempo máximo; el PDF puede estar dañado".to_string())
            }
        }
    }

    async fn ejecutar(&self, pdf: &[u8]) -> Result<Vec<PaginaExtraida>, String> {
        let mut comando = Command::new(&self.programa);
        comando
            .args(&self.argumentos)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        sin_ventana(&mut comando);
        let mut hijo = comando
            .spawn()
            .map_err(|e| format!("no se pudo iniciar el extractor de PDF: {e}"))?;
        // El job vive hasta que termina la función: al cerrarse, mata al hijo si sigue vivo
        let _limite = limitar_memoria(&hijo, self.limite_memoria)?;

        let mut entrada = hijo.stdin.take().ok_or("el extractor no tiene stdin")?;
        let mut salida = hijo.stdout.take().ok_or("el extractor no tiene stdout")?;
        let mut errores = hijo.stderr.take().ok_or("el extractor no tiene stderr")?;
        let bytes = pdf.to_vec();
        let escritura = async move {
            // Si el hijo muere antes de leerlo todo, la escritura falla: lo decide su estado final
            let _ = entrada.write_all(&bytes).await;
            let _ = entrada.shutdown().await;
        };
        let lectura = async {
            let mut respuesta = Vec::new();
            (&mut salida)
                .take(MAX_BYTES_RESPUESTA + 1)
                .read_to_end(&mut respuesta)
                .await
                .map(|_| respuesta)
        };
        let diagnostico = async {
            let mut texto = Vec::new();
            let _ = (&mut errores)
                .take(MAX_BYTES_ERRORES)
                .read_to_end(&mut texto)
                .await;
            texto
        };
        let ((), respuesta, diagnostico) = tokio::join!(escritura, lectura, diagnostico);
        let respuesta = respuesta.map_err(|e| format!("no se pudo leer el extractor: {e}"))?;
        if respuesta.len() as u64 > MAX_BYTES_RESPUESTA {
            return Err("el texto del PDF supera el tamaño máximo admitido".to_string());
        }
        let estado = hijo
            .wait()
            .await
            .map_err(|e| format!("no se pudo esperar al extractor: {e}"))?;
        interpretar(&respuesta, estado.success(), estado.code(), &diagnostico)
    }
}

fn interpretar(
    respuesta: &[u8],
    exito: bool,
    codigo: Option<i32>,
    diagnostico: &[u8],
) -> Result<Vec<PaginaExtraida>, String> {
    match serde_json::from_slice::<RespuestaExtraccion>(respuesta) {
        Ok(RespuestaExtraccion::Paginas(paginas)) if exito => Ok(paginas),
        Ok(RespuestaExtraccion::Error(motivo)) => Err(motivo),
        _ => {
            let detalle = String::from_utf8_lossy(diagnostico);
            let detalle = detalle.trim();
            Err(format!(
                "el extractor de PDF terminó de forma anómala (código {}){}; el PDF puede ser \
                 incompatible o superar el límite de memoria",
                codigo.map_or_else(|| "sin código".to_string(), |c| c.to_string()),
                if detalle.is_empty() {
                    String::new()
                } else {
                    format!(": {}", detalle.chars().take(300).collect::<String>())
                }
            ))
        }
    }
}

fn ruta_extractor_por_defecto() -> PathBuf {
    let Some(carpeta) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    else {
        return PathBuf::from(NOMBRE_EXTRACTOR);
    };
    // En las subcarpetas de cargo puede haber una copia del binario sin la DLL al lado: manda la
    // de la carpeta de los binarios, donde build.rs copia pdfium
    let es_subcarpeta_de_cargo = carpeta
        .file_name()
        .is_some_and(|nombre| nombre == "deps" || nombre == "examples");
    match carpeta.parent() {
        Some(superior) if es_subcarpeta_de_cargo => superior.join(NOMBRE_EXTRACTOR),
        _ => carpeta.join(NOMBRE_EXTRACTOR),
    }
}

/// La app de ventana no debe abrir una consola por cada PDF
#[cfg(windows)]
fn sin_ventana(comando: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    comando.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn sin_ventana(_comando: &mut Command) {}

#[cfg(windows)]
fn limitar_memoria(
    hijo: &tokio::process::Child,
    limite: u64,
) -> Result<job_windows::JobConLimite, String> {
    let manejador = hijo
        .raw_handle()
        .ok_or("el extractor terminó antes de poder limitarlo")?;
    job_windows::JobConLimite::asociar(manejador, limite)
}

#[cfg(not(windows))]
fn limitar_memoria(_hijo: &tokio::process::Child, _limite: u64) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
mod job_windows {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    /// Job anónimo con límite de memoria por proceso; al cerrarlo mata a sus procesos
    pub(super) struct JobConLimite(HANDLE);

    // El manejador de un job es un identificador del núcleo, válido desde cualquier hilo
    unsafe impl Send for JobConLimite {}
    unsafe impl Sync for JobConLimite {}

    impl JobConLimite {
        pub(super) fn asociar(proceso: *mut c_void, limite: u64) -> Result<Self, String> {
            // SAFETY: llamadas documentadas de Win32 con punteros válidos durante la llamada; el
            // manejador del job se cierra en Drop y el del proceso lo posee tokio
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return Err("no se pudo crear el límite de memoria del extractor".to_string());
                }
                let job = Self(job);
                let mut informacion: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                informacion.BasicLimitInformation.LimitFlags =
                    JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                informacion.ProcessMemoryLimit = usize::try_from(limite).unwrap_or(usize::MAX);
                let configurado = SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&raw const informacion).cast(),
                    u32::try_from(std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                        .unwrap_or(u32::MAX),
                );
                if configurado == 0 || AssignProcessToJobObject(job.0, proceso) == 0 {
                    return Err(
                        "no se pudo aplicar el límite de memoria al extractor de PDF".to_string(),
                    );
                }
                Ok(job)
            }
        }
    }

    impl Drop for JobConLimite {
        fn drop(&mut self) {
            // SAFETY: el manejador se creó en `asociar` y solo se cierra aquí
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_una_respuesta_de_paginas_con_exito_se_acepta() {
        let paginas = interpretar(
            br#"{"paginas":[{"pagina":1,"texto":"a"}]}"#,
            true,
            Some(0),
            b"",
        )
        .unwrap();
        assert_eq!(paginas.len(), 1);
    }

    #[test]
    fn test_un_error_declarado_por_el_hijo_se_propaga() {
        let motivo = interpretar(br#"{"error":"falta pdfium"}"#, false, Some(1), b"").unwrap_err();
        assert_eq!(motivo, "falta pdfium");
    }

    #[test]
    fn test_una_salida_truncada_o_un_codigo_anomalo_es_un_error_controlado() {
        let truncada = interpretar(br#"{"paginas":[{"pag"#, false, None, b"boom").unwrap_err();
        assert!(
            truncada.contains("anómala") && truncada.contains("boom"),
            "{truncada}"
        );
        let sin_exito =
            interpretar(br#"{"paginas":[]}"#, false, Some(-1073741819), b"").unwrap_err();
        assert!(sin_exito.contains("-1073741819"), "{sin_exito}");
    }
}
