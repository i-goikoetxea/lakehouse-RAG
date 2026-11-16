//! La micro-batería sintética de la Fase 5.1, comprobada sin modelo: cada traza oráculo de una
//! cifra legítima la acepta Rust y ninguna traza adversaria aprueba una cifra falsa. Así la
//! calibración con Haiku real mide al modelo, no a la batería.

use lakehouse_rag::application::dtos::procedencia::{
    SalidaTurno, SolicitudProcedencia, TipoTraza, Traza, TrazasPropuestas,
};
use lakehouse_rag::application::use_cases::arnes_turno::procedencia::{
    CifraPendiente, cifras_verificadas,
};
use serde::Deserialize;

const BATERIA: &str = include_str!("../benchmarks/calibracion_procedencia/bateria_sintetica.json");

#[derive(Deserialize)]
struct Bateria {
    casos: Vec<Caso>,
}

#[derive(Deserialize)]
struct Caso {
    id: String,
    idioma: String,
    pregunta: String,
    salidas: Vec<SalidaTurno>,
    borrador: String,
    cifras: Vec<CifraCaso>,
}

#[derive(Deserialize)]
struct CifraCaso {
    literal: String,
    legitima: bool,
    oraculo: Option<TipoTraza>,
    #[serde(default)]
    adversarias: Vec<TipoTraza>,
}

fn bateria() -> Bateria {
    serde_json::from_str(BATERIA).expect("batería con el esquema cerrado")
}

fn solicitud(caso: &Caso) -> SolicitudProcedencia {
    SolicitudProcedencia {
        pregunta: caso.pregunta.clone(),
        salidas: caso.salidas.clone(),
        borrador: caso.borrador.clone(),
        cifras: caso.cifras.iter().map(|c| c.literal.clone()).collect(),
    }
}

fn pendientes(caso: &Caso) -> Vec<CifraPendiente> {
    caso.cifras
        .iter()
        .map(|c| CifraPendiente {
            literal: c.literal.clone(),
            solo_estructural: false,
        })
        .collect()
}

fn aprobada(caso: &Caso, cifra: &CifraCaso, tipo: &TipoTraza) -> bool {
    let trazas = TrazasPropuestas {
        trazas: vec![Traza {
            cifra: cifra.literal.clone(),
            tipo: tipo.clone(),
        }],
    };
    cifras_verificadas(&solicitud(caso), &pendientes(caso), &trazas).contains(&cifra.literal)
}

#[test]
fn test_la_bateria_cubre_variedad_y_tamano() {
    let bateria = bateria();
    assert!((15..=20).contains(&bateria.casos.len()));
    let idiomas: std::collections::HashSet<_> = bateria.casos.iter().map(|c| &c.idioma).collect();
    assert!(idiomas.len() >= 5, "{idiomas:?}");
    let falsas = bateria
        .casos
        .iter()
        .flat_map(|c| &c.cifras)
        .filter(|c| !c.legitima)
        .count();
    assert!(falsas >= 8, "{falsas}");
    for caso in &bateria.casos {
        for cifra in &caso.cifras {
            assert!(
                caso.borrador.contains(&cifra.literal),
                "{}: {}",
                caso.id,
                cifra.literal
            );
            assert_eq!(
                cifra.legitima,
                cifra.oraculo.is_some(),
                "{}: {}",
                caso.id,
                cifra.literal
            );
        }
    }
}

#[test]
fn test_ninguna_traza_adversaria_aprueba_una_cifra_falsa() {
    for caso in &bateria().casos {
        for cifra in caso.cifras.iter().filter(|c| !c.legitima) {
            for tipo in &cifra.adversarias {
                assert!(
                    !aprobada(caso, cifra, tipo),
                    "{}: {} aprobada con {tipo:?}",
                    caso.id,
                    cifra.literal
                );
            }
        }
    }
}

/// Las trazas oráculo que Rust rechaza hoy: es el diagnóstico de lo legítimo que el Nivel 2 no
/// puede rescatar aunque el modelo proponga la traza correcta
fn oraculos_rechazados() -> Vec<String> {
    let bateria = bateria();
    let mut rechazados = Vec::new();
    for caso in &bateria.casos {
        for cifra in caso.cifras.iter().filter(|c| c.legitima) {
            if !aprobada(caso, cifra, cifra.oraculo.as_ref().unwrap()) {
                rechazados.push(format!("{}:{}", caso.id, cifra.literal));
            }
        }
    }
    rechazados
}

/// Límite declarado (Fase 5.2): un identificador jerárquico es un token literal indivisible
/// («Artículo 7.3» respalda «artículo 7.3», nunca «73» ni un componente suelto). La paráfrasis que
/// lo descompone («apartado 3 del artículo 7», c12) no se rescata sin listas de palabras: abriría
/// la puerta al redondeo de c19. Cualquier otro rechazo es un retroceso
#[test]
fn test_cada_oraculo_legitimo_lo_acepta_rust_salvo_el_limite_conocido() {
    assert_eq!(
        oraculos_rechazados(),
        vec!["c12:3".to_string(), "c12:7".to_string()]
    );
}
