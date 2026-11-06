//! Puerto de aplicación para conocer la fecha de hoy sin leer el reloj del sistema en el núcleo

use chrono::NaiveDate;

/// Fecha de hoy del operador. Claude Code se la da al modelo, y el arnés la necesita para anclar
/// una respuesta que la cite («Hoy (2026-11-03)»).
pub trait RelojPort: Send + Sync {
    fn hoy(&self) -> NaiveDate;
}
