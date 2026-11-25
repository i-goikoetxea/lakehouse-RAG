//! Exhaustividad de la lista de evidencia del arnés (F-1a) contra las salidas reales de las 8
//! herramientas, sobre los centros de prueba en disco de las suites de tools (LanceDB →
//! proyección → tools, con dobles para Claude y FastEmbed).
//!
//! Si una herramienta añade un campo y nadie decide si es evidencia documental o no, este test
//! falla y nombra la ruta de clave. Clasificarlo es editar
//! `src/application/use_cases/arnes_turno/evidencia_herramientas.rs`.

#[path = "comun_centros/mod.rs"]
mod comun_centros;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;
#[path = "comun_store_v2/mod.rs"]
mod comun_store_v2;
#[path = "comun_tools_stair/mod.rs"]
mod comun_tools_stair;
#[path = "comun_tools_v2/mod.rs"]
mod comun_tools_v2;

use std::collections::BTreeSet;

use lakehouse_rag::application::dtos::tools::{LakehouseToolName, ToolResult};
use lakehouse_rag::application::use_cases::arnes_turno::evidencia_herramientas::{
    claves_sin_clasificar, evidencia_de,
};
use lakehouse_rag::application::use_cases::lakehouse_tool_service::ConfiguracionHerramientas;
use serde_json::{Value, json};

/// Cifra que no está en ningún documento de los centros de prueba
const SENUELO: &str = "43719";

/// Salidas reales recogidas, con la herramienta que las produjo
#[derive(Default)]
struct Inventario {
    salidas: Vec<(LakehouseToolName, String, Value)>,
}

impl Inventario {
    fn anotar(&mut self, herramienta: &str, caso: &str, salida: Value) {
        let herramienta = LakehouseToolName::from_str_name(herramienta).unwrap();
        self.salidas.push((herramienta, caso.to_string(), salida));
    }

    /// Claves de primer nivel de todas las salidas de la herramienta
    fn claves_de(&self, herramienta: LakehouseToolName) -> BTreeSet<String> {
        self.salidas
            .iter()
            .filter(|(h, _, _)| *h == herramienta)
            .flat_map(|(_, _, salida)| salida.as_object().unwrap().keys().cloned())
            .collect()
    }
}

async fn salidas_de_las_tools_v2(inventario: &mut Inventario) {
    use comun_tools_v2::{CENTRO_VACIO, FECHA_ANTERIOR_REV04, args, entorno_sembrado, ok};
    let entorno = entorno_sembrado().await;
    let h = &entorno.herramientas;
    let vacio = |mut extra: Value| {
        extra["centro_id"] = json!(CENTRO_VACIO);
        extra
    };
    let casos: Vec<(&str, &str, Value)> = vec![
        ("inspect_lakehouse_catalog", "vigente", args(json!({}))),
        (
            "inspect_lakehouse_catalog",
            "a fecha",
            args(json!({ "fecha_referencia": FECHA_ANTERIOR_REV04 })),
        ),
        (
            "inspect_lakehouse_catalog",
            "centro vacío",
            vacio(json!({})),
        ),
        (
            "query_operational_matrix",
            "todos los orígenes",
            args(json!({ "solo_declarados": false, "fase": "Custodia" })),
        ),
        ("query_operational_matrix", "por defecto", args(json!({}))),
        (
            "query_operational_matrix",
            "con todos los filtros",
            args(json!({
                "fase": "Recepción", "rol": "Jefe de Almacén", "rol_en": "responsable",
                "registro": "FOR-ALM-01", "doc_id": "PO-ALM-03", "objeto": "tubería",
                "fecha_referencia": FECHA_ANTERIOR_REV04,
            })),
        ),
        (
            "query_operational_matrix",
            "sin filas",
            vacio(json!({ "fase": "CUSTODIA" })),
        ),
        (
            "lookup_double_entry_table",
            "resuelto con nota y ejes ambiguos",
            args(json!({ "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42 })),
        ),
        (
            "lookup_double_entry_table",
            "resuelto sin nota",
            args(json!({ "tabla_id": "4-A", "valor_x": 20, "valor_y": 0.42, "eje_z": "Proceso" })),
        ),
        (
            "lookup_double_entry_table",
            "ejes invertidos",
            args(json!({ "tabla_id": "4-A", "valor_x": 0.42, "valor_y": 30 })),
        ),
        (
            "lookup_double_entry_table",
            "por búsqueda y a fecha",
            args(json!({
                "busqueda_tabla": "precalentamiento", "valor_x": 30, "valor_y": 0.42,
                "fecha_referencia": FECHA_ANTERIOR_REV04,
            })),
        ),
        (
            "lookup_double_entry_table",
            "sin coincidencia",
            args(json!({ "tabla_id": "4-A", "valor_x": "categoría ajena", "valor_y": 0.42 })),
        ),
        (
            "lookup_double_entry_table",
            "tabla no encontrada",
            args(json!({ "tabla_id": "9-Z", "valor_x": 1, "valor_y": 1 })),
        ),
        (
            "execute_athena_sql",
            "listado",
            args(json!({ "sql_query": "SELECT * FROM 03_matriz_operativa WHERE vigente = true" })),
        ),
        (
            "execute_athena_sql",
            "agregado a fecha",
            args(json!({
                "sql_query": "SELECT fase_operativa, COUNT(*) AS n FROM 03_matriz_operativa GROUP BY fase_operativa",
                "fecha_referencia": FECHA_ANTERIOR_REV04,
            })),
        ),
        (
            "execute_athena_sql",
            "truncado",
            args(
                json!({ "sql_query": "SELECT a.chunk_id FROM 02_chunks_normativa a, 02_chunks_normativa b, 02_chunks_normativa c" }),
            ),
        ),
        (
            "traverse_normative_graph",
            "con hub",
            args(json!({ "nodo_origen": "PO-ALM-03" })),
        ),
        (
            "traverse_normative_graph",
            "desde el hub",
            args(json!({ "nodo_origen": "NORMA-HUB", "max_vecinos": 3 })),
        ),
        (
            "traverse_normative_graph",
            "con filtros y a fecha",
            args(json!({
                "nodo_origen": "PO-ALM-03", "tipo_relacion": ["EXIGE_LLENAR_REGISTRO"],
                "max_saltos": 1, "fecha_referencia": FECHA_ANTERIOR_REV04,
            })),
        ),
        (
            "traverse_normative_graph",
            "nodo no encontrado",
            args(json!({ "nodo_origen": "PO-ALM" })),
        ),
        (
            "search_normative_context",
            "con conflicto de prelación",
            args(json!({ "consulta": "porcentaje de soldaduras a tope que se deben ensayar" })),
        ),
        (
            "search_normative_context",
            "con filtros",
            args(json!({
                "consulta": "calentar la zona de soldadura", "top_k": 3, "modo": "EXACT_BM25",
                "nivel_prelacion_max": 2, "fecha_referencia": FECHA_ANTERIOR_REV04,
            })),
        ),
        (
            "search_normative_context",
            "documento inexistente",
            args(json!({ "consulta": "soldadura", "doc_id": "NO-EXISTE" })),
        ),
        (
            "search_normative_context",
            "sin resultados",
            vacio(json!({ "consulta": "soldadura" })),
        ),
    ];
    for (herramienta, caso, argumentos) in casos {
        let salida = ok(h, herramienta, argumentos).await;
        inventario.anotar(herramienta, caso, salida);
    }
}

async fn salidas_de_las_tools_stair(inventario: &mut Inventario) {
    use comun_tools_stair::{
        DOC, FECHA_V1, args, capitulo, entorno_stair_con, ok, raiz_procedimiento,
    };
    // Límites pequeños: las respuestas truncadas traen los campos para continuar
    let entorno = entorno_stair_con(ConfiguracionHerramientas {
        max_nodos_toc: 7,
        max_caracteres_subarbol: 2_000,
        ..ConfiguracionHerramientas::default()
    })
    .await;
    let paginado = comun_pdf::pdf(&[
        &[
            "1 Alcance general",
            "Texto suficiente para que la página cuente como extraíble por el extractor",
        ],
        &[
            "Texto suficiente para que la página cuente como extraíble por el extractor",
            "Frase con el termino zanahoria en la segunda pagina",
        ],
    ]);
    let ruta = entorno.escribir("informe_paginado.pdf", &paginado);
    entorno
        .app
        .ingestar(&ruta, comun_tools_stair::CENTRO)
        .await
        .unwrap();

    let raiz = raiz_procedimiento(&entorno).await;
    let custodia = capitulo(&entorno, "Custodia").await;
    let clausulas = capitulo(&entorno, "Cláusulas").await;
    let casos: Vec<(&str, &str, Value)> = vec![
        (
            "stair_navigate_toc",
            "raíces",
            args(json!({ "doc_id": DOC })),
        ),
        (
            "stair_navigate_toc",
            "hijos truncados",
            args(json!({ "doc_id": DOC, "nodo_padre_id": raiz })),
        ),
        (
            "stair_navigate_toc",
            "con desplazamiento y a fecha",
            args(json!({
                "doc_id": DOC, "nodo_padre_id": raiz, "desplazamiento": 3,
                "fecha_referencia": FECHA_V1,
            })),
        ),
        (
            "stair_get_subtree_context",
            "con condiciones heredadas",
            args(json!({ "doc_id": DOC, "nodo_id": custodia })),
        ),
        (
            "stair_get_subtree_context",
            "truncado",
            args(json!({ "doc_id": DOC, "nodo_id": clausulas })),
        ),
        (
            "stair_get_subtree_context",
            "documento entero, continuado",
            args(json!({ "doc_id": DOC, "nodo_id": raiz, "desde_caracter": 40 })),
        ),
        (
            "search_normative_context",
            "con provisional y sin texto",
            args(json!({ "consulta": "carretillas bocina", "modo": "EXACT_BM25" })),
        ),
        (
            "search_normative_context",
            "con páginas",
            args(json!({ "consulta": "zanahoria", "modo": "EXACT_BM25" })),
        ),
        (
            "search_normative_context",
            "restringida a una sección",
            args(json!({ "consulta": "durmientes", "doc_id": DOC, "nodo_id": custodia })),
        ),
        (
            "inspect_lakehouse_catalog",
            "con provisional y escaneado",
            args(json!({})),
        ),
    ];
    for (herramienta, caso, argumentos) in casos {
        let salida = ok(&entorno, herramienta, argumentos).await;
        inventario.anotar(herramienta, caso, salida);
    }
    // Índice y lectura del documento paginado y del provisional
    let catalogo = ok(&entorno, "inspect_lakehouse_catalog", args(json!({}))).await;
    for documento in catalogo["documentos"].as_array().unwrap() {
        let doc_id = documento["doc_id"].as_str().unwrap();
        if doc_id == DOC {
            continue;
        }
        let indice = ok(
            &entorno,
            "stair_navigate_toc",
            args(json!({ "doc_id": doc_id })),
        )
        .await;
        if let Some(nodo) = indice["nodos"][0]["nodo_id"].as_str() {
            let lectura = ok(
                &entorno,
                "stair_get_subtree_context",
                args(json!({ "doc_id": doc_id, "nodo_id": nodo })),
            )
            .await;
            inventario.anotar("stair_get_subtree_context", doc_id, lectura);
        }
        inventario.anotar("stair_navigate_toc", doc_id, indice);
    }
}

#[tokio::test]
async fn test_toda_clave_de_las_salidas_reales_esta_clasificada_como_evidencia_o_no() {
    let mut inventario = Inventario::default();
    salidas_de_las_tools_v2(&mut inventario).await;
    salidas_de_las_tools_stair(&mut inventario).await;

    for herramienta in LakehouseToolName::TODAS {
        assert!(
            !inventario.claves_de(herramienta).is_empty(),
            "sin ninguna salida real de {herramienta:?}"
        );
    }
    let sin_clasificar: Vec<String> = inventario
        .salidas
        .iter()
        .flat_map(|(herramienta, caso, salida)| {
            claves_sin_clasificar(*herramienta, salida)
                .into_iter()
                .map(move |ruta| format!("{} · {caso} · {ruta}", herramienta.as_str()))
        })
        .collect();
    assert!(
        sin_clasificar.is_empty(),
        "claves sin clasificar como evidencia o no evidencia:\n{}",
        sin_clasificar.join("\n")
    );

    // Las variantes que el test debe haber ejercitado de verdad, no solo las salidas felices
    let variantes: Vec<(LakehouseToolName, &[&str])> = vec![
        (
            LakehouseToolName::LookupDoubleEntryTable,
            &[
                "ejes_ambiguos",
                "rangos_disponibles",
                "tablas",
                "nota_pie_condicion",
                "cita",
            ],
        ),
        (LakehouseToolName::ExecuteAthenaSql, &["aviso", "resultado"]),
        (
            LakehouseToolName::TraverseNormativeGraph,
            &["sugerencias", "nodos", "aristas"],
        ),
        (
            LakehouseToolName::QueryOperationalMatrix,
            &["accion_sugerida", "filas", "filtros"],
        ),
        (
            LakehouseToolName::StairNavigateToc,
            &["siguiente_desplazamiento", "nodos"],
        ),
        (
            LakehouseToolName::StairGetSubtreeContext,
            &["como_continuar", "secciones", "condiciones_heredadas"],
        ),
    ];
    for (herramienta, esperadas) in variantes {
        let presentes = inventario.claves_de(herramienta);
        for clave in esperadas {
            assert!(
                presentes.contains(*clave),
                "ninguna salida real de {herramienta:?} trae `{clave}`"
            );
        }
    }
}

fn resultado_de(herramienta: &str, salida: Value) -> ToolResult {
    ToolResult {
        call_id: "real".into(),
        tool_name: herramienta.into(),
        output: salida,
        is_error: false,
    }
}

/// El caso de la auditoría con las herramientas reales: la cifra que el modelo escribe en sus
/// argumentos vuelve en la salida, pero no en su evidencia
#[tokio::test]
async fn test_la_cifra_de_los_argumentos_vuelve_en_la_salida_real_pero_no_en_su_evidencia() {
    use comun_tools_v2::{args, entorno_sembrado, ok};
    let entorno = entorno_sembrado().await;
    let casos: Vec<(&str, Value)> = vec![
        (
            "search_normative_context",
            args(json!({ "consulta": format!("temperatura de precalentamiento {SENUELO}") })),
        ),
        (
            "execute_athena_sql",
            args(json!({
                "sql_query": format!("SELECT titulo FROM 01_documentos_catalogo WHERE version_num < {SENUELO}")
            })),
        ),
        (
            "lookup_double_entry_table",
            args(json!({ "tabla_id": "4-A", "valor_x": 43719, "valor_y": 0.42 })),
        ),
        (
            "query_operational_matrix",
            args(json!({ "registro": format!("FOR-{SENUELO}") })),
        ),
        (
            "traverse_normative_graph",
            args(json!({ "nodo_origen": format!("DOC-{SENUELO}") })),
        ),
    ];
    for (herramienta, argumentos) in casos {
        let salida = ok(&entorno.herramientas, herramienta, argumentos).await;
        assert!(
            salida.to_string().contains(SENUELO),
            "{herramienta}: la salida ya no repite el argumento; revisa el caso"
        );
        let evidencia = evidencia_de(&resultado_de(herramienta, salida));
        assert!(
            !evidencia.to_string().contains(SENUELO),
            "{herramienta}: el eco del argumento sigue en la evidencia: {evidencia}"
        );
    }
}

/// Lo que sí es documento sigue en la evidencia de las salidas reales
#[tokio::test]
async fn test_el_contenido_documental_de_las_salidas_reales_sigue_en_la_evidencia() {
    use comun_tools_v2::{NOTA_ASTERISCO, args, entorno_sembrado, ok};
    let entorno = entorno_sembrado().await;
    let casos: Vec<(&str, Value, &[&str])> = vec![
        (
            "search_normative_context",
            args(json!({ "consulta": "calentar la zona de soldadura a 80 °C", "top_k": 3 })),
            &["80 °C", "< 0 °C", "Rev. 04", "PO-ALM-03"],
        ),
        (
            "lookup_double_entry_table",
            args(json!({ "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42 })),
            &[
                "125 (*)",
                NOTA_ASTERISCO,
                "20 < x <= 40",
                "espesor_nominal_mm",
                "Rev. 04",
            ],
        ),
        (
            "query_operational_matrix",
            args(json!({ "solo_declarados": false, "fase": "Custodia" })),
            &["Jefe de Producción", "FOR-ALM-05", "7 días"],
        ),
        (
            "execute_athena_sql",
            args(
                json!({ "sql_query": "SELECT DISTINCT registro_codigo FROM 03_matriz_operativa WHERE vigente = true" }),
            ),
            &["FOR-ALM-05", "PPI-MAT-09"],
        ),
        (
            "traverse_normative_graph",
            args(json!({ "nodo_origen": "PO-ALM-03" })),
            &["FOR-ALM-05", "DEROGA_A", "NORMA-HUB"],
        ),
        (
            "inspect_lakehouse_catalog",
            args(json!({})),
            &["PCC-01", "Tabla 4-A", "Rev. 04", "2026-09-15"],
        ),
    ];
    for (herramienta, argumentos, esperados) in casos {
        let salida = ok(&entorno.herramientas, herramienta, argumentos).await;
        let evidencia = evidencia_de(&resultado_de(herramienta, salida)).to_string();
        for esperado in esperados {
            assert!(
                evidencia.contains(esperado),
                "{herramienta}: falta «{esperado}» en la evidencia: {evidencia}"
            );
        }
    }
}
