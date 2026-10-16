//! Definición estricta de esquemas Apache Arrow para las 6 tablas del Lakehouse

use std::sync::Arc;

use arrow_schema::{DataType, Field, Schema, SchemaRef};

use crate::application::ports::lakehouse_store::{
    TABLA_DOC_CATALOG, TABLA_DOC_CHUNKS, TABLA_DOUBLE_ENTRY_CELLS, TABLA_DOUBLE_ENTRY_TABLES,
    TABLA_OPERATIONAL_MATRIX, TABLA_REFERENCE_GRAPH,
};

/// Dimensión por defecto para los vectores de embeddings (BGE-M3 / FastEmbed)
pub const DIMENSION_EMBEDDINGS: i32 = 1024;

/// Esquema de `01_doc_catalog` (Catálogo Maestro y Control de Revisiones)
pub fn schema_doc_catalog() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("titulo", DataType::Utf8, false),
        Field::new("tipo_documental", DataType::Utf8, false),
        Field::new("emisor", DataType::Utf8, false),
        Field::new("fecha_emision", DataType::Utf8, false),
        Field::new("nivel_prelacion", DataType::Int32, false),
        Field::new("centro_id", DataType::Utf8, false),
        Field::new("vigente", DataType::Boolean, false),
        Field::new("version", DataType::Int32, false),
        Field::new("hash_sha256", DataType::Utf8, false),
    ]))
}

/// Esquema de `02_doc_chunks` (Fragmentos de Texto y Vectores)
pub fn schema_doc_chunks() -> SchemaRef {
    let item_field = Arc::new(Field::new("item", DataType::Float32, true));
    Arc::new(Schema::new(vec![
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("centro_id", DataType::Utf8, false),
        Field::new("seccion", DataType::Utf8, false),
        Field::new("contenido", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(item_field, DIMENSION_EMBEDDINGS),
            true,
        ),
        Field::new("vigente", DataType::Boolean, false),
    ]))
}

/// Esquema de `03_operational_matrix` (Tuplas Operativas y Obligaciones Técnicas)
pub fn schema_operational_matrix() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("tupla_id", DataType::Utf8, false),
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("centro_id", DataType::Utf8, false),
        Field::new("fase_operativa", DataType::Utf8, false),
        Field::new("rol_responsable", DataType::Utf8, false),
        Field::new("accion", DataType::Utf8, false),
        Field::new("parametro_control", DataType::Utf8, false),
        Field::new("tolerancia_criterio", DataType::Utf8, false),
        Field::new("condicion_aplicacion", DataType::Utf8, false),
        Field::new("referencia_normativa", DataType::Utf8, false),
        Field::new("vigente", DataType::Boolean, false),
    ]))
}

/// Esquema de `04_double_entry_tables` (Metadatos de Tablas de Doble Entrada)
pub fn schema_double_entry_tables() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("tabla_id", DataType::Utf8, false),
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("centro_id", DataType::Utf8, false),
        Field::new("nombre_tabla", DataType::Utf8, false),
        Field::new("coordenada_x_nombre", DataType::Utf8, false),
        Field::new("coordenada_y_nombre", DataType::Utf8, false),
        Field::new("nota_pie", DataType::Utf8, true),
        Field::new("vigente", DataType::Boolean, false),
    ]))
}

/// Esquema de `05_double_entry_cells` (Celdas Desnormalizadas con Rangos Numéricos y Categorías)
pub fn schema_double_entry_cells() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("celda_id", DataType::Utf8, false),
        Field::new("tabla_id", DataType::Utf8, false),
        Field::new("doc_id", DataType::Utf8, false),
        Field::new("centro_id", DataType::Utf8, false),
        Field::new("rango_x_min", DataType::Float64, true),
        Field::new("rango_x_max", DataType::Float64, true),
        Field::new("x_inclusivo_min", DataType::Boolean, false),
        Field::new("x_inclusivo_max", DataType::Boolean, false),
        Field::new("categoria_x", DataType::Utf8, true),
        Field::new("rango_y_min", DataType::Float64, true),
        Field::new("rango_y_max", DataType::Float64, true),
        Field::new("y_inclusivo_min", DataType::Boolean, false),
        Field::new("y_inclusivo_max", DataType::Boolean, false),
        Field::new("categoria_y", DataType::Utf8, true),
        Field::new("valor_resultado", DataType::Utf8, false),
        Field::new("unidad_medida", DataType::Utf8, false),
        Field::new("notas_especificas", DataType::Utf8, true),
        Field::new("vigente", DataType::Boolean, false),
    ]))
}

/// Esquema de `06_reference_graph` (Aristas del Grafo de Referencias y Prelación)
pub fn schema_reference_graph() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("arista_id", DataType::Utf8, false),
        Field::new("centro_id", DataType::Utf8, false),
        Field::new("doc_origen_id", DataType::Utf8, false),
        Field::new("doc_destino_id", DataType::Utf8, false),
        Field::new("tipo_relacion", DataType::Utf8, false),
        Field::new("peso_prelacion", DataType::Float32, false),
        Field::new("contexto_cita", DataType::Utf8, true),
        Field::new("vigente", DataType::Boolean, false),
    ]))
}

/// Resuelve el esquema correspondiente según el nombre de la tabla
pub fn obtener_esquema_por_nombre(nombre_tabla: &str) -> Option<SchemaRef> {
    match nombre_tabla {
        TABLA_DOC_CATALOG => Some(schema_doc_catalog()),
        TABLA_DOC_CHUNKS => Some(schema_doc_chunks()),
        TABLA_OPERATIONAL_MATRIX => Some(schema_operational_matrix()),
        TABLA_DOUBLE_ENTRY_TABLES => Some(schema_double_entry_tables()),
        TABLA_DOUBLE_ENTRY_CELLS => Some(schema_double_entry_cells()),
        TABLA_REFERENCE_GRAPH => Some(schema_reference_graph()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todos_los_esquemas_estan_definidos_y_tienen_campo_vigente() {
        let tablas = [
            TABLA_DOC_CATALOG,
            TABLA_DOC_CHUNKS,
            TABLA_OPERATIONAL_MATRIX,
            TABLA_DOUBLE_ENTRY_TABLES,
            TABLA_DOUBLE_ENTRY_CELLS,
            TABLA_REFERENCE_GRAPH,
        ];

        for nombre in tablas {
            let esquema = obtener_esquema_por_nombre(nombre)
                .unwrap_or_else(|| panic!("Esquema no encontrado para tabla {nombre}"));

            // Cada esquema debe tener al menos campos esenciales
            assert!(esquema.fields().len() >= 6);

            // Cada tabla debe poseer el campo `vigente` booleano para el control de versiones inmutable
            let campo_vigente = esquema
                .field_with_name("vigente")
                .unwrap_or_else(|_| panic!("La tabla {nombre} debe tener campo 'vigente'"));
            assert_eq!(campo_vigente.data_type(), &DataType::Boolean);

            // Cada tabla debe poseer el campo `centro_id` para garantizar el aislamiento multi-centro
            let campo_centro = esquema
                .field_with_name("centro_id")
                .unwrap_or_else(|_| panic!("La tabla {nombre} debe tener campo 'centro_id'"));
            assert_eq!(campo_centro.data_type(), &DataType::Utf8);
        }
    }

    #[test]
    fn test_esquema_chunks_vector_dimension_exacta() {
        let esquema = schema_doc_chunks();
        let campo_vector = esquema.field_with_name("vector").expect("Campo vector");
        match campo_vector.data_type() {
            DataType::FixedSizeList(item, dim) => {
                assert_eq!(*dim, DIMENSION_EMBEDDINGS);
                assert_eq!(item.data_type(), &DataType::Float32);
            }
            _ => panic!("El campo vector debe ser FixedSizeList"),
        }
    }

    #[test]
    fn test_nombre_tabla_desconocido_retorna_none() {
        assert!(obtener_esquema_por_nombre("tabla_inexistente").is_none());
    }
}
