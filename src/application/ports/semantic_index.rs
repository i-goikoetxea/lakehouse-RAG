//! Puerto del índice semántico en memoria (plan v2 §2.3, `SemanticSearchPort`): fragmentos de
//! `02_chunks_normativa` e hitos de `03_matriz_operativa` **con sus vectores**, por centro.
//! Lo alimenta la proyección desde LanceDB (todas las revisiones confirmadas) y lo consulta
//! `search_normative_context`, que filtra por vigencia, prelación y documento.

use async_trait::async_trait;

use crate::domain::models::center::CenterId;

/// Tabla de origen de un fragmento indexado
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrigenFragmento {
    Chunk,
    Hito,
}

impl OrigenFragmento {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Chunk => "02_chunks_normativa",
            Self::Hito => "03_matriz_operativa",
        }
    }
}

/// Fragmento recuperable (chunk normativo o hito) con su linaje y su vector
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentoIndexado {
    /// `chunk_id` o `hito_id`
    pub id: String,
    pub origen: OrigenFragmento,
    pub doc_id: String,
    pub version_num: i32,
    pub nivel_prelacion: i8,
    pub breadcrumb_jerarquia: String,
    pub condicion_aplicacion: Option<String>,
    /// Texto que se devuelve y sobre el que se calcula BM25
    pub texto: String,
    /// Chunk de origen (solo hitos), para la cita
    pub chunk_origen_id: Option<String>,
    /// Vector del fragmento; vacío si la fila no tenía vector
    pub vector: Vec<f32>,
}

/// Índice semántico por centro (vista derivada, desechable)
#[async_trait]
pub trait IndiceSemanticoPort: Send + Sync {
    /// Sustituye todos los fragmentos de un centro
    async fn reemplazar_centro(&self, centro: &CenterId, fragmentos: Vec<FragmentoIndexado>);

    /// Olvida los fragmentos de un centro (al desactivarlo)
    async fn retirar_centro(&self, centro: &CenterId);

    /// Fragmentos del centro (vacío si no está proyectado)
    async fn fragmentos(&self, centro: &CenterId) -> Vec<FragmentoIndexado>;
}
