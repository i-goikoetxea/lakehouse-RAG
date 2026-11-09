//! Estructura de un PDF a partir del texto de sus páginas, sin depender de ninguna biblioteca PDF:
//! por marcadores (`/Outlines`), por numeración de encabezados o, en último término, en una única
//! sección «Contenido General». Todas las estrategias conservan cada línea del texto.
//!
//! La numeración es una heurística (`docs/arquitectura/decisiones_stair.md` §5): la salida real de
//! pdf-extract no conserva saltos de línea fiables, así que solo se exige línea aislada, patrón
//! estricto y numeración creciente.

use std::collections::{HashMap, HashSet};

use crate::application::dtos::ingestion::{ParsedSection, SaltoPagina};
use crate::domain::models::alcance::{MAX_CARACTERES_TITULO, PilaEncabezados};
use crate::domain::models::estructura_documental::{
    CODIGO_CONTENIDO_GENERAL, MARCA_NODO_POR_PAGINAS, SEGMENTO_PREAMBULO, TipoNodoIndice,
};
use crate::domain::services::arbol_secciones::{CODIGO_PREAMBULO, TITULO_PREAMBULO};

/// Por debajo de esta media de caracteres por página el PDF se considera escaneado
pub const MEDIA_MINIMA_CARACTERES_POR_PAGINA: usize = 50;
pub const TITULO_CONTENIDO_GENERAL: &str = "Contenido General";
pub const TITULO_SIN_NOMBRE: &str = "(Sin título)";

const MAX_NIVEL: u8 = 6;
const MAX_CARACTERES_ENCABEZADO: usize = 120;
const MAX_PALABRAS_ENCABEZADO: usize = 16;
const MAX_DIGITOS_COMPONENTE: usize = 3;
/// Salto tolerado entre encabezados: los documentos reales omiten alguno (`1.4` → `1.4.2`)
const MAX_SALTO_NUMERACION: u32 = 3;
/// Los ítems seguidos de una lista numerada no distan más de estas líneas
const MAX_LINEAS_ENTRE_ITEMS: usize = 15;
const MIN_ENCABEZADOS: usize = 2;
/// Primeras palabras de una nota o remisión, nunca de un apartado con título dudoso
const INICIOS_DE_NOTA: [&str; 8] = [
    "nota", "ver", "véase", "vease", "fuente", "figura", "tabla", "ejemplo",
];
/// Una cabecera o un pie se reconoce a partir de este número de páginas
const MIN_PAGINAS_REPETICION: usize = 3;
/// Un índice solo se busca en las primeras páginas: al menos 3 y hasta 1/5 del documento
const MIN_PAGINAS_INDICE: u32 = 3;
const FRACCION_PAGINAS_INDICE: u32 = 5;
/// `A.1` se ordena detrás de cualquier capítulo numérico
const BASE_ANEXOS: u32 = 1_000;
const MAX_ROMANO: u32 = 100;
/// Margen para el prefijo numérico de un marcador localizado en una línea
const MARGEN_PREFIJO_TITULO: usize = 12;
const MIN_CARACTERES_COINCIDENCIA_PARCIAL: usize = 12;
/// Palabra inicial de la línea que abre un capítulo (`Capítulo 3: Título`)
const PALABRAS_DE_CAPITULO: [&str; 3] = ["capítulo", "capitulo", "chapter"];
/// Prefijo del código de un capítulo, que no se repite en su breadcrumb
const PREFIJO_CODIGO_CAPITULO: &str = "Cap-";
const PALABRA_DE_ANEXO: &str = "ANEXO";
/// Prefijo del código de un anexo, que tampoco se repite en su breadcrumb
const PREFIJO_CODIGO_ANEXO: &str = "Anexo-";
/// El primer apartado de un capítulo no dista más de estas líneas de su título
const MAX_LINEAS_TRAS_CAPITULO: usize = 60;
/// Encabezados encadenados que prueban un capítulo o un reinicio sin línea de capítulo
const MIN_CADENA_CAPITULO: usize = 2;
const MIN_CADENA_REINICIO: usize = 4;
const MIN_SUBAPARTADOS_REINICIO: usize = 2;
const MAX_CADENA: usize = 8;
/// Una sección se divide por páginas si supera ambos umbrales (estrictos)
const UMBRAL_PAGINAS_NODO_SINTETICO: u32 = 15;
const UMBRAL_CARACTERES_NODO_SINTETICO: usize = 40_000;
const PAGINAS_POR_NODO_SINTETICO: u32 = 10;

/// Entrada de `/Outlines` ya resuelta; `pagina` es `None` si su destino no es válido
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntradaMarcador {
    pub titulo: String,
    pub nivel: u8,
    pub pagina: Option<u32>,
}

#[derive(Debug, Clone)]
struct LineaPaginada {
    pagina: u32,
    texto: String,
}

/// Punto del texto donde empieza una sección
struct Apertura {
    linea: usize,
    /// La línea es exactamente el encabezado: su texto ya viaja en el título y el breadcrumb
    omitir_linea: bool,
    codigo: String,
    titulo: String,
    nivel: u8,
    pagina: Option<u32>,
    tipo_nodo: TipoNodoIndice,
    clave_nodo: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Numeracion {
    Decimal(Vec<u32>),
    Romana(u32),
    Capitulo,
}

#[derive(Debug, Clone)]
struct Candidato {
    linea: usize,
    pagina: u32,
    codigo: String,
    titulo: String,
    numeracion: Numeracion,
    /// Título con `.`, `,` o `:` final o con muchas palabras: solo vale si continúa la secuencia
    /// exacta
    dudoso: bool,
    /// Parte abierta por un reinicio sin línea de capítulo: no existe en el documento
    generado: bool,
    clave_nodo: Option<String>,
}

/// Quita caracteres de control y líneas en blanco; el resto del texto queda igual
pub fn normalizar_pagina(texto: &str) -> String {
    texto
        .lines()
        .map(|linea| {
            linea
                .chars()
                .filter(|c| !c.is_control() || *c == '\t')
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .filter(|linea| !linea.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Título apto para mostrar: sin caracteres de control y con espacios simples
pub fn sanear_titulo(titulo: &str) -> String {
    let sin_control: String = titulo
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    sin_control.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn requiere_ocr(paginas: &[String]) -> bool {
    let caracteres: usize = paginas
        .iter()
        .map(|p| p.chars().filter(|c| !c.is_whitespace()).count())
        .sum();
    caracteres < MEDIA_MINIMA_CARACTERES_POR_PAGINA * paginas.len().max(1)
}

fn lineas_de(paginas: &[String]) -> Vec<LineaPaginada> {
    paginas
        .iter()
        .enumerate()
        .flat_map(|(indice, pagina)| {
            pagina.lines().map(move |linea| LineaPaginada {
                pagina: u32::try_from(indice + 1).unwrap_or(u32::MAX),
                texto: linea.to_string(),
            })
        })
        .collect()
}

/// Sección única con todo el texto. `texto_de_respaldo` cubre un PDF sin ninguna línea.
pub fn seccion_contenido_general(paginas: &[String], texto_de_respaldo: &str) -> ParsedSection {
    let lineas = lineas_de(paginas);
    let contenido = texto_de(&lineas);
    // El texto de respaldo no sale de las líneas: sus saltos no se conocen
    let (contenido, saltos_pagina) = if contenido.trim().is_empty() {
        (texto_de_respaldo.to_string(), Vec::new())
    } else {
        (contenido, saltos_de(&lineas, 0))
    };
    ParsedSection {
        codigo: CODIGO_CONTENIDO_GENERAL.to_string(),
        titulo: TITULO_CONTENIDO_GENERAL.to_string(),
        nivel: 1,
        contenido,
        breadcrumb: TITULO_CONTENIDO_GENERAL.to_string(),
        condicion_aplicacion: None,
        pagina_inicio: (!paginas.is_empty()).then_some(1),
        pagina_fin: u32::try_from(paginas.len()).ok().filter(|total| *total > 0),
        saltos_pagina,
        ..ParsedSection::default()
    }
}

// --- Marcadores ---

/// Árbol por `/Outlines`: cada título se localiza en su página; si no aparece, la sección abre al
/// inicio de esa página. Un destino inválido o fuera de orden abre donde va el recorrido.
pub fn estructurar_por_marcadores(
    paginas: &[String],
    marcadores: &[EntradaMarcador],
) -> Vec<ParsedSection> {
    let lineas = lineas_de(paginas);
    let mut aperturas = Vec::with_capacity(marcadores.len());
    let mut cursor = 0;
    for (indice, marcador) in marcadores.iter().enumerate() {
        let titulo = sanear_titulo(&marcador.titulo);
        let (linea, omitir_linea) = ubicar_marcador(&lineas, cursor, marcador.pagina, &titulo);
        cursor = linea + usize::from(omitir_linea);
        let (codigo, titulo) = codigo_y_titulo_de_marcador(&titulo, indice);
        aperturas.push(Apertura {
            linea,
            omitir_linea,
            codigo,
            titulo,
            nivel: marcador.nivel.clamp(1, MAX_NIVEL),
            pagina: marcador.pagina,
            tipo_nodo: TipoNodoIndice::Real,
            clave_nodo: None,
        });
    }
    construir_secciones(&lineas, &aperturas)
}

fn ubicar_marcador(
    lineas: &[LineaPaginada],
    cursor: usize,
    pagina: Option<u32>,
    titulo: &str,
) -> (usize, bool) {
    let Some(pagina) = pagina else {
        return (cursor, false);
    };
    let inicio_pagina = lineas.partition_point(|linea| linea.pagina < pagina);
    let desde = inicio_pagina.max(cursor);
    let normalizado = forma_comparable(titulo);
    for (indice, linea) in lineas.iter().enumerate().skip(desde) {
        if linea.pagina != pagina {
            break;
        }
        match coincidencia_de_titulo(&forma_comparable(&linea.texto), &normalizado) {
            // Solo se omite si el título la reproduce tal cual: otra grafía son palabras distintas
            Coincidencia::Exacta => return (indice, sanear_titulo(&linea.texto) == titulo),
            Coincidencia::Parcial => return (indice, false),
            Coincidencia::Ninguna => {}
        }
    }
    (desde, false)
}

enum Coincidencia {
    Exacta,
    Parcial,
    Ninguna,
}

fn coincidencia_de_titulo(linea: &str, titulo: &str) -> Coincidencia {
    if titulo.is_empty() || linea.is_empty() {
        return Coincidencia::Ninguna;
    }
    if linea == titulo {
        return Coincidencia::Exacta;
    }
    let contiene_titulo =
        linea.contains(titulo) && linea.len() <= titulo.len() + MARGEN_PREFIJO_TITULO;
    let es_titulo_partido =
        linea.len() >= MIN_CARACTERES_COINCIDENCIA_PARCIAL && titulo.starts_with(linea);
    if contiene_titulo || es_titulo_partido {
        Coincidencia::Parcial
    } else {
        Coincidencia::Ninguna
    }
}

/// Solo alfanuméricos en minúsculas: la extracción altera espacios y puntuación
fn forma_comparable(texto: &str) -> String {
    texto
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn codigo_y_titulo_de_marcador(titulo: &str, indice: usize) -> (String, String) {
    if titulo.is_empty() {
        return (format!("M-{}", indice + 1), TITULO_SIN_NOMBRE.to_string());
    }
    match interpretar_numeracion(titulo) {
        Some((codigo, _, resto)) => (codigo, resto),
        None => (format!("M-{}", indice + 1), titulo.to_string()),
    }
}

// --- Numeración ---

/// Árbol por numeración de encabezados; `None` si no hay al menos dos encabezados válidos
pub fn estructurar_por_numeracion(paginas: &[String]) -> Option<Vec<ParsedSection>> {
    let paginas: Vec<String> = paginas.iter().map(|p| normalizar_pagina(p)).collect();
    let lineas = lineas_de(&paginas);
    let total_paginas = u32::try_from(paginas.len()).unwrap_or(u32::MAX);
    let candidatos = candidatos_de_encabezado(&lineas, paginas.len());
    let candidatos = sin_entradas_de_indice(candidatos, total_paginas);
    let encabezados = aceptar_por_tramos(candidatos);
    if encabezados.len() < MIN_ENCABEZADOS {
        return None;
    }
    let hay_romanos = encabezados
        .iter()
        .any(|c| matches!(c.numeracion, Numeracion::Romana(_)));
    // Un anexo no desplaza el cuerpo: sus niveles solo cambian dentro de él
    let hay_capitulos = encabezados
        .iter()
        .any(|c| c.numeracion == Numeracion::Capitulo && !es_anexo(c));
    let mut en_anexo = false;
    let aperturas: Vec<Apertura> = encabezados
        .into_iter()
        .map(|candidato| {
            if candidato.numeracion == Numeracion::Capitulo {
                en_anexo = es_anexo(&candidato);
            }
            Apertura {
                linea: candidato.linea,
                omitir_linea: !candidato.generado,
                nivel: nivel_de(&candidato.numeracion, hay_romanos, hay_capitulos, en_anexo),
                pagina: Some(candidato.pagina),
                tipo_nodo: if candidato.generado {
                    TipoNodoIndice::Sintetico
                } else {
                    TipoNodoIndice::Real
                },
                clave_nodo: candidato.clave_nodo,
                codigo: candidato.codigo,
                titulo: candidato.titulo,
            }
        })
        .collect();
    Some(construir_secciones(&lineas, &aperturas))
}

/// Con apartados romanos (`IX. CONTENIDO`) o capítulos, los decimales cuelgan de ellos
fn nivel_de(numeracion: &Numeracion, hay_romanos: bool, hay_capitulos: bool, en_anexo: bool) -> u8 {
    match numeracion {
        Numeracion::Romana(_) | Numeracion::Capitulo => 1,
        Numeracion::Decimal(clave) => {
            let padres = usize::from(hay_romanos) + usize::from(hay_capitulos);
            let profundidad = clave.len() + padres.max(usize::from(en_anexo));
            u8::try_from(profundidad)
                .unwrap_or(MAX_NIVEL)
                .min(MAX_NIVEL)
        }
    }
}

/// Cabeceras y pies: líneas (con los dígitos neutralizados) presentes en al menos la mitad de las
/// páginas. Solo dejan de contar como encabezado: su texto sigue en el corpus.
fn lineas_repetidas(lineas: &[LineaPaginada], total_paginas: usize) -> HashMap<String, usize> {
    let mut paginas_por_linea: HashMap<String, Vec<u32>> = HashMap::new();
    for linea in lineas {
        let paginas = paginas_por_linea
            .entry(forma_de_repeticion(&linea.texto))
            .or_default();
        if paginas.last() != Some(&linea.pagina) {
            paginas.push(linea.pagina);
        }
    }
    let umbral = MIN_PAGINAS_REPETICION.max(total_paginas.div_ceil(2));
    paginas_por_linea
        .into_iter()
        .filter(|(_, paginas)| paginas.len() >= umbral)
        .map(|(linea, paginas)| (linea, paginas.len()))
        .collect()
}

fn forma_de_repeticion(linea: &str) -> String {
    linea
        .trim()
        .chars()
        .map(|c| if c.is_ascii_digit() { '#' } else { c })
        .collect()
}

fn candidatos_de_encabezado(lineas: &[LineaPaginada], total_paginas: usize) -> Vec<Candidato> {
    let repetidas = lineas_repetidas(lineas, total_paginas);
    lineas
        .iter()
        .enumerate()
        .filter(|(_, linea)| !repetidas.contains_key(&forma_de_repeticion(&linea.texto)))
        .filter_map(|(indice, linea)| {
            if let Some((codigo, titulo)) =
                interpretar_capitulo(&linea.texto).or_else(|| interpretar_anexo(&linea.texto))
            {
                return titulo_de_encabezado_valido(&titulo).then_some(Candidato {
                    linea: indice,
                    pagina: linea.pagina,
                    codigo,
                    titulo,
                    numeracion: Numeracion::Capitulo,
                    dudoso: false,
                    generado: false,
                    clave_nodo: None,
                });
            }
            let (codigo, numeracion, titulo) = interpretar_numeracion(&linea.texto)?;
            let dudoso = titulo_dudoso(&titulo);
            (titulo_de_encabezado_valido(&titulo) && (!dudoso || dudoso_admisible(&titulo)))
                .then_some(Candidato {
                    linea: indice,
                    pagina: linea.pagina,
                    codigo,
                    titulo,
                    numeracion,
                    dudoso,
                    generado: false,
                    clave_nodo: None,
                })
        })
        .collect()
}

/// Patrón estricto: `1`, `1.`, `1.2`, `1.2.3.`, `A.1` o `IX.`, espacio e inicial en mayúscula
fn interpretar_numeracion(linea: &str) -> Option<(String, Numeracion, String)> {
    let linea = linea.trim();
    let (ficha, resto) = linea.split_once(char::is_whitespace)?;
    let titulo = resto.split_whitespace().collect::<Vec<_>>().join(" ");
    if !titulo.chars().next().is_some_and(es_inicial_de_titulo) {
        return None;
    }
    let (numeracion, codigo) = match ficha.strip_suffix('.') {
        Some(sin_punto) => match valor_romano(sin_punto) {
            Some(valor) => (Numeracion::Romana(valor), sin_punto.to_string()),
            None => (clave_decimal(sin_punto)?, sin_punto.to_string()),
        },
        None => (clave_decimal(ficha)?, ficha.to_string()),
    };
    Some((codigo, numeracion, titulo))
}

/// `ANEXO A`, `ANEXO 3` o `ANEXO IV`, con título opcional. Solo en mayúsculas y con identificador
/// corto: `Anexo A` y `ANEXO DE FORMULARIOS` suelen ser una remisión o texto corrido.
fn interpretar_anexo(linea: &str) -> Option<(String, String)> {
    let saneada = sanear_titulo(linea);
    let resto = saneada.strip_prefix(PALABRA_DE_ANEXO)?.strip_prefix(' ')?;
    let (ficha, titulo) = resto.split_once(' ').unwrap_or((resto, ""));
    let identificador = ficha.trim_end_matches([':', '.', '-', '–', '—']);
    let identificador_valido = (identificador.len() == 1
        && identificador.chars().all(|c| c.is_ascii_uppercase()))
        || componente(identificador).is_some()
        || valor_romano(identificador).is_some();
    let titulo = titulo.trim_start_matches([':', '-', '–', '—', ' ']);
    let titulo_valido = titulo.is_empty()
        || (titulo.chars().next().is_some_and(es_inicial_de_titulo) && !titulo_dudoso(titulo));
    (identificador_valido && titulo_valido).then(|| {
        (
            format!("{PREFIJO_CODIGO_ANEXO}{identificador}"),
            saneada.clone(),
        )
    })
}

fn es_anexo(candidato: &Candidato) -> bool {
    candidato.codigo.starts_with(PREFIJO_CODIGO_ANEXO)
}

/// `Capítulo 3: Título`, `Capítulo X. Título` o `Chapter 2 Título`, con inicial en mayúscula
fn interpretar_capitulo(linea: &str) -> Option<(String, String)> {
    let saneada = sanear_titulo(linea);
    let (palabra, resto) = saneada.split_once(' ')?;
    if !PALABRAS_DE_CAPITULO.contains(&palabra.to_lowercase().as_str()) {
        return None;
    }
    let (ficha, titulo) = resto.split_once(' ')?;
    let numero = ficha.trim_end_matches([':', '.', '-', '–', '—']);
    let numero_valido = componente(numero).is_some() || valor_romano(numero).is_some();
    let titulo = titulo.trim_start_matches([':', '-', '–', '—', ' ']);
    (numero_valido && titulo.chars().next().is_some_and(es_inicial_de_titulo)).then(|| {
        (
            format!("{PREFIJO_CODIGO_CAPITULO}{numero}"),
            saneada.clone(),
        )
    })
}

fn es_inicial_de_titulo(c: char) -> bool {
    c.is_ascii_uppercase() || matches!(c, 'Á' | 'É' | 'Í' | 'Ó' | 'Ú' | 'Ñ' | 'Ü')
}

fn clave_decimal(ficha: &str) -> Option<Numeracion> {
    let mut partes = ficha.split('.');
    let primera = partes.next()?;
    let mut clave = Vec::new();
    let letra = primera
        .chars()
        .next()
        .filter(|_| primera.len() == 1)
        .filter(char::is_ascii_uppercase);
    if let Some(letra) = letra {
        clave.push(BASE_ANEXOS + (u32::from(letra) - u32::from('A')));
    } else {
        clave.push(componente(primera)?);
    }
    for parte in partes {
        clave.push(componente(parte)?);
        if clave.len() > usize::from(MAX_NIVEL) {
            return None;
        }
    }
    // `A` a secas no es numeración; `A.1` sí
    if letra.is_some() && clave.len() < 2 {
        return None;
    }
    Some(Numeracion::Decimal(clave))
}

fn componente(parte: &str) -> Option<u32> {
    if parte.is_empty()
        || parte.len() > MAX_DIGITOS_COMPONENTE
        || !parte.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    parte.parse().ok()
}

fn valor_romano(ficha: &str) -> Option<u32> {
    (1..=MAX_ROMANO).find(|valor| romano(*valor) == ficha)
}

fn romano(mut valor: u32) -> String {
    let mut salida = String::new();
    for (peso, simbolo) in [
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ] {
        while valor >= peso {
            salida.push_str(simbolo);
            valor -= peso;
        }
    }
    salida
}

/// Descarta frases: un encabezado es corto y no termina como una oración o una enumeración
fn titulo_de_encabezado_valido(titulo: &str) -> bool {
    titulo.chars().count() <= MAX_CARACTERES_ENCABEZADO && !titulo.ends_with(';')
}

fn titulo_dudoso(titulo: &str) -> bool {
    titulo.ends_with(['.', ',', ':']) || titulo.split_whitespace().count() > MAX_PALABRAS_ENCABEZADO
}

/// Descarta los puntos guía de un índice (`Título.....`) y las notas o remisiones
fn dudoso_admisible(titulo: &str) -> bool {
    let primera = titulo
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or_default()
        .to_lowercase();
    !titulo.ends_with("..")
        && !titulo.ends_with('…')
        && !INICIOS_DE_NOTA.contains(&primera.as_str())
}

/// Un índice repite los títulos reales antes de tiempo. Si un encabezado decimal de las primeras
/// páginas reaparece después, lo que hay entre ambos era el índice.
fn sin_entradas_de_indice(candidatos: Vec<Candidato>, total_paginas: u32) -> Vec<Candidato> {
    let limite = MIN_PAGINAS_INDICE.max(total_paginas / FRACCION_PAGINAS_INDICE);
    let mut vistos: HashMap<(String, String), usize> = HashMap::new();
    let mut indice_detectado = None;
    for (posicion, candidato) in candidatos.iter().enumerate() {
        if candidato.dudoso || !matches!(candidato.numeracion, Numeracion::Decimal(_)) {
            continue;
        }
        let clave = (
            candidato.codigo.clone(),
            forma_comparable(&candidato.titulo)
                .trim_end_matches(|c: char| c.is_ascii_digit())
                .to_string(),
        );
        if let Some(primero) = vistos.get(&clave) {
            if candidatos[*primero].pagina <= limite {
                indice_detectado = Some(*primero..posicion);
            }
            break;
        }
        vistos.insert(clave, posicion);
    }
    let Some(indice) = indice_detectado else {
        return candidatos;
    };
    candidatos
        .into_iter()
        .enumerate()
        .filter(|(posicion, candidato)| {
            // El capítulo justo antes del primer encabezado que reaparece ya es el real; un anexo
            // va al final del documento, así que ahí solo puede ser la última entrada del índice
            let capitulo_real = candidato.numeracion == Numeracion::Capitulo
                && !es_anexo(candidato)
                && *posicion + 1 == indice.end;
            !indice.contains(posicion)
                || capitulo_real
                || matches!(candidato.numeracion, Numeracion::Romana(_))
        })
        .map(|(_, candidato)| candidato)
        .collect()
}

/// Un ítem de lista sin punto (`3 Construyes...`) parece un capítulo y bloquearía los subapartados
/// reales. Si tras un supuesto capítulo `3` el siguiente subapartado es `2.2.2`, no era capítulo.
fn sin_capitulos_contradichos(candidatos: Vec<Candidato>) -> Vec<Candidato> {
    let mut capitulo_del_siguiente_subapartado = None;
    let mut conservados: Vec<Candidato> = candidatos
        .into_iter()
        .rev()
        .filter(|candidato| {
            let Numeracion::Decimal(clave) = &candidato.numeracion else {
                return true;
            };
            if candidato.dudoso {
                return true;
            }
            if clave.len() > 1 {
                capitulo_del_siguiente_subapartado = Some(clave[0]);
                return true;
            }
            capitulo_del_siguiente_subapartado.is_none_or(|capitulo| capitulo >= clave[0])
        })
        .collect();
    conservados.reverse();
    conservados
}

/// Lista numerada en curso: `1.` fuera de secuencia y sus continuaciones inmediatas
struct ListaAbierta {
    prefijo: Vec<u32>,
    ultimo: u32,
    linea: usize,
}

impl ListaAbierta {
    fn continua_con(&self, clave: &[u32], linea: usize) -> bool {
        let Some((ultimo, prefijo)) = clave.split_last() else {
            return false;
        };
        prefijo == self.prefijo.as_slice()
            && *ultimo == self.ultimo + 1
            && linea - self.linea <= MAX_LINEAS_ENTRE_ITEMS
    }
}

/// Estado de la numeración en curso; `admitir` decide si un candidato abre sección
#[derive(Default)]
struct Secuencia {
    ultimo_decimal: Option<Vec<u32>>,
    ultimo_romano: u32,
    lista: Option<ListaAbierta>,
    /// Tras un capítulo o un reinicio, el siguiente decimal abre la numeración sea cual sea
    tramo_recien_abierto: bool,
}

impl Secuencia {
    fn abrir_tramo(&mut self) {
        self.ultimo_decimal = None;
        self.lista = None;
        self.tramo_recien_abierto = true;
    }

    fn fijar_decimal(&mut self, clave: &[u32]) -> bool {
        self.ultimo_decimal = Some(clave.to_vec());
        self.lista = None;
        self.tramo_recien_abierto = false;
        true
    }

    fn admitir(&mut self, candidato: &Candidato) -> bool {
        match &candidato.numeracion {
            Numeracion::Capitulo => false,
            Numeracion::Decimal(clave) if candidato.dudoso => {
                continua_exacto(self.ultimo_decimal.as_deref(), clave) && self.fijar_decimal(clave)
            }
            Numeracion::Romana(_) if candidato.dudoso => false,
            Numeracion::Romana(valor) => {
                let sigue = *valor == self.ultimo_romano + 1;
                if sigue {
                    self.ultimo_romano = *valor;
                }
                sigue
            }
            Numeracion::Decimal(clave) => self.admitir_decimal(clave, candidato.linea),
        }
    }

    fn admitir_decimal(&mut self, clave: &[u32], linea: usize) -> bool {
        if self.tramo_recien_abierto {
            return self.fijar_decimal(clave);
        }
        if let Some(abierta) = self.lista.as_mut().filter(|l| l.continua_con(clave, linea)) {
            abierta.ultimo += 1;
            abierta.linea = linea;
            return false;
        }
        if es_sucesor(self.ultimo_decimal.as_deref(), clave) {
            return self.fijar_decimal(clave);
        }
        if clave.last() == Some(&1) {
            self.lista = Some(ListaAbierta {
                prefijo: clave[..clave.len() - 1].to_vec(),
                ultimo: 1,
                linea,
            });
        }
        false
    }
}

/// Parte del documento con numeración propia. El primer tramo no tiene capítulo si el documento
/// no empieza por uno.
#[derive(Default)]
struct Tramo {
    capitulo: Option<Candidato>,
    candidatos: Vec<Candidato>,
}

/// Encabezados de cada tramo; sus códigos pueden repetirse, así que su identidad lleva el tramo
fn aceptar_por_tramos(candidatos: Vec<Candidato>) -> Vec<Candidato> {
    let mut secuencia = Secuencia::default();
    let mut aceptados = Vec::new();
    for (posicion, tramo) in dividir_en_tramos(candidatos).into_iter().enumerate() {
        let prefijo = (posicion > 0).then(|| format!("C{posicion}"));
        if let Some(mut capitulo) = tramo.capitulo {
            capitulo.clave_nodo = prefijo.clone();
            aceptados.push(capitulo);
            secuencia.abrir_tramo();
        }
        for mut candidato in sin_capitulos_contradichos(tramo.candidatos) {
            if secuencia.admitir(&candidato) {
                candidato.clave_nodo = prefijo
                    .as_ref()
                    .map(|p| format!("{p}-{}", candidato.codigo));
                aceptados.push(candidato);
            }
        }
    }
    aceptados
}

/// Corta donde la numeración se reinicia con coherencia: en una línea de capítulo seguida de sus
/// apartados o, sin ella, en un reinicio con subapartados que continúa en otra página
fn dividir_en_tramos(candidatos: Vec<Candidato>) -> Vec<Tramo> {
    let mut tramos = vec![Tramo::default()];
    let mut secuencia = Secuencia::default();
    let mut capitulos_vistos = HashSet::new();
    // Un ítem que parece capítulo adelantaría la numeración y fingiría un reinicio en lo siguiente
    let fiables: HashSet<usize> = sin_capitulos_contradichos(candidatos.clone())
        .iter()
        .map(|candidato| candidato.linea)
        .collect();
    for (indice, candidato) in candidatos.iter().enumerate() {
        if candidato.numeracion == Numeracion::Capitulo {
            let coherente = if es_anexo(candidato) {
                anexo_coherente(&candidatos, indice)
            } else {
                capitulo_coherente(&candidatos, indice)
            };
            if coherente && capitulos_vistos.insert(forma_comparable(&candidato.titulo)) {
                tramos.push(Tramo {
                    capitulo: Some(candidato.clone()),
                    candidatos: Vec::new(),
                });
                secuencia.abrir_tramo();
            }
            continue;
        }
        let previo = secuencia.ultimo_decimal.clone();
        if fiables.contains(&candidato.linea)
            && !secuencia.admitir(candidato)
            && reinicio_coherente(&candidatos, indice, previo.as_deref())
        {
            tramos.push(Tramo {
                capitulo: Some(parte_generada(candidato, tramos.len())),
                candidatos: Vec::new(),
            });
            secuencia.abrir_tramo();
            secuencia.admitir(candidato);
        }
        if let Some(tramo) = tramos.last_mut() {
            tramo.candidatos.push(candidato.clone());
        }
    }
    tramos
}

fn clave_decimal_firme(candidato: &Candidato) -> Option<&[u32]> {
    match &candidato.numeracion {
        Numeracion::Decimal(clave) if !candidato.dudoso => Some(clave),
        _ => None,
    }
}

/// Encabezados que continúan al de `inicio` sin salirse de la numeración; ignora lo intercalado,
/// como las listas, y se detiene en el siguiente capítulo
fn cadena_desde(candidatos: &[Candidato], inicio: usize) -> Vec<&Candidato> {
    let mut cadena = vec![&candidatos[inicio]];
    let Some(clave) = clave_decimal_firme(&candidatos[inicio]) else {
        return cadena;
    };
    let mut ultimo = clave;
    for candidato in &candidatos[inicio + 1..] {
        if candidato.numeracion == Numeracion::Capitulo || cadena.len() == MAX_CADENA {
            break;
        }
        if let Some(clave) = clave_decimal_firme(candidato)
            && es_sucesor(Some(ultimo), clave)
        {
            cadena.push(candidato);
            ultimo = clave;
        }
    }
    cadena
}

fn subapartados_en(cadena: &[&Candidato]) -> usize {
    cadena
        .iter()
        .filter(|c| clave_decimal_firme(c).is_some_and(|clave| clave.len() > 1))
        .count()
}

/// Algún encabezado de la cadena dista más que los ítems seguidos de una lista
fn cadena_espaciada(cadena: &[&Candidato]) -> bool {
    cadena
        .windows(2)
        .any(|par| par[1].linea - par[0].linea > MAX_LINEAS_ENTRE_ITEMS)
}

fn abre_numeracion(candidato: &Candidato) -> bool {
    clave_decimal_firme(candidato).is_some_and(|clave| clave.last() == Some(&1))
}

/// Una línea de capítulo solo abre capítulo si le siguen sus apartados: así se descartan las
/// remisiones y las entradas de un índice
fn capitulo_coherente(candidatos: &[Candidato], indice: usize) -> bool {
    let Some(siguiente) = candidatos.get(indice + 1) else {
        return false;
    };
    let cadena = cadena_desde(candidatos, indice + 1);
    abre_numeracion(siguiente)
        && siguiente.linea - candidatos[indice].linea <= MAX_LINEAS_TRAS_CAPITULO
        && cadena.len() >= MIN_CADENA_CAPITULO
        && (subapartados_en(&cadena) > 0 || cadena_espaciada(&cadena))
}

/// Un anexo puede no tener apartados, pero si los tiene reinicia la numeración: una mención suelta
/// en mitad del cuerpo va seguida del apartado siguiente y no corta nada
fn anexo_coherente(candidatos: &[Candidato], indice: usize) -> bool {
    candidatos[indice + 1..]
        .iter()
        .find(|candidato| !candidato.dudoso)
        .is_none_or(|siguiente| {
            siguiente.numeracion == Numeracion::Capitulo || abre_numeracion(siguiente)
        })
}

/// Sin línea de capítulo hace falta más: vuelve atrás en la numeración, tiene subapartados, sus
/// encabezados están espaciados y continúa en otra página. Una lista, una nota o una bibliografía
/// no lo cumplen.
fn reinicio_coherente(candidatos: &[Candidato], indice: usize, previo: Option<&[u32]>) -> bool {
    let candidato = &candidatos[indice];
    let Some(clave) = clave_decimal_firme(candidato) else {
        return false;
    };
    let cadena = cadena_desde(candidatos, indice);
    abre_numeracion(candidato)
        && previo.is_some_and(|previo| clave <= previo)
        && cadena.len() >= MIN_CADENA_REINICIO
        && subapartados_en(&cadena) >= MIN_SUBAPARTADOS_REINICIO
        && cadena_espaciada(&cadena)
        && cadena
            .last()
            .is_some_and(|ultimo| ultimo.pagina > candidato.pagina)
}

/// Nodo que agrupa un tramo sin título en el documento: no tiene texto ni línea propios
fn parte_generada(apertura: &Candidato, posicion: usize) -> Candidato {
    Candidato {
        linea: apertura.linea,
        pagina: apertura.pagina,
        codigo: format!("{PREFIJO_CODIGO_CAPITULO}{}", posicion + 1),
        titulo: format!("Parte {} (numeración reiniciada)", posicion + 1),
        numeracion: Numeracion::Capitulo,
        dudoso: false,
        generado: true,
        clave_nodo: None,
    }
}

/// Secuencia sin huecos para un título dudoso: subapartado (`clave` de 2 o más
/// niveles) que es el primer hijo del anterior o el siguiente de alguno de sus ancestros
fn continua_exacto(previo: Option<&[u32]>, clave: &[u32]) -> bool {
    let Some(previo) = previo else {
        return false;
    };
    if clave.len() < 2 || clave <= previo {
        return false;
    }
    let comun = previo.iter().zip(clave).take_while(|(a, b)| a == b).count();
    let base = previo.get(comun).copied().unwrap_or(0);
    clave[comun] == base + 1 && clave[comun + 1..].iter().all(|c| *c == 1)
}

fn es_sucesor(previo: Option<&[u32]>, clave: &[u32]) -> bool {
    let Some(previo) = previo else {
        return clave[0] <= 1 || clave[0] >= BASE_ANEXOS;
    };
    if clave <= previo {
        return false;
    }
    let comun = previo.iter().zip(clave).take_while(|(a, b)| a == b).count();
    let base = previo.get(comun).copied().unwrap_or(0);
    let abre_anexos = clave[comun] >= BASE_ANEXOS && base < BASE_ANEXOS;
    (abre_anexos || clave[comun] - base <= MAX_SALTO_NUMERACION)
        && clave[comun + 1..]
            .iter()
            .all(|c| *c <= MAX_SALTO_NUMERACION)
}

// --- Secciones ---

/// Divide por páginas las secciones que superan los umbrales (`decisiones_stair.md` §9). La real
/// queda como padre sin texto, y sus sub-nodos sintéticos heredan breadcrumb y condición.
pub fn dividir_secciones_largas(secciones: Vec<ParsedSection>) -> Vec<ParsedSection> {
    secciones.into_iter().flat_map(dividir_si_supera).collect()
}

fn supera_umbrales(seccion: &ParsedSection) -> bool {
    let paginas = match (seccion.saltos_pagina.first(), seccion.saltos_pagina.last()) {
        (Some(primero), Some(ultimo)) => ultimo.pagina - primero.pagina + 1,
        _ => 0,
    };
    seccion.nivel < MAX_NIVEL
        && seccion.tipo_nodo == TipoNodoIndice::Real
        && paginas > UMBRAL_PAGINAS_NODO_SINTETICO
        && seccion.contenido.chars().count() > UMBRAL_CARACTERES_NODO_SINTETICO
}

/// Saltos de cada bloque de páginas consecutivas
fn bloques_de_paginas(saltos: &[SaltoPagina]) -> Vec<&[SaltoPagina]> {
    let primera = saltos.first().map_or(0, |salto| salto.pagina);
    saltos
        .chunk_by(|a, b| {
            (a.pagina - primera) / PAGINAS_POR_NODO_SINTETICO
                == (b.pagina - primera) / PAGINAS_POR_NODO_SINTETICO
        })
        .collect()
}

fn dividir_si_supera(mut real: ParsedSection) -> Vec<ParsedSection> {
    if !supera_umbrales(&real) {
        return vec![real];
    }
    let saltos = std::mem::take(&mut real.saltos_pagina);
    let bloques = bloques_de_paginas(&saltos);
    if bloques.len() < 2 {
        real.saltos_pagina = saltos;
        return vec![real];
    }
    let contenido = std::mem::take(&mut real.contenido);
    let mut secciones = Vec::with_capacity(bloques.len() + 1);
    for (posicion, bloque) in bloques.iter().enumerate() {
        // El primer bloque incluye lo que hubiera antes del primer salto
        let inicio = if posicion == 0 {
            0
        } else {
            bloque[0].desplazamiento
        };
        let fin = bloques
            .get(posicion + 1)
            .map_or(contenido.len(), |siguiente| siguiente[0].desplazamiento);
        secciones.push(sub_nodo_por_paginas(
            &real,
            contenido[inicio..fin].trim_end_matches('\n'),
            bloque,
            inicio,
        ));
    }
    secciones.insert(0, real);
    secciones
}

fn sub_nodo_por_paginas(
    real: &ParsedSection,
    contenido: &str,
    saltos: &[SaltoPagina],
    inicio: usize,
) -> ParsedSection {
    let primera = saltos[0].pagina;
    let ultima = saltos[saltos.len() - 1].pagina;
    let sufijo = format!("{MARCA_NODO_POR_PAGINAS}{primera}-{ultima}");
    let clave_base = match (&real.clave_nodo, real.nivel) {
        (Some(clave), _) => Some(clave.as_str()),
        (None, 0) => Some(SEGMENTO_PREAMBULO),
        (None, _) => None,
    };
    ParsedSection {
        codigo: format!("{}{sufijo}", real.codigo),
        titulo: format!("Páginas {primera}-{ultima} de «{}»", real.titulo),
        nivel: real.nivel + 1,
        contenido: contenido.to_string(),
        breadcrumb: real.breadcrumb.clone(),
        condicion_aplicacion: real.condicion_aplicacion.clone(),
        pagina_inicio: Some(primera),
        pagina_fin: Some(ultima),
        saltos_pagina: saltos
            .iter()
            .map(|salto| SaltoPagina {
                desplazamiento: salto.desplazamiento.saturating_sub(inicio),
                pagina: salto.pagina,
            })
            .collect(),
        tipo_nodo: TipoNodoIndice::Sintetico,
        clave_nodo: clave_base.map(|clave| format!("{clave}{sufijo}")),
    }
}

fn construir_secciones(lineas: &[LineaPaginada], aperturas: &[Apertura]) -> Vec<ParsedSection> {
    let mut secciones = Vec::with_capacity(aperturas.len() + 1);
    let primera = aperturas.first().map_or(lineas.len(), |a| a.linea);
    if primera > 0 {
        let previas = &lineas[..primera];
        secciones.push(ParsedSection {
            codigo: CODIGO_PREAMBULO.to_string(),
            titulo: TITULO_PREAMBULO.to_string(),
            nivel: 0,
            contenido: texto_de(previas),
            breadcrumb: TITULO_PREAMBULO.to_string(),
            condicion_aplicacion: None,
            pagina_inicio: previas.first().map(|l| l.pagina),
            pagina_fin: previas.last().map(|l| l.pagina),
            saltos_pagina: saltos_de(previas, 0),
            ..ParsedSection::default()
        });
    }
    let mut pila = PilaEncabezados::default();
    for (indice, apertura) in aperturas.iter().enumerate() {
        let fin = aperturas.get(indice + 1).map_or(lineas.len(), |a| a.linea);
        let inicio = (apertura.linea + usize::from(apertura.omitir_linea)).min(fin);
        let propias = &lineas[inicio..fin];
        let etiqueta = etiqueta_de(apertura);
        // Una parte generada cierra lo anterior, pero no entra en el breadcrumb: no es del documento
        if apertura.tipo_nodo == TipoNodoIndice::Sintetico {
            pila = PilaEncabezados::default();
        } else {
            pila.apilar(apertura.nivel, &etiqueta);
        }
        let mut contenido = texto_de(propias);
        let mut prefijo = 0;
        // La pila recorta los títulos enormes: el texto íntegro se conserva en la sección
        if etiqueta.chars().count() > MAX_CARACTERES_TITULO {
            let sin_prefijo = contenido.len();
            contenido = format!("{etiqueta}\n{contenido}").trim().to_string();
            prefijo = contenido.len().saturating_sub(sin_prefijo);
        }
        let pagina_inicio = apertura
            .pagina
            .or_else(|| lineas.get(apertura.linea).map(|l| l.pagina));
        secciones.push(ParsedSection {
            codigo: apertura.codigo.clone(),
            titulo: apertura.titulo.clone(),
            nivel: apertura.nivel,
            contenido,
            breadcrumb: if apertura.tipo_nodo == TipoNodoIndice::Sintetico {
                etiqueta
            } else {
                pila.breadcrumb()
            },
            condicion_aplicacion: pila.condicion_heredada(),
            pagina_inicio,
            pagina_fin: propias.last().map(|l| l.pagina).or(pagina_inicio),
            saltos_pagina: saltos_de(propias, prefijo),
            tipo_nodo: apertura.tipo_nodo,
            clave_nodo: apertura.clave_nodo.clone(),
        });
    }
    secciones
}

/// Texto del encabezado para el breadcrumb: los códigos generados (`M-3`) no le aportan nada
fn etiqueta_de(apertura: &Apertura) -> String {
    if apertura.codigo.starts_with("M-")
        || apertura.codigo.starts_with(PREFIJO_CODIGO_CAPITULO)
        || apertura.codigo.starts_with(PREFIJO_CODIGO_ANEXO)
    {
        apertura.titulo.clone()
    } else {
        format!("{} {}", apertura.codigo, apertura.titulo)
    }
}

/// Dónde empieza cada página en el texto que produce `texto_de`
fn saltos_de(lineas: &[LineaPaginada], desplazamiento_inicial: usize) -> Vec<SaltoPagina> {
    let mut saltos: Vec<SaltoPagina> = Vec::new();
    let mut desplazamiento = desplazamiento_inicial;
    for linea in lineas {
        if saltos
            .last()
            .is_none_or(|salto| salto.pagina != linea.pagina)
        {
            saltos.push(SaltoPagina {
                desplazamiento,
                pagina: linea.pagina,
            });
        }
        desplazamiento += linea.texto.len() + 1;
    }
    saltos
}

fn texto_de(lineas: &[LineaPaginada]) -> String {
    lineas
        .iter()
        .map(|linea| linea.texto.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contenido_general_guarda_donde_empieza_cada_pagina() {
        let paginas = [
            "uno\ndos".to_string(),
            "tres".to_string(),
            "cuatro".to_string(),
        ];
        let seccion = seccion_contenido_general(&paginas, "respaldo");
        assert_eq!(seccion.contenido, "uno\ndos\ntres\ncuatro");
        let saltos: Vec<(usize, u32)> = seccion
            .saltos_pagina
            .iter()
            .map(|s| (s.desplazamiento, s.pagina))
            .collect();
        assert_eq!(saltos, [(0, 1), (8, 2), (13, 3)]);
        for salto in &seccion.saltos_pagina {
            let linea = seccion.contenido[salto.desplazamiento..]
                .lines()
                .next()
                .unwrap();
            assert_eq!(
                linea,
                paginas[salto.pagina as usize - 1].lines().next().unwrap()
            );
        }

        let vacio = seccion_contenido_general(&["  ".to_string(), String::new()], "respaldo");
        assert_eq!(vacio.contenido, "respaldo");
        assert!(
            vacio.saltos_pagina.is_empty(),
            "el respaldo no tiene saltos conocidos"
        );
    }

    fn clave(linea: &str) -> Option<(String, String)> {
        interpretar_numeracion(linea).map(|(codigo, _, titulo)| (codigo, titulo))
    }

    fn par(codigo: &str, titulo: &str) -> Option<(String, String)> {
        Some((codigo.to_string(), titulo.to_string()))
    }

    fn relleno(lineas: usize) -> String {
        (0..lineas)
            .map(|n| format!("texto corrido de relleno {n} sin numeración alguna"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn codigos(secciones: &[ParsedSection]) -> Vec<&str> {
        secciones.iter().map(|s| s.codigo.as_str()).collect()
    }

    fn pagina(encabezados: &[&str]) -> String {
        encabezados
            .iter()
            .map(|e| format!("{e}\n{}", relleno(20)))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_reinicio_con_linea_de_capitulo_abre_capitulo() {
        let paginas = [
            pagina(&["Capítulo 1: Primero", "1.1 Alfa", "1.2 Beta", "1.2.1 Gamma"]),
            pagina(&[
                "Capítulo 2: Segundo",
                "1. Introducción",
                "2. Desarrollo",
                "2.1 Delta",
                "2.2 Épsilon",
                "3. Ejemplos",
            ]),
            pagina(&[
                "Capítulo 3: Tercero",
                "2.1. Zeta",
                "2.2. Eta",
                "2.2.1. Theta",
            ]),
        ];
        let secciones = estructurar_por_numeracion(&paginas).unwrap();
        let capitulos: Vec<&str> = secciones
            .iter()
            .filter(|s| s.nivel == 1)
            .map(|s| s.titulo.as_str())
            .collect();
        assert_eq!(
            capitulos,
            [
                "Capítulo 1: Primero",
                "Capítulo 2: Segundo",
                "Capítulo 3: Tercero"
            ]
        );
        let ejemplos = secciones.iter().find(|s| s.titulo == "Ejemplos").unwrap();
        assert_eq!(ejemplos.breadcrumb, "Capítulo 2: Segundo > 3 Ejemplos");
        let theta = secciones.iter().find(|s| s.titulo == "Theta").unwrap();
        assert_eq!(theta.nivel, 4);
        assert!(theta.breadcrumb.starts_with("Capítulo 3: Tercero > "));
        let claves: std::collections::HashSet<_> = secciones
            .iter()
            .map(|s| s.clave_nodo.clone().unwrap_or(s.codigo.clone()))
            .collect();
        assert_eq!(
            claves.len(),
            secciones.len(),
            "cada sección tiene identidad propia"
        );
    }

    fn cuerpo_y_anexo_a() -> Vec<ParsedSection> {
        let paginas = [
            pagina(&[
                "1. Objeto",
                "2. Alcance",
                "3. Roles",
                "4. Método",
                "5. Registros",
            ]),
            pagina(&["ANEXO A", "1. Formularios", "2. Plantillas"]),
        ];
        estructurar_por_numeracion(&paginas).unwrap()
    }

    #[test]
    fn test_anexo_sin_numero_abre_nodo_raiz_real() {
        let secciones = cuerpo_y_anexo_a();
        let anexo = secciones.iter().find(|s| s.titulo == "ANEXO A").unwrap();
        assert_eq!(anexo.nivel, 1);
        assert_eq!(anexo.tipo_nodo, TipoNodoIndice::Real);
        assert_eq!(anexo.pagina_inicio, Some(2));
    }

    #[test]
    fn test_apartados_del_anexo_cuelgan_de_el_y_el_cuerpo_no_cambia() {
        let secciones = cuerpo_y_anexo_a();
        let formularios = secciones
            .iter()
            .find(|s| s.titulo == "Formularios")
            .unwrap();
        assert_eq!(formularios.nivel, 2);
        assert_eq!(formularios.breadcrumb, "ANEXO A > 1 Formularios");
        let registros = secciones.iter().find(|s| s.titulo == "Registros").unwrap();
        assert_eq!(registros.nivel, 1, "el cuerpo conserva su nivel");
        assert_eq!(registros.breadcrumb, "5 Registros");
        assert!(
            !registros.contenido.contains("ANEXO A"),
            "el anexo no queda absorbido"
        );
        let claves: HashSet<_> = secciones
            .iter()
            .map(|s| s.clave_nodo.clone().unwrap_or(s.codigo.clone()))
            .collect();
        assert_eq!(claves.len(), secciones.len());
    }

    #[test]
    fn test_formas_de_anexo_y_titulo_opcional() {
        let anexo = |linea: &str| interpretar_anexo(linea).map(|(_, titulo)| titulo);
        assert_eq!(anexo("ANEXO A").as_deref(), Some("ANEXO A"));
        assert_eq!(anexo("ANEXO  3").as_deref(), Some("ANEXO 3"));
        assert_eq!(anexo("ANEXO IV").as_deref(), Some("ANEXO IV"));
        assert_eq!(
            anexo("ANEXO A: Formularios").as_deref(),
            Some("ANEXO A: Formularios")
        );
        for linea in [
            "Anexo A",
            "ANEXO",
            "ANEXOS",
            "ANEXO a",
            "ANEXO DE FORMULARIOS",
            "ANEXO A del presente procedimiento",
            "ANEXO A ........ 12",
            "ANEXO A 12",
            "VER ANEXO A",
        ] {
            assert_eq!(anexo(linea), None, "{linea}");
        }
    }

    #[test]
    fn test_anexo_sin_apartados_abre_y_el_segundo_tambien() {
        let paginas = [
            pagina(&["1. Objeto", "2. Alcance"]),
            pagina(&["ANEXO A", "ANEXO B"]),
        ];
        let secciones = estructurar_por_numeracion(&paginas).unwrap();
        assert_eq!(
            codigos(&secciones),
            ["1", "2", "Anexo-A", "Anexo-B"],
            "{secciones:?}"
        );
    }

    #[test]
    fn test_mencion_de_anexo_en_medio_del_cuerpo_no_corta() {
        let paginas = [pagina(&[
            "1. Objeto",
            "2. Alcance",
            "ANEXO A",
            "3. Roles",
            "4. Método",
        ])];
        let secciones = estructurar_por_numeracion(&paginas).unwrap();
        assert_eq!(codigos(&secciones), ["1", "2", "3", "4"]);
        let alcance = secciones.iter().find(|s| s.titulo == "Alcance").unwrap();
        assert!(alcance.contenido.contains("ANEXO A"), "no se pierde texto");
    }

    #[test]
    fn test_anexo_listado_en_el_indice_no_abre_nodo_ni_captura_el_cuerpo() {
        let paginas = [
            "1. Objeto\n2. Alcance\nANEXO A".to_string(),
            pagina(&["1. Objeto", "2. Alcance"]),
            pagina(&["ANEXO A", "1. Formularios", "2. Plantillas"]),
            relleno(10),
            relleno(10),
        ];
        let secciones = estructurar_por_numeracion(&paginas).unwrap();
        let anexos: Vec<_> = secciones
            .iter()
            .filter(|s| s.titulo == "ANEXO A")
            .map(|s| s.pagina_inicio)
            .collect();
        assert_eq!(anexos, [Some(3)], "{:?}", codigos(&secciones));
        let objeto = secciones.iter().find(|s| s.titulo == "Objeto").unwrap();
        assert_eq!(objeto.breadcrumb, "1 Objeto");
    }

    #[test]
    fn test_reinicio_coherente_sin_linea_de_capitulo_abre_parte_sintetica() {
        let paginas = [
            pagina(&["1 Uno", "1.1 Alfa", "2 Dos", "2.1 Beta", "2.2 Gamma"]),
            pagina(&["1 Otro inicio", "1.1 Delta", "1.2 Épsilon"]),
            pagina(&["1.3 Zeta", "2 Fin"]),
        ];
        let secciones = estructurar_por_numeracion(&paginas).unwrap();
        let parte = secciones
            .iter()
            .find(|s| s.tipo_nodo == TipoNodoIndice::Sintetico)
            .expect("parte");
        assert_eq!(parte.nivel, 1);
        assert_eq!(parte.contenido, "");
        let zeta = secciones.iter().find(|s| s.titulo == "Zeta").unwrap();
        // La parte la genera la ingesta: no entra en el breadcrumb que se vectoriza y se cita
        assert!(!zeta.breadcrumb.contains("Parte"), "{}", zeta.breadcrumb);
        assert!(zeta.breadcrumb.ends_with("1.3 Zeta"), "{}", zeta.breadcrumb);
        assert!(secciones.iter().any(|s| s.titulo == "Fin"));
    }

    #[test]
    fn test_item_de_lista_con_aspecto_de_capitulo_no_finge_reinicio() {
        let paginas = [
            pagina(&[
                "1 Uno",
                "1.1 Alfa",
                "1.2 Beta",
                "2 Falso capítulo",
                "1.3 Gamma",
            ]),
            pagina(&["1.3.1 Delta", "1.3.2 Épsilon", "1.3.3 Zeta", "1.4 Eta"]),
        ];
        let secciones = estructurar_por_numeracion(&paginas).unwrap();
        assert!(
            secciones
                .iter()
                .all(|s| s.tipo_nodo == TipoNodoIndice::Real)
        );
    }

    #[test]
    fn test_falsos_reinicios_no_abren_capitulo() {
        let casos: [(&str, Vec<String>); 5] = [
            ("lista plana", vec![pagina(&["1 Uno", "1.1 Alfa", "2 Dos", "2.1 Beta"]), "1. Primero de lista
texto
2. Segundo de lista
texto
3. Tercero de lista
texto
4. Cuarto de lista".to_string()]),
            ("lista de dos niveles apretada", vec![
                pagina(&["1 Uno", "1.1 Alfa", "2 Dos", "2.1 Beta"]),
                "1.1 Ítem\n1.2 Ítem dos\n1.3 Ítem tres".to_string(),
                "1.4 Ítem cuatro\n1.5 Ítem cinco".to_string(),
            ]),
            ("notas al pie", vec![pagina(&["1 Uno", "1.1 Alfa", "2 Dos", "2.1 Beta"]), "1 Pérez, obra citada\n2 Gómez, obra citada\n3 Ruiz, obra citada".to_string()]),
            ("referencias", vec![pagina(&["1 Uno", "1.1 Alfa", "2 Dos", "2.1 Beta"]), "1. Smith J. Un artículo\n2. Doe A. Otro artículo\n3. Roe B. Tercero\n4. Poe C. Cuarto".to_string()]),
            ("capítulo citado sin numeración detrás", vec![pagina(&["1 Uno", "1.1 Alfa", "2 Dos", "2.1 Beta"]), format!("Capítulo 9: Una remisión\n{}", relleno(5))]),
        ];
        for (caso, paginas) in casos {
            let secciones = estructurar_por_numeracion(&paginas).unwrap();
            assert_eq!(codigos(&secciones), ["1", "1.1", "2", "2.1"], "{caso}");
            let corpus: String = secciones.iter().map(|s| s.contenido.as_str()).collect();
            assert!(
                corpus.contains(paginas[1].lines().next().unwrap()),
                "{caso}: se pierde texto"
            );
        }
    }

    #[test]
    fn test_indice_inicial_con_capitulos_no_abre_capitulos_falsos() {
        let paginas = [
            "Capítulo 1: Primero\n1.1 Alfa\n1.2 Beta\nCapítulo 2: Segundo\n2.1 Gamma\n2.2 Delta"
                .to_string(),
            pagina(&["Capítulo 1: Primero", "1.1 Alfa", "1.2 Beta"]),
            pagina(&[
                "Capítulo 2: Segundo",
                "2.1 Gamma",
                "2.2 Delta",
                "2.3 Épsilon",
            ]),
            relleno(10),
            relleno(10),
        ];
        let secciones = estructurar_por_numeracion(&paginas).unwrap();
        let capitulos = secciones.iter().filter(|s| s.nivel == 1).count();
        assert_eq!(capitulos, 2, "{:?}", codigos(&secciones));
        let alfa = secciones.iter().find(|s| s.titulo == "Alfa").unwrap();
        assert_eq!(alfa.pagina_inicio, Some(2));
    }

    fn seccion_de_paginas(paginas: u32, caracteres_por_pagina: usize) -> ParsedSection {
        let texto: Vec<String> = (1..=paginas)
            .map(|p| format!("P{p:03}{}", "x".repeat(caracteres_por_pagina - 4)))
            .collect();
        estructurar_por_marcadores(
            &texto,
            &[EntradaMarcador {
                titulo: "2.9 Largo".to_string(),
                nivel: 2,
                pagina: Some(1),
            }],
        )
        .remove(0)
    }

    #[test]
    fn test_saltos_de_pagina_apuntan_al_texto_de_cada_pagina() {
        let seccion = seccion_de_paginas(3, 10);
        assert_eq!(seccion.saltos_pagina.len(), 3);
        for salto in &seccion.saltos_pagina {
            assert!(
                seccion.contenido[salto.desplazamiento..]
                    .starts_with(&format!("P{:03}", salto.pagina)),
                "{salto:?}"
            );
        }
    }

    #[test]
    fn test_seccion_larga_se_divide_por_paginas_sin_perder_texto() {
        let real = seccion_de_paginas(23, 2_000);
        let contenido = real.contenido.clone();
        let breadcrumb = real.breadcrumb.clone();
        let secciones = dividir_secciones_largas(vec![ParsedSection {
            condicion_aplicacion: Some("con lluvia".to_string()),
            ..real
        }]);
        assert_eq!(secciones.len(), 4, "{:?}", codigos(&secciones));
        assert_eq!(secciones[0].tipo_nodo, TipoNodoIndice::Real);
        assert_eq!(secciones[0].contenido, "");
        assert_eq!(
            (secciones[0].pagina_inicio, secciones[0].pagina_fin),
            (Some(1), Some(23))
        );
        let hijas = &secciones[1..];
        assert_eq!(hijas[0].codigo, "2.9.PAG1-10");
        assert_eq!(hijas[2].codigo, "2.9.PAG21-23");
        for hija in hijas {
            assert_eq!(hija.tipo_nodo, TipoNodoIndice::Sintetico);
            assert_eq!(hija.nivel, 3);
            assert_eq!(hija.breadcrumb, breadcrumb);
            assert_eq!(hija.condicion_aplicacion.as_deref(), Some("con lluvia"));
            for salto in &hija.saltos_pagina {
                assert!(
                    hija.contenido[salto.desplazamiento..]
                        .starts_with(&format!("P{:03}", salto.pagina))
                );
            }
        }
        let unido = hijas
            .iter()
            .map(|h| h.contenido.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(unido, contenido);
    }

    #[test]
    fn test_fronteras_de_division_son_estrictas() {
        let dividida = |paginas, por_pagina| {
            dividir_secciones_largas(vec![seccion_de_paginas(paginas, por_pagina)]).len() > 1
        };
        assert!(!dividida(15, 5_000), "15 páginas exactas no divide");
        assert!(dividida(16, 5_000));
        // 16 páginas: 15 saltos de línea + texto = 40.000 caracteres exactos
        let exacta = |extra: i64| {
            let total = 40_000 + extra;
            let mut seccion = seccion_de_paginas(16, 100);
            let faltan = total - seccion.contenido.chars().count() as i64;
            seccion.contenido.push_str(&"y".repeat(faltan as usize));
            assert_eq!(seccion.contenido.chars().count() as i64, total);
            dividir_secciones_largas(vec![seccion]).len() > 1
        };
        assert!(!exacta(0), "40.000 caracteres exactos no divide");
        assert!(exacta(1));
        assert!(!dividida(1, 50_000), "PDF sin saltos de página");
    }

    #[test]
    fn test_nivel_maximo_no_se_divide_y_preambulo_si() {
        let mut maxima = seccion_de_paginas(20, 3_000);
        maxima.nivel = MAX_NIVEL;
        assert_eq!(dividir_secciones_largas(vec![maxima]).len(), 1);
        let mut preambulo = seccion_de_paginas(20, 3_000);
        preambulo.nivel = 0;
        preambulo.codigo = CODIGO_PREAMBULO.to_string();
        let secciones = dividir_secciones_largas(vec![preambulo]);
        assert_eq!(secciones[1].nivel, 1);
        assert_eq!(secciones[1].codigo, "0.0.PAG1-10");
    }

    #[test]
    fn test_patron_de_numeracion_acepta_las_formas_reales() {
        assert_eq!(
            clave("1.  DEFINICIONES Y ABREVIATURAS "),
            par("1", "DEFINICIONES Y ABREVIATURAS")
        );
        assert_eq!(
            clave("1.2.1 Tabla de Hechos"),
            par("1.2.1", "Tabla de Hechos")
        );
        assert_eq!(clave("1.2.1. Arquitectura"), par("1.2.1", "Arquitectura"));
        assert_eq!(clave("A.1 Anexo primero"), par("A.1", "Anexo primero"));
        assert_eq!(clave("IX.  CONTENIDO"), par("IX", "CONTENIDO"));
        assert_eq!(clave("3 Ámbito"), par("3", "Ámbito"));
    }

    #[test]
    fn test_patron_de_numeracion_rechaza_lo_demas() {
        for linea in [
            "1. minúscula inicial",
            "1.",
            "A Mayúscula",
            "IX CONTENIDO",
            "IIII. MAL ESCRITO",
            "1..2 Doble punto",
            "2024 Informe",
            "1a. Mixto",
            "Texto 1. Normal",
        ] {
            assert_eq!(clave(linea), None, "{linea}");
        }
    }

    #[test]
    fn test_sucesion_admite_hijos_y_saltos_pequenos() {
        assert!(es_sucesor(None, &[1]));
        assert!(es_sucesor(None, &[1, 1]));
        assert!(!es_sucesor(None, &[7]));
        assert!(es_sucesor(Some(&[1, 4]), &[1, 4, 2]));
        assert!(es_sucesor(Some(&[1, 4, 3]), &[2]));
        assert!(es_sucesor(Some(&[9]), &[BASE_ANEXOS, 1]));
        assert!(!es_sucesor(Some(&[1, 4]), &[1, 4]));
        assert!(!es_sucesor(Some(&[1]), &[5]));
        assert!(!es_sucesor(Some(&[2]), &[3, 9]));
    }

    #[test]
    fn test_apartados_romanos_son_padres_de_la_numeracion() {
        let paginas = ["I. OBJETO\ntexto\nII. CONTENIDO\n1. PRIMERO\n1.1 Hijo".to_string()];
        let secciones = estructurar_por_numeracion(&paginas).unwrap();
        let niveles: Vec<(&str, u8)> = secciones
            .iter()
            .map(|s| (s.codigo.as_str(), s.nivel))
            .collect();
        assert_eq!(niveles, [("I", 1), ("II", 1), ("1", 2), ("1.1", 3)]);
        assert_eq!(
            secciones[3].breadcrumb,
            "II CONTENIDO > 1 PRIMERO > 1.1 Hijo"
        );
    }

    #[test]
    fn test_requiere_ocr_mide_la_media_por_pagina() {
        assert!(requiere_ocr(&[String::new()]));
        assert!(requiere_ocr(&["a".repeat(99), String::new()]));
        assert!(!requiere_ocr(&["a".repeat(100), String::new()]));
    }

    #[test]
    fn test_normalizar_quita_control_y_lineas_en_blanco() {
        assert_eq!(normalizar_pagina("  a\u{1}b  \n \n\t\nc\r\n"), "  ab\nc");
    }

    #[test]
    fn test_linea_con_otra_grafia_que_el_marcador_se_conserva() {
        let marcador = |titulo: &str| EntradaMarcador {
            titulo: titulo.to_string(),
            nivel: 1,
            pagina: Some(1),
        };
        let paginas = ["ALCANCE-GENERAL
texto"
            .to_string()];
        let distinta = estructurar_por_marcadores(&paginas, &[marcador("Alcance general")]);
        assert!(distinta[0].contenido.contains("ALCANCE-GENERAL"));
        let identica = estructurar_por_marcadores(&paginas, &[marcador("ALCANCE-GENERAL")]);
        assert_eq!(identica[0].contenido, "texto");
    }

    #[test]
    fn test_titulo_enorme_de_marcador_queda_en_la_seccion() {
        let titulo = "T".repeat(MAX_CARACTERES_TITULO + 5);
        let secciones = estructurar_por_marcadores(
            &["texto".to_string()],
            &[EntradaMarcador {
                titulo: titulo.clone(),
                nivel: 1,
                pagina: Some(1),
            }],
        );
        assert!(secciones[0].contenido.contains(&titulo));
    }
}
