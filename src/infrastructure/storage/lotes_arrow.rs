//! Conversión de las filas tipadas v2 (`LoteIngesta`) a `RecordBatch` de Apache Arrow

use std::sync::Arc;

use arrow::buffer::{NullBuffer, OffsetBuffer};
use arrow_array::builder::{Int32Builder, ListBuilder, StringBuilder};
use arrow_array::{
    ArrayRef, BooleanArray, Date32Array, FixedSizeListArray, Float32Array, Float64Array, Int8Array,
    Int32Array, ListArray, RecordBatch, StringArray, StructArray,
};
use arrow_schema::{DataType, Field, SchemaRef};
use chrono::NaiveDate;

use crate::application::dtos::lakehouse_filas::{
    ClaveDocumental, FilaArista, FilaCatalogo, FilaCelda, FilaChunk, FilaHito, FilaIndice,
    FilaTabla, LimitesEje, LoteIngesta,
};
use crate::application::ports::lakehouse_store::EstadoIngesta;
use crate::domain::errors::DomainError;
use crate::domain::models::identificadores::IngestId;
use crate::domain::services::marcas_pagina::SaltoPagina;
use crate::infrastructure::storage::schemas::{
    DIMENSION_EMBEDDINGS, campo_item_salto, campos_salto_pagina, schema_catalogo_tablas,
    schema_celdas_doble_entrada, schema_chunks_normativa, schema_documentos_catalogo,
    schema_grafo_referencias, schema_indice_jerarquico, schema_matriz_operativa,
};

/// Valores de las columnas transaccionales que fija el almacén (nunca el llamador)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnasTransaccionales {
    pub ingest_id: IngestId,
    pub estado: EstadoIngesta,
    pub vigente: bool,
}

impl ColumnasTransaccionales {
    /// Filas recién escritas: `STAGED` y no vigentes
    pub fn staged(ingest_id: &IngestId) -> Self {
        Self {
            ingest_id: ingest_id.clone(),
            estado: EstadoIngesta::Staged,
            vigente: false,
        }
    }
}

/// Días desde 1970-01-01 (representación de `Date32`)
pub fn fecha_a_date32(fecha: NaiveDate) -> i32 {
    let epoca = NaiveDate::from_ymd_opt(1970, 1, 1).expect("fecha de época válida");
    (fecha - epoca).num_days() as i32
}

/// Convierte un lote tipado en el `RecordBatch` de su tabla
pub fn lote_a_record_batch(
    lote: &LoteIngesta,
    transaccion: &ColumnasTransaccionales,
) -> Result<RecordBatch, DomainError> {
    match lote {
        LoteIngesta::Catalogo(filas) => lote_catalogo(filas, transaccion),
        LoteIngesta::Chunks(filas) => lote_chunks(filas, transaccion),
        LoteIngesta::Hitos(filas) => lote_hitos(filas, transaccion),
        LoteIngesta::Tablas(filas) => lote_tablas(filas, transaccion),
        LoteIngesta::Celdas(filas) => lote_celdas(filas, transaccion),
        LoteIngesta::Aristas(filas) => lote_aristas(filas, transaccion),
        LoteIngesta::Indice(filas) => lote_indice(filas, transaccion),
    }
}

fn texto<'a>(valores: impl Iterator<Item = &'a str>) -> ArrayRef {
    Arc::new(StringArray::from_iter_values(valores))
}

fn texto_nulable<'a>(valores: impl Iterator<Item = Option<&'a str>>) -> ArrayRef {
    Arc::new(valores.collect::<StringArray>())
}

/// Páginas de citas; un valor fuera de rango de `Int32` queda nulo en lugar de corromperse
fn paginas(valores: impl Iterator<Item = Option<u32>>) -> ArrayRef {
    Arc::new(
        valores
            .map(|pagina| pagina.and_then(|p| i32::try_from(p).ok()))
            .collect::<Int32Array>(),
    )
}

/// Saltos de página de cada chunk como `List<Struct<desplazamiento, pagina>>`. Una fila sin
/// saltos, o con un valor fuera de `Int32`, queda nula en lugar de corromperse
fn saltos_de_pagina<'a>(valores: impl Iterator<Item = Option<&'a [SaltoPagina]>>) -> ArrayRef {
    let mut desplazamientos: Vec<i32> = Vec::new();
    let mut paginas: Vec<i32> = Vec::new();
    let mut limites: Vec<i32> = vec![0];
    let mut validas: Vec<bool> = Vec::new();
    for saltos in valores {
        let convertidos: Option<Vec<(i32, i32)>> = saltos.and_then(|lista| {
            lista
                .iter()
                .map(|salto| {
                    Some((
                        i32::try_from(salto.desplazamiento).ok()?,
                        i32::try_from(salto.pagina).ok()?,
                    ))
                })
                .collect()
        });
        validas.push(convertidos.is_some());
        for (desplazamiento, pagina) in convertidos.unwrap_or_default() {
            desplazamientos.push(desplazamiento);
            paginas.push(pagina);
        }
        limites.push(desplazamientos.len() as i32);
    }
    let estructuras = StructArray::new(
        campos_salto_pagina(),
        vec![
            Arc::new(Int32Array::from(desplazamientos)),
            Arc::new(Int32Array::from(paginas)),
        ],
        None,
    );
    Arc::new(ListArray::new(
        campo_item_salto(),
        OffsetBuffer::new(limites.into()),
        Arc::new(estructuras),
        Some(NullBuffer::from(validas)),
    ))
}

fn flotante_nulable(valores: impl Iterator<Item = Option<f64>>) -> ArrayRef {
    Arc::new(valores.collect::<Float64Array>())
}

fn booleano(valores: impl Iterator<Item = bool>) -> ArrayRef {
    Arc::new(valores.map(Some).collect::<BooleanArray>())
}

fn nivel(valores: impl Iterator<Item = i8>) -> ArrayRef {
    Arc::new(Int8Array::from_iter_values(valores))
}

/// Las 6 columnas comunes, en el orden de `schemas::columnas_comunes`
fn columnas_comunes(claves: &[&ClaveDocumental], tx: &ColumnasTransaccionales) -> Vec<ArrayRef> {
    let filas = claves.len();
    vec![
        texto(claves.iter().map(|c| c.centro_id.as_str())),
        texto(claves.iter().map(|c| c.doc_id.as_str())),
        Arc::new(Int32Array::from_iter_values(
            claves.iter().map(|c| c.version_num as i32),
        )),
        booleano(std::iter::repeat_n(tx.vigente, filas)),
        texto(std::iter::repeat_n(tx.ingest_id.as_str(), filas)),
        texto(std::iter::repeat_n(tx.estado.as_str(), filas)),
    ]
}

fn vectores(valores: &[Option<&[f32]>]) -> Result<ArrayRef, DomainError> {
    let dimension = DIMENSION_EMBEDDINGS as usize;
    let mut planos = Vec::with_capacity(valores.len() * dimension);
    for vector in valores {
        match vector {
            Some(v) if v.len() == dimension => planos.extend_from_slice(v),
            Some(v) => {
                return Err(DomainError::StorageError(format!(
                    "Dimensión de embedding incompatible: esperada {dimension}, obtenida {}",
                    v.len()
                )));
            }
            None => planos.extend(std::iter::repeat_n(0.0, dimension)),
        }
    }
    let nulos = NullBuffer::from(valores.iter().map(Option::is_some).collect::<Vec<bool>>());
    let item = Arc::new(Field::new("item", DataType::Float32, true));
    let lista = FixedSizeListArray::try_new(
        item,
        DIMENSION_EMBEDDINGS,
        Arc::new(Float32Array::from(planos)),
        Some(nulos),
    )
    .map_err(|e| DomainError::StorageError(format!("Error construyendo columna vectorial: {e}")))?;
    Ok(Arc::new(lista))
}

/// Una lista vacía se guarda como nula: «sin páginas degradadas»
fn listas_enteras_nulables<'a>(valores: impl Iterator<Item = &'a [u32]>) -> ArrayRef {
    let mut constructor = ListBuilder::new(Int32Builder::new());
    for lista in valores {
        for elemento in lista {
            constructor
                .values()
                .append_value(i32::try_from(*elemento).unwrap_or(i32::MAX));
        }
        constructor.append(!lista.is_empty());
    }
    Arc::new(constructor.finish())
}

fn listas_texto<'a>(valores: impl Iterator<Item = &'a [String]>) -> ArrayRef {
    let mut constructor = ListBuilder::new(StringBuilder::new());
    for lista in valores {
        for elemento in lista {
            constructor.values().append_value(elemento);
        }
        constructor.append(true);
    }
    Arc::new(constructor.finish())
}

fn ensamblar(
    esquema: SchemaRef,
    pk: Option<ArrayRef>,
    comunes: Vec<ArrayRef>,
    especificas: Vec<ArrayRef>,
    tabla: &str,
) -> Result<RecordBatch, DomainError> {
    let columnas: Vec<ArrayRef> = pk.into_iter().chain(comunes).chain(especificas).collect();
    RecordBatch::try_new(esquema, columnas).map_err(|e| {
        DomainError::StorageError(format!("Error al armar el RecordBatch de {tabla}: {e}"))
    })
}

fn lote_catalogo(
    filas: &[FilaCatalogo],
    tx: &ColumnasTransaccionales,
) -> Result<RecordBatch, DomainError> {
    let claves: Vec<&ClaveDocumental> = filas.iter().map(|f| &f.clave).collect();
    let especificas = vec![
        texto(filas.iter().map(|f| f.titulo.as_str())),
        texto(filas.iter().map(|f| f.tipo_documento.as_str())),
        nivel(
            filas
                .iter()
                .map(|f| f.tipo_documento.nivel_prelacion().valor()),
        ),
        texto(filas.iter().map(|f| f.revision.as_str())),
        Arc::new(
            filas
                .iter()
                .map(|f| f.vigencia.fecha_vigor().map(fecha_a_date32))
                .collect::<Date32Array>(),
        ),
        Arc::new(
            filas
                .iter()
                .map(|f| f.vigencia.fecha_derogacion().map(fecha_a_date32))
                .collect::<Date32Array>(),
        ),
        texto(filas.iter().map(|f| f.hash_sha256.as_str())),
        texto_nulable(
            filas
                .iter()
                .map(|f| f.doc_id_provisional.as_ref().map(|d| d.as_str())),
        ),
        listas_enteras_nulables(filas.iter().map(|f| f.paginas_degradadas.as_slice())),
    ];
    ensamblar(
        schema_documentos_catalogo(),
        None,
        columnas_comunes(&claves, tx),
        especificas,
        "01",
    )
}

fn lote_chunks(
    filas: &[FilaChunk],
    tx: &ColumnasTransaccionales,
) -> Result<RecordBatch, DomainError> {
    let claves: Vec<&ClaveDocumental> = filas.iter().map(|f| &f.clave).collect();
    let vectores_chunk: Vec<Option<&[f32]>> =
        filas.iter().map(|f| Some(f.vector.as_slice())).collect();
    let especificas = vec![
        texto(filas.iter().map(|f| f.nodo_id.as_str())),
        texto(filas.iter().map(|f| f.tipo_bloque.as_str())),
        texto(filas.iter().map(|f| f.estado_contenido.as_str())),
        nivel(filas.iter().map(|f| f.nivel_prelacion.valor())),
        texto(filas.iter().map(|f| f.breadcrumb_jerarquia.as_str())),
        texto_nulable(filas.iter().map(|f| f.condicion_aplicacion.as_deref())),
        texto(filas.iter().map(|f| f.texto_contenido.as_str())),
        paginas(filas.iter().map(|f| f.pagina_inicio)),
        paginas(filas.iter().map(|f| f.pagina_fin)),
        saltos_de_pagina(filas.iter().map(|f| f.saltos_pagina.as_deref())),
        listas_texto(filas.iter().map(|f| f.tablas_vinculadas.as_slice())),
        listas_texto(filas.iter().map(|f| f.hitos_vinculados.as_slice())),
        vectores(&vectores_chunk)?,
    ];
    ensamblar(
        schema_chunks_normativa(),
        Some(texto(filas.iter().map(|f| f.chunk_id.as_str()))),
        columnas_comunes(&claves, tx),
        especificas,
        "02",
    )
}

fn lote_hitos(
    filas: &[FilaHito],
    tx: &ColumnasTransaccionales,
) -> Result<RecordBatch, DomainError> {
    let claves: Vec<&ClaveDocumental> = filas.iter().map(|f| &f.clave).collect();
    let vectores_hito: Vec<Option<&[f32]>> =
        filas.iter().map(|f| f.vector_hito.as_deref()).collect();
    let especificas = vec![
        Arc::new(Int32Array::from_iter_values(
            filas.iter().map(|f| f.orden_secuencia as i32),
        )) as ArrayRef,
        texto(filas.iter().map(|f| f.fase_operativa.as_str())),
        texto(filas.iter().map(|f| f.objeto_alcance.as_str())),
        texto(filas.iter().map(|f| f.rol_ejecutor.as_str())),
        texto(filas.iter().map(|f| f.rol_responsable.as_str())),
        texto(filas.iter().map(|f| f.origen_responsable.as_str())),
        texto_nulable(filas.iter().map(|f| f.registro_codigo.as_deref())),
        texto_nulable(filas.iter().map(|f| f.registro_nombre.as_deref())),
        texto(filas.iter().map(|f| f.criterio_aceptacion.as_str())),
        texto_nulable(filas.iter().map(|f| f.condicion_aplicacion.as_deref())),
        nivel(filas.iter().map(|f| f.nivel_prelacion.valor())),
        texto(filas.iter().map(|f| f.calidad_extraccion.as_str())),
        texto_nulable(filas.iter().map(|f| f.chunk_origen_id.as_deref())),
        vectores(&vectores_hito)?,
    ];
    ensamblar(
        schema_matriz_operativa(),
        Some(texto(filas.iter().map(|f| f.hito_id.as_str()))),
        columnas_comunes(&claves, tx),
        especificas,
        "03",
    )
}

fn lote_tablas(
    filas: &[FilaTabla],
    tx: &ColumnasTransaccionales,
) -> Result<RecordBatch, DomainError> {
    let claves: Vec<&ClaveDocumental> = filas.iter().map(|f| &f.clave).collect();
    let vectores_tabla: Vec<Option<&[f32]>> =
        filas.iter().map(|f| f.vector_tabla.as_deref()).collect();
    let especificas = vec![
        texto(filas.iter().map(|f| f.titulo_tabla.as_str())),
        texto(filas.iter().map(|f| f.eje_x.nombre.as_str())),
        texto(filas.iter().map(|f| f.eje_x.tipo.as_str())),
        texto_nulable(filas.iter().map(|f| f.eje_x.unidad.as_deref())),
        texto(filas.iter().map(|f| f.eje_y.nombre.as_str())),
        texto(filas.iter().map(|f| f.eje_y.tipo.as_str())),
        texto_nulable(filas.iter().map(|f| f.eje_y.unidad.as_deref())),
        texto(filas.iter().map(|f| f.valor_salida_nombre.as_str())),
        texto(filas.iter().map(|f| f.instrucciones_uso_sql.as_str())),
        vectores(&vectores_tabla)?,
        paginas(filas.iter().map(|f| f.paginas.map(|(inicio, _)| inicio))),
        paginas(filas.iter().map(|f| f.paginas.map(|(_, fin)| fin))),
    ];
    ensamblar(
        schema_catalogo_tablas(),
        Some(texto(filas.iter().map(|f| f.tabla_id.as_str()))),
        columnas_comunes(&claves, tx),
        especificas,
        "04",
    )
}

fn columnas_limites(limites: &[&LimitesEje]) -> Vec<ArrayRef> {
    vec![
        texto_nulable(limites.iter().map(|l| l.categoria.as_deref())),
        flotante_nulable(limites.iter().map(|l| l.minimo)),
        booleano(limites.iter().map(|l| l.minimo_inclusivo)),
        flotante_nulable(limites.iter().map(|l| l.maximo)),
        booleano(limites.iter().map(|l| l.maximo_inclusivo)),
    ]
}

fn lote_celdas(
    filas: &[FilaCelda],
    tx: &ColumnasTransaccionales,
) -> Result<RecordBatch, DomainError> {
    let claves: Vec<&ClaveDocumental> = filas.iter().map(|f| &f.clave).collect();
    let ejes_x: Vec<&LimitesEje> = filas.iter().map(|f| &f.eje_x).collect();
    let ejes_y: Vec<&LimitesEje> = filas.iter().map(|f| &f.eje_y).collect();
    let mut especificas = vec![texto(filas.iter().map(|f| f.tabla_id.as_str()))];
    especificas.extend(columnas_limites(&ejes_x));
    especificas.extend(columnas_limites(&ejes_y));
    especificas.extend([
        texto_nulable(filas.iter().map(|f| f.eje_z_filtro_adicional.as_deref())),
        texto(filas.iter().map(|f| f.valor_resultado_texto.as_str())),
        flotante_nulable(filas.iter().map(|f| f.valor_resultado_num)),
        texto_nulable(filas.iter().map(|f| f.unidad_medida.as_deref())),
        texto_nulable(filas.iter().map(|f| f.nota_pie_condicion.as_deref())),
        nivel(filas.iter().map(|f| f.nivel_prelacion.valor())),
        paginas(filas.iter().map(|f| f.pagina)),
    ]);
    ensamblar(
        schema_celdas_doble_entrada(),
        Some(texto(filas.iter().map(|f| f.celda_id.as_str()))),
        columnas_comunes(&claves, tx),
        especificas,
        "05",
    )
}

fn lote_indice(
    filas: &[FilaIndice],
    tx: &ColumnasTransaccionales,
) -> Result<RecordBatch, DomainError> {
    let claves: Vec<&ClaveDocumental> = filas.iter().map(|f| &f.clave).collect();
    let vectores_seccion: Vec<Option<&[f32]>> = filas
        .iter()
        .map(|f| Some(f.vector_seccion.as_slice()))
        .collect();
    let especificas = vec![
        texto(filas.iter().map(|f| f.estado_contenido.as_str())),
        texto(filas.iter().map(|f| f.tipo_nodo.as_str())),
        nivel(filas.iter().map(|f| f.nivel_jerarquico.valor())),
        texto(filas.iter().map(|f| f.codigo_seccion.as_str())),
        texto(filas.iter().map(|f| f.titulo_seccion.as_str())),
        texto_nulable(
            filas
                .iter()
                .map(|f| f.nodo_padre_id.as_ref().map(|n| n.as_str())),
        ),
        texto(filas.iter().map(|f| f.ruta_linaje.as_str())),
        texto_nulable(filas.iter().map(|f| f.condiciones_alcance.as_deref())),
        texto(filas.iter().map(|f| f.resumen_seccion.as_str())),
        vectores(&vectores_seccion)?,
    ];
    ensamblar(
        schema_indice_jerarquico(),
        Some(texto(filas.iter().map(|f| f.nodo_id.as_str()))),
        columnas_comunes(&claves, tx),
        especificas,
        "07",
    )
}

fn lote_aristas(
    filas: &[FilaArista],
    tx: &ColumnasTransaccionales,
) -> Result<RecordBatch, DomainError> {
    let claves: Vec<&ClaveDocumental> = filas.iter().map(|f| &f.clave).collect();
    let especificas = vec![
        texto(filas.iter().map(|f| f.origen_id.as_str())),
        texto(filas.iter().map(|f| f.origen_tipo.as_str())),
        texto(filas.iter().map(|f| f.tipo_relacion.as_str())),
        texto(filas.iter().map(|f| f.destino_id.as_str())),
        texto(filas.iter().map(|f| f.destino_tipo.as_str())),
        texto_nulable(filas.iter().map(|f| f.contexto_relacion.as_deref())),
    ];
    ensamblar(
        schema_grafo_referencias(),
        Some(texto(filas.iter().map(|f| f.edge_id.as_str()))),
        columnas_comunes(&claves, tx),
        especificas,
        "06",
    )
}
