//! Nivel 2 de procedencia por la API de Messages (Fase 5.1): un turno con Claude Haiku 5.5, la
//! tool del esquema cerrado forzada y prompt caching. Solo propone trazas; las comprueba
//! `arnes_turno::procedencia` en Rust.

use async_trait::async_trait;
use serde_json::Value;

use super::cliente::{AnthropicHttpClient, RespuestaConMetricas};
use super::error::ErrorApiAnthropic;
use super::mensajes::{
    BloqueSistema, ControlCache, DefinicionTool, EleccionTool, MensajeEntrada, PeticionMensajes,
    RespuestaMensajes, TARIFA_HAIKU_5_5,
};
use crate::application::dtos::procedencia::{SolicitudProcedencia, TrazasPropuestas};
use crate::application::ports::verificador_procedencia::VerificadorProcedenciaPort;
use crate::domain::errors::DomainError;
use crate::infrastructure::llm::claude_headless_procedencia::ClaudeHeadlessProcedencia;

/// Identificador fijo, sin alias (SSD de la Fase 5.1 §1)
pub const MODELO_PROCEDENCIA_API: &str = "claude-haiku-5-5";
pub const TOOL_TRAZAS: &str = "emitir_trazas_procedencia";
/// Holgado: el pensamiento adaptativo cuenta para `max_tokens` y un corte nunca vale
const MAX_TOKENS_PROCEDENCIA: u32 = 4096;
const DESCRIPCION_TOOL: &str = "Devuelve, para cada cifra pedida, la traza de procedencia que la respalda. Es la única forma de responder.";
const PROMPT_PROCEDENCIA_API: &str = "Eres un comprobador de procedencia. Recibes en JSON una pregunta, las salidas de herramientas de un turno (cada una con su id t0, t1…) y un borrador de respuesta. Para cada cifra de la lista `cifras`, indica de dónde sale con uno de los 4 tipos del esquema:
- formato_o_escala: la misma cantidad escrita de otro modo (separadores, decimales) o en otra potencia de diez (unidades). Se cumple cifra = valor de `fuente` × 10^factor_potencia_diez. El factor es negativo si la cifra es menor que la fuente (2000 g → 2 kg: −3), positivo si es mayor (0,5 km → 500 m: 3) y 0 si solo cambia la escritura;
- aritmetica: el resultado de una operación declarada (suma, resta, producto, division, porcentaje) sobre operandos que aparecen en los textos;
- referencia_estructural: un número de artículo, apartado, página, fecha u otra referencia que aparece en una subcadena de los textos;
- conteo_elementos: el número de elementos distintos que se enumeran en los textos.
Reglas:
1. En `cifra`, copia la cifra tal como aparece en la lista `cifras`.
2. Cada `texto` de un literal debe copiarse EXACTAMENTE, carácter a carácter, de su `origen`: el id de una salida (t0, t1…) o \"pregunta\". No lo reformatees ni cambies separadores.
3. Si una cifra no sale de esos textos, no propongas traza para ella. No inventes operandos.
Responde solo llamando a la herramienta.";

/// Métricas de la última llamada, para la calibración
pub type ObservadorMetricas = Box<dyn Fn(&RespuestaConMetricas) + Send + Sync>;

pub struct AnthropicProcedenciaVerifier {
    cliente: AnthropicHttpClient,
    modelo: String,
    observador: Option<ObservadorMetricas>,
    /// `tool_choice: auto` (por defecto) para que el modelo piense antes de llamar: con la tool
    /// forzada, Claude Haiku 5.5 responde sin bloque de pensamiento y en la calibración 5.1
    /// invertía el signo del factor de escala. Sin llamada, falla cerrada igual
    razonamiento_previo: bool,
}

impl AnthropicProcedenciaVerifier {
    pub fn new(cliente: AnthropicHttpClient) -> Self {
        Self {
            cliente,
            modelo: MODELO_PROCEDENCIA_API.into(),
            observador: None,
            razonamiento_previo: true,
        }
    }

    /// `tool_choice` forzado a la tool de trazas: más rápido y barato, sin pensamiento previo
    pub fn con_tool_forzada(mut self) -> Self {
        self.razonamiento_previo = false;
        self
    }

    fn eleccion_tool(&self) -> EleccionTool {
        if self.razonamiento_previo {
            EleccionTool::Auto
        } else {
            EleccionTool::Tool {
                name: TOOL_TRAZAS.into(),
            }
        }
    }

    /// Recibe la respuesta y sus métricas tras cada llamada válida (latencia, tokens, intentos)
    pub fn con_observador(mut self, observador: ObservadorMetricas) -> Self {
        self.observador = Some(observador);
        self
    }

    /// El esquema cerrado del adaptador CLI adaptado a `strict`: `oneOf` → `anyOf` y sin
    /// `minimum`, `maximum` ni `maxItems`, que la API no admite y que Rust ya comprueba
    pub fn esquema_estricto() -> Value {
        adaptar_a_strict(ClaudeHeadlessProcedencia::esquema())
    }

    pub fn construir_peticion(
        &self,
        solicitud: &SolicitudProcedencia,
    ) -> Result<PeticionMensajes, DomainError> {
        let entrada = serde_json::to_string_pretty(solicitud).map_err(|e| {
            DomainError::AgentError(format!("No se pudo serializar la solicitud: {e}"))
        })?;
        Ok(PeticionMensajes {
            model: self.modelo.clone(),
            max_tokens: MAX_TOKENS_PROCEDENCIA,
            system: vec![BloqueSistema::texto(
                PROMPT_PROCEDENCIA_API,
                Some(ControlCache::efimera()),
            )],
            messages: vec![MensajeEntrada::usuario(entrada)],
            tools: vec![DefinicionTool {
                name: TOOL_TRAZAS.into(),
                description: DESCRIPCION_TOOL.into(),
                input_schema: Self::esquema_estricto(),
                strict: true,
                cache_control: Some(ControlCache::efimera()),
            }],
            tool_choice: Some(self.eleccion_tool()),
            ..Default::default()
        })
    }

    /// La entrada de la tool forzada, con el esquema cerrado. Cualquier otra cosa falla cerrada
    pub fn interpretar_respuesta(
        respuesta: &RespuestaMensajes,
    ) -> Result<TrazasPropuestas, ErrorApiAnthropic> {
        let entrada = respuesta.entrada_de_tool(TOOL_TRAZAS).ok_or_else(|| {
            ErrorApiAnthropic::RespuestaNoValida(
                "la respuesta no trae la tool de trazas de procedencia".into(),
            )
        })?;
        serde_json::from_value(entrada.clone()).map_err(|e| {
            ErrorApiAnthropic::RespuestaNoValida(format!("trazas fuera del esquema: {e}"))
        })
    }
}

/// Recorre el esquema y quita lo que `strict` no admite
fn adaptar_a_strict(valor: Value) -> Value {
    match valor {
        Value::Object(campos) => Value::Object(
            campos
                .into_iter()
                .filter(|(clave, _)| !matches!(clave.as_str(), "minimum" | "maximum" | "maxItems"))
                .map(|(clave, contenido)| {
                    let clave = if clave == "oneOf" {
                        "anyOf".to_string()
                    } else {
                        clave
                    };
                    (clave, adaptar_a_strict(contenido))
                })
                .collect(),
        ),
        Value::Array(elementos) => {
            Value::Array(elementos.into_iter().map(adaptar_a_strict).collect())
        }
        otro => otro,
    }
}

#[async_trait]
impl VerificadorProcedenciaPort for AnthropicProcedenciaVerifier {
    async fn proponer_trazas(
        &self,
        solicitud: &SolicitudProcedencia,
    ) -> Result<TrazasPropuestas, DomainError> {
        let peticion = self.construir_peticion(solicitud)?;
        let resultado = self.cliente.crear_mensaje(&peticion).await?;
        let uso = resultado.respuesta.usage;
        tracing::info!(
            latencia_ms = resultado.latencia.as_millis() as u64,
            intentos = resultado.intentos,
            input_tokens = uso.input_tokens,
            cache_creation_input_tokens = uso.cache_creation_input_tokens,
            cache_read_input_tokens = uso.cache_read_input_tokens,
            output_tokens = uso.output_tokens,
            coste_estimado_usd = uso.coste_estimado_usd(&TARIFA_HAIKU_5_5),
            "procedencia por la API de Anthropic"
        );
        if let Some(observador) = &self.observador {
            observador(&resultado);
        }
        Ok(Self::interpretar_respuesta(&resultado.respuesta)?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::json;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::application::dtos::procedencia::{SalidaTurno, TipoTraza};
    use crate::infrastructure::llm::anthropic_api::cliente::{
        ConfigClienteAnthropic, PoliticaReintentos,
    };
    use crate::infrastructure::llm::anthropic_api::secreto::SecretApiKey;

    fn solicitud() -> SolicitudProcedencia {
        SolicitudProcedencia {
            pregunta: "¿Cuánto suman A y B?".into(),
            salidas: vec![SalidaTurno {
                id: "t0".into(),
                herramienta: "lookup".into(),
                texto: "A = 1.250,5\nB = 749,5".into(),
            }],
            borrador: "Suman 2.000.".into(),
            cifras: vec!["2.000".into()],
        }
    }

    fn verificador(servidor: &MockServer) -> AnthropicProcedenciaVerifier {
        let config = ConfigClienteAnthropic {
            reintentos: PoliticaReintentos {
                espera_base: Duration::from_millis(1),
                espera_maxima: Duration::from_millis(2),
                ..PoliticaReintentos::default()
            },
            ..ConfigClienteAnthropic::procedencia().con_base_url(servidor.uri())
        };
        let cliente =
            AnthropicHttpClient::new(SecretApiKey::new("sk-ant-falsa").unwrap(), config).unwrap();
        AnthropicProcedenciaVerifier::new(cliente)
    }

    fn respuesta_con(content: Value) -> Value {
        json!({ "id": "msg", "model": MODELO_PROCEDENCIA_API, "stop_reason": "tool_use",
                "content": content, "usage": { "input_tokens": 1, "output_tokens": 1 } })
    }

    fn traza_suma() -> Value {
        json!({ "trazas": [{ "cifra": "2.000", "tipo": { "tipo": "aritmetica", "operacion": "suma",
            "operandos": [ { "origen": "t0", "texto": "1.250,5" }, { "origen": "t0", "texto": "749,5" } ] } }] })
    }

    #[test]
    fn test_peticion_con_tool_forzada_cache_y_sin_temperature() {
        let servidor_falso = "http://127.0.0.1:9";
        let cliente = AnthropicHttpClient::new(
            SecretApiKey::new("sk-ant-falsa").unwrap(),
            ConfigClienteAnthropic::procedencia().con_base_url(servidor_falso),
        )
        .unwrap();
        let peticion = AnthropicProcedenciaVerifier::new(cliente)
            .con_tool_forzada()
            .construir_peticion(&solicitud())
            .unwrap();
        let cuerpo = serde_json::to_value(&peticion).unwrap();
        assert_eq!(cuerpo["model"], "claude-haiku-5-5");
        assert_eq!(
            cuerpo["tool_choice"],
            json!({ "type": "tool", "name": TOOL_TRAZAS })
        );
        assert_eq!(
            cuerpo["system"][0]["cache_control"],
            json!({ "type": "ephemeral" })
        );
        assert_eq!(cuerpo["tools"].as_array().unwrap().len(), 1);
        assert_eq!(cuerpo["tools"][0]["name"], TOOL_TRAZAS);
        assert_eq!(
            cuerpo["tools"][0]["cache_control"],
            json!({ "type": "ephemeral" })
        );
        assert_eq!(cuerpo["tools"][0]["strict"], true);
        for prohibido in ["temperature", "top_p", "top_k"] {
            assert!(cuerpo.get(prohibido).is_none(), "{prohibido}");
        }
        let mensaje: SolicitudProcedencia =
            serde_json::from_str(cuerpo["messages"][0]["content"].as_str().unwrap()).unwrap();
        assert_eq!(mensaje, solicitud());
    }

    /// Con razonamiento previo, `auto` deja pensar a Haiku antes de llamar (la tool forzada se lo
    /// salta); la falta de llamada sigue fallando cerrada
    #[tokio::test]
    async fn test_por_defecto_razona_antes_con_auto_y_sin_llamada_falla_cerrado() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg", "model": MODELO_PROCEDENCIA_API, "stop_reason": "end_turn",
                "content": [{ "type": "text", "text": "No hay trazas." }], "usage": {} })))
            .mount(&servidor)
            .await;
        let verificador = verificador(&servidor);
        let cuerpo =
            serde_json::to_value(verificador.construir_peticion(&solicitud()).unwrap()).unwrap();
        assert_eq!(cuerpo["tool_choice"], json!({ "type": "auto" }));
        assert_eq!(cuerpo["tools"][0]["strict"], true);
        assert!(verificador.proponer_trazas(&solicitud()).await.is_err());
    }

    /// Calibración 5.1: sin la fórmula, Haiku invertía el signo del factor en los cambios de unidad
    #[test]
    fn test_el_prompt_define_la_formula_y_el_signo_del_factor() {
        for tipo in [
            "formato_o_escala",
            "aritmetica",
            "referencia_estructural",
            "conteo_elementos",
        ] {
            assert!(PROMPT_PROCEDENCIA_API.contains(tipo), "{tipo}");
        }
        assert!(PROMPT_PROCEDENCIA_API.contains("× 10^factor_potencia_diez"));
        assert!(PROMPT_PROCEDENCIA_API.contains("negativo"));
        assert!(PROMPT_PROCEDENCIA_API.contains("positivo"));
    }

    #[test]
    fn test_esquema_estricto_conserva_los_4_tipos_sin_lo_no_admitido() {
        let esquema = AnthropicProcedenciaVerifier::esquema_estricto();
        let texto = esquema.to_string();
        for prohibido in ["\"oneOf\"", "\"minimum\"", "\"maximum\"", "\"maxItems\""] {
            assert!(!texto.contains(prohibido), "{prohibido}");
        }
        let tipos = esquema["properties"]["trazas"]["items"]["properties"]["tipo"]["anyOf"]
            .as_array()
            .unwrap();
        assert_eq!(tipos.len(), 4);
        assert_eq!(esquema["additionalProperties"], false);
        // El esquema de la CLI no cambia
        assert!(
            ClaudeHeadlessProcedencia::esquema()
                .to_string()
                .contains("\"oneOf\"")
        );
    }

    #[tokio::test]
    async fn test_tool_use_conforme_da_trazas() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(respuesta_con(json!([
                    { "type": "thinking", "thinking": "", "signature": "s" },
                    { "type": "tool_use", "id": "tu", "name": TOOL_TRAZAS, "input": traza_suma() },
                ]))),
            )
            .mount(&servidor)
            .await;
        let vistos = Arc::new(Mutex::new(0u32));
        let contador = vistos.clone();
        let verificador = verificador(&servidor)
            .con_observador(Box::new(move |_| *contador.lock().unwrap() += 1));
        let trazas = verificador.proponer_trazas(&solicitud()).await.unwrap();
        assert_eq!(trazas.trazas.len(), 1);
        assert!(matches!(
            trazas.trazas[0].tipo,
            TipoTraza::Aritmetica { .. }
        ));
        assert_eq!(*vistos.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_sin_tool_otra_tool_o_fuera_del_esquema_falla_cerrado() {
        let casos = [
            json!([{ "type": "text", "text": "La cifra es correcta." }]),
            json!([{ "type": "tool_use", "id": "tu", "name": "otra", "input": traza_suma() }]),
            json!([{ "type": "tool_use", "id": "tu", "name": TOOL_TRAZAS,
                     "input": { "trazas": [{ "cifra": "2.000", "tipo": { "tipo": "aprobada" } }] } }]),
            json!([{ "type": "tool_use", "id": "tu", "name": TOOL_TRAZAS,
                     "input": { "trazas": [{ "cifra": "2.000", "tipo": { "tipo": "aritmetica",
                        "operacion": "suma", "operandos": [], "aprobada": true } }] } }]),
        ];
        for content in casos {
            let servidor = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(respuesta_con(content.clone())),
                )
                .mount(&servidor)
                .await;
            assert!(
                verificador(&servidor)
                    .proponer_trazas(&solicitud())
                    .await
                    .is_err(),
                "{content}"
            );
        }
    }

    #[tokio::test]
    async fn test_max_tokens_y_errores_de_la_api_fallan_cerrado() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "msg", "model": MODELO_PROCEDENCIA_API, "stop_reason": "max_tokens",
                "content": [{ "type": "tool_use", "id": "tu", "name": TOOL_TRAZAS, "input": traza_suma() }],
                "usage": {} })))
            .mount(&servidor)
            .await;
        let error = verificador(&servidor)
            .proponer_trazas(&solicitud())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("max_tokens"), "{error}");

        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "type": "error", "error": { "type": "authentication_error", "message": "x" } })))
            .mount(&servidor)
            .await;
        let error = verificador(&servidor)
            .proponer_trazas(&solicitud())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("ANTHROPIC_API_KEY"), "{error}");
    }
}
