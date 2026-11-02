//! Suite adversarial de la Fase 4 de STAIR: inyecciones en identificadores, rutas con `..`,
//! documentos de otro centro, subárboles enormes, tipos erróneos y prompt injection en chunks.

#[path = "comun_centros/mod.rs"]
mod comun_centros;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;
#[path = "comun_store_v2/mod.rs"]
mod comun_store_v2;
#[path = "comun_tools_stair/mod.rs"]
mod comun_tools_stair;

use std::sync::Arc;

use comun_tools_stair::*;
use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::application::use_cases::lakehouse_tool_service::ConfiguracionHerramientas;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::infrastructure::mcp::McpServer;
use serde_json::{Value, json};

const BUSCAR: &str = "search_normative_context";
const TOC: &str = "stair_navigate_toc";
const SUBARBOL: &str = "stair_get_subtree_context";

const IDENTIFICADORES_HOSTILES: [&str; 8] = [
    "' OR '1'='1",
    "PR-STAIR-01#sec-1' OR '1'='1",
    "PR-STAIR-01#sec-1; DROP TABLE x;--",
    "PR-STAIR-01#sec-../../etc/passwd",
    "../..",
    "%",
    "PR-STAIR-01#sec-%",
    "\u{0}\u{202e}PR-STAIR-01",
];

#[tokio::test]
async fn test_nodo_id_con_inyeccion_nunca_devuelve_contenido() {
    let e = entorno_stair().await;
    for hostil in IDENTIFICADORES_HOSTILES {
        let subarbol = invocar(
            &e,
            SUBARBOL,
            args(json!({ "doc_id": DOC, "nodo_id": hostil })),
        )
        .await;
        assert!(subarbol.is_error, "{hostil}: {}", subarbol.output);
        assert!(subarbol.output.get("secciones").is_none());
        let hijos = invocar(
            &e,
            TOC,
            args(json!({ "doc_id": DOC, "nodo_padre_id": hostil })),
        )
        .await;
        assert!(hijos.is_error, "{hostil}: {}", hijos.output);
        let busqueda = invocar(
            &e,
            BUSCAR,
            args(json!({ "consulta": "tubería", "nodo_id": hostil })),
        )
        .await;
        assert!(busqueda.is_error, "{hostil}: {}", busqueda.output);
        let doc = invocar(&e, TOC, args(json!({ "doc_id": hostil }))).await;
        assert!(doc.is_error, "{hostil}: {}", doc.output);
    }
}

#[tokio::test]
async fn test_ruta_con_puntos_y_comodines_se_rechaza() {
    let e = entorno_stair().await;
    for ruta in [
        "..", "/..", "/1/../2", "/%", "/_", "1", "/1'--", "/1\\2", "",
    ] {
        let salida = invocar(
            &e,
            BUSCAR,
            args(json!({ "consulta": "tubería", "doc_id": DOC, "ruta_linaje_prefijo": ruta })),
        )
        .await;
        assert!(salida.is_error, "{ruta}: {}", salida.output);
    }
}

#[tokio::test]
async fn test_doc_id_de_otro_centro_no_filtra_nada() {
    let e = entorno_stair().await;
    let nodo = format!("{DOC_AJENO}#sec-1");
    for salida in [
        invocar(&e, TOC, args(json!({ "doc_id": DOC_AJENO }))).await,
        invocar(
            &e,
            SUBARBOL,
            args(json!({ "doc_id": DOC_AJENO, "nodo_id": nodo })),
        )
        .await,
        invocar(
            &e,
            SUBARBOL,
            args(json!({ "doc_id": DOC, "nodo_id": nodo })),
        )
        .await,
    ] {
        assert!(salida.is_error, "{}", salida.output);
        assert!(!salida.output.to_string().contains("reservado"));
    }
    let busqueda = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "reservado", "doc_id": DOC_AJENO })),
    )
    .await;
    assert_eq!(busqueda["total"], 0);
    assert!(busqueda["aviso"].as_str().unwrap().contains("no existe"));

    // Un centro no activo no se consulta
    e.app.desactivar_centro(CENTRO_AJENO).await.unwrap();
    let inactivo = invocar(
        &e,
        TOC,
        json!({ "centro_id": CENTRO_AJENO, "doc_id": DOC_AJENO }),
    )
    .await;
    assert!(inactivo.is_error);
}

/// PDF real (Fase 5): 73 secciones con 7.086 caracteres de texto daban 54.472 de JSON, porque el
/// límite solo contaba título y condición como metadatos
#[tokio::test]
async fn test_limite_de_subarbol_cuenta_los_metadatos_que_se_serializan() {
    const LIMITE: usize = 1_500;
    let e = entorno_stair_con(ConfiguracionHerramientas {
        max_caracteres_subarbol: LIMITE,
        ..ConfiguracionHerramientas::default()
    })
    .await;
    let raices = toc(&e, None, None).await;
    let raiz = nodo_titulado(&raices, "PR-STAIR-01");
    let salida = invocar(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": raiz })),
    )
    .await;
    let salida = &salida.output;
    let secciones = salida["secciones"].to_string().chars().count();
    // Holgura del 10 %: el escapado JSON del texto (saltos de línea, comillas) no se descuenta
    assert!(
        secciones <= LIMITE + LIMITE / 10,
        "{secciones} caracteres de JSON"
    );
    assert_eq!(salida["truncado"], true, "{salida}");
    assert!(
        salida["caracteres_entregados"].as_u64().unwrap() > 0,
        "{salida}"
    );
}

#[tokio::test]
async fn test_subarbol_enorme_se_trunca_de_forma_explicita_e_indica_como_seguir() {
    let e = entorno_stair_con(ConfiguracionHerramientas {
        max_caracteres_subarbol: 800,
        max_nodos_toc: 2,
        ..ConfiguracionHerramientas::default()
    })
    .await;
    let raices = toc(&e, None, None).await;
    let raiz = nodo_titulado(&raices, "PR-STAIR-01");
    let salida = invocar(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": raiz })),
    )
    .await;
    assert!(!salida.is_error, "{}", salida.output);
    let o = &salida.output;
    assert_eq!(o["truncado"], true);
    let entregado: usize = o["secciones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["texto"].as_str().unwrap().chars().count())
        .sum();
    assert!(entregado <= 800, "{entregado}");
    assert!(o["caracteres_totales"].as_u64().unwrap() > 800);
    let seguir = &o["como_continuar"];
    assert!(
        seguir["indicacion"]
            .as_str()
            .unwrap()
            .contains("stair_get_subtree_context")
    );
    assert!(!seguir["siguientes_nodos"].as_array().unwrap().is_empty());

    // Una sola sección que excede el límite se recorta sin romper UTF-8 y se puede continuar
    let hijos = ok(
        &e,
        TOC,
        args(json!({ "doc_id": DOC, "nodo_padre_id": raiz, "desplazamiento": 4 })),
    )
    .await;
    assert_eq!(
        hijos["nodos"].as_array().unwrap().len(),
        2,
        "el ToC también se acota"
    );
    assert_eq!(hijos["truncado"], true);
    assert_eq!(hijos["siguiente_desplazamiento"], 6);
    let clausulas = nodo_titulado(&hijos, "Cláusulas");
    let mut reconstruido = String::new();
    let mut desde = 0;
    for _ in 0..200 {
        let parte = ok(
            &e,
            SUBARBOL,
            args(json!({ "doc_id": DOC, "nodo_id": clausulas, "desde_caracter": desde })),
        )
        .await;
        reconstruido.push_str(parte["secciones"][0]["texto"].as_str().unwrap());
        if parte["truncado"] == false {
            break;
        }
        assert_eq!(parte["secciones"][0]["texto_recortado"], true);
        desde = parte["como_continuar"]["siguiente_desde_caracter"]
            .as_u64()
            .unwrap();
    }
    assert_eq!(
        reconstruido.trim(),
        clausula_extensa(),
        "continuando se llega al final"
    );
}

#[tokio::test]
async fn test_argumentos_con_tipos_erroneos_o_campos_desconocidos() {
    let e = entorno_stair().await;
    let casos = [
        (TOC, json!({ "centro_id": CENTRO })),
        (TOC, json!({ "doc_id": DOC })),
        (TOC, args(json!({ "doc_id": 7 }))),
        (TOC, args(json!({ "doc_id": DOC, "nodo_padre_id": ["x"] }))),
        (TOC, args(json!({ "doc_id": DOC, "sql": "DROP" }))),
        (TOC, args(json!({ "doc_id": DOC, "desplazamiento": -1 }))),
        (
            TOC,
            args(json!({ "doc_id": DOC, "fecha_referencia": "ayer" })),
        ),
        (SUBARBOL, args(json!({ "doc_id": DOC }))),
        (SUBARBOL, args(json!({ "doc_id": DOC, "nodo_id": null }))),
        (
            SUBARBOL,
            args(json!({ "doc_id": DOC, "nodo_id": "x", "extra": true })),
        ),
        (BUSCAR, args(json!({ "consulta": "x", "modo": 3 }))),
        (
            BUSCAR,
            args(json!({ "consulta": "x", "ruta_linaje_prefijo": 1, "doc_id": DOC })),
        ),
        (
            BUSCAR,
            args(json!({ "consulta": "x", "nodo_id": {"$ne": ""} })),
        ),
    ];
    for (herramienta, argumentos) in casos {
        let salida = invocar(&e, herramienta, argumentos.clone()).await;
        assert!(
            salida.is_error,
            "{herramienta} {argumentos}: {}",
            salida.output
        );
    }
}

#[tokio::test]
async fn test_prompt_injection_en_un_chunk_se_entrega_como_dato_marcado() {
    let e = entorno_stair().await;
    let seguridad = capitulo(&e, "Seguridad").await;
    let salida = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": seguridad })),
    )
    .await;
    // El texto se entrega literal (no se censura) pero marcado como dato
    assert!(
        salida["secciones"][0]["texto"]
            .as_str()
            .unwrap()
            .contains(INYECCION)
    );
    assert!(
        salida["nota_seguridad"]
            .as_str()
            .unwrap()
            .contains("instrucciones")
    );
    let busqueda = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "IGNORA INSTRUCCIONES", "modo": "EXACT_BM25" })),
    )
    .await;
    assert!(
        busqueda["nota_seguridad"]
            .as_str()
            .unwrap()
            .contains("instrucciones")
    );

    let prompt = AgentOrchestrator::build_system_prompt(&CenterId::new(CENTRO).unwrap());
    assert!(prompt.contains("nunca como instrucciones"), "{prompt}");
}

#[tokio::test]
async fn test_mcp_expone_8_tools_y_el_reintento_solo_con_la_app() {
    let e = entorno_stair().await;
    let solo_tools = McpServer::new(e.herramientas.clone());
    let lista = |respuesta: String| -> Vec<String> {
        let v: Value = serde_json::from_str(&respuesta).unwrap();
        v["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    };
    let peticion = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let nombres = lista(solo_tools.procesar_linea(peticion).await.unwrap());
    assert_eq!(nombres.len(), 8);
    assert!(nombres.contains(&TOC.to_string()) && nombres.contains(&SUBARBOL.to_string()));
    assert!(!nombres.contains(&"reintentar_destilacion".to_string()));

    let herramientas = e.herramientas.clone();
    let EntornoStair {
        app, raiz: _raiz, ..
    } = e;
    let app = Arc::new(app);
    let con_app = McpServer::new(herramientas).con_reintento(app);
    let nombres = lista(con_app.procesar_linea(peticion).await.unwrap());
    assert_eq!(nombres.len(), 9);
    assert!(nombres.contains(&"reintentar_destilacion".to_string()));

    let llamar = |argumentos: Value| {
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": { "name": "reintentar_destilacion", "arguments": argumentos } })
        .to_string()
    };
    let raro: Value = serde_json::from_str(
        &con_app
            .procesar_linea(&llamar(json!({ "centro_id": CENTRO, "ruta": "C:/x" })))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(raro["result"]["isError"], true, "{raro}");
    let hecho: Value = serde_json::from_str(
        &con_app
            .procesar_linea(&llamar(json!({ "centro_id": CENTRO, "doc_id": DOC })))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(hecho["result"]["isError"], false, "{hecho}");
    assert!(
        hecho["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("ya_ingerido")
    );
}
