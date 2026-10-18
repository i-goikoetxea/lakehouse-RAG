//! Integración de la Fase 14: comandos de la ventana sobre `LakehouseApp` (disco real,
//! destilador y agente simulados) y mapeo de citas, `matrixResult` y `graphResult` desde las
//! salidas reales de las tools.

mod comun_centros;
mod comun_store_v2;

use std::sync::{Arc, Mutex};

use comun_centros::*;
use comun_store_v2::{ARCHIVO_V4, paquete_revision};
use lakehouse_rag::application::dtos::agent::AgentTurnOutput;
use lakehouse_rag::application::dtos::ingestion::{EtapaIngesta, ObservadorIngesta};
use lakehouse_rag::application::dtos::tools::{ToolCall, ToolResult};
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::infrastructure::llm::{MockAgentRunner, MockDistiller};
use lakehouse_rag::presentation::gui::chat_service::GuiChatService;
use lakehouse_rag::presentation::gui::comandos;
use serde_json::{Value, json};

const CENTRO: &str = "obra-gui-01";

fn observador_en(destino: &Arc<Mutex<Vec<EtapaIngesta>>>) -> ObservadorIngesta {
    let destino = Arc::clone(destino);
    Arc::new(move |etapa| destino.lock().unwrap().push(etapa))
}

async fn centro_activo_con_revisiones(entorno: &Entorno) {
    let (v3, v4) = entorno.escribir_revisiones();
    comandos::crear_centro(&entorno.app, CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    comandos::activar_centro(&entorno.app, CENTRO)
        .await
        .unwrap();
    for ruta in [v3, v4] {
        comandos::ingestar_documento(&entorno.app, &ruta.to_string_lossy(), CENTRO, None)
            .await
            .unwrap();
    }
}

async fn herramienta(
    herramientas: &Arc<dyn ToolServerPort>,
    nombre: &str,
    args: Value,
) -> ToolResult {
    herramientas
        .ejecutar_herramienta(&ToolCall {
            id: "t".into(),
            name: nombre.into(),
            arguments: args,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn test_comandos_ciclo_crear_activar_ingerir_catalogo_integridad_y_eliminar() {
    let entorno = entorno();
    let (v3, v4) = entorno.escribir_revisiones();

    let creado = comandos::crear_centro(
        &entorno.app,
        CENTRO,
        &entorno.ruta_centro_texto("a"),
        Some("Obra GUI"),
    )
    .await
    .unwrap();
    assert!(!creado.activo);
    assert_eq!(creado.nombre, "Obra GUI");
    let activacion = comandos::activar_centro(&entorno.app, CENTRO)
        .await
        .unwrap();
    assert_eq!((activacion.vistas, activacion.filas), (6, 0));
    assert!(comandos::listar_centros(&entorno.app).await.unwrap()[0].activo);

    let etapas: Arc<Mutex<Vec<EtapaIngesta>>> = Arc::default();
    let r3 = comandos::ingestar_documento(
        &entorno.app,
        &v3.to_string_lossy(),
        CENTRO,
        Some(observador_en(&etapas)),
    )
    .await
    .unwrap();
    assert_eq!(
        *etapas.lock().unwrap(),
        vec![
            EtapaIngesta::Copia,
            EtapaIngesta::Parseo,
            EtapaIngesta::Destilacion,
            EtapaIngesta::Embeddings,
            EtapaIngesta::Escritura,
            EtapaIngesta::Proyeccion
        ]
    );
    assert_eq!((r3.doc_id.as_str(), r3.version), ("PO-ALM-03", 3));
    assert!(r3.proyectado && r3.aviso.is_none());
    let r4 = comandos::ingestar_documento(&entorno.app, &v4.to_string_lossy(), CENTRO, None)
        .await
        .unwrap();
    assert_eq!(r4.versiones_derogadas, vec![3]);
    assert_eq!(
        r4.archivo, ARCHIVO_V4,
        "solo el nombre, no la ruta completa"
    );

    let catalogo = comandos::catalogo_centro(&entorno.app, CENTRO)
        .await
        .unwrap();
    assert_eq!(catalogo.len(), 2, "vigente y derogada: {catalogo:?}");
    let (vigente, derogada) = (&catalogo[0], &catalogo[1]);
    assert!(vigente.vigente && vigente.revision == "Rev. 04");
    assert_eq!(vigente.tipo_documento, "PROCEDIMIENTO_OBRA");
    assert_eq!(vigente.nivel_prelacion, Some(2));
    assert!(!derogada.vigente);
    assert_eq!(derogada.fecha_derogacion.as_deref(), Some("2026-09-15"));

    let integridad = comandos::integridad(&entorno.app, CENTRO).await.unwrap();
    assert!(integridad.integro, "{integridad:?}");

    comandos::desactivar_centro(&entorno.app, CENTRO)
        .await
        .unwrap();
    let error = comandos::catalogo_centro(&entorno.app, CENTRO)
        .await
        .unwrap_err();
    assert!(error.contains(CENTRO), "{error}");
    let eliminado = comandos::eliminar_centro(&entorno.app, CENTRO)
        .await
        .unwrap();
    assert_eq!(eliminado.id, CENTRO);
    assert!(
        comandos::listar_centros(&entorno.app)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        entorno.ruta_centro("a").exists(),
        "eliminar no toca el disco"
    );
}

#[tokio::test]
async fn test_lookup_por_codigo_4a_resuelve_a_la_primera_aunque_el_titulo_destilado_no_lo_lleve() {
    // Claude destila el título sin el código («Temperatura mínima…») y la tabla se guarda con
    // id versionado: la ingesta debe conservar «Tabla 4-A» en el título (arreglo de la Fase 13)
    let distiller = MockDistiller::new();
    let mut paquete = paquete_revision(4, "2026-09-15", "FOR-ALM-05");
    paquete.tablas[0].titulo = "Temperatura mínima de precalentamiento".to_string();
    distiller.registrar_respuesta(ARCHIVO_V4, paquete);
    let entorno = entorno();
    let (app, herramientas) = construir_app(
        &entorno.config(),
        Arc::new(distiller),
        Arc::new(MockAgentRunner::new_scripted(Vec::new())),
        modelo_e5(),
    );
    let (_, v4) = entorno.escribir_revisiones();
    app.crear_centro(CENTRO, &entorno.ruta_centro_texto("b"), None)
        .await
        .unwrap();
    app.activar_centro(CENTRO).await.unwrap();
    app.ingestar(&v4, CENTRO).await.unwrap();

    let resultado = herramienta(
        &herramientas,
        "lookup_double_entry_table",
        json!({ "centro_id": CENTRO, "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42 }),
    )
    .await;
    assert!(!resultado.is_error, "{}", resultado.output);
    assert_eq!(
        resultado.output["estado"], "resuelto",
        "{}",
        resultado.output
    );
    assert_eq!(resultado.output["valor_resultado_texto"], "100 (*)");
    assert_eq!(
        resultado.output["tabla"]["titulo_tabla"],
        "Tabla 4-A — Temperatura mínima de precalentamiento"
    );
}

#[tokio::test]
async fn test_chat_de_la_ventana_rellena_citas_matriz_y_grafo_desde_las_tools_reales() {
    let llamada = |id: &str, nombre: &str, args: Value| ToolCall {
        id: id.into(),
        name: nombre.into(),
        arguments: args,
    };
    let agente = Arc::new(MockAgentRunner::new_scripted(vec![
        AgentTurnOutput::ToolCalls(vec![
            llamada(
                "m",
                "query_operational_matrix",
                json!({ "centro_id": CENTRO, "fase": "custodia" }),
            ),
            llamada(
                "g",
                "traverse_normative_graph",
                json!({ "centro_id": CENTRO, "nodo_origen": "PO-ALM-03" }),
            ),
        ]),
        AgentTurnOutput::ToolCalls(vec![llamada(
            "l",
            "lookup_double_entry_table",
            json!({ "centro_id": CENTRO, "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42 }),
        )]),
        AgentTurnOutput::FinalMessage(
            "**Jefe de Producción** firma <script>alert(1)</script> FOR-ALM-05".into(),
        ),
    ]));
    let entorno = entorno_con_agente(agente);
    centro_activo_con_revisiones(&entorno).await;
    let app = Arc::new(entorno.app);
    let chat = GuiChatService::desde_asistente(app);

    let respuesta = chat
        .enviar_mensaje("¿Quién custodia?", CENTRO, "sonnet")
        .await
        .unwrap();

    assert_eq!(respuesta.tools_used.len(), 3);
    assert!(
        respuesta
            .reply_html
            .starts_with("<p><strong>Jefe de Producción</strong>")
            && respuesta.reply_html.contains("&lt;script&gt;")
            && !respuesta.reply_html.contains("<script>"),
        "{}",
        respuesta.reply_html
    );
    let custodia = respuesta
        .citations
        .iter()
        .find(|c| c.herramienta == "query_operational_matrix")
        .expect("cita de la matriz");
    assert_eq!(custodia.doc_id, "PO-ALM-03");
    assert_eq!(custodia.revision.as_deref(), Some("Rev. 04"));
    assert!(
        custodia
            .breadcrumb
            .as_deref()
            .is_some_and(|b| b.to_uppercase().contains("CUSTODIA")),
        "{custodia:?}"
    );
    assert!(respuesta.citations.iter().any(|c| {
        c.herramienta == "lookup_double_entry_table"
            && c.fragmento_id
                .as_deref()
                .is_some_and(|f| f.contains("@v4-"))
    }));

    let matriz = respuesta.matrix_result.expect("matrixResult");
    assert_eq!(
        matriz["tipo"], "lookup",
        "el último dato tabular es el lookup"
    );
    assert_eq!(matriz["valor_resultado_texto"], "100 (*)");
    let grafo = respuesta.graph_result.expect("graphResult");
    assert_eq!(grafo["nodoOrigen"], "PO-ALM-03");
    assert!(!grafo["nodos"].as_array().unwrap().is_empty());
    assert!(grafo["aristas"][0]["cita"]["doc_id"].is_string());
}
