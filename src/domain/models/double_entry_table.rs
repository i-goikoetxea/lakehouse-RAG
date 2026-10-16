//! Modelado de tablas de doble entrada y evaluación determinista de rangos numéricos (Tabla 04)

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;

/// Límite inferior o superior de un intervalo numérico
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LimiteRango {
    Inclusivo(f64),
    Exclusivo(f64),
    Infinito,
}

/// Rango numérico para evaluación de condiciones técnicas (ej. temperatura, presión, espesor)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RangoNumerico {
    pub inferior: LimiteRango,
    pub superior: LimiteRango,
}

impl RangoNumerico {
    pub fn new(inferior: LimiteRango, superior: LimiteRango) -> Result<Self, DomainError> {
        let validar_finito = |limite: &LimiteRango| -> Result<(), DomainError> {
            match limite {
                LimiteRango::Inclusivo(val) | LimiteRango::Exclusivo(val) => {
                    if !val.is_finite() {
                        return Err(DomainError::InvalidRange(format!(
                            "Los límites numéricos deben ser valores finitos reales (se recibió: {val})"
                        )));
                    }
                }
                LimiteRango::Infinito => {}
            }
            Ok(())
        };

        validar_finito(&inferior)?;
        validar_finito(&superior)?;

        match (inferior, superior) {
            (LimiteRango::Inclusivo(min), LimiteRango::Inclusivo(max)) if min > max => Err(
                DomainError::InvalidRange(format!("min ({min}) > max ({max})")),
            ),
            (LimiteRango::Inclusivo(min), LimiteRango::Exclusivo(max)) if min >= max => {
                Err(DomainError::InvalidRange(format!(
                    "min ({min}) >= max ({max}) con extremo superior exclusivo"
                )))
            }
            (LimiteRango::Exclusivo(min), LimiteRango::Inclusivo(max)) if min >= max => {
                Err(DomainError::InvalidRange(format!(
                    "min ({min}) >= max ({max}) con extremo inferior exclusivo"
                )))
            }
            (LimiteRango::Exclusivo(min), LimiteRango::Exclusivo(max)) if min >= max => Err(
                DomainError::InvalidRange(format!("min ({min}) >= max ({max}) en rango abierto")),
            ),
            _ => Ok(Self { inferior, superior }),
        }
    }

    pub fn entre_inclusivo(min: f64, max: f64) -> Result<Self, DomainError> {
        Self::new(LimiteRango::Inclusivo(min), LimiteRango::Inclusivo(max))
    }

    pub fn entre_exclusivo(min: f64, max: f64) -> Result<Self, DomainError> {
        Self::new(LimiteRango::Exclusivo(min), LimiteRango::Exclusivo(max))
    }

    pub fn mayor_que(min: f64) -> Self {
        Self {
            inferior: LimiteRango::Exclusivo(min),
            superior: LimiteRango::Infinito,
        }
    }

    pub fn mayor_o_igual_que(min: f64) -> Self {
        Self {
            inferior: LimiteRango::Inclusivo(min),
            superior: LimiteRango::Infinito,
        }
    }

    pub fn menor_que(max: f64) -> Self {
        Self {
            inferior: LimiteRango::Infinito,
            superior: LimiteRango::Exclusivo(max),
        }
    }

    pub fn menor_o_igual_que(max: f64) -> Self {
        Self {
            inferior: LimiteRango::Infinito,
            superior: LimiteRango::Inclusivo(max),
        }
    }

    pub fn exacto(valor: f64) -> Self {
        Self {
            inferior: LimiteRango::Inclusivo(valor),
            superior: LimiteRango::Inclusivo(valor),
        }
    }

    /// Evalúa si un valor numérico pertenece al rango especificado
    pub fn contiene(&self, valor: f64) -> bool {
        if !valor.is_finite() {
            return false;
        }

        let cumple_inferior = match self.inferior {
            LimiteRango::Inclusivo(min) => valor >= min,
            LimiteRango::Exclusivo(min) => valor > min,
            LimiteRango::Infinito => true,
        };

        let cumple_superior = match self.superior {
            LimiteRango::Inclusivo(max) => valor <= max,
            LimiteRango::Exclusivo(max) => valor < max,
            LimiteRango::Infinito => true,
        };

        cumple_inferior && cumple_superior
    }
}

/// Celda unitaria de una matriz o tabla de doble entrada técnica
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CeldaDobleEntrada {
    pub id: String,
    pub doc_id: DocId,
    pub nombre_tabla: String,
    pub encabezado_fila: String,
    pub rango_fila: Option<RangoNumerico>,
    pub encabezado_columna: String,
    pub rango_columna: Option<RangoNumerico>,
    pub valor_resultado: String,
    pub unidad_medida: Option<String>,
    pub notas_al_pie: Vec<String>,
    pub vigente: bool,
}

impl CeldaDobleEntrada {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        doc_id: DocId,
        nombre_tabla: impl Into<String>,
        encabezado_fila: impl Into<String>,
        rango_fila: Option<RangoNumerico>,
        encabezado_columna: impl Into<String>,
        rango_columna: Option<RangoNumerico>,
        valor_resultado: impl Into<String>,
        unidad_medida: Option<String>,
        notas_al_pie: Vec<String>,
    ) -> Result<Self, DomainError> {
        let id = id.into().trim().to_string();
        if id.is_empty() {
            return Err(DomainError::ValidationError(
                "El identificador de la celda no puede estar vacío".to_string(),
            ));
        }

        let nombre = nombre_tabla.into().trim().to_string();
        if nombre.is_empty() {
            return Err(DomainError::ValidationError(
                "El nombre de la tabla no puede estar vacío".to_string(),
            ));
        }

        let fila = encabezado_fila.into().trim().to_string();
        let col = encabezado_columna.into().trim().to_string();

        Ok(Self {
            id,
            doc_id,
            nombre_tabla: nombre,
            encabezado_fila: fila,
            rango_fila,
            encabezado_columna: col,
            rango_columna,
            valor_resultado: valor_resultado.into().trim().to_string(),
            unidad_medida: unidad_medida.map(|u| u.trim().to_string()),
            notas_al_pie,
            vigente: true,
        })
    }

    /// Evalúa si esta celda coincide con los parámetros de consulta dados
    pub fn coincide(
        &self,
        etiqueta_fila: Option<&str>,
        valor_fila: Option<f64>,
        etiqueta_columna: Option<&str>,
        valor_columna: Option<f64>,
    ) -> bool {
        // Validación de fila
        let match_fila = if let Some(vf) = valor_fila {
            self.rango_fila.as_ref().is_some_and(|r| r.contiene(vf))
        } else if let Some(ef) = etiqueta_fila {
            self.encabezado_fila.eq_ignore_ascii_case(ef.trim())
        } else {
            true
        };

        // Validación de columna
        let match_col = if let Some(vc) = valor_columna {
            self.rango_columna.as_ref().is_some_and(|r| r.contiene(vc))
        } else if let Some(ec) = etiqueta_columna {
            self.encabezado_columna.eq_ignore_ascii_case(ec.trim())
        } else {
            true
        };

        match_fila && match_col
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rango_inclusivo_evaluacion() {
        let rango = RangoNumerico::entre_inclusivo(10.0, 50.0).expect("Rango válido");
        assert!(rango.contiene(10.0));
        assert!(rango.contiene(30.0));
        assert!(rango.contiene(50.0));
        assert!(!rango.contiene(9.99));
        assert!(!rango.contiene(50.01));
    }

    #[test]
    fn test_rango_exclusivo_evaluacion() {
        let rango = RangoNumerico::entre_exclusivo(10.0, 50.0).expect("Rango válido");
        assert!(!rango.contiene(10.0));
        assert!(rango.contiene(10.01));
        assert!(rango.contiene(49.99));
        assert!(!rango.contiene(50.0));
    }

    #[test]
    fn test_rango_abierto_mayor_menor() {
        let mayor = RangoNumerico::mayor_que(100.0);
        assert!(!mayor.contiene(100.0));
        assert!(mayor.contiene(100.1));
        assert!(mayor.contiene(5000.0));

        let menor_o_igual = RangoNumerico::menor_o_igual_que(25.0);
        assert!(menor_o_igual.contiene(-10.0));
        assert!(menor_o_igual.contiene(25.0));
        assert!(!menor_o_igual.contiene(25.001));
    }

    #[test]
    fn test_rango_invalido_min_mayor_que_max() {
        let err = RangoNumerico::entre_inclusivo(50.0, 10.0).unwrap_err();
        match err {
            DomainError::InvalidRange(msg) => {
                assert!(msg.contains("min (50) > max (10)"));
            }
            other => panic!("Error inesperado: {other:?}"),
        }
    }

    #[test]
    fn test_celda_doble_entrada_cruce_exitoso() {
        let doc_id = DocId::new("DOC-002").unwrap();
        let rango_temp = RangoNumerico::entre_inclusivo(0.0, 100.0).unwrap();
        let celda = CeldaDobleEntrada::new(
            "CELDA-01",
            doc_id,
            "Tabla 4.1: Espesores Mínimos",
            "Acero al Carbón",
            None,
            "0-100 C",
            Some(rango_temp),
            "6.35",
            Some("mm".to_string()),
            vec!["Nota 1: Aplica solo para fluidos no corrosivos".to_string()],
        )
        .expect("Celda válida");

        // Coincide por etiqueta de material y rango de temperatura 50°C
        assert!(celda.coincide(Some("Acero al Carbón"), None, None, Some(50.0)));

        // No coincide con otra temperatura fuera de rango
        assert!(!celda.coincide(Some("Acero al Carbón"), None, None, Some(150.0)));

        // No coincide con otro material
        assert!(!celda.coincide(Some("Acero Inoxidable"), None, None, Some(50.0)));
    }
}
