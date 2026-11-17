//! Circuit breaker por herramienta dentro de un turno del agente.
//!
//! Regla general, sin listas de nombres: cada herramienta lleva su contador de errores
//! consecutivos. Un error lo incrementa y un éxito de esa misma herramienta lo pone a cero. Con
//! `UMBRAL_ERRORES_CONSECUTIVOS` errores seguidos, la siguiente llamada a esa herramienta no se
//! ejecuta: se devuelve un resultado de error determinista que la deshabilita el resto del turno.
//! El estado vive en el turno: el orquestador crea un circuito nuevo en cada `procesar_turno`.

use std::collections::HashMap;

use serde_json::json;

use crate::application::dtos::tools::{ToolCall, ToolDefinition, ToolResult};

/// Errores consecutivos de una herramienta a partir de los cuales deja de ejecutarse en el turno
pub const UMBRAL_ERRORES_CONSECUTIVOS: u8 = 2;

#[derive(Debug, Default)]
pub struct CircuitoHerramientas {
    errores_consecutivos: HashMap<String, u8>,
}

impl CircuitoHerramientas {
    pub fn abierto(&self, herramienta: &str) -> bool {
        self.errores_consecutivos
            .get(herramienta)
            .is_some_and(|errores| *errores >= UMBRAL_ERRORES_CONSECUTIVOS)
    }

    /// Anota el resultado de una ejecución real: un error suma y un éxito reinicia
    pub fn anotar(&mut self, herramienta: &str, es_error: bool) {
        if es_error {
            let errores = self
                .errores_consecutivos
                .entry(herramienta.to_string())
                .or_default();
            *errores = errores.saturating_add(1);
        } else {
            self.errores_consecutivos.remove(herramienta);
        }
    }

    /// Resultado de error para una llamada bloqueada. Nombra como alternativas las demás
    /// herramientas del turno que siguen habilitadas
    pub fn resultado_bloqueado(
        &self,
        llamada: &ToolCall,
        disponibles: &[ToolDefinition],
    ) -> ToolResult {
        let alternativas: Vec<String> = disponibles
            .iter()
            .map(|d| d.name.as_str())
            .filter(|nombre| *nombre != llamada.name && !self.abierto(nombre))
            .map(|nombre| format!("`{nombre}`"))
            .collect();
        let siguiente = if alternativas.is_empty() {
            "No quedan otras herramientas habilitadas en este turno: responde con la información ya obtenida e indica qué no se ha podido consultar.".to_string()
        } else {
            format!(
                "Resuelve la consulta con las otras herramientas disponibles: {}.",
                alternativas.join(", ")
            )
        };
        ToolResult {
            call_id: llamada.id.clone(),
            tool_name: llamada.name.clone(),
            output: json!({
                "error": format!(
                    "La herramienta `{}` queda deshabilitada durante el resto de este turno por fallos repetidos y esta llamada no se ha ejecutado. {siguiente}",
                    llamada.name
                ),
            }),
            is_error: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definicion(nombre: &str) -> ToolDefinition {
        ToolDefinition {
            name: nombre.to_string(),
            description: String::new(),
            parameters: json!({}),
        }
    }

    fn llamada(nombre: &str) -> ToolCall {
        ToolCall {
            id: "id_1".to_string(),
            name: nombre.to_string(),
            arguments: json!({}),
        }
    }

    #[test]
    fn test_circuito_dos_errores_abren_la_herramienta() {
        let mut circuito = CircuitoHerramientas::default();
        circuito.anotar("a", true);
        assert!(!circuito.abierto("a"));
        circuito.anotar("a", true);
        assert!(circuito.abierto("a"));
    }

    #[test]
    fn test_circuito_un_exito_intermedio_reinicia_el_contador() {
        let mut circuito = CircuitoHerramientas::default();
        circuito.anotar("a", true);
        circuito.anotar("a", false);
        circuito.anotar("a", true);
        assert!(!circuito.abierto("a"));
    }

    #[test]
    fn test_circuito_el_contador_va_por_herramienta() {
        let mut circuito = CircuitoHerramientas::default();
        circuito.anotar("a", true);
        circuito.anotar("b", true);
        circuito.anotar("b", false);
        circuito.anotar("a", true);
        assert!(circuito.abierto("a"));
        assert!(!circuito.abierto("b"));
    }

    #[test]
    fn test_circuito_resultado_bloqueado_es_error_y_nombra_las_alternativas_habilitadas() {
        let mut circuito = CircuitoHerramientas::default();
        for nombre in ["a", "a", "c", "c"] {
            circuito.anotar(nombre, true);
        }
        let disponibles = [definicion("a"), definicion("b"), definicion("c")];
        let resultado = circuito.resultado_bloqueado(&llamada("a"), &disponibles);
        assert!(resultado.is_error);
        assert_eq!(resultado.tool_name, "a");
        assert_eq!(resultado.call_id, "id_1");
        let texto = resultado.output["error"].as_str().unwrap();
        assert!(texto.contains("`a` queda deshabilitada"));
        assert!(texto.contains("`b`"));
        assert!(!texto.contains("`c`"));
        // Sin cifras: el texto no puede anclar un número de la respuesta
        assert!(!texto.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_circuito_sin_alternativas_lo_dice() {
        let mut circuito = CircuitoHerramientas::default();
        circuito.anotar("a", true);
        circuito.anotar("a", true);
        let resultado = circuito.resultado_bloqueado(&llamada("a"), &[definicion("a")]);
        let texto = resultado.output["error"].as_str().unwrap();
        assert!(texto.contains("No quedan otras herramientas habilitadas"));
    }
}
