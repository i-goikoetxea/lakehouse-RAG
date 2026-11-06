//! Fragmentación determinista de secciones para el corpus 02 y estimación de tokens.
//!
//! **Estimación (`estimar_tokens`):** cada carácter ideográfico o de puntuación cuenta 1 token y
//! cada tramo alfanumérico cuenta 1 token por cada 4 caracteres (redondeando hacia arriba). No se
//! usa el tokenizer de E5: exigiría cargar el modelo (≈ 2,2 GB) en una capa pura, y el resultado
//! dependería del modelo activo. La estimación solo decide dónde cortar; quien vectoriza comprueba
//! el truncado real con `EmbeddingEnginePort::contar_truncados`.

/// Una sección de hasta este número de tokens es un único chunk. Coincide con la ventana: E5
/// trunca a 512 tokens reales y un chunk único no tiene solape que cubra su cola
pub const UMBRAL_CHUNK_UNICO: usize = TOKENS_VENTANA;
/// Tamaño máximo en tokens de cada ventana de una sección mayor
pub const TOKENS_VENTANA: usize = 512;
/// Tokens que comparten dos ventanas consecutivas
pub const TOKENS_SOLAPE: usize = 64;

/// Caracteres alfanuméricos que componen un token estimado
const CARACTERES_POR_TOKEN: usize = 4;
/// Primer punto de código tratado como ideográfico (CJK y siguientes): 1 token por carácter
const INICIO_IDEOGRAFICO: char = '\u{2E80}';

/// Tramo de bytes del texto que equivale a un token estimado
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PiezaToken {
    inicio: usize,
    fin: usize,
}

/// Piezas del texto; los espacios no cuentan y quedan entre dos piezas
fn piezas_de_token(texto: &str) -> Vec<PiezaToken> {
    let mut piezas = Vec::with_capacity(texto.len() / CARACTERES_POR_TOKEN + 1);
    let mut abierta: Option<(usize, usize)> = None;
    for (posicion, caracter) in texto.char_indices() {
        let fin = posicion + caracter.len_utf8();
        if caracter.is_alphanumeric() && caracter < INICIO_IDEOGRAFICO {
            let (inicio, caracteres) = abierta.unwrap_or((posicion, 0));
            if caracteres + 1 == CARACTERES_POR_TOKEN {
                piezas.push(PiezaToken { inicio, fin });
                abierta = None;
            } else {
                abierta = Some((inicio, caracteres + 1));
            }
            continue;
        }
        if let Some((inicio, _)) = abierta.take() {
            piezas.push(PiezaToken {
                inicio,
                fin: posicion,
            });
        }
        if !caracter.is_whitespace() {
            piezas.push(PiezaToken {
                inicio: posicion,
                fin,
            });
        }
    }
    if let Some((inicio, _)) = abierta {
        piezas.push(PiezaToken {
            inicio,
            fin: texto.len(),
        });
    }
    piezas
}

/// Tokens estimados de un texto (ver la cabecera del módulo)
pub fn estimar_tokens(texto: &str) -> usize {
    piezas_de_token(texto).len()
}

/// Divide el texto de una sección sin perder nada salvo espacios entre fragmentos.
/// Hasta `UMBRAL_CHUNK_UNICO` tokens devuelve un fragmento; por encima, ventanas de
/// `TOKENS_VENTANA` con `TOKENS_SOLAPE` de solapamiento. Un texto en blanco no da fragmentos.
pub fn fragmentar_seccion(texto: &str) -> Vec<String> {
    fragmentar_con_posiciones(texto)
        .into_iter()
        .map(|fragmento| fragmento.texto)
        .collect()
}

/// Pieza (exclusiva) donde termina la ventana: prefiere párrafo, luego frase y luego palabra, en
/// la segunda mitad de la ventana. Sin ninguno (palabra gigante), corta en el límite exacto.
fn fin_de_ventana(texto: &str, piezas: &[PiezaToken], inicio: usize) -> usize {
    let limite = inicio + TOKENS_VENTANA;
    if limite >= piezas.len() {
        return piezas.len();
    }
    let minimo = inicio + TOKENS_VENTANA / 2;
    let criterios: [fn(&str, &str) -> bool; 3] = [
        |_, hueco| hueco.matches('\n').count() >= 2,
        |previo, hueco| !hueco.is_empty() && previo.ends_with(['.', '!', '?', ':', ';']),
        |_, hueco| !hueco.is_empty(),
    ];
    for criterio in criterios {
        let corte = (minimo..=limite).rev().find(|&fin| {
            let previo = &texto[piezas[fin - 1].inicio..piezas[fin - 1].fin];
            criterio(previo, hueco_antes(texto, piezas, fin))
        });
        if let Some(corte) = corte {
            return corte;
        }
    }
    limite
}

/// Texto (solo espacios) entre la pieza `indice - 1` y la pieza `indice`
fn hueco_antes<'a>(texto: &'a str, piezas: &[PiezaToken], indice: usize) -> &'a str {
    &texto[piezas[indice - 1].fin..piezas[indice].inicio]
}

/// La siguiente ventana retrocede `TOKENS_SOLAPE` y avanza hasta un inicio de palabra para no
/// empezar en mitad de una; una palabra gigante conserva el solape exacto.
fn inicio_con_solape(texto: &str, piezas: &[PiezaToken], fin: usize) -> usize {
    let inicio = fin - TOKENS_SOLAPE;
    (inicio..fin)
        .find(|&i| i > 0 && !hueco_antes(texto, piezas, i).is_empty())
        .unwrap_or(inicio)
}

/// Fragmento con su posición en bytes dentro del texto de la sección
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentoSeccion {
    pub texto: String,
    pub inicio: usize,
    pub fin: usize,
}

/// `fragmentar_seccion` con la posición de cada fragmento, para situarlo en sus páginas
pub fn fragmentar_con_posiciones(texto: &str) -> Vec<FragmentoSeccion> {
    let base = texto.len() - texto.trim_start().len();
    let texto = texto.trim();
    let fragmento = |inicio: usize, fin: usize| FragmentoSeccion {
        texto: texto[inicio..fin].to_string(),
        inicio: base + inicio,
        fin: base + fin,
    };
    let piezas = piezas_de_token(texto);
    if piezas.is_empty() {
        return if texto.is_empty() {
            Vec::new()
        } else {
            vec![fragmento(0, texto.len())]
        };
    }
    if piezas.len() <= UMBRAL_CHUNK_UNICO {
        return vec![fragmento(0, texto.len())];
    }

    let mut fragmentos = Vec::new();
    let mut inicio = 0;
    loop {
        let fin = fin_de_ventana(texto, &piezas, inicio);
        fragmentos.push(fragmento(piezas[inicio].inicio, piezas[fin - 1].fin));
        if fin == piezas.len() {
            return fragmentos;
        }
        inicio = inicio_con_solape(texto, &piezas, fin);
    }
}

/// Reconstruye el texto de una sección a partir de sus ventanas consecutivas: quita de cada una
/// el prefijo que repite el final de la anterior. Solo se admite un solape de hasta
/// `TOKENS_SOLAPE` tokens, el máximo que genera `fragmentar_seccion`; así un texto repetitivo no
/// pierde contenido real. Sin solape, las une con un salto de párrafo.
pub fn reconstruir_sin_solape<S: AsRef<str>>(ventanas: &[S]) -> String {
    let mut texto = String::new();
    for ventana in ventanas {
        let ventana = ventana.as_ref();
        if texto.is_empty() {
            texto.push_str(ventana);
            continue;
        }
        let solape = longitud_de_solape(&texto, ventana);
        if solape == 0 {
            texto.push_str("\n\n");
        }
        texto.push_str(&ventana[solape..]);
    }
    texto
}

/// Bytes del prefijo más largo de `siguiente` que termina `previo`, dentro del solape admitido
fn longitud_de_solape(previo: &str, siguiente: &str) -> usize {
    siguiente
        .char_indices()
        .map(|(posicion, _)| posicion)
        .skip(1)
        .chain(std::iter::once(siguiente.len()))
        .take_while(|&fin| estimar_tokens(&siguiente[..fin]) <= TOKENS_SOLAPE)
        .filter(|&fin| previo.ends_with(&siguiente[..fin]))
        .last()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests_posiciones {
    use super::*;

    #[test]
    fn test_posiciones_recuperan_cada_fragmento_del_texto() {
        let texto = format!(
            "  
{}",
            "palabra ñandú. ".repeat(500)
        );
        let fragmentos = fragmentar_con_posiciones(&texto);
        assert!(fragmentos.len() > 1);
        for fragmento in &fragmentos {
            assert_eq!(&texto[fragmento.inicio..fragmento.fin], fragmento.texto);
        }
        assert_eq!(
            fragmentar_seccion(&texto),
            fragmentos.into_iter().map(|f| f.texto).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconstruir_recupera_el_texto_de_las_ventanas_reales() {
        let original = palabras(2000);
        let ventanas = fragmentar_seccion(&original);
        assert!(ventanas.len() > 2);
        assert_eq!(reconstruir_sin_solape(&ventanas), original);
    }

    #[test]
    fn test_reconstruir_no_pierde_contenido_en_texto_repetitivo() {
        let original = "dato repetido. ".repeat(1200);
        let original = original.trim();
        let ventanas = fragmentar_seccion(original);
        assert!(ventanas.len() > 1);
        assert_eq!(
            reconstruir_sin_solape(&ventanas).matches("dato").count(),
            1200,
            "el solape repetitivo no elimina apariciones reales"
        );
    }

    #[test]
    fn test_reconstruir_sin_solape_une_con_parrafo_y_admite_vacio() {
        assert_eq!(reconstruir_sin_solape::<&str>(&[]), "");
        assert_eq!(reconstruir_sin_solape(&["uno"]), "uno");
        assert_eq!(reconstruir_sin_solape(&["uno", "dos"]), "uno\n\ndos");
        assert_eq!(reconstruir_sin_solape(&["año ñu", "ñu más"]), "año ñu más");
    }

    fn palabras(total: usize) -> String {
        (0..total)
            .map(|i| format!("p{:03}", i % 1000))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn test_estimacion_determinista_por_tramos_y_puntuacion() {
        assert_eq!(estimar_tokens(""), 0);
        assert_eq!(estimar_tokens("   \n "), 0);
        assert_eq!(estimar_tokens("abcd"), 1);
        assert_eq!(estimar_tokens("abcde"), 2);
        assert_eq!(estimar_tokens("a, b."), 4);
        assert_eq!(estimar_tokens("ñandú"), 2);
        assert_eq!(estimar_tokens("日本語"), 3);
    }

    #[test]
    fn test_frontera_511_y_512_tokens_son_un_chunk_y_513_se_divide() {
        for total in [511, 512] {
            let seccion = palabras(total);
            assert_eq!(estimar_tokens(&seccion), total);
            assert_eq!(fragmentar_seccion(&seccion), vec![seccion.clone()]);
        }

        let fragmentos = fragmentar_seccion(&palabras(513));
        assert_eq!(fragmentos.len(), 2);
        assert!(
            fragmentos
                .iter()
                .all(|f| estimar_tokens(f) <= TOKENS_VENTANA)
        );
    }

    #[test]
    fn test_ningun_fragmento_supera_la_ventana_del_modelo() {
        for total in [400, 512, 513, 580, 600, 601, 700, 1500] {
            let fragmentos = fragmentar_seccion(&palabras(total));
            assert!(
                fragmentos
                    .iter()
                    .all(|f| estimar_tokens(f) <= TOKENS_VENTANA),
                "{total} tokens"
            );
        }
    }

    #[test]
    fn test_justo_sobre_el_umbral_el_solape_cubre_la_cola_sin_perdidas() {
        for total in [513, 514, 576, 577, 600, 1024, 1025] {
            let original = palabras(total);
            let ventanas = fragmentar_seccion(&original);
            assert!(ventanas.len() >= 2, "{total} tokens");
            assert!(original.ends_with(ventanas.last().unwrap().as_str()));
            for par in ventanas.windows(2) {
                let previas: Vec<&str> = par[0].split_whitespace().collect();
                let siguientes: Vec<&str> = par[1].split_whitespace().collect();
                assert_eq!(previas.last(), siguientes.get(TOKENS_SOLAPE - 1), "{total}");
            }
            assert_eq!(
                reconstruir_sin_solape(&ventanas),
                original,
                "{total} tokens"
            );
        }
    }

    #[test]
    fn test_ventanas_solapan_64_tokens_y_cubren_todas_las_palabras() {
        let texto = palabras(2000);
        let fragmentos = fragmentar_seccion(&texto);
        assert!(fragmentos.len() >= 4);
        for par in fragmentos.windows(2) {
            let previas: Vec<&str> = par[0].split_whitespace().collect();
            let siguientes: Vec<&str> = par[1].split_whitespace().collect();
            assert_eq!(previas.last(), siguientes.get(TOKENS_SOLAPE - 1));
        }
        let union = fragmentos.join(" ");
        for palabra in texto.split_whitespace() {
            assert!(union.contains(palabra), "{palabra}");
        }
    }

    #[test]
    fn test_corta_por_parrafo_y_si_no_por_frase() {
        let parrafo = format!("{}\n\n{}", palabras(400), palabras(400));
        let fragmentos = fragmentar_seccion(&parrafo);
        assert_eq!(fragmentos[0], palabras(400), "corta en el salto de párrafo");

        let frases = format!("{}. {}", palabras(400), palabras(400));
        let fragmentos = fragmentar_seccion(&frases);
        assert!(fragmentos[0].ends_with('.'), "corta tras el punto");
    }

    #[test]
    fn test_parrafo_gigante_sin_espacios_no_pierde_caracteres_ni_rompe_utf8() {
        let gigante = "ñ".repeat(10_000);
        let fragmentos = fragmentar_seccion(&gigante);
        assert!(fragmentos.len() > 1);
        assert!(
            fragmentos
                .iter()
                .all(|f| estimar_tokens(f) <= TOKENS_VENTANA)
        );
        let sin_solape: usize = fragmentos[0].chars().count()
            + fragmentos[1..]
                .iter()
                .map(|f| f.chars().count() - TOKENS_SOLAPE * CARACTERES_POR_TOKEN)
                .sum::<usize>();
        assert_eq!(sin_solape, 10_000);
    }

    #[test]
    fn test_seccion_vacia_y_solo_simbolos() {
        assert!(fragmentar_seccion("  \n\t ").is_empty());
        assert_eq!(fragmentar_seccion(" --- "), vec!["---"]);
    }

    #[test]
    fn test_unicode_mixto_es_determinista_y_cubre_todo() {
        let texto = "日本語のテキスト 🚀 Sección ñandú — ".repeat(300);
        let primero = fragmentar_seccion(&texto);
        assert_eq!(primero, fragmentar_seccion(&texto));
        assert!(primero.len() > 1);
        assert!(primero.iter().any(|f| f.contains('🚀')));
    }
}
