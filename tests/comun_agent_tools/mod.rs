//! Fixtures en memoria de las suites heredadas de tools/agente adaptadas al contrato v2
//! (Fase 13): lotes Arrow con los esquemas `schema_*` registrados como vistas DataFusion de un
//! centro, más el índice semántico en memoria y embeddings mock. Sin disco ni LanceDB.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::builder::{ListBuilder, StringBuilder};
use arrow_array::{
    ArrayRef, BooleanArray, Date32Array, Float64Array, Int8Array, Int32Array, RecordBatch,
    StringArray, new_null_array,
};
use arrow_schema::{DataType, Field, SchemaRef};
use chrono::NaiveDate;

use lakehouse_rag::application::ports::analytical_engine::AnalyticalEnginePort;
use lakehouse_rag::application::ports::semantic_index::{
    FragmentoIndexado, IndiceSemanticoPort, OrigenFragmento,
};
use lakehouse_rag::application::use_cases::lakehouse_tool_service::LakehouseToolService;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use lakehouse_rag::infrastructure::query_engine::indice_semantico_memoria::IndiceSemanticoMemoria;
use lakehouse_rag::infrastructure::storage::schemas::{
    schema_catalogo_tablas, schema_celdas_doble_entrada, schema_chunks_normativa,
    schema_documentos_catalogo, schema_grafo_referencias, schema_matriz_operativa,
};

/// Nota al pie que acompaña a la celda «Electrodo E-308 (1)»
pub const NOTA_PRECALENTAR: &str = "(1) Nota técnica: precalentar a 100 °C";
/// Fecha en la que solo regían DOC-001 Rev. 01 y DOC-002
pub const FECHA_ANTERIOR: &str = "2025-01-01";

/// Motor, índice y servicio de tools compartiendo las mismas vistas
pub struct EntornoTools {
    pub motor: Arc<DataFusionEngine>,
    pub indice: Arc<IndiceSemanticoMemoria>,
    pub servicio: Arc<LakehouseToolService>,
}

pub fn entorno_vacio() -> EntornoTools {
    let motor = Arc::new(DataFusionEngine::new());
    let indice = Arc::new(IndiceSemanticoMemoria::new());
    let servicio = Arc::new(LakehouseToolService::new(
        motor.clone(),
        indice.clone(),
        Arc::new(MockEmbeddingEngine::new(Some(1024))),
    ));
    EntornoTools {
        motor,
        indice,
        servicio,
    }
}

/// Días desde 1970-01-01 (representación de `Date32`)
pub fn dias(fecha: &str) -> i32 {
    let fecha = NaiveDate::parse_from_str(fecha, "%Y-%m-%d").unwrap();
    let epoca = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    i32::try_from((fecha - epoca).num_days()).unwrap()
}

pub fn textos(valores: &[&str]) -> ArrayRef {
    Arc::new(StringArray::from(valores.to_vec()))
}

pub fn textos_nulables(valores: &[Option<&str>]) -> ArrayRef {
    Arc::new(StringArray::from(valores.to_vec()))
}

pub fn enteros32(valores: &[i32]) -> ArrayRef {
    Arc::new(Int32Array::from(valores.to_vec()))
}

pub fn niveles(valores: &[i8]) -> ArrayRef {
    Arc::new(Int8Array::from(valores.to_vec()))
}

pub fn booleanos(valores: &[bool]) -> ArrayRef {
    Arc::new(BooleanArray::from(valores.to_vec()))
}

pub fn decimales(valores: &[Option<f64>]) -> ArrayRef {
    Arc::new(Float64Array::from(valores.to_vec()))
}

pub fn fechas(valores: &[Option<&str>]) -> ArrayRef {
    Arc::new(Date32Array::from(
        valores.iter().map(|f| f.map(dias)).collect::<Vec<_>>(),
    ))
}

/// Valor neutro de una columna no nula que el test no fija
fn columna_por_defecto(campo: &Field, filas: usize) -> ArrayRef {
    if campo.is_nullable() {
        return new_null_array(campo.data_type(), filas);
    }
    match campo.data_type() {
        DataType::Utf8 => Arc::new(StringArray::from(vec![""; filas])),
        DataType::Int8 => Arc::new(Int8Array::from(vec![0; filas])),
        DataType::Int32 => Arc::new(Int32Array::from(vec![0; filas])),
        DataType::Boolean => Arc::new(BooleanArray::from(vec![false; filas])),
        DataType::Date32 => Arc::new(Date32Array::from(vec![0; filas])),
        DataType::List(_) => {
            let mut listas = ListBuilder::new(StringBuilder::new());
            for _ in 0..filas {
                listas.append(true);
            }
            Arc::new(listas.finish())
        }
        otro => panic!("Tipo sin valor por defecto en la fixture: {otro:?}"),
    }
}

/// Lote con el esquema v2 completo: las columnas indicadas y valores neutros en el resto
pub fn lote_v2(esquema: SchemaRef, filas: usize, columnas: Vec<(&str, ArrayRef)>) -> RecordBatch {
    let mut indicadas: HashMap<&str, ArrayRef> = columnas.into_iter().collect();
    let arrays: Vec<ArrayRef> = esquema
        .fields()
        .iter()
        .map(|campo| {
            indicadas
                .remove(campo.name().as_str())
                .unwrap_or_else(|| columna_por_defecto(campo, filas))
        })
        .collect();
    assert!(
        indicadas.is_empty(),
        "Columnas fuera del esquema: {:?}",
        indicadas.keys().collect::<Vec<_>>()
    );
    RecordBatch::try_new(esquema, arrays).unwrap()
}

/// `01_documentos_catalogo`: DOC-001 Rev. 02 (vigente) y Rev. 01 (derogada), DOC-002
/// (derogado) y DOC-003 (vigente desde 2026-04-01)
pub fn lote_catalogo() -> RecordBatch {
    lote_v2(
        schema_documentos_catalogo(),
        4,
        vec![
            (
                "doc_id",
                textos(&["DOC-001", "DOC-001", "DOC-002", "DOC-003"]),
            ),
            ("version_num", enteros32(&[2, 1, 1, 1])),
            ("vigente", booleanos(&[true, false, false, true])),
            (
                "titulo",
                textos(&[
                    "Especificación Acero",
                    "Especificación Acero",
                    "Procedimiento Soldadura Obsoleto",
                    "Norma de Soldadura 2026",
                ]),
            ),
            (
                "tipo_documento",
                textos(&[
                    "ESPECIFICACION_CLIENTE",
                    "ESPECIFICACION_CLIENTE",
                    "PROCEDIMIENTO_GENERAL",
                    "NORMA_INTERNACIONAL",
                ]),
            ),
            ("nivel_prelacion", niveles(&[1, 1, 3, 5])),
            (
                "revision",
                textos(&["Rev. 02", "Rev. 01", "Rev. 01", "Rev. 01"]),
            ),
            (
                "fecha_vigor",
                fechas(&[
                    Some("2026-03-01"),
                    Some("2024-01-01"),
                    Some("2024-01-01"),
                    Some("2026-04-01"),
                ]),
            ),
            (
                "fecha_derogacion",
                fechas(&[None, Some("2026-03-01"), Some("2026-04-01"), None]),
            ),
        ],
    )
}

/// `03_matriz_operativa`: dos hitos de la Rev. 02 y uno de la Rev. 01 (time travel)
pub fn lote_matriz() -> RecordBatch {
    lote_v2(
        schema_matriz_operativa(),
        3,
        vec![
            (
                "hito_id",
                textos(&[
                    "DOC-001@v2-HIT-0001",
                    "DOC-001@v2-HIT-0002",
                    "DOC-001@v1-HIT-0001",
                ]),
            ),
            ("doc_id", textos(&["DOC-001", "DOC-001", "DOC-001"])),
            ("version_num", enteros32(&[2, 2, 1])),
            ("vigente", booleanos(&[true, true, false])),
            ("orden_secuencia", enteros32(&[1, 2, 1])),
            (
                "fase_operativa",
                textos(&["RECEPCION", "INSPECCION", "RECEPCION"]),
            ),
            (
                "objeto_alcance",
                textos(&[
                    "Temperatura Almacén",
                    "Presión Hidráulica",
                    "Temperatura Almacén",
                ]),
            ),
            (
                "rol_ejecutor",
                textos(&["Almacenero", "Inspector QA/QC", "Almacenero"]),
            ),
            (
                "rol_responsable",
                textos(&["Jefe de Almacén", "Jefe de Calidad", "Jefe de Obra"]),
            ),
            (
                "registro_codigo",
                textos_nulables(&[Some("FOR-ALM-01"), Some("PPI-MAT-09"), Some("FOR-ALM-00")]),
            ),
            (
                "registro_nombre",
                textos_nulables(&[Some("Acta"), Some("PPI"), Some("Acta antigua")]),
            ),
            (
                "criterio_aceptacion",
                textos(&["15-25 °C", "140-160 bar", "10-30 °C"]),
            ),
            (
                "condicion_aplicacion",
                textos_nulables(&[Some("Ambiente controlado"), Some("Carga máxima"), None]),
            ),
            ("nivel_prelacion", niveles(&[1, 1, 1])),
            ("calidad_extraccion", textos(&["OK", "OK", "OK"])),
        ],
    )
}

/// `04_catalogo_tablas`: Tabla 4-A de DOC-001 Rev. 02 (espesor × material → electrodo)
pub fn lote_catalogo_tablas() -> RecordBatch {
    lote_v2(
        schema_catalogo_tablas(),
        1,
        vec![
            ("tabla_id", textos(&["DOC-001@v2-TAB-0000"])),
            ("doc_id", textos(&["DOC-001"])),
            ("version_num", enteros32(&[2])),
            ("vigente", booleanos(&[true])),
            (
                "titulo_tabla",
                textos(&["Tabla 4-A Electrodo de soldadura"]),
            ),
            ("eje_x_nombre", textos(&["espesor_mm"])),
            ("eje_x_tipo", textos(&["RANGO_NUMERICO"])),
            ("eje_x_unidad", textos_nulables(&[Some("mm")])),
            ("eje_y_nombre", textos(&["material"])),
            ("eje_y_tipo", textos(&["CATEGORIA"])),
            ("valor_salida_nombre", textos(&["electrodo"])),
            (
                "instrucciones_uso_sql",
                textos(&["Usa lookup_double_entry_table con tabla_id"]),
            ),
        ],
    )
}

/// `05_celdas_doble_entrada`: [10, 20) y [20, 40] mm para «Acero Inox»
pub fn lote_celdas() -> RecordBatch {
    lote_v2(
        schema_celdas_doble_entrada(),
        2,
        vec![
            (
                "celda_id",
                textos(&["DOC-001@v2-TAB-0000-C01", "DOC-001@v2-TAB-0000-C02"]),
            ),
            ("doc_id", textos(&["DOC-001", "DOC-001"])),
            ("version_num", enteros32(&[2, 2])),
            ("vigente", booleanos(&[true, true])),
            (
                "tabla_id",
                textos(&["DOC-001@v2-TAB-0000", "DOC-001@v2-TAB-0000"]),
            ),
            ("eje_x_min", decimales(&[Some(10.0), Some(20.0)])),
            ("eje_x_min_inclusivo", booleanos(&[true, true])),
            ("eje_x_max", decimales(&[Some(20.0), Some(40.0)])),
            ("eje_x_max_inclusivo", booleanos(&[false, true])),
            (
                "eje_y_categoria",
                textos_nulables(&[Some("Acero Inox"), Some("Acero Inox")]),
            ),
            (
                "valor_resultado_texto",
                textos(&["Electrodo E-308 (1)", "Electrodo E-309"]),
            ),
            (
                "nota_pie_condicion",
                textos_nulables(&[Some(NOTA_PRECALENTAR), None]),
            ),
            ("nivel_prelacion", niveles(&[1, 1])),
        ],
    )
}

/// `06_grafo_referencias`: DOC-003 deroga a DOC-002; DOC-001 se subordina a DOC-003 y exige
/// FOR-ALM-01
pub fn lote_grafo() -> RecordBatch {
    lote_v2(
        schema_grafo_referencias(),
        3,
        vec![
            ("edge_id", textos(&["E-001", "E-002", "E-003"])),
            ("doc_id", textos(&["DOC-003", "DOC-001", "DOC-001"])),
            ("version_num", enteros32(&[1, 2, 2])),
            ("vigente", booleanos(&[true, true, true])),
            ("origen_id", textos(&["DOC-003", "DOC-001", "DOC-001"])),
            (
                "origen_tipo",
                textos(&["DOCUMENTO", "DOCUMENTO", "DOCUMENTO"]),
            ),
            (
                "tipo_relacion",
                textos(&["DEROGA_A", "SUBORDINADO_A_NORMA", "EXIGE_LLENAR_REGISTRO"]),
            ),
            ("destino_id", textos(&["DOC-002", "DOC-003", "FOR-ALM-01"])),
            (
                "destino_tipo",
                textos(&["DOCUMENTO", "DOCUMENTO", "REGISTRO"]),
            ),
            (
                "contexto_relacion",
                textos_nulables(&[Some("Deroga el procedimiento previo"), None, None]),
            ),
        ],
    )
}

fn fragmento(id: &str, version: i32, texto: &str) -> FragmentoIndexado {
    FragmentoIndexado {
        id: id.to_string(),
        origen: OrigenFragmento::Chunk,
        doc_id: "DOC-001".to_string(),
        version_num: version,
        nivel_prelacion: 1,
        breadcrumb_jerarquia: "Especificación Acero > 3. Soldadura".to_string(),
        condicion_aplicacion: Some("Cuando la temperatura ambiente es < 0 °C".to_string()),
        texto: texto.to_string(),
        chunk_origen_id: None,
        vector: Vec::new(),
    }
}

/// Registra las 6 vistas del centro con las fixtures y alimenta el índice semántico
pub async fn sembrar_centro(entorno: &EntornoTools, centro: &CenterId) {
    let vistas = [
        ("01_documentos_catalogo", lote_catalogo()),
        (
            "02_chunks_normativa",
            RecordBatch::new_empty(schema_chunks_normativa()),
        ),
        ("03_matriz_operativa", lote_matriz()),
        ("04_catalogo_tablas", lote_catalogo_tablas()),
        ("05_celdas_doble_entrada", lote_celdas()),
        ("06_grafo_referencias", lote_grafo()),
    ];
    for (tabla, lote) in vistas {
        entorno
            .motor
            .registrar_centro(centro, tabla, vec![lote])
            .await
            .unwrap();
    }
    entorno
        .indice
        .reemplazar_centro(
            centro,
            vec![
                fragmento(
                    "DOC-001@v2-CH-0001",
                    2,
                    "Precalentar la zona de soldadura a 100 °C antes de soldar.",
                ),
                fragmento(
                    "DOC-001@v1-CH-0001",
                    1,
                    "Precalentar la zona de soldadura a 50 °C antes de soldar.",
                ),
            ],
        )
        .await;
}

/// Registra las 6 vistas del centro vacías (centro activo sin documentos)
pub async fn registrar_centro_vacio(entorno: &EntornoTools, centro: &CenterId) {
    let esquemas = [
        ("01_documentos_catalogo", schema_documentos_catalogo()),
        ("02_chunks_normativa", schema_chunks_normativa()),
        ("03_matriz_operativa", schema_matriz_operativa()),
        ("04_catalogo_tablas", schema_catalogo_tablas()),
        ("05_celdas_doble_entrada", schema_celdas_doble_entrada()),
        ("06_grafo_referencias", schema_grafo_referencias()),
    ];
    for (tabla, esquema) in esquemas {
        entorno
            .motor
            .registrar_centro(centro, tabla, vec![RecordBatch::new_empty(esquema)])
            .await
            .unwrap();
    }
}
