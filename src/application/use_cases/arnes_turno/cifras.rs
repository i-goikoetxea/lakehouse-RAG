//! Extracción y normalización de cifras en Rust estándar, sin `regex`.

use super::fechas::{FechaHallada, fecha_iso_en, fecha_textual_en};

/// Cifra hallada en un texto, con sus lecturas canónicas posibles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CifraHallada {
    pub literal: String,
    pub lecturas: Vec<String>,
    /// Inicio de la cifra en el texto, en caracteres
    pub posicion: usize,
}

/// Unidades que pueden ir pegadas a una cifra: `30mm` es una cifra y `3D` no.
pub(super) const UNIDADES: &[&str] = &[
    "mm", "cm", "m", "km", "in", "pulg", "kg", "g", "t", "n", "kn", "pa", "kpa", "mpa", "gpa",
    "bar", "c", "k", "f", "a", "v", "kv", "w", "kw", "j", "kj", "nm", "s", "min", "h", "hrc", "hb",
    "hv", "l", "ºc", "ºf",
];

const PALABRAS_SIN_CIFRA: &[&str] = &["capa", "capas"];
/// Un número mayor al inicio de línea es un valor, no una numeración
const DIGITOS_MAXIMOS_DE_MARCADOR: usize = 2;
const DECORACION_DE_MARCADOR: &[char] = &['#', '*', '_', '>', '+', '-', '(', '`'];
/// Con más tramos de dígitos, un token es un hash o un id: sus dígitos no son datos
const TRAMOS_MAXIMOS_DE_TOKEN: usize = 2;
const DIGITOS_POR_GRUPO_DE_MILES: usize = 3;

/// Las cifras de un texto, sin identificadores (`4-A`, `S355`, `5.2.1`) ni marcadores de lista.
/// Compara por texto canónico: una cifra de cualquier longitud no desborda.
pub(super) fn extraer_cifras(texto: &str) -> Vec<CifraHallada> {
    let letras: Vec<char> = texto.chars().collect();
    let mut cifras = Vec::new();
    let mut posicion = 0;
    while posicion < letras.len() {
        if !letras[posicion].is_ascii_digit() {
            posicion += 1;
            continue;
        }
        if let Some(fecha) = fecha_que_empieza_en(&letras, posicion) {
            cifras.push(CifraHallada {
                literal: letras[posicion..fecha.fin].iter().collect(),
                lecturas: vec![fecha.iso],
                posicion,
            });
            posicion = fecha.fin;
            continue;
        }
        let fin_numero = fin_de_numero(&letras, posicion);
        let fin_sufijo = fin_de_sufijo(&letras, fin_numero);
        if es_cifra(&letras, posicion, fin_numero, fin_sufijo) {
            let cuerpo = cuerpo_de_numero(&letras[posicion..fin_numero]);
            if let Some(cifra) = interpretar(&cuerpo, tiene_signo(&letras, posicion), posicion) {
                cifras.push(cifra);
            }
        }
        posicion = fin_sufijo;
    }
    cifras
}

/// Una fecha se cita entera: sus partes sueltas no son cifras de la respuesta.
fn fecha_que_empieza_en(letras: &[char], inicio: usize) -> Option<FechaHallada> {
    if va_tras_identificador(letras, inicio) {
        return None;
    }
    fecha_iso_en(letras, inicio).or_else(|| fecha_textual_en(letras, inicio))
}

/// Lo que una pregunta o una salida de tool permiten citar. Es más amplio que `extraer_cifras`: el
/// dato de `125ºC` o `Rev02` debe anclar la respuesta `125 °C` o `Rev. 2`, y una fecha ancla
/// tanto la fecha entera como sus partes.
pub(super) fn lecturas_de_fuente(texto: &str) -> Vec<String> {
    let letras: Vec<char> = texto.chars().collect();
    let mut lecturas = Vec::new();
    let mut posicion = 0;
    while posicion < letras.len() {
        if !letras[posicion].is_ascii_digit() {
            posicion += 1;
            continue;
        }
        // La fecha entera ancla la respuesta que la cite, y sus partes, la que cite el año o el día.
        if let Some(fecha) =
            fecha_iso_en(&letras, posicion).or_else(|| fecha_textual_en(&letras, posicion))
        {
            lecturas.push(fecha.iso);
        }
        let fin_token = fin_de_token(&letras, posicion);
        if es_token_opaco(&letras[posicion..fin_token]) {
            posicion = fin_token;
            continue;
        }
        let fin_numero = fin_de_numero(&letras, posicion);
        let cuerpo = cuerpo_de_numero(&letras[posicion..fin_numero]);
        let magnitudes =
            lecturas_de_magnitud(&cuerpo).unwrap_or_else(|| prefijos_de_numeracion(&cuerpo));
        lecturas.extend(con_signo(magnitudes, tiene_signo(&letras, posicion)));
        posicion = fin_numero;
    }
    lecturas
}

fn fin_de_token(letras: &[char], inicio: usize) -> usize {
    letras[inicio..]
        .iter()
        .position(|l| !es_parte_de_identificador(*l))
        .map_or(letras.len(), |desplazamiento| inicio + desplazamiento)
}

fn es_token_opaco(token: &[char]) -> bool {
    let tramos = token
        .iter()
        .enumerate()
        .filter(|(i, l)| {
            l.is_ascii_digit() && i.checked_sub(1).is_none_or(|a| !token[a].is_ascii_digit())
        })
        .count();
    tramos > TRAMOS_MAXIMOS_DE_TOKEN
}

/// De `5.2.1` se puede citar el apartado padre, `5` o `5.2`, pero no un `2` suelto.
fn prefijos_de_numeracion(cuerpo: &str) -> Vec<String> {
    let mut grupos = cuerpo.split(['.', ',']);
    let primero = grupos.next().unwrap_or_default();
    let segundo = grupos.next().unwrap_or_default();
    vec![canonica(primero, ""), canonica(primero, segundo)]
}

fn es_parte_de_identificador(letra: char) -> bool {
    letra.is_alphanumeric() || letra == '_'
}

fn es_guion(letra: char) -> bool {
    matches!(letra, '-' | '−' | '‐' | '‑' | '–')
}

fn es_espacio_duro(letra: char) -> bool {
    matches!(letra, '\u{00A0}' | '\u{202F}' | '\u{2009}')
}

fn separa_miles_con_espacio_duro(letras: &[char], posicion: usize) -> bool {
    let inicio_grupo = posicion + 1;
    let fin_grupo = inicio_grupo + DIGITOS_POR_GRUPO_DE_MILES;
    es_espacio_duro(letras[posicion])
        && letras
            .get(inicio_grupo..fin_grupo)
            .is_some_and(|grupo| grupo.iter().all(char::is_ascii_digit))
        && !letras.get(fin_grupo).is_some_and(char::is_ascii_digit)
}

fn fin_de_numero(letras: &[char], inicio: usize) -> usize {
    let mut fin = inicio;
    while fin < letras.len() {
        let separador_decimal = matches!(letras[fin], '.' | ',')
            && letras.get(fin + 1).is_some_and(char::is_ascii_digit);
        if !letras[fin].is_ascii_digit()
            && !separador_decimal
            && !separa_miles_con_espacio_duro(letras, fin)
        {
            break;
        }
        fin += 1;
    }
    fin
}

fn cuerpo_de_numero(letras: &[char]) -> String {
    letras.iter().filter(|l| !es_espacio_duro(**l)).collect()
}

fn fin_de_sufijo(letras: &[char], inicio: usize) -> usize {
    let mut fin = inicio;
    while fin < letras.len()
        && es_parte_de_identificador(letras[fin])
        && !letras[fin].is_ascii_digit()
    {
        fin += 1;
    }
    fin
}

fn es_cifra(letras: &[char], inicio: usize, fin_numero: usize, fin_sufijo: usize) -> bool {
    !va_tras_identificador(letras, inicio)
        && sufijo_admitido(letras, fin_numero, fin_sufijo)
        && !es_marcador_de_lista(letras, inicio, fin_numero)
        && !va_tras_palabra_sin_cifra(letras, inicio)
}

fn va_tras_identificador(letras: &[char], inicio: usize) -> bool {
    let Some(anterior) = inicio.checked_sub(1).map(|i| letras[i]) else {
        return false;
    };
    if es_parte_de_identificador(anterior) {
        return true;
    }
    es_guion(anterior)
        && inicio
            .checked_sub(2)
            .is_some_and(|i| es_parte_de_identificador(letras[i]) && !letras[i].is_ascii_digit())
}

fn sufijo_admitido(letras: &[char], fin_numero: usize, fin_sufijo: usize) -> bool {
    if fin_sufijo > fin_numero {
        let sufijo: String = letras[fin_numero..fin_sufijo]
            .iter()
            .flat_map(|l| l.to_lowercase())
            .collect();
        return UNIDADES.contains(&sufijo.as_str());
    }
    let sigue_identificador = letras.get(fin_numero).copied().is_some_and(es_guion)
        && letras
            .get(fin_numero + 1)
            .is_some_and(|l| es_parte_de_identificador(*l) && !l.is_ascii_digit());
    !sigue_identificador
}

fn es_marcador_de_lista(letras: &[char], inicio: usize, fin_numero: usize) -> bool {
    let numero = &letras[inicio..fin_numero];
    let longitud_de_marcador =
        numero.len() <= DIGITOS_MAXIMOS_DE_MARCADOR && numero.iter().all(char::is_ascii_digit);
    let cierra_marcador = matches!(letras.get(fin_numero), Some('.' | ')'))
        && letras
            .get(fin_numero + 1)
            .is_none_or(|l| l.is_whitespace() || matches!(l, '*' | '_'));
    let pegado_a_guion = inicio.checked_sub(1).is_some_and(|i| es_guion(letras[i]));
    longitud_de_marcador
        && cierra_marcador
        && !pegado_a_guion
        && letras[..inicio]
            .iter()
            .rev()
            .take_while(|l| **l != '\n')
            .all(|l| l.is_whitespace() || DECORACION_DE_MARCADOR.contains(l))
}

fn va_tras_palabra_sin_cifra(letras: &[char], inicio: usize) -> bool {
    let palabra_inversa: String = letras[..inicio]
        .iter()
        .rev()
        .skip_while(|l| l.is_whitespace())
        .take_while(|l| l.is_alphabetic())
        .take(6)
        .flat_map(|l| l.to_lowercase())
        .collect();
    let palabra: String = palabra_inversa.chars().rev().collect();
    PALABRAS_SIN_CIFRA.contains(&palabra.as_str())
}

fn tiene_signo(letras: &[char], inicio: usize) -> bool {
    inicio.checked_sub(1).is_some_and(|i| es_guion(letras[i]))
        && inicio
            .checked_sub(2)
            .is_none_or(|i| !es_parte_de_identificador(letras[i]))
}

fn interpretar(cuerpo: &str, negativa: bool, posicion: usize) -> Option<CifraHallada> {
    let signo = if negativa { "-" } else { "" };
    Some(CifraHallada {
        literal: format!("{signo}{cuerpo}"),
        lecturas: con_signo(lecturas_de_magnitud(cuerpo)?, negativa),
        posicion,
    })
}

fn con_signo(lecturas: Vec<String>, negativa: bool) -> Vec<String> {
    lecturas
        .into_iter()
        .map(|lectura| {
            if negativa && lectura != "0" {
                format!("-{lectura}")
            } else {
                lectura
            }
        })
        .collect()
}

/// `None` si no es una cifra sino una numeración, como `5.2.1`.
fn lecturas_de_magnitud(cuerpo: &str) -> Option<Vec<String>> {
    let grupos: Vec<&str> = cuerpo.split(['.', ',']).collect();
    let separadores: Vec<char> = cuerpo.chars().filter(|l| matches!(l, '.' | ',')).collect();
    let forma_miles =
        |grupos: &[&str]| grupos[0].len() <= 3 && grupos[1..].iter().all(|g| g.len() == 3);
    match separadores.as_slice() {
        [] => Some(vec![canonica(cuerpo, "")]),
        [_] => {
            let mut lecturas = vec![canonica(grupos[0], grupos[1])];
            if forma_miles(&grupos) && grupos[0].chars().any(|d| d != '0') {
                lecturas.push(canonica(&grupos.concat(), ""));
            }
            Some(lecturas)
        }
        [iniciales @ .., ultimo] if iniciales.iter().all(|s| s == ultimo) => {
            forma_miles(&grupos).then(|| vec![canonica(&grupos.concat(), "")])
        }
        [iniciales @ .., ultimo] => {
            let (decimales, enteros) = grupos.split_last()?;
            let miles_uniformes = iniciales.iter().all(|s| *s == iniciales[0] && s != ultimo);
            (miles_uniformes && forma_miles(enteros))
                .then(|| vec![canonica(&enteros.concat(), decimales)])
        }
    }
}

fn canonica(entero: &str, decimales: &str) -> String {
    let entero = entero.trim_start_matches('0');
    let entero = if entero.is_empty() { "0" } else { entero };
    let decimales = decimales.trim_end_matches('0');
    if decimales.is_empty() {
        entero.to_string()
    } else {
        format!("{entero}.{decimales}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literales(texto: &str) -> Vec<String> {
        extraer_cifras(texto)
            .into_iter()
            .map(|c| c.literal)
            .collect()
    }

    fn lecturas(texto: &str) -> Vec<String> {
        extraer_cifras(texto)
            .into_iter()
            .next()
            .expect("debe haber una cifra")
            .lecturas
    }

    #[test]
    fn test_coma_y_punto_decimal_tienen_la_misma_lectura() {
        assert_eq!(lecturas("0,42"), lecturas("0.42"));
    }

    #[test]
    fn test_ceros_sobrantes_no_cambian_la_lectura() {
        assert_eq!(lecturas("0,40"), lecturas("0.4"));
        assert_eq!(lecturas("030"), lecturas("30.0"));
    }

    #[test]
    fn test_separador_de_miles_equivale_al_entero() {
        assert!(lecturas("1.000").contains(&"1000".to_string()));
        assert!(lecturas("1.234,5").contains(&"1234.5".to_string()));
        assert!(lecturas("1,234.5").contains(&"1234.5".to_string()));
        assert!(lecturas("1.000.000").contains(&"1000000".to_string()));
    }

    #[test]
    fn test_el_signo_forma_parte_de_la_cifra() {
        assert_eq!(lecturas("-10"), vec!["-10".to_string()]);
        assert_eq!(lecturas("−10"), lecturas("-10"));
        assert_ne!(lecturas("-10"), lecturas("10"));
    }

    #[test]
    fn test_rango_con_guion_son_dos_cifras_positivas() {
        assert_eq!(literales("20-30 mm"), vec!["20", "30"]);
        assert_eq!(lecturas("20-30"), vec!["20".to_string()]);
    }

    #[test]
    fn test_unidad_pegada_sigue_siendo_cifra() {
        assert_eq!(literales("30mm y 0,42%"), vec!["30", "0,42"]);
    }

    #[test]
    fn test_identificadores_no_son_cifras() {
        for texto in [
            "S355", "4-A", "ISO-9606", "M12", "5.2.1", "1º", "3D", "A-10",
        ] {
            assert!(literales(texto).is_empty(), "{texto} no es una cifra");
        }
    }

    #[test]
    fn test_marcadores_de_lista_y_capas_no_son_cifras() {
        let texto = "1. Primero\n  2) Segundo\nDesde la Capa 0 y la capa 2 sale 7";
        assert_eq!(literales(texto), vec!["7"]);
    }

    #[test]
    fn test_numero_inicial_sin_marcador_de_lista_es_cifra() {
        assert_eq!(literales("30 mm"), vec!["30"]);
        assert_eq!(literales("1.5 mm"), vec!["1.5"]);
    }

    #[test]
    fn test_ceros_a_la_izquierda_no_cambian_la_lectura() {
        assert_eq!(lecturas("01"), lecturas("1"));
    }

    #[test]
    fn test_fecha_iso_es_una_sola_cifra() {
        assert_eq!(literales("2024-01-15"), vec!["2024-01-15"]);
        assert_eq!(lecturas("2024-01-15"), vec!["2024-01-15".to_string()]);
        assert_eq!(literales("| Hoy (2026-11-03) |"), vec!["2026-11-03"]);
        assert_eq!(literales("2026-11-03T10 y 7"), vec!["2026-11-03", "7"]);
    }

    #[test]
    fn test_fecha_textual_es_una_sola_cifra_con_lectura_iso() {
        for (texto, iso) in [
            ("el 11 de septiembre de 2025", "2025-09-11"),
            ("Hoy, 3 de Noviembre de 2026,", "2026-11-03"),
            ("el 1 de junio del 2026", "2026-06-01"),
            ("el 7 de setiembre de 2024", "2024-09-07"),
        ] {
            let cifras = extraer_cifras(texto);
            assert_eq!(cifras.len(), 1, "{texto}: {cifras:?}");
            assert_eq!(cifras[0].lecturas, vec![iso.to_string()], "{texto}");
        }
    }

    #[test]
    fn test_lo_que_no_es_fecha_valida_se_trocea_como_antes() {
        assert_eq!(literales("2024-13-45").len(), 3);
        assert_eq!(literales("12024-01-15").len(), 3);
        assert_eq!(literales("2024-01-155").len(), 3);
        assert_eq!(literales("32 de enero de 2024"), vec!["32", "2024"]);
        assert_eq!(literales("11 de espesor de 2025"), vec!["11", "2025"]);
    }

    #[test]
    fn test_cifra_gigante_no_desborda() {
        let gigante = "9".repeat(100_000);
        assert_eq!(extraer_cifras(&gigante).len(), 1);
    }

    #[test]
    fn test_numeraciones_con_formato_markdown_no_son_cifras() {
        for texto in [
            "**1.** Paso",
            "**2)** Paso",
            "### 3. Resultado",
            "- 4. Paso",
            "> 5. Paso",
            "(6) Paso",
        ] {
            assert!(literales(texto).is_empty(), "{texto}");
        }
    }

    #[test]
    fn test_numero_largo_o_pegado_a_un_guion_al_inicio_de_linea_es_cifra() {
        assert_eq!(literales("999. Es el valor"), vec!["999"]);
        assert_eq!(literales("- 125. Es el valor"), vec!["125"]);
        assert_eq!(literales("-30. Es el valor"), vec!["-30"]);
    }

    #[test]
    fn test_guiones_tipograficos_no_rompen_un_identificador() {
        for texto in ["Tabla 4‑A", "Tabla 4–A", "Tabla 4‐A"] {
            assert!(literales(texto).is_empty(), "{texto}");
        }
        assert_eq!(literales("20–30 mm"), vec!["20", "30"]);
    }

    #[test]
    fn test_grados_con_ordinal_pegado_son_cifra_y_el_ordinal_solo_no() {
        assert_eq!(literales("999ºC"), vec!["999"]);
        assert!(literales("1º").is_empty());
    }

    #[test]
    fn test_espacio_duro_separa_miles_y_el_espacio_normal_no() {
        for texto in ["1\u{00A0}000", "1\u{202F}000", "1\u{2009}000"] {
            assert!(lecturas(texto).contains(&"1000".to_string()), "{texto:?}");
        }
        assert_eq!(literales("1 000").len(), 2);
    }

    fn anclas(texto: &str) -> Vec<String> {
        lecturas_de_fuente(texto)
    }

    #[test]
    fn test_una_fuente_ancla_las_cifras_con_sufijo_desconocido() {
        for (fuente, lectura) in [
            ("125ºC", "125"),
            ("7días", "7"),
            ("30mm²", "30"),
            ("10²", "10"),
        ] {
            assert!(
                anclas(fuente).contains(&lectura.to_string()),
                "{fuente}: {:?}",
                anclas(fuente)
            );
        }
    }

    #[test]
    fn test_una_fuente_ancla_el_numero_de_un_identificador() {
        for (fuente, lectura) in [
            ("Rev02", "2"),
            ("REV-2", "2"),
            ("R-02", "2"),
            ("4-A", "4"),
            ("ZX-9931", "9931"),
        ] {
            assert!(
                anclas(fuente).contains(&lectura.to_string()),
                "{fuente}: {:?}",
                anclas(fuente)
            );
        }
    }

    #[test]
    fn test_una_fuente_ancla_los_prefijos_de_una_numeracion() {
        let numeracion = anclas("5.2.1");
        for lectura in ["5", "5.2"] {
            assert!(numeracion.contains(&lectura.to_string()), "{numeracion:?}");
        }
        assert!(!numeracion.contains(&"2".to_string()), "{numeracion:?}");
    }

    #[test]
    fn test_una_fuente_conserva_el_signo_y_no_lo_inventa() {
        assert_eq!(anclas("-10"), vec!["-10".to_string()]);
        assert!(!anclas("REV-10").contains(&"-10".to_string()));
    }

    #[test]
    fn test_una_fuente_ancla_la_fecha_iso_entera_y_sus_partes() {
        let fecha = anclas("vigor: 2026-06-01");
        for lectura in ["2026-06-01", "2026", "6", "1"] {
            assert!(fecha.contains(&lectura.to_string()), "{fecha:?}");
        }
    }

    #[test]
    fn test_una_fuente_ancla_la_fecha_textual_como_iso_y_sus_partes() {
        let fecha = anclas("¿qué temperatura el 11 de septiembre de 2025?");
        for lectura in ["2025-09-11", "11", "2025"] {
            assert!(fecha.contains(&lectura.to_string()), "{fecha:?}");
        }
    }

    #[test]
    fn test_un_token_opaco_no_ancla_sus_digitos() {
        assert!(anclas("8f3a12").is_empty(), "{:?}", anclas("8f3a12"));
        assert!(anclas("c0a1b2d3").is_empty());
    }

    #[test]
    fn test_fuente_enorme_de_token_opaco_termina() {
        let opaco = "a1".repeat(500_000);
        assert!(anclas(&opaco).is_empty());
    }

    #[test]
    fn test_texto_sin_cifras_y_multibyte_no_rompen() {
        assert!(extraer_cifras("").is_empty());
        assert!(extraer_cifras("ñandú 🦀 ¿qué?").is_empty());
        assert_eq!(literales("🦀30🦀"), vec!["30"]);
    }
}
