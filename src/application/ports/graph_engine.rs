//! Puerto de aplicación para el motor de grafos normativos (Petgraph)

use async_trait::async_trait;

use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;
use crate::domain::models::reference_graph::{AristaGrafo, TipoRelacionNormativa};

/// Resultado del análisis de linaje y precedencia normativa
#[derive(Debug, Clone, PartialEq)]
pub struct LinajeNormativo {
    pub raiz: DocId,
    pub referencias_directas: Vec<(DocId, TipoRelacionNormativa)>,
    pub camino_derogacion: Vec<DocId>,
    pub prevalente: DocId,
}

/// Puerto para operaciones y recorridos sobre el grafo de referencias normativas
#[async_trait]
pub trait GraphEnginePort: Send + Sync {
    /// Carga e hidrata el grafo en memoria a partir de aristas normativas
    async fn cargar_aristas(&self, aristas: &[AristaGrafo]) -> Result<(), DomainError>;

    /// Encuentra la norma vigente prevalente siguiendo las relaciones de derogación
    async fn obtener_norma_prevalente(&self, doc_id: &DocId) -> Result<DocId, DomainError>;

    /// Obtiene el linaje completo de un documento (referencias, derogaciones y dependencias)
    async fn obtener_linaje(&self, doc_id: &DocId) -> Result<LinajeNormativo, DomainError>;

    /// Detecta si existen ciclos normativos paradójicos en el grafo
    async fn detectar_ciclos(&self) -> Result<Vec<Vec<DocId>>, DomainError>;

    /// Retorna el número de nodos y aristas cargados en el grafo actual
    async fn estadisticas_grafo(&self) -> Result<(usize, usize), DomainError>;
}
