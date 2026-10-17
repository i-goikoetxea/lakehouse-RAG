//! Modelos y Value Objects para la gestión del catálogo de documentos normativos

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::services::normalizacion::codigo_canonico;
use crate::domain::services::normalizacion::normalizar_token;

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

        if let Some(prohibido) = cleaned.chars().find(|c| !Self::es_caracter_permitido(*c)) {
            return Err(DomainError::InvalidDocId(format!(
                "El identificador contiene el carácter no permitido {prohibido:?}; solo se admite [A-Za-z0-9._-]"
            )));
        }

        if cleaned.contains("..") || !cleaned.chars().any(|c| c.is_ascii_alphanumeric()) {
            return Err(DomainError::InvalidDocId(
                "El identificador debe contener alfanuméricos y no puede contener '..'".to_string(),
            ));
        }

        Ok(Self(cleaned))
    }

    /// Construye el identificador a partir de un código escrito libremente
    /// (`UNE-EN ISO 15614-1` → `UNE-EN-ISO-15614-1`)
    pub fn desde_codigo_libre(codigo: &str) -> Result<Self, DomainError> {
        Self::new(codigo_canonico(codigo))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn es_caracter_permitido(caracter: char) -> bool {
        caracter.is_ascii_alphanumeric() || matches!(caracter, '.' | '_' | '-')
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

/// Nivel de prelación técnica/contractual (1 a 5)
///
/// 1 = Máxima prelación (especificación del cliente)
/// 5 = Menor prelación (norma supletoria o formato de registro). Ver [`TipoDocumento::nivel_prelacion`].
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

/// Tipo de documento normativo; determina su nivel de prelación (problema 5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoDocumento {
    EspecificacionCliente,
    ProcedimientoObra,
    ProcedimientoGeneral,
    InstructivoTecnico,
    NormaInternacional,
    FormatoRegistro,
}

impl TipoDocumento {
    /// Vocabulario cerrado que aceptan el JSON Schema de destilación y las tablas Arrow
    pub const VALORES_CANONICOS: [&'static str; 6] = [
        "ESPECIFICACION_CLIENTE",
        "PROCEDIMIENTO_OBRA",
        "PROCEDIMIENTO_GENERAL",
        "INSTRUCTIVO_TECNICO",
        "NORMA_INTERNACIONAL",
        "FORMATO_REGISTRO",
    ];

    /// Interpreta el valor canónico o un sinónimo (pliego, procedimiento, norma/UNE/ISO,
    /// instrucción, formato). Se decide por la primera palabra del texto normalizado.
    pub fn parse(valor: impl AsRef<str>) -> Result<Self, DomainError> {
        let token = normalizar_token(valor.as_ref());
        if let Some(canonico) = Self::desde_canonico(&token) {
            return Ok(canonico);
        }
        let palabras: Vec<&str> = token.split('_').collect();
        Self::desde_palabra_inicial(&palabras).ok_or_else(|| {
            DomainError::ValidationError(format!(
                "Tipo de documento no reconocido: '{}'",
                valor.as_ref().trim()
            ))
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EspecificacionCliente => "ESPECIFICACION_CLIENTE",
            Self::ProcedimientoObra => "PROCEDIMIENTO_OBRA",
            Self::ProcedimientoGeneral => "PROCEDIMIENTO_GENERAL",
            Self::InstructivoTecnico => "INSTRUCTIVO_TECNICO",
            Self::NormaInternacional => "NORMA_INTERNACIONAL",
            Self::FormatoRegistro => "FORMATO_REGISTRO",
        }
    }

    /// Nivel de prelación: 1 = especificación del cliente … 5 = norma o formato
    pub fn nivel_prelacion(&self) -> NivelPrelacion {
        let nivel = match self {
            Self::EspecificacionCliente => 1,
            Self::ProcedimientoObra => 2,
            Self::ProcedimientoGeneral => 3,
            Self::InstructivoTecnico => 4,
            Self::NormaInternacional | Self::FormatoRegistro => 5,
        };
        NivelPrelacion(nivel)
    }

    fn desde_canonico(token: &str) -> Option<Self> {
        [
            Self::EspecificacionCliente,
            Self::ProcedimientoObra,
            Self::ProcedimientoGeneral,
            Self::InstructivoTecnico,
            Self::NormaInternacional,
            Self::FormatoRegistro,
        ]
        .into_iter()
        .find(|tipo| tipo.as_str() == token)
    }

    fn desde_palabra_inicial(palabras: &[&str]) -> Option<Self> {
        let es_de_obra = palabras.iter().any(|p| matches!(*p, "OBRA" | "PROYECTO"));
        match *palabras.first()? {
            "PLIEGO" | "ESPECIFICACION" | "ESPECIFICACIONES" | "CLIENTE" | "CONTRATO" => {
                Some(Self::EspecificacionCliente)
            }
            "PROCEDIMIENTO" | "PROCEDIMIENTOS" if es_de_obra => Some(Self::ProcedimientoObra),
            "PROCEDIMIENTO" | "PROCEDIMIENTOS" => Some(Self::ProcedimientoGeneral),
            "INSTRUCTIVO" | "INSTRUCCION" | "INSTRUCCIONES" | "IT" => {
                Some(Self::InstructivoTecnico)
            }
            "NORMA" | "NORMAS" | "NORMATIVA" | "UNE" | "ISO" | "EN" | "IEC" | "ASTM" | "ASME"
            | "AWS" | "DIN" => Some(Self::NormaInternacional),
            "FORMATO" | "FORMATOS" | "FORMULARIO" | "REGISTRO" | "PLANTILLA" => {
                Some(Self::FormatoRegistro)
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for TipoDocumento {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
    #[test]
    fn test_doc_id_rechaza_caracteres_fuera_del_conjunto_seguro() {
        for ataque in [
            "PO-ALM-03'; DROP TABLE x;--",
            "PO ALM 03",
            "PO-ALM-03@v3",
            "doc\"comillas",
            "../etc/passwd",
            "..",
            "Recepción",
        ] {
            assert!(DocId::new(ataque).is_err(), "Debe rechazar '{ataque}'");
        }
        assert_eq!(
            DocId::new(" PO-ALM-03.v2_b ").unwrap().as_str(),
            "PO-ALM-03.v2_b"
        );
    }

    #[test]
    fn test_doc_id_desde_codigo_libre_canonicaliza() {
        assert_eq!(
            DocId::desde_codigo_libre("UNE-EN ISO 15614-1")
                .unwrap()
                .as_str(),
            "UNE-EN-ISO-15614-1"
        );
        assert_eq!(
            DocId::desde_codigo_libre("po_alm_03").unwrap().as_str(),
            "PO-ALM-03"
        );
        assert!(DocId::desde_codigo_libre("¿?").is_err());
    }

    #[test]
    fn test_tipo_documento_parse_y_prelacion() {
        let casos = [
            (
                "ESPECIFICACION_CLIENTE",
                TipoDocumento::EspecificacionCliente,
                1,
            ),
            (
                "Pliego de condiciones técnicas",
                TipoDocumento::EspecificacionCliente,
                1,
            ),
            ("procedimiento de obra", TipoDocumento::ProcedimientoObra, 2),
            ("Procedimiento", TipoDocumento::ProcedimientoGeneral, 3),
            (
                "PROCEDIMIENTO_GENERAL",
                TipoDocumento::ProcedimientoGeneral,
                3,
            ),
            ("Instrucción técnica", TipoDocumento::InstructivoTecnico, 4),
            ("Norma UNE-EN 1090-2", TipoDocumento::NormaInternacional, 5),
            ("ISO 3834", TipoDocumento::NormaInternacional, 5),
            ("formato de registro", TipoDocumento::FormatoRegistro, 5),
        ];
        for (entrada, esperado, nivel) in casos {
            let tipo = TipoDocumento::parse(entrada).unwrap();
            assert_eq!(tipo, esperado, "{entrada}");
            assert_eq!(tipo.nivel_prelacion().valor(), nivel, "{entrada}");
        }
        assert_eq!(
            TipoDocumento::ProcedimientoObra.as_str(),
            "PROCEDIMIENTO_OBRA"
        );
        assert!(TipoDocumento::parse("acta de reunión").is_err());
        assert!(TipoDocumento::parse("   ").is_err());
        assert!(
            TipoDocumento::EspecificacionCliente
                .nivel_prelacion()
                .tiene_mayor_prelacion_que(&TipoDocumento::NormaInternacional.nivel_prelacion())
        );
    }

    #[test]
    fn test_tipo_documento_valores_canonicos_son_reparseables() {
        for valor in TipoDocumento::VALORES_CANONICOS {
            assert_eq!(TipoDocumento::parse(valor).unwrap().as_str(), valor);
        }
    }
}
