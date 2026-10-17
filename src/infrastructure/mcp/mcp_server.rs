//! Servidor MCP (Model Context Protocol) sobre stdio con JSON-RPC 2.0

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

use crate::application::dtos::tools::ToolCall;
use crate::application::ports::tool_server::ToolServerPort;
use crate::domain::errors::DomainError;

/// Petición JSON-RPC 2.0 estándar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Respuesta JSON-RPC 2.0 estándar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Servidor MCP que expone las herramientas operativas del Lakehouse a clientes compatibles
pub struct McpServer {
    tool_server: Arc<dyn ToolServerPort>,
}

impl McpServer {
    pub const PROTOCOL_VERSION: &'static str = "2024-11-05";
    pub const SERVER_NAME: &'static str = "lakehouse-rag";
    pub const SERVER_VERSION: &'static str = "0.1.0";

    pub fn new(tool_server: Arc<dyn ToolServerPort>) -> Self {
        Self { tool_server }
    }

    /// Procesa una línea de texto JSON-RPC y retorna la respuesta serializada si aplica
    pub async fn procesar_linea(&self, linea: &str) -> Option<String> {
        let trimmed = linea.trim();
        if trimmed.is_empty() {
            return None;
        }

        let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(req) => req,
            Err(e) => {
                let err_resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                        data: None,
                    }),
                };
                return serde_json::to_string(&err_resp).ok();
            }
        };

        // Si es una notificación (sin id), se procesa silenciosamente
        let req_id = match request.id {
            Some(id) => id,
            None => {
                // Notificación recibida (ej. notifications/initialized)
                return None;
            }
        };

        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(req_id, &request.params),
            "tools/list" => self.handle_tools_list(req_id),
            "tools/call" => self.handle_tools_call(req_id, &request.params).await,
            "ping" => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req_id,
                result: Some(json!({})),
                error: None,
            },
            metodo_desconocido => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: req_id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: '{metodo_desconocido}'"),
                    data: None,
                }),
            },
        };

        serde_json::to_string(&response).ok()
    }

    fn handle_initialize(&self, id: Value, _params: &Value) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "protocolVersion": Self::PROTOCOL_VERSION,
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": Self::SERVER_NAME,
                    "version": Self::SERVER_VERSION
                }
            })),
            error: None,
        }
    }

    fn handle_tools_list(&self, id: Value) -> JsonRpcResponse {
        let tools_def = self.tool_server.listar_herramientas();

        // En MCP, el esquema JSON de parámetros se serializa bajo la clave `inputSchema`
        let mcp_tools: Vec<Value> = tools_def
            .into_iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": t.parameters
                })
            })
            .collect();

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(json!({
                "tools": mcp_tools
            })),
            error: None,
        }
    }

    async fn handle_tools_call(&self, id: Value, params: &Value) -> JsonRpcResponse {
        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Invalid params: el campo 'name' es requerido".to_string(),
                        data: None,
                    }),
                };
            }
        };

        let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);

        let call = ToolCall {
            id: format!("mcp_call_{tool_name}"),
            name: tool_name.to_string(),
            arguments,
        };

        match self.tool_server.ejecutar_herramienta(&call).await {
            Ok(tool_result) => {
                let text_output = tool_result.output.to_string();
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(json!({
                        "content": [
                            {
                                "type": "text",
                                "text": text_output
                            }
                        ],
                        "isError": tool_result.is_error
                    })),
                    error: None,
                }
            }
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: Some(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Error ejecutando herramienta: {e}")
                        }
                    ],
                    "isError": true
                })),
                error: None,
            },
        }
    }

    /// Inicia el bucle de servidor de lectura y escritura asíncrona sobre flujos de E/S
    pub async fn run_io<R, W>(&self, reader: R, mut writer: W) -> Result<(), DomainError>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut lines = reader.lines();

        while let Ok(Some(linea)) = lines.next_line().await {
            if let Some(respuesta) = self.procesar_linea(&linea).await {
                writer.write_all(respuesta.as_bytes()).await.map_err(|e| {
                    DomainError::ToolExecutionError(format!("Error escribiendo a stdout: {e}"))
                })?;
                writer.write_all(b"\n").await.map_err(|e| {
                    DomainError::ToolExecutionError(format!(
                        "Error escribiendo salto de línea: {e}"
                    ))
                })?;
                writer.flush().await.map_err(|e| {
                    DomainError::ToolExecutionError(format!("Error haciendo flush a stdout: {e}"))
                })?;
            }
        }

        Ok(())
    }

    /// Ejecuta el servidor MCP conectado directamente a stdin y stdout del proceso
    pub async fn run_stdio(&self) -> Result<(), DomainError> {
        let stdin = tokio::io::BufReader::new(tokio::io::stdin());
        let stdout = tokio::io::stdout();
        self.run_io(stdin, stdout).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dtos::tools::{ToolDefinition, ToolResult};
    use async_trait::async_trait;

    struct DummyToolServer;

    #[async_trait]
    impl ToolServerPort for DummyToolServer {
        fn listar_herramientas(&self) -> Vec<ToolDefinition> {
            ToolDefinition::lakehouse_tools()
        }

        async fn ejecutar_herramienta(&self, call: &ToolCall) -> Result<ToolResult, DomainError> {
            if call.name == "inspect_lakehouse_catalog" {
                Ok(ToolResult {
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    output: json!([{ "doc_id": "DOC-MCP-001", "vigente": true }]),
                    is_error: false,
                })
            } else {
                Ok(ToolResult {
                    call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    output: json!({ "mensaje": "Herramienta ejecutada con éxito" }),
                    is_error: false,
                })
            }
        }
    }

    #[tokio::test]
    async fn test_mcp_initialize_handshake() {
        let server = McpServer::new(Arc::new(DummyToolServer));
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "claude-code", "version": "2.1.296" }
            }
        });

        let resp_str = server.procesar_linea(&req.to_string()).await.unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp.id, json!(1));
        let res = resp.result.unwrap();
        assert_eq!(res["protocolVersion"], "2024-11-05");
        assert_eq!(res["serverInfo"]["name"], "lakehouse-rag");
        assert!(res["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn test_mcp_tools_list_retorna_las_5_herramientas() {
        let server = McpServer::new(Arc::new(DummyToolServer));
        let req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });

        let resp_str = server.procesar_linea(&req.to_string()).await.unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();

        let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        assert_eq!(tools.len(), 5);

        let nombres: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(nombres.contains(&"inspect_lakehouse_catalog"));
        assert!(nombres.contains(&"query_operational_matrix"));
        assert!(nombres.contains(&"lookup_double_entry_table"));
        assert!(nombres.contains(&"execute_athena_sql"));
        assert!(nombres.contains(&"traverse_normative_graph"));
    }

    #[tokio::test]
    async fn test_mcp_tools_call_ejecuta_herramienta_y_retorna_content() {
        let server = McpServer::new(Arc::new(DummyToolServer));
        let req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "inspect_lakehouse_catalog",
                "arguments": {
                    "centro_id": "planta_alfa",
                    "solo_vigentes": true
                }
            }
        });

        let resp_str = server.procesar_linea(&req.to_string()).await.unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();

        let result = resp.result.unwrap();
        assert_eq!(result["isError"], false);
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert!(content[0]["text"].as_str().unwrap().contains("DOC-MCP-001"));
    }

    #[tokio::test]
    async fn test_mcp_metodo_desconocido_retorna_error_32601() {
        let server = McpServer::new(Arc::new(DummyToolServer));
        let req = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "metodo_inexistente"
        });

        let resp_str = server.procesar_linea(&req.to_string()).await.unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();

        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
