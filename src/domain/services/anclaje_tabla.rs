//! Página exacta de una tabla destilada por ventana de tabla (`ssd_generalidad_4_piezas.md` §2).
//!
//! Sin reglas de idioma: solo tokens alfanuméricos de Unicode, cifras con la coma y el punto
//! decimal equivalentes y un token por carácter en las escrituras sin espacios (CJK y kana). Entre
//! una página dudosa y `None`, siempre `None`.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::domain::services::normalizacion::plegar_mayusculas;

/// Páginas consecutivas máximas de un bloque de tabla
pub const MAX_PAGINAS_BLOQUE: u32 = 4;
/// Líneas siguientes a la clave de fila en las que se buscan también sus valores
const RADIO_LINEAS_VALORES: usize = 2;
/// Valores distintos de la fila que deben seguir a su clave (o todos, si tiene menos)
const MIN_VALORES_CERCA: usize = 2;
/// Filas mínimas de la subsecuencia creciente
const MIN_FILAS_EN_ORDEN: usize = 2;
/// Líneas anteriores a la primera fila del bloque en las que se busca una cabecera multinivel
const LINEAS_CABECERA_COMPUESTA: usize = 4;
/// Cabeceras distintas que deben estar en esas líneas para que cuenten las compuestas: las
/// palabras de una sola columna pueden ser prosa
const MIN_CABECERAS_COMPUESTAS: usize = 2;

/// Línea del texto paginado con su página física
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineaPaginada<'a> {
    pub pagina: u32,
    pub texto: &'a str,
}

/// Tabla destilada reducida a lo que se busca en el texto
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TablaParaAnclar {
    /// Cabeceras de columna: categorías del eje x o extremos de sus rangos
    pub cabeceras: Vec<String>,
    /// Filas en el orden de la tabla destilada
    pub filas: Vec<FilaParaAnclar>,
}

/// Fila de la tabla: su clave y los valores de sus celdas
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FilaParaAnclar {
    /// Piezas de la clave, que deben aparecer todas en la misma línea: la categoría del eje y o
    /// los extremos de su rango
    pub clave: Vec<String>,
    pub valores: Vec<String>,
}

/// Bloque físico de la tabla y página de cada fila (mismo orden que `TablaParaAnclar::filas`)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnclajeTabla {
    pub bloque: Option<(u32, u32)>,
    pub por_fila: Vec<Option<u32>>,
}

/// Ancla la tabla a su bloque de páginas y a la página de cada fila
pub fn anclar_tabla(lineas: &[LineaPaginada], tabla: &TablaParaAnclar) -> AnclajeTabla {
    let sin_anclar = AnclajeTabla {
        bloque: None,
        por_fila: vec![None; tabla.filas.len()],
    };
    let texto = TextoIndexado::new(lineas);
    let filas: Vec<FilaIndexada> = tabla
        .filas
        .iter()
        .map(|fila| FilaIndexada::new(&texto, fila))
        .collect();
    let cabeceras: Vec<CabeceraIndexada> = tabla
        .cabeceras
        .iter()
        .map(|cabecera| CabeceraIndexada::new(&texto, cabecera))
        .collect();
    let Some(bloque) = elegir_bloque(&texto, &filas, &cabeceras) else {
        return sin_anclar;
    };
    let por_fila = if bloque.0 == bloque.1 {
        filas
            .iter()
            .map(|fila| fila.primera_en.contains_key(&bloque.0).then_some(bloque.0))
            .collect()
    } else if claves_en_lineas_distintas(&filas, bloque) {
        paginas_por_fila(&texto, &filas, bloque)
    } else {
        sin_anclar.por_fila
    };
    AnclajeTabla {
        bloque: Some(bloque),
        por_fila,
    }
}

/// Tokens normalizados: mayúsculas plegadas, rachas alfanuméricas de Unicode separadas entre letras
/// y cifras, cifras decimales con punto (`0,45` y `0.45` → `0.45`) y un token por carácter en las
/// escrituras sin espacios (CJK y kana)
pub fn tokens(texto: &str) -> Vec<String> {
    trocear(&plegar_mayusculas(texto))
}

/// Rachas alfanuméricas del texto tal cual, sin plegar mayúsculas
fn trocear(texto: &str) -> Vec<String> {
    let caracteres: Vec<char> = texto.chars().collect();
    let mut resultado = Vec::new();
    let mut actual = String::new();
    for (indice, &caracter) in caracteres.iter().enumerate() {
        if es_escritura_sin_espacios(caracter) {
            cerrar_token(&mut resultado, &mut actual);
            resultado.push(caracter.to_string());
        } else if caracter.is_alphanumeric() {
            let cambia_de_clase = actual.chars().last().is_some_and(|ultimo| {
                ultimo != '.' && ultimo.is_numeric() != caracter.is_numeric()
            });
            if cambia_de_clase {
                cerrar_token(&mut resultado, &mut actual);
            }
            actual.push(caracter);
        } else if es_separador_decimal(&actual, caracter, caracteres.get(indice + 1)) {
            actual.push('.');
        } else {
            cerrar_token(&mut resultado, &mut actual);
        }
    }
    cerrar_token(&mut resultado, &mut actual);
    resultado
}

/// `patron` aparece en `texto` como secuencia completa y contigua de tokens
pub fn contiene_secuencia(texto: &str, patron: &str) -> bool {
    let aguja = tokens(patron);
    posicion_de(&tokens(texto), &aguja).is_some()
}

/// Variante de búsqueda de un valor con potencias (`10^19` → `1019`): el texto extraído aplana el
/// superíndice. Solo se aplica a la celda, nunca al texto. `None` si no hay `\d+\^\d+`
pub fn variante_sin_circunflejo(valor: &str) -> Option<String> {
    let caracteres: Vec<char> = valor.chars().collect();
    let es_potencia = |indice: usize| {
        caracteres[indice] == '^'
            && indice > 0
            && caracteres[indice - 1].is_ascii_digit()
            && caracteres.get(indice + 1).is_some_and(char::is_ascii_digit)
    };
    let variante: String = (0..caracteres.len())
        .filter(|&indice| !es_potencia(indice))
        .map(|indice| caracteres[indice])
        .collect();
    (variante.len() < valor.len()).then_some(variante)
}

/// Tokens que una cabecera multinivel debe tener en las líneas previas a la tabla: se ignoran los
/// conectores de una letra minúscula; cuentan las palabras, las cifras y los símbolos o unidades
/// de un carácter (mayúsculas, CJK)
fn tokens_no_triviales(cabecera: &str) -> BTreeSet<String> {
    trocear(cabecera)
        .into_iter()
        .filter(|token| {
            let mut caracteres = token.chars();
            !matches!((caracteres.next(), caracteres.next()), (Some(c), None) if c.is_lowercase())
        })
        .flat_map(|token| tokens(&token))
        .collect()
}

fn cerrar_token(resultado: &mut Vec<String>, actual: &mut String) {
    if !actual.is_empty() {
        resultado.push(std::mem::take(actual));
    }
}

fn es_separador_decimal(actual: &str, caracter: char, siguiente: Option<&char>) -> bool {
    matches!(caracter, '.' | ',')
        && actual.chars().last().is_some_and(|c| c.is_ascii_digit())
        && siguiente.is_some_and(char::is_ascii_digit)
}

/// Bloques de Unicode de escrituras que no separan palabras con espacios (Han y kana)
fn es_escritura_sin_espacios(caracter: char) -> bool {
    matches!(
        u32::from(caracter),
        0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3134F
    )
}

/// Posición de la primera aparición contigua de `aguja` en `pajar`
fn posicion_de(pajar: &[String], aguja: &[String]) -> Option<usize> {
    if aguja.is_empty() || aguja.len() > pajar.len() {
        return None;
    }
    pajar
        .windows(aguja.len())
        .position(|ventana| ventana == aguja)
}

/// Posición de una aparición: línea y token
type Posicion = (usize, usize);

/// Texto paginado tokenizado, con un índice de token a líneas
struct TextoIndexado {
    paginas: Vec<u32>,
    lineas: Vec<Vec<String>>,
    indice: HashMap<String, Vec<usize>>,
}

impl TextoIndexado {
    fn new(lineas: &[LineaPaginada]) -> Self {
        let tokenizadas: Vec<Vec<String>> = lineas.iter().map(|l| tokens(l.texto)).collect();
        let mut indice: HashMap<String, Vec<usize>> = HashMap::new();
        for (numero, linea) in tokenizadas.iter().enumerate() {
            for token in linea {
                let lineas_del_token = indice.entry(token.clone()).or_default();
                if lineas_del_token.last() != Some(&numero) {
                    lineas_del_token.push(numero);
                }
            }
        }
        Self {
            paginas: lineas.iter().map(|l| l.pagina).collect(),
            lineas: tokenizadas,
            indice,
        }
    }

    /// Líneas (en orden) en las que aparecen todas las piezas, con la posición de la primera
    fn apariciones(&self, piezas: &[Vec<String>]) -> Vec<Posicion> {
        let Some(primera) = piezas
            .first()
            .filter(|_| piezas.iter().all(|p| !p.is_empty()))
        else {
            return Vec::new();
        };
        let Some(candidatas) = self.indice.get(&primera[0]) else {
            return Vec::new();
        };
        candidatas
            .iter()
            .filter_map(|&linea| {
                let tokens_linea = &self.lineas[linea];
                let posicion = posicion_de(tokens_linea, primera)?;
                piezas[1..]
                    .iter()
                    .all(|pieza| posicion_de(tokens_linea, pieza).is_some())
                    .then_some((linea, posicion))
            })
            .collect()
    }

    fn paginas_de(&self, piezas: &[Vec<String>]) -> BTreeSet<u32> {
        self.apariciones(piezas)
            .into_iter()
            .map(|(linea, _)| self.paginas[linea])
            .collect()
    }

    /// `valor` aparece detrás de la clave: en su línea a partir del token `desde`, o en las
    /// `RADIO_LINEAS_VALORES` líneas siguientes de la misma página. Lo que precede a la clave es
    /// de otra fila
    fn contiene_tras(&self, linea: usize, desde: usize, valor: &[String]) -> bool {
        let pagina = self.paginas[linea];
        let resto = self.lineas[linea].get(desde..).unwrap_or_default();
        let hasta = (linea + RADIO_LINEAS_VALORES).min(self.lineas.len() - 1);
        posicion_de(resto, valor).is_some()
            || (linea + 1..=hasta).any(|siguiente| {
                self.paginas[siguiente] == pagina
                    && posicion_de(&self.lineas[siguiente], valor).is_some()
            })
    }
}

/// Fila con sus apariciones en el texto: solo las que llevan detrás sus valores
struct FilaIndexada {
    apariciones: Vec<Posicion>,
    /// Primera aparición de la clave en cada página
    primera_en: HashMap<u32, Posicion>,
}

impl FilaIndexada {
    fn new(texto: &TextoIndexado, fila: &FilaParaAnclar) -> Self {
        let piezas: Vec<Vec<String>> = fila
            .clave
            .iter()
            .map(|pieza| tokens(pieza))
            .filter(|pieza| !pieza.is_empty())
            .collect();
        let mut valores: Vec<Vec<Vec<String>>> = Vec::new();
        for valor in &fila.valores {
            let principal = tokens(valor);
            let repetido = valores.iter().any(|formas| formas[0] == principal);
            if principal.is_empty() || piezas.contains(&principal) || repetido {
                continue;
            }
            let mut formas = vec![principal];
            if let Some(variante) = variante_sin_circunflejo(valor).map(|v| tokens(&v))
                && !variante.is_empty()
                && !formas.contains(&variante)
            {
                formas.push(variante);
            }
            valores.push(formas);
        }
        let exigidos = MIN_VALORES_CERCA.min(valores.len());
        let fin_de_clave = piezas.first().map_or(0, Vec::len);
        let apariciones: Vec<Posicion> = texto
            .apariciones(&piezas)
            .into_iter()
            .filter(|&(linea, posicion)| {
                valores
                    .iter()
                    .filter(|formas| {
                        formas
                            .iter()
                            .any(|forma| texto.contiene_tras(linea, posicion + fin_de_clave, forma))
                    })
                    .count()
                    >= exigidos
            })
            .collect();
        let apariciones = preferir_las_que_abren_linea(apariciones);
        let mut primera_en = HashMap::new();
        for &(linea, posicion) in &apariciones {
            primera_en
                .entry(texto.paginas[linea])
                .or_insert((linea, posicion));
        }
        Self {
            apariciones,
            primera_en,
        }
    }

    /// Primera aparición dentro de las páginas `[inicio, fin]`
    fn primera_entre(&self, (inicio, fin): (u32, u32)) -> Option<Posicion> {
        (inicio..=fin)
            .filter_map(|pagina| self.primera_en.get(&pagina).copied())
            .min()
    }
}

/// Cabecera de columna: páginas en las que aparece contigua y tokens con los que puede aparecer
/// compuesta por niveles en varias líneas
struct CabeceraIndexada {
    contigua_en: BTreeSet<u32>,
    no_triviales: BTreeSet<String>,
}

impl CabeceraIndexada {
    fn new(texto: &TextoIndexado, cabecera: &str) -> Self {
        Self {
            contigua_en: texto.paginas_de(&[tokens(cabecera)]),
            no_triviales: tokens_no_triviales(cabecera),
        }
    }
}

/// Cabeceras presentes en la ventana: contiguas en alguna de sus páginas o compuestas justo antes
/// de su primera fila
fn cabeceras_presentes(
    texto: &TextoIndexado,
    filas: &[FilaIndexada],
    cabeceras: &[CabeceraIndexada],
    (inicio, fin): (u32, u32),
) -> usize {
    let compuestas = cabeceras_compuestas(texto, filas, cabeceras, (inicio, fin));
    cabeceras
        .iter()
        .enumerate()
        .filter(|(indice, cabecera)| {
            cabecera.contigua_en.range(inicio..=fin).next().is_some() || compuestas.contains(indice)
        })
        .count()
}

/// Cabeceras multinivel: todos sus tokens no triviales, en cualquier orden, en las
/// `LINEAS_CABECERA_COMPUESTA` líneas de la ventana previas a su primera fila anclada. Solo
/// cuentan si en esas líneas están las de `MIN_CABECERAS_COMPUESTAS` columnas distintas cuyos
/// tokens, juntos, no caben en los de una sola cabecera
fn cabeceras_compuestas(
    texto: &TextoIndexado,
    filas: &[FilaIndexada],
    cabeceras: &[CabeceraIndexada],
    (inicio, fin): (u32, u32),
) -> BTreeSet<usize> {
    let Some(primera) = filas
        .iter()
        .filter_map(|fila| fila.primera_entre((inicio, fin)))
        .map(|(linea, _)| linea)
        .min()
    else {
        return BTreeSet::new();
    };
    let previos: HashSet<&String> = (primera.saturating_sub(LINEAS_CABECERA_COMPUESTA)..primera)
        .filter(|&linea| (inicio..=fin).contains(&texto.paginas[linea]))
        .flat_map(|linea| &texto.lineas[linea])
        .collect();
    let presentes: BTreeSet<usize> = cabeceras
        .iter()
        .enumerate()
        .filter(|(_, cabecera)| {
            !cabecera.no_triviales.is_empty()
                && cabecera.no_triviales.iter().all(|t| previos.contains(t))
        })
        .map(|(indice, _)| indice)
        .collect();
    let distintas: BTreeSet<&BTreeSet<String>> = presentes
        .iter()
        .map(|&indice| &cabeceras[indice].no_triviales)
        .collect();
    let union: BTreeSet<&String> = distintas.iter().copied().flatten().collect();
    // Una frase con las palabras de una columna contiene también las columnas incluidas en ella
    let caben_en_una = cabeceras.iter().any(|cabecera| {
        union
            .iter()
            .all(|token| cabecera.no_triviales.contains(*token))
    });
    if distintas.len() >= MIN_CABECERAS_COMPUESTAS && !caben_en_una {
        presentes
    } else {
        BTreeSet::new()
    }
}

/// La clave de una fila es su primera celda: si alguna aparición abre su línea, las que van en
/// medio de otra línea son de otra fila (un número que reaparece como valor) y se descartan
fn preferir_las_que_abren_linea(apariciones: Vec<Posicion>) -> Vec<Posicion> {
    if apariciones.iter().any(|&(_, posicion)| posicion == 0) {
        apariciones
            .into_iter()
            .filter(|&(_, posicion)| posicion == 0)
            .collect()
    } else {
        apariciones
    }
}

/// Ventana de páginas inequívoca con más filas en orden y más cabeceras; a igualdad, la más corta
fn elegir_bloque(
    texto: &TextoIndexado,
    filas: &[FilaIndexada],
    cabeceras: &[CabeceraIndexada],
) -> Option<(u32, u32)> {
    let paginas: Vec<u32> = texto
        .paginas
        .iter()
        .copied()
        .collect::<BTreeSet<u32>>()
        .into_iter()
        .collect();
    let mut mejor_puntuacion = (0, 0);
    let mut mejores: Vec<(u32, u32)> = Vec::new();
    for (indice, &inicio) in paginas.iter().enumerate() {
        for &fin in paginas[indice..]
            .iter()
            .take_while(|&&fin| fin - inicio < MAX_PAGINAS_BLOQUE)
        {
            let puntuacion = (
                filas_en_orden(filas, (inicio, fin)),
                cabeceras_presentes(texto, filas, cabeceras, (inicio, fin)),
            );
            if puntuacion > mejor_puntuacion {
                mejor_puntuacion = puntuacion;
                mejores.clear();
            }
            if puntuacion == mejor_puntuacion {
                mejores.push((inicio, fin));
            }
        }
    }
    let filas_minimas = MIN_FILAS_EN_ORDEN.max(filas.len().div_ceil(2));
    let cabeceras_minimas = 1.max(cabeceras.len().div_ceil(2));
    if mejor_puntuacion.0 < filas_minimas || mejor_puntuacion.1 < cabeceras_minimas {
        return None;
    }
    let extension = |(inicio, fin): (u32, u32)| fin - inicio;
    let minima = mejores.iter().copied().map(extension).min()?;
    let mut cortas = mejores
        .iter()
        .copied()
        .filter(|&ventana| extension(ventana) == minima);
    let elegida = cortas.next()?;
    let disjunta = |(inicio, fin): (u32, u32)| fin < elegida.0 || inicio > elegida.1;
    (cortas.next().is_none() && !mejores.iter().copied().any(disjunta)).then_some(elegida)
}

/// Longitud de la subsecuencia creciente más larga de las primeras apariciones de las claves
fn filas_en_orden(filas: &[FilaIndexada], ventana: (u32, u32)) -> usize {
    let mut colas: Vec<Posicion> = Vec::new();
    for posicion in filas.iter().filter_map(|f| f.primera_entre(ventana)) {
        let hueco = colas.partition_point(|cola| *cola < posicion);
        if hueco == colas.len() {
            colas.push(posicion);
        } else {
            colas[hueco] = posicion;
        }
    }
    colas.len()
}

/// En una tabla leída en el sentido de las filas, cada clave abre su propia línea. Si se amontonan
/// en pocas líneas, las «filas» destiladas son columnas del texto y su página no es fiable
fn claves_en_lineas_distintas(filas: &[FilaIndexada], bloque: (u32, u32)) -> bool {
    let primeras: Vec<usize> = filas
        .iter()
        .filter_map(|f| f.primera_entre(bloque).map(|(linea, _)| linea))
        .collect();
    let distintas = primeras.iter().collect::<BTreeSet<_>>().len();
    distintas >= MIN_FILAS_EN_ORDEN.max(primeras.len().div_ceil(2))
}

/// Página única de un conjunto de líneas, con la primera de ellas
fn pagina_unica(texto: &TextoIndexado, lineas: &[usize]) -> Option<(u32, usize)> {
    let paginas: BTreeSet<u32> = lineas.iter().map(|&l| texto.paginas[l]).collect();
    (paginas.len() == 1).then(|| (texto.paginas[lineas[0]], lineas[0]))
}

/// Página de cada fila en un bloque de varias páginas: apariciones de la clave con sus valores,
/// desempate por el orden de las filas vecinas y anulación de las filas que invierten el orden
fn paginas_por_fila(
    texto: &TextoIndexado,
    filas: &[FilaIndexada],
    (inicio, fin): (u32, u32),
) -> Vec<Option<u32>> {
    let candidatas: Vec<Vec<usize>> = filas
        .iter()
        .map(|fila| {
            fila.apariciones
                .iter()
                .map(|&(linea, _)| linea)
                .filter(|&linea| (inicio..=fin).contains(&texto.paginas[linea]))
                .collect()
        })
        .collect();
    let unicas: Vec<Option<(u32, usize)>> = candidatas
        .iter()
        .map(|lineas| pagina_unica(texto, lineas))
        .collect();
    let desempatadas = candidatas
        .iter()
        .enumerate()
        .map(|(fila, lineas)| {
            if let Some((pagina, _)) = unicas[fila] {
                return Some(pagina);
            }
            let anterior = unicas[..fila].iter().rev().flatten().next().map(|a| a.1);
            let siguiente = unicas[fila + 1..].iter().flatten().next().map(|s| s.1);
            let entre: Vec<usize> = lineas
                .iter()
                .copied()
                .filter(|&linea| {
                    anterior.is_none_or(|a| linea > a) && siguiente.is_none_or(|s| linea < s)
                })
                .collect();
            pagina_unica(texto, &entre).map(|(pagina, _)| pagina)
        })
        .collect();
    anular_inversiones(desempatadas)
}

/// Deja en `None` las filas cuya página es menor que la de una fila anterior o mayor que la de una
/// posterior: el orden de la tabla y el del texto no concuerdan y no se sabe cuál falla
fn anular_inversiones(paginas: Vec<Option<u32>>) -> Vec<Option<u32>> {
    let mut maximo_previo = Vec::with_capacity(paginas.len());
    let mut maximo: Option<u32> = None;
    for pagina in &paginas {
        maximo_previo.push(maximo);
        maximo = maximo.max(*pagina);
    }
    let mut minimo_posterior = vec![None; paginas.len()];
    let mut minimo: Option<u32> = None;
    for (indice, pagina) in paginas.iter().enumerate().rev() {
        minimo_posterior[indice] = minimo;
        if let Some(p) = pagina {
            minimo = Some(minimo.map_or(*p, |m| m.min(*p)));
        }
    }
    paginas
        .iter()
        .enumerate()
        .map(|(indice, pagina)| {
            pagina.filter(|p| {
                maximo_previo[indice].is_none_or(|m| m <= *p)
                    && minimo_posterior[indice].is_none_or(|m| m >= *p)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Texto paginado de prueba: `(página, línea)`
    fn paginado(lineas: &[(u32, &'static str)]) -> Vec<LineaPaginada<'static>> {
        lineas
            .iter()
            .map(|(pagina, texto)| LineaPaginada {
                pagina: *pagina,
                texto,
            })
            .collect()
    }

    fn fila(clave: &[&str], valores: &[&str]) -> FilaParaAnclar {
        FilaParaAnclar {
            clave: clave.iter().map(|s| s.to_string()).collect(),
            valores: valores.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn tabla(cabeceras: &[&str], filas: Vec<FilaParaAnclar>) -> TablaParaAnclar {
        TablaParaAnclar {
            cabeceras: cabeceras.iter().map(|s| s.to_string()).collect(),
            filas,
        }
    }

    fn todas_en(anclaje: &AnclajeTabla, pagina: u32) -> bool {
        anclaje.por_fila.iter().all(|p| *p == Some(pagina))
    }

    // 1. Tabla en una página, en varios idiomas y dominios

    #[test]
    fn test_tabla_en_una_pagina_en_espanol_material_y_temperatura() {
        let lineas = paginado(&[
            (20, "Texto previo sobre el procedimiento general."),
            (21, "Material Espesor ≤ 20 Espesor > 20"),
            (21, "Acero S275 75 125"),
            (21, "Acero S355 100 150"),
            (21, "Acero inoxidable 50 80"),
            (22, "Capítulo siguiente sin tabla."),
        ]);
        let t = tabla(
            &["20"],
            vec![
                fila(&["Acero S275"], &["75", "125"]),
                fila(&["Acero S355"], &["100", "150"]),
                fila(&["Acero inoxidable"], &["50", "80"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((21, 21)));
        assert!(todas_en(&anclaje, 21), "{anclaje:?}");
    }

    #[test]
    fn test_tabla_en_una_pagina_en_ingles_ingresos_por_trimestre() {
        let lineas = paginado(&[
            (3, "Revenue by region (USD million)"),
            (3, "Region Q1 Q2 Q3 Q4"),
            (3, "North America 12.5 13.1 14.0 15.2"),
            (3, "Europe 8.4 8.9 9.3 9.9"),
            (3, "Asia Pacific 5.1 5.6 6.2 7.0"),
        ]);
        let t = tabla(
            &["Q1", "Q2", "Q3", "Q4"],
            vec![
                fila(&["North America"], &["12.5", "13.1", "14.0", "15.2"]),
                fila(&["Europe"], &["8.4", "8.9", "9.3", "9.9"]),
                fila(&["Asia Pacific"], &["5.1", "5.6", "6.2", "7.0"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((3, 3)));
        assert!(todas_en(&anclaje, 3));
    }

    #[test]
    fn test_tabla_en_una_pagina_en_aleman_dosis_por_peso_con_coma_decimal() {
        let lineas = paginado(&[
            (7, "Dosierung nach Körpergewicht"),
            (7, "Körpergewicht Morgens Abends"),
            (7, "unter 40 kg 0,5 mg 0,25 mg"),
            (7, "40 bis 70 kg 1,0 mg 0,5 mg"),
            (7, "über 70 kg 1,5 mg 0,75 mg"),
        ]);
        let t = tabla(
            &["Morgens", "Abends"],
            vec![
                fila(&["40"], &["0.5 mg", "0.25 mg"]),
                fila(&["40", "70"], &["1.0 mg", "0.5 mg"]),
                fila(&["70"], &["1.5 mg", "0.75 mg"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((7, 7)));
        assert!(todas_en(&anclaje, 7));
    }

    #[test]
    fn test_tabla_en_una_pagina_en_griego() {
        let lineas = paginado(&[
            (12, "Πίνακας θερμοκρασιών λειτουργίας"),
            (12, "Υλικό Ελάχιστη Μέγιστη"),
            (12, "Χάλυβας −20 400"),
            (12, "Αλουμίνιο −40 150"),
            (12, "Χαλκός −30 250"),
        ]);
        let t = tabla(
            &["Ελάχιστη", "Μέγιστη"],
            vec![
                fila(&["Χάλυβας"], &["−20", "400"]),
                fila(&["Αλουμίνιο"], &["−40", "150"]),
                fila(&["Χαλκός"], &["−30", "250"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((12, 12)));
        assert!(todas_en(&anclaje, 12));
    }

    // 2. Una introducción menciona una clave y un valor en otra página

    #[test]
    fn test_introduccion_en_otra_pagina_no_atrae_ninguna_fila() {
        let lineas = paginado(&[
            (
                6,
                "Como se verá, el acero S275 exige 75 grados en la tabla del capítulo 4.",
            ),
            (21, "Material Espesor ≤ 20 Espesor > 20"),
            (21, "Acero S275 75 125"),
            (21, "Acero S355 100 150"),
            (21, "Acero inoxidable 50 80"),
            (22, "Material Espesor ≤ 20 Espesor > 20"),
            (22, "Aluminio 6061 30 45"),
            (22, "Cobre C110 40 60"),
        ]);
        let t = tabla(
            &["20"],
            vec![
                fila(&["Acero S275"], &["75", "125"]),
                fila(&["Acero S355"], &["100", "150"]),
                fila(&["Acero inoxidable"], &["50", "80"]),
                fila(&["Aluminio 6061"], &["30", "45"]),
                fila(&["Cobre C110"], &["40", "60"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((21, 22)));
        assert!(!anclaje.por_fila.contains(&Some(6)), "{anclaje:?}");
        assert_eq!(
            anclaje.por_fila,
            vec![Some(21), Some(21), Some(21), Some(22), Some(22)]
        );
    }

    // 3. Tabla partida en dos páginas

    #[test]
    fn test_tabla_partida_cada_fila_en_su_pagina() {
        let mut lineas = vec![(21, "Lote Humedad Densidad Resistencia")];
        let filas_texto = [
            (21, "L-01 12 2,41 31"),
            (21, "L-02 13 2,43 33"),
            (21, "L-03 11 2,40 30"),
            (21, "L-04 14 2,44 35"),
            (21, "L-05 12 2,42 32"),
            (22, "L-06 15 2,45 36"),
            (22, "L-07 13 2,39 29"),
            (22, "L-08 12 2,46 37"),
            (22, "L-09 16 2,38 28"),
        ];
        lineas.extend(filas_texto);
        let lineas = paginado(&lineas);
        let filas = filas_texto
            .iter()
            .map(|(_, texto)| {
                let partes: Vec<&str> = texto.split(' ').collect();
                fila(&[partes[0]], &partes[1..])
            })
            .collect();
        let t = tabla(&["Humedad", "Densidad", "Resistencia"], filas);
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((21, 22)));
        let esperado: Vec<Option<u32>> = filas_texto.iter().map(|(p, _)| Some(*p)).collect();
        assert_eq!(anclaje.por_fila, esperado);
    }

    // 4. Clave repetida en las dos páginas del bloque

    fn tabla_con_clave_repetida() -> TablaParaAnclar {
        tabla(
            &["Mín", "Máx"],
            vec![
                fila(&["A1"], &["10", "20"]),
                fila(&["A2"], &["11", "21"]),
                fila(&["B7"], &["12", "22"]),
                fila(&["A3"], &["13", "23"]),
                fila(&["A4"], &["14", "24"]),
            ],
        )
    }

    #[test]
    fn test_clave_repetida_se_resuelve_por_monotonia_entre_vecinas() {
        let lineas = paginado(&[
            (21, "Código Mín Máx"),
            (21, "A1 10 20"),
            (21, "A2 11 21"),
            (21, "B7 12 22"),
            (22, "Código Mín Máx"),
            (22, "A3 13 23"),
            (22, "A4 14 24"),
            (22, "Nota: B7 sustituye a 12 22 de la edición anterior."),
        ]);
        let anclaje = anclar_tabla(&lineas, &tabla_con_clave_repetida());
        assert_eq!(anclaje.bloque, Some((21, 22)));
        assert_eq!(
            anclaje.por_fila,
            vec![Some(21), Some(21), Some(21), Some(22), Some(22)]
        );
    }

    #[test]
    fn test_clave_repetida_que_las_vecinas_no_deciden_queda_sin_pagina() {
        let lineas = paginado(&[
            (21, "Código Mín Máx"),
            (21, "A1 10 20"),
            (21, "A2 11 21"),
            (21, "B7 12 22"),
            (22, "B7 12 22"),
            (22, "Código Mín Máx"),
            (22, "A3 13 23"),
            (22, "A4 14 24"),
        ]);
        let anclaje = anclar_tabla(&lineas, &tabla_con_clave_repetida());
        assert_eq!(anclaje.bloque, Some((21, 22)));
        assert_eq!(anclaje.por_fila[2], None, "{anclaje:?}");
        assert_eq!(anclaje.por_fila[0], Some(21));
        assert_eq!(anclaje.por_fila[4], Some(22));
    }

    // 5. Dos ventanas disjuntas igual de buenas

    #[test]
    fn test_tabla_duplicada_en_un_anexo_queda_sin_bloque() {
        let lineas = paginado(&[
            (10, "Zona Invierno Verano"),
            (10, "Norte 5 18"),
            (10, "Centro 8 24"),
            (10, "Sur 12 30"),
            (40, "Zona Invierno Verano"),
            (40, "Norte 5 18"),
            (40, "Centro 8 24"),
            (40, "Sur 12 30"),
        ]);
        let t = tabla(
            &["Invierno", "Verano"],
            vec![
                fila(&["Norte"], &["5", "18"]),
                fila(&["Centro"], &["8", "24"]),
                fila(&["Sur"], &["12", "30"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, None);
        assert!(anclaje.por_fila.iter().all(Option::is_none));
    }

    // 6. Claves en desorden

    #[test]
    fn test_claves_en_desorden_no_alcanzan_el_minimo() {
        let lineas = paginado(&[
            (5, "Clase Valor"),
            (5, "E 5"),
            (5, "D 4"),
            (5, "C 3"),
            (5, "B 2"),
            (5, "A 1"),
        ]);
        let t = tabla(
            &["Valor"],
            ["A", "B", "C", "D", "E"]
                .iter()
                .zip(["1", "2", "3", "4", "5"])
                .map(|(c, v)| fila(&[c], &[v]))
                .collect(),
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, None);
    }

    // 7. CJK sin espacios

    #[test]
    fn test_cjk_sin_espacios_se_ancla_por_caracteres() {
        let lineas = paginado(&[
            (4, "前言没有表格。"),
            (9, "材料最低温度最高温度"),
            (9, "钢材零下二十度四百度"),
            (9, "铝材零下四十度一百五十度"),
            (9, "铜材零下三十度二百五十度"),
        ]);
        let t = tabla(
            &["最低温度", "最高温度"],
            vec![
                fila(&["钢材"], &["零下二十度", "四百度"]),
                fila(&["铝材"], &["零下四十度", "一百五十度"]),
                fila(&["铜材"], &["零下三十度", "二百五十度"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((9, 9)));
        assert!(todas_en(&anclaje, 9));
    }

    // 8. Sin páginas (Markdown)

    #[test]
    fn test_sin_lineas_paginadas_todo_queda_en_none() {
        let t = tabla(
            &["Q1"],
            vec![fila(&["Norte"], &["1", "2"]), fila(&["Sur"], &["3", "4"])],
        );
        let anclaje = anclar_tabla(&[], &t);
        assert_eq!(anclaje.bloque, None);
        assert_eq!(anclaje.por_fila, vec![None, None]);
    }

    // Reglas añadidas para no dar páginas falsas

    #[test]
    fn test_en_bloque_de_una_pagina_la_fila_ausente_queda_sin_pagina() {
        let lineas = paginado(&[
            (30, "Región Q1 Q2"),
            (30, "Norte 1 2"),
            (30, "Centro 3 4"),
            (30, "Sur 5 6"),
            (31, "Islas 7 8"),
        ]);
        let t = tabla(
            &["Q1", "Q2"],
            vec![
                fila(&["Norte"], &["1", "2"]),
                fila(&["Centro"], &["3", "4"]),
                fila(&["Sur"], &["5", "6"]),
                fila(&["Insular"], &["7", "8"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((30, 30)));
        assert_eq!(anclaje.por_fila, vec![Some(30), Some(30), Some(30), None]);
    }

    #[test]
    fn test_tabla_transpuesta_en_varias_paginas_queda_sin_pagina_por_fila() {
        // Las «filas» destiladas son en el texto las columnas: sus claves comparten una línea
        let lineas = paginado(&[
            (50, "Ensayo Norte Centro Sur Este"),
            (50, "Tracción 410 420 430 440"),
            (50, "Flexión 210 220 230 240"),
            (51, "Torsión 110 120 130 140"),
            (51, "Fatiga 310 320 330 340"),
        ]);
        let t = tabla(
            &["Tracción", "Flexión", "Torsión", "Fatiga"],
            vec![
                fila(&["Norte"], &["410", "210", "110", "310"]),
                fila(&["Centro"], &["420", "220", "120", "320"]),
                fila(&["Sur"], &["430", "230", "130", "330"]),
                fila(&["Este"], &["440", "240", "140", "340"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((50, 51)));
        assert!(anclaje.por_fila.iter().all(Option::is_none), "{anclaje:?}");
    }

    #[test]
    fn test_numeros_repetidos_entre_paginas_no_dan_pagina_falsa() {
        // Las claves de fila son números que reaparecen como valores en la otra página
        let lineas = paginado(&[
            (60, "Diámetro Par Carga"),
            (60, "10 25 40"),
            (60, "12 40 55"),
            (60, "16 55 70"),
            (61, "20 70 25"),
            (61, "24 12 16"),
        ]);
        let t = tabla(
            &["Par", "Carga"],
            vec![
                fila(&["10"], &["25", "40"]),
                fila(&["12"], &["40", "55"]),
                fila(&["16"], &["55", "70"]),
                fila(&["20"], &["70", "25"]),
                fila(&["24"], &["12", "16"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        let correctas = [60, 60, 60, 61, 61];
        for (pagina, correcta) in anclaje.por_fila.iter().zip(correctas) {
            assert!(pagina.is_none() || *pagina == Some(correcta), "{anclaje:?}");
        }
    }

    #[test]
    fn test_claves_numericas_que_son_valores_de_otra_pagina_no_dan_bloque_de_una_pagina() {
        // Las claves 20 y 24 (p. 61) aparecen en la p. 60 como valores de otras filas, y sus
        // valores también están cerca, pero antes de la clave: no son su fila
        let lineas = paginado(&[
            (60, "Diámetro Par Carga"),
            (60, "10 25 24"),
            (60, "12 20 55"),
            (60, "16 24 20"),
            (61, "Diámetro Par Carga"),
            (61, "20 10 12"),
            (61, "24 16 10"),
        ]);
        let t = tabla(
            &["Par", "Carga"],
            vec![
                fila(&["10"], &["25", "24"]),
                fila(&["12"], &["20", "55"]),
                fila(&["16"], &["24", "20"]),
                fila(&["20"], &["10", "12"]),
                fila(&["24"], &["16", "10"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        let correctas = [60, 60, 60, 61, 61];
        for (pagina, correcta) in anclaje.por_fila.iter().zip(correctas) {
            assert!(pagina.is_none() || *pagina == Some(correcta), "{anclaje:?}");
        }
        assert_eq!(anclaje.bloque, Some((60, 61)));
    }

    #[test]
    fn test_clave_al_final_de_una_linea_ajena_no_es_su_fila() {
        // «28» cierra una fila de la p. 94 y la línea siguiente lleva sus valores por casualidad;
        // su fila real está en la p. 95. Caso del generador sintético N2
        let lineas = paginado(&[
            (94, "# Fino Medio"),
            (94, "6 6 42"),
            (94, "8 27 6"),
            (94, "20 8 28"),
            (94, "27 52 58"),
            (95, "# Fino Medio"),
            (95, "28 27 52"),
            (95, "42 28 42"),
            (95, "52 27 20"),
            (95, "58 42 58"),
        ]);
        let t = tabla(
            &["Fino", "Medio"],
            vec![
                fila(&["6"], &["6", "42"]),
                fila(&["8"], &["27", "6"]),
                fila(&["20"], &["8", "28"]),
                fila(&["27"], &["52", "58"]),
                fila(&["28"], &["27", "52"]),
                fila(&["42"], &["28", "42"]),
                fila(&["52"], &["27", "20"]),
                fila(&["58"], &["42", "58"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        let correctas = [94, 94, 94, 94, 95, 95, 95, 95];
        for (pagina, correcta) in anclaje.por_fila.iter().zip(correctas) {
            assert!(pagina.is_none() || *pagina == Some(correcta), "{anclaje:?}");
        }
    }

    #[test]
    fn test_inversion_de_orden_anula_las_filas_en_conflicto() {
        let lineas = paginado(&[
            (70, "Pieza Ancho Alto"),
            (70, "P1 1 2"),
            (70, "P2 3 4"),
            (70, "P4 7 8"),
            (71, "P3 5 6"),
            (71, "P5 9 10"),
            (71, "P6 11 12"),
        ]);
        let t = tabla(
            &["Ancho", "Alto"],
            ["P1", "P2", "P3", "P4", "P5", "P6"]
                .iter()
                .enumerate()
                .map(|(i, clave)| {
                    let a = (2 * i + 1).to_string();
                    let b = (2 * i + 2).to_string();
                    FilaParaAnclar {
                        clave: vec![clave.to_string()],
                        valores: vec![a, b],
                    }
                })
                .collect(),
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((70, 71)));
        // P3 (71) antes que P4 (70) en la tabla: ninguna de las dos es fiable
        assert_eq!(anclaje.por_fila[2], None);
        assert_eq!(anclaje.por_fila[3], None);
        assert_eq!(anclaje.por_fila[0], Some(70));
        assert_eq!(anclaje.por_fila[5], Some(71));
    }

    #[test]
    fn test_tabla_de_una_sola_fila_no_se_ancla() {
        let lineas = paginado(&[(2, "Clase Valor"), (2, "Única 5")]);
        let t = tabla(&["Valor"], vec![fila(&["Única"], &["5"])]);
        assert_eq!(anclar_tabla(&lineas, &t).bloque, None);
    }

    // Cabeceras compuestas de varios niveles

    fn tabla_rendimiento_y_coste() -> TablaParaAnclar {
        // El destilador compone cada cabecera juntando los niveles: ninguna aparece contigua
        tabla(
            &[
                "Rendimiento ES-FR",
                "Rendimiento ES-IT",
                "Coste de cálculo ES-FR",
                "Coste de cálculo ES-IT",
            ],
            vec![
                fila(&["Modelo A"], &["21.4", "30.2"]),
                fila(&["Modelo B"], &["22.8", "31.0"]),
                fila(&["Modelo C"], &["23.5", "32.7"]),
            ],
        )
    }

    #[test]
    fn test_cabecera_compuesta_en_dos_lineas_cuenta_como_presente() {
        let lineas = paginado(&[
            (8, "Tabla 2: comparación de modelos en las dos tareas."),
            (8, "Sistema"),
            (8, "Rendimiento Coste de cálculo (GFLOP)"),
            (8, "ES-FR ES-IT ES-FR ES-IT"),
            (8, "Modelo A 21.4 30.2"),
            (8, "Modelo B 22.8 31.0"),
            (8, "Modelo C 23.5 32.7"),
        ]);
        let anclaje = anclar_tabla(&lineas, &tabla_rendimiento_y_coste());
        assert_eq!(anclaje.bloque, Some((8, 8)), "{anclaje:?}");
        assert!(todas_en(&anclaje, 8));
    }

    #[test]
    fn test_cabecera_compuesta_de_tres_niveles_con_unidades_en_linea_aparte() {
        let lineas = paginado(&[
            (14, "Ensayo de laboratorio"),
            (14, "Tracción Compresión"),
            (14, "Inicial Final Inicial Final"),
            (14, "(MPa) (MPa) (MPa) (MPa)"),
            (14, "Probeta 1 310 295 410 402"),
            (14, "Probeta 2 305 290 415 398"),
            (14, "Probeta 3 318 301 420 405"),
        ]);
        let t = tabla(
            &[
                "Ensayo Tracción Inicial MPa",
                "Ensayo Tracción Final MPa",
                "Ensayo Compresión Inicial MPa",
                "Ensayo Compresión Final MPa",
            ],
            vec![
                fila(&["Probeta 1"], &["310", "295", "410", "402"]),
                fila(&["Probeta 2"], &["305", "290", "415", "398"]),
                fila(&["Probeta 3"], &["318", "301", "420", "405"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((14, 14)), "{anclaje:?}");
        assert!(todas_en(&anclaje, 14));
    }

    #[test]
    fn test_prosa_con_las_palabras_de_una_sola_columna_no_activa_la_cabecera_compuesta() {
        let lineas = paginado(&[
            (
                8,
                "El coste de cálculo entre ES-FR fue menor que el previsto.",
            ),
            (8, "Modelo A 21.4 30.2"),
            (8, "Modelo B 22.8 31.0"),
            (8, "Modelo C 23.5 32.7"),
        ]);
        let anclaje = anclar_tabla(&lineas, &tabla_rendimiento_y_coste());
        assert_eq!(anclaje.bloque, None, "{anclaje:?}");
    }

    #[test]
    fn test_prosa_de_una_columna_no_activa_por_otra_columna_contenida_en_ella() {
        // «e Máximo» se reduce a «Máximo», que está dentro de «Carga Máximo»: la frase sigue
        // teniendo solo las palabras de una columna
        let lineas = paginado(&[
            (8, "Se revisó la carga y el máximo antes del turno."),
            (8, "Lote 1 10 11 12 13"),
            (8, "Lote 2 20 21 22 23"),
            (8, "Lote 3 30 31 32 33"),
        ]);
        let t = tabla(
            &["Carga Máximo", "Carga Mínimo", "e Máximo", "e Mínimo"],
            vec![
                fila(&["Lote 1"], &["10", "11", "12", "13"]),
                fila(&["Lote 2"], &["20", "21", "22", "23"]),
                fila(&["Lote 3"], &["30", "31", "32", "33"]),
            ],
        );
        assert_eq!(anclar_tabla(&lineas, &t).bloque, None);
    }

    #[test]
    fn test_cabecera_compuesta_a_mas_de_cuatro_lineas_de_la_primera_fila_no_cuenta() {
        let lineas = paginado(&[
            (8, "Rendimiento Coste de cálculo (GFLOP)"),
            (8, "ES-FR ES-IT ES-FR ES-IT"),
            (8, "Primera línea de notas sin relación."),
            (8, "Segunda línea de notas sin relación."),
            (8, "Tercera línea de notas sin relación."),
            (8, "Cuarta línea de notas sin relación."),
            (8, "Modelo A 21.4 30.2"),
            (8, "Modelo B 22.8 31.0"),
            (8, "Modelo C 23.5 32.7"),
        ]);
        let anclaje = anclar_tabla(&lineas, &tabla_rendimiento_y_coste());
        assert_eq!(anclaje.bloque, None, "{anclaje:?}");
    }

    #[test]
    fn test_cabecera_compuesta_ignora_los_conectores_de_una_letra_en_minuscula() {
        let lineas = paginado(&[
            (5, "Carga Descarga"),
            (5, "Norte Sur Norte Sur"),
            (5, "Lote 1 10 11 12 13"),
            (5, "Lote 2 20 21 22 23"),
            (5, "Lote 3 30 31 32 33"),
        ]);
        let t = tabla(
            &[
                "Carga y Norte",
                "Carga y Sur",
                "Descarga y Norte",
                "Descarga y Sur",
            ],
            vec![
                fila(&["Lote 1"], &["10", "11", "12", "13"]),
                fila(&["Lote 2"], &["20", "21", "22", "23"]),
                fila(&["Lote 3"], &["30", "31", "32", "33"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((5, 5)), "{anclaje:?}");
    }

    #[test]
    fn test_cabecera_compuesta_exige_la_letra_mayuscula_de_una_cabecera() {
        // «A» y «B» son símbolos de columna, no conectores: si faltan, la cabecera no está
        let lineas = paginado(&[
            (5, "Grupo Serie"),
            (5, "Lote 1 10 11"),
            (5, "Lote 2 20 21"),
            (5, "Lote 3 30 31"),
        ]);
        let t = tabla(
            &["Grupo Serie A", "Grupo Serie B"],
            vec![
                fila(&["Lote 1"], &["10", "11"]),
                fila(&["Lote 2"], &["20", "21"]),
                fila(&["Lote 3"], &["30", "31"]),
            ],
        );
        assert_eq!(anclar_tabla(&lineas, &t).bloque, None);
    }

    #[test]
    fn test_cabecera_compuesta_en_tabla_partida_ancla_cada_fila() {
        let lineas = paginado(&[
            (30, "Rendimiento Coste de cálculo (GFLOP)"),
            (30, "ES-FR ES-IT ES-FR ES-IT"),
            (30, "Modelo A 21.4 30.2"),
            (30, "Modelo B 22.8 31.0"),
            (31, "Modelo C 23.5 32.7"),
            (31, "Modelo D 24.1 33.3"),
        ]);
        let mut t = tabla_rendimiento_y_coste();
        t.filas.push(fila(&["Modelo D"], &["24.1", "33.3"]));
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((30, 31)), "{anclaje:?}");
        assert_eq!(
            anclaje.por_fila,
            vec![Some(30), Some(30), Some(31), Some(31)]
        );
    }

    // Potencias de la celda destilada frente al texto aplanado

    #[test]
    fn test_potencia_de_la_celda_se_busca_sin_el_circunflejo() {
        let lineas = paginado(&[
            (8, "Sistema Precisión Coste"),
            (8, "Modelo A 21.4 2.3 · 1019"),
            (8, "Modelo B 22.8 9.6 · 1018"),
            (8, "Modelo C 23.5 1.2x1021"),
        ]);
        let t = tabla(
            &["Precisión", "Coste"],
            vec![
                fila(&["Modelo A"], &["21.4", "2.3 × 10^19"]),
                fila(&["Modelo B"], &["22.8", "9.6 × 10^18"]),
                fila(&["Modelo C"], &["23.5", "1.2x10^21"]),
            ],
        );
        let anclaje = anclar_tabla(&lineas, &t);
        assert_eq!(anclaje.bloque, Some((8, 8)), "{anclaje:?}");
        assert!(todas_en(&anclaje, 8));
    }

    #[test]
    fn test_numero_de_cuatro_cifras_del_texto_no_se_convierte_en_potencia() {
        // Solo la celda se transforma: un año del texto no equivale a una potencia ni al revés
        let lineas = paginado(&[
            (8, "Sistema Precisión Coste"),
            (8, "Modelo A 21.4 10^19"),
            (8, "Modelo B 22.8 10^18"),
            (8, "Modelo C 23.5 10^21"),
        ]);
        let t = tabla(
            &["Precisión", "Coste"],
            vec![
                fila(&["Modelo A"], &["21.4", "1019"]),
                fila(&["Modelo B"], &["22.8", "1018"]),
                fila(&["Modelo C"], &["23.5", "1021"]),
            ],
        );
        assert_eq!(anclar_tabla(&lineas, &t).bloque, None);
    }

    #[test]
    fn test_variante_sin_circunflejo_solo_entre_cifras() {
        assert_eq!(
            variante_sin_circunflejo("2.5x10^19").as_deref(),
            Some("2.5x1019")
        );
        assert_eq!(
            variante_sin_circunflejo("1.0 × 10^20").as_deref(),
            Some("1.0 × 1020")
        );
        assert_eq!(variante_sin_circunflejo("10^-3"), None);
        assert_eq!(variante_sin_circunflejo("x^2"), None);
        assert_eq!(variante_sin_circunflejo("2024"), None);
    }

    #[test]
    fn test_tokens_pliegan_mayusculas_y_unifican_coma_y_punto_decimal() {
        assert_eq!(
            tokens("Acero S275: 0,45 mm"),
            ["ACERO", "S", "275", "0.45", "MM"]
        );
        assert_eq!(tokens("1.5-2,5"), ["1.5", "2.5"]);
        assert_eq!(tokens("钢材 4-A"), ["钢", "材", "4", "A"]);
        assert!(tokens(" — ").is_empty());
    }

    #[test]
    fn test_contiene_secuencia_exige_tokens_completos_y_contiguos() {
        assert!(contiene_secuencia("Véase la Table 4-A.", "4-A"));
        assert!(!contiene_secuencia("Véase la Table 14-A.", "4-A"));
        assert!(!contiene_secuencia("Véase la Table 4-AB.", "4-A"));
        assert!(!contiene_secuencia("texto", " "));
    }
}
