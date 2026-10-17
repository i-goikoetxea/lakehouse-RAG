//! Alcance jerárquico: breadcrumb de encabezados y condiciones heredadas (problema 7)

use crate::domain::services::normalizacion::normalizar_token;

/// Separador entre niveles del breadcrumb
pub const SEPARADOR_BREADCRUMB: &str = " > ";
/// Conector entre condiciones heredadas de varios ancestros
pub const CONECTOR_CONDICIONES: &str = " Y ";
/// Longitud máxima (en caracteres) de un título apilado; el exceso se recorta
pub const MAX_CARACTERES_TITULO: usize = 512;

/// Comparadores que, entre paréntesis, delatan un alcance condicional (`(< 0 °C)`)
const COMPARADORES: [char; 4] = ['<', '>', '≤', '≥'];

/// Secuencias de tokens normalizados que introducen una condición de aplicación
const PALABRAS_CLAVE_CONDICION: [&[&str]; 9] = [
    &["CONDICIONES", "PARA"],
    &["CONDICION", "PARA"],
    &["CUANDO"],
    &["EN", "CASO", "DE"],
    &["SOLO", "APLICABLE"],
    &["UNICAMENTE", "APLICABLE"],
    &["BAJO", "CERO"],
    &["EXCEPTO"],
    &["SIEMPRE", "QUE"],
];

/// Extrae la condición de aplicación de un título de sección, sin su numeración inicial.
/// Devuelve `None` si el título no expresa un alcance condicional.
pub fn extraer_condicion(titulo: &str) -> Option<String> {
    let sin_numeracion = quitar_numeracion(titulo);
    let es_condicional = contiene_comparador_entre_parentesis(sin_numeracion)
        || contiene_palabra_clave(sin_numeracion);
    es_condicional.then(|| sin_numeracion.to_string())
}

/// Quita las almohadillas Markdown y la numeración arábiga (`4.`, `4.3`…) del inicio del título
fn quitar_numeracion(titulo: &str) -> &str {
    let sin_markdown = titulo.trim().trim_start_matches('#').trim_start();
    let numeracion = sin_markdown
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(sin_markdown.len());
    let tiene_numeracion = sin_markdown[..numeracion]
        .chars()
        .any(|c| c.is_ascii_digit());
    if tiene_numeracion {
        sin_markdown[numeracion..].trim_start()
    } else {
        sin_markdown
    }
}

fn contiene_comparador_entre_parentesis(texto: &str) -> bool {
    let mut profundidad = 0usize;
    texto.chars().any(|caracter| {
        match caracter {
            '(' => profundidad += 1,
            ')' => profundidad = profundidad.saturating_sub(1),
            _ => {}
        }
        profundidad > 0 && COMPARADORES.contains(&caracter)
    })
}

fn contiene_palabra_clave(texto: &str) -> bool {
    let token = normalizar_token(texto);
    let tokens: Vec<&str> = token.split('_').collect();
    PALABRAS_CLAVE_CONDICION.iter().any(|secuencia| {
        tokens
            .windows(secuencia.len())
            .any(|ventana| ventana == *secuencia)
    })
}

/// Encabezado apilado con su condición de aplicación ya extraída
#[derive(Debug, Clone, PartialEq, Eq)]
struct EncabezadoApilado {
    nivel: u8,
    titulo: String,
    condicion: Option<String>,
}

/// Pila de encabezados abiertos durante el recorrido de un documento.
/// Mantiene el breadcrumb y las condiciones que heredan los bloques descendientes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PilaEncabezados {
    abiertos: Vec<EncabezadoApilado>,
}

impl PilaEncabezados {
    /// Abre un encabezado de `nivel`, cerrando antes los de nivel igual o más profundo.
    /// Los títulos vacíos se ignoran.
    pub fn apilar(&mut self, nivel: u8, titulo: &str) {
        let titulo = sanear_titulo(titulo);
        if titulo.is_empty() {
            return;
        }
        while self
            .abiertos
            .last()
            .is_some_and(|ultimo| ultimo.nivel >= nivel)
        {
            self.abiertos.pop();
        }
        let condicion = extraer_condicion(&titulo);
        self.abiertos.push(EncabezadoApilado {
            nivel,
            titulo,
            condicion,
        });
    }

    /// Ruta jerárquica de los encabezados abiertos (`A > B > C`)
    pub fn breadcrumb(&self) -> String {
        self.abiertos
            .iter()
            .map(|encabezado| encabezado.titulo.as_str())
            .collect::<Vec<_>>()
            .join(SEPARADOR_BREADCRUMB)
    }

    /// Condiciones de todos los ancestros abiertos, unidas por ` Y `
    pub fn condicion_heredada(&self) -> Option<String> {
        let condiciones: Vec<&str> = self
            .abiertos
            .iter()
            .filter_map(|encabezado| encabezado.condicion.as_deref())
            .collect();
        (!condiciones.is_empty()).then(|| condiciones.join(CONECTOR_CONDICIONES))
    }

    /// Número de encabezados abiertos
    pub fn profundidad(&self) -> usize {
        self.abiertos.len()
    }
}

/// Sustituye caracteres de control por espacios, recorta y limita la longitud del título
fn sanear_titulo(titulo: &str) -> String {
    let sin_control: String = titulo
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_CARACTERES_TITULO)
        .collect();
    sin_control.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraer_condicion_detecta_alcance_condicional() {
        let condicion = extraer_condicion("4. Condiciones para soldadura bajo cero (< 0 °C)")
            .expect("Debe detectar la condición");
        assert!(condicion.contains("< 0 °C"));
        assert!(!condicion.starts_with("4."));

        assert!(extraer_condicion("Solo aplicable cuando el espesor supere 30 mm").is_some());
        assert!(extraer_condicion("4.3 En caso de lluvia").is_some());
        assert!(extraer_condicion("2.2 Fase 2 — CUSTODIA").is_none());
        assert!(extraer_condicion("1. Objeto y alcance").is_none());
    }

    #[test]
    fn test_pila_encabezados_construye_breadcrumb_y_hereda_condicion() {
        let mut pila = PilaEncabezados::default();
        pila.apilar(1, "PO-SOL-01 Procedimiento de soldeo");
        pila.apilar(2, "4. Condiciones para soldadura bajo cero (< 0 °C)");
        pila.apilar(3, "4.1 Precalentamiento");

        assert_eq!(
            pila.breadcrumb(),
            "PO-SOL-01 Procedimiento de soldeo > 4. Condiciones para soldadura bajo cero (< 0 °C) > 4.1 Precalentamiento"
        );
        assert!(pila.condicion_heredada().unwrap().contains("< 0 °C"));

        // Un hermano de nivel 2 sustituye a la rama condicional
        pila.apilar(2, "5. Inspección final");
        assert_eq!(
            pila.breadcrumb(),
            "PO-SOL-01 Procedimiento de soldeo > 5. Inspección final"
        );
        assert!(pila.condicion_heredada().is_none());
    }

    #[test]
    fn test_condiciones_anidadas_se_acumulan() {
        let mut pila = PilaEncabezados::default();
        pila.apilar(2, "4. Soldadura bajo cero (< 0 °C)");
        pila.apilar(3, "4.2 Cuando el espesor sea mayor de 30 mm");
        let condicion = pila.condicion_heredada().unwrap();
        assert!(condicion.contains("< 0 °C") && condicion.contains("30 mm"));
        assert!(condicion.contains(" Y "));
    }

    #[test]
    fn test_pila_tolera_saltos_de_nivel_y_niveles_extremos() {
        let mut pila = PilaEncabezados::default();
        pila.apilar(3, "Sección huérfana");
        pila.apilar(1, "Raíz");
        assert_eq!(pila.breadcrumb(), "Raíz");
        pila.apilar(0, "Nivel cero");
        pila.apilar(255, "Nivel extremo");
        assert_eq!(pila.breadcrumb(), "Nivel cero > Nivel extremo");
    }
}
