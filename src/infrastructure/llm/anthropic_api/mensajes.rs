//! Tipos propios de `POST /v1/messages` (sin SDK de la comunidad). Solo lo que usa la Fase 5.1:
//! un turno sin streaming con tools y prompt caching.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `{"type": "ephemeral"}`: punto de caché al final del prefijo estable
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlCache {
    #[serde(rename = "type")]
    pub tipo: String,
}

impl ControlCache {
    pub fn efimera() -> Self {
        Self {
            tipo: "ephemeral".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BloqueSistema {
    #[serde(rename = "type")]
    pub tipo: &'static str,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<ControlCache>,
}

impl BloqueSistema {
    pub fn texto(text: impl Into<String>, cache_control: Option<ControlCache>) -> Self {
        Self {
            tipo: "text",
            text: text.into(),
            cache_control,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DefinicionTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub strict: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<ControlCache>,
}

/// `auto`, `any`, `tool` (con nombre) o `none`
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EleccionTool {
    Auto,
    Any,
    Tool { name: String },
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MensajeEntrada {
    pub role: &'static str,
    pub content: String,
}

impl MensajeEntrada {
    pub fn usuario(content: impl Into<String>) -> Self {
        Self {
            role: "user",
            content: content.into(),
        }
    }
}

/// Cuerpo de la petición. Sin `temperature`: Claude Haiku 5.5 y la versión `2026-10-21` la
/// rechazan (SSD de la Fase 5.1 §1)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PeticionMensajes {
    pub model: String,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub system: Vec<BloqueSistema>,
    pub messages: Vec<MensajeEntrada>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<DefinicionTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<EleccionTool>,
}

/// Bloques de la respuesta, leídos por `type` y nunca por posición (puede haber `thinking`)
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BloqueRespuesta {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(other)]
    Otro,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct Uso {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

/// Precio por millón de tokens de un modelo (entrada, escritura de caché de 5 min, lectura, salida)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TarifaModelo {
    pub entrada: f64,
    pub escritura_cache: f64,
    pub lectura_cache: f64,
    pub salida: f64,
}

/// Claude Haiku 5.5 con prompts ≤ 100 K tokens: 0,10 / 0,50 $/MTok; caché ×1,25 y ×0,1
pub const TARIFA_HAIKU_5_5: TarifaModelo = TarifaModelo {
    entrada: 0.10,
    escritura_cache: 0.125,
    lectura_cache: 0.01,
    salida: 0.50,
};

impl Uso {
    pub fn coste_estimado_usd(&self, tarifa: &TarifaModelo) -> f64 {
        let millon = 1_000_000.0;
        (self.input_tokens as f64 * tarifa.entrada
            + self.cache_creation_input_tokens as f64 * tarifa.escritura_cache
            + self.cache_read_input_tokens as f64 * tarifa.lectura_cache
            + self.output_tokens as f64 * tarifa.salida)
            / millon
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RespuestaMensajes {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub model: String,
    pub content: Vec<BloqueRespuesta>,
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Uso,
}

impl RespuestaMensajes {
    /// Entrada del primer `tool_use` con ese nombre
    pub fn entrada_de_tool(&self, nombre: &str) -> Option<&Value> {
        self.content.iter().find_map(|bloque| match bloque {
            BloqueRespuesta::ToolUse { name, input, .. } if name == nombre => Some(input),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_la_peticion_serializa_tool_choice_forzado_y_cache() {
        let peticion = PeticionMensajes {
            model: "m".into(),
            max_tokens: 10,
            system: vec![BloqueSistema::texto("s", Some(ControlCache::efimera()))],
            messages: vec![MensajeEntrada::usuario("hola")],
            tools: vec![DefinicionTool {
                name: "t".into(),
                description: "d".into(),
                input_schema: json!({"type": "object"}),
                strict: true,
                cache_control: Some(ControlCache::efimera()),
            }],
            tool_choice: Some(EleccionTool::Tool { name: "t".into() }),
        };
        let cuerpo = serde_json::to_value(&peticion).unwrap();
        assert_eq!(cuerpo["tool_choice"], json!({"type": "tool", "name": "t"}));
        assert_eq!(
            cuerpo["system"][0]["cache_control"],
            json!({"type": "ephemeral"})
        );
        assert_eq!(
            cuerpo["tools"][0]["cache_control"],
            json!({"type": "ephemeral"})
        );
        assert_eq!(cuerpo["tools"][0]["strict"], true);
        assert_eq!(
            cuerpo["messages"][0],
            json!({"role": "user", "content": "hola"})
        );
        assert!(cuerpo.get("temperature").is_none());
    }

    #[test]
    fn test_la_respuesta_se_lee_por_tipo_con_thinking_delante() {
        let respuesta: RespuestaMensajes = serde_json::from_value(json!({
            "id": "msg_1", "model": "m", "stop_reason": "tool_use",
            "content": [
                { "type": "thinking", "thinking": "", "signature": "x" },
                { "type": "tool_use", "id": "tu_1", "name": "otra", "input": {} },
                { "type": "tool_use", "id": "tu_2", "name": "emitir", "input": { "a": 1 } },
            ],
            "usage": { "input_tokens": 3, "cache_read_input_tokens": 5, "output_tokens": 7 },
        }))
        .unwrap();
        assert_eq!(
            respuesta.entrada_de_tool("emitir"),
            Some(&json!({ "a": 1 }))
        );
        assert_eq!(respuesta.entrada_de_tool("ninguna"), None);
        assert_eq!(respuesta.usage.cache_read_input_tokens, 5);
        assert_eq!(respuesta.usage.cache_creation_input_tokens, 0);
    }

    #[test]
    fn test_coste_estimado_con_la_tarifa_de_haiku() {
        let uso = Uso {
            input_tokens: 1_000_000,
            cache_creation_input_tokens: 1_000_000,
            cache_read_input_tokens: 1_000_000,
            output_tokens: 1_000_000,
        };
        let coste = uso.coste_estimado_usd(&TARIFA_HAIKU_5_5);
        assert!((coste - 0.735).abs() < 1e-9, "{coste}");
    }
}
