//! Ejecución de los subcomandos de consola sobre la fachada `LakehouseApp`.
//!
//! Devuelve el texto y el código de salida en lugar de escribirlos, para que `main.rs` decida
//! dónde imprimir (stdout/stderr) y los tests puedan comprobarlo sin procesos.

use std::path::Path;

use serde_json::json;

use crate::application::use_cases::lakehouse_app::LakehouseApp;
use crate::domain::errors::DomainError;
use crate::presentation::cli::salida;
use crate::presentation::cli::{ComandoCli, FormatoSalida};

/// Códigos de salida del proceso
pub const CODIGO_OK: i32 = 0;
pub const CODIGO_ERROR: i32 = 1;
pub const CODIGO_ARGUMENTOS: i32 = 2;
pub const CODIGO_NO_INTEGRO: i32 = 3;

/// Resultado de un subcomando: lo que va a stdout, lo que va a stderr y el código de salida
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalidaComando {
    pub stdout: String,
    pub stderr: String,
    pub codigo: i32,
}

impl SalidaComando {
    fn ok(stdout: String) -> Self {
        Self {
            stdout,
            stderr: String::new(),
            codigo: CODIGO_OK,
        }
    }

    /// Error formateado según el formato pedido (en JSON también va a stdout)
    pub fn error(mensaje: &str, formato: FormatoSalida, codigo: i32) -> Self {
        let stdout = match formato {
            FormatoSalida::Json => salida::error_json(mensaje).to_string(),
            FormatoSalida::Legible => String::new(),
        };
        Self {
            stdout,
            stderr: format!("ERROR: {mensaje}"),
            codigo,
        }
    }
}

fn elegir(formato: FormatoSalida, json: serde_json::Value, legible: String) -> String {
    match formato {
        FormatoSalida::Json => json.to_string(),
        FormatoSalida::Legible => legible,
    }
}

/// Ejecuta un subcomando y traduce cualquier error de dominio a un código de salida
pub async fn ejecutar(
    app: &LakehouseApp,
    comando: ComandoCli,
    formato: FormatoSalida,
) -> SalidaComando {
    match ejecutar_comando(app, comando, formato).await {
        Ok(salida) => salida,
        Err(error) => SalidaComando::error(&error.to_string(), formato, CODIGO_ERROR),
    }
}

async fn ejecutar_comando(
    app: &LakehouseApp,
    comando: ComandoCli,
    formato: FormatoSalida,
) -> Result<SalidaComando, DomainError> {
    let texto = match comando {
        ComandoCli::Centros => {
            let centros = app.listar_centros().await?;
            elegir(
                formato,
                salida::centros_json(&centros),
                salida::centros_legible(&centros),
            )
        }
        ComandoCli::CrearCentro { id, ruta, nombre } => {
            let centro = app.crear_centro(&id, &ruta, nombre.as_deref()).await?;
            elegir(
                formato,
                json!({ "ok": true, "centro": salida::centro_json(&centro) }),
                format!(
                    "Centro '{}' creado en {} (inactivo; actívalo con --activar {}).",
                    centro.id(),
                    centro.manifiesto().ruta_almacenamiento,
                    centro.id()
                ),
            )
        }
        ComandoCli::Activar(id) => {
            let resumen = app.activar_centro(&id).await?;
            elegir(
                formato,
                json!({ "ok": true, "proyeccion": salida::proyeccion_json(&resumen) }),
                salida::proyeccion_legible(&resumen),
            )
        }
        ComandoCli::Desactivar(id) => {
            app.desactivar_centro(&id).await?;
            elegir(
                formato,
                json!({ "ok": true, "centro_id": id, "activo": false }),
                format!("Centro '{id}' desactivado: sus vistas se han retirado."),
            )
        }
        ComandoCli::EliminarCentro(id) => {
            let centro = app.eliminar_centro(&id).await?;
            elegir(
                formato,
                json!({ "ok": true, "eliminado": salida::centro_json(&centro) }),
                format!(
                    "Centro '{}' eliminado del registro. Su disco sigue intacto en {}.",
                    centro.id(),
                    centro.manifiesto().ruta_almacenamiento
                ),
            )
        }
        ComandoCli::Ingest { archivo, centro } => {
            let resultado = app.ingestar(Path::new(&archivo), &centro).await?;
            elegir(
                formato,
                salida::ingesta_json(&resultado),
                salida::ingesta_legible(&resultado),
            )
        }
        ComandoCli::Ask {
            texto,
            centro,
            modelo,
        } => {
            let respuesta = app.preguntar(&texto, &centro, modelo.as_deref()).await?;
            elegir(
                formato,
                salida::respuesta_json(&respuesta),
                salida::respuesta_legible(&respuesta),
            )
        }
        ComandoCli::Integridad(id) => {
            let informe = app.integridad(&id).await?;
            let mut resultado = SalidaComando::ok(elegir(
                formato,
                salida::integridad_json(&informe),
                salida::integridad_legible(&informe),
            ));
            if !informe.es_integro() {
                resultado.codigo = CODIGO_NO_INTEGRO;
            }
            return Ok(resultado);
        }
    };
    Ok(SalidaComando::ok(texto))
}
