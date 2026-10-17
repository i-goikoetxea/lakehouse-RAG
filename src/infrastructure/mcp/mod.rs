//! Módulo de servidor MCP (Model Context Protocol)

pub mod mcp_server;

pub use mcp_server::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpServer};
