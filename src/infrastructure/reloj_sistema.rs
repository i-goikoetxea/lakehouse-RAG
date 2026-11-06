//! Adaptador del reloj: la fecha local del equipo, la misma que Claude Code da al modelo

use chrono::{Local, NaiveDate};

use crate::application::ports::reloj::RelojPort;

pub struct RelojDelSistema;

impl RelojPort for RelojDelSistema {
    fn hoy(&self) -> NaiveDate {
        Local::now().date_naive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_el_reloj_del_sistema_da_la_fecha_local() {
        let antes = Local::now().date_naive();
        let hoy = RelojDelSistema.hoy();
        let despues = Local::now().date_naive();
        assert!(antes <= hoy && hoy <= despues);
    }
}
