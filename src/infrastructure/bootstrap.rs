//! Composición única de la aplicación (plan v2 §3.1): la GUI, el MCP y la CLI obtienen aquí
//! la misma `LakehouseApp` con los adaptadores reales.

use std::path::PathBuf;
use std::sync::Arc;

use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::semantic_index::IndiceSemanticoPort;
use crate::application::use_cases::lakehouse_app::{DependenciasLakehouse, LakehouseApp};
use crate::application::use_cases::lakehouse_tool_service::{
    ConfiguracionHerramientas, LakehouseToolService,
};
use crate::infrastructure::embeddings::fastembed_engine::MODELO_POR_DEFECTO;
use crate::infrastructure::embeddings::lazy_engine::LazyFastEmbedEngine;
use crate::infrastructure::llm::anthropic_api::AnthropicMessagesRunner;
use crate::infrastructure::llm::modo_llm::{
    ModoLlm, agente_para, catalogo_modelos_para, verificador_procedencia_para,
};
use crate::infrastructure::llm::preferencia_modo::{
    ModoResuelto, leer_preferencias, resolver_modo_al_arrancar,
};
use crate::infrastructure::llm::{
    ClaudeAgentConfig, ClaudeHeadlessConfig, ClaudeHeadlessDistiller,
};
use crate::infrastructure::parsers::ParserPorFormato;
use crate::infrastructure::query_engine::datafusion_engine::DataFusionEngine;
use crate::infrastructure::query_engine::indice_semantico_memoria::IndiceSemanticoMemoria;
use crate::infrastructure::query_engine::lakehouse_projection::LanceProjection;
use crate::infrastructure::reloj_sistema::RelojDelSistema;
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
    motores_consulta_con(embeddings, ConfiguracionHerramientas::default())
}

/// `motores_consulta` con los límites de las tools indicados
pub fn motores_consulta_con(
    embeddings: Arc<dyn EmbeddingEnginePort>,
    config: ConfiguracionHerramientas,
) -> MotoresConsulta {
    let datafusion = Arc::new(DataFusionEngine::new());
    let indice = Arc::new(IndiceSemanticoMemoria::new());
    let herramientas = Arc::new(
        LakehouseToolService::new(datafusion.clone(), indice.clone(), embeddings)
            .con_configuracion(config),
    );
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
    /// El runner de la API cuando el modo es `api`, para leer sus métricas (A/B)
    pub agente_api: Option<Arc<AnthropicMessagesRunner>>,
    /// Modo pedido al arrancar (entorno > `llm_config.json` > `Cli`) y su origen
    pub modo_pedido: ModoResuelto,
    /// Modo que de verdad usa el agente: `Cli` si se pidió `Api` o `FreeLlmApi` sin cliente
    pub modo_en_uso: ModoLlm,
    /// URL de FreeLLMAPI con que se compuso (guardada o la de la app de escritorio)
    pub freellmapi_url: String,
}

/// Registro JSON, centros en disco, LanceDB, FastEmbed diferido y Claude Code
pub fn composicion_real(config: &ConfiguracionComposicion) -> ComposicionReal {
    let embeddings = Arc::new(LazyFastEmbedEngine::new(
        MODELO_POR_DEFECTO,
        config.directorio_cache_embeddings.clone(),
    ));
    let motores = motores_consulta(embeddings.clone());
    // El modo `api` (por `LAKEHOUSE_RAG_MODO_LLM` o por `llm_config.json`) elige la API de
    // Messages para el agente y la procedencia; por defecto, `claude -p`
    let modo_pedido = resolver_modo_al_arrancar(&config.directorio_config);
    let modo = modo_pedido.modo;
    let url_freellmapi = leer_preferencias(&config.directorio_config)
        .url_freellmapi()
        .to_string();
    let agente = agente_para(modo, &url_freellmapi, ClaudeAgentConfig::default());
    let modo_en_uso = agente.en_uso;
    tracing::info!(
        pedido = ?modo,
        origen = modo_pedido.origen.nombre(),
        en_uso = ?modo_en_uso,
        "modo del LLM resuelto al arrancar"
    );
    ComposicionReal {
        dependencias: DependenciasLakehouse {
            registro: Arc::new(JsonCenterRegistry::new(&config.directorio_config)),
            espacio: Arc::new(DiskCenterWorkspace::new()),
            proyeccion: motores.proyeccion,
            parser: Arc::new(ParserPorFormato::new()),
            destilador: Arc::new(ClaudeHeadlessDistiller::new(ClaudeHeadlessConfig::default())),
            modelo_embeddings: embeddings.modelo_activo(),
            embeddings,
            agente: agente.runner,
            herramientas: motores.herramientas,
            catalogo_modelos: catalogo_modelos_para(modo_en_uso, &url_freellmapi),
            reloj: Arc::new(RelojDelSistema),
            verificador_procedencia: Some(verificador_procedencia_para(
                modo_en_uso,
                &url_freellmapi,
                ClaudeAgentConfig::default().binary_path,
            )),
        },
        indice: motores.indice,
        agente_api: agente.api,
        modo_pedido,
        modo_en_uso,
        freellmapi_url: url_freellmapi,
    }
}

/// Composición real que usan la GUI, el MCP y la CLI
pub fn componer_lakehouse(config: &ConfiguracionComposicion) -> Arc<LakehouseApp> {
    componer_lakehouse_con_modo(config).app
}

/// La fachada y el motor del LLM con que quedó compuesta (lo muestra la ventana)
pub struct LakehouseCompuesta {
    pub app: Arc<LakehouseApp>,
    pub modo_pedido: ModoResuelto,
    pub modo_en_uso: ModoLlm,
    pub freellmapi_url: String,
}

pub fn componer_lakehouse_con_modo(config: &ConfiguracionComposicion) -> LakehouseCompuesta {
    let composicion = composicion_real(config);
    LakehouseCompuesta {
        app: Arc::new(LakehouseApp::new(composicion.dependencias)),
        modo_pedido: composicion.modo_pedido,
        modo_en_uso: composicion.modo_en_uso,
        freellmapi_url: composicion.freellmapi_url,
    }
}
