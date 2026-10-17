//! Servicio de ejecución determinista de las 5 herramientas operativas del Lakehouse

use std::sync::Arc;

use arrow_array::RecordBatch;
use async_trait::async_trait;
use serde_json::json;

use crate::application::dtos::tools::*;
use crate::application::ports::analytical_engine::AnalyticalEnginePort;
use crate::application::ports::graph_engine::GraphEnginePort;
use crate::application::ports::table_lookup::TableLookupPort;
use crate::application::ports::tool_server::ToolServerPort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::document::DocId;

/// Servicio orquestador que implementa el servidor de herramientas del Lakehouse
pub struct LakehouseToolService {
    analytical_engine: Arc<dyn AnalyticalEnginePort>,
    table_lookup: Arc<dyn TableLookupPort>,
    graph_engine: Arc<dyn GraphEnginePort>,
}

impl LakehouseToolService {
    pub fn new(
        analytical_engine: Arc<dyn AnalyticalEnginePort>,
        table_lookup: Arc<dyn TableLookupPort>,
        graph_engine: Arc<dyn GraphEnginePort>,
    ) -> Self {
        Self {
            analytical_engine,
            table_lookup,
            graph_engine,
        }
    }

    /// Convierte lotes RecordBatch a serde_json::Value
    fn batches_to_json(batches: &[RecordBatch]) -> Result<serde_json::Value, DomainError> {
        if batches.is_empty() {
            return Ok(json!([]));
        }

        let mut buf = Vec::new();
        {
            let mut writer = arrow::json::ArrayWriter::new(&mut buf);
            for b in batches {
                writer.write(b).map_err(|e| {
                    DomainError::ToolExecutionError(format!("Error escribiendo Arrow a JSON: {e}"))
                })?;
            }
            writer.finish().map_err(|e| {
                DomainError::ToolExecutionError(format!(
                    "Error finalizando ArrayWriter de Arrow: {e}"
                ))
            })?;
        }

        serde_json::from_slice(&buf).map_err(|e| {
            DomainError::ToolExecutionError(format!(
                "Error deserializando buffer Arrow a JSON Value: {e}"
            ))
        })
    }

    /// Sanitiza un valor alfanumérico para evitar inyecciones simples en filtros SQL
    fn sanitizar_filtro(s: &str) -> String {
        s.replace('\'', "''").replace(';', "")
    }

    /// Resuelve el nombre de la tabla calificada por el centro
    async fn resolver_nombre_tabla(&self, centro_id: &CenterId, nombre_base: &str) -> String {
        let con_prefijo = format!("{}_{}", centro_id.as_str(), nombre_base);
        if let Ok(registradas) = self.analytical_engine.listar_tablas_registradas().await {
            if registradas.contains(&con_prefijo) {
                return con_prefijo;
            }
            if registradas.contains(&nombre_base.to_string()) {
                return nombre_base.to_string();
            }
        }
        con_prefijo
    }

    // --- Implementaciones internas de las 5 herramientas ---

    async fn handle_inspect_catalog(
        &self,
        call_id: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, DomainError> {
        let params: InspectCatalogParams =
            serde_json::from_value(arguments.clone()).map_err(|e| {
                DomainError::ToolExecutionError(format!(
                    "Argumentos inválidos para inspect_lakehouse_catalog: {e}"
                ))
            })?;

        let centro = CenterId::new(&params.centro_id)?;
        let tabla = self.resolver_nombre_tabla(&centro, "doc_catalog").await;

        let solo_vigentes = params.solo_vigentes.unwrap_or(true);
        let where_clause = if solo_vigentes {
            " WHERE vigente = true"
        } else {
            ""
        };
        let sql = format!(
            "SELECT doc_id, titulo, tipo, nivel_prelacion, version, fecha_emision, vigente FROM {tabla}{where_clause} ORDER BY nivel_prelacion ASC"
        );

        let batches = match self.analytical_engine.ejecutar_sql(&sql).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolResult {
                    call_id: call_id.to_string(),
                    tool_name: LakehouseToolName::InspectLakehouseCatalog
                        .as_str()
                        .to_string(),
                    output: json!({ "error": e.to_string(), "centro_id": centro.as_str() }),
                    is_error: true,
                });
            }
        };

        let output = Self::batches_to_json(&batches)?;
        Ok(ToolResult {
            call_id: call_id.to_string(),
            tool_name: LakehouseToolName::InspectLakehouseCatalog
                .as_str()
                .to_string(),
            output,
            is_error: false,
        })
    }

    async fn handle_query_operational_matrix(
        &self,
        call_id: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, DomainError> {
        let params: QueryOperationalMatrixParams = serde_json::from_value(arguments.clone())
            .map_err(|e| {
                DomainError::ToolExecutionError(format!(
                    "Argumentos inválidos para query_operational_matrix: {e}"
                ))
            })?;

        let centro = CenterId::new(&params.centro_id)?;
        let tabla = self
            .resolver_nombre_tabla(&centro, "operational_matrix")
            .await;

        let mut condiciones = vec!["vigente = true".to_string()];

        if let Some(fase) = params
            .fase
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
        {
            let limpia = Self::sanitizar_filtro(fase);
            condiciones.push(format!("LOWER(fase_operativa) = LOWER('{limpia}')"));
        }

        if let Some(parametro) = params
            .parametro
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            let limpia = Self::sanitizar_filtro(parametro);
            condiciones.push(format!("LOWER(parametro_tecnico) LIKE LOWER('%{limpia}%')"));
        }

        let where_clause = if condiciones.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", condiciones.join(" AND "))
        };

        let sql = format!(
            "SELECT fase_operativa, doc_id, parametro_tecnico, valor_nominal, valor_minimo, valor_maximo, unidad_medida, condicion_aplicacion, vigente FROM {tabla}{where_clause}"
        );

        let batches = match self.analytical_engine.ejecutar_sql(&sql).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolResult {
                    call_id: call_id.to_string(),
                    tool_name: LakehouseToolName::QueryOperationalMatrix
                        .as_str()
                        .to_string(),
                    output: json!({ "error": e.to_string(), "centro_id": centro.as_str() }),
                    is_error: true,
                });
            }
        };

        let output = Self::batches_to_json(&batches)?;
        Ok(ToolResult {
            call_id: call_id.to_string(),
            tool_name: LakehouseToolName::QueryOperationalMatrix
                .as_str()
                .to_string(),
            output,
            is_error: false,
        })
    }

    async fn handle_lookup_double_entry_table(
        &self,
        call_id: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, DomainError> {
        let params: LookupDoubleEntryTableParams = serde_json::from_value(arguments.clone())
            .map_err(|e| {
                DomainError::ToolExecutionError(format!(
                    "Argumentos inválidos para lookup_double_entry_table: {e}"
                ))
            })?;

        let centro = CenterId::new(&params.centro_id)?;

        // Intento 1: valor_x en fila (rango) y categoria_y en columna (etiqueta)
        let hallado = self
            .table_lookup
            .buscar_celda(
                &params.tabla_id,
                None,
                Some(params.valor_x),
                Some(&params.categoria_y),
                None,
            )
            .await;

        let celda_opt = match hallado {
            Ok(Some(c)) => Some(c),
            Ok(None) => {
                // Intento 2: transpuesta (categoria_y en fila y valor_x en columna)
                self.table_lookup
                    .buscar_celda(
                        &params.tabla_id,
                        Some(&params.categoria_y),
                        None,
                        None,
                        Some(params.valor_x),
                    )
                    .await
                    .unwrap_or(None)
            }
            Err(e) => {
                return Ok(ToolResult {
                    call_id: call_id.to_string(),
                    tool_name: LakehouseToolName::LookupDoubleEntryTable
                        .as_str()
                        .to_string(),
                    output: json!({ "error": e.to_string(), "centro_id": centro.as_str() }),
                    is_error: true,
                });
            }
        };

        let output = match celda_opt {
            Some(celda) => json!({
                "encontrado": true,
                "tabla_id": params.tabla_id,
                "centro_id": centro.as_str(),
                "valor_resultado": celda.valor_resultado,
                "unidad_medida": celda.unidad_medida,
                "notas_al_pie": celda.notas_al_pie,
                "doc_id": celda.doc_id.as_str(),
            }),
            None => json!({
                "encontrado": false,
                "tabla_id": params.tabla_id,
                "centro_id": centro.as_str(),
                "mensaje": format!("No se encontró celda que cumpla con valor_x={} y categoria_y='{}'", params.valor_x, params.categoria_y),
            }),
        };

        Ok(ToolResult {
            call_id: call_id.to_string(),
            tool_name: LakehouseToolName::LookupDoubleEntryTable
                .as_str()
                .to_string(),
            output,
            is_error: false,
        })
    }

    async fn handle_execute_athena_sql(
        &self,
        call_id: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, DomainError> {
        let params: ExecuteAthenaSqlParams =
            serde_json::from_value(arguments.clone()).map_err(|e| {
                DomainError::ToolExecutionError(format!(
                    "Argumentos inválidos para execute_athena_sql: {e}"
                ))
            })?;

        let centro = CenterId::new(&params.centro_id)?;
        let mut sql = params.sql_query.trim().to_string();

        // Control de aislamiento: Si el SQL usa nombres de tablas canónicos no cualificados, sustituirlos por el prefijo del centro
        let tablas_base = [
            "doc_catalog",
            "doc_chunks",
            "operational_matrix",
            "double_entry_tables",
            "double_entry_cells",
            "reference_graph",
        ];

        for t in tablas_base {
            let prefijo_centro = format!("{}_{}", centro.as_str(), t);
            // Reemplaza ocurrencias aisladas de la tabla sin prefijo
            let patron_espacio = format!(" {t} ");
            let reemplazo_espacio = format!(" {prefijo_centro} ");
            sql = sql.replace(&patron_espacio, &reemplazo_espacio);

            let patron_fin = format!(" {t}");
            if sql.ends_with(&patron_fin) {
                sql = format!("{} {prefijo_centro}", &sql[..sql.len() - patron_fin.len()]);
            }
        }

        let batches = match self.analytical_engine.ejecutar_sql(&sql).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolResult {
                    call_id: call_id.to_string(),
                    tool_name: LakehouseToolName::ExecuteAthenaSql.as_str().to_string(),
                    output: json!({ "error": e.to_string(), "query_ejecutada": sql, "centro_id": centro.as_str() }),
                    is_error: true,
                });
            }
        };

        let output = Self::batches_to_json(&batches)?;
        Ok(ToolResult {
            call_id: call_id.to_string(),
            tool_name: LakehouseToolName::ExecuteAthenaSql.as_str().to_string(),
            output,
            is_error: false,
        })
    }

    async fn handle_traverse_normative_graph(
        &self,
        call_id: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolResult, DomainError> {
        let params: TraverseNormativeGraphParams = serde_json::from_value(arguments.clone())
            .map_err(|e| {
                DomainError::ToolExecutionError(format!(
                    "Argumentos inválidos para traverse_normative_graph: {e}"
                ))
            })?;

        let centro = CenterId::new(&params.centro_id)?;
        let doc_id = DocId::new(&params.doc_origen_id)?;

        let modo_limpio = params.modo.trim().to_lowercase();
        let output = match modo_limpio.as_str() {
            "prevalencia" => match self.graph_engine.obtener_norma_prevalente(&doc_id).await {
                Ok(prevalente) => json!({
                    "centro_id": centro.as_str(),
                    "doc_origen": doc_id.as_str(),
                    "norma_prevalente": prevalente.as_str(),
                    "ha_sido_derogado": doc_id != prevalente,
                }),
                Err(e) => json!({ "error": e.to_string(), "doc_origen": doc_id.as_str() }),
            },
            "linaje" => match self.graph_engine.obtener_linaje(&doc_id).await {
                Ok(linaje) => {
                    let derogaciones: Vec<String> = linaje
                        .camino_derogacion
                        .iter()
                        .map(|d| d.as_str().to_string())
                        .collect();
                    let referencias: Vec<serde_json::Value> = linaje.referencias_directas.iter().map(|(d, rel)| {
                            json!({ "doc_destino": d.as_str(), "relacion": format!("{rel:?}") })
                        }).collect();
                    json!({
                        "centro_id": centro.as_str(),
                        "raiz": linaje.raiz.as_str(),
                        "prevalente": linaje.prevalente.as_str(),
                        "camino_derogacion": derogaciones,
                        "referencias_directas": referencias,
                    })
                }
                Err(e) => json!({ "error": e.to_string(), "doc_origen": doc_id.as_str() }),
            },
            "adyacentes" => match self.graph_engine.obtener_linaje(&doc_id).await {
                Ok(linaje) => {
                    let referencias: Vec<serde_json::Value> = linaje.referencias_directas.iter().map(|(d, rel)| {
                            json!({ "doc_destino": d.as_str(), "relacion": format!("{rel:?}") })
                        }).collect();
                    json!({
                        "centro_id": centro.as_str(),
                        "doc_origen": doc_id.as_str(),
                        "referencias_directas": referencias,
                    })
                }
                Err(e) => json!({ "error": e.to_string(), "doc_origen": doc_id.as_str() }),
            },
            otro => {
                return Ok(ToolResult {
                    call_id: call_id.to_string(),
                    tool_name: LakehouseToolName::TraverseNormativeGraph
                        .as_str()
                        .to_string(),
                    output: json!({
                        "error": format!("Modo de recorrido desconocido: '{otro}'. Modos válidos: prevalencia, linaje, adyacentes")
                    }),
                    is_error: true,
                });
            }
        };

        let is_error = output.get("error").is_some();
        Ok(ToolResult {
            call_id: call_id.to_string(),
            tool_name: LakehouseToolName::TraverseNormativeGraph
                .as_str()
                .to_string(),
            output,
            is_error,
        })
    }
}

#[async_trait]
impl ToolServerPort for LakehouseToolService {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        ToolDefinition::lakehouse_tools()
    }

    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
        let tool_name = match LakehouseToolName::from_str_name(&call.name) {
            Ok(n) => n,
            Err(e) => {
                return Ok(ToolResult {
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    output: json!({ "error": e.to_string() }),
                    is_error: true,
                });
            }
        };

        match tool_name {
            LakehouseToolName::InspectLakehouseCatalog => {
                self.handle_inspect_catalog(&call.id, &call.arguments).await
            }
            LakehouseToolName::QueryOperationalMatrix => {
                self.handle_query_operational_matrix(&call.id, &call.arguments)
                    .await
            }
            LakehouseToolName::LookupDoubleEntryTable => {
                self.handle_lookup_double_entry_table(&call.id, &call.arguments)
                    .await
            }
            LakehouseToolName::ExecuteAthenaSql => {
                self.handle_execute_athena_sql(&call.id, &call.arguments)
                    .await
            }
            LakehouseToolName::TraverseNormativeGraph => {
                self.handle_traverse_normative_graph(&call.id, &call.arguments)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::use_cases::table_lookup_service::TableLookupService;
    use crate::domain::models::reference_graph::{AristaGrafo, TipoRelacionNormativa};
    use crate::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
    use crate::infrastructure::query_engine::petgraph_engine::PetgraphEngine;
    use arrow_array::{BooleanArray, Int8Array, Int32Array, StringArray};
    use arrow_schema::{DataType, Field, Schema};

    fn fixture_catalog_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("doc_id", DataType::Utf8, false),
            Field::new("titulo", DataType::Utf8, false),
            Field::new("tipo", DataType::Utf8, false),
            Field::new("nivel_prelacion", DataType::Int8, false),
            Field::new("version", DataType::Int32, false),
            Field::new("fecha_emision", DataType::Utf8, false),
            Field::new("vigente", DataType::Boolean, false),
        ]));

        let doc_ids = Arc::new(StringArray::from(vec!["DOC-001", "DOC-002"]));
        let titulos = Arc::new(StringArray::from(vec![
            "Manual de Soldadura",
            "Protocolo de Pintura",
        ]));
        let tipos = Arc::new(StringArray::from(vec!["Norma", "Procedimiento"]));
        let prelaciones = Arc::new(Int8Array::from(vec![1, 3]));
        let versiones = Arc::new(Int32Array::from(vec![1, 2]));
        let fechas = Arc::new(StringArray::from(vec!["2026-01-01", "2026-02-01"]));
        let vigentes = Arc::new(BooleanArray::from(vec![true, false]));

        RecordBatch::try_new(
            schema,
            vec![
                doc_ids,
                titulos,
                tipos,
                prelaciones,
                versiones,
                fechas,
                vigentes,
            ],
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_tool_service_inspect_catalog() {
        let datafusion = Arc::new(DataFusionEngine::new());
        let lookup = Arc::new(TableLookupService::new());
        let petgraph = Arc::new(PetgraphEngine::new());

        let centro = CenterId::new("planta_alfa").unwrap();
        datafusion
            .registrar_centro(&centro, "doc_catalog", vec![fixture_catalog_batch()])
            .await
            .unwrap();

        let service = LakehouseToolService::new(datafusion, lookup, petgraph);

        let call = ToolCall {
            id: "call_1".to_string(),
            name: "inspect_lakehouse_catalog".to_string(),
            arguments: json!({
                "centro_id": "planta_alfa",
                "solo_vigentes": true
            }),
        };

        let result = service.ejecutar_herramienta(&call).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.tool_name, "inspect_lakehouse_catalog");

        let rows = result.output.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["doc_id"], "DOC-001");
    }

    #[tokio::test]
    async fn test_tool_service_traverse_normative_graph_prevalencia() {
        let datafusion = Arc::new(DataFusionEngine::new());
        let lookup = Arc::new(TableLookupService::new());
        let petgraph = Arc::new(PetgraphEngine::new());

        let doc1 = DocId::new("DOC-ORIGEN").unwrap();
        let doc2 = DocId::new("DOC-NUEVO").unwrap();
        let arista = AristaGrafo::new(
            "arista_1",
            doc2.clone(),
            "sec_2",
            doc1.clone(),
            "sec_1",
            TipoRelacionNormativa::DerogaA,
            1.0,
            Some("Derogación técnica total".to_string()),
        )
        .unwrap();
        petgraph.cargar_aristas(&[arista]).await.unwrap();

        let service = LakehouseToolService::new(datafusion, lookup, petgraph);

        let call = ToolCall {
            id: "call_2".to_string(),
            name: "traverse_normative_graph".to_string(),
            arguments: json!({
                "centro_id": "planta_alfa",
                "doc_origen_id": "DOC-ORIGEN",
                "modo": "prevalencia"
            }),
        };

        let result = service.ejecutar_herramienta(&call).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.output["norma_prevalente"], "DOC-NUEVO");
        assert_eq!(result.output["ha_sido_derogado"], true);
    }
}
