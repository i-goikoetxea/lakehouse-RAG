//! Pruebas adversariales de seguridad y resiliencia para el servidor MCP (Fase 7)

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use lakehouse_rag::application::dtos::tools::{ToolCall, ToolDefinition, ToolResult};
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::infrastructure::mcp::{JsonRpcResponse, McpServer};

struct MockToolServerForMcp;

#[async_trait]
impl ToolServerPort for MockToolServerForMcp {
    fn listar_herramientas(&self) -> Vec<ToolDefinition> {
        ToolDefinition::lakehouse_tools()
    }

    async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
        if call.name == "lookup_double_entry_table" {
            Ok(ToolResult {
                call_id: call.id.clone(),
                tool_name: call.name.clone(),
                output: json!({ "encontrado": true, "valor": "Electrodo E-308" }),
                is_error: false,
            })
        } else {
            Err(DomainError::ToolExecutionError(
                "Fallo inducido para test".to_string(),
            ))
        }
    }
}

#[tokio::test]
async fn test_adv_mcp_tolerancia_a_payloads_json_rotos() {
    let server = McpServer::new(Arc::new(MockToolServerForMcp));

    let entradas_rotas = [
        "{ jsonrpc: 2.0, roto }",
        "",
        "   ",
        "{\"jsonrpc\": \"2.0\", \"id\": 1, \"method\":",
        "[\"array_no_objeto\"]",
    ];

    for entrada in entradas_rotas {
        let resp_opt = server.procesar_linea(entrada).await;
        if entrada.trim().is_empty() {
            assert!(resp_opt.is_none());
        } else {
            let resp_str = resp_opt.expect("Debe retornar respuesta de error JSON-RPC");
            let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();
            assert!(resp.error.is_some());
            assert_eq!(resp.error.unwrap().code, -32700); // Parse error
        }
    }
}

#[tokio::test]
async fn test_adv_mcp_notificaciones_sin_id_no_emiten_respuesta() {
    let server = McpServer::new(Arc::new(MockToolServerForMcp));

    // Las notificaciones en JSON-RPC 2.0 no llevan el campo 'id'
    let notificacion = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });

    let resp = server.procesar_linea(&notificacion.to_string()).await;
    assert!(
        resp.is_none(),
        "Las notificaciones no deben emitir respuestas"
    );
}

#[tokio::test]
async fn test_adv_mcp_herramienta_sin_nombre_retorna_error_32602() {
    let server = McpServer::new(Arc::new(MockToolServerForMcp));

    let req = json!({
        "jsonrpc": "2.0",
        "id": 99,
        "method": "tools/call",
        "params": {
            // Falta 'name'
            "arguments": { "foo": "bar" }
        }
    });

    let resp_str = server.procesar_linea(&req.to_string()).await.unwrap();
    let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();

    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32602); // Invalid params
}

#[tokio::test]
async fn test_adv_mcp_fallo_de_herramienta_retorna_is_error_true_sin_pánico() {
    let server = McpServer::new(Arc::new(MockToolServerForMcp));

    let req = json!({
        "jsonrpc": "2.0",
        "id": 100,
        "method": "tools/call",
        "params": {
            "name": "inspect_lakehouse_catalog",
            "arguments": {}
        }
    });

    let resp_str = server.procesar_linea(&req.to_string()).await.unwrap();
    let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();

    assert!(resp.error.is_none());
    let result = resp.result.unwrap();
    assert_eq!(result["isError"], true);
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Fallo inducido"));
}
