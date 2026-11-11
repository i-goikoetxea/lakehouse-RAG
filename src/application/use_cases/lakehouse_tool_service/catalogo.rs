//! `inspect_lakehouse_catalog`: documentos con prelación y vigencia + glue catalog de tablas

use std::collections::HashMap;

use serde_json::{Value, json};

use super::estructura::explicacion_de_estado;
use super::{LakehouseToolService, entero, lista_de_paginas, texto, texto_opcional};
use crate::application::dtos::tools::{
    InspectCatalogParams, LakehouseToolName, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::{
    TABLA_CATALOGO_TABLAS, TABLA_DOCUMENTOS_CATALOGO, TODAS_LAS_TABLAS,
};
use crate::application::ports::semantic_index::{FragmentoIndexado, OrigenFragmento};
use crate::domain::errors::DomainError;
use crate::domain::models::estructura_documental::{EstadoContenido, TipoBloque};
use crate::domain::services::muestra_documento::{escritura_dominante, muestra_de_texto_corrido};

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
        let rasgos = rasgos_de_idioma(&self.indice.fragmentos(&contexto.centro).await);
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
                let (escritura, muestra) = rasgos
                    .get(&(texto(&fila, "doc_id"), version))
                    .cloned()
                    .unwrap_or_default();
                fila.insert("escritura_dominante".to_string(), json!(escritura));
                fila.insert("muestra_texto".to_string(), json!(muestra));
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
            "nota": "Prelación: 1 = especificación del cliente … 5 = norma. Ante conflicto prevalece el nivel menor; el resto es supletorio. Para cruzar una tabla usa lookup_double_entry_table con su tabla_id. Un documento con aviso_contenido no está completo: dilo si lo usas. escritura_dominante y muestra_texto (un extracto literal) dejan ver el idioma de cada documento: busca en ese idioma y con sus términos.",
        }))
    }
}

/// Escritura y muestra de texto corrido de un documento, por (doc_id, version_num)
type RasgosDeIdioma = HashMap<(String, i64), (Option<&'static str>, Option<String>)>;

/// Rasgos calculados al vuelo sobre los chunks de 02 ya cargados en el índice, sin tocar el
/// esquema. Se saltan las filas de lectura degradada; la muestra prefiere los bloques que no son
/// preámbulo (portadas e índices) y sigue el orden de páginas
fn rasgos_de_idioma(fragmentos: &[FragmentoIndexado]) -> RasgosDeIdioma {
    let mut por_revision: HashMap<(String, i64), Vec<&FragmentoIndexado>> = HashMap::new();
    for fragmento in fragmentos.iter().filter(|f| {
        f.origen == OrigenFragmento::Chunk
            && f.estado_contenido != Some(EstadoContenido::LecturaDegradada)
    }) {
        por_revision
            .entry((fragmento.doc_id.clone(), i64::from(fragmento.version_num)))
            .or_default()
            .push(fragmento);
    }
    por_revision
        .into_iter()
        .map(|(clave, mut chunks)| {
            chunks.sort_by(|a, b| (a.pagina_inicio, &a.id).cmp(&(b.pagina_inicio, &b.id)));
            let texto_completo: String = chunks
                .iter()
                .map(|c| c.texto.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let escritura = escritura_dominante(&texto_completo).map(|e| e.as_str());
            let cuerpo = chunks
                .iter()
                .filter(|c| c.tipo_bloque != Some(TipoBloque::Preambulo))
                .map(|c| c.texto.as_str());
            let muestra = muestra_de_texto_corrido(cuerpo)
                .or_else(|| muestra_de_texto_corrido(chunks.iter().map(|c| c.texto.as_str())));
            (clave, (escritura, muestra))
        })
        .collect()
}
