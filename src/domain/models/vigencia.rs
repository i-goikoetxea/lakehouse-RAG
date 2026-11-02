//! Ventanas de vigencia documental para time travel y auditoría retroactiva (problema 3)

use chrono::{Datelike, NaiveDate};

use crate::domain::errors::DomainError;

/// Primer año admisible como fecha de vigor; `1900-01-01` y anteriores son valores centinela
const ANIO_MINIMO: i32 = 1900;
/// Primer año rechazado por imposible (`9999-12-31` y similares)
const ANIO_LIMITE: i32 = 2100;
/// Formas de decir que no hay fecha, que no merecen aviso
const MARCAS_SIN_FECHA: [&str; 6] = ["", "null", "none", "n/a", "s/f", "no declarada"];
const MESES: [&str; 12] = [
    "enero",
    "febrero",
    "marzo",
    "abril",
    "mayo",
    "junio",
    "julio",
    "agosto",
    "septiembre",
    "octubre",
    "noviembre",
    "diciembre",
];

/// Lo que se puede afirmar de una revisión en una fecha concreta
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EstadoVigenciaEnFecha {
    Vigente,
    NoVigente,
    /// El documento no declara la fecha o el fin de su vigencia: no se puede afirmar nada
    NoDeclarada,
}

/// Intervalo semiabierto `[fecha_vigor, fecha_derogacion)` en el que una revisión está vigente.
/// Una ventana sin `fecha_derogacion` sigue abierta (revisión vigente hoy). `fecha_vigor` es
/// `None` si el documento no la declara.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VentanaVigencia {
    fecha_vigor: Option<NaiveDate>,
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
            fecha_vigor: Some(fecha_vigor),
            fecha_derogacion,
        })
    }

    /// Ventana abierta de un documento con fecha declarada
    pub fn desde_fecha_declarada(fecha_vigor: NaiveDate) -> Self {
        Self {
            fecha_vigor: Some(fecha_vigor),
            fecha_derogacion: None,
        }
    }

    /// Ventana de un documento que no declara su fecha de entrada en vigor
    pub fn sin_fecha_declarada(fecha_derogacion: Option<NaiveDate>) -> Self {
        Self {
            fecha_vigor: None,
            fecha_derogacion,
        }
    }

    pub fn fecha_vigor(&self) -> Option<NaiveDate> {
        self.fecha_vigor
    }

    pub fn fecha_derogacion(&self) -> Option<NaiveDate> {
        self.fecha_derogacion
    }

    /// `fecha_vigor <= fecha AND (fecha_derogacion IS NULL OR fecha < fecha_derogacion)`.
    /// Sin fecha declarada nunca se afirma que esté vigente.
    pub fn vigente_en(&self, fecha: NaiveDate) -> bool {
        self.fecha_vigor.is_some_and(|vigor| vigor <= fecha)
            && self
                .fecha_derogacion
                .is_none_or(|derogacion| fecha < derogacion)
    }

    /// Clasifica la revisión en `fecha`; `vigente_hoy` es la columna `vigente` de 01
    pub fn estado_en(&self, fecha: NaiveDate, vigente_hoy: bool) -> EstadoVigenciaEnFecha {
        let derogada_antes = self.fecha_derogacion.is_some_and(|d| d <= fecha);
        let aun_no_vigente = self.fecha_vigor.is_some_and(|vigor| fecha < vigor);
        if derogada_antes || aun_no_vigente {
            EstadoVigenciaEnFecha::NoVigente
        } else if self.fecha_vigor.is_none() || (self.fecha_derogacion.is_none() && !vigente_hoy) {
            EstadoVigenciaEnFecha::NoDeclarada
        } else {
            EstadoVigenciaEnFecha::Vigente
        }
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
        if let Some(vigor) = self.fecha_vigor {
            validar_derogacion_posterior(vigor, fecha)?;
        }
        self.fecha_derogacion = Some(fecha);
        Ok(())
    }
}

/// Fecha de vigor aceptada tras la validación y motivo del rechazo, si lo hubo
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FechaVigorValidada {
    pub fecha: Option<NaiveDate>,
    pub aviso: Option<String>,
}

/// Valida la fecha que devuelve el extractor. Nunca la inventa ni la corrige: un valor ausente,
/// no ISO, centinela, imposible o igual a `hoy` sin aparecer en el documento queda en `None`.
pub fn validar_fecha_vigor(
    valor: Option<&str>,
    hoy: NaiveDate,
    texto_documento: &str,
) -> FechaVigorValidada {
    let valor = valor.map_or("", str::trim);
    if MARCAS_SIN_FECHA.contains(&valor.to_lowercase().as_str()) {
        return FechaVigorValidada {
            fecha: None,
            aviso: None,
        };
    }
    let rechazo = |motivo: &str| FechaVigorValidada {
        fecha: None,
        aviso: Some(format!(
            "Fecha de vigor «{valor}» descartada ({motivo}): queda como no declarada."
        )),
    };
    let Some(fecha) = fecha_iso(valor) else {
        return rechazo("no es una fecha ISO válida");
    };
    if !(ANIO_MINIMO..ANIO_LIMITE).contains(&fecha.year()) || es_centinela(fecha) {
        return rechazo("valor centinela o fuera de rango");
    }
    if fecha == hoy && !aparece_en_texto(fecha, texto_documento) {
        return rechazo("coincide con el día de la ingesta y no figura en el documento");
    }
    FechaVigorValidada {
        fecha: Some(fecha),
        aviso: None,
    }
}

/// `AAAA-MM-DD`, con hora opcional (`T...`). Cualquier otro formato se rechaza: `03-04-2026` es
/// ambiguo y adivinarlo sería inventar la fecha.
fn fecha_iso(valor: &str) -> Option<NaiveDate> {
    let (dia, hora) = match valor.split_once('T') {
        Some((dia, hora)) => (dia, Some(hora)),
        None => (valor, None),
    };
    let forma_valida = dia.len() == 10 && dia.as_bytes()[4] == b'-' && dia.as_bytes()[7] == b'-';
    let hora_valida = hora.is_none_or(|h| h.starts_with(|c: char| c.is_ascii_digit()));
    (forma_valida && hora_valida)
        .then(|| NaiveDate::parse_from_str(dia, "%Y-%m-%d").ok())
        .flatten()
}

fn es_centinela(fecha: NaiveDate) -> bool {
    [(1900, 1, 1), (1970, 1, 1)]
        .iter()
        .any(|(a, m, d)| (fecha.year(), fecha.month(), fecha.day()) == (*a, *m, *d))
}

fn aparece_en_texto(fecha: NaiveDate, texto: &str) -> bool {
    let texto = texto.to_lowercase();
    let (a, m, d) = (fecha.year(), fecha.month(), fecha.day());
    let mes = MESES[fecha.month0() as usize];
    [
        format!("{a}-{m:02}-{d:02}"),
        format!("{d:02}/{m:02}/{a}"),
        format!("{d}/{m}/{a}"),
        format!("{d:02}-{m:02}-{a}"),
        format!("{d:02}.{m:02}.{a}"),
        format!("{d} de {mes} de {a}"),
    ]
    .iter()
    .any(|forma| texto.contains(forma))
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

    #[test]
    fn test_sin_fecha_declarada_nunca_se_afirma_vigente() {
        let abierta = VentanaVigencia::sin_fecha_declarada(None);
        assert_eq!(abierta.fecha_vigor(), None);
        assert!(!abierta.vigente_en(fecha(2026, 1, 1)));
        let mut derogable = abierta;
        derogable.derogar(fecha(2020, 1, 1)).unwrap();
        assert_eq!(derogable.fecha_derogacion(), Some(fecha(2020, 1, 1)));
    }

    #[test]
    fn test_estado_en_fecha_cubre_los_casos_con_y_sin_fecha() {
        use EstadoVigenciaEnFecha::*;
        let d = fecha(2025, 6, 1);
        let antes = fecha(2020, 1, 1);
        let despues = fecha(2030, 1, 1);
        // Sin NULL: ventana exacta
        let completa = VentanaVigencia::new(antes, Some(despues)).unwrap();
        assert_eq!(completa.estado_en(d, false), Vigente);
        assert_eq!(completa.estado_en(fecha(2019, 1, 1), false), NoVigente);
        assert_eq!(completa.estado_en(despues, false), NoVigente);
        assert_eq!(
            VentanaVigencia::new(antes, None)
                .unwrap()
                .estado_en(d, true),
            Vigente
        );
        // fecha → NULL: retirada sin saber cuándo
        let fin_desconocido = VentanaVigencia::new(antes, None).unwrap();
        assert_eq!(fin_desconocido.estado_en(d, false), NoDeclarada);
        assert_eq!(
            fin_desconocido.estado_en(fecha(2019, 1, 1), false),
            NoVigente
        );
        // NULL → fecha: inicio desconocido, fin conocido
        let inicio_desconocido = VentanaVigencia::sin_fecha_declarada(Some(despues));
        assert_eq!(inicio_desconocido.estado_en(d, false), NoDeclarada);
        assert_eq!(inicio_desconocido.estado_en(despues, false), NoVigente);
        // NULL → NULL y NULL vigente hoy
        let sin_nada = VentanaVigencia::sin_fecha_declarada(None);
        assert_eq!(sin_nada.estado_en(d, false), NoDeclarada);
        assert_eq!(sin_nada.estado_en(d, true), NoDeclarada);
    }

    fn validar(valor: Option<&str>, texto: &str) -> FechaVigorValidada {
        validar_fecha_vigor(valor, fecha(2026, 10, 31), texto)
    }

    #[test]
    fn test_fecha_valida_se_conserva_y_admite_hora() {
        for valor in ["2026-09-15", " 2026-09-15 ", "2026-09-15T00:00:00Z"] {
            let validada = validar(Some(valor), "");
            assert_eq!(validada.fecha, Some(fecha(2026, 9, 15)), "{valor}");
            assert_eq!(validada.aviso, None, "{valor}");
        }
    }

    #[test]
    fn test_fecha_ausente_es_nula_sin_aviso() {
        for valor in [None, Some(""), Some("   "), Some("null"), Some("N/A")] {
            let validada = validar(valor, "");
            assert_eq!(validada.fecha, None, "{valor:?}");
            assert_eq!(validada.aviso, None, "{valor:?}");
        }
    }

    #[test]
    fn test_fechas_centinela_imposibles_o_raras_quedan_nulas_con_aviso() {
        for valor in [
            "0001-01-01",
            "1899-12-30",
            "1900-01-01",
            "1970-01-01",
            "9999-12-31",
            "2100-01-01",
            "2026-02-30",
            "2026-13-01",
            "15/03/2026",
            "03-04-2026",
            "20260915",
            "marzo de 2026",
            "2026-09-15 y siguientes",
            "-2026-09-15",
        ] {
            let validada = validar(Some(valor), "");
            assert_eq!(validada.fecha, None, "{valor}");
            assert!(validada.aviso.is_some(), "{valor}");
        }
    }

    #[test]
    fn test_fecha_de_hoy_solo_vale_si_aparece_en_el_documento() {
        assert_eq!(validar(Some("2026-10-31"), "Sin fecha alguna").fecha, None);
        assert!(
            validar(Some("2026-10-31"), "Sin fecha alguna")
                .aviso
                .is_some()
        );
        for texto in [
            "Vigor: 2026-10-31.",
            "Vigor: 31/10/2026",
            "Vigor: 31-10-2026",
            "Vigor: 31.10.2026",
            "Vigor: 31 de octubre de 2026",
            "VIGOR: 31 DE OCTUBRE DE 2026",
        ] {
            let validada = validar(Some("2026-10-31"), texto);
            assert_eq!(validada.fecha, Some(fecha(2026, 10, 31)), "{texto}");
        }
    }
}
