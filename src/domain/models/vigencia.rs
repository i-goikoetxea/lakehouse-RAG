//! Ventanas de vigencia documental para time travel y auditoría retroactiva (problema 3)

use chrono::NaiveDate;

use crate::domain::errors::DomainError;

/// Intervalo semiabierto `[fecha_vigor, fecha_derogacion)` en el que una revisión está vigente.
/// Una ventana sin `fecha_derogacion` sigue abierta (revisión vigente hoy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VentanaVigencia {
    fecha_vigor: NaiveDate,
    fecha_derogacion: Option<NaiveDate>,
}

impl VentanaVigencia {
    /// Crea la ventana; la derogación, si existe, debe ser estrictamente posterior a la entrada en vigor
    pub fn new(
        fecha_vigor: NaiveDate,
        fecha_derogacion: Option<NaiveDate>,
    ) -> Result<Self, DomainError> {
        if let Some(derogacion) = fecha_derogacion {
            validar_derogacion_posterior(fecha_vigor, derogacion)?;
        }
        Ok(Self {
            fecha_vigor,
            fecha_derogacion,
        })
    }

    pub fn fecha_vigor(&self) -> NaiveDate {
        self.fecha_vigor
    }

    pub fn fecha_derogacion(&self) -> Option<NaiveDate> {
        self.fecha_derogacion
    }

    /// `fecha_vigor <= fecha AND (fecha_derogacion IS NULL OR fecha < fecha_derogacion)`
    pub fn vigente_en(&self, fecha: NaiveDate) -> bool {
        self.fecha_vigor <= fecha
            && self
                .fecha_derogacion
                .is_none_or(|derogacion| fecha < derogacion)
    }

    /// Verdadero si la revisión aún no ha sido derogada
    pub fn esta_abierta(&self) -> bool {
        self.fecha_derogacion.is_none()
    }

    /// Cierra la ventana en `fecha`. Rechaza fechas no posteriores a la entrada en vigor
    /// y las ventanas ya derogadas (la derogación es inmutable para la auditoría).
    pub fn derogar(&mut self, fecha: NaiveDate) -> Result<(), DomainError> {
        if let Some(previa) = self.fecha_derogacion {
            return Err(DomainError::ValidationError(format!(
                "La revisión ya fue derogada el {previa}"
            )));
        }
        validar_derogacion_posterior(self.fecha_vigor, fecha)?;
        self.fecha_derogacion = Some(fecha);
        Ok(())
    }
}

fn validar_derogacion_posterior(
    fecha_vigor: NaiveDate,
    fecha_derogacion: NaiveDate,
) -> Result<(), DomainError> {
    if fecha_derogacion <= fecha_vigor {
        return Err(DomainError::ValidationError(format!(
            "Paradoja temporal: la derogación ({fecha_derogacion}) debe ser posterior a la entrada en vigor ({fecha_vigor})"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fecha(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn test_vigente_en_respeta_limites_semiabiertos() {
        let ventana = VentanaVigencia::new(fecha(2024, 1, 15), Some(fecha(2025, 6, 1))).unwrap();
        assert!(!ventana.vigente_en(fecha(2024, 1, 14)));
        assert!(ventana.vigente_en(fecha(2024, 1, 15)));
        assert!(ventana.vigente_en(fecha(2025, 5, 31)));
        assert!(!ventana.vigente_en(fecha(2025, 6, 1)));
        assert!(!ventana.esta_abierta());
    }

    #[test]
    fn test_derogar_cierra_la_ventana_y_rechaza_paradojas() {
        let mut ventana = VentanaVigencia::new(fecha(2025, 1, 1), None).unwrap();
        assert!(ventana.esta_abierta());
        assert!(ventana.vigente_en(fecha(2030, 1, 1)));

        assert!(ventana.derogar(fecha(2024, 12, 31)).is_err());
        assert!(ventana.derogar(fecha(2025, 1, 1)).is_err());
        ventana.derogar(fecha(2025, 3, 1)).unwrap();
        assert!(!ventana.vigente_en(fecha(2025, 3, 1)));

        assert!(VentanaVigencia::new(fecha(2025, 1, 1), Some(fecha(2024, 1, 1))).is_err());
    }
}
