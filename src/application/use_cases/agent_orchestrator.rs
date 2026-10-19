//! Orquestador del ciclo de vida del agente (bucle de razonamiento y herramientas)

use std::sync::Arc;

use crate::application::dtos::agent::{
    AgentExecutionEvent, AgentMessage, AgentTurnOutput, ClaudeModel,
};
use crate::application::ports::agent_runner::{AgentRunnerPort, ObservadorTexto};
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

/// Tablas v2 que `execute_athena_sql` acepta por su nombre canónico (esquema §4)
pub const INVENTARIO_TABLAS_V2: &str = crate::application::dtos::tools::DESCRIPCION_VISTAS_SQL;

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

    /// System prompt v2 (plan v2 §1): aislamiento de centro, elección de herramienta, citas,
    /// notas al pie, condiciones de aplicación, prelación normativa y prohibición de inventar
    pub fn build_system_prompt(centro_id: &CenterId) -> String {
        format!(
            "Eres el Asistente Técnico y Normativo del Lakehouse Industrial.\n\
            Operas exclusivamente sobre el Centro de Conocimiento activo '{0}': pasa siempre centro_id = '{0}' a las herramientas.\n\
            ELECCIÓN DE HERRAMIENTA:\n\
            - Roles (quién ejecuta, quién es responsable o firma) y registros/formatos de una fase → query_operational_matrix (filtra por fase, rol con rol_en o registro). Cada fila es un hito: nunca combines el responsable de una fila con el registro de otra.\n\
            - Valores de una tabla de doble entrada (temperaturas, espesores, tolerancias…) → lookup_double_entry_table con tabla_id (o busqueda_tabla) y los valores de los ejes; si no conoces la tabla, consulta antes inspect_lakehouse_catalog.\n\
            - Listados exhaustivos, recuentos y agregaciones (todos los registros que firma un rol, cuántos procedimientos…) → execute_athena_sql (una sola SELECT/WITH; filtra vigente = true salvo que se pida otra fecha).\n\
            - Contexto, alcance, condiciones y texto normativo → search_normative_context.\n\
            - Relaciones entre documentos, derogaciones y remisiones → traverse_normative_graph.\n\
            - Qué documentos y tablas hay, su prelación y su vigencia → inspect_lakehouse_catalog.\n\
            VIGENCIA: sin fecha, responde con lo vigente. Si la pregunta se refiere a una fecha o a una revisión anterior, pasa fecha_referencia (AAAA-MM-DD) a las herramientas.\n\
            REGLAS INNEGOCIABLES:\n\
            1. No inventes valores, roles, registros ni normas: responde solo con lo que devuelven las herramientas. Si una herramienta devuelve sin_coincidencia, ambiguo o vacío, dilo explícitamente y no interpoles.\n\
            2. Cita siempre doc_id y revisión (y el apartado o breadcrumb si lo hay).\n\
            3. Si una celda trae nota_pie_condicion, inclúyela literalmente en la respuesta junto al valor (p. ej. «125 °C (*): …»).\n\
            4. Incluye siempre la condicion_aplicacion del dato (p. ej. «solo cuando el acopio supera 7 días»).\n\
            5. Prelación: 1 = especificación del cliente, 2 = procedimiento de obra, 3 = procedimiento general, 4 = instructivo técnico, 5 = norma o formato. Ante un conflicto prevalece el nivel menor (prevalece: true): responde con él y menciona el otro como supletorio.\n\
            6. Mantén un tono técnico, formal y orientado a la ingeniería de procesos.\n\
            {1}",
            centro_id.as_str(),
            INVENTARIO_TABLAS_V2
        )
    }

    /// Ejecuta el bucle de turnos hasta obtener una respuesta final o agotar max_steps
    pub async fn procesar_turno(
        &self,
        modelo: &ClaudeModel,
        centro_id: &CenterId,
        historial: &mut Vec<AgentMessage>,
        on_event: Option<EventCallback>,
    ) -> Result<String, DomainError> {
        let system_prompt = Self::build_system_prompt(centro_id);
        let herramientas_disponibles = self.tools.listar_herramientas();

        let mut paso = 0;
        while paso < self.max_steps {
            paso += 1;

            let turn_output = match &on_event {
                Some(cb) => {
                    let cb = Arc::clone(cb);
                    let observador: ObservadorTexto = Arc::new(move |fragmento: &str| {
                        cb(AgentExecutionEvent::TextDelta(fragmento.to_string()))
                    });
                    self.runner
                        .ejecutar_turno_en_vivo(
                            modelo,
                            &system_prompt,
                            historial,
                            &herramientas_disponibles,
                            observador,
                        )
                        .await?
                }
                None => {
                    self.runner
                        .ejecutar_turno(
                            modelo,
                            &system_prompt,
                            historial,
                            &herramientas_disponibles,
                        )
                        .await?
                }
            };

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
            _modelo: &ClaudeModel,
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

    /// Runner con streaming: emite la respuesta final en dos fragmentos
    struct RunnerEnVivo;

    #[async_trait]
    impl AgentRunnerPort for RunnerEnVivo {
        async fn ejecutar_turno(
            &self,
            _modelo: &ClaudeModel,
            _system_prompt: &str,
            _historial: &[AgentMessage],
            _herramientas: &[ToolDefinition],
        ) -> Result<AgentTurnOutput, DomainError> {
            Ok(AgentTurnOutput::FinalMessage("Hola mundo".to_string()))
        }

        async fn ejecutar_turno_en_vivo(
            &self,
            modelo: &ClaudeModel,
            system_prompt: &str,
            historial: &[AgentMessage],
            herramientas: &[ToolDefinition],
            observador: ObservadorTexto,
        ) -> Result<AgentTurnOutput, DomainError> {
            observador("Hola ");
            observador("mundo");
            self.ejecutar_turno(modelo, system_prompt, historial, herramientas)
                .await
        }
    }

    #[tokio::test]
    async fn test_el_texto_de_la_respuesta_llega_en_vivo_antes_de_completar() {
        let orquestador = AgentOrchestrator::new(Arc::new(RunnerEnVivo), Arc::new(DummyToolServer));
        let eventos: Arc<Mutex<Vec<AgentExecutionEvent>>> = Arc::default();
        let destino = eventos.clone();
        let mut historial = vec![AgentMessage::user("Hola")];
        orquestador
            .procesar_turno(
                &ClaudeModel::sonnet(),
                &CenterId::new("planta_1").unwrap(),
                &mut historial,
                Some(Arc::new(move |e| destino.lock().unwrap().push(e))),
            )
            .await
            .unwrap();
        assert_eq!(
            *eventos.lock().unwrap(),
            vec![
                AgentExecutionEvent::TextDelta("Hola ".into()),
                AgentExecutionEvent::TextDelta("mundo".into()),
                AgentExecutionEvent::Completed("Hola mundo".into()),
            ]
        );
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
            .procesar_turno(&ClaudeModel::sonnet(), &centro, &mut historial, None)
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
            .procesar_turno(
                &ClaudeModel::new("haiku").unwrap(),
                &centro,
                &mut historial,
                None,
            )
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::AgentError(_)));
        assert!(err.to_string().contains("límite máximo de iteraciones"));
    }

    #[test]
    fn test_system_prompt_v2_guia_herramientas_citas_notas_y_prelacion() {
        let centro = CenterId::new("obra-a").unwrap();
        let prompt = AgentOrchestrator::build_system_prompt(&centro);
        for nombre in crate::application::dtos::tools::LakehouseToolName::TODAS {
            assert!(prompt.contains(nombre.as_str()), "{}", nombre.as_str());
        }
        for regla in [
            "centro_id = 'obra-a'",
            "fecha_referencia",
            "doc_id y revisión",
            "nota_pie_condicion",
            "condicion_aplicacion",
            "supletorio",
            "No inventes",
        ] {
            assert!(prompt.contains(regla), "{regla}");
        }
    }
}
