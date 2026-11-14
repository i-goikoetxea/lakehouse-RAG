//! Contrato del Nivel 2 del verificador de procedencia (SSD de generalidad §4.3). El modelo solo
//! propone trazas; Rust las comprueba sobre los bytes del turno y nunca aprueba por el modelo.

use serde::{Deserialize, Serialize};

/// Lo que se envía al verificador: lo mínimo para proponer trazas, sin historial
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolicitudProcedencia {
    pub pregunta: String,
    pub salidas: Vec<SalidaTurno>,
    pub borrador: String,
    /// Cifras del borrador que no pasaron ni el anclaje literal ni el Nivel 1
    pub cifras: Vec<String>,
}

/// Texto plano de una salida de tool del turno (sin marcas de página), con su id `t{i}`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SalidaTurno {
    pub id: String,
    pub herramienta: String,
    pub texto: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrazasPropuestas {
    pub trazas: Vec<Traza>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Traza {
    pub cifra: String,
    pub tipo: TipoTraza,
}

/// Los 4 tipos cerrados del esquema. Cualquier otra forma no se deserializa y la cifra no pasa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum TipoTraza {
    /// `0,42` = `0.42` (factor 0); `1500` mm = `1.5` m (factor -3)
    FormatoOEscala {
        fuente: Literal,
        factor_potencia_diez: i8,
    },
    Aritmetica {
        operacion: Operacion,
        operandos: Vec<Literal>,
    },
    ReferenciaEstructural {
        subcadena: Literal,
    },
    ConteoElementos {
        elementos: Vec<Literal>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operacion {
    Suma,
    Resta,
    Producto,
    Division,
    /// `a / b · 100`
    Porcentaje,
}

/// Subcadena exacta de un origen: el id de una salida (`t0`, `t1`…) o `pregunta`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Literal {
    pub origen: String,
    pub texto: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_un_tipo_fuera_del_esquema_no_se_deserializa() {
        let desconocido =
            r#"{"trazas": [{"cifra": "7", "tipo": {"tipo": "memoria", "valor": "7"}}]}"#;
        assert!(serde_json::from_str::<TrazasPropuestas>(desconocido).is_err());
        let extra = r#"{"trazas": [{"cifra": "7", "tipo": {"tipo": "referencia_estructural",
            "subcadena": {"origen": "t0", "texto": "Art. 7"}, "aprobada": true}}]}"#;
        assert!(serde_json::from_str::<TrazasPropuestas>(extra).is_err());
    }

    #[test]
    fn test_traza_aritmetica_se_lee_del_json() {
        let json = r#"{"trazas": [{"cifra": "30", "tipo": {"tipo": "aritmetica",
            "operacion": "resta", "operandos": [{"origen": "t0", "texto": "80"},
            {"origen": "t1", "texto": "50"}]}}]}"#;
        let trazas: TrazasPropuestas = serde_json::from_str(json).unwrap();
        assert!(matches!(
            trazas.trazas[0].tipo,
            TipoTraza::Aritmetica {
                operacion: Operacion::Resta,
                ..
            }
        ));
    }
}
