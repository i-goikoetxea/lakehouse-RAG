//! Deduplicación de llamadas idénticas dentro de un turno.
//!
//! Regla general, sin nombres de herramienta: la clave es (herramienta, argumentos canónicos),
//! con el JSON de los argumentos en forma canónica (claves ordenadas y sin espacios). Si el
//! modelo repite exactamente una llamada ya ejecutada en el turno, no se ejecuta otra vez: recibe
//! un resultado determinista que remite al que ya tiene en la conversación.
//!
//! Lo que ve el arnés no cambia: la llamada original ya está en el registro del turno (guard y
//! procedencia la cuentan como hecha) y la repetida no se registra.
//!
//! Con el circuit breaker, la repetición hereda el desenlace del original: repetir una llamada
//! que falló cuenta como otro error de esa herramienta (el modelo insiste en el mismo fallo) y
//! repetir una que acertó no cuenta nada (no hay una ejecución nueva que reinicie el contador).
//!
//! El estado vive en el turno: el orquestador crea uno nuevo en cada `procesar_turno`.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::application::dtos::tools::{ToolCall, ToolResult};

#[derive(Debug, Default)]
pub struct LlamadasDelTurno {
    /// (herramienta, argumentos canónicos) → si su ejecución fue un error
    ejecutadas: HashMap<(String, String), bool>,
}

impl LlamadasDelTurno {
    fn clave(llamada: &ToolCall) -> (String, String) {
        (llamada.name.clone(), json_canonico(&llamada.arguments))
    }

    /// Si la llamada repite una ya ejecutada en el turno, si aquella fue un error
    pub fn repetida(&self, llamada: &ToolCall) -> Option<bool> {
        self.ejecutadas.get(&Self::clave(llamada)).copied()
    }

    /// Anota una ejecución real
    pub fn anotar(&mut self, llamada: &ToolCall, es_error: bool) {
        self.ejecutadas.insert(Self::clave(llamada), es_error);
    }

    /// Resultado determinista de una llamada repetida. Conserva el carácter de error del original
    pub fn resultado_repetido(llamada: &ToolCall, original_es_error: bool) -> ToolResult {
        let texto = format!(
            "Esta llamada repite exactamente una consulta ya ejecutada en este turno (`{}` con los mismos argumentos) y no se ha vuelto a ejecutar: su resultado sería el mismo que ya tienes en la conversación. Varía los parámetros, usa otra herramienta o concluye con la evidencia disponible.",
            llamada.name
        );
        let clave = if original_es_error { "error" } else { "aviso" };
        ToolResult {
            call_id: llamada.id.clone(),
            tool_name: llamada.name.clone(),
            output: json!({ clave: texto }),
            is_error: original_es_error,
        }
    }
}

/// JSON con las claves de cada objeto ordenadas y sin espacios, a cualquier profundidad.
/// No depende del orden en que el modelo escribió las claves ni de la configuración de serde_json
pub fn json_canonico(valor: &Value) -> String {
    let mut salida = String::new();
    escribir_canonico(valor, &mut salida);
    salida
}

fn escribir_canonico(valor: &Value, salida: &mut String) {
    match valor {
        Value::Object(mapa) => {
            let mut claves: Vec<&String> = mapa.keys().collect();
            claves.sort();
            salida.push('{');
            for (i, clave) in claves.into_iter().enumerate() {
                if i > 0 {
                    salida.push(',');
                }
                salida.push_str(&Value::String(clave.clone()).to_string());
                salida.push(':');
                escribir_canonico(&mapa[clave], salida);
            }
            salida.push('}');
        }
        Value::Array(elementos) => {
            salida.push('[');
            for (i, elemento) in elementos.iter().enumerate() {
                if i > 0 {
                    salida.push(',');
                }
                escribir_canonico(elemento, salida);
            }
            salida.push(']');
        }
        escalar => salida.push_str(&escalar.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn llamada(nombre: &str, argumentos: Value) -> ToolCall {
        ToolCall {
            id: "id_1".to_string(),
            name: nombre.to_string(),
            arguments: argumentos,
        }
    }

    #[test]
    fn test_json_canonico_ordena_claves_a_cualquier_profundidad_y_quita_espacios() {
        let a: Value =
            serde_json::from_str(r#"{ "b": 1, "a": { "y": [ {"q": 2, "p": 1} ], "x": "t" } }"#)
                .unwrap();
        assert_eq!(
            json_canonico(&a),
            r#"{"a":{"x":"t","y":[{"p":1,"q":2}]},"b":1}"#
        );
    }

    #[test]
    fn test_llamada_identica_con_claves_en_otro_orden_es_repetida() {
        let mut turno = LlamadasDelTurno::default();
        turno.anotar(&llamada("a", json!({ "x": 1, "y": "z" })), false);
        let reordenada: Value = serde_json::from_str(r#"{"y":"z","x":1}"#).unwrap();
        assert_eq!(turno.repetida(&llamada("a", reordenada)), Some(false));
    }

    #[test]
    fn test_otros_argumentos_u_otra_herramienta_no_son_repetidas() {
        let mut turno = LlamadasDelTurno::default();
        turno.anotar(&llamada("a", json!({ "x": 1 })), true);
        assert_eq!(turno.repetida(&llamada("a", json!({ "x": 2 }))), None);
        assert_eq!(turno.repetida(&llamada("b", json!({ "x": 1 }))), None);
        // El orden de un array sí importa
        turno.anotar(&llamada("a", json!({ "l": [1, 2] })), false);
        assert_eq!(turno.repetida(&llamada("a", json!({ "l": [2, 1] }))), None);
        assert_eq!(turno.repetida(&llamada("a", json!({ "x": 1 }))), Some(true));
    }

    #[test]
    fn test_resultado_repetido_conserva_el_error_del_original_y_no_lleva_cifras() {
        let repetida = llamada("a", json!({}));
        for original_es_error in [false, true] {
            let resultado = LlamadasDelTurno::resultado_repetido(&repetida, original_es_error);
            assert_eq!(resultado.is_error, original_es_error);
            assert_eq!(resultado.call_id, "id_1");
            let clave = if original_es_error { "error" } else { "aviso" };
            let texto = resultado.output[clave].as_str().unwrap();
            assert!(texto.contains("ya ejecutada en este turno"));
            assert!(!texto.chars().any(|c| c.is_ascii_digit()));
        }
    }
}
