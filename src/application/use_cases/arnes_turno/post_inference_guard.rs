//! Valida la respuesta final antes de emitirla. Es la garantía real del arnés: `--json-schema`
//! solo dirige la llamada, no la asegura.

use std::collections::HashSet;

use chrono::NaiveDate;
use serde_json::Value;

use super::cifras::{extraer_cifras, lecturas_de_fuente};
use super::intent_detector::IntencionTurno;
use super::negaciones::{LoConsultado, negacion_sin_busqueda};
use super::texto_normalizado::plegar_texto;
use super::verificador_paginas::{textos_de_metadatos, textos_marcables};
use crate::application::dtos::tools::{LakehouseToolName, ToolCall, ToolResult};
use crate::domain::services::marcas_pagina::quitar_marcas;

const LOCUCIONES_DE_RECHAZO: &[&str] = &["no consta", "no declarado", "no declarada"];
/// Lista cerrada (SSD §3.3): una abstención redactada de otro modo no se detecta.
const LOCUCIONES_DE_ABSTENCION: &[&str] = &[
    "no consta",
    "no constan",
    "no declarado",
    "no declarada",
    "no declarados",
    "no declaradas",
    "no figura",
    "no figuran",
    "no aparece",
    "no aparecen",
    "no se encuentra",
    "no se encuentran",
    "no he encontrado",
    "no se ha encontrado",
    "no se han encontrado",
    "no hay dato",
    "no hay datos",
    "no hay informacion",
    "no existe",
    "no existen",
    "no dispongo",
    "no se dispone",
    "no esta disponible",
    "no estan disponibles",
    "no disponible",
    "no se especifica",
    "no especifica",
    "no se indica",
    "no indica",
    "no se recoge",
    "no recoge",
    "sin coincidencia",
    "sin datos",
];
const PETICIONES_DE_REFERENCIA: &[&str] = &[
    "valor de referencia",
    "valores de referencia",
    "cifra de referencia",
    "cifras de referencia",
    "como referencia",
    "a modo de referencia",
    "orientativo",
    "orientativa",
    "orientativos",
    "orientativas",
    "aproximado",
    "aproximada",
    "aproximados",
    "aproximadas",
    "valor tipico",
    "valores tipicos",
    "valor habitual",
    "valores habituales",
];
/// Valor de `origen_responsable` que la regla 9 del prompt lee como «no consta».
const IDENTIFICADORES_DE_ABSTENCION: &[(&str, &str)] = &[("no_consta", "no consta")];
/// Tools cuyos campos `texto` llevan marcas de página (en el cruce, el contexto del apartado)
const HERRAMIENTAS_CON_MARCAS: &[LakehouseToolName] = &[
    LakehouseToolName::SearchNormativeContext,
    LakehouseToolName::StairGetSubtreeContext,
    LakehouseToolName::LookupDoubleEntryTable,
];
/// Tools que leen texto del documento: si el turno usó alguna, una negación no es «sin buscar»
const HERRAMIENTAS_DE_TEXTO: &[LakehouseToolName] = &[
    LakehouseToolName::SearchNormativeContext,
    LakehouseToolName::StairGetSubtreeContext,
];
const SENAL_SIN_RESPONSABLE: &str = "sin_responsable_declarado";
const SENAL_EJES_AMBIGUOS: &str = "ejes_ambiguos";
/// Palabras de enlace de un nombre de eje: que aparezcan en la respuesta no nombra el eje.
const PALABRAS_DE_ENLACE: &[&str] = &[
    "de", "del", "la", "el", "los", "las", "en", "y", "o", "a", "por", "para",
];
const PARES_DE_COMILLAS: &[(char, char)] = &[('«', '»'), ('“', '”'), ('"', '"')];
/// Una cita más corta (`«el jefe»`) coincide por azar: no respalda un rol.
const LONGITUD_MINIMA_CITA: usize = 12;
const MAXIMO_CITAS_COMPROBADAS: usize = 32;
const LONGITUD_MAXIMA_CIFRA_INFORMADA: usize = 64;

/// Tools y salidas del turno actual. El historial no entra aquí: es la pereza que se quiere evitar.
#[derive(Debug, Default, Clone)]
pub struct RegistroTurno {
    llamadas: Vec<ToolCall>,
    /// Instrucciones de la app al modelo: su vocabulario describe campos, no el mundo
    vocabulario_de_la_app: String,
    resultados: Vec<ToolResult>,
    fecha_de_hoy: Option<NaiveDate>,
}

impl RegistroTurno {
    /// Las palabras de las instrucciones cuentan como consultadas al juzgar una negación
    pub fn fijar_vocabulario_de_la_app(&mut self, instrucciones: &str) {
        self.vocabulario_de_la_app = instrucciones.to_string();
    }

    /// Lo que pidió el modelo en el turno (argumentos incluidos), haya ido bien o no
    pub fn registrar_llamada(&mut self, llamada: &ToolCall) {
        self.llamadas.push(llamada.clone());
    }

    /// Claude Code da la fecha de hoy al modelo: citarla entera no es inventar una cifra. Sus
    /// partes sueltas siguen sin anclar.
    pub fn fijar_fecha_de_hoy(&mut self, hoy: NaiveDate) {
        self.fecha_de_hoy = Some(hoy);
    }

    pub fn registrar(&mut self, resultado: ToolResult) {
        self.resultados.push(resultado);
    }

    /// Un error o un `sin_coincidencia` cuentan: se exige haberlo intentado, no haber acertado.
    fn intento_alguna(&self, herramientas: &[LakehouseToolName]) -> bool {
        self.resultados
            .iter()
            .any(|r| herramientas.iter().any(|h| h.as_str() == r.tool_name))
    }

    /// Un error puede repetir los argumentos que eligió el modelo: no es una fuente de datos.
    fn salidas_validas(&self) -> impl Iterator<Item = &ToolResult> {
        self.resultados.iter().filter(|r| !r.is_error)
    }

    fn matriz_sin_responsable(&self) -> bool {
        self.salidas_validas().any(es_matriz_sin_responsable)
    }

    /// Textos de las tools que marcan páginas (`⟦p. N⟧`), para comprobar las citas
    pub(super) fn textos_paginables(&self) -> Vec<&str> {
        self.salidas_validas()
            .filter(|r| {
                HERRAMIENTAS_CON_MARCAS
                    .iter()
                    .any(|h| h.as_str() == r.tool_name)
            })
            .flat_map(|r| textos_marcables(&r.output))
            .collect()
    }

    /// Argumentos de las llamadas y textos (y claves) de las salidas válidas del turno
    fn lo_consultado(&self) -> LoConsultado {
        let mut textos: Vec<String> = vec![self.vocabulario_de_la_app.clone()];
        for llamada in &self.llamadas {
            recorrer_textos(&llamada.arguments, |texto| textos.push(texto.to_string()));
        }
        for resultado in self.salidas_validas() {
            recorrer_textos(&resultado.output, |texto| textos.push(texto.to_string()));
        }
        let consulto = !self.llamadas.is_empty() || !self.resultados.is_empty();
        let es_de_texto = |nombre: &str| HERRAMIENTAS_DE_TEXTO.iter().any(|h| h.as_str() == nombre);
        let leyo_texto = self.llamadas.iter().any(|l| es_de_texto(&l.name))
            || self.resultados.iter().any(|r| es_de_texto(&r.tool_name));
        LoConsultado::nuevo(textos.iter().map(String::as_str), consulto, leyo_texto)
    }

    /// Títulos, breadcrumbs y demás cadenas de las salidas que no son texto del documento
    pub(super) fn metadatos(&self) -> Vec<&str> {
        self.salidas_validas()
            .flat_map(|r| textos_de_metadatos(&r.output))
            .collect()
    }
}

fn es_matriz_sin_responsable(resultado: &ToolResult) -> bool {
    resultado.tool_name == LakehouseToolName::QueryOperationalMatrix.as_str()
        && resultado.output.get(SENAL_SIN_RESPONSABLE) == Some(&Value::Bool(true))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolacionArnes {
    CapaDosNoIntentada,
    CifraSinAnclar { cifra: String },
    RolSinRespaldo,
    EjesAmbiguosSinAclarar,
    CifraEnAbstencion { cifra: String },
    NegacionSinBusqueda { aspecto: String },
}

impl ViolacionArnes {
    /// Texto para la telemetría y para el recordatorio del reintento
    pub fn motivo(&self) -> String {
        match self {
            Self::CapaDosNoIntentada => {
                "no se consultó la Capa 2 en este turno (lookup_double_entry_table, query_operational_matrix o execute_athena_sql, según la pregunta)".to_string()
            }
            Self::CifraSinAnclar { cifra } => format!(
                "la cifra «{cifra}» no aparece en ninguna salida de herramienta de este turno ni en la pregunta"
            ),
            Self::RolSinRespaldo => {
                "la matriz devolvió sin_responsable_declarado: di que no consta o cita literalmente un fragmento devuelto por una herramienta".to_string()
            }
            Self::EjesAmbiguosSinAclarar => {
                "lookup_double_entry_table devolvió ejes_ambiguos y la respuesta da un único valor sin aclarar los ejes: nombra qué valor corresponde a cada eje (nombre o unidad), muestra ambos valores o pregunta".to_string()
            }
            Self::NegacionSinBusqueda { aspecto } => format!(
                "la respuesta afirma que no hay «{aspecto}» (o que no se aplica) sin haberlo buscado en este turno: búscalo con search_normative_context o lee el apartado con stair_get_subtree_context y responde con lo que conste; si no aparece, di que no consta en lo consultado, sin afirmar que no existe"
            ),
            Self::CifraEnAbstencion { cifra } => format!(
                "la respuesta declara que algo no consta y añade «{cifra}», que no aparece en la pregunta ni en ninguna salida de herramienta de este turno: di que no consta, sin ofrecer valores de referencia"
            ),
        }
    }
}

pub struct PostInferenceGuard;

impl PostInferenceGuard {
    pub fn validar(
        respuesta: &str,
        pregunta: &str,
        intencion: &IntencionTurno,
        registro: &RegistroTurno,
    ) -> Result<(), ViolacionArnes> {
        if *intencion == IntencionTurno::Ninguna {
            return Ok(());
        }
        let herramientas = herramientas_de_capa_dos(intencion);
        if herramientas.is_some_and(|exigidas| !registro.intento_alguna(exigidas)) {
            return Err(ViolacionArnes::CapaDosNoIntentada);
        }
        comprobar_abstencion_sin_cifras_ajenas(respuesta, pregunta, registro)?;
        comprobar_negaciones_buscadas(respuesta, pregunta, registro)?;
        if herramientas.is_none() {
            return Ok(());
        }
        comprobar_respaldo_de_rol(respuesta, registro)?;
        comprobar_cifras_ancladas(respuesta, pregunta, registro)?;
        comprobar_ejes_aclarados(respuesta, registro)
    }
}

fn herramientas_de_capa_dos(intencion: &IntencionTurno) -> Option<&'static [LakehouseToolName]> {
    match intencion {
        IntencionTurno::Ninguna | IntencionTurno::DatoConcreto => None,
        IntencionTurno::TabularIncompleta | IntencionTurno::TabularCruzable { .. } => Some(&[
            LakehouseToolName::LookupDoubleEntryTable,
            LakehouseToolName::ExecuteAthenaSql,
        ]),
        IntencionTurno::Responsable => Some(&[
            LakehouseToolName::QueryOperationalMatrix,
            LakehouseToolName::ExecuteAthenaSql,
        ]),
    }
}

fn comprobar_cifras_ancladas(
    respuesta: &str,
    pregunta: &str,
    registro: &RegistroTurno,
) -> Result<(), ViolacionArnes> {
    match primera_cifra_sin_anclar(respuesta, pregunta, registro) {
        Some(cifra) => Err(ViolacionArnes::CifraSinAnclar { cifra }),
        None => Ok(()),
    }
}

/// Las cifras ancladas pasan aunque sean de otra entidad: es lo que permite la respuesta parcial,
/// y distinguir de quién es cada cifra no se puede hacer con reglas (SSD §3.3).
fn comprobar_abstencion_sin_cifras_ajenas(
    respuesta: &str,
    pregunta: &str,
    registro: &RegistroTurno,
) -> Result<(), ViolacionArnes> {
    if !contiene_locucion(respuesta, LOCUCIONES_DE_ABSTENCION)
        || contiene_locucion(pregunta, PETICIONES_DE_REFERENCIA)
    {
        return Ok(());
    }
    match primera_cifra_sin_anclar(respuesta, pregunta, registro) {
        Some(cifra) => Err(ViolacionArnes::CifraEnAbstencion { cifra }),
        None => Ok(()),
    }
}

/// Una ausencia afirmada con rotundidad exige haber buscado ese aspecto en el turno
/// (`ssd_arnes_negaciones.md`). Las abstenciones sobre la búsqueda no cuentan
fn comprobar_negaciones_buscadas(
    respuesta: &str,
    pregunta: &str,
    registro: &RegistroTurno,
) -> Result<(), ViolacionArnes> {
    match negacion_sin_busqueda(respuesta, pregunta, || registro.lo_consultado()) {
        Some(aspecto) => Err(ViolacionArnes::NegacionSinBusqueda { aspecto }),
        None => Ok(()),
    }
}

/// Por palabras completas: da igual cómo se separen y `noconsta` no cuenta. Un identificador con
/// `_` (`vigencia_no_declarada`) es una sola palabra, salvo los de `IDENTIFICADORES_DE_ABSTENCION`.
fn contiene_locucion(texto: &str, locuciones: &[&str]) -> bool {
    let frase = frase_de(&palabras_con_identificadores(&plegar_texto(texto)));
    locuciones
        .iter()
        .any(|locucion| frase.contains(&format!(" {locucion} ")))
}

fn palabras_con_identificadores(plegado: &str) -> Vec<String> {
    plegado
        .split(|letra: char| !(letra.is_alphanumeric() || letra == '_'))
        .map(|palabra| palabra.trim_matches('_'))
        .filter(|palabra| !palabra.is_empty())
        .map(|palabra| {
            IDENTIFICADORES_DE_ABSTENCION
                .iter()
                .find(|(identificador, _)| *identificador == palabra)
                .map_or(palabra, |(_, locucion)| *locucion)
                .to_string()
        })
        .collect()
}

fn primera_cifra_sin_anclar(
    respuesta: &str,
    pregunta: &str,
    registro: &RegistroTurno,
) -> Option<String> {
    let anclas = lecturas_ancladas(pregunta, registro);
    extraer_cifras(respuesta)
        .into_iter()
        .find(|cifra| !cifra.lecturas.iter().any(|l| anclas.contains(l)))
        .map(|cifra| {
            cifra
                .literal
                .chars()
                .take(LONGITUD_MAXIMA_CIFRA_INFORMADA)
                .collect()
        })
}

fn lecturas_ancladas(pregunta: &str, registro: &RegistroTurno) -> HashSet<String> {
    let mut anclas: HashSet<String> = lecturas_de_fuente(pregunta).into_iter().collect();
    anclas.extend(
        registro
            .fecha_de_hoy
            .map(|hoy| hoy.format("%Y-%m-%d").to_string()),
    );
    for resultado in registro.salidas_validas() {
        recorrer_textos(&resultado.output, |texto| {
            anclas.extend(lecturas_de_fuente(texto));
        });
    }
    anclas
}

/// Recorrido con pila propia: una salida muy anidada no agota la pila del hilo.
fn recorrer_textos(raiz: &Value, mut visitar: impl FnMut(&str)) {
    let mut pendientes = vec![raiz];
    while let Some(valor) = pendientes.pop() {
        match valor {
            Value::String(texto) => visitar(texto),
            Value::Number(numero) => visitar(&numero.to_string()),
            Value::Array(elementos) => pendientes.extend(elementos),
            Value::Object(campos) => {
                for (clave, contenido) in campos {
                    visitar(clave);
                    pendientes.push(contenido);
                }
            }
            Value::Null | Value::Bool(_) => {}
        }
    }
}

fn comprobar_respaldo_de_rol(
    respuesta: &str,
    registro: &RegistroTurno,
) -> Result<(), ViolacionArnes> {
    if !registro.matriz_sin_responsable() {
        return Ok(());
    }
    let plegada = plegar_texto(respuesta);
    if LOCUCIONES_DE_RECHAZO.iter().any(|l| plegada.contains(l))
        || cita_una_tool(&plegada, registro)
    {
        return Ok(());
    }
    Err(ViolacionArnes::RolSinRespaldo)
}

fn cita_una_tool(respuesta_plegada: &str, registro: &RegistroTurno) -> bool {
    let citas = citas_literales(respuesta_plegada);
    if citas.is_empty() {
        return false;
    }
    let mut fuentes = Vec::new();
    // La señal de la matriz es texto del arnés, no del documento: citarla no respalda nada.
    for resultado in registro
        .salidas_validas()
        .filter(|r| !es_matriz_sin_responsable(r))
    {
        // Una cita que cruza un salto de página no contiene la marca `⟦p. N⟧` de la salida ni el
        // salto de línea que la precede
        recorrer_textos(&resultado.output, |texto| {
            fuentes.push(con_espacios_simples(&plegar_texto(&quitar_marcas(texto))))
        });
    }
    citas.iter().any(|cita| {
        let cita = con_espacios_simples(cita);
        fuentes.iter().any(|fuente| fuente.contains(&cita))
    })
}

fn con_espacios_simples(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn citas_literales(texto: &str) -> Vec<String> {
    let mut citas = Vec::new();
    for (apertura, cierre) in PARES_DE_COMILLAS {
        let mut resto = texto;
        while let Some((_, tras_apertura)) = resto.split_once(*apertura) {
            let Some((cita, tras_cierre)) = tras_apertura.split_once(*cierre) else {
                break;
            };
            let cita = cita.trim();
            if cita.chars().count() >= LONGITUD_MINIMA_CITA {
                citas.push(cita.to_string());
            }
            resto = tras_cierre;
        }
    }
    citas.truncate(MAXIMO_CITAS_COMPROBADAS);
    citas
}

/// Lo que se puede buscar de una respuesta sin volver a recorrerla por cada aviso
struct RespuestaIndexada {
    plegada: String,
    palabras: HashSet<String>,
    frase: String,
    cifras: HashSet<String>,
}

impl RespuestaIndexada {
    fn nueva(respuesta: &str) -> Self {
        let plegada = plegar_texto(respuesta);
        let palabras = palabras_de(&plegada);
        Self {
            frase: frase_de(&palabras),
            palabras: palabras.into_iter().collect(),
            cifras: extraer_cifras(respuesta)
                .into_iter()
                .flat_map(|c| c.lecturas)
                .collect(),
            plegada,
        }
    }

    /// `125 (*)` se busca por sus cifras y `No admisible (*)`, que no tiene, por sus palabras.
    fn menciona_valor(&self, valor: &str) -> bool {
        let cifras = extraer_cifras(valor);
        if !cifras.is_empty() {
            return cifras
                .iter()
                .all(|c| c.lecturas.iter().any(|l| self.cifras.contains(l)));
        }
        let buscada = frase_de(&palabras_de(&plegar_texto(valor)));
        buscada != " " && self.frase.contains(&buscada)
    }

    fn contiene_sena(&self, sena: &str) -> bool {
        if sena.chars().all(char::is_alphanumeric) {
            self.palabras.contains(sena)
        } else {
            self.plegada.contains(sena)
        }
    }
}

fn palabras_de(plegado: &str) -> Vec<String> {
    plegado
        .split(|letra: char| !letra.is_alphanumeric())
        .filter(|palabra| !palabra.is_empty())
        .map(str::to_string)
        .collect()
}

/// Con espacios en los extremos, para buscar palabras completas con `contains`.
fn frase_de(palabras: &[String]) -> String {
    format!(" {} ", palabras.join(" "))
}

/// Palabras del nombre del eje y su unidad: lo que permite reconocerlo en una respuesta
fn senas_del_eje(eje: &Value) -> HashSet<String> {
    let nombre = eje
        .get("nombre")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut senas: HashSet<String> = palabras_de(&plegar_texto(nombre))
        .into_iter()
        .filter(|palabra| !PALABRAS_DE_ENLACE.contains(&palabra.as_str()))
        .collect();
    if let Some(unidad) = eje.get("unidad").and_then(Value::as_str) {
        let unidad = plegar_texto(unidad.trim());
        if !unidad.is_empty() {
            senas.insert(unidad);
        }
    }
    senas
}

/// Una seña compartida por los dos ejes no distingue nada, así que no los aclara.
fn nombra_ambos_ejes(respuesta: &RespuestaIndexada, aviso: &Value) -> bool {
    let senas_x = senas_del_eje(&aviso["eje_x"]);
    let senas_y = senas_del_eje(&aviso["eje_y"]);
    senas_x
        .difference(&senas_y)
        .any(|sena| respuesta.contiene_sena(sena))
        && senas_y
            .difference(&senas_x)
            .any(|sena| respuesta.contiene_sena(sena))
}

fn menciona_orden(respuesta: &RespuestaIndexada, orden: &Value) -> bool {
    orden["celdas"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|celda| celda["valor_resultado_texto"].as_str())
        .any(|valor| respuesta.menciona_valor(valor))
}

/// Solo se rechaza dar el valor de un único orden sin decir qué eje es cada dato. Mostrar los dos
/// valores o preguntar sin dar ninguno es lo que pide la regla 13 del prompt.
fn comprobar_ejes_aclarados(
    respuesta: &str,
    registro: &RegistroTurno,
) -> Result<(), ViolacionArnes> {
    let mut avisos = registro
        .salidas_validas()
        .filter(|r| r.tool_name == LakehouseToolName::LookupDoubleEntryTable.as_str())
        .filter_map(|r| r.output.get(SENAL_EJES_AMBIGUOS))
        .filter(|aviso| aviso.is_object())
        .peekable();
    if avisos.peek().is_none() {
        return Ok(());
    }
    let indexada = RespuestaIndexada::nueva(respuesta);
    let sin_aclarar = avisos.any(|aviso| {
        menciona_orden(&indexada, &aviso["orden_recibido"])
            != menciona_orden(&indexada, &aviso["orden_invertido"])
            && !nombra_ambos_ejes(&indexada, aviso)
    });
    if sin_aclarar {
        return Err(ViolacionArnes::EjesAmbiguosSinAclarar);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use serde_json::{Value, json};

    const PREGUNTA: &str = "Tabla 4-A, ¿temperatura para 30 mm y CE 0,42 %?";

    fn cruzable() -> IntencionTurno {
        IntencionTurno::TabularCruzable {
            tabla_mencionada: Some("4-A".to_string()),
        }
    }

    fn resultado(herramienta: &str, salida: Value, es_error: bool) -> ToolResult {
        ToolResult {
            call_id: format!("llamada_{herramienta}"),
            tool_name: herramienta.to_string(),
            output: salida,
            is_error: es_error,
        }
    }

    fn registro_con(resultados: Vec<ToolResult>) -> RegistroTurno {
        let mut registro = RegistroTurno::default();
        for resultado in resultados {
            registro.registrar(resultado);
        }
        registro
    }

    fn lookup_resuelto() -> RegistroTurno {
        registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "estado": "resuelto", "valor_salida": 150, "condicion": "≥ 0.35" }),
            false,
        )])
    }

    fn validar_cruzable(respuesta: &str, registro: &RegistroTurno) -> Result<(), ViolacionArnes> {
        PostInferenceGuard::validar(respuesta, PREGUNTA, &cruzable(), registro)
    }

    #[test]
    fn test_turno_sin_intencion_no_se_valida() {
        assert_eq!(
            PostInferenceGuard::validar(
                "La cifra es 999",
                "¿Qué es?",
                &IntencionTurno::Ninguna,
                &RegistroTurno::default()
            ),
            Ok(())
        );
    }

    #[test]
    fn test_turno_sin_llamada_es_capa_dos_no_intentada() {
        assert_eq!(
            validar_cruzable("Es la tabla anterior", &RegistroTurno::default()),
            Err(ViolacionArnes::CapaDosNoIntentada)
        );
    }

    #[test]
    fn test_tabular_incompleta_tambien_exige_el_intento() {
        assert_eq!(
            PostInferenceGuard::validar(
                "Falta el otro eje",
                "¿Qué tolerancia corresponde a 30 mm?",
                &IntencionTurno::TabularIncompleta,
                &RegistroTurno::default()
            ),
            Err(ViolacionArnes::CapaDosNoIntentada)
        );
    }

    #[test]
    fn test_otra_tool_o_tool_inexistente_no_cuentan_como_intento() {
        let registro = registro_con(vec![
            resultado("search_normative_context", json!({ "texto": "150" }), false),
            resultado("tool_inventada", json!({}), true),
        ]);
        assert_eq!(
            validar_cruzable("Son 150", &registro),
            Err(ViolacionArnes::CapaDosNoIntentada)
        );
    }

    #[test]
    fn test_sql_cuenta_como_capa_dos_intentada() {
        let registro = registro_con(vec![resultado(
            "execute_athena_sql",
            json!({ "filas": [{ "total": 3 }] }),
            false,
        )]);
        assert_eq!(validar_cruzable("Hay 3", &registro), Ok(()));
    }

    #[test]
    fn test_sin_coincidencia_cuenta_como_capa_dos_intentada() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "estado": "sin_coincidencia" }),
            false,
        )]);
        assert_eq!(
            validar_cruzable("La tabla no cubre 30 mm", &registro),
            Ok(())
        );
    }

    #[test]
    fn test_error_de_la_tool_cuenta_como_intento_pero_no_ancla_cifras() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "error": "Tabla 777 inexistente" }),
            true,
        )]);
        assert_eq!(validar_cruzable("No se pudo consultar", &registro), Ok(()));
        assert_eq!(
            validar_cruzable("El valor es 777", &registro),
            Err(ViolacionArnes::CifraSinAnclar {
                cifra: "777".to_string()
            })
        );
    }

    #[test]
    fn test_documento_sin_capa_dos_responde_desde_capa_cero_tras_el_intento() {
        let registro = registro_con(vec![
            resultado(
                "lookup_double_entry_table",
                json!({ "estado": "sin_coincidencia" }),
                false,
            ),
            resultado(
                "search_normative_context",
                json!({ "fragmentos": [{ "texto": "Precalentar a 120 °C" }] }),
                false,
            ),
        ]);
        assert_eq!(
            validar_cruzable("Según la Capa 0, precalentar a 120 °C", &registro),
            Ok(())
        );
    }

    #[test]
    fn test_cifra_de_la_pregunta_o_de_la_tool_se_acepta() {
        assert_eq!(
            validar_cruzable(
                "Para 30 mm y CE 0.42 % corresponden 150 °C (≥ 0,35)",
                &lookup_resuelto()
            ),
            Ok(())
        );
    }

    #[test]
    fn test_cifra_presente_solo_en_historial_se_rechaza() {
        // El registro y la pregunta son lo único que ve el guard: el 180 solo estaba en el historial.
        assert_eq!(
            validar_cruzable("Corresponden 180 °C", &lookup_resuelto()),
            Err(ViolacionArnes::CifraSinAnclar {
                cifra: "180".to_string()
            })
        );
    }

    #[test]
    fn test_cifra_anclada_dentro_de_estructuras_anidadas() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "celdas": [{ "rango": ["10 a 20", { "valor": 2.5 }] }] }),
            false,
        )]);
        assert_eq!(validar_cruzable("Valen 10, 20 y 2,50", &registro), Ok(()));
    }

    #[test]
    fn test_signo_contrario_no_ancla() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "valor": -10 }),
            false,
        )]);
        assert_eq!(
            validar_cruzable("Son 10 °C", &registro),
            Err(ViolacionArnes::CifraSinAnclar {
                cifra: "10".to_string()
            })
        );
    }

    #[test]
    fn test_lista_numerada_e_identificadores_no_se_rechazan() {
        assert_eq!(
            validar_cruzable(
                "1. Según la Tabla 4-A y S355:\n2. Corresponden 150 °C",
                &lookup_resuelto()
            ),
            Ok(())
        );
    }

    fn matriz_sin_responsable() -> ToolResult {
        resultado(
            "query_operational_matrix",
            json!({
                "sin_responsable_declarado": true,
                "accion_sugerida": "Busca el texto literal en la Capa 0 del documento",
            }),
            false,
        )
    }

    fn validar_responsable(
        respuesta: &str,
        registro: &RegistroTurno,
    ) -> Result<(), ViolacionArnes> {
        PostInferenceGuard::validar(
            respuesta,
            "¿Quién firma el registro?",
            &IntencionTurno::Responsable,
            registro,
        )
    }

    #[test]
    fn test_responsable_sin_llamada_a_la_matriz_es_capa_dos_no_intentada() {
        assert_eq!(
            validar_responsable("Lo firma el jefe de obra", &RegistroTurno::default()),
            Err(ViolacionArnes::CapaDosNoIntentada)
        );
    }

    #[test]
    fn test_sin_responsable_exige_locucion_de_rechazo() {
        let registro = registro_con(vec![matriz_sin_responsable()]);
        assert_eq!(
            validar_responsable("Lo firma el jefe de obra", &registro),
            Err(ViolacionArnes::RolSinRespaldo)
        );
        for respuesta in ["No consta quién lo firma", "Responsable NO DECLARADO"] {
            assert_eq!(
                validar_responsable(respuesta, &registro),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    fn busqueda_con_marcas() -> ToolResult {
        resultado(
            "search_normative_context",
            json!({ "grupos": [{ "resultados": [{
                "texto": "⟦p. 52⟧ El acta de recepción la firma \n⟦p. 53⟧ el inspector de turno del almacén.",
                "cita": { "pagina_inicio": 52, "pagina_fin": 54, "paginas": "52-54" }
            }] }] }),
            false,
        )
    }

    #[test]
    fn test_la_pagina_de_una_marca_queda_anclada_y_la_intermedia_sin_marca_no() {
        let con_marcas = registro_con(vec![busqueda_con_marcas()]);
        assert_eq!(
            primera_cifra_sin_anclar("El dato está en la p. 53.", "¿Quién firma?", &con_marcas),
            None
        );
        let solo_rango = registro_con(vec![resultado(
            "search_normative_context",
            json!({ "texto": "El acta la firma el inspector.", "cita": { "paginas": "52-54" } }),
            false,
        )]);
        assert_eq!(
            primera_cifra_sin_anclar("El dato está en la p. 53.", "¿Quién firma?", &solo_rango),
            Some("53".to_string())
        );
    }

    #[test]
    fn test_cita_literal_que_cruza_una_marca_de_pagina_respalda_el_rol() {
        let registro = registro_con(vec![matriz_sin_responsable(), busqueda_con_marcas()]);
        assert_eq!(
            validar_responsable(
                "El documento dice «la firma el inspector de turno del almacén» (pp. 52-53).",
                &registro
            ),
            Ok(())
        );
    }

    #[test]
    fn test_abstencion_con_pagina_de_una_marca_no_es_cifra_ajena() {
        let registro = registro_con(vec![matriz_sin_responsable(), busqueda_con_marcas()]);
        assert_eq!(
            validar_responsable(
                "No consta el responsable; el párrafo de la p. 53 solo nombra al inspector.",
                &registro
            ),
            Ok(())
        );
    }

    #[test]
    fn test_sin_responsable_admite_cita_literal_de_una_tool() {
        let registro = registro_con(vec![
            matriz_sin_responsable(),
            resultado(
                "search_normative_context",
                json!({ "texto": "El registro lo cumplimenta el inspector de turno." }),
                false,
            ),
        ]);
        assert_eq!(
            validar_responsable(
                "El documento dice: «lo cumplimenta el inspector de turno»",
                &registro
            ),
            Ok(())
        );
        assert_eq!(
            validar_responsable(
                "El documento dice: \"lo firma el inspector de guardia\"",
                &registro
            ),
            Err(ViolacionArnes::RolSinRespaldo)
        );
    }

    #[test]
    fn test_cita_de_la_propia_senal_de_la_matriz_no_es_respaldo() {
        let registro = registro_con(vec![matriz_sin_responsable()]);
        assert_eq!(
            validar_responsable("Dice: «Busca el texto literal en la Capa 0»", &registro),
            Err(ViolacionArnes::RolSinRespaldo)
        );
    }

    #[test]
    fn test_cita_demasiado_corta_no_es_respaldo() {
        let registro = registro_con(vec![
            matriz_sin_responsable(),
            resultado(
                "search_normative_context",
                json!({ "texto": "el jefe de obra" }),
                false,
            ),
        ]);
        assert_eq!(
            validar_responsable("Lo firma «el jefe»", &registro),
            Err(ViolacionArnes::RolSinRespaldo)
        );
    }

    #[test]
    fn test_con_responsable_declarado_no_hace_falta_locucion() {
        let registro = registro_con(vec![resultado(
            "query_operational_matrix",
            json!({ "sin_responsable_declarado": false, "filas": [{ "rol_responsable": "Jefe" }] }),
            false,
        )]);
        assert_eq!(validar_responsable("Lo firma el Jefe", &registro), Ok(()));
    }

    #[test]
    fn test_cifra_sin_anclar_tambien_rige_en_responsable() {
        let registro = registro_con(vec![resultado(
            "query_operational_matrix",
            json!({ "sin_responsable_declarado": false }),
            false,
        )]);
        assert_eq!(
            validar_responsable("Lo firma en 48 horas", &registro),
            Err(ViolacionArnes::CifraSinAnclar {
                cifra: "48".to_string()
            })
        );
    }

    fn lookup_con_ejes_ambiguos() -> RegistroTurno {
        registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({
                "estado": "resuelto",
                "valor_resultado_texto": "125 (*)",
                "ejes_ambiguos": {
                    "eje_x": { "nombre": "espesor_nominal_mm", "unidad": "mm" },
                    "eje_y": { "nombre": "carbono_equivalente_pct", "unidad": "%" },
                    "orden_recibido": { "valor_x": 30, "valor_y": 0.42, "celdas": [{ "valor_resultado_texto": "125 (*)" }] },
                    "orden_invertido": { "valor_x": 0.42, "valor_y": 30, "celdas": [{ "valor_resultado_texto": "75" }] },
                },
            }),
            false,
        )])
    }

    #[test]
    fn test_ejes_ambiguos_con_un_unico_valor_sin_aclarar_se_rechaza() {
        for respuesta in ["Corresponden 125 °C (*).", "Corresponden 75 °C."] {
            assert_eq!(
                validar_cruzable(respuesta, &lookup_con_ejes_ambiguos()),
                Err(ViolacionArnes::EjesAmbiguosSinAclarar),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_ejes_ambiguos_con_un_solo_eje_nombrado_se_rechaza() {
        assert_eq!(
            validar_cruzable(
                "Para un espesor nominal corresponden 125 °C.",
                &lookup_con_ejes_ambiguos()
            ),
            Err(ViolacionArnes::EjesAmbiguosSinAclarar)
        );
    }

    #[test]
    fn test_ejes_ambiguos_aclarados_con_nombres_o_unidades_se_aceptan() {
        for respuesta in [
            "Con espesor 30 y carbono equivalente 0,42 corresponden 125 °C (*).",
            "Con ESPESOR 30 y CARBONO 0,42 corresponden 125 °C (*).",
            "Para 30 mm y CE 0,42 % corresponden 125 °C (*).",
        ] {
            assert_eq!(
                validar_cruzable(respuesta, &lookup_con_ejes_ambiguos()),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_ejes_ambiguos_con_ambos_valores_o_sin_valor_se_aceptan() {
        for respuesta in [
            "Según el orden de los ejes son 125 °C (*) o 75 °C.",
            "¿Cuál es el espesor y cuál el otro dato? La tabla admite los dos órdenes.",
        ] {
            assert_eq!(
                validar_cruzable(respuesta, &lookup_con_ejes_ambiguos()),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_ejes_con_nombres_y_unidades_iguales_solo_se_aclaran_con_ambos_valores() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "ejes_ambiguos": {
                "eje_x": { "nombre": "espesor", "unidad": "mm" },
                "eje_y": { "nombre": "espesor", "unidad": "mm" },
                "orden_recibido": { "celdas": [{ "valor_resultado_texto": "125" }] },
                "orden_invertido": { "celdas": [{ "valor_resultado_texto": "75" }] },
            }}),
            false,
        )]);
        assert_eq!(
            validar_cruzable("Con espesor 30 mm son 125 °C", &registro),
            Err(ViolacionArnes::EjesAmbiguosSinAclarar)
        );
        assert_eq!(validar_cruzable("Son 125 °C o 75 °C", &registro), Ok(()));
    }

    #[test]
    fn test_ejes_ambiguos_con_valores_categoricos() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "ejes_ambiguos": {
                "eje_x": { "nombre": "clase", "unidad": null },
                "eje_y": { "nombre": "grupo", "unidad": null },
                "orden_recibido": { "celdas": [{ "valor_resultado_texto": "No admisible (*)" }] },
                "orden_invertido": { "celdas": [{ "valor_resultado_texto": "Admitido" }] },
            }}),
            false,
        )]);
        assert_eq!(
            validar_cruzable("Resulta no admisible.", &registro),
            Err(ViolacionArnes::EjesAmbiguosSinAclarar)
        );
        assert_eq!(
            validar_cruzable(
                "Resulta no admisible o admitido, según el orden.",
                &registro
            ),
            Ok(())
        );
    }

    #[test]
    fn test_mismo_valor_en_ambas_celdas_no_exige_aclarar() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "ejes_ambiguos": {
                "eje_x": { "nombre": "a", "unidad": null },
                "eje_y": { "nombre": "b", "unidad": null },
                "orden_recibido": { "celdas": [{ "valor_resultado_texto": "125" }] },
                "orden_invertido": { "celdas": [{ "valor_resultado_texto": "125" }] },
            }}),
            false,
        )]);
        assert_eq!(validar_cruzable("Son 125 °C", &registro), Ok(()));
    }

    const PREGUNTA_DATO: &str = "¿Qué presión tiene la línea L-999?";

    fn validar_dato(respuesta: &str, registro: &RegistroTurno) -> Result<(), ViolacionArnes> {
        PostInferenceGuard::validar(
            respuesta,
            PREGUNTA_DATO,
            &IntencionTurno::DatoConcreto,
            registro,
        )
    }

    fn busqueda(texto: &str) -> RegistroTurno {
        registro_con(vec![resultado(
            "search_normative_context",
            json!({ "fragmentos": [{ "texto": texto }] }),
            false,
        )])
    }

    fn abstencion_con(cifra: &str) -> Result<(), ViolacionArnes> {
        Err(ViolacionArnes::CifraEnAbstencion {
            cifra: cifra.to_string(),
        })
    }

    #[test]
    fn test_abstencion_con_cifra_ajena_se_rechaza() {
        assert_eq!(
            validar_dato(
                "No consta. Como referencia, suele ser 7,5 bar.",
                &busqueda("Sin datos de esa línea")
            ),
            abstencion_con("7,5")
        );
    }

    #[test]
    fn test_cada_locucion_de_abstencion_activa_el_guard() {
        for frase in [
            "No consta",
            "NO CONSTAN",
            "no declarado",
            "No figura",
            "no aparece",
            "No se encuentra",
            "No he encontrado",
            "no se ha encontrado",
            "No hay datos",
            "no dispongo",
            "No está disponible",
            "No se especifica",
            "no se indica",
            "Sin coincidencia",
            "no existe",
        ] {
            assert_eq!(
                validar_dato(&format!("{frase} ese dato; suele ser 7,5."), &busqueda("")),
                abstencion_con("7,5"),
                "{frase}"
            );
        }
    }

    #[test]
    fn test_abstencion_sin_cifras_o_con_las_de_la_pregunta_se_acepta() {
        for respuesta in [
            "No consta la presión de esa línea.",
            "La línea L-999 no consta en el centro.",
            "La línea L 999 no consta.",
        ] {
            assert_eq!(
                validar_dato(respuesta, &busqueda("")),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_respuesta_parcial_con_cifra_anclada_no_se_bloquea() {
        let registro = busqueda("La línea L-999 trabaja a 14,5 bar.");
        for respuesta in [
            "La presión es 14,5 bar; el caudal no consta.",
            "El caudal no consta. La presión es 14.5 bar.",
            "No consta el caudal, pero sí la presión: 14,50 bar.",
        ] {
            assert_eq!(validar_dato(respuesta, &registro), Ok(()), "{respuesta}");
        }
    }

    #[test]
    fn test_respuesta_parcial_con_una_cifra_anclada_y_otra_no_se_rechaza() {
        assert_eq!(
            validar_dato(
                "La presión es 14,5 bar; el caudal no consta, suele rondar 30.",
                &busqueda("La línea L-999 trabaja a 14,5 bar.")
            ),
            abstencion_con("30")
        );
    }

    #[test]
    fn test_si_el_operador_pide_referencias_la_abstencion_puede_llevar_cifras() {
        for pregunta in [
            "¿Qué presión tiene la línea L-999? Si no consta, dame un valor de referencia",
            "¿Qué presión tiene? Dame VALORES DE REFERENCIA",
            "¿Qué presión es orientativa?",
            "¿Cuál es el valor típico de presión?",
            "¿Qué presión es habitual? Dame un valor aproximado",
        ] {
            assert_eq!(
                PostInferenceGuard::validar(
                    "No consta. Suele ser 7,5 bar.",
                    pregunta,
                    &IntencionTurno::DatoConcreto,
                    &busqueda("")
                ),
                Ok(()),
                "{pregunta}"
            );
        }
    }

    #[test]
    fn test_la_palabra_referencia_sola_no_es_pedir_referencias() {
        assert_eq!(
            PostInferenceGuard::validar(
                "No consta. Suele ser UNE 1234.",
                "¿Cuál es la referencia del procedimiento?",
                &IntencionTurno::DatoConcreto,
                &busqueda("")
            ),
            abstencion_con("1234")
        );
    }

    #[test]
    fn test_la_locucion_no_coincide_dentro_de_otra_palabra() {
        assert_eq!(
            validar_dato("El tubo noconsta de 7,5 bar", &busqueda("")),
            Ok(())
        );
    }

    #[test]
    fn test_abstencion_en_turno_tabular_nombra_la_violacion_propia() {
        assert_eq!(
            validar_cruzable("No consta; suele ser 180 °C.", &lookup_resuelto()),
            abstencion_con("180")
        );
    }

    #[test]
    fn test_abstencion_tabular_sin_intento_sigue_pidiendo_la_consulta() {
        assert_eq!(
            validar_cruzable("No consta; suele ser 180 °C.", &RegistroTurno::default()),
            Err(ViolacionArnes::CapaDosNoIntentada)
        );
    }

    #[test]
    fn test_referencia_pedida_no_exime_del_anclaje_en_un_turno_tabular() {
        assert_eq!(
            PostInferenceGuard::validar(
                "No consta; suele ser 180 °C.",
                "Tabla 4-A, 30 mm y CE 0,42 %: dame un valor de referencia",
                &cruzable(),
                &lookup_resuelto()
            ),
            Err(ViolacionArnes::CifraSinAnclar {
                cifra: "180".to_string()
            })
        );
    }

    #[test]
    fn test_dato_concreto_no_exige_tool_ni_ancla_si_no_hay_abstencion() {
        // SSD §3.3: en esta intención solo se valida la abstención.
        assert_eq!(
            validar_dato("Son 999 bar.", &RegistroTurno::default()),
            Ok(())
        );
    }

    #[test]
    fn test_turno_sin_intencion_no_valida_la_abstencion() {
        assert_eq!(
            PostInferenceGuard::validar(
                "No consta; suele ser 7,5 bar.",
                "¿Qué es el refuerzo?",
                &IntencionTurno::Ninguna,
                &RegistroTurno::default()
            ),
            Ok(())
        );
    }

    #[test]
    fn test_negacion_sin_busqueda_se_rechaza_en_turnos_retenidos() {
        let negacion =
            "Para 30 mm y CE 0,42 % corresponden 150 °C. No hay ampliación de ese límite.";
        assert_eq!(
            validar_cruzable(negacion, &lookup_resuelto()),
            Err(ViolacionArnes::NegacionSinBusqueda {
                aspecto: "ampliacion".to_string()
            })
        );
        assert_eq!(
            PostInferenceGuard::validar(
                "No existe excepción alguna.",
                "¿Cuál es el plazo de acopio?",
                &IntencionTurno::DatoConcreto,
                &registro_con(vec![resultado(
                    "execute_athena_sql",
                    json!({ "filas": [{ "plazo_acopio_dias": 7 }] }),
                    false
                )])
            ),
            Err(ViolacionArnes::NegacionSinBusqueda {
                aspecto: "excepcion".to_string()
            })
        );
        assert!(
            ViolacionArnes::NegacionSinBusqueda {
                aspecto: "ampliacion".to_string()
            }
            .motivo()
            .contains("search_normative_context")
        );
    }

    #[test]
    fn test_negacion_respaldada_o_no_consta_tras_buscar_se_aceptan() {
        let con_regla = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "estado": "resuelto", "valor_salida": 150, "contexto_apartado": {
                    "texto": "El límite no admite ampliación alguna."
                } }),
            false,
        )]);
        assert_eq!(
            validar_cruzable("Son 150 °C. No hay ampliación posible.", &con_regla),
            Ok(())
        );
        let mut tras_buscar = RegistroTurno::default();
        tras_buscar.registrar_llamada(&ToolCall {
            id: "b".into(),
            name: "search_normative_context".into(),
            arguments: json!({ "consulta": "plazo de garantía de la bomba P-12" }),
        });
        tras_buscar.registrar(resultado(
            "search_normative_context",
            json!({ "grupos": [], "total": 0 }),
            false,
        ));
        for respuesta in [
            "No consta el plazo de garantía de la bomba P-12.",
            "No hay plazo de garantía declarado para la P-12.",
        ] {
            assert_eq!(
                PostInferenceGuard::validar(
                    respuesta,
                    "¿Qué plazo de garantía tiene la bomba P-12?",
                    &IntencionTurno::DatoConcreto,
                    &tras_buscar
                ),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_negacion_tras_leer_texto_o_con_vocabulario_de_la_app_se_acepta() {
        assert_eq!(
            PostInferenceGuard::validar(
                "No hay ampliación de ese plazo.",
                "¿Cuál es el plazo de acopio?",
                &IntencionTurno::DatoConcreto,
                &busqueda("El plazo de acopio es de 7 días.")
            ),
            Ok(())
        );
        let mut registro = lookup_resuelto();
        registro.fijar_vocabulario_de_la_app("Ante un conflicto prevalece el nivel menor.");
        assert_eq!(
            validar_cruzable(
                "Son 150 °C. No hay conflicto con otro documento.",
                &registro
            ),
            Ok(())
        );
    }

    #[test]
    fn test_negacion_en_turno_sin_intencion_no_se_valida() {
        assert_eq!(
            PostInferenceGuard::validar(
                "No hay ampliación.",
                "¿Qué es el refuerzo?",
                &IntencionTurno::Ninguna,
                &RegistroTurno::default()
            ),
            Ok(())
        );
    }

    /// Salida con la forma real de `celda_salida` en `lookup.rs`, con datos sintéticos
    fn lookup_con_forma_real(revision: &str, valor: &str) -> RegistroTurno {
        registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({
                "estado": "resuelto",
                "valor_x": 30,
                "valor_y": 0.42,
                "tabla": {
                    "tabla_id": "4-A",
                    "titulo_tabla": "Precalentamiento sintético",
                    "eje_x": { "nombre": "espesor_nominal_mm", "unidad": "mm" },
                    "eje_y": { "nombre": "carbono_equivalente_pct", "unidad": "%" },
                    "valor_salida_nombre": "temperatura_c",
                },
                "resultado": {
                    "valor_resultado_texto": valor,
                    "valor_resultado_num": 125.0,
                    "unidad_medida": "°C",
                    "nota_pie_condicion": "Solo con acopio de más de 7 días",
                    "rango_x": "20 < x <= 40",
                    "rango_y": "0.40 < y <= 0.45",
                    "cita": {
                        "doc_id": "DOC-SINT-007",
                        "revision": revision,
                        "version_num": 3,
                        "tabla_id": "4-A",
                        "celda_id": "8f3a12c4",
                    },
                },
            }),
            false,
        )])
    }

    #[test]
    fn test_respuesta_correcta_en_cualquier_formato_de_cifra_no_se_rechaza() {
        let registro = lookup_con_forma_real("Rev. 03", "125 (*)");
        for respuesta in [
            "Son 125 °C.",
            "Son 125°C.",
            "Son 125ºC.",
            "Son 125 ºC (*).",
            "**125 °C**",
            "| 30 mm | 0,42 % | 125 °C |",
            "Para 30 mm y CE 0,42 %: 125 °C",
            "Para 30mm y CE 0.42%: 125 °C",
            "Rango 20-40 mm y CE 0,40-0,45 %: 125 °C",
            "Rango 20–40 mm: 125 °C",
            "Entre 20 y 40 mm: 125 °C",
            "**1.** Tabla 4-A\n**2.** Son 125 °C",
            "### 1. Resultado\nSon 125 °C",
            "Según la Tabla 4‑A son 125 °C",
            "DOC-SINT-007, Rev. 03: 125 °C",
            "Rev. 3, versión 3: 125 °C",
            "Si el acopio supera 7 días: 125 °C",
        ] {
            assert_eq!(
                validar_cruzable(respuesta, &registro),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_cifra_de_la_fuente_con_formato_pegado_ancla_la_respuesta() {
        for (revision, valor, respuesta) in [
            ("Rev. 03", "125ºC", "Son 125 °C"),
            ("Rev03", "125", "Rev. 03: 125 °C"),
            ("REV-3", "125", "Revisión 3: 125 °C"),
            ("R-03", "125", "Revisión 03: 125 °C"),
        ] {
            let registro = registro_con(vec![resultado(
                "lookup_double_entry_table",
                json!({ "valor_resultado_texto": valor, "cita": { "revision": revision } }),
                false,
            )]);
            assert_eq!(
                validar_cruzable(respuesta, &registro),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_apartado_padre_de_una_numeracion_de_la_fuente_se_acepta() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "valor_resultado_texto": "125", "breadcrumb": "5.2.1 Precalentamiento" }),
            false,
        )]);
        assert_eq!(
            validar_cruzable("Según el apartado 5.2: 125 °C", &registro),
            Ok(())
        );
    }

    #[test]
    fn test_limite_conocido_dato_del_historial_o_recuento_propio_se_siguen_rechazando() {
        // SSD §3.3: ni la página de otro turno ni un recuento redactado están anclados.
        let registro = lookup_con_forma_real("Rev. 03", "125 (*)");
        for (respuesta, cifra) in [
            ("Página 88: 125 °C", "88"),
            ("Vigente desde 2026: 125 °C", "2026"),
            ("Los 2 órdenes dan 125 °C", "2"),
        ] {
            assert_eq!(
                validar_cruzable(respuesta, &registro),
                Err(ViolacionArnes::CifraSinAnclar {
                    cifra: cifra.to_string()
                }),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_turno_sin_intencion_no_exige_aclarar_ejes() {
        assert_eq!(
            PostInferenceGuard::validar(
                "Son 125 °C",
                "¿Y entonces?",
                &IntencionTurno::Ninguna,
                &lookup_con_ejes_ambiguos()
            ),
            Ok(())
        );
    }
    fn hoy() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()
    }

    fn con_fecha_de_hoy(mut registro: RegistroTurno) -> RegistroTurno {
        registro.fijar_fecha_de_hoy(hoy());
        registro
    }

    #[test]
    fn test_locucion_dentro_de_un_identificador_no_es_abstencion() {
        for respuesta in [
            "El campo `vigencia_no_declarada` vino vacío; la Rev. 2 fija 7,5 bar.",
            "El campo vigencia_no_declarada vino vacío; la Rev. 2 fija 7,5 bar.",
            "VIGENCIA_NO_DECLARADA vacío: la Rev. 2 fija 7,5 bar.",
            "Sin `no_consta_en_indice`: la Rev. 2 fija 7,5 bar.",
        ] {
            assert_eq!(
                validar_dato(respuesta, &busqueda("")),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_no_consta_del_origen_responsable_sigue_siendo_abstencion() {
        // Regla 9 del prompt: NO_CONSTA significa que el documento no lo indica.
        for respuesta in [
            "El origen_responsable es NO_CONSTA; suele firmarlo en 7,5 días.",
            "Origen `NO_CONSTA`; suele ser 7,5.",
            "origen no_consta, suele ser 7,5.",
        ] {
            assert_eq!(
                validar_dato(respuesta, &busqueda("")),
                abstencion_con("7,5"),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_fecha_iso_de_la_respuesta_se_ancla_entera() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "valor_resultado_texto": "125", "fecha_vigor": "2026-06-01", "pagina": 11 }),
            false,
        )]);
        assert_eq!(
            validar_cruzable("Desde el 2026-06-01 rigen 125 °C (página 11).", &registro),
            Ok(())
        );
        assert_eq!(
            validar_cruzable("Desde el 2026-06-11 rigen 125 °C.", &registro),
            Err(ViolacionArnes::CifraSinAnclar {
                cifra: "2026-06-11".to_string()
            })
        );
    }

    #[test]
    fn test_fecha_textual_y_fecha_iso_se_anclan_entre_si() {
        let registro = registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "valor_resultado_texto": "125", "fecha_vigor": "2026-06-01" }),
            false,
        )]);
        assert_eq!(
            validar_cruzable("Desde el 1 de junio de 2026 rigen 125 °C.", &registro),
            Ok(())
        );
        assert_eq!(
            PostInferenceGuard::validar(
                "El 2025-09-11 regían 125 °C.",
                "Tabla 4-A, 30 mm y CE 0,42 %: ¿qué temperatura el 11 de septiembre de 2025?",
                &cruzable(),
                &lookup_resuelto_125()
            ),
            Ok(())
        );
    }

    fn lookup_resuelto_125() -> RegistroTurno {
        registro_con(vec![resultado(
            "lookup_double_entry_table",
            json!({ "estado": "resuelto", "valor_resultado_texto": "125" }),
            false,
        )])
    }

    #[test]
    fn test_fecha_de_hoy_inyectada_ancla_la_fecha_de_hoy() {
        for respuesta in [
            "| Hoy (2026-11-03) | 125 °C |",
            "Hoy, 3 de noviembre de 2026, rigen 125 °C.",
        ] {
            assert_eq!(
                validar_cruzable(respuesta, &con_fecha_de_hoy(lookup_resuelto_125())),
                Ok(()),
                "{respuesta}"
            );
        }
    }

    #[test]
    fn test_sin_fecha_de_hoy_la_fecha_no_anclada_se_rechaza_entera() {
        assert_eq!(
            validar_cruzable("| Hoy (2026-11-03) | 125 °C |", &lookup_resuelto_125()),
            Err(ViolacionArnes::CifraSinAnclar {
                cifra: "2026-11-03".to_string()
            })
        );
    }

    #[test]
    fn test_la_fecha_de_hoy_no_ancla_sus_partes_sueltas() {
        assert_eq!(
            validar_cruzable(
                "Son 125 °C, unos 11 más.",
                &con_fecha_de_hoy(lookup_resuelto_125())
            ),
            Err(ViolacionArnes::CifraSinAnclar {
                cifra: "11".to_string()
            })
        );
    }

    #[test]
    fn test_respuesta_rechazada_en_p3_reducida_se_acepta() {
        // Fase 4, P3 con Sonnet: los dos falsos positivos juntos, con datos sintéticos.
        let registro = con_fecha_de_hoy(registro_con(vec![
            resultado(
                "lookup_double_entry_table",
                json!({ "estado": "resuelto", "valor_resultado_texto": "125 (*)" }),
                false,
            ),
            resultado(
                "lookup_double_entry_table",
                json!({ "estado": "resuelto", "valor_resultado_texto": "100", "vigencia_no_declarada": [] }),
                false,
            ),
        ]));
        let respuesta = "| Fecha | Valor |
|---|---|
| Hoy (2026-11-03) | 125 °C (*) |
| 1 de junio de 2026 | 100 °C |

El campo `vigencia_no_declarada` vino vacío.";
        assert_eq!(
            PostInferenceGuard::validar(
                respuesta,
                "¿Cuál es hoy la temperatura de la Tabla 4-A para 30 mm y CE 0,42 %, y cuál era el 1 de junio de 2026?",
                &cruzable(),
                &registro
            ),
            Ok(())
        );
    }
}
