//! Puerto del Nivel 2 del verificador de procedencia: una microllamada que propone trazas

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::dtos::procedencia::{SolicitudProcedencia, TrazasPropuestas};
use crate::domain::errors::DomainError;

/// Propone, para cada cifra sin anclar, de dónde sale. Un error o un timeout dejan el
/// comportamiento de hoy: la cifra cuenta como no anclada (falla cerrada).
#[async_trait]
pub trait VerificadorProcedenciaPort: Send + Sync {
    async fn proponer_trazas(
        &self,
        solicitud: &SolicitudProcedencia,
    ) -> Result<TrazasPropuestas, DomainError>;

    /// Como `AgentRunnerPort::motor_de_la_consulta`: el verificador activo al empezar la
    /// consulta. Por defecto `None`: el propio verificador
    fn motor_de_la_consulta(&self) -> Option<Arc<dyn VerificadorProcedenciaPort>> {
        None
    }
}
