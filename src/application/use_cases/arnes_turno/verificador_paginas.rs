//! Comprueba la página de cada extracto literal que cita la respuesta
//! (`docs/arquitectura/ssd_citas_pagina.md` §5). No rechaza ni reintenta: anota.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoCita {
    Confirmada,
    Discrepante { verificadas: Paginas },
    Ambigua { apariciones: Vec<Paginas> },
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
    /// Explicación para el operador; `None` si la página quedó confirmada
    pub fn motivo(&self) -> Option<String> {
        let extracto = extracto_mostrado(&self.extracto);
        let citadas = formatear_paginas(&self.citadas);
        let explicacion = match &self.resultado {
            ResultadoCita::Confirmada => return None,
            ResultadoCita::Discrepante { verificadas } => format!(
                "la respuesta cita {citadas}, pero el texto está en {}",
                formatear_paginas(verificadas)
            ),
            ResultadoCita::Ambigua { apariciones } => format!(
                "aparece en {}; no se puede confirmar si es {citadas}",
                apariciones
                    .iter()
                    .map(formatear_paginas)
                    .collect::<Vec<_>>()
                    .join(" y en ")
            ),
            ResultadoCita::SinPagina => format!(
                "el texto consultado no tiene páginas registradas (por ejemplo, un documento Markdown); {citadas} no se puede comprobar"
            ),
            ResultadoCita::NoEncontrada => format!(
                "no aparece literalmente en lo consultado en este turno; {citadas} no está verificada"
            ),
        };
        Some(format!("«{extracto}»: {explicacion}."))
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

    /// Bloque que se añade a la respuesta si algún extracto no quedó confirmado
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
    match apariciones.len() {
        0 if sin_pagina => ResultadoCita::SinPagina,
        0 => ResultadoCita::NoEncontrada,
        1 if apariciones[0] == *citadas => ResultadoCita::Confirmada,
        1 => ResultadoCita::Discrepante {
            verificadas: apariciones.remove(0),
        },
        _ => {
            apariciones.sort();
            ResultadoCita::Ambigua { apariciones }
        }
    }
}

/// Esqueleto de un texto marcado y la página de cada byte del esqueleto
struct FuentePaginada {
    esqueleto: String,
    paginas: Vec<Option<u32>>,
}

impl FuentePaginada {
    fn nueva(texto: &str) -> Self {
        let mut esqueleto = String::new();
        let mut paginas = Vec::new();
        for tramo in tramos_paginados(texto) {
            for letra in letras_de_esqueleto(tramo.texto) {
                esqueleto.push(letra);
                paginas.extend(std::iter::repeat_n(tramo.pagina, letra.len_utf8()));
            }
        }
        Self { esqueleto, paginas }
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

/// Textos `texto` de las salidas válidas de las tools que marcan páginas
pub(super) fn textos_marcables(salida: &Value) -> Vec<&str> {
    let mut textos = Vec::new();
    let mut pendientes = vec![salida];
    while let Some(valor) = pendientes.pop() {
        match valor {
            Value::Array(elementos) => pendientes.extend(elementos),
            Value::Object(campos) => {
                for (clave, contenido) in campos {
                    match contenido {
                        Value::String(texto) if clave == "texto" => textos.push(texto.as_str()),
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
    fn test_pagina_equivocada_o_rango_del_chunk_se_corrige() {
        let registro = busqueda(&[&fuente_52_53()]);
        for citada in ["p. 52", "pp. 52-53", "págs. 52 y 53", "page 54"] {
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
    fn test_extracto_que_cruza_el_salto_exige_las_dos_paginas() {
        let fuente =
            "⟦p. 52⟧ The limit applies to every hoist frame \n⟦p. 53⟧ listed in the annex.";
        let registro = busqueda(&[fuente]);
        let respuesta = "«applies to every hoist frame listed in the annex» (pp. 52-53)";
        assert_eq!(
            unico(respuesta, &registro).resultado,
            ResultadoCita::Confirmada
        );
        let solo_una = "«applies to every hoist frame listed in the annex» (p. 52)";
        assert_eq!(
            unico(solo_una, &registro).resultado,
            ResultadoCita::Discrepante {
                verificadas: paginas(&[52, 53])
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
    fn test_ambigua_no_encontrada_y_sin_pagina() {
        let repetida = busqueda(&[&format!(
            "⟦p. 52⟧ {FRASE}.\n⟦p. 53⟧ Other.\n⟦p. 54⟧ {FRASE}."
        )]);
        let cita = unico(&format!("«{FRASE}» (p. 52)"), &repetida);
        assert_eq!(
            cita.resultado,
            ResultadoCita::Ambigua {
                apariciones: vec![paginas(&[52]), paginas(&[54])]
            }
        );
        assert!(cita.motivo().unwrap().contains("no se puede confirmar"));

        let otra = busqueda(&["⟦p. 9⟧ Nothing related here at all."]);
        assert_eq!(
            unico(&format!("«{FRASE}» (p. 9)"), &otra).resultado,
            ResultadoCita::NoEncontrada
        );

        let markdown = busqueda(&[&format!("{FRASE}.")]);
        assert_eq!(
            unico(&format!("«{FRASE}» (p. 1)"), &markdown).resultado,
            ResultadoCita::SinPagina
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

    #[test]
    fn test_formato_de_paginas() {
        assert_eq!(formatear_paginas(&paginas(&[53])), "p. 53");
        assert_eq!(formatear_paginas(&paginas(&[52, 53, 54])), "pp. 52-54");
        assert_eq!(formatear_paginas(&paginas(&[52, 54])), "pp. 52, 54");
    }
}
