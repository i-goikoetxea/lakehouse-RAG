//! DTOs de ingesta documental y esquema estructurado de destilación LLM (v2, especificación §4).
//!
//! Los campos de vocabulario cerrado (tipo de documento, fase, relación, tipo de eje y de nodo)
//! se deserializan como `String` para que un valor fuera de vocabulario no tumbe el paquete
//! entero: el JSON Schema que recibe Claude sí los restringe con `enum`, y la ingesta los
//! normaliza con los enums del dominio como red de seguridad.

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Serialize};

use crate::application::dtos::lakehouse_filas::{TipoEjeTabla, TipoNodoGrafo};
use crate::domain::models::document::TipoDocumento;
use crate::domain::models::operational_matrix::FaseOperativa;
use crate::domain::models::reference_graph::TipoRelacionNormativa;

/// Documento parseado con jerarquía estructural de títulos, secciones y tablas
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub nombre_archivo: String,
    pub titulo: String,
    pub secciones: Vec<ParsedSection>,
    pub tablas: Vec<ParsedTableRaw>,
    pub contenido_completo: String,
}

/// Sección documental con su código, su linaje jerárquico y la condición heredada (H7)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedSection {
    pub codigo: String,
    pub titulo: String,
    pub nivel: u8,
    pub contenido: String,
    /// Ruta completa de encabezados abiertos, incluida la propia sección (`H1 > H2 > H3`)
    pub breadcrumb: String,
    /// Condiciones de aplicación de la sección y de sus ancestros, unidas por ` Y `
    pub condicion_aplicacion: Option<String>,
}

/// Tabla cruda parseada desde formato Markdown u origen estructurado
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedTableRaw {
    pub titulo: String,
    pub encabezados: Vec<String>,
    pub filas: Vec<Vec<String>>,
    /// Notas al pie (`(*)`, `(1)`, `(a)`, `Nota:`), una por línea
    pub notas: Option<String>,
    /// Código de la sección que contiene la tabla
    pub seccion_codigo: Option<String>,
}

/// Paquete completo de destilación estructurada generado por Claude Headless (v2)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DistilledDocumentPackage {
    pub catalogo: CatalogoDestilado,
    /// Una entrada por fase operativa (anti-fusión, problema 1)
    pub hitos: Vec<HitoDestilado>,
    pub tablas: Vec<TablaDestilada>,
    pub aristas: Vec<AristaDestilada>,
}

/// Metadatos del documento (Tabla 01)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CatalogoDestilado {
    /// Código del documento SIN la revisión (`PO-ALM-03`)
    pub doc_id: String,
    pub titulo: String,
    #[schemars(schema_with = "esquema_tipo_documento")]
    pub tipo_documento: String,
    /// Revisión tal como figura en el documento (`Rev. 04`)
    pub revision: String,
    /// Número de la revisión (`Rev. 04` → 4)
    pub version_num: i64,
    /// Fecha de entrada en vigor en ISO 8601 (`AAAA-MM-DD`)
    pub fecha_vigor: String,
    pub emisor: String,
}

/// Hito operativo: una fila por fase con roles y registro separados (Tabla 03)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HitoDestilado {
    pub orden_secuencia: i64,
    #[schemars(schema_with = "esquema_fase_operativa")]
    pub fase_operativa: String,
    /// Material, equipo o elemento sobre el que recae la acción
    pub objeto_alcance: String,
    /// Quién ejecuta físicamente la acción (un solo rol)
    pub rol_ejecutor: String,
    /// Quién firma o responde formalmente (un solo rol, distinto del ejecutor)
    pub rol_responsable: String,
    /// Código exacto del formato a rellenar (`FOR-ALM-01`)
    pub registro_codigo: Option<String>,
    pub registro_nombre: Option<String>,
    pub criterio_aceptacion: String,
    /// Condición de contorno, incluida la heredada del encabezado
    pub condicion_aplicacion: Option<String>,
    /// Código de la sección de la que procede el hito (`2.2`)
    pub seccion_origen: String,
}

/// Variable de entrada de una tabla de doble entrada
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EjeDestilado {
    /// Nombre de la variable en snake_case (`espesor_nominal_mm`)
    pub nombre: String,
    #[schemars(schema_with = "esquema_tipo_eje")]
    pub tipo: String,
    pub unidad: Option<String>,
}

/// Límites de una celda en un eje: categoría exacta o rango con inclusividad por extremo
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LimitesDestilados {
    pub categoria: Option<String>,
    pub min: Option<f64>,
    /// `true` para `>=`, `false` para `>`
    pub min_inclusivo: bool,
    pub max: Option<f64>,
    /// `true` para `<=`, `false` para `<`
    pub max_inclusivo: bool,
}

/// Intersección de una tabla de doble entrada (Tabla 05)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CeldaDestilada {
    pub eje_x: LimitesDestilados,
    pub eje_y: LimitesDestilados,
    /// Tercer filtro de tablas de triple entrada o subtablas
    pub eje_z: Option<String>,
    /// Texto exacto de la celda (`125 (*)`, `No admisible`)
    pub valor_resultado: String,
    pub unidad_medida: Option<String>,
    /// Texto íntegro de la nota al pie que afecta a la celda
    pub nota_pie_condicion: Option<String>,
}

/// Tabla de doble entrada con sus ejes tipados y sus celdas (Tablas 04 y 05)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TablaDestilada {
    /// Etiqueta de la tabla en el documento (`4-A`)
    pub tabla_id: String,
    pub titulo: String,
    pub eje_x: EjeDestilado,
    pub eje_y: EjeDestilado,
    /// Magnitud que entrega la intersección (`temperatura_precalentamiento_min_c`)
    pub valor_salida_nombre: String,
    /// Código de la sección que contiene la tabla (`3.1`)
    pub seccion_origen: Option<String>,
    pub celdas: Vec<CeldaDestilada>,
}

/// Arista del grafo de referencias (Tabla 06)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AristaDestilada {
    #[schemars(schema_with = "esquema_tipo_nodo_origen")]
    pub origen_tipo: String,
    #[schemars(schema_with = "esquema_tipo_relacion")]
    pub tipo_relacion: String,
    /// Código canónico del destino (`FOR-ALM-01`, `UNE-EN 1090-2`, `PO-ALM-03`)
    pub destino_id: String,
    #[schemars(schema_with = "esquema_tipo_nodo_destino")]
    pub destino_tipo: String,
    pub contexto: Option<String>,
}

/// Fragmento semántico listo para indexar con embeddings (Tabla 02)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkToIngest {
    pub chunk_id: String,
    pub doc_id: String,
    pub seccion: String,
    pub contenido: String,
}

/// Tipos de nodo que pueden originar una arista según §4
pub const TIPOS_NODO_ORIGEN: [TipoNodoGrafo; 3] = [
    TipoNodoGrafo::Documento,
    TipoNodoGrafo::HitoOperativo,
    TipoNodoGrafo::Chunk,
];
/// Tipos de nodo que pueden ser destino de una arista
pub const TIPOS_NODO_DESTINO: [TipoNodoGrafo; 3] = [
    TipoNodoGrafo::Documento,
    TipoNodoGrafo::Registro,
    TipoNodoGrafo::Tabla,
];

fn esquema_enum<'a>(valores: impl IntoIterator<Item = &'a str>) -> Schema {
    SchemaObject {
        instance_type: Some(InstanceType::String.into()),
        enum_values: Some(valores.into_iter().map(serde_json::Value::from).collect()),
        ..Default::default()
    }
    .into()
}

fn esquema_tipo_documento(_: &mut SchemaGenerator) -> Schema {
    esquema_enum(TipoDocumento::VALORES_CANONICOS)
}

fn esquema_fase_operativa(_: &mut SchemaGenerator) -> Schema {
    esquema_enum(FaseOperativa::VALORES_CANONICOS)
}

fn esquema_tipo_relacion(_: &mut SchemaGenerator) -> Schema {
    esquema_enum(TipoRelacionNormativa::VALORES_CANONICOS)
}

fn esquema_tipo_eje(_: &mut SchemaGenerator) -> Schema {
    esquema_enum([
        TipoEjeTabla::RangoNumerico.as_str(),
        TipoEjeTabla::CategoriaExacta.as_str(),
    ])
}

fn esquema_tipo_nodo_origen(_: &mut SchemaGenerator) -> Schema {
    esquema_enum(TIPOS_NODO_ORIGEN.iter().map(TipoNodoGrafo::as_str))
}

fn esquema_tipo_nodo_destino(_: &mut SchemaGenerator) -> Schema {
    esquema_enum(TIPOS_NODO_DESTINO.iter().map(TipoNodoGrafo::as_str))
}

/// JSON Schema compacto del paquete v2 (lo que recibe `claude --json-schema`)
pub fn json_schema_destilacion() -> String {
    let schema = schemars::schema_for!(DistilledDocumentPackage);
    serde_json::to_string(&schema).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enum_de(schema: &serde_json::Value, ruta: &[&str]) -> Vec<String> {
        let mut nodo = schema;
        for paso in ruta {
            nodo = &nodo[*paso];
        }
        nodo["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("sin enum en {ruta:?}: {nodo}"))
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn test_json_schema_restringe_fase_relacion_y_tipo_documento_con_enum() {
        let schema: serde_json::Value = serde_json::from_str(&json_schema_destilacion()).unwrap();
        let def = |nombre: &str, campo: &str| {
            enum_de(&schema, &["definitions", nombre, "properties", campo])
        };
        assert_eq!(
            def("CatalogoDestilado", "tipo_documento"),
            TipoDocumento::VALORES_CANONICOS
        );
        assert_eq!(
            def("HitoDestilado", "fase_operativa"),
            FaseOperativa::VALORES_CANONICOS
        );
        assert_eq!(
            def("AristaDestilada", "tipo_relacion"),
            TipoRelacionNormativa::VALORES_CANONICOS
        );
        assert_eq!(
            def("AristaDestilada", "destino_tipo"),
            ["DOCUMENTO", "REGISTRO", "TABLA"]
        );
        assert_eq!(
            def("EjeDestilado", "tipo"),
            ["RANGO_NUMERICO", "CATEGORIA_EXACTA"]
        );
    }

    #[test]
    fn test_json_schema_separa_ejecutor_responsable_y_registro() {
        let schema: serde_json::Value = serde_json::from_str(&json_schema_destilacion()).unwrap();
        let hito = &schema["definitions"]["HitoDestilado"];
        for campo in [
            "rol_ejecutor",
            "rol_responsable",
            "registro_codigo",
            "registro_nombre",
            "objeto_alcance",
            "seccion_origen",
        ] {
            assert!(hito["properties"][campo].is_object(), "falta {campo}");
        }
        let requeridos = hito["required"].as_array().unwrap();
        assert!(requeridos.iter().any(|r| r == "rol_ejecutor"));
        assert!(requeridos.iter().any(|r| r == "rol_responsable"));
    }

    #[test]
    fn test_enum_invalido_no_impide_deserializar_el_paquete() {
        let json = r#"{"catalogo":{"doc_id":"X-1","titulo":"T","tipo_documento":"MEMORIA",
            "revision":"Rev. 1","version_num":1,"fecha_vigor":"2026-01-01","emisor":"E"},
            "hitos":[],"tablas":[],"aristas":[{"origen_tipo":"PLANETA","tipo_relacion":"AMISTAD",
            "destino_id":"Y","destino_tipo":"?","contexto":null}]}"#;
        let paquete: DistilledDocumentPackage = serde_json::from_str(json).unwrap();
        assert_eq!(paquete.aristas[0].tipo_relacion, "AMISTAD");
    }
}
