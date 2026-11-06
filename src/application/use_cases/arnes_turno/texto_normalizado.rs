//! Normalización de texto compartida por el detector y el guard.

/// Minúsculas y sin tildes, para comparar vocabulario sin depender de cómo se escribió.
pub(super) fn plegar_texto(texto: &str) -> String {
    texto
        .chars()
        .flat_map(char::to_lowercase)
        .map(plegar_tilde)
        .collect()
}

fn plegar_tilde(letra: char) -> char {
    match letra {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        otra => otra,
    }
}
