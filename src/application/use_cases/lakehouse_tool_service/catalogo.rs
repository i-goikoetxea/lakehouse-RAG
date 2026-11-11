//! `inspect_lakehouse_catalog`: documentos con prelación y vigencia + glue catalog de tablas

use serde_json::{Value, json};

use super::estructura::explicacion_de_estado;
use super::{LakehouseToolService, entero, lista_de_paginas, texto, texto_opcional};
use crate::application::dtos::tools::{
    InspectCatalogParams, LakehouseToolName, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::{
    TABLA_CATALOGO_TABLAS, TABLA_DOCUMENTOS_CATALOGO, TODAS_LAS_TABLAS,
};
use crate::domain::errors::DomainError;

/// Lo que se muestra en lugar de una fecha que el documento no declara
const FECHA_NO_DECLARADA: &str = "no declarada";

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
                    "SELECT d.doc_id, d.titulo, d.tipo_documento, d.nivel_prelacion, d.revision, d.version_num, d.fecha_vigor, d.fecha_derogacion, d.vigente, d.doc_id_provisional, d.paginas_degradadas, {} AS vigencia_no_declarada FROM {} d WHERE {} ORDER BY d.nivel_prelacion ASC, d.doc_id ASC, d.version_num DESC",
                    contexto.marca_no_declarada("d"),
                    contexto.vista(TABLA_DOCUMENTOS_CATALOGO),
                    contexto.predicado_vigencia("d", "d")
                ),
            )
            .await?;

        let estados = self.estados_de_revisiones(&contexto).await?;
        let documentos: Vec<Value> = documentos
            .into_iter()
            .map(|mut fila| {
                let doc_id = texto(&fila, "doc_id");
                let provisional = texto(&fila, "doc_id_provisional") == doc_id;
                let version = entero(&fila, "version_num").unwrap_or_default();
                let estado = estados.get(&(doc_id, version)).cloned();
                let aviso = estado.as_deref().and_then(|e| {
                    explicacion_de_estado(
                        e,
                        provisional,
                        &lista_de_paginas(&fila, "paginas_degradadas"),
                    )
                });
                let fecha_vigor = texto_opcional(&fila, "fecha_vigor");
                fila.insert(
                    "fecha_vigor_texto".to_string(),
                    json!(fecha_vigor.as_deref().unwrap_or(FECHA_NO_DECLARADA)),
                );
                fila.insert("fecha_vigor".to_string(), json!(fecha_vigor));
                fila.insert("provisional".to_string(), json!(provisional));
                fila.insert("estado_contenido".to_string(), json!(estado));
                fila.insert("aviso_contenido".to_string(), json!(aviso));
                Value::Object(fila)
            })
            .collect();

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
            "nota": "Prelación: 1 = especificación del cliente … 5 = norma. Ante conflicto prevalece el nivel menor; el resto es supletorio. Para cruzar una tabla usa lookup_double_entry_table con su tabla_id. Un documento con aviso_contenido no está completo: dilo si lo usas.",
        }))
    }
}
