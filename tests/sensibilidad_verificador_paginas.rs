//! Sensibilidad del verificador de páginas (Pieza 3c) ante páginas mutadas, con fixtures
//! sintéticos en 4 escrituras. Cada frase del documento se cita con su página correcta y con
//! páginas ajenas: otra página recuperada en el turno para el mismo documento, o una página que
//! el turno no recuperó. Se cuenta cuántas mutaciones reciben nota (`Discrepante`), por separado
//! para extractos cortos y largos.
//!
//! Los fixtures y el umbral los escribió la misma sesión que implementa la regla (CLAUDE.md §1):
//! mide la sensibilidad en casos conocidos, no la general.

use lakehouse_rag::application::dtos::tools::ToolResult;
use lakehouse_rag::application::use_cases::arnes_turno::{
    RegistroTurno, ResultadoCita, VerificadorPaginas,
};
use serde_json::{Value, json};

/// Corte entre extracto corto y largo que se informa (el de la regla, en caracteres)
const CORTE_CORTO: usize = 40;
const PAGINAS_POR_DOCUMENTO: u32 = 8;
/// Páginas que recupera el turno (en chunks de dos páginas); las demás no se recuperan
const PAGINAS_RECUPERADAS: u32 = 6;
const FRASES_POR_PAGINA: usize = 4;

struct Azar(u64);

impl Azar {
    fn siguiente(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn hasta(&mut self, n: usize) -> usize {
        (self.siguiente() % n as u64) as usize
    }
}

/// Sílabas de cada escritura: palabras inventadas, sin texto de documentos reales
const SILABAS: &[(&str, &[&str])] = &[
    (
        "ES",
        &[
            "ma", "ri", "lo", "ten", "ca", "sur", "de", "va", "pli", "nor",
        ],
    ),
    (
        "EN",
        &[
            "th", "ock", "ern", "wil", "pre", "ang", "sto", "bry", "ma", "le",
        ],
    ),
    (
        "RU",
        &[
            "ка", "ро", "ми", "сто", "пре", "ла", "ну", "вет", "до", "жи",
        ],
    ),
    (
        "EL",
        &["κα", "λο", "μη", "στα", "πρε", "νι", "θο", "ρυ", "δε", "ξα"],
    ),
];

fn palabra(azar: &mut Azar, silabas: &[&str]) -> String {
    (0..2 + azar.hasta(3))
        .map(|_| silabas[azar.hasta(silabas.len())])
        .collect()
}

/// Frase de al menos `minimo` caracteres y como mucho `maximo`, con un número para hacerla única
fn frase(azar: &mut Azar, silabas: &[&str], minimo: usize, maximo: usize, serie: usize) -> String {
    loop {
        let mut texto = format!("{} {serie}", palabra(azar, silabas));
        while texto.chars().count() < minimo {
            texto = format!("{texto} {}", palabra(azar, silabas));
        }
        if texto.chars().count() <= maximo {
            return texto;
        }
    }
}

struct Documento {
    doc_id: String,
    /// Frases de cada página (índice 0 = p. 1)
    paginas: Vec<Vec<String>>,
}

fn documentos(azar: &mut Azar) -> Vec<Documento> {
    let mut serie = 0;
    SILABAS
        .iter()
        .map(|(idioma, silabas)| Documento {
            doc_id: format!("SIN-{idioma}"),
            paginas: (0..PAGINAS_POR_DOCUMENTO)
                .map(|_| {
                    (0..FRASES_POR_PAGINA)
                        .map(|i| {
                            serie += 1;
                            // La mitad cortas (16-38 caracteres) y la mitad largas (45-110)
                            if i % 2 == 0 {
                                frase(azar, silabas, 16, 38, serie)
                            } else {
                                frase(azar, silabas, 45, 110, serie)
                            }
                        })
                        .collect()
                })
                .collect(),
        })
        .collect()
}

/// Búsqueda con los chunks de las páginas recuperadas, de dos en dos, con marcas y `doc_id`
fn registro(documentos: &[Documento]) -> RegistroTurno {
    let resultados: Vec<Value> = documentos
        .iter()
        .flat_map(|documento| {
            (1..=PAGINAS_RECUPERADAS).step_by(2).map(move |primera| {
                let texto: String = (primera..primera + 2)
                    .map(|p| {
                        format!(
                            "⟦p. {p}⟧ {}.",
                            documento.paginas[p as usize - 1].join(". ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                json!({
                    "texto": texto,
                    "cita": { "doc_id": documento.doc_id, "pagina_inicio": primera, "pagina_fin": primera + 1 },
                })
            })
        })
        .collect();
    let mut registro = RegistroTurno::default();
    registro.registrar(ToolResult {
        call_id: "c".into(),
        tool_name: "search_normative_context".into(),
        output: json!({ "grupos": [{ "resultados": resultados }], "total": 1 }),
        is_error: false,
    });
    registro
}

#[derive(Default, Debug)]
struct Recuento {
    notadas: usize,
    total: usize,
}

impl Recuento {
    fn sumar(&mut self, notada: bool) {
        self.notadas += usize::from(notada);
        self.total += 1;
    }

    fn tasa(&self) -> f64 {
        self.notadas as f64 / self.total.max(1) as f64
    }
}

fn notada(respuesta: &str, registro: &RegistroTurno) -> bool {
    VerificadorPaginas::revisar(respuesta, registro)
        .iter()
        .any(|cita| cita.motivo().is_some())
}

#[test]
fn test_sensibilidad_ante_paginas_mutadas_por_longitud_del_extracto() {
    let mut azar = Azar(0x9A61_0001);
    let documentos = documentos(&mut azar);
    let registro = registro(&documentos);
    // [corto, largo] × [ajena recuperada, ajena no recuperada]
    let mut mutadas: [[Recuento; 2]; 2] = Default::default();
    let mut correctas_notadas = 0;
    let mut correctas = 0;
    for documento in &documentos {
        for pagina in 1..=PAGINAS_RECUPERADAS {
            for extracto in &documento.paginas[pagina as usize - 1] {
                let largo = usize::from(extracto.chars().count() >= CORTE_CORTO);
                let correcta = format!("Dice «{extracto}» (p. {pagina}).");
                correctas += 1;
                correctas_notadas += usize::from(notada(&correcta, &registro));
                let ajena_recuperada = pagina % PAGINAS_RECUPERADAS + 1;
                for (columna, ajena) in [ajena_recuperada, PAGINAS_POR_DOCUMENTO, 999]
                    .into_iter()
                    .enumerate()
                {
                    let respuesta = format!("Dice «{extracto}» (p. {ajena}).");
                    mutadas[largo][usize::from(columna > 0)].sumar(notada(&respuesta, &registro));
                }
            }
        }
    }
    for (fila, nombre) in [(0, "cortos (< 40)"), (1, "largos (≥ 40)")] {
        for (columna, destino) in [(0, "otra página recuperada"), (1, "página no recuperada")] {
            let recuento = &mutadas[fila][columna];
            println!(
                "Sensibilidad, extractos {nombre}, {destino}: {}/{} ({:.1} %)",
                recuento.notadas,
                recuento.total,
                recuento.tasa() * 100.0
            );
        }
    }
    println!("Citas correctas con nota: {correctas_notadas}/{correctas}");
    assert_eq!(correctas_notadas, 0, "una cita correcta no lleva nota");
    // Lo que la regla no puede tocar: los largos y las páginas que el turno no recuperó
    assert_eq!(mutadas[1][0].notadas, mutadas[1][0].total);
    assert_eq!(mutadas[1][1].notadas, mutadas[1][1].total);
    assert_eq!(mutadas[0][1].notadas, mutadas[0][1].total);
}

#[test]
fn test_el_resultado_de_un_corto_en_pagina_recuperada_es_ambiguo_o_discrepante() {
    // Guarda de forma: la mutación da `Discrepante` (antes) o `Ambigua` (después), nunca otro
    let mut azar = Azar(0x9A61_0002);
    let documentos = documentos(&mut azar);
    let registro = registro(&documentos);
    let extracto = &documentos[0].paginas[0][0];
    let revisadas = VerificadorPaginas::revisar(&format!("«{extracto}» (p. 3)"), &registro);
    assert_eq!(revisadas.len(), 1, "{revisadas:?}");
    assert!(
        matches!(
            revisadas[0].resultado,
            ResultadoCita::Discrepante { .. } | ResultadoCita::Ambigua { .. }
        ),
        "{:?}",
        revisadas[0].resultado
    );
}
