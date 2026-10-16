//! Adaptador del Motor Analítico sobre Apache DataFusion 45

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use arrow_array::RecordBatch;
use async_trait::async_trait;
use datafusion::datasource::MemTable;
use datafusion::execution::context::SessionContext;
use tokio::sync::RwLock;

use crate::application::ports::analytical_engine::AnalyticalEnginePort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

/// Adaptador que encapsula SessionContext de Apache DataFusion garantizando
/// aislamiento de centros y consultas SQL analíticas de solo lectura.
pub struct DataFusionEngine {
    context: Arc<SessionContext>,
    tablas_por_centro: RwLock<HashMap<CenterId, HashSet<String>>>,
    tablas_registradas: RwLock<HashSet<String>>,
}

impl Default for DataFusionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DataFusionEngine {
    pub fn new() -> Self {
        Self {
            context: Arc::new(SessionContext::new()),
            tablas_por_centro: RwLock::new(HashMap::new()),
            tablas_registradas: RwLock::new(HashSet::new()),
        }
    }

    /// Valida que la sentencia SQL sea de solo lectura
    fn validar_seguridad_sql(sql: &str) -> Result<(), DomainError> {
        let sql_limpio = sql.trim().to_uppercase();

        // Lista de comandos DDL / DML mutables prohibidos
        let comandos_prohibidos = [
            "DROP ",
            "DELETE ",
            "INSERT ",
            "UPDATE ",
            "ALTER ",
            "CREATE ",
            "TRUNCATE ",
            "GRANT ",
            "REVOKE ",
        ];

        for cmd in comandos_prohibidos {
            if sql_limpio.starts_with(cmd) || sql_limpio.contains(&format!("; {cmd}")) {
                return Err(DomainError::QueryError(format!(
                    "Operación no permitida: solo se permiten consultas analíticas de lectura (SELECT). Comando detectado: '{cmd}'"
                )));
            }
        }

        if !sql_limpio.starts_with("SELECT")
            && !sql_limpio.starts_with("WITH")
            && !sql_limpio.starts_with("EXPLAIN")
            && !sql_limpio.starts_with("SHOW")
        {
            return Err(DomainError::QueryError(
                "La consulta SQL debe comenzar con SELECT, WITH, SHOW o EXPLAIN".to_string(),
            ));
        }

        Ok(())
    }
}

#[async_trait]
impl AnalyticalEnginePort for DataFusionEngine {
    async fn registrar_tabla(
        &self,
        nombre_tabla: &str,
        lotes: Vec<RecordBatch>,
    ) -> Result<(), DomainError> {
        if lotes.is_empty() {
            return Err(DomainError::QueryError(format!(
                "No se puede registrar la tabla '{nombre_tabla}' sin lotes de datos ni esquema"
            )));
        }

        let esquema = lotes[0].schema();
        let partitions = vec![lotes];

        let mem_table = MemTable::try_new(esquema, partitions)
            .map_err(|e| DomainError::QueryError(format!("Error creando MemTable: {e}")))?;

        // Desregistrar primero si ya existía para actualización atómica
        let _ = self.context.deregister_table(nombre_tabla);

        self.context
            .register_table(nombre_tabla, Arc::new(mem_table))
            .map_err(|e| DomainError::QueryError(format!("Error registrando tabla: {e}")))?;

        let mut reg = self.tablas_registradas.write().await;
        reg.insert(nombre_tabla.to_string());

        Ok(())
    }

    async fn desregistrar_tabla(&self, nombre_tabla: &str) -> Result<(), DomainError> {
        let _ = self.context.deregister_table(nombre_tabla);
        let mut reg = self.tablas_registradas.write().await;
        reg.remove(nombre_tabla);
        Ok(())
    }

    async fn registrar_centro(
        &self,
        center_id: &CenterId,
        nombre_tabla: &str,
        lotes: Vec<RecordBatch>,
    ) -> Result<(), DomainError> {
        // Formato con prefijo de centro para aislamiento: "{center_id}_{nombre_tabla}"
        let nombre_con_prefijo = format!("{}_{}", center_id.as_str(), nombre_tabla);
        self.registrar_tabla(&nombre_con_prefijo, lotes).await?;

        let mut centros = self.tablas_por_centro.write().await;
        centros
            .entry(center_id.clone())
            .or_default()
            .insert(nombre_con_prefijo);

        Ok(())
    }

    async fn desregistrar_centro(&self, center_id: &CenterId) -> Result<(), DomainError> {
        let mut centros = self.tablas_por_centro.write().await;
        if let Some(tablas) = centros.remove(center_id) {
            for tabla in tablas {
                self.desregistrar_tabla(&tabla).await?;
            }
        }
        Ok(())
    }

    async fn ejecutar_sql(&self, sql: &str) -> Result<Vec<RecordBatch>, DomainError> {
        Self::validar_seguridad_sql(sql)?;

        let dataframe = self.context.sql(sql).await.map_err(|e| {
            DomainError::QueryError(format!("Error de sintaxis o ejecución SQL: {e}"))
        })?;

        let resultados = dataframe.collect().await.map_err(|e| {
            DomainError::QueryError(format!("Error recolectando resultados de consulta: {e}"))
        })?;

        Ok(resultados)
    }

    async fn listar_tablas_registradas(&self) -> Result<Vec<String>, DomainError> {
        let reg = self.tablas_registradas.read().await;
        let mut lista: Vec<String> = reg.iter().cloned().collect();
        lista.sort();
        Ok(lista)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::{Int32Array, StringArray};
    use arrow_schema::{DataType, Field, Schema};

    fn crear_lote_prueba() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("valor", DataType::Int32, false),
        ]));

        let id_col = Arc::new(StringArray::from(vec!["A1", "A2", "A3"]));
        let val_col = Arc::new(Int32Array::from(vec![10, 20, 30]));

        RecordBatch::try_new(schema, vec![id_col, val_col]).unwrap()
    }

    #[tokio::test]
    async fn test_datafusion_registro_y_consulta_sql() {
        let engine = DataFusionEngine::new();
        let batch = crear_lote_prueba();

        engine
            .registrar_tabla("metricas", vec![batch])
            .await
            .unwrap();

        let res = engine
            .ejecutar_sql("SELECT id, valor FROM metricas WHERE valor >= 20 ORDER BY valor ASC")
            .await
            .unwrap();

        assert_eq!(res.len(), 1);
        assert_eq!(res[0].num_rows(), 2);
    }

    #[tokio::test]
    async fn test_datafusion_seguridad_bloquea_mutaciones() {
        let engine = DataFusionEngine::new();
        let err_drop = engine
            .ejecutar_sql("DROP TABLE metricas")
            .await
            .unwrap_err();
        assert!(matches!(err_drop, DomainError::QueryError(_)));

        let err_insert = engine
            .ejecutar_sql("INSERT INTO metricas VALUES ('A4', 40)")
            .await
            .unwrap_err();
        assert!(matches!(err_insert, DomainError::QueryError(_)));
    }

    #[tokio::test]
    async fn test_datafusion_aislamiento_multi_centro() {
        let engine = DataFusionEngine::new();
        let centro1 = CenterId::new("planta_norte").unwrap();
        let centro2 = CenterId::new("planta_sur").unwrap();

        let batch1 = crear_lote_prueba();
        let batch2 = crear_lote_prueba();

        engine
            .registrar_centro(&centro1, "metricas", vec![batch1])
            .await
            .unwrap();
        engine
            .registrar_centro(&centro2, "metricas", vec![batch2])
            .await
            .unwrap();

        let tablas = engine.listar_tablas_registradas().await.unwrap();
        assert!(tablas.contains(&"planta_norte_metricas".to_string()));
        assert!(tablas.contains(&"planta_sur_metricas".to_string()));

        // Desregistrar centro 1 no afecta al centro 2
        engine.desregistrar_centro(&centro1).await.unwrap();
        let tablas_post = engine.listar_tablas_registradas().await.unwrap();
        assert!(!tablas_post.contains(&"planta_norte_metricas".to_string()));
        assert!(tablas_post.contains(&"planta_sur_metricas".to_string()));
    }
}
