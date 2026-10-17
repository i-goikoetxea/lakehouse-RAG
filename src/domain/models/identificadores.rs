//! Identificadores primarios versionados: evitan colisiones entre revisiones (H4)

use crate::domain::models::document::DocId;

/// Separador entre el documento y su versión. `DocId` nunca contiene `@`,
/// por lo que el identificador versionado es inequívoco.
pub const SEPARADOR_VERSION: char = '@';

/// Tipo de entidad del Lakehouse que recibe un identificador versionado
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoEntidad {
    Chunk,
    Hito,
    Tabla,
    Celda,
    Arista,
}

impl TipoEntidad {
    /// Prefijo de tres letras que aparece en el identificador
    pub fn prefijo(&self) -> &'static str {
        match self {
            Self::Chunk => "CHK",
            Self::Hito => "HIT",
            Self::Tabla => "TAB",
            Self::Celda => "CEL",
            Self::Arista => "ARI",
        }
    }
}

/// Identificador primario estable por documento, versión, tipo e índice
/// (`PO-ALM-03@v3-HIT-0002`).
pub fn id_versionado(doc_id: &DocId, version: u32, tipo: TipoEntidad, indice: usize) -> String {
    format!(
        "{doc_id}{SEPARADOR_VERSION}v{version}-{}-{indice:04}",
        tipo.prefijo()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::document::DocId;

    #[test]
    fn test_id_versionado_distingue_revisiones_y_entidades() {
        let doc = DocId::new("PO-ALM-03").unwrap();
        let v3 = id_versionado(&doc, 3, TipoEntidad::Hito, 2);
        let v4 = id_versionado(&doc, 4, TipoEntidad::Hito, 2);
        assert_eq!(v3, "PO-ALM-03@v3-HIT-0002");
        assert_ne!(v3, v4);
        assert_ne!(v4, id_versionado(&doc, 4, TipoEntidad::Chunk, 2));
        for tipo in [
            TipoEntidad::Chunk,
            TipoEntidad::Hito,
            TipoEntidad::Tabla,
            TipoEntidad::Celda,
            TipoEntidad::Arista,
        ] {
            assert!(id_versionado(&doc, 1, tipo, 0).starts_with("PO-ALM-03@v1-"));
        }
    }
}
