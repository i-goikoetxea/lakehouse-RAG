//! Extrae de las salidas reales de las tools lo que pinta la ventana: citas (doc_id +
//! revisión + breadcrumb, con su condición y nota al pie), `matrixResult` (filas de la matriz o
//! resultado del lookup) y `graphResult` (nodos y aristas del recorrido).
//!
//! Formas de salida (Fase 13): matriz → `filas[].cita`; lookup → `estado`,
//! `nota_pie_condicion`, `cita` (o `candidatas[].cita`); grafo → `nodos[]`, `aristas[].cita`;
//! búsqueda → `grupos[].resultados[].cita`; SQL → `resultado[]` (sin citas).
//! Todo el texto se entrega tal cual: el frontend lo escapa siempre al pintarlo.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::application::dtos::tools::LakehouseToolName;
use crate::presentation::gui::ToolCardDto;

/// Máximo de citas por respuesta (un grafo puede traer decenas de aristas)
pub const MAX_CITAS: usize = 30;

/// Cita verificable de un dato devuelto por una tool
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationDto {
    pub doc_id: String,
    pub revision: Option<String>,
    pub version_num: Option<i64>,
    pub breadcrumb: Option<String>,
    /// Chunk, celda o arista de origen
    pub fragmento_id: Option<String>,
    pub herramienta: String,
    pub condicion_aplicacion: Option<String>,
    pub nota_pie: Option<String>,
}

/// Citas de todas las tools que terminaron bien, sin duplicados y acotadas a `MAX_CITAS`
pub fn extraer_citas(tarjetas: &[ToolCardDto]) -> Vec<CitationDto> {
    let mut citas = Vec::new();
    for tarjeta in tarjetas.iter().filter(|t| t.status == "ok") {
        recoger_citas(&tarjeta.result, &tarjeta.name, &mut citas);
    }
    let mut vistas = HashSet::new();
    citas.retain(|c: &CitationDto| {
        vistas.insert((
            c.doc_id.clone(),
            c.revision.clone(),
            c.breadcrumb.clone(),
            c.fragmento_id.clone(),
        ))
    });
    citas.truncate(MAX_CITAS);
    citas
}

/// Una fuente (documento + revisión) con todas sus citas: un solo chip por fuente en el chat
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FuenteCitadaDto {
    pub doc_id: String,
    pub revision: Option<String>,
    pub version_num: Option<i64>,
    /// Apartados (breadcrumb) distintos, en orden de aparición
    pub apartados: Vec<String>,
    pub citas: Vec<CitationDto>,
}

/// Agrupa las citas por documento + revisión conservando el orden de aparición
pub fn agrupar_fuentes(citas: &[CitationDto]) -> Vec<FuenteCitadaDto> {
    let mut fuentes: Vec<FuenteCitadaDto> = Vec::new();
    for cita in citas {
        let indice = match fuentes
            .iter()
            .position(|f| f.doc_id == cita.doc_id && f.revision == cita.revision)
        {
            Some(indice) => indice,
            None => {
                fuentes.push(FuenteCitadaDto {
                    doc_id: cita.doc_id.clone(),
                    revision: cita.revision.clone(),
                    version_num: cita.version_num,
                    apartados: Vec::new(),
                    citas: Vec::new(),
                });
                fuentes.len() - 1
            }
        };
        let fuente = &mut fuentes[indice];
        if let Some(apartado) = &cita.breadcrumb
            && !fuente.apartados.contains(apartado)
        {
            fuente.apartados.push(apartado.clone());
        }
        fuente.citas.push(cita.clone());
    }
    fuentes
}

/// Línea legible del resultado de una tool para su tarjeta en el chat
pub fn resumen_tool(nombre: &str, salida: &Value, estado: &str) -> String {
    if estado != "ok" {
        let mensaje = salida["error"].as_str().unwrap_or("sin detalle");
        return format!("error · {}", recortar(mensaje, 120));
    }
    let cuenta = |campo: &str| salida[campo].as_array().map_or(0, Vec::len);
    let herramienta = LakehouseToolName::TODAS
        .iter()
        .find(|t| t.as_str() == nombre)
        .copied();
    let partes: Vec<String> = match herramienta {
        Some(LakehouseToolName::QueryOperationalMatrix) => {
            let total = salida["total"]
                .as_u64()
                .map_or(cuenta("filas"), |n| n as usize);
            let mut fases: Vec<String> = Vec::new();
            for fila in salida["filas"].as_array().into_iter().flatten() {
                if let Some(fase) = fila["fase_operativa"].as_str()
                    && !fases.iter().any(|f| f == fase)
                {
                    fases.push(fase.to_string());
                }
            }
            let mut partes = vec![plural(total, "fila", "filas")];
            if !fases.is_empty() {
                partes.push(fases.join(", "));
            }
            partes
        }
        Some(LakehouseToolName::LookupDoubleEntryTable) => {
            let estado = salida["estado"]
                .as_str()
                .unwrap_or("sin estado")
                .replace('_', " ");
            match salida["valor_resultado_texto"].as_str() {
                Some(valor) if estado == "resuelto" => {
                    let unidad = salida["unidad_medida"]
                        .as_str()
                        .map(|u| format!(" {u}"))
                        .unwrap_or_default();
                    vec![estado, format!("{valor}{unidad}")]
                }
                _ => vec![estado],
            }
        }
        Some(LakehouseToolName::TraverseNormativeGraph) if salida["nodos"].is_array() => {
            let mut partes = vec![
                plural(cuenta("nodos"), "nodo", "nodos"),
                plural(cuenta("aristas"), "arista", "aristas"),
            ];
            if salida["truncado"].as_bool() == Some(true) {
                partes.push("truncado".to_string());
            }
            partes
        }
        Some(LakehouseToolName::SearchNormativeContext) => {
            let grupos = salida["grupos"].as_array().cloned().unwrap_or_default();
            let fragmentos: usize = grupos
                .iter()
                .map(|g| g["resultados"].as_array().map_or(0, Vec::len))
                .sum();
            let mut partes = vec![
                plural(grupos.len(), "tema", "temas"),
                plural(fragmentos, "fragmento", "fragmentos"),
            ];
            if grupos
                .iter()
                .any(|g| g["conflicto_prelacion"].as_bool() == Some(true))
            {
                partes.push("conflicto de prelación".to_string());
            }
            partes
        }
        Some(LakehouseToolName::ExecuteAthenaSql) => {
            let filas = salida["filas"]
                .as_u64()
                .map_or(cuenta("resultado"), |n| n as usize);
            let mut partes = vec![plural(filas, "fila", "filas")];
            if salida["truncado"].as_bool() == Some(true) {
                partes.push("truncado".to_string());
            }
            partes
        }
        Some(LakehouseToolName::InspectLakehouseCatalog) => vec![
            plural(cuenta("documentos"), "documento", "documentos"),
            plural(cuenta("tablas_doble_entrada"), "tabla", "tablas"),
        ],
        _ => vec![salida["estado"].as_str().unwrap_or("ok").replace('_', " ")],
    };
    partes.join(" · ")
}

fn plural(n: usize, singular: &str, plural: &str) -> String {
    format!("{n} {}", if n == 1 { singular } else { plural })
}

fn recortar(texto: &str, maximo: usize) -> String {
    if texto.chars().count() <= maximo {
        return texto.to_string();
    }
    format!("{}…", texto.chars().take(maximo).collect::<String>())
}

/// Última salida de la matriz o del lookup, etiquetada con `tipo`
pub fn extraer_matrix_result(tarjetas: &[ToolCardDto]) -> Option<Value> {
    let matriz = LakehouseToolName::QueryOperationalMatrix.as_str();
    let lookup = LakehouseToolName::LookupDoubleEntryTable.as_str();
    let tarjeta = tarjetas
        .iter()
        .rev()
        .find(|t| t.status == "ok" && (t.name == matriz || t.name == lookup))?;
    let salida = &tarjeta.result;
    if tarjeta.name == matriz {
        Some(json!({
            "tipo": "matriz",
            "fechaReferencia": salida["fecha_referencia"],
            "total": salida["total"],
            "filas": salida["filas"],
        }))
    } else {
        let mut resultado = salida.clone();
        if let Some(objeto) = resultado.as_object_mut() {
            objeto.insert("tipo".to_string(), json!("lookup"));
        }
        Some(resultado)
    }
}

/// Último recorrido del grafo con nodos
pub fn extraer_graph_result(tarjetas: &[ToolCardDto]) -> Option<Value> {
    let grafo = LakehouseToolName::TraverseNormativeGraph.as_str();
    let tarjeta = tarjetas
        .iter()
        .rev()
        .find(|t| t.status == "ok" && t.name == grafo && t.result["nodos"].is_array())?;
    let salida = &tarjeta.result;
    Some(json!({
        "nodoOrigen": salida["nodo_origen"],
        "fechaReferencia": salida["fecha_referencia"],
        "truncado": salida["truncado"],
        "nodos": salida["nodos"],
        "aristas": salida["aristas"],
    }))
}

/// Recorre la salida y convierte cada objeto con `cita` en una `CitationDto`
fn recoger_citas(valor: &Value, herramienta: &str, destino: &mut Vec<CitationDto>) {
    match valor {
        Value::Object(objeto) => {
            if let Some(cita) = objeto
                .get("cita")
                .and_then(|c| cita_desde(c, valor, herramienta))
            {
                destino.push(cita);
            }
            for (clave, hijo) in objeto {
                if clave != "cita" {
                    recoger_citas(hijo, herramienta, destino);
                }
            }
        }
        Value::Array(elementos) => {
            for elemento in elementos {
                recoger_citas(elemento, herramienta, destino);
            }
        }
        _ => {}
    }
}

fn cita_desde(cita: &Value, contenedor: &Value, herramienta: &str) -> Option<CitationDto> {
    let doc_id = texto(cita, "doc_id")?;
    Some(CitationDto {
        doc_id,
        revision: texto(cita, "revision"),
        version_num: cita["version_num"].as_i64(),
        breadcrumb: texto(cita, "breadcrumb").or_else(|| texto(contenedor, "breadcrumb_jerarquia")),
        fragmento_id: ["chunk_origen_id", "celda_id", "edge_id"]
            .iter()
            .find_map(|campo| texto(cita, campo)),
        herramienta: herramienta.to_string(),
        condicion_aplicacion: texto(contenedor, "condicion_aplicacion"),
        nota_pie: texto(contenedor, "nota_pie_condicion"),
    })
}

fn texto(valor: &Value, campo: &str) -> Option<String> {
    valor[campo]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tarjeta(nombre: &str, resultado: Value, status: &str) -> ToolCardDto {
        ToolCardDto {
            name: nombre.to_string(),
            args: json!({}),
            result: resultado,
            status: status.to_string(),
            duration: "1ms".to_string(),
            resumen: String::new(),
        }
    }

    #[test]
    fn test_fuentes_agrupan_por_documento_y_revision_en_orden_de_aparicion() {
        let aristas: Vec<Value> = (0..10)
            .map(|i| json!({ "cita": { "doc_id": "PO-ALM-03", "revision": "Rev. 04", "edge_id": format!("E{i}") } }))
            .collect();
        let citas = extraer_citas(&[
            tarjeta("query_operational_matrix", salida_matriz(), "ok"),
            tarjeta(
                "traverse_normative_graph",
                json!({ "aristas": aristas }),
                "ok",
            ),
            tarjeta(
                "lookup_double_entry_table",
                json!({ "cita": { "doc_id": "NT-SOL-01", "revision": "Rev. 02" } }),
                "ok",
            ),
        ]);
        let fuentes = agrupar_fuentes(&citas);
        assert_eq!(fuentes.len(), 2, "{fuentes:?}");
        assert_eq!(
            (fuentes[0].doc_id.as_str(), fuentes[0].revision.as_deref()),
            ("PO-ALM-03", Some("Rev. 04"))
        );
        assert_eq!(fuentes[0].citas.len(), 11);
        assert_eq!(
            fuentes[0].apartados,
            vec!["2. Desarrollo > 2.2 Fase 2 — CUSTODIA".to_string()]
        );
        assert_eq!(fuentes[1].doc_id, "NT-SOL-01");
    }

    #[test]
    fn test_resumen_legible_de_cada_tool() {
        let casos = [
            (
                "query_operational_matrix",
                salida_matriz(),
                "ok",
                "1 fila · CUSTODIA",
            ),
            (
                "lookup_double_entry_table",
                json!({ "estado": "resuelto", "valor_resultado_texto": "125 (*)", "unidad_medida": "°C" }),
                "ok",
                "resuelto · 125 (*) °C",
            ),
            (
                "lookup_double_entry_table",
                json!({ "estado": "tabla_no_encontrada" }),
                "ok",
                "tabla no encontrada",
            ),
            (
                "traverse_normative_graph",
                json!({ "estado": "ok", "nodos": [1, 2, 3], "aristas": [1, 2], "truncado": true }),
                "ok",
                "3 nodos · 2 aristas · truncado",
            ),
            (
                "search_normative_context",
                json!({ "grupos": [{ "conflicto_prelacion": true, "resultados": [1, 2] }, { "resultados": [3] }] }),
                "ok",
                "2 temas · 3 fragmentos · conflicto de prelación",
            ),
            (
                "execute_athena_sql",
                json!({ "filas": 1000, "truncado": true }),
                "ok",
                "1000 filas · truncado",
            ),
            (
                "inspect_lakehouse_catalog",
                json!({ "documentos": [1, 2], "tablas_doble_entrada": [1] }),
                "ok",
                "2 documentos · 1 tabla",
            ),
            (
                "execute_athena_sql",
                json!({ "error": "SQL no permitida: solo SELECT" }),
                "error",
                "error · SQL no permitida: solo SELECT",
            ),
        ];
        for (nombre, salida, estado, esperado) in casos {
            assert_eq!(resumen_tool(nombre, &salida, estado), esperado, "{nombre}");
        }
    }

    fn salida_matriz() -> Value {
        json!({
            "centro_id": "obra", "fecha_referencia": null, "total": 1,
            "filas": [{
                "fase_operativa": "CUSTODIA", "rol_responsable": "Jefe de Producción",
                "registro_codigo": "FOR-ALM-05", "condicion_aplicacion": "acopio > 7 días",
                "cita": { "doc_id": "PO-ALM-03", "revision": "Rev. 04", "version_num": 4,
                          "chunk_origen_id": "PO-ALM-03@v4-CHK-0003",
                          "breadcrumb": "2. Desarrollo > 2.2 Fase 2 — CUSTODIA" }
            }]
        })
    }

    fn salida_lookup() -> Value {
        json!({
            "estado": "resuelto", "valor_resultado_texto": "125 (*)",
            "nota_pie_condicion": "(*) Requiere aprobación del Inspector QA/QC.",
            "tabla": { "tabla_id": "PO-ALM-03@v4-TAB-0000", "titulo_tabla": "Tabla 4-A — Temperatura" },
            "cita": { "doc_id": "PO-ALM-03", "revision": "Rev. 04", "version_num": 4,
                      "tabla_id": "PO-ALM-03@v4-TAB-0000", "celda_id": "PO-ALM-03@v4-CEL-0004" }
        })
    }

    #[test]
    fn test_citas_de_matriz_lookup_y_busqueda_con_condicion_y_nota() {
        let busqueda = json!({ "grupos": [{ "tema": "PRECALENTAMIENTO", "resultados": [{
            "breadcrumb_jerarquia": "4. Soldeo > 4.1 Bajo cero",
            "condicion_aplicacion": "bajo cero (< 0 °C)",
            "cita": { "doc_id": "NT-SOL-01", "revision": "Rev. 02", "version_num": 2, "chunk_origen_id": "NT@v2-CHK-1" }
        }]}]});
        let citas = extraer_citas(&[
            tarjeta("query_operational_matrix", salida_matriz(), "ok"),
            tarjeta("lookup_double_entry_table", salida_lookup(), "ok"),
            tarjeta("search_normative_context", busqueda, "ok"),
        ]);
        assert_eq!(citas.len(), 3);
        assert_eq!(
            citas[0].breadcrumb.as_deref(),
            Some("2. Desarrollo > 2.2 Fase 2 — CUSTODIA")
        );
        assert_eq!(
            citas[0].condicion_aplicacion.as_deref(),
            Some("acopio > 7 días")
        );
        assert_eq!(
            citas[1].fragmento_id.as_deref(),
            Some("PO-ALM-03@v4-CEL-0004")
        );
        assert_eq!(
            citas[1].nota_pie.as_deref(),
            Some("(*) Requiere aprobación del Inspector QA/QC.")
        );
        assert_eq!(
            citas[2].breadcrumb.as_deref(),
            Some("4. Soldeo > 4.1 Bajo cero")
        );
        assert_eq!(citas[2].herramienta, "search_normative_context");
    }

    #[test]
    fn test_citas_ignoran_tools_con_error_duplicados_y_se_acotan() {
        let aristas: Vec<Value> = (0..50)
            .map(|i| {
                json!({ "origen": "A", "destino": format!("D{i}"),
                "cita": { "doc_id": "A", "revision": "Rev. 01", "edge_id": format!("E{i}") } })
            })
            .collect();
        let citas = extraer_citas(&[
            tarjeta("query_operational_matrix", salida_matriz(), "error"),
            tarjeta(
                "traverse_normative_graph",
                json!({ "aristas": aristas }),
                "ok",
            ),
            tarjeta("query_operational_matrix", salida_matriz(), "ok"),
            tarjeta("query_operational_matrix", salida_matriz(), "ok"),
        ]);
        assert_eq!(citas.len(), MAX_CITAS);
        assert!(citas.iter().all(|c| c.doc_id == "A"));
        let sin_grafo = extraer_citas(&[
            tarjeta("query_operational_matrix", salida_matriz(), "ok"),
            tarjeta("query_operational_matrix", salida_matriz(), "ok"),
        ]);
        assert_eq!(sin_grafo.len(), 1, "duplicados fuera");
    }

    #[test]
    fn test_matrix_result_toma_la_ultima_matriz_o_lookup_correcta() {
        assert!(extraer_matrix_result(&[]).is_none());
        let matriz =
            extraer_matrix_result(&[tarjeta("query_operational_matrix", salida_matriz(), "ok")])
                .unwrap();
        assert_eq!(matriz["tipo"], "matriz");
        assert_eq!(matriz["filas"][0]["registro_codigo"], "FOR-ALM-05");

        let lookup = extraer_matrix_result(&[
            tarjeta("query_operational_matrix", salida_matriz(), "ok"),
            tarjeta("lookup_double_entry_table", salida_lookup(), "ok"),
            tarjeta(
                "lookup_double_entry_table",
                json!({ "error": "x" }),
                "error",
            ),
        ])
        .unwrap();
        assert_eq!(lookup["tipo"], "lookup");
        assert_eq!(lookup["valor_resultado_texto"], "125 (*)");
        assert_eq!(
            lookup["nota_pie_condicion"],
            "(*) Requiere aprobación del Inspector QA/QC."
        );
    }

    #[test]
    fn test_graph_result_con_nodos_aristas_y_hub_resumido() {
        let salida = json!({
            "estado": "ok", "nodo_origen": "PO-ALM-03", "truncado": false,
            "nodos": [{ "id": "PO-ALM-03", "salto": 0 }, { "id": "HUB", "salto": 1, "hub_resumido": true,
                        "resumen_hub": { "grado": 1000 } }],
            "aristas": [{ "origen": "PO-ALM-03", "tipo_relacion": "REFERENCIA_A", "destino": "HUB",
                          "cita": { "doc_id": "PO-ALM-03", "revision": "Rev. 04" } }]
        });
        let sin_nodos = json!({ "estado": "nodo_no_encontrado", "sugerencias": [] });
        let grafo = extraer_graph_result(&[
            tarjeta("traverse_normative_graph", salida, "ok"),
            tarjeta("traverse_normative_graph", sin_nodos, "ok"),
        ])
        .unwrap();
        assert_eq!(grafo["nodoOrigen"], "PO-ALM-03");
        assert_eq!(grafo["nodos"][1]["hub_resumido"], true);
        assert_eq!(grafo["aristas"][0]["tipo_relacion"], "REFERENCIA_A");
    }
}
