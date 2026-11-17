//! Cliente base de `POST /v1/messages`: autenticación, timeouts, reintentos de transporte y
//! validación de `stop_reason` (SSD de la Fase 5.1 §3). Lo compartirán los adaptadores de
//! procedencia, agente y destilación.

use std::time::{Duration, Instant};

use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde_json::Value;

use futures::StreamExt;

use super::error::ErrorApiAnthropic;
use super::flujo::{AporteEvento, LectorSse, MensajeEnFlujo};
use super::mensajes::{ContenidoEntrada, PeticionMensajes, RespuestaMensajes};
use super::secreto::SecretApiKey;

pub const URL_BASE_POR_DEFECTO: &str = "https://api.anthropic.com";
/// Versión de la API fijada (cabecera `anthropic-version`)
pub const VERSION_API: &str = "2026-10-21";
/// Versión documentada por FreeLLMAPI para su endpoint compatible con Anthropic
pub const VERSION_API_FREELLMAPI: &str = "2023-06-01";
const LONGITUD_MAXIMA_DETALLE: usize = 400;
/// Tope de la respuesta de `GET /v1/models`: un catálogo legítimo ocupa unas decenas de KB
const TAMANO_MAXIMO_CATALOGO: usize = 1024 * 1024;

/// Qué admite el servidor de destino de la API de Messages. El cliente quita de cada petición lo
/// que el perfil no admite justo antes de enviarla, así que el runner y la procedencia no se
/// duplican (`docs/arquitectura/ssd_motor_freellmapi.md` §3.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerfilApi {
    /// Cabecera `anthropic-version`
    pub version_api: &'static str,
    pub thinking: bool,
    pub cache_control: bool,
    pub strict_tools: bool,
    /// Cabecera `anthropic-beta`
    pub betas: bool,
    /// Traducir los alias de la CLI (`sonnet` → `claude-sonnet-5-5`); si no, el id va tal cual
    pub alias_claude: bool,
    /// Hay tarifas publicadas: si no, el coste es desconocido (`None`), nunca 0
    pub tarifas: bool,
    /// Sin `stop_reason` la respuesta falla cerrada. Anthropic siempre lo envía (el modo API no
    /// cambia); un proxy puede omitirlo
    pub exige_stop_reason: bool,
}

impl PerfilApi {
    pub const ANTHROPIC: Self = Self {
        version_api: VERSION_API,
        thinking: true,
        cache_control: true,
        strict_tools: true,
        betas: true,
        alias_claude: true,
        tarifas: true,
        exige_stop_reason: false,
    };

    /// El proxy traduce a formato OpenAI: no documenta `thinking` ni `cache_control` y no
    /// reenvía `anthropic-beta`. Desactivados de serie (medido en el SSD §2)
    pub const FREELLMAPI: Self = Self {
        version_api: VERSION_API_FREELLMAPI,
        thinking: false,
        cache_control: false,
        strict_tools: false,
        betas: false,
        alias_claude: false,
        tarifas: false,
        exige_stop_reason: true,
    };

    /// La petición sin lo que el destino no admite
    pub fn adaptar(&self, peticion: &PeticionMensajes) -> PeticionMensajes {
        let mut adaptada = peticion.clone();
        if !self.thinking {
            adaptada.thinking = None;
        }
        if !self.betas {
            adaptada.betas.clear();
        }
        if !self.strict_tools {
            adaptada.tools.iter_mut().for_each(|t| t.strict = false);
        }
        if !self.cache_control {
            adaptada
                .system
                .iter_mut()
                .for_each(|b| b.cache_control = None);
            adaptada
                .tools
                .iter_mut()
                .for_each(|t| t.cache_control = None);
            for mensaje in &mut adaptada.messages {
                if let ContenidoEntrada::Bloques(bloques) = &mut mensaje.content {
                    for bloque in bloques.iter_mut() {
                        if let Some(campos) = bloque.as_object_mut() {
                            campos.remove("cache_control");
                        }
                    }
                }
            }
        }
        adaptada
    }
}

impl Default for PerfilApi {
    fn default() -> Self {
        Self::ANTHROPIC
    }
}

/// Una entrada de `GET /v1/models` en formato Anthropic, sin validar todavía
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeloListado {
    pub id: String,
    pub nombre_visible: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PoliticaReintentos {
    /// Reintentos tras el primer intento (el contrato fija 3)
    pub max_reintentos: u32,
    pub espera_base: Duration,
    pub espera_maxima: Duration,
    /// Un `retry-after` mayor que esto se recorta
    pub tope_retry_after: Duration,
}

impl Default for PoliticaReintentos {
    fn default() -> Self {
        Self {
            max_reintentos: 3,
            espera_base: Duration::from_millis(500),
            espera_maxima: Duration::from_secs(8),
            tope_retry_after: Duration::from_secs(60),
        }
    }
}

impl PoliticaReintentos {
    /// Backoff exponencial con jitter completo: aleatorio en `[0, min(máx, base·2^n)]`
    fn espera_exponencial(&self, reintento: u32) -> Duration {
        let techo = self
            .espera_base
            .saturating_mul(2u32.saturating_pow(reintento))
            .min(self.espera_maxima);
        let azar = (uuid::Uuid::new_v4().as_u128() % 1_000_001) as f64 / 1_000_000.0;
        techo.mul_f64(azar)
    }

    fn espera(&self, reintento: u32, indicada: Option<Duration>) -> Duration {
        indicada.map_or_else(
            || self.espera_exponencial(reintento),
            |espera| espera.min(self.tope_retry_after),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigClienteAnthropic {
    pub base_url: String,
    pub timeout_conexion: Duration,
    /// Máximo entre lecturas del socket
    pub timeout_lectura: Duration,
    pub reintentos: PoliticaReintentos,
    pub perfil: PerfilApi,
}

impl ConfigClienteAnthropic {
    /// Timeouts del contrato para procedencia: 10 s de conexión y 30 s de lectura
    pub fn procedencia() -> Self {
        Self {
            base_url: URL_BASE_POR_DEFECTO.into(),
            timeout_conexion: Duration::from_secs(10),
            timeout_lectura: Duration::from_secs(30),
            reintentos: PoliticaReintentos::default(),
            perfil: PerfilApi::ANTHROPIC,
        }
    }

    /// Timeouts del contrato para el agente: 10 s de conexión y 60 s entre eventos del flujo
    pub fn agente() -> Self {
        Self {
            timeout_lectura: Duration::from_secs(60),
            ..Self::procedencia()
        }
    }

    pub fn con_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn con_perfil(mut self, perfil: PerfilApi) -> Self {
        self.perfil = perfil;
        self
    }
}

/// La respuesta válida y lo que costó obtenerla
#[derive(Debug, Clone, PartialEq)]
pub struct RespuestaConMetricas {
    pub respuesta: RespuestaMensajes,
    pub intentos: u32,
    pub latencia: Duration,
}

/// El mensaje leído del flujo SSE y lo que costó: `ttft` es el tiempo hasta el primer delta de
/// contenido, contado desde el primer intento
#[derive(Debug, Clone, PartialEq)]
pub struct RespuestaEnFlujo {
    pub mensaje: MensajeEnFlujo,
    pub intentos: u32,
    pub latencia: Duration,
    pub ttft: Option<Duration>,
}

/// Recibe cada fragmento de texto visible mientras llega
pub type ObservadorFragmentos<'a> = &'a (dyn Fn(&str) + Send + Sync);

enum FalloIntento {
    Reintentable {
        espera_indicada: Option<Duration>,
        descripcion: String,
    },
    Definitivo(ErrorApiAnthropic),
}

pub struct AnthropicHttpClient {
    http: reqwest::Client,
    key: SecretApiKey,
    config: ConfigClienteAnthropic,
}

impl AnthropicHttpClient {
    pub fn new(
        key: SecretApiKey,
        config: ConfigClienteAnthropic,
    ) -> Result<Self, ErrorApiAnthropic> {
        let http = reqwest::Client::builder()
            .connect_timeout(config.timeout_conexion)
            .read_timeout(config.timeout_lectura)
            .build()
            .map_err(|e| ErrorApiAnthropic::RespuestaNoValida(format!("cliente HTTP: {e}")))?;
        Ok(Self { http, key, config })
    }

    fn url_mensajes(&self) -> String {
        format!("{}/v1/messages", self.config.base_url.trim_end_matches('/'))
    }

    pub fn perfil(&self) -> PerfilApi {
        self.config.perfil
    }

    /// `GET /v1/models?available=true` en formato Anthropic (con `anthropic-version`). Un solo
    /// intento: lo pide la ventana y un error se muestra en vez de esperar
    pub async fn listar_modelos(&self) -> Result<Vec<ModeloListado>, ErrorApiAnthropic> {
        let url = format!(
            "{}/v1/models?available=true",
            self.config.base_url.trim_end_matches('/')
        );
        let respuesta = self
            .http
            .get(url)
            .header("x-api-key", self.key.exponer())
            .header("anthropic-version", self.config.perfil.version_api)
            .send()
            .await
            .map_err(|e| {
                ErrorApiAnthropic::RespuestaNoValida(format!(
                    "sin conexión con el servidor: {}",
                    self.sin_secreto(&e.to_string())
                ))
            })?;
        let estado = respuesta.status();
        let cuerpo = respuesta
            .bytes()
            .await
            .map_err(|e| ErrorApiAnthropic::RespuestaNoValida(self.sin_secreto(&e.to_string())))?;
        if !estado.is_success() {
            let detalle = self.sin_secreto(&detalle_de_error(&String::from_utf8_lossy(&cuerpo)));
            return Err(match clasificar_estado(estado, detalle.clone(), None) {
                FalloIntento::Definitivo(error) => error,
                FalloIntento::Reintentable { .. } => ErrorApiAnthropic::EstadoNoReintentable {
                    estado: estado.as_u16(),
                    detalle,
                },
            });
        }
        if cuerpo.len() > TAMANO_MAXIMO_CATALOGO {
            return Err(ErrorApiAnthropic::RespuestaNoValida(
                "el catálogo de modelos es desmesurado".into(),
            ));
        }
        let valor: Value = serde_json::from_slice(&cuerpo)
            .map_err(|e| ErrorApiAnthropic::RespuestaNoValida(format!("catálogo: {e}")))?;
        let entradas = valor["data"].as_array().ok_or_else(|| {
            ErrorApiAnthropic::RespuestaNoValida("el catálogo no trae `data`".into())
        })?;
        Ok(entradas
            .iter()
            .filter_map(|entrada| {
                Some(ModeloListado {
                    id: entrada["id"].as_str()?.to_string(),
                    nombre_visible: entrada["display_name"].as_str().map(str::to_string),
                })
            })
            .collect())
    }

    /// Envía la petición con los reintentos de transporte y valida `stop_reason`
    pub async fn crear_mensaje(
        &self,
        peticion: &PeticionMensajes,
    ) -> Result<RespuestaConMetricas, ErrorApiAnthropic> {
        let peticion = &self.config.perfil.adaptar(peticion);
        let inicio = Instant::now();
        let politica = &self.config.reintentos;
        let mut intentos = 0;
        loop {
            intentos += 1;
            match self.un_intento(peticion).await {
                Ok(respuesta) => {
                    return validar_stop_reason(respuesta, self.config.perfil.exige_stop_reason)
                        .map(|respuesta| RespuestaConMetricas {
                            respuesta,
                            intentos,
                            latencia: inicio.elapsed(),
                        });
                }
                Err(FalloIntento::Definitivo(error)) => return Err(error),
                Err(FalloIntento::Reintentable {
                    espera_indicada,
                    descripcion,
                }) => {
                    if intentos > politica.max_reintentos {
                        return Err(ErrorApiAnthropic::ReintentosAgotados {
                            intentos,
                            ultimo: descripcion,
                        });
                    }
                    let espera = politica.espera(intentos - 1, espera_indicada);
                    tracing::warn!(
                        intento = intentos,
                        espera_ms = espera.as_millis() as u64,
                        motivo = %descripcion,
                        "API de Anthropic: reintento de transporte"
                    );
                    tokio::time::sleep(espera).await;
                }
            }
        }
    }

    /// Como `crear_mensaje`, con `stream: true`. Los reintentos de transporte solo se hacen
    /// mientras no ha llegado ningún contenido: después ya pudo emitirse texto en vivo
    pub async fn crear_mensaje_en_flujo(
        &self,
        peticion: &PeticionMensajes,
        al_texto: ObservadorFragmentos<'_>,
    ) -> Result<RespuestaEnFlujo, ErrorApiAnthropic> {
        let mut peticion = self.config.perfil.adaptar(peticion);
        peticion.stream = true;
        let inicio = Instant::now();
        let politica = &self.config.reintentos;
        let mut intentos = 0;
        loop {
            intentos += 1;
            match self.un_intento_en_flujo(&peticion, inicio, al_texto).await {
                Ok((mensaje, ttft)) => {
                    return validar_stop_reason_de(
                        mensaje.stop_reason.as_deref(),
                        self.config.perfil.exige_stop_reason,
                    )
                    .map(|()| RespuestaEnFlujo {
                        mensaje,
                        intentos,
                        latencia: inicio.elapsed(),
                        ttft,
                    });
                }
                Err(FalloIntento::Definitivo(error)) => return Err(error),
                Err(FalloIntento::Reintentable {
                    espera_indicada,
                    descripcion,
                }) => {
                    if intentos > politica.max_reintentos {
                        return Err(ErrorApiAnthropic::ReintentosAgotados {
                            intentos,
                            ultimo: descripcion,
                        });
                    }
                    let espera = politica.espera(intentos - 1, espera_indicada);
                    tracing::warn!(
                        intento = intentos,
                        espera_ms = espera.as_millis() as u64,
                        motivo = %descripcion,
                        "API de Anthropic (flujo): reintento de transporte"
                    );
                    tokio::time::sleep(espera).await;
                }
            }
        }
    }

    async fn un_intento_en_flujo(
        &self,
        peticion: &PeticionMensajes,
        inicio: Instant,
        al_texto: ObservadorFragmentos<'_>,
    ) -> Result<(MensajeEnFlujo, Option<Duration>), FalloIntento> {
        let mut envio = self
            .http
            .post(self.url_mensajes())
            .header("x-api-key", self.key.exponer())
            .header("anthropic-version", self.config.perfil.version_api)
            .header("accept", "text/event-stream");
        if !peticion.betas.is_empty() {
            envio = envio.header("anthropic-beta", peticion.betas.join(","));
        }
        let respuesta = envio
            .json(peticion)
            .send()
            .await
            .map_err(|e| self.fallo_de_transporte(&e))?;
        let estado = respuesta.status();
        if !estado.is_success() {
            let espera_indicada = leer_retry_after(respuesta.headers());
            let cuerpo = respuesta
                .text()
                .await
                .map_err(|e| self.fallo_de_transporte(&e))?;
            let detalle = self.sin_secreto(&detalle_de_error(&cuerpo));
            return Err(clasificar_estado(estado, detalle, espera_indicada));
        }
        let mut lector = LectorSse::default();
        let mut mensaje = MensajeEnFlujo::default();
        let mut ttft = None;
        let mut bytes = respuesta.bytes_stream();
        while let Some(trozo) = bytes.next().await {
            let trozo = trozo.map_err(|e| {
                let fallo = self.fallo_de_transporte(&e);
                if ttft.is_some() {
                    FalloIntento::Definitivo(ErrorApiAnthropic::RespuestaNoValida(format!(
                        "el flujo se cortó a mitad de la respuesta: {}",
                        self.sin_secreto(&e.to_string())
                    )))
                } else {
                    fallo
                }
            })?;
            for datos in lector.alimentar(&trozo) {
                match mensaje.aplicar(&datos) {
                    Ok(AporteEvento::Texto(fragmento)) => {
                        ttft.get_or_insert_with(|| inicio.elapsed());
                        al_texto(&fragmento);
                    }
                    Ok(AporteEvento::OtroContenido) => {
                        ttft.get_or_insert_with(|| inicio.elapsed());
                    }
                    Ok(AporteEvento::Fin | AporteEvento::Nada) => {}
                    Err(error) => return Err(self.fallo_en_flujo(error, ttft.is_some())),
                }
            }
            if mensaje.terminado() {
                break;
            }
        }
        if !mensaje.terminado() {
            let error =
                ErrorApiAnthropic::RespuestaNoValida("el flujo terminó sin message_stop".into());
            return Err(if ttft.is_some() {
                FalloIntento::Definitivo(error)
            } else {
                FalloIntento::Reintentable {
                    espera_indicada: None,
                    descripcion: error.to_string(),
                }
            });
        }
        Ok((mensaje, ttft))
    }

    /// Un `error` sobrecargado o interno antes del primer contenido se reintenta; después, no
    fn fallo_en_flujo(&self, error: ErrorApiAnthropic, hubo_contenido: bool) -> FalloIntento {
        let error = match error {
            ErrorApiAnthropic::EstadoNoReintentable { estado, detalle } => {
                ErrorApiAnthropic::EstadoNoReintentable {
                    estado,
                    detalle: self.sin_secreto(&detalle),
                }
            }
            otro => otro,
        };
        match &error {
            ErrorApiAnthropic::EstadoNoReintentable { estado, detalle }
                if !hubo_contenido && ErrorApiAnthropic::estado_reintentable(*estado) =>
            {
                FalloIntento::Reintentable {
                    espera_indicada: None,
                    descripcion: format!("{estado}: {detalle}"),
                }
            }
            _ => FalloIntento::Definitivo(error),
        }
    }

    async fn un_intento(
        &self,
        peticion: &PeticionMensajes,
    ) -> Result<RespuestaMensajes, FalloIntento> {
        let respuesta = self
            .http
            .post(self.url_mensajes())
            .header("x-api-key", self.key.exponer())
            .header("anthropic-version", self.config.perfil.version_api)
            .json(peticion)
            .send()
            .await
            .map_err(|e| self.fallo_de_transporte(&e))?;
        let estado = respuesta.status();
        let espera_indicada = leer_retry_after(respuesta.headers());
        let cuerpo = respuesta
            .text()
            .await
            .map_err(|e| self.fallo_de_transporte(&e))?;
        if estado.is_success() {
            return serde_json::from_str(&cuerpo).map_err(|e| {
                FalloIntento::Definitivo(ErrorApiAnthropic::RespuestaNoValida(e.to_string()))
            });
        }
        let detalle = self.sin_secreto(&detalle_de_error(&cuerpo));
        Err(clasificar_estado(estado, detalle, espera_indicada))
    }

    fn fallo_de_transporte(&self, error: &reqwest::Error) -> FalloIntento {
        let descripcion = self.sin_secreto(&error.to_string());
        if error.is_timeout() || error.is_connect() || error.is_request() || error.is_body() {
            FalloIntento::Reintentable {
                espera_indicada: None,
                descripcion,
            }
        } else {
            FalloIntento::Definitivo(ErrorApiAnthropic::RespuestaNoValida(descripcion))
        }
    }

    /// Defensa en profundidad: si un texto externo repitiera la key, no sale de aquí
    fn sin_secreto(&self, texto: &str) -> String {
        texto.replace(self.key.exponer(), "[REDACTED]")
    }
}

fn leer_retry_after(cabeceras: &HeaderMap) -> Option<Duration> {
    let valor = cabeceras.get(RETRY_AFTER)?.to_str().ok()?.trim();
    valor
        .parse::<f64>()
        .ok()
        .filter(|s| s.is_finite() && *s >= 0.0)
        .map(Duration::from_secs_f64)
}

/// `error.message` del cuerpo de error de la API, o el cuerpo recortado
fn detalle_de_error(cuerpo: &str) -> String {
    let mensaje = serde_json::from_str::<Value>(cuerpo)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| cuerpo.to_string());
    mensaje.chars().take(LONGITUD_MAXIMA_DETALLE).collect()
}

fn clasificar_estado(
    estado: StatusCode,
    detalle: String,
    espera_indicada: Option<Duration>,
) -> FalloIntento {
    let codigo = estado.as_u16();
    if ErrorApiAnthropic::estado_reintentable(codigo) {
        return FalloIntento::Reintentable {
            espera_indicada,
            descripcion: format!("{codigo}: {detalle}"),
        };
    }
    FalloIntento::Definitivo(match codigo {
        400 => ErrorApiAnthropic::PeticionInvalida { detalle },
        401 => ErrorApiAnthropic::Autenticacion,
        403 => ErrorApiAnthropic::Permiso { detalle },
        _ => ErrorApiAnthropic::EstadoNoReintentable {
            estado: codigo,
            detalle,
        },
    })
}

/// `max_tokens` nunca es un resultado válido; `refusal` es una salida controlada
fn validar_stop_reason(
    respuesta: RespuestaMensajes,
    exige: bool,
) -> Result<RespuestaMensajes, ErrorApiAnthropic> {
    validar_stop_reason_de(respuesta.stop_reason.as_deref(), exige).map(|()| respuesta)
}

/// `max_tokens` y `refusal` nunca valen. Sin `stop_reason` no se sabe si la respuesta terminó:
/// si el perfil lo exige (un proxy), falla cerrada
fn validar_stop_reason_de(stop_reason: Option<&str>, exige: bool) -> Result<(), ErrorApiAnthropic> {
    match stop_reason {
        Some("max_tokens") => Err(ErrorApiAnthropic::RespuestaTruncada),
        Some("refusal") => Err(ErrorApiAnthropic::Rechazo),
        None if exige => Err(ErrorApiAnthropic::RespuestaNoValida(
            "la respuesta no trae stop_reason".into(),
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {

    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::infrastructure::llm::anthropic_api::mensajes::{BloqueRespuesta, MensajeEntrada};

    const KEY_FALSA: &str = "sk-ant-test-NO-ES-REAL-7f3a9c";

    fn config_rapida(base_url: &str) -> ConfigClienteAnthropic {
        ConfigClienteAnthropic {
            base_url: base_url.into(),
            timeout_conexion: Duration::from_millis(300),
            timeout_lectura: Duration::from_secs(2),
            reintentos: PoliticaReintentos {
                max_reintentos: 3,
                espera_base: Duration::from_millis(1),
                espera_maxima: Duration::from_millis(5),
                tope_retry_after: Duration::from_millis(50),
            },
            perfil: PerfilApi::ANTHROPIC,
        }
    }

    fn cliente(servidor: &MockServer) -> AnthropicHttpClient {
        AnthropicHttpClient::new(
            SecretApiKey::new(KEY_FALSA).unwrap(),
            config_rapida(&servidor.uri()),
        )
        .unwrap()
    }

    fn peticion() -> PeticionMensajes {
        PeticionMensajes {
            model: "claude-haiku-5-5".into(),
            max_tokens: 64,
            system: Vec::new(),
            messages: vec![MensajeEntrada::usuario("hola")],
            tools: Vec::new(),
            tool_choice: None,
            ..Default::default()
        }
    }

    fn cuerpo_tool_use(stop_reason: &str) -> Value {
        json!({
            "id": "msg_1", "type": "message", "role": "assistant", "model": "claude-haiku-5-5",
            "stop_reason": stop_reason,
            "content": [{ "type": "tool_use", "id": "tu_1", "name": "emitir", "input": { "x": 1 } }],
            "usage": { "input_tokens": 11, "cache_creation_input_tokens": 600,
                       "cache_read_input_tokens": 0, "output_tokens": 22 },
        })
    }

    fn error_api(tipo: &str, mensaje: &str) -> Value {
        json!({ "type": "error", "error": { "type": tipo, "message": mensaje } })
    }

    async fn peticiones_recibidas(servidor: &MockServer) -> usize {
        servidor.received_requests().await.map_or(0, |r| r.len())
    }

    fn sse(eventos: &[Value]) -> String {
        eventos
            .iter()
            .map(|e| {
                format!(
                    "event: {}
data: {e}

",
                    e["type"].as_str().unwrap()
                )
            })
            .collect()
    }

    fn flujo_de_texto(texto: &str, stop_reason: &str) -> String {
        sse(&[
            json!({"type":"message_start","message":{"id":"m","model":"claude-sonnet-5-5","content":[],"usage":{"input_tokens":5,"cache_read_input_tokens":900,"output_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":texto}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{"stop_reason":stop_reason},"usage":{"output_tokens":7}}),
            json!({"type":"message_stop"}),
        ])
    }

    fn respuesta_sse(cuerpo: String) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(cuerpo, "text/event-stream")
    }

    async fn en_flujo(
        servidor: &MockServer,
        peticion: &PeticionMensajes,
    ) -> (Result<RespuestaEnFlujo, ErrorApiAnthropic>, Vec<String>) {
        let textos = std::sync::Mutex::new(Vec::new());
        let anotar = |t: &str| textos.lock().unwrap().push(t.to_string());
        let resultado = cliente(servidor)
            .crear_mensaje_en_flujo(peticion, &anotar)
            .await;
        (resultado, textos.into_inner().unwrap())
    }

    #[tokio::test]
    async fn test_flujo_pide_stream_con_betas_y_entrega_texto_y_uso() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(wiremock::matchers::headers(
                "anthropic-beta",
                vec!["beta-a", "beta-b"],
            ))
            .and(wiremock::matchers::body_partial_json(
                json!({"stream": true}),
            ))
            .respond_with(respuesta_sse(flujo_de_texto("Hola", "end_turn")))
            .mount(&servidor)
            .await;
        let mut con_betas = peticion();
        con_betas.betas = vec!["beta-a".into(), "beta-b".into()];
        let (resultado, textos) = en_flujo(&servidor, &con_betas).await;
        let resultado = resultado.unwrap();
        assert_eq!(textos, vec!["Hola"]);
        assert_eq!(resultado.mensaje.contenido[0]["text"], "Hola");
        assert_eq!(resultado.mensaje.uso.cache_read_input_tokens, 900);
        assert_eq!(resultado.mensaje.uso.output_tokens, 7);
        assert!(resultado.ttft.is_some_and(|t| t <= resultado.latencia));
    }

    #[tokio::test]
    async fn test_flujo_reintenta_un_429_y_un_error_antes_del_contenido() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "0")
                    .set_body_json(error_api("rate_limit_error", "despacio")),
            )
            .up_to_n_times(1)
            .mount(&servidor)
            .await;
        let sobrecarga = sse(&[
            json!({"type":"message_start","message":{"id":"m","model":"x","usage":{}}}),
            json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}),
        ]);
        Mock::given(method("POST"))
            .respond_with(respuesta_sse(sobrecarga))
            .up_to_n_times(1)
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .respond_with(respuesta_sse(flujo_de_texto("Ya", "end_turn")))
            .mount(&servidor)
            .await;
        let (resultado, textos) = en_flujo(&servidor, &peticion()).await;
        assert_eq!(resultado.unwrap().intentos, 3);
        assert_eq!(textos, vec!["Ya"]);
    }

    #[tokio::test]
    async fn test_flujo_con_error_tras_el_contenido_no_se_reintenta() {
        let servidor = MockServer::start().await;
        let cortado = sse(&[
            json!({"type":"message_start","message":{"id":"m","model":"x","usage":{}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Medio"}}),
            json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}),
        ]);
        Mock::given(method("POST"))
            .respond_with(respuesta_sse(cortado))
            .mount(&servidor)
            .await;
        let (resultado, textos) = en_flujo(&servidor, &peticion()).await;
        assert!(matches!(
            resultado.unwrap_err(),
            ErrorApiAnthropic::EstadoNoReintentable { estado: 529, .. }
        ));
        assert_eq!(textos, vec!["Medio"]);
        assert_eq!(peticiones_recibidas(&servidor).await, 1);
    }

    #[tokio::test]
    async fn test_flujo_max_tokens_refusal_o_sin_message_stop_no_valen() {
        for (cuerpo, esperado) in [
            (flujo_de_texto("Cortado", "max_tokens"), "truncada"),
            (flujo_de_texto("No", "refusal"), "rechazo"),
        ] {
            let servidor = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(respuesta_sse(cuerpo))
                .mount(&servidor)
                .await;
            let error = en_flujo(&servidor, &peticion()).await.0.unwrap_err();
            match esperado {
                "truncada" => assert_eq!(error, ErrorApiAnthropic::RespuestaTruncada),
                _ => assert_eq!(error, ErrorApiAnthropic::Rechazo),
            }
        }
        let servidor = MockServer::start().await;
        let sin_fin = flujo_de_texto("Hola", "end_turn").replace("event: message_stop", "event: x");
        let sin_fin = sin_fin.replace(r#"{"type":"message_stop"}"#, r#"{"type":"x"}"#);
        Mock::given(method("POST"))
            .respond_with(respuesta_sse(sin_fin))
            .mount(&servidor)
            .await;
        assert!(matches!(
            en_flujo(&servidor, &peticion()).await.0.unwrap_err(),
            ErrorApiAnthropic::RespuestaNoValida(_)
        ));
    }

    #[tokio::test]
    async fn test_tool_use_con_cabeceras_y_uso() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", KEY_FALSA))
            .and(header("anthropic-version", VERSION_API))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(cuerpo_tool_use("tool_use")))
            .mount(&servidor)
            .await;
        let resultado = cliente(&servidor).crear_mensaje(&peticion()).await.unwrap();
        assert_eq!(resultado.intentos, 1);
        assert_eq!(resultado.respuesta.stop_reason.as_deref(), Some("tool_use"));
        assert!(matches!(
            &resultado.respuesta.content[0],
            BloqueRespuesta::ToolUse { name, .. } if name == "emitir"
        ));
        assert_eq!(resultado.respuesta.usage.cache_creation_input_tokens, 600);
        assert_eq!(resultado.respuesta.usage.output_tokens, 22);
    }

    #[tokio::test]
    async fn test_un_429_con_retry_after_y_luego_200() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "0")
                    .set_body_json(error_api("rate_limit_error", "despacio")),
            )
            .up_to_n_times(1)
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(cuerpo_tool_use("tool_use")))
            .mount(&servidor)
            .await;
        let resultado = cliente(&servidor).crear_mensaje(&peticion()).await.unwrap();
        assert_eq!(resultado.intentos, 2);
        assert_eq!(peticiones_recibidas(&servidor).await, 2);
    }

    #[tokio::test]
    async fn test_529_y_5xx_agotan_3_reintentos() {
        for estado in [529u16, 503, 500] {
            let servidor = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(estado).set_body_json(error_api("overloaded_error", "x")),
                )
                .mount(&servidor)
                .await;
            let error = cliente(&servidor)
                .crear_mensaje(&peticion())
                .await
                .unwrap_err();
            assert!(
                matches!(
                    error,
                    ErrorApiAnthropic::ReintentosAgotados { intentos: 4, .. }
                ),
                "{estado}: {error:?}"
            );
            assert_eq!(peticiones_recibidas(&servidor).await, 4, "{estado}");
        }
    }

    #[tokio::test]
    async fn test_400_401_403_sin_reintento_y_accionables() {
        let casos = [
            (400u16, "invalid_request_error"),
            (401, "authentication_error"),
            (403, "permission_error"),
        ];
        for (estado, tipo) in casos {
            let servidor = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(estado)
                        .set_body_json(error_api(tipo, "detalle de la api")),
                )
                .mount(&servidor)
                .await;
            let error = cliente(&servidor)
                .crear_mensaje(&peticion())
                .await
                .unwrap_err();
            assert_eq!(peticiones_recibidas(&servidor).await, 1, "{estado}");
            let esperado = match estado {
                400 => {
                    matches!(&error, ErrorApiAnthropic::PeticionInvalida { detalle } if detalle == "detalle de la api")
                }
                401 => error == ErrorApiAnthropic::Autenticacion,
                _ => matches!(error, ErrorApiAnthropic::Permiso { .. }),
            };
            assert!(esperado, "{estado}: {error:?}");
        }
    }

    #[tokio::test]
    async fn test_max_tokens_nunca_es_valido_y_refusal_es_controlado() {
        for (stop_reason, esperado) in [
            ("max_tokens", ErrorApiAnthropic::RespuestaTruncada),
            ("refusal", ErrorApiAnthropic::Rechazo),
        ] {
            let servidor = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(cuerpo_tool_use(stop_reason)),
                )
                .mount(&servidor)
                .await;
            let error = cliente(&servidor)
                .crear_mensaje(&peticion())
                .await
                .unwrap_err();
            assert_eq!(error, esperado);
            assert_eq!(peticiones_recibidas(&servidor).await, 1);
        }
    }

    #[tokio::test]
    async fn test_servidor_caido_agota_reintentos_sin_colgarse() {
        // Un puerto liberado lo puede reutilizar otro test en paralelo: el 1 no escucha
        let cliente = AnthropicHttpClient::new(
            SecretApiKey::new(KEY_FALSA).unwrap(),
            config_rapida("http://127.0.0.1:1"),
        )
        .unwrap();
        let error = cliente.crear_mensaje(&peticion()).await.unwrap_err();
        assert!(
            matches!(
                error,
                ErrorApiAnthropic::ReintentosAgotados { intentos: 4, .. }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn test_retry_after_se_respeta_con_tope() {
        let politica = PoliticaReintentos {
            tope_retry_after: Duration::from_secs(60),
            ..PoliticaReintentos::default()
        };
        assert_eq!(
            politica.espera(0, Some(Duration::from_secs(7))),
            Duration::from_secs(7)
        );
        assert_eq!(
            politica.espera(0, Some(Duration::from_secs(600))),
            Duration::from_secs(60)
        );
        for reintento in 0..6 {
            assert!(politica.espera(reintento, None) <= politica.espera_maxima);
        }
        let mut cabeceras = HeaderMap::new();
        cabeceras.insert(RETRY_AFTER, "3".parse().unwrap());
        assert_eq!(leer_retry_after(&cabeceras), Some(Duration::from_secs(3)));
        cabeceras.insert(
            RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(leer_retry_after(&cabeceras), None);
    }

    fn cliente_freellmapi(servidor: &MockServer) -> AnthropicHttpClient {
        AnthropicHttpClient::new(
            SecretApiKey::new("freellmapi-test-NO-ES-REAL-1234").unwrap(),
            config_rapida(&servidor.uri()).con_perfil(PerfilApi::FREELLMAPI),
        )
        .unwrap()
    }

    fn peticion_completa() -> PeticionMensajes {
        let mut completa = peticion();
        completa.system = vec![super::super::mensajes::BloqueSistema::texto(
            "sistema",
            Some(super::super::mensajes::ControlCache::efimera()),
        )];
        completa.messages = vec![MensajeEntrada::con_bloques(
            "user",
            vec![json!({"type":"text","text":"hola","cache_control":{"type":"ephemeral"}})],
        )];
        completa.tools = vec![super::super::mensajes::DefinicionTool {
            name: "t".into(),
            description: "d".into(),
            input_schema: json!({"type":"object"}),
            strict: true,
            cache_control: Some(super::super::mensajes::ControlCache::efimera()),
        }];
        completa.thinking = Some(json!({"type":"adaptive"}));
        completa.betas = vec!["beta-x".into()];
        completa
    }

    #[test]
    fn test_el_perfil_anthropic_no_toca_la_peticion() {
        let completa = peticion_completa();
        assert_eq!(PerfilApi::ANTHROPIC.adaptar(&completa), completa);
    }

    #[tokio::test]
    async fn test_el_perfil_freellmapi_quita_lo_que_el_proxy_no_reenvia() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("anthropic-version", VERSION_API_FREELLMAPI))
            .respond_with(respuesta_sse(flujo_de_texto("Hola", "end_turn")))
            .mount(&servidor)
            .await;
        cliente_freellmapi(&servidor)
            .crear_mensaje_en_flujo(&peticion_completa(), &|_| {})
            .await
            .unwrap();
        let recibidas = servidor.received_requests().await.unwrap();
        let enviada = &recibidas[0];
        assert!(enviada.headers.get("anthropic-beta").is_none());
        let cuerpo = String::from_utf8_lossy(&enviada.body);
        for ausente in ["thinking", "cache_control", "strict"] {
            assert!(!cuerpo.contains(ausente), "{ausente} en {cuerpo}");
        }
        assert!(cuerpo.contains("\"stream\":true"), "{cuerpo}");
    }

    #[tokio::test]
    async fn test_sin_stop_reason_falla_cerrada_con_el_perfil_freellmapi_y_anthropic_no_cambia() {
        let servidor = MockServer::start().await;
        let sin_stop = sse(&[
            json!({"type":"message_start","message":{"id":"m","model":"auto","content":[]}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"417"}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"message_delta","delta":{}}),
            json!({"type":"message_stop"}),
        ]);
        Mock::given(method("POST"))
            .respond_with(respuesta_sse(sin_stop))
            .mount(&servidor)
            .await;
        let resultado = cliente_freellmapi(&servidor)
            .crear_mensaje_en_flujo(&peticion(), &|_| {})
            .await;
        assert!(
            matches!(resultado, Err(ErrorApiAnthropic::RespuestaNoValida(_))),
            "{resultado:?}"
        );
        let (anthropic, _) = en_flujo(&servidor, &peticion()).await;
        assert!(anthropic.is_ok(), "el modo API no cambia: {anthropic:?}");
        let servidor = MockServer::start().await;
        let mut cuerpo = cuerpo_tool_use("end_turn");
        cuerpo.as_object_mut().unwrap().remove("stop_reason");
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(cuerpo))
            .mount(&servidor)
            .await;
        let error = cliente_freellmapi(&servidor)
            .crear_mensaje(&peticion())
            .await
            .unwrap_err();
        assert!(
            matches!(error, ErrorApiAnthropic::RespuestaNoValida(_)),
            "{error:?}"
        );
        assert!(cliente(&servidor).crear_mensaje(&peticion()).await.is_ok());
    }

    #[tokio::test]
    async fn test_listar_modelos_pide_los_disponibles_en_formato_anthropic() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(wiremock::matchers::query_param("available", "true"))
            .and(header("anthropic-version", VERSION_API_FREELLMAPI))
            .and(header("x-api-key", "freellmapi-test-NO-ES-REAL-1234"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    {"type":"model","id":"auto","display_name":"Auto"},
                    {"type":"model","id":"gemini-flash"},
                    {"type":"model","display_name":"sin id"},
                ],
                "has_more": false,
            })))
            .mount(&servidor)
            .await;
        let modelos = cliente_freellmapi(&servidor)
            .listar_modelos()
            .await
            .unwrap();
        assert_eq!(
            modelos,
            vec![
                ModeloListado {
                    id: "auto".into(),
                    nombre_visible: Some("Auto".into())
                },
                ModeloListado {
                    id: "gemini-flash".into(),
                    nombre_visible: None
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_listar_modelos_con_key_mala_o_proxy_caido_da_un_error_sin_la_key() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401).set_body_json(error_api(
                "authentication_error",
                "freellmapi-test-NO-ES-REAL-1234 no vale",
            )))
            .mount(&servidor)
            .await;
        let error = cliente_freellmapi(&servidor)
            .listar_modelos()
            .await
            .unwrap_err();
        assert!(
            matches!(error, ErrorApiAnthropic::Autenticacion),
            "{error:?}"
        );
        let caido = AnthropicHttpClient::new(
            SecretApiKey::new("freellmapi-test-NO-ES-REAL-1234").unwrap(),
            config_rapida("http://127.0.0.1:1").con_perfil(PerfilApi::FREELLMAPI),
        )
        .unwrap();
        let error = caido.listar_modelos().await.unwrap_err().to_string();
        assert!(!error.contains("NO-ES-REAL"), "{error}");
    }
}
