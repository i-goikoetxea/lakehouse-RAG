//! Lectura del streaming SSE de `POST /v1/messages` (Fase 5.2). Los bytes llegan en trozos TCP
//! arbitrarios; aquí se cortan en eventos y se reconstruye el mensaje: bloques de contenido tal
//! cual (con `thinking` y su firma, para devolverlos sin tocar), `stop_reason` y uso.

use serde_json::{Map, Value};

use super::error::ErrorApiAnthropic;
use super::mensajes::Uso;

/// Corta el flujo de bytes en los `data:` de cada evento, sin depender de cómo se partió
#[derive(Debug, Default)]
pub struct LectorSse {
    pendiente: Vec<u8>,
}

impl LectorSse {
    /// Los datos de los eventos completos que ya han llegado
    pub fn alimentar(&mut self, trozo: &[u8]) -> Vec<String> {
        self.pendiente.extend_from_slice(trozo);
        let mut eventos = Vec::new();
        while let Some((fin, separador)) = fin_de_evento(&self.pendiente) {
            let crudo: Vec<u8> = self.pendiente.drain(..fin + separador).collect();
            if let Some(datos) = datos_del_evento(&String::from_utf8_lossy(&crudo[..fin])) {
                eventos.push(datos);
            }
        }
        eventos
    }
}

fn fin_de_evento(bytes: &[u8]) -> Option<(usize, usize)> {
    let lf = bytes.windows(2).position(|v| v == b"\n\n").map(|p| (p, 2));
    let crlf = bytes
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .map(|p| (p, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

/// Las líneas `data:` de un evento, unidas; los comentarios y `event:` no hacen falta: el tipo
/// va también dentro del JSON
fn datos_del_evento(evento: &str) -> Option<String> {
    let datos: Vec<&str> = evento
        .lines()
        .filter_map(|linea| linea.strip_prefix("data:"))
        .map(|d| d.strip_prefix(' ').unwrap_or(d))
        .collect();
    (!datos.is_empty()).then(|| datos.join("\n"))
}

/// Lo que un evento aporta a quien lo escucha
#[derive(Debug, Clone, PartialEq)]
pub enum AporteEvento {
    /// Fragmento de un bloque `text`
    Texto(String),
    /// Cualquier otro delta de contenido (pensamiento, argumentos de una tool)
    OtroContenido,
    Fin,
    Nada,
}

/// El mensaje que se va formando con los eventos
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MensajeEnFlujo {
    pub id: String,
    pub modelo: String,
    /// Bloques en el orden de la API, tal cual para devolverlos en el historial
    pub contenido: Vec<Value>,
    pub stop_reason: Option<String>,
    pub uso: Uso,
    /// Bloques `thinking` que la API descartó por la comprobación de historial (`drop_block`)
    pub bloques_thinking_descartados: usize,
    argumentos_parciales: Vec<String>,
    terminado: bool,
}

impl MensajeEnFlujo {
    pub fn terminado(&self) -> bool {
        self.terminado
    }

    /// Aplica un evento (el JSON de su `data:`). Un evento `error` de la API es un fallo
    pub fn aplicar(&mut self, datos: &str) -> Result<AporteEvento, ErrorApiAnthropic> {
        let evento: Value = serde_json::from_str(datos).map_err(|e| {
            ErrorApiAnthropic::RespuestaNoValida(format!("evento SSE que no es JSON: {e}"))
        })?;
        match evento["type"].as_str().unwrap_or_default() {
            "message_start" => {
                self.al_empezar(&evento["message"]);
                Ok(AporteEvento::Nada)
            }
            "content_block_start" => {
                let indice = indice_de(&evento)?;
                let bloque = evento["content_block"].clone();
                while self.contenido.len() <= indice {
                    self.contenido.push(Value::Null);
                    self.argumentos_parciales.push(String::new());
                }
                self.contenido[indice] = bloque;
                Ok(AporteEvento::Nada)
            }
            "content_block_delta" => self.al_recibir_delta(indice_de(&evento)?, &evento["delta"]),
            "content_block_stop" => {
                self.al_cerrar_bloque(indice_de(&evento)?)?;
                Ok(AporteEvento::Nada)
            }
            "message_delta" => {
                if let Some(razon) = evento["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(razon.to_string());
                }
                acumular_uso(&mut self.uso, &evento["usage"]);
                Ok(AporteEvento::Nada)
            }
            "message_stop" => {
                self.terminado = true;
                Ok(AporteEvento::Fin)
            }
            "error" => Err(error_en_flujo(&evento["error"])),
            _ => Ok(AporteEvento::Nada),
        }
    }

    fn al_empezar(&mut self, mensaje: &Value) {
        self.id = mensaje["id"].as_str().unwrap_or_default().to_string();
        self.modelo = mensaje["model"].as_str().unwrap_or_default().to_string();
        acumular_uso(&mut self.uso, &mensaje["usage"]);
        self.bloques_thinking_descartados =
            mensaje["input_transformations"].as_array().map_or(0, |t| {
                t.iter().filter(|e| e["type"] == "thinking_dropped").count()
            });
    }

    fn bloque(&mut self, indice: usize) -> Result<&mut Map<String, Value>, ErrorApiAnthropic> {
        self.contenido
            .get_mut(indice)
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                ErrorApiAnthropic::RespuestaNoValida(format!("delta del bloque {indice} sin abrir"))
            })
    }

    fn al_recibir_delta(
        &mut self,
        indice: usize,
        delta: &Value,
    ) -> Result<AporteEvento, ErrorApiAnthropic> {
        let texto = |campo: &str| delta[campo].as_str().unwrap_or_default().to_string();
        match delta["type"].as_str().unwrap_or_default() {
            "text_delta" => {
                let fragmento = texto("text");
                anexar(self.bloque(indice)?, "text", &fragmento);
                Ok(AporteEvento::Texto(fragmento))
            }
            "input_json_delta" => {
                self.bloque(indice)?;
                self.argumentos_parciales[indice].push_str(&texto("partial_json"));
                Ok(AporteEvento::OtroContenido)
            }
            "thinking_delta" => {
                anexar(self.bloque(indice)?, "thinking", &texto("thinking"));
                Ok(AporteEvento::OtroContenido)
            }
            "signature_delta" => {
                self.bloque(indice)?
                    .insert("signature".into(), Value::String(texto("signature")));
                Ok(AporteEvento::OtroContenido)
            }
            _ => Ok(AporteEvento::OtroContenido),
        }
    }

    /// Los argumentos de un `tool_use` se parsean al cerrar el bloque: vacíos son `{}`
    fn al_cerrar_bloque(&mut self, indice: usize) -> Result<(), ErrorApiAnthropic> {
        let parciales = std::mem::take(
            self.argumentos_parciales
                .get_mut(indice)
                .ok_or_else(|| ErrorApiAnthropic::RespuestaNoValida("cierre sin bloque".into()))?,
        );
        let bloque = self.bloque(indice)?;
        if bloque.get("type").and_then(Value::as_str) == Some("tool_use") {
            let entrada = if parciales.trim().is_empty() {
                bloque
                    .get("input")
                    .cloned()
                    .unwrap_or_else(|| Value::Object(Map::new()))
            } else {
                serde_json::from_str(&parciales).map_err(|e| {
                    ErrorApiAnthropic::RespuestaNoValida(format!(
                        "argumentos de tool que no son JSON: {e}"
                    ))
                })?
            };
            bloque.insert("input".into(), entrada);
        }
        Ok(())
    }
}

fn indice_de(evento: &Value) -> Result<usize, ErrorApiAnthropic> {
    evento["index"]
        .as_u64()
        .map(|i| i as usize)
        .filter(|i| *i < 4096)
        .ok_or_else(|| ErrorApiAnthropic::RespuestaNoValida("evento sin índice válido".into()))
}

fn anexar(bloque: &mut Map<String, Value>, campo: &str, fragmento: &str) {
    let actual = bloque
        .get(campo)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    bloque.insert(campo.into(), Value::String(actual + fragmento));
}

/// Los campos presentes sustituyen a los anteriores: `message_delta` trae valores acumulados
fn acumular_uso(uso: &mut Uso, valor: &Value) {
    let leer = |campo: &str| valor[campo].as_u64();
    if let Some(n) = leer("input_tokens") {
        uso.input_tokens = n;
    }
    if let Some(n) = leer("cache_creation_input_tokens") {
        uso.cache_creation_input_tokens = n;
    }
    if let Some(n) = leer("cache_read_input_tokens") {
        uso.cache_read_input_tokens = n;
    }
    if let Some(n) = leer("output_tokens") {
        uso.output_tokens = n;
    }
}

/// `overloaded_error` y `api_error` a mitad del flujo se tratan como un 529 o un 500
fn error_en_flujo(error: &Value) -> ErrorApiAnthropic {
    let tipo = error["type"].as_str().unwrap_or_default();
    let detalle: String = error["message"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .take(400)
        .collect();
    let estado = match tipo {
        "overloaded_error" => 529,
        "rate_limit_error" => 429,
        "invalid_request_error" => 400,
        _ => 500,
    };
    ErrorApiAnthropic::EstadoNoReintentable {
        estado,
        detalle: format!("{tipo} en el flujo: {detalle}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn evento(datos: Value) -> String {
        format!(
            "event: {}\ndata: {}\n\n",
            datos["type"].as_str().unwrap(),
            datos
        )
    }

    fn flujo_con_tool_y_thinking() -> String {
        [
            json!({"type":"message_start","message":{"id":"msg_1","model":"claude-sonnet-5-5","content":[],"usage":{"input_tokens":12,"cache_creation_input_tokens":3000,"cache_read_input_tokens":0,"output_tokens":1}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"FIRMA=="}}),
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Busco "}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"la tabla."}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"toolu_1","name":"search_normative_context","input":{}}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"consulta\": \"pla"}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"zo ñ\"}"}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":42}}),
            json!({"type":"message_stop"}),
        ]
        .into_iter()
        .map(evento)
        .collect()
    }

    fn reconstruir(trozos: &[&[u8]]) -> (MensajeEnFlujo, Vec<String>) {
        let mut lector = LectorSse::default();
        let mut mensaje = MensajeEnFlujo::default();
        let mut textos = Vec::new();
        for trozo in trozos {
            for datos in lector.alimentar(trozo) {
                if let AporteEvento::Texto(t) = mensaje.aplicar(&datos).unwrap() {
                    textos.push(t);
                }
            }
        }
        (mensaje, textos)
    }

    #[test]
    fn test_reconstruye_thinking_texto_y_tool_use_con_su_uso() {
        let flujo = flujo_con_tool_y_thinking();
        let (mensaje, textos) = reconstruir(&[flujo.as_bytes()]);
        assert!(mensaje.terminado());
        assert_eq!(mensaje.stop_reason.as_deref(), Some("tool_use"));
        assert_eq!(textos, vec!["Busco ", "la tabla."]);
        assert_eq!(
            mensaje.contenido[0],
            json!({"type":"thinking","thinking":"","signature":"FIRMA=="})
        );
        assert_eq!(mensaje.contenido[1]["text"], "Busco la tabla.");
        assert_eq!(
            mensaje.contenido[2]["input"],
            json!({"consulta": "plazo ñ"})
        );
        assert_eq!(mensaje.uso.input_tokens, 12);
        assert_eq!(mensaje.uso.cache_creation_input_tokens, 3000);
        assert_eq!(mensaje.uso.output_tokens, 42);
    }

    #[test]
    fn test_trozos_tcp_arbitrarios_dan_el_mismo_mensaje() {
        let flujo = flujo_con_tool_y_thinking().replace('\n', "\r\n");
        let bytes = flujo.as_bytes();
        let (entero, _) = reconstruir(&[bytes]);
        for tamano in [1, 2, 3, 7, 13, 64] {
            let trozos: Vec<&[u8]> = bytes.chunks(tamano).collect();
            let (partido, textos) = reconstruir(&trozos);
            assert_eq!(partido, entero, "trozos de {tamano} bytes");
            assert_eq!(textos.concat(), "Busco la tabla.");
        }
    }

    #[test]
    fn test_ping_y_comentarios_no_aportan_nada() {
        let mut lector = LectorSse::default();
        let eventos =
            lector.alimentar(b": comentario\n\nevent: ping\ndata: {\"type\": \"ping\"}\n\n");
        assert_eq!(eventos.len(), 1);
        assert_eq!(
            MensajeEnFlujo::default().aplicar(&eventos[0]).unwrap(),
            AporteEvento::Nada
        );
    }

    #[test]
    fn test_un_error_a_mitad_del_flujo_es_un_fallo() {
        let error = MensajeEnFlujo::default()
            .aplicar(
                r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ErrorApiAnthropic::EstadoNoReintentable { estado: 529, .. }
        ));
    }

    #[test]
    fn test_argumentos_truncados_o_delta_sin_bloque_fallan_cerrado() {
        let mut mensaje = MensajeEnFlujo::default();
        mensaje
            .aplicar(r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t","name":"x","input":{}}}"#)
            .unwrap();
        mensaje
            .aplicar(r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"a\": "}}"#)
            .unwrap();
        assert!(
            mensaje
                .aplicar(r#"{"type":"content_block_stop","index":0}"#)
                .is_err()
        );
        assert!(MensajeEnFlujo::default()
            .aplicar(r#"{"type":"content_block_delta","index":3,"delta":{"type":"text_delta","text":"x"}}"#)
            .is_err());
    }

    #[test]
    fn test_cuenta_los_bloques_thinking_descartados_por_la_api() {
        let mut mensaje = MensajeEnFlujo::default();
        mensaje
            .aplicar(r#"{"type":"message_start","message":{"id":"m","model":"x","usage":{},"input_transformations":[{"type":"thinking_dropped","path":"messages.1.content.0","reason":"prefix_binding_mismatch"},{"type":"otro"}]}}"#)
            .unwrap();
        assert_eq!(mensaje.bloques_thinking_descartados, 1);
    }
}
