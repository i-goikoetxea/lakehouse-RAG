//! Proyección LanceDB → DataFusion / Petgraph / TableLookup (plan v2 §0, Fase 12).
//!
//! Decisiones:
//! * DataFusion recibe **todas las filas `COMMITTED`** (vigentes e históricas, con `vigente`,
//!   `version_num`, `fecha_vigor` y `fecha_derogacion`), para que la Fase 13 resuelva el
//!   *time travel* con SQL. Las filas `STAGED` nunca se proyectan. Las tools filtran
//!   `vigente = true` por defecto.
//! * Las columnas vectoriales (`vector`, `vector_hito`, `vector_tabla`) no se proyectan:
//!   la búsqueda semántica lee LanceDB directamente y un `SELECT *` no debe volcar 1024 floats.
//! * Petgraph y TableLookup son globales: se recargan con lo **vigente** de todos los centros
//!   proyectados cada vez que uno entra o sale.
//! * La lectura de las 6 tablas se completa antes de tocar DataFusion: si una falla (p. ej. un
//!   `.lance` borrado a mano), las vistas previas no se alteran y el error nombra la tabla.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use arrow_array::{Array, BooleanArray, RecordBatch};
use arrow_schema::{DataType, SchemaRef};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

use crate::application::dtos::centros::DIRECTORIO_GLUE_CATALOG;
use crate::application::ports::analytical_engine::AnalyticalEnginePort;
use crate::application::ports::center_workspace::CentroAbierto;
use crate::application::ports::lakehouse_projection::{
    LakehouseProjectionPort, ResumenProyeccion, VistaProyectada, nombre_vista,
};
use crate::application::ports::lakehouse_store::{
    ConsultaTabla, LIMITE_MAXIMO_LECTURA, TABLA_CELDAS_DOBLE_ENTRADA, TABLA_GRAFO_REFERENCIAS,
    TODAS_LAS_TABLAS,
};
use crate::application::use_cases::table_lookup_service::TableLookupService;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use crate::infrastructure::query_engine::petgraph_engine::PetgraphEngine;
use crate::infrastructure::storage::schemas::obtener_esquema_por_nombre;

/// Archivo del inventario exportado para el LLM
pub const ARCHIVO_INVENTARIO_GLUE: &str = "inventario_tablas.json";

/// Lotes vigentes que alimentan los motores globales
#[derive(Default)]
struct LotesGlobales {
    aristas: Vec<RecordBatch>,
    celdas: Vec<RecordBatch>,
}

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
    lookup: Arc<TableLookupService>,
    grafo: Arc<PetgraphEngine>,
    proyectados: Mutex<HashMap<CenterId, LotesGlobales>>,
}

impl LanceProjection {
    pub fn new(
        datafusion: Arc<DataFusionEngine>,
        lookup: Arc<TableLookupService>,
        grafo: Arc<PetgraphEngine>,
    ) -> Self {
        Self {
            datafusion,
            lookup,
            grafo,
            proyectados: Mutex::new(HashMap::new()),
        }
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

    /// Recarga Petgraph y TableLookup con lo vigente de todos los centros proyectados
    async fn recargar_motores_globales(
        &self,
        proyectados: &HashMap<CenterId, LotesGlobales>,
    ) -> Result<(usize, usize), DomainError> {
        self.grafo.vaciar().await;
        self.lookup.vaciar()?;
        let mut aristas = 0;
        let mut celdas = 0;
        for lotes in proyectados.values() {
            for lote in &lotes.aristas {
                aristas += self.grafo.cargar_desde_record_batch(lote).await?;
            }
            for lote in &lotes.celdas {
                celdas += self.lookup.cargar_desde_record_batch(lote)?;
            }
        }
        Ok((aristas, celdas))
    }
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
        "schema_version": 2,
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

        let mut proyectados = self.proyectados.lock().await;
        self.datafusion.desregistrar_centro(&centro.id).await?;
        let mut globales = LotesGlobales::default();
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
            match leida.tabla {
                TABLA_GRAFO_REFERENCIAS => globales.aristas = leida.lotes_completos,
                TABLA_CELDAS_DOBLE_ENTRADA => globales.celdas = leida.lotes_completos,
                _ => {}
            }
            inventario.push((vista, esquema));
        }
        proyectados.insert(centro.id.clone(), globales);
        let (aristas_grafo, celdas_lookup) = self.recargar_motores_globales(&proyectados).await?;

        exportar_glue_catalog(&centro.ruta, &inventario_glue(&centro.id, &inventario))?;
        Ok(ResumenProyeccion {
            centro_id: centro.id.to_string(),
            vistas: inventario.into_iter().map(|(v, _)| v).collect(),
            aristas_grafo,
            celdas_lookup,
        })
    }

    async fn retirar_vistas(&self, centro_id: &CenterId) -> Result<(), DomainError> {
        let mut proyectados = self.proyectados.lock().await;
        self.datafusion.desregistrar_centro(centro_id).await?;
        if proyectados.remove(centro_id).is_some() {
            self.recargar_motores_globales(&proyectados).await?;
        }
        Ok(())
    }

    async fn centros_proyectados(&self) -> Vec<CenterId> {
        let mut centros: Vec<CenterId> = self.proyectados.lock().await.keys().cloned().collect();
        centros.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        centros
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::storage::schemas::schema_matriz_operativa;

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
