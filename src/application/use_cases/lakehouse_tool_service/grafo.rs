//! `traverse_normative_graph`: recorrido acotado antisupernodos (problema 4).
//!
//! Se construye por llamada desde `06_grafo_referencias` filtrada por vigencia (o fecha), así
//! que distingue versiones y nunca mezcla centros. Los nodos son códigos canónicos: el origen
//! de cada arista es su documento (`doc_id`) y el destino, su código sin revisión.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde_json::{Map, Value, json};

use super::{LakehouseToolService, entero, texto, texto_opcional};
use crate::application::dtos::tools::{
    LakehouseToolName, TraverseNormativeGraphParams, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::{
    TABLA_DOCUMENTOS_CATALOGO, TABLA_GRAFO_REFERENCIAS,
};
use crate::domain::errors::DomainError;
use crate::domain::services::normalizacion::{codigo_canonico, normalizar_token, separar_revision};

/// Saltos máximos admitidos
const MAX_SALTOS: u8 = 2;
/// Relaciones que se muestran en el resumen de un hub
const TOP_RELACIONES_HUB: usize = 5;

/// Arista vigente ya canonicalizada
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AristaVigente {
    pub edge_id: String,
    pub origen: String,
    pub destino: String,
    pub tipo_relacion: String,
    pub contexto: Option<String>,
    pub doc_id: String,
    pub revision: String,
    pub version_num: i64,
}

/// Límites del recorrido
#[derive(Debug, Clone, Copy)]
pub(crate) struct LimitesRecorrido {
    pub max_saltos: u8,
    pub max_vecinos: usize,
    pub umbral_hub: usize,
    pub max_nodos: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodoRecorrido {
    pub id: String,
    pub salto: u8,
    pub grado: usize,
    pub nivel_prelacion: Option<i64>,
    pub hub_resumido: bool,
    pub top_relaciones: Vec<(String, usize)>,
    pub vecinos_omitidos: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct RecorridoAcotado {
    pub nodos: Vec<NodoRecorrido>,
    pub aristas: Vec<AristaVigente>,
    pub truncado: bool,
}

/// Vecinos de un nodo: (vecino, índice de la arista)
fn adyacencia(aristas: &[AristaVigente]) -> HashMap<&str, Vec<(&str, usize)>> {
    let mut mapa: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();
    for (i, a) in aristas.iter().enumerate() {
        mapa.entry(a.origen.as_str())
            .or_default()
            .push((a.destino.as_str(), i));
        if a.origen != a.destino {
            mapa.entry(a.destino.as_str())
                .or_default()
                .push((a.origen.as_str(), i));
        }
    }
    mapa
}

fn top_relaciones(vecinos: &[(&str, usize)], aristas: &[AristaVigente]) -> Vec<(String, usize)> {
    let mut cuenta: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, i) in vecinos {
        *cuenta
            .entry(aristas[*i].tipo_relacion.as_str())
            .or_default() += 1;
    }
    let mut lista: Vec<(String, usize)> = cuenta
        .into_iter()
        .map(|(t, n)| (t.to_string(), n))
        .collect();
    lista.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    lista.truncate(TOP_RELACIONES_HUB);
    lista
}

/// BFS acotado: el origen siempre se expande (con tope de vecinos); el resto de nodos con
/// grado > umbral se devuelven como hub resumido sin expandir. Vecinos por prelación.
pub(crate) fn recorrer(
    aristas: &[AristaVigente],
    origen: &str,
    limites: LimitesRecorrido,
    nivel_de: &HashMap<String, i64>,
) -> RecorridoAcotado {
    let mapa = adyacencia(aristas);
    let nivel = |id: &str| nivel_de.get(id).copied();
    let clave_orden = |id: &str| (nivel(id).unwrap_or(i64::MAX), id.to_string());
    let mut recorrido = RecorridoAcotado::default();
    let mut vistos: HashSet<String> = HashSet::from([origen.to_string()]);
    let mut aristas_incluidas: HashSet<usize> = HashSet::new();
    let mut cola: VecDeque<(String, u8)> = VecDeque::from([(origen.to_string(), 0)]);

    while let Some((id, salto)) = cola.pop_front() {
        let vecinos = mapa.get(id.as_str()).cloned().unwrap_or_default();
        let grado = vecinos.len();
        let es_hub = salto > 0 && grado > limites.umbral_hub;
        let mut nodo = NodoRecorrido {
            id: id.clone(),
            salto,
            grado,
            nivel_prelacion: nivel(&id),
            hub_resumido: es_hub,
            top_relaciones: if es_hub {
                top_relaciones(&vecinos, aristas)
            } else {
                Vec::new()
            },
            vecinos_omitidos: 0,
        };
        if !es_hub && salto < limites.max_saltos {
            let mut ordenados = vecinos.clone();
            ordenados.sort_by_key(|(v, i)| (clave_orden(v), aristas[*i].tipo_relacion.clone()));
            let mut nuevos = 0;
            for (vecino, indice) in ordenados {
                if vistos.contains(vecino) {
                    aristas_incluidas.insert(indice);
                } else if nuevos >= limites.max_vecinos {
                    nodo.vecinos_omitidos += 1;
                } else if vistos.len() >= limites.max_nodos {
                    recorrido.truncado = true;
                    nodo.vecinos_omitidos += 1;
                } else {
                    nuevos += 1;
                    vistos.insert(vecino.to_string());
                    aristas_incluidas.insert(indice);
                    cola.push_back((vecino.to_string(), salto + 1));
                }
            }
        }
        recorrido.nodos.push(nodo);
    }
    recorrido.aristas = aristas_incluidas
        .into_iter()
        .filter(|i| vistos.contains(&aristas[*i].origen) && vistos.contains(&aristas[*i].destino))
        .map(|i| aristas[i].clone())
        .collect();
    recorrido.aristas.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));
    recorrido
        .nodos
        .sort_by_key(|n| (n.nivel_prelacion.unwrap_or(i64::MAX), n.salto, n.id.clone()));
    recorrido
}

fn arista_desde_fila(fila: &Map<String, Value>) -> AristaVigente {
    AristaVigente {
        edge_id: texto(fila, "edge_id"),
        origen: codigo_canonico(&texto(fila, "doc_id")),
        destino: separar_revision(&texto(fila, "destino_id")).0,
        tipo_relacion: texto(fila, "tipo_relacion"),
        contexto: texto_opcional(fila, "contexto_relacion"),
        doc_id: texto(fila, "doc_id"),
        revision: texto(fila, "revision"),
        version_num: entero(fila, "version_num").unwrap_or_default(),
    }
}

fn nodo_salida(nodo: &NodoRecorrido) -> Value {
    let mut salida = json!({
        "id": nodo.id,
        "salto": nodo.salto,
        "grado": nodo.grado,
        "nivel_prelacion": nodo.nivel_prelacion,
        "hub_resumido": nodo.hub_resumido,
    });
    if nodo.hub_resumido {
        salida["resumen_hub"] = json!({
            "grado": nodo.grado,
            "top_relaciones": nodo.top_relaciones.iter().map(|(t, n)| json!({ "tipo_relacion": t, "aristas": n })).collect::<Vec<_>>(),
        });
    }
    if nodo.vecinos_omitidos > 0 {
        salida["vecinos_omitidos"] = json!(nodo.vecinos_omitidos);
    }
    salida
}

impl LakehouseToolService {
    pub(super) async fn recorrer_grafo(&self, argumentos: &Value) -> Result<Value, DomainError> {
        let params: TraverseNormativeGraphParams =
            parsear_argumentos(LakehouseToolName::TraverseNormativeGraph, argumentos)?;
        let contexto = self
            .contexto(&params.centro_id, params.fecha_referencia.as_deref())
            .await?;
        let origen = separar_revision(params.nodo_origen.trim()).0;
        if origen.is_empty() {
            return Err(DomainError::ValidationError(
                "nodo_origen está vacío".to_string(),
            ));
        }
        let tipos: Option<HashSet<String>> = params
            .tipo_relacion
            .as_ref()
            .map(|t| t.iter().map(|r| normalizar_token(r)).collect());
        let limites = LimitesRecorrido {
            max_saltos: params.max_saltos.unwrap_or(MAX_SALTOS).clamp(1, MAX_SALTOS),
            max_vecinos: params
                .max_vecinos
                .map_or(self.config.max_vecinos, usize::from)
                .clamp(1, self.config.max_nodos),
            umbral_hub: self.config.umbral_hub,
            max_nodos: self.config.max_nodos,
        };

        let filas = self
            .consultar(
                &contexto,
                &format!(
                    "SELECT g.edge_id, g.doc_id, g.version_num, g.origen_id, g.tipo_relacion, g.destino_id, g.contexto_relacion, d.revision FROM {} g {} WHERE {}",
                    contexto.vista(TABLA_GRAFO_REFERENCIAS),
                    contexto.union_catalogo("g", "d"),
                    contexto.predicado_vigencia("g", "d")
                ),
            )
            .await?;
        let aristas: Vec<AristaVigente> = filas
            .iter()
            .map(arista_desde_fila)
            .filter(|a| {
                tipos
                    .as_ref()
                    .is_none_or(|t| t.contains(&normalizar_token(&a.tipo_relacion)))
            })
            .collect();
        let documentos = self
            .consultar(
                &contexto,
                &format!(
                    "SELECT d.doc_id, d.nivel_prelacion FROM {} d WHERE {}",
                    contexto.vista(TABLA_DOCUMENTOS_CATALOGO),
                    contexto.predicado_vigencia("d", "d")
                ),
            )
            .await?;
        let nivel_de: HashMap<String, i64> = documentos
            .iter()
            .filter_map(|d| {
                Some((
                    codigo_canonico(&texto(d, "doc_id")),
                    entero(d, "nivel_prelacion")?,
                ))
            })
            .collect();

        let base = json!({
            "centro_id": contexto.centro.as_str(),
            "fecha_referencia": contexto.fecha_json(),
            "nodo_origen": origen,
            "limites": { "max_saltos": limites.max_saltos, "max_vecinos": limites.max_vecinos, "umbral_hub": limites.umbral_hub, "max_nodos": limites.max_nodos },
        });
        let existe = aristas
            .iter()
            .any(|a| a.origen == origen || a.destino == origen);
        if !existe {
            let mut sugerencias: Vec<String> = aristas
                .iter()
                .flat_map(|a| [a.origen.clone(), a.destino.clone()])
                .filter(|n| n.contains(&origen) || origen.contains(n.as_str()))
                .collect();
            sugerencias.sort();
            sugerencias.dedup();
            sugerencias.truncate(10);
            let mut salida = base;
            salida["estado"] = json!("nodo_no_encontrado");
            salida["mensaje"] = json!("El nodo no tiene aristas vigentes con esos filtros");
            salida["sugerencias"] = json!(sugerencias);
            return Ok(salida);
        }

        let recorrido = recorrer(&aristas, &origen, limites, &nivel_de);
        let mut salida = base;
        salida["estado"] = json!("ok");
        salida["truncado"] = json!(recorrido.truncado);
        salida["nodos"] = json!(recorrido.nodos.iter().map(nodo_salida).collect::<Vec<_>>());
        salida["aristas"] = json!(recorrido.aristas.iter().map(|a| json!({
            "origen": a.origen, "tipo_relacion": a.tipo_relacion, "destino": a.destino,
            "contexto": a.contexto,
            "cita": { "doc_id": a.doc_id, "revision": a.revision, "version_num": a.version_num, "edge_id": a.edge_id },
        })).collect::<Vec<_>>());
        Ok(salida)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arista(i: usize, origen: &str, destino: &str, tipo: &str) -> AristaVigente {
        AristaVigente {
            edge_id: format!("e{i:05}"),
            origen: origen.to_string(),
            destino: destino.to_string(),
            tipo_relacion: tipo.to_string(),
            contexto: None,
            doc_id: origen.to_string(),
            revision: "Rev. 01".to_string(),
            version_num: 1,
        }
    }

    fn limites() -> LimitesRecorrido {
        LimitesRecorrido {
            max_saltos: 2,
            max_vecinos: 15,
            umbral_hub: 25,
            max_nodos: 50,
        }
    }

    fn grafo_con_hub() -> Vec<AristaVigente> {
        let mut aristas = vec![arista(0, "PO-X", "HUB", "SUBORDINADO_A_NORMA")];
        aristas.extend((1..=1000).map(|i| arista(i, &format!("DOC-{i}"), "HUB", "REFERENCIA_A")));
        aristas
    }

    #[test]
    fn test_hub_de_1000_aristas_se_resume_sin_expandir() {
        let recorrido = recorrer(&grafo_con_hub(), "PO-X", limites(), &HashMap::new());
        assert!(recorrido.nodos.len() <= 50);
        let hub = recorrido.nodos.iter().find(|n| n.id == "HUB").unwrap();
        assert!(hub.hub_resumido);
        assert_eq!(hub.grado, 1001);
        assert_eq!(hub.top_relaciones[0], ("REFERENCIA_A".to_string(), 1000));
        assert_eq!(recorrido.nodos.len(), 2);
    }

    #[test]
    fn test_origen_hub_se_expande_con_tope_de_vecinos_y_respuesta_acotada() {
        let recorrido = recorrer(&grafo_con_hub(), "HUB", limites(), &HashMap::new());
        assert!(recorrido.nodos.len() <= 50, "{}", recorrido.nodos.len());
        let hub = recorrido.nodos.iter().find(|n| n.id == "HUB").unwrap();
        assert!(!hub.hub_resumido);
        assert_eq!(hub.vecinos_omitidos, 1001 - 15);
        assert_eq!(recorrido.nodos.len(), 16);
    }

    #[test]
    fn test_vecinos_ordenados_por_prelacion_y_dos_saltos() {
        let aristas = vec![
            arista(1, "PO-ALM-03", "UNE-EN-1090-2", "SUBORDINADO_A_NORMA"),
            arista(2, "PO-ALM-03", "PCC-01", "SUBORDINADO_A_NORMA"),
            arista(3, "PCC-01", "ANEXO-II", "REFERENCIA_A"),
            arista(4, "ANEXO-II", "TERCER-SALTO", "REFERENCIA_A"),
        ];
        let niveles = HashMap::from([
            ("PCC-01".to_string(), 1),
            ("PO-ALM-03".to_string(), 2),
            ("UNE-EN-1090-2".to_string(), 5),
        ]);
        let mut l = limites();
        l.max_vecinos = 1;
        let recorrido = recorrer(&aristas, "PO-ALM-03", l, &niveles);
        let ids: Vec<&str> = recorrido.nodos.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["PCC-01", "PO-ALM-03", "ANEXO-II"],
            "prelación 1 primero; sin tercer salto"
        );
    }
}
