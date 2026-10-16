//! Adaptador de Almacenamiento Columnar sobre LanceDB (.lance) y Apache Arrow

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::RecordBatch;
use async_trait::async_trait;
use lancedb::Table;
use lancedb::connection::Connection;

use crate::application::ports::lakehouse_store::{LakehouseStorePort, TODAS_LAS_TABLAS};
use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;
use crate::infrastructure::storage::schemas::obtener_esquema_por_nombre;

/// Adaptador de almacenamiento que gestiona las 6 tablas columnares en formato LanceDB
pub struct LanceDbStore {
    base_dir: PathBuf,
    connection: Arc<Connection>,
    init_lock: tokio::sync::Mutex<()>,
}

impl LanceDbStore {
    /// Conecta o crea el almacén LanceDB en la ruta de directorio indicada
    pub async fn conectar(base_dir: impl AsRef<Path>) -> Result<Self, DomainError> {
        let path = base_dir.as_ref().to_path_buf();
        let path_str = path.to_str().ok_or_else(|| {
            DomainError::StorageError("Ruta de almacenamiento inválida".to_string())
        })?;

        let connection = lancedb::connect(path_str)
            .execute()
            .await
            .map_err(|e| DomainError::StorageError(format!("Error conectando a LanceDB: {e}")))?;

        Ok(Self {
            base_dir: path,
            connection: Arc::new(connection),
            init_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Obtiene una referencia a la tabla abierta
    pub async fn abrir_tabla(&self, nombre_tabla: &str) -> Result<Table, DomainError> {
        self.connection
            .open_table(nombre_tabla)
            .execute()
            .await
            .map_err(|e| {
                DomainError::StorageError(format!("Error abriendo tabla '{nombre_tabla}': {e}"))
            })
    }

    /// Retorna la ruta base del almacén
    pub fn ruta_base(&self) -> &Path {
        &self.base_dir
    }
}

#[async_trait]
impl LakehouseStorePort for LanceDbStore {
    async fn inicializar_tablas(&self) -> Result<(), DomainError> {
        let _guard = self.init_lock.lock().await;
        let tablas_existentes = self.listar_tablas().await?;

        for nombre_tabla in TODAS_LAS_TABLAS {
            if !tablas_existentes.contains(&nombre_tabla.to_string()) {
                let esquema = obtener_esquema_por_nombre(nombre_tabla).ok_or_else(|| {
                    DomainError::StorageError(format!(
                        "Esquema no definido para la tabla '{nombre_tabla}'"
                    ))
                })?;

                let res = self
                    .connection
                    .create_empty_table(nombre_tabla, esquema)
                    .execute()
                    .await;

                if let Err(e) = res {
                    let msg = e.to_string();
                    if !msg.contains("already exists") && !msg.contains("AlreadyExists") {
                        return Err(DomainError::StorageError(format!(
                            "Error creando tabla vacía '{nombre_tabla}': {e}"
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    async fn insertar_lote(
        &self,
        nombre_tabla: &str,
        lote: RecordBatch,
    ) -> Result<(), DomainError> {
        let tabla = self.abrir_tabla(nombre_tabla).await?;
        let esquema = lote.schema();

        let iter = arrow_array::RecordBatchIterator::new(vec![Ok(lote)], esquema);
        let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(iter);

        tabla.add(reader).execute().await.map_err(|e| {
            DomainError::StorageError(format!(
                "Error insertando lote en tabla '{nombre_tabla}': {e}"
            ))
        })?;

        Ok(())
    }

    async fn retirar_vigencia_documento(&self, doc_id: &DocId) -> Result<u64, DomainError> {
        let tablas_con_doc_id = TODAS_LAS_TABLAS;
        let mut total_invalidados: u64 = 0;

        let filtro_sql = format!("doc_id = '{}' AND vigente = true", doc_id.as_str());

        for nombre_tabla in tablas_con_doc_id {
            if let Ok(tabla) = self.abrir_tabla(nombre_tabla).await {
                // Actualizar vigente = false para los registros coincidentes
                let res = tabla
                    .update()
                    .only_if(&filtro_sql)
                    .column("vigente", "false")
                    .execute()
                    .await;

                if let Ok(filas) = res {
                    total_invalidados += filas;
                }
            }
        }

        Ok(total_invalidados)
    }

    async fn contar_registros(&self, nombre_tabla: &str) -> Result<usize, DomainError> {
        let tabla = self.abrir_tabla(nombre_tabla).await?;
        let count = tabla.count_rows(None).await.map_err(|e| {
            DomainError::StorageError(format!(
                "Error contando filas en tabla '{nombre_tabla}': {e}"
            ))
        })?;

        Ok(count)
    }

    async fn listar_tablas(&self) -> Result<Vec<String>, DomainError> {
        self.connection
            .table_names()
            .execute()
            .await
            .map_err(|e| DomainError::StorageError(format!("Error listando tablas: {e}")))
    }
}
