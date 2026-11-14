//! Rasgos del idioma de un documento sin detector de idioma ni listas de palabras: la escritura
//! dominante por bloques Unicode y una muestra literal de texto corrido. El modelo ve la muestra y
//! decide en qué idioma buscar; aquí no se clasifica ningún idioma concreto.

/// Caracteres máximos de la muestra (sin contar la elipsis final)
pub const LONGITUD_MAXIMA_MUESTRA: usize = 150;

/// Escritura de un carácter alfabético según su bloque Unicode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Escritura {
    Latina,
    Griega,
    Cirilica,
    Armenia,
    Hebrea,
    Arabe,
    Devanagari,
    Tailandesa,
    Georgiana,
    Hangul,
    Kana,
    Han,
    Otra,
}

impl Escritura {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Latina => "LATINA",
            Self::Griega => "GRIEGA",
            Self::Cirilica => "CIRILICA",
            Self::Armenia => "ARMENIA",
            Self::Hebrea => "HEBREA",
            Self::Arabe => "ARABE",
            Self::Devanagari => "DEVANAGARI",
            Self::Tailandesa => "TAILANDESA",
            Self::Georgiana => "GEORGIANA",
            Self::Hangul => "HANGUL",
            Self::Kana => "KANA",
            Self::Han => "HAN",
            Self::Otra => "OTRA",
        }
    }

    /// Bloque Unicode del carácter (solo se llama con caracteres alfabéticos)
    fn de(caracter: char) -> Self {
        match u32::from(caracter) {
            0x0000..=0x024F | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F | 0xA720..=0xA7FF => Self::Latina,
            0x0370..=0x03FF | 0x1F00..=0x1FFF => Self::Griega,
            0x0400..=0x052F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Self::Cirilica,
            0x0530..=0x058F => Self::Armenia,
            0x0590..=0x05FF => Self::Hebrea,
            0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFEFF => Self::Arabe,
            0x0900..=0x097F => Self::Devanagari,
            0x0E00..=0x0E7F => Self::Tailandesa,
            0x10A0..=0x10FF => Self::Georgiana,
            0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Self::Hangul,
            0x3040..=0x30FF | 0x31F0..=0x31FF => Self::Kana,
            0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FFFF => Self::Han,
            _ => Self::Otra,
        }
    }
}

/// Caracteres que se miran para la escritura: acota el coste en documentos largos
const CARACTERES_PARA_ESCRITURA: usize = 20_000;
/// Una línea de texto corrido tiene al menos estos caracteres…
const CARACTERES_MINIMOS_LINEA: usize = 60;
/// …y estas palabras
const PALABRAS_MINIMAS_LINEA: usize = 8;
/// Proporción mínima de letras y espacios: deja fuera filas de cifras y códigos
const PROPORCION_MINIMA_LETRAS: f32 = 0.8;
/// Una puntuación repetida tantas veces seguidas es un relleno de índice («.....»)
const RELLENO_DE_INDICE: usize = 4;

/// Escritura con más caracteres alfabéticos; `None` si el texto no tiene ninguno. A igualdad,
/// la que aparece antes
pub fn escritura_dominante(texto: &str) -> Option<Escritura> {
    let mut recuento: Vec<(Escritura, usize)> = Vec::new();
    for escritura in texto
        .chars()
        .take(CARACTERES_PARA_ESCRITURA)
        .filter(|c| c.is_alphabetic())
        .map(Escritura::de)
    {
        match recuento.iter_mut().find(|(e, _)| *e == escritura) {
            Some((_, n)) => *n += 1,
            None => recuento.push((escritura, 1)),
        }
    }
    recuento
        .iter()
        .rev()
        .max_by_key(|(_, n)| *n)
        .map(|(escritura, _)| *escritura)
}

/// Primera línea de texto corrido de los textos dados, acotada a [`LONGITUD_MAXIMA_MUESTRA`]. Una
/// línea que aparece más de una vez es una cabecera o un pie de página repetido: no cuenta
pub fn muestra_de_texto_corrido<'a>(textos: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let lineas: Vec<&str> = textos
        .into_iter()
        .flat_map(str::lines)
        .map(str::trim)
        .collect();
    let mut apariciones: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for linea in &lineas {
        *apariciones.entry(linea).or_default() += 1;
    }
    lineas
        .into_iter()
        .find(|linea| apariciones[linea] == 1 && es_texto_corrido(linea))
        .map(acotar)
}

/// Línea larga, de muchas palabras, casi toda letras, sin relleno de índice ni celdas de tabla
fn es_texto_corrido(linea: &str) -> bool {
    let caracteres = linea.chars().count();
    let letras = linea
        .chars()
        .filter(|c| c.is_alphabetic() || c.is_whitespace())
        .count();
    caracteres >= CARACTERES_MINIMOS_LINEA
        && linea.split_whitespace().count() >= PALABRAS_MINIMAS_LINEA
        && letras as f32 >= PROPORCION_MINIMA_LETRAS * caracteres as f32
        && linea.matches('|').count() < 2
        && !tiene_relleno_de_indice(linea)
}

fn tiene_relleno_de_indice(linea: &str) -> bool {
    let mut anterior = None;
    let mut seguidos = 0;
    for caracter in linea.chars() {
        if caracter.is_ascii_punctuation() && Some(caracter) == anterior {
            seguidos += 1;
            if seguidos + 1 >= RELLENO_DE_INDICE {
                return true;
            }
        } else {
            seguidos = 0;
        }
        anterior = Some(caracter);
    }
    false
}

/// Prefijo literal de como mucho [`LONGITUD_MAXIMA_MUESTRA`] caracteres, cortado en un espacio
fn acotar(linea: &str) -> String {
    if linea.chars().count() <= LONGITUD_MAXIMA_MUESTRA {
        return linea.to_string();
    }
    let corte = linea
        .char_indices()
        .nth(LONGITUD_MAXIMA_MUESTRA)
        .map_or(linea.len(), |(posicion, _)| posicion);
    let prefijo = &linea[..corte];
    let hasta_palabra = prefijo
        .rfind(char::is_whitespace)
        .map_or(prefijo, |espacio| &prefijo[..espacio]);
    format!("{}…", hasta_palabra.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARRAFO_EN: &str = "The contractor shall document the hazard analysis results and keep them available for the program office during the whole life cycle.";
    const PARRAFO_ES: &str = "El responsable de la obra registrará los resultados del ensayo y los conservará a disposición de la dirección facultativa durante la ejecución.";

    #[test]
    fn test_escritura_por_bloques_unicode() {
        assert_eq!(escritura_dominante(PARRAFO_EN), Some(Escritura::Latina));
        assert_eq!(escritura_dominante(PARRAFO_ES), Some(Escritura::Latina));
        assert_eq!(
            escritura_dominante("Η δοκιμή πίεσης διαρκεί τριάντα λεπτά"),
            Some(Escritura::Griega)
        );
        assert_eq!(
            escritura_dominante("Испытательное давление составляет 12 бар"),
            Some(Escritura::Cirilica)
        );
        assert_eq!(
            escritura_dominante("试验压力为十二巴"),
            Some(Escritura::Han)
        );
        assert_eq!(escritura_dominante("ضغط الاختبار"), Some(Escritura::Arabe));
        assert_eq!(
            escritura_dominante("परीक्षण दबाव"),
            Some(Escritura::Devanagari)
        );
        assert_eq!(escritura_dominante("시험 압력"), Some(Escritura::Hangul));
        assert_eq!(escritura_dominante("12 345 — 6,7 %"), None);
        assert_eq!(Escritura::Cirilica.as_str(), "CIRILICA");
    }

    #[test]
    fn test_escritura_mayoritaria_en_texto_mixto() {
        let mixto = format!(
            "ISO 9001 {}",
            "Испытательное давление составляет двенадцать бар"
        );
        assert_eq!(escritura_dominante(&mixto), Some(Escritura::Cirilica));
    }

    #[test]
    fn test_muestra_salta_titulos_indices_y_tablas() {
        let indice = "1.2 Scope ........................................................ 5";
        let tabla =
            "| Presión de prueba | Tiempo de mantenimiento | Temperatura del agua | Espesor |";
        let titulo = "4. HAZARD ANALYSIS AND RISK ASSESSMENT";
        let texto = format!("{titulo}\n{indice}\n{tabla}\n{PARRAFO_EN}");
        assert!(PARRAFO_EN.chars().count() <= LONGITUD_MAXIMA_MUESTRA);
        assert_eq!(
            muestra_de_texto_corrido([texto.as_str()]).as_deref(),
            Some(PARRAFO_EN)
        );
    }

    #[test]
    fn test_muestra_recorre_los_textos_en_orden() {
        let muestra = muestra_de_texto_corrido(["Contenido", PARRAFO_ES, PARRAFO_EN]).unwrap();
        assert!(muestra.starts_with("El responsable de la obra"));
    }

    #[test]
    fn test_muestra_acotada_en_limite_de_palabra_y_de_caracter() {
        let largo = format!("{PARRAFO_ES} {PARRAFO_ES}");
        let muestra = muestra_de_texto_corrido([largo.as_str()]).unwrap();
        assert!(muestra.ends_with('…'));
        let sin_elipsis = muestra.trim_end_matches('…');
        assert!(sin_elipsis.chars().count() <= LONGITUD_MAXIMA_MUESTRA);
        assert!(largo.starts_with(sin_elipsis), "es un prefijo literal");
        assert!(!sin_elipsis.ends_with(' '));
        let griego = "Η δοκιμή πίεσης διαρκεί τριάντα λεπτά και καταγράφεται στο ημερολόγιο του έργου από τον υπεύθυνο. ".repeat(3);
        let muestra = muestra_de_texto_corrido([griego.as_str()]).unwrap();
        assert!(muestra.trim_end_matches('…').chars().count() <= LONGITUD_MAXIMA_MUESTRA);
    }

    #[test]
    fn test_muestra_salta_lineas_repetidas_como_cabeceras_y_pies() {
        let sello = "Printed copy of the plant handbook, kept for reference only and not controlled by the quality office.";
        let pagina_1 = format!("{sello}\nSafety\n");
        let pagina_2 = format!("{sello}\n{PARRAFO_EN}\n");
        assert_eq!(
            muestra_de_texto_corrido([pagina_1.as_str(), pagina_2.as_str()]).as_deref(),
            Some(PARRAFO_EN)
        );
    }

    #[test]
    fn test_sin_texto_corrido_no_hay_muestra() {
        assert_eq!(muestra_de_texto_corrido(["Índice", "1. Objeto", ""]), None);
        assert_eq!(muestra_de_texto_corrido(Vec::<&str>::new()), None);
    }
}
