//! Decide cómo arranca el turno según la intención. No toca el puerto: solo produce el plan que
//! el orquestador aplicará en la Fase 2.

use super::intent_detector::IntencionTurno;
use crate::application::dtos::tools::LakehouseToolName;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EleccionHerramienta {
    Libre,
    /// Se pide la tool con `--json-schema`. Es una directiva: el modelo puede no llamarla
    Dirigida(LakehouseToolName),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanTurno {
    pub primer_paso: EleccionHerramienta,
    pub retener_texto: bool,
}

pub struct PreInferenceInterceptor;

impl PreInferenceInterceptor {
    pub fn plan(intencion: &IntencionTurno) -> PlanTurno {
        let primer_paso = match intencion {
            IntencionTurno::TabularCruzable { .. } => {
                EleccionHerramienta::Dirigida(LakehouseToolName::LookupDoubleEntryTable)
            }
            IntencionTurno::Responsable => {
                EleccionHerramienta::Dirigida(LakehouseToolName::QueryOperationalMatrix)
            }
            IntencionTurno::Ninguna
            | IntencionTurno::TabularIncompleta
            | IntencionTurno::DatoConcreto => EleccionHerramienta::Libre,
        };
        PlanTurno {
            primer_paso,
            retener_texto: *intencion != IntencionTurno::Ninguna,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::use_cases::arnes_turno::IntentDetector;

    fn plan_de(mensaje: &str) -> PlanTurno {
        PreInferenceInterceptor::plan(&IntentDetector::clasificar(mensaje))
    }

    #[test]
    fn test_cruzable_dirige_lookup_en_el_primer_paso() {
        assert_eq!(
            plan_de("Tabla 4-A, ¿temperatura para 30 mm y CE 0,42 %?"),
            PlanTurno {
                primer_paso: EleccionHerramienta::Dirigida(
                    LakehouseToolName::LookupDoubleEntryTable
                ),
                retener_texto: true,
            }
        );
    }

    #[test]
    fn test_cruzable_con_tabla_en_historial_sigue_dirigiendo_lookup() {
        // El plan solo depende del mensaje actual: tener la tabla en el historial no lo cambia,
        // aunque la pregunta se limite a los dos valores y a la magnitud.
        assert_eq!(
            plan_de("Con la tabla anterior, ¿qué temperatura para 30 mm y CE 0,42 %?").primer_paso,
            EleccionHerramienta::Dirigida(LakehouseToolName::LookupDoubleEntryTable)
        );
    }

    #[test]
    fn test_un_solo_eje_no_dirige_lookup() {
        assert_eq!(
            plan_de("¿Qué tolerancia corresponde a 30 mm?"),
            PlanTurno {
                primer_paso: EleccionHerramienta::Libre,
                retener_texto: true,
            }
        );
    }

    #[test]
    fn test_responsable_dirige_la_matriz() {
        assert_eq!(
            plan_de("¿Quién firma el registro de recepción?"),
            PlanTurno {
                primer_paso: EleccionHerramienta::Dirigida(
                    LakehouseToolName::QueryOperationalMatrix
                ),
                retener_texto: true,
            }
        );
    }

    #[test]
    fn test_dato_concreto_retiene_el_texto_sin_dirigir_ninguna_tool() {
        assert_eq!(
            plan_de("¿Qué presión tiene la línea L-999?"),
            PlanTurno {
                primer_paso: EleccionHerramienta::Libre,
                retener_texto: true,
            }
        );
    }

    #[test]
    fn test_sin_intencion_mantiene_flujo_libre_y_streaming() {
        assert_eq!(
            plan_de("¿Qué es el refuerzo de soldadura?"),
            PlanTurno {
                primer_paso: EleccionHerramienta::Libre,
                retener_texto: false,
            }
        );
    }
}
