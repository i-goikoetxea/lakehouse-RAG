//! Composición única de la aplicación (plan v2 §3.1): la GUI, el MCP y la CLI obtienen aquí
//! la misma `LakehouseApp` con los adaptadores reales.

use std::path::PathBuf;
use std::sync::Arc;

use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::semantic_index::IndiceSemanticoPort;
use crate::application::use_cases::lakehouse_app::{DependenciasLakehouse, LakehouseApp};
use crate::application::use_cases::lakehouse_tool_service::LakehouseToolService;
use crate::infrastructure::embeddings::fastembed_engine::MODELO_POR_DEFECTO;
use crate::infrastructure::embeddings::lazy_engine::LazyFastEmbedEngine;
use crate::infrastructure::llm::{
    ClaudeAgentConfig, ClaudeAgentRunner, ClaudeHeadlessConfig, ClaudeHeadlessDistiller,
    ClaudeHeadlessModelCatalog, ClaudeModelCatalogConfig,
};
use crate::infrastructure::parsers::markdown_parser::MarkdownParser;
use crate::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use crate::infrastructure::query_engine::indice_semantico_memoria::IndiceSemanticoMemoria;
use crate::infrastructure::query_engine::lakehouse_projection::LanceProjection;
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
    /// Índice semántico que rellena la proyección (lectura para verificar la vista)
    pub indice: Arc<dyn IndiceSemanticoPort>,
}

/// DataFusion y el índice semántico vacíos, enlazados con la
/// proyección y las tools (que vectorizan la consulta de búsqueda con `embeddings`)
pub fn motores_consulta(embeddings: Arc<dyn EmbeddingEnginePort>) -> MotoresConsulta {
    let datafusion = Arc::new(DataFusionEngine::new());
    let indice = Arc::new(IndiceSemanticoMemoria::new());
    let herramientas = Arc::new(LakehouseToolService::new(
        datafusion.clone(),
        indice.clone(),
        embeddings,
    ));
    MotoresConsulta {
        proyeccion: Arc::new(LanceProjection::new(datafusion).con_indice_semantico(indice.clone())),
        herramientas,
        indice,
    }
}

/// Adaptadores reales de la composición, antes de construir la fachada. Las suites de
/// aceptación sustituyen una sola pieza (p. ej. un almacén con fallo inyectado) y mantienen
/// el resto idéntico a la GUI.
pub struct ComposicionReal {
    pub dependencias: DependenciasLakehouse,
    pub indice: Arc<dyn IndiceSemanticoPort>,
}

/// Registro JSON, centros en disco, LanceDB, FastEmbed diferido y Claude Code
pub fn composicion_real(config: &ConfiguracionComposicion) -> ComposicionReal {
    let embeddings = Arc::new(LazyFastEmbedEngine::new(
        MODELO_POR_DEFECTO,
        config.directorio_cache_embeddings.clone(),
    ));
    let motores = motores_consulta(embeddings.clone());
    ComposicionReal {
        dependencias: DependenciasLakehouse {
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
        },
        indice: motores.indice,
    }
}

/// Composición real que usan la GUI, el MCP y la CLI
pub fn componer_lakehouse(config: &ConfiguracionComposicion) -> Arc<LakehouseApp> {
    Arc::new(LakehouseApp::new(composicion_real(config).dependencias))
}
