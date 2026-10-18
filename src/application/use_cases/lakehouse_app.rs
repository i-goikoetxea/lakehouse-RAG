//! Fachada única de la aplicación (plan v2 §3.1): la GUI, el MCP y la CLI usan la misma
//! composición para gestionar centros, ingerir y preguntar.
//!
//! Invariantes:
//! * Un centro solo se abre si su `center_manifest.json` coincide con el modelo de embeddings.
//! * Las vistas DataFusion (y el índice semántico) se reconstruyen al activar y tras cada
//!   ingesta confirmada; al desactivar se retiran (cero acceso).
//! * Solo se pregunta sobre centros registrados y activos.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::application::dtos::agent::{
    AgentExecutionEvent, AgentMessage, ClaudeModel, ModelDescriptor,
};
use crate::application::dtos::centros::{
    ManifiestoCentroDisco, ModeloEmbeddingsActivo, validar_ruta_centro, validar_ruta_documento,
};
use crate::application::dtos::ingestion::{EtapaIngesta, ObservadorIngesta};
use crate::application::ports::agent_runner::AgentRunnerPort;
use crate::application::ports::center_registry::CenterRegistryPort;
use crate::application::ports::center_workspace::{CenterWorkspacePort, CentroAbierto};
use crate::application::ports::document_parser::DocumentParserPort;
use crate::application::ports::embedding_engine::EmbeddingEnginePort;
use crate::application::ports::lakehouse_projection::{LakehouseProjectionPort, ResumenProyeccion};
use crate::application::ports::lakehouse_store::{InformeIntegridad, TODAS_LAS_TABLAS};
use crate::application::ports::llm_distiller::LlmDistillerPort;
use crate::application::ports::model_catalog::ModelCatalogPort;
use crate::application::ports::tool_server::ToolServerPort;
use crate::application::use_cases::agent_orchestrator::{AgentOrchestrator, EventCallback};
use crate::application::use_cases::document_ingestion_service::{
    DocumentIngestionService, IngestionReport,
};
use crate::domain::errors::DomainError;
use crate::domain::models::center::{CenterId, CenterManifest, CentroConocimiento};

/// Puertos que necesita la fachada (se componen en `infrastructure::bootstrap`)
pub struct DependenciasLakehouse {
    pub registro: Arc<dyn CenterRegistryPort>,
    pub espacio: Arc<dyn CenterWorkspacePort>,
    pub proyeccion: Arc<dyn LakehouseProjectionPort>,
    pub parser: Arc<dyn DocumentParserPort>,
    pub destilador: Arc<dyn LlmDistillerPort>,
    pub embeddings: Arc<dyn EmbeddingEnginePort>,
    pub modelo_embeddings: ModeloEmbeddingsActivo,
    pub agente: Arc<dyn AgentRunnerPort>,
    pub herramientas: Arc<dyn ToolServerPort>,
    pub catalogo_modelos: Arc<dyn ModelCatalogPort>,
}

/// Resultado de `ingestar`
#[derive(Debug, Clone)]
pub struct ResultadoIngesta {
    pub informe: IngestionReport,
    pub copia_original: String,
    /// Vistas reconstruidas (solo si el centro está activo y hubo cambios)
    pub proyeccion: Option<ResumenProyeccion>,
    /// La ingesta ya está confirmada en LanceDB; si la reconstrucción de vistas falla después,
    /// no se informa como fallo de ingesta sino como aviso (las vistas previas siguen intactas)
    pub aviso_proyeccion: Option<String>,
}

/// Una herramienta invocada por el agente durante `preguntar`
#[derive(Debug, Clone, PartialEq)]
pub struct UsoHerramienta {
    pub nombre: String,
    pub argumentos: serde_json::Value,
    pub es_error: bool,
    pub resultado: serde_json::Value,
}

/// Respuesta de `preguntar`
#[derive(Debug, Clone, PartialEq)]
pub struct RespuestaLakehouse {
    pub centro_id: String,
    pub modelo: String,
    pub respuesta: String,
    pub herramientas: Vec<UsoHerramienta>,
}

/// Diagnóstico de `integridad`: el del almacén más las tablas que faltan en disco
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InformeIntegridadCentro {
    pub centro_id: String,
    pub informe: InformeIntegridad,
    pub tablas_ausentes: Vec<String>,
}

impl InformeIntegridadCentro {
    pub fn es_integro(&self) -> bool {
        self.tablas_ausentes.is_empty() && self.informe.es_integro()
    }
}

/// Resultado de rehidratar un centro activo al arrancar
#[derive(Debug, Clone)]
pub struct HidratacionCentro {
    pub centro_id: String,
    pub resultado: Result<ResumenProyeccion, String>,
}

/// Contrato conversacional que consumen la GUI y otros adaptadores de entrada
#[async_trait]
pub trait AsistenteLakehousePort: Send + Sync {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError>;

    /// Un turno completo del agente sobre un historial existente
    async fn conversar(
        &self,
        modelo: &ClaudeModel,
        centro_id: &CenterId,
        historial: &mut Vec<AgentMessage>,
        on_event: Option<EventCallback>,
    ) -> Result<String, DomainError>;
}

/// Fachada de la aplicación
pub struct LakehouseApp {
    registro: Arc<dyn CenterRegistryPort>,
    espacio: Arc<dyn CenterWorkspacePort>,
    proyeccion: Arc<dyn LakehouseProjectionPort>,
    parser: Arc<dyn DocumentParserPort>,
    destilador: Arc<dyn LlmDistillerPort>,
    embeddings: Arc<dyn EmbeddingEnginePort>,
    modelo_embeddings: ModeloEmbeddingsActivo,
    herramientas: Arc<dyn ToolServerPort>,
    catalogo_modelos: Arc<dyn ModelCatalogPort>,
    orquestador: AgentOrchestrator,
    /// Centros abiertos (manifiesto validado y almacén conectado)
    abiertos: Mutex<HashMap<CenterId, CentroAbierto>>,
    /// Serializa las operaciones que cambian el registro o las vistas
    cambios: Mutex<()>,
    /// Una ingesta cada vez: dos ingestas simultáneas (dos drops en la ventana) se encolan
    ingestas: Mutex<()>,
}

impl LakehouseApp {
    pub fn new(dependencias: DependenciasLakehouse) -> Self {
        let orquestador =
            AgentOrchestrator::new(dependencias.agente, Arc::clone(&dependencias.herramientas));
        Self {
            registro: dependencias.registro,
            espacio: dependencias.espacio,
            proyeccion: dependencias.proyeccion,
            parser: dependencias.parser,
            destilador: dependencias.destilador,
            embeddings: dependencias.embeddings,
            modelo_embeddings: dependencias.modelo_embeddings,
            herramientas: dependencias.herramientas,
            catalogo_modelos: dependencias.catalogo_modelos,
            orquestador,
            abiertos: Mutex::new(HashMap::new()),
            cambios: Mutex::new(()),
            ingestas: Mutex::new(()),
        }
    }

    /// Servidor de herramientas sobre las vistas reconstruidas (lo usa el MCP)
    pub fn servidor_herramientas(&self) -> Arc<dyn ToolServerPort> {
        Arc::clone(&self.herramientas)
    }

    pub fn modelo_embeddings(&self) -> &ModeloEmbeddingsActivo {
        &self.modelo_embeddings
    }

    // --- Centros ---

    /// Crea la estructura del centro en `ruta` y lo registra **inactivo**
    pub async fn crear_centro(
        &self,
        id: &str,
        ruta: &str,
        nombre: Option<&str>,
    ) -> Result<CentroConocimiento, DomainError> {
        let _guardia = self.cambios.lock().await;
        let id = CenterId::new(id)?;
        let ruta = self.espacio.ruta_absoluta(&validar_ruta_centro(ruta)?)?;
        let nombre = nombre
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(id.as_str())
            .to_string();
        self.comprobar_alta_libre(&id, &ruta).await?;

        let manifiesto = ManifiestoCentroDisco::nuevo(
            id.as_str(),
            &nombre,
            &self.modelo_embeddings,
            chrono::Utc::now().to_rfc3339(),
        );
        let en_disco = self
            .espacio
            .preparar(&ruta, &manifiesto, &self.modelo_embeddings)
            .await?;
        if en_disco.centro_id != id.as_str() {
            return Err(DomainError::ValidationError(format!(
                "La ruta '{}' ya contiene el centro '{}': no se puede registrar como '{id}'",
                ruta.display(),
                en_disco.centro_id
            )));
        }

        let mut centro =
            CentroConocimiento::new(CenterManifest::new(id, nombre, "", ruta.to_string_lossy())?);
        centro.desactivar();
        self.registro.registrar(&centro).await?;
        Ok(centro)
    }

    async fn comprobar_alta_libre(&self, id: &CenterId, ruta: &Path) -> Result<(), DomainError> {
        let registrados = self.registro.listar_todos().await?;
        if registrados.iter().any(|c| c.id() == id) {
            return Err(DomainError::ValidationError(format!(
                "El centro '{id}' ya está registrado"
            )));
        }
        let ruta_texto = ruta.to_string_lossy().to_lowercase();
        if let Some(otro) = registrados
            .iter()
            .find(|c| c.manifiesto().ruta_almacenamiento.to_lowercase() == ruta_texto)
        {
            return Err(DomainError::ValidationError(format!(
                "La ruta '{}' ya pertenece al centro '{}'",
                ruta.display(),
                otro.id()
            )));
        }
        Ok(())
    }

    pub async fn listar_centros(&self) -> Result<Vec<CentroConocimiento>, DomainError> {
        let mut centros = self.registro.listar_todos().await?;
        centros.sort_by(|a, b| a.id().as_str().cmp(b.id().as_str()));
        Ok(centros)
    }

    /// Valida el manifiesto, reconstruye las vistas y marca el centro como activo
    pub async fn activar_centro(&self, id: &str) -> Result<ResumenProyeccion, DomainError> {
        let _guardia = self.cambios.lock().await;
        let centro = self.centro_registrado(id).await?;
        let abierto = self.abrir(&centro).await?;
        let resumen = self.proyeccion.reconstruir_vistas(&abierto).await?;
        self.registro.cambiar_estado(centro.id(), true).await?;
        Ok(resumen)
    }

    /// Retira las vistas (cero acceso) y marca el centro como inactivo
    pub async fn desactivar_centro(&self, id: &str) -> Result<(), DomainError> {
        let _guardia = self.cambios.lock().await;
        let centro = self.centro_registrado(id).await?;
        self.retirar(centro.id()).await?;
        self.registro.cambiar_estado(centro.id(), false).await
    }

    /// Quita el centro del registro sin borrar nada del disco
    pub async fn eliminar_centro(&self, id: &str) -> Result<CentroConocimiento, DomainError> {
        let _guardia = self.cambios.lock().await;
        let centro = self.centro_registrado(id).await?;
        self.retirar(centro.id()).await?;
        self.registro.eliminar(centro.id()).await
    }

    async fn retirar(&self, id: &CenterId) -> Result<(), DomainError> {
        self.proyeccion.retirar_vistas(id).await?;
        self.abiertos.lock().await.remove(id);
        Ok(())
    }

    /// Reconstruye las vistas de los centros marcados como activos (arranque de GUI y MCP)
    pub async fn hidratar_centros_activos(&self) -> Result<Vec<HidratacionCentro>, DomainError> {
        let _guardia = self.cambios.lock().await;
        let mut resultados = Vec::new();
        for centro in self.registro.listar_activos().await? {
            let resultado = match self.abrir(&centro).await {
                Ok(abierto) => self.proyeccion.reconstruir_vistas(&abierto).await,
                Err(e) => Err(e),
            };
            resultados.push(HidratacionCentro {
                centro_id: centro.id().to_string(),
                resultado: resultado.map_err(|e| e.to_string()),
            });
        }
        Ok(resultados)
    }

    async fn centro_registrado(&self, id: &str) -> Result<CentroConocimiento, DomainError> {
        let id = CenterId::new(id)?;
        self.registro.obtener(&id).await?.ok_or_else(|| {
            DomainError::InvalidCenterId(format!(
                "El centro '{id}' no está registrado (usa --centros para ver los disponibles)"
            ))
        })
    }

    async fn abrir(&self, centro: &CentroConocimiento) -> Result<CentroAbierto, DomainError> {
        if let Some(abierto) = self.abiertos.lock().await.get(centro.id()) {
            return Ok(abierto.clone());
        }
        let ruta = Path::new(&centro.manifiesto().ruta_almacenamiento);
        let abierto = self
            .espacio
            .abrir(centro.id(), ruta, &self.modelo_embeddings)
            .await?;
        self.abiertos
            .lock()
            .await
            .insert(centro.id().clone(), abierto.clone());
        Ok(abierto)
    }

    // --- Ingesta ---

    /// Copia el original a `raw_docs/`, ingiere de forma transaccional y, si el centro está
    /// activo y hubo cambios, reconstruye sus vistas
    pub async fn ingestar(
        &self,
        ruta_archivo: &Path,
        centro_id: &str,
    ) -> Result<ResultadoIngesta, DomainError> {
        self.ingestar_con_progreso(ruta_archivo, centro_id, None)
            .await
    }

    /// `ingestar` notificando cada etapa (`EtapaIngesta`) al observador
    pub async fn ingestar_con_progreso(
        &self,
        ruta_archivo: &Path,
        centro_id: &str,
        observador: Option<ObservadorIngesta>,
    ) -> Result<ResultadoIngesta, DomainError> {
        let ruta_archivo = validar_ruta_documento(&ruta_archivo.to_string_lossy())?;
        let _turno = self.ingestas.lock().await;
        let centro = self.centro_registrado(centro_id).await?;
        let abierto = self.abrir(&centro).await?;
        notificar(&observador, EtapaIngesta::Copia);
        let original = self
            .espacio
            .guardar_original(&abierto.ruta, &ruta_archivo)
            .await?;

        let servicio = DocumentIngestionService::new(
            Arc::clone(&self.parser),
            Arc::clone(&self.destilador),
            Arc::clone(&self.embeddings),
            Arc::clone(&abierto.store),
        )
        .con_observador(observador.clone());
        let informe = servicio
            .ingestar_documento(centro.id(), &original.nombre_archivo, &original.contenido)
            .await?;

        let (proyeccion, aviso_proyeccion) = if centro.esta_activo() && !informe.ya_ingerido {
            let _guardia = self.cambios.lock().await;
            notificar(&observador, EtapaIngesta::Proyeccion);
            match self.proyeccion.reconstruir_vistas(&abierto).await {
                Ok(resumen) => (Some(resumen), None),
                Err(e) => (
                    None,
                    Some(format!(
                        "Ingesta confirmada, pero no se reconstruyeron las vistas: {e}"
                    )),
                ),
            }
        } else {
            (None, None)
        };

        Ok(ResultadoIngesta {
            informe,
            copia_original: original.ruta_copia.to_string_lossy().to_string(),
            proyeccion,
            aviso_proyeccion,
        })
    }

    // --- Consulta ---

    /// Pregunta de un solo turno (CLI): devuelve la respuesta y las herramientas usadas
    pub async fn preguntar(
        &self,
        texto: &str,
        centro_id: &str,
        modelo: Option<&str>,
    ) -> Result<RespuestaLakehouse, DomainError> {
        let texto = texto.trim();
        if texto.is_empty() {
            return Err(DomainError::ValidationError(
                "La pregunta está vacía".to_string(),
            ));
        }
        let centro = CenterId::new(centro_id)?;
        let modelo = match modelo {
            Some(alias) => ClaudeModel::new(alias)?,
            None => ClaudeModel::sonnet(),
        };

        let usos: Arc<StdMutex<Vec<UsoHerramienta>>> = Arc::default();
        let mut historial = vec![AgentMessage::user(texto)];
        let respuesta = self
            .conversar(
                &modelo,
                &centro,
                &mut historial,
                Some(registrador_de_herramientas(&usos)),
            )
            .await?;

        let herramientas = usos.lock().map(|u| u.clone()).unwrap_or_default();
        Ok(RespuestaLakehouse {
            centro_id: centro.to_string(),
            modelo: modelo.cli_model_name().to_string(),
            respuesta,
            herramientas,
        })
    }

    /// Garantiza que el centro está registrado, activo y con sus vistas cargadas
    async fn asegurar_proyectado(&self, id: &CenterId) -> Result<(), DomainError> {
        let centro = self.centro_registrado(id.as_str()).await?;
        if !centro.esta_activo() {
            return Err(DomainError::ValidationError(format!(
                "El centro '{id}' está inactivo: actívalo antes de consultarlo"
            )));
        }
        if self.proyeccion.centros_proyectados().await.contains(id) {
            return Ok(());
        }
        let _guardia = self.cambios.lock().await;
        let abierto = self.abrir(&centro).await?;
        self.proyeccion.reconstruir_vistas(&abierto).await?;
        Ok(())
    }

    // --- Integridad ---

    pub async fn integridad(
        &self,
        centro_id: &str,
    ) -> Result<InformeIntegridadCentro, DomainError> {
        let centro = self.centro_registrado(centro_id).await?;
        let abierto = self.abrir(&centro).await?;
        let existentes = abierto.store.listar_tablas().await?;
        let tablas_ausentes: Vec<String> = TODAS_LAS_TABLAS
            .iter()
            .filter(|t| !existentes.iter().any(|e| e == *t))
            .map(|t| t.to_string())
            .collect();
        let informe = if tablas_ausentes.is_empty() {
            abierto.store.verificar_integridad().await?
        } else {
            InformeIntegridad::default()
        };
        Ok(InformeIntegridadCentro {
            centro_id: centro.id().to_string(),
            informe,
            tablas_ausentes,
        })
    }
}

fn notificar(observador: &Option<ObservadorIngesta>, etapa: EtapaIngesta) {
    if let Some(observador) = observador {
        observador(etapa);
    }
}

fn registrador_de_herramientas(destino: &Arc<StdMutex<Vec<UsoHerramienta>>>) -> EventCallback {
    let destino = Arc::clone(destino);
    let pendientes: Arc<StdMutex<Vec<(String, serde_json::Value)>>> = Arc::default();
    Arc::new(move |evento| match evento {
        AgentExecutionEvent::CallingTool { name, arguments } => {
            if let Ok(mut p) = pendientes.lock() {
                p.push((name, arguments));
            }
        }
        AgentExecutionEvent::ToolResultReceived {
            name,
            result,
            is_error,
        } => {
            let argumentos = pendientes
                .lock()
                .ok()
                .and_then(|mut p| {
                    let idx = p.iter().position(|(n, _)| *n == name)?;
                    Some(p.remove(idx).1)
                })
                .unwrap_or(serde_json::Value::Null);
            if let Ok(mut usos) = destino.lock() {
                usos.push(UsoHerramienta {
                    nombre: name,
                    argumentos,
                    es_error: is_error,
                    resultado: result,
                });
            }
        }
        _ => {}
    })
}

#[async_trait]
impl AsistenteLakehousePort for LakehouseApp {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        self.catalogo_modelos.listar_modelos().await
    }

    async fn conversar(
        &self,
        modelo: &ClaudeModel,
        centro_id: &CenterId,
        historial: &mut Vec<AgentMessage>,
        on_event: Option<EventCallback>,
    ) -> Result<String, DomainError> {
        self.asegurar_proyectado(centro_id).await?;
        self.orquestador
            .procesar_turno(modelo, centro_id, historial, on_event)
            .await
    }
}

/// Asistente sin centros: delega directamente en el orquestador (tests y usos embebidos)
pub struct AsistenteDirecto {
    orquestador: AgentOrchestrator,
    catalogo: Arc<dyn ModelCatalogPort>,
}

impl AsistenteDirecto {
    pub fn new(orquestador: AgentOrchestrator, catalogo: Arc<dyn ModelCatalogPort>) -> Self {
        Self {
            orquestador,
            catalogo,
        }
    }
}

#[async_trait]
impl AsistenteLakehousePort for AsistenteDirecto {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        self.catalogo.listar_modelos().await
    }

    async fn conversar(
        &self,
        modelo: &ClaudeModel,
        centro_id: &CenterId,
        historial: &mut Vec<AgentMessage>,
        on_event: Option<EventCallback>,
    ) -> Result<String, DomainError> {
        self.orquestador
            .procesar_turno(modelo, centro_id, historial, on_event)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_registrador_empareja_argumentos_con_resultados() {
        let usos: Arc<StdMutex<Vec<UsoHerramienta>>> = Arc::default();
        let cb = registrador_de_herramientas(&usos);
        cb(AgentExecutionEvent::CallingTool {
            name: "execute_athena_sql".into(),
            arguments: json!({"sql_query": "SELECT 1"}),
        });
        cb(AgentExecutionEvent::ToolResultReceived {
            name: "execute_athena_sql".into(),
            result: json!([{"x": 1}]),
            is_error: false,
        });
        cb(AgentExecutionEvent::ToolResultReceived {
            name: "huerfana".into(),
            result: json!(null),
            is_error: true,
        });
        let usos = usos.lock().unwrap();
        assert_eq!(usos.len(), 2);
        assert_eq!(usos[0].argumentos["sql_query"], "SELECT 1");
        assert!(!usos[0].es_error);
        assert_eq!(usos[1].argumentos, serde_json::Value::Null);
    }

    #[test]
    fn test_integridad_con_tablas_ausentes_no_es_integra() {
        let informe = InformeIntegridadCentro {
            centro_id: "obra-a".into(),
            informe: InformeIntegridad::default(),
            tablas_ausentes: vec!["03_matriz_operativa".into()],
        };
        assert!(!informe.es_integro());
    }
}
