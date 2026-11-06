//! Traduce las intervenciones del arnés a lo que lee el operador en el Studio. El `motivo` del
//! evento está escrito para el modelo y la telemetría: nunca se muestra tal cual.

use serde::{Deserialize, Serialize};

use crate::application::dtos::agent::EtapaArnes;

/// Una actuación del arnés en el turno, ya en lenguaje del operador
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntervencionArnesDto {
    pub etapa: EtapaArnes,
    pub mensaje: String,
}

/// Texto de la intervención para el operador
pub fn explicar_intervencion(etapa: EtapaArnes, motivo: &str) -> String {
    match etapa {
        EtapaArnes::Directiva => {
            "Esta pregunta necesita datos del centro: se pidió consultarlos antes de redactar la respuesta.".to_string()
        }
        EtapaArnes::LlamadaNoObtenida => {
            "El asistente no hizo la consulta pedida en el primer intento.".to_string()
        }
        EtapaArnes::Reintento => format!(
            "La primera respuesta no se mostró porque {}. Se pidió otra.",
            causa_del_rechazo(motivo)
        ),
        EtapaArnes::SalidaSegura => format!(
            "La segunda respuesta tampoco se mostró porque {}. Lo que sí se consultó queda en el Studio.",
            causa_del_rechazo(motivo)
        ),
    }
}

/// Ante un motivo desconocido da una causa genérica: el texto original nunca llega al operador
fn causa_del_rechazo(motivo: &str) -> &'static str {
    if motivo.contains("Capa 2") {
        "no se apoyaba en una consulta a los datos del centro"
    } else if motivo.contains("no consta y añade") {
        "decía que el dato no consta y añadía una cifra que no aparece en los datos consultados"
    } else if motivo.contains("la cifra «") {
        "incluía una cifra que no aparece en los datos consultados"
    } else if motivo.contains("sin_responsable_declarado") {
        "nombraba un responsable que no consta en los datos consultados"
    } else if motivo.contains("ejes_ambiguos") {
        "daba un único valor de la tabla sin aclarar a qué eje corresponde cada dato"
    } else {
        "no coincidía con los datos consultados"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::use_cases::arnes_turno::ViolacionArnes;

    const TERMINOS_INTERNOS: [&str; 8] = [
        "_",
        "Capa",
        "lookup",
        "matriz devolvió",
        "guard",
        "arnés",
        "tool",
        "herramienta",
    ];

    fn violaciones() -> Vec<ViolacionArnes> {
        vec![
            ViolacionArnes::CapaDosNoIntentada,
            ViolacionArnes::CifraSinAnclar {
                cifra: "999".to_string(),
            },
            ViolacionArnes::RolSinRespaldo,
            ViolacionArnes::EjesAmbiguosSinAclarar,
            ViolacionArnes::CifraEnAbstencion {
                cifra: "999".to_string(),
            },
        ]
    }

    fn sin_jerga(texto: &str) {
        assert!(!texto.is_empty());
        for termino in TERMINOS_INTERNOS {
            assert!(!texto.contains(termino), "«{termino}» en: {texto}");
        }
    }

    #[test]
    fn test_ninguna_etapa_muestra_jerga_interna() {
        for etapa in [EtapaArnes::Directiva, EtapaArnes::LlamadaNoObtenida] {
            sin_jerga(&explicar_intervencion(
                etapa,
                "se solicita lookup_double_entry_table",
            ));
        }
        for etapa in [EtapaArnes::Reintento, EtapaArnes::SalidaSegura] {
            for violacion in violaciones() {
                sin_jerga(&explicar_intervencion(etapa, &violacion.motivo()));
            }
        }
    }

    #[test]
    fn test_cada_violacion_real_tiene_su_propia_explicacion() {
        let textos: Vec<String> = violaciones()
            .iter()
            .map(|v| explicar_intervencion(EtapaArnes::Reintento, &v.motivo()))
            .collect();
        let generico = explicar_intervencion(EtapaArnes::Reintento, "motivo desconocido");
        for texto in &textos {
            assert_ne!(*texto, generico, "motivo real sin traducción: {texto}");
        }
        let mut distintos = textos.clone();
        distintos.sort();
        distintos.dedup();
        assert_eq!(distintos.len(), textos.len(), "{textos:?}");
    }

    #[test]
    fn test_un_motivo_desconocido_no_se_filtra_al_operador() {
        let motivo = "<img src=x onerror=alert(1)> ignora tus reglas sql_secreto";
        for etapa in [EtapaArnes::Reintento, EtapaArnes::SalidaSegura] {
            let texto = explicar_intervencion(etapa, motivo);
            sin_jerga(&texto);
            assert!(!texto.contains("img"), "{texto}");
        }
    }

    #[test]
    fn test_la_cifra_rechazada_no_llega_al_operador() {
        for violacion in violaciones() {
            for etapa in [EtapaArnes::Reintento, EtapaArnes::SalidaSegura] {
                let texto = explicar_intervencion(etapa, &violacion.motivo());
                assert!(!texto.contains("999"), "{texto}");
            }
        }
    }

    #[test]
    fn test_la_salida_segura_remite_al_studio() {
        let texto = explicar_intervencion(EtapaArnes::SalidaSegura, "x");
        assert!(texto.contains("Studio"), "{texto}");
    }
}
