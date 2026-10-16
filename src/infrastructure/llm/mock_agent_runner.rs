//! Adaptador Mock determinista para AgentRunnerPort

use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::json;

use crate::application::dtos::agent::{AgentMessage, AgentRole, AgentTurnOutput, ClaudeModel};
use crate::application::dtos::tools::{LakehouseToolName, ToolCall, ToolDefinition};
use crate::application::ports::agent_runner::AgentRunnerPort;
use crate::domain::errors::DomainError;

/// Mock determinista para pruebas continuas sin invocar procesos externos
pub struct MockAgentRunner {
    scripted_responses: Mutex<Vec<AgentTurnOutput>>,
    heuristic_mode: bool,
    error_programado: Mutex<Option<String>>,
}

impl Default for MockAgentRunner {
    fn default() -> Self {
        Self::new_heuristic()
    }
}

impl MockAgentRunner {
    pub fn new_heuristic() -> Self {
        Self {
            scripted_responses: Mutex::new(Vec::new()),
            heuristic_mode: true,
            error_programado: Mutex::new(None),
        }
    }

    pub fn new_scripted(respuestas: Vec<AgentTurnOutput>) -> Self {
        Self {
            scripted_responses: Mutex::new(respuestas),
            heuristic_mode: false,
            error_programado: Mutex::new(None),
        }
    }

    pub fn with_error(error_msg: &str) -> Self {
        Self {
            scripted_responses: Mutex::new(Vec::new()),
            heuristic_mode: false,
            error_programado: Mutex::new(Some(error_msg.to_string())),
        }
    }

    fn resolver_heuristica(
        &self,
        historial: &[AgentMessage],
    ) -> Result<AgentTurnOutput, DomainError> {
        let ultimo = match historial.last() {
            Some(m) => m,
            None => {
                return Ok(AgentTurnOutput::FinalMessage(
                    "Historial vacío, sin solicitud.".to_string(),
                ));
            }
        };

        // Si el último mensaje es el resultado de una herramienta, generamos la síntesis final
        if ultimo.role == AgentRole::Tool {
            let tool_id = ultimo.tool_call_id.as_deref().unwrap_or("unknown");
            return Ok(AgentTurnOutput::FinalMessage(format!(
                "Dictamen Técnico Final: Con base en la herramienta ejecutada (ID: {tool_id}), los datos obtenidos son válidos y conformes según las normas vigentes del Lakehouse."
            )));
        }

        // Si es el usuario, analizamos el texto para invocar la herramienta correspondiente
        let prompt = ultimo.content.to_lowercase();

        if prompt.contains("catálogo")
            || prompt.contains("catalogo")
            || prompt.contains("documentos")
        {
            Ok(AgentTurnOutput::ToolCalls(vec![ToolCall {
                id: "call_mock_cat".to_string(),
                name: LakehouseToolName::InspectLakehouseCatalog
                    .as_str()
                    .to_string(),
                arguments: json!({
                    "centro_id": "planta_alfa",
                    "solo_vigentes": true
                }),
            }]))
        } else if prompt.contains("matriz")
            || prompt.contains("operativa")
            || prompt.contains("fase")
        {
            Ok(AgentTurnOutput::ToolCalls(vec![ToolCall {
                id: "call_mock_mat".to_string(),
                name: LakehouseToolName::QueryOperationalMatrix
                    .as_str()
                    .to_string(),
                arguments: json!({
                    "centro_id": "planta_alfa",
                    "fase": "Recepcion"
                }),
            }]))
        } else if prompt.contains("tabla")
            || prompt.contains("doble entrada")
            || prompt.contains("espesor")
        {
            Ok(AgentTurnOutput::ToolCalls(vec![ToolCall {
                id: "call_mock_lookup".to_string(),
                name: LakehouseToolName::LookupDoubleEntryTable
                    .as_str()
                    .to_string(),
                arguments: json!({
                    "centro_id": "planta_alfa",
                    "tabla_id": "tabla_soldadura",
                    "valor_x": 12.5,
                    "categoria_y": "Acero"
                }),
            }]))
        } else if prompt.contains("sql") || prompt.contains("select") || prompt.contains("athena") {
            Ok(AgentTurnOutput::ToolCalls(vec![ToolCall {
                id: "call_mock_sql".to_string(),
                name: LakehouseToolName::ExecuteAthenaSql.as_str().to_string(),
                arguments: json!({
                    "centro_id": "planta_alfa",
                    "sql_query": "SELECT * FROM doc_catalog WHERE vigente = true"
                }),
            }]))
        } else if prompt.contains("prevalen")
            || prompt.contains("deroga")
            || prompt.contains("grafo")
        {
            Ok(AgentTurnOutput::ToolCalls(vec![ToolCall {
                id: "call_mock_graph".to_string(),
                name: LakehouseToolName::TraverseNormativeGraph
                    .as_str()
                    .to_string(),
                arguments: json!({
                    "centro_id": "planta_alfa",
                    "doc_origen_id": "DOC-NORM-001",
                    "modo": "prevalencia"
                }),
            }]))
        } else {
            Ok(AgentTurnOutput::FinalMessage(format!(
                "Respuesta directa sin herramientas: '{prompt}' procesada con éxito."
            )))
        }
    }
}

#[async_trait]
impl AgentRunnerPort for MockAgentRunner {
    async fn ejecutar_turno(
        &self,
        _modelo: ClaudeModel,
        _system_prompt: &str,
        historial: &[AgentMessage],
        _herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        if let Some(err) = self.error_programado.lock().unwrap().as_ref() {
            return Err(DomainError::AgentError(err.clone()));
        }

        if self.heuristic_mode {
            return self.resolver_heuristica(historial);
        }

        let mut respuestas = self.scripted_responses.lock().unwrap();
        if respuestas.is_empty() {
            return Err(DomainError::AgentError(
                "MockAgentRunner se quedó sin respuestas programadas".to_string(),
            ));
        }

        Ok(respuestas.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_agent_runner_heuristica_detecta_catalogo() {
        let runner = MockAgentRunner::new_heuristic();
        let historial = vec![AgentMessage::user(
            "Por favor muéstrame el catálogo de normas",
        )];
        let tools = ToolDefinition::lakehouse_tools();

        let output = runner
            .ejecutar_turno(ClaudeModel::Sonnet, "prompt", &historial, &tools)
            .await
            .unwrap();

        match output {
            AgentTurnOutput::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "inspect_lakehouse_catalog");
            }
            _ => panic!("Se esperaba llamada a herramienta"),
        }
    }

    #[tokio::test]
    async fn test_mock_agent_runner_error_programado() {
        let runner = MockAgentRunner::with_error("Fallo de conexión simulado");
        let historial = vec![AgentMessage::user("test")];
        let err = runner
            .ejecutar_turno(ClaudeModel::Haiku, "", &historial, &[])
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::AgentError(_)));
        assert_eq!(
            err.to_string(),
            "Error en agente o modelo de lenguaje: 'Fallo de conexión simulado'"
        );
    }
}
