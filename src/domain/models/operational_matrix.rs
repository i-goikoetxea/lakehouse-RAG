//! Modelado de la matriz operacional y fases de procesos industriales (Tabla 03)

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;

/// Fases operativas estandarizadas de procesos industriales
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FaseOperativa {
    Recepcion,
    Custodia,
    Inspeccion,
    Despacho,
    Mantenimiento,
    Personalizada(String),
}

impl FaseOperativa {
    pub fn parse(valor: impl AsRef<str>) -> Result<Self, DomainError> {
        let v = valor.as_ref().trim();
        if v.is_empty() {
            return Err(DomainError::ValidationError(
                "La fase operativa no puede estar vacía".to_string(),
            ));
        }

        match v.to_lowercase().as_str() {
            "recepcion" | "recepción" => Ok(Self::Recepcion),
            "custodia" => Ok(Self::Custodia),
            "inspeccion" | "inspección" => Ok(Self::Inspeccion),
            "despacho" => Ok(Self::Despacho),
            "mantenimiento" => Ok(Self::Mantenimiento),
            otro => Ok(Self::Personalizada(otro.to_string())),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Recepcion => "Recepcion",
            Self::Custodia => "Custodia",
            Self::Inspeccion => "Inspeccion",
            Self::Despacho => "Despacho",
            Self::Mantenimiento => "Mantenimiento",
            Self::Personalizada(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for FaseOperativa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Tupla atómica desnormalizada de la matriz operacional
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuplaOperativa {
    pub id: String,
    pub doc_id: DocId,
    pub fase: FaseOperativa,
    pub parametro: String,
    pub condicion: String,
    pub valor_referencia: String,
    pub unidad_medida: Option<String>,
    pub tolerancia: Option<String>,
    pub nivel_criticidad: String,
    pub seccion_origen: String,
    pub vigente: bool,
}

impl TuplaOperativa {
    pub const MAX_LONGITUD_CAMPO: usize = 16_384;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        doc_id: DocId,
        fase: FaseOperativa,
        parametro: impl Into<String>,
        condicion: impl Into<String>,
        valor_referencia: impl Into<String>,
        unidad_medida: Option<String>,
        tolerancia: Option<String>,
        nivel_criticidad: impl Into<String>,
        seccion_origen: impl Into<String>,
    ) -> Result<Self, DomainError> {
        let id = id.into().trim().to_string();
        if id.is_empty() {
            return Err(DomainError::ValidationError(
                "El id de la tupla no puede estar vacío".to_string(),
            ));
        }

        let parametro = parametro.into().trim().to_string();
        if parametro.is_empty() {
            return Err(DomainError::ValidationError(
                "El parámetro de la tupla no puede estar vacío".to_string(),
            ));
        }
        if parametro.len() > Self::MAX_LONGITUD_CAMPO {
            return Err(DomainError::ValidationError(format!(
                "El parámetro excede la longitud máxima permitida de {} bytes",
                Self::MAX_LONGITUD_CAMPO
            )));
        }

        let condicion = condicion.into().trim().to_string();
        if condicion.is_empty() {
            return Err(DomainError::ValidationError(
                "La condición no puede estar vacía".to_string(),
            ));
        }
        if condicion.len() > Self::MAX_LONGITUD_CAMPO {
            return Err(DomainError::ValidationError(format!(
                "La condición excede la longitud máxima permitida de {} bytes",
                Self::MAX_LONGITUD_CAMPO
            )));
        }

        let seccion = seccion_origen.into().trim().to_string();
        if seccion.is_empty() {
            return Err(DomainError::ValidationError(
                "La sección de origen no puede estar vacía".to_string(),
            ));
        }

        Ok(Self {
            id,
            doc_id,
            fase,
            parametro,
            condicion,
            valor_referencia: valor_referencia.into().trim().to_string(),
            unidad_medida: unidad_medida.map(|u| u.trim().to_string()),
            tolerancia: tolerancia.map(|t| t.trim().to_string()),
            nivel_criticidad: nivel_criticidad.into().trim().to_string(),
            seccion_origen: seccion,
            vigente: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fase_operativa_parsing() {
        assert_eq!(
            FaseOperativa::parse("recepcion").unwrap(),
            FaseOperativa::Recepcion
        );
        assert_eq!(
            FaseOperativa::parse("Recepción").unwrap(),
            FaseOperativa::Recepcion
        );
        assert_eq!(
            FaseOperativa::parse("custodia").unwrap(),
            FaseOperativa::Custodia
        );
        assert_eq!(
            FaseOperativa::parse("inspección").unwrap(),
            FaseOperativa::Inspeccion
        );
        assert_eq!(
            FaseOperativa::parse("despacho").unwrap(),
            FaseOperativa::Despacho
        );
        assert_eq!(
            FaseOperativa::parse("mantenimiento").unwrap(),
            FaseOperativa::Mantenimiento
        );

        let custom = FaseOperativa::parse("almacenamiento_temporal").unwrap();
        assert_eq!(
            custom,
            FaseOperativa::Personalizada("almacenamiento_temporal".to_string())
        );
        assert_eq!(custom.as_str(), "almacenamiento_temporal");
    }

    #[test]
    fn test_fase_operativa_vacia_falla() {
        let err = FaseOperativa::parse("   ").unwrap_err();
        assert!(matches!(err, DomainError::ValidationError(_)));
    }

    #[test]
    fn test_tupla_operativa_valida() {
        let doc_id = DocId::new("DOC-001").unwrap();
        let tupla = TuplaOperativa::new(
            "TUP-001",
            doc_id,
            FaseOperativa::Inspeccion,
            "Presión hidrostática",
            "Prueba de estanqueidad a 20°C",
            "150",
            Some("psi".to_string()),
            Some("± 2%".to_string()),
            "Critica",
            "Cláusula 5.3",
        )
        .expect("Tupla válida");

        assert_eq!(tupla.parametro, "Presión hidrostática");
        assert_eq!(tupla.fase, FaseOperativa::Inspeccion);
        assert_eq!(tupla.unidad_medida.as_deref(), Some("psi"));
        assert!(tupla.vigente);
    }

    #[test]
    fn test_tupla_operativa_invalida_sin_parametro() {
        let doc_id = DocId::new("DOC-001").unwrap();
        let err = TuplaOperativa::new(
            "TUP-001",
            doc_id,
            FaseOperativa::Recepcion,
            "  ",
            "Condicion",
            "100",
            None,
            None,
            "Alta",
            "Sec 1",
        )
        .unwrap_err();

        assert!(matches!(err, DomainError::ValidationError(_)));
    }
}
