//! Medición F-1 / F-3 del arnés sobre respuestas guardadas (ignorada, sin Claude, sin red).
//!
//! Solo mide: no cambia el guard. Reutiliza la forma de leer las crudas de
//! `repeticion_arnes_guardadas.rs` (funciones copiadas; `leer` guarda además las herramientas en
//! crudo para poder filtrarlas). La salida solo lleva recuentos, ids de fila, nombres de
//! herramienta, rutas de clave JSON y cifras sueltas: nunca texto de preguntas, respuestas ni
//! documentos. Las claves dinámicas (no `snake_case` corto) se enmascaran como `<*>`.
//!
//! Uso (PowerShell):
//! `$env:CRUDAS_GUARDADAS="a.jsonl;b.jsonl"; $env:SALIDA_MEDICION="salida.json";
//!  cargo test --test medicion_f1_f3 -- --ignored --nocapture`
//! Opcional: `CLAVES_B_EXTRA="clave1,clave2"` amplía la lista de la variante B.

// Herramienta de medición puntual: se conserva tal cual se usó, sin pulir estos avisos de estilo
#![allow(
    clippy::collapsible_if,
    clippy::manual_contains,
    clippy::type_complexity
)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::NaiveDate;
use lakehouse_rag::application::dtos::tools::ToolResult;
use lakehouse_rag::application::use_cases::arnes_turno::{
    ENCABEZADO_VERIFICACION_PAGINAS, IntencionTurno, IntentDetector, PostInferenceGuard,
    RegistroTurno, ViolacionArnes,
};
use serde_json::{Map, Value, json};

const CLAVES_B_INICIALES: &[&str] = &[
    "consulta",
    "query_ejecutada",
    "filtros",
    "limite_filas",
    "filas",
    "total",
    "top_k",
    "truncado",
    "modo",
    "centro_id",
    "fecha_referencia",
    "aviso",
];
const LONGITUD_LARGA: usize = 200;
const MAXIMO_RESPALDOS_POR_CIFRA: usize = 8;

// ---------------------------------------------------------------------------------------------
// Lectura de crudas (copiada de repeticion_arnes_guardadas.rs; `leer` añade `herramientas`)
// ---------------------------------------------------------------------------------------------

struct RespuestaGuardada {
    origen: String,
    id: String,
    pregunta: String,
    respuesta: String,
    registro: RegistroTurno,
    herramientas: Vec<Value>,
    medible: bool,
    fecha: Option<NaiveDate>,
}

fn sin_bloque_de_verificacion(respuesta: &str) -> (String, usize) {
    let mut partes = respuesta.split(ENCABEZADO_VERIFICACION_PAGINAS);
    let modelo = partes.next().unwrap_or_default().trim_end().to_string();
    let avisos_guardados = partes
        .next()
        .map(|bloque| {
            bloque
                .lines()
                .filter(|l| l.trim_start().starts_with("- "))
                .count()
        })
        .unwrap_or(0);
    (modelo, avisos_guardados)
}

fn registro_de_herramientas(herramientas: &[Value]) -> RegistroTurno {
    let mut registro = RegistroTurno::default();
    for (indice, herramienta) in herramientas.iter().enumerate() {
        registro.registrar(ToolResult {
            call_id: format!("guardada_{indice}"),
            tool_name: herramienta["nombre"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            output: herramienta["resultado"].clone(),
            is_error: herramienta["es_error"].as_bool().unwrap_or(false),
        });
    }
    registro
}

fn registro_aguja(caso: &Value) -> RegistroTurno {
    let mut agujas: Vec<(String, u64)> = Vec::new();
    if let Some(aguja) = caso.get("aguja").filter(|a| a.is_object()) {
        agujas.push((
            aguja["frase"].as_str().unwrap_or_default().to_string(),
            caso["pagina_esperada"].as_u64().unwrap_or_default(),
        ));
    }
    if let (Some(lista), Some(paginas)) = (
        caso["agujas"].as_array(),
        caso["paginas_esperadas"].as_array(),
    ) {
        for (aguja, pagina) in lista.iter().zip(paginas) {
            agujas.push((
                aguja["frase"].as_str().unwrap_or_default().to_string(),
                pagina.as_u64().unwrap_or_default(),
            ));
        }
    }
    let resultados: Vec<Value> = agujas
        .iter()
        .map(|(frase, pagina)| json!({ "texto": format!("⟦p. {pagina}⟧ {frase}") }))
        .collect();
    let mut registro = RegistroTurno::default();
    registro.registrar(ToolResult {
        call_id: "reconstruido".into(),
        tool_name: "search_normative_context".into(),
        output: json!({ "grupos": [{ "resultados": resultados }] }),
        is_error: false,
    });
    registro
}

/// Fecha de la ejecución, del sello `_AAAAMMDDT` del nombre de las crudas
fn fecha_de_la_ejecucion(ruta: &str) -> Option<NaiveDate> {
    let nombre = ruta.rsplit(['/', '\\']).next()?;
    let sello = nombre
        .split('_')
        .find(|parte| parte.len() > 8 && parte.as_bytes()[8] == b'T')?;
    NaiveDate::parse_from_str(&sello[..8], "%Y%m%d").ok()
}

fn leer(ruta: &str) -> Vec<RespuestaGuardada> {
    let hoy = fecha_de_la_ejecucion(ruta);
    let contenido = std::fs::read_to_string(ruta).expect("crudas");
    let mut respuestas = Vec::new();
    for (linea, texto) in contenido.lines().enumerate() {
        if texto.trim().is_empty() {
            continue;
        }
        let fila: Value = serde_json::from_str(texto).expect("línea JSON");
        if let Some(consultas) = fila["consultas"].as_array() {
            let mut registro = registro_aguja(&fila);
            if let Some(hoy) = hoy {
                registro.fijar_fecha_de_hoy(hoy);
            }
            for (indice, consulta) in consultas.iter().enumerate() {
                if !consulta["fallo"].is_null() {
                    continue;
                }
                respuestas.push(RespuestaGuardada {
                    origen: ruta.to_string(),
                    id: format!("caso{linea}-consulta{indice}"),
                    pregunta: consulta["pregunta"].as_str().unwrap_or_default().into(),
                    respuesta: consulta["respuesta"].as_str().unwrap_or_default().into(),
                    registro: registro.clone(),
                    herramientas: Vec::new(),
                    medible: false,
                    fecha: hoy,
                });
            }
        } else {
            if !fila["fallo"].is_null() || fila["respuesta"].is_null() {
                continue;
            }
            let herramientas = fila["herramientas"].as_array().cloned().unwrap_or_default();
            let mut registro = registro_de_herramientas(&herramientas);
            if let Some(hoy) = hoy {
                registro.fijar_fecha_de_hoy(hoy);
            }
            respuestas.push(RespuestaGuardada {
                origen: ruta.to_string(),
                id: format!(
                    "{}-r{}",
                    fila["id"].as_str().unwrap_or("?"),
                    fila["repeticion"]
                ),
                pregunta: fila["pregunta"].as_str().unwrap_or_default().into(),
                respuesta: fila["respuesta"].as_str().unwrap_or_default().into(),
                registro,
                herramientas,
                medible: true,
                fecha: hoy,
            });
        }
    }
    respuestas
}

// ---------------------------------------------------------------------------------------------
// Utilidades
// ---------------------------------------------------------------------------------------------

fn forzada() -> IntencionTurno {
    IntencionTurno::TabularIncompleta
}

fn etiqueta_intencion(i: &IntencionTurno) -> &'static str {
    match i {
        IntencionTurno::Ninguna => "Ninguna",
        IntencionTurno::TabularIncompleta => "TabularIncompleta",
        IntencionTurno::TabularCruzable { .. } => "TabularCruzable",
        IntencionTurno::Responsable => "Responsable",
        IntencionTurno::DatoConcreto => "DatoConcreto",
    }
}

fn etiqueta(v: &Result<(), ViolacionArnes>) -> &'static str {
    match v {
        Ok(()) => "Ok",
        Err(ViolacionArnes::CapaDosNoIntentada) => "CapaDosNoIntentada",
        Err(ViolacionArnes::CifraSinAnclar { .. }) => "CifraSinAnclar",
        Err(ViolacionArnes::RolSinRespaldo) => "RolSinRespaldo",
        Err(ViolacionArnes::EjesAmbiguosSinAclarar) => "EjesAmbiguosSinAclarar",
        Err(ViolacionArnes::CifraEnAbstencion { .. }) => "CifraEnAbstencion",
        Err(ViolacionArnes::LlamadaEnCrudo) => "LlamadaEnCrudo",
    }
}

fn es_rechazo_cifra(v: &Result<(), ViolacionArnes>) -> bool {
    matches!(
        v,
        Err(ViolacionArnes::CifraSinAnclar { .. }) | Err(ViolacionArnes::CifraEnAbstencion { .. })
    )
}

fn cifra_del_veredicto(v: &Result<(), ViolacionArnes>) -> Option<String> {
    match v {
        Err(ViolacionArnes::CifraSinAnclar { cifra })
        | Err(ViolacionArnes::CifraEnAbstencion { cifra }) => Some(cifra.clone()),
        _ => None,
    }
}

fn registro_con(herramientas: &[Value], fecha: Option<NaiveDate>) -> RegistroTurno {
    let mut registro = registro_de_herramientas(herramientas);
    if let Some(hoy) = fecha {
        registro.fijar_fecha_de_hoy(hoy);
    }
    registro
}

fn registro_de_valor(valor: Value, fecha: Option<NaiveDate>) -> RegistroTurno {
    let mut registro = RegistroTurno::default();
    registro.registrar(ToolResult {
        call_id: "hoja".into(),
        tool_name: "x".into(),
        output: valor,
        is_error: false,
    });
    if let Some(hoy) = fecha {
        registro.fijar_fecha_de_hoy(hoy);
    }
    registro
}

fn pend(
    respuesta: &str,
    pregunta: &str,
    intencion: &IntencionTurno,
    registro: &RegistroTurno,
) -> Vec<String> {
    PostInferenceGuard::cifras_pendientes(respuesta, pregunta, intencion, registro)
        .into_iter()
        .map(|p| p.literal)
        .collect()
}

fn menos(a: &[String], b: &[String]) -> Vec<String> {
    let conjunto: HashSet<&String> = b.iter().collect();
    let mut salida: Vec<String> = Vec::new();
    for x in a {
        if !conjunto.contains(x) && !salida.contains(x) {
            salida.push(x.clone());
        }
    }
    salida
}

fn nombre_de_fichero(ruta: &str) -> String {
    ruta.rsplit(['/', '\\']).next().unwrap_or(ruta).to_string()
}

fn argumentos_de(herramienta: &Value) -> Value {
    match &herramienta["argumentos"] {
        Value::String(texto) => {
            serde_json::from_str(texto).unwrap_or_else(|_| Value::String(texto.clone()))
        }
        otro => otro.clone(),
    }
}

// ---------------------------------------------------------------------------------------------
// Rutas de clave y hojas
// ---------------------------------------------------------------------------------------------

fn clave_segura(clave: &str) -> String {
    let valida = clave.len() <= 40
        && clave.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && clave
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if valida {
        clave.to_string()
    } else {
        "<*>".to_string()
    }
}

struct Hoja {
    ruta: String,
    clave: bool,
    texto: String,
}

fn tipo_de(valor: &Value) -> &'static str {
    match valor {
        Value::Null => "nulo",
        Value::Bool(_) => "bool",
        Value::Number(_) => "numero",
        Value::String(_) => "texto",
        Value::Array(_) => "lista",
        Value::Object(_) => "objeto",
    }
}

/// Hojas (valores escalares y nombres de clave) y nodos (ruta, tipo) de una salida
fn recorrer_hojas(
    valor: &Value,
    ruta: &str,
    hojas: &mut Vec<Hoja>,
    nodos: &mut BTreeSet<(String, &'static str)>,
) {
    if !ruta.is_empty() {
        nodos.insert((ruta.to_string(), tipo_de(valor)));
    }
    match valor {
        Value::String(texto) => hojas.push(Hoja {
            ruta: ruta.to_string(),
            clave: false,
            texto: texto.clone(),
        }),
        Value::Number(numero) => hojas.push(Hoja {
            ruta: ruta.to_string(),
            clave: false,
            texto: numero.to_string(),
        }),
        Value::Array(elementos) => {
            let hija = format!("{ruta}[]");
            for e in elementos {
                recorrer_hojas(e, &hija, hojas, nodos);
            }
        }
        Value::Object(campos) => {
            for (clave, contenido) in campos {
                let segura = clave_segura(clave);
                let hija = if ruta.is_empty() {
                    segura
                } else {
                    format!("{ruta}.{segura}")
                };
                hojas.push(Hoja {
                    ruta: hija.clone(),
                    clave: true,
                    texto: clave.clone(),
                });
                recorrer_hojas(contenido, &hija, hojas, nodos);
            }
        }
        Value::Null | Value::Bool(_) => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Filtros de las variantes A y B
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct ValoresDeArgumentos {
    textos: HashSet<String>,
    largos: Vec<String>,
    numeros: HashSet<String>,
}

fn recoger_argumentos(valor: &Value, acc: &mut ValoresDeArgumentos) {
    match valor {
        Value::String(t) => {
            acc.textos.insert(t.clone());
            if t.chars().count() >= 3 {
                acc.largos.push(t.clone());
            }
        }
        Value::Number(n) => {
            acc.numeros.insert(n.to_string());
        }
        Value::Array(xs) => xs.iter().for_each(|x| recoger_argumentos(x, acc)),
        Value::Object(m) => m.values().for_each(|x| recoger_argumentos(x, acc)),
        Value::Null | Value::Bool(_) => {}
    }
}

#[derive(Default, Clone)]
struct EstadisticaA {
    igual_cadena: u64,
    igual_numero: u64,
    contiene: u64,
    contiene_largas: u64,
}

/// Sensibilidad: con `SIN_CONTENCION` definida, la variante A solo quita valores iguales a un
/// argumento (sin la regla «contiene entero un argumento de texto»)
fn contencion_activa() -> bool {
    static ACTIVA: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ACTIVA.get_or_init(|| std::env::var("SIN_CONTENCION").is_err())
}

fn filtrar_eco(valor: &Value, args: &ValoresDeArgumentos, st: &mut EstadisticaA) -> Value {
    match valor {
        Value::String(texto) => {
            if args.textos.contains(texto) {
                st.igual_cadena += 1;
                Value::Null
            } else if contencion_activa() && args.largos.iter().any(|a| texto.contains(a.as_str()))
            {
                st.contiene += 1;
                if texto.chars().count() > LONGITUD_LARGA {
                    st.contiene_largas += 1;
                }
                Value::Null
            } else {
                valor.clone()
            }
        }
        Value::Number(n) => {
            if args.numeros.contains(&n.to_string()) {
                st.igual_numero += 1;
                Value::Null
            } else {
                valor.clone()
            }
        }
        Value::Array(xs) => Value::Array(xs.iter().map(|x| filtrar_eco(x, args, st)).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), filtrar_eco(v, args, st)))
                .collect::<Map<String, Value>>(),
        ),
        _ => valor.clone(),
    }
}

fn quitar_metadatos(
    valor: Value,
    claves: &HashSet<String>,
    quitadas: &mut BTreeMap<String, u64>,
) -> Value {
    match valor {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .filter(|(k, v)| {
                    let quitar = claves.contains(k) && (k != "filas" || v.is_number());
                    if quitar {
                        *quitadas.entry(k.clone()).or_default() += 1;
                    }
                    !quitar
                })
                .collect::<Map<String, Value>>(),
        ),
        otro => otro,
    }
}

/// Devuelve las herramientas con `resultado` filtrado (A) o filtrado y sin metadatos (B)
fn herramientas_filtradas(
    herramientas: &[Value],
    solo_a: bool,
    claves: &HashSet<String>,
    st: &mut EstadisticaA,
    quitadas: &mut BTreeMap<String, u64>,
) -> Vec<Value> {
    herramientas
        .iter()
        .map(|h| {
            let mut copia = h.clone();
            if h["es_error"].as_bool().unwrap_or(false) {
                return copia;
            }
            let mut args = ValoresDeArgumentos::default();
            recoger_argumentos(&argumentos_de(h), &mut args);
            let mut resultado = filtrar_eco(&h["resultado"], &args, st);
            if !solo_a {
                resultado = quitar_metadatos(resultado, claves, quitadas);
            }
            copia["resultado"] = resultado;
            copia
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Variante C: literales numéricos de la SQL
// ---------------------------------------------------------------------------------------------

/// (literal, va tras LIMIT)
fn literales_numericos_sql(sql: &str) -> Vec<(String, bool)> {
    let ch: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut salida = Vec::new();
    let mut ultima = String::new();
    while i < ch.len() {
        let c = ch[i];
        if c.is_ascii_digit() && (i == 0 || !(ch[i - 1].is_alphanumeric() || ch[i - 1] == '_')) {
            let mut j = i;
            while j < ch.len() && ch[j].is_ascii_digit() {
                j += 1;
            }
            if j + 1 < ch.len() && ch[j] == '.' && ch[j + 1].is_ascii_digit() {
                j += 1;
                while j < ch.len() && ch[j].is_ascii_digit() {
                    j += 1;
                }
            }
            salida.push((ch[i..j].iter().collect(), ultima == "limit"));
            ultima.clear();
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < ch.len() && (ch[j].is_alphanumeric() || ch[j] == '_') {
                j += 1;
            }
            ultima = ch[i..j].iter().collect::<String>().to_lowercase();
            i = j;
            continue;
        }
        if !c.is_whitespace() {
            ultima.clear();
        }
        i += 1;
    }
    salida
}

/// Lecturas numéricas posibles de una cifra o literal (decimal y, si es ambigua, de miles)
fn lecturas_num(texto: &str) -> Vec<String> {
    let t: String = texto
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    let t = t.trim_matches(['.', ',']).to_string();
    if t.is_empty() {
        return Vec::new();
    }
    let norma = |entero: &str, decimal: &str| {
        let e = entero.trim_start_matches('0');
        let e = if e.is_empty() { "0" } else { e };
        let d = decimal.trim_end_matches('0');
        if d.is_empty() {
            e.to_string()
        } else {
            format!("{e}.{d}")
        }
    };
    match t.rfind(['.', ',']) {
        None => vec![norma(&t, "")],
        Some(p) => {
            let entero: String = t[..p].chars().filter(|c| c.is_ascii_digit()).collect();
            let decimal = &t[p + 1..];
            let mut lecturas = vec![norma(&entero, decimal)];
            if decimal.len() == 3 {
                let todo: String = t.chars().filter(|c| c.is_ascii_digit()).collect();
                lecturas.push(norma(&todo, ""));
            }
            lecturas
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Resultado por fila
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct ResVariante {
    veredicto: &'static str,
    nuevo_rechazo_real: bool,
    otro_cambio_real: bool,
    pend_real: Vec<String>,
    pend_f: Vec<String>,
    perdidas_real: Vec<String>,
    perdidas_f: Vec<String>,
    solo_clave: Vec<String>,
}

struct Fila {
    fichero: String,
    id: String,
    medible: bool,
    intencion: &'static str,
    herramientas: Vec<String>,
    cifras: Vec<String>,
    veredicto: &'static str,
    cifra_veredicto: Option<String>,
    abstencion: bool,
    comprueba_hoy: bool,
    pend_real_orig: Vec<String>,
    pend_f_orig: Vec<String>,
    forzado_validar: &'static str,
    afirma_sin_consulta: bool,
    solo_clave_orig: Vec<String>,
    a: Option<ResVariante>,
    b: Option<ResVariante>,
    atenea: bool,
    cand_sql: Vec<String>,
    cand_sql_solo_limit: Vec<String>,
    cand_sql_sin_otro_respaldo_b: Vec<String>,
    perdidas: Vec<Value>,
}

/// Cifras ancladas solo por claves: ancladas por las claves y no por los valores
fn cifras_solo_clave(
    resultados_validos: &[&Value],
    cifras: &[String],
    respuesta: &str,
    pregunta: &str,
    fecha: Option<NaiveDate>,
) -> Vec<String> {
    let mut claves: Vec<Value> = Vec::new();
    let mut valores: Vec<Value> = Vec::new();
    for salida in resultados_validos {
        let mut hojas = Vec::new();
        let mut nodos = BTreeSet::new();
        recorrer_hojas(salida, "", &mut hojas, &mut nodos);
        for h in hojas {
            if h.clave {
                claves.push(Value::String(h.texto));
            } else {
                valores.push(Value::String(h.texto));
            }
        }
    }
    let pend_k = pend(
        respuesta,
        pregunta,
        &forzada(),
        &registro_de_valor(Value::Array(claves), fecha),
    );
    let pend_v = pend(
        respuesta,
        pregunta,
        &forzada(),
        &registro_de_valor(Value::Array(valores), fecha),
    );
    let ancladas_k = menos(cifras, &pend_k);
    let ancladas_v = menos(cifras, &pend_v);
    menos(&ancladas_k, &ancladas_v)
}

fn salidas_validas(herramientas: &[Value]) -> Vec<&Value> {
    herramientas
        .iter()
        .filter(|h| !h["es_error"].as_bool().unwrap_or(false))
        .map(|h| &h["resultado"])
        .collect()
}

#[test]
#[ignore = "herramienta de medición: necesita crudas guardadas fuera del repo"]
fn medir_f1_f3() {
    let rutas = std::env::var("CRUDAS_GUARDADAS").expect("CRUDAS_GUARDADAS");
    let salida = std::env::var("SALIDA_MEDICION").expect("SALIDA_MEDICION");
    let mut claves_b: Vec<String> = CLAVES_B_INICIALES.iter().map(|s| s.to_string()).collect();
    if let Ok(extra) = std::env::var("CLAVES_B_EXTRA") {
        claves_b.extend(
            extra
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        );
    }
    let claves_b_set: HashSet<String> = claves_b.iter().cloned().collect();

    let mut filas: Vec<Fila> = Vec::new();
    let (mut rechazos, mut microllamadas, mut crudo, mut exigen) = (0, 0, 0, 0);
    let mut inventario: BTreeMap<String, BTreeMap<(String, &'static str), u64>> = BTreeMap::new();
    let mut inventario_resultados: BTreeMap<String, u64> = BTreeMap::new();
    let mut est_a = EstadisticaA::default();
    let mut est_b = EstadisticaA::default();
    let mut quitadas_b: BTreeMap<String, u64> = BTreeMap::new();
    let mut sin_argumentos = 0u64;
    let mut herramientas_medibles = 0u64;

    for ruta in rutas.split(';').filter(|r| !r.is_empty()) {
        for g in leer(ruta) {
            let (modelo, _) = sin_bloque_de_verificacion(&g.respuesta);
            let intencion = IntentDetector::clasificar(&g.pregunta);
            let veredicto =
                PostInferenceGuard::validar(&modelo, &g.pregunta, &intencion, &g.registro);
            let pend_real_orig = pend(&modelo, &g.pregunta, &intencion, &g.registro);
            let afirma = PostInferenceGuard::afirma_sin_consulta(
                &modelo,
                &g.pregunta,
                &intencion,
                &g.registro,
            );
            rechazos += usize::from(veredicto.is_err());
            microllamadas +=
                usize::from(es_rechazo_cifra(&veredicto) && !pend_real_orig.is_empty());
            crudo += usize::from(veredicto == Err(ViolacionArnes::LlamadaEnCrudo));
            exigen += usize::from(afirma);

            let vacio = registro_con(&[], g.fecha);
            let cifras = pend(&modelo, &g.pregunta, &forzada(), &vacio);
            let abstencion =
                !pend(&modelo, &g.pregunta, &IntencionTurno::DatoConcreto, &vacio).is_empty();
            let comprueba_hoy = match &intencion {
                IntencionTurno::Ninguna => false,
                IntencionTurno::DatoConcreto => abstencion,
                _ => true,
            };
            let pend_f_orig = pend(&modelo, &g.pregunta, &forzada(), &g.registro);
            let forzado_validar = etiqueta(&PostInferenceGuard::validar(
                &modelo,
                &g.pregunta,
                &forzada(),
                &g.registro,
            ));

            let nombres: Vec<String> = if g.medible {
                g.herramientas
                    .iter()
                    .map(|h| h["nombre"].as_str().unwrap_or("?").to_string())
                    .collect()
            } else {
                g.registro
                    .resultados()
                    .iter()
                    .map(|r| r.tool_name.clone())
                    .collect()
            };

            let mut fila = Fila {
                fichero: nombre_de_fichero(&g.origen),
                id: g.id.clone(),
                medible: g.medible,
                intencion: etiqueta_intencion(&intencion),
                herramientas: nombres,
                cifras: cifras.clone(),
                veredicto: etiqueta(&veredicto),
                cifra_veredicto: cifra_del_veredicto(&veredicto),
                abstencion,
                comprueba_hoy,
                pend_real_orig: pend_real_orig.clone(),
                pend_f_orig: pend_f_orig.clone(),
                forzado_validar,
                afirma_sin_consulta: afirma,
                solo_clave_orig: Vec::new(),
                a: None,
                b: None,
                atenea: false,
                cand_sql: Vec::new(),
                cand_sql_solo_limit: Vec::new(),
                cand_sql_sin_otro_respaldo_b: Vec::new(),
                perdidas: Vec::new(),
            };

            if g.medible {
                // Inventario de claves por herramienta
                for h in &g.herramientas {
                    let nombre = h["nombre"].as_str().unwrap_or("?").to_string();
                    herramientas_medibles += 1;
                    if h.get("argumentos").is_none() {
                        sin_argumentos += 1;
                    }
                    if h["es_error"].as_bool().unwrap_or(false) {
                        continue;
                    }
                    let mut hojas = Vec::new();
                    let mut nodos = BTreeSet::new();
                    recorrer_hojas(&h["resultado"], "", &mut hojas, &mut nodos);
                    *inventario_resultados.entry(nombre.clone()).or_default() += 1;
                    let por_ruta = inventario.entry(nombre).or_default();
                    for nodo in nodos {
                        *por_ruta.entry(nodo).or_default() += 1;
                    }
                }

                let validas = salidas_validas(&g.herramientas);
                fila.solo_clave_orig =
                    cifras_solo_clave(&validas, &cifras, &modelo, &g.pregunta, g.fecha);

                let mut variantes: Vec<(bool, ResVariante)> = Vec::new();
                for solo_a in [true, false] {
                    let filtradas = if solo_a {
                        herramientas_filtradas(
                            &g.herramientas,
                            true,
                            &claves_b_set,
                            &mut est_a,
                            &mut quitadas_b,
                        )
                    } else {
                        herramientas_filtradas(
                            &g.herramientas,
                            false,
                            &claves_b_set,
                            &mut est_b,
                            &mut quitadas_b,
                        )
                    };
                    let reg = registro_con(&filtradas, g.fecha);
                    let ver = PostInferenceGuard::validar(&modelo, &g.pregunta, &intencion, &reg);
                    let pr = pend(&modelo, &g.pregunta, &intencion, &reg);
                    let pf = pend(&modelo, &g.pregunta, &forzada(), &reg);
                    let validas_f = salidas_validas(&filtradas);
                    let solo_clave =
                        cifras_solo_clave(&validas_f, &cifras, &modelo, &g.pregunta, g.fecha);
                    let nuevo = veredicto.is_ok() && es_rechazo_cifra(&ver);
                    let otro = etiqueta(&ver) != etiqueta(&veredicto) && !nuevo;
                    variantes.push((
                        solo_a,
                        ResVariante {
                            veredicto: etiqueta(&ver),
                            nuevo_rechazo_real: nuevo,
                            otro_cambio_real: otro,
                            perdidas_real: menos(&pr, &pend_real_orig),
                            perdidas_f: menos(&pf, &pend_f_orig),
                            pend_real: pr,
                            pend_f: pf,
                            solo_clave,
                        },
                    ));
                }
                for (solo_a, v) in variantes {
                    if solo_a {
                        fila.a = Some(v);
                    } else {
                        fila.b = Some(v);
                    }
                }

                // Ubicación del respaldo de cada cifra perdida (hojas con dígitos de la original)
                let mut perdidas_todas: Vec<(&str, &str, String)> = Vec::new();
                for (nombre, v) in [("A", &fila.a), ("B", &fila.b)] {
                    if let Some(v) = v {
                        for c in &v.perdidas_real {
                            perdidas_todas.push((nombre, "real", c.clone()));
                        }
                        for c in &v.perdidas_f {
                            perdidas_todas.push((nombre, "forzado", c.clone()));
                        }
                    }
                }
                if !perdidas_todas.is_empty() {
                    let mut hojas_por_herramienta: Vec<(String, Vec<Hoja>)> = Vec::new();
                    for h in &g.herramientas {
                        if h["es_error"].as_bool().unwrap_or(false) {
                            continue;
                        }
                        let mut hojas = Vec::new();
                        let mut nodos = BTreeSet::new();
                        recorrer_hojas(&h["resultado"], "", &mut hojas, &mut nodos);
                        hojas.retain(|x| x.texto.chars().any(char::is_numeric));
                        hojas_por_herramienta
                            .push((h["nombre"].as_str().unwrap_or("?").to_string(), hojas));
                    }
                    let mut cache: HashMap<String, HashSet<String>> = HashMap::new();
                    let mut respaldo_de = |texto: &str| -> HashSet<String> {
                        cache
                            .entry(texto.to_string())
                            .or_insert_with(|| {
                                let reg =
                                    registro_de_valor(Value::String(texto.to_string()), g.fecha);
                                pend(&modelo, &g.pregunta, &forzada(), &reg)
                                    .into_iter()
                                    .collect()
                            })
                            .clone()
                    };
                    for (variante, modo, cifra) in perdidas_todas {
                        let mut respaldos: BTreeSet<(String, String, bool)> = BTreeSet::new();
                        for (herramienta, hojas) in &hojas_por_herramienta {
                            for hoja in hojas {
                                if !respaldo_de(&hoja.texto).contains(&cifra) {
                                    respaldos.insert((
                                        herramienta.clone(),
                                        hoja.ruta.clone(),
                                        hoja.clave,
                                    ));
                                }
                            }
                        }
                        let total = respaldos.len();
                        fila.perdidas.push(json!({
                            "variante": variante,
                            "modo": modo,
                            "cifra": cifra,
                            "n_respaldos": total,
                            "sin_ancla_literal": total == 0,
                            "respaldos": respaldos
                                .into_iter()
                                .take(MAXIMO_RESPALDOS_POR_CIFRA)
                                .map(|(h, r, k)| json!({ "herramienta": h, "ruta": r, "es_clave": k }))
                                .collect::<Vec<_>>(),
                        }));
                    }
                }

                // Variante C: literales numéricos de la SQL de execute_athena_sql
                let mut literales: Vec<(Vec<String>, bool)> = Vec::new();
                for h in &g.herramientas {
                    if h["nombre"].as_str() != Some("execute_athena_sql") {
                        continue;
                    }
                    fila.atenea = true;
                    let mut args = ValoresDeArgumentos::default();
                    recoger_argumentos(&argumentos_de(h), &mut args);
                    for sql in &args.textos {
                        for (lit, en_limit) in literales_numericos_sql(sql) {
                            literales.push((lecturas_num(&lit), en_limit));
                        }
                    }
                }
                if fila.atenea {
                    let f_b: HashSet<String> = fila
                        .b
                        .as_ref()
                        .map(|v| v.pend_f.iter().cloned().collect())
                        .unwrap_or_default();
                    for cifra in &cifras {
                        let lecturas = lecturas_num(cifra);
                        let coincide: Vec<bool> = literales
                            .iter()
                            .filter(|(l, _)| l.iter().any(|x| lecturas.contains(x)))
                            .map(|(_, lim)| *lim)
                            .collect();
                        if coincide.is_empty() {
                            continue;
                        }
                        fila.cand_sql.push(cifra.clone());
                        if coincide.iter().all(|x| *x) {
                            fila.cand_sql_solo_limit.push(cifra.clone());
                        }
                        if f_b.contains(cifra) {
                            fila.cand_sql_sin_otro_respaldo_b.push(cifra.clone());
                        }
                    }
                }
            }
            filas.push(fila);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Agregados
    // -----------------------------------------------------------------------------------------
    let medibles: Vec<&Fila> = filas.iter().filter(|f| f.medible).collect();
    let no_medibles: Vec<&Fila> = filas.iter().filter(|f| !f.medible).collect();

    let resumen_variante = |nombre: &str, filas: &[&Fila]| -> Value {
        fn seleccion<'a>(f: &'a Fila, nombre: &str) -> Option<&'a ResVariante> {
            if nombre == "A" {
                f.a.as_ref()
            } else {
                f.b.as_ref()
            }
        }
        let mut ok_hoy = 0;
        let mut a_rechazo = 0;
        let mut otro_cambio = 0;
        let mut perdidas_real = 0;
        let mut resp_perdida_real = 0;
        let mut f_base = 0;
        let mut f_a_rechazo = 0;
        let mut f_a_rechazo_ok_hoy = 0;
        let mut perdidas_f = 0;
        let mut resp_perdida_f = 0;
        let mut solo_clave = 0;
        let mut resp_solo_clave = 0;
        for f in filas {
            let Some(v) = seleccion(f, nombre) else {
                continue;
            };
            ok_hoy += usize::from(f.veredicto == "Ok");
            a_rechazo += usize::from(v.nuevo_rechazo_real);
            otro_cambio += usize::from(v.otro_cambio_real);
            perdidas_real += v.perdidas_real.len();
            resp_perdida_real += usize::from(!v.perdidas_real.is_empty());
            if !f.cifras.is_empty() && f.pend_f_orig.is_empty() {
                f_base += 1;
                if !v.pend_f.is_empty() {
                    f_a_rechazo += 1;
                    f_a_rechazo_ok_hoy += usize::from(f.veredicto == "Ok");
                }
            }
            perdidas_f += v.perdidas_f.len();
            resp_perdida_f += usize::from(!v.perdidas_f.is_empty());
            solo_clave += v.solo_clave.len();
            resp_solo_clave += usize::from(!v.solo_clave.is_empty());
        }
        json!({
            "real": {
                "respuestas_ok_hoy": ok_hoy,
                "pasan_a_rechazo_por_cifra": a_rechazo,
                "otros_cambios_de_veredicto": otro_cambio,
                "cifras_que_pierden_anclaje": perdidas_real,
                "respuestas_con_cifra_perdida": resp_perdida_real,
            },
            "forzado": {
                "base_con_cifras_y_sin_pendientes_hoy": f_base,
                "pasan_a_rechazo": f_a_rechazo,
                "de_ellas_hoy_ok": f_a_rechazo_ok_hoy,
                "cifras_que_pierden_anclaje": perdidas_f,
                "respuestas_con_cifra_perdida": resp_perdida_f,
            },
            "cifras_ancladas_solo_por_clave_tras_el_filtro": solo_clave,
            "respuestas_con_cifra_solo_por_clave_tras_el_filtro": resp_solo_clave,
        })
    };

    // Perdidas sin ancla literal (rescate de Nivel 1)
    let mut perdidas_sin_ancla = 0;
    let mut perdidas_total = 0;
    for f in &filas {
        for p in &f.perdidas {
            perdidas_total += 1;
            perdidas_sin_ancla += usize::from(p["sin_ancla_literal"] == json!(true));
        }
    }

    // C
    let c_resp_atenea = medibles.iter().filter(|f| f.atenea).count();
    let c_resp_atenea_con_cifras = medibles
        .iter()
        .filter(|f| f.atenea && !f.cifras.is_empty())
        .count();
    let c_resp_cand = medibles.iter().filter(|f| !f.cand_sql.is_empty()).count();
    let c_cand: usize = medibles.iter().map(|f| f.cand_sql.len()).sum();
    let c_cand_limit: usize = medibles.iter().map(|f| f.cand_sql_solo_limit.len()).sum();
    let c_cand_sin_otro: usize = medibles
        .iter()
        .map(|f| f.cand_sql_sin_otro_respaldo_b.len())
        .sum();
    let c_resp_cand_sin_otro = medibles
        .iter()
        .filter(|f| !f.cand_sql_sin_otro_respaldo_b.is_empty())
        .count();

    // 3.b
    let tabla_f3 = |lista: &[&Fila]| -> Value {
        let mut por: BTreeMap<String, Vec<&Fila>> = BTreeMap::new();
        for f in lista {
            let clave = if f.intencion == "DatoConcreto" {
                if f.abstencion {
                    "DatoConcreto (con abstención)".to_string()
                } else {
                    "DatoConcreto (sin abstención)".to_string()
                }
            } else {
                f.intencion.to_string()
            };
            por.entry(clave).or_default().push(f);
        }
        let mut salida = Map::new();
        for (clave, fs) in por {
            let con_cifras: Vec<&&Fila> = fs.iter().filter(|f| !f.cifras.is_empty()).collect();
            let nuevos: Vec<&&&Fila> = con_cifras
                .iter()
                .filter(|f| !f.comprueba_hoy && !f.pend_f_orig.is_empty())
                .collect();
            let nuevos_b: usize = con_cifras
                .iter()
                .filter(|f| !f.comprueba_hoy && f.b.as_ref().is_some_and(|b| !b.pend_f.is_empty()))
                .count();
            let nuevos_solo_b: usize = con_cifras
                .iter()
                .filter(|f| {
                    !f.comprueba_hoy
                        && f.pend_f_orig.is_empty()
                        && f.b.as_ref().is_some_and(|b| !b.pend_f.is_empty())
                })
                .count();
            let cifras_orig: usize = nuevos.iter().map(|f| f.pend_f_orig.len()).sum();
            let cifras_b: usize = con_cifras
                .iter()
                .filter(|f| !f.comprueba_hoy)
                .map(|f| f.b.as_ref().map_or(0, |b| b.pend_f.len()))
                .sum();
            let cubiertos_por_sin_consulta =
                nuevos.iter().filter(|f| f.afirma_sin_consulta).count();
            let mut validar_forzado: BTreeMap<&str, usize> = BTreeMap::new();
            for f in &con_cifras {
                *validar_forzado.entry(f.forzado_validar).or_default() += 1;
            }
            salida.insert(
                clave,
                json!({
                    "respuestas": fs.len(),
                    "con_cifras": con_cifras.len(),
                    "con_cifras_que_hoy_no_comprueban_anclas":
                        con_cifras.iter().filter(|f| !f.comprueba_hoy).count(),
                    "rechazos_nuevos_con_anclas_originales": nuevos.len(),
                    "cifras_sin_anclar_en_esos_rechazos": cifras_orig,
                    "de_ellos_ya_cubiertos_por_afirma_sin_consulta": cubiertos_por_sin_consulta,
                    "rechazos_con_anclas_de_variante_b": nuevos_b,
                    "rechazos_solo_con_variante_b": nuevos_solo_b,
                    "cifras_sin_anclar_variante_b_en_no_comprobadas": cifras_b,
                    "validar_forzado_a_capa2_sobre_con_cifras": validar_forzado,
                }),
            );
        }
        Value::Object(salida)
    };

    // Reparto por fichero
    let mut ficheros: BTreeMap<String, Vec<&Fila>> = BTreeMap::new();
    for f in &filas {
        ficheros.entry(f.fichero.clone()).or_default().push(f);
    }
    let por_fichero: Value = Value::Object(
        ficheros
            .iter()
            .map(|(nombre, fs)| {
                let med: Vec<&&Fila> = fs.iter().filter(|f| f.medible).collect();
                let cuenta = |sel: &dyn Fn(&Fila) -> bool| fs.iter().filter(|f| sel(f)).count();
                (
                    nombre.clone(),
                    json!({
                        "respuestas": fs.len(),
                        "medibles": med.len(),
                        "con_cifras": cuenta(&|f| !f.cifras.is_empty()),
                        "a_rechazo_real_a": cuenta(&|f| f.a.as_ref().is_some_and(|v| v.nuevo_rechazo_real)),
                        "a_rechazo_real_b": cuenta(&|f| f.b.as_ref().is_some_and(|v| v.nuevo_rechazo_real)),
                        "a_rechazo_forzado_a": cuenta(&|f| !f.cifras.is_empty() && f.pend_f_orig.is_empty() && f.a.as_ref().is_some_and(|v| !v.pend_f.is_empty())),
                        "a_rechazo_forzado_b": cuenta(&|f| !f.cifras.is_empty() && f.pend_f_orig.is_empty() && f.b.as_ref().is_some_and(|v| !v.pend_f.is_empty())),
                        "f3_rechazos_nuevos": cuenta(&|f| f.medible && !f.cifras.is_empty() && !f.comprueba_hoy && !f.pend_f_orig.is_empty()),
                        "f3_no_comprueban_con_cifras": cuenta(&|f| f.medible && !f.cifras.is_empty() && !f.comprueba_hoy),
                    }),
                )
            })
            .collect(),
    );

    // Distribución global de intenciones
    let mut por_intencion: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for f in &filas {
        let e = por_intencion.entry(f.intencion).or_default();
        e.0 += 1;
        e.1 += usize::from(!f.cifras.is_empty());
    }

    let solo_clave_orig_cifras: usize = medibles.iter().map(|f| f.solo_clave_orig.len()).sum();
    let solo_clave_orig_resp = medibles
        .iter()
        .filter(|f| !f.solo_clave_orig.is_empty())
        .count();
    let cifras_medibles_total: usize = medibles.iter().map(|f| f.cifras.len()).sum();

    let inventario_json: Value = Value::Object(
        inventario
            .iter()
            .map(|(herramienta, rutas)| {
                let mut mapa = Map::new();
                for ((ruta, tipo), n) in rutas {
                    mapa.insert(format!("{ruta} [{tipo}]"), json!(n));
                }
                (
                    herramienta.clone(),
                    json!({
                        "resultados_vistos": inventario_resultados.get(herramienta),
                        "rutas": mapa,
                    }),
                )
            })
            .collect(),
    );

    let filas_json: Vec<Value> = filas
        .iter()
        .map(|f| {
            let variante = |v: &Option<ResVariante>| {
                v.as_ref().map(|v| {
                    json!({
                        "veredicto": v.veredicto,
                        "nuevo_rechazo_real": v.nuevo_rechazo_real,
                        "otro_cambio_real": v.otro_cambio_real,
                        "pendientes_real": v.pend_real,
                        "pendientes_forzado": v.pend_f,
                        "perdidas_real": v.perdidas_real,
                        "perdidas_forzado": v.perdidas_f,
                        "cifras_solo_por_clave": v.solo_clave,
                    })
                })
            };
            json!({
                "fichero": f.fichero,
                "id": f.id,
                "medible": f.medible,
                "intencion": f.intencion,
                "herramientas": f.herramientas,
                "n_herramientas": f.herramientas.len(),
                "cifras_de_la_respuesta": f.cifras,
                "veredicto": f.veredicto,
                "cifra_del_veredicto": f.cifra_veredicto,
                "abstencion": f.abstencion,
                "comprueba_anclas_hoy": f.comprueba_hoy,
                "pendientes_real_original": f.pend_real_orig,
                "pendientes_forzado_original": f.pend_f_orig,
                "validar_forzado_a_capa2": f.forzado_validar,
                "afirma_sin_consulta": f.afirma_sin_consulta,
                "cifras_ancladas_solo_por_clave_original": f.solo_clave_orig,
                "A": variante(&f.a),
                "B": variante(&f.b),
                "usa_execute_athena_sql": f.atenea,
                "candidatas_sql": f.cand_sql,
                "candidatas_sql_solo_tras_limit": f.cand_sql_solo_limit,
                "candidatas_sql_sin_otro_respaldo_tras_b": f.cand_sql_sin_otro_respaldo_b,
                "perdidas_detalle": f.perdidas,
            })
        })
        .collect();

    let resumen = json!({
        "sanidad": {
            "respuestas": filas.len(),
            "rechazos_del_guard": rechazos,
            "microllamadas_nivel_2": microllamadas,
            "llamadas_en_crudo": crudo,
            "exigen_consulta": exigen,
        },
        "alcance": {
            "medibles_formato_benchmark": medibles.len(),
            "no_medibles_formato_aguja": no_medibles.len(),
            "herramientas_en_medibles": herramientas_medibles,
            "herramientas_sin_campo_argumentos": sin_argumentos,
            "cifras_totales_en_medibles": cifras_medibles_total,
            "definicion_de_cifra_de_la_respuesta": "cifras_pendientes con intención forzada (TabularIncompleta) y registro vacío (solo fecha de hoy): cifras fuera de la pregunta, sin referencia estructural",
        },
        "intenciones_global": por_intencion
            .iter()
            .map(|(k, (n, c))| (k.to_string(), json!({ "respuestas": n, "con_cifras": c })))
            .collect::<Map<String, Value>>(),
        "claves_b_usadas": claves_b,
        "claves_b_quitadas_recuento": quitadas_b,
        "filtro_a_recuento_valores_quitados": {
            "variante_a": {
                "igual_cadena": est_a.igual_cadena, "igual_numero": est_a.igual_numero,
                "contiene_argumento": est_a.contiene, "contiene_y_mas_de_200_caracteres": est_a.contiene_largas,
            },
            "variante_b_parte_a": {
                "igual_cadena": est_b.igual_cadena, "igual_numero": est_b.igual_numero,
                "contiene_argumento": est_b.contiene, "contiene_y_mas_de_200_caracteres": est_b.contiene_largas,
            },
        },
        "f1": {
            "A": resumen_variante("A", &medibles),
            "B": resumen_variante("B", &medibles),
            "C_candidatas": {
                "respuestas_con_execute_athena_sql": c_resp_atenea,
                "de_ellas_con_cifras": c_resp_atenea_con_cifras,
                "respuestas_con_cifra_candidata": c_resp_cand,
                "cifras_candidatas": c_cand,
                "de_ellas_solo_coinciden_con_un_literal_tras_LIMIT": c_cand_limit,
                "cifras_candidatas_sin_otro_respaldo_tras_b": c_cand_sin_otro,
                "respuestas_con_candidata_sin_otro_respaldo_tras_b": c_resp_cand_sin_otro,
            },
            "cifras_perdidas_total_detalle": perdidas_total,
            "cifras_perdidas_sin_ancla_literal_rescate_nivel_1": perdidas_sin_ancla,
            "cifras_solo_por_clave_en_original": solo_clave_orig_cifras,
            "respuestas_con_cifra_solo_por_clave_en_original": solo_clave_orig_resp,
        },
        "f3": {
            "benchmark_medible": tabla_f3(&medibles),
            "aguja_registro_reconstruido_no_fiable": tabla_f3(&no_medibles),
        },
        "por_fichero": por_fichero,
        "inventario_claves_por_herramienta": inventario_json,
        "filas": filas_json,
    });
    std::fs::write(&salida, serde_json::to_string_pretty(&resumen).unwrap()).unwrap();
    println!(
        "{} respuestas ({} medibles); {rechazos} rechazos; {microllamadas} microllamadas; {crudo} llamadas en crudo; {exigen} exigen consulta",
        filas.len(),
        medibles.len()
    );
}

// =============================================================================================
// Parte 2: clasificación a ciegas de las cifras sin anclar de los rechazos nuevos de F-3
// (extractor propio y aproximado sobre el JSON crudo; no usa la API privada del guard)
// =============================================================================================

#[derive(Clone, Copy, Debug)]
struct Lectura {
    valor: f64,
    decimales: usize,
}

#[derive(Clone)]
struct Num {
    lectura: Lectura,
    token: String,
    origen: String,
}

fn iguales(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// Tokens numéricos: dígitos con separadores `.`/`,` seguidos de dígito
fn tokens_numericos(texto: &str) -> Vec<String> {
    let letras: Vec<char> = texto.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < letras.len() {
        if !letras[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let inicio = i;
        while i < letras.len() && letras[i].is_ascii_digit() {
            i += 1;
        }
        while i + 1 < letras.len()
            && (letras[i] == '.' || letras[i] == ',')
            && letras[i + 1].is_ascii_digit()
        {
            i += 1;
            while i < letras.len() && letras[i].is_ascii_digit() {
                i += 1;
            }
        }
        tokens.push(letras[inicio..i].iter().collect());
    }
    tokens
}

/// Lecturas posibles de un token (coma decimal y punto de millares, y al revés)
fn lecturas_de_token(t: &str) -> Vec<Lectura> {
    let mk = |ent: &str, dec: &str| -> Option<Lectura> {
        let s = if dec.is_empty() {
            ent.to_string()
        } else {
            format!("{ent}.{dec}")
        };
        s.parse::<f64>().ok().map(|valor| Lectura {
            valor,
            decimales: dec.len(),
        })
    };
    let posiciones: Vec<usize> = t
        .char_indices()
        .filter(|(_, c)| *c == '.' || *c == ',')
        .map(|(p, _)| p)
        .collect();
    let mut lecturas = Vec::new();
    match posiciones.len() {
        0 => lecturas.extend(mk(t, "")),
        1 => {
            let (a, b) = (&t[..posiciones[0]], &t[posiciones[0] + 1..]);
            lecturas.extend(mk(a, b));
            if b.len() == 3 && a.len() <= 3 {
                lecturas.extend(mk(&format!("{a}{b}"), ""));
            }
        }
        _ => {
            let separadores: HashSet<char> = t.chars().filter(|c| *c == '.' || *c == ',').collect();
            if separadores.len() == 1 {
                let partes: Vec<&str> = t.split(['.', ',']).collect();
                if partes[1..].iter().all(|p| p.len() == 3) && partes[0].len() <= 3 {
                    lecturas.extend(mk(&partes.concat(), ""));
                } else {
                    for p in &partes {
                        lecturas.extend(mk(p, ""));
                    }
                    for par in partes.windows(2) {
                        lecturas.extend(mk(par[0], par[1]));
                    }
                }
            } else {
                let ultimo = *posiciones.last().unwrap();
                let entero: String = t[..ultimo].chars().filter(|c| c.is_ascii_digit()).collect();
                lecturas.extend(mk(&entero, &t[ultimo + 1..]));
            }
        }
    }
    lecturas
}

fn numeros_de_valor(valor: &Value, origen: &str, salida: &mut Vec<Num>) {
    match valor {
        Value::String(texto) => {
            for token in tokens_numericos(texto) {
                for lectura in lecturas_de_token(&token) {
                    salida.push(Num {
                        lectura,
                        token: token.clone(),
                        origen: origen.to_string(),
                    });
                }
            }
        }
        Value::Number(n) => {
            let texto = n.to_string();
            if let Some(v) = n.as_f64() {
                let decimales = texto.split('.').nth(1).map_or(0, |d| d.len());
                salida.push(Num {
                    lectura: Lectura {
                        valor: v.abs(),
                        decimales,
                    },
                    token: texto,
                    origen: origen.to_string(),
                });
            }
        }
        Value::Array(xs) => xs.iter().for_each(|x| numeros_de_valor(x, origen, salida)),
        Value::Object(m) => m.values().for_each(|x| numeros_de_valor(x, origen, salida)),
        Value::Null | Value::Bool(_) => {}
    }
}

fn textos_de_valor<'a>(valor: &'a Value, salida: &mut Vec<&'a str>) {
    match valor {
        Value::String(t) => salida.push(t),
        Value::Array(xs) => xs.iter().for_each(|x| textos_de_valor(x, salida)),
        Value::Object(m) => m.values().for_each(|x| textos_de_valor(x, salida)),
        _ => {}
    }
}

fn longitudes_de_arrays(valor: &Value, ruta: &str, salida: &mut Vec<(usize, String)>) {
    match valor {
        Value::Array(xs) => {
            salida.push((
                xs.len(),
                if ruta.is_empty() {
                    "$".into()
                } else {
                    ruta.to_string()
                },
            ));
            let hija = format!("{ruta}[]");
            xs.iter()
                .for_each(|x| longitudes_de_arrays(x, &hija, salida));
        }
        Value::Object(m) => {
            for (k, v) in m {
                let hija = if ruta.is_empty() {
                    clave_segura(k)
                } else {
                    format!("{ruta}.{}", clave_segura(k))
                };
                longitudes_de_arrays(v, &hija, salida);
            }
        }
        _ => {}
    }
}

const MESES: &[(&str, u32)] = &[
    ("enero", 1),
    ("febrero", 2),
    ("marzo", 3),
    ("abril", 4),
    ("mayo", 5),
    ("junio", 6),
    ("julio", 7),
    ("agosto", 8),
    ("septiembre", 9),
    ("setiembre", 9),
    ("octubre", 10),
    ("noviembre", 11),
    ("diciembre", 12),
    ("january", 1),
    ("february", 2),
    ("march", 3),
    ("april", 4),
    ("may", 5),
    ("june", 6),
    ("july", 7),
    ("august", 8),
    ("september", 9),
    ("october", 10),
    ("november", 11),
    ("december", 12),
];

type Fecha = (i32, u32, u32);

fn fecha_valida(a: i32, m: u32, d: u32) -> Option<Fecha> {
    ((1..=12).contains(&m) && (1..=31).contains(&d) && (1000..=2999).contains(&a))
        .then_some((a, m, d))
}

/// Fechas de un texto: ISO, DD/MM/AAAA (con `/`, `-` o `.`) y escritas con nombre de mes
fn fechas_en_texto(texto: &str) -> Vec<Fecha> {
    let mut fechas = Vec::new();
    for palabra in texto.split(|c: char| !(c.is_alphanumeric() || c == '/' || c == '-' || c == '.'))
    {
        let palabra = palabra.trim_matches(['.', '-', '/']);
        let partes: Vec<&str> = palabra.split(['/', '-', '.']).collect();
        if partes.len() == 3
            && partes
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        {
            let n: Vec<i64> = partes.iter().filter_map(|p| p.parse().ok()).collect();
            if n.len() == 3 {
                if partes[0].len() == 4 {
                    fechas.extend(fecha_valida(n[0] as i32, n[1] as u32, n[2] as u32));
                } else if partes[2].len() == 4 {
                    fechas.extend(fecha_valida(n[2] as i32, n[1] as u32, n[0] as u32));
                }
            }
        }
    }
    let minusculas = texto.to_lowercase();
    let palabras: Vec<&str> = minusculas
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .collect();
    let numero = |i: isize| -> Option<u32> {
        (i >= 0)
            .then(|| palabras.get(i as usize))
            .flatten()
            .and_then(|p| p.parse::<u32>().ok())
    };
    let enlace = |i: isize| -> bool {
        i >= 0
            && palabras
                .get(i as usize)
                .is_some_and(|p| *p == "de" || *p == "of")
    };
    for (i, palabra) in palabras.iter().enumerate() {
        let Some((_, mes)) = MESES.iter().find(|(nombre, _)| nombre == palabra) else {
            continue;
        };
        let i = i as isize;
        let dia = if enlace(i - 1) {
            numero(i - 2)
        } else {
            numero(i - 1)
        };
        let anio = if enlace(i + 1) {
            numero(i + 2)
        } else {
            numero(i + 1)
        };
        if let (Some(d), Some(a)) = (dia, anio) {
            fechas.extend(fecha_valida(a as i32, *mes, d));
            continue;
        }
        // Formato inglés: «March 5, 2019»
        if let (Some(d), Some(a)) = (numero(i + 1), numero(i + 2)) {
            fechas.extend(fecha_valida(a as i32, *mes, d));
        }
    }
    fechas
}

/// (operación, a, b) si el resultado de una operación simple entre dos del `pool` da `t`
fn buscar_derivada(t: f64, decimales: usize, pool: &[f64]) -> Option<(&'static str, f64, f64)> {
    let tol = 0.5 * 10f64.powi(-(decimales as i32)) + 1e-9;
    fn en_rango(pool: &[f64], lo: f64, hi: f64) -> &[f64] {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        let inicio = pool.partition_point(|x| *x < lo - 1e-12);
        let fin = pool.partition_point(|x| *x <= hi + 1e-12);
        &pool[inicio..fin.max(inicio)]
    }
    fn aplicar(nombre: &str, a: f64, b: f64) -> Option<f64> {
        match nombre {
            "suma" => Some(a + b),
            "resta" => Some(a - b),
            "producto" => Some(a * b),
            "cociente" => (b != 0.0).then(|| a / b),
            _ => (b != 0.0).then(|| a / b * 100.0),
        }
    }
    for nombre in ["suma", "resta", "producto", "cociente", "porcentaje"] {
        for &a in pool {
            let candidatos: Vec<f64> = match nombre {
                "suma" => en_rango(pool, t - a - tol, t - a + tol).to_vec(),
                "resta" => en_rango(pool, a - t - tol, a - t + tol).to_vec(),
                "producto" => en_rango(pool, (t - tol) / a, (t + tol) / a).to_vec(),
                "cociente" if t - tol > 0.0 => {
                    en_rango(pool, a / (t + tol), a / (t - tol)).to_vec()
                }
                "porcentaje" if t - tol > 0.0 => {
                    en_rango(pool, a * 100.0 / (t + tol), a * 100.0 / (t - tol)).to_vec()
                }
                _ => pool.to_vec(),
            };
            for b in candidatos {
                if iguales(a, b) {
                    continue;
                }
                if let Some(r) = aplicar(nombre, a, b) {
                    if (r - t).abs() <= tol {
                        return Some((nombre, a, b));
                    }
                }
            }
        }
    }
    None
}

struct CifraClasificada {
    fila: String,
    intencion: &'static str,
    cifra: String,
    categoria: &'static str,
    detalle: Value,
}

fn forma_de_cifra(literal: &str, lecturas: &[Lectura], es_fecha: bool) -> &'static str {
    if es_fecha {
        return "fecha";
    }
    let solo_digitos = literal.chars().all(|c| c.is_ascii_digit());
    if solo_digitos && literal.len() == 4 {
        if let Ok(n) = literal.parse::<u32>() {
            if (1900..=2100).contains(&n) {
                return "anio_de_cuatro_digitos";
            }
        }
    }
    let enteras = lecturas
        .iter()
        .any(|l| l.decimales == 0 && l.valor.fract() == 0.0);
    let decimales = lecturas.iter().any(|l| l.decimales > 0);
    match (enteras, decimales) {
        (true, true) => "ambigua_miles_o_decimal",
        (true, false) => {
            if lecturas.iter().any(|l| l.valor < 20.0) {
                "entero_menor_que_20"
            } else {
                "entero_20_o_mayor"
            }
        }
        _ => "decimal",
    }
}

#[test]
#[ignore = "herramienta de medición: necesita crudas guardadas fuera del repo"]
fn clasificar_f3() {
    let rutas = std::env::var("CRUDAS_GUARDADAS").expect("CRUDAS_GUARDADAS");
    let salida = std::env::var("SALIDA_CLASIFICACION").expect("SALIDA_CLASIFICACION");

    let mut cifras: Vec<CifraClasificada> = Vec::new();
    let mut respuestas: Vec<(String, &'static str, String, Vec<usize>)> = Vec::new();
    let mut control: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut tamanos_pool: Vec<usize> = Vec::new();

    for ruta in rutas.split(';').filter(|r| !r.is_empty()) {
        let fichero = nombre_de_fichero(ruta);
        for (posicion, g) in leer(ruta).into_iter().enumerate() {
            let (modelo, _) = sin_bloque_de_verificacion(&g.respuesta);
            let intencion = IntentDetector::clasificar(&g.pregunta);
            let vacio = registro_con(&[], g.fecha);
            let con_cifras = !pend(&modelo, &g.pregunta, &forzada(), &vacio).is_empty();
            let abstencion =
                !pend(&modelo, &g.pregunta, &IntencionTurno::DatoConcreto, &vacio).is_empty();
            let comprueba_hoy = match &intencion {
                IntencionTurno::Ninguna => false,
                IntencionTurno::DatoConcreto => abstencion,
                _ => true,
            };
            let pendientes = pend(&modelo, &g.pregunta, &forzada(), &g.registro);
            if !(con_cifras && !comprueba_hoy && !pendientes.is_empty()) {
                continue;
            }
            let fila = format!("{fichero}#{posicion}");
            let etiqueta_i = etiqueta_intencion(&intencion);

            // Números y fechas de las salidas sin error, de las salidas con error y de la pregunta
            let mut validos: Vec<Num> = Vec::new();
            let mut errores: Vec<Num> = Vec::new();
            let mut textos_validos: Vec<&str> = Vec::new();
            let mut textos_error: Vec<&str> = Vec::new();
            let mut longitudes: Vec<(usize, String)> = Vec::new();
            for h in &g.herramientas {
                let nombre = h["nombre"].as_str().unwrap_or("?");
                if h["es_error"].as_bool().unwrap_or(false) {
                    numeros_de_valor(&h["resultado"], nombre, &mut errores);
                    textos_de_valor(&h["resultado"], &mut textos_error);
                } else {
                    numeros_de_valor(&h["resultado"], nombre, &mut validos);
                    textos_de_valor(&h["resultado"], &mut textos_validos);
                    longitudes_de_arrays(&h["resultado"], "", &mut longitudes);
                }
            }
            let mut de_pregunta: Vec<Num> = Vec::new();
            numeros_de_valor(
                &Value::String(g.pregunta.clone()),
                "pregunta",
                &mut de_pregunta,
            );
            let fechas_validas: HashSet<Fecha> = textos_validos
                .iter()
                .flat_map(|t| fechas_en_texto(t))
                .collect();
            let fechas_error: HashSet<Fecha> = textos_error
                .iter()
                .flat_map(|t| fechas_en_texto(t))
                .collect();
            // Pool de operandos: salidas válidas y pregunta, sin 0 ni 1, sin repetidos
            let mut pool: Vec<f64> = validos
                .iter()
                .chain(de_pregunta.iter())
                .map(|n| n.lectura.valor.abs())
                .filter(|v| *v > 1e-9 && !iguales(*v, 1.0))
                .collect();
            pool.sort_by(|a, b| a.partial_cmp(b).unwrap());
            pool.dedup_by(|a, b| iguales(*a, *b));
            let n_herramientas = g.herramientas.len();
            tamanos_pool.push(pool.len());

            let mut indices = Vec::new();
            for literal in &pendientes {
                let fechas_cifra = fechas_en_texto(literal);
                let es_fecha = !fechas_cifra.is_empty();
                let token = tokens_numericos(literal)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                let lecturas = lecturas_de_token(&token);
                let (categoria, detalle): (&'static str, Value) = 'clasif: {
                    // 1. formato
                    if !es_fecha {
                        for n in &validos {
                            if lecturas.iter().any(|l| iguales(l.valor, n.lectura.valor)) {
                                break 'clasif (
                                    "formato",
                                    json!({ "numero_salida": n.token, "herramienta": n.origen, "mismo_token_textual": n.token == token }),
                                );
                            }
                        }
                    }
                    // 2. fecha redactada
                    if es_fecha {
                        for f in &fechas_cifra {
                            if fechas_validas.contains(f) {
                                break 'clasif (
                                    "fecha_redactada",
                                    json!({ "fecha": format!("{}-{:02}-{:02}", f.0, f.1, f.2), "por": "salida" }),
                                );
                            }
                            if g.fecha.is_some_and(|h| {
                                use chrono::Datelike;
                                (h.year(), h.month(), h.day()) == *f
                            }) {
                                break 'clasif (
                                    "fecha_redactada",
                                    json!({ "fecha": format!("{}-{:02}-{:02}", f.0, f.1, f.2), "por": "fecha_de_la_ejecucion" }),
                                );
                            }
                        }
                    } else {
                        // 3. redondeo
                        for l in &lecturas {
                            for n in &validos {
                                if n.lectura.decimales > l.decimales {
                                    let escala = 10f64.powi(l.decimales as i32);
                                    let redondeado = (n.lectura.valor * escala).round() / escala;
                                    if iguales(redondeado, l.valor) {
                                        break 'clasif (
                                            "redondeo",
                                            json!({ "numero_salida": n.token, "herramienta": n.origen, "decimales_cifra": l.decimales }),
                                        );
                                    }
                                }
                            }
                        }
                        // 4. derivada
                        for l in &lecturas {
                            if let Some((op, a, b)) =
                                buscar_derivada(l.valor.abs(), l.decimales, &pool)
                            {
                                let poco_fiable =
                                    l.decimales == 0 && l.valor.fract() == 0.0 && l.valor < 20.0;
                                break 'clasif (
                                    "derivada",
                                    json!({ "operacion": op, "a": a, "b": b, "decimales_cifra": l.decimales, "poco_fiable_entero_menor_20": poco_fiable }),
                                );
                            }
                        }
                        // 5. conteo
                        for l in &lecturas {
                            if l.decimales == 0
                                && l.valor.fract() == 0.0
                                && (1.0..=50.0).contains(&l.valor)
                            {
                                let n = l.valor as usize;
                                if let Some((_, ruta)) =
                                    longitudes.iter().find(|(len, _)| *len == n)
                                {
                                    break 'clasif (
                                        "conteo",
                                        json!({ "elementos": n, "origen": ruta }),
                                    );
                                }
                                if n_herramientas == n {
                                    break 'clasif (
                                        "conteo",
                                        json!({ "elementos": n, "origen": "herramientas_del_turno" }),
                                    );
                                }
                            }
                        }
                    }
                    // 6. solo en salida de error
                    if es_fecha {
                        if fechas_cifra.iter().any(|f| fechas_error.contains(f)) {
                            break 'clasif (
                                "solo_en_error",
                                json!({ "herramienta": "con_es_error" }),
                            );
                        }
                    } else if let Some(n) = errores
                        .iter()
                        .find(|n| lecturas.iter().any(|l| iguales(l.valor, n.lectura.valor)))
                    {
                        break 'clasif ("solo_en_error", json!({ "herramienta": n.origen }));
                    }
                    // 7. sin respaldo
                    let en_pregunta = !es_fecha
                        && de_pregunta
                            .iter()
                            .any(|n| lecturas.iter().any(|l| iguales(l.valor, n.lectura.valor)));
                    (
                        "sin_respaldo",
                        json!({ "forma": forma_de_cifra(&token, &lecturas, es_fecha), "coincide_formato_con_pregunta": en_pregunta }),
                    )
                };
                // Control de azar de «derivada»: dos señuelos (no son cifras de la respuesta) con
                // los mismos decimales, buscados en el mismo pool
                if !es_fecha {
                    if let Some(l) = lecturas.first() {
                        let escala = 10f64.powi(l.decimales as i32);
                        let clave = match l.decimales {
                            0 => "0_decimales",
                            1 => "1_decimal",
                            2 => "2_decimales",
                            _ => "3_o_mas_decimales",
                        };
                        for senuelo in [l.valor.abs() * 1.7, l.valor.abs() * 0.6 + 3.3] {
                            let t = (senuelo * escala).round() / escala;
                            let e = control.entry(clave.to_string()).or_insert((0, 0));
                            e.0 += 1;
                            e.1 += usize::from(buscar_derivada(t, l.decimales, &pool).is_some());
                        }
                    }
                }
                indices.push(cifras.len());
                cifras.push(CifraClasificada {
                    fila: fila.clone(),
                    intencion: etiqueta_i,
                    cifra: literal.clone(),
                    categoria,
                    detalle,
                });
            }
            respuestas.push((fila, etiqueta_i, fichero.clone(), indices));
        }
    }

    // Condición de parada: la población debe ser la del informe anterior
    assert_eq!(respuestas.len(), 76, "población distinta de 76 respuestas");
    assert_eq!(cifras.len(), 295, "población distinta de 295 cifras");

    let categorias = [
        "formato",
        "fecha_redactada",
        "redondeo",
        "derivada",
        "conteo",
        "solo_en_error",
        "sin_respaldo",
    ];
    let cuenta = |sel: &dyn Fn(&CifraClasificada) -> bool| cifras.iter().filter(|c| sel(c)).count();
    let por_categoria: Map<String, Value> = categorias
        .iter()
        .map(|c| {
            let n = cuenta(&|x| x.categoria == *c);
            (
                c.to_string(),
                json!({ "cifras": n, "porcentaje": (n as f64 / 295.0 * 1000.0).round() / 10.0 }),
            )
        })
        .collect();

    // Tipo de respuesta
    let tipo_de = |indices: &[usize]| -> &'static str {
        let cats: Vec<&str> = indices.iter().map(|i| cifras[*i].categoria).collect();
        if cats.iter().all(|c| *c == "sin_respaldo") {
            "todas_sin_respaldo"
        } else if cats.iter().any(|c| *c == "sin_respaldo") {
            "con_alguna_sin_respaldo_y_otras_explicadas_o_en_error"
        } else if cats.iter().any(|c| *c == "solo_en_error") {
            "mixta_sin_sin_respaldo_con_alguna_solo_en_error"
        } else {
            "explicada_todas_en_categorias_1_a_5"
        }
    };
    let tipos = [
        "explicada_todas_en_categorias_1_a_5",
        "mixta_sin_sin_respaldo_con_alguna_solo_en_error",
        "con_alguna_sin_respaldo_y_otras_explicadas_o_en_error",
        "todas_sin_respaldo",
    ];
    let desglose =
        |clave: &dyn Fn(&(String, &'static str, String, Vec<usize>)) -> String| -> Value {
            let mut mapa: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
            for r in &respuestas {
                *mapa
                    .entry(clave(r))
                    .or_default()
                    .entry(tipo_de(&r.3))
                    .or_default() += 1;
            }
            let mut salida = Map::new();
            for (k, v) in mapa {
                let total: usize = v.values().sum();
                let mut o = Map::new();
                o.insert("respuestas".into(), json!(total));
                for t in tipos {
                    o.insert(t.into(), json!(v.get(t).copied().unwrap_or(0)));
                }
                salida.insert(k, Value::Object(o));
            }
            Value::Object(salida)
        };
    let mut tipos_totales = Map::new();
    for t in tipos {
        tipos_totales.insert(
            t.into(),
            json!(respuestas.iter().filter(|r| tipo_de(&r.3) == t).count()),
        );
    }

    // Cifras por categoría y por intención / fichero
    let mut cat_por: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    for c in &cifras {
        let fichero = c.fila.split('#').next().unwrap_or("").to_string();
        *cat_por
            .entry(format!("intencion:{}", c.intencion))
            .or_default()
            .entry(c.categoria)
            .or_default() += 1;
        *cat_por
            .entry(format!("fichero:{fichero}"))
            .or_default()
            .entry(c.categoria)
            .or_default() += 1;
    }

    // Subtipos
    let detalle_bool = |cat: &str, clave: &str, valor: bool| {
        cifras
            .iter()
            .filter(|c| c.categoria == cat && c.detalle[clave].as_bool() == Some(valor))
            .count()
    };
    let derivada_d0 = cifras
        .iter()
        .filter(|c| c.categoria == "derivada" && c.detalle["decimales_cifra"].as_u64() == Some(0))
        .count();
    let redondeo_d0 = cifras
        .iter()
        .filter(|c| c.categoria == "redondeo" && c.detalle["decimales_cifra"].as_u64() == Some(0))
        .count();
    let mut formas: BTreeMap<String, usize> = BTreeMap::new();
    let mut repetidas: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in cifras.iter().filter(|c| c.categoria == "sin_respaldo") {
        *formas
            .entry(c.detalle["forma"].as_str().unwrap_or("?").to_string())
            .or_default() += 1;
        repetidas
            .entry(c.cifra.clone())
            .or_default()
            .insert(c.fila.clone());
    }
    let sin_respaldo_total = cuenta(&|c| c.categoria == "sin_respaldo");
    let literales_en_varias: Vec<(&String, usize)> = repetidas
        .iter()
        .filter(|(_, filas)| filas.len() >= 2)
        .map(|(l, filas)| (l, filas.len()))
        .collect();
    let cifras_en_literales_repetidos: usize = cifras
        .iter()
        .filter(|c| c.categoria == "sin_respaldo" && repetidas[&c.cifra].len() >= 2)
        .count();

    // Ejemplos: hasta 8 por categoría, repartidos
    let mut ejemplos = Map::new();
    for cat in categorias {
        let candidatos: Vec<&CifraClasificada> =
            cifras.iter().filter(|c| c.categoria == cat).collect();
        let paso = (candidatos.len() as f64 / 8.0).max(1.0);
        let mut elegidos = Vec::new();
        let mut x = 0.0;
        while (x as usize) < candidatos.len() && elegidos.len() < 8 {
            let c = candidatos[x as usize];
            elegidos.push(json!({ "fila": c.fila, "intencion": c.intencion, "cifra": c.cifra, "detalle": c.detalle }));
            x += paso;
        }
        ejemplos.insert(cat.to_string(), Value::Array(elegidos));
    }

    let mediana_pool = {
        let mut v = tamanos_pool.clone();
        v.sort();
        v.get(v.len() / 2).copied()
    };
    let resumen = json!({
        "poblacion": { "respuestas": respuestas.len(), "cifras": cifras.len() },
        "por_categoria": por_categoria,
        "respuestas_por_tipo": tipos_totales,
        "respuestas_por_tipo_e_intencion": desglose(&|r| r.1.to_string()),
        "respuestas_por_tipo_y_fichero": desglose(&|r| r.2.clone()),
        "cifras_por_categoria_segun_intencion_y_fichero": cat_por,
        "subtipos": {
            "formato_mismo_token_textual": detalle_bool("formato", "mismo_token_textual", true),
            "formato_otro_formato": detalle_bool("formato", "mismo_token_textual", false),
            "redondeo_con_0_decimales": redondeo_d0,
            "derivada_poco_fiable_entero_menor_20": detalle_bool("derivada", "poco_fiable_entero_menor_20", true),
            "derivada_con_0_decimales": derivada_d0,
            "sin_respaldo_coincide_formato_con_pregunta": detalle_bool("sin_respaldo", "coincide_formato_con_pregunta", true),
        },
        "sin_respaldo": {
            "cifras": sin_respaldo_total,
            "por_forma": formas,
            "literales_distintos": repetidas.len(),
            "literales_en_2_o_mas_respuestas": literales_en_varias.len(),
            "cifras_cuyo_literal_esta_en_2_o_mas_respuestas": cifras_en_literales_repetidos,
            "literales_repetidos_y_n_respuestas": literales_en_varias.iter().map(|(l, n)| json!({ "cifra": l, "respuestas": n })).collect::<Vec<_>>(),
        },
        "control_de_azar_de_derivada": {
            "descripcion": "dos señuelos por cifra no fecha (1,7x y 0,6x+3,3, con los mismos decimales) buscados en el mismo pool de operandos",
            "por_decimales": control.iter().map(|(k, (n, h))| (k.clone(), json!({ "senuelos": n, "derivables": h, "porcentaje": (*h as f64 / *n as f64 * 1000.0).round() / 10.0 }))).collect::<Map<String, Value>>(),
            "derivadas_reales_por_decimales": {
                "0_decimales": derivada_d0,
                "1_decimal": cuenta(&|c| c.categoria == "derivada" && c.detalle["decimales_cifra"].as_u64() == Some(1)),
                "2_decimales": cuenta(&|c| c.categoria == "derivada" && c.detalle["decimales_cifra"].as_u64() == Some(2)),
                "3_o_mas_decimales": cuenta(&|c| c.categoria == "derivada" && c.detalle["decimales_cifra"].as_u64() >= Some(3)),
            },
            "tamano_del_pool_de_operandos": {
                "min": tamanos_pool.iter().min(),
                "mediana": mediana_pool,
                "max": tamanos_pool.iter().max(),
            },
        },
        "ejemplos": ejemplos,
        "filas": cifras.iter().map(|c| json!({
            "fila": c.fila, "intencion": c.intencion, "cifra": c.cifra, "categoria": c.categoria, "detalle": c.detalle,
        })).collect::<Vec<_>>(),
    });
    std::fs::write(&salida, serde_json::to_string_pretty(&resumen).unwrap()).unwrap();
    println!(
        "{} respuestas, {} cifras clasificadas",
        respuestas.len(),
        cifras.len()
    );
}
