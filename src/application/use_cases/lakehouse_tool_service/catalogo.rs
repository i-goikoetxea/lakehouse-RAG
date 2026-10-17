//! `inspect_lakehouse_catalog`: documentos con prelación y vigencia + glue catalog de tablas

use serde_json::{Value, json};

use super::LakehouseToolService;
use crate::application::dtos::tools::{
    InspectCatalogParams, LakehouseToolName, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::{
    TABLA_CATALOGO_TABLAS, TABLA_DOCUMENTOS_CATALOGO, TODAS_LAS_TABLAS,
};
use crate::domain::errors::DomainError;

impl LakehouseToolService {
    pub(super) async fn inspeccionar_catalogo(
        &self,
        argumentos: &Value,
    ) -> Result<Value, DomainError> {
        let params: InspectCatalogParams =
            parsear_argumentos(LakehouseToolName::InspectLakehouseCatalog, argumentos)?;
        let contexto = self
            .contexto(&params.centro_id, params.fecha_referencia.as_deref())
            .await?;

        let documentos = self
            .consultar(
                &contexto,
                &format!(
                    "SELECT d.doc_id, d.titulo, d.tipo_documento, d.nivel_prelacion, d.revision, d.version_num, d.fecha_vigor, d.fecha_derogacion, d.vigente FROM {} d WHERE {} ORDER BY d.nivel_prelacion ASC, d.doc_id ASC, d.version_num DESC",
                    contexto.vista(TABLA_DOCUMENTOS_CATALOGO),
                    contexto.predicado_vigencia("d", "d")
                ),
            )
            .await?;

        let tablas = self
            .consultar(
                &contexto,
                &format!(
                    "SELECT t.tabla_id, t.titulo_tabla, t.doc_id, d.revision, t.version_num, d.nivel_prelacion, t.eje_x_nombre, t.eje_x_tipo, t.eje_x_unidad, t.eje_y_nombre, t.eje_y_tipo, t.eje_y_unidad, t.valor_salida_nombre, t.instrucciones_uso_sql FROM {} t {} WHERE {} ORDER BY d.nivel_prelacion ASC, t.tabla_id ASC",
                    contexto.vista(TABLA_CATALOGO_TABLAS),
                    contexto.union_catalogo("t", "d"),
                    contexto.predicado_vigencia("t", "d")
                ),
            )
            .await?;

        Ok(json!({
            "centro_id": contexto.centro.as_str(),
            "fecha_referencia": contexto.fecha_json(),
            "documentos": documentos,
            "tablas_doble_entrada": tablas,
            "vistas_sql": TODAS_LAS_TABLAS,
            "nota": "Prelación: 1 = especificación del cliente … 5 = norma. Ante conflicto prevalece el nivel menor; el resto es supletorio. Para cruzar una tabla usa lookup_double_entry_table con su tabla_id.",
        }))
    }
}
