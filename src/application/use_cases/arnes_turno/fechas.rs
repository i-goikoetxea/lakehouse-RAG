//! Reconocimiento de fechas en Rust estándar: una fecha es un dato entero, no tres cifras sueltas.

use super::texto_normalizado::plegar_texto;

/// Índice del mes (1 a 12) por posición; `setiembre` es variante admitida de septiembre.
const MESES: &[&[&str]] = &[
    &["enero"],
    &["febrero"],
    &["marzo"],
    &["abril"],
    &["mayo"],
    &["junio"],
    &["julio"],
    &["agosto"],
    &["septiembre", "setiembre"],
    &["octubre"],
    &["noviembre"],
    &["diciembre"],
];
const DIGITOS_DEL_ANO: usize = 4;
const DIGITOS_MAXIMOS_DEL_DIA: usize = 2;
const DIA_MAXIMO: u32 = 31;
const LONGITUD_MAXIMA_DE_MES: usize = 10;

/// Fecha hallada en un texto: dónde acaba y su forma canónica `AAAA-MM-DD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FechaHallada {
    pub fin: usize,
    pub iso: String,
}

/// Número de mes de una palabra ya plegada.
pub(super) fn mes_de(plegada: &str) -> Option<u32> {
    MESES
        .iter()
        .position(|variantes| variantes.contains(&plegada))
        .and_then(|indice| u32::try_from(indice + 1).ok())
}

/// `2026-11-03` que empieza en `inicio`. Quien llama comprueba que no va tras un identificador.
pub(super) fn fecha_iso_en(letras: &[char], inicio: usize) -> Option<FechaHallada> {
    let fin_ano = inicio + DIGITOS_DEL_ANO;
    let ano = digitos_exactos(letras, inicio, fin_ano)?;
    let mes = digitos_exactos(letras, fin_ano + 1, fin_ano + 3)?;
    let dia = digitos_exactos(letras, fin_ano + 4, fin_ano + 6)?;
    let separada_con_guiones = letras.get(fin_ano).copied().is_some_and(es_guion_de_fecha)
        && letras
            .get(fin_ano + 3)
            .copied()
            .is_some_and(es_guion_de_fecha);
    let fin = fin_ano + 6;
    let termina = !letras.get(fin).is_some_and(char::is_ascii_digit);
    (separada_con_guiones && termina)
        .then(|| iso_valida(ano, mes, dia).map(|iso| FechaHallada { fin, iso }))
        .flatten()
}

/// `11 de septiembre de 2025` (o `del 2025`) que empieza en `inicio`, con el día en cifras.
pub(super) fn fecha_textual_en(letras: &[char], inicio: usize) -> Option<FechaHallada> {
    let fin_dia = fin_de_digitos(letras, inicio);
    if fin_dia - inicio > DIGITOS_MAXIMOS_DEL_DIA {
        return None;
    }
    let dia = numero(&letras[inicio..fin_dia])?;
    let (enlace, tras_enlace) = palabra_tras_espacios(letras, fin_dia)?;
    let (mes, tras_mes) = palabra_tras_espacios(letras, tras_enlace)?;
    let (enlace_ano, tras_enlace_ano) = palabra_tras_espacios(letras, tras_mes)?;
    let inicio_ano = saltar_espacios(letras, tras_enlace_ano);
    let fin = inicio_ano + DIGITOS_DEL_ANO;
    let ano = digitos_exactos(letras, inicio_ano, fin)?;
    let enlaces_validos = enlace == "de" && matches!(enlace_ano.as_str(), "de" | "del");
    let termina = !letras
        .get(fin)
        .is_some_and(|l| l.is_alphanumeric() || *l == '_');
    if !enlaces_validos || !termina {
        return None;
    }
    let mes = mes_de(&mes)?;
    iso_valida(ano, mes, dia).map(|iso| FechaHallada { fin, iso })
}

fn es_guion_de_fecha(letra: char) -> bool {
    matches!(letra, '-' | '‐' | '‑' | '–' | '−')
}

/// Una palabra suelta que es una fecha en cifras: `2025-06-16`, `16/06/2025` o `16-06-2025`.
pub(super) fn es_fecha_en_cifras(palabra: &str) -> bool {
    let letras: Vec<char> = palabra.chars().collect();
    fecha_iso_en(&letras, 0).is_some_and(|fecha| fecha.fin == letras.len())
        || es_fecha_dia_mes_ano(palabra)
}

fn es_fecha_dia_mes_ano(palabra: &str) -> bool {
    let partes: Vec<&str> = palabra.split(['/', '-']).collect();
    let [dia, mes, ano] = partes.as_slice() else {
        return false;
    };
    let solo_digitos = |parte: &str, maximo: usize| {
        !parte.is_empty() && parte.len() <= maximo && parte.chars().all(|l| l.is_ascii_digit())
    };
    solo_digitos(dia, DIGITOS_MAXIMOS_DEL_DIA)
        && solo_digitos(mes, 2)
        && ano.len() == DIGITOS_DEL_ANO
        && solo_digitos(ano, DIGITOS_DEL_ANO)
        && iso_valida(
            ano.parse().unwrap_or_default(),
            mes.parse().unwrap_or_default(),
            dia.parse().unwrap_or_default(),
        )
        .is_some()
}

fn iso_valida(ano: u32, mes: u32, dia: u32) -> Option<String> {
    ((1..=12).contains(&mes) && (1..=DIA_MAXIMO).contains(&dia))
        .then(|| format!("{ano:04}-{mes:02}-{dia:02}"))
}

fn digitos_exactos(letras: &[char], inicio: usize, fin: usize) -> Option<u32> {
    let tramo = letras.get(inicio..fin)?;
    tramo
        .iter()
        .all(char::is_ascii_digit)
        .then(|| numero(tramo))
        .flatten()
}

fn numero(digitos: &[char]) -> Option<u32> {
    digitos.iter().collect::<String>().parse().ok()
}

fn fin_de_digitos(letras: &[char], inicio: usize) -> usize {
    letras[inicio..]
        .iter()
        .position(|l| !l.is_ascii_digit())
        .map_or(letras.len(), |desplazamiento| inicio + desplazamiento)
}

fn saltar_espacios(letras: &[char], inicio: usize) -> usize {
    letras[inicio.min(letras.len())..]
        .iter()
        .position(|l| !l.is_whitespace())
        .map_or(letras.len(), |desplazamiento| inicio + desplazamiento)
}

/// La palabra plegada que sigue a uno o más espacios, y dónde acaba. Sin espacio no hay palabra.
fn palabra_tras_espacios(letras: &[char], inicio: usize) -> Option<(String, usize)> {
    let inicio_palabra = saltar_espacios(letras, inicio);
    if inicio_palabra == inicio {
        return None;
    }
    let longitud = letras[inicio_palabra..]
        .iter()
        .take(LONGITUD_MAXIMA_DE_MES + 1)
        .take_while(|l| l.is_alphabetic())
        .count();
    if longitud == 0 || longitud > LONGITUD_MAXIMA_DE_MES {
        return None;
    }
    let fin = inicio_palabra + longitud;
    let palabra: String = letras[inicio_palabra..fin].iter().collect();
    Some((plegar_texto(&palabra), fin))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iso(texto: &str) -> Option<String> {
        let letras: Vec<char> = texto.chars().collect();
        fecha_iso_en(&letras, 0).map(|fecha| fecha.iso)
    }

    fn textual(texto: &str) -> Option<String> {
        let letras: Vec<char> = texto.chars().collect();
        fecha_textual_en(&letras, 0).map(|fecha| fecha.iso)
    }

    #[test]
    fn test_fecha_iso_valida_y_sus_fronteras() {
        assert_eq!(iso("2026-11-03"), Some("2026-11-03".to_string()));
        assert_eq!(iso("2026-11-03T10:00"), Some("2026-11-03".to_string()));
        assert_eq!(iso("2026‑11–03"), Some("2026-11-03".to_string()));
        for texto in [
            "2026-13-03",
            "2026-11-00",
            "2026-11-032",
            "2026/11/03",
            "26-11-03",
            "2026-1-03",
        ] {
            assert_eq!(iso(texto), None, "{texto}");
        }
    }

    #[test]
    fn test_fecha_textual_con_mes_en_cualquier_caja() {
        assert_eq!(
            textual("11 de SEPTIEMBRE de 2025"),
            Some("2025-09-11".to_string())
        );
        assert_eq!(
            textual("1 de junio del 2026"),
            Some("2026-06-01".to_string())
        );
        for texto in [
            "11 de espesor de 2025",
            "11 de septiembre",
            "11 de septiembre de 20255",
            "111 de junio de 2025",
            "0 de junio de 2025",
            "11 septiembre 2025",
            "11 de septiembre de 2025mm",
        ] {
            assert_eq!(textual(texto), None, "{texto}");
        }
    }

    #[test]
    fn test_palabra_que_es_fecha_en_cifras() {
        for palabra in ["2025-06-16", "16/06/2025", "1/6/2025", "16-06-2025"] {
            assert!(es_fecha_en_cifras(palabra), "{palabra}");
        }
        for palabra in [
            "2025-06-16x",
            "30/12",
            "16/13/2025",
            "0,42",
            "4-A",
            "16/06/25",
        ] {
            assert!(!es_fecha_en_cifras(palabra), "{palabra}");
        }
    }

    #[test]
    fn test_textos_extremos_no_rompen() {
        assert_eq!(textual("1"), None);
        assert_eq!(textual("1 de"), None);
        assert_eq!(iso("2026-"), None);
        let largo = format!("1 de {} de 2025", "a".repeat(100_000));
        assert_eq!(textual(&largo), None);
    }
}
