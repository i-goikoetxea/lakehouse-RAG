//! Composición única de la aplicación (plan v2 §3.1): la GUI, el MCP y la CLI obtienen aquí
//! la misma `LakehouseApp` con los adaptadores reales.

use std::path::PathBuf;
use std::sync::Arc;

use crate::application::use_cases::lakehouse_app::{DependenciasLakehouse, LakehouseApp};
use crate::application::use_cases::lakehouse_tool_service::LakehouseToolService;
use crate::application::use_cases::table_lookup_service::TableLookupService;
use crate::infrastructure::embeddings::fastembed_engine::MODELO_POR_DEFECTO;
use crate::infrastructure::embeddings::lazy_engine::LazyFastEmbedEngine;
use crate::infrastructure::llm::{
    ClaudeAgentConfig, ClaudeAgentRunner, ClaudeHeadlessConfig, ClaudeHeadlessDistiller,
    ClaudeHeadlessModelCatalog, ClaudeModelCatalogConfig,
};
use crate::infrastructure::parsers::markdown_parser::MarkdownParser;
use crate::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use crate::infrastructure::query_engine::lakehouse_projection::LanceProjection;
use crate::infrastructure::query_engine::petgraph_engine::PetgraphEngine;
use crate::infrastructure::storage::center_registry_json::{
    JsonCenterRegistry, directorio_configuracion_efectivo,
};
use crate::infrastructure::storage::center_workspace_disk::DiskCenterWorkspace;

/// Parámetros inyectables de la composición
#[derive(Debug, Clone)]
pub struct ConfiguracionComposicion {
    /// Directorio donde vive `centers_registry.json`
    pub directorio_config: PathBuf,
    /// Caché de FastEmbed (`None` = `FASTEMBED_CACHE_DIR` o el directorio de datos de la app)
    pub directorio_cache_embeddings: Option<PathBuf>,
}

impl ConfiguracionComposicion {
    /// Configuración a partir del entorno (`LAKEHOUSE_RAG_CONFIG_DIR`, `FASTEMBED_CACHE_DIR`)
    pub fn desde_entorno() -> Self {
        Self {
            directorio_config: directorio_configuracion_efectivo(),
            directorio_cache_embeddings: None,
        }
    }
}

/// Motores de consulta en memoria compartidos por la proyección y las herramientas
pub struct MotoresConsulta {
    pub proyeccion: Arc<LanceProjection>,
    pub herramientas: Arc<LakehouseToolService>,
}

/// DataFusion, TableLookup y Petgraph vacíos, enlazados con la proyección y las tools
pub fn motores_consulta() -> MotoresConsulta {
    let datafusion = Arc::new(DataFusionEngine::new());
    let lookup = Arc::new(TableLookupService::new());
    let grafo = Arc::new(PetgraphEngine::new());
    let herramientas = Arc::new(LakehouseToolService::new(
        datafusion.clone(),
        lookup.clone(),
        grafo.clone(),
    ));
    MotoresConsulta {
        proyeccion: Arc::new(LanceProjection::new(datafusion, lookup, grafo)),
        herramientas,
    }
}

/// Composición real: registro JSON, centros en disco, LanceDB, FastEmbed diferido y Claude Code
pub fn componer_lakehouse(config: &ConfiguracionComposicion) -> Arc<LakehouseApp> {
    let motores = motores_consulta();
    let embeddings = Arc::new(LazyFastEmbedEngine::new(
        MODELO_POR_DEFECTO,
        config.directorio_cache_embeddings.clone(),
    ));
    Arc::new(LakehouseApp::new(DependenciasLakehouse {
        registro: Arc::new(JsonCenterRegistry::new(&config.directorio_config)),
        espacio: Arc::new(DiskCenterWorkspace::new()),
        proyeccion: motores.proyeccion,
        parser: Arc::new(MarkdownParser::new()),
        destilador: Arc::new(ClaudeHeadlessDistiller::new(ClaudeHeadlessConfig::default())),
        modelo_embeddings: embeddings.modelo_activo(),
        embeddings,
        agente: Arc::new(ClaudeAgentRunner::new(ClaudeAgentConfig::default())),
        herramientas: motores.herramientas,
        catalogo_modelos: Arc::new(ClaudeHeadlessModelCatalog::new(
            ClaudeModelCatalogConfig::default(),
        )),
    }))
}
