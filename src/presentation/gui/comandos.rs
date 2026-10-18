//! Lógica de los comandos IPC de la ventana: adaptadores finos sobre `LakehouseApp` que
//! traducen sus resultados a DTOs serializables. Los `#[tauri::command]` de `gui/mod.rs`
//! solo extraen el estado de Tauri y delegan aquí (así se prueban sin ventana).
//!
//! Invariantes:
//! * La ingesta recibe la **ruta** del archivo (diálogo nativo o drop de la ventana), nunca
//!   su contenido por IPC; la ruta se valida en `LakehouseApp::ingestar_con_progreso`.
//! * El catálogo se lee con la misma tool de solo lectura que usa el agente
//!   (`execute_athena_sql` sobre `01_documentos_catalogo`): exige el centro activo.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::application::dtos::ingestion::{EtapaIngesta, ObservadorIngesta};
use crate::application::dtos::tools::{LakehouseToolName, ToolCall};
use crate::application::use_cases::lakehouse_app::{InformeIntegridadCentro, LakehouseApp};
use crate::domain::models::center::CentroConocimiento;

/// Evento de Tauri con el avance de una ingesta
pub const EVENTO_PROGRESO_INGESTA: &str = "ingesta-progreso";
/// Evento de Tauri con las rutas soltadas sobre la ventana
pub const EVENTO_DOCUMENTOS_SOLTADOS: &str = "documentos-soltados";
/// Evento de Tauri con el estado del arrastre (`entrar`, `salir`)
pub const EVENTO_ARRASTRE: &str = "documentos-arrastre";

/// Centro tal como lo consume el frontend
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CentroDto {
    pub id: String,
    pub nombre: String,
    pub ruta_disco: String,
    pub activo: bool,
}

impl From<&CentroConocimiento> for CentroDto {
    fn from(centro: &CentroConocimiento) -> Self {
        Self {
            id: centro.id().to_string(),
            nombre: centro.manifiesto().nombre.clone(),
            ruta_disco: centro.manifiesto().ruta_almacenamiento.clone(),
            activo: centro.esta_activo(),
        }
    }
}

/// Resultado de activar un centro
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivacionDto {
    pub centro_id: String,
    pub vistas: usize,
    pub filas: usize,
}

/// Avance de una ingesta (payload de `EVENTO_PROGRESO_INGESTA`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgresoIngestaDto {
    pub centro_id: String,
    pub archivo: String,
    pub etapa: EtapaIngesta,
    pub descripcion: String,
    pub porcentaje: u8,
}

impl ProgresoIngestaDto {
    pub fn new(centro_id: &str, ruta: &str, etapa: EtapaIngesta) -> Self {
        Self {
            centro_id: centro_id.to_string(),
            archivo: nombre_archivo(ruta),
            etapa,
            descripcion: etapa.descripcion().to_string(),
            porcentaje: etapa.porcentaje(),
        }
    }
}

/// Resultado de `ingestar_documento`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestaDto {
    pub archivo: String,
    pub doc_id: String,
    pub version: i32,
    pub ya_ingerido: bool,
    pub total_chunks: usize,
    pub total_hitos: usize,
    pub total_tablas: usize,
    pub total_celdas: usize,
    pub total_aristas: usize,
    pub versiones_derogadas: Vec<u32>,
    /// `true` si las vistas del centro ya incluyen el documento
    pub proyectado: bool,
    pub aviso: Option<String>,
}

/// Una revisión del catálogo `01_documentos_catalogo`
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentoCatalogoDto {
    pub doc_id: String,
    pub titulo: String,
    pub tipo_documento: String,
    pub nivel_prelacion: Option<i64>,
    pub revision: String,
    pub version_num: Option<i64>,
    pub fecha_vigor: Option<String>,
    pub fecha_derogacion: Option<String>,
    pub vigente: bool,
}

/// Diagnóstico de integridad para la ventana
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegridadDto {
    pub centro_id: String,
    pub integro: bool,
    pub tablas_ausentes: Vec<String>,
    pub ingestas_staged: Vec<String>,
    pub huerfanos: usize,
    pub versiones_vigentes_multiples: usize,
}

impl From<&InformeIntegridadCentro> for IntegridadDto {
    fn from(informe: &InformeIntegridadCentro) -> Self {
        Self {
            centro_id: informe.centro_id.clone(),
            integro: informe.es_integro(),
            tablas_ausentes: informe.tablas_ausentes.clone(),
            ingestas_staged: informe.informe.ingestas_staged.clone(),
            huerfanos: informe.informe.huerfanos.iter().map(|h| h.filas).sum(),
            versiones_vigentes_multiples: informe.informe.versiones_vigentes_multiples.len(),
        }
    }
}

const SQL_CATALOGO: &str = "SELECT doc_id, titulo, tipo_documento, nivel_prelacion, revision, version_num, fecha_vigor, fecha_derogacion, vigente FROM 01_documentos_catalogo ORDER BY nivel_prelacion ASC, doc_id ASC, version_num DESC";

pub async fn listar_centros(app: &LakehouseApp) -> Result<Vec<CentroDto>, String> {
    let centros = app.listar_centros().await.map_err(|e| e.to_string())?;
    Ok(centros.iter().map(CentroDto::from).collect())
}

pub async fn crear_centro(
    app: &LakehouseApp,
    centro_id: &str,
    ruta: &str,
    nombre: Option<&str>,
) -> Result<CentroDto, String> {
    let centro = app
        .crear_centro(centro_id, ruta, nombre)
        .await
        .map_err(|e| e.to_string())?;
    Ok(CentroDto::from(&centro))
}

pub async fn activar_centro(app: &LakehouseApp, centro_id: &str) -> Result<ActivacionDto, String> {
    let resumen = app
        .activar_centro(centro_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ActivacionDto {
        centro_id: resumen.centro_id,
        filas: resumen.vistas.iter().map(|v| v.filas).sum(),
        vistas: resumen.vistas.len(),
    })
}

pub async fn desactivar_centro(app: &LakehouseApp, centro_id: &str) -> Result<(), String> {
    app.desactivar_centro(centro_id)
        .await
        .map_err(|e| e.to_string())
}

/// Quita el centro del registro (su directorio en disco no se toca)
pub async fn eliminar_centro(app: &LakehouseApp, centro_id: &str) -> Result<CentroDto, String> {
    let centro = app
        .eliminar_centro(centro_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(CentroDto::from(&centro))
}

/// Ingesta por ruta; `observador` recibe cada etapa (la ventana la reemite como evento)
pub async fn ingestar_documento(
    app: &LakehouseApp,
    ruta: &str,
    centro_id: &str,
    observador: Option<ObservadorIngesta>,
) -> Result<IngestaDto, String> {
    let resultado = app
        .ingestar_con_progreso(Path::new(ruta), centro_id, observador)
        .await
        .map_err(|e| e.to_string())?;
    let informe = &resultado.informe;
    let proyectado = resultado.proyeccion.is_some();
    let aviso = resultado.aviso_proyeccion.clone().or_else(|| {
        (!proyectado && !informe.ya_ingerido).then(|| {
            format!(
                "El centro '{centro_id}' está inactivo: el documento quedó guardado y será consultable al activarlo"
            )
        })
    });
    Ok(IngestaDto {
        archivo: nombre_archivo(ruta),
        doc_id: informe.doc_id.clone(),
        version: informe.version,
        ya_ingerido: informe.ya_ingerido,
        total_chunks: informe.total_chunks,
        total_hitos: informe.total_tuplas,
        total_tablas: informe.total_tablas,
        total_celdas: informe.total_celdas,
        total_aristas: informe.total_aristas,
        versiones_derogadas: informe.versiones_derogadas.clone(),
        proyectado,
        aviso,
    })
}

/// Todas las revisiones confirmadas del centro activo (vigentes y derogadas)
pub async fn catalogo_centro(
    app: &LakehouseApp,
    centro_id: &str,
) -> Result<Vec<DocumentoCatalogoDto>, String> {
    let llamada = ToolCall {
        id: "gui-catalogo".to_string(),
        name: LakehouseToolName::ExecuteAthenaSql.as_str().to_string(),
        arguments: json!({ "centro_id": centro_id, "sql_query": SQL_CATALOGO }),
    };
    let resultado = app
        .servidor_herramientas()
        .ejecutar_herramienta(&llamada)
        .await
        .map_err(|e| e.to_string())?;
    if resultado.is_error {
        return Err(mensaje_de_error(&resultado.output));
    }
    Ok(resultado.output["resultado"]
        .as_array()
        .map(|filas| filas.iter().map(documento_catalogo).collect())
        .unwrap_or_default())
}

pub async fn integridad(app: &LakehouseApp, centro_id: &str) -> Result<IntegridadDto, String> {
    let informe = app.integridad(centro_id).await.map_err(|e| e.to_string())?;
    Ok(IntegridadDto::from(&informe))
}

fn documento_catalogo(fila: &Value) -> DocumentoCatalogoDto {
    let texto = |campo: &str| fila[campo].as_str().unwrap_or_default().to_string();
    let opcional = |campo: &str| fila[campo].as_str().map(str::to_string);
    DocumentoCatalogoDto {
        doc_id: texto("doc_id"),
        titulo: texto("titulo"),
        tipo_documento: texto("tipo_documento"),
        nivel_prelacion: fila["nivel_prelacion"].as_i64(),
        revision: texto("revision"),
        version_num: fila["version_num"].as_i64(),
        fecha_vigor: opcional("fecha_vigor"),
        fecha_derogacion: opcional("fecha_derogacion"),
        vigente: fila["vigente"].as_bool().unwrap_or(false),
    }
}

fn mensaje_de_error(salida: &Value) -> String {
    salida["error"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| salida.to_string())
}

/// Nombre del archivo sin la ruta (para mostrarlo; la ruta completa no se reenvía)
fn nombre_archivo(ruta: &str) -> String {
    Path::new(ruta)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| ruta.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progreso_lleva_solo_el_nombre_del_archivo_y_la_etapa() {
        let progreso =
            ProgresoIngestaDto::new("obra-a", "C:\\docs\\rev03.md", EtapaIngesta::Destilacion);
        assert_eq!(progreso.archivo, "rev03.md");
        let json = serde_json::to_value(&progreso).unwrap();
        assert_eq!(json["etapa"], "destilacion");
        assert_eq!(json["centroId"], "obra-a");
        assert_eq!(json["porcentaje"], 20);
    }

    #[test]
    fn test_documento_de_catalogo_desde_fila_sql() {
        let fila = json!({
            "doc_id": "PO-ALM-03", "titulo": "Recepción", "tipo_documento": "PROCEDIMIENTO_OBRA",
            "nivel_prelacion": 2, "revision": "Rev. 03", "version_num": 3,
            "fecha_vigor": "2026-03-01", "fecha_derogacion": "2026-09-15", "vigente": false
        });
        let documento = documento_catalogo(&fila);
        assert_eq!(documento.nivel_prelacion, Some(2));
        assert_eq!(documento.fecha_derogacion.as_deref(), Some("2026-09-15"));
        assert!(!documento.vigente);
    }
}
