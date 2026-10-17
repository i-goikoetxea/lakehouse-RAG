//! Modelado del grafo de referencias normativas y precedencia jurídica (Tabla 06)

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;
use crate::domain::services::normalizacion::normalizar_token;

/// Tipo de relación entre dos nodos normativos: vocabulario cerrado de la especificación §4
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoRelacionNormativa {
    ExigeLlenarRegistro,
    DerogaA,
    ModificaApartado,
    RemiteATabla,
    SubordinadoANorma,
    ReferenciaA,
}

impl TipoRelacionNormativa {
    /// Vocabulario cerrado que aceptan el JSON Schema de destilación y la Tabla 06
    pub const VALORES_CANONICOS: [&'static str; 6] = [
        "EXIGE_LLENAR_REGISTRO",
        "DEROGA_A",
        "MODIFICA_APARTADO",
        "REMITE_A_TABLA",
        "SUBORDINADO_A_NORMA",
        "REFERENCIA_A",
    ];

    /// Interpretación estricta: valor canónico o sinónimo conocido; cualquier otro es error
    pub fn parse(valor: impl AsRef<str>) -> Result<Self, DomainError> {
        let token = normalizar_token(valor.as_ref());
        if token.is_empty() {
            return Err(DomainError::ValidationError(
                "El tipo de relación normativa no puede estar vacío".to_string(),
            ));
        }
        Self::desde_token(&token).ok_or_else(|| {
            DomainError::ValidationError(format!(
                "Tipo de relación fuera del vocabulario §4: '{}'",
                valor.as_ref().trim()
            ))
        })
    }

    /// Interpretación tolerante para datos heredados: lo no reconocido se degrada a
    /// `REFERENCIA_A`. El booleano indica si se aplicó ese respaldo.
    pub fn parse_con_respaldo(valor: impl AsRef<str>) -> (Self, bool) {
        match Self::parse(valor) {
            Ok(tipo) => (tipo, false),
            Err(_) => (Self::ReferenciaA, true),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ExigeLlenarRegistro => "EXIGE_LLENAR_REGISTRO",
            Self::DerogaA => "DEROGA_A",
            Self::ModificaApartado => "MODIFICA_APARTADO",
            Self::RemiteATabla => "REMITE_A_TABLA",
            Self::SubordinadoANorma => "SUBORDINADO_A_NORMA",
            Self::ReferenciaA => "REFERENCIA_A",
        }
    }

    fn desde_token(token: &str) -> Option<Self> {
        match token {
            "EXIGE_LLENAR_REGISTRO" | "EXIGE_REGISTRO" | "LLENA_REGISTRO" | "REQUIERE_REGISTRO" => {
                Some(Self::ExigeLlenarRegistro)
            }
            "DEROGA_A" | "DEROGA" | "SUSTITUYE_A" | "SUSTITUYE" | "ANULA_A" | "ANULA" => {
                Some(Self::DerogaA)
            }
            "MODIFICA_APARTADO" | "MODIFICA_A" | "MODIFICA" | "ENMIENDA_A" | "ENMIENDA" => {
                Some(Self::ModificaApartado)
            }
            "REMITE_A_TABLA" | "REMITE_TABLA" | "CONSULTA_TABLA" => Some(Self::RemiteATabla),
            "SUBORDINADO_A_NORMA" | "SUBORDINADO_A" | "SUBORDINADO" | "SE_SUBORDINA_A" => {
                Some(Self::SubordinadoANorma)
            }
            "REFERENCIA_A" | "REFERENCIA" | "REFERENCIAS" | "COMPLEMENTA_A" | "COMPLEMENTA"
            | "REMITE_A" | "REMITE" | "CITA_A" | "CITA" => Some(Self::ReferenciaA),
            _ => None,
        }
    }
}

impl std::fmt::Display for TipoRelacionNormativa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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

        if tipo_relacion == TipoRelacionNormativa::DerogaA
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
    fn test_tipo_relacion_parse_estricto_con_sinonimos() {
        let casos = [
            (
                "EXIGE_LLENAR_REGISTRO",
                TipoRelacionNormativa::ExigeLlenarRegistro,
            ),
            ("deroga", TipoRelacionNormativa::DerogaA),
            ("DEROGA_A", TipoRelacionNormativa::DerogaA),
            ("Modifica apartado", TipoRelacionNormativa::ModificaApartado),
            ("REMITE_A_TABLA", TipoRelacionNormativa::RemiteATabla),
            ("subordinado_a", TipoRelacionNormativa::SubordinadoANorma),
            ("Referencia", TipoRelacionNormativa::ReferenciaA),
            ("COMPLEMENTA", TipoRelacionNormativa::ReferenciaA),
            ("remite_a", TipoRelacionNormativa::ReferenciaA),
            ("CITA", TipoRelacionNormativa::ReferenciaA),
        ];
        for (entrada, esperado) in casos {
            assert_eq!(
                TipoRelacionNormativa::parse(entrada).unwrap(),
                esperado,
                "{entrada}"
            );
        }
        assert_eq!(TipoRelacionNormativa::DerogaA.as_str(), "DEROGA_A");
        assert!(TipoRelacionNormativa::parse("armoniza_con").is_err());
        assert!(TipoRelacionNormativa::parse("  ").is_err());
    }

    #[test]
    fn test_tipo_relacion_parse_con_respaldo() {
        assert_eq!(
            TipoRelacionNormativa::parse_con_respaldo("DEROGA"),
            (TipoRelacionNormativa::DerogaA, false)
        );
        assert_eq!(
            TipoRelacionNormativa::parse_con_respaldo("armoniza_con"),
            (TipoRelacionNormativa::ReferenciaA, true)
        );
        for valor in TipoRelacionNormativa::VALORES_CANONICOS {
            assert_eq!(TipoRelacionNormativa::parse(valor).unwrap().as_str(), valor);
        }
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
            TipoRelacionNormativa::DerogaA,
            1.0,
            Some("La nueva disposición deroga el procedimiento anterior".to_string()),
        )
        .expect("Arista válida");

        assert_eq!(arista.origen_doc_id, doc_a);
        assert_eq!(arista.destino_doc_id, doc_b);
        assert_eq!(arista.tipo_relacion, TipoRelacionNormativa::DerogaA);
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
            TipoRelacionNormativa::ReferenciaA,
            -0.5,
            None,
        )
        .unwrap_err();

        assert!(matches!(err, DomainError::ValidationError(_)));
    }
}
