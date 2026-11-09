//! Proyección LanceDB → DataFusion e índice semántico (plan v2 §0, Fases 12-13).
//!
//! Decisiones:
//! * DataFusion recibe **todas las filas `COMMITTED`** (vigentes e históricas, con `vigente`,
//!   `version_num`, `fecha_vigor` y `fecha_derogacion`), para que la Fase 13 resuelva el
//!   *time travel* con SQL. Las filas `STAGED` nunca se proyectan. Las tools filtran
//!   `vigente = true` por defecto.
//! * Las columnas vectoriales (`vector`, `vector_hito`, `vector_tabla`, `vector_seccion`) no se proyectan:
//!   los vectores de chunks e hitos alimentan el índice semántico en memoria y un `SELECT *`
//!   no debe volcar 1024 floats.
//! * Las tools v2 resuelven grafo y tablas de doble entrada con SQL sobre las vistas del
//!   centro, así que no hay motores globales que recargar.
//! * La lectura de las 7 tablas se completa antes de tocar DataFusion: si una falla (p. ej. un
//!   `.lance` borrado a mano), las vistas previas no se alteran y el error nombra la tabla.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use arrow_array::{
    Array, BooleanArray, FixedSizeListArray, Float32Array, Int8Array, Int32Array, ListArray,
    RecordBatch, StringArray, StructArray,
};
use arrow_schema::{DataType, SchemaRef};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

use crate::application::dtos::centros::{DIRECTORIO_GLUE_CATALOG, VERSION_ESQUEMA_CENTRO};
use crate::application::ports::analytical_engine::AnalyticalEnginePort;
use crate::application::ports::center_workspace::CentroAbierto;
use crate::application::ports::lakehouse_projection::{
    LakehouseProjectionPort, ResumenProyeccion, VistaProyectada, nombre_vista,
};
use crate::application::ports::lakehouse_store::{
    ConsultaTabla, LIMITE_MAXIMO_LECTURA, TABLA_CHUNKS_NORMATIVA, TABLA_MATRIZ_OPERATIVA,
    TODAS_LAS_TABLAS,
};
use crate::application::ports::semantic_index::{
    FragmentoIndexado, IndiceSemanticoPort, OrigenFragmento,
};
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::estructura_documental::{EstadoContenido, TipoBloque};
use crate::domain::services::marcas_pagina::SaltoPagina;
use crate::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use crate::infrastructure::storage::schemas::{
    CAMPO_SALTO_DESPLAZAMIENTO, CAMPO_SALTO_PAGINA, COLUMNA_SALTOS_PAGINA,
    obtener_esquema_por_nombre,
};

/// Archivo del inventario exportado para el LLM
pub const ARCHIVO_INVENTARIO_GLUE: &str = "inventario_tablas.json";

/// Una tabla leída de LanceDB y lista para registrar
struct TablaLeida {
    tabla: &'static str,
    lotes: Vec<RecordBatch>,
    lotes_completos: Vec<RecordBatch>,
    filas: usize,
    filas_vigentes: usize,
}

/// Adaptador de proyección sobre los motores en memoria
pub struct LanceProjection {
    datafusion: Arc<DataFusionEngine>,
    indice: Option<Arc<dyn IndiceSemanticoPort>>,
    /// Centros con vistas registradas; el cerrojo serializa reconstrucciones y retiradas
    proyectados: Mutex<HashSet<CenterId>>,
}

impl LanceProjection {
    pub fn new(datafusion: Arc<DataFusionEngine>) -> Self {
        Self {
            datafusion,
            indice: None,
            proyectados: Mutex::new(HashSet::new()),
        }
    }

    /// Alimenta también el índice semántico de `search_normative_context` (Fase 13)
    pub fn con_indice_semantico(mut self, indice: Arc<dyn IndiceSemanticoPort>) -> Self {
        self.indice = Some(indice);
        self
    }

    async fn leer_tabla(
        centro: &CentroAbierto,
        tabla: &'static str,
    ) -> Result<TablaLeida, DomainError> {
        let consulta = ConsultaTabla::confirmadas(LIMITE_MAXIMO_LECTURA).del_centro(&centro.id);
        let lotes_completos = centro
            .store
            .leer_tabla(tabla, &consulta)
            .await
            .map_err(|e| {
                DomainError::StorageError(format!(
                    "No se pudo proyectar '{tabla}' del centro '{}' ({e}). El centro puede \
                     estar dañado: ejecuta --integridad {}",
                    centro.id, centro.id
                ))
            })?;
        let esquema = obtener_esquema_por_nombre(tabla).ok_or_else(|| {
            DomainError::StorageError(format!("Tabla sin esquema v2 conocido: '{tabla}'"))
        })?;

        let mut lotes = Vec::with_capacity(lotes_completos.len());
        for lote in &lotes_completos {
            lotes.push(sin_columnas_vectoriales(lote)?);
        }
        if lotes.iter().all(|l| l.num_rows() == 0) {
            lotes = vec![RecordBatch::new_empty(esquema_sin_vectores(&esquema))];
        }
        let filas = lotes.iter().map(RecordBatch::num_rows).sum();
        let filas_vigentes = lotes.iter().map(contar_vigentes).sum();
        Ok(TablaLeida {
            tabla,
            lotes,
            lotes_completos,
            filas,
            filas_vigentes,
        })
    }
}

fn columna_texto<'a>(lote: &'a RecordBatch, nombre: &str) -> Option<&'a StringArray> {
    lote.column_by_name(nombre)
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
}

fn texto_en(lote: &RecordBatch, nombre: &str, fila: usize) -> Option<String> {
    columna_texto(lote, nombre)
        .filter(|c| c.is_valid(fila))
        .map(|c| c.value(fila).to_string())
        .filter(|t| !t.trim().is_empty())
}

fn entero_en(lote: &RecordBatch, nombre: &str, fila: usize) -> i64 {
    let columna = lote.column_by_name(nombre);
    if let Some(c) = columna.and_then(|c| c.as_any().downcast_ref::<Int32Array>()) {
        return i64::from(c.value(fila));
    }
    columna
        .and_then(|c| c.as_any().downcast_ref::<Int8Array>())
        .map_or(0, |c| i64::from(c.value(fila)))
}

fn pagina_en(lote: &RecordBatch, nombre: &str, fila: usize) -> Option<i32> {
    lote.column_by_name(nombre)
        .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
        .filter(|c| c.is_valid(fila))
        .map(|c| c.value(fila))
}

/// Saltos de página de un chunk; `None` si la fila es nula o algún valor es negativo
fn saltos_en(lote: &RecordBatch, fila: usize) -> Option<Vec<SaltoPagina>> {
    let lista = lote
        .column_by_name(COLUMNA_SALTOS_PAGINA)?
        .as_any()
        .downcast_ref::<ListArray>()
        .filter(|l| l.is_valid(fila))?;
    let valores = lista.value(fila);
    let estructuras = valores.as_any().downcast_ref::<StructArray>()?;
    let columna = |nombre: &str| {
        estructuras
            .column_by_name(nombre)
            .and_then(|c| c.as_any().downcast_ref::<Int32Array>())
    };
    let desplazamientos = columna(CAMPO_SALTO_DESPLAZAMIENTO)?;
    let paginas = columna(CAMPO_SALTO_PAGINA)?;
    (0..estructuras.len())
        .map(|i| {
            Some(SaltoPagina {
                desplazamiento: usize::try_from(desplazamientos.value(i)).ok()?,
                pagina: u32::try_from(paginas.value(i)).ok()?,
            })
        })
        .collect()
}

fn vector_en(lote: &RecordBatch, nombre: &str, fila: usize) -> Vec<f32> {
    let Some(lista) = lote
        .column_by_name(nombre)
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>())
        .filter(|l| l.is_valid(fila))
    else {
        return Vec::new();
    };
    let valores = lista.value(fila);
    valores
        .as_any()
        .downcast_ref::<Float32Array>()
        .map(|f| f.values().to_vec())
        .unwrap_or_default()
}

/// Fragmento con las columnas comunes a chunks e hitos
fn fragmento_base(
    lote: &RecordBatch,
    fila: usize,
    id_columna: &str,
    origen: OrigenFragmento,
) -> FragmentoIndexado {
    FragmentoIndexado {
        id: texto_en(lote, id_columna, fila).unwrap_or_default(),
        origen,
        doc_id: texto_en(lote, "doc_id", fila).unwrap_or_default(),
        version_num: i32::try_from(entero_en(lote, "version_num", fila)).unwrap_or_default(),
        nivel_prelacion: i8::try_from(entero_en(lote, "nivel_prelacion", fila)).unwrap_or(5),
        breadcrumb_jerarquia: String::new(),
        condicion_aplicacion: texto_en(lote, "condicion_aplicacion", fila),
        texto: String::new(),
        chunk_origen_id: None,
        nodo_id: None,
        tipo_bloque: None,
        estado_contenido: None,
        pagina_inicio: None,
        pagina_fin: None,
        saltos_pagina: None,
        vector: Vec::new(),
    }
}

fn fragmentos_de_chunks(lotes: &[RecordBatch]) -> Vec<FragmentoIndexado> {
    let mut fragmentos = Vec::new();
    for lote in lotes {
        for fila in 0..lote.num_rows() {
            let mut f = fragmento_base(lote, fila, "chunk_id", OrigenFragmento::Chunk);
            f.breadcrumb_jerarquia =
                texto_en(lote, "breadcrumb_jerarquia", fila).unwrap_or_default();
            f.texto = texto_en(lote, "texto_contenido", fila).unwrap_or_default();
            f.nodo_id = texto_en(lote, "nodo_id", fila);
            f.tipo_bloque =
                texto_en(lote, "tipo_bloque", fila).and_then(|t| TipoBloque::parse(&t).ok());
            f.estado_contenido = texto_en(lote, "estado_contenido", fila)
                .and_then(|t| EstadoContenido::parse(&t).ok());
            f.pagina_inicio = pagina_en(lote, "pagina_inicio", fila);
            f.pagina_fin = pagina_en(lote, "pagina_fin", fila);
            f.saltos_pagina = saltos_en(lote, fila);
            f.vector = vector_en(lote, "vector", fila);
            fragmentos.push(f);
        }
    }
    fragmentos
}

/// Texto legible de un hito: roles y registro en campos separados (anti-fusión)
fn texto_hito(lote: &RecordBatch, fila: usize) -> String {
    let campo = |nombre: &str| texto_en(lote, nombre, fila).unwrap_or_else(|| "-".to_string());
    format!(
        "Fase {}: {}. Ejecutor: {}. Responsable: {} (origen: {}). Registro: {} {}. Criterio: {}",
        campo("fase_operativa"),
        campo("objeto_alcance"),
        campo("rol_ejecutor"),
        campo("rol_responsable"),
        campo("origen_responsable"),
        campo("registro_codigo"),
        campo("registro_nombre"),
        campo("criterio_aceptacion"),
    )
}

fn fragmentos_de_hitos(
    lotes: &[RecordBatch],
    chunks_origen: &HashMap<String, &FragmentoIndexado>,
) -> Vec<FragmentoIndexado> {
    let mut fragmentos = Vec::new();
    for lote in lotes {
        for fila in 0..lote.num_rows() {
            let mut f = fragmento_base(lote, fila, "hito_id", OrigenFragmento::Hito);
            f.chunk_origen_id = texto_en(lote, "chunk_origen_id", fila);
            if let Some(chunk) = f
                .chunk_origen_id
                .as_ref()
                .and_then(|c| chunks_origen.get(c))
            {
                f.breadcrumb_jerarquia = chunk.breadcrumb_jerarquia.clone();
                f.nodo_id = chunk.nodo_id.clone();
                f.pagina_inicio = chunk.pagina_inicio;
                f.pagina_fin = chunk.pagina_fin;
            }
            f.texto = texto_hito(lote, fila);
            f.vector = vector_en(lote, "vector_hito", fila);
            fragmentos.push(f);
        }
    }
    fragmentos
}

/// Chunks e hitos (con vector) de todas las revisiones confirmadas del centro
fn fragmentos_indexables(leidas: &[TablaLeida]) -> Vec<FragmentoIndexado> {
    let lotes_de = |tabla: &str| {
        leidas
            .iter()
            .find(|l| l.tabla == tabla)
            .map(|l| l.lotes_completos.clone())
            .unwrap_or_default()
    };
    let mut fragmentos = fragmentos_de_chunks(&lotes_de(TABLA_CHUNKS_NORMATIVA));
    let chunks_origen: HashMap<String, &FragmentoIndexado> =
        fragmentos.iter().map(|f| (f.id.clone(), f)).collect();
    let hitos = fragmentos_de_hitos(&lotes_de(TABLA_MATRIZ_OPERATIVA), &chunks_origen);
    fragmentos.extend(hitos);
    fragmentos
}

fn es_vectorial(tipo: &DataType) -> bool {
    matches!(tipo, DataType::FixedSizeList(_, _))
}

fn esquema_sin_vectores(esquema: &SchemaRef) -> SchemaRef {
    let campos: Vec<_> = esquema
        .fields()
        .iter()
        .filter(|f| !es_vectorial(f.data_type()))
        .cloned()
        .collect();
    Arc::new(arrow_schema::Schema::new(campos))
}

fn sin_columnas_vectoriales(lote: &RecordBatch) -> Result<RecordBatch, DomainError> {
    let indices: Vec<usize> = lote
        .schema()
        .fields()
        .iter()
        .enumerate()
        .filter(|(_, f)| !es_vectorial(f.data_type()))
        .map(|(i, _)| i)
        .collect();
    lote.project(&indices)
        .map_err(|e| DomainError::StorageError(format!("Error proyectando columnas: {e}")))
}

fn contar_vigentes(lote: &RecordBatch) -> usize {
    lote.column_by_name("vigente")
        .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())
        .map_or(0, |v| {
            (0..v.len())
                .filter(|&i| v.is_valid(i) && v.value(i))
                .count()
        })
}

fn tipo_legible(tipo: &DataType) -> String {
    match tipo {
        DataType::Utf8 => "VARCHAR".to_string(),
        DataType::Int8 | DataType::Int32 => "INT".to_string(),
        DataType::Float64 => "DOUBLE".to_string(),
        DataType::Boolean => "BOOLEAN".to_string(),
        DataType::Date32 => "DATE".to_string(),
        DataType::List(_) => "LIST<VARCHAR>".to_string(),
        otro => format!("{otro}"),
    }
}

/// Inventario de tablas y esquemas que se exporta a `glue_catalog/` para el LLM
pub fn inventario_glue(
    centro_id: &CenterId,
    tablas: &[(VistaProyectada, SchemaRef)],
) -> serde_json::Value {
    let entradas: Vec<serde_json::Value> = tablas
        .iter()
        .map(|(vista, esquema)| {
            let columnas: Vec<serde_json::Value> = esquema
                .fields()
                .iter()
                .map(|f| {
                    json!({
                        "nombre": f.name(),
                        "tipo": tipo_legible(f.data_type()),
                        "nulable": f.is_nullable(),
                    })
                })
                .collect();
            json!({
                "tabla": vista.tabla,
                "vista_datafusion": vista.vista,
                "filas_committed": vista.filas,
                "filas_vigentes": vista.filas_vigentes,
                "columnas": columnas,
            })
        })
        .collect();
    json!({
        "centro_id": centro_id.as_str(),
        "schema_version": VERSION_ESQUEMA_CENTRO,
        "instrucciones": "En execute_athena_sql usa el nombre canónico de la tabla (p. ej. \
            03_matriz_operativa): la herramienta lo traduce a la vista del centro. Las vistas \
            contienen todas las revisiones confirmadas: filtra vigente = true para la normativa \
            en vigor. Las columnas vectoriales no se proyectan.",
        "tablas": entradas,
    })
}

fn exportar_glue_catalog(
    ruta_centro: &Path,
    inventario: &serde_json::Value,
) -> Result<(), DomainError> {
    let directorio = ruta_centro.join(DIRECTORIO_GLUE_CATALOG);
    std::fs::create_dir_all(&directorio).map_err(|e| {
        DomainError::StorageError(format!("No se pudo crear '{}': {e}", directorio.display()))
    })?;
    let destino = directorio.join(ARCHIVO_INVENTARIO_GLUE);
    let temporal = destino.with_extension("tmp");
    let json = serde_json::to_vec_pretty(inventario)
        .map_err(|e| DomainError::StorageError(format!("Error serializando glue catalog: {e}")))?;
    std::fs::write(&temporal, json)
        .and_then(|_| std::fs::rename(&temporal, &destino))
        .map_err(|e| {
            DomainError::StorageError(format!("No se pudo escribir '{}': {e}", destino.display()))
        })
}

#[async_trait]
impl LakehouseProjectionPort for LanceProjection {
    async fn reconstruir_vistas(
        &self,
        centro: &CentroAbierto,
    ) -> Result<ResumenProyeccion, DomainError> {
        let mut leidas = Vec::with_capacity(TODAS_LAS_TABLAS.len());
        for tabla in TODAS_LAS_TABLAS {
            leidas.push(Self::leer_tabla(centro, tabla).await?);
        }

        let fragmentos = fragmentos_indexables(&leidas);
        let mut proyectados = self.proyectados.lock().await;
        self.datafusion.desregistrar_centro(&centro.id).await?;
        if let Some(indice) = &self.indice {
            indice.reemplazar_centro(&centro.id, fragmentos).await;
        }
        let mut inventario = Vec::with_capacity(leidas.len());
        for leida in leidas {
            let esquema = leida.lotes[0].schema();
            let vista = VistaProyectada {
                tabla: leida.tabla.to_string(),
                vista: nombre_vista(&centro.id, leida.tabla),
                filas: leida.filas,
                filas_vigentes: leida.filas_vigentes,
            };
            self.datafusion
                .registrar_centro(&centro.id, leida.tabla, leida.lotes)
                .await?;
            inventario.push((vista, esquema));
        }
        proyectados.insert(centro.id.clone());

        exportar_glue_catalog(&centro.ruta, &inventario_glue(&centro.id, &inventario))?;
        Ok(ResumenProyeccion {
            centro_id: centro.id.to_string(),
            vistas: inventario.into_iter().map(|(v, _)| v).collect(),
        })
    }

    async fn retirar_vistas(&self, centro_id: &CenterId) -> Result<(), DomainError> {
        let mut proyectados = self.proyectados.lock().await;
        self.datafusion.desregistrar_centro(centro_id).await?;
        if let Some(indice) = &self.indice {
            indice.retirar_centro(centro_id).await;
        }
        proyectados.remove(centro_id);
        Ok(())
    }

    async fn centros_proyectados(&self) -> Vec<CenterId> {
        let mut centros: Vec<CenterId> = self.proyectados.lock().await.iter().cloned().collect();
        centros.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        centros
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dtos::lakehouse_filas::{ClaveDocumental, FilaChunk, LoteIngesta};
    use crate::domain::models::document::{DocId, TipoDocumento};
    use crate::domain::models::estructura_documental::NodoId;
    use crate::domain::models::identificadores::IngestId;
    use crate::infrastructure::storage::lotes_arrow::{
        ColumnasTransaccionales, lote_a_record_batch,
    };
    use crate::infrastructure::storage::schemas::{
        DIMENSION_EMBEDDINGS, schema_indice_jerarquico, schema_matriz_operativa,
    };

    #[test]
    fn test_esquema_sin_vectores_quita_solo_las_columnas_vectoriales() {
        let original = schema_matriz_operativa();
        let proyectado = esquema_sin_vectores(&original);
        assert!(original.field_with_name("vector_hito").is_ok());
        assert!(proyectado.field_with_name("vector_hito").is_err());
        assert_eq!(proyectado.fields().len(), original.fields().len() - 1);
        assert!(proyectado.field_with_name("rol_responsable").is_ok());
    }

    #[test]
    fn test_esquema_del_indice_07_se_proyecta_sin_su_vector() {
        let proyectado = esquema_sin_vectores(&schema_indice_jerarquico());
        assert!(proyectado.field_with_name("vector_seccion").is_err());
        assert!(proyectado.field_with_name("ruta_linaje").is_ok());
    }

    #[test]
    fn test_fragmento_de_chunk_lleva_las_columnas_stair() {
        let clave = ClaveDocumental {
            centro_id: CenterId::new("obra-a").unwrap(),
            doc_id: DocId::new("DOC").unwrap(),
            version_num: 1,
        };
        let chunk = FilaChunk {
            pagina_inicio: Some(3),
            pagina_fin: Some(4),
            saltos_pagina: None,
            chunk_id: "DOC-c0".to_string(),
            clave,
            nodo_id: NodoId::new("DOC#sec-1").unwrap(),
            tipo_bloque: TipoBloque::Alcance,
            estado_contenido: EstadoContenido::ParcialUniversal,
            nivel_prelacion: TipoDocumento::ProcedimientoObra.nivel_prelacion(),
            breadcrumb_jerarquia: "1".to_string(),
            condicion_aplicacion: None,
            texto_contenido: "texto".to_string(),
            tablas_vinculadas: Vec::new(),
            hitos_vinculados: Vec::new(),
            vector: vec![0.5; DIMENSION_EMBEDDINGS as usize],
        };
        let transaccion = ColumnasTransaccionales::staged(&IngestId::new("ING").unwrap());
        let lote = lote_a_record_batch(&LoteIngesta::Chunks(vec![chunk]), &transaccion).unwrap();

        let fragmentos = fragmentos_de_chunks(&[lote]);
        assert_eq!(fragmentos[0].nodo_id.as_deref(), Some("DOC#sec-1"));
        assert_eq!(fragmentos[0].tipo_bloque, Some(TipoBloque::Alcance));
        assert_eq!(
            fragmentos[0].estado_contenido,
            Some(EstadoContenido::ParcialUniversal)
        );
        assert_eq!(fragmentos[0].pagina_inicio, Some(3));
        assert_eq!(fragmentos[0].pagina_fin, Some(4));
        assert_eq!(fragmentos[0].saltos_pagina, None, "sin saltos guardados");
    }

    #[test]
    fn test_saltos_de_pagina_van_y_vuelven_por_arrow() {
        use crate::domain::services::marcas_pagina::SaltoPagina;
        let salto = |desplazamiento, pagina| SaltoPagina {
            desplazamiento,
            pagina,
        };
        let chunk = |id: &str, saltos: Option<Vec<SaltoPagina>>| FilaChunk {
            pagina_inicio: Some(3),
            pagina_fin: Some(4),
            saltos_pagina: saltos,
            chunk_id: id.to_string(),
            clave: ClaveDocumental {
                centro_id: CenterId::new("obra-a").unwrap(),
                doc_id: DocId::new("DOC").unwrap(),
                version_num: 1,
            },
            nodo_id: NodoId::new("DOC#sec-1").unwrap(),
            tipo_bloque: TipoBloque::Alcance,
            estado_contenido: EstadoContenido::Completo,
            nivel_prelacion: TipoDocumento::ProcedimientoObra.nivel_prelacion(),
            breadcrumb_jerarquia: "1".to_string(),
            condicion_aplicacion: None,
            texto_contenido: "texto de dos paginas".to_string(),
            tablas_vinculadas: Vec::new(),
            hitos_vinculados: Vec::new(),
            vector: vec![0.5; DIMENSION_EMBEDDINGS as usize],
        };
        let filas = vec![
            chunk("DOC-c0", Some(vec![salto(0, 3), salto(9, 4)])),
            chunk("DOC-c1", None),
            chunk("DOC-c2", Some(vec![salto(0, 4)])),
            chunk("DOC-c3", Some(vec![salto(0, u32::MAX)])),
        ];
        let transaccion = ColumnasTransaccionales::staged(&IngestId::new("ING").unwrap());
        let lote = lote_a_record_batch(&LoteIngesta::Chunks(filas), &transaccion).unwrap();
        let fragmentos = fragmentos_de_chunks(&[lote]);
        assert_eq!(
            fragmentos[0].saltos_pagina,
            Some(vec![salto(0, 3), salto(9, 4)])
        );
        assert_eq!(fragmentos[1].saltos_pagina, None);
        assert_eq!(fragmentos[2].saltos_pagina, Some(vec![salto(0, 4)]));
        assert_eq!(
            fragmentos[3].saltos_pagina, None,
            "fuera de Int32 queda nulo"
        );
    }

    #[test]
    fn test_inventario_glue_lista_vistas_y_columnas() {
        let centro = CenterId::new("obra-a").unwrap();
        let esquema = esquema_sin_vectores(&schema_matriz_operativa());
        let vista = VistaProyectada {
            tabla: "03_matriz_operativa".into(),
            vista: nombre_vista(&centro, "03_matriz_operativa"),
            filas: 6,
            filas_vigentes: 3,
        };
        let inventario = inventario_glue(&centro, &[(vista, esquema)]);
        let tabla = &inventario["tablas"][0];
        assert_eq!(tabla["vista_datafusion"], "obra-a_03_matriz_operativa");
        assert_eq!(tabla["filas_vigentes"], 3);
        let columnas = tabla["columnas"].as_array().unwrap();
        assert!(columnas.iter().any(|c| c["nombre"] == "registro_codigo"));
        assert!(!columnas.iter().any(|c| c["nombre"] == "vector_hito"));
    }
}
