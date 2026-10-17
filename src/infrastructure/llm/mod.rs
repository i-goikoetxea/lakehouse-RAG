pub mod claude_agent_runner;
pub mod claude_headless_distiller;
pub mod claude_model_catalog;
pub mod mock_agent_runner;
pub mod mock_distiller;

pub use claude_agent_runner::{ClaudeAgentConfig, ClaudeAgentRunner};
pub use claude_headless_distiller::{ClaudeHeadlessConfig, ClaudeHeadlessDistiller};
pub use claude_model_catalog::{ClaudeHeadlessModelCatalog, ClaudeModelCatalogConfig};
pub use mock_agent_runner::MockAgentRunner;
pub use mock_distiller::MockDistiller;

/// Evita que cada invocación de Claude CLI abra una ventana de consola desde la GUI
pub(crate) fn ocultar_consola_en_windows(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}
