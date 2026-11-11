//! Estructura documental de STAIR: estado del contenido, tipo de bloque y nodos del índice.
//! El contrato completo está en `docs/arquitectura/decisiones_stair.md`.

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;
use crate::domain::services::normalizacion::codigo_canonico;

/// Separador entre el documento y la sección en un `nodo_id`
const SEPARADOR_SECCION: &str = "#sec-";
/// Segmento reservado del preámbulo (texto previo al primer encabezado)
pub const SEGMENTO_PREAMBULO: &str = "preambulo";
/// Código de la sección única de un documento sin encabezados (`<doc_id>#sec-cuerpo`)
pub const CODIGO_CONTENIDO_GENERAL: &str = "cuerpo";

/// Grado de procesamiento del contenido de una fila (independiente de `estado_ingesta`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EstadoContenido {
    Completo,
    ParcialUniversal,
    RequiereOcr,
    /// Destilado, pero la fila corta una página cuyo texto no superó la compuerta de
    /// legibilidad (`ssd_generalidad_4_piezas.md` §1.4). Tiene Capa 2
    LecturaDegradada,
}

impl EstadoContenido {
    pub const VALORES_CANONICOS: [&'static str; 4] = [
        "COMPLETO",
        "PARCIAL_UNIVERSAL",
        "REQUIERE_OCR",
        "LECTURA_DEGRADADA",
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completo => "COMPLETO",
            Self::ParcialUniversal => "PARCIAL_UNIVERSAL",
            Self::RequiereOcr => "REQUIERE_OCR",
            Self::LecturaDegradada => "LECTURA_DEGRADADA",
        }
    }

    /// Hay Capa 2 (matriz, tablas y grafo) para la revisión de la fila
    pub fn tiene_capa_2(&self) -> bool {
        matches!(self, Self::Completo | Self::LecturaDegradada)
    }

    pub fn parse(valor: &str) -> Result<Self, DomainError> {
        match valor {
            "COMPLETO" => Ok(Self::Completo),
            "PARCIAL_UNIVERSAL" => Ok(Self::ParcialUniversal),
            "REQUIERE_OCR" => Ok(Self::RequiereOcr),
            "LECTURA_DEGRADADA" => Ok(Self::LecturaDegradada),
            otro => Err(DomainError::ValidationError(format!(
                "estado_contenido fuera de vocabulario: {otro:?}"
            ))),
        }
    }
}

/// Naturaleza de un nodo de la tabla 07
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TipoNodoIndice {
    /// Apartado que existe en el documento
    #[default]
    Real,
    /// División técnica de la ingesta: nunca se presenta como apartado del documento
    Sintetico,
}

impl TipoNodoIndice {
    pub const VALORES_CANONICOS: [&'static str; 2] = ["REAL", "SINTETICO"];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Real => "REAL",
            Self::Sintetico => "SINTETICO",
        }
    }

    pub fn parse(valor: &str) -> Result<Self, DomainError> {
        match valor {
            "REAL" => Ok(Self::Real),
            "SINTETICO" => Ok(Self::Sintetico),
            otro => Err(DomainError::ValidationError(format!(
                "tipo_nodo desconocido: {otro:?}"
            ))),
        }
    }
}

/// Marca del código de un sub-nodo sintético por páginas (`2.9.PAG32-41`)
pub const MARCA_NODO_POR_PAGINAS: &str = ".PAG";

/// Código de la sección real a la que pertenece un código, sintético o no
pub fn codigo_real_de(codigo: &str) -> &str {
    codigo
        .rfind(MARCA_NODO_POR_PAGINAS)
        .map_or(codigo, |posicion| &codigo[..posicion])
}

/// Naturaleza del bloque de texto de un chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoBloque {
    Preambulo,
    Narrativa,
    Definicion,
    Alcance,
    Procedimiento,
}

impl TipoBloque {
    pub const VALORES_CANONICOS: [&'static str; 5] = [
        "PREAMBULO",
        "NARRATIVA",
        "DEFINICION",
        "ALCANCE",
        "PROCEDIMIENTO",
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preambulo => "PREAMBULO",
            Self::Narrativa => "NARRATIVA",
            Self::Definicion => "DEFINICION",
            Self::Alcance => "ALCANCE",
            Self::Procedimiento => "PROCEDIMIENTO",
        }
    }

    pub fn parse(valor: &str) -> Result<Self, DomainError> {
        match valor {
            "PREAMBULO" => Ok(Self::Preambulo),
            "NARRATIVA" => Ok(Self::Narrativa),
            "DEFINICION" => Ok(Self::Definicion),
            "ALCANCE" => Ok(Self::Alcance),
            "PROCEDIMIENTO" => Ok(Self::Procedimiento),
            otro => Err(DomainError::ValidationError(format!(
                "tipo_bloque fuera de vocabulario: {otro:?}"
            ))),
        }
    }
}

/// Nivel de un nodo del índice: 0 = preámbulo, 1 a 6 = encabezados H1 a H6
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NivelJerarquico(i8);

impl NivelJerarquico {
    pub const PREAMBULO: Self = Self(0);
    pub const MAXIMO: i8 = 6;

    pub fn new(valor: i8) -> Result<Self, DomainError> {
        if (0..=Self::MAXIMO).contains(&valor) {
            Ok(Self(valor))
        } else {
            Err(DomainError::ValidationError(format!(
                "nivel_jerarquico {valor} fuera de rango (0 a {})",
                Self::MAXIMO
            )))
        }
    }

    pub fn valor(&self) -> i8 {
        self.0
    }
}

/// Clave de una sección: `<doc_id>#sec-<segmento>`. Une 02 y 07 dentro de una revisión
/// (`centro_id`, `doc_id`, `version_num`). Solo admite `[A-Za-z0-9._-]` y el separador, por lo
/// que puede viajar en un predicado SQL sin riesgo de inyección.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodoId(String);

impl NodoId {
    pub const MAX_LONGITUD: usize = 512;

    pub fn new(valor: impl Into<String>) -> Result<Self, DomainError> {
        let valor = valor.into();
        if valor.len() > Self::MAX_LONGITUD {
            return Err(DomainError::ValidationError(format!(
                "El nodo_id excede la longitud máxima de {} caracteres",
                Self::MAX_LONGITUD
            )));
        }
        let formato_valido = valor
            .split_once(SEPARADOR_SECCION)
            .is_some_and(|(doc, segmento)| {
                DocId::new(doc).is_ok_and(|d| d.as_str() == doc) && segmento_valido(segmento)
            });
        if !formato_valido {
            return Err(DomainError::ValidationError(format!(
                "nodo_id inválido {valor:?}: el formato es <doc_id>#sec-<segmento> con [A-Za-z0-9._-]"
            )));
        }
        Ok(Self(valor))
    }

    /// Nodo de una sección: usa su código canónico o, si no sirve, su índice en el documento
    pub fn de_seccion(doc_id: &DocId, codigo: &str, indice: usize) -> Self {
        if codigo == CODIGO_CONTENIDO_GENERAL {
            return Self(format!(
                "{doc_id}{SEPARADOR_SECCION}{CODIGO_CONTENIDO_GENERAL}"
            ));
        }
        // Los sub-nodos conservan el segmento en minúsculas de su padre
        let real = codigo_real_de(codigo);
        if real != codigo && (real == CODIGO_CONTENIDO_GENERAL || real == SEGMENTO_PREAMBULO) {
            return Self(format!("{doc_id}{SEPARADOR_SECCION}{codigo}"));
        }
        let canonico = codigo_canonico(codigo);
        let nodo = format!("{doc_id}{SEPARADOR_SECCION}{canonico}");
        if segmento_valido(&canonico) && nodo.len() <= Self::MAX_LONGITUD {
            Self(nodo)
        } else {
            Self::de_indice(doc_id, indice)
        }
    }

    /// Nodo de una sección sin código utilizable
    pub fn de_indice(doc_id: &DocId, indice: usize) -> Self {
        Self(format!("{doc_id}{SEPARADOR_SECCION}idx-{indice}"))
    }

    /// Nodo del texto previo al primer encabezado
    pub fn preambulo(doc_id: &DocId) -> Self {
        Self(format!("{doc_id}{SEPARADOR_SECCION}{SEGMENTO_PREAMBULO}"))
    }

    /// Parte posterior al separador (`4.2` en `PO-ALM-03#sec-4.2`)
    /// Sección real: el padre de un sub-nodo por páginas, o el propio nodo
    pub fn seccion_real(&self) -> Self {
        Self(codigo_real_de(&self.0).to_string())
    }

    pub fn segmento(&self) -> &str {
        self.0
            .split_once(SEPARADOR_SECCION)
            .map_or("", |(_, segmento)| segmento)
    }

    pub fn pertenece_a(&self, doc_id: &DocId) -> bool {
        self.0
            .split_once(SEPARADOR_SECCION)
            .is_some_and(|(doc, _)| doc == doc_id.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NodoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn segmento_valido(segmento: &str) -> bool {
    !segmento.is_empty()
        && !segmento.contains("..")
        && segmento
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        && segmento.chars().any(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> DocId {
        DocId::new("PO-ALM-03").unwrap()
    }

    #[test]
    fn test_nodo_de_seccion_usa_el_codigo_canonico() {
        assert_eq!(
            NodoId::de_seccion(&doc(), "4.2", 0).as_str(),
            "PO-ALM-03#sec-4.2"
        );
        assert_eq!(
            NodoId::de_seccion(&doc(), "Anexo B", 0).as_str(),
            "PO-ALM-03#sec-ANEXO-B"
        );
        assert_eq!(
            NodoId::de_seccion(&doc(), "  ", 5).as_str(),
            "PO-ALM-03#sec-idx-5"
        );
    }

    #[test]
    fn test_nodo_reconoce_su_documento_exacto() {
        let nodo = NodoId::new("PO-ALM-03#sec-1").unwrap();
        assert!(nodo.pertenece_a(&doc()));
        assert!(!nodo.pertenece_a(&DocId::new("PO-ALM").unwrap()));
    }

    #[test]
    fn test_nodo_rechaza_doc_con_espacios_que_docid_recortaria() {
        assert!(NodoId::new(" PO-ALM-03#sec-1").is_err());
    }

    #[test]
    fn test_lectura_degradada_es_un_estado_con_capa_2() {
        let estado = EstadoContenido::parse("LECTURA_DEGRADADA").unwrap();
        assert_eq!(estado, EstadoContenido::LecturaDegradada);
        assert!(estado.tiene_capa_2());
        assert!(EstadoContenido::Completo.tiene_capa_2());
        assert!(!EstadoContenido::ParcialUniversal.tiene_capa_2());
        assert!(!EstadoContenido::RequiereOcr.tiene_capa_2());
    }

    #[test]
    fn test_vocabularios_son_reversibles() {
        for valor in EstadoContenido::VALORES_CANONICOS {
            assert_eq!(EstadoContenido::parse(valor).unwrap().as_str(), valor);
        }
        for valor in TipoBloque::VALORES_CANONICOS {
            assert_eq!(TipoBloque::parse(valor).unwrap().as_str(), valor);
        }
        assert_eq!(NivelJerarquico::PREAMBULO.valor(), 0);
    }
}
