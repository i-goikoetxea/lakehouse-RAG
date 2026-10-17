//! Pruebas de integración E2E del Servidor de Herramientas y Agente (Fase 6, adaptadas al
//! contrato v2 de la Fase 13): las 6 tools sobre vistas DataFusion en memoria y el ciclo
//! completo del orquestador con `MockAgentRunner`.

mod comun_agent_tools;

use std::sync::Arc;

use serde_json::{Value, json};

use comun_agent_tools::*;
use lakehouse_rag::application::dtos::agent::{
    AgentMessage, AgentRole, AgentTurnOutput, ClaudeModel,
};
use lakehouse_rag::application::dtos::tools::{LakehouseToolName, ToolCall, ToolDefinition};
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::agent_orchestrator::AgentOrchestrator;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::infrastructure::llm::mock_agent_runner::MockAgentRunner;

const CENTRO: &str = "planta_norte";

async fn invocar(
    servicio: &dyn ToolServerPort,
    herramienta: LakehouseToolName,
    argumentos: Value,
) -> Value {
    let resultado = servicio
        .ejecutar_herramienta(&ToolCall {
            id: format!("call_{}", herramienta.as_str()),
            name: herramienta.as_str().to_string(),
            arguments: argumentos,
        })
        .await
        .expect("la herramienta devuelve un ToolResult");
    assert!(
        !resultado.is_error,
        "{} falló: {}",
        herramienta.as_str(),
        resultado.output
    );
    resultado.output
}

fn columna(filas: &Value, clave: &str) -> Vec<String> {
    filas
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f[clave].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn test_integration_e2e_las_6_tools_del_lakehouse() {
    let entorno = entorno_vacio();
    let centro = CenterId::new(CENTRO).unwrap();
    sembrar_centro(&entorno, &centro).await;
    let servicio = entorno.servicio.as_ref();

    assert_eq!(servicio.listar_herramientas().len(), 6);
    assert_eq!(ToolDefinition::lakehouse_tools().len(), 6);

    // --- TOOL 1: inspect_lakehouse_catalog (solo lo vigente, prelación ascendente) ---
    let catalogo = invocar(
        servicio,
        LakehouseToolName::InspectLakehouseCatalog,
        json!({ "centro_id": CENTRO }),
    )
    .await;
    assert_eq!(
        columna(&catalogo["documentos"], "doc_id"),
        vec!["DOC-001", "DOC-003"]
    );
    assert_eq!(catalogo["documentos"][0]["revision"], "Rev. 02");
    let tablas = catalogo["tablas_doble_entrada"].as_array().unwrap();
    assert_eq!(tablas.len(), 1);
    assert_eq!(tablas[0]["tabla_id"], "DOC-001@v2-TAB-0000");

    let catalogo_antiguo = invocar(
        servicio,
        LakehouseToolName::InspectLakehouseCatalog,
        json!({ "centro_id": CENTRO, "fecha_referencia": FECHA_ANTERIOR }),
    )
    .await;
    assert_eq!(
        columna(&catalogo_antiguo["documentos"], "revision"),
        vec!["Rev. 01", "Rev. 01"]
    );
    assert_eq!(
        columna(&catalogo_antiguo["documentos"], "doc_id"),
        vec!["DOC-001", "DOC-002"]
    );

    // --- TOOL 2: query_operational_matrix (filtro exacto con tildes/mayúsculas) ---
    let matriz = invocar(
        servicio,
        LakehouseToolName::QueryOperationalMatrix,
        json!({ "centro_id": CENTRO, "fase": "Recepción" }),
    )
    .await;
    assert_eq!(matriz["total"], 1, "{matriz}");
    let fila = &matriz["filas"][0];
    assert_eq!(fila["objeto_alcance"], "Temperatura Almacén");
    assert_eq!(fila["rol_responsable"], "Jefe de Almacén");
    assert_eq!(fila["registro_codigo"], "FOR-ALM-01");
    assert_eq!(fila["cita"]["revision"], "Rev. 02");

    let matriz_antigua = invocar(
        servicio,
        LakehouseToolName::QueryOperationalMatrix,
        json!({ "centro_id": CENTRO, "fase": "RECEPCION", "fecha_referencia": FECHA_ANTERIOR }),
    )
    .await;
    assert_eq!(matriz_antigua["total"], 1);
    assert_eq!(matriz_antigua["filas"][0]["registro_codigo"], "FOR-ALM-00");
    assert_eq!(matriz_antigua["filas"][0]["cita"]["revision"], "Rev. 01");

    let por_rol = invocar(
        servicio,
        LakehouseToolName::QueryOperationalMatrix,
        json!({ "centro_id": CENTRO, "rol": "jefe de calidad", "rol_en": "responsable" }),
    )
    .await;
    assert_eq!(
        columna(&por_rol["filas"], "registro_codigo"),
        vec!["PPI-MAT-09"]
    );

    // --- TOOL 3: lookup_double_entry_table (rango numérico × categoría, nota obligatoria) ---
    let lookup = invocar(
        servicio,
        LakehouseToolName::LookupDoubleEntryTable,
        json!({ "centro_id": CENTRO, "tabla_id": "4-A", "valor_x": 15.0, "valor_y": "Acero Inox" }),
    )
    .await;
    assert_eq!(lookup["estado"], "resuelto", "{lookup}");
    assert_eq!(lookup["valor_resultado_texto"], "Electrodo E-308 (1)");
    assert_eq!(lookup["nota_pie_condicion"], NOTA_PRECALENTAR);
    assert_eq!(lookup["nota_obligatoria"], true);
    assert_eq!(lookup["cita"]["revision"], "Rev. 02");

    let frontera = invocar(
        servicio,
        LakehouseToolName::LookupDoubleEntryTable,
        json!({ "centro_id": CENTRO, "tabla_id": "DOC-001@v2-TAB-0000", "valor_x": "20", "valor_y": "acero inox" }),
    )
    .await;
    assert_eq!(
        frontera["valor_resultado_texto"], "Electrodo E-309",
        "20 mm queda fuera de [10, 20) y dentro de [20, 40]"
    );

    let fuera = invocar(
        servicio,
        LakehouseToolName::LookupDoubleEntryTable,
        json!({ "centro_id": CENTRO, "busqueda_tabla": "electrodo", "valor_x": 50, "valor_y": "Acero Inox" }),
    )
    .await;
    assert_eq!(fuera["estado"], "sin_coincidencia", "no se interpola");

    let inexistente = invocar(
        servicio,
        LakehouseToolName::LookupDoubleEntryTable,
        json!({ "centro_id": CENTRO, "tabla_id": "9-Z", "valor_x": 15, "valor_y": "Acero Inox" }),
    )
    .await;
    assert_eq!(inexistente["estado"], "tabla_no_encontrada");

    // --- TOOL 4: execute_athena_sql (nombres canónicos reescritos a las vistas del centro) ---
    let sql = invocar(
        servicio,
        LakehouseToolName::ExecuteAthenaSql,
        json!({
            "centro_id": CENTRO,
            "sql_query": "SELECT COUNT(*) AS total FROM 01_documentos_catalogo WHERE vigente = true"
        }),
    )
    .await;
    assert_eq!(sql["filas"], 1);
    assert_eq!(sql["truncado"], false);
    assert_eq!(sql["resultado"][0]["total"], 2);

    let sql_antigua = invocar(
        servicio,
        LakehouseToolName::ExecuteAthenaSql,
        json!({
            "centro_id": CENTRO,
            "sql_query": "SELECT doc_id FROM 01_documentos_catalogo ORDER BY doc_id",
            "fecha_referencia": FECHA_ANTERIOR
        }),
    )
    .await;
    assert_eq!(
        columna(&sql_antigua["resultado"], "doc_id"),
        vec!["DOC-001", "DOC-002"]
    );

    // --- TOOL 5: traverse_normative_graph (sustituye al modo 'prevalencia' de v1) ---
    let grafo = invocar(
        servicio,
        LakehouseToolName::TraverseNormativeGraph,
        json!({ "centro_id": CENTRO, "nodo_origen": "DOC-001" }),
    )
    .await;
    assert_eq!(grafo["estado"], "ok", "{grafo}");
    let ids = columna(&grafo["nodos"], "id");
    assert!(ids.contains(&"DOC-003".to_string()), "{ids:?}");
    assert!(ids.contains(&"FOR-ALM-01".to_string()), "{ids:?}");
    let aristas = grafo["aristas"].as_array().unwrap();
    assert!(
        aristas.iter().any(|a| a["origen"] == "DOC-003"
            && a["tipo_relacion"] == "DEROGA_A"
            && a["destino"] == "DOC-002"),
        "la derogación se alcanza en el segundo salto: {grafo}"
    );

    let derogaciones = invocar(
        servicio,
        LakehouseToolName::TraverseNormativeGraph,
        json!({ "centro_id": CENTRO, "nodo_origen": "DOC-002", "tipo_relacion": ["DEROGA_A"], "max_saltos": 1 }),
    )
    .await;
    let aristas = derogaciones["aristas"].as_array().unwrap();
    assert_eq!(aristas.len(), 1, "{derogaciones}");
    assert_eq!(
        aristas[0]["origen"], "DOC-003",
        "DOC-003 prevalece sobre DOC-002"
    );
    assert_eq!(aristas[0]["contexto"], "Deroga el procedimiento previo");

    // --- TOOL 6: search_normative_context (solo la revisión vigente, con condición) ---
    let busqueda = invocar(
        servicio,
        LakehouseToolName::SearchNormativeContext,
        json!({ "centro_id": CENTRO, "consulta": "precalentar la zona de soldadura" }),
    )
    .await;
    assert_eq!(busqueda["total"], 1, "{busqueda}");
    let resultado = &busqueda["grupos"][0]["resultados"][0];
    assert!(resultado["texto"].as_str().unwrap().contains("100 °C"));
    assert!(
        resultado["condicion_aplicacion"]
            .as_str()
            .unwrap()
            .contains("< 0 °C")
    );
    assert_eq!(resultado["cita"]["revision"], "Rev. 02");
}

#[tokio::test]
async fn test_integration_agent_orchestrator_ciclo_completo() {
    let entorno = entorno_vacio();
    let centro = CenterId::new("planta_alfa").unwrap();
    sembrar_centro(&entorno, &centro).await;

    let runner = Arc::new(MockAgentRunner::new_scripted(vec![
        AgentTurnOutput::ToolCalls(vec![ToolCall {
            id: "call_cat".to_string(),
            name: LakehouseToolName::InspectLakehouseCatalog
                .as_str()
                .to_string(),
            arguments: json!({ "centro_id": "planta_alfa" }),
        }]),
        AgentTurnOutput::FinalMessage(
            "Dictamen Técnico Final: DOC-001 Rev. 02 y DOC-003 están vigentes.".to_string(),
        ),
    ]));
    let orchestrator = AgentOrchestrator::new(runner, entorno.servicio.clone());

    let mut historial = vec![AgentMessage::user(
        "Por favor revisa el catálogo de normas vigentes",
    )];

    let respuesta_final = orchestrator
        .procesar_turno(&ClaudeModel::sonnet(), &centro, &mut historial, None)
        .await
        .unwrap();

    assert!(respuesta_final.contains("Dictamen Técnico Final"));
    // El historial debe contener: [user, assistant_tools, tool_result, assistant_final]
    assert_eq!(historial.len(), 4);
    assert_eq!(
        historial[1].tool_calls.as_ref().unwrap()[0].name,
        "inspect_lakehouse_catalog"
    );
    assert_eq!(historial[2].role, AgentRole::Tool);
    let salida_tool: Value = serde_json::from_str(&historial[2].content)
        .unwrap_or_else(|_| panic!("resultado de tool no JSON: {}", historial[2].content));
    assert!(salida_tool.get("error").is_none(), "{salida_tool}");
    assert_eq!(
        columna(&salida_tool["documentos"], "doc_id"),
        vec!["DOC-001", "DOC-003"]
    );
}
