//! Integración real con Claude Code headless (requiere `claude` en PATH y sesión iniciada).
//! Ejecutar con: cargo test --test integration_claude_model_catalog -- --ignored

use lakehouse_rag::application::ports::model_catalog::ModelCatalogPort;
use lakehouse_rag::infrastructure::llm::{ClaudeHeadlessModelCatalog, ClaudeModelCatalogConfig};

#[tokio::test]
#[ignore = "Requiere Claude Code instalado y autenticado"]
async fn test_catalogo_real_lista_alias_principales() {
    let catalogo = ClaudeHeadlessModelCatalog::new(ClaudeModelCatalogConfig::default());

    let modelos = catalogo
        .listar_modelos()
        .await
        .expect("Claude Code debe devolver su catálogo de modelos");

    let alias: Vec<&str> = modelos.iter().map(|m| m.alias.cli_model_name()).collect();
    for esperado in ["opus", "sonnet", "haiku"] {
        assert!(alias.contains(&esperado), "Falta '{esperado}' en {alias:?}");
    }
}
