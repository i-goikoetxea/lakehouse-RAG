//! Motor del LLM conmutable sin reiniciar la app. El motor activo (runner del agente,
//! verificador de procedencia y catálogo de modelos) vive detrás de un `RwLock` y se recompone
//! cuando la ventana guarda otro motor, otra URL del proxy o una key.
//!
//! Una consulta nunca cambia de motor a mitad: el orquestador pide `motor_de_la_consulta` al
//! empezarla y usa ese runner y ese verificador hasta el final. El catálogo se lee al pedirlo.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::application::dtos::agent::{
    AgentMessage, AgentTurnOutput, ClaudeModel, ModelDescriptor, TurnoDirigido,
};
use crate::application::dtos::procedencia::{SolicitudProcedencia, TrazasPropuestas};
use crate::application::dtos::tools::{LakehouseToolName, ToolDefinition};
use crate::application::ports::agent_runner::{AgentRunnerPort, ObservadorTexto};
use crate::application::ports::model_catalog::ModelCatalogPort;
use crate::application::ports::verificador_procedencia::VerificadorProcedenciaPort;
use crate::domain::errors::DomainError;
use crate::infrastructure::llm::ClaudeAgentConfig;
use crate::infrastructure::llm::anthropic_api::AnthropicMessagesRunner;
use crate::infrastructure::llm::modo_llm::{
    ModoLlm, agente_para, catalogo_modelos_para, verificador_procedencia_para,
};
use crate::infrastructure::llm::preferencia_modo::ModoResuelto;

/// Un motor compuesto: lo pedido, lo que quedó en uso (`Cli` si faltaba la key) y sus piezas
pub struct MotorCompuesto {
    pub pedido: ModoResuelto,
    pub en_uso: ModoLlm,
    pub freellmapi_url: String,
    pub runner: Arc<dyn AgentRunnerPort>,
    /// El runner de la API, para leer sus métricas (A/B); `None` en `Cli`
    pub api: Option<Arc<AnthropicMessagesRunner>>,
    pub verificador: Arc<dyn VerificadorProcedenciaPort>,
    pub catalogo: Arc<dyn ModelCatalogPort>,
}

/// Compone el motor de un modo con la URL del proxy
pub type Compositor = Arc<dyn Fn(ModoResuelto, &str) -> MotorCompuesto + Send + Sync>;

/// La composición real: la misma que se hacía al arrancar
pub fn compositor_real(config_cli: ClaudeAgentConfig) -> Compositor {
    Arc::new(move |pedido: ModoResuelto, url: &str| {
        let agente = agente_para(pedido.modo, url, config_cli.clone());
        MotorCompuesto {
            pedido,
            en_uso: agente.en_uso,
            freellmapi_url: url.to_string(),
            runner: agente.runner,
            api: agente.api,
            verificador: verificador_procedencia_para(
                agente.en_uso,
                url,
                config_cli.binary_path.clone(),
            ),
            catalogo: catalogo_modelos_para(agente.en_uso, url),
        }
    })
}

pub struct MotorConmutable {
    compositor: Compositor,
    actual: RwLock<Arc<MotorCompuesto>>,
}

impl MotorConmutable {
    pub fn new(compositor: Compositor, pedido: ModoResuelto, freellmapi_url: &str) -> Arc<Self> {
        let inicial = compositor(pedido, freellmapi_url);
        Arc::new(Self {
            compositor,
            actual: RwLock::new(Arc::new(inicial)),
        })
    }

    /// El motor activo ahora
    pub fn actual(&self) -> Arc<MotorCompuesto> {
        let guarda = self.actual.read().unwrap_or_else(|e| e.into_inner());
        Arc::clone(&guarda)
    }

    /// Recompone el motor si cambia el modo pedido, o la URL con el proxy pedido, o si
    /// `forzar` (se guardó la key del motor pedido). Las consultas en curso siguen con el
    /// motor que tenían. Devuelve el motor activo tras aplicar
    pub fn aplicar(
        &self,
        pedido: ModoResuelto,
        freellmapi_url: &str,
        forzar: bool,
    ) -> Arc<MotorCompuesto> {
        let actual = self.actual();
        let cambia_url =
            pedido.modo == ModoLlm::FreeLlmApi && actual.freellmapi_url != freellmapi_url;
        if !forzar && actual.pedido.modo == pedido.modo && !cambia_url {
            if actual.pedido != pedido {
                // Mismo motor por otro origen: solo se anota el origen
                let mismo = MotorCompuesto {
                    pedido,
                    en_uso: actual.en_uso,
                    freellmapi_url: actual.freellmapi_url.clone(),
                    runner: Arc::clone(&actual.runner),
                    api: actual.api.clone(),
                    verificador: Arc::clone(&actual.verificador),
                    catalogo: Arc::clone(&actual.catalogo),
                };
                return self.sustituir(mismo);
            }
            return actual;
        }
        let nuevo = (self.compositor)(pedido, freellmapi_url);
        tracing::info!(
            pedido = nuevo.pedido.modo.nombre(),
            en_uso = nuevo.en_uso.nombre(),
            "motor del LLM cambiado sin reiniciar"
        );
        self.sustituir(nuevo)
    }

    fn sustituir(&self, nuevo: MotorCompuesto) -> Arc<MotorCompuesto> {
        let nuevo = Arc::new(nuevo);
        let mut guarda = self.actual.write().unwrap_or_else(|e| e.into_inner());
        *guarda = Arc::clone(&nuevo);
        nuevo
    }

    pub fn runner(self: &Arc<Self>) -> Arc<dyn AgentRunnerPort> {
        Arc::new(RunnerConmutable(Arc::clone(self)))
    }

    pub fn verificador(self: &Arc<Self>) -> Arc<dyn VerificadorProcedenciaPort> {
        Arc::new(VerificadorConmutable(Arc::clone(self)))
    }

    pub fn catalogo(self: &Arc<Self>) -> Arc<dyn ModelCatalogPort> {
        Arc::new(CatalogoConmutable(Arc::clone(self)))
    }
}

/// Delega en el runner activo. El orquestador lo fija al empezar cada consulta
struct RunnerConmutable(Arc<MotorConmutable>);

#[async_trait]
impl AgentRunnerPort for RunnerConmutable {
    async fn ejecutar_turno(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        let runner = Arc::clone(&self.0.actual().runner);
        runner
            .ejecutar_turno(modelo, system_prompt, historial, herramientas)
            .await
    }

    async fn ejecutar_turno_en_vivo(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
        observador: ObservadorTexto,
    ) -> Result<AgentTurnOutput, DomainError> {
        let runner = Arc::clone(&self.0.actual().runner);
        runner
            .ejecutar_turno_en_vivo(modelo, system_prompt, historial, herramientas, observador)
            .await
    }

    async fn ejecutar_turno_dirigido(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
        dirigida: LakehouseToolName,
    ) -> Result<TurnoDirigido, DomainError> {
        let runner = Arc::clone(&self.0.actual().runner);
        runner
            .ejecutar_turno_dirigido(modelo, system_prompt, historial, herramientas, dirigida)
            .await
    }

    async fn ejecutar_turno_de_cierre(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        let runner = Arc::clone(&self.0.actual().runner);
        runner
            .ejecutar_turno_de_cierre(modelo, system_prompt, historial, herramientas)
            .await
    }

    fn necesita_correccion_de_omision(&self) -> bool {
        self.0.actual().runner.necesita_correccion_de_omision()
    }

    fn motor_de_la_consulta(&self) -> Option<Arc<dyn AgentRunnerPort>> {
        Some(Arc::clone(&self.0.actual().runner))
    }
}

struct VerificadorConmutable(Arc<MotorConmutable>);

#[async_trait]
impl VerificadorProcedenciaPort for VerificadorConmutable {
    async fn proponer_trazas(
        &self,
        solicitud: &SolicitudProcedencia,
    ) -> Result<TrazasPropuestas, DomainError> {
        let verificador = Arc::clone(&self.0.actual().verificador);
        verificador.proponer_trazas(solicitud).await
    }

    fn motor_de_la_consulta(&self) -> Option<Arc<dyn VerificadorProcedenciaPort>> {
        Some(Arc::clone(&self.0.actual().verificador))
    }
}

struct CatalogoConmutable(Arc<MotorConmutable>);

#[async_trait]
impl ModelCatalogPort for CatalogoConmutable {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        let catalogo = Arc::clone(&self.0.actual().catalogo);
        catalogo.listar_modelos().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::llm::preferencia_modo::OrigenModo;
    use std::sync::Mutex;

    /// Runner que responde con su nombre
    struct Nombrado(&'static str);

    #[async_trait]
    impl AgentRunnerPort for Nombrado {
        async fn ejecutar_turno(
            &self,
            _modelo: &ClaudeModel,
            _system_prompt: &str,
            _historial: &[AgentMessage],
            _herramientas: &[ToolDefinition],
        ) -> Result<AgentTurnOutput, DomainError> {
            Ok(AgentTurnOutput::FinalMessage(self.0.to_string()))
        }
    }

    #[async_trait]
    impl VerificadorProcedenciaPort for Nombrado {
        async fn proponer_trazas(
            &self,
            _solicitud: &SolicitudProcedencia,
        ) -> Result<TrazasPropuestas, DomainError> {
            Err(DomainError::AgentError(self.0.to_string()))
        }
    }

    #[async_trait]
    impl ModelCatalogPort for Nombrado {
        async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
            Err(DomainError::AgentError(self.0.to_string()))
        }
    }

    fn pedido(modo: ModoLlm) -> ModoResuelto {
        ModoResuelto {
            modo,
            origen: OrigenModo::Fichero,
        }
    }

    /// Compositor de prueba: cuenta las composiciones y nombra las piezas por el modo
    fn compositor(composiciones: Arc<Mutex<usize>>) -> Compositor {
        Arc::new(move |pedido: ModoResuelto, url: &str| {
            *composiciones.lock().unwrap() += 1;
            let nombre = pedido.modo.nombre();
            MotorCompuesto {
                pedido,
                en_uso: pedido.modo,
                freellmapi_url: url.to_string(),
                runner: Arc::new(Nombrado(nombre)),
                api: None,
                verificador: Arc::new(Nombrado(nombre)),
                catalogo: Arc::new(Nombrado(nombre)),
            }
        })
    }

    async fn responde(runner: &dyn AgentRunnerPort) -> String {
        match runner
            .ejecutar_turno(&ClaudeModel::sonnet(), "", &[], &[])
            .await
            .unwrap()
        {
            AgentTurnOutput::FinalMessage(texto) => texto,
            otro => panic!("{otro:?}"),
        }
    }

    #[tokio::test]
    async fn test_aplicar_otro_modo_cambia_runner_verificador_y_catalogo_al_instante() {
        let composiciones = Arc::default();
        let motor = MotorConmutable::new(
            compositor(Arc::clone(&composiciones)),
            pedido(ModoLlm::Cli),
            "u",
        );
        let (runner, verificador, catalogo) =
            (motor.runner(), motor.verificador(), motor.catalogo());
        assert_eq!(responde(runner.as_ref()).await, "cli");
        let aplicado = motor.aplicar(pedido(ModoLlm::Api), "u", false);
        assert_eq!(aplicado.en_uso, ModoLlm::Api);
        assert_eq!(responde(runner.as_ref()).await, "api");
        let solicitud = SolicitudProcedencia {
            pregunta: String::new(),
            salidas: vec![],
            borrador: String::new(),
            cifras: vec![],
        };
        assert!(
            verificador
                .proponer_trazas(&solicitud)
                .await
                .unwrap_err()
                .to_string()
                .contains("api")
        );
        assert!(
            catalogo
                .listar_modelos()
                .await
                .unwrap_err()
                .to_string()
                .contains("api")
        );
        assert_eq!(*composiciones.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn test_el_motor_de_la_consulta_no_cambia_aunque_se_aplique_otro() {
        let motor = MotorConmutable::new(compositor(Arc::default()), pedido(ModoLlm::Cli), "u");
        let fijado = motor.runner().motor_de_la_consulta().unwrap();
        motor.aplicar(pedido(ModoLlm::FreeLlmApi), "u", false);
        assert_eq!(responde(fijado.as_ref()).await, "cli");
        assert_eq!(responde(motor.runner().as_ref()).await, "freellmapi");
    }

    #[test]
    fn test_sin_cambios_no_se_recompone_y_forzar_o_la_url_del_proxy_si() {
        let composiciones: Arc<Mutex<usize>> = Arc::default();
        let motor = MotorConmutable::new(
            compositor(Arc::clone(&composiciones)),
            pedido(ModoLlm::FreeLlmApi),
            "http://a",
        );
        motor.aplicar(pedido(ModoLlm::FreeLlmApi), "http://a", false);
        assert_eq!(*composiciones.lock().unwrap(), 1, "mismo motor y URL");
        // Otro origen del mismo motor: se anota sin recomponer
        let por_entorno = ModoResuelto {
            modo: ModoLlm::FreeLlmApi,
            origen: OrigenModo::Entorno,
        };
        assert_eq!(
            motor.aplicar(por_entorno, "http://a", false).pedido,
            por_entorno
        );
        assert_eq!(*composiciones.lock().unwrap(), 1);
        motor.aplicar(pedido(ModoLlm::FreeLlmApi), "http://b", false);
        assert_eq!(*composiciones.lock().unwrap(), 2, "otra URL del proxy");
        motor.aplicar(pedido(ModoLlm::FreeLlmApi), "http://b", true);
        assert_eq!(*composiciones.lock().unwrap(), 3, "forzado (key guardada)");
    }

    #[test]
    fn test_la_url_no_recompone_un_motor_que_no_es_el_proxy() {
        let composiciones: Arc<Mutex<usize>> = Arc::default();
        let motor = MotorConmutable::new(
            compositor(Arc::clone(&composiciones)),
            pedido(ModoLlm::Api),
            "http://a",
        );
        motor.aplicar(pedido(ModoLlm::Api), "http://b", false);
        assert_eq!(*composiciones.lock().unwrap(), 1);
    }
}
