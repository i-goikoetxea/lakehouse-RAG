//! Lectura incremental de `claude -p --output-format stream-json --include-partial-messages`.
//!
//! Cada línea de stdout es un objeto JSON. Interesan dos tipos:
//! - `stream_event` con `content_block_delta` / `text_delta`: un fragmento de texto del modelo;
//! - `result`: el texto completo del turno (autoritativo) y si terminó en error.
//!
//! El protocolo del agente hace que el modelo responda O BIEN con un JSON de `tool_calls`
//! O BIEN con el texto final para el operador. `FiltroTextoVisible` decide con el primer
//! carácter significativo: si empieza por `{` o por una valla de código, es una petición de
//! herramienta y no se muestra; en otro caso, el texto se reenvía en vivo.

use serde_json::Value;

/// Lo que aporta una línea del stream
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoStream {
    /// Fragmento de texto del modelo
    Fragmento(String),
    /// Final del turno: texto completo y si el CLI lo marca como error
    Resultado { texto: String, es_error: bool },
    /// Línea sin interés para el chat (init, estado, límites…) o no JSON
    Ignorado,
}

/// Interpreta una línea de stdout del modo `stream-json`
pub fn interpretar_linea(linea: &str) -> EventoStream {
    let Ok(valor) = serde_json::from_str::<Value>(linea.trim()) else {
        return EventoStream::Ignorado;
    };
    match valor.get("type").and_then(Value::as_str) {
        Some("stream_event") => fragmento_de_texto(&valor["event"])
            .map(EventoStream::Fragmento)
            .unwrap_or(EventoStream::Ignorado),
        Some("result") => EventoStream::Resultado {
            texto: valor
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            es_error: valor.get("is_error").and_then(Value::as_bool) == Some(true),
        },
        _ => EventoStream::Ignorado,
    }
}

fn fragmento_de_texto(evento: &Value) -> Option<String> {
    if evento.get("type").and_then(Value::as_str) != Some("content_block_delta") {
        return None;
    }
    let delta = evento.get("delta")?;
    if delta.get("type").and_then(Value::as_str) != Some("text_delta") {
        return None;
    }
    delta
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Decide si el texto de un turno se muestra en vivo (respuesta) o no (petición de tool)
#[derive(Debug, Default)]
pub struct FiltroTextoVisible {
    pendiente: String,
    visible: Option<bool>,
}

impl FiltroTextoVisible {
    pub fn nuevo() -> Self {
        Self::default()
    }

    /// Recibe un fragmento y devuelve el texto que puede mostrarse ya (si lo hay)
    pub fn alimentar(&mut self, fragmento: &str) -> Option<String> {
        match self.visible {
            Some(true) => Some(fragmento.to_string()).filter(|f| !f.is_empty()),
            Some(false) => None,
            None => {
                self.pendiente.push_str(fragmento);
                let primero = self.pendiente.trim_start().chars().next()?;
                let es_herramienta = primero == '{' || primero == '`';
                self.visible = Some(!es_herramienta);
                if es_herramienta {
                    None
                } else {
                    Some(std::mem::take(&mut self.pendiente))
                }
            }
        }
    }

    /// ¿Se ha mostrado (o se mostrará) el texto de este turno?
    pub fn es_visible(&self) -> bool {
        self.visible == Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXTO_REAL: &str = include_str!("../../../tests/fixtures/claude_stream_json/texto.jsonl");
    const HERRAMIENTA: &str =
        include_str!("../../../tests/fixtures/claude_stream_json/herramienta.jsonl");

    fn reproducir(stream: &str) -> (String, Option<(String, bool)>) {
        let mut filtro = FiltroTextoVisible::nuevo();
        let mut mostrado = String::new();
        let mut resultado = None;
        for linea in stream.lines() {
            match interpretar_linea(linea) {
                EventoStream::Fragmento(f) => {
                    if let Some(visible) = filtro.alimentar(&f) {
                        mostrado.push_str(&visible);
                    }
                }
                EventoStream::Resultado { texto, es_error } => resultado = Some((texto, es_error)),
                EventoStream::Ignorado => {}
            }
        }
        (mostrado, resultado)
    }

    #[test]
    fn test_stream_real_de_texto_se_muestra_en_vivo_y_coincide_con_el_resultado() {
        let (mostrado, resultado) = reproducir(TEXTO_REAL);
        assert_eq!(mostrado, "El HA-30 es un hormigón en masa o armado de res");
        assert_eq!(resultado, Some((mostrado, false)));
    }

    #[test]
    fn test_peticion_de_herramienta_no_se_muestra_pero_el_resultado_la_conserva() {
        let (mostrado, resultado) = reproducir(HERRAMIENTA);
        assert!(mostrado.is_empty(), "se filtró JSON de tool: {mostrado}");
        let (texto, es_error) = resultado.expect("result");
        assert!(texto.starts_with("{\"tool_calls\""));
        assert!(!es_error);
    }

    #[test]
    fn test_espacios_iniciales_esperan_al_primer_caracter_significativo() {
        let mut filtro = FiltroTextoVisible::nuevo();
        assert_eq!(filtro.alimentar("\n  "), None);
        assert_eq!(
            filtro.alimentar("## Resultado"),
            Some("\n  ## Resultado".to_string())
        );
        assert_eq!(filtro.alimentar(" final"), Some(" final".to_string()));
        assert!(filtro.es_visible());

        let mut valla = FiltroTextoVisible::nuevo();
        assert_eq!(valla.alimentar("```json\n{"), None);
        assert_eq!(valla.alimentar("\"tool_calls\": []}"), None);
        assert!(!valla.es_visible());
    }

    #[test]
    fn test_lineas_ajenas_corruptas_o_de_error_se_interpretan_sin_panico() {
        for linea in [
            "",
            "no es json",
            "{\"type\":\"system\",\"subtype\":\"init\"}",
            "[1,2]",
        ] {
            assert_eq!(interpretar_linea(linea), EventoStream::Ignorado, "{linea}");
        }
        let error =
            r#"{"type":"result","is_error":true,"result":"Not logged in · Please run /login"}"#;
        assert_eq!(
            interpretar_linea(error),
            EventoStream::Resultado {
                texto: "Not logged in · Please run /login".into(),
                es_error: true
            }
        );
        let delta_no_texto = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"input_json_delta","partial_json":"{"}}}"#;
        assert_eq!(interpretar_linea(delta_no_texto), EventoStream::Ignorado);
    }
}
