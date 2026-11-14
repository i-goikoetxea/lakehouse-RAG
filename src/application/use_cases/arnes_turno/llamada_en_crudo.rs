//! Llamadas a herramientas escritas dentro de la prosa (SSD de generalidad §3b). La regla es
//! estructural: un objeto JSON con las claves del protocolo de la app, nunca una lista de palabras.

use serde_json::Value;

use crate::application::dtos::tools::ToolCall;

/// Claves del protocolo: `{"tool_calls": [...]}` o `{"type": "tool_use", ...}`
const CLAVE_LLAMADAS: &str = "\"tool_calls\"";
const VALOR_TOOL_USE: &str = "\"tool_use\"";
/// Un objeto del protocolo abre como mucho unas llaves antes de su clave: acota el coste en un
/// texto hostil con miles de `{`
const MAXIMO_LLAVES_PREVIAS: usize = 8;
/// Más claves que estas no son una respuesta: se evalúan solo las primeras
const MAXIMO_CLAVES: usize = 64;
/// Bytes que se leen como mucho para un objeto del protocolo: una llamada no ocupa más. Acota el
/// coste de las MAXIMO_CLAVES · MAXIMO_LLAVES_PREVIAS lecturas en un JSON enorme sin cerrar.
const MAXIMO_BYTES_OBJETO: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum LlamadaEmbebida {
    /// Uno o más objetos completos del protocolo, en orden de aparición
    Completa(Vec<ToolCall>),
    /// La clave del protocolo aparece, pero ningún objeto se puede leer entero (JSON truncado)
    Parcial,
}

/// Busca objetos del protocolo en cualquier punto del texto, también dentro de una valla ```.
pub fn detectar(texto: &str) -> Option<LlamadaEmbebida> {
    let claves = posiciones_de_claves(texto);
    if claves.is_empty() {
        return None;
    }
    let llamadas = llamadas_embebidas(texto, &claves);
    if !llamadas.is_empty() {
        return Some(LlamadaEmbebida::Completa(llamadas));
    }
    claves
        .iter()
        .any(|&(inicio, longitud)| va_seguida_de_dos_puntos_o_es_valor(texto, inicio, longitud))
        .then_some(LlamadaEmbebida::Parcial)
}

/// Llamadas de un objeto JSON del protocolo. Una llamada sin nombre no cuenta.
pub fn llamadas_de_valor(valor: &Value) -> Vec<ToolCall> {
    if let Some(lista) = valor.get("tool_calls").and_then(Value::as_array) {
        return lista
            .iter()
            .enumerate()
            .filter_map(|(indice, llamada)| llamada_con_id(llamada, format!("call_{indice}")))
            .collect();
    }
    if valor.get("type").and_then(Value::as_str) == Some("tool_use") {
        return llamada_con_id(valor, "call_0".to_string())
            .into_iter()
            .collect();
    }
    Vec::new()
}

fn llamada_con_id(llamada: &Value, id_por_defecto: String) -> Option<ToolCall> {
    let nombre = llamada.get("name").and_then(Value::as_str).unwrap_or("");
    if nombre.is_empty() {
        return None;
    }
    Some(ToolCall {
        id: llamada
            .get("id")
            .and_then(Value::as_str)
            .map_or(id_por_defecto, str::to_string),
        name: nombre.to_string(),
        arguments: llamada
            .get("arguments")
            .or_else(|| llamada.get("input"))
            .cloned()
            .unwrap_or(Value::Null),
    })
}

/// Inicio y longitud de cada clave o valor del protocolo en el texto
fn posiciones_de_claves(texto: &str) -> Vec<(usize, usize)> {
    let mut posiciones: Vec<(usize, usize)> = [CLAVE_LLAMADAS, VALOR_TOOL_USE]
        .iter()
        .flat_map(|clave| texto.match_indices(clave).map(|(i, c)| (i, c.len())))
        .collect();
    posiciones.sort_unstable();
    posiciones.truncate(MAXIMO_CLAVES);
    posiciones
}

fn llamadas_embebidas(texto: &str, claves: &[(usize, usize)]) -> Vec<ToolCall> {
    let mut llamadas = Vec::new();
    let mut leido_hasta = 0;
    for &(inicio_clave, _) in claves {
        if inicio_clave < leido_hasta {
            continue;
        }
        if let Some((fin, encontradas)) = objeto_que_contiene(texto, inicio_clave, leido_hasta) {
            llamadas.extend(encontradas);
            leido_hasta = fin;
        }
    }
    llamadas
}

/// El objeto del protocolo más interior que contiene la clave: se prueban las llaves anteriores,
/// de la más cercana a la más lejana. Devuelve dónde termina y sus llamadas.
fn objeto_que_contiene(
    texto: &str,
    inicio_clave: usize,
    desde: usize,
) -> Option<(usize, Vec<ToolCall>)> {
    texto[desde..inicio_clave]
        .rmatch_indices('{')
        .take(MAXIMO_LLAVES_PREVIAS)
        .find_map(|(relativo, _)| {
            let inicio = desde + relativo;
            let mut fin_ventana = (inicio + MAXIMO_BYTES_OBJETO).min(texto.len());
            while !texto.is_char_boundary(fin_ventana) {
                fin_ventana -= 1;
            }
            let mut lector = serde_json::Deserializer::from_str(&texto[inicio..fin_ventana])
                .into_iter::<Value>();
            let valor = lector.next()?.ok()?;
            let fin = inicio + lector.byte_offset();
            let llamadas = llamadas_de_valor(&valor);
            (fin > inicio_clave && !llamadas.is_empty()).then_some((fin, llamadas))
        })
}

/// `"tool_calls":` es una clave; `"tool_use"` tras `"type":` es un valor. La mención suelta de la
/// palabra en una frase no lleva comillas JSON ni dos puntos.
fn va_seguida_de_dos_puntos_o_es_valor(texto: &str, inicio: usize, longitud: usize) -> bool {
    let clave = &texto[inicio..inicio + longitud];
    if clave == CLAVE_LLAMADAS {
        return texto[inicio + longitud..].trim_start().starts_with(':');
    }
    texto[..inicio].trim_end().ends_with(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    const LLAMADA: &str = r#"{"tool_calls": [{"id": "call_1", "name": "search_normative_context", "arguments": {"consulta": "x"}}]}"#;

    fn nombres(texto: &str) -> Vec<String> {
        match detectar(texto) {
            Some(LlamadaEmbebida::Completa(llamadas)) => {
                llamadas.into_iter().map(|l| l.name).collect()
            }
            otro => panic!("se esperaba una llamada completa: {otro:?}"),
        }
    }

    #[test]
    fn test_prosa_seguida_de_una_valla_json_completa_es_una_llamada() {
        for prosa in [
            "Voy a buscar en el documento.",
            "I will look it up in the document.",
            "文書を検索します。",
        ] {
            let texto = format!("{prosa}\n\n```json\n{LLAMADA}\n```");
            assert_eq!(nombres(&texto), vec!["search_normative_context"], "{prosa}");
        }
    }

    #[test]
    fn test_objeto_en_medio_de_la_prosa_sin_valla_es_una_llamada() {
        let texto = format!("Primero consulto {LLAMADA} y luego respondo.");
        assert_eq!(nombres(&texto), vec!["search_normative_context"]);
    }

    #[test]
    fn test_dos_bloques_son_dos_llamadas_en_orden() {
        let segunda =
            r#"{"type": "tool_use", "id": "c2", "name": "inspect_lakehouse_catalog", "input": {}}"#;
        let texto = format!("Paso 1:\n```json\n{LLAMADA}\n```\nPaso 2:\n```\n{segunda}\n```");
        assert_eq!(
            nombres(&texto),
            vec!["search_normative_context", "inspect_lakehouse_catalog"]
        );
    }

    #[test]
    fn test_tool_use_con_el_tipo_despues_de_otras_claves() {
        let texto = r#"Llamo: {"id": "c9", "name": "lookup_double_entry_table", "type": "tool_use", "input": {"valor_x": 3}}"#;
        assert_eq!(nombres(texto), vec!["lookup_double_entry_table"]);
    }

    #[test]
    fn test_json_truncado_es_parcial() {
        let truncado = &LLAMADA[..LLAMADA.len() - 12];
        for texto in [
            format!("Busco:\n```json\n{truncado}"),
            "{\"tool_calls\" : [".to_string(),
            "Respuesta {\"type\": \"tool_use\", \"name\": \"x\"".to_string(),
        ] {
            assert_eq!(detectar(&texto), Some(LlamadaEmbebida::Parcial), "{texto}");
        }
    }

    #[test]
    fn test_json_sin_claves_del_protocolo_no_es_una_llamada() {
        for texto in [
            "Ejemplo de configuración:\n```json\n{\"umbral\": 30, \"modo\": \"estricto\"}\n```",
            "El campo tool_calls del protocolo es una lista.",
            "La palabra \"tool_calls\" aparece entre comillas en esta frase.",
            "",
        ] {
            assert_eq!(detectar(texto), None, "{texto}");
        }
    }

    #[test]
    fn test_objeto_del_protocolo_sin_llamadas_validas_es_parcial() {
        for texto in [
            "Valor {\"tool_calls\": []}",
            "Sin nombre {\"tool_calls\": [{\"id\": \"a\"}]}",
        ] {
            assert_eq!(detectar(texto), Some(LlamadaEmbebida::Parcial), "{texto}");
        }
    }

    #[test]
    fn test_texto_hostil_con_miles_de_llaves_termina() {
        let hostil = format!("{}\"tool_calls\": [", "{".repeat(200_000));
        assert_eq!(detectar(&hostil), Some(LlamadaEmbebida::Parcial));
        let muchas_claves = "{\"tool_calls\" x ".repeat(20_000);
        assert_eq!(detectar(&muchas_claves), None);
    }

    #[test]
    fn test_llamadas_de_valor_conserva_el_id_y_los_argumentos() {
        let valor: Value = serde_json::from_str(LLAMADA).unwrap();
        let llamadas = llamadas_de_valor(&valor);
        assert_eq!(llamadas[0].id, "call_1");
        assert_eq!(llamadas[0].arguments["consulta"], "x");
    }
}
