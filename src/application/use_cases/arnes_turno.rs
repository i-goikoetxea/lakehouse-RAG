//! Arnés determinista del turno de chat (`docs/arquitectura/ssd_arnes_determinista.md`). Lógica
//! pura: clasifica el mensaje del operador, planifica el primer paso y valida la respuesta final.

mod cifras;
mod fechas;
mod intent_detector;
mod post_inference_guard;
mod pre_inference_interceptor;
mod texto_normalizado;
mod verificador_paginas;

pub use intent_detector::{IntencionTurno, IntentDetector};
pub use post_inference_guard::{PostInferenceGuard, RegistroTurno, ViolacionArnes};
pub use pre_inference_interceptor::{EleccionHerramienta, PlanTurno, PreInferenceInterceptor};
pub use verificador_paginas::{
    CitaRevisada, ENCABEZADO_VERIFICACION_PAGINAS, Paginas, ResultadoCita, VerificadorPaginas,
    formatear_paginas,
};
