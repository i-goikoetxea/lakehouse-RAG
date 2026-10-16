//! Errores de Dominio tipados

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Clone, Eq)]
pub enum DomainError {
    #[error("Identificador de centro inválido: '{0}'")]
    InvalidCenterId(String),

    #[error("Identificador de documento inválido: '{0}'")]
    InvalidDocId(String),

    #[error("Nivel de prelación fuera de rango: {0} (debe estar entre 1 y 5)")]
    InvalidPrelacionLevel(i8),

    #[error("Rango numérico inválido: {0}")]
    InvalidRange(String),

    #[error("Conflicto de versiones: el documento '{0}' ya existe con una revisión vigente")]
    VersionConflict(String),

    #[error("Error de validación de entidad: '{0}'")]
    ValidationError(String),

    #[error("Error de almacenamiento en Lakehouse: '{0}'")]
    StorageError(String),

    #[error("Error en motor analítico o consulta: '{0}'")]
    QueryError(String),

    #[error("Error en grafo normativo: '{0}'")]
    GraphError(String),

    #[error("Error en motor de embeddings: '{0}'")]
    EmbeddingError(String),

    #[error("Error en búsqueda híbrida: '{0}'")]
    SearchError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_error_display() {
        let err = DomainError::InvalidPrelacionLevel(6);
        assert_eq!(
            err.to_string(),
            "Nivel de prelación fuera de rango: 6 (debe estar entre 1 y 5)"
        );

        let range_err = DomainError::InvalidRange("min (20) > max (10)".to_string());
        assert_eq!(
            range_err.to_string(),
            "Rango numérico inválido: min (20) > max (10)"
        );

        let doc_err = DomainError::InvalidDocId("".to_string());
        assert_eq!(
            doc_err.to_string(),
            "Identificador de documento inválido: ''"
        );
    }
}
