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
/// Señal de la calibración N1 (no estaba en el SSD): tokens con letras y símbolos ASCII dentro
pub const MIN_TOKENS_CON_LETRAS: usize = 40;
pub const UMBRAL_SIMBOLOS_EN_PALABRAS: f32 = 0.35;

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
    let (tokens_evaluables, tokens_sospechosos) = contar_tokens(texto);
    let (tokens_con_letras, tokens_con_simbolos) = contar_tokens_con_simbolos(texto);
    MetricasPagina {
        no_blancos,
        reemplazo_o_control: reemplazos_o_controles_no_aislados(&caracteres),
        uso_privado: caracteres.iter().filter(|c| es_uso_privado(**c)).count(),
        uso_privado_dentro_de_palabra: uso_privado_dentro_de_palabra(&caracteres),
        tokens_evaluables,
        tokens_sospechosos,
        tokens_con_letras,
        tokens_con_simbolos,
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

fn motivo_simbolos_en_palabras(metricas: &MetricasPagina) -> Option<MotivoDegradacion> {
    if metricas.tokens_con_letras < MIN_TOKENS_CON_LETRAS {
        return None;
    }
    let proporcion = proporcion(metricas.tokens_con_simbolos, metricas.tokens_con_letras);
    (proporcion > UMBRAL_SIMBOLOS_EN_PALABRAS).then_some(MotivoDegradacion::SimbolosEnPalabras {
        proporcion,
        tokens_evaluados: metricas.tokens_con_letras,
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

/// Signos ASCII que sí aparecen dentro de tokens reales con letras: guiones, apóstrofos,
/// abreviaturas, decimales, fechas, unidades, horas, identificadores, correos y URL
fn es_signo_interior_habitual(caracter: char) -> bool {
    matches!(caracter, '-' | '\'' | '.' | ',' | '/' | ':' | '_' | '@')
}

/// Tokens con al menos dos caracteres y alguna letra (cualquier caja y escritura) y, de ellos,
/// los que llevan dentro un símbolo ASCII que no es un signo interior habitual. Es tipografía, no
/// una lista de palabras
fn contar_tokens_con_simbolos(texto: &str) -> (usize, usize) {
    let mut con_letras = 0;
    let mut con_simbolos = 0;
    for token in texto.split_whitespace() {
        let recortado = token.trim_matches(|c: char| c.is_ascii_punctuation());
        if recortado.chars().count() < 2 || !recortado.chars().any(char::is_alphabetic) {
            continue;
        }
        con_letras += 1;
        con_simbolos += usize::from(
            recortado
                .chars()
                .any(|c| c.is_ascii_punctuation() && !es_signo_interior_habitual(c)),
        );
    }
    (con_letras, con_simbolos)
}

/// Puntuación ASCII que no aparece dentro de una palabra real (`-` y `'` sí aparecen)
fn es_puntuacion_anomala(caracter: char) -> bool {
    caracter.is_ascii_punctuation() && caracter != '-' && caracter != '\''
}

fn contar_tokens(texto: &str) -> (usize, usize) {
    let mut evaluables = 0;
    let mut sospechosos = 0;
    for token in texto.split_whitespace() {
        let recortado = token.trim_matches(|c: char| !c.is_alphanumeric());
        if let Some(sospechoso) = evaluar_token(recortado) {
            evaluables += 1;
            sospechosos += usize::from(sospechoso);
        }
    }
    (evaluables, sospechosos)
}

/// `None` si el token no es evaluable; si lo es, si resulta sospechoso
fn evaluar_token(token: &str) -> Option<bool> {
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
    let sin_vocal = !letras.iter().any(|c| es_vocal(alfabeto, *c));
    let puntuacion_interior = token.chars().any(es_puntuacion_anomala);
    Some(sin_vocal || puntuacion_interior)
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

    #[test]
    fn test_los_simbolos_dentro_de_las_palabras_degradan() {
        let motivos = motivos(&basura_de_simbolos());
        assert!(
            motivos
                .iter()
                .any(|m| matches!(m, MotivoDegradacion::SimbolosEnPalabras { proporcion, .. } if *proporcion > UMBRAL_SIMBOLOS_EN_PALABRAS)),
            "{motivos:?}"
        );
    }

    #[test]
    fn test_las_formulas_codigos_y_tablas_con_signos_internos_no_degradan() {
        let texto = repetir(
            "Sea f(x) = a+b con x=1, y R&D del 12% en km/h; el valor 2,487,862 (2026-T1) \
             y la referencia A.2-4 de e-mail usuario@dominio.org en http://ejemplo.org/a_b \
             cumple que la norma aplica sin excepciones durante todo el periodo anual",
            10,
        );
        let (clase, metricas) = clasificar_pagina(&texto);
        assert!(
            metricas.tokens_con_letras >= MIN_TOKENS_CON_LETRAS,
            "{metricas:?}"
        );
        assert_eq!(clase, ClasePagina::Legible, "{metricas:?}");
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
