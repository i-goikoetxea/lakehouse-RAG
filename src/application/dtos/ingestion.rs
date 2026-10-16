//! DTOs de Ingesta Documental y Esquemas Estructurados de Destilación LLM

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Documento parseado con jerarquía estructural de títulos, secciones y tablas
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub nombre_archivo: String,
    pub titulo: String,
    pub secciones: Vec<ParsedSection>,
    pub tablas: Vec<ParsedTableRaw>,
    pub contenido_completo: String,
}

/// Sección documental con código jerárquico y nivel de encabezado
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedSection {
    pub codigo: String,
    pub titulo: String,
    pub nivel: u8,
    pub contenido: String,
}

/// Tabla cruda parseada desde formato Markdown u origen estructurado
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedTableRaw {
    pub titulo: String,
    pub encabezados: Vec<String>,
    pub filas: Vec<Vec<String>>,
    pub notas: Option<String>,
}

/// Paquete completo de destilación estructurada generado por Claude Headless
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DistilledDocumentPackage {
    pub catalogo: DistilledCatalog,
    pub tuplas_operativas: Vec<DistilledOperationalTuple>,
    pub tablas_doble_entrada: Vec<DistilledDoubleEntryTable>,
    pub aristas_referencia: Vec<DistilledReferenceEdge>,
}

/// Metadatos maestros del catálogo documental destilados (Tabla 01)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DistilledCatalog {
    pub doc_id: String,
    pub titulo: String,
    pub tipo_documental: String,
    pub emisor: String,
    pub fecha_emision: String,
    pub nivel_prelacion: i32,
    pub version: i32,
}

/// Tupla operativa y obligaciones técnicas extraídas (Tabla 03)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DistilledOperationalTuple {
    pub seccion: String,
    pub fase_operativa: String,
    pub rol_responsable: String,
    pub accion: String,
    pub parametro_control: String,
    pub tolerancia_criterio: String,
    pub condicion_aplicacion: String,
    pub referencia_normativa: String,
}

/// Tabla de doble entrada con celdas desnormalizadas (Tablas 04 y 05)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DistilledDoubleEntryTable {
    pub nombre_tabla: String,
    pub coordenada_x_nombre: String,
    pub coordenada_y_nombre: String,
    pub nota_pie: Option<String>,
    pub celdas: Vec<DistilledCell>,
}

/// Celda individual de tabla cruzada con rangos numéricos o categorías
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DistilledCell {
    pub rango_x_min: Option<f64>,
    pub rango_x_max: Option<f64>,
    pub x_inclusivo_min: bool,
    pub x_inclusivo_max: bool,
    pub categoria_x: Option<String>,
    pub rango_y_min: Option<f64>,
    pub rango_y_max: Option<f64>,
    pub y_inclusivo_min: bool,
    pub y_inclusivo_max: bool,
    pub categoria_y: Option<String>,
    pub valor_resultado: String,
    pub unidad_medida: String,
    pub notas_especificas: Option<String>,
}

/// Arista de referencia o derogación hacia otros documentos (Tabla 06)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DistilledReferenceEdge {
    pub doc_destino_id: String,
    pub tipo_relacion: String,
    pub peso_prelacion: f32,
    pub contexto_cita: Option<String>,
}

/// Fragmento semántico listo para indexar con embeddings (Tabla 02)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkToIngest {
    pub chunk_id: String,
    pub doc_id: String,
    pub seccion: String,
    pub contenido: String,
}
