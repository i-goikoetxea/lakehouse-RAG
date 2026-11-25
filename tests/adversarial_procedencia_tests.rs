//! Pieza 4, Nivel 1 del verificador de procedencia (SSD de generalidad §4 y ajustes del mentor):
//! datos sintéticos en varios idiomas y dominios, y la salvaguarda (c) por Monte Carlo.
//!
//! Los casos los escribió la misma sesión que implementa las reglas: «pasan» significa
//! «funciona en los casos conocidos» (CLAUDE.md §1).

use lakehouse_rag::application::dtos::tools::ToolResult;
use lakehouse_rag::application::use_cases::arnes_turno::{
    IntencionTurno, PostInferenceGuard, RegistroTurno, ViolacionArnes,
};
use serde_json::{Value, json};

fn salida(herramienta: &str, output: Value) -> ToolResult {
    ToolResult {
        call_id: format!("c_{herramienta}"),
        tool_name: herramienta.to_string(),
        output,
        is_error: false,
    }
}

fn busqueda(doc: &str, textos: &[&str]) -> ToolResult {
    let resultados: Vec<Value> = textos
        .iter()
        .map(|texto| json!({ "texto": texto, "cita": { "doc_id": doc } }))
        .collect();
    salida(
        "search_normative_context",
        json!({ "grupos": [{ "resultados": resultados }], "total": textos.len() }),
    )
}

fn registro(salidas: Vec<ToolResult>) -> RegistroTurno {
    let mut registro = RegistroTurno::default();
    for s in salidas {
        registro.registrar(s);
    }
    registro
}

fn dato(respuesta: &str, registro: &RegistroTurno) -> Result<(), ViolacionArnes> {
    PostInferenceGuard::validar(
        respuesta,
        "¿Cuál es el dato?",
        &IntencionTurno::DatoConcreto,
        registro,
    )
}

fn tabular(respuesta: &str, registro: &RegistroTurno) -> Result<(), ViolacionArnes> {
    PostInferenceGuard::validar(
        respuesta,
        "What is the figure?",
        &IntencionTurno::TabularIncompleta,
        registro,
    )
}

fn en_abstencion(cifra: &str) -> Result<(), ViolacionArnes> {
    Err(ViolacionArnes::CifraEnAbstencion {
        cifra: cifra.to_string(),
    })
}

const TABLA_FINANCIERA: &str =
    "| Concepto | 2025 | 2024 |\n| Ingresos | 1.482,6 | 1.317,9 |\n| Costes | 903,4 | 811,0 |";

// --- Abstención lateral: el cálculo principal sale de cifras del documento ---

#[test]
fn test_abstencion_lateral_con_resta_del_documento_se_acepta() {
    let r = registro(vec![busqueda("FIN-1", &[TABLA_FINANCIERA])]);
    for respuesta in [
        "Los ingresos pasan de 1.317,9 a 1.482,6: suben 164,7. No consta el desglose por región.",
        "Ingresos 2025: 1.482,6; costes 2025: 903,4; margen: 579,2. El margen por región no consta.",
        "Los ingresos crecen un 12,5 % (de 1.317,9 a 1.482,6). No se indica la causa.",
    ] {
        assert_eq!(dato(respuesta, &r), Ok(()), "{respuesta}");
    }
}

#[test]
fn test_abstencion_lateral_clinica_con_suma_de_dosis() {
    let r = registro(vec![busqueda(
        "MED-4",
        &["Posología: 250 mg por la mañana y 125 mg por la noche."],
    )]);
    assert_eq!(
        dato(
            "Son 250 mg y 125 mg: 375 mg al día. No consta la dosis pediátrica.",
            &r
        ),
        Ok(())
    );
}

#[test]
fn test_resta_y_porcentaje_con_decimales_en_ingles_por_sql() {
    let r = registro(vec![salida(
        "execute_athena_sql",
        json!({ "resultado": [{ "revenue_2025": 1482.6, "revenue_2024": 1317.9 }] }),
    )]);
    assert_eq!(
        tabular("Revenue rose from 1317.9 to 1482.6, up 164.7.", &r),
        Ok(())
    );
    assert_eq!(
        tabular("From 1317.9 to 1482.6, a 12.5 % increase.", &r),
        Ok(())
    );
    assert_eq!(
        tabular("From 1317.9 to 1482.6, a 12.8 % increase.", &r),
        Err(ViolacionArnes::CifraSinAnclar {
            cifra: "12.8".into()
        })
    );
}

// --- Abstención estricta: salvaguarda (b) ---

#[test]
fn test_abstencion_total_con_cifras_de_referencia_o_de_memoria_se_rechaza() {
    let r = registro(vec![
        salida(
            "lookup_double_entry_table",
            json!({ "estado": "sin_coincidencia", "entrada": { "valor_x": 80, "valor_y": 50 } }),
        ),
        salida(
            "search_normative_context",
            json!({ "grupos": [], "total": 0 }),
        ),
    ]);
    assert_eq!(
        dato("No consta. Suele estar entre 7,5 y 9 bar.", &r),
        en_abstencion("7,5")
    );
    // F-1a: 80 y 50 solo están en el eco de la entrada del cruce, que ya no ancla. Antes se
    // rechazaba por la derivada (30); ahora, por la primera cifra sin evidencia documental
    assert_eq!(
        dato("No consta; entre 80 y 50 hay 30.", &r),
        en_abstencion("80")
    );
}

#[test]
fn test_primera_oracion_que_declara_la_ausencia_bloquea_las_derivadas() {
    let r = registro(vec![busqueda("FIN-1", &[TABLA_FINANCIERA])]);
    assert_eq!(
        dato(
            "No consta el margen de 2025. Con 1.482,6 y 903,4, sería 579,2.",
            &r
        ),
        en_abstencion("579,2")
    );
}

#[test]
fn test_derivada_en_la_oracion_de_la_locucion_se_rechaza() {
    let r = registro(vec![busqueda("FIN-1", &[TABLA_FINANCIERA])]);
    assert_eq!(
        dato(
            "Los ingresos son 1.482,6 y los costes 903,4. El margen de 579,2 no consta en el informe.",
            &r
        ),
        en_abstencion("579,2")
    );
}

#[test]
fn test_operando_citado_solo_en_la_oracion_de_la_locucion_no_respalda() {
    let r = registro(vec![busqueda("FIN-1", &[TABLA_FINANCIERA])]);
    assert_eq!(
        dato(
            "Los ingresos son 1.482,6. No consta el margen, solo los costes de 903,4.\nDiferencia: 579,2.",
            &r
        ),
        en_abstencion("579,2")
    );
}

// --- Artículos y secciones en varios idiomas y escrituras ---

#[test]
fn test_articulos_y_secciones_en_varias_lenguas_en_abstencion() {
    let r = registro(vec![busqueda(
        "LEY-1",
        &[
            "Artículo 21. Plazos.",
            "Article 14 — Scope",
            "Статья 7. Сфера",
            "Άρθρο 12 Πεδίο",
            "§ 33 Geltung",
        ],
    )]);
    for respuesta in [
        "No consta un plazo distinto del del artículo 21.",
        "No consta otra regla que la del Article 14.",
        "No consta nada fuera de la статья 7.",
        "No consta; βλ. άρθρο 12.",
        "No consta; siehe § 33.",
    ] {
        assert_eq!(dato(respuesta, &r), Ok(()), "{respuesta}");
    }
    assert_eq!(
        dato("No consta; lo regula el artículo 22.", &r),
        en_abstencion("22")
    );
}

// --- Paso 1: falsos «no consta + valor nuevo» (diagnóstico con casos sintéticos propios) ---

#[test]
fn test_referencia_compuesta_a_un_apartado_numerado_del_encabezado() {
    for (fuente, respuesta) in [
        (
            "Artículo 21. Plazos\n1. El plazo general.\n3. Las solicitudes urgentes.",
            "El plazo urgente lo fija el artículo 21.3; no consta un plazo para recursos.",
        ),
        (
            "Section 4 Records\n2. The operator shall keep records.",
            "Lo exige la Section 4.2; no consta la duración.",
        ),
        (
            "Статья 7. Сфера\n2. Действует для всех.",
            "Lo dice la статья 7.2; no consta excepción.",
        ),
        (
            "Artículo 21. Plazos\n(3) Las solicitudes urgentes.",
            "Lo fija el art. 21.3; no consta otro plazo.",
        ),
    ] {
        let r = registro(vec![busqueda("LEY-2", &[fuente])]);
        assert_eq!(dato(respuesta, &r), Ok(()), "{respuesta}");
    }
}

#[test]
fn test_referencia_compuesta_sin_apartado_o_sin_encabezado_se_rechaza() {
    for (fuente, respuesta, cifra) in [
        // El artículo existe, pero no tiene apartado 5
        (
            "Artículo 21. Plazos\n1. El plazo general.\n3. Las solicitudes urgentes.",
            "Lo fija el artículo 21.5; no consta otro plazo.",
            "21.5",
        ),
        // «de 2» y una línea «5.» no son un encabezado: un valor de memoria no pasa
        (
            "El plazo es de 2 días.\n5. Otras normas.",
            "No consta la presión. Suele ser de 2.5 bar.",
            "2.5",
        ),
        (
            "El plazo es de 2 días.\n5. Otras normas.",
            "La presión suele ser de 2.5 bar; no consta en el documento.",
            "2.5",
        ),
    ] {
        let r = registro(vec![busqueda("LEY-2", &[fuente])]);
        assert_eq!(dato(respuesta, &r), en_abstencion(cifra), "{respuesta}");
    }
}

#[test]
fn test_fecha_de_la_fuente_en_cifras_ancla_la_misma_fecha_en_otro_formato() {
    for (fuente, respuesta) in [
        (
            "Revisión aprobada el 15/03/2023.",
            "La revisión es del 15 de marzo de 2023; no consta la siguiente.",
        ),
        (
            "Freigabe: 15.03.2023",
            "Se aprobó el 2023-03-15; no consta la siguiente revisión.",
        ),
        (
            "Effective 03/15/2023.",
            "Entra en vigor el 15 de marzo de 2023; no consta la siguiente.",
        ),
        (
            "Утверждено 2023/03/15.",
            "Se aprobó el 2023-03-15; no consta la siguiente.",
        ),
        (
            "Aprobado el 15 de marzo de 2023.",
            "Se aprobó en 03/2023; no consta la siguiente revisión.",
        ),
    ] {
        let r = registro(vec![busqueda("DOC-F", &[fuente])]);
        assert_eq!(dato(respuesta, &r), Ok(()), "{respuesta}");
    }
}

#[test]
fn test_fecha_distinta_de_la_de_la_fuente_se_sigue_rechazando() {
    let r = registro(vec![busqueda(
        "DOC-F",
        &["Revisión aprobada el 15/03/2023."],
    )]);
    assert_eq!(
        dato(
            "Se aprobó el 16 de marzo de 2023; no consta la siguiente.",
            &r
        ),
        en_abstencion("16 de marzo de 2023")
    );
    assert_eq!(
        dato("No consta. La siguiente será el 2024-03-15.", &r),
        en_abstencion("2024-03-15")
    );
}

#[test]
fn test_calculo_en_otra_clausula_de_la_oracion_de_la_abstencion_lateral() {
    let tabla = registro(vec![busqueda(
        "FIN-1",
        &["| Concepto | 2025 | 2024 |\n| Ingresos | 1.482,6 | 1.317,9 |"],
    )]);
    for respuesta in [
        "Los ingresos son 1.482,6 y 1.317,9. Suben 164,7 (de 1.317,9 a 1.482,6); no consta el desglose.",
        "Resumen de ingresos. Suben 164,7 (de 1.317,9 a 1.482,6), aunque no consta el desglose.",
        "Resumen. Revenue grew by 164,7 (from 1.317,9 to 1.482,6) — no consta el desglose por región.",
    ] {
        assert_eq!(dato(respuesta, &tabla), Ok(()), "{respuesta}");
    }
    let dosis = registro(vec![busqueda(
        "MED-4",
        &["Доза утром 250 мг, вечером 125 мг."],
    )]);
    assert_eq!(
        dato(
            "Pauta. Son 375 mg al día (250 mg y 125 mg); no consta la dosis pediátrica.",
            &dosis
        ),
        Ok(())
    );
}

#[test]
fn test_cifra_en_la_clausula_de_la_locucion_o_de_memoria_se_sigue_rechazando() {
    let tabla = registro(vec![busqueda(
        "FIN-1",
        &["| Concepto | 2025 | 2024 |\n| Ingresos | 1.482,6 | 1.317,9 |"],
    )]);
    for (respuesta, cifra) in [
        // La cifra comparte cláusula con la locución
        (
            "Resumen. Los ingresos son 1.482,6 y 1.317,9; la subida de 164,7 no consta.",
            "164,7",
        ),
        // Operandos solo en la cláusula de la locución
        (
            "Resumen. No consta el desglose de 1.317,9 y 1.482,6; suben 164,7.",
            "164,7",
        ),
        // Otra cláusula, pero de memoria: no deriva de lo citado
        (
            "Resumen. Los ingresos son 1.482,6; no consta la previsión (rondará 1.600).",
            "1.600",
        ),
    ] {
        assert_eq!(dato(respuesta, &tabla), en_abstencion(cifra), "{respuesta}");
    }
}

// --- Trampas: la cifra de memoria se sigue rechazando ---

#[test]
fn test_trampa_cercana_al_valor_del_documento_se_rechaza() {
    let r = registro(vec![salida(
        "lookup_double_entry_table",
        json!({ "estado": "resuelto", "valor_resultado_texto": "412", "rango_x": "20 < x <= 40" }),
    )]);
    assert_eq!(
        tabular("The table gives 412; the usual value is 400.", &r),
        Err(ViolacionArnes::CifraSinAnclar {
            cifra: "400".into()
        })
    );
}

// --- Los dos operandos van citados cerca de la cifra derivada ---

#[test]
fn test_diferencia_con_los_dos_operandos_citados_se_acepta() {
    let r = registro(vec![busqueda(
        "IND-7",
        &["Índice 2024: 85. Índice 2025: 120."],
    )]);
    assert_eq!(
        dato(
            "La diferencia entre 120 y 85 es 35. No consta la causa.",
            &r
        ),
        Ok(())
    );
}

#[test]
fn test_diferencia_con_un_solo_operando_citado_se_rechaza() {
    let r = registro(vec![busqueda(
        "IND-7",
        &["Índice 2024: 85. Índice 2025: 120."],
    )]);
    assert_eq!(
        dato("Sube 35 puntos respecto a 85. No consta la causa.", &r),
        en_abstencion("35")
    );
}

// --- Clausura de 3 y 4 operandos, todos citados ---

#[test]
fn test_suma_de_tres_y_resta_de_cuatro_citadas_en_abstencion_lateral() {
    let pesos = registro(vec![busqueda(
        "ALM-2",
        &["Pieza A 120 kg, pieza B 85 kg, pieza C 40 kg."],
    )]);
    assert_eq!(
        dato(
            "Las tres piezas suman 245 kg (120, 85 y 40). No consta el embalaje.",
            &pesos
        ),
        Ok(())
    );
    let cuenta = registro(vec![busqueda(
        "FIN-9",
        &["| Revenue | 1,482.6 |\n| COGS | 903.4 |\n| Opex | 211.0 |\n| Tax | 58.2 |"],
    )]);
    assert_eq!(
        dato(
            "El neto es 310,0 (1.482,6 menos 903,4, 211,0 y 58,2). No consta el desglose.",
            &cuenta
        ),
        Ok(())
    );
}

#[test]
fn test_suma_de_tres_con_un_operando_sin_citar_se_rechaza() {
    let pesos = registro(vec![busqueda(
        "ALM-2",
        &["Pieza A 120 kg, pieza B 85 kg, pieza C 40 kg."],
    )]);
    assert_eq!(
        dato(
            "Las piezas A y B (120 y 85) y la C suman 245 kg. No consta el embalaje.",
            &pesos
        ),
        en_abstencion("245")
    );
}

// --- Salvaguarda (c): Monte Carlo ---

/// xorshift64*: determinista y sin dependencias
struct Azar(u64);

impl Azar {
    fn siguiente(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn uniforme(&mut self) -> f64 {
        (self.siguiente() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn hasta(&mut self, n: usize) -> usize {
        (self.siguiente() % n as u64) as usize
    }
}

/// Cifra con magnitud log-uniforme entre 1 y 10 000 y 0, 1 o 2 decimales, en formato español
fn cifra_aleatoria(azar: &mut Azar) -> String {
    let valor = 10f64.powf(azar.uniforme() * 4.0);
    let decimales = azar.hasta(3);
    format!("{valor:.decimales$}").replace('.', ",")
}

const CIFRAS_EN_CONTEXTO: usize = 60;
const POR_FILA: usize = 5;
const FILAS_POR_CHUNK: usize = 4;
const ENSAYOS: usize = 5000;

/// 60 cifras en 3 chunks del mismo documento (tablas de 4 filas de 5 valores). La respuesta cita
/// `citadas` cifras de un mismo chunk y añade una cifra aleatoria que no está en el contexto, en
/// una abstención lateral.
/// Dónde va la cifra inventada respecto a la locución de la abstención lateral
#[derive(Clone, Copy)]
enum Redaccion {
    /// En otra oración: «…; el resultado es X. No consta el desglose.»
    OtraOracion,
    /// En otra cláusula de la oración de la locución: «… (resultado: X); no consta el desglose.»
    OtraClausula,
}

fn tasa_de_falsos_aceptados(
    semilla: u64,
    cerca_de_los_citados: bool,
    citadas: usize,
    redaccion: Redaccion,
) -> f64 {
    let numero_de_citadas = citadas;
    let mut azar = Azar(semilla);
    let (mut aceptados, mut evaluados) = (0, 0);
    for _ in 0..ENSAYOS {
        let cifras: Vec<String> = (0..CIFRAS_EN_CONTEXTO)
            .map(|_| cifra_aleatoria(&mut azar))
            .collect();
        let chunks: Vec<String> = cifras
            .chunks(POR_FILA * FILAS_POR_CHUNK)
            .map(|chunk| {
                chunk
                    .chunks(POR_FILA)
                    .enumerate()
                    .map(|(fila, valores)| {
                        format!(
                            "| Fila {} | {} |",
                            (b'A' + fila as u8) as char,
                            valores.join(" | ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        let textos: Vec<&str> = chunks.iter().map(String::as_str).collect();
        let r = registro(vec![busqueda("DOC-MC", &textos)]);
        let chunk = azar.hasta(textos.len());
        let base = chunk * POR_FILA * FILAS_POR_CHUNK;
        let citadas: Vec<&String> = (0..numero_de_citadas)
            .map(|_| &cifras[base + azar.hasta(POR_FILA * FILAS_POR_CHUNK)])
            .collect();
        let inyectada = if cerca_de_los_citados {
            let referencia: f64 = citadas[0].replace(',', ".").parse().unwrap();
            let decimales = azar.hasta(3);
            format!("{:.decimales$}", referencia * (0.5 + azar.uniforme())).replace('.', ",")
        } else {
            cifra_aleatoria(&mut azar)
        };
        // Una cifra que ya está en la salida se ancla literalmente: no es una inyección. Cuenta
        // también el campo `total` de la búsqueda (sin él, el 80 % de los «falsos aceptados»
        // eran un `3` anclado por `"total": 3`, no un rescate del Nivel 1)
        let valor = |texto: &str| texto.replace(',', ".").parse::<f64>().unwrap();
        let total = textos.len() as f64;
        if cifras
            .iter()
            .map(|c| valor(c))
            .chain([total])
            .any(|v| (v - valor(&inyectada)).abs() < 1e-9)
        {
            continue;
        }
        // Un tercio de las inyectadas se escribe como porcentaje, que admite otra operación
        let signo = if azar.hasta(3) == 0 { " %" } else { "" };
        let (ultima, primeras) = citadas.split_last().expect("al menos una citada");
        let primeras: Vec<&str> = primeras.iter().map(|c| c.as_str()).collect();
        let primeras = primeras.join(", ");
        let respuesta = match redaccion {
            Redaccion::OtraOracion => format!(
                "Los valores son {primeras} y {ultima}; el resultado es {inyectada}{signo}. No consta el desglose."
            ),
            Redaccion::OtraClausula => format!(
                "Resumen. Los valores son {primeras} y {ultima} (resultado: {inyectada}{signo}); no consta el desglose."
            ),
        };
        let aceptada = match dato(&respuesta, &r) {
            Ok(()) => true,
            Err(ViolacionArnes::CifraEnAbstencion { cifra }) => {
                assert_eq!(
                    cifra, inyectada,
                    "solo puede faltar la inyectada: {respuesta}"
                );
                false
            }
            Err(otra) => panic!("violación inesperada {otra:?}: {respuesta}"),
        };
        aceptados += usize::from(aceptada);
        evaluados += 1;
    }
    aceptados as f64 / evaluados.max(1) as f64
}

/// Mide todas las semillas y luego exige el umbral: un fallo no oculta las tasas restantes
fn exigir_tasas(variante: &str, semillas: &[u64], cerca_de_los_citados: bool) {
    let mut excedidas = Vec::new();
    for &semilla in semillas {
        for citadas in [3, 4] {
            for (redaccion, lugar) in [
                (Redaccion::OtraOracion, "otra oración"),
                (Redaccion::OtraClausula, "otra cláusula"),
            ] {
                let tasa =
                    tasa_de_falsos_aceptados(semilla, cerca_de_los_citados, citadas, redaccion);
                println!(
                    "Monte Carlo ({variante}, {citadas} citadas, {lugar}), semilla {semilla:#x}: {:.3} %",
                    tasa * 100.0
                );
                if tasa > 0.003 {
                    excedidas.push((semilla, citadas, lugar, tasa));
                }
            }
        }
    }
    assert!(
        excedidas.is_empty(),
        "tasas de falsos aceptados > 0,3 %: {excedidas:?}"
    );
}

#[test]
fn test_monte_carlo_falsos_aceptados_por_debajo_del_tres_por_mil() {
    exigir_tasas(
        "cifra aleatoria",
        &[
            0x5EED_0001,
            0x5EED_0002,
            0x5EED_0003,
            0x5EED_0021,
            0x5EED_0022,
        ],
        false,
    );
}

#[test]
fn test_monte_carlo_cifra_cercana_a_las_citadas_por_debajo_del_tres_por_mil() {
    exigir_tasas(
        "cifra cercana",
        &[0x5EED_0011, 0x5EED_0012, 0x5EED_0013, 0x5EED_0031],
        true,
    );
}

// --- Cotas: una entrada hostil no cuelga el Nivel 1 ni la detección de JSON embebido ---

const LIMITE_ENTRADA_ENORME: std::time::Duration = std::time::Duration::from_secs(5);

/// Ejecuta `validar` en otro hilo y falla si no termina a tiempo (un cuelgue no bloquea la suite)
fn termina_pronto(
    caso: &str,
    validar: impl FnOnce() -> Result<(), ViolacionArnes> + Send + 'static,
) -> Result<(), ViolacionArnes> {
    let (enviar, recibir) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = enviar.send(validar());
    });
    recibir
        .recv_timeout(LIMITE_ENTRADA_ENORME)
        .unwrap_or_else(|_| panic!("{caso}: no terminó en {LIMITE_ENTRADA_ENORME:?}"))
}

#[test]
fn test_cotas_mega_de_cifras_sin_anclar_en_abstencion_termina_pronto() {
    let resultado = termina_pronto("abstención", || {
        let respuesta = format!("No consta. {}", "relleno 7 ".repeat(100_000));
        let r = registro(vec![salida(
            "search_normative_context",
            json!({ "grupos": [{ "resultados": [{ "texto": "a1".repeat(500_000) }] }] }),
        )]);
        dato(&respuesta, &r)
    });
    assert_eq!(resultado, en_abstencion("7"));
}

#[test]
fn test_cotas_mega_de_cifras_citadas_con_miles_de_operandos_termina_pronto() {
    let resultado = termina_pronto("clausura", || {
        let fila: String = (0..2_500)
            .map(|i| format!("{},{} ", i + 10, i % 10))
            .collect();
        let r = registro(vec![busqueda("MEGA", &[&fila])]);
        let respuesta = format!("El total es {}", "10,0 11,1 99999 ".repeat(60_000));
        tabular(&respuesta, &r)
    });
    assert!(resultado.is_err(), "{resultado:?}");
}

#[test]
fn test_cotas_mega_de_claves_del_protocolo_sin_cerrar_termina_pronto() {
    let resultado = termina_pronto("JSON embebido", || {
        let bloque = format!("{}\"tool_calls\",", "1,".repeat(20_000));
        let respuesta = format!("Texto. {{\"tool_calls\":[{}", bloque.repeat(64));
        dato(&respuesta, &RegistroTurno::default())
    });
    assert!(resultado.is_err(), "{resultado:?}");
}
