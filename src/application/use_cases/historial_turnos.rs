//! Recorte del historial conversacional por turnos completos.
//!
//! Un **turno** va de un mensaje del operador (`User`) al siguiente: la pregunta, las peticiones
//! de herramienta, sus resultados (`Tool`) y la respuesta final. Es la unidad mínima que se
//! conserva o se descarta: partir un turno dejaría resultados sin su petición, o una respuesta
//! sin la pregunta que la originó.
//!
//! La regla solo mira la estructura (los roles), nunca el contenido de los mensajes.

use crate::application::dtos::agent::{AgentMessage, AgentRole};

/// Deja en `historial` los turnos completos más recientes que caben en `tope_mensajes`.
///
/// - Lo descartado es siempre un prefijo: caen los turnos más antiguos, enteros.
/// - El turno más reciente se conserva aunque él solo supere el tope.
/// - Los mensajes anteriores al primer `User` no pertenecen a ningún turno y se eliminan.
/// - Sin ningún `User` no hay turno que conservar: la lista queda vacía. Es la salida más
///   segura, porque cualquier resto sería un resultado o una respuesta sin su pregunta.
///
/// Es determinista e idempotente.
pub fn recortar_por_turnos(historial: &mut Vec<AgentMessage>, tope_mensajes: usize) {
    let inicio = inicio_de_lo_conservado(historial, tope_mensajes);
    historial.drain(..inicio);
}

/// Índice del primer mensaje que se conserva: el inicio del turno más antiguo desde el que el
/// resto cabe en el tope o, si ninguno cabe, el del turno más reciente
fn inicio_de_lo_conservado(historial: &[AgentMessage], tope_mensajes: usize) -> usize {
    let total = historial.len();
    let es_inicio_de_turno = |indice: &usize| historial[*indice].role == AgentRole::User;
    let Some(turno_mas_reciente) = (0..total).rev().find(es_inicio_de_turno) else {
        return total;
    };
    (0..turno_mas_reciente)
        .filter(es_inicio_de_turno)
        .find(|inicio| total - inicio <= tope_mensajes)
        .unwrap_or(turno_mas_reciente)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::*;
    use crate::application::dtos::tools::ToolCall;

    const TOPE: usize = 40;

    fn llamada(id: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: "herramienta".to_string(),
            arguments: json!({}),
        }
    }

    /// Turno bien formado de exactamente `mensajes` mensajes (2, o 4 en adelante): pregunta, una
    /// petición con sus resultados y respuesta final
    fn turno(etiqueta: &str, mensajes: usize) -> Vec<AgentMessage> {
        assert!(
            mensajes == 2 || mensajes >= 4,
            "tamaño de turno no construible"
        );
        let mut turno = vec![AgentMessage::user(format!("pregunta {etiqueta}"))];
        if mensajes > 2 {
            let ids: Vec<String> = (0..mensajes - 3)
                .map(|i| format!("{etiqueta}_{i}"))
                .collect();
            turno.push(AgentMessage::assistant_with_tools(
                ids.iter().map(|id| llamada(id)).collect(),
            ));
            turno.extend(
                ids.iter()
                    .map(|id| AgentMessage::tool_result(id, format!("resultado {id}"))),
            );
        }
        turno.push(AgentMessage::assistant(format!("respuesta {etiqueta}")));
        assert_eq!(turno.len(), mensajes);
        turno
    }

    fn recortado(mut historial: Vec<AgentMessage>, tope: usize) -> Vec<AgentMessage> {
        recortar_por_turnos(&mut historial, tope);
        historial
    }

    #[test]
    fn test_un_turno_en_la_frontera_del_tope_se_conserva_entero() {
        for mensajes in [39, 40, 41] {
            let original = turno("a", mensajes);
            assert_eq!(
                recortado(original.clone(), TOPE),
                original,
                "{mensajes} mensajes"
            );
        }
    }

    #[test]
    fn test_dos_turnos_en_la_frontera_del_tope() {
        for (reciente, cae_el_antiguo) in [(29, false), (30, false), (31, true)] {
            let antiguo = turno("a", 10);
            let nuevo = turno("b", reciente);
            let original = [antiguo, nuevo.clone()].concat();
            let esperado = if cae_el_antiguo {
                nuevo
            } else {
                original.clone()
            };
            assert_eq!(
                recortado(original, TOPE),
                esperado,
                "{} mensajes",
                10 + reciente
            );
        }
    }

    #[test]
    fn test_turno_largo_tras_uno_corto_se_conserva_entero() {
        let largo = turno("largo", 50);
        let resultado = recortado([turno("corto", 2), largo.clone()].concat(), TOPE);
        assert!(!resultado.is_empty());
        assert_eq!(resultado, largo);
    }

    #[test]
    fn test_turno_corto_tras_uno_largo_descarta_el_largo_entero() {
        let corto = turno("corto", 2);
        let resultado = recortado([turno("largo", 50), corto.clone()].concat(), TOPE);
        assert_eq!(resultado, corto);
    }

    #[test]
    fn test_varios_turnos_sobre_el_tope_caen_los_mas_antiguos_completos() {
        let turnos: Vec<Vec<AgentMessage>> = (0..6).map(|i| turno(&format!("t{i}"), 12)).collect();
        let resultado = recortado(turnos.concat(), TOPE);
        // Caben tres turnos de 12 (36); un cuarto llegaría a 48
        assert_eq!(resultado, turnos[3..].concat());
    }

    #[test]
    fn test_lista_vacia_queda_vacia() {
        assert!(recortado(Vec::new(), TOPE).is_empty());
    }

    #[test]
    fn test_sin_mensaje_del_operador_no_queda_nada() {
        let huerfanos = vec![
            AgentMessage::assistant_with_tools(vec![llamada("x")]),
            AgentMessage::tool_result("x", "resultado"),
            AgentMessage::assistant("respuesta sin pregunta"),
        ];
        assert!(recortado(huerfanos.clone(), TOPE).is_empty());
        assert!(recortado(huerfanos, 0).is_empty());
    }

    #[test]
    fn test_huerfanos_al_inicio_se_eliminan_aunque_quepan() {
        let completo = turno("a", 6);
        let original = [
            vec![
                AgentMessage::tool_result("perdido", "resultado sin petición"),
                AgentMessage::assistant("respuesta sin pregunta"),
            ],
            completo.clone(),
        ]
        .concat();
        assert_eq!(recortado(original, TOPE), completo);
    }

    #[test]
    fn test_tope_cero_conserva_el_turno_mas_reciente() {
        let reciente = turno("b", 5);
        assert_eq!(
            recortado([turno("a", 4), reciente.clone()].concat(), 0),
            reciente
        );
    }

    #[test]
    fn test_recortar_dos_veces_da_lo_mismo_que_una() {
        let original = [turno("a", 20), turno("b", 20), turno("c", 20)].concat();
        let una = recortado(original, TOPE);
        assert_eq!(recortado(una.clone(), TOPE), una);
    }

    /// Generador congruencial con semilla fija: listas reproducibles sin dependencias nuevas
    struct Generador(u64);

    impl Generador {
        fn siguiente(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }

        fn hasta(&mut self, limite: usize) -> usize {
            (self.siguiente() % (limite as u64 + 1)) as usize
        }

        /// Turno con un número aleatorio de pasos de herramientas y de llamadas por paso
        fn turno(&mut self, etiqueta: usize) -> Vec<AgentMessage> {
            let mut turno = vec![AgentMessage::user(format!("pregunta {etiqueta}"))];
            for paso in 0..self.hasta(4) {
                let ids: Vec<String> = (0..1 + self.hasta(11))
                    .map(|i| format!("t{etiqueta}_p{paso}_{i}"))
                    .collect();
                turno.push(AgentMessage::assistant_with_tools(
                    ids.iter().map(|id| llamada(id)).collect(),
                ));
                turno.extend(
                    ids.iter()
                        .map(|id| AgentMessage::tool_result(id, format!("resultado {id}"))),
                );
            }
            turno.push(AgentMessage::assistant(format!("respuesta {etiqueta}")));
            turno
        }

        /// Mensajes sin pregunta delante, como los que dejaría un corte a mitad de turno
        fn huerfanos(&mut self) -> Vec<AgentMessage> {
            (0..self.hasta(5))
                .map(|i| {
                    if self.hasta(1) == 0 {
                        AgentMessage::tool_result(format!("huerfano_{i}"), "resultado")
                    } else {
                        AgentMessage::assistant("respuesta sin pregunta")
                    }
                })
                .collect()
        }
    }

    fn turnos_de(historial: &[AgentMessage]) -> Vec<&[AgentMessage]> {
        let inicios: Vec<usize> = (0..historial.len())
            .filter(|&i| historial[i].role == AgentRole::User)
            .collect();
        inicios
            .iter()
            .enumerate()
            .map(|(n, &inicio)| {
                let fin = inicios.get(n + 1).copied().unwrap_or(historial.len());
                &historial[inicio..fin]
            })
            .collect()
    }

    /// Cada resultado tiene su petición antes, en el mismo turno, y cada petición sus resultados
    fn turno_emparejado(turno: &[AgentMessage]) -> bool {
        let mut pedidas: HashSet<&str> = HashSet::new();
        let mut resueltas: HashSet<&str> = HashSet::new();
        for mensaje in turno {
            if let Some(llamadas) = &mensaje.tool_calls {
                pedidas.extend(llamadas.iter().map(|l| l.id.as_str()));
            }
            if mensaje.role == AgentRole::Tool {
                let Some(id) = mensaje.tool_call_id.as_deref() else {
                    return false;
                };
                if !pedidas.contains(id) {
                    return false;
                }
                resueltas.insert(id);
            }
        }
        pedidas == resueltas
    }

    #[test]
    fn test_propiedades_del_recorte_sobre_listas_generadas() {
        let mut generador = Generador(0x00F2_2026);
        let mut recortes = 0;
        let mut turnos_sobre_el_tope = 0;
        for caso in 0..2_000 {
            let tope = generador.hasta(60);
            let mut original = generador.huerfanos();
            let n_turnos = generador.hasta(8);
            for etiqueta in 0..n_turnos {
                original.extend(generador.turno(etiqueta));
            }
            let resultado = recortado(original.clone(), tope);
            let contexto = format!("caso {caso}, tope {tope}, {} mensajes", original.len());

            // Lo eliminado es un prefijo
            assert!(original.ends_with(&resultado), "{contexto}");
            // Idempotente
            assert_eq!(recortado(resultado.clone(), tope), resultado, "{contexto}");

            if n_turnos == 0 {
                assert!(resultado.is_empty(), "{contexto}");
                continue;
            }
            assert_eq!(resultado[0].role, AgentRole::User, "{contexto}");
            assert_eq!(resultado.last(), original.last(), "{contexto}");

            let turnos = turnos_de(&resultado);
            assert!(turnos.iter().all(|t| turno_emparejado(t)), "{contexto}");
            // El más reciente siempre está y es el mismo que antes de recortar
            assert_eq!(turnos.last(), turnos_de(&original).last(), "{contexto}");
            // Solo se supera el tope cuando queda un único turno
            assert!(resultado.len() <= tope || turnos.len() == 1, "{contexto}");
            // No se descarta de más: el turno anterior al primero conservado no cabía
            let originales = turnos_de(&original);
            if turnos.len() < originales.len() {
                let anterior = originales[originales.len() - turnos.len() - 1];
                assert!(resultado.len() + anterior.len() > tope, "{contexto}");
                recortes += 1;
            }
            if turnos.len() == 1 && resultado.len() > tope {
                turnos_sobre_el_tope += 1;
            }
        }
        // El generador ejercita de verdad las dos ramas que importan
        assert!(recortes > 100, "{recortes} recortes");
        assert!(
            turnos_sobre_el_tope > 100,
            "{turnos_sobre_el_tope} turnos sobre el tope"
        );
    }
}
