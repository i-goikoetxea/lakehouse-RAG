//! Comprobación determinista de las trazas del Nivel 2 (SSD de generalidad §4.3). El modelo solo
//! propone: cada literal debe ser una subcadena exacta de su origen y el cálculo se rehace aquí.

use std::collections::HashSet;

use super::cifras::{CifraHallada, extraer_cifras, lecturas_de_fuente};
use super::evidencia_herramientas::{Escalar, escalares_de, evidencia_de};
use super::procedencia_determinista::{exige_lectura_literal, mismo_precedente};
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
            // Una traza cita subcadenas de este texto: solo lleva evidencia documental, sin ecos
            // de argumentos, metadatos ni nombres de clave
            let textos: Vec<String> = escalares_de(&evidencia_de(resultado))
                .into_iter()
                .map(|escalar| match escalar {
                    Escalar::Texto(texto) => quitar_marcas(&texto),
                    Escalar::Numero(numero) => numero,
                })
                .collect();
            SalidaTurno {
                id: format!("t{indice}"),
                herramienta: resultado.tool_name.clone(),
                texto: textos.join("\n"),
            }
        })
        .collect()
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
            existe(solicitud, subcadena)
                && contiene_los_digitos(&subcadena.texto, literal)
                && (!exige_lectura_literal(literal)
                    || mismo_precedente(&solicitud.borrador, &subcadena.texto, literal))
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
/// «21», pero «Art. 210» no. Si uno de los dos exige lectura literal (`7.3`, `4.25`), el número
/// debe ser el mismo token, con sus separadores: «Art. 7.3» no respalda «73» ni «7»
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
        let mut token = String::new();
        while fin < letras.len()
            && (letras[fin].is_ascii_digit()
                || (matches!(letras[fin], '.' | ',')
                    && letras.get(fin + 1).is_some_and(char::is_ascii_digit)))
        {
            if letras[fin].is_ascii_digit() {
                digitos.push(letras[fin]);
            }
            token.push(letras[fin]);
            fin += 1;
        }
        let literal = exige_lectura_literal(&token) || exige_lectura_literal(cifra);
        if (literal && token == cifra.trim_start_matches('-')) || (!literal && digitos == buscados)
        {
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

    fn referencia(salida: &str, borrador: &str, cifra: &str, subcadena: &str) -> bool {
        let mut s = solicitud(&[salida], "");
        s.borrador = borrador.into();
        valida(
            &s,
            cifra,
            TipoTraza::ReferenciaEstructural {
                subcadena: lit("t0", subcadena),
            },
        )
    }

    /// Un identificador jerárquico `N.M` es un token literal: se valida entero, con sus puntos y
    /// con la misma palabra o símbolo delante que en la respuesta, en cualquier idioma
    #[test]
    fn test_referencia_jerarquica_literal_en_varios_dominios_e_idiomas() {
        for (salida, borrador, cifra, subcadena) in [
            (
                "Artículo 7.3. El plazo de entrega se fija en el contrato.",
                "Se regula en el artículo 7.3.",
                "7.3",
                "Artículo 7.3",
            ),
            (
                "Note 12.4 Leases and other commitments",
                "See Note 12.4 of the annual report.",
                "12.4",
                "Note 12.4",
            ),
            (
                "§ 5.2 Haftung des Auftragnehmers",
                "Das regelt § 5.2.",
                "5.2",
                "§ 5.2",
            ),
            (
                "Статья 3.1. Сфера применения",
                "Это статья 3.1.",
                "3.1",
                "Статья 3.1",
            ),
            (
                "Cláusula 4.2 Prazo de pagamento",
                "Conforme a cláusula 4.2, o prazo é fixo.",
                "4.2",
                "Cláusula 4.2",
            ),
        ] {
            assert!(referencia(salida, borrador, cifra, subcadena), "{borrador}");
        }
    }

    #[test]
    fn test_referencia_jerarquica_no_se_lee_sin_sus_puntos_ni_con_otra_palabra() {
        for (salida, borrador, cifra, subcadena) in [
            // `7.3` no es `73`, ni al revés
            (
                "Artículo 73. Recursos",
                "Lo regula el artículo 7.3.",
                "7.3",
                "Artículo 73",
            ),
            (
                "Artículo 7.3. Plazos",
                "Lo regula el artículo 73.",
                "73",
                "Artículo 7.3",
            ),
            (
                "Cláusula 4.2 Prazo",
                "O prazo é de 42 dias.",
                "42",
                "Cláusula 4.2",
            ),
            // Un componente suelto no es el identificador
            (
                "Clause 4.12.1 Scope",
                "See clause 4.12.",
                "4.12",
                "Clause 4.12.1",
            ),
            // Misma cifra con otra palabra delante: es un decimal, no la referencia
            (
                "Artículo 7.3. Plazos",
                "La pieza pesa 7.3 kg.",
                "7.3",
                "Artículo 7.3",
            ),
            (
                "Note 12.4 Leases",
                "Revenue grew 12.4 percent.",
                "12.4",
                "Note 12.4",
            ),
        ] {
            assert!(
                !referencia(salida, borrador, cifra, subcadena),
                "{borrador}"
            );
        }
    }

    #[test]
    fn test_decimal_redondeado_sigue_rechazado_y_los_miles_siguen_enteros() {
        // c19: «4 m» con una fuente de «4.25 m»
        assert!(!referencia(
            "Main beam length: 4.25 m (section 2).",
            "The main beam is 4 m long.",
            "4",
            "4.25 m"
        ));
        // «1.250» se lee como miles: respalda el expediente 1250
        assert!(referencia(
            "Expediente 1.250 de la junta",
            "Consta en el expediente 1250.",
            "1250",
            "Expediente 1.250"
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
                output: serde_json::json!({ "grupos": [{ "resultados": [{ "texto": "⟦p. 4⟧ Art. 21" }] }] }),
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

    #[test]
    fn test_el_texto_del_nivel_2_no_lleva_ecos_metadatos_ni_claves() {
        let resultados = [ToolResult {
            call_id: "a".into(),
            tool_name: "search_normative_context".into(),
            output: serde_json::json!({
                "consulta": "plazo 437",
                "total": 437,
                "grupos": [{ "resultados": [{ "texto": "Plazo de 30 días", "score": 0.437, "tiene_capa_2": true }] }],
                "nota": "Nota 437",
            }),
            is_error: false,
        }];
        let salidas = salidas_del_turno(&resultados);
        assert_eq!(salidas[0].texto, "Plazo de 30 días");
        // Una traza que cita el eco o una clave ya no encuentra su literal
        let s = SolicitudProcedencia {
            pregunta: String::new(),
            salidas,
            borrador: String::new(),
            cifras: Vec::new(),
        };
        for subcadena in ["plazo 437", "tiene_capa_2", "437"] {
            assert!(!valida(
                &s,
                "437",
                TipoTraza::ReferenciaEstructural {
                    subcadena: lit("t0", subcadena)
                }
            ));
        }
    }

    #[test]
    fn test_salida_de_herramienta_desconocida_no_da_texto_al_nivel_2() {
        let resultados = [ToolResult {
            call_id: "a".into(),
            tool_name: "tool_inventada".into(),
            output: serde_json::json!({ "texto": "Art. 21" }),
            is_error: false,
        }];
        let salidas = salidas_del_turno(&resultados);
        assert_eq!(salidas.len(), 1);
        assert_eq!(salidas[0].texto, "");
    }
}
