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
        .map(|texto| json!({ "doc_id": doc, "texto": texto }))
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
            json!({ "estado": "sin_coincidencia", "valor_x": 80, "valor_y": 50 }),
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
    assert_eq!(
        dato("No consta; entre 80 y 50 hay 30.", &r),
        en_abstencion("30")
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
/// 3 cifras de un mismo chunk y añade una cifra aleatoria que no está en el contexto, en una
/// abstención lateral.
fn tasa_de_falsos_aceptados(semilla: u64, cerca_de_los_citados: bool) -> f64 {
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
        let citadas: Vec<&String> = (0..3)
            .map(|_| &cifras[base + azar.hasta(POR_FILA * FILAS_POR_CHUNK)])
            .collect();
        let inyectada = if cerca_de_los_citados {
            let referencia: f64 = citadas[0].replace(',', ".").parse().unwrap();
            let decimales = azar.hasta(3);
            format!("{:.decimales$}", referencia * (0.5 + azar.uniforme())).replace('.', ",")
        } else {
            cifra_aleatoria(&mut azar)
        };
        // Una cifra que ya está en el contexto se ancla literalmente: no es una inyección
        let valor = |texto: &str| texto.replace(',', ".").parse::<f64>().unwrap();
        if cifras
            .iter()
            .any(|c| (valor(c) - valor(&inyectada)).abs() < 1e-9)
        {
            continue;
        }
        // Un tercio de las inyectadas se escribe como porcentaje, que admite otra operación
        let signo = if azar.hasta(3) == 0 { " %" } else { "" };
        let respuesta = format!(
            "Los valores son {}, {} y {}; el resultado es {inyectada}{signo}. No consta el desglose.",
            citadas[0], citadas[1], citadas[2]
        );
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

#[test]
fn test_monte_carlo_falsos_aceptados_por_debajo_del_uno_por_ciento() {
    for semilla in [0x5EED_0001, 0x5EED_0002, 0x5EED_0003] {
        let tasa = tasa_de_falsos_aceptados(semilla, false);
        println!(
            "Monte Carlo (cifra aleatoria), semilla {semilla:#x}: {:.3} %",
            tasa * 100.0
        );
        assert!(tasa <= 0.01, "tasa de falsos aceptados {tasa} > 1 %");
    }
}

#[test]
fn test_monte_carlo_cifra_cercana_a_las_citadas_por_debajo_del_uno_por_ciento() {
    for semilla in [0x5EED_0011, 0x5EED_0012, 0x5EED_0013] {
        let tasa = tasa_de_falsos_aceptados(semilla, true);
        println!(
            "Monte Carlo (cifra cercana), semilla {semilla:#x}: {:.3} %",
            tasa * 100.0
        );
        assert!(tasa <= 0.01, "tasa de falsos aceptados {tasa} > 1 %");
    }
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
            json!({ "texto": "a1".repeat(500_000) }),
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
