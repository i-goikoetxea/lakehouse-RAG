//! Compuerta de legibilidad por página (`docs/arquitectura/ssd_generalidad_4_piezas.md` §1.3).
//!
//! Puro, sin I/O. Se aplica al texto **crudo** de cada página, antes de `normalizar_pagina`, que
//! borra los caracteres de control. Solo mide propiedades de Unicode y de los alfabetos; ninguna
//! regla depende de un idioma ni de una lista de palabras.

pub use crate::application::dtos::ingestion::MotivoDegradacion;

/// Por debajo, la página es `SinTexto` y no se evalúa ninguna otra señal
pub const MIN_NO_BLANCOS: usize = 50;
pub const UMBRAL_REEMPLAZO_O_CONTROL: f32 = 0.01;
pub const UMBRAL_USO_PRIVADO: f32 = 0.05;
pub const MIN_USO_PRIVADO_DENTRO_DE_PALABRA: usize = 3;
pub const MIN_TOKENS_EVALUABLES: usize = 80;
pub const UMBRAL_MOJIBAKE: f32 = 0.30;
const MIN_LETRAS_TOKEN_EVALUABLE: usize = 4;
/// Señal de la calibración N1 (no estaba en el SSD): símbolos anómalos entre dos letras
pub const MIN_TOKENS_CON_LETRAS: usize = 40;
/// Por encima, degrada sola
pub const UMBRAL_SIMBOLOS_EN_PALABRAS: f32 = 0.35;
/// Entre este umbral y el anterior, degrada solo si otra señal independiente lo confirma
pub const UMBRAL_SIMBOLOS_ZONA_GRIS: f32 = 0.15;
/// Confirmación de la zona gris: palabras de 4 o más letras sin vocales de su alfabeto
pub const MIN_TOKENS_CONFIRMACION: usize = 20;
pub const UMBRAL_SIN_VOCAL_CONFIRMACION: f32 = 0.15;
/// Incoherencia estructural (decisión del mentor tras la calibración N1)
pub const MIN_NO_BLANCOS_INCOHERENCIA: usize = 80;
pub const MIN_LETRAS_INCOHERENCIA: usize = 20;
pub const MAX_PROPORCION_DIGITOS: f32 = 0.50;
pub const MAX_PROPORCION_CJK: f32 = 0.30;
pub const UMBRAL_COHERENCIA: f32 = 0.40;
pub const UMBRAL_RUIDO: f32 = 0.05;
const MIN_LETRAS_RACHA: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub enum ClasePagina {
    Legible,
    Degradada { motivos: Vec<MotivoDegradacion> },
    SinTexto,
}

/// Conteos de una página. La densidad alfanumérica solo se informa: no clasifica
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetricasPagina {
    pub no_blancos: usize,
    pub reemplazo_o_control: usize,
    pub uso_privado: usize,
    pub uso_privado_dentro_de_palabra: usize,
    pub tokens_evaluables: usize,
    pub tokens_sospechosos: usize,
    pub tokens_con_letras: usize,
    pub tokens_con_simbolos: usize,
    /// Tokens evaluables por la red de vocales que no tienen ninguna
    pub tokens_sin_vocal: usize,
    pub digitos: usize,
    /// Caracteres Han, Kana y Hangul
    pub cjk: usize,
    pub letras_no_cjk: usize,
    /// Letras en rachas de 3 o más letras de una misma escritura
    pub letras_en_rachas: usize,
    /// Símbolos ASCII anómalos (`$ # * = + ; ? ! ( )`…) en la página
    pub simbolos_anomalos: usize,
    /// Signos de Latin-1 y letras de Latin-1 sueltas
    pub ruido_latin1: usize,
    pub densidad_alfanumerica: f32,
}

pub fn clasificar_pagina(texto_crudo: &str) -> (ClasePagina, MetricasPagina) {
    let metricas = medir(texto_crudo);
    if metricas.no_blancos < MIN_NO_BLANCOS {
        return (ClasePagina::SinTexto, metricas);
    }
    let motivos: Vec<MotivoDegradacion> = [
        motivo_reemplazo_o_control(&metricas),
        motivo_uso_privado(&metricas),
        motivo_mojibake(&metricas),
        motivo_simbolos_en_palabras(&metricas),
        motivo_incoherencia(&metricas),
    ]
    .into_iter()
    .flatten()
    .collect();
    let clase = if motivos.is_empty() {
        ClasePagina::Legible
    } else {
        ClasePagina::Degradada { motivos }
    };
    (clase, metricas)
}

fn medir(texto: &str) -> MetricasPagina {
    let caracteres: Vec<char> = texto.chars().collect();
    let no_blancos = caracteres.iter().filter(|c| !c.is_whitespace()).count();
    let alfanumericos = caracteres.iter().filter(|c| c.is_alphanumeric()).count();
    let ConteoTokens {
        evaluables: tokens_evaluables,
        sospechosos: tokens_sospechosos,
        sin_vocal: tokens_sin_vocal,
    } = contar_tokens(texto);
    let (tokens_con_letras, tokens_con_simbolos) = contar_tokens_con_simbolos(texto);
    let (letras_no_cjk, letras_en_rachas) = contar_letras_en_rachas(&caracteres);
    MetricasPagina {
        no_blancos,
        reemplazo_o_control: reemplazos_o_controles_no_aislados(&caracteres),
        uso_privado: caracteres.iter().filter(|c| es_uso_privado(**c)).count(),
        uso_privado_dentro_de_palabra: uso_privado_dentro_de_palabra(&caracteres),
        tokens_evaluables,
        tokens_sospechosos,
        tokens_con_letras,
        tokens_con_simbolos,
        tokens_sin_vocal,
        digitos: caracteres.iter().filter(|c| c.is_numeric()).count(),
        cjk: caracteres.iter().filter(|c| es_cjk(**c)).count(),
        letras_no_cjk,
        letras_en_rachas,
        simbolos_anomalos: caracteres.iter().filter(|c| es_simbolo_suelto(**c)).count(),
        ruido_latin1: ruido_latin1(texto),
        densidad_alfanumerica: proporcion(alfanumericos, no_blancos),
    }
}

fn proporcion(parte: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        parte as f32 / total as f32
    }
}

fn motivo_reemplazo_o_control(metricas: &MetricasPagina) -> Option<MotivoDegradacion> {
    let proporcion = proporcion(metricas.reemplazo_o_control, metricas.no_blancos);
    (proporcion > UMBRAL_REEMPLAZO_O_CONTROL)
        .then_some(MotivoDegradacion::ReemplazoOControl { proporcion })
}

fn motivo_uso_privado(metricas: &MetricasPagina) -> Option<MotivoDegradacion> {
    let proporcion = proporcion(metricas.uso_privado, metricas.no_blancos);
    let dentro_de_palabra = metricas.uso_privado_dentro_de_palabra;
    (proporcion > UMBRAL_USO_PRIVADO || dentro_de_palabra >= MIN_USO_PRIVADO_DENTRO_DE_PALABRA)
        .then_some(MotivoDegradacion::UsoPrivado {
            proporcion,
            dentro_de_palabra,
        })
}

fn motivo_mojibake(metricas: &MetricasPagina) -> Option<MotivoDegradacion> {
    if metricas.tokens_evaluables < MIN_TOKENS_EVALUABLES {
        return None;
    }
    let proporcion = proporcion(metricas.tokens_sospechosos, metricas.tokens_evaluables);
    (proporcion > UMBRAL_MOJIBAKE).then_some(MotivoDegradacion::Mojibake {
        proporcion,
        tokens_evaluados: metricas.tokens_evaluables,
    })
}

/// Por encima de `UMBRAL_SIMBOLOS_EN_PALABRAS`, degrada sola. En la zona gris, solo si lo confirma
/// una señal independiente: palabras sin vocales o el colapso de las palabras coherentes
fn motivo_simbolos_en_palabras(metricas: &MetricasPagina) -> Option<MotivoDegradacion> {
    if metricas.tokens_con_letras < MIN_TOKENS_CON_LETRAS {
        return None;
    }
    let proporcion = proporcion(metricas.tokens_con_simbolos, metricas.tokens_con_letras);
    let confirmada = proporcion > UMBRAL_SIMBOLOS_ZONA_GRIS
        && (hay_palabras_sin_vocales(metricas) || colapsan_las_palabras(metricas));
    (proporcion > UMBRAL_SIMBOLOS_EN_PALABRAS || confirmada).then_some(
        MotivoDegradacion::SimbolosEnPalabras {
            proporcion,
            tokens_evaluados: metricas.tokens_con_letras,
        },
    )
}

fn hay_palabras_sin_vocales(metricas: &MetricasPagina) -> bool {
    metricas.tokens_evaluables >= MIN_TOKENS_CONFIRMACION
        && proporcion(metricas.tokens_sin_vocal, metricas.tokens_evaluables)
            > UMBRAL_SIN_VOCAL_CONFIRMACION
}

/// La incoherencia solo se juzga en páginas con texto suficiente, que no sean sobre todo cifras
/// (tablas) ni sobre todo CJK (que no separa palabras)
fn incoherencia_evaluable(metricas: &MetricasPagina) -> bool {
    metricas.no_blancos >= MIN_NO_BLANCOS_INCOHERENCIA
        && metricas.letras_no_cjk >= MIN_LETRAS_INCOHERENCIA
        && proporcion(metricas.digitos, metricas.no_blancos) < MAX_PROPORCION_DIGITOS
        && proporcion(metricas.cjk, metricas.no_blancos) < MAX_PROPORCION_CJK
}

fn coherencia(metricas: &MetricasPagina) -> f32 {
    proporcion(metricas.letras_en_rachas, metricas.letras_no_cjk)
}

fn colapsan_las_palabras(metricas: &MetricasPagina) -> bool {
    incoherencia_evaluable(metricas) && coherencia(metricas) < UMBRAL_COHERENCIA
}

/// Las letras no forman palabras y, además, abundan los símbolos sueltos o el ruido de Latin-1
fn motivo_incoherencia(metricas: &MetricasPagina) -> Option<MotivoDegradacion> {
    if !colapsan_las_palabras(metricas) {
        return None;
    }
    let ruido = proporcion(metricas.simbolos_anomalos, metricas.no_blancos)
        .max(proporcion(metricas.ruido_latin1, metricas.no_blancos));
    (ruido >= UMBRAL_RUIDO).then_some(MotivoDegradacion::IncoherenciaEstructural {
        coherencia: coherencia(metricas),
        ruido,
    })
}

/// Reemplazos y controles que tocan algo que no es un blanco. Uno aislado entre blancos (el
/// delimitador de una fuente matemática sin mapa Unicode en su propia línea) no daña ninguna
/// palabra; una racha de controles seguidos sí cuenta, porque cada uno toca al siguiente
fn reemplazos_o_controles_no_aislados(caracteres: &[char]) -> usize {
    let es_blanco_o_borde = |indice: Option<usize>| {
        indice
            .and_then(|i| caracteres.get(i))
            // Algunos controles (`U+000C`, `U+0085`) son blancos para Unicode: no aíslan a otro
            .is_none_or(|c| c.is_whitespace() && !es_reemplazo_o_control(*c))
    };
    (0..caracteres.len())
        .filter(|&i| es_reemplazo_o_control(caracteres[i]))
        .filter(|&i| !(es_blanco_o_borde(i.checked_sub(1)) && es_blanco_o_borde(Some(i + 1))))
        .count()
}

/// `U+FFFD` y controles C0 y C1, salvo el salto de línea, el retorno y el tabulador
fn es_reemplazo_o_control(caracter: char) -> bool {
    match caracter {
        '\n' | '\r' | '\t' => false,
        '\u{FFFD}' | '\u{0000}'..='\u{001F}' | '\u{0080}'..='\u{009F}' => true,
        _ => false,
    }
}

/// Área de uso privado del plano básico
fn es_uso_privado(caracter: char) -> bool {
    ('\u{E000}'..='\u{F8FF}').contains(&caracter)
}

/// Tramos de uso privado con un alfanumérico justo antes y justo después
fn uso_privado_dentro_de_palabra(caracteres: &[char]) -> usize {
    let mut apariciones = 0;
    let mut indice = 0;
    while indice < caracteres.len() {
        if !es_uso_privado(caracteres[indice]) {
            indice += 1;
            continue;
        }
        let inicio = indice;
        while indice < caracteres.len() && es_uso_privado(caracteres[indice]) {
            indice += 1;
        }
        let antes = inicio
            .checked_sub(1)
            .is_some_and(|i| caracteres[i].is_alphanumeric());
        let despues = caracteres.get(indice).is_some_and(|c| c.is_alphanumeric());
        if antes && despues {
            apariciones += 1;
        }
    }
    apariciones
}

// --- Red contra mojibake ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Alfabeto {
    Latino,
    Griego,
    Cirilico,
}

/// Bloques Unicode de los tres alfabetos con vocales escritas que evalúa la red
fn alfabeto_de(letra: char) -> Option<Alfabeto> {
    match letra as u32 {
        0x0041..=0x024F
        | 0x0250..=0x02AF
        | 0x1E00..=0x1EFF
        | 0x2C60..=0x2C7F
        | 0xA720..=0xA7FF
        | 0xAB30..=0xAB6F
        | 0xFB00..=0xFB06 => Some(Alfabeto::Latino),
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Some(Alfabeto::Griego),
        0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => {
            Some(Alfabeto::Cirilico)
        }
        _ => None,
    }
}

/// Vocales de cada alfabeto, sin diacrítico. En el latino se añaden las letras vocálicas que no
/// se descomponen (`æ`, `ø`, `œ`, `ı`, `ə`, `ɛ`, `ɔ`); en el cirílico, `ъ`, que es vocal en
/// búlgaro. Es una propiedad del sistema de escritura, no de un idioma (SSD §1.3)
fn es_vocal_base(alfabeto: Alfabeto, base: char) -> bool {
    match alfabeto {
        Alfabeto::Latino => "aeiouyæøœıəɛɔ".contains(base),
        Alfabeto::Griego => "αεηιουω".contains(base),
        Alfabeto::Cirilico => "аеёиоуыэюяіїєъ".contains(base),
    }
}

/// Una letra es vocal si alguna letra de su descomposición de compatibilidad (NFKD) lo es: así
/// cuentan los diacríticos (`ą`, `ώ`, `ї`) y las ligaduras (`ﬁ`)
fn es_vocal(alfabeto: Alfabeto, letra: char) -> bool {
    use unicode_normalization::char::decompose_compatible;
    let mut vocal = false;
    decompose_compatible(letra, |base| {
        vocal |= base
            .to_lowercase()
            .any(|minuscula| es_vocal_base(alfabeto, minuscula));
    });
    vocal
}

/// Separadores interiores que delatan correos, URL y códigos: el token no se evalúa
fn excluye_token(caracter: char) -> bool {
    matches!(caracter, '@' | '/' | ':' | '_' | '.')
}

/// Símbolos ASCII que no unen dos letras de una palabra real. Los conectores (`-`, `/`, `_`, `.`,
/// `'`, `:`, `,`) quedan fuera: unen palabras en rutas, códigos, unidades y abreviaturas
fn es_simbolo_entre_letras(caracter: char) -> bool {
    matches!(
        caracter,
        '$' | '@'
            | '#'
            | '*'
            | '~'
            | '\\'
            | '|'
            | '{'
            | '}'
            | '['
            | ']'
            | '^'
            | '='
            | '+'
            | ';'
            | '?'
            | '!'
            | '('
            | ')'
    )
}

/// Símbolos ASCII que cuentan como ruido de página en la incoherencia estructural
fn es_simbolo_suelto(caracter: char) -> bool {
    es_simbolo_entre_letras(caracter) || matches!(caracter, '<' | '>' | '"' | '%' | '&')
}

/// Tokens con al menos dos caracteres y alguna letra (cualquier caja y escritura) y, de ellos,
/// los que tienen un símbolo anómalo entre dos letras (`th=s`, `f(x)`). Tipografía, no palabras
fn contar_tokens_con_simbolos(texto: &str) -> (usize, usize) {
    let mut con_letras = 0;
    let mut con_simbolos = 0;
    for token in texto.split_whitespace() {
        let recortado = token.trim_matches(|c: char| c.is_ascii_punctuation());
        if recortado.chars().count() < 2 || !recortado.chars().any(char::is_alphabetic) {
            continue;
        }
        con_letras += 1;
        let caracteres: Vec<char> = recortado.chars().collect();
        con_simbolos += usize::from(caracteres.windows(3).any(|v| {
            v[0].is_alphabetic() && es_simbolo_entre_letras(v[1]) && v[2].is_alphabetic()
        }));
    }
    (con_letras, con_simbolos)
}

// --- Incoherencia estructural ---

/// Han, Kana y Hangul, por bloques Unicode
fn es_cjk(caracter: char) -> bool {
    matches!(caracter as u32,
        0x3040..=0x30FF
        | 0x31F0..=0x31FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0x1100..=0x11FF
        | 0x3130..=0x318F
        | 0xAC00..=0xD7AF
        | 0x20000..=0x2FA1F)
}

/// Escritura de una letra: latín, griego, cirílico y árabe por sus bloques; el resto, por su
/// bloque de 128 puntos de código (cada escritura índica ocupa uno)
fn escritura_de(caracter: char) -> u32 {
    match (alfabeto_de(caracter), caracter as u32) {
        (Some(Alfabeto::Latino), _) => 1,
        (Some(Alfabeto::Griego), _) => 2,
        (Some(Alfabeto::Cirilico), _) => 3,
        (
            None,
            0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF,
        ) => 4,
        (None, punto) => 0x100 + (punto >> 7),
    }
}

/// Signos de Latin-1 (`U+00A1..U+00BF` salvo `°` y `±`) y `×` `÷`
fn es_signo_latin1(caracter: char) -> bool {
    matches!(caracter, '\u{00A1}'..='\u{00BF}' | '×' | '÷') && !matches!(caracter, '°' | '±')
}

/// Signos de Latin-1 y letras de Latin-1 (`U+00C0..U+00FF`) sueltas: en un token de 1 o 2
/// caracteres o pegadas a algo que no es alfanumérico
fn ruido_latin1(texto: &str) -> usize {
    let mut ruido = 0;
    for token in texto.split_whitespace() {
        let caracteres: Vec<char> = token.chars().collect();
        for (i, caracter) in caracteres.iter().enumerate() {
            if es_signo_latin1(*caracter) {
                ruido += 1;
                continue;
            }
            let letra_latin1 =
                ('\u{00C0}'..='\u{00FF}').contains(caracter) && caracter.is_alphabetic();
            let vecinos = [i.checked_sub(1), Some(i + 1)]
                .into_iter()
                .flatten()
                .filter_map(|j| caracteres.get(j));
            let pegada_a_simbolo = vecinos.into_iter().any(|v| !v.is_alphanumeric());
            if letra_latin1 && (caracteres.len() <= 2 || pegada_a_simbolo) {
                ruido += 1;
            }
        }
    }
    ruido
}

/// Letras fuera de CJK y, de ellas, las que forman rachas de 3 o más letras seguidas de una misma
/// escritura. Las marcas que no son letras (el virama índico) prolongan la racha si son de la
/// misma escritura y no son ASCII
fn contar_letras_en_rachas(caracteres: &[char]) -> (usize, usize) {
    let mut letras = 0;
    let mut en_rachas = 0;
    let mut racha = 0;
    let mut escritura = 0;
    let mut cerrar = |racha: &mut usize| {
        if *racha >= MIN_LETRAS_RACHA {
            en_rachas += *racha;
        }
        *racha = 0;
    };
    for caracter in caracteres {
        if caracter.is_alphabetic() && !es_cjk(*caracter) {
            letras += 1;
            let propia = escritura_de(*caracter);
            if racha > 0 && propia != escritura {
                cerrar(&mut racha);
            }
            escritura = propia;
            racha += 1;
        } else {
            let marca = racha > 0
                && !caracter.is_ascii()
                && !caracter.is_whitespace()
                && !caracter.is_alphanumeric()
                && !es_signo_latin1(*caracter)
                && escritura_de(*caracter) == escritura;
            if !marca {
                cerrar(&mut racha);
            }
        }
    }
    cerrar(&mut racha);
    (letras, en_rachas)
}

/// Puntuación ASCII que no aparece dentro de una palabra real (`-` y `'` sí aparecen)
fn es_puntuacion_anomala(caracter: char) -> bool {
    caracter.is_ascii_punctuation() && caracter != '-' && caracter != '\''
}

#[derive(Default)]
struct ConteoTokens {
    evaluables: usize,
    sospechosos: usize,
    sin_vocal: usize,
}

fn contar_tokens(texto: &str) -> ConteoTokens {
    let mut conteo = ConteoTokens::default();
    for token in texto.split_whitespace() {
        let recortado = token.trim_matches(|c: char| !c.is_alphanumeric());
        if let Some(evaluacion) = evaluar_token(recortado) {
            conteo.evaluables += 1;
            conteo.sospechosos +=
                usize::from(evaluacion.sin_vocal || evaluacion.puntuacion_interior);
            conteo.sin_vocal += usize::from(evaluacion.sin_vocal);
        }
    }
    conteo
}

struct EvaluacionToken {
    sin_vocal: bool,
    puntuacion_interior: bool,
}

/// `None` si el token no es evaluable; si lo es, sus dos rasgos sospechosos
fn evaluar_token(token: &str) -> Option<EvaluacionToken> {
    if token.chars().any(|c| c.is_numeric() || excluye_token(c)) {
        return None;
    }
    let letras: Vec<char> = token.chars().filter(|c| c.is_alphabetic()).collect();
    if letras.len() < MIN_LETRAS_TOKEN_EVALUABLE || letras.iter().any(|c| !c.is_lowercase()) {
        return None;
    }
    let alfabeto = alfabeto_de(letras[0])?;
    if letras.iter().any(|c| alfabeto_de(*c) != Some(alfabeto)) {
        return None;
    }
    Some(EvaluacionToken {
        sin_vocal: !letras.iter().any(|c| es_vocal(alfabeto, *c)),
        puntuacion_interior: token.chars().any(es_puntuacion_anomala),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repetir(frase: &str, veces: usize) -> String {
        vec![frase; veces].join("\n")
    }

    fn es_legible(texto: &str) -> bool {
        matches!(clasificar_pagina(texto).0, ClasePagina::Legible)
    }

    fn motivos(texto: &str) -> Vec<MotivoDegradacion> {
        match clasificar_pagina(texto).0 {
            ClasePagina::Degradada { motivos } => motivos,
            otra => panic!("se esperaba Degradada y salió {otra:?}"),
        }
    }

    // Prosa redactada para el test en cada idioma; no procede de ningún documento real
    const PROSA_ES: &str = "La comisión revisará cada solicitud antes de aprobar el presupuesto \
        anual, y después publicará una nota breve con las razones de su decisión para que \
        cualquier persona interesada pueda consultarla.";
    const PROSA_EN: &str = "The committee will review every request before approving the yearly \
        budget, and afterwards it will publish a short note explaining the reasons behind its \
        decision so that anyone interested can read them.";
    const PROSA_DE: &str = "Die Kommission prüft jeden Antrag gründlich, bevor sie den jährlichen \
        Haushalt genehmigt, und veröffentlicht danach eine kurze Mitteilung über die Gründe \
        ihrer Entscheidung für alle interessierten Bürger.";
    const PROSA_PL: &str = "Komisja dokładnie sprawdzi każdy wniosek przed zatwierdzeniem \
        rocznego budżetu, a następnie opublikuje krótką notatkę wyjaśniającą powody swojej \
        decyzji, żeby każdy zainteresowany mógł się z nią zapoznać.";
    const PROSA_CS: &str = "Komise pečlivě prověří každou žádost, než schválí roční rozpočet; \
        vlk utekl přes krk hory a smrt prstu nikoho nezajímala, ale potom zveřejní krátké \
        oznámení s důvody svého rozhodnutí pro všechny zájemce.";
    const PROSA_EL: &str = "Η επιτροπή θα εξετάσει προσεκτικά κάθε αίτηση πριν εγκρίνει τον \
        ετήσιο προϋπολογισμό και έπειτα θα δημοσιεύσει ένα σύντομο σημείωμα που εξηγεί τους \
        λόγους της απόφασής της για όλους τους ενδιαφερόμενους πολίτες.";
    const PROSA_RU: &str = "Комиссия внимательно рассмотрит каждую заявку перед утверждением \
        годового бюджета, а затем опубликует короткую записку с объяснением причин своего \
        решения для всех заинтересованных граждан страны.";
    const PROSA_BG: &str = "Комисията ще прегледа внимателно всяко искане преди да одобри \
        годишния бюджет, след което ще публикува кратка бележка с причините за своето решение \
        за всички заинтересовани граждани, дори в дъжд.";
    const PROSA_JA: &str = "委員会は年間予算を承認する前にすべての申請を慎重に審査し、その後、\
        関心のあるすべての人が読めるように決定の理由を説明する短い通知を公表する。";
    const PROSA_ZH: &str = "委员会将在批准年度预算之前认真审查每一项申请，随后发布一份简短的\
        说明，解释其决定的理由，以便所有感兴趣的人都能查阅。";
    const PROSA_AR: &str = "ستقوم اللجنة بمراجعة كل طلب بعناية قبل الموافقة على الميزانية \
        السنوية، ثم ستنشر مذكرة قصيرة توضح أسباب قرارها لجميع المهتمين.";
    const PROSA_HI: &str = "समिति वार्षिक बजट को मंजूरी देने से पहले हर आवेदन की सावधानी से \
        जांच करेगी और फिर अपने निर्णय के कारणों को समझाने वाला एक छोटा नोट प्रकाशित करेगी।";

    #[test]
    fn test_la_prosa_de_alfabetos_con_vocales_es_legible() {
        for prosa in [
            PROSA_ES, PROSA_EN, PROSA_DE, PROSA_PL, PROSA_CS, PROSA_EL, PROSA_RU, PROSA_BG,
        ] {
            let texto = repetir(prosa, 8);
            let (clase, metricas) = clasificar_pagina(&texto);
            assert!(
                metricas.tokens_evaluables >= MIN_TOKENS_EVALUABLES,
                "la prosa debe ser evaluable: {metricas:?}"
            );
            assert_eq!(clase, ClasePagina::Legible, "{prosa}: {metricas:?}");
        }
    }

    #[test]
    fn test_las_escrituras_sin_red_de_mojibake_son_legibles_y_no_evaluables() {
        for prosa in [PROSA_JA, PROSA_ZH, PROSA_AR, PROSA_HI] {
            let texto = repetir(prosa, 10);
            let (clase, metricas) = clasificar_pagina(&texto);
            assert_eq!(metricas.tokens_evaluables, 0, "{prosa}");
            assert_eq!(clase, ClasePagina::Legible, "{prosa}");
        }
    }

    #[test]
    fn test_las_formulas_son_legibles() {
        let formulas = repetir(
            "Sea ∑ᵢ xᵢ ≤ α·β con x₁, x₂ ∈ ℝ y ∀ε > 0 ∃δ: |f(x) − f(y)| < ε; entonces \
             ∫₀¹ g(t) dt = π²/6 y λ_max ≥ σ₂(A).",
            12,
        );
        assert!(es_legible(&formulas));
    }

    #[test]
    fn test_una_tabla_financiera_numerica_es_legible() {
        let fila = "│ 2026-T1 │ 1.234.567,89 │ −45.210,00 │ 12,5 % │ 98 765 │";
        let tabla = format!(
            "┌─────────┬──────────────┐\n{}\n└─────────┴──────────────┘",
            repetir(fila, 30)
        );
        assert!(es_legible(&tabla));
    }

    #[test]
    fn test_iconos_de_uso_privado_escasos_y_fuera_de_palabras_son_legibles() {
        let texto = format!("\u{E001} {}\n\u{F0B7} {}", repetir(PROSA_ES, 4), PROSA_EN);
        let (clase, metricas) = clasificar_pagina(&texto);
        assert!(metricas.uso_privado > 0);
        assert_eq!(clase, ClasePagina::Legible);
    }

    /// Sustitución que quita las vocales: el patrón de una fuente con la codificación cambiada
    fn cifrar(texto: &str) -> String {
        texto
            .chars()
            .map(|c| match c {
                'a' | 'á' => 'q',
                'e' | 'é' => 'x',
                'i' | 'í' => 'z',
                'o' | 'ó' => 'k',
                'u' | 'ú' => 'w',
                otro => otro,
            })
            .collect()
    }

    #[test]
    fn test_la_prosa_cifrada_por_sustitucion_se_degrada_por_mojibake() {
        for prosa in [PROSA_ES, PROSA_EN] {
            let motivos = motivos(&repetir(&cifrar(prosa), 8));
            assert!(
                motivos
                    .iter()
                    .any(|m| matches!(m, MotivoDegradacion::Mojibake { proporcion, .. } if *proporcion > 0.3)),
                "{motivos:?}"
            );
        }
    }

    #[test]
    fn test_la_puntuacion_ascii_dentro_de_las_palabras_se_degrada_por_mojibake() {
        let palabras: String = PROSA_EN
            .split_whitespace()
            .map(|palabra| {
                let letras: Vec<char> = palabra.chars().collect();
                if letras.len() >= 4 {
                    format!(
                        "{}#{}",
                        letras[..2].iter().collect::<String>(),
                        letras[3..].iter().collect::<String>()
                    )
                } else {
                    palabra.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let motivos = motivos(&repetir(&palabras, 8));
        // `#` dentro de la palabra también es un símbolo incrustado: pueden salir los dos motivos
        assert!(
            motivos
                .iter()
                .any(|m| matches!(m, MotivoDegradacion::Mojibake { .. })),
            "{motivos:?}"
        );
    }

    #[test]
    fn test_reemplazo_y_controles_por_encima_del_umbral_degradan() {
        let base = repetir(PROSA_ES, 4);
        let no_blancos = base.chars().filter(|c| !c.is_whitespace()).count();
        let cuantos = no_blancos / 50; // 2 %
        for intruso in ['\u{FFFD}', '\u{0085}', '\u{0007}'] {
            let texto = format!("{base}{}", intruso.to_string().repeat(cuantos));
            let motivos = motivos(&texto);
            assert!(
                matches!(motivos.as_slice(), [MotivoDegradacion::ReemplazoOControl { proporcion }] if *proporcion > 0.01),
                "{intruso:?}: {motivos:?}"
            );
        }
    }

    #[test]
    fn test_los_saltos_de_linea_retornos_y_tabuladores_no_son_controles() {
        let texto = repetir(PROSA_ES, 4)
            .replace(' ', "\t")
            .replace('\n', "\r\n\r\n");
        let (clase, metricas) = clasificar_pagina(&texto);
        assert_eq!(metricas.reemplazo_o_control, 0);
        assert_eq!(clase, ClasePagina::Legible);
    }

    #[test]
    fn test_un_reemplazo_aislado_no_degrada() {
        let texto = format!("{}\u{FFFD}", repetir(PROSA_EN, 4));
        assert!(es_legible(&texto));
    }

    #[test]
    fn test_el_uso_privado_abundante_degrada() {
        let base = repetir(PROSA_ES, 4);
        let no_blancos = base.chars().filter(|c| !c.is_whitespace()).count();
        let iconos = " \u{E010}".repeat(no_blancos * 8 / 100);
        let motivos = motivos(&format!("{base}{iconos}"));
        assert!(
            matches!(motivos.as_slice(), [MotivoDegradacion::UsoPrivado { proporcion, .. }] if *proporcion > 0.05),
            "{motivos:?}"
        );
    }

    #[test]
    fn test_el_uso_privado_dentro_de_las_palabras_degrada_aunque_sea_escaso() {
        let texto = format!(
            "{} pa\u{E021}abra ot\u{E022}\u{E023}a cu\u{E024}rta qu\u{E025}nta se\u{E026}ta",
            repetir(PROSA_ES, 4)
        );
        let motivos = motivos(&texto);
        assert!(
            matches!(
                motivos.as_slice(),
                [MotivoDegradacion::UsoPrivado {
                    dentro_de_palabra: 5,
                    ..
                }]
            ),
            "{motivos:?}"
        );
    }

    #[test]
    fn test_una_pagina_corta_no_tiene_texto() {
        let (clase, metricas) = clasificar_pagina("Página 7 de 120 — anexo técnico");
        assert!(metricas.no_blancos < MIN_NO_BLANCOS);
        assert_eq!(clase, ClasePagina::SinTexto);
        assert_eq!(clasificar_pagina("").0, ClasePagina::SinTexto);
        assert_eq!(clasificar_pagina(" \n\t\r\n ").0, ClasePagina::SinTexto);
    }

    #[test]
    fn test_una_pagina_corta_con_reemplazos_no_tiene_texto_y_no_se_degrada() {
        assert_eq!(
            clasificar_pagina("\u{FFFD}\u{FFFD} x").0,
            ClasePagina::SinTexto
        );
    }

    #[test]
    fn test_correos_urls_y_codigos_no_son_tokens_evaluables() {
        let texto = repetir("abcd@xyzw.qrst pqrs/tvwx bcdf:ghjk mnpq_rstv wxyz.bcdf", 30);
        let (clase, metricas) = clasificar_pagina(&texto);
        assert_eq!(metricas.tokens_evaluables, 0);
        assert_eq!(clase, ClasePagina::Legible);
    }

    #[test]
    fn test_las_mayusculas_digitos_y_mezclas_de_alfabeto_no_son_evaluables() {
        let texto = repetir("BCDFG Bcdfg bcd4fg bcdfα xyzпр", 30);
        let (_, metricas) = clasificar_pagina(&texto);
        assert_eq!(metricas.tokens_evaluables, 0);
    }

    #[test]
    fn test_las_ligaduras_y_los_diacriticos_cuentan_sus_vocales() {
        // ﬁ (U+FB01) se descompone en «fi»; ø, æ y œ son vocales sin descomposición
        let texto = repetir(
            "ﬁrst ﬂyer søster ærlig œuvre ﬁnal ūdens čtyři žluť ąęół",
            12,
        );
        let (clase, metricas) = clasificar_pagina(&texto);
        assert!(
            metricas.tokens_evaluables >= MIN_TOKENS_EVALUABLES,
            "{metricas:?}"
        );
        assert_eq!(metricas.tokens_sospechosos, 0, "{metricas:?}");
        assert_eq!(clase, ClasePagina::Legible);
    }

    #[test]
    fn test_pocas_palabras_evaluables_no_activan_la_red_de_mojibake() {
        let texto = repetir(&cifrar(PROSA_ES), 2);
        let (clase, metricas) = clasificar_pagina(&texto);
        assert!(metricas.tokens_evaluables < MIN_TOKENS_EVALUABLES);
        assert_eq!(clase, ClasePagina::Legible);
    }

    #[test]
    fn test_la_densidad_alfanumerica_solo_se_informa() {
        let tabla = repetir("── ── ── ── │ │ │ ── ── ──", 20);
        let (clase, metricas) = clasificar_pagina(&tabla);
        assert!(metricas.densidad_alfanumerica < 0.01);
        assert_eq!(clase, ClasePagina::Legible);
    }

    /// Calibración N1: los delimitadores de una fuente matemática sin mapa Unicode salen como
    /// controles sueltos en su propia línea. No dañan ninguna palabra
    #[test]
    fn test_los_controles_aislados_entre_blancos_no_degradan() {
        let base = repetir(PROSA_EN, 4);
        let no_blancos = base.chars().filter(|c| !c.is_whitespace()).count();
        let delimitadores = "\n\u{000C}\n".repeat(no_blancos * 3 / 100);
        let (clase, metricas) = clasificar_pagina(&format!("{base}{delimitadores}"));
        assert_eq!(metricas.reemplazo_o_control, 0, "{metricas:?}");
        assert_eq!(clase, ClasePagina::Legible);
    }

    #[test]
    fn test_los_controles_pegados_a_una_palabra_si_degradan() {
        let base = repetir(PROSA_EN, 4);
        let rotas: String = base
            .split_whitespace()
            .enumerate()
            .map(|(i, palabra)| {
                if i % 8 == 0 {
                    format!("de\u{000C}ne{palabra}")
                } else {
                    palabra.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let motivos = motivos(&rotas);
        assert!(
            matches!(
                motivos.as_slice(),
                [MotivoDegradacion::ReemplazoOControl { .. }]
            ),
            "{motivos:?}"
        );
    }

    /// Calibración N1: una fuente sin `ToUnicode` produce texto como `O'H)E9 1#+$0#*`, con
    /// mayúsculas, dígitos y símbolos: ningún token es evaluable por la red de vocales
    fn basura_de_simbolos() -> String {
        repetir(
            "O'H)E9 1#+$0#* .=##4 /,'0$ 3' H.53<, 3' \"0$%$)D . 3' +.13D8 N4'4<( J.*4$ \
             3# )$+H#3 \"$)#,H'4<\"( 0 ,'%\"40# .+. 0 /$)3#0$+<3$= \"$\"4$(3..; ,'%\"40$",
            6,
        )
    }

    fn es_incoherente(motivos: &[MotivoDegradacion]) -> bool {
        motivos
            .iter()
            .any(|m| matches!(m, MotivoDegradacion::IncoherenciaEstructural { .. }))
    }

    #[test]
    fn test_la_basura_de_simbolos_y_digitos_se_degrada_por_incoherencia() {
        let motivos = motivos(&basura_de_simbolos());
        assert!(es_incoherente(&motivos), "{motivos:?}");
    }

    /// Calibración N1: una fuente CJK sin `ToUnicode` sale como letras sueltas de Latin-1 y
    /// símbolos, sin un solo token evaluable por las demás señales
    fn basura_cjk() -> String {
        repetir(
            "# # fl § é è, _ É Ñ d $ â ( , ) Ü áA f ‘ * D + Ô d ¢ £, ; , l - . Õ Œ I / 0 1 ~ \
             2A Æ ) G k # # Ü á, ; D $ ¶ Å Æ ) I Ü á « », T Å B C m ïA # D $ ¶ Å Ü á « »",
            6,
        )
    }

    #[test]
    fn test_el_cjk_corrupto_se_degrada_por_incoherencia() {
        let (clase, metricas) = clasificar_pagina(&basura_cjk());
        assert_eq!(metricas.tokens_evaluables, 0, "las demás señales no lo ven");
        let ClasePagina::Degradada { motivos } = clase else {
            panic!("se esperaba Degradada: {metricas:?}");
        };
        assert!(es_incoherente(&motivos), "{motivos:?}");
    }

    #[test]
    fn test_el_cjk_real_no_se_evalua_por_incoherencia() {
        for prosa in [PROSA_JA, PROSA_ZH] {
            let (clase, metricas) = clasificar_pagina(&repetir(prosa, 10));
            assert!(metricas.cjk * 10 > metricas.no_blancos * 3, "{metricas:?}");
            assert_eq!(clase, ClasePagina::Legible);
        }
    }

    #[test]
    fn test_una_pagina_mayoritariamente_numerica_no_se_evalua_por_incoherencia() {
        let tabla = repetir(
            "| 1.234,5 | (67,8) | = 9 | +10 | 11; 12 | ¿13? | ×14 | ÷15 | ±16 |",
            20,
        );
        let (clase, metricas) = clasificar_pagina(&tabla);
        assert!(metricas.digitos * 2 >= metricas.no_blancos, "{metricas:?}");
        assert_eq!(clase, ClasePagina::Legible);
    }

    /// Textos legítimos con signos dentro de los tokens: ninguno debe degradarse
    #[test]
    fn test_software_codigos_unidades_y_referencias_legales_no_degradan() {
        let muestras = [
            "use crate::domain::models::center::CenterId;\n\
             pub fn clasificar(texto: &str) -> Option<Vec<u32>> {\n\
             let mut paginas = Vec::new();\n\
             for (i, linea) in texto.lines().enumerate() {\n\
             if linea.is_empty() { continue; }\n\
             paginas.push(u32::try_from(i + 1).ok()?);\n}\nSome(paginas)\n}\n\
             // src/domain/mod.rs y src/infrastructure/parsers/pdf_parser.rs\n\
             fn main() { let x = foo(bar, baz[2]) + qux * 3; println!(\"{x}\"); }",
            "Referencias: PTF-HID-10, MIL-STD-882E, ISO 9001:2015, EN-1090-2, PO-ALM-03 Rev. 04, \
             3.2.1(b), A.2-4, IEC 61508-3:2010 y DIN EN ISO 13849-1.",
            "Glucosa 98 mg/dL; creatinina 0,9 mg/dL; potasio 4,1 mmol/L; Hb 13,5 g/dL; TSH 2,1 \
             µUI/mL (VR 0,4-4,0); PA 120/80 mmHg; FC 72 lpm; SatO2 98 %.",
            "Conforme al Art.14(a) y al art. 3.2.b) de la Ley 31/1995, y según el §21 BGB, \
             Art.7(1)(c) RGPD, el responsable (art. 24) deberá notificarlo a la autoridad.",
            "Sea f(x) = a+b con x=1, y R&D del 12% en km/h; el valor 2,487,862 (2026-T1) y la \
             referencia A.2-4 de e-mail usuario@dominio.org en http://ejemplo.org/a_b cumple que \
             la norma aplica sin excepciones durante todo el periodo anual",
        ];
        for muestra in muestras {
            let texto = repetir(muestra, 8);
            let (clase, metricas) = clasificar_pagina(&texto);
            assert_eq!(clase, ClasePagina::Legible, "{muestra}\n{metricas:?}");
        }
    }

    /// Prosa con `=` entre letras en una de cada `cada` palabras largas. Esas palabras empiezan
    /// en mayúscula para que la red de vocales (regla b del SSD) no las cuente también
    fn prosa_con_simbolos(cada: usize, veces: usize) -> String {
        let palabras: Vec<String> = repetir(PROSA_EN, veces)
            .split_whitespace()
            .enumerate()
            .map(|(i, palabra)| {
                let letras: Vec<char> = palabra.chars().collect();
                if i % cada == 0 && letras.len() >= 4 && letras.iter().all(|c| c.is_alphabetic()) {
                    let mitad = letras.len() / 2;
                    format!(
                        "{}={}",
                        letras[..mitad]
                            .iter()
                            .enumerate()
                            .map(|(j, c)| if j == 0 { c.to_ascii_uppercase() } else { *c })
                            .collect::<String>(),
                        letras[mitad..].iter().collect::<String>()
                    )
                } else {
                    palabra.to_string()
                }
            })
            .collect();
        palabras.join(" ")
    }

    #[test]
    fn test_simbolos_en_zona_gris_sin_otra_senal_no_degradan() {
        let texto = prosa_con_simbolos(3, 6);
        let (clase, metricas) = clasificar_pagina(&texto);
        let proporcion = proporcion(metricas.tokens_con_simbolos, metricas.tokens_con_letras);
        assert!(
            (UMBRAL_SIMBOLOS_ZONA_GRIS..=UMBRAL_SIMBOLOS_EN_PALABRAS).contains(&proporcion),
            "{proporcion} {metricas:?}"
        );
        assert_eq!(clase, ClasePagina::Legible, "{metricas:?}");
    }

    #[test]
    fn test_simbolos_en_zona_gris_con_palabras_sin_vocales_degradan() {
        // Pocas palabras evaluables: la red de mojibake no actúa por sí sola, pero confirma
        let base = prosa_con_simbolos(3, 2);
        let texto: String = base
            .split_whitespace()
            .enumerate()
            .map(|(i, palabra)| {
                if i % 3 == 1 {
                    cifrar(palabra)
                } else {
                    palabra.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let (clase, metricas) = clasificar_pagina(&texto);
        assert!(
            metricas.tokens_evaluables < MIN_TOKENS_EVALUABLES,
            "{metricas:?}"
        );
        let ClasePagina::Degradada { motivos } = clase else {
            panic!("se esperaba Degradada: {metricas:?}");
        };
        assert!(
            motivos
                .iter()
                .any(|m| matches!(m, MotivoDegradacion::SimbolosEnPalabras { .. })),
            "{motivos:?}"
        );
    }

    #[test]
    fn test_simbolos_por_encima_de_la_zona_gris_degradan_solos() {
        let (clase, metricas) = clasificar_pagina(&prosa_con_simbolos(1, 6));
        let ClasePagina::Degradada { motivos } = clase else {
            panic!("se esperaba Degradada: {metricas:?}");
        };
        assert!(
            motivos
                .iter()
                .any(|m| matches!(m, MotivoDegradacion::SimbolosEnPalabras { .. })),
            "{motivos:?}"
        );
    }

    #[test]
    fn test_los_conectores_entre_letras_no_cuentan_como_simbolos() {
        let texto = repetir(
            "src/domain/mod.rs snake_case e-mail l'acqua A.B.C don't y/o",
            10,
        );
        let (_, metricas) = clasificar_pagina(&texto);
        assert_eq!(metricas.tokens_con_simbolos, 0, "{metricas:?}");
    }

    #[test]
    fn test_pocos_tokens_con_letras_no_activan_la_senal_de_simbolos() {
        let texto = "O'H)E9 1#+$0#* .=##4 /,'0$ 3' H.53<, 3' \"0$%$)D . 3' +.13D8 N4'4<( J.*4$";
        let (_, metricas) = clasificar_pagina(texto);
        assert!(metricas.tokens_con_letras < MIN_TOKENS_CON_LETRAS);
        assert_eq!(clasificar_pagina(texto).0, ClasePagina::Legible);
    }

    #[test]
    fn test_varios_motivos_se_acumulan() {
        let base = repetir(&cifrar(PROSA_ES), 8);
        let no_blancos = base.chars().filter(|c| !c.is_whitespace()).count();
        let texto = format!("{base}{}", "\u{FFFD}".repeat(no_blancos / 20));
        assert_eq!(motivos(&texto).len(), 2);
    }
}
