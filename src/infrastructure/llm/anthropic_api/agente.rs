//! `AgentRunnerPort` por la API de Messages (Fase 5.2): tool use nativo, streaming SSE en todos
//! los pasos y prompt caching. Contrato: `docs/arquitectura/ssd_pieza5_fase52_agente_api.md`.
//!
//! El orquestador conserva el bucle, el arnés y el historial (`AgentMessage`). Este adaptador solo
//! traduce cada paso a una petición y guarda lo que el historial no puede llevar: el contenido
//! original de cada turno de tools, con sus bloques `thinking` firmados, para devolverlo intacto.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{Value, json};

use super::cliente::{AnthropicHttpClient, RespuestaEnFlujo};
use super::mensajes::{
    BloqueSistema, ControlCache, DefinicionTool, EleccionTool, MensajeEntrada, PeticionMensajes,
    TARIFA_HAIKU_5_5, TARIFA_SONNET_5_5, TarifaModelo,
};
use crate::application::dtos::agent::{
    AgentMessage, AgentRole, AgentTurnOutput, ClaudeModel, TurnoDirigido,
};
use crate::application::dtos::tools::{LakehouseToolName, ToolCall, ToolDefinition};
use crate::application::ports::agent_runner::{AgentRunnerPort, ObservadorTexto};
use crate::domain::errors::DomainError;

pub const MODELO_AGENTE_API: &str = "claude-sonnet-5-5";
/// El pensamiento cuenta para `max_tokens`, y un corte nunca vale
pub const MAX_TOKENS_AGENTE: u32 = 32_000;
/// Comprobación del historial de los bloques `thinking`: descartar en vez de dar un 400
pub const BETA_THINKING_BINDING: &str = "thinking-binding-controls-2026-08-01";
/// Turnos de tools cuyo contenido original se recuerda (todas las conversaciones)
const TURNOS_RECORDADOS: usize = 256;
const RESULTADO_VACIO: &str = "(sin contenido)";

/// Métricas de una llamada a la API (un paso del bucle del agente). Sin texto del operador ni
/// secretos: es telemetría
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetricaPasoApi {
    pub modelo: String,
    pub ttft_ms: Option<u64>,
    pub latencia_ms: u64,
    pub intentos: u32,
    pub herramientas: Vec<String>,
    pub stop_reason: Option<String>,
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub output_tokens: u64,
    /// `None` si el modelo no tiene tarifa registrada
    pub coste_usd: Option<f64>,
    pub bloques_thinking_descartados: usize,
    pub dirigido: bool,
}

/// Lo que el historial del orquestador no guarda de un turno de tools
#[derive(Debug, Clone)]
struct TurnoGuardado {
    ids: Vec<String>,
    contenido: Vec<Value>,
    /// Directiva de sistema que precedió al turno dirigido: se vuelve a enviar en su sitio
    directiva: Option<String>,
}

pub struct AnthropicMessagesRunner {
    cliente: AnthropicHttpClient,
    max_tokens: u32,
    turnos: Mutex<VecDeque<TurnoGuardado>>,
    metricas: Mutex<Vec<MetricaPasoApi>>,
}

impl AnthropicMessagesRunner {
    pub fn new(cliente: AnthropicHttpClient) -> Self {
        Self {
            cliente,
            max_tokens: MAX_TOKENS_AGENTE,
            turnos: Mutex::new(VecDeque::new()),
            metricas: Mutex::new(Vec::new()),
        }
    }

    /// Devuelve las métricas acumuladas desde la última llamada y vacía el registro
    pub fn tomar_metricas(&self) -> Vec<MetricaPasoApi> {
        self.metricas
            .lock()
            .map(|mut m| std::mem::take(&mut *m))
            .unwrap_or_default()
    }

    /// Identificador de la API: los alias de la CLI se traducen; un `claude-*` pasa tal cual
    pub fn modelo_api(modelo: &ClaudeModel) -> String {
        let nombre = modelo.cli_model_name();
        match nombre {
            "sonnet" | "default" => MODELO_AGENTE_API.to_string(),
            "haiku" => "claude-haiku-5-5".to_string(),
            "opus" => "claude-opus-5-5".to_string(),
            otro if otro.starts_with("claude-") => otro.to_string(),
            _ => MODELO_AGENTE_API.to_string(),
        }
    }

    /// El id que se envía: con el perfil de Anthropic se traducen los alias de la CLI; con un
    /// proxy, el id del desplegable va tal cual
    pub fn modelo_para(&self, modelo: &ClaudeModel) -> String {
        if self.cliente.perfil().alias_claude {
            Self::modelo_api(modelo)
        } else {
            modelo.cli_model_name().to_string()
        }
    }

    /// Coste del paso: desconocido (`None`) si el destino no tiene tarifas (un proxy) o si el
    /// modelo no tiene tarifa registrada. Nunca 0 por falta de datos. Con el perfil de Anthropic,
    /// igual que antes
    fn coste_de(&self, modelo: &str, mensaje: &super::flujo::MensajeEnFlujo) -> Option<f64> {
        if !self.cliente.perfil().tarifas {
            return None;
        }
        Self::tarifa(modelo).map(|t| mensaje.uso.coste_estimado_usd(&t))
    }

    fn tarifa(modelo: &str) -> Option<TarifaModelo> {
        match modelo {
            "claude-sonnet-5-5" => Some(TARIFA_SONNET_5_5),
            "claude-haiku-5-5" => Some(TARIFA_HAIKU_5_5),
            _ => None,
        }
    }

    pub fn directiva_dirigida(dirigida: LakehouseToolName) -> String {
        format!(
            "INSTRUCCIÓN DEL SISTEMA PARA ESTE TURNO: la aplicación exige consultar `{0}` antes de responder. Empieza tu respuesta llamando a `{0}` con los argumentos que correspondan a la pregunta del operador.",
            dirigida.as_str()
        )
    }

    /// Petición de un paso: el historial traducido, la directiva (si la hay) al final y los
    /// puntos de caché en `system`, en la última tool y en el último mensaje del historial
    pub fn construir_peticion(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
        directiva: Option<&str>,
    ) -> PeticionMensajes {
        let mut mensajes = self.mensajes_de(historial);
        marcar_cache_en_el_ultimo_bloque(&mut mensajes);
        if let Some(directiva) = directiva {
            mensajes.push(MensajeEntrada::sistema(directiva));
        }
        let ultima = herramientas.len().saturating_sub(1);
        PeticionMensajes {
            model: self.modelo_para(modelo),
            max_tokens: self.max_tokens,
            system: vec![BloqueSistema::texto(
                system_prompt,
                Some(ControlCache::efimera()),
            )],
            messages: mensajes,
            tools: herramientas
                .iter()
                .enumerate()
                .map(|(i, h)| DefinicionTool {
                    name: h.name.clone(),
                    description: h.description.clone(),
                    input_schema: h.parameters.clone(),
                    strict: false,
                    cache_control: (i == ultima).then(ControlCache::efimera),
                })
                .collect(),
            tool_choice: (!herramientas.is_empty()).then_some(EleccionTool::Auto),
            thinking: Some(json!({
                "type": "adaptive",
                "block_binding": { "prefix_mismatch_behavior": "drop_block" },
            })),
            stream: true,
            betas: vec![BETA_THINKING_BINDING.to_string()],
        }
    }

    /// Petición del aterrizaje forzoso: igual que un paso normal, pero el modelo no puede llamar
    /// a ninguna herramienta (`tool_choice: none`, admitido con thinking)
    pub fn construir_peticion_de_cierre(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
    ) -> PeticionMensajes {
        let mut peticion =
            self.construir_peticion(modelo, system_prompt, historial, herramientas, None);
        if !peticion.tools.is_empty() {
            peticion.tool_choice = Some(EleccionTool::None);
        }
        peticion
    }

    fn mensajes_de(&self, historial: &[AgentMessage]) -> Vec<MensajeEntrada> {
        let turnos = self.turnos.lock().map(|t| t.clone()).unwrap_or_default();
        let mut mensajes: Vec<(&'static str, Vec<Value>)> = Vec::new();
        for mensaje in historial {
            match mensaje.role {
                AgentRole::User => {
                    if !mensaje.content.trim().is_empty() {
                        anadir(&mut mensajes, "user", vec![bloque_texto(&mensaje.content)]);
                    }
                }
                AgentRole::Assistant => match &mensaje.tool_calls {
                    Some(llamadas) if !llamadas.is_empty() => {
                        let guardado = turno_de(&turnos, llamadas);
                        if let Some(directiva) = guardado.and_then(|t| t.directiva.as_deref()) {
                            mensajes.push(("system", vec![bloque_texto(directiva)]));
                        }
                        let contenido = guardado.map_or_else(
                            || contenido_reconstruido(&mensaje.content, llamadas),
                            |t| t.contenido.clone(),
                        );
                        anadir(&mut mensajes, "assistant", contenido);
                    }
                    _ => {
                        if !mensaje.content.trim().is_empty() {
                            anadir(
                                &mut mensajes,
                                "assistant",
                                vec![bloque_texto(&mensaje.content)],
                            );
                        }
                    }
                },
                AgentRole::Tool => {
                    let contenido = if mensaje.content.is_empty() {
                        RESULTADO_VACIO
                    } else {
                        &mensaje.content
                    };
                    anadir(
                        &mut mensajes,
                        "user",
                        vec![json!({
                            "type": "tool_result",
                            "tool_use_id": mensaje.tool_call_id.clone().unwrap_or_default(),
                            "content": contenido,
                        })],
                    );
                }
            }
        }
        mensajes
            .into_iter()
            .map(|(rol, bloques)| {
                if rol == "system" {
                    let texto = bloques
                        .first()
                        .and_then(|b| b["text"].as_str())
                        .unwrap_or_default()
                        .to_string();
                    MensajeEntrada::sistema(texto)
                } else {
                    MensajeEntrada::con_bloques(rol, bloques)
                }
            })
            .collect()
    }

    fn recordar(&self, turno: TurnoGuardado) {
        if let Ok(mut turnos) = self.turnos.lock() {
            turnos.push_back(turno);
            while turnos.len() > TURNOS_RECORDADOS {
                turnos.pop_front();
            }
        }
    }

    fn registrar_metrica(&self, respuesta: &RespuestaEnFlujo, modelo: &str, dirigido: bool) {
        let mensaje = &respuesta.mensaje;
        let metrica = MetricaPasoApi {
            modelo: modelo.to_string(),
            ttft_ms: respuesta.ttft.map(|t| t.as_millis() as u64),
            latencia_ms: respuesta.latencia.as_millis() as u64,
            intentos: respuesta.intentos,
            herramientas: llamadas_de(&mensaje.contenido)
                .iter()
                .map(|l| l.name.clone())
                .collect(),
            stop_reason: mensaje.stop_reason.clone(),
            input_tokens: mensaje.uso.input_tokens,
            cache_creation_input_tokens: mensaje.uso.cache_creation_input_tokens,
            cache_read_input_tokens: mensaje.uso.cache_read_input_tokens,
            output_tokens: mensaje.uso.output_tokens,
            coste_usd: self.coste_de(modelo, mensaje),
            bloques_thinking_descartados: mensaje.bloques_thinking_descartados,
            dirigido,
        };
        tracing::info!(
            modelo = %metrica.modelo,
            ttft_ms = metrica.ttft_ms,
            latencia_ms = metrica.latencia_ms,
            intentos = metrica.intentos,
            herramientas = ?metrica.herramientas,
            stop_reason = ?metrica.stop_reason,
            input_tokens = metrica.input_tokens,
            cache_creation_input_tokens = metrica.cache_creation_input_tokens,
            cache_read_input_tokens = metrica.cache_read_input_tokens,
            output_tokens = metrica.output_tokens,
            coste_usd = metrica.coste_usd,
            bloques_thinking_descartados = metrica.bloques_thinking_descartados,
            "paso del agente por la API de Messages"
        );
        if let Ok(mut metricas) = self.metricas.lock() {
            metricas.push(metrica);
        }
    }

    async fn paso(
        &self,
        peticion: &PeticionMensajes,
        observador: Option<&ObservadorTexto>,
        dirigido: bool,
    ) -> Result<RespuestaEnFlujo, DomainError> {
        let al_texto = |fragmento: &str| {
            if let Some(observador) = observador {
                observador(fragmento);
            }
        };
        let respuesta = self
            .cliente
            .crear_mensaje_en_flujo(peticion, &al_texto)
            .await?;
        self.registrar_metrica(&respuesta, &peticion.model, dirigido);
        Ok(respuesta)
    }

    /// `tool_use` → llamadas (y se recuerda el turno); si no, el texto de los bloques `text`
    fn interpretar(&self, respuesta: &RespuestaEnFlujo) -> AgentTurnOutput {
        let contenido = &respuesta.mensaje.contenido;
        let llamadas = llamadas_de(contenido);
        if llamadas.is_empty() {
            return AgentTurnOutput::FinalMessage(texto_de(contenido));
        }
        self.recordar(TurnoGuardado {
            ids: llamadas.iter().map(|l| l.id.clone()).collect(),
            contenido: contenido.clone(),
            directiva: None,
        });
        AgentTurnOutput::ToolCalls(llamadas)
    }
}

#[async_trait]
impl AgentRunnerPort for AnthropicMessagesRunner {
    async fn ejecutar_turno(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        let peticion =
            self.construir_peticion(modelo, system_prompt, historial, herramientas, None);
        let respuesta = self.paso(&peticion, None, false).await?;
        Ok(self.interpretar(&respuesta))
    }

    async fn ejecutar_turno_en_vivo(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
        observador: ObservadorTexto,
    ) -> Result<AgentTurnOutput, DomainError> {
        let peticion =
            self.construir_peticion(modelo, system_prompt, historial, herramientas, None);
        let respuesta = self.paso(&peticion, Some(&observador), false).await?;
        Ok(self.interpretar(&respuesta))
    }

    /// `auto` con la instrucción explícita en el turno (Sonnet 5.5 no admite `tool`). Solo la
    /// llamada a `dirigida` cuenta; las demás del mismo turno se descartan
    async fn ejecutar_turno_dirigido(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
        dirigida: LakehouseToolName,
    ) -> Result<TurnoDirigido, DomainError> {
        let directiva = Self::directiva_dirigida(dirigida);
        let peticion = self.construir_peticion(
            modelo,
            system_prompt,
            historial,
            herramientas,
            Some(&directiva),
        );
        let respuesta = self.paso(&peticion, None, true).await?;
        let contenido = &respuesta.mensaje.contenido;
        let Some(llamada) = llamadas_de(contenido)
            .into_iter()
            .find(|l| l.name == dirigida.as_str())
        else {
            return Ok(TurnoDirigido::SinLlamada);
        };
        let solo_la_dirigida: Vec<Value> = contenido
            .iter()
            .filter(|b| b["type"] != "tool_use" || b["id"] == llamada.id.as_str())
            .cloned()
            .collect();
        self.recordar(TurnoGuardado {
            ids: vec![llamada.id.clone()],
            contenido: solo_la_dirigida,
            directiva: Some(directiva),
        });
        Ok(TurnoDirigido::Llamada(llamada))
    }

    /// `tool_choice: none` con el mismo catálogo: sin `tools` la API rechaza un historial con
    /// `tool_use`, y cambiar `tool_choice` no invalida la caché de `tools` y `system`
    async fn ejecutar_turno_de_cierre(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        let peticion =
            self.construir_peticion_de_cierre(modelo, system_prompt, historial, herramientas);
        let respuesta = self.paso(&peticion, None, false).await?;
        Ok(self.interpretar(&respuesta))
    }

    fn necesita_correccion_de_omision(&self) -> bool {
        true
    }
}

fn bloque_texto(texto: &str) -> Value {
    json!({ "type": "text", "text": texto })
}

/// Mensajes seguidos del mismo rol van en uno: los `tool_result` de un paso juntos, y el texto
/// de un recordatorio tras ellos
fn anadir(mensajes: &mut Vec<(&'static str, Vec<Value>)>, rol: &'static str, bloques: Vec<Value>) {
    match mensajes.last_mut() {
        Some((ultimo, existentes)) if *ultimo == rol => existentes.extend(bloques),
        _ => mensajes.push((rol, bloques)),
    }
}

fn turno_de<'a>(
    turnos: &'a VecDeque<TurnoGuardado>,
    llamadas: &[ToolCall],
) -> Option<&'a TurnoGuardado> {
    let ids: Vec<&str> = llamadas.iter().map(|l| l.id.as_str()).collect();
    turnos
        .iter()
        .rev()
        .find(|t| t.ids.iter().map(String::as_str).eq(ids.iter().copied()))
}

fn contenido_reconstruido(texto: &str, llamadas: &[ToolCall]) -> Vec<Value> {
    let mut bloques = Vec::new();
    if !texto.trim().is_empty() {
        bloques.push(bloque_texto(texto));
    }
    bloques.extend(
        llamadas.iter().map(
            |l| json!({ "type": "tool_use", "id": l.id, "name": l.name, "input": l.arguments }),
        ),
    );
    bloques
}

fn marcar_cache_en_el_ultimo_bloque(mensajes: &mut [MensajeEntrada]) {
    use super::mensajes::ContenidoEntrada;
    if let Some(ContenidoEntrada::Bloques(bloques)) = mensajes
        .iter_mut()
        .rev()
        .find(|m| m.role != "system")
        .map(|m| &mut m.content)
        && let Some(Value::Object(ultimo)) = bloques.last_mut()
    {
        ultimo.insert("cache_control".into(), json!({ "type": "ephemeral" }));
    }
}

fn llamadas_de(contenido: &[Value]) -> Vec<ToolCall> {
    contenido
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| ToolCall {
            id: b["id"].as_str().unwrap_or_default().to_string(),
            name: b["name"].as_str().unwrap_or_default().to_string(),
            arguments: b["input"].clone(),
        })
        .collect()
}

fn texto_de(contenido: &[Value]) -> String {
    contenido
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::super::cliente::{AnthropicHttpClient, ConfigClienteAnthropic};
    use super::super::secreto::SecretApiKey;
    use super::*;
    use crate::application::dtos::tools::ToolCall;

    fn runner() -> AnthropicMessagesRunner {
        AnthropicMessagesRunner::new(
            AnthropicHttpClient::new(
                SecretApiKey::new("sk-ant-test-NO-ES-REAL-0000000000").unwrap(),
                ConfigClienteAnthropic::procedencia(),
            )
            .unwrap(),
        )
    }

    fn historial_con_tool_use() -> Vec<AgentMessage> {
        vec![
            AgentMessage::user("¿Cuántas filas hay?"),
            AgentMessage::assistant_with_tools(vec![ToolCall {
                id: "c1".into(),
                name: "execute_athena_sql".into(),
                arguments: json!({ "sql_query": "SELECT 1" }),
            }]),
            AgentMessage::tool_result("c1", "{\"filas\":42}"),
        ]
    }

    #[test]
    fn test_cierre_mantiene_el_catalogo_y_prohibe_llamar() {
        let runner = runner();
        let modelo = ClaudeModel::sonnet();
        let herramientas = ToolDefinition::lakehouse_tools();
        let historial = historial_con_tool_use();
        let normal = runner.construir_peticion(&modelo, "s", &historial, &herramientas, None);
        let cierre = runner.construir_peticion_de_cierre(&modelo, "s", &historial, &herramientas);
        let cuerpo = serde_json::to_value(&cierre).unwrap();
        assert_eq!(cuerpo["tool_choice"], json!({ "type": "none" }));
        // Mismo `tools` y `system` que el paso anterior: la caché de ese prefijo se conserva
        assert_eq!(
            serde_json::to_value(&normal.tools).unwrap(),
            serde_json::to_value(&cierre.tools).unwrap()
        );
        assert_eq!(normal.system, cierre.system);
        assert_eq!(normal.thinking, cierre.thinking);
    }

    #[test]
    fn test_cierre_sin_catalogo_no_envia_tool_choice() {
        let cierre = runner().construir_peticion_de_cierre(
            &ClaudeModel::sonnet(),
            "s",
            &[AgentMessage::user("Hola")],
            &[],
        );
        assert!(cierre.tool_choice.is_none());
    }
}
