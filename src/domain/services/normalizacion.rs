//! Normalización determinista de vocabularios y códigos documentales

/// Separador de los tokens canónicos (`INSPECCION_DE_RECEPCION`)
const SEPARADOR_TOKEN: char = '_';
/// Separador de los códigos canónicos (`UNE-EN-ISO-15614-1`)
const SEPARADOR_CODIGO: char = '-';
/// Prefijos que introducen el número de revisión, ya plegados a mayúsculas
const PREFIJOS_REVISION: [&str; 2] = ["REVISION", "REV"];

/// Pliega un texto a mayúsculas y elimina los diacríticos latinos (`Recepción` → `RECEPCION`)
pub fn plegar_mayusculas(texto: &str) -> String {
    texto
        .chars()
        .flat_map(char::to_uppercase)
        .map(quitar_diacritico)
        .collect()
}

/// Token canónico: mayúsculas ASCII sin tildes y cualquier separador → `_`
pub fn normalizar_token(texto: &str) -> String {
    unir_alfanumericos(&plegar_mayusculas(texto), SEPARADOR_TOKEN, &[])
}

/// Código canónico de documento o registro: solo `[A-Z0-9.-]`; el resto de separadores → `-`
pub fn codigo_canonico(texto: &str) -> String {
    unir_alfanumericos(&plegar_mayusculas(texto), SEPARADOR_CODIGO, &['.'])
}

/// Separa el código canónico de su número de revisión (`PO-ALM-03 Rev. 03` → `("PO-ALM-03", Some(3))`)
pub fn separar_revision(texto: &str) -> (String, Option<u32>) {
    let plegado: Vec<char> = plegar_mayusculas(texto).chars().collect();
    match localizar_revision(&plegado) {
        Some((inicio, revision)) => {
            let codigo: String = plegado[..inicio].iter().collect();
            (codigo_canonico(&codigo), Some(revision))
        }
        None => (codigo_canonico(texto), None),
    }
}

fn quitar_diacritico(caracter: char) -> char {
    match caracter {
        'Á' | 'À' | 'Ä' | 'Â' | 'Ã' | 'Å' => 'A',
        'É' | 'È' | 'Ë' | 'Ê' => 'E',
        'Í' | 'Ì' | 'Ï' | 'Î' => 'I',
        'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'O',
        'Ú' | 'Ù' | 'Ü' | 'Û' => 'U',
        'Ñ' => 'N',
        'Ç' => 'C',
        otro => otro,
    }
}

/// Conserva los alfanuméricos ASCII (y los `conservados`), colapsa el resto en un único
/// `separador` y recorta los separadores y conservados de los extremos.
fn unir_alfanumericos(plegado: &str, separador: char, conservados: &[char]) -> String {
    let mut resultado = String::with_capacity(plegado.len());
    for caracter in plegado.chars() {
        let es_valido = caracter.is_ascii_alphanumeric() || conservados.contains(&caracter);
        let siguiente = if es_valido { caracter } else { separador };
        let repetido = !caracter.is_ascii_alphanumeric() && resultado.ends_with(siguiente);
        if !repetido {
            resultado.push(siguiente);
        }
    }
    let es_borde = |c: char| c == separador || conservados.contains(&c);
    resultado.trim_matches(es_borde).to_string()
}

/// Busca la última marca de revisión (`REV`, `REV.`, `REVISION`) seguida de dígitos.
/// Devuelve la posición donde empieza la marca y el número de revisión.
fn localizar_revision(plegado: &[char]) -> Option<(usize, u32)> {
    (0..plegado.len())
        .rev()
        .filter(|&inicio| inicio == 0 || !plegado[inicio - 1].is_alphanumeric())
        .find_map(|inicio| leer_revision_desde(plegado, inicio).map(|rev| (inicio, rev)))
}

fn leer_revision_desde(plegado: &[char], inicio: usize) -> Option<u32> {
    let resto = &plegado[inicio..];
    let longitud_prefijo = PREFIJOS_REVISION
        .iter()
        .find(|prefijo| {
            resto
                .iter()
                .take(prefijo.len())
                .copied()
                .eq(prefijo.chars())
        })
        .map(|prefijo| prefijo.len())?;
    let tras_prefijo = &resto[longitud_prefijo..];
    let espacios = tras_prefijo
        .iter()
        .take_while(|c| matches!(c, '.' | ':' | ' ' | '_' | '-'))
        .count();
    let digitos: String = tras_prefijo[espacios..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let siguiente = tras_prefijo.get(espacios + digitos.len());
    if digitos.is_empty() || siguiente.is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    digitos.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalizar_token_quita_tildes_y_unifica_separadores() {
        assert_eq!(normalizar_token("Recepción"), "RECEPCION");
        assert_eq!(
            normalizar_token("  inspección de  recepción "),
            "INSPECCION_DE_RECEPCION"
        );
        assert_eq!(
            normalizar_token("SOLDEO (REQUISITOS POSTERIORES)"),
            "SOLDEO_REQUISITOS_POSTERIORES"
        );
        assert_eq!(normalizar_token("Señal ÜBER"), "SENAL_UBER");
        assert_eq!(normalizar_token("¿¿??"), "");
    }

    #[test]
    fn test_codigo_canonico_produce_identificadores_estables() {
        assert_eq!(codigo_canonico("UNE-EN ISO 15614-1"), "UNE-EN-ISO-15614-1");
        assert_eq!(codigo_canonico("une en 1090/2"), "UNE-EN-1090-2");
        assert_eq!(codigo_canonico("PO-ALM-03"), "PO-ALM-03");
        assert_eq!(codigo_canonico("for_alm_02"), "FOR-ALM-02");
        assert_eq!(codigo_canonico("Anexo II"), "ANEXO-II");
        assert_eq!(codigo_canonico("x'; DROP TABLE--"), "X-DROP-TABLE");
    }

    #[test]
    fn test_separar_revision_del_codigo() {
        assert_eq!(
            separar_revision("PO-ALM-03 Rev. 03"),
            ("PO-ALM-03".to_string(), Some(3))
        );
        assert_eq!(
            separar_revision("PO-ALM-03 rev 4"),
            ("PO-ALM-03".to_string(), Some(4))
        );
        assert_eq!(
            separar_revision("PO-ALM-03 (Rev.04)"),
            ("PO-ALM-03".to_string(), Some(4))
        );
        assert_eq!(
            separar_revision("UNE-EN 1090-2"),
            ("UNE-EN-1090-2".to_string(), None)
        );
    }
}
