//! Acotado del historial conversacional por turnos completos: compactación y recorte.
//!
//! Un **turno** va de un mensaje del operador (`User`) al siguiente: la pregunta, las peticiones
//! de herramienta, sus resultados (`Tool`) y la respuesta final. Es la unidad mínima que se
//! conserva, se compacta o se descarta: partir un turno por otro sitio dejaría resultados sin
//! su petición, o una respuesta sin la pregunta que la originó.
//!
//! **Compactar** un turno es dejarlo en dos mensajes: su pregunta y su respuesta final de texto
//! (el último mensaje, si es del asistente, no pide herramientas y lleva texto). Se sueltan las
//! peticiones de herramienta, sus resultados y cualquier respuesta intermedia, siempre en
//! ciclos completos. Un turno que no termina en una respuesta de texto no se compacta. No se
//! añade ningún texto que marque lo retirado.
//!
//! Las reglas solo miran la estructura (los roles y si hay `tool_calls`), nunca el contenido de
//! los mensajes: no dependen del idioma, del dominio ni de la forma de la pregunta.
//!
//! # Contrato de orden al cerrar un turno
//!
//! 1. **Extracción de evidencias verificadas** (F-4, aún no existe): necesita los resultados de
//!    herramienta del turno, así que va antes de que nada los suelte.
//! 2. **Compactación** de los turnos antiguos, solo si el historial no cabe.
//! 3. **Recorte** de turnos enteros, solo si compactar no bastó.
//!
//! `acotar_historial_por_turnos` hace los pasos 2 y 3 y es la única llamada del servicio de
//! chat. Quien añada el paso 1 debe ejecutarlo antes de esa llamada.
//!
//! # Tope
//!
//! El tope es por número de mensajes. Un tope por tamaño es otra fase: la medida de lo que
//! ocupa el historial está en un solo sitio (`compactar_turnos_antiguos` e
//! `inicio_de_lo_conservado`), y el orden de las decisiones no depende de ella.

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

/// Acota `historial` a `tope_mensajes` al cerrar un turno: compacta los turnos antiguos que
/// haga falta y, solo si no basta, descarta los más antiguos. Es el único punto de entrada.
///
/// - Si el historial cabe, no cambia.
/// - Si no cabe, se compactan turnos de uno en uno, del más antiguo al más nuevo, y se para en
///   cuanto cabe. Compactar un turno es dejarlo en su pregunta y su respuesta final.
/// - El turno más reciente nunca se compacta.
/// - Si con todos los anteriores compactados sigue sin caber, `recortar_por_turnos` descarta
///   turnos enteros. Como él, elimina siempre los mensajes anteriores al primer `User`, que no
///   cuentan para decidir si el historial cabe.
///
/// Es determinista e idempotente.
pub fn acotar_historial_por_turnos(historial: &mut Vec<AgentMessage>, tope_mensajes: usize) {
    compactar_turnos_antiguos(historial, tope_mensajes);
    recortar_por_turnos(historial, tope_mensajes);
}

/// Compacta turnos anteriores al más reciente, del más antiguo al más nuevo, hasta que los
/// turnos caben en el tope o no queda ninguno por compactar
fn compactar_turnos_antiguos(historial: &mut Vec<AgentMessage>, tope_mensajes: usize) {
    let inicios: Vec<usize> = (0..historial.len())
        .filter(|&indice| historial[indice].role == AgentRole::User)
        .collect();
    let Some(&primer_turno) = inicios.first() else {
        return;
    };
    let mut conservar = vec![true; historial.len()];
    let mut mensajes_en_turnos = historial.len() - primer_turno;
    // Cada ventana es un turno seguido de otro: el más reciente no entra
    for limites in inicios.windows(2) {
        if mensajes_en_turnos <= tope_mensajes {
            break;
        }
        let (inicio, fin) = (limites[0], limites[1]);
        if termina_en_respuesta_de_texto(&historial[inicio..fin]) {
            let intermedios = inicio + 1..fin - 1;
            mensajes_en_turnos -= intermedios.len();
            conservar[intermedios].fill(false);
        }
    }
    let mut se_conserva = conservar.into_iter();
    historial.retain(|_| se_conserva.next().unwrap_or(true));
}

/// El último mensaje del turno es del asistente, no pide herramientas y lleva texto. Solo se
/// comprueba que el texto existe, nunca lo que dice
fn termina_en_respuesta_de_texto(turno: &[AgentMessage]) -> bool {
    turno.last().is_some_and(|ultimo| {
        ultimo.role == AgentRole::Assistant
            && ultimo.tool_calls.as_ref().is_none_or(Vec::is_empty)
            && !ultimo.content.trim().is_empty()
    })
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

    // ---- Compactación perezosa de turnos antiguos ----

    fn acotado(mut historial: Vec<AgentMessage>, tope: usize) -> Vec<AgentMessage> {
        acotar_historial_por_turnos(&mut historial, tope);
        historial
    }

    /// La pregunta y el último mensaje del turno
    fn compacto(turno: &[AgentMessage]) -> Vec<AgentMessage> {
        vec![turno[0].clone(), turno[turno.len() - 1].clone()]
    }

    /// Definición independiente de la del módulo: el turno acaba en texto del asistente
    fn acaba_en_texto(turno: &[AgentMessage]) -> bool {
        turno.last().is_some_and(|m| {
            m.role == AgentRole::Assistant
                && m.tool_calls.as_ref().is_none_or(|l| l.is_empty())
                && !m.content.trim().is_empty()
        })
    }

    /// Paso de herramientas: una petición con `llamadas` resultados
    fn paso(etiqueta: &str, llamadas: usize) -> Vec<AgentMessage> {
        let ids: Vec<String> = (0..llamadas).map(|i| format!("{etiqueta}_{i}")).collect();
        let mut paso = vec![AgentMessage::assistant_with_tools(
            ids.iter().map(|id| llamada(id)).collect(),
        )];
        paso.extend(
            ids.iter()
                .map(|id| AgentMessage::tool_result(id, format!("resultado {id}"))),
        );
        paso
    }

    #[test]
    fn test_acotar_historial_que_cabe_queda_identico() {
        for reciente in [19, 20] {
            let original = [turno("a", 10), turno("b", 10), turno("c", reciente)].concat();
            assert_eq!(
                acotado(original.clone(), TOPE),
                original,
                "{} mensajes",
                20 + reciente
            );
        }
    }

    #[test]
    fn test_acotar_con_un_mensaje_sobre_el_tope_compacta_solo_el_turno_mas_antiguo() {
        let (a, b, c) = (turno("a", 10), turno("b", 10), turno("c", 21));
        let resultado = acotado([a.clone(), b.clone(), c.clone()].concat(), TOPE);
        assert_eq!(resultado, [compacto(&a), b, c].concat());
        assert_eq!(resultado.len(), 33);
    }

    #[test]
    fn test_turno_largo_sobrevive_compactado_a_dos_seguimientos() {
        let largo = turno("largo", 50);
        let (primero, segundo) = (turno("s1", 2), turno("s2", 2));

        // Como en el chat: se acota al cerrar cada turno
        let mut historial = acotado(largo.clone(), TOPE);
        assert_eq!(historial, largo);
        historial.extend(primero.clone());
        let historial = acotado(historial, TOPE);
        assert_eq!(historial, [compacto(&largo), primero.clone()].concat());

        let historial = acotado([historial, segundo.clone()].concat(), TOPE);
        assert_eq!(historial, [compacto(&largo), primero, segundo].concat());
        assert_eq!(historial[0], AgentMessage::user("pregunta largo"));
        assert_eq!(historial[1], AgentMessage::assistant("respuesta largo"));
    }

    #[test]
    fn test_se_compacta_del_mas_antiguo_al_mas_nuevo_y_solo_lo_necesario() {
        let (a, b, c) = (turno("a", 12), turno("b", 12), turno("c", 12));
        let antiguos = [a.clone(), b.clone(), c.clone()].concat();

        // 42 mensajes: basta compactar el más antiguo
        let reciente = turno("r", 6);
        assert_eq!(
            acotado([antiguos.clone(), reciente.clone()].concat(), TOPE),
            [compacto(&a), b.clone(), c.clone(), reciente].concat()
        );

        // 52 mensajes: hacen falta los dos más antiguos; el tercero queda intacto
        let reciente = turno("r", 16);
        assert_eq!(
            acotado([antiguos, reciente.clone()].concat(), TOPE),
            [compacto(&a), compacto(&b), c, reciente].concat()
        );
    }

    #[test]
    fn test_el_turno_mas_reciente_nunca_se_compacta_aunque_supere_el_tope() {
        let reciente = turno("r", 50);
        for antiguo in [turno("a", 2), turno("a", 10)] {
            assert_eq!(
                acotado([antiguo, reciente.clone()].concat(), TOPE),
                reciente
            );
        }
        assert_eq!(acotado(reciente.clone(), TOPE), reciente);
        assert_eq!(acotado(reciente.clone(), 0), reciente);
    }

    #[test]
    fn test_turno_sin_respuesta_final_de_texto_no_se_compacta() {
        let acaba_en_resultado = [vec![AgentMessage::user("pregunta x")], paso("x", 18)].concat();
        let acaba_en_peticion = [
            vec![AgentMessage::user("pregunta x")],
            paso("x", 17),
            vec![AgentMessage::assistant_with_tools(vec![llamada("x_fin")])],
        ]
        .concat();
        let acaba_en_texto_vacio = [
            vec![AgentMessage::user("pregunta x")],
            paso("x", 17),
            vec![AgentMessage::assistant("  ")],
        ]
        .concat();
        let (b, c) = (turno("b", 12), turno("c", 12));
        for sin_respuesta in [acaba_en_resultado, acaba_en_peticion, acaba_en_texto_vacio] {
            assert_eq!(sin_respuesta.len(), 20);
            // 44 mensajes: el primero no se puede compactar y se pasa al siguiente
            let resultado = acotado([sin_respuesta.clone(), b.clone(), c.clone()].concat(), TOPE);
            assert_eq!(resultado, [sin_respuesta, compacto(&b), c.clone()].concat());
        }
    }

    #[test]
    fn test_respuesta_de_texto_con_lista_de_llamadas_vacia_cuenta_como_texto() {
        let mut a = turno("a", 30);
        a[29].tool_calls = Some(Vec::new());
        let b = turno("b", 12);
        assert_eq!(
            acotado([a.clone(), b.clone()].concat(), TOPE),
            [compacto(&a), b].concat()
        );
    }

    #[test]
    fn test_turno_con_respuestas_intermedias_conserva_solo_la_ultima() {
        let con_intermedias = [
            vec![AgentMessage::user("pregunta x")],
            paso("x0", 10),
            vec![AgentMessage::assistant("intermedia rechazada 1")],
            paso("x1", 10),
            vec![
                AgentMessage::assistant("intermedia rechazada 2"),
                AgentMessage::assistant("definitiva"),
            ],
        ]
        .concat();
        let reciente = turno("r", 20);
        let resultado = acotado([con_intermedias, reciente.clone()].concat(), TOPE);
        assert_eq!(
            resultado[..2],
            [
                AgentMessage::user("pregunta x"),
                AgentMessage::assistant("definitiva")
            ]
        );
        assert_eq!(resultado[2..], reciente[..]);
        assert!(
            resultado
                .iter()
                .all(|m| !m.content.starts_with("intermedia"))
        );
    }

    #[test]
    fn test_turno_que_acabo_en_salida_segura_se_compacta_como_cualquier_otro() {
        use crate::application::use_cases::agent_orchestrator::MENSAJE_RESPUESTA_NO_VALIDADA;
        let salida_segura = [
            vec![AgentMessage::user("pregunta x")],
            paso("x", 28),
            vec![AgentMessage::assistant(MENSAJE_RESPUESTA_NO_VALIDADA)],
        ]
        .concat();
        let reciente = turno("r", 12);
        assert_eq!(
            acotado([salida_segura.clone(), reciente.clone()].concat(), TOPE),
            [compacto(&salida_segura), reciente].concat()
        );
    }

    #[test]
    fn test_si_con_todo_compactado_no_cabe_se_descartan_los_turnos_mas_antiguos() {
        let turnos: Vec<Vec<AgentMessage>> = (0..26).map(|i| turno(&format!("t{i}"), 4)).collect();
        let resultado = acotado(turnos.concat(), TOPE);
        // 25 compactados más el reciente son 54: caben el reciente (4) y 18 compactados (36)
        let esperado = [
            turnos[7..25]
                .iter()
                .map(|t| compacto(t))
                .collect::<Vec<_>>()
                .concat(),
            turnos[25].clone(),
        ]
        .concat();
        assert_eq!(resultado.len(), 40);
        assert_eq!(resultado, esperado);
    }

    #[test]
    fn test_acotar_entradas_mal_formadas() {
        assert!(acotado(Vec::new(), TOPE).is_empty());

        let sin_operador = vec![
            AgentMessage::assistant_with_tools(vec![llamada("x")]),
            AgentMessage::tool_result("x", "resultado"),
            AgentMessage::assistant("respuesta sin pregunta"),
        ];
        assert!(acotado(sin_operador.clone(), TOPE).is_empty());
        assert!(acotado(sin_operador, 0).is_empty());

        // Los huérfanos del inicio se eliminan, quepan o no, y no cuentan para decidir si cabe
        let huerfanos = vec![
            AgentMessage::tool_result("perdido", "resultado sin petición"),
            AgentMessage::assistant("respuesta sin pregunta"),
        ];
        let (a, b) = (turno("a", 20), turno("b", 20));
        assert_eq!(
            acotado([huerfanos.clone(), a.clone(), b.clone()].concat(), TOPE),
            [a.clone(), b.clone()].concat()
        );
        let c = turno("c", 21);
        assert_eq!(
            acotado([huerfanos, a.clone(), c.clone()].concat(), TOPE),
            [compacto(&a), c].concat()
        );
    }

    #[test]
    fn test_acotar_dos_veces_da_lo_mismo_que_una() {
        let casos = [
            [turno("a", 20), turno("b", 20), turno("c", 20)].concat(),
            [turno("a", 50), turno("b", 2), turno("c", 2)].concat(),
            [turno("a", 10), turno("b", 50)].concat(),
            (0..30)
                .map(|i| turno(&format!("t{i}"), 6))
                .collect::<Vec<_>>()
                .concat(),
        ];
        for original in casos {
            let una = acotado(original, TOPE);
            assert_eq!(acotado(una.clone(), TOPE), una);
        }
    }

    impl Generador {
        /// Turno de forma variada: con o sin herramientas, con respuestas intermedias de texto y,
        /// a veces, sin respuesta final (acaba en un resultado, en una petición o en la pregunta)
        fn turno_variado(&mut self, etiqueta: usize) -> Vec<AgentMessage> {
            let mut turno = vec![AgentMessage::user(format!("pregunta {etiqueta}"))];
            for n in 0..self.hasta(3) {
                turno.extend(paso(&format!("t{etiqueta}_p{n}"), 1 + self.hasta(11)));
                if self.hasta(3) == 0 {
                    turno.push(AgentMessage::assistant(format!(
                        "intermedia {etiqueta}.{n}"
                    )));
                }
            }
            match self.hasta(7) {
                0 => turno.push(AgentMessage::assistant_with_tools(vec![llamada(&format!(
                    "t{etiqueta}_pendiente"
                ))])),
                1 => {}
                _ => turno.push(AgentMessage::assistant(format!("respuesta {etiqueta}"))),
            }
            turno
        }
    }

    /// Ningún resultado sin su petición antes, en el mismo turno
    fn sin_resultados_huerfanos(turno: &[AgentMessage]) -> bool {
        let mut pedidas: HashSet<&str> = HashSet::new();
        turno.iter().all(|mensaje| {
            if let Some(llamadas) = &mensaje.tool_calls {
                pedidas.extend(llamadas.iter().map(|l| l.id.as_str()));
            }
            mensaje.role != AgentRole::Tool
                || mensaje
                    .tool_call_id
                    .as_deref()
                    .is_some_and(|id| pedidas.contains(id))
        })
    }

    #[test]
    fn test_propiedades_de_acotar_sobre_listas_generadas() {
        let mut generador = Generador(0x0F2C_2026);
        let (mut intactos, mut solo_compactados, mut con_descartes) = (0, 0, 0);
        for caso in 0..3_000 {
            let tope = generador.hasta(60);
            let huerfanos = generador.huerfanos();
            let n_turnos = generador.hasta(8);
            let sin_huerfanos: Vec<AgentMessage> = (0..n_turnos)
                .flat_map(|etiqueta| generador.turno_variado(etiqueta))
                .collect();
            let original = [huerfanos, sin_huerfanos.clone()].concat();
            let resultado = acotado(original.clone(), tope);
            let contexto = format!("caso {caso}, tope {tope}, {} mensajes", original.len());

            // Idempotente
            assert_eq!(acotado(resultado.clone(), tope), resultado, "{contexto}");
            // Si el original cabía, no cambia (los huérfanos no son parte de ningún turno)
            if sin_huerfanos.len() <= tope {
                assert_eq!(resultado, sin_huerfanos, "{contexto}");
                intactos += 1;
            }
            if n_turnos == 0 {
                assert!(resultado.is_empty(), "{contexto}");
                continue;
            }

            // Es una secuencia de turnos completos
            assert_eq!(resultado[0].role, AgentRole::User, "{contexto}");
            let originales = turnos_de(&original);
            let turnos = turnos_de(&resultado);
            assert!(!turnos.is_empty(), "{contexto}");
            assert!(turnos.len() <= originales.len(), "{contexto}");
            // Lo descartado es un prefijo de turnos: los conservados son los últimos
            let conservados = &originales[originales.len() - turnos.len()..];
            // Cada turno es idéntico al original o es su pregunta y su última respuesta de texto
            let compactado =
                |n: usize| acaba_en_texto(conservados[n]) && turnos[n] == compacto(conservados[n]);
            for (n, turno) in turnos.iter().enumerate() {
                assert!(*turno == conservados[n] || compactado(n), "{contexto}");
                assert!(sin_resultados_huerfanos(turno), "{contexto}");
            }
            // El más reciente es idéntico
            assert_eq!(turnos.last(), originales.last(), "{contexto}");
            // Solo se supera el tope cuando queda un único turno
            assert!(resultado.len() <= tope || turnos.len() == 1, "{contexto}");

            let ultimo_cambiado = (0..turnos.len()).rfind(|&n| turnos[n] != conservados[n]);
            let hubo_descartes = turnos.len() < originales.len();
            // Orden: todo turno compactable anterior a uno compactado también lo está. Con
            // descartes, lo están todos los anteriores al reciente
            let limite = if hubo_descartes {
                turnos.len() - 1
            } else {
                ultimo_cambiado.unwrap_or(0)
            };
            for (n, original) in conservados.iter().enumerate().take(limite) {
                assert!(!acaba_en_texto(original) || compactado(n), "{contexto}");
            }
            if hubo_descartes {
                // No se descarta de más: el turno anterior no cabía ni compactado
                let anterior = originales[originales.len() - turnos.len() - 1];
                let coste = if acaba_en_texto(anterior) {
                    2
                } else {
                    anterior.len()
                };
                assert!(resultado.len() + coste > tope, "{contexto}");
                con_descartes += 1;
            } else if let Some(n) = ultimo_cambiado {
                // No se compacta de más: sin la última compactación no cabía
                let ahorro = conservados[n].len() - turnos[n].len();
                assert!(resultado.len() + ahorro > tope, "{contexto}");
                solo_compactados += 1;
            }
        }
        // El generador ejercita de verdad las tres salidas
        assert!(intactos > 100, "{intactos} intactos");
        assert!(
            solo_compactados > 100,
            "{solo_compactados} solo compactados"
        );
        assert!(con_descartes > 100, "{con_descartes} con descartes");
    }
}
