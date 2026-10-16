//! Modelado del grafo de referencias normativas y precedencia jurídica (Tabla 06)

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;

/// Tipo de relación o arista entre dos nodos normativos
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoRelacionNormativa {
    Deroga,
    Complementa,
    Referencia,
    Condiciona,
    Excepciona,
    Personalizada(String),
}

impl TipoRelacionNormativa {
    pub fn parse(valor: impl AsRef<str>) -> Result<Self, DomainError> {
        let v = valor.as_ref().trim();
        if v.is_empty() {
            return Err(DomainError::ValidationError(
                "El tipo de relación normativa no puede estar vacío".to_string(),
            ));
        }

        match v.to_lowercase().as_str() {
            "deroga" => Ok(Self::Deroga),
            "complementa" => Ok(Self::Complementa),
            "referencia" => Ok(Self::Referencia),
            "condiciona" => Ok(Self::Condiciona),
            "excepciona" => Ok(Self::Excepciona),
            otro => Ok(Self::Personalizada(otro.to_string())),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Deroga => "Deroga",
            Self::Complementa => "Complementa",
            Self::Referencia => "Referencia",
            Self::Condiciona => "Condiciona",
            Self::Excepciona => "Excepciona",
            Self::Personalizada(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for TipoRelacionNormativa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Arista dirigida que conecta dos cláusulas o documentos normativos
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AristaGrafo {
    pub id: String,
    pub origen_doc_id: DocId,
    pub origen_seccion: String,
    pub destino_doc_id: DocId,
    pub destino_seccion: String,
    pub tipo_relacion: TipoRelacionNormativa,
    pub peso_prelacion: f32,
    pub descripcion: Option<String>,
    pub vigente: bool,
}

impl AristaGrafo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        origen_doc_id: DocId,
        origen_seccion: impl Into<String>,
        destino_doc_id: DocId,
        destino_seccion: impl Into<String>,
        tipo_relacion: TipoRelacionNormativa,
        peso_prelacion: f32,
        descripcion: Option<String>,
    ) -> Result<Self, DomainError> {
        let id = id.into().trim().to_string();
        if id.is_empty() {
            return Err(DomainError::ValidationError(
                "El identificador de la arista no puede estar vacío".to_string(),
            ));
        }

        let o_sec = origen_seccion.into().trim().to_string();
        if o_sec.is_empty() {
            return Err(DomainError::ValidationError(
                "La sección de origen no puede estar vacía".to_string(),
            ));
        }

        let d_sec = destino_seccion.into().trim().to_string();
        if d_sec.is_empty() {
            return Err(DomainError::ValidationError(
                "La sección de destino no puede estar vacía".to_string(),
            ));
        }

        if !peso_prelacion.is_finite() || peso_prelacion < 0.0 {
            return Err(DomainError::ValidationError(
                "El peso de prelación debe ser un número real finito no negativo".to_string(),
            ));
        }

        if tipo_relacion == TipoRelacionNormativa::Deroga
            && origen_doc_id == destino_doc_id
            && o_sec.eq_ignore_ascii_case(&d_sec)
        {
            return Err(DomainError::ValidationError(
                "Paradoja lógica: una sección normativa no puede derogarse a sí misma".to_string(),
            ));
        }

        Ok(Self {
            id,
            origen_doc_id,
            origen_seccion: o_sec,
            destino_doc_id,
            destino_seccion: d_sec,
            tipo_relacion,
            peso_prelacion,
            descripcion: descripcion.map(|d| d.trim().to_string()),
            vigente: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tipo_relacion_parsing() {
        assert_eq!(
            TipoRelacionNormativa::parse("deroga").unwrap(),
            TipoRelacionNormativa::Deroga
        );
        assert_eq!(
            TipoRelacionNormativa::parse("Complementa").unwrap(),
            TipoRelacionNormativa::Complementa
        );
        assert_eq!(
            TipoRelacionNormativa::parse("condiciona").unwrap(),
            TipoRelacionNormativa::Condiciona
        );
        assert_eq!(
            TipoRelacionNormativa::parse("excepciona").unwrap(),
            TipoRelacionNormativa::Excepciona
        );

        let custom = TipoRelacionNormativa::parse("armoniza_con").unwrap();
        assert_eq!(
            custom,
            TipoRelacionNormativa::Personalizada("armoniza_con".to_string())
        );
    }

    #[test]
    fn test_arista_grafo_valida() {
        let doc_a = DocId::new("DOC-001").unwrap();
        let doc_b = DocId::new("DOC-002").unwrap();

        let arista = AristaGrafo::new(
            "EDGE-001",
            doc_a.clone(),
            "Sección 4.1",
            doc_b.clone(),
            "Sección 8.3",
            TipoRelacionNormativa::Deroga,
            1.0,
            Some("La nueva disposición deroga el procedimiento anterior".to_string()),
        )
        .expect("Arista válida");

        assert_eq!(arista.origen_doc_id, doc_a);
        assert_eq!(arista.destino_doc_id, doc_b);
        assert_eq!(arista.tipo_relacion, TipoRelacionNormativa::Deroga);
        assert!(arista.vigente);
    }

    #[test]
    fn test_arista_grafo_peso_negativo_falla() {
        let doc_a = DocId::new("DOC-001").unwrap();
        let doc_b = DocId::new("DOC-002").unwrap();

        let err = AristaGrafo::new(
            "EDGE-001",
            doc_a,
            "Sec 1",
            doc_b,
            "Sec 2",
            TipoRelacionNormativa::Referencia,
            -0.5,
            None,
        )
        .unwrap_err();

        assert!(matches!(err, DomainError::ValidationError(_)));
    }
}
