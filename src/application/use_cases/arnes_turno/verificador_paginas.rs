//! Comprueba la página de cada extracto literal que cita la respuesta
//! (`docs/arquitectura/ssd_citas_pagina.md` §5). No rechaza ni reintenta: anota, y solo con
//! evidencia positiva de error. En caso de duda guarda silencio
//! (`docs/arquitectura/ssd_generalidad_4_piezas.md` §3c).
//!
//! El extracto se normaliza a su esqueleto (letras y dígitos, en minúsculas y sin tildes ni
//! ligaduras), se busca en los textos marcados con `⟦p. N⟧` de las tools del turno y su página
//! se lee de las marcas, que salen de los saltos guardados en `02_chunks_normativa`.

use std::collections::BTreeSet;

use serde_json::Value;

use super::post_inference_guard::RegistroTurno;
use super::texto_normalizado::plegar_texto;
use crate::domain::services::marcas_pagina::tramos_paginados;

/// Encabezado fijo del bloque que se añade a la respuesta
pub const ENCABEZADO_VERIFICACION_PAGINAS: &str = "**Verificación automática de páginas**";
/// Un esqueleto más corto coincide por azar
const LONGITUD_MINIMA_ESQUELETO: usize = 12;
const MAXIMO_EXTRACTOS: usize = 32;
/// Distancia máxima, en caracteres, entre las comillas y la referencia de página
const VENTANA_REFERENCIA: usize = 80;
/// Un rango más largo cuenta solo por sus extremos
const MAX_PAGINAS_EN_RANGO: u32 = 50;
const LONGITUD_MAXIMA_EXTRACTO_MOSTRADO: usize = 80;
const PARES_DE_COMILLAS: &[(char, char)] = &[('«', '»'), ('“', '”'), ('"', '"')];
const APERTURAS: &[char] = &['«', '“', '"'];
/// De la más larga a la más corta: `paginas` antes que `pagina`
const PALABRAS_DE_PAGINA: &[&str] = &[
    "paginas", "pagina", "pags.", "pags", "pag.", "pages", "page", "pp.", "p.",
];

/// Páginas, ordenadas
pub type Paginas = BTreeSet<u32>;

/// Solo `Discrepante` es evidencia positiva de error y genera nota: el resto son casos de duda o
/// de acierto y el verificador guarda silencio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoCita {
    /// Alguna aparición del extracto corta las páginas citadas (también si se cita el rango que
    /// la contiene o una de las páginas de un extracto a caballo)
    Confirmada,
    /// El extracto aparece en un único lugar de lo consultado y ninguna de sus páginas es la citada
    Discrepante {
        verificadas: Paginas,
    },
    /// Aparece en varias páginas distintas y ninguna es la citada: puede ser un texto repetido
    Ambigua {
        apariciones: Vec<Paginas>,
    },
    SinPagina,
    NoEncontrada,
}

/// Un extracto de la respuesta con la página que cita y lo que se comprobó
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitaRevisada {
    pub extracto: String,
    pub citadas: Paginas,
    pub resultado: ResultadoCita,
}

impl CitaRevisada {
    /// Explicación para el operador; `None` salvo que haya evidencia positiva de error
    pub fn motivo(&self) -> Option<String> {
        let ResultadoCita::Discrepante { verificadas } = &self.resultado else {
            return None;
        };
        Some(format!(
            "«{}»: la respuesta cita {}, pero el texto está en {}.",
            extracto_mostrado(&self.extracto),
            formatear_paginas(&self.citadas),
            formatear_paginas(verificadas)
        ))
    }
}

pub struct VerificadorPaginas;

impl VerificadorPaginas {
    /// Extractos con página de la respuesta, comprobados contra los textos del turno
    pub fn revisar(respuesta: &str, registro: &RegistroTurno) -> Vec<CitaRevisada> {
        let fuentes: Vec<FuentePaginada> = registro
            .textos_paginables()
            .into_iter()
            .map(FuentePaginada::nueva)
            .collect();
        let metadatos: Vec<String> = registro.metadatos().into_iter().map(esqueleto).collect();
        let mut revisadas: Vec<CitaRevisada> = Vec::new();
        for (extracto, citadas) in extractos_con_pagina(respuesta) {
            let esqueleto = esqueleto(&extracto);
            if esqueleto.chars().count() < LONGITUD_MINIMA_ESQUELETO {
                continue;
            }
            if revisadas
                .iter()
                .any(|r| esqueleto_de(r) == esqueleto && r.citadas == citadas)
            {
                continue;
            }
            let resultado = comprobar(&esqueleto, &citadas, &fuentes);
            if resultado != ResultadoCita::Confirmada
                && no_es_extracto_del_cuerpo(&esqueleto, &metadatos, &fuentes)
            {
                continue;
            }
            revisadas.push(CitaRevisada {
                extracto,
                citadas,
                resultado,
            });
            if revisadas.len() == MAXIMO_EXTRACTOS {
                break;
            }
        }
        revisadas
    }

    /// Bloque que se añade a la respuesta si algún extracto tiene evidencia positiva de error
    pub fn nota(revisadas: &[CitaRevisada]) -> Option<String> {
        let lineas: Vec<String> = revisadas
            .iter()
            .filter_map(CitaRevisada::motivo)
            .map(|motivo| format!("- {motivo}"))
            .collect();
        (!lineas.is_empty()).then(|| {
            format!(
                "\n\n{ENCABEZADO_VERIFICACION_PAGINAS}\n{}",
                lineas.join("\n")
            )
        })
    }
}

fn esqueleto_de(revisada: &CitaRevisada) -> String {
    esqueleto(&revisada.extracto)
}

fn comprobar(esqueleto: &str, citadas: &Paginas, fuentes: &[FuentePaginada]) -> ResultadoCita {
    let mut apariciones: Vec<Paginas> = Vec::new();
    let mut sin_pagina = false;
    for fuente in fuentes {
        for (inicio, _) in fuente.esqueleto.match_indices(esqueleto) {
            let paginas: Paginas = fuente.paginas[inicio..inicio + esqueleto.len()]
                .iter()
                .flatten()
                .copied()
                .collect();
            if paginas.is_empty() {
                sin_pagina = true;
            } else if !apariciones.contains(&paginas) {
                apariciones.push(paginas);
            }
        }
    }
    if apariciones
        .iter()
        .any(|paginas| !paginas.is_disjoint(citadas))
    {
        return ResultadoCita::Confirmada;
    }
    match apariciones.len() {
        0 if sin_pagina => ResultadoCita::SinPagina,
        0 => ResultadoCita::NoEncontrada,
        1 => ResultadoCita::Discrepante {
            verificadas: apariciones.remove(0),
        },
        _ => {
            apariciones.sort();
            ResultadoCita::Ambigua { apariciones }
        }
    }
}

/// Un título o un breadcrumb del turno, o la línea que abre una página (cabecera o título), no
/// son extractos del cuerpo: una página distinta en ellos no es un error que señalar
fn no_es_extracto_del_cuerpo(
    esqueleto: &str,
    metadatos: &[String],
    fuentes: &[FuentePaginada],
) -> bool {
    metadatos.iter().any(|m| m.contains(esqueleto)) || es_linea_que_abre_pagina(esqueleto, fuentes)
}

/// Todas las apariciones ocupan exactamente la primera línea de una página, y esa línea no cierra
/// con puntuación como una frase
fn es_linea_que_abre_pagina(esqueleto: &str, fuentes: &[FuentePaginada]) -> bool {
    let mut apariciones = fuentes.iter().flat_map(|fuente| {
        fuente
            .esqueleto
            .match_indices(esqueleto)
            .map(move |(inicio, _)| {
                fuente.es_primera_linea_de_pagina(inicio, inicio + esqueleto.len())
            })
    });
    let primera = apariciones.next();
    primera == Some(true) && apariciones.all(|es_primera_linea| es_primera_linea)
}

/// Esqueleto de un texto marcado, la página de cada byte del esqueleto y dónde empiezan las
/// páginas y terminan las líneas, en bytes del esqueleto
struct FuentePaginada {
    esqueleto: String,
    paginas: Vec<Option<u32>>,
    /// Solo los saltos interiores: la marca inicial abre el fragmento, no necesariamente la página
    inicios_de_pagina: Vec<usize>,
    finales_de_linea: Vec<FinalDeLinea>,
}

#[derive(Debug, Clone, Copy)]
struct FinalDeLinea {
    posicion: usize,
    /// Tipografía: la última letra visible no es puntuación, como en un título o una cabecera
    sin_puntuacion_final: bool,
}

impl FuentePaginada {
    fn nueva(texto: &str) -> Self {
        let mut fuente = Self {
            esqueleto: String::new(),
            paginas: Vec::new(),
            inicios_de_pagina: Vec::new(),
            finales_de_linea: Vec::new(),
        };
        let mut ultima_visible = None;
        for (indice, tramo) in tramos_paginados(texto).into_iter().enumerate() {
            if indice > 0 && tramo.pagina.is_some() {
                fuente.inicios_de_pagina.push(fuente.esqueleto.len());
            }
            for (numero, linea) in tramo.texto.split('\n').enumerate() {
                if numero > 0 {
                    fuente.cerrar_linea(ultima_visible);
                }
                ultima_visible = linea
                    .chars()
                    .rev()
                    .find(|letra| !letra.is_whitespace())
                    .or(ultima_visible);
                for letra in letras_de_esqueleto(linea) {
                    fuente.esqueleto.push(letra);
                    fuente
                        .paginas
                        .extend(std::iter::repeat_n(tramo.pagina, letra.len_utf8()));
                }
            }
        }
        fuente.cerrar_linea(ultima_visible);
        fuente
    }

    fn cerrar_linea(&mut self, ultima_visible: Option<char>) {
        self.finales_de_linea.push(FinalDeLinea {
            posicion: self.esqueleto.len(),
            sin_puntuacion_final: ultima_visible.is_some_and(char::is_alphanumeric),
        });
    }

    /// `inicio..fin` empieza justo tras un salto de página, acaba donde acaba esa línea y la
    /// línea no cierra con puntuación
    fn es_primera_linea_de_pagina(&self, inicio: usize, fin: usize) -> bool {
        if self.inicios_de_pagina.binary_search(&inicio).is_err() {
            return false;
        }
        self.finales_de_linea
            .iter()
            .find(|final_| final_.posicion > inicio)
            .is_some_and(|final_| final_.posicion == fin && final_.sin_puntuacion_final)
    }
}

/// Letras y dígitos en minúsculas, sin tildes ni ligaduras: espacios, guiones partidos, comillas
/// y puntuación no cuentan
fn esqueleto(texto: &str) -> String {
    letras_de_esqueleto(texto).collect()
}

fn letras_de_esqueleto(texto: &str) -> impl Iterator<Item = char> + '_ {
    plegar_texto(&sin_ligaduras(texto))
        .chars()
        .filter(|letra| letra.is_alphanumeric())
        .collect::<Vec<_>>()
        .into_iter()
}

fn sin_ligaduras(texto: &str) -> String {
    let mut limpio = String::with_capacity(texto.len());
    for letra in texto.chars() {
        match letra {
            'ﬀ' => limpio.push_str("ff"),
            'ﬁ' => limpio.push_str("fi"),
            'ﬂ' => limpio.push_str("fl"),
            'ﬃ' => limpio.push_str("ffi"),
            'ﬄ' => limpio.push_str("ffl"),
            'ﬅ' | 'ﬆ' => limpio.push_str("st"),
            otra => limpio.push(otra),
        }
    }
    limpio
}

/// Extractos entre comillas y las páginas que cita la referencia más cercana
fn extractos_con_pagina(respuesta: &str) -> Vec<(String, Paginas)> {
    let letras: Vec<char> = respuesta.chars().collect();
    let plegadas: Vec<char> = letras.iter().map(|l| plegar_letra(*l)).collect();
    let mut citas = Vec::new();
    for (apertura, cierre) in citas_entre_comillas(&letras) {
        let extracto: String = letras[apertura + 1..cierre].iter().collect();
        if let Some(paginas) =
            referencia_tras(&plegadas, cierre).or_else(|| referencia_antes(&plegadas, apertura))
        {
            citas.push((extracto, paginas));
        }
    }
    citas
}

fn plegar_letra(letra: char) -> char {
    plegar_texto(&letra.to_string())
        .chars()
        .next()
        .unwrap_or(letra)
}

/// Posiciones (apertura, cierre) de cada cita entre comillas, en orden de aparición
fn citas_entre_comillas(letras: &[char]) -> Vec<(usize, usize)> {
    let mut citas = Vec::new();
    for (apertura, cierre) in PARES_DE_COMILLAS {
        let mut posicion = 0;
        while let Some(inicio) = (posicion..letras.len()).find(|&i| letras[i] == *apertura) {
            let Some(fin) = (inicio + 1..letras.len()).find(|&i| letras[i] == *cierre) else {
                break;
            };
            citas.push((inicio, fin));
            posicion = fin + 1;
        }
    }
    citas.sort_unstable();
    citas
}

/// Primera referencia de página después del cierre, en la misma línea y antes de otra cita
fn referencia_tras(letras: &[char], cierre: usize) -> Option<Paginas> {
    let inicio = cierre + 1;
    let limite = (inicio..letras.len())
        .take(VENTANA_REFERENCIA)
        .find(|&i| letras[i] == '\n' || APERTURAS.contains(&letras[i]))
        .unwrap_or_else(|| (inicio + VENTANA_REFERENCIA).min(letras.len()));
    referencias_en(letras, inicio, limite).into_iter().next()
}

/// Última referencia de página antes de la apertura, en la misma línea y tras la cita anterior
fn referencia_antes(letras: &[char], apertura: usize) -> Option<Paginas> {
    let minimo = apertura.saturating_sub(VENTANA_REFERENCIA);
    let inicio = (minimo..apertura)
        .rev()
        .find(|&i| letras[i] == '\n' || "»”\"".contains(letras[i]))
        .map_or(minimo, |i| i + 1);
    referencias_en(letras, inicio, apertura).into_iter().last()
}

/// Referencias de página (`p. 53`, `pp. 52-53`, `págs. 3 y 4`, `page 7`) dentro de `inicio..fin`
fn referencias_en(letras: &[char], inicio: usize, fin: usize) -> Vec<Paginas> {
    let mut referencias = Vec::new();
    let mut posicion = inicio;
    while posicion < fin {
        let tras_palabra = (posicion == 0 || !letras[posicion - 1].is_alphanumeric())
            .then(|| palabra_de_pagina_en(letras, posicion, fin))
            .flatten();
        let Some(tras_palabra) = tras_palabra else {
            posicion += 1;
            continue;
        };
        match numeros_de_pagina(letras, tras_palabra, fin) {
            Some((paginas, final_)) => {
                referencias.push(paginas);
                posicion = final_;
            }
            None => posicion = tras_palabra,
        }
    }
    referencias
}

/// Posición tras una palabra de página que empieza en `posicion`
fn palabra_de_pagina_en(letras: &[char], posicion: usize, fin: usize) -> Option<usize> {
    PALABRAS_DE_PAGINA.iter().find_map(|palabra| {
        let longitud = palabra.chars().count();
        let tramo = letras.get(posicion..(posicion + longitud).min(fin))?;
        let coincide = tramo.len() == longitud && tramo.iter().copied().eq(palabra.chars());
        let termina = palabra.ends_with('.')
            || letras
                .get(posicion + longitud)
                .is_none_or(|siguiente| !siguiente.is_alphanumeric());
        (coincide && termina).then_some(posicion + longitud)
    })
}

/// Lista de páginas tras la palabra: `53`, `52-53`, `3 y 4`, `3, 5 and 7`
fn numeros_de_pagina(letras: &[char], desde: usize, fin: usize) -> Option<(Paginas, usize)> {
    let mut posicion = saltar(letras, desde, fin, |l| l == ' ' || l == ':' || l == '*');
    let (primero, tras_primero) = leer_numero(letras, posicion, fin)?;
    let mut paginas = Paginas::new();
    let mut anterior = primero;
    paginas.insert(primero);
    posicion = tras_primero;
    loop {
        let tras_espacios = saltar(letras, posicion, fin, |l| l == ' ');
        let Some((separador, tras_separador)) = separador_en(letras, tras_espacios, fin) else {
            break;
        };
        let inicio_numero = saltar(letras, tras_separador, fin, |l| l == ' ');
        let Some((numero, tras_numero)) = leer_numero(letras, inicio_numero, fin) else {
            break;
        };
        if separador == Separador::Rango
            && numero >= anterior
            && numero - anterior <= MAX_PAGINAS_EN_RANGO
        {
            paginas.extend(anterior..=numero);
        } else {
            paginas.insert(numero);
        }
        anterior = numero;
        posicion = tras_numero;
    }
    Some((paginas, posicion))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Separador {
    Rango,
    Lista,
}

fn separador_en(letras: &[char], posicion: usize, fin: usize) -> Option<(Separador, usize)> {
    let letra = *letras.get(posicion).filter(|_| posicion < fin)?;
    match letra {
        '-' | '–' => Some((Separador::Rango, posicion + 1)),
        ',' => Some((Separador::Lista, posicion + 1)),
        _ => ["y", "and"].iter().find_map(|palabra| {
            let longitud = palabra.chars().count();
            let tramo = letras.get(posicion..(posicion + longitud).min(fin))?;
            let separada = letras.get(posicion + longitud) == Some(&' ');
            (tramo.iter().copied().eq(palabra.chars()) && separada)
                .then_some((Separador::Lista, posicion + longitud))
        }),
    }
}

fn leer_numero(letras: &[char], desde: usize, fin: usize) -> Option<(u32, usize)> {
    let hasta = saltar(letras, desde, fin, |l| l.is_ascii_digit());
    if hasta == desde || hasta - desde > 6 {
        return None;
    }
    let texto: String = letras[desde..hasta].iter().collect();
    let numero = texto.parse().ok().filter(|n| *n > 0)?;
    Some((numero, hasta))
}

fn saltar(letras: &[char], desde: usize, fin: usize, admitida: impl Fn(char) -> bool) -> usize {
    (desde..fin)
        .find(|&i| !admitida(letras[i]))
        .unwrap_or(fin.max(desde))
}

fn extracto_mostrado(extracto: &str) -> String {
    let compacto = extracto.split_whitespace().collect::<Vec<_>>().join(" ");
    if compacto.chars().count() <= LONGITUD_MAXIMA_EXTRACTO_MOSTRADO {
        return compacto;
    }
    let recorte: String = compacto
        .chars()
        .take(LONGITUD_MAXIMA_EXTRACTO_MOSTRADO)
        .collect();
    format!("{recorte}…")
}

/// `p. 53`, `pp. 52-54` (consecutivas) o `pp. 52, 54`
pub fn formatear_paginas(paginas: &Paginas) -> String {
    let lista: Vec<u32> = paginas.iter().copied().collect();
    match lista.as_slice() {
        [] => "ninguna página".to_string(),
        [unica] => format!("p. {unica}"),
        [primera, .., ultima] if lista.windows(2).all(|par| par[1] == par[0] + 1) => {
            format!("pp. {primera}-{ultima}")
        }
        _ => format!(
            "pp. {}",
            lista
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Campo con el texto marcado de un fragmento o de una sección
const CLAVE_TEXTO_MARCADO: &str = "texto";
/// Campos que llevan texto del documento: lo demás (títulos, breadcrumbs, códigos) son metadatos
const CLAVES_DE_TEXTO_DOCUMENTAL: &[&str] = &[CLAVE_TEXTO_MARCADO, "texto_contenido"];

/// Textos `texto` de la salida de una tool que marca páginas
pub(super) fn textos_marcables(salida: &Value) -> Vec<&str> {
    cadenas_con_clave(salida, |clave| clave == CLAVE_TEXTO_MARCADO)
}

/// Cadenas de una salida que no son texto del documento
pub(super) fn textos_de_metadatos(salida: &Value) -> Vec<&str> {
    cadenas_con_clave(salida, |clave| !CLAVES_DE_TEXTO_DOCUMENTAL.contains(&clave))
}

/// Cadenas que cuelgan directamente de una clave admitida, en cualquier anidamiento. Recorrido
/// con pila propia: una salida muy anidada no agota la pila del hilo
fn cadenas_con_clave(salida: &Value, admitida: impl Fn(&str) -> bool) -> Vec<&str> {
    let mut textos = Vec::new();
    let mut pendientes = vec![salida];
    while let Some(valor) = pendientes.pop() {
        match valor {
            Value::Array(elementos) => pendientes.extend(elementos),
            Value::Object(campos) => {
                for (clave, contenido) in campos {
                    match contenido {
                        Value::String(texto) if admitida(clave) => textos.push(texto.as_str()),
                        Value::String(_) => {}
                        otro => pendientes.push(otro),
                    }
                }
            }
            _ => {}
        }
    }
    textos
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::application::dtos::tools::{LakehouseToolName, ToolResult};

    const FRASE: &str = "The maximum test pressure of batch AGJ-7K3M shall be 512.5 kPa";

    fn registro_con(herramienta: LakehouseToolName, textos: &[&str]) -> RegistroTurno {
        let resultados: Vec<_> = textos.iter().map(|t| json!({ "texto": t })).collect();
        let mut registro = RegistroTurno::default();
        registro.registrar(ToolResult {
            call_id: "1".into(),
            tool_name: herramienta.as_str().into(),
            output: json!({ "grupos": [{ "resultados": resultados }] }),
            is_error: false,
        });
        registro
    }

    fn busqueda(textos: &[&str]) -> RegistroTurno {
        registro_con(LakehouseToolName::SearchNormativeContext, textos)
    }

    fn unico(respuesta: &str, registro: &RegistroTurno) -> CitaRevisada {
        let revisadas = VerificadorPaginas::revisar(respuesta, registro);
        assert_eq!(revisadas.len(), 1, "{revisadas:?}");
        revisadas.into_iter().next().unwrap()
    }

    fn paginas(lista: &[u32]) -> Paginas {
        lista.iter().copied().collect()
    }

    fn fuente_52_53() -> String {
        format!("⟦p. 52⟧ Intro of the note.\n⟦p. 53⟧ {FRASE}.\nEnd.")
    }

    #[test]
    fn test_pagina_correcta_se_confirma_sin_nota() {
        let registro = busqueda(&[&fuente_52_53()]);
        let cita = unico(&format!("Dice «{FRASE}» (p. 53)."), &registro);
        assert_eq!(cita.resultado, ResultadoCita::Confirmada);
        assert_eq!(cita.citadas, paginas(&[53]));
        assert!(VerificadorPaginas::nota(&[cita]).is_none());
    }

    #[test]
    fn test_pagina_que_no_contiene_el_extracto_se_corrige() {
        let registro = busqueda(&[&fuente_52_53()]);
        for citada in ["p. 52", "page 54", "págs. 50 y 51"] {
            let cita = unico(&format!("«{FRASE}» ({citada})"), &registro);
            assert_eq!(
                cita.resultado,
                ResultadoCita::Discrepante {
                    verificadas: paginas(&[53])
                },
                "{citada}"
            );
            let nota = VerificadorPaginas::nota(&[cita]).unwrap();
            assert!(nota.contains(ENCABEZADO_VERIFICACION_PAGINAS));
            assert!(nota.contains("el texto está en p. 53"), "{nota}");
        }
    }

    #[test]
    fn test_rango_que_contiene_la_pagina_del_extracto_no_es_un_error() {
        let registro = busqueda(&[&fuente_52_53()]);
        for citada in ["pp. 52-53", "págs. 52 y 53", "pp. 50-60"] {
            let cita = unico(&format!("«{FRASE}» ({citada})"), &registro);
            assert_eq!(cita.resultado, ResultadoCita::Confirmada, "{citada}");
            assert!(VerificadorPaginas::nota(&[cita]).is_none());
        }
    }

    #[test]
    fn test_extracto_que_cruza_el_salto_y_cita_una_de_sus_paginas_no_se_anota() {
        let fuente =
            "⟦p. 52⟧ The limit applies to every hoist frame \n⟦p. 53⟧ listed in the annex.";
        let registro = busqueda(&[fuente]);
        for citada in ["pp. 52-53", "p. 52", "p. 53"] {
            let respuesta =
                format!("«applies to every hoist frame listed in the annex» ({citada})");
            assert_eq!(
                unico(&respuesta, &registro).resultado,
                ResultadoCita::Confirmada,
                "{citada}"
            );
        }
        let ajena = "«applies to every hoist frame listed in the annex» (p. 60)";
        assert_eq!(
            unico(ajena, &registro).resultado,
            ResultadoCita::Discrepante {
                verificadas: paginas(&[52, 53])
            }
        );
    }

    #[test]
    fn test_en_caso_de_duda_se_guarda_silencio() {
        let repetida = busqueda(&[&format!(
            "⟦p. 52⟧ {FRASE}.\n⟦p. 53⟧ Other.\n⟦p. 54⟧ {FRASE}."
        )]);
        // Una de sus apariciones está en la página citada: no hay error que señalar
        let cita = unico(&format!("«{FRASE}» (p. 52)"), &repetida);
        assert_eq!(cita.resultado, ResultadoCita::Confirmada);
        // En varias páginas y ninguna citada: puede ser un texto repetido en todas; no se sabe
        let cita = unico(&format!("«{FRASE}» (p. 60)"), &repetida);
        assert_eq!(
            cita.resultado,
            ResultadoCita::Ambigua {
                apariciones: vec![paginas(&[52]), paginas(&[54])]
            }
        );
        assert!(cita.motivo().is_none());

        let otra = busqueda(&["⟦p. 9⟧ Nothing related here at all."]);
        let no_encontrada = unico(&format!("«{FRASE}» (p. 9)"), &otra);
        assert_eq!(no_encontrada.resultado, ResultadoCita::NoEncontrada);
        assert!(no_encontrada.motivo().is_none());

        let markdown = busqueda(&[&format!("{FRASE}.")]);
        let sin_pagina = unico(&format!("«{FRASE}» (p. 1)"), &markdown);
        assert_eq!(sin_pagina.resultado, ResultadoCita::SinPagina);
        assert!(VerificadorPaginas::nota(&[cita, no_encontrada, sin_pagina]).is_none());
    }

    #[test]
    fn test_parafrasis_no_se_anota() {
        let registro = busqueda(&[&fuente_52_53()]);
        let parafrasis = "«The batch AGJ-7K3M test pressure is limited to 512.5 kPa» (p. 52)";
        let revisadas = VerificadorPaginas::revisar(parafrasis, &registro);
        assert!(
            VerificadorPaginas::nota(&revisadas).is_none(),
            "{revisadas:?}"
        );
    }

    /// Un documento por idioma: título de portada, cabecera de una página y una frase del cuerpo
    struct DocumentoSintetico {
        titulo: &'static str,
        cabecera: &'static str,
        cuerpo: &'static str,
    }

    const DOCUMENTOS: &[DocumentoSintetico] = &[
        DocumentoSintetico {
            titulo: "Hoist Frame Maintenance Manual",
            cabecera: "Chapter 4 Inspection of welded joints",
            cuerpo: "Every welded joint shall be inspected after the load test",
        },
        DocumentoSintetico {
            titulo: "Manual de mantenimiento del bastidor de elevación",
            cabecera: "Capítulo 4 Inspección de uniones soldadas",
            cuerpo: "Cada unión soldada se inspeccionará después del ensayo de carga",
        },
        DocumentoSintetico {
            titulo: "Руководство по обслуживанию подъёмной рамы",
            cabecera: "Глава 4 Осмотр сварных соединений",
            cuerpo: "Каждое сварное соединение осматривается после испытания нагрузкой",
        },
    ];

    impl DocumentoSintetico {
        /// Portada (p. 1) en un fragmento; la p. 7 empieza con la cabecera dentro de otro
        fn registro(&self) -> RegistroTurno {
            let portada = format!("⟦p. 1⟧ {}\nRevision 3, 2026.", self.titulo);
            let interior = format!(
                "⟦p. 6⟧ End of the previous section.\n⟦p. 7⟧ {}\n{}. Further text.",
                self.cabecera, self.cuerpo
            );
            let mut registro = RegistroTurno::default();
            registro.registrar(ToolResult {
                call_id: "1".into(),
                tool_name: LakehouseToolName::SearchNormativeContext.as_str().into(),
                output: json!({ "grupos": [{ "resultados": [
                    { "texto": portada, "cita": { "titulo": self.titulo, "paginas": "1" } },
                    { "texto": interior, "cita": { "titulo": self.titulo, "paginas": "6-7" } },
                ] }] }),
                is_error: false,
            });
            registro
        }
    }

    #[test]
    fn test_el_titulo_de_la_portada_citado_con_otra_pagina_no_se_anota() {
        for documento in DOCUMENTOS {
            let respuesta = format!("Según el «{}» (p. 22), …", documento.titulo);
            let revisadas = VerificadorPaginas::revisar(&respuesta, &documento.registro());
            assert!(
                VerificadorPaginas::nota(&revisadas).is_none(),
                "{}: {revisadas:?}",
                documento.titulo
            );
        }
    }

    #[test]
    fn test_una_cabecera_al_inicio_de_pagina_citada_con_otra_no_se_anota() {
        for documento in DOCUMENTOS {
            let respuesta = format!("En «{}» (p. 9) se explica…", documento.cabecera);
            let revisadas = VerificadorPaginas::revisar(&respuesta, &documento.registro());
            assert!(
                VerificadorPaginas::nota(&revisadas).is_none(),
                "{}: {revisadas:?}",
                documento.cabecera
            );
        }
    }

    #[test]
    fn test_un_extracto_del_cuerpo_con_pagina_ajena_sigue_anotandose() {
        for documento in DOCUMENTOS {
            let respuesta = format!("«{}» (p. 9)", documento.cuerpo);
            let cita = unico(&respuesta, &documento.registro());
            assert_eq!(
                cita.resultado,
                ResultadoCita::Discrepante {
                    verificadas: paginas(&[7])
                },
                "{}",
                documento.cuerpo
            );
            assert!(VerificadorPaginas::nota(&[cita]).is_some());
        }
    }

    #[test]
    fn test_solo_es_cabecera_la_linea_entera_que_abre_una_pagina() {
        for documento in DOCUMENTOS {
            let registro = documento.registro();
            // Empieza la página pero sigue en la línea siguiente: es cuerpo
            let dos_lineas = format!("«{} {}» (p. 9)", documento.cabecera, documento.cuerpo);
            // Empieza la página pero no llega al final de la línea: puede ser cuerpo
            let palabras: Vec<&str> = documento.cabecera.split(' ').collect();
            let parcial = format!("«{}» (p. 9)", palabras[..palabras.len() - 1].join(" "));
            for respuesta in [dos_lineas, parcial] {
                let cita = unico(&respuesta, &registro);
                assert!(
                    matches!(cita.resultado, ResultadoCita::Discrepante { .. }),
                    "{respuesta}: {cita:?}"
                );
            }
        }
    }

    #[test]
    fn test_una_frase_que_ocupa_la_primera_linea_de_la_pagina_no_es_cabecera() {
        for documento in DOCUMENTOS {
            let fuente = format!(
                "⟦p. 6⟧ End of the previous section.\n⟦p. 7⟧ {}.\nFurther text.",
                documento.cuerpo
            );
            let cita = unico(
                &format!("«{}» (p. 9)", documento.cuerpo),
                &busqueda(&[&fuente]),
            );
            assert_eq!(
                cita.resultado,
                ResultadoCita::Discrepante {
                    verificadas: paginas(&[7])
                },
                "{}",
                documento.cuerpo
            );
        }
    }

    #[test]
    fn test_el_inicio_de_un_fragmento_no_es_inicio_de_pagina() {
        // La marca inicial de un fragmento abre el fragmento, no necesariamente la página
        let registro = busqueda(&["⟦p. 7⟧ Every welded joint shall be inspected\nafter the test."]);
        let cita = unico("«Every welded joint shall be inspected» (p. 9)", &registro);
        assert_eq!(
            cita.resultado,
            ResultadoCita::Discrepante {
                verificadas: paginas(&[7])
            }
        );
    }

    #[test]
    fn test_referencia_antes_de_la_cita_y_marca_copiada() {
        let registro = busqueda(&[&fuente_52_53()]);
        let antes = format!("En la página 53 se lee: «{FRASE}».");
        assert_eq!(unico(&antes, &registro).citadas, paginas(&[53]));
        let marca = format!("«{FRASE}» ⟦p. 53⟧");
        assert_eq!(
            unico(&marca, &registro).resultado,
            ResultadoCita::Confirmada
        );
    }

    #[test]
    fn test_sin_referencia_cercana_o_extracto_corto_no_se_revisa() {
        let registro = busqueda(&[&fuente_52_53()]);
        let lejos = format!("«{FRASE}».\nFuente: p. 52");
        assert!(VerificadorPaginas::revisar(&lejos, &registro).is_empty());
        assert!(VerificadorPaginas::revisar("«512.5 kPa» (p. 52)", &registro).is_empty());
        let otra_cita = format!("«{FRASE}» y «otra cosa» (p. 52)");
        let revisadas = VerificadorPaginas::revisar(&otra_cita, &registro);
        assert!(
            revisadas.iter().all(|r| r.extracto != FRASE),
            "{revisadas:?}"
        );
    }

    #[test]
    fn test_el_solape_entre_chunks_no_es_ambiguedad() {
        let a = format!("⟦p. 52⟧ Intro.\n⟦p. 53⟧ {FRASE}.");
        let b = format!("⟦p. 53⟧ {FRASE}.\n⟦p. 54⟧ Next.");
        let registro = busqueda(&[&a, &b]);
        assert_eq!(
            unico(&format!("«{FRASE}» (p. 53)"), &registro).resultado,
            ResultadoCita::Confirmada
        );
    }

    #[test]
    fn test_normaliza_guiones_partidos_espacios_ligaduras_y_comillas() {
        let fuente = "⟦p. 7⟧ The cer-\ntiﬁcate of the “hoist” fra me is ﬁled on site.";
        let registro = busqueda(&[fuente]);
        let respuesta = "“The certificate of the \"hoist\" frame is filed on site” (p. 7)";
        assert_eq!(
            unico(respuesta, &registro).resultado,
            ResultadoCita::Confirmada
        );
    }

    #[test]
    fn test_solo_cuentan_las_tools_que_marcan_paginas_y_sin_error() {
        let sql = registro_con(LakehouseToolName::ExecuteAthenaSql, &[&fuente_52_53()]);
        assert_eq!(
            unico(&format!("«{FRASE}» (p. 53)"), &sql).resultado,
            ResultadoCita::NoEncontrada
        );
        let mut con_error = RegistroTurno::default();
        con_error.registrar(ToolResult {
            call_id: "1".into(),
            tool_name: LakehouseToolName::SearchNormativeContext.as_str().into(),
            output: json!({ "texto": fuente_52_53() }),
            is_error: true,
        });
        assert_eq!(
            unico(&format!("«{FRASE}» (p. 53)"), &con_error).resultado,
            ResultadoCita::NoEncontrada
        );
        let subarbol = registro_con(
            LakehouseToolName::StairGetSubtreeContext,
            &[&fuente_52_53()],
        );
        assert_eq!(
            unico(&format!("«{FRASE}» (p. 53)"), &subarbol).resultado,
            ResultadoCita::Confirmada
        );
    }

    fn busqueda_con_metadatos() -> RegistroTurno {
        let mut registro = RegistroTurno::default();
        registro.registrar(ToolResult {
            call_id: "1".into(),
            tool_name: LakehouseToolName::SearchNormativeContext.as_str().into(),
            output: json!({ "grupos": [{ "tema": "TAREA 201", "resultados": [{
                "texto": fuente_52_53(),
                "breadcrumb_jerarquia": "NORMA-01 > TAREA 201 LISTA PRELIMINAR DE PELIGROS",
                "cita": { "titulo": "Norma de seguridad de sistemas", "paginas": "52-53" },
            }] }] }),
            is_error: false,
        });
        registro
    }

    #[test]
    fn test_un_titulo_o_breadcrumb_entre_comillas_no_es_un_extracto_del_documento() {
        let registro = busqueda_con_metadatos();
        for metadato in [
            "Apartado «TAREA 201 LISTA PRELIMINAR DE PELIGROS» (págs. 52-53)",
            "Documento «Norma de seguridad de sistemas», p. 52",
        ] {
            assert!(
                VerificadorPaginas::revisar(metadato, &registro).is_empty(),
                "{metadato}"
            );
        }
        // Lo que no está ni en el texto ni en los metadatos sigue sin verificar
        assert_eq!(
            unico("«Frase inventada que no devolvió nadie» (p. 52)", &registro).resultado,
            ResultadoCita::NoEncontrada
        );
        // Y un extracto del texto se revisa aunque también haya metadatos
        assert_eq!(
            unico(&format!("«{FRASE}» (p. 52)"), &registro).resultado,
            ResultadoCita::Discrepante {
                verificadas: paginas(&[53])
            }
        );
    }

    #[test]
    fn test_formato_de_paginas() {
        assert_eq!(formatear_paginas(&paginas(&[53])), "p. 53");
        assert_eq!(formatear_paginas(&paginas(&[52, 53, 54])), "pp. 52-54");
        assert_eq!(formatear_paginas(&paginas(&[52, 54])), "pp. 52, 54");
    }
}
