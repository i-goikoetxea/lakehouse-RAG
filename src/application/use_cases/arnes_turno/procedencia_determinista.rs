//! Nivel 1 del verificador de procedencia (Pieza 4): reglas deterministas, sin Claude, para las
//! cifras de la respuesta que no aparecen literalmente en las salidas del turno.
//!
//! - **Clausura aritmética:** la cifra es la suma, la resta o la variación porcentual de dos
//!   cifras, o la suma o resta de 3 o 4, de la misma salida. Los operandos van citados en la
//!   respuesta, seguidos, y la cifra derivada junto a ellos.
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
/// Número de operandos de la clausura más allá de los pares
const OPERANDOS_DE_LA_CLAUSURA: std::ops::RangeInclusive<usize> = 3..=4;
/// Operandos citados (los más cercanos a la cifra) que se combinan de 3 en 3 y de 4 en 4 por
/// origen: C(12, 4) · 15 signos ≈ 7400 sumas
const MAXIMO_OPERANDOS_POR_ORIGEN: usize = 12;
/// Orígenes (chunk, fila o documento) que se examinan por cifra
const MAXIMO_ORIGENES: usize = 8;
/// Un entero de 4 cifras en este intervalo se trata como año: nunca es operando
const ANIOS: std::ops::RangeInclusive<u64> = 1800..=2199;
const MARCADORES_ESTRUCTURALES: &[char] = &['§', '#', '-'];
/// Un apartado numerado (`3.`, `(12)`) tiene como mucho estos dígitos
const DIGITOS_MAXIMOS_DE_APARTADO: usize = 3;
/// Letras mínimas de una abreviatura (`art.`) que se compara con la palabra de un encabezado
const LETRAS_MINIMAS_DE_ABREVIATURA: usize = 3;
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
    /// Encabezados y apartados de línea de cada texto, para las referencias compuestas (`21.3`)
    estructuras: Vec<EstructuraDeLineas>,
    bytes_de_texto: usize,
}

/// Lo que la tipografía de líneas de un texto declara como estructura: encabezados que abren
/// línea con una palabra (o `§`) y un número (`Artículo 21. Plazos`, `## Section 4`), y apartados
/// numerados que abren línea (`3.`, `2)`, `(4)`)
#[derive(Debug, Default)]
struct EstructuraDeLineas {
    /// (palabra plegada o `§`, número)
    encabezados: HashSet<(String, String)>,
    apartados: HashSet<String>,
}

impl EstructuraDeLineas {
    fn de(texto: &str) -> Self {
        let mut estructura = Self::default();
        for linea in texto.lines() {
            let linea = linea.trim_start_matches(|l: char| {
                l.is_whitespace() || matches!(l, '#' | '*' | '>' | '-' | '_')
            });
            if let Some(apartado) = apartado_que_abre(linea) {
                estructura.apartados.insert(apartado);
            } else if let Some(encabezado) = encabezado_que_abre(linea) {
                estructura.encabezados.insert(encabezado);
            }
        }
        estructura
    }
}

/// `3.`, `2)` o `(4)` seguidos de un blanco o del final de la línea
fn apartado_que_abre(linea: &str) -> Option<String> {
    let sin_parentesis = linea.strip_prefix('(');
    let resto = sin_parentesis.unwrap_or(linea);
    let digitos: String = resto.chars().take_while(char::is_ascii_digit).collect();
    if digitos.is_empty() || digitos.len() > DIGITOS_MAXIMOS_DE_APARTADO {
        return None;
    }
    let mut tras = resto[digitos.len()..].chars();
    let cierre = tras.next()?;
    let cierra = if sin_parentesis.is_some() {
        cierre == ')'
    } else {
        matches!(cierre, '.' | ')')
    };
    let separado = tras.next().is_none_or(char::is_whitespace);
    (cierra && separado).then(|| digitos.trim_start_matches('0').to_string())
}

/// Una palabra (con su punto de abreviatura) o `§`, y un número entero que no sigue con dígitos
fn encabezado_que_abre(linea: &str) -> Option<(String, String)> {
    let palabra: String = if linea.starts_with('§') {
        "§".to_string()
    } else {
        linea.chars().take_while(|l| l.is_alphabetic()).collect()
    };
    if palabra.is_empty() {
        return None;
    }
    let resto = linea[palabra.len()..].trim_start_matches('.').trim_start();
    let numero: String = resto.chars().take_while(char::is_ascii_digit).collect();
    let tras = resto[numero.len()..].chars().next();
    // `21` sí; `21b`, `21.5` o `21,5` no son el número entero de un encabezado
    let sigue_decimal = |l: char| {
        matches!(l, '.' | ',')
            && resto[numero.len() + 1..].starts_with(|d: char| d.is_ascii_digit())
    };
    let entero = tras.is_none_or(|l| !(l.is_alphanumeric() || sigue_decimal(l)));
    (!numero.is_empty() && entero).then(|| (plegar_texto(&palabra), numero))
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
                    let prefijo = prefijo_de_bytes(texto, restante);
                    contexto.estructuras.push(EstructuraDeLineas::de(&prefijo));
                    let texto = espacios_simples(&plegar_texto(&prefijo));
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
    /// Inicio y fin de cada cláusula: las oraciones, cortadas además por tipografía
    clausulas: Vec<(usize, usize)>,
    /// Posiciones de las cifras que no son años ni fechas, en orden: la secuencia en que se mide
    /// si unos operandos van citados seguidos
    secuencia: Vec<usize>,
}

impl RespuestaAnalizada {
    pub(super) fn nueva(respuesta: &str) -> Self {
        let letras: Vec<char> = respuesta.chars().collect();
        let cifras = extraer_cifras(respuesta);
        let secuencia = cifras
            .iter()
            .filter(|cifra| !es_anio_o_fecha(cifra))
            .map(|cifra| cifra.posicion)
            .collect();
        let oraciones = oraciones(&letras);
        Self {
            cifras,
            clausulas: clausulas(&letras, &oraciones),
            oraciones,
            letras,
            secuencia,
        }
    }

    pub(super) fn oraciones(&self) -> impl Iterator<Item = String> + '_ {
        self.oraciones
            .iter()
            .map(|(inicio, fin)| self.letras[*inicio..*fin].iter().collect())
    }

    pub(super) fn clausulas(&self) -> impl Iterator<Item = String> + '_ {
        self.clausulas
            .iter()
            .map(|(inicio, fin)| self.letras[*inicio..*fin].iter().collect())
    }

    pub(super) fn clausula_de(&self, posicion: usize) -> usize {
        indice_de_tramo(&self.clausulas, posicion)
    }

    pub(super) fn oracion_de(&self, posicion: usize) -> usize {
        indice_de_tramo(&self.oraciones, posicion)
    }

    /// Tipografía: `12,5 %` o `12,5%` es un porcentaje
    fn va_seguida_de_porcentaje(&self, cifra: &CifraHallada) -> bool {
        let fin = cifra.posicion + cifra.literal.trim_start_matches('-').chars().count();
        self.letras[fin.min(self.letras.len())..]
            .iter()
            .find(|l| !matches!(l, ' ' | '\u{00A0}' | '\u{202F}'))
            .is_some_and(|l| matches!(l, '%' | '٪'))
    }

    /// Los operandos van seguidos en la respuesta (sin otras cifras entre ellos, salvo años o
    /// fechas) y la cifra derivada va justo antes o justo después del grupo
    fn citados_junto_a(&self, derivada: usize, operandos: &[usize]) -> bool {
        let rango = |posicion: usize| self.secuencia.binary_search(&posicion).ok();
        let (Some(objetivo), Some(rangos)) = (
            rango(derivada),
            operandos
                .iter()
                .map(|p| rango(*p))
                .collect::<Option<Vec<usize>>>(),
        ) else {
            return false;
        };
        let (Some(primero), Some(ultimo)) = (rangos.iter().min(), rangos.iter().max()) else {
            return false;
        };
        ultimo - primero + 1 == rangos.len() && (objetivo + 1 == *primero || objetivo == ultimo + 1)
    }

    fn cerca(&self, una: usize, otra: usize) -> bool {
        una.abs_diff(otra) <= VENTANA_DE_CITA || self.oracion_de(una) == self.oracion_de(otra)
    }
}

/// Tramos ordenados y contiguos: búsqueda binaria, no un recorrido por cifra
fn indice_de_tramo(tramos: &[(usize, usize)], posicion: usize) -> usize {
    tramos
        .partition_point(|(_, fin)| *fin <= posicion)
        .min(tramos.len().saturating_sub(1))
}

/// Las oraciones, cortadas también tras `;`, antes de un paréntesis o corchete que abre, tras el
/// que cierra y en una raya entre espacios (` — `, ` – `). La coma no corta: une la locución con
/// lo que la completa («no consta el margen, solo los costes de 903,4»).
fn clausulas(letras: &[char], oraciones: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut clausulas = Vec::new();
    for &(inicio_oracion, fin_oracion) in oraciones {
        let mut inicio = inicio_oracion;
        for posicion in inicio_oracion..fin_oracion {
            let letra = letras[posicion];
            let entre_espacios = |p: usize| {
                p > 0
                    && letras[p - 1].is_whitespace()
                    && letras.get(p + 1).is_some_and(|l| l.is_whitespace())
            };
            let corte = match letra {
                '(' | '[' | '（' => Some(posicion),
                ';' | '；' | ')' | ']' | '）' => Some(posicion + 1),
                '—' | '–' if entre_espacios(posicion) => Some(posicion),
                _ => None,
            };
            if let Some(corte) = corte.filter(|c| *c > inicio) {
                clausulas.push((inicio, corte));
                inicio = corte;
            }
        }
        if fin_oracion > inicio {
            clausulas.push((inicio, fin_oracion));
        }
    }
    if clausulas.is_empty() {
        clausulas.push((0, letras.len()));
    }
    clausulas
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

/// ¿La cifra `indice` de la respuesta es suma, resta o variación porcentual de dos operandos, o
/// suma o resta de 3 o 4?
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
    es_derivada_de_un_par(respuesta, cifra, &operandos, &citadas)
        || es_suma_o_resta_de_varios(respuesta, cifra, &citadas, &operandos)
}

fn es_derivada_de_un_par(
    respuesta: &RespuestaAnalizada,
    cifra: &CifraHallada,
    operandos: &[&Operando],
    citadas: &[&CifraHallada],
) -> bool {
    // Como en la clausura de 3 y 4: los dos operandos, citados seguidos y junto a la cifra
    let citados_junto = |a: &Operando, b: &Operando| {
        posiciones_de_cita(a, citadas).any(|pa| {
            posiciones_de_cita(b, citadas)
                .any(|pb| pa != pb && respuesta.citados_junto_a(cifra.posicion, &[pa, pb]))
        })
    };
    cifra.lecturas.iter().any(|lectura| {
        let Ok(objetivo) = lectura.parse::<f64>() else {
            return false;
        };
        let decimales = decimales_escritos(cifra, lectura);
        let tolerancia = 0.5 * 10f64.powi(-decimales) + 1e-9;
        let encaja = |valor: f64| (valor - objetivo).abs() <= tolerancia;
        let es_porcentaje = respuesta.va_seguida_de_porcentaje(cifra);
        // Los dos operandos van citados cerca de la cifra (salvaguarda (a)). Con uno solo en la
        // misma fila, el Monte Carlo daba un 2,6 % de falsos aceptados con 60 cifras en contexto;
        // sin exigirlos seguidos y junto a la cifra, un 0,35 % con 4 cifras citadas.
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
                    && citados_junto(a, b)
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

/// Posiciones, en la respuesta, de las cifras citadas que leen igual que el operando
fn posiciones_de_cita<'a>(
    operando: &'a Operando,
    citadas: &'a [&CifraHallada],
) -> impl Iterator<Item = usize> + 'a {
    citadas
        .iter()
        .filter(|c| c.lecturas.iter().any(|l| operando.lecturas.contains(l)))
        .map(|c| c.posicion)
}

/// Un operando de la clausura de 3 y 4, con la cifra citada de la respuesta que lo respalda
type OperandoCitado<'a> = (usize, &'a Operando);

/// Suma o resta de 3 o 4 operandos. Cada operando es una cifra **distinta** citada en la respuesta
/// y todos salen del mismo chunk, tabla o fila (o del mismo documento): nunca se combinan cifras
/// del contexto que la respuesta no cita. Además, los operandos van citados seguidos y la cifra
/// derivada junto al grupo, como se escribe un total (`245 (120, 85 y 40)`): sin esto, cada
/// cifra citada de más multiplica las combinaciones y las coincidencias por azar (Monte Carlo).
/// Sin porcentajes: la variación es de dos valores.
fn es_suma_o_resta_de_varios(
    respuesta: &RespuestaAnalizada,
    cifra: &CifraHallada,
    citadas: &[&CifraHallada],
    operandos: &[&Operando],
) -> bool {
    if respuesta.va_seguida_de_porcentaje(cifra) {
        return false;
    }
    let respaldados: Vec<OperandoCitado> = operandos
        .iter()
        .filter(|operando| !operando.lecturas.iter().any(|l| cifra.lecturas.contains(l)))
        .filter_map(|operando| {
            let cita = citadas
                .iter()
                .position(|c| c.lecturas.iter().any(|l| operando.lecturas.contains(l)))?;
            Some((cita, *operando))
        })
        .collect();
    let distancia = |(cita, _): &OperandoCitado| citadas[*cita].posicion.abs_diff(cifra.posicion);
    origenes_comunes(&respaldados)
        .into_iter()
        .take(MAXIMO_ORIGENES)
        .any(|mut origen| {
            origen.sort_by_key(distancia);
            origen.truncate(MAXIMO_OPERANDOS_POR_ORIGEN);
            OPERANDOS_DE_LA_CLAUSURA.into_iter().any(|cuantos| {
                hay_combinacion(&origen, cuantos, 0, &mut Vec::new(), &|elegidos| {
                    let posiciones: Vec<usize> = elegidos
                        .iter()
                        .map(|(cita, _)| citadas[*cita].posicion)
                        .collect();
                    respuesta.citados_junto_a(cifra.posicion, &posiciones)
                        && suma_con_signos_encaja(cifra, elegidos)
                })
            })
        })
}

/// Grupos de operandos de un mismo objeto (chunk, tabla o fila) o de un mismo documento, con al
/// menos tantas citas distintas como el mínimo de la clausura
fn origenes_comunes<'a>(respaldados: &[OperandoCitado<'a>]) -> Vec<Vec<OperandoCitado<'a>>> {
    let mut por_unidad: Vec<(usize, Vec<OperandoCitado>)> = Vec::new();
    let mut por_documento: Vec<(&str, Vec<OperandoCitado>)> = Vec::new();
    for respaldado in respaldados {
        let (_, operando) = respaldado;
        match por_unidad.iter_mut().find(|(u, _)| *u == operando.unidad) {
            Some((_, grupo)) => grupo.push(*respaldado),
            None => por_unidad.push((operando.unidad, vec![*respaldado])),
        }
        if let Some(doc) = operando.doc.as_deref() {
            match por_documento.iter_mut().find(|(d, _)| *d == doc) {
                Some((_, grupo)) => grupo.push(*respaldado),
                None => por_documento.push((doc, vec![*respaldado])),
            }
        }
    }
    let minimo = *OPERANDOS_DE_LA_CLAUSURA.start();
    por_unidad
        .into_iter()
        .map(|(_, grupo)| grupo)
        .chain(por_documento.into_iter().map(|(_, grupo)| grupo))
        .filter(|grupo| {
            let citas: HashSet<usize> = grupo.iter().map(|(cita, _)| *cita).collect();
            citas.len() >= minimo
        })
        .collect()
}

/// ¿Alguna combinación de `cuantos` operandos con citas y valores distintos cumple `encaja`?
fn hay_combinacion<'a>(
    candidatos: &[OperandoCitado<'a>],
    cuantos: usize,
    desde: usize,
    elegidos: &mut Vec<OperandoCitado<'a>>,
    encaja: &impl Fn(&[OperandoCitado<'a>]) -> bool,
) -> bool {
    if elegidos.len() == cuantos {
        return encaja(elegidos);
    }
    for siguiente in desde..candidatos.len() {
        let (cita, operando) = candidatos[siguiente];
        let repetido = elegidos
            .iter()
            .any(|(otra, otro)| *otra == cita || otro.lecturas == operando.lecturas);
        if repetido {
            continue;
        }
        elegidos.push(candidatos[siguiente]);
        let hallada = hay_combinacion(candidatos, cuantos, siguiente + 1, elegidos, encaja);
        elegidos.pop();
        if hallada {
            return true;
        }
    }
    false
}

/// `±a ± b ± c (± d)` con al menos un sumando positivo, exacto a la precisión de los operandos
fn suma_con_signos_encaja(cifra: &CifraHallada, elegidos: &[OperandoCitado]) -> bool {
    cifra.lecturas.iter().any(|lectura| {
        let Ok(objetivo) = lectura.parse::<f64>() else {
            return false;
        };
        let precision = elegidos
            .iter()
            .map(|(_, o)| o.decimales)
            .fold(decimales_escritos(cifra, lectura), i32::max);
        let tolerancia = 0.5 * 10f64.powi(-precision) + 1e-9;
        (1u32..(1 << elegidos.len())).any(|positivos| {
            let total: f64 = elegidos
                .iter()
                .enumerate()
                .map(|(i, (_, o))| {
                    if positivos & (1 << i) != 0 {
                        o.valor
                    } else {
                        -o.valor
                    }
                })
                .sum();
            (total - objetivo).abs() <= tolerancia
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
    }) || es_referencia_compuesta(respuesta, cifra, contexto)
}

/// `artículo 21.3`, `Section 4.2`, `§ 33.4`: el texto que abre una línea con el encabezado
/// (`Artículo 21`, o su abreviatura `art.`) abre otra con el apartado (`3.`, `(3)`)
fn es_referencia_compuesta(
    respuesta: &RespuestaAnalizada,
    cifra: &CifraHallada,
    contexto: &ContextoProcedencia,
) -> bool {
    let Some((numero, apartado)) = cifra.literal.split_once('.') else {
        return false;
    };
    let solo_digitos = |parte: &str| !parte.is_empty() && parte.chars().all(|l| l.is_ascii_digit());
    if !solo_digitos(numero)
        || !solo_digitos(apartado)
        || apartado.len() > DIGITOS_MAXIMOS_DE_APARTADO
    {
        return false;
    }
    let apartado = apartado.trim_start_matches('0');
    let tras_parrafo = respuesta.letras[..cifra.posicion]
        .iter()
        .rev()
        .find(|l| !l.is_whitespace())
        == Some(&'§');
    let palabra = if tras_parrafo {
        Some("§".to_string())
    } else {
        palabra_anterior(&respuesta.letras, cifra.posicion)
    };
    let Some(palabra) = palabra else {
        return false;
    };
    let nombra = |encabezado: &str| match palabra.strip_suffix('.') {
        Some(abreviatura) => {
            abreviatura.chars().count() >= LETRAS_MINIMAS_DE_ABREVIATURA
                && encabezado.starts_with(abreviatura)
        }
        None => encabezado == palabra,
    };
    contexto.estructuras.iter().any(|estructura| {
        estructura.apartados.contains(apartado)
            && estructura
                .encabezados
                .iter()
                .any(|(encabezado, n)| n == numero && nombra(encabezado))
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
    fn test_suma_de_tres_operandos_citados_del_mismo_chunk() {
        let pesos = busqueda(&[("ALM-2", "Pieza A 120 kg, pieza B 85 kg, pieza C 40 kg.")]);
        assert_eq!(
            derivadas("Las tres piezas suman 245 kg (120, 85 y 40).", &[pesos]),
            vec!["245"]
        );
    }

    #[test]
    fn test_resta_de_cuatro_operandos_con_decimales_en_ingles() {
        let cuenta = busqueda(&[(
            "FIN-9",
            "| Revenue | 1,482.6 |\n| COGS | 903.4 |\n| Opex | 211.0 |\n| Tax | 58.2 |",
        )]);
        // 1482,6 − 903,4 − 211,0 − 58,2 = 310,0
        assert_eq!(
            derivadas(
                "Net income is 310.0 (1,482.6 less 903.4, 211.0 and 58.2).",
                &[cuenta]
            ),
            vec!["310.0"]
        );
    }

    #[test]
    fn test_signos_mezclados_de_tres_operandos_en_aleman() {
        let lager = busqueda(&[("LAG-1", "Anfangsbestand 500 Stück, Zugang 120, Abgang 80.")]);
        // 500 + 120 − 80 = 540
        assert_eq!(
            derivadas("Endbestand: 540 (500 + 120 − 80).", &[lager]),
            vec!["540"]
        );
    }

    #[test]
    fn test_un_operando_sin_citar_impide_la_suma_de_tres() {
        let pesos = busqueda(&[("ALM-2", "Pieza A 120 kg, pieza B 85 kg, pieza C 40 kg.")]);
        // 120 + 85 + 40 = 245, pero 40 no está en la respuesta: nada de sumas sobre el chunk
        assert!(derivadas("Las piezas A y B (120 y 85) y la C suman 245 kg.", &[pesos]).is_empty());
    }

    #[test]
    fn test_la_suma_de_tres_exige_los_operandos_seguidos_y_junto_al_total() {
        let pesos = busqueda(&[(
            "ALM-2",
            "Pieza A 120 kg, pieza B 85 kg, pieza C 40 kg, palé 9 kg.",
        )]);
        // Seguidos y junto al total, aunque medie un año
        assert_eq!(
            derivadas(
                "En 2025, 120, 85 y 40 kg suman 245 kg.",
                std::slice::from_ref(&pesos)
            ),
            vec!["245"]
        );
        // El palé (9) se interpone entre los operandos y el total
        assert!(
            derivadas(
                "A 120 kg, B 85 kg, C 40 kg y palé 9 kg; las piezas suman 245 kg.",
                std::slice::from_ref(&pesos)
            )
            .is_empty()
        );
        // Operandos separados por otra cifra
        assert!(derivadas("Con 120, 9, 85 y 40 kg: 245 kg.", &[pesos]).is_empty());
    }

    #[test]
    fn test_el_par_exige_los_operandos_seguidos_y_junto_a_la_cifra() {
        let tabla = busqueda(&[("IND-7", "Índice A: 85. Índice B: 120. Índice C: 40.")]);
        // 120 − 85 = 35, con el 40 citado entre los operandos y la cifra: no se rescata aquí
        // (queda pendiente para el Nivel 2)
        assert!(
            derivadas(
                "A vale 85 y B 120; C, 40. De A a B hay 35.",
                std::slice::from_ref(&tabla)
            )
            .is_empty()
        );
        assert_eq!(
            derivadas("C vale 40. De 85 a 120 hay 35.", &[tabla]),
            vec!["35"]
        );
    }

    #[test]
    fn test_operandos_de_documentos_distintos_no_suman_tres() {
        let a = busqueda(&[("DOC-A", "Caudal 80")]);
        let b = busqueda(&[("DOC-B", "Caudal 50 y 20")]);
        assert!(derivadas("Entre 80, 50 y 20 hay 150.", &[a, b]).is_empty());
    }

    #[test]
    fn test_cinco_operandos_quedan_fuera_de_la_clausura() {
        let lista = busqueda(&[("INV-1", "Lotes: 11, 22, 33, 44 y 55 unidades.")]);
        // 11 + 22 + 33 + 44 + 55 = 165: más de 4 operandos no se combinan
        // (las citadas sí son pares entre sí: 22 = 33 − 11; solo cuenta el 165)
        let derivadas = derivadas("Los lotes 11, 22, 33, 44 y 55 suman 165.", &[lista]);
        assert!(!derivadas.contains(&"165".to_string()), "{derivadas:?}");
    }

    #[test]
    fn test_un_operando_citado_dos_veces_no_cuenta_como_dos() {
        let lista = busqueda(&[("INV-1", "Lotes: 30 y 12 unidades; reserva 30.")]);
        // 30 + 30 + 12 = 72 exige citar dos veces el 30 como operandos distintos
        assert!(derivadas("Con 30 y 12 unidades el total es 72.", &[lista]).is_empty());
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
    fn test_referencia_compuesta_a_encabezado_y_apartado_en_varias_escrituras() {
        let ley = busqueda(&[
            ("LEY-1", "Artículo 21. Plazos\n1. General.\n3. Urgente."),
            ("LEY-1", "## Section 4 Records\n(2) Keep records."),
            ("LEY-1", "Статья 7. Сфера\n2) Для всех."),
            ("LEY-1", "§ 33 Geltung\n(4) Frist."),
        ]);
        for (respuesta, cifra) in [
            ("Lo regula el artículo 21.3.", "21.3"),
            ("Lo regula el art. 21.3.", "21.3"),
            ("See Section 4.2.", "4.2"),
            ("Это статья 7.2.", "7.2"),
            ("Siehe § 33.4.", "33.4"),
        ] {
            assert_eq!(
                estructurales(respuesta, std::slice::from_ref(&ley)),
                vec![cifra],
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_referencia_compuesta_exige_encabezado_apartado_y_mismo_texto() {
        let ley = busqueda(&[
            ("LEY-1", "Artículo 21. Plazos\n1. General.\n3. Urgente."),
            ("LEY-1", "El plazo es de 2 días.\n5. Otras normas."),
            ("LEY-1", "Artículo 30. Recursos"),
            ("LEY-1", "7. Apartado suelto"),
        ]);
        for respuesta in [
            "Lo regula el artículo 21.5.",
            "Lo regula la sección 21.3.",
            "Suele ser de 2.5 bar.",
            // El apartado 7 está en otro texto que el artículo 30
            "Lo regula el artículo 30.7.",
            "Lo regula el ar. 21.3.",
        ] {
            assert!(
                estructurales(respuesta, std::slice::from_ref(&ley)).is_empty(),
                "{respuesta}"
            );
        }
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
