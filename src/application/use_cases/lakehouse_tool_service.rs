//! Servicio de ejecución determinista de las 5 herramientas operativas del Lakehouse

use std::sync::Arc;

use arrow_array::RecordBatch;
use async_trait::async_trait;
use serde_json::json;

use crate::application::dtos::tools::*;
use crate::application::ports::analytical_engine::AnalyticalEnginePort;
use crate::application::ports::graph_engine::GraphEnginePort;
use crate::application::ports::lakehouse_projection::nombre_vista;
use crate::application::ports::lakehouse_store::{
    TABLA_DOCUMENTOS_CATALOGO, TABLA_MATRIZ_OPERATIVA, TODAS_LAS_TABLAS,
};
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

    /// Nombre SQL (entrecomillado) de la vista de una tabla v2 del centro. Las vistas solo
    /// existen mientras el centro está activo: si no, la consulta falla (cero acceso).
    fn vista_sql(centro_id: &CenterId, tabla: &str) -> String {
        format!("\"{}\"", nombre_vista(centro_id, tabla))
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
        let tabla = Self::vista_sql(&centro, TABLA_DOCUMENTOS_CATALOGO);

        let solo_vigentes = params.solo_vigentes.unwrap_or(true);
        let where_clause = if solo_vigentes {
            " WHERE vigente = true"
        } else {
            ""
        };
        let sql = format!(
            "SELECT doc_id, titulo, tipo_documento, nivel_prelacion, revision, version_num, fecha_vigor, fecha_derogacion, vigente FROM {tabla}{where_clause} ORDER BY nivel_prelacion ASC, doc_id ASC, version_num DESC"
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
        let tabla = Self::vista_sql(&centro, TABLA_MATRIZ_OPERATIVA);

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
            // v2 no tiene `parametro_tecnico`: el filtro libre se aplica al objeto del hito
            condiciones.push(format!("LOWER(objeto_alcance) LIKE LOWER('%{limpia}%')"));
        }

        let where_clause = if condiciones.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", condiciones.join(" AND "))
        };

        let sql = format!(
            "SELECT fase_operativa, doc_id, version_num, hito_id, orden_secuencia, objeto_alcance, rol_ejecutor, rol_responsable, registro_codigo, registro_nombre, criterio_aceptacion, condicion_aplicacion, calidad_extraccion, vigente FROM {tabla}{where_clause} ORDER BY doc_id, orden_secuencia"
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
        // Aislamiento: los nombres canónicos v2 se traducen a las vistas del centro
        let sql = reescribir_tablas_canonicas(params.sql_query.trim(), &centro);

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

fn es_caracter_de_identificador(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

/// Sustituye cada aparición aislada de `tabla` (sin distinguir mayúsculas) por la vista
/// entrecomillada; respeta las ya entrecomilladas y las que forman parte de otro identificador.
fn reemplazar_identificador(sql: &str, tabla: &str, vista: &str) -> String {
    let minusculas = sql.to_ascii_lowercase();
    let mut salida = String::with_capacity(sql.len() + vista.len());
    let mut cursor = 0;
    while let Some(relativo) = minusculas[cursor..].find(tabla) {
        let inicio = cursor + relativo;
        let fin = inicio + tabla.len();
        let previo = sql[..inicio].chars().last();
        let siguiente = sql[fin..].chars().next();
        salida.push_str(&sql[cursor..inicio]);
        if previo.is_some_and(es_caracter_de_identificador)
            || siguiente.is_some_and(es_caracter_de_identificador)
        {
            salida.push_str(&sql[inicio..fin]);
        } else if previo == Some('"') && siguiente == Some('"') {
            salida.push_str(vista);
        } else {
            salida.push('"');
            salida.push_str(vista);
            salida.push('"');
        }
        cursor = fin;
    }
    salida.push_str(&sql[cursor..]);
    salida
}

/// Traduce las 6 tablas canónicas v2 (`03_matriz_operativa`…) a las vistas del centro
pub fn reescribir_tablas_canonicas(sql: &str, centro: &CenterId) -> String {
    TODAS_LAS_TABLAS
        .iter()
        .fold(sql.to_string(), |acumulado, tabla| {
            reemplazar_identificador(&acumulado, tabla, &nombre_vista(centro, tabla))
        })
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
            Field::new("tipo_documento", DataType::Utf8, false),
            Field::new("nivel_prelacion", DataType::Int8, false),
            Field::new("revision", DataType::Utf8, false),
            Field::new("version_num", DataType::Int32, false),
            Field::new("fecha_vigor", DataType::Utf8, false),
            Field::new("fecha_derogacion", DataType::Utf8, true),
            Field::new("vigente", DataType::Boolean, false),
        ]));

        let doc_ids = Arc::new(StringArray::from(vec!["DOC-001", "DOC-002"]));
        let titulos = Arc::new(StringArray::from(vec![
            "Manual de Soldadura",
            "Protocolo de Pintura",
        ]));
        let tipos = Arc::new(StringArray::from(vec!["Norma", "Procedimiento"]));
        let prelaciones = Arc::new(Int8Array::from(vec![1, 3]));
        let revisiones = Arc::new(StringArray::from(vec!["Rev. 01", "Rev. 02"]));
        let versiones = Arc::new(Int32Array::from(vec![1, 2]));
        let fechas = Arc::new(StringArray::from(vec!["2026-01-01", "2026-02-01"]));
        let derogaciones = Arc::new(StringArray::from(vec![None, Some("2026-03-01")]));
        let vigentes = Arc::new(BooleanArray::from(vec![true, false]));

        RecordBatch::try_new(
            schema,
            vec![
                doc_ids,
                titulos,
                tipos,
                prelaciones,
                revisiones,
                versiones,
                fechas,
                derogaciones,
                vigentes,
            ],
        )
        .unwrap()
    }

    #[test]
    fn test_reescritura_traduce_tablas_canonicas_a_vistas_del_centro() {
        let centro = CenterId::new("obra-a").unwrap();
        let sql = "SELECT rol_responsable, COUNT(*) FROM 03_matriz_operativa m JOIN \"01_documentos_catalogo\" d ON m.doc_id = d.doc_id GROUP BY 1";
        let reescrita = reescribir_tablas_canonicas(sql, &centro);
        assert!(
            reescrita.contains("FROM \"obra-a_03_matriz_operativa\" m"),
            "{reescrita}"
        );
        assert!(
            reescrita.contains("JOIN \"obra-a_01_documentos_catalogo\" d"),
            "{reescrita}"
        );
    }

    #[test]
    fn test_reescritura_respeta_identificadores_que_contienen_el_nombre() {
        let centro = CenterId::new("obra-a").unwrap();
        for intacta in [
            "SELECT * FROM \"obra-a_03_matriz_operativa\"",
            "SELECT * FROM x03_matriz_operativa",
            "SELECT * FROM 03_matriz_operativa_bis",
        ] {
            assert_eq!(reescribir_tablas_canonicas(intacta, &centro), intacta);
        }
        assert_eq!(
            reescribir_tablas_canonicas("select * from 06_GRAFO_REFERENCIAS", &centro),
            "select * from \"obra-a_06_grafo_referencias\""
        );
    }

    #[tokio::test]
    async fn test_tool_service_inspect_catalog() {
        let datafusion = Arc::new(DataFusionEngine::new());
        let lookup = Arc::new(TableLookupService::new());
        let petgraph = Arc::new(PetgraphEngine::new());

        let centro = CenterId::new("planta_alfa").unwrap();
        datafusion
            .registrar_centro(
                &centro,
                "01_documentos_catalogo",
                vec![fixture_catalog_batch()],
            )
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
