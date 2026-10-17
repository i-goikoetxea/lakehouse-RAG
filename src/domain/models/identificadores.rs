//! Identificadores primarios versionados: evitan colisiones entre revisiones (H4)

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;

/// Separador entre el documento y su versión. `DocId` nunca contiene `@`,
/// por lo que el identificador versionado es inequívoco.
pub const SEPARADOR_VERSION: char = '@';

/// Tipo de entidad del Lakehouse que recibe un identificador versionado
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoEntidad {
    Chunk,
    Hito,
    Tabla,
    Celda,
    Arista,
}

impl TipoEntidad {
    /// Prefijo de tres letras que aparece en el identificador
    pub fn prefijo(&self) -> &'static str {
        match self {
            Self::Chunk => "CHK",
            Self::Hito => "HIT",
            Self::Tabla => "TAB",
            Self::Celda => "CEL",
            Self::Arista => "ARI",
        }
    }
}

/// Identificador primario estable por documento, versión, tipo e índice
/// (`PO-ALM-03@v3-HIT-0002`).
pub fn id_versionado(doc_id: &DocId, version: u32, tipo: TipoEntidad, indice: usize) -> String {
    format!(
        "{doc_id}{SEPARADOR_VERSION}v{version}-{}-{indice:04}",
        tipo.prefijo()
    )
}

/// Identificador de una ingesta transaccional (staged → commit / rollback).
/// Solo admite `[A-Za-z0-9_-]`, por lo que puede viajar en un predicado SQL sin riesgo de inyección.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IngestId(String);

impl IngestId {
    pub const MAX_LONGITUD: usize = 64;

    pub fn new(valor: impl Into<String>) -> Result<Self, DomainError> {
        let limpio = valor.into().trim().to_string();
        if limpio.is_empty() || limpio.len() > Self::MAX_LONGITUD {
            return Err(DomainError::ValidationError(format!(
                "El ingest_id debe tener entre 1 y {} caracteres",
                Self::MAX_LONGITUD
            )));
        }
        let charset_valido = limpio
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
        if !charset_valido || !limpio.chars().any(|c| c.is_ascii_alphanumeric()) {
            return Err(DomainError::ValidationError(format!(
                "El ingest_id {limpio:?} solo admite [A-Za-z0-9_-] y al menos un alfanumérico"
            )));
        }
        Ok(Self(limpio))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for IngestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_versionado_distingue_revisiones_y_entidades() {
        let doc = DocId::new("PO-ALM-03").unwrap();
        let v3 = id_versionado(&doc, 3, TipoEntidad::Hito, 2);
        let v4 = id_versionado(&doc, 4, TipoEntidad::Hito, 2);
        assert_eq!(v3, "PO-ALM-03@v3-HIT-0002");
        assert_ne!(v3, v4);
        assert_ne!(v4, id_versionado(&doc, 4, TipoEntidad::Chunk, 2));
        for tipo in [
            TipoEntidad::Chunk,
            TipoEntidad::Hito,
            TipoEntidad::Tabla,
            TipoEntidad::Celda,
            TipoEntidad::Arista,
        ] {
            assert!(id_versionado(&doc, 1, tipo, 0).starts_with("PO-ALM-03@v1-"));
        }
    }

    #[test]
    fn test_ingest_id_solo_admite_charset_seguro() {
        let valido = IngestId::new(" ING-20261017_abc ").unwrap();
        assert_eq!(valido.as_str(), "ING-20261017_abc");
        assert_eq!(valido.to_string(), "ING-20261017_abc");

        for malicioso in [
            "",
            "   ",
            "x' OR '1'='1",
            "a;DROP",
            "a b",
            "a.b",
            "ñandú",
            "--",
            &"A".repeat(IngestId::MAX_LONGITUD + 1),
        ] {
            assert!(
                matches!(
                    IngestId::new(malicioso),
                    Err(DomainError::ValidationError(_))
                ),
                "debería rechazar {malicioso:?}"
            );
        }
    }
}
