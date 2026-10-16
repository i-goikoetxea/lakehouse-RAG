//! Orquestador del ciclo de vida del agente (bucle de razonamiento y herramientas)

use std::sync::Arc;

use crate::application::dtos::agent::{
    AgentExecutionEvent, AgentMessage, AgentTurnOutput, ClaudeModel,
};
use crate::application::ports::agent_runner::AgentRunnerPort;
use crate::application::ports::tool_server::ToolServerPort;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

/// Callback asíncrono/síncrono para streaming y emisión de eventos en UI
pub type EventCallback = Arc<dyn Fn(AgentExecutionEvent) + Send + Sync>;

/// Orquestador que ejecuta el bucle determinista de herramientas hasta respuesta final
pub struct AgentOrchestrator {
    runner: Arc<dyn AgentRunnerPort>,
    tools: Arc<dyn ToolServerPort>,
    max_steps: usize,
}

impl AgentOrchestrator {
    pub const DEFAULT_MAX_STEPS: usize = 10;

    pub fn new(runner: Arc<dyn AgentRunnerPort>, tools: Arc<dyn ToolServerPort>) -> Self {
        Self {
            runner,
            tools,
            max_steps: Self::DEFAULT_MAX_STEPS,
        }
    }

    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Construye el System Prompt base inyectando el aislamiento de centro y rigor de citas
    pub fn build_system_prompt(centro_id: &CenterId) -> String {
        format!(
            "Eres el Asistente Técnico y Normativo del Lakehouse Industrial.\n\
            Estás operando exclusivamente sobre el Centro de Conocimiento activo: '{0}'.\n\
            REGLAS INNEGOCIABLES:\n\
            1. No inventes tolerancias, valores nominales ni normas vigentes.\n\
            2. Usa las herramientas provistas ('inspect_lakehouse_catalog', 'query_operational_matrix', \
            'lookup_double_entry_table', 'execute_athena_sql', 'traverse_normative_graph') para consultar los datos.\n\
            3. Al responder, cita siempre el 'doc_id', la sección y si la norma es prevalente o derogada.\n\
            4. Si una tabla de doble entrada contiene notas al pie, inclúyelas explícitamente en tu dictamen.\n\
            5. Mantén un tono técnico, formal y orientado a la ingeniería de procesos.",
            centro_id.as_str()
        )
    }

    /// Ejecuta el bucle de turnos hasta obtener una respuesta final o agotar max_steps
    pub async fn procesar_turno(
        &self,
        modelo: ClaudeModel,
        centro_id: &CenterId,
        historial: &mut Vec<AgentMessage>,
        on_event: Option<EventCallback>,
    ) -> Result<String, DomainError> {
        let system_prompt = Self::build_system_prompt(centro_id);
        let herramientas_disponibles = self.tools.listar_herramientas();

        let mut paso = 0;
        while paso < self.max_steps {
            paso += 1;

            let turn_output = self
                .runner
                .ejecutar_turno(modelo, &system_prompt, historial, &herramientas_disponibles)
                .await?;

            match turn_output {
                AgentTurnOutput::FinalMessage(respuesta) => {
                    if let Some(cb) = &on_event {
                        cb(AgentExecutionEvent::Completed(respuesta.clone()));
                    }
                    historial.push(AgentMessage::assistant(respuesta.clone()));
                    return Ok(respuesta);
                }
                AgentTurnOutput::ToolCalls(calls) => {
                    if calls.is_empty() {
                        return Err(DomainError::AgentError(
                            "El modelo emitió un turno de herramientas sin ninguna llamada"
                                .to_string(),
                        ));
                    }

                    // Registramos la intención del asistente en el historial
                    historial.push(AgentMessage::assistant_with_tools(calls.clone()));

                    for call in &calls {
                        if let Some(cb) = &on_event {
                            cb(AgentExecutionEvent::CallingTool {
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            });
                        }

                        let result = self.tools.ejecutar_herramienta(call).await?;

                        if let Some(cb) = &on_event {
                            cb(AgentExecutionEvent::ToolResultReceived {
                                name: result.tool_name.clone(),
                                result: result.output.clone(),
                                is_error: result.is_error,
                            });
                        }

                        historial.push(AgentMessage::tool_result(
                            call.id.clone(),
                            result.output.to_string(),
                        ));
                    }
                }
            }
        }

        Err(DomainError::AgentError(format!(
            "Se superó el límite máximo de iteraciones de herramientas ({}) sin alcanzar una conclusión",
            self.max_steps
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dtos::tools::*;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::Mutex;

    struct DummyToolServer;

    #[async_trait]
    impl ToolServerPort for DummyToolServer {
        fn listar_herramientas(&self) -> Vec<ToolDefinition> {
            ToolDefinition::lakehouse_tools()
        }

        async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
            Ok(ToolResult {
                call_id: call.id.clone(),
                tool_name: call.name.clone(),
                output: json!({ "encontrado": true, "doc_id": "DOC-TEST-001" }),
                is_error: false,
            })
        }
    }

    struct MockRunnerScripted {
        respuestas: Mutex<Vec<AgentTurnOutput>>,
    }

    #[async_trait]
    impl AgentRunnerPort for MockRunnerScripted {
        async fn ejecutar_turno(
            &self,
            _modelo: ClaudeModel,
            _system_prompt: &str,
            _historial: &[AgentMessage],
            _herramientas: &[ToolDefinition],
        ) -> Result<AgentTurnOutput, DomainError> {
            let mut guard = self.respuestas.lock().unwrap();
            if guard.is_empty() {
                return Err(DomainError::AgentError(
                    "Sin más respuestas configuradas".to_string(),
                ));
            }
            Ok(guard.remove(0))
        }
    }

    #[tokio::test]
    async fn test_agent_orchestrator_flujo_exitoso_con_herramienta() {
        let runner = Arc::new(MockRunnerScripted {
            respuestas: Mutex::new(vec![
                AgentTurnOutput::ToolCalls(vec![ToolCall {
                    id: "call_abc".to_string(),
                    name: "inspect_lakehouse_catalog".to_string(),
                    arguments: json!({ "centro_id": "planta_1" }),
                }]),
                AgentTurnOutput::FinalMessage(
                    "El documento DOC-TEST-001 está vigente.".to_string(),
                ),
            ]),
        });

        let tools = Arc::new(DummyToolServer);
        let orchestrator = AgentOrchestrator::new(runner, tools);

        let centro = CenterId::new("planta_1").unwrap();
        let mut historial = vec![AgentMessage::user("¿Qué documentos existen?")];

        let res = orchestrator
            .procesar_turno(ClaudeModel::Sonnet, &centro, &mut historial, None)
            .await
            .unwrap();

        assert_eq!(res, "El documento DOC-TEST-001 está vigente.");
        // historial: [user, assistant_tools, tool_result, assistant_final]
        assert_eq!(historial.len(), 4);
    }

    #[tokio::test]
    async fn test_agent_orchestrator_limite_max_steps_previene_bucle_infinito() {
        let runner = Arc::new(MockRunnerScripted {
            respuestas: Mutex::new(vec![
                AgentTurnOutput::ToolCalls(vec![ToolCall {
                    id: "call_1".to_string(),
                    name: "inspect_lakehouse_catalog".to_string(),
                    arguments: json!({ "centro_id": "planta_1" }),
                }]),
                AgentTurnOutput::ToolCalls(vec![ToolCall {
                    id: "call_2".to_string(),
                    name: "inspect_lakehouse_catalog".to_string(),
                    arguments: json!({ "centro_id": "planta_1" }),
                }]),
            ]),
        });

        let tools = Arc::new(DummyToolServer);
        // Configuramos max_steps = 2
        let orchestrator = AgentOrchestrator::new(runner, tools).with_max_steps(2);

        let centro = CenterId::new("planta_1").unwrap();
        let mut historial = vec![AgentMessage::user("Bucle infinito")];

        let err = orchestrator
            .procesar_turno(ClaudeModel::Haiku, &centro, &mut historial, None)
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::AgentError(_)));
        assert!(err.to_string().contains("límite máximo de iteraciones"));
    }
}
