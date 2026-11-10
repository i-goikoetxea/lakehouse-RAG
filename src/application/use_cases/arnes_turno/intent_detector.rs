//! Clasifica el mensaje del operador. Nunca analiza el texto de los documentos, para que un
//! documento malicioso no pueda activar el arnés.
//!
//! Es deliberadamente conservador: el spike de la Fase 0 mostró que dirigir una tool sin dos ejes
//! con valor produce llamadas con argumentos inventados.

use super::cifras::{UNIDADES, extraer_cifras};
use super::fechas::{es_fecha_en_cifras, mes_de};
use super::texto_normalizado::plegar_texto;

const MAGNITUDES: &[&str] = &[
    "toleranci",
    "temperatur",
    "presion",
    "separacion",
    "distancia",
    "holgura",
    "espesor",
    "diametro",
    "dureza",
    "resistencia",
    "velocidad",
    "caudal",
    "intensidad",
    "tension",
    "longitud",
    "anchura",
    "altura",
];
const ETIQUETAS_CATEGORICAS: &[&str] = &[
    "clase",
    "grupo",
    "tipo",
    "categoria",
    "grado",
    "nivel",
    "material",
    "proceso",
    "posicion",
    "serie",
];
const REFERENCIAS: &[&str] = &[
    "tabla", "tablas", "apartado", "seccion", "articulo", "capitulo", "pagina", "pag", "anexo",
    "figura", "norma", "iso", "une", "asme", "astm", "din", "aws", "punto", "paso", "version",
    "revision", "rev", "capa", "ano",
];
/// Abreviatura de revisión que puede ir pegada a su número: `Rev.01`, `rev2`.
const PREFIJO_DE_REVISION: &str = "rev";
const DIGITOS_MAXIMOS_DEL_DIA: usize = 2;
const PALABRAS_VACIAS: &[&str] = &[
    "a",
    "al",
    "ante",
    "con",
    "corresponde",
    "de",
    "del",
    "e",
    "el",
    "en",
    "entre",
    "es",
    "hay",
    "la",
    "las",
    "lo",
    "los",
    "o",
    "para",
    "por",
    "que",
    "se",
    "segun",
    "si",
    "su",
    "sus",
    "un",
    "una",
    "y",
];
const PREFIJOS_NUMERO_DE_TABLA: &[&str] = &["n", "no", "nº", "num", "numero"];
const ENTIDADES_CONTABLES: &[&str] = &[
    "tablas",
    "documentos",
    "registros",
    "hitos",
    "normas",
    "secciones",
    "apartados",
    "filas",
    "paginas",
    "centros",
    "responsables",
    "roles",
];
const VERBOS_DE_RESPONSABILIDAD: &[&str] = &[
    "firm",
    "ejecut",
    "aprueb",
    "aprob",
    "autoriz",
    "realiz",
    "revis",
    "valid",
    "supervis",
    "verific",
    "inspeccion",
    "emit",
    "custodi",
    "respons",
    "encarg",
    "correspond",
    "hace",
    "cumpliment",
];
const ORDENES_DE_IDENTIFICAR: &[&str] = &["indica", "dime", "identifica", "nombra"];
const DATOS_CONCRETOS: &[&str] = &[
    "valor",
    "valores",
    "codigo",
    "codigos",
    "cifra",
    "cifras",
    "numero",
    "referencia",
    "limite",
    "limites",
    "plazo",
    "plazos",
    "tiempo",
    "duracion",
    "coeficiente",
];
const ORDENES_DE_DAR_DATO: &[&str] = &[
    "indica",
    "dime",
    "identifica",
    "nombra",
    "dame",
    "facilita",
    "proporciona",
];
const VENTANA_DATO_DE_CUAL: usize = 4;
const VENTANA_VERBO_DE_QUIEN: usize = 4;
const VENTANA_ORDEN_DE_IDENTIFICAR: usize = 3;
const LONGITUD_MAXIMA_ID_TABLA: usize = 64;
/// Con esta cota, un mensaje enorme lleno de cifras no vuelve cuadrática la comparación de ejes.
const MAXIMO_EJES_COMPARADOS: usize = 32;

struct Palabra<'a> {
    cruda: &'a str,
    plegada: String,
}

#[derive(Debug, PartialEq, Eq)]
struct EjeConValor {
    etiqueta: Option<String>,
    unidad: Option<String>,
}

impl EjeConValor {
    fn distinto_de(&self, otro: &Self) -> bool {
        match (&self.etiqueta, &otro.etiqueta, &self.unidad, &otro.unidad) {
            (Some(propia), Some(ajena), _, _) => propia != ajena,
            (_, _, Some(propia), Some(ajena)) => propia != ajena,
            _ => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntencionTurno {
    Ninguna,
    /// Tabla o magnitud sin dos ejes con valor: no se puede cruzar aún
    TabularIncompleta,
    /// Tabla o magnitud y dos ejes con valor: se dirige el lookup
    TabularCruzable {
        tabla_mencionada: Option<String>,
    },
    /// Pregunta por quién es responsable, firma o ejecuta
    Responsable,
    /// Pide un valor o un código sin ejes que cruzar: se retiene para validar la abstención
    DatoConcreto,
}

pub struct IntentDetector;

impl IntentDetector {
    /// El mensaje nombra una tabla («tabla», «tablas»), como palabra completa
    pub fn menciona_tabla(mensaje: &str) -> bool {
        trocear(mensaje).iter().any(es_palabra_tabla)
    }

    pub fn clasificar(mensaje: &str) -> IntencionTurno {
        let palabras = trocear(mensaje);
        if pide_agregacion(&palabras) {
            return IntencionTurno::Ninguna;
        }
        if pregunta_por_responsable(&palabras) {
            return IntencionTurno::Responsable;
        }
        match clasificar_tabular(&palabras) {
            IntencionTurno::Ninguna if pide_dato_concreto(&palabras) => {
                IntencionTurno::DatoConcreto
            }
            intencion => intencion,
        }
    }
}

fn trocear(mensaje: &str) -> Vec<Palabra<'_>> {
    mensaje
        .split(|l: char| !(l.is_alphanumeric() || matches!(l, '%' | '°' | '/' | ',' | '.' | '-')))
        .map(|trozo| trozo.trim_matches(|l| matches!(l, ',' | '.' | '-' | '/')))
        .filter(|trozo| !trozo.is_empty())
        .map(|cruda| Palabra {
            cruda,
            plegada: plegar_texto(cruda),
        })
        .collect()
}

fn pide_agregacion(palabras: &[Palabra]) -> bool {
    palabras.windows(2).any(|par| {
        matches!(par[0].plegada.as_str(), "cuantos" | "cuantas")
            && ENTIDADES_CONTABLES.contains(&par[1].plegada.as_str())
    })
}

fn pregunta_por_responsable(palabras: &[Palabra]) -> bool {
    palabras.iter().enumerate().any(|(posicion, palabra)| {
        let siguientes = &palabras[posicion + 1..];
        match palabra.plegada.as_str() {
            "quien" | "quienes" => siguientes
                .iter()
                .take(VENTANA_VERBO_DE_QUIEN)
                .any(|p| empieza_por_alguno(&p.plegada, VERBOS_DE_RESPONSABILIDAD)),
            "que" => siguientes.first().is_some_and(|p| {
                matches!(
                    p.plegada.as_str(),
                    "responsable" | "responsables" | "rol" | "roles"
                )
            }),
            "cual" | "cuales" => {
                siguientes.len() >= 3
                    && matches!(siguientes[0].plegada.as_str(), "es" | "son")
                    && es_responsable(&siguientes[2])
            }
            orden if ORDENES_DE_IDENTIFICAR.contains(&orden) => siguientes
                .iter()
                .take(VENTANA_ORDEN_DE_IDENTIFICAR)
                .any(es_responsable),
            _ => false,
        }
    })
}

/// Un acierto de más solo cuesta el efecto de escritura: el guard rechaza únicamente abstenciones
/// con cifras sin anclar.
fn pide_dato_concreto(palabras: &[Palabra]) -> bool {
    palabras.iter().enumerate().any(|(posicion, palabra)| {
        let siguientes = &palabras[posicion + 1..];
        match palabra.plegada.as_str() {
            "que" => siguientes.first().is_some_and(es_dato_concreto),
            "cuanto" | "cuanta" => siguientes.first().is_some_and(|p| p.plegada != "a"),
            "cual" | "cuales" => siguientes
                .iter()
                .take(VENTANA_DATO_DE_CUAL)
                .any(es_dato_concreto),
            orden if ORDENES_DE_DAR_DATO.contains(&orden) => siguientes
                .iter()
                .take(VENTANA_ORDEN_DE_IDENTIFICAR)
                .any(es_dato_concreto),
            _ => false,
        }
    })
}

fn es_dato_concreto(palabra: &Palabra) -> bool {
    DATOS_CONCRETOS.contains(&palabra.plegada.as_str())
        || empieza_por_alguno(&palabra.plegada, MAGNITUDES)
}

fn es_responsable(palabra: &Palabra) -> bool {
    matches!(palabra.plegada.as_str(), "responsable" | "responsables")
}

fn empieza_por_alguno(palabra: &str, prefijos: &[&str]) -> bool {
    prefijos.iter().any(|prefijo| palabra.starts_with(prefijo))
}

fn clasificar_tabular(palabras: &[Palabra]) -> IntencionTurno {
    let menciona_tabla = palabras.iter().any(es_palabra_tabla);
    let menciona_magnitud = palabras
        .iter()
        .any(|p| empieza_por_alguno(&p.plegada, MAGNITUDES));
    if !menciona_tabla && !menciona_magnitud {
        return IntencionTurno::Ninguna;
    }
    let ejes = ejes_con_valor(palabras);
    if ejes.is_empty() {
        return IntencionTurno::Ninguna;
    }
    if hay_dos_ejes_distintos(&ejes) {
        return IntencionTurno::TabularCruzable {
            tabla_mencionada: tabla_mencionada(palabras),
        };
    }
    IntencionTurno::TabularIncompleta
}

fn es_palabra_tabla(palabra: &Palabra) -> bool {
    matches!(palabra.plegada.as_str(), "tabla" | "tablas")
}

fn hay_dos_ejes_distintos(ejes: &[EjeConValor]) -> bool {
    let comparados = &ejes[..ejes.len().min(MAXIMO_EJES_COMPARADOS)];
    comparados
        .iter()
        .enumerate()
        .any(|(i, eje)| comparados[i + 1..].iter().any(|otro| eje.distinto_de(otro)))
}

fn tabla_mencionada(palabras: &[Palabra]) -> Option<String> {
    let posicion = palabras.iter().position(es_palabra_tabla)?;
    let identificador = palabra_de_id_de_tabla(palabras, posicion)?;
    Some(palabras[identificador].cruda.to_string())
}

/// Posición del identificador que sigue a `tabla`, si lo hay: `Tabla 4-A`, `Tabla nº 7`, `Tabla B`.
fn palabra_de_id_de_tabla(palabras: &[Palabra], posicion_tabla: usize) -> Option<usize> {
    let mut candidata = posicion_tabla + 1;
    if PREFIJOS_NUMERO_DE_TABLA.contains(&palabras.get(candidata)?.plegada.as_str()) {
        candidata += 1;
    }
    let cruda = palabras.get(candidata)?.cruda;
    let longitud_valida = cruda.chars().count() <= LONGITUD_MAXIMA_ID_TABLA;
    (longitud_valida
        && (cruda.contains(|l: char| l.is_ascii_digit()) || es_letra_mayuscula_sola(cruda)))
    .then_some(candidata)
}

fn es_letra_mayuscula_sola(cruda: &str) -> bool {
    let mut letras = cruda.chars();
    matches!((letras.next(), letras.next()), (Some(l), None) if l.is_uppercase())
}

fn ejes_con_valor(palabras: &[Palabra]) -> Vec<EjeConValor> {
    let mut ejes = Vec::new();
    let mut posicion = 0;
    while posicion < palabras.len() {
        if es_palabra_tabla(&palabras[posicion]) {
            posicion = palabra_de_id_de_tabla(palabras, posicion).unwrap_or(posicion) + 1;
        } else if es_eje_categorico(palabras, posicion) {
            ejes.push(EjeConValor {
                etiqueta: Some(palabras[posicion].plegada.clone()),
                unidad: None,
            });
            posicion += 2;
        } else {
            if es_valor_numerico(palabras, posicion) && !abre_rango(palabras, posicion) {
                ejes.push(eje_numerico(palabras, posicion));
            }
            posicion += 1;
        }
    }
    ejes
}

fn es_eje_categorico(palabras: &[Palabra], posicion: usize) -> bool {
    ETIQUETAS_CATEGORICAS.contains(&palabras[posicion].plegada.as_str())
        && palabras
            .get(posicion + 1)
            .is_some_and(|valor| es_valor_categorico(valor.cruda))
}

fn es_valor_categorico(cruda: &str) -> bool {
    let numeral_romano = cruda.chars().count() <= 4
        && cruda
            .chars()
            .all(|l| matches!(l.to_ascii_lowercase(), 'i' | 'v' | 'x' | 'l' | 'c'));
    numeral_romano || cruda.contains(|l: char| l.is_ascii_digit()) || es_letra_mayuscula_sola(cruda)
}

fn es_numero(palabra: &Palabra) -> bool {
    !extraer_cifras(palabra.cruda).is_empty()
}

fn es_valor_numerico(palabras: &[Palabra], posicion: usize) -> bool {
    let tras_referencia = posicion
        .checked_sub(1)
        .is_some_and(|i| REFERENCIAS.contains(&palabras[i].plegada.as_str()));
    es_numero(&palabras[posicion])
        && !tras_referencia
        && !es_revision_pegada(&palabras[posicion])
        && !es_ano_suelto(palabras, posicion)
        && !es_fecha(palabras, posicion)
}

fn es_revision_pegada(palabra: &Palabra) -> bool {
    palabra
        .plegada
        .strip_prefix(PREFIJO_DE_REVISION)
        .is_some_and(|resto| resto.starts_with(|l: char| l == '.' || l.is_ascii_digit()))
}

/// La fecha de una pregunta histórica sitúa la consulta en el tiempo: no es un eje de la tabla.
fn es_fecha(palabras: &[Palabra], posicion: usize) -> bool {
    let palabra = &palabras[posicion];
    if es_fecha_en_cifras(palabra.cruda) {
        return true;
    }
    let es_dia = palabra.cruda.len() <= DIGITOS_MAXIMOS_DEL_DIA
        && palabra.cruda.chars().all(|l| l.is_ascii_digit());
    es_dia
        && palabras
            .get(posicion + 1)
            .is_some_and(|p| p.plegada == "de")
        && palabras
            .get(posicion + 2)
            .is_some_and(|p| mes_de(&p.plegada).is_some())
}

fn es_ano_suelto(palabras: &[Palabra], posicion: usize) -> bool {
    let cruda = palabras[posicion].cruda;
    let parece_ano = cruda.len() == 4
        && (cruda.starts_with("19") || cruda.starts_with("20"))
        && cruda.chars().all(|l| l.is_ascii_digit());
    parece_ano && unidad_del_valor(palabras, posicion).is_none()
}

/// `entre 20 y 30` o `de 20 a 30`: un rango es un solo eje, y cuenta por su segundo extremo.
fn abre_rango(palabras: &[Palabra], posicion: usize) -> bool {
    let Some(anterior) = posicion
        .checked_sub(1)
        .map(|i| palabras[i].plegada.as_str())
    else {
        return false;
    };
    let enlace_de_rango = match anterior {
        "entre" => "y",
        "de" => "a",
        _ => return false,
    };
    palabras
        .get(posicion + 1)
        .is_some_and(|p| p.plegada == enlace_de_rango)
        && palabras.get(posicion + 2).is_some_and(es_numero)
}

fn eje_numerico(palabras: &[Palabra], posicion: usize) -> EjeConValor {
    let unidad = unidad_del_valor(palabras, posicion);
    let palabras_de_unidad_sueltas = usize::from(unidad_suelta(palabras, posicion).is_some());
    EjeConValor {
        etiqueta: etiqueta_anterior(palabras, posicion)
            .or_else(|| etiqueta_posterior(palabras, posicion + palabras_de_unidad_sueltas)),
        unidad,
    }
}

fn unidad_del_valor(palabras: &[Palabra], posicion: usize) -> Option<String> {
    unidad_pegada(&palabras[posicion].plegada).or_else(|| unidad_suelta(palabras, posicion))
}

fn unidad_pegada(plegada: &str) -> Option<String> {
    let sufijo = plegada.trim_start_matches(|l: char| l.is_ascii_digit() || matches!(l, ',' | '.'));
    (!sufijo.is_empty()).then(|| sufijo.to_string())
}

fn unidad_suelta(palabras: &[Palabra], posicion: usize) -> Option<String> {
    let siguiente = palabras.get(posicion + 1)?.plegada.as_str();
    // Una `a` suelta tras un número es la preposición, no el amperio.
    let es_unidad = siguiente != "a" && UNIDADES.contains(&siguiente);
    (es_unidad || matches!(siguiente, "%" | "°c" | "ºc")).then(|| siguiente.to_string())
}

fn es_palabra_etiqueta(palabra: &Palabra) -> bool {
    palabra.plegada.chars().all(char::is_alphabetic)
        && !PALABRAS_VACIAS.contains(&palabra.plegada.as_str())
        && !UNIDADES.contains(&palabra.plegada.as_str())
}

/// `CE 0,42` o `espesor de 30 mm`.
fn etiqueta_anterior(palabras: &[Palabra], posicion: usize) -> Option<String> {
    let anterior = palabras.get(posicion.checked_sub(1)?)?;
    let etiqueta = if anterior.plegada == "de" {
        palabras.get(posicion.checked_sub(2)?)?
    } else {
        anterior
    };
    es_palabra_etiqueta(etiqueta).then(|| etiqueta.plegada.clone())
}

/// `30 mm de espesor`.
fn etiqueta_posterior(palabras: &[Palabra], ultima_del_valor: usize) -> Option<String> {
    if palabras.get(ultima_del_valor + 1)?.plegada != "de" {
        return None;
    }
    let etiqueta = palabras.get(ultima_del_valor + 2)?;
    es_palabra_etiqueta(etiqueta).then(|| etiqueta.plegada.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cruzable(tabla: Option<&str>) -> IntencionTurno {
        IntencionTurno::TabularCruzable {
            tabla_mencionada: tabla.map(str::to_string),
        }
    }

    fn clasificar(mensaje: &str) -> IntencionTurno {
        IntentDetector::clasificar(mensaje)
    }

    #[test]
    fn test_filas_del_ssd() {
        assert_eq!(
            clasificar("Según la Tabla 4-A, ¿temperatura para 30 mm y CE 0,42 %?"),
            cruzable(Some("4-A"))
        );
        assert_eq!(
            clasificar("¿Qué tolerancia corresponde a 30 mm?"),
            IntencionTurno::TabularIncompleta
        );
        assert_eq!(
            clasificar("¿Qué es el refuerzo de soldadura?"),
            IntencionTurno::Ninguna
        );
        assert_eq!(clasificar("¿Cuántas tablas hay?"), IntencionTurno::Ninguna);
        assert_eq!(
            clasificar("¿Quién firma el registro de recepción?"),
            IntencionTurno::Responsable
        );
    }

    #[test]
    fn test_punto_decimal_y_unidad_pegada_cruzan_igual() {
        assert_eq!(
            clasificar("Tabla 4-A: temperatura para 30mm y CE 0.42%"),
            cruzable(Some("4-A"))
        );
    }

    #[test]
    fn test_magnitud_sin_tabla_y_dos_ejes_es_cruzable_sin_tabla() {
        assert_eq!(
            clasificar("¿Qué temperatura aplica a 30 mm y 0,42 %?"),
            cruzable(None)
        );
    }

    #[test]
    fn test_eje_categorico_cuenta_como_eje_con_valor() {
        assert_eq!(
            clasificar("En la Tabla 7, ¿tolerancia para clase III y 30 mm?"),
            cruzable(Some("7"))
        );
        assert_eq!(
            clasificar("Tolerancia para clase A y grupo 2"),
            cruzable(None)
        );
    }

    #[test]
    fn test_un_solo_eje_categorico_es_incompleta() {
        assert_eq!(
            clasificar("¿Qué tolerancia tiene la clase III?"),
            IntencionTurno::TabularIncompleta
        );
    }

    #[test]
    fn test_palabra_de_eje_sin_valor_no_cuenta() {
        assert_eq!(
            clasificar("¿Qué tipo de tolerancia y qué clase de temperatura hay?"),
            IntencionTurno::Ninguna
        );
    }

    #[test]
    fn test_el_identificador_de_tabla_no_cuenta_como_eje() {
        assert_eq!(clasificar("¿Qué dice la Tabla 4?"), IntencionTurno::Ninguna);
        assert_eq!(
            clasificar("Tabla 4, ¿qué temperatura para 30 mm?"),
            IntencionTurno::TabularIncompleta
        );
    }

    #[test]
    fn test_referencias_normativas_no_cuentan_como_eje() {
        assert_eq!(
            clasificar("Según ISO 9606 y el apartado 5.2, ¿qué tolerancia corresponde a 30 mm?"),
            IntencionTurno::TabularIncompleta
        );
    }

    #[test]
    fn test_rango_y_misma_unidad_son_un_solo_eje() {
        for mensaje in [
            "¿Qué tolerancia hay entre 20 y 30 mm?",
            "¿Qué tolerancia hay de 20 a 30 mm?",
            "Compara la tolerancia de 30 mm y 40 mm",
        ] {
            assert_eq!(
                clasificar(mensaje),
                IntencionTurno::TabularIncompleta,
                "{mensaje}"
            );
        }
    }

    #[test]
    fn test_misma_unidad_con_ejes_nombrados_son_dos_ejes() {
        assert_eq!(
            clasificar("¿Qué tolerancia para espesor 30 mm y diámetro 20 mm?"),
            cruzable(None)
        );
        assert_eq!(
            clasificar("¿Qué tolerancia para 30 mm de espesor y 20 mm de diámetro?"),
            cruzable(None)
        );
    }

    #[test]
    fn test_agregacion_gana_aunque_haya_valores() {
        assert_eq!(
            clasificar("¿Cuántas tablas hay para 30 mm y CE 0,42 %?"),
            IntencionTurno::Ninguna
        );
        assert_eq!(
            clasificar("¿Cuántos responsables hay?"),
            IntencionTurno::Ninguna
        );
    }

    #[test]
    fn test_cuantos_con_magnitud_no_es_agregacion() {
        assert_eq!(
            clasificar("¿Cuántos grados de temperatura para 30 mm y CE 0,42 %?"),
            cruzable(None)
        );
    }

    #[test]
    fn test_variantes_de_responsable() {
        for mensaje in [
            "¿Quién ejecuta la inspección?",
            "¿Quiénes deben aprobar el plan?",
            "¿A quién corresponde validar el ensayo?",
            "¿Cuál es el responsable de la recepción?",
            "Indica el responsable del registro",
            "¿Qué rol firma el acta?",
            "¿QUIÉN FIRMA?",
        ] {
            assert_eq!(
                clasificar(mensaje),
                IntencionTurno::Responsable,
                "{mensaje}"
            );
        }
    }

    #[test]
    fn test_falsos_amigos_de_responsable() {
        for mensaje in [
            "¿Quién es Arquímedes?",
            "¿Qué hace el responsable de calidad?",
            "El consumo es responsable del gasto",
        ] {
            assert_eq!(clasificar(mensaje), IntencionTurno::Ninguna, "{mensaje}");
        }
    }

    #[test]
    fn test_pregunta_de_dato_concreto() {
        for mensaje in [
            "¿Cuál es el valor de la dureza Brinell?",
            "¿Cuál es el código del procedimiento de izado?",
            "¿Cuáles son los códigos de la norma ZX-9931?",
            "¿Qué valor tiene el caudal?",
            "¿Qué código corresponde al ensayo?",
            "¿Qué presión tiene la línea L-999?",
            "¿Qué caudal admite la bomba?",
            "¿Cuál es la temperatura máxima?",
            "¿Cuál es la máxima temperatura admitida?",
            "¿Cuál es el plazo de acopio?",
            "¿Cuánto vale el coeficiente?",
            "¿Cuánta presión admite?",
            "Dime el valor de la holgura",
            "Indica el código del registro",
            "Dame la presión de la línea",
            "¿QUÉ VALOR TIENE?",
        ] {
            assert_eq!(
                clasificar(mensaje),
                IntencionTurno::DatoConcreto,
                "{mensaje}"
            );
        }
    }

    #[test]
    fn test_prioridad_de_las_otras_intenciones_sobre_el_dato_concreto() {
        assert_eq!(
            clasificar("¿Qué tolerancia corresponde a 30 mm?"),
            IntencionTurno::TabularIncompleta
        );
        assert_eq!(
            clasificar("¿Qué temperatura aplica a 30 mm y 0,42 %?"),
            cruzable(None)
        );
        assert_eq!(
            clasificar("¿Cuál es el responsable del código?"),
            IntencionTurno::Responsable
        );
        assert_eq!(
            clasificar("¿Cuántas normas hay? Dime el valor"),
            IntencionTurno::Ninguna
        );
    }

    #[test]
    fn test_falsos_amigos_del_dato_concreto() {
        for mensaje in [
            "¿Qué es el refuerzo de soldadura?",
            "¿Qué es un valor nominal?",
            "Explica la tensión de vapor",
            "¿Por qué importa la temperatura?",
            "¿Cómo se mide el caudal?",
            "El valor de la presión es alto",
            "¿Qué dice la Tabla 4?",
        ] {
            assert_eq!(clasificar(mensaje), IntencionTurno::Ninguna, "{mensaje}");
        }
    }

    #[test]
    fn test_menciona_tabla_por_palabra_completa() {
        assert!(IntentDetector::menciona_tabla(
            "Según la Tabla 4-A, ¿qué valor?"
        ));
        assert!(IntentDetector::menciona_tabla("¿Qué dicen las TABLAS?"));
        assert!(!IntentDetector::menciona_tabla("¿Qué tablazón se usa?"));
        assert!(!IntentDetector::menciona_tabla("¿Qué temperatura a 30 mm?"));
    }

    #[test]
    fn test_mensaje_vacio_o_sin_palabras() {
        for mensaje in ["", "   ", "¿?¡!", "\n\t"] {
            assert_eq!(clasificar(mensaje), IntencionTurno::Ninguna);
        }
    }
    #[test]
    fn test_fecha_de_la_pregunta_no_cuenta_como_eje() {
        for mensaje in [
            "¿Cuál era la temperatura del agua de purga del procedimiento PX-PUR-07 el 11 de septiembre de 2025? Indica la revisión.",
            "¿Qué presión de purga exigía el procedimiento PX-PUR-07 el 16 de junio de 2025? Indica la revisión.",
            "¿Qué presión de purga exigía el procedimiento PX-PUR-07 el 2025-06-16?",
            "¿Qué presión de purga exigía el 16/06/2025?",
            "¿Qué temperatura regía el 3 de Setiembre de 2024?",
        ] {
            let intencion = clasificar(mensaje);
            assert!(
                matches!(
                    intencion,
                    IntencionTurno::Ninguna | IntencionTurno::DatoConcreto
                ),
                "{mensaje}: {intencion:?}"
            );
        }
    }

    #[test]
    fn test_revision_de_la_pregunta_no_cuenta_como_eje() {
        for mensaje in [
            "Según la Rev. 00 del procedimiento PX-PUR-07, ¿cuál es la temperatura del agua de purga?",
            "¿Qué presión de purga fijaba la Rev. 00 del procedimiento PX-PUR-07?",
            "¿Qué presión fijaba la Rev.01?",
            "¿Qué presión fijaba la REV 2?",
        ] {
            let intencion = clasificar(mensaje);
            assert!(
                matches!(
                    intencion,
                    IntencionTurno::Ninguna | IntencionTurno::DatoConcreto
                ),
                "{mensaje}: {intencion:?}"
            );
        }
    }

    #[test]
    fn test_fecha_o_revision_no_ocultan_los_ejes_reales() {
        assert_eq!(
            clasificar(
                "¿Cuál es hoy la temperatura mínima de precalentamiento de la Tabla 4-A para 30 mm y CE 0,42 %, y cuál era el 1 de junio de 2026, antes de que entrara en vigor la Rev. 04?"
            ),
            cruzable(Some("4-A"))
        );
        assert_eq!(
            clasificar(
                "Según la Rev. 03, Tabla 4-A: temperatura para 30 mm y CE 0,42 % el 2026-06-01"
            ),
            cruzable(Some("4-A"))
        );
        assert_eq!(
            clasificar("¿Qué tolerancia corresponde a 30 mm el 11 de septiembre de 2025?"),
            IntencionTurno::TabularIncompleta
        );
        assert_eq!(
            clasificar("¿Qué tolerancia corresponde a 11 mm de espesor y clase III?"),
            cruzable(None)
        );
    }
}
