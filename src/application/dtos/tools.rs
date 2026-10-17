//! DTOs de las 6 herramientas v2 del Lakehouse (plan v2 §2.4): nombres, definiciones con
//! JSON Schema estricto, parámetros tipados (`deny_unknown_fields`) y resultados.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::domain::errors::DomainError;

/// Nombres de las herramientas oficiales del Lakehouse
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LakehouseToolName {
    #[serde(rename = "inspect_lakehouse_catalog")]
    InspectLakehouseCatalog,
    #[serde(rename = "query_operational_matrix")]
    QueryOperationalMatrix,
    #[serde(rename = "lookup_double_entry_table")]
    LookupDoubleEntryTable,
    #[serde(rename = "execute_athena_sql")]
    ExecuteAthenaSql,
    #[serde(rename = "traverse_normative_graph")]
    TraverseNormativeGraph,
    #[serde(rename = "search_normative_context")]
    SearchNormativeContext,
}

impl LakehouseToolName {
    pub const TODAS: [Self; 6] = [
        Self::InspectLakehouseCatalog,
        Self::QueryOperationalMatrix,
        Self::LookupDoubleEntryTable,
        Self::ExecuteAthenaSql,
        Self::TraverseNormativeGraph,
        Self::SearchNormativeContext,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InspectLakehouseCatalog => "inspect_lakehouse_catalog",
            Self::QueryOperationalMatrix => "query_operational_matrix",
            Self::LookupDoubleEntryTable => "lookup_double_entry_table",
            Self::ExecuteAthenaSql => "execute_athena_sql",
            Self::TraverseNormativeGraph => "traverse_normative_graph",
            Self::SearchNormativeContext => "search_normative_context",
        }
    }

    pub fn from_str_name(s: &str) -> Result<Self, DomainError> {
        let nombre = s.trim();
        Self::TODAS
            .into_iter()
            .find(|t| t.as_str() == nombre)
            .ok_or_else(|| {
                DomainError::ToolExecutionError(format!("Herramienta desconocida: '{nombre}'"))
            })
    }
}

/// Definición de una herramienta para el modelo de lenguaje (compatible con Anthropic Tools / JSON Schema)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Invocación de una herramienta emitida por el LLM
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Resultado de la ejecución de una herramienta
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub tool_name: String,
    pub output: serde_json::Value,
    pub is_error: bool,
}

// --- Parámetros (estrictos: un campo desconocido o de otro tipo es un error explícito) ---

/// `inspect_lakehouse_catalog`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectCatalogParams {
    pub centro_id: String,
    pub fecha_referencia: Option<String>,
}

/// En qué columna de rol se busca en `query_operational_matrix`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RolBuscado {
    Ejecutor,
    Responsable,
    #[default]
    Cualquiera,
}

/// `query_operational_matrix`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryOperationalMatrixParams {
    pub centro_id: String,
    pub fase: Option<String>,
    pub rol: Option<String>,
    #[serde(default)]
    pub rol_en: RolBuscado,
    pub registro: Option<String>,
    pub doc_id: Option<String>,
    pub objeto: Option<String>,
    pub fecha_referencia: Option<String>,
}

/// Valor de un eje en `lookup_double_entry_table`: número o categoría textual
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ValorEje {
    Numero(f64),
    Texto(String),
}

/// `lookup_double_entry_table`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LookupDoubleEntryTableParams {
    pub centro_id: String,
    pub tabla_id: Option<String>,
    pub busqueda_tabla: Option<String>,
    pub valor_x: ValorEje,
    pub valor_y: ValorEje,
    pub eje_z: Option<String>,
    pub fecha_referencia: Option<String>,
}

/// `execute_athena_sql`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteAthenaSqlParams {
    pub centro_id: String,
    pub sql_query: String,
    pub fecha_referencia: Option<String>,
}

/// `traverse_normative_graph`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraverseNormativeGraphParams {
    pub centro_id: String,
    pub nodo_origen: String,
    pub tipo_relacion: Option<Vec<String>>,
    pub max_saltos: Option<u8>,
    pub max_vecinos: Option<u16>,
    pub fecha_referencia: Option<String>,
}

/// `search_normative_context`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchNormativeContextParams {
    pub centro_id: String,
    pub consulta: String,
    pub top_k: Option<u16>,
    pub nivel_prelacion_max: Option<i8>,
    pub doc_id: Option<String>,
    pub fecha_referencia: Option<String>,
}

/// Interpreta los argumentos de una tool con un mensaje que nombra la herramienta
pub fn parsear_argumentos<T: serde::de::DeserializeOwned>(
    herramienta: LakehouseToolName,
    argumentos: &serde_json::Value,
) -> Result<T, DomainError> {
    serde_json::from_value(argumentos.clone()).map_err(|e| {
        DomainError::ToolExecutionError(format!(
            "Argumentos inválidos para {}: {e}",
            herramienta.as_str()
        ))
    })
}

/// Años admitidos en `fecha_referencia` (fuera de este rango la fecha es un error explícito)
pub const ANIO_MINIMO_REFERENCIA: i32 = 1900;
pub const ANIO_MAXIMO_REFERENCIA: i32 = 9999;

/// `fecha_referencia` ISO `AAAA-MM-DD`. `None` o vacía → consulta de lo vigente hoy.
pub fn parsear_fecha_referencia(texto: Option<&str>) -> Result<Option<NaiveDate>, DomainError> {
    let Some(limpio) = texto.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    let formato_iso = limpio.len() == 10
        && limpio.char_indices().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    let fecha = formato_iso
        .then(|| NaiveDate::parse_from_str(limpio, "%Y-%m-%d").ok())
        .flatten()
        .ok_or_else(|| {
            DomainError::ValidationError(format!(
                "fecha_referencia inválida '{limpio}': usa el formato ISO AAAA-MM-DD"
            ))
        })?;
    let anio = chrono::Datelike::year(&fecha);
    if !(ANIO_MINIMO_REFERENCIA..=ANIO_MAXIMO_REFERENCIA).contains(&anio) {
        return Err(DomainError::ValidationError(format!(
            "fecha_referencia fuera de rango ({ANIO_MINIMO_REFERENCIA}-{ANIO_MAXIMO_REFERENCIA}): '{limpio}'"
        )));
    }
    Ok(Some(fecha))
}

// --- Definiciones para el LLM ---

fn propiedad_centro() -> serde_json::Value {
    json!({
        "type": "string",
        "description": "Identificador del centro de conocimiento activo. Todas las consultas se limitan a este centro."
    })
}

fn propiedad_fecha() -> serde_json::Value {
    json!({
        "type": "string",
        "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$",
        "description": "Opcional (ISO AAAA-MM-DD). Sin ella → solo lo vigente hoy. Con ella → las revisiones vigentes en esa fecha (fecha_vigor <= D < fecha_derogacion)."
    })
}

fn esquema_objeto(propiedades: serde_json::Value, requeridos: &[&str]) -> serde_json::Value {
    json!({
        "type": "object",
        "properties": propiedades,
        "required": requeridos,
        "additionalProperties": false
    })
}

/// Vistas SQL y columnas que `execute_athena_sql` admite (nombres canónicos)
pub const DESCRIPCION_VISTAS_SQL: &str = "Vistas (usa el nombre canónico; la herramienta lo traduce a la vista del centro activo y rechaza cualquier otra tabla). Contienen TODAS las revisiones confirmadas: filtra vigente = true para lo vigente, o usa la ventana de 01 (fecha_vigor <= D AND (fecha_derogacion IS NULL OR fecha_derogacion > D)) para otra fecha.
- 01_documentos_catalogo(doc_id, version_num, vigente, titulo, tipo_documento, nivel_prelacion, revision, fecha_vigor DATE, fecha_derogacion DATE, hash_sha256)
- 02_chunks_normativa(chunk_id, doc_id, version_num, vigente, nivel_prelacion, breadcrumb_jerarquia, condicion_aplicacion, texto_contenido, tablas_vinculadas, hitos_vinculados)
- 03_matriz_operativa(hito_id, doc_id, version_num, vigente, orden_secuencia, fase_operativa, objeto_alcance, rol_ejecutor, rol_responsable, registro_codigo, registro_nombre, criterio_aceptacion, condicion_aplicacion, nivel_prelacion, calidad_extraccion, chunk_origen_id)
- 04_catalogo_tablas(tabla_id, doc_id, version_num, vigente, titulo_tabla, eje_x_nombre, eje_x_tipo, eje_x_unidad, eje_y_nombre, eje_y_tipo, eje_y_unidad, valor_salida_nombre, instrucciones_uso_sql)
- 05_celdas_doble_entrada(celda_id, tabla_id, doc_id, version_num, vigente, eje_x_categoria, eje_x_min, eje_x_min_inclusivo, eje_x_max, eje_x_max_inclusivo, eje_y_categoria, eje_y_min, eje_y_min_inclusivo, eje_y_max, eje_y_max_inclusivo, eje_z_filtro_adicional, valor_resultado_texto, valor_resultado_num, unidad_medida, nota_pie_condicion, nivel_prelacion)
- 06_grafo_referencias(edge_id, doc_id, version_num, vigente, origen_id, origen_tipo, tipo_relacion, destino_id, destino_tipo, contexto_relacion)
Todas llevan además centro_id, ingest_id y estado_ingesta. fase_operativa va en MAYÚSCULAS sin tildes (RECEPCION, CUSTODIA, INSPECCION…).";

impl ToolDefinition {
    fn nueva(nombre: LakehouseToolName, descripcion: &str, parametros: serde_json::Value) -> Self {
        Self {
            name: nombre.as_str().to_string(),
            description: descripcion.to_string(),
            parameters: parametros,
        }
    }

    /// Las 6 definiciones oficiales de herramientas del Lakehouse (plan v2 §2.4)
    pub fn lakehouse_tools() -> Vec<Self> {
        vec![
            Self::nueva(
                LakehouseToolName::InspectLakehouseCatalog,
                "Catálogo del centro activo: documentos (tipo, nivel de prelación 1=cliente…5=norma, revisión y ventana de vigencia) e inventario de tablas de doble entrada con sus ejes, unidades e instrucciones de uso. Úsala primero para saber qué hay.",
                esquema_objeto(
                    json!({ "centro_id": propiedad_centro(), "fecha_referencia": propiedad_fecha() }),
                    &["centro_id"],
                ),
            ),
            Self::nueva(
                LakehouseToolName::QueryOperationalMatrix,
                "Matriz operativa (una fila por hito, nunca mezcla filas): filtros EXACTOS por fase (se normaliza: 'Recepción' = RECEPCION), rol (ejecutor o responsable, sin tildes ni mayúsculas), registro, doc y objeto. Devuelve ejecutor, responsable y registro en campos separados, la condicion_aplicacion, el nivel de prelación y la cita (doc_id, revisión, breadcrumb). Úsala para roles y registros.",
                esquema_objeto(
                    json!({
                        "centro_id": propiedad_centro(),
                        "fase": { "type": "string", "description": "Fase operativa (p. ej. CUSTODIA, Recepción, Inspección)." },
                        "rol": { "type": "string", "description": "Rol exacto, p. ej. 'Jefe de Producción'." },
                        "rol_en": { "type": "string", "enum": ["ejecutor", "responsable", "cualquiera"], "description": "Columna donde se busca el rol (por defecto cualquiera)." },
                        "registro": { "type": "string", "description": "Código exacto del registro, p. ej. FOR-ALM-05." },
                        "doc_id": { "type": "string", "description": "Código del documento, p. ej. PO-ALM-03." },
                        "objeto": { "type": "string", "description": "Texto contenido en el objeto/alcance del hito." },
                        "fecha_referencia": propiedad_fecha()
                    }),
                    &["centro_id"],
                ),
            ),
            Self::nueva(
                LakehouseToolName::LookupDoubleEntryTable,
                "Cruce EXACTO en una tabla de doble entrada (respeta <, <=, > y >= de cada extremo; no interpola). Indica tabla_id (o busqueda_tabla), valor_x y valor_y (número o categoría) y, si la tabla lo tiene, eje_z. Devuelve valor_resultado_texto/num, la nota_pie_condicion (OBLIGATORIO citarla si existe), la cita y la prelación. Si no hay celda o hay varias candidatas lo dice explícitamente.",
                esquema_objeto(
                    json!({
                        "centro_id": propiedad_centro(),
                        "tabla_id": { "type": "string", "description": "Identificador de la tabla (p. ej. 4-A)." },
                        "busqueda_tabla": { "type": "string", "description": "Texto para localizar la tabla por título o ejes si no conoces el tabla_id." },
                        "valor_x": { "type": ["number", "string"], "description": "Valor del eje X: número finito o categoría." },
                        "valor_y": { "type": ["number", "string"], "description": "Valor del eje Y: número finito o categoría." },
                        "eje_z": { "type": "string", "description": "Filtro adicional (tercer eje), si la tabla lo usa." },
                        "fecha_referencia": propiedad_fecha()
                    }),
                    &["centro_id", "valor_x", "valor_y"],
                ),
            ),
            Self::nueva(
                LakehouseToolName::ExecuteAthenaSql,
                &format!(
                    "SQL de solo lectura (una única sentencia SELECT o WITH; se rechazan DDL, DML, COPY, varias sentencias y funciones de fichero) sobre las tablas COMPLETAS del centro activo: úsala para listados exhaustivos, recuentos y agregaciones. Límite de filas y tiempo; si el resultado se trunca lo indica. fecha_referencia no reescribe tu SQL: aplícala tú con la ventana de 01.\n{DESCRIPCION_VISTAS_SQL}"
                ),
                esquema_objeto(
                    json!({
                        "centro_id": propiedad_centro(),
                        "sql_query": { "type": "string", "description": "Una única consulta SELECT/WITH." },
                        "fecha_referencia": propiedad_fecha()
                    }),
                    &["centro_id", "sql_query"],
                ),
            ),
            Self::nueva(
                LakehouseToolName::TraverseNormativeGraph,
                "Recorre el grafo de referencias normativas (documentos, registros y tablas) desde un nodo: solo aristas vigentes (o a fecha_referencia), filtro por tipo_relacion, máximo 2 saltos y max_vecinos por nodo. Los nodos con muchas aristas se devuelven como hub_resumido (grado y relaciones principales) sin expandirlos. Respuesta acotada (≤ 50 nodos) y ordenada por prelación.",
                esquema_objeto(
                    json!({
                        "centro_id": propiedad_centro(),
                        "nodo_origen": { "type": "string", "description": "Código del documento, registro o tabla de partida (p. ej. PO-ALM-03)." },
                        "tipo_relacion": { "type": "array", "items": { "type": "string", "enum": ["EXIGE_LLENAR_REGISTRO", "DEROGA_A", "MODIFICA_APARTADO", "REMITE_A_TABLA", "SUBORDINADO_A_NORMA", "REFERENCIA_A"] }, "description": "Tipos de relación a seguir (por defecto todos)." },
                        "max_saltos": { "type": "integer", "minimum": 1, "maximum": 2, "description": "Saltos (por defecto 2)." },
                        "max_vecinos": { "type": "integer", "minimum": 1, "maximum": 50, "description": "Vecinos por nodo (por defecto 15)." },
                        "fecha_referencia": propiedad_fecha()
                    }),
                    &["centro_id", "nodo_origen"],
                ),
            ),
            Self::nueva(
                LakehouseToolName::SearchNormativeContext,
                "Búsqueda híbrida (semántica + léxica) en los fragmentos normativos y en los hitos del centro activo. Cada resultado trae breadcrumb_jerarquia, condicion_aplicacion (la condición heredada del apartado: menciónala siempre), nivel de prelación y cita. Los resultados se agrupan por tema; cuando documentos de distinta prelación tratan el mismo tema, prevalece: true marca el de menor nivel y supletorio: true los demás. Úsala para contexto, alcance y condiciones.",
                esquema_objeto(
                    json!({
                        "centro_id": propiedad_centro(),
                        "consulta": { "type": "string", "description": "Pregunta o términos a buscar." },
                        "top_k": { "type": "integer", "minimum": 1, "maximum": 20, "description": "Resultados (por defecto 8)." },
                        "nivel_prelacion_max": { "type": "integer", "minimum": 1, "maximum": 5, "description": "Solo documentos con nivel de prelación <= este valor." },
                        "doc_id": { "type": "string", "description": "Limitar a un documento." },
                        "fecha_referencia": propiedad_fecha()
                    }),
                    &["centro_id", "consulta"],
                ),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lakehouse_tool_names_as_str_y_parsing() {
        for nombre in LakehouseToolName::TODAS {
            assert_eq!(
                LakehouseToolName::from_str_name(nombre.as_str()).unwrap(),
                nombre
            );
        }
        let err = LakehouseToolName::from_str_name("herramienta_fantasma").unwrap_err();
        assert!(matches!(err, DomainError::ToolExecutionError(_)));
    }

    #[test]
    fn test_las_6_tools_tienen_esquema_estricto_con_centro_y_fecha() {
        let tools = ToolDefinition::lakehouse_tools();
        assert_eq!(tools.len(), 6);
        for tool in tools {
            let esquema = &tool.parameters;
            assert_eq!(esquema["additionalProperties"], false, "{}", tool.name);
            assert!(esquema["properties"]["centro_id"].is_object());
            assert!(esquema["properties"]["fecha_referencia"].is_object());
            let requeridos = esquema["required"].as_array().unwrap();
            assert!(requeridos.contains(&json!("centro_id")));
        }
    }

    #[test]
    fn test_parametros_rechazan_campos_desconocidos_y_tipos_erroneos() {
        let desconocido = json!({ "centro_id": "c", "sql_query": "SELECT 1", "extra": 1 });
        assert!(
            parsear_argumentos::<ExecuteAthenaSqlParams>(
                LakehouseToolName::ExecuteAthenaSql,
                &desconocido
            )
            .is_err()
        );
        let tipo_erroneo = json!({ "centro_id": 7, "consulta": "x" });
        assert!(
            parsear_argumentos::<SearchNormativeContextParams>(
                LakehouseToolName::SearchNormativeContext,
                &tipo_erroneo
            )
            .is_err()
        );
        let lookup: LookupDoubleEntryTableParams = parsear_argumentos(
            LakehouseToolName::LookupDoubleEntryTable,
            &json!({ "centro_id": "c", "tabla_id": "4-A", "valor_x": 30, "valor_y": "0,42" }),
        )
        .unwrap();
        assert_eq!(lookup.valor_x, ValorEje::Numero(30.0));
        assert_eq!(lookup.valor_y, ValorEje::Texto("0,42".into()));
    }

    #[test]
    fn test_fecha_referencia_iso_valida_invalida_y_fuera_de_rango() {
        assert_eq!(parsear_fecha_referencia(None).unwrap(), None);
        assert_eq!(parsear_fecha_referencia(Some("  ")).unwrap(), None);
        assert_eq!(
            parsear_fecha_referencia(Some("2026-06-01")).unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 1)
        );
        assert!(
            parsear_fecha_referencia(Some("9999-12-31"))
                .unwrap()
                .is_some()
        );
        for invalida in [
            "01/06/2026",
            "2026-13-01",
            "2026-02-30",
            "ayer",
            "+10000-01-01",
            "0001-01-01",
            "2026-6-1",
        ] {
            assert!(
                parsear_fecha_referencia(Some(invalida)).is_err(),
                "{invalida}"
            );
        }
    }
}
