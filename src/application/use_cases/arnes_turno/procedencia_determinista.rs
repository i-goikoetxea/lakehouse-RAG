//! Nivel 1 del verificador de procedencia (Pieza 4): reglas deterministas, sin Claude, para las
//! cifras de la respuesta que no aparecen literalmente en las salidas del turno.
//!
//! - **Clausura aritmética de pares:** la cifra es la suma, la resta o la variación porcentual de
//!   dos cifras de la misma salida, citadas cerca de ella en la respuesta.
//! - **Referencia estructural:** el par (palabra anterior, cifra), o la cifra tras `§`, `#` o `-`,
//!   está literalmente en las salidas.
//!
//! Nada de listas de palabras: posiciones, oraciones por tipografía y estructura del JSON.

use std::collections::HashSet;

use serde_json::Value;

use super::cifras::{CifraHallada, extraer_cifras};
use super::texto_normalizado::plegar_texto;
use crate::application::dtos::tools::ToolResult;
use crate::domain::services::marcas_pagina::quitar_marcas;

/// Distancia, en caracteres, a la que una cifra citada cuenta como «cerca» de la derivada
pub(super) const VENTANA_DE_CITA: usize = 120;
/// Tope de operandos que se consideran: acota el coste ante una salida hostil
const MAXIMO_OPERANDOS: usize = 2000;
/// Tope de cifras citadas cerca de la derivada, y de operandos citados, que se combinan en pares
const MAXIMO_CITAS: usize = 64;
/// Tope de bytes de texto de las salidas en que se buscan las referencias estructurales
const MAXIMO_BYTES_DE_REFERENCIA: usize = 256 * 1024;
/// Un entero de 4 cifras en este intervalo se trata como año: nunca es operando
const ANIOS: std::ops::RangeInclusive<u64> = 1800..=2199;
const MARCADORES_ESTRUCTURALES: &[char] = &['§', '#', '-'];
/// Estados del protocolo de las tools que significan que no hubo resultado
const ESTADOS_SIN_RESULTADO: &[&str] = &["sin_coincidencia", "no_consta"];
const CLAVE_ESTADO: &str = "estado";
const CLAVE_DOC_ID: &str = "doc_id";

/// Una cifra de una salida de tool que puede ser operando de una derivación
#[derive(Debug, Clone)]
struct Operando {
    valor: f64,
    /// Decimales con que está escrito en la salida, en esta lectura
    decimales: i32,
    lecturas: Vec<String>,
    /// Objeto JSON (resultado, chunk o fila) del que sale
    unidad: usize,
    doc: Option<String>,
}

/// Lo que el Nivel 1 necesita de las salidas del turno, calculado una vez por respuesta
#[derive(Debug, Default)]
pub(super) struct ContextoProcedencia {
    operandos: Vec<Operando>,
    /// Textos de las salidas, plegados y con espacios simples, para las referencias estructurales
    textos: Vec<String>,
    bytes_de_texto: usize,
}

impl ContextoProcedencia {
    pub(super) fn desde<'a>(salidas: impl IntoIterator<Item = &'a ToolResult>) -> Self {
        let mut contexto = Self::default();
        let mut siguiente_unidad = 0;
        let salidas: Vec<&ToolResult> = salidas.into_iter().collect();
        let abstencion_total = !salidas.is_empty() && salidas.iter().all(|s| salida_vacia(s));
        for salida in salidas.iter().filter(|s| !s.is_error) {
            recorrer_textos_planos(&salida.output, |texto| {
                let restante = MAXIMO_BYTES_DE_REFERENCIA.saturating_sub(contexto.bytes_de_texto);
                if restante > 0 {
                    let texto = prefijo_de_bytes(&espacios_simples(&plegar_texto(texto)), restante);
                    contexto.bytes_de_texto += texto.len();
                    contexto.textos.push(texto);
                }
            });
            if !abstencion_total {
                contexto.recoger_operandos(&salida.output, &mut siguiente_unidad);
            }
        }
        contexto.operandos.truncate(MAXIMO_OPERANDOS);
        contexto
    }

    /// Cada objeto JSON es una unidad: un resultado, un chunk o una fila de SQL. Sus campos
    /// escalares, incluido su texto, son sus operandos. El `doc_id` se hereda del objeto padre o
    /// se toma de un objeto hijo directo (`cita`).
    fn recoger_operandos(&mut self, raiz: &Value, siguiente_unidad: &mut usize) {
        let mut pendientes: Vec<(&Value, Option<String>)> = vec![(raiz, None)];
        while let Some((valor, doc_heredado)) = pendientes.pop() {
            let unidad = *siguiente_unidad;
            match valor {
                Value::Array(elementos) => {
                    pendientes.extend(elementos.iter().map(|e| (e, doc_heredado.clone())))
                }
                Value::Object(campos) => {
                    *siguiente_unidad += 1;
                    let doc = doc_de_objeto(valor).or(doc_heredado);
                    for (clave, contenido) in campos {
                        match contenido {
                            Value::String(_) | Value::Number(_) => {
                                self.operandos_de_escalar(contenido, unidad, &doc)
                            }
                            Value::Array(escalares) if escalares.iter().all(es_escalar) => {
                                for escalar in escalares {
                                    self.operandos_de_escalar(escalar, unidad, &doc);
                                }
                            }
                            otro if clave != CLAVE_ESTADO => pendientes.push((otro, doc.clone())),
                            _ => {}
                        }
                    }
                }
                Value::String(_) | Value::Number(_) => {
                    *siguiente_unidad += 1;
                    self.operandos_de_escalar(valor, unidad, &doc_heredado);
                }
                Value::Null | Value::Bool(_) => {}
            }
        }
    }

    fn operandos_de_escalar(&mut self, escalar: &Value, unidad: usize, doc: &Option<String>) {
        let texto = match escalar {
            Value::String(texto) => quitar_marcas(texto),
            Value::Number(numero) => numero.to_string(),
            _ => return,
        };
        for linea in texto.lines() {
            let letras: Vec<char> = linea.chars().collect();
            for cifra in extraer_cifras(linea) {
                if !es_codigo_de_seccion(&letras, &cifra) {
                    self.empujar_cifra(cifra, unidad, doc);
                }
            }
        }
    }

    fn empujar_cifra(&mut self, cifra: CifraHallada, unidad: usize, doc: &Option<String>) {
        if self.operandos.len() >= MAXIMO_OPERANDOS || es_anio_o_fecha(&cifra) {
            return;
        }
        for lectura in &cifra.lecturas {
            if let Ok(valor) = lectura.parse::<f64>() {
                self.operandos.push(Operando {
                    valor,
                    decimales: decimales_escritos(&cifra, lectura),
                    lecturas: cifra.lecturas.clone(),
                    unidad,
                    doc: doc.clone(),
                });
            }
        }
    }

    pub(super) fn sin_operandos(&self) -> bool {
        self.operandos.is_empty()
    }
}

fn es_escalar(valor: &Value) -> bool {
    matches!(valor, Value::String(_) | Value::Number(_))
}

fn doc_de_objeto(objeto: &Value) -> Option<String> {
    let directo = objeto.get(CLAVE_DOC_ID).and_then(Value::as_str);
    let en_hijo = || {
        objeto
            .as_object()?
            .values()
            .find_map(|hijo| hijo.as_object()?.get(CLAVE_DOC_ID).and_then(Value::as_str))
    };
    directo.or_else(en_hijo).map(str::to_string)
}

/// Una salida sin resultado: error, estado del protocolo de «no hay», o todas sus listas vacías
pub(super) fn salida_vacia(salida: &ToolResult) -> bool {
    if salida.is_error {
        return true;
    }
    let estado = salida
        .output
        .get(CLAVE_ESTADO)
        .and_then(Value::as_str)
        .map(str::to_lowercase);
    if estado.is_some_and(|e| ESTADOS_SIN_RESULTADO.contains(&e.as_str())) {
        return true;
    }
    let Some(campos) = salida.output.as_object() else {
        return false;
    };
    let mut listas = campos.values().filter_map(Value::as_array).peekable();
    listas.peek().is_some() && listas.all(Vec::is_empty)
}

fn es_anio_o_fecha(cifra: &CifraHallada) -> bool {
    cifra.lecturas.iter().any(|lectura| {
        lectura.contains('-') && lectura.len() > 4
            || (lectura.len() == 4
                && lectura
                    .parse::<u64>()
                    .is_ok_and(|anio| ANIOS.contains(&anio)))
    })
}

/// `5.2 Alcance` al inicio de una línea, o `§ 21`, es un código de sección y no un dato
fn es_codigo_de_seccion(letras: &[char], cifra: &CifraHallada) -> bool {
    let inicio = cifra.posicion;
    let tras_marcador = letras[..inicio]
        .iter()
        .rev()
        .find(|l| !l.is_whitespace())
        .is_some_and(|l| *l == '§');
    let abre_linea = letras[..inicio]
        .iter()
        .all(|l| l.is_whitespace() || matches!(l, '#' | '*' | '>' | '-'));
    // `4.2 Alcance`: abre la línea y le sigue un título en mayúscula; `1482.6` suelto es un dato
    let fin = inicio + cifra.literal.chars().count();
    let titulo_detras = letras[fin.min(letras.len())..]
        .iter()
        .find(|l| !l.is_whitespace())
        .is_some_and(|l| l.is_uppercase())
        && letras.get(fin).is_some_and(|l| l.is_whitespace());
    tras_marcador || (abre_linea && cifra.literal.contains(['.', ',']) && titulo_detras)
}

fn recorrer_textos_planos(raiz: &Value, mut visitar: impl FnMut(&str)) {
    let mut pendientes = vec![raiz];
    while let Some(valor) = pendientes.pop() {
        match valor {
            Value::String(texto) => visitar(&quitar_marcas(texto)),
            Value::Array(elementos) => pendientes.extend(elementos),
            Value::Object(campos) => pendientes.extend(campos.values()),
            _ => {}
        }
    }
}

/// Los primeros `maximo` bytes del texto, sin cortar un carácter
fn prefijo_de_bytes(texto: &str, maximo: usize) -> String {
    let mut fin = maximo.min(texto.len());
    while !texto.is_char_boundary(fin) {
        fin -= 1;
    }
    texto[..fin].to_string()
}

fn espacios_simples(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// La respuesta, troceada en oraciones por tipografía
pub(super) struct RespuestaAnalizada {
    letras: Vec<char>,
    pub(super) cifras: Vec<CifraHallada>,
    /// Inicio y fin (en caracteres) de cada oración
    oraciones: Vec<(usize, usize)>,
}

impl RespuestaAnalizada {
    pub(super) fn nueva(respuesta: &str) -> Self {
        let letras: Vec<char> = respuesta.chars().collect();
        Self {
            cifras: extraer_cifras(respuesta),
            oraciones: oraciones(&letras),
            letras,
        }
    }

    pub(super) fn oraciones(&self) -> impl Iterator<Item = String> + '_ {
        self.oraciones
            .iter()
            .map(|(inicio, fin)| self.letras[*inicio..*fin].iter().collect())
    }

    pub(super) fn oracion_de(&self, posicion: usize) -> usize {
        // Oraciones ordenadas y contiguas: búsqueda binaria, no un recorrido por cifra
        self.oraciones
            .partition_point(|(_, fin)| *fin <= posicion)
            .min(self.oraciones.len().saturating_sub(1))
    }

    /// Tipografía: `12,5 %` o `12,5%` es un porcentaje
    fn va_seguida_de_porcentaje(&self, cifra: &CifraHallada) -> bool {
        let fin = cifra.posicion + cifra.literal.trim_start_matches('-').chars().count();
        self.letras[fin.min(self.letras.len())..]
            .iter()
            .find(|l| !matches!(l, ' ' | '\u{00A0}' | '\u{202F}'))
            .is_some_and(|l| matches!(l, '%' | '٪'))
    }

    fn cerca(&self, una: usize, otra: usize) -> bool {
        una.abs_diff(otra) <= VENTANA_DE_CITA || self.oracion_de(una) == self.oracion_de(otra)
    }
}

/// Una oración termina en `.` `!` `?` (o sus formas CJK) seguidos de un blanco, o en un salto de
/// línea. El punto de `1.250` no corta.
fn oraciones(letras: &[char]) -> Vec<(usize, usize)> {
    let mut oraciones = Vec::new();
    let mut inicio = 0;
    for (posicion, letra) in letras.iter().enumerate() {
        let siguiente_blanca = letras.get(posicion + 1).is_none_or(|l| l.is_whitespace());
        let corta = *letra == '\n'
            || matches!(letra, '。' | '！' | '？')
            || (matches!(letra, '.' | '!' | '?') && siguiente_blanca);
        if corta {
            oraciones.push((inicio, posicion + 1));
            inicio = posicion + 1;
        }
    }
    if inicio < letras.len() || oraciones.is_empty() {
        oraciones.push((inicio, letras.len()));
    }
    oraciones
}

/// Decimales con que está escrita la cifra en la lectura dada (0 si la lectura es entera)
fn decimales_escritos(cifra: &CifraHallada, lectura: &str) -> i32 {
    if !lectura.contains('.') {
        return 0;
    }
    cifra.literal.rsplit(['.', ',']).next().map_or(0, |parte| {
        parte.chars().filter(char::is_ascii_digit).count() as i32
    })
}

/// ¿La cifra `indice` de la respuesta es suma, resta o variación porcentual de dos operandos?
/// `vetada(posicion)` excluye las citas que no pueden respaldar (las de una oración de
/// abstención).
pub(super) fn es_derivada(
    respuesta: &RespuestaAnalizada,
    indice: usize,
    contexto: &ContextoProcedencia,
    vetada: impl Fn(usize) -> bool,
) -> bool {
    let cifra = &respuesta.cifras[indice];
    let citadas: Vec<&CifraHallada> = respuesta
        .cifras
        .iter()
        .enumerate()
        .filter(|(otro, citada)| {
            *otro != indice
                && respuesta.cerca(citada.posicion, cifra.posicion)
                && !vetada(citada.posicion)
        })
        .map(|(_, citada)| citada)
        .take(MAXIMO_CITAS)
        .collect();
    let lecturas_citadas: HashSet<&String> =
        citadas.iter().flat_map(|c| c.lecturas.iter()).collect();
    // Solo se combinan operandos citados: los pares son a lo sumo MAXIMO_CITAS²
    let operandos: Vec<&Operando> = contexto
        .operandos
        .iter()
        .filter(|operando| {
            operando
                .lecturas
                .iter()
                .any(|l| lecturas_citadas.contains(l))
        })
        .take(MAXIMO_CITAS)
        .collect();
    if operandos.is_empty() {
        return false;
    }
    cifra.lecturas.iter().any(|lectura| {
        let Ok(objetivo) = lectura.parse::<f64>() else {
            return false;
        };
        let decimales = decimales_escritos(cifra, lectura);
        let tolerancia = 0.5 * 10f64.powi(-decimales) + 1e-9;
        let encaja = |valor: f64| (valor - objetivo).abs() <= tolerancia;
        let es_porcentaje = respuesta.va_seguida_de_porcentaje(cifra);
        // Los dos operandos van citados cerca de la cifra (salvaguarda (a)). Con uno solo en la
        // misma fila, el Monte Carlo daba un 2,6 % de falsos aceptados con 60 cifras en contexto.
        (0..operandos.len()).any(|i| {
            (0..operandos.len()).any(|j| {
                let (a, b) = (operandos[i], operandos[j]);
                // `X = a − b` con `b = X` no deriva nada
                let degenerado =
                    |o: &Operando| o.lecturas.iter().any(|l| cifra.lecturas.contains(l));
                if i == j || a.lecturas == b.lecturas || degenerado(a) || degenerado(b) {
                    return false;
                }
                let mismo_origen = a.unidad == b.unidad || (a.doc.is_some() && a.doc == b.doc);
                mismo_origen
                    && [(a, b), (b, a)].iter().any(|(x, y)| {
                        if es_porcentaje {
                            y.valor != 0.0 && encaja((x.valor - y.valor) / y.valor * 100.0)
                        } else {
                            // Una suma o una resta es exacta a la precisión de sus operandos:
                            // un resultado redondeado no se acepta aquí (lo ve el Nivel 2)
                            let precision = decimales.max(x.decimales).max(y.decimales);
                            let exacta = |valor: f64| {
                                (valor - objetivo).abs() <= 0.5 * 10f64.powi(-precision) + 1e-9
                            };
                            exacta(x.valor + y.valor) || exacta(x.valor - y.valor)
                        }
                    })
            })
        })
    })
}

/// ¿La cifra `indice` es un número de artículo, sección o marcador que está tal cual en las
/// salidas, con la palabra que la precede o tras `§`, `#` o `-`?
pub(super) fn es_referencia_estructural(
    respuesta: &RespuestaAnalizada,
    indice: usize,
    contexto: &ContextoProcedencia,
) -> bool {
    let cifra = &respuesta.cifras[indice];
    let variantes = variantes_del_literal(&cifra.literal);
    let mut agujas: Vec<String> = Vec::new();
    if let Some(palabra) = palabra_anterior(&respuesta.letras, cifra.posicion) {
        for variante in &variantes {
            agujas.push(format!("{palabra} {variante}"));
            agujas.push(format!("{palabra}{variante}"));
        }
    }
    for marcador in MARCADORES_ESTRUCTURALES {
        for variante in &variantes {
            agujas.push(format!("{marcador} {variante}"));
            agujas.push(format!("{marcador}{variante}"));
        }
    }
    agujas.iter().any(|aguja| {
        contexto
            .textos
            .iter()
            .any(|texto| contiene_con_limites(texto, aguja))
    })
}

fn variantes_del_literal(literal: &str) -> Vec<String> {
    let literal = literal.trim_start_matches('-');
    let cambiado: String = literal
        .chars()
        .map(|l| match l {
            '.' => ',',
            ',' => '.',
            otra => otra,
        })
        .collect();
    if cambiado == literal {
        vec![literal.to_string()]
    } else {
        vec![literal.to_string(), cambiado]
    }
}

/// La palabra pegada a la cifra (separada como mucho por un blanco), con el punto de una
/// abreviatura. En las escrituras sin espacios (Han, kana, hangul) basta su último carácter.
fn palabra_anterior(letras: &[char], posicion: usize) -> Option<String> {
    let mut fin = posicion;
    if fin > 0 && letras[fin - 1].is_whitespace() {
        fin -= 1;
    }
    if fin > 0 && letras[fin - 1] == '.' {
        fin -= 1;
    }
    let mut inicio = fin;
    while inicio > 0 && letras[inicio - 1].is_alphabetic() {
        inicio -= 1;
    }
    if inicio == fin {
        return None;
    }
    if es_escritura_sin_espacios(letras[fin - 1]) {
        inicio = fin - 1;
    }
    let tras_palabra = if posicion > fin && letras[fin] == '.' {
        "."
    } else {
        ""
    };
    let palabra: String = letras[inicio..fin].iter().collect();
    Some(plegar_texto(&format!("{palabra}{tras_palabra}")))
}

fn es_escritura_sin_espacios(letra: char) -> bool {
    matches!(letra as u32,
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF)
}

/// Coincidencia que no corta una palabra delante ni un número detrás
fn contiene_con_limites(texto: &str, aguja: &str) -> bool {
    texto.match_indices(aguja).any(|(inicio, _)| {
        let antes = texto[..inicio].chars().next_back();
        let mut despues = texto[inicio + aguja.len()..].chars();
        let primero = despues.next();
        let segundo = despues.next();
        let corta_palabra = aguja.chars().next().is_some_and(char::is_alphanumeric)
            && antes.is_some_and(char::is_alphanumeric)
            && !es_escritura_sin_espacios(aguja.chars().next().unwrap_or(' '));
        let sigue_numero = primero.is_some_and(|l| l.is_ascii_digit())
            || (matches!(primero, Some('.' | ',')) && segundo.is_some_and(|l| l.is_ascii_digit()));
        !corta_palabra && !sigue_numero
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn salida(herramienta: &str, output: Value) -> ToolResult {
        ToolResult {
            call_id: "c".into(),
            tool_name: herramienta.into(),
            output,
            is_error: false,
        }
    }

    fn busqueda(textos: &[(&str, &str)]) -> ToolResult {
        let resultados: Vec<Value> = textos
            .iter()
            .map(|(doc, texto)| json!({ "doc_id": doc, "texto": texto }))
            .collect();
        salida(
            "search_normative_context",
            json!({ "grupos": [{ "resultados": resultados }] }),
        )
    }

    fn derivadas(respuesta: &str, salidas: &[ToolResult]) -> Vec<String> {
        let contexto = ContextoProcedencia::desde(salidas);
        let analizada = RespuestaAnalizada::nueva(respuesta);
        (0..analizada.cifras.len())
            .filter(|i| es_derivada(&analizada, *i, &contexto, |_| false))
            .map(|i| analizada.cifras[i].literal.clone())
            .collect()
    }

    fn estructurales(respuesta: &str, salidas: &[ToolResult]) -> Vec<String> {
        let contexto = ContextoProcedencia::desde(salidas);
        let analizada = RespuestaAnalizada::nueva(respuesta);
        (0..analizada.cifras.len())
            .filter(|i| es_referencia_estructural(&analizada, *i, &contexto))
            .map(|i| analizada.cifras[i].literal.clone())
            .collect()
    }

    #[test]
    fn test_resta_con_decimales_de_dos_celdas_de_la_misma_fila() {
        let tabla = busqueda(&[(
            "FIN-1",
            "| Concepto | 2025 | 2024 |\n| Revenue | 1,482.6 | 1,317.9 |\n| Costs | 903.4 | 811.0 |",
        )]);
        assert_eq!(
            derivadas(
                "Revenue grew from 1,317.9 to 1,482.6, i.e. by 164.7.",
                &[tabla]
            ),
            vec!["164.7"]
        );
    }

    #[test]
    fn test_porcentaje_de_variacion_con_un_decimal() {
        let tabla = busqueda(&[("FIN-1", "Ingresos: 1.250 (2025) y 1.100 (2024)")]);
        // (1250 - 1100) / 1100 · 100 = 13,636…
        assert_eq!(
            derivadas(
                "Los ingresos pasan de 1.100 a 1.250: un 13,6 % más.",
                &[tabla]
            ),
            vec!["13,6"]
        );
        assert!(
            derivadas(
                "Pasan de 1.100 a 1.250: un 13,9 % más.",
                &[busqueda(&[(
                    "FIN-1",
                    "Ingresos: 1.250 (2025) y 1.100 (2024)"
                )])]
            )
            .is_empty()
        );
    }

    #[test]
    fn test_suma_en_otro_dominio_y_escritura() {
        let dosis = busqueda(&[("MED-3", "Доза утром 250 мг, вечером 125 мг.")]);
        assert_eq!(
            derivadas(
                "Утром 250 мг и вечером 125 мг: всего 375 мг в сутки.",
                &[dosis]
            ),
            vec!["375"]
        );
    }

    #[test]
    fn test_sin_ninguna_cita_cercana_no_se_deriva() {
        let tabla = busqueda(&[("FIN-1", "Revenue 1,482.6 and costs 1,317.9")]);
        assert!(derivadas("The margin is 164.7.", &[tabla]).is_empty());
    }

    #[test]
    fn test_cita_lejana_fuera_de_la_ventana_y_de_la_oracion_no_respalda() {
        let tabla = busqueda(&[("FIN-1", "Revenue 1,482.6 and costs 1,317.9")]);
        let relleno = "Texto de relleno sin cifras. ".repeat(10);
        let respuesta =
            format!("Revenue was 1,482.6 and costs 1,317.9. {relleno}The margin is 164.7.");
        assert!(derivadas(&respuesta, &[tabla]).is_empty());
    }

    #[test]
    fn test_operandos_de_salidas_distintas_sin_documento_comun_no_se_combinan() {
        let a = busqueda(&[("DOC-A", "Caudal 80")]);
        let b = busqueda(&[("DOC-B", "Caudal 50")]);
        assert!(derivadas("Entre 80 y 50 hay 30.", &[a, b]).is_empty());
    }

    #[test]
    fn test_mismo_documento_en_chunks_distintos_exige_citar_los_dos() {
        let mismo = busqueda(&[("DOC-A", "Caudal 80"), ("DOC-A", "Caudal mínimo 50")]);
        assert_eq!(
            derivadas("Entre 80 y 50 hay 30.", std::slice::from_ref(&mismo)),
            vec!["30"]
        );
        assert!(derivadas("Con 80 la diferencia es 30.", &[mismo]).is_empty());
    }

    #[test]
    fn test_fila_sql_exige_citar_los_dos_operandos() {
        let fila = salida(
            "execute_athena_sql",
            json!({ "resultado": [{ "importe_2025": 1250.5, "importe_2024": 1100.25 }] }),
        );
        assert!(
            derivadas(
                "El importe fue 1250,5; sube 150,25.",
                std::slice::from_ref(&fila)
            )
            .is_empty()
        );
        assert_eq!(
            derivadas("De 1100,25 a 1250,5: sube 150,25.", &[fila]),
            vec!["150,25"]
        );
    }

    #[test]
    fn test_el_porcentaje_exige_el_signo_y_la_suma_no_lo_lleva() {
        let tabla = busqueda(&[("FIN-1", "Ingresos: 200 y 160")]);
        // (200 − 160) / 160 · 100 = 25; 200 − 160 = 40
        assert_eq!(
            derivadas("De 160 a 200: un 25 % más.", std::slice::from_ref(&tabla)),
            vec!["25"]
        );
        assert!(derivadas("De 160 a 200: 25 más.", std::slice::from_ref(&tabla)).is_empty());
        assert!(derivadas("De 160 a 200: un 40 % más.", &[tabla]).is_empty());
    }

    #[test]
    fn test_anios_y_codigos_de_seccion_no_son_operandos() {
        let texto = busqueda(&[(
            "LEY-1",
            "Vigente desde 2019 hasta 2024.\n4.2 Alcance\nPlazo 30 días",
        )]);
        // 2024 − 2019 = 5; 4.2 + 30 = 34.2
        assert!(
            derivadas(
                "Desde 2019 a 2024 son 5 años; en 4.2 el plazo de 30 suma 34,2.",
                &[texto]
            )
            .is_empty()
        );
    }

    #[test]
    fn test_abstencion_total_no_da_operandos() {
        let vacias = [
            salida(
                "lookup_double_entry_table",
                json!({ "estado": "sin_coincidencia", "valor_x": 30, "valor_y": 12 }),
            ),
            salida(
                "search_normative_context",
                json!({ "grupos": [], "total": 0, "nota": "top 8 de 20" }),
            ),
        ];
        assert!(ContextoProcedencia::desde(&vacias).sin_operandos());
        assert!(derivadas("No consta. Entre 30 y 12 hay 18.", &vacias).is_empty());
    }

    #[test]
    fn test_cita_vetada_no_respalda_la_derivacion() {
        let tabla = busqueda(&[("FIN-1", "Revenue 1,482.6 and costs 1,317.9")]);
        let contexto = ContextoProcedencia::desde(&[tabla]);
        let analizada = RespuestaAnalizada::nueva("Revenue 1,482.6, costs 1,317.9, margin 164.7.");
        let indice = analizada
            .cifras
            .iter()
            .position(|c| c.literal == "164.7")
            .unwrap();
        assert!(es_derivada(&analizada, indice, &contexto, |_| false));
        assert!(!es_derivada(&analizada, indice, &contexto, |_| true));
    }

    #[test]
    fn test_referencia_estructural_en_varios_idiomas_y_escrituras() {
        let ley = busqueda(&[(
            "LEY-1",
            "Artículo 21. Plazos.\nArticle 14 — Scope\nСтатья 7. Сфера\nΆρθρο 12 Πεδίο\n§ 33 Geltung",
        )]);
        for (respuesta, cifra) in [
            ("Lo regula el artículo 21.", "21"),
            ("See Article 14.", "14"),
            ("Это статья 7.", "7"),
            ("Βλ. άρθρο 12.", "12"),
            ("Siehe § 33.", "33"),
        ] {
            assert_eq!(
                estructurales(respuesta, std::slice::from_ref(&ley)),
                vec![cifra],
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_referencia_estructural_no_acepta_otro_numero_ni_otra_palabra() {
        let ley = busqueda(&[("LEY-1", "Artículo 210. Plazos. El plazo es de 21 días.")]);
        assert!(estructurales("Lo regula el artículo 21.", std::slice::from_ref(&ley)).is_empty());
        assert!(estructurales("Lo regula la sección 210.", &[ley]).is_empty());
    }

    #[test]
    fn test_oraciones_no_cortan_un_decimal() {
        let analizada = RespuestaAnalizada::nueva("Son 1.250 kg. No consta el resto.\nFin");
        let oraciones: Vec<String> = analizada.oraciones().collect();
        assert_eq!(
            oraciones,
            vec!["Son 1.250 kg.", " No consta el resto.", "\n", "Fin"]
        );
    }

    #[test]
    fn test_salida_hostil_con_miles_de_cifras_termina() {
        let enorme: String = (0..50_000).map(|i| format!("{i} ")).collect();
        let contexto = ContextoProcedencia::desde(&[busqueda(&[("D", &enorme)])]);
        let analizada = RespuestaAnalizada::nueva("Con 7 y 9 resulta 123456789.");
        assert!(!es_derivada(&analizada, 2, &contexto, |_| false));
    }
}
