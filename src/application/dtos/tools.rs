//! DTOs para definición, llamadas y resultados de herramientas operativas del Lakehouse

use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

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
}

impl LakehouseToolName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InspectLakehouseCatalog => "inspect_lakehouse_catalog",
            Self::QueryOperationalMatrix => "query_operational_matrix",
            Self::LookupDoubleEntryTable => "lookup_double_entry_table",
            Self::ExecuteAthenaSql => "execute_athena_sql",
            Self::TraverseNormativeGraph => "traverse_normative_graph",
        }
    }

    pub fn from_str_name(s: &str) -> Result<Self, DomainError> {
        match s.trim() {
            "inspect_lakehouse_catalog" => Ok(Self::InspectLakehouseCatalog),
            "query_operational_matrix" => Ok(Self::QueryOperationalMatrix),
            "lookup_double_entry_table" => Ok(Self::LookupDoubleEntryTable),
            "execute_athena_sql" => Ok(Self::ExecuteAthenaSql),
            "traverse_normative_graph" => Ok(Self::TraverseNormativeGraph),
            otro => Err(DomainError::ToolExecutionError(format!(
                "Herramienta desconocida: '{otro}'"
            ))),
        }
    }
}

/// Definición de una herramienta para el modelo de lenguaje (compatible con Anthropic Tools / JSON Schema)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Parámetros para la herramienta inspect_lakehouse_catalog
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InspectCatalogParams {
    /// Identificador del centro de conocimiento activo (ej. 'planta_alfa')
    pub centro_id: String,
    /// Si es true, retorna solo documentos con vigencia activa (por defecto true)
    pub solo_vigentes: Option<bool>,
}

/// Parámetros para la herramienta query_operational_matrix
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueryOperationalMatrixParams {
    /// Identificador del centro de conocimiento activo
    pub centro_id: String,
    /// Fase operativa a filtrar (ej. 'Recepcion', 'Custodia', 'Inspeccion')
    pub fase: Option<String>,
    /// Subcadena o nombre del parámetro técnico a buscar (ej. 'temperatura', 'presion')
    pub parametro: Option<String>,
}

/// Parámetros para la herramienta lookup_double_entry_table
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LookupDoubleEntryTableParams {
    /// Identificador del centro de conocimiento activo
    pub centro_id: String,
    /// Nombre o identificador de la tabla de doble entrada (ej. 'tabla_tolerancias_soldadura')
    pub tabla_id: String,
    /// Valor numérico continuo a evaluar en la fila o columna
    pub valor_x: f64,
    /// Categoría o etiqueta textual a cruzar
    pub categoria_y: String,
}

/// Parámetros para la herramienta execute_athena_sql
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteAthenaSqlParams {
    /// Identificador del centro de conocimiento activo
    pub centro_id: String,
    /// Consulta analítica SQL determinista de solo lectura (SELECT ...)
    pub sql_query: String,
}

/// Parámetros para la herramienta traverse_normative_graph
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TraverseNormativeGraphParams {
    /// Identificador del centro de conocimiento activo
    pub centro_id: String,
    /// Identificador del documento de origen (ej. 'DOC-NORM-001')
    pub doc_origen_id: String,
    /// Modo de recorrido: 'prevalencia' (busca la norma vigente que deroga), 'linaje' (árbol completo), 'adyacentes' (referencias directas)
    pub modo: String,
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

impl ToolDefinition {
    /// Genera la lista oficial de las 5 definiciones de herramientas del Lakehouse
    pub fn lakehouse_tools() -> Vec<Self> {
        vec![
            Self {
                name: LakehouseToolName::InspectLakehouseCatalog.as_str().to_string(),
                description: "Inspecciona el catálogo de especificaciones y documentos normativos del centro activo, permitiendo auditar vigencia, versiones y niveles de prelación jurídica.".to_string(),
                parameters: serde_json::to_value(schema_for!(InspectCatalogParams)).unwrap_or_default(),
            },
            Self {
                name: LakehouseToolName::QueryOperationalMatrix.as_str().to_string(),
                description: "Consulta la matriz operacional de parámetros técnicos, rangos tolerables, valores nominales y condiciones de aplicación por fase operativa (Recepción, Custodia, Inspección).".to_string(),
                parameters: serde_json::to_value(schema_for!(QueryOperationalMatrixParams)).unwrap_or_default(),
            },
            Self {
                name: LakehouseToolName::LookupDoubleEntryTable.as_str().to_string(),
                description: "Resuelve una celda técnica en tablas matriciales cruzadas de doble entrada mediante interpolación/inclusión de rangos numéricos continuos, categorías y notas al pie normativas.".to_string(),
                parameters: serde_json::to_value(schema_for!(LookupDoubleEntryTableParams)).unwrap_or_default(),
            },
            Self {
                name: LakehouseToolName::ExecuteAthenaSql.as_str().to_string(),
                description: "Ejecuta consultas analíticas SQL deterministas (solo lectura SELECT) sobre las tablas del Lakehouse, respetando estrictamente el aislamiento del centro de conocimiento.".to_string(),
                parameters: serde_json::to_value(schema_for!(ExecuteAthenaSqlParams)).unwrap_or_default(),
            },
            Self {
                name: LakehouseToolName::TraverseNormativeGraph.as_str().to_string(),
                description: "Navega el grafo de precedencia y referencias normativas para resolver la norma prevalente tras derogaciones sucesivas o auditar el linaje documental completo.".to_string(),
                parameters: serde_json::to_value(schema_for!(TraverseNormativeGraphParams)).unwrap_or_default(),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lakehouse_tool_names_as_str_y_parsing() {
        let name = LakehouseToolName::InspectLakehouseCatalog;
        assert_eq!(name.as_str(), "inspect_lakehouse_catalog");
        let parsed = LakehouseToolName::from_str_name("inspect_lakehouse_catalog").unwrap();
        assert_eq!(parsed, name);

        let err = LakehouseToolName::from_str_name("herramienta_fantasma").unwrap_err();
        assert!(matches!(err, DomainError::ToolExecutionError(_)));
    }

    #[test]
    fn test_lakehouse_tools_schemas_son_validos() {
        let tools = ToolDefinition::lakehouse_tools();
        assert_eq!(tools.len(), 5);

        for tool in tools {
            assert!(!tool.name.is_empty());
            assert!(!tool.description.is_empty());
            assert!(tool.parameters.is_object());
            assert!(tool.parameters.get("properties").is_some());
        }
    }
}
