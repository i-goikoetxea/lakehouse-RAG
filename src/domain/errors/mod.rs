//! Errores de Dominio tipados

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("Identificador de centro inválido: '{0}'")]
    InvalidCenterId(String),

    #[error("Nivel de prelación fuera de rango: {0} (debe estar entre 1 y 5)")]
    InvalidPrelacionLevel(i8),

    #[error("Rango numérico inválido: min ({0}) > max ({1})")]
    InvalidRange(f64, f64),

    #[error("Conflicto de versiones: el documento '{0}' ya existe con una revisión vigente")]
    VersionConflict(String),
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

        let range_err = DomainError::InvalidRange(20.0, 10.0);
        assert_eq!(
            range_err.to_string(),
            "Rango numérico inválido: min (20) > max (10)"
        );
    }
}
