//! Negaciones sin evidencia (`docs/arquitectura/ssd_arnes_negaciones.md`): una respuesta que
//! afirma con rotundidad que algo no existe o no se aplica («no hay ampliación», «there is no
//! exception») sobre un aspecto que el turno no buscó. Lógica pura.
//!
//! Solo se aplica si el turno no consultó texto del documento (búsqueda o subárbol): es negar
//! tras leer solo una fila o nada. Las palabras de las instrucciones de la app cuentan como
//! consultadas: «no hay conflicto de prelación» lee un campo estructurado, no afirma nada del mundo.
//!
//! Solo cuentan las afirmaciones sobre el mundo. Las abstenciones sobre la búsqueda («no consta»,
//! «no he encontrado», «no aparece en lo consultado») no entran: son la forma correcta de decir
//! que la búsqueda no devolvió nada. Lo que va entre comillas es texto del documento y tampoco.

use std::collections::HashSet;

use super::texto_normalizado::plegar_texto;

/// Locución que afirma la ausencia de lo que la sigue («no hay X», «there is no X»)
const NEGACIONES_EXISTENCIALES: &[&[&str]] = &[
    &["no", "hay"],
    &["no", "existe"],
    &["no", "existen"],
    &["no", "se", "contempla"],
    &["no", "se", "contemplan"],
    &["no", "se", "preve"],
    &["no", "se", "preven"],
    &["no", "se", "establece"],
    &["no", "se", "establecen"],
    &["no", "se", "admite"],
    &["no", "se", "admiten"],
    &["there", "is", "no"],
    &["there", "are", "no"],
    &["there", "is", "not", "any"],
    &["there", "are", "not", "any"],
];
/// Locución que niega lo que la precede («la ampliación no se aplica»)
const NEGACIONES_VERBALES: &[&[&str]] = &[
    &["no", "se", "aplica"],
    &["no", "se", "aplican"],
    &["no", "aplica"],
    &["no", "aplican"],
    &["does", "not", "apply"],
    &["do", "not", "apply"],
];
/// Palabras que no nombran un aspecto: artículos, cuantificadores y palabras de abstención
const PALABRAS_SIN_ASPECTO: &[&str] = &[
    "ningun",
    "ninguna",
    "ninguno",
    "ningunas",
    "ningunos",
    "algun",
    "alguna",
    "alguno",
    "un",
    "una",
    "unos",
    "unas",
    "el",
    "la",
    "los",
    "las",
    "lo",
    "de",
    "del",
    "a",
    "al",
    "en",
    "ese",
    "esa",
    "esos",
    "esas",
    "este",
    "esta",
    "estos",
    "estas",
    "tal",
    "tales",
    "otra",
    "otro",
    "otras",
    "otros",
    "mas",
    "se",
    "que",
    "para",
    "por",
    "con",
    "sobre",
    "dicha",
    "dicho",
    "nada",
    "ahi",
    "aqui",
    "any",
    "an",
    "the",
    "such",
    "other",
    "further",
    "additional",
    "for",
    "to",
    "of",
    "in",
    "this",
    "that",
    "these",
    "those",
    "specific",
    "explicit",
    "explicita",
    "explicito",
    "expresa",
    "expreso",
    "especifica",
    "especifico",
    "dato",
    "datos",
    "informacion",
    "mencion",
    "menciones",
    "referencia",
    "referencias",
    "constancia",
    "indicacion",
    "evidencia",
    "coincidencia",
    "coincidencias",
    "resultado",
    "resultados",
    "information",
    "data",
    "mention",
    "reference",
    "references",
    "evidence",
    "record",
    "records",
    "match",
    "matches",
];
/// Tras la locución, estas palabras indican que no se niega la existencia de nada («no hay que»,
/// «no hay nada más»)
const PALABRAS_QUE_ANULAN: &[&str] = &["que", "nada", "nadie", "nothing", "need"];
/// Una palabra más corta no nombra un aspecto con seguridad
const LETRAS_MINIMAS_DEL_ASPECTO: usize = 4;
/// Letras de la raíz con la que se busca el aspecto («ampliación» → «ampli»)
const LETRAS_DE_LA_RAIZ: usize = 5;
/// Palabras tras (o antes de) la locución en las que se busca el aspecto
const VENTANA_DEL_ASPECTO: usize = 4;
/// Con esta cota, una respuesta enorme no multiplica el coste
const MAXIMO_NEGACIONES_REVISADAS: usize = 16;
const PARES_DE_COMILLAS: &[(char, char)] = &[('«', '»'), ('“', '”'), ('"', '"')];
const FINES_DE_FRASE: &[char] = &['.', ';', '!', '?', '\n'];
/// Separa una etiqueta de lo que dice de ella («Nota al pie: no hay ninguna asociada»)
const FIN_DE_ETIQUETA: char = ':';

/// Lo que el turno consultó: textos de los argumentos de las llamadas y de las salidas válidas
pub(super) struct LoConsultado {
    raices: HashSet<String>,
    consulto_alguna_tool: bool,
    /// El turno leyó texto del documento (búsqueda o subárbol): la regla no se aplica
    consulto_texto: bool,
}

impl LoConsultado {
    pub(super) fn nuevo<'a>(
        textos: impl IntoIterator<Item = &'a str>,
        consulto_alguna_tool: bool,
        consulto_texto: bool,
    ) -> Self {
        let raices = textos
            .into_iter()
            .flat_map(|texto| palabras(&plegar_texto(texto)))
            .map(|palabra| raiz(&palabra))
            .collect();
        Self {
            raices,
            consulto_alguna_tool,
            consulto_texto,
        }
    }

    fn contiene(&self, aspecto: &str) -> bool {
        self.raices.contains(&raiz(aspecto))
    }
}

/// Primer aspecto que la respuesta niega sin haberlo buscado en el turno. Un aspecto está buscado
/// si aparece (por su raíz) en lo consultado; si solo aparece en la pregunta, basta con que el
/// turno haya consultado alguna tool: lo pidió el operador y se buscó con otras palabras.
/// Lo consultado solo se indexa si hay alguna negación
pub(super) fn negacion_sin_busqueda(
    respuesta: &str,
    pregunta: &str,
    consultado: impl FnOnce() -> LoConsultado,
) -> Option<String> {
    let aspectos = aspectos_negados(respuesta);
    if aspectos.is_empty() {
        return None;
    }
    let consultado = consultado();
    if consultado.consulto_texto {
        return None;
    }
    let preguntado: HashSet<String> = palabras(&plegar_texto(pregunta))
        .iter()
        .map(|p| raiz(p))
        .collect();
    let buscada = |palabra: &str| {
        consultado.contiene(palabra)
            || (consultado.consulto_alguna_tool && preguntado.contains(&raiz(palabra)))
    };
    aspectos
        .into_iter()
        .find(|negacion| {
            !buscada(&negacion.aspecto) && !negacion.etiqueta.iter().any(|p| buscada(p))
        })
        .map(|negacion| negacion.aspecto)
}

/// Aspecto negado y, si la negación abre lo que sigue a una etiqueta («Nota al pie: no hay
/// ninguna»), las palabras de la etiqueta, que también nombran de qué se habla
struct Negacion {
    aspecto: String,
    etiqueta: Vec<String>,
}

/// Negaciones rotundas de la respuesta, fuera de las comillas
fn aspectos_negados(respuesta: &str) -> Vec<Negacion> {
    let sin_citas = sin_texto_entre_comillas(&plegar_texto(respuesta));
    let mut negaciones = Vec::new();
    for frase in sin_citas.split(FINES_DE_FRASE) {
        let mut etiqueta_anterior: Vec<String> = Vec::new();
        for tramo in frase.split(FIN_DE_ETIQUETA) {
            let palabras = palabras(tramo);
            let mut posicion = 0;
            while posicion < palabras.len() && negaciones.len() < MAXIMO_NEGACIONES_REVISADAS {
                let al_principio = palabras[..posicion]
                    .iter()
                    .all(|p| PALABRAS_SIN_ASPECTO.contains(&p.as_str()));
                let etiqueta = if al_principio {
                    etiqueta_anterior.clone()
                } else {
                    Vec::new()
                };
                let aspecto = if let Some(longitud) =
                    locucion_en(&palabras, posicion, NEGACIONES_EXISTENCIALES)
                {
                    let siguientes = &palabras[posicion + longitud..];
                    posicion += longitud;
                    let anulada = siguientes
                        .first()
                        .is_some_and(|p| PALABRAS_QUE_ANULAN.contains(&p.as_str()));
                    (!anulada)
                        .then(|| primer_aspecto(siguientes.iter()))
                        .flatten()
                } else if let Some(longitud) = locucion_en(&palabras, posicion, NEGACIONES_VERBALES)
                {
                    let anteriores = &palabras[..posicion];
                    posicion += longitud;
                    primer_aspecto(anteriores.iter().rev())
                } else {
                    posicion += 1;
                    continue;
                };
                negaciones.extend(aspecto.map(|aspecto| Negacion { aspecto, etiqueta }));
            }
            etiqueta_anterior = palabras;
        }
    }
    negaciones
}

fn locucion_en(palabras: &[String], posicion: usize, locuciones: &[&[&str]]) -> Option<usize> {
    locuciones
        .iter()
        .find(|locucion| {
            palabras
                .get(posicion..posicion + locucion.len())
                .is_some_and(|tramo| tramo.iter().zip(locucion.iter()).all(|(a, b)| a == b))
        })
        .map(|locucion| locucion.len())
}

fn primer_aspecto<'a>(mut cercanas: impl Iterator<Item = &'a String>) -> Option<String> {
    cercanas
        .by_ref()
        .take(VENTANA_DEL_ASPECTO)
        .find(|palabra| {
            !PALABRAS_SIN_ASPECTO.contains(&palabra.as_str())
                && palabra.chars().count() >= LETRAS_MINIMAS_DEL_ASPECTO
                && palabra.chars().all(char::is_alphabetic)
        })
        .cloned()
}

fn sin_texto_entre_comillas(texto: &str) -> String {
    let mut limpio = String::with_capacity(texto.len());
    let mut cierre_pendiente: Option<char> = None;
    for letra in texto.chars() {
        match cierre_pendiente {
            Some(cierre) if letra == cierre => cierre_pendiente = None,
            Some(_) => {}
            None => match PARES_DE_COMILLAS.iter().find(|(a, _)| *a == letra) {
                Some((_, cierre)) => cierre_pendiente = Some(*cierre),
                None => limpio.push(letra),
            },
        }
    }
    limpio
}

/// Palabras plegadas; un identificador con `_` se parte (`sin_responsable_declarado`)
fn palabras(plegado: &str) -> Vec<String> {
    plegado
        .split(|letra: char| !letra.is_alphanumeric())
        .filter(|palabra| !palabra.is_empty())
        .map(str::to_string)
        .collect()
}

fn raiz(palabra: &str) -> String {
    palabra.chars().take(LETRAS_DE_LA_RAIZ).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consultado(textos: &[&str]) -> LoConsultado {
        LoConsultado::nuevo(textos.iter().copied(), !textos.is_empty(), false)
    }

    fn nada() -> LoConsultado {
        LoConsultado::nuevo(std::iter::empty(), false, false)
    }

    fn con_texto_leido(textos: &[&str]) -> LoConsultado {
        LoConsultado::nuevo(textos.iter().copied(), true, true)
    }

    const PREGUNTA: &str = "¿Cuál es la carga máxima del modelo B?";
    const FILA: &str = r#"{"valor_resultado_texto": "12 t", "rango_x": "modelo = B"}"#;

    #[test]
    fn test_negacion_de_un_aspecto_no_buscado_se_detecta() {
        for (respuesta, aspecto) in [
            (
                "La carga máxima es 12 t. No hay ampliación posible.",
                "ampliacion",
            ),
            (
                "Son 12 t; no existe excepción para el modelo B.",
                "excepcion",
            ),
            ("Son 12 t; no existen prórrogas.", "prorrogas"),
            (
                "Son 12 t. La ampliación no se aplica a este modelo.",
                "ampliacion",
            ),
            (
                "It is 12 t. There is no exception for model B.",
                "exception",
            ),
            ("12 t. No se contempla ninguna prórroga.", "prorroga"),
        ] {
            assert_eq!(
                negacion_sin_busqueda(respuesta, PREGUNTA, || consultado(&[FILA])),
                Some(aspecto.to_string()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_aspecto_presente_en_lo_consultado_no_se_detecta() {
        let con_regla = consultado(&[FILA, "La carga podrá ampliarse un 10 % con autorización."]);
        assert_eq!(
            negacion_sin_busqueda("No hay ampliación sin autorización.", PREGUNTA, || {
                con_regla
            }),
            None
        );
        let buscado = consultado(&[FILA, "excepciones carga modelo B"]);
        assert_eq!(
            negacion_sin_busqueda("No existe excepción.", PREGUNTA, || buscado),
            None
        );
    }

    #[test]
    fn test_no_consta_legitimo_tras_buscar_no_se_detecta() {
        // La búsqueda se hizo (sus argumentos están en lo consultado) y no devolvió nada
        let pregunta = "¿Qué plazo de garantía tiene la bomba P-12?";
        let tras_buscar = || consultado(&[r#"{"consulta": "plazo de garantía bomba P-12"}"#, "[]"]);
        for respuesta in [
            "No consta el plazo de garantía de la bomba P-12.",
            "No hay plazo de garantía declarado para la P-12 en el centro.",
            "No he encontrado ningún plazo de garantía en lo consultado.",
            "No hay datos sobre la garantía de la P-12.",
            "No figura la garantía de esa bomba.",
        ] {
            assert_eq!(
                negacion_sin_busqueda(respuesta, pregunta, tras_buscar),
                None,
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_aspecto_de_la_pregunta_exige_haber_consultado_alguna_tool() {
        let pregunta = "¿Hay alguna excepción al límite de carga?";
        // Buscado con otras palabras: basta con haber consultado
        assert_eq!(
            negacion_sin_busqueda("No hay excepción.", pregunta, || consultado(&[
                "salvedades"
            ])),
            None
        );
        // Sin consultar nada, negarlo es inventar
        assert_eq!(
            negacion_sin_busqueda("No hay excepción.", pregunta, nada),
            Some("excepcion".to_string())
        );
    }

    #[test]
    fn test_si_el_turno_leyo_texto_del_documento_no_se_aplica() {
        assert_eq!(
            negacion_sin_busqueda("Son 12 t. No hay ampliación posible.", PREGUNTA, || {
                con_texto_leido(&[FILA])
            }),
            None
        );
    }

    #[test]
    fn test_etiqueta_consultada_antes_de_dos_puntos_nombra_lo_negado() {
        let nota = || consultado(&[r#"{"nota_pie_condicion": null}"#]);
        assert_eq!(
            negacion_sin_busqueda(
                "Nota al pie: no hay ninguna asociada a esta celda.",
                PREGUNTA,
                nota
            ),
            None
        );
        assert_eq!(
            negacion_sin_busqueda(
                "- Nota al pie (`nota_pie_condicion`): no hay ninguna asociada a esta celda.",
                PREGUNTA,
                || consultado(&[r#"{"nota_pie_condicion": null}"#])
            ),
            None
        );
        // La etiqueta solo vale si está consultada y si la negación abre lo que sigue
        assert_eq!(
            negacion_sin_busqueda("Ampliación: no hay ninguna prevista.", PREGUNTA, || {
                consultado(&[FILA])
            }),
            Some("prevista".to_string())
        );
        assert_eq!(
            negacion_sin_busqueda("Carga máxima: 12 t, no hay ampliación.", PREGUNTA, || {
                consultado(&[FILA])
            }),
            Some("ampliacion".to_string())
        );
    }

    #[test]
    fn test_texto_entre_comillas_es_del_documento() {
        assert_eq!(
            negacion_sin_busqueda(
                "El apartado dice «no hay excepción para cargas fijas» (p. 4).",
                PREGUNTA,
                || consultado(&[FILA])
            ),
            None
        );
    }

    #[test]
    fn test_negaciones_sin_aspecto_o_con_palabras_genericas_no_se_detectan() {
        for respuesta in [
            "No hay datos.",
            "No hay nada más que añadir.",
            "No hay IVA.",
            "No existe.",
            "No hay que hacer nada.",
            "La tabla no recoge ese caso.",
            "El resultado no incluye apartado ni página.",
            "Ese documento no figura entre los resultados.",
        ] {
            assert_eq!(
                negacion_sin_busqueda(respuesta, PREGUNTA, || consultado(&[FILA])),
                None,
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_la_locucion_no_cruza_frases_ni_palabras() {
        assert_eq!(
            negacion_sin_busqueda("Son 12 t. No. Hay ampliación con permiso.", PREGUNTA, nada),
            None
        );
        assert_eq!(
            negacion_sin_busqueda("El nohay ampliación.", PREGUNTA, nada),
            None
        );
    }

    #[test]
    fn test_respuesta_enorme_termina_pronto() {
        let respuesta = "No hay ampliación. ".repeat(200_000);
        let inicio = std::time::Instant::now();
        let _ = negacion_sin_busqueda(&respuesta, PREGUNTA, || consultado(&[FILA]));
        assert!(inicio.elapsed() < std::time::Duration::from_secs(5));
    }
}
