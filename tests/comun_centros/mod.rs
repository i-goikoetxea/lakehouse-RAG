//! Utilidades compartidas por las suites de centros y proyección (Fase 12): una `LakehouseApp`
//! con los adaptadores reales de disco (registro JSON, centros, LanceDB y proyección) y
//! dobles para Claude (destilador y agente) y FastEmbed.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use lakehouse_rag::application::dtos::agent::{AgentTurnOutput, ModelDescriptor};
use lakehouse_rag::application::dtos::centros::ModeloEmbeddingsActivo;
use lakehouse_rag::application::dtos::tools::{ToolCall, ToolResult};
use lakehouse_rag::application::ports::llm_distiller::LlmDistillerPort;
use lakehouse_rag::application::ports::model_catalog::ModelCatalogPort;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::lakehouse_app::{DependenciasLakehouse, LakehouseApp};
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::infrastructure::bootstrap::motores_consulta;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::llm::MockAgentRunner;
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;
use lakehouse_rag::infrastructure::storage::center_registry_json::JsonCenterRegistry;
use lakehouse_rag::infrastructure::storage::center_workspace_disk::DiskCenterWorkspace;
use serde_json::json;

use crate::comun_store_v2::{ARCHIVO_V3, ARCHIVO_V4, CONTENIDO_V3, CONTENIDO_V4};

pub const MODELO_E5: &str = "MultilingualE5Large";
pub const DIMENSION: usize = 1024;

pub struct CatalogoVacio;

#[async_trait]
impl ModelCatalogPort for CatalogoVacio {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        Ok(Vec::new())
    }
}

/// Entorno de prueba: directorio de configuración + directorio de datos temporales
pub struct Entorno {
    pub raiz: tempfile::TempDir,
    pub app: LakehouseApp,
    pub herramientas: Arc<dyn ToolServerPort>,
}

impl Entorno {
    pub fn config(&self) -> PathBuf {
        self.raiz.path().join("config")
    }

    pub fn ruta_centro(&self, nombre: &str) -> PathBuf {
        self.raiz.path().join("centros").join(nombre)
    }

    pub fn ruta_centro_texto(&self, nombre: &str) -> String {
        self.ruta_centro(nombre).to_string_lossy().to_string()
    }

    /// Escribe los dos fixtures sintéticos (Rev. 03 y Rev. 04) y devuelve sus rutas
    pub fn escribir_revisiones(&self) -> (PathBuf, PathBuf) {
        let entrada = self.raiz.path().join("entrada");
        std::fs::create_dir_all(&entrada).unwrap();
        let v3 = entrada.join(ARCHIVO_V3);
        let v4 = entrada.join(ARCHIVO_V4);
        std::fs::write(&v3, CONTENIDO_V3).unwrap();
        std::fs::write(&v4, CONTENIDO_V4).unwrap();
        (v3, v4)
    }

    /// Otra instancia de la app sobre el mismo disco (simula un proceso nuevo de la CLI)
    pub fn reabrir(&self, agente: Arc<MockAgentRunner>) -> (LakehouseApp, Arc<dyn ToolServerPort>) {
        construir_app(
            &self.config(),
            crate::comun_store_v2::distiller_revisiones(),
            agente,
            modelo_e5(),
        )
    }
}

pub fn modelo_e5() -> ModeloEmbeddingsActivo {
    ModeloEmbeddingsActivo::new(MODELO_E5, DIMENSION)
}

pub fn construir_app(
    config: &Path,
    destilador: Arc<dyn LlmDistillerPort>,
    agente: Arc<MockAgentRunner>,
    modelo: ModeloEmbeddingsActivo,
) -> (LakehouseApp, Arc<dyn ToolServerPort>) {
    let motores = motores_consulta();
    let herramientas: Arc<dyn ToolServerPort> = motores.herramientas.clone();
    let app = LakehouseApp::new(DependenciasLakehouse {
        registro: Arc::new(JsonCenterRegistry::new(config)),
        espacio: Arc::new(DiskCenterWorkspace::new()),
        proyeccion: motores.proyeccion,
        parser: Arc::new(MarkdownParser::new()),
        destilador,
        embeddings: Arc::new(MockEmbeddingEngine::new(Some(DIMENSION))),
        modelo_embeddings: modelo,
        agente,
        herramientas: herramientas.clone(),
        catalogo_modelos: Arc::new(CatalogoVacio),
    });
    (app, herramientas)
}

pub fn entorno_con_agente(agente: Arc<MockAgentRunner>) -> Entorno {
    let raiz = tempfile::tempdir().unwrap();
    let (app, herramientas) = construir_app(
        &raiz.path().join("config"),
        crate::comun_store_v2::distiller_revisiones(),
        agente,
        modelo_e5(),
    );
    Entorno {
        raiz,
        app,
        herramientas,
    }
}

pub fn entorno() -> Entorno {
    entorno_con_agente(Arc::new(MockAgentRunner::new_scripted(Vec::new())))
}

/// Agente guionizado: una llamada a `execute_athena_sql` y después una respuesta final
pub fn agente_sql(centro: &str, sql: &str, respuesta: &str) -> Arc<MockAgentRunner> {
    Arc::new(MockAgentRunner::new_scripted(vec![
        AgentTurnOutput::ToolCalls(vec![ToolCall {
            id: "call_sql".into(),
            name: "execute_athena_sql".into(),
            arguments: json!({ "centro_id": centro, "sql_query": sql }),
        }]),
        AgentTurnOutput::FinalMessage(respuesta.into()),
    ]))
}

/// Ejecuta `execute_athena_sql` directamente sobre el servidor de herramientas
pub async fn sql(
    herramientas: &Arc<dyn ToolServerPort>,
    centro: &str,
    consulta: &str,
) -> ToolResult {
    herramientas
        .ejecutar_herramienta(&ToolCall {
            id: "sql".into(),
            name: "execute_athena_sql".into(),
            arguments: json!({ "centro_id": centro, "sql_query": consulta }),
        })
        .await
        .expect("la herramienta devuelve un ToolResult")
}

/// Primer valor numérico de la primera fila de un resultado SQL
pub fn escalar(resultado: &ToolResult) -> i64 {
    let fila = resultado.output.as_array().and_then(|f| f.first()).cloned();
    fila.and_then(|f| f.as_object().and_then(|o| o.values().next().cloned()))
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| panic!("sin escalar en {}", resultado.output))
}
