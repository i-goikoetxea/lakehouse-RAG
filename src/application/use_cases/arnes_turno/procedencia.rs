//! Comprobación determinista de las trazas del Nivel 2 (SSD de generalidad §4.3). El modelo solo
//! propone: cada literal debe ser una subcadena exacta de su origen y el cálculo se rehace aquí.

use std::collections::HashSet;

use serde_json::Value;

use super::cifras::{CifraHallada, extraer_cifras, lecturas_de_fuente};
use super::texto_normalizado::plegar_texto;
use crate::application::dtos::procedencia::{
    Literal, Operacion, SalidaTurno, SolicitudProcedencia, TipoTraza, TrazasPropuestas,
};
use crate::application::dtos::tools::ToolResult;
use crate::domain::services::marcas_pagina::quitar_marcas;

pub const ORIGEN_PREGUNTA: &str = "pregunta";
/// Tope de cifras sin anclar que los Niveles 1 y 2 examinan por respuesta. Las demás quedan
/// pendientes sin examinar y, con más de este número, no hay microllamada: se rechaza como hoy.
/// Una respuesta con miles de cifras sin anclar no es un cálculo que rescatar.
pub const MAXIMO_CIFRAS_PENDIENTES: usize = 64;
const MAXIMO_OPERANDOS: usize = 6;
const FACTOR_MAXIMO: i8 = 9;
/// Lecturas que se combinan como mucho al rehacer un cálculo (`1.250` admite dos)
const MAXIMO_COMBINACIONES: usize = 64;

/// Una cifra que el Nivel 2 puede rescatar. En una abstención estricta solo vale una
/// `referencia_estructural`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CifraPendiente {
    pub literal: String,
    pub solo_estructural: bool,
}

/// Salidas del turno como texto plano, con el id `t{i}` que citan las trazas
pub fn salidas_del_turno<'a>(
    resultados: impl IntoIterator<Item = &'a ToolResult>,
) -> Vec<SalidaTurno> {
    resultados
        .into_iter()
        .filter(|r| !r.is_error)
        .enumerate()
        .map(|(indice, resultado)| {
            let mut textos = Vec::new();
            recorrer(&resultado.output, &mut textos);
            SalidaTurno {
                id: format!("t{indice}"),
                herramienta: resultado.tool_name.clone(),
                texto: textos.join("\n"),
            }
        })
        .collect()
}

fn recorrer(raiz: &Value, textos: &mut Vec<String>) {
    let mut pendientes = vec![raiz];
    while let Some(valor) = pendientes.pop() {
        match valor {
            Value::String(texto) => textos.push(quitar_marcas(texto)),
            Value::Number(numero) => textos.push(numero.to_string()),
            Value::Array(elementos) => pendientes.extend(elementos.iter().rev()),
            Value::Object(campos) => {
                for (clave, contenido) in campos {
                    textos.push(clave.clone());
                    pendientes.push(contenido);
                }
            }
            Value::Null | Value::Bool(_) => {}
        }
    }
}

/// Literales de las cifras pendientes cuya traza valida. Una cifra sin traza, con varias que no
/// validan o fuera de las pendientes, no pasa.
pub fn cifras_verificadas(
    solicitud: &SolicitudProcedencia,
    pendientes: &[CifraPendiente],
    trazas: &TrazasPropuestas,
) -> HashSet<String> {
    pendientes
        .iter()
        .filter(|pendiente| {
            trazas.trazas.iter().any(|traza| {
                traza.cifra == pendiente.literal
                    && (!pendiente.solo_estructural
                        || matches!(traza.tipo, TipoTraza::ReferenciaEstructural { .. }))
                    && traza_valida(solicitud, &pendiente.literal, &traza.tipo)
            })
        })
        .map(|pendiente| pendiente.literal.clone())
        .collect()
}

fn traza_valida(solicitud: &SolicitudProcedencia, literal: &str, tipo: &TipoTraza) -> bool {
    let Some(cifra) = extraer_cifras(literal).into_iter().next() else {
        return false;
    };
    match tipo {
        TipoTraza::FormatoOEscala {
            fuente,
            factor_potencia_diez,
        } => {
            factor_potencia_diez.unsigned_abs() <= FACTOR_MAXIMO as u8
                && existe(solicitud, fuente)
                && valores(&fuente.texto).iter().any(|valor| {
                    let escalado = valor * 10f64.powi(i32::from(*factor_potencia_diez));
                    valores_de_cifra(&cifra).iter().any(|(objetivo, _)| {
                        (escalado - objetivo).abs() <= objetivo.abs() * 1e-9 + 1e-12
                    })
                })
        }
        TipoTraza::Aritmetica {
            operacion,
            operandos,
        } => {
            (2..=MAXIMO_OPERANDOS).contains(&operandos.len())
                && (*operacion != Operacion::Porcentaje || operandos.len() == 2)
                && operandos.iter().all(|o| existe(solicitud, o))
                && resultados(*operacion, operandos).iter().any(|resultado| {
                    valores_de_cifra(&cifra)
                        .iter()
                        .any(|(objetivo, tolerancia)| (resultado - objetivo).abs() <= *tolerancia)
                })
        }
        TipoTraza::ReferenciaEstructural { subcadena } => {
            existe(solicitud, subcadena) && contiene_los_digitos(&subcadena.texto, literal)
        }
        TipoTraza::ConteoElementos { elementos } => {
            let esqueletos: HashSet<String> =
                elementos.iter().map(|e| esqueleto(&e.texto)).collect();
            !elementos.is_empty()
                && elementos
                    .iter()
                    .all(|e| existe(solicitud, e) && !esqueleto(&e.texto).is_empty())
                && esqueletos.len() == elementos.len()
                && valores_de_cifra(&cifra)
                    .iter()
                    .any(|(objetivo, _)| *objetivo == elementos.len() as f64)
        }
    }
}

/// El literal es una subcadena exacta del origen que cita, sin cortar un número ni una palabra:
/// `12` no está en `412`
fn existe(solicitud: &SolicitudProcedencia, literal: &Literal) -> bool {
    if literal.texto.trim().is_empty() {
        return false;
    }
    if literal.origen == ORIGEN_PREGUNTA {
        return contiene_entero(&solicitud.pregunta, &literal.texto);
    }
    solicitud
        .salidas
        .iter()
        .find(|salida| salida.id == literal.origen)
        .is_some_and(|salida| contiene_entero(&salida.texto, &literal.texto))
}

fn contiene_entero(texto: &str, buscado: &str) -> bool {
    let abre = buscado.chars().next().is_some_and(char::is_alphanumeric);
    let cierra = buscado
        .chars()
        .next_back()
        .is_some_and(char::is_alphanumeric);
    texto.match_indices(buscado).any(|(inicio, _)| {
        let antes = texto[..inicio].chars().next_back();
        let mut despues = texto[inicio + buscado.len()..].chars();
        let siguiente = despues.next();
        let tras_separador = despues.next();
        let corta_delante = abre && antes.is_some_and(char::is_alphanumeric);
        let corta_detras = cierra
            && (siguiente.is_some_and(char::is_alphanumeric)
                || (matches!(siguiente, Some('.' | ','))
                    && tras_separador.is_some_and(|l| l.is_ascii_digit())));
        !corta_delante && !corta_detras
    })
}

/// Valores de un literal de origen (`1.250` puede ser 1,25 o 1250)
fn valores(texto: &str) -> Vec<f64> {
    let mut valores: Vec<f64> = lecturas_de_fuente(texto)
        .iter()
        .filter_map(|l| l.parse().ok())
        .collect();
    valores.dedup();
    valores
}

/// Valor de cada lectura de la cifra de la respuesta, con la tolerancia de sus decimales escritos
fn valores_de_cifra(cifra: &CifraHallada) -> Vec<(f64, f64)> {
    cifra
        .lecturas
        .iter()
        .filter_map(|lectura| {
            let valor: f64 = lectura.parse().ok()?;
            let decimales = if lectura.contains('.') {
                cifra
                    .literal
                    .rsplit(['.', ','])
                    .next()
                    .map_or(0, |p| p.chars().filter(char::is_ascii_digit).count() as i32)
            } else {
                0
            };
            Some((valor, 0.5 * 10f64.powi(-decimales) + 1e-9))
        })
        .collect()
}

fn resultados(operacion: Operacion, operandos: &[Literal]) -> Vec<f64> {
    let mut combinaciones: Vec<Vec<f64>> = vec![Vec::new()];
    for operando in operandos {
        let opciones = valores(&operando.texto);
        if opciones.is_empty() {
            return Vec::new();
        }
        combinaciones = combinaciones
            .into_iter()
            .flat_map(|parcial| {
                opciones.iter().map(move |valor| {
                    let mut siguiente = parcial.clone();
                    siguiente.push(*valor);
                    siguiente
                })
            })
            .take(MAXIMO_COMBINACIONES)
            .collect();
    }
    combinaciones
        .iter()
        .filter_map(|valores| calcular(operacion, valores))
        .collect()
}

fn calcular(operacion: Operacion, valores: &[f64]) -> Option<f64> {
    let (primero, resto) = valores.split_first()?;
    let resultado = match operacion {
        Operacion::Suma => valores.iter().sum(),
        Operacion::Resta => resto.iter().fold(*primero, |a, b| a - b),
        Operacion::Producto => valores.iter().product(),
        Operacion::Division => {
            if resto.contains(&0.0) {
                return None;
            }
            resto.iter().fold(*primero, |a, b| a / b)
        }
        Operacion::Porcentaje => {
            let divisor = *resto.first()?;
            if divisor == 0.0 {
                return None;
            }
            primero / divisor * 100.0
        }
    };
    resultado.is_finite().then_some(resultado)
}

/// Algún número de la subcadena tiene exactamente los dígitos de la cifra: «Art. 21» respalda
/// «21», pero «Art. 210» no
fn contiene_los_digitos(subcadena: &str, cifra: &str) -> bool {
    let buscados: String = cifra.chars().filter(char::is_ascii_digit).collect();
    if buscados.is_empty() {
        return false;
    }
    let letras: Vec<char> = subcadena.chars().collect();
    let mut posicion = 0;
    while posicion < letras.len() {
        if !letras[posicion].is_ascii_digit() {
            posicion += 1;
            continue;
        }
        let mut fin = posicion;
        let mut digitos = String::new();
        while fin < letras.len()
            && (letras[fin].is_ascii_digit()
                || (matches!(letras[fin], '.' | ',')
                    && letras.get(fin + 1).is_some_and(char::is_ascii_digit)))
        {
            if letras[fin].is_ascii_digit() {
                digitos.push(letras[fin]);
            }
            fin += 1;
        }
        if digitos == buscados {
            return true;
        }
        posicion = fin;
    }
    false
}

fn esqueleto(texto: &str) -> String {
    plegar_texto(texto)
        .chars()
        .filter(|l| l.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dtos::procedencia::Traza;

    fn solicitud(salidas: &[&str], pregunta: &str) -> SolicitudProcedencia {
        SolicitudProcedencia {
            pregunta: pregunta.to_string(),
            salidas: salidas
                .iter()
                .enumerate()
                .map(|(i, texto)| SalidaTurno {
                    id: format!("t{i}"),
                    herramienta: "search_normative_context".into(),
                    texto: texto.to_string(),
                })
                .collect(),
            borrador: String::new(),
            cifras: Vec::new(),
        }
    }

    fn lit(origen: &str, texto: &str) -> Literal {
        Literal {
            origen: origen.into(),
            texto: texto.into(),
        }
    }

    fn valida(solicitud: &SolicitudProcedencia, cifra: &str, tipo: TipoTraza) -> bool {
        let trazas = TrazasPropuestas {
            trazas: vec![Traza {
                cifra: cifra.into(),
                tipo,
            }],
        };
        let pendientes = [CifraPendiente {
            literal: cifra.into(),
            solo_estructural: false,
        }];
        cifras_verificadas(solicitud, &pendientes, &trazas).contains(cifra)
    }

    #[test]
    fn test_formato_y_escala_validos() {
        let s = solicitud(&["CE 0.42 %; longitud 1500 mm"], "");
        assert!(valida(
            &s,
            "0,42",
            TipoTraza::FormatoOEscala {
                fuente: lit("t0", "0.42"),
                factor_potencia_diez: 0
            }
        ));
        assert!(valida(
            &s,
            "1.5",
            TipoTraza::FormatoOEscala {
                fuente: lit("t0", "1500"),
                factor_potencia_diez: -3
            }
        ));
    }

    #[test]
    fn test_resta_financiera_en_ingles_con_un_decimal() {
        let s = solicitud(&["| Revenue | 1,482.6 |", "| Revenue 2024 | 1,317.9 |"], "");
        let resta = |cifra: &str| {
            valida(
                &s,
                cifra,
                TipoTraza::Aritmetica {
                    operacion: Operacion::Resta,
                    operandos: vec![lit("t0", "1,482.6"), lit("t1", "1,317.9")],
                },
            )
        };
        assert!(resta("164.7"));
        assert!(!resta("164.9"), "fuera de la tolerancia");
    }

    #[test]
    fn test_referencias_estructurales_en_es_y_de() {
        let s = solicitud(&["Art. 21. Plazos", "§ 33 Geltungsbereich"], "");
        assert!(valida(
            &s,
            "21",
            TipoTraza::ReferenciaEstructural {
                subcadena: lit("t0", "Art. 21")
            }
        ));
        assert!(valida(
            &s,
            "33",
            TipoTraza::ReferenciaEstructural {
                subcadena: lit("t1", "§ 33")
            }
        ));
        assert!(!valida(
            &s,
            "2",
            TipoTraza::ReferenciaEstructural {
                subcadena: lit("t0", "Art. 21")
            }
        ));
        assert!(!valida(
            &s,
            "21",
            TipoTraza::ReferenciaEstructural {
                subcadena: lit("t0", "Plazos")
            }
        ));
    }

    #[test]
    fn test_conteo_de_tres_elementos_en_griego() {
        let s = solicitud(&["Απαιτούνται: πιστοποιητικό, άδεια, ασφάλιση."], "");
        let elementos = vec![
            lit("t0", "πιστοποιητικό"),
            lit("t0", "άδεια"),
            lit("t0", "ασφάλιση"),
        ];
        assert!(valida(
            &s,
            "3",
            TipoTraza::ConteoElementos {
                elementos: elementos.clone()
            }
        ));
        assert!(!valida(&s, "4", TipoTraza::ConteoElementos { elementos }));
        let duplicados = vec![
            lit("t0", "άδεια"),
            lit("t0", "άδεια"),
            lit("t0", "ασφάλιση"),
        ];
        assert!(!valida(
            &s,
            "3",
            TipoTraza::ConteoElementos {
                elementos: duplicados
            }
        ));
    }

    #[test]
    fn test_operando_inventado_u_origen_inexistente_no_valida() {
        let s = solicitud(&["Caudal 412"], "¿Cuánto?");
        for operandos in [
            vec![lit("t0", "412"), lit("t0", "12")],
            vec![lit("t7", "412"), lit("t0", "412")],
            vec![lit("pregunta", "412"), lit("t0", "412")],
        ] {
            assert!(!valida(
                &s,
                "400",
                TipoTraza::Aritmetica {
                    operacion: Operacion::Resta,
                    operandos
                }
            ));
        }
    }

    #[test]
    fn test_factor_fuera_de_rango_o_mas_de_seis_operandos_no_valida() {
        let s = solicitud(&["1 1 1 1 1 1 1 0.42"], "");
        assert!(!valida(
            &s,
            "0.42",
            TipoTraza::FormatoOEscala {
                fuente: lit("t0", "0.42"),
                factor_potencia_diez: 12
            }
        ));
        let siete = vec![lit("t0", "1"); 7];
        assert!(!valida(
            &s,
            "7",
            TipoTraza::Aritmetica {
                operacion: Operacion::Suma,
                operandos: siete
            }
        ));
    }

    #[test]
    fn test_traza_de_una_cifra_que_no_esta_pendiente_se_ignora() {
        let s = solicitud(&["Art. 21"], "");
        let trazas = TrazasPropuestas {
            trazas: vec![Traza {
                cifra: "21".into(),
                tipo: TipoTraza::ReferenciaEstructural {
                    subcadena: lit("t0", "Art. 21"),
                },
            }],
        };
        assert!(cifras_verificadas(&s, &[], &trazas).is_empty());
    }

    #[test]
    fn test_en_abstencion_estricta_solo_vale_la_referencia_estructural() {
        let s = solicitud(&["Art. 21; caudal 80 y 50"], "");
        let pendientes = [
            CifraPendiente {
                literal: "21".into(),
                solo_estructural: true,
            },
            CifraPendiente {
                literal: "30".into(),
                solo_estructural: true,
            },
        ];
        let trazas = TrazasPropuestas {
            trazas: vec![
                Traza {
                    cifra: "21".into(),
                    tipo: TipoTraza::ReferenciaEstructural {
                        subcadena: lit("t0", "Art. 21"),
                    },
                },
                Traza {
                    cifra: "30".into(),
                    tipo: TipoTraza::Aritmetica {
                        operacion: Operacion::Resta,
                        operandos: vec![lit("t0", "80"), lit("t0", "50")],
                    },
                },
            ],
        };
        assert_eq!(
            cifras_verificadas(&s, &pendientes, &trazas),
            HashSet::from(["21".to_string()])
        );
    }

    #[test]
    fn test_salidas_del_turno_quitan_marcas_y_omiten_errores() {
        let resultados = [
            ToolResult {
                call_id: "a".into(),
                tool_name: "search_normative_context".into(),
                output: serde_json::json!({ "texto": "⟦p. 4⟧ Art. 21" }),
                is_error: false,
            },
            ToolResult {
                call_id: "b".into(),
                tool_name: "x".into(),
                output: serde_json::json!({ "error": "777" }),
                is_error: true,
            },
        ];
        let salidas = salidas_del_turno(&resultados);
        assert_eq!(salidas.len(), 1);
        assert!(salidas[0].texto.contains("Art. 21") && !salidas[0].texto.contains("⟦"));
    }
}
