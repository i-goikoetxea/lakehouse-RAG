//! Árbol ToC de un documento: identidad, padre y linaje de cada sección, y su resumen extractivo.
//!
//! Solo depende del nivel y del código de cada sección, así que sirve para cualquier extractor.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::domain::models::document::DocId;
use crate::domain::models::estructura_documental::{NivelJerarquico, NodoId, TipoBloque};
use crate::domain::services::normalizacion::normalizar_token;

/// Código del preámbulo (`decisiones_stair.md`)
pub const CODIGO_PREAMBULO: &str = "0.0";
/// Título del preámbulo
pub const TITULO_PREAMBULO: &str = "Preámbulo / Introducción";
/// Ruta del preámbulo
const RUTA_PREAMBULO: &str = "/0";
/// Longitud máxima del resumen, en caracteres
pub const MAX_CARACTERES_RESUMEN: usize = 250;
/// Frases del cuerpo que entran en el resumen
const FRASES_RESUMEN: usize = 2;

/// Sección de entrada del árbol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeccionDelArbol<'a> {
    /// 0 = preámbulo, 1 a 6 = H1 a H6
    pub nivel: NivelJerarquico,
    pub codigo: &'a str,
    /// Sub-nodo técnico: cuelga siempre de la sección real anterior
    pub sintetica: bool,
}

/// Nodo resuelto: identidad única, padre y linaje únicos dentro del documento
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodoDelArbol {
    pub nodo_id: NodoId,
    pub nodo_padre_id: Option<NodoId>,
    pub ruta_linaje: String,
}

/// Construye el árbol en orden de documento. Un salto de nivel cuelga del ancestro abierto más
/// cercano; un código repetido o inutilizable recurre al índice de la sección, de modo que ni el
/// `nodo_id` ni la ruta colisionan.
pub fn construir_arbol(doc_id: &DocId, secciones: &[SeccionDelArbol<'_>]) -> Vec<NodoDelArbol> {
    let mut usados = HashSet::with_capacity(secciones.len());
    let mut abiertos: Vec<(NivelJerarquico, NodoId, String)> = Vec::new();
    let mut nodos: Vec<NodoDelArbol> = Vec::with_capacity(secciones.len());
    let mut ultima_real: Option<usize> = None;

    for (indice, seccion) in secciones.iter().enumerate() {
        if seccion.sintetica
            && let Some(padre) = ultima_real.map(|posicion| &nodos[posicion])
        {
            let nodo_id = nodo_sin_colision(doc_id, seccion.codigo, indice, &mut usados);
            let ruta_linaje = format!("{}/{}", padre.ruta_linaje, nodo_id.segmento());
            let nodo_padre_id = Some(padre.nodo_id.clone());
            nodos.push(NodoDelArbol {
                nodo_id,
                nodo_padre_id,
                ruta_linaje,
            });
            continue;
        }
        ultima_real = Some(nodos.len());
        if seccion.nivel == NivelJerarquico::PREAMBULO {
            nodos.push(NodoDelArbol {
                nodo_id: NodoId::preambulo(doc_id),
                nodo_padre_id: None,
                ruta_linaje: RUTA_PREAMBULO.to_string(),
            });
            continue;
        }
        let nodo_id = nodo_sin_colision(doc_id, seccion.codigo, indice, &mut usados);
        while abiertos.last().is_some_and(|(n, _, _)| *n >= seccion.nivel) {
            abiertos.pop();
        }
        let (nodo_padre_id, ruta_padre) = abiertos
            .last()
            .map_or((None, ""), |(_, nodo, ruta)| (Some(nodo.clone()), ruta));
        let ruta_linaje = format!("{ruta_padre}/{}", nodo_id.segmento());
        abiertos.push((seccion.nivel, nodo_id.clone(), ruta_linaje.clone()));
        nodos.push(NodoDelArbol {
            nodo_id,
            nodo_padre_id,
            ruta_linaje,
        });
    }
    nodos
}

fn nodo_sin_colision(
    doc_id: &DocId,
    codigo: &str,
    indice: usize,
    usados: &mut HashSet<NodoId>,
) -> NodoId {
    let nodo = NodoId::de_seccion(doc_id, codigo, indice);
    if usados.insert(nodo.clone()) {
        return nodo;
    }
    let por_indice = NodoId::de_indice(doc_id, indice);
    usados.insert(por_indice.clone());
    por_indice
}

/// Orden de documento de dos rutas de linaje, segmento a segmento y con los números como números:
/// `/2/2.2` va antes que `/2/2.10`, y `/IX` después de `/V`.
pub fn comparar_rutas_linaje(a: &str, b: &str) -> Ordering {
    let segmentos = |ruta: &'_ str| -> Vec<Vec<TramoOrden>> {
        ruta.split('/')
            .filter(|s| !s.is_empty())
            .map(tramos_de_orden)
            .collect()
    };
    segmentos(a).cmp(&segmentos(b)).then_with(|| a.cmp(b))
}

/// Orden natural de dos identificadores: `c-2` antes que `c-10`
pub fn comparar_natural(a: &str, b: &str) -> Ordering {
    tramos_de_orden(a)
        .cmp(&tramos_de_orden(b))
        .then_with(|| a.cmp(b))
}

/// Parte comparable de un identificador. Los números van antes que el texto.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum TramoOrden {
    Numero(u64),
    Texto(String),
}

fn tramos_de_orden(segmento: &str) -> Vec<TramoOrden> {
    if let Some(valor) = valor_romano(segmento) {
        return vec![TramoOrden::Numero(valor)];
    }
    let mut tramos = Vec::new();
    let mut actual = String::new();
    let mut en_digitos = false;
    let cerrar = |actual: &mut String, en_digitos: bool, tramos: &mut Vec<TramoOrden>| {
        if actual.is_empty() {
            return;
        }
        tramos.push(if en_digitos {
            TramoOrden::Numero(actual.parse().unwrap_or(u64::MAX))
        } else {
            TramoOrden::Texto(actual.clone())
        });
        actual.clear();
    };
    for caracter in segmento.chars() {
        if caracter.is_ascii_digit() != en_digitos {
            cerrar(&mut actual, en_digitos, &mut tramos);
            en_digitos = caracter.is_ascii_digit();
        }
        actual.push(caracter);
    }
    cerrar(&mut actual, en_digitos, &mut tramos);
    tramos
}

/// Valor de un segmento que solo tiene `I`, `V` y `X`. `C`, `D` o `L` no se interpretan: en un
/// documento suelen ser letras de anexo.
fn valor_romano(segmento: &str) -> Option<u64> {
    if segmento.is_empty() || !segmento.chars().all(|c| matches!(c, 'I' | 'V' | 'X')) {
        return None;
    }
    let valor_de = |c: char| match c {
        'I' => 1,
        'V' => 5,
        _ => 10,
    };
    let valores: Vec<u64> = segmento.chars().map(valor_de).collect();
    let total = valores.iter().enumerate().fold(0i64, |suma, (i, valor)| {
        let resta = valores
            .get(i + 1)
            .is_some_and(|siguiente| siguiente > valor);
        suma + if resta {
            -(*valor as i64)
        } else {
            *valor as i64
        }
    });
    u64::try_from(total).ok()
}

/// Resumen extractivo y determinista: título y dos primeras frases, hasta 250 caracteres
pub fn resumen_extractivo(titulo: &str, contenido: &str) -> String {
    let cuerpo = contenido.split_whitespace().collect::<Vec<_>>().join(" ");
    let frases = primeras_frases(&cuerpo, FRASES_RESUMEN);
    let resumen = match (titulo.trim(), frases) {
        (titulo, "") => titulo.to_string(),
        ("", frases) => frases.to_string(),
        (titulo, frases) => format!("{titulo}. {frases}"),
    };
    resumen.chars().take(MAX_CARACTERES_RESUMEN).collect()
}

/// Prefijo con las `total` primeras frases. Una frase acaba en `.`, `!` o `?` seguido de espacio,
/// por lo que `3.5` o `QA.QC` no la cortan.
fn primeras_frases(texto: &str, total: usize) -> &str {
    let mut vistas = 0;
    let mut caracteres = texto.char_indices().peekable();
    while let Some((posicion, caracter)) = caracteres.next() {
        let es_cierre = matches!(caracter, '.' | '!' | '?');
        if es_cierre && caracteres.peek().is_none_or(|(_, c)| c.is_whitespace()) {
            vistas += 1;
            if vistas == total {
                return &texto[..=posicion];
            }
        }
    }
    texto
}

/// Tipo de bloque según el título de la sección; ante la duda, `NARRATIVA`
pub fn clasificar_bloque(nivel: NivelJerarquico, titulo: &str) -> TipoBloque {
    if nivel == NivelJerarquico::PREAMBULO {
        return TipoBloque::Preambulo;
    }
    let token = normalizar_token(titulo);
    let contiene = |palabras: &[&str]| token.split('_').any(|t| palabras.contains(&t));
    if contiene(&["ALCANCE"]) {
        TipoBloque::Alcance
    } else if contiene(&["DEFINICION", "DEFINICIONES", "TERMINOLOGIA", "GLOSARIO"]) {
        TipoBloque::Definicion
    } else if contiene(&["PROCEDIMIENTO", "PROCEDIMIENTOS"]) {
        TipoBloque::Procedimiento
    } else {
        TipoBloque::Narrativa
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orden_de_rutas_natural_y_con_romanos() {
        let mut rutas = vec!["/2/2.10", "/10", "/0", "/2", "/2/2.2", "/2/2.2/2.2.1", "/9"];
        rutas.sort_by(|a, b| comparar_rutas_linaje(a, b));
        assert_eq!(
            rutas,
            ["/0", "/2", "/2/2.2", "/2/2.2/2.2.1", "/2/2.10", "/9", "/10"]
        );
        let mut romanos = vec!["/IX", "/V", "/I", "/IV", "/X"];
        romanos.sort_by(|a, b| comparar_rutas_linaje(a, b));
        assert_eq!(romanos, ["/I", "/IV", "/V", "/IX", "/X"]);
        let mut anexos = vec!["/ANEXO-B", "/ANEXO-A", "/M-10", "/M-2"];
        anexos.sort_by(|a, b| comparar_rutas_linaje(a, b));
        assert_eq!(anexos, ["/ANEXO-A", "/ANEXO-B", "/M-2", "/M-10"]);
        assert_eq!(comparar_natural("d-c2", "d-c10"), Ordering::Less);
    }

    fn doc() -> DocId {
        DocId::new("PO-ALM-03").unwrap()
    }

    fn arbol(secciones: &[(i8, &str)]) -> Vec<NodoDelArbol> {
        let entradas: Vec<SeccionDelArbol<'_>> = secciones
            .iter()
            .map(|(nivel, codigo)| SeccionDelArbol {
                nivel: NivelJerarquico::new(*nivel).unwrap(),
                codigo,
                sintetica: false,
            })
            .collect();
        construir_arbol(&doc(), &entradas)
    }

    #[test]
    fn test_linaje_con_preambulo_y_descendientes() {
        let nodos = arbol(&[(0, "0.0"), (1, "1"), (2, "1.1"), (3, "1.1.1"), (1, "2")]);
        let rutas: Vec<&str> = nodos.iter().map(|n| n.ruta_linaje.as_str()).collect();
        assert_eq!(rutas, ["/0", "/1", "/1/1.1", "/1/1.1/1.1.1", "/2"]);
        assert_eq!(nodos[0].nodo_id.as_str(), "PO-ALM-03#sec-preambulo");
        assert_eq!(nodos[0].nodo_padre_id, None);
        assert_eq!(nodos[3].nodo_padre_id.as_ref(), Some(&nodos[2].nodo_id));
        assert_eq!(nodos[4].nodo_padre_id, None);
    }

    #[test]
    fn test_codigo_repetido_o_invalido_no_colisiona_en_nodo_ni_en_ruta() {
        let nodos = arbol(&[
            (1, "1"),
            (2, "1.1"),
            (1, "1"),
            (2, "1.1"),
            (2, ""),
            (2, "x'--"),
        ]);
        let ids: HashSet<&str> = nodos.iter().map(|n| n.nodo_id.as_str()).collect();
        let rutas: HashSet<&str> = nodos.iter().map(|n| n.ruta_linaje.as_str()).collect();
        assert_eq!(ids.len(), nodos.len());
        assert_eq!(rutas.len(), nodos.len());
        assert_eq!(nodos[2].nodo_id.as_str(), "PO-ALM-03#sec-idx-2");
        assert_eq!(nodos[3].ruta_linaje, "/idx-2/idx-3");
    }

    #[test]
    fn test_salto_de_nivel_cuelga_del_ancestro_abierto() {
        let nodos = arbol(&[(1, "1"), (4, "1.1.1.1"), (2, "1.2")]);
        assert_eq!(nodos[1].nodo_padre_id.as_ref(), Some(&nodos[0].nodo_id));
        assert_eq!(nodos[1].ruta_linaje, "/1/1.1.1.1");
        assert_eq!(nodos[2].ruta_linaje, "/1/1.2");
    }

    #[test]
    fn test_resumen_toma_titulo_y_dos_frases_sin_cortar_decimales() {
        assert_eq!(
            resumen_extractivo("Soldadura", "Usar 3.5 mm.\nPrecalentar. No leer esto."),
            "Soldadura. Usar 3.5 mm. Precalentar."
        );
        assert_eq!(resumen_extractivo("Solo título", "  "), "Solo título");
        let largo = resumen_extractivo("T", &"ñ".repeat(1000));
        assert_eq!(largo.chars().count(), MAX_CARACTERES_RESUMEN);
    }

    #[test]
    fn test_clasificacion_de_bloque() {
        let h2 = NivelJerarquico::new(2).unwrap();
        assert_eq!(
            clasificar_bloque(NivelJerarquico::PREAMBULO, "x"),
            TipoBloque::Preambulo
        );
        assert_eq!(
            clasificar_bloque(h2, "Alcance técnico"),
            TipoBloque::Alcance
        );
        assert_eq!(
            clasificar_bloque(h2, "Definiciones"),
            TipoBloque::Definicion
        );
        assert_eq!(
            clasificar_bloque(h2, "Procedimiento de izado"),
            TipoBloque::Procedimiento
        );
        assert_eq!(clasificar_bloque(h2, "Otros"), TipoBloque::Narrativa);
    }
}
