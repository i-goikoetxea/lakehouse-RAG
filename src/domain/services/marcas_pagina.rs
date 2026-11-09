//! Saltos de página dentro de un texto y marcas `⟦p. N⟧` que los hacen visibles al modelo
//! (`docs/arquitectura/ssd_citas_pagina.md` §1 y §2).
//!
//! Las marcas solo existen en la salida de las tools: nunca se guardan en `texto_contenido`, en
//! el vector ni en BM25. Antes de insertarlas, los corchetes `⟦ ⟧` del documento se neutralizan
//! a `[ ]`, así que en un texto marcado toda secuencia `⟦p. N⟧` es una marca de la app.

use serde::{Deserialize, Serialize};

/// Apertura de una marca de página (U+27E6)
pub const APERTURA_MARCA: char = '⟦';
/// Cierre de una marca de página (U+27E7)
pub const CIERRE_MARCA: char = '⟧';
/// Prefijo entre la apertura y el número
const PREFIJO_MARCA: &str = "p. ";
/// Máximo de dígitos de la página de una marca: una página mayor no es una marca
const MAX_DIGITOS_PAGINA: usize = 6;

/// Punto de un texto donde empieza una página
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaltoPagina {
    /// Bytes UTF-8 desde el inicio del texto al que pertenece
    pub desplazamiento: usize,
    /// Página física del fichero, base 1
    pub pagina: u32,
}

/// Marca visible de una página: `⟦p. 53⟧`
pub fn marca_de_pagina(pagina: u32) -> String {
    format!("{APERTURA_MARCA}{PREFIJO_MARCA}{pagina}{CIERRE_MARCA}")
}

/// Página del byte `posicion`: la del último salto que empieza en ella o antes. Antes del primer
/// salto vale la del primero (texto previo de la misma página).
pub fn pagina_en(saltos: &[SaltoPagina], posicion: usize) -> Option<u32> {
    let siguientes = saltos.partition_point(|salto| salto.desplazamiento <= posicion);
    saltos.get(siguientes.saturating_sub(1)).map(|s| s.pagina)
}

/// Saltos del tramo `inicio..fin` de un texto con `saltos`, relativos a `inicio`: el primero en
/// 0 con la página de `inicio` y después uno por cada página nueva dentro del tramo.
pub fn saltos_del_tramo(saltos: &[SaltoPagina], inicio: usize, fin: usize) -> Vec<SaltoPagina> {
    let Some(primera) = pagina_en(saltos, inicio) else {
        return Vec::new();
    };
    let mut tramo = vec![SaltoPagina {
        desplazamiento: 0,
        pagina: primera,
    }];
    for salto in saltos
        .iter()
        .filter(|s| s.desplazamiento > inicio && s.desplazamiento < fin)
    {
        empujar_si_cambia(
            &mut tramo,
            SaltoPagina {
                desplazamiento: salto.desplazamiento - inicio,
                pagina: salto.pagina,
            },
        );
    }
    tramo
}

/// Añade el salto solo si cambia de página respecto al anterior
pub fn empujar_si_cambia(saltos: &mut Vec<SaltoPagina>, salto: SaltoPagina) {
    if saltos
        .last()
        .is_none_or(|ultimo| ultimo.pagina != salto.pagina)
    {
        saltos.push(salto);
    }
}

/// Saltos utilizables para marcar `texto`: el primero en 0, desplazamientos estrictamente
/// crecientes dentro del texto y en frontera de carácter, y páginas mayores que 0
pub fn saltos_validos(texto: &str, saltos: &[SaltoPagina]) -> bool {
    saltos
        .first()
        .is_some_and(|primero| primero.desplazamiento == 0)
        && saltos.iter().all(|s| {
            s.pagina > 0
                && s.desplazamiento < texto.len().max(1)
                && texto.is_char_boundary(s.desplazamiento)
        })
        && saltos
            .windows(2)
            .all(|par| par[0].desplazamiento < par[1].desplazamiento)
}

/// `⟦` y `⟧` del documento pasan a `[` y `]`: así no se confunden con una marca
pub fn neutralizar_corchetes(texto: &str) -> String {
    texto
        .replace(APERTURA_MARCA, "[")
        .replace(CIERRE_MARCA, "]")
}

/// Texto para la salida de una tool: corchetes neutralizados y, si los saltos son válidos, una
/// marca al inicio y otra antes de cada salto, seguidas de un espacio. Con saltos inválidos o
/// sin saltos no se marca nada: mejor sin marca que con una falsa.
pub fn texto_con_marcas(texto: &str, saltos: &[SaltoPagina]) -> String {
    if !saltos_validos(texto, saltos) {
        return neutralizar_corchetes(texto);
    }
    let mut marcado = String::with_capacity(texto.len() + saltos.len() * 12);
    for (indice, salto) in saltos.iter().enumerate() {
        let fin = saltos
            .get(indice + 1)
            .map_or(texto.len(), |siguiente| siguiente.desplazamiento);
        marcado.push_str(&marca_de_pagina(salto.pagina));
        marcado.push(' ');
        marcado.push_str(&neutralizar_corchetes(&texto[salto.desplazamiento..fin]));
    }
    marcado
}

/// Tramo de un texto marcado y la página de la marca que lo precede (`None` antes de la primera)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TramoPaginado<'a> {
    pub pagina: Option<u32>,
    pub texto: &'a str,
}

/// Parte un texto marcado en tramos sin marcas. El espacio que sigue a cada marca no es del
/// documento y se descarta.
pub fn tramos_paginados(texto: &str) -> Vec<TramoPaginado<'_>> {
    let mut tramos = Vec::new();
    let mut pagina = None;
    let mut resto = texto;
    while let Some((antes, pagina_marca, despues)) = siguiente_marca(resto) {
        if !antes.is_empty() {
            tramos.push(TramoPaginado {
                pagina,
                texto: antes,
            });
        }
        pagina = Some(pagina_marca);
        resto = despues.strip_prefix(' ').unwrap_or(despues);
    }
    if !resto.is_empty() {
        tramos.push(TramoPaginado {
            pagina,
            texto: resto,
        });
    }
    tramos
}

/// Texto marcado sin sus marcas
pub fn quitar_marcas(texto: &str) -> String {
    tramos_paginados(texto)
        .into_iter()
        .map(|tramo| tramo.texto)
        .collect()
}

/// `true` si el texto contiene alguna marca bien formada
pub fn tiene_marcas(texto: &str) -> bool {
    siguiente_marca(texto).is_some()
}

/// Primera marca bien formada: texto previo, página y texto posterior
fn siguiente_marca(texto: &str) -> Option<(&str, u32, &str)> {
    let mut desde = 0;
    while let Some(relativa) = texto[desde..].find(APERTURA_MARCA) {
        let apertura = desde + relativa;
        let tras_apertura = apertura + APERTURA_MARCA.len_utf8();
        if let Some((pagina, longitud)) = leer_marca(&texto[tras_apertura..]) {
            return Some((
                &texto[..apertura],
                pagina,
                &texto[tras_apertura + longitud..],
            ));
        }
        desde = tras_apertura;
    }
    None
}

/// `p. N⟧` al inicio: la página y los bytes que ocupa
fn leer_marca(texto: &str) -> Option<(u32, usize)> {
    let numero = texto.strip_prefix(PREFIJO_MARCA)?;
    let digitos = numero.chars().take_while(char::is_ascii_digit).count();
    if digitos == 0 || digitos > MAX_DIGITOS_PAGINA || numero[..digitos].starts_with('0') {
        return None;
    }
    numero[digitos..].strip_prefix(CIERRE_MARCA)?;
    let pagina = numero[..digitos].parse().ok()?;
    Some((
        pagina,
        PREFIJO_MARCA.len() + digitos + CIERRE_MARCA.len_utf8(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salto(desplazamiento: usize, pagina: u32) -> SaltoPagina {
        SaltoPagina {
            desplazamiento,
            pagina,
        }
    }

    #[test]
    fn test_marca_inicio_y_cada_salto_con_espacio() {
        let texto = "fin de la 52\ninicio de la 53";
        let marcado = texto_con_marcas(texto, &[salto(0, 52), salto(13, 53)]);
        assert_eq!(marcado, "⟦p. 52⟧ fin de la 52\n⟦p. 53⟧ inicio de la 53");
    }

    #[test]
    fn test_sin_saltos_o_invalidos_no_marca_pero_neutraliza() {
        let texto = "dato ⟦p. 99⟧ falso";
        assert_eq!(texto_con_marcas(texto, &[]), "dato [p. 99] falso");
        for invalidos in [
            vec![salto(3, 1)],
            vec![salto(0, 0)],
            vec![salto(0, 1), salto(0, 2)],
            vec![salto(0, 1), salto(500, 2)],
            vec![salto(0, 1), salto(6, 2)],
        ] {
            assert_eq!(
                texto_con_marcas(texto, &invalidos),
                "dato [p. 99] falso",
                "{invalidos:?}"
            );
        }
    }

    #[test]
    fn test_marcas_falsas_del_documento_no_son_marcas() {
        let marcado = texto_con_marcas("ver ⟦p. 99⟧ y [p. 98]", &[salto(0, 4)]);
        let tramos = tramos_paginados(&marcado);
        assert_eq!(tramos.len(), 1);
        assert_eq!(tramos[0].pagina, Some(4));
        assert_eq!(tramos[0].texto, "ver [p. 99] y [p. 98]");
    }

    #[test]
    fn test_tramos_y_quitar_marcas_recuperan_el_original() {
        let texto = "uno\ndos\ntres";
        let saltos = [salto(0, 7), salto(4, 8), salto(8, 9)];
        let marcado = texto_con_marcas(texto, &saltos);
        assert_eq!(quitar_marcas(&marcado), texto);
        let paginas: Vec<_> = tramos_paginados(&marcado)
            .iter()
            .map(|t| (t.pagina, t.texto))
            .collect();
        assert_eq!(
            paginas,
            [(Some(7), "uno\n"), (Some(8), "dos\n"), (Some(9), "tres")]
        );
    }

    #[test]
    fn test_marca_mal_formada_es_texto() {
        for texto in [
            "⟦p. ⟧",
            "⟦p. 012⟧",
            "⟦p.12⟧",
            "⟦p. 1234567⟧",
            "⟦p. 12",
            "⟦⟦p. 3⟧",
        ] {
            let tramos = tramos_paginados(texto);
            let paginas: Vec<_> = tramos.iter().map(|t| t.pagina).collect();
            if texto == "⟦⟦p. 3⟧" {
                assert_eq!(paginas, [None], "{texto}");
                assert_eq!(tramos[0].texto, "⟦");
            } else {
                assert_eq!(paginas, [None], "{texto}");
                assert!(!tiene_marcas(texto), "{texto}");
            }
        }
    }

    #[test]
    fn test_saltos_del_tramo_rebasan_y_empiezan_en_cero() {
        let saltos = [salto(0, 10), salto(100, 11), salto(200, 12)];
        assert_eq!(
            saltos_del_tramo(&saltos, 150, 260),
            [salto(0, 11), salto(50, 12)]
        );
        assert_eq!(saltos_del_tramo(&saltos, 0, 100), [salto(0, 10)]);
        assert_eq!(
            saltos_del_tramo(&saltos, 100, 201),
            [salto(0, 11), salto(100, 12)]
        );
        assert!(saltos_del_tramo(&[], 0, 10).is_empty());
    }

    #[test]
    fn test_pagina_antes_del_primer_salto_es_la_del_primero() {
        let saltos = [salto(5, 3), salto(9, 4)];
        assert_eq!(pagina_en(&saltos, 0), Some(3));
        assert_eq!(pagina_en(&saltos, 9), Some(4));
        assert_eq!(pagina_en(&[], 0), None);
    }

    #[test]
    fn test_multibyte_en_la_frontera() {
        let texto = "añoé";
        assert!(saltos_validos(texto, &[salto(0, 1), salto(3, 2)]));
        assert!(!saltos_validos(texto, &[salto(0, 1), salto(2, 2)]));
        assert_eq!(
            texto_con_marcas(texto, &[salto(0, 1), salto(3, 2)]),
            "⟦p. 1⟧ añ⟦p. 2⟧ oé"
        );
    }
}
