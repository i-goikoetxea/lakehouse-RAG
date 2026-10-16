//! Modelos y Value Objects para la gestión del catálogo de documentos normativos

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

/// Identificador único inmutable de un documento dentro del catálogo
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocId(String);

impl DocId {
    pub const MAX_LONGITUD: usize = 256;

    pub fn new(id: impl Into<String>) -> Result<Self, DomainError> {
        let cleaned = id.into().trim().to_string();
        if cleaned.is_empty() {
            return Err(DomainError::InvalidDocId(
                "El identificador de documento no puede estar vacío".to_string(),
            ));
        }

        if cleaned.len() > Self::MAX_LONGITUD {
            return Err(DomainError::InvalidDocId(format!(
                "El identificador de documento excede la longitud máxima permitida de {} caracteres",
                Self::MAX_LONGITUD
            )));
        }

        if cleaned.chars().any(|c| c.is_control() || c == '\0') {
            return Err(DomainError::InvalidDocId(
                "El identificador contiene caracteres de control o bytes nulos no permitidos"
                    .to_string(),
            ));
        }

        Ok(Self(cleaned))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DocId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Deref for DocId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Nivel de prelación jurídica/técnica (1 a 5)
///
/// 1 = Máxima prelación (ej. Norma Oficial, Ley de Seguridad)
/// 5 = Menor prelación (ej. Guía informativa, recomendación interna)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NivelPrelacion(i8);

impl NivelPrelacion {
    pub const MAX_PRELACION: i8 = 1;
    pub const MIN_PRELACION: i8 = 5;

    pub fn new(nivel: i8) -> Result<Self, DomainError> {
        if !(Self::MAX_PRELACION..=Self::MIN_PRELACION).contains(&nivel) {
            return Err(DomainError::InvalidPrelacionLevel(nivel));
        }
        Ok(Self(nivel))
    }

    pub fn valor(&self) -> i8 {
        self.0
    }

    /// Retorna verdadero si este nivel tiene mayor jerarquía jurídica que `otro`.
    /// Recordar: Nivel 1 tiene mayor prelación que Nivel 2.
    pub fn tiene_mayor_prelacion_que(&self, otro: &Self) -> bool {
        self.0 < otro.0
    }
}

impl std::fmt::Display for NivelPrelacion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Nivel {}", self.0)
    }
}

/// Entidad que representa un documento en el catálogo maestro (Tabla 01)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentoCatalogo {
    pub id: DocId,
    pub center_id: CenterId,
    pub codigo: String,
    pub titulo: String,
    pub version: u32,
    pub nivel_prelacion: NivelPrelacion,
    pub hash_sha256: String,
    pub ruta_origen: String,
    pub vigente: bool,
    pub fecha_registro: DateTime<Utc>,
}

impl DocumentoCatalogo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: DocId,
        center_id: CenterId,
        codigo: impl Into<String>,
        titulo: impl Into<String>,
        version: u32,
        nivel_prelacion: NivelPrelacion,
        hash_sha256: impl Into<String>,
        ruta_origen: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let codigo = codigo.into().trim().to_string();
        if codigo.is_empty() {
            return Err(DomainError::ValidationError(
                "El código del documento no puede estar vacío".to_string(),
            ));
        }

        let titulo = titulo.into().trim().to_string();
        if titulo.is_empty() {
            return Err(DomainError::ValidationError(
                "El título del documento no puede estar vacío".to_string(),
            ));
        }

        let hash = hash_sha256.into().trim().to_string();
        if hash.is_empty() {
            return Err(DomainError::ValidationError(
                "El hash SHA-256 no puede estar vacío".to_string(),
            ));
        }

        Ok(Self {
            id,
            center_id,
            codigo,
            titulo,
            version,
            nivel_prelacion,
            hash_sha256: hash,
            ruta_origen: ruta_origen.into().trim().to_string(),
            vigente: true,
            fecha_registro: Utc::now(),
        })
    }

    /// Desmarca la vigencia del documento cuando es superado por una nueva versión
    pub fn retirar_vigencia(&mut self) {
        self.vigente = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doc_id_valido() {
        let doc_id = DocId::new("DOC-NOM-001").unwrap();
        assert_eq!(doc_id.as_str(), "DOC-NOM-001");
        assert_eq!(doc_id.to_string(), "DOC-NOM-001");
    }

    #[test]
    fn test_doc_id_vacio_falla() {
        let err = DocId::new("   ").unwrap_err();
        assert!(matches!(err, DomainError::InvalidDocId(_)));
    }

    #[test]
    fn test_nivel_prelacion_limites() {
        assert!(NivelPrelacion::new(1).is_ok());
        assert!(NivelPrelacion::new(3).is_ok());
        assert!(NivelPrelacion::new(5).is_ok());

        assert_eq!(
            NivelPrelacion::new(0).unwrap_err(),
            DomainError::InvalidPrelacionLevel(0)
        );
        assert_eq!(
            NivelPrelacion::new(6).unwrap_err(),
            DomainError::InvalidPrelacionLevel(6)
        );
    }

    #[test]
    fn test_prelacion_comparacion_jerarquica() {
        let nivel1 = NivelPrelacion::new(1).unwrap();
        let nivel3 = NivelPrelacion::new(3).unwrap();

        assert!(nivel1.tiene_mayor_prelacion_que(&nivel3));
        assert!(!nivel3.tiene_mayor_prelacion_que(&nivel1));
    }

    #[test]
    fn test_documento_catalogo_creacion_y_retiro_vigencia() {
        let doc_id = DocId::new("DOC-001").unwrap();
        let center_id = CenterId::new("planta-sur").unwrap();
        let prelacion = NivelPrelacion::new(2).unwrap();

        let mut doc = DocumentoCatalogo::new(
            doc_id.clone(),
            center_id.clone(),
            "NOM-STPS-001",
            "Condiciones de Seguridad",
            1,
            prelacion,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "/docs/nom-001.pdf",
        )
        .expect("Documento válido");

        assert_eq!(doc.id, doc_id);
        assert_eq!(doc.center_id, center_id);
        assert!(doc.vigente);

        doc.retirar_vigencia();
        assert!(!doc.vigente);
    }

    #[test]
    fn test_documento_catalogo_validacion_campos_vacios() {
        let doc_id = DocId::new("DOC-001").unwrap();
        let center_id = CenterId::new("planta-sur").unwrap();
        let prelacion = NivelPrelacion::new(2).unwrap();

        let err = DocumentoCatalogo::new(
            doc_id, center_id, "", "Titulo", 1, prelacion, "hash", "path",
        )
        .unwrap_err();

        assert!(matches!(err, DomainError::ValidationError(_)));
    }
}
