//! Adaptador de Grafo Normativo sobre Petgraph (DiGraph en memoria)

use std::collections::{HashMap, HashSet};

use arrow_array::{Array, BooleanArray, Float32Array, RecordBatch, StringArray};
use async_trait::async_trait;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use tokio::sync::RwLock;

use crate::application::ports::graph_engine::{GraphEnginePort, LinajeNormativo};
use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;
use crate::domain::models::reference_graph::{AristaGrafo, TipoRelacionNormativa};

struct GrafoInterno {
    grafo: DiGraph<DocId, AristaGrafo>,
    indices_nodos: HashMap<DocId, NodeIndex>,
}

impl GrafoInterno {
    fn new() -> Self {
        Self {
            grafo: DiGraph::new(),
            indices_nodos: HashMap::new(),
        }
    }

    fn obtener_o_crear_nodo(&mut self, doc_id: &DocId) -> NodeIndex {
        if let Some(&idx) = self.indices_nodos.get(doc_id) {
            idx
        } else {
            let idx = self.grafo.add_node(doc_id.clone());
            self.indices_nodos.insert(doc_id.clone(), idx);
            idx
        }
    }
}

/// Adaptador de infraestructura que gestiona el grafo normativo en memoria usando Petgraph
pub struct PetgraphEngine {
    estado: RwLock<GrafoInterno>,
}

impl Default for PetgraphEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PetgraphEngine {
    pub fn new() -> Self {
        Self {
            estado: RwLock::new(GrafoInterno::new()),
        }
    }

    /// Hidrata el grafo normativo a partir de un RecordBatch de Arrow de `06_reference_graph`
    pub async fn cargar_desde_record_batch(
        &self,
        batch: &RecordBatch,
    ) -> Result<usize, DomainError> {
        let arista_id_arr = batch
            .column_by_name("arista_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                DomainError::StorageError("Columna arista_id no encontrada o inválida".to_string())
            })?;

        let doc_origen_arr = batch
            .column_by_name("doc_origen_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                DomainError::StorageError(
                    "Columna doc_origen_id no encontrada o inválida".to_string(),
                )
            })?;

        let doc_destino_arr = batch
            .column_by_name("doc_destino_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                DomainError::StorageError(
                    "Columna doc_destino_id no encontrada o inválida".to_string(),
                )
            })?;

        let tipo_rel_arr = batch
            .column_by_name("tipo_relacion")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                DomainError::StorageError(
                    "Columna tipo_relacion no encontrada o inválida".to_string(),
                )
            })?;

        let peso_arr = batch
            .column_by_name("peso_prelacion")
            .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
            .ok_or_else(|| {
                DomainError::StorageError(
                    "Columna peso_prelacion no encontrada o inválida".to_string(),
                )
            })?;

        let contexto_arr = batch
            .column_by_name("contexto_cita")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());

        let vigente_arr = batch
            .column_by_name("vigente")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())
            .ok_or_else(|| {
                DomainError::StorageError("Columna vigente no encontrada o inválida".to_string())
            })?;

        let mut aristas = Vec::with_capacity(batch.num_rows());

        for i in 0..batch.num_rows() {
            if !vigente_arr.value(i) {
                continue;
            }

            let arista_id = arista_id_arr.value(i);
            let doc_origen = DocId::new(doc_origen_arr.value(i))?;
            let doc_destino = DocId::new(doc_destino_arr.value(i))?;
            // Lectura tolerante: filas heredadas con vocabulario previo a §4 se degradan a REFERENCIA_A
            let (tipo_rel, _fue_respaldo) =
                TipoRelacionNormativa::parse_con_respaldo(tipo_rel_arr.value(i));
            let peso = peso_arr.value(i);
            let contexto = contexto_arr
                .filter(|a| a.is_valid(i))
                .map(|a| a.value(i).to_string());

            let arista = AristaGrafo::new(
                arista_id,
                doc_origen,
                "Sección General",
                doc_destino,
                "Sección General",
                tipo_rel,
                peso,
                contexto,
            )?;

            aristas.push(arista);
        }

        let total = aristas.len();
        self.cargar_aristas(&aristas).await?;
        Ok(total)
    }
}

#[async_trait]
impl GraphEnginePort for PetgraphEngine {
    async fn cargar_aristas(&self, aristas: &[AristaGrafo]) -> Result<(), DomainError> {
        let mut estado = self.estado.write().await;
        *estado = GrafoInterno::new();

        for arista in aristas {
            if !arista.vigente {
                continue;
            }
            let u = estado.obtener_o_crear_nodo(&arista.origen_doc_id);
            let v = estado.obtener_o_crear_nodo(&arista.destino_doc_id);
            estado.grafo.add_edge(u, v, arista.clone());
        }

        Ok(())
    }

    async fn obtener_norma_prevalente(&self, doc_id: &DocId) -> Result<DocId, DomainError> {
        let estado = self.estado.read().await;

        let mut visitados = HashSet::new();
        let mut actual = doc_id.clone();
        visitados.insert(actual.clone());

        loop {
            let Some(&nodo_actual) = estado.indices_nodos.get(&actual) else {
                return Ok(actual);
            };

            let mut derogador = None;

            for edge in estado
                .grafo
                .edges_directed(nodo_actual, petgraph::Direction::Incoming)
            {
                let peso = edge.weight();
                if peso.vigente && peso.tipo_relacion == TipoRelacionNormativa::DerogaA {
                    let source_idx = edge.source();
                    let source_doc = &estado.grafo[source_idx];
                    derogador = Some(source_doc.clone());
                    break;
                }
            }

            if let Some(nuevo_doc) = derogador {
                if visitados.contains(&nuevo_doc) {
                    return Err(DomainError::GraphError(format!(
                        "Ciclo o paradoja de derogación detectada involucrando al documento '{}'",
                        nuevo_doc.as_str()
                    )));
                }
                visitados.insert(nuevo_doc.clone());
                actual = nuevo_doc;
            } else {
                break;
            }
        }

        Ok(actual)
    }

    async fn obtener_linaje(&self, doc_id: &DocId) -> Result<LinajeNormativo, DomainError> {
        let referencias_directas = {
            let estado = self.estado.read().await;
            let mut refs = Vec::new();

            if let Some(&nodo) = estado.indices_nodos.get(doc_id) {
                // Referencias salientes
                for edge in estado
                    .grafo
                    .edges_directed(nodo, petgraph::Direction::Outgoing)
                {
                    let target = &estado.grafo[edge.target()];
                    refs.push((target.clone(), edge.weight().tipo_relacion));
                }
                // Referencias entrantes
                for edge in estado
                    .grafo
                    .edges_directed(nodo, petgraph::Direction::Incoming)
                {
                    let source = &estado.grafo[edge.source()];
                    refs.push((source.clone(), edge.weight().tipo_relacion));
                }
            }
            refs
        };

        let prevalente = self.obtener_norma_prevalente(doc_id).await?;
        let camino_derogacion = if &prevalente != doc_id {
            vec![doc_id.clone(), prevalente.clone()]
        } else {
            vec![doc_id.clone()]
        };

        Ok(LinajeNormativo {
            raiz: doc_id.clone(),
            referencias_directas,
            camino_derogacion,
            prevalente,
        })
    }

    async fn detectar_ciclos(&self) -> Result<Vec<Vec<DocId>>, DomainError> {
        let estado = self.estado.read().await;

        // Algoritmo de Tarjan para encontrar componentes fuertemente conexos (SCC)
        let sccs = petgraph::algo::tarjan_scc(&estado.grafo);
        let mut ciclos = Vec::new();

        for scc in sccs {
            if scc.len() > 1 {
                let ciclo: Vec<DocId> = scc
                    .into_iter()
                    .map(|idx| estado.grafo[idx].clone())
                    .collect();
                ciclos.push(ciclo);
            }
        }

        Ok(ciclos)
    }

    async fn estadisticas_grafo(&self) -> Result<(usize, usize), DomainError> {
        let estado = self.estado.read().await;
        Ok((estado.grafo.node_count(), estado.grafo.edge_count()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_petgraph_linaje_derogacion_sucesiva() {
        let engine = PetgraphEngine::new();

        let doc_v1 = DocId::new("NORMA-2015").unwrap();
        let doc_v2 = DocId::new("NORMA-2020").unwrap();
        let doc_v3 = DocId::new("NORMA-2025").unwrap();

        // 2020 deroga a 2015
        let edge1 = AristaGrafo::new(
            "E-1",
            doc_v2.clone(),
            "Art 1",
            doc_v1.clone(),
            "Art 1",
            TipoRelacionNormativa::DerogaA,
            1.0,
            None,
        )
        .unwrap();

        // 2025 deroga a 2020
        let edge2 = AristaGrafo::new(
            "E-2",
            doc_v3.clone(),
            "Art 1",
            doc_v2.clone(),
            "Art 1",
            TipoRelacionNormativa::DerogaA,
            1.0,
            None,
        )
        .unwrap();

        engine.cargar_aristas(&[edge1, edge2]).await.unwrap();

        // Si consultamos por la norma de 2015, la prevalente actual debe ser 2025
        let prevalente = engine.obtener_norma_prevalente(&doc_v1).await.unwrap();
        assert_eq!(prevalente, doc_v3);

        let linaje = engine.obtener_linaje(&doc_v1).await.unwrap();
        assert_eq!(linaje.prevalente, doc_v3);
    }

    #[tokio::test]
    async fn test_petgraph_detectar_ciclos() {
        let engine = PetgraphEngine::new();
        let doc_a = DocId::new("DOC-A").unwrap();
        let doc_b = DocId::new("DOC-B").unwrap();

        // A complementa a B y B complementa a A (ciclo de referencia)
        let edge_ab = AristaGrafo::new(
            "E-AB",
            doc_a.clone(),
            "Sec 1",
            doc_b.clone(),
            "Sec 1",
            TipoRelacionNormativa::ReferenciaA,
            1.0,
            None,
        )
        .unwrap();
        let edge_ba = AristaGrafo::new(
            "E-BA",
            doc_b.clone(),
            "Sec 1",
            doc_a.clone(),
            "Sec 1",
            TipoRelacionNormativa::ReferenciaA,
            1.0,
            None,
        )
        .unwrap();

        engine.cargar_aristas(&[edge_ab, edge_ba]).await.unwrap();

        let ciclos = engine.detectar_ciclos().await.unwrap();
        assert_eq!(ciclos.len(), 1);
        assert_eq!(ciclos[0].len(), 2);
    }
}
