//! Matriz operacional: una fila por hito de proceso (Tabla 03, problema 1)

use serde::{Deserialize, Serialize};

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;
use crate::domain::services::normalizacion::{codigo_canonico, normalizar_token};

/// Fase operativa canónica (mayúsculas ASCII sin tilde)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FaseOperativa {
    Recepcion,
    Descarga,
    Custodia,
    Inspeccion,
    Ensayo,
    Liberacion,
    Ejecucion,
    Despacho,
    Mantenimiento,
    /// Fase no prevista, ya normalizada (`SOLDEO`, `ALMACENAMIENTO_TEMPORAL`)
    Otra(String),
}

impl FaseOperativa {
    /// Vocabulario cerrado que acepta el JSON Schema de destilación (además de `Otra`)
    pub const VALORES_CANONICOS: [&'static str; 9] = [
        "RECEPCION",
        "DESCARGA",
        "CUSTODIA",
        "INSPECCION",
        "ENSAYO",
        "LIBERACION",
        "EJECUCION",
        "DESPACHO",
        "MANTENIMIENTO",
    ];
    /// Longitud máxima de una fase `Otra` normalizada
    pub const MAX_LONGITUD_OTRA: usize = 64;

    /// Palabras que preceden al nombre de la fase y se descartan (`Fase 2 — CUSTODIA`)
    const PREFIJOS: [&'static str; 4] = ["FASE", "ETAPA", "PASO", "SUBFASE"];

    /// Interpreta la fase tolerando tildes, calificadores entre paréntesis y prefijos.
    /// Decide por la primera palabra significativa; lo no previsto se conserva como `Otra`.
    pub fn parse(valor: impl AsRef<str>) -> Result<Self, DomainError> {
        let token = normalizar_token(&quitar_parentesis(valor.as_ref()));
        if token.is_empty() {
            return Err(DomainError::ValidationError(
                "La fase operativa no puede estar vacía".to_string(),
            ));
        }
        let palabras = Self::quitar_prefijos(token.split('_').collect());
        if let Some(canonica) = Self::desde_palabra(palabras[0]) {
            return Ok(canonica);
        }
        let es_solo_numero = palabras
            .iter()
            .all(|p| p.chars().all(|c| c.is_ascii_digit()));
        let nombre = if es_solo_numero {
            token.clone()
        } else {
            palabras.join("_")
        };
        if nombre.len() > Self::MAX_LONGITUD_OTRA {
            return Err(DomainError::ValidationError(format!(
                "La fase operativa excede {} caracteres",
                Self::MAX_LONGITUD_OTRA
            )));
        }
        Ok(Self::Otra(nombre))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Recepcion => "RECEPCION",
            Self::Descarga => "DESCARGA",
            Self::Custodia => "CUSTODIA",
            Self::Inspeccion => "INSPECCION",
            Self::Ensayo => "ENSAYO",
            Self::Liberacion => "LIBERACION",
            Self::Ejecucion => "EJECUCION",
            Self::Despacho => "DESPACHO",
            Self::Mantenimiento => "MANTENIMIENTO",
            Self::Otra(nombre) => nombre.as_str(),
        }
    }

    /// Verdadero si la fase pertenece al vocabulario cerrado
    pub fn es_canonica(&self) -> bool {
        !matches!(self, Self::Otra(_))
    }

    /// Descarta prefijos y numeraciones iniciales mientras quede más de una palabra
    fn quitar_prefijos(mut palabras: Vec<&str>) -> Vec<&str> {
        while palabras.len() > 1 {
            let inicial = palabras[0];
            let es_prefijo =
                Self::PREFIJOS.contains(&inicial) || inicial.chars().all(|c| c.is_ascii_digit());
            if !es_prefijo {
                break;
            }
            palabras.remove(0);
        }
        palabras
    }

    fn desde_palabra(palabra: &str) -> Option<Self> {
        match palabra {
            "RECEPCION" | "RECEPCIONES" => Some(Self::Recepcion),
            "DESCARGA" | "DESCARGAS" => Some(Self::Descarga),
            "CUSTODIA" => Some(Self::Custodia),
            "INSPECCION" | "INSPECCIONES" => Some(Self::Inspeccion),
            "ENSAYO" | "ENSAYOS" | "PRUEBA" | "PRUEBAS" => Some(Self::Ensayo),
            "LIBERACION" => Some(Self::Liberacion),
            "EJECUCION" => Some(Self::Ejecucion),
            "DESPACHO" | "EXPEDICION" => Some(Self::Despacho),
            "MANTENIMIENTO" => Some(Self::Mantenimiento),
            _ => None,
        }
    }
}

impl std::fmt::Display for FaseOperativa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Elimina el contenido entre paréntesis o corchetes (calificadores de la fase)
fn quitar_parentesis(texto: &str) -> String {
    let mut profundidad = 0usize;
    texto
        .chars()
        .filter(|caracter| match caracter {
            '(' | '[' => {
                profundidad += 1;
                false
            }
            ')' | ']' => {
                profundidad = profundidad.saturating_sub(1);
                false
            }
            _ => profundidad == 0,
        })
        .collect()
}

/// Defecto de extracción que obliga a revisar el hito (`calidad_extraccion = REVISAR`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlertaExtraccion {
    /// Un campo de rol contiene varios roles o mezcla ejecutor y responsable
    RolesFusionados,
    EjecutorVacio,
    ResponsableVacio,
    SinRegistro,
    /// El código de registro no está en forma canónica (`FOR-ALM-02`)
    RegistroNoCanonico,
}

impl AlertaExtraccion {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RolesFusionados => "ROLES_FUSIONADOS",
            Self::EjecutorVacio => "EJECUTOR_VACIO",
            Self::ResponsableVacio => "RESPONSABLE_VACIO",
            Self::SinRegistro => "SIN_REGISTRO",
            Self::RegistroNoCanonico => "REGISTRO_NO_CANONICO",
        }
    }
}

/// Veredicto de calidad de la extracción de un hito
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CalidadExtraccion {
    Ok,
    Revisar,
}

impl CalidadExtraccion {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Revisar => "REVISAR",
        }
    }
}

/// Datos en bruto de un hito tal como llegan del extractor, antes de validarlos
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BorradorHito {
    pub hito_id: String,
    pub doc_id: DocId,
    pub orden_secuencia: u32,
    pub fase: FaseOperativa,
    pub objeto_alcance: String,
    pub rol_ejecutor: String,
    pub rol_responsable: String,
    pub registro_codigo: Option<String>,
    pub registro_nombre: Option<String>,
    pub criterio_aceptacion: String,
    pub condicion_aplicacion: Option<String>,
}

/// Hito operativo validado: una fila de la matriz con roles y registro en columnas separadas
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HitoOperativo {
    pub hito_id: String,
    pub doc_id: DocId,
    pub orden_secuencia: u32,
    pub fase: FaseOperativa,
    pub objeto_alcance: String,
    pub rol_ejecutor: String,
    pub rol_responsable: String,
    pub registro_codigo: Option<String>,
    pub registro_nombre: Option<String>,
    pub criterio_aceptacion: String,
    pub condicion_aplicacion: Option<String>,
}

/// Separadores (incluidos sus homoglifos Unicode) que delatan varios roles en un campo
const SEPARADORES_DE_ROLES: [char; 16] = [
    ';', '/', '|', ',', '&', '+', '\n', '\u{037E}', '\u{2044}', '\u{2215}', '\u{FF1B}', '\u{FF0F}',
    '\u{FF5C}', '\u{FF0C}', '\u{FF06}', '\u{FF0B}',
];
/// Palabras que indican que el campo de responsable describe también la ejecución
/// Longitud máxima de cada pieza de una sigla compuesta (`QA/QC`)
const MAX_LONGITUD_PIEZA_SIGLA: usize = 4;
const MARCAS_DE_EJECUCION: [&str; 4] = ["EJECUTOR", "EJECUTA", "EJECUTANTE", "REALIZA"];
/// Conjunciones que, seguidas de un cargo, unen dos roles distintos
const CONJUNCIONES: [&str; 5] = ["Y", "E", "O", "U", "MAS"];
const ARTICULOS: [&str; 5] = ["EL", "LA", "LOS", "LAS", "DEL"];
/// Sustantivos que encabezan un cargo
const CARGOS: [&str; 20] = [
    "JEFE",
    "JEFA",
    "RESPONSABLE",
    "ENCARGADO",
    "ENCARGADA",
    "DIRECTOR",
    "DIRECTORA",
    "SUPERVISOR",
    "SUPERVISORA",
    "COORDINADOR",
    "COORDINADORA",
    "INSPECTOR",
    "INSPECTORA",
    "OPERARIO",
    "OPERARIA",
    "ALMACENERO",
    "ALMACENERA",
    "SOLDADOR",
    "GERENTE",
    "TECNICO",
];

impl HitoOperativo {
    pub const MAX_LONGITUD_CAMPO: usize = 16_384;
    pub const MAX_LONGITUD_ID: usize = 512;

    /// Valida el borrador: recorta campos, convierte opcionales vacíos en `None`, exige
    /// `hito_id` y `objeto_alcance` y rechaza longitudes abusivas o bytes nulos.
    pub fn new(borrador: BorradorHito) -> Result<Self, DomainError> {
        let hito_id = limpiar_campo("hito_id", &borrador.hito_id, Self::MAX_LONGITUD_ID)?;
        exigir_no_vacio("hito_id", &hito_id)?;
        let objeto_alcance = limpiar_campo(
            "objeto_alcance",
            &borrador.objeto_alcance,
            Self::MAX_LONGITUD_CAMPO,
        )?;
        exigir_no_vacio("objeto_alcance", &objeto_alcance)?;

        Ok(Self {
            hito_id,
            doc_id: borrador.doc_id,
            orden_secuencia: borrador.orden_secuencia,
            fase: borrador.fase,
            objeto_alcance,
            rol_ejecutor: limpiar_campo(
                "rol_ejecutor",
                &borrador.rol_ejecutor,
                Self::MAX_LONGITUD_CAMPO,
            )?,
            rol_responsable: limpiar_campo(
                "rol_responsable",
                &borrador.rol_responsable,
                Self::MAX_LONGITUD_CAMPO,
            )?,
            registro_codigo: limpiar_opcional("registro_codigo", borrador.registro_codigo)?,
            registro_nombre: limpiar_opcional("registro_nombre", borrador.registro_nombre)?,
            criterio_aceptacion: limpiar_campo(
                "criterio_aceptacion",
                &borrador.criterio_aceptacion,
                Self::MAX_LONGITUD_CAMPO,
            )?,
            condicion_aplicacion: limpiar_opcional(
                "condicion_aplicacion",
                borrador.condicion_aplicacion,
            )?,
        })
    }

    /// Defectos de extracción detectados en la fila (vacío = fila limpia)
    pub fn alertas_calidad(&self) -> Vec<AlertaExtraccion> {
        let comprobaciones = [
            (
                AlertaExtraccion::RolesFusionados,
                self.tiene_roles_fusionados(),
            ),
            (
                AlertaExtraccion::EjecutorVacio,
                self.rol_ejecutor.is_empty(),
            ),
            (
                AlertaExtraccion::ResponsableVacio,
                self.rol_responsable.is_empty(),
            ),
            (
                AlertaExtraccion::SinRegistro,
                self.registro_codigo.is_none(),
            ),
            (
                AlertaExtraccion::RegistroNoCanonico,
                self.registro_no_canonico(),
            ),
        ];
        comprobaciones
            .into_iter()
            .filter_map(|(alerta, detectada)| detectada.then_some(alerta))
            .collect()
    }

    /// `REVISAR` si la fila tiene alguna alerta; `OK` en caso contrario
    pub fn calidad_extraccion(&self) -> CalidadExtraccion {
        if self.alertas_calidad().is_empty() {
            CalidadExtraccion::Ok
        } else {
            CalidadExtraccion::Revisar
        }
    }

    /// Texto del `vector_hito` (especificación §4): fase, objeto, responsable, registro y
    /// criterio de esta fila, más la condición de aplicación heredada si existe. El ejecutor
    /// no se vectoriza para no contaminar la búsqueda por responsable.
    pub fn texto_vectorizable(&self) -> String {
        let registro = [
            self.registro_codigo.as_deref(),
            self.registro_nombre.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        let partes = [
            ("Fase", self.fase.as_str()),
            ("Objeto", self.objeto_alcance.as_str()),
            ("Responsable", self.rol_responsable.as_str()),
            ("Registro", registro.as_str()),
            ("Criterio", self.criterio_aceptacion.as_str()),
            (
                "Condición",
                self.condicion_aplicacion.as_deref().unwrap_or_default(),
            ),
        ];
        partes
            .into_iter()
            .filter(|(_, valor)| !valor.is_empty())
            .map(|(etiqueta, valor)| format!("{etiqueta}: {valor}"))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn tiene_roles_fusionados(&self) -> bool {
        rol_parece_multiple(&self.rol_ejecutor)
            || rol_parece_multiple(&self.rol_responsable)
            || responsable_describe_ejecucion(&self.rol_responsable)
            || responsable_incluye_ejecutor(&self.rol_responsable, &self.rol_ejecutor)
    }

    fn registro_no_canonico(&self) -> bool {
        self.registro_codigo.as_deref().is_some_and(|codigo| {
            codigo != codigo_canonico(codigo) || !codigo.chars().any(|c| c.is_ascii_digit())
        })
    }
}

fn tokens_de(rol: &str) -> Vec<String> {
    normalizar_token(rol)
        .split('_')
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Separadores de lista o una conjunción seguida de otro cargo (`Jefe … y Responsable …`)
/// Sigla compuesta (`QA/QC`, `I+D`): piezas cortas en mayúsculas unidas por un separador.
/// No delata dos roles distintos, así que su separador no cuenta.
fn es_sigla_compuesta(palabra: &str) -> bool {
    let piezas: Vec<&str> = palabra
        .split(|c| SEPARADORES_DE_ROLES.contains(&c))
        .collect();
    piezas.len() > 1
        && piezas.iter().all(|pieza| {
            !pieza.is_empty()
                && pieza.chars().count() <= MAX_LONGITUD_PIEZA_SIGLA
                && pieza
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
}

fn rol_parece_multiple(rol: &str) -> bool {
    let tiene_separador = rol
        .split_whitespace()
        .filter(|palabra| !es_sigla_compuesta(palabra))
        .any(|palabra| palabra.chars().any(|c| SEPARADORES_DE_ROLES.contains(&c)));
    if tiene_separador {
        return true;
    }
    let tokens = tokens_de(rol);
    tokens.iter().enumerate().any(|(posicion, token)| {
        CONJUNCIONES.contains(&token.as_str())
            && tokens[posicion + 1..]
                .iter()
                .find(|siguiente| !ARTICULOS.contains(&siguiente.as_str()))
                .is_some_and(|siguiente| CARGOS.contains(&siguiente.as_str()))
    })
}

fn responsable_describe_ejecucion(responsable: &str) -> bool {
    tokens_de(responsable)
        .iter()
        .any(|token| MARCAS_DE_EJECUCION.contains(&token.as_str()))
}

/// El responsable repite al ejecutor (iguales o el ejecutor contenido como palabras completas)
fn responsable_incluye_ejecutor(responsable: &str, ejecutor: &str) -> bool {
    let tokens_ejecutor = tokens_de(ejecutor);
    if tokens_ejecutor.is_empty() {
        return false;
    }
    tokens_de(responsable)
        .windows(tokens_ejecutor.len())
        .any(|ventana| ventana == tokens_ejecutor.as_slice())
}

fn limpiar_campo(nombre: &str, valor: &str, max_longitud: usize) -> Result<String, DomainError> {
    let recortado = valor.trim();
    if recortado.len() > max_longitud {
        return Err(DomainError::ValidationError(format!(
            "El campo '{nombre}' excede la longitud máxima permitida de {max_longitud} bytes"
        )));
    }
    if recortado.contains('\0') {
        return Err(DomainError::ValidationError(format!(
            "El campo '{nombre}' contiene bytes nulos"
        )));
    }
    Ok(recortado.to_string())
}

fn limpiar_opcional(nombre: &str, valor: Option<String>) -> Result<Option<String>, DomainError> {
    let limpio = match valor {
        Some(texto) => limpiar_campo(nombre, &texto, HitoOperativo::MAX_LONGITUD_CAMPO)?,
        None => return Ok(None),
    };
    Ok((!limpio.is_empty()).then_some(limpio))
}

fn exigir_no_vacio(nombre: &str, valor: &str) -> Result<(), DomainError> {
    if valor.is_empty() {
        return Err(DomainError::ValidationError(format!(
            "El campo '{nombre}' del hito no puede estar vacío"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> DocId {
        DocId::new("PO-ALM-03").unwrap()
    }

    fn borrador_custodia() -> BorradorHito {
        BorradorHito {
            hito_id: "PO-ALM-03@v3-HIT-0002".to_string(),
            doc_id: doc(),
            orden_secuencia: 2,
            fase: FaseOperativa::Custodia,
            objeto_alcance: "Material acopiado en almacén".to_string(),
            rol_ejecutor: "Almacenero".to_string(),
            rol_responsable: "Jefe de Producción".to_string(),
            registro_codigo: Some("FOR-ALM-02".to_string()),
            registro_nombre: Some("Parte de custodia".to_string()),
            criterio_aceptacion: "Sin daños ni corrosión".to_string(),
            condicion_aplicacion: None,
        }
    }

    #[test]
    fn test_fase_operativa_parse_canonico_tolerante() {
        let casos = [
            ("RECEPCIÓN", FaseOperativa::Recepcion),
            ("recepcion", FaseOperativa::Recepcion),
            ("Descarga", FaseOperativa::Descarga),
            ("custodia", FaseOperativa::Custodia),
            ("Inspección de recepción", FaseOperativa::Inspeccion),
            ("ENSAYOS", FaseOperativa::Ensayo),
            ("Liberación", FaseOperativa::Liberacion),
            ("EJECUCIÓN", FaseOperativa::Ejecucion),
            ("despacho", FaseOperativa::Despacho),
            ("Mantenimiento", FaseOperativa::Mantenimiento),
            ("Fase 2 — CUSTODIA", FaseOperativa::Custodia),
            ("FASE_3_INSPECCION", FaseOperativa::Inspeccion),
        ];
        for (entrada, esperado) in casos {
            assert_eq!(
                FaseOperativa::parse(entrada).unwrap(),
                esperado,
                "{entrada}"
            );
        }
        assert_eq!(FaseOperativa::Recepcion.as_str(), "RECEPCION");
    }

    #[test]
    fn test_fase_operativa_otra_se_normaliza_sin_calificadores() {
        assert_eq!(
            FaseOperativa::parse("SOLDEO (REQUISITOS POSTERIORES)").unwrap(),
            FaseOperativa::Otra("SOLDEO".to_string())
        );
        let otra = FaseOperativa::parse("almacenamiento temporal").unwrap();
        assert_eq!(otra.as_str(), "ALMACENAMIENTO_TEMPORAL");
        for valor in FaseOperativa::VALORES_CANONICOS {
            assert_eq!(FaseOperativa::parse(valor).unwrap().as_str(), valor);
        }
    }

    #[test]
    fn test_fase_operativa_vacia_falla() {
        for vacia in ["   ", "¿¿??", "()"] {
            let err = FaseOperativa::parse(vacia).unwrap_err();
            assert!(matches!(err, DomainError::ValidationError(_)), "{vacia}");
        }
    }

    #[test]
    fn test_hito_operativo_valido_sin_alertas() {
        let hito = HitoOperativo::new(borrador_custodia()).expect("Hito válido");
        assert_eq!(hito.fase, FaseOperativa::Custodia);
        assert_eq!(hito.rol_responsable, "Jefe de Producción");
        assert!(hito.alertas_calidad().is_empty());
        assert_eq!(hito.calidad_extraccion(), CalidadExtraccion::Ok);
    }

    #[test]
    fn test_hito_operativo_recorta_y_vacia_opcionales() {
        let mut borrador = borrador_custodia();
        borrador.objeto_alcance = "  Material  ".to_string();
        borrador.registro_nombre = Some("   ".to_string());
        borrador.condicion_aplicacion = Some(" T ambiente < 0 °C ".to_string());
        let hito = HitoOperativo::new(borrador).unwrap();
        assert_eq!(hito.objeto_alcance, "Material");
        assert_eq!(hito.registro_nombre, None);
        assert_eq!(
            hito.condicion_aplicacion.as_deref(),
            Some("T ambiente < 0 °C")
        );
    }

    #[test]
    fn test_hito_operativo_rechaza_vacios_y_excesos() {
        let mut sin_id = borrador_custodia();
        sin_id.hito_id = "  ".to_string();
        assert!(HitoOperativo::new(sin_id).is_err());

        let mut sin_objeto = borrador_custodia();
        sin_objeto.objeto_alcance = String::new();
        assert!(HitoOperativo::new(sin_objeto).is_err());

        let mut criterio_enorme = borrador_custodia();
        criterio_enorme.criterio_aceptacion = "X".repeat(HitoOperativo::MAX_LONGITUD_CAMPO + 1);
        assert!(HitoOperativo::new(criterio_enorme).is_err());
    }

    #[test]
    fn test_hito_detecta_roles_fusionados() {
        for responsable in [
            "Jefe de Producción; Almacenero",
            "Almacenero (ejecutor) / Jefe de Producción",
            "Almacenero",
            "Jefe de Producción y Responsable de Calidad",
        ] {
            let mut borrador = borrador_custodia();
            borrador.rol_responsable = responsable.to_string();
            let hito = HitoOperativo::new(borrador).unwrap();
            assert!(
                hito.alertas_calidad()
                    .contains(&AlertaExtraccion::RolesFusionados),
                "{responsable}"
            );
            assert_eq!(hito.calidad_extraccion(), CalidadExtraccion::Revisar);
        }
    }

    #[test]
    fn test_siglas_con_barra_no_son_roles_fusionados() {
        for (responsable, fusionado) in [
            ("Inspector QA/QC", false),
            ("Responsable I+D/QA", false),
            ("Inspector QA/QC / Jefe de Obra", true),
            ("Almacenero/Jefe de Producción", true),
            ("QA/Jefe", true),
        ] {
            let mut borrador = borrador_custodia();
            borrador.rol_responsable = responsable.to_string();
            let hito = HitoOperativo::new(borrador).unwrap();
            assert_eq!(
                hito.alertas_calidad()
                    .contains(&AlertaExtraccion::RolesFusionados),
                fusionado,
                "{responsable}"
            );
        }
    }

    #[test]
    fn test_hito_detecta_responsable_vacio_y_registro() {
        let mut borrador = borrador_custodia();
        borrador.rol_responsable = String::new();
        borrador.registro_codigo = None;
        let alertas = HitoOperativo::new(borrador).unwrap().alertas_calidad();
        assert!(alertas.contains(&AlertaExtraccion::ResponsableVacio));
        assert!(alertas.contains(&AlertaExtraccion::SinRegistro));

        let mut no_canonico = borrador_custodia();
        no_canonico.registro_codigo = Some("for alm 02".to_string());
        let alertas = HitoOperativo::new(no_canonico).unwrap().alertas_calidad();
        assert_eq!(alertas, vec![AlertaExtraccion::RegistroNoCanonico]);
        assert_eq!(
            AlertaExtraccion::RegistroNoCanonico.as_str(),
            "REGISTRO_NO_CANONICO"
        );
    }

    #[test]
    fn test_hito_texto_vectorizable_solo_con_su_fila() {
        let texto = HitoOperativo::new(borrador_custodia())
            .unwrap()
            .texto_vectorizable();
        for fragmento in [
            "CUSTODIA",
            "Material acopiado en almacén",
            "Jefe de Producción",
            "FOR-ALM-02",
            "Sin daños ni corrosión",
        ] {
            assert!(texto.contains(fragmento), "Falta '{fragmento}' en: {texto}");
        }
        assert!(!texto.contains("Almacenero"), "El ejecutor no se vectoriza");
    }
}
