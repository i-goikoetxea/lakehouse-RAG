//! Punto de entrada de la aplicación Lakehouse RAG Industrial

use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Iniciando Lakehouse RAG Industrial en Rust...");
    println!("=== Lakehouse RAG Industrial en Rust ===");
    println!("Arquitectura Hexagonal inicializada.");
    Ok(())
}
