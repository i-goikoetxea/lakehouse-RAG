//! Índice semántico en memoria por centro (adaptador de `IndiceSemanticoPort`)

use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::application::ports::semantic_index::{FragmentoIndexado, IndiceSemanticoPort};
use crate::domain::models::center::CenterId;

/// Fragmentos por centro; cada centro se sustituye entero en cada proyección
#[derive(Default)]
pub struct IndiceSemanticoMemoria {
    por_centro: RwLock<HashMap<CenterId, Vec<FragmentoIndexado>>>,
}

impl IndiceSemanticoMemoria {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl IndiceSemanticoPort for IndiceSemanticoMemoria {
    async fn reemplazar_centro(&self, centro: &CenterId, fragmentos: Vec<FragmentoIndexado>) {
        self.por_centro
            .write()
            .await
            .insert(centro.clone(), fragmentos);
    }

    async fn retirar_centro(&self, centro: &CenterId) {
        self.por_centro.write().await.remove(centro);
    }

    async fn fragmentos(&self, centro: &CenterId) -> Vec<FragmentoIndexado> {
        self.por_centro
            .read()
            .await
            .get(centro)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::semantic_index::OrigenFragmento;

    fn fragmento(id: &str) -> FragmentoIndexado {
        FragmentoIndexado {
            id: id.to_string(),
            origen: OrigenFragmento::Chunk,
            doc_id: "DOC".to_string(),
            version_num: 1,
            nivel_prelacion: 2,
            breadcrumb_jerarquia: "1 > 1.1".to_string(),
            condicion_aplicacion: None,
            texto: "texto".to_string(),
            chunk_origen_id: None,
            vector: vec![1.0, 0.0],
        }
    }

    #[tokio::test]
    async fn test_indice_aisla_centros_y_sustituye_por_completo() {
        let indice = IndiceSemanticoMemoria::new();
        let a = CenterId::new("obra-a").unwrap();
        let b = CenterId::new("obra-b").unwrap();
        indice
            .reemplazar_centro(&a, vec![fragmento("a1"), fragmento("a2")])
            .await;
        indice.reemplazar_centro(&b, vec![fragmento("b1")]).await;
        indice.reemplazar_centro(&a, vec![fragmento("a3")]).await;
        let ids: Vec<String> = indice
            .fragmentos(&a)
            .await
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(ids, vec!["a3"]);
        indice.retirar_centro(&b).await;
        assert!(indice.fragmentos(&b).await.is_empty());
    }
}
