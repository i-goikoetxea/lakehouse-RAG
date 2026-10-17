//! Esquemas Apache Arrow v2 de las 6 tablas del Lakehouse (§4 de la especificación y plan v2 §2.2)

use std::sync::Arc;

use arrow_schema::{DataType, Field, Schema, SchemaRef};

use crate::application::ports::lakehouse_store::{
    TABLA_CATALOGO_TABLAS, TABLA_CELDAS_DOBLE_ENTRADA, TABLA_CHUNKS_NORMATIVA,
    TABLA_DOCUMENTOS_CATALOGO, TABLA_GRAFO_REFERENCIAS, TABLA_MATRIZ_OPERATIVA,
};

/// Versión del esquema que se guarda en `center_manifest.json`
pub const VERSION_ESQUEMA: u32 = 2;

/// Dimensión de los vectores de embeddings (BGE-M3 / multilingual-e5-large)
pub const DIMENSION_EMBEDDINGS: i32 = 1024;

/// Columnas transaccionales y de linaje comunes a las 6 tablas
pub const COLUMNA_CENTRO_ID: &str = "centro_id";
pub const COLUMNA_DOC_ID: &str = "doc_id";
pub const COLUMNA_VERSION_NUM: &str = "version_num";
pub const COLUMNA_VIGENTE: &str = "vigente";
pub const COLUMNA_INGEST_ID: &str = "ingest_id";
pub const COLUMNA_ESTADO_INGESTA: &str = "estado_ingesta";

fn utf8(nombre: &str) -> Field {
    Field::new(nombre, DataType::Utf8, false)
}

fn utf8_nulable(nombre: &str) -> Field {
    Field::new(nombre, DataType::Utf8, true)
}

fn tipo_vector() -> DataType {
    DataType::FixedSizeList(
        Arc::new(Field::new("item", DataType::Float32, true)),
        DIMENSION_EMBEDDINGS,
    )
}

fn vector_nulable(nombre: &str) -> Field {
    Field::new(nombre, tipo_vector(), true)
}

fn lista_utf8(nombre: &str) -> Field {
    Field::new(
        nombre,
        DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
        false,
    )
}

fn nivel_prelacion() -> Field {
    Field::new("nivel_prelacion", DataType::Int8, false)
}

fn columnas_comunes() -> Vec<Field> {
    vec![
        utf8(COLUMNA_CENTRO_ID),
        utf8(COLUMNA_DOC_ID),
        Field::new(COLUMNA_VERSION_NUM, DataType::Int32, false),
        Field::new(COLUMNA_VIGENTE, DataType::Boolean, false),
        utf8(COLUMNA_INGEST_ID),
        utf8(COLUMNA_ESTADO_INGESTA),
    ]
}

fn esquema_con_comunes(clave_primaria: Option<&str>, especificas: Vec<Field>) -> SchemaRef {
    let mut campos = Vec::with_capacity(especificas.len() + 7);
    if let Some(pk) = clave_primaria {
        campos.push(utf8(pk));
    }
    campos.extend(columnas_comunes());
    campos.extend(especificas);
    Arc::new(Schema::new(campos))
}

/// `01_documentos_catalogo`: control documental, prelación y ventana de vigencia
pub fn schema_documentos_catalogo() -> SchemaRef {
    esquema_con_comunes(
        None,
        vec![
            utf8("titulo"),
            utf8("tipo_documento"),
            nivel_prelacion(),
            utf8("revision"),
            Field::new("fecha_vigor", DataType::Date32, false),
            Field::new("fecha_derogacion", DataType::Date32, true),
            utf8("hash_sha256"),
        ],
    )
}

/// `02_chunks_normativa`: contexto semántico con linaje jerárquico
pub fn schema_chunks_normativa() -> SchemaRef {
    esquema_con_comunes(
        Some("chunk_id"),
        vec![
            nivel_prelacion(),
            utf8("breadcrumb_jerarquia"),
            utf8_nulable("condicion_aplicacion"),
            utf8("texto_contenido"),
            lista_utf8("tablas_vinculadas"),
            lista_utf8("hitos_vinculados"),
            vector_nulable("vector"),
        ],
    )
}

/// `03_matriz_operativa`: un hito por fila (anti-fusión)
pub fn schema_matriz_operativa() -> SchemaRef {
    esquema_con_comunes(
        Some("hito_id"),
        vec![
            Field::new("orden_secuencia", DataType::Int32, false),
            utf8("fase_operativa"),
            utf8("objeto_alcance"),
            utf8("rol_ejecutor"),
            utf8("rol_responsable"),
            utf8_nulable("registro_codigo"),
            utf8_nulable("registro_nombre"),
            utf8("criterio_aceptacion"),
            utf8_nulable("condicion_aplicacion"),
            nivel_prelacion(),
            utf8("calidad_extraccion"),
            utf8_nulable("chunk_origen_id"),
            vector_nulable("vector_hito"),
        ],
    )
}

/// `04_catalogo_tablas`: glue catalog de tablas de doble entrada
pub fn schema_catalogo_tablas() -> SchemaRef {
    esquema_con_comunes(
        Some("tabla_id"),
        vec![
            utf8("titulo_tabla"),
            utf8("eje_x_nombre"),
            utf8("eje_x_tipo"),
            utf8_nulable("eje_x_unidad"),
            utf8("eje_y_nombre"),
            utf8("eje_y_tipo"),
            utf8_nulable("eje_y_unidad"),
            utf8("valor_salida_nombre"),
            utf8("instrucciones_uso_sql"),
            vector_nulable("vector_tabla"),
        ],
    )
}

fn limites_eje(eje: &str) -> Vec<Field> {
    vec![
        utf8_nulable(&format!("eje_{eje}_categoria")),
        Field::new(format!("eje_{eje}_min"), DataType::Float64, true),
        Field::new(format!("eje_{eje}_min_inclusivo"), DataType::Boolean, false),
        Field::new(format!("eje_{eje}_max"), DataType::Float64, true),
        Field::new(format!("eje_{eje}_max_inclusivo"), DataType::Boolean, false),
    ]
}

/// `05_celdas_doble_entrada`: celdas desnormalizadas para el cruce exacto
pub fn schema_celdas_doble_entrada() -> SchemaRef {
    let mut especificas = vec![utf8("tabla_id")];
    especificas.extend(limites_eje("x"));
    especificas.extend(limites_eje("y"));
    especificas.extend([
        utf8_nulable("eje_z_filtro_adicional"),
        utf8("valor_resultado_texto"),
        Field::new("valor_resultado_num", DataType::Float64, true),
        utf8_nulable("unidad_medida"),
        utf8_nulable("nota_pie_condicion"),
        nivel_prelacion(),
    ]);
    esquema_con_comunes(Some("celda_id"), especificas)
}

/// `06_grafo_referencias`: aristas del GraphRAG columnar
pub fn schema_grafo_referencias() -> SchemaRef {
    esquema_con_comunes(
        Some("edge_id"),
        vec![
            utf8("origen_id"),
            utf8("origen_tipo"),
            utf8("tipo_relacion"),
            utf8("destino_id"),
            utf8("destino_tipo"),
            utf8_nulable("contexto_relacion"),
        ],
    )
}

/// Resuelve el esquema correspondiente según el nombre de la tabla
pub fn obtener_esquema_por_nombre(nombre_tabla: &str) -> Option<SchemaRef> {
    match nombre_tabla {
        TABLA_DOCUMENTOS_CATALOGO => Some(schema_documentos_catalogo()),
        TABLA_CHUNKS_NORMATIVA => Some(schema_chunks_normativa()),
        TABLA_MATRIZ_OPERATIVA => Some(schema_matriz_operativa()),
        TABLA_CATALOGO_TABLAS => Some(schema_catalogo_tablas()),
        TABLA_CELDAS_DOBLE_ENTRADA => Some(schema_celdas_doble_entrada()),
        TABLA_GRAFO_REFERENCIAS => Some(schema_grafo_referencias()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::lakehouse_store::{
        TABLA_CATALOGO_TABLAS, TABLA_CELDAS_DOBLE_ENTRADA, TABLA_CHUNKS_NORMATIVA,
        TABLA_DOCUMENTOS_CATALOGO, TABLA_GRAFO_REFERENCIAS, TABLA_MATRIZ_OPERATIVA,
        TODAS_LAS_TABLAS,
    };

    fn tipo(esquema: &SchemaRef, campo: &str) -> DataType {
        esquema
            .field_with_name(campo)
            .unwrap_or_else(|_| panic!("falta la columna {campo}"))
            .data_type()
            .clone()
    }

    fn es_vector(tipo_dato: &DataType) -> bool {
        matches!(tipo_dato, DataType::FixedSizeList(item, dim)
            if *dim == DIMENSION_EMBEDDINGS && item.data_type() == &DataType::Float32)
    }

    #[test]
    fn test_version_y_nombres_de_tabla_de_la_especificacion() {
        assert_eq!(VERSION_ESQUEMA, 2);
        assert_eq!(
            TODAS_LAS_TABLAS,
            [
                "01_documentos_catalogo",
                "02_chunks_normativa",
                "03_matriz_operativa",
                "04_catalogo_tablas",
                "05_celdas_doble_entrada",
                "06_grafo_referencias",
            ]
        );
        assert!(obtener_esquema_por_nombre("01_doc_catalog").is_none());
    }

    #[test]
    fn test_las_6_tablas_tienen_las_columnas_comunes_tipadas() {
        for nombre in TODAS_LAS_TABLAS {
            let esquema = obtener_esquema_por_nombre(nombre).expect("esquema");
            assert_eq!(tipo(&esquema, "centro_id"), DataType::Utf8, "{nombre}");
            assert_eq!(tipo(&esquema, "doc_id"), DataType::Utf8, "{nombre}");
            assert_eq!(tipo(&esquema, "version_num"), DataType::Int32, "{nombre}");
            assert_eq!(tipo(&esquema, "vigente"), DataType::Boolean, "{nombre}");
            assert_eq!(tipo(&esquema, "ingest_id"), DataType::Utf8, "{nombre}");
            assert_eq!(tipo(&esquema, "estado_ingesta"), DataType::Utf8, "{nombre}");
        }
    }

    #[test]
    fn test_catalogo_01_controla_revision_vigor_y_derogacion() {
        let esquema = obtener_esquema_por_nombre(TABLA_DOCUMENTOS_CATALOGO).unwrap();
        assert_eq!(tipo(&esquema, "tipo_documento"), DataType::Utf8);
        assert_eq!(tipo(&esquema, "nivel_prelacion"), DataType::Int8);
        assert_eq!(tipo(&esquema, "revision"), DataType::Utf8);
        assert_eq!(tipo(&esquema, "fecha_vigor"), DataType::Date32);
        assert_eq!(tipo(&esquema, "fecha_derogacion"), DataType::Date32);
        assert!(
            esquema
                .field_with_name("fecha_derogacion")
                .unwrap()
                .is_nullable()
        );
        assert!(
            !esquema
                .field_with_name("fecha_vigor")
                .unwrap()
                .is_nullable()
        );
        assert_eq!(tipo(&esquema, "hash_sha256"), DataType::Utf8);
    }

    #[test]
    fn test_chunks_02_y_matriz_03_siguen_la_especificacion() {
        let chunks = obtener_esquema_por_nombre(TABLA_CHUNKS_NORMATIVA).unwrap();
        for campo in ["chunk_id", "breadcrumb_jerarquia", "texto_contenido"] {
            assert_eq!(tipo(&chunks, campo), DataType::Utf8, "{campo}");
        }
        assert!(
            chunks
                .field_with_name("condicion_aplicacion")
                .unwrap()
                .is_nullable()
        );
        assert_eq!(tipo(&chunks, "nivel_prelacion"), DataType::Int8);
        for campo in ["tablas_vinculadas", "hitos_vinculados"] {
            assert!(
                matches!(tipo(&chunks, campo), DataType::List(item) if item.data_type() == &DataType::Utf8),
                "{campo} debe ser List<Utf8>"
            );
        }
        assert!(es_vector(&tipo(&chunks, "vector")));

        let matriz = obtener_esquema_por_nombre(TABLA_MATRIZ_OPERATIVA).unwrap();
        for campo in [
            "hito_id",
            "fase_operativa",
            "objeto_alcance",
            "rol_ejecutor",
            "rol_responsable",
            "registro_codigo",
            "registro_nombre",
            "criterio_aceptacion",
            "chunk_origen_id",
            "condicion_aplicacion",
            "calidad_extraccion",
        ] {
            assert_eq!(tipo(&matriz, campo), DataType::Utf8, "{campo}");
        }
        assert_eq!(tipo(&matriz, "orden_secuencia"), DataType::Int32);
        assert_eq!(tipo(&matriz, "nivel_prelacion"), DataType::Int8);
        assert!(es_vector(&tipo(&matriz, "vector_hito")));
        assert!(matriz.field_with_name("vector_hito").unwrap().is_nullable());
    }

    #[test]
    fn test_tablas_04_05_y_grafo_06_siguen_la_especificacion() {
        let tablas = obtener_esquema_por_nombre(TABLA_CATALOGO_TABLAS).unwrap();
        for campo in [
            "tabla_id",
            "titulo_tabla",
            "eje_x_nombre",
            "eje_x_tipo",
            "eje_x_unidad",
            "eje_y_nombre",
            "eje_y_tipo",
            "eje_y_unidad",
            "valor_salida_nombre",
            "instrucciones_uso_sql",
        ] {
            assert_eq!(tipo(&tablas, campo), DataType::Utf8, "{campo}");
        }
        assert!(es_vector(&tipo(&tablas, "vector_tabla")));

        let celdas = obtener_esquema_por_nombre(TABLA_CELDAS_DOBLE_ENTRADA).unwrap();
        for eje in ["x", "y"] {
            assert_eq!(
                tipo(&celdas, &format!("eje_{eje}_categoria")),
                DataType::Utf8
            );
            assert_eq!(tipo(&celdas, &format!("eje_{eje}_min")), DataType::Float64);
            assert_eq!(tipo(&celdas, &format!("eje_{eje}_max")), DataType::Float64);
            assert_eq!(
                tipo(&celdas, &format!("eje_{eje}_min_inclusivo")),
                DataType::Boolean
            );
            assert_eq!(
                tipo(&celdas, &format!("eje_{eje}_max_inclusivo")),
                DataType::Boolean
            );
        }
        for campo in [
            "celda_id",
            "tabla_id",
            "eje_z_filtro_adicional",
            "valor_resultado_texto",
            "nota_pie_condicion",
        ] {
            assert_eq!(tipo(&celdas, campo), DataType::Utf8, "{campo}");
        }
        assert_eq!(tipo(&celdas, "valor_resultado_num"), DataType::Float64);
        assert_eq!(tipo(&celdas, "nivel_prelacion"), DataType::Int8);

        let grafo = obtener_esquema_por_nombre(TABLA_GRAFO_REFERENCIAS).unwrap();
        for campo in [
            "edge_id",
            "origen_id",
            "origen_tipo",
            "tipo_relacion",
            "destino_id",
            "destino_tipo",
            "contexto_relacion",
        ] {
            assert_eq!(tipo(&grafo, campo), DataType::Utf8, "{campo}");
        }
    }

    #[test]
    fn test_nombre_tabla_desconocido_retorna_none() {
        assert!(obtener_esquema_por_nombre("tabla_inexistente").is_none());
    }
}
