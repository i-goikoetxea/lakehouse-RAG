//! Adaptador para invocación de Claude Headless CLI (`claude -p`) y servidor MCP

pub mod claude_headless_distiller;
pub mod mock_distiller;

pub use claude_headless_distiller::{ClaudeHeadlessConfig, ClaudeHeadlessDistiller};
pub use mock_distiller::MockDistiller;
