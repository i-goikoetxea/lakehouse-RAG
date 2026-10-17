//! Caso de uso: Búsqueda determinista en tablas de doble entrada (TableLookupService)

use std::collections::HashMap;
use std::sync::RwLock;

use arrow_array::{Array, BooleanArray, Float64Array, RecordBatch, StringArray};
use async_trait::async_trait;

use crate::application::ports::lakehouse_store::EstadoIngesta;
use crate::application::ports::table_lookup::TableLookupPort;
use crate::domain::errors::DomainError;
use crate::domain::models::document::DocId;
use crate::domain::models::double_entry_table::{CeldaDobleEntrada, LimiteRango, RangoNumerico};

/// Servicio de resolución de tablas de doble entrada con almacenamiento indexado en memoria
pub struct TableLookupService {
    celdas_por_tabla: RwLock<HashMap<String, Vec<CeldaDobleEntrada>>>,
}

impl Default for TableLookupService {
    fn default() -> Self {
        Self::new()
    }
}

impl TableLookupService {
    pub fn new() -> Self {
        Self {
            celdas_por_tabla: RwLock::new(HashMap::new()),
        }
    }

    /// Elimina todas las celdas (la proyección recarga TableLookup desde LanceDB)
    pub fn vaciar(&self) -> Result<(), DomainError> {
        self.celdas_por_tabla
            .write()
            .map_err(|_| {
                DomainError::StorageError("Error de contención en lock de celdas".to_string())
            })?
            .clear();
        Ok(())
    }

    /// Registra directamente una lista de celdas de dominio
    pub fn registrar_celdas(&self, celdas: Vec<CeldaDobleEntrada>) -> Result<(), DomainError> {
        let mut mapa = self.celdas_por_tabla.write().map_err(|_| {
            DomainError::StorageError("Error de contención en lock de celdas".to_string())
        })?;

        for celda in celdas {
            if celda.vigente {
                mapa.entry(celda.nombre_tabla.clone())
                    .or_default()
                    .push(celda);
            }
        }
        Ok(())
    }

    /// Hidrata celdas a partir de un RecordBatch de Arrow correspondiente a `05_celdas_doble_entrada` (esquema v2; solo filas COMMITTED y vigentes)
    pub fn cargar_desde_record_batch(&self, batch: &RecordBatch) -> Result<usize, DomainError> {
        let celda_id_arr = batch
            .column_by_name("celda_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                DomainError::StorageError("Columna celda_id no encontrada o inválida".to_string())
            })?;

        let tabla_id_arr = batch
            .column_by_name("tabla_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                DomainError::StorageError("Columna tabla_id no encontrada o inválida".to_string())
            })?;

        let doc_id_arr = batch
            .column_by_name("doc_id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                DomainError::StorageError("Columna doc_id no encontrada o inválida".to_string())
            })?;

        let rango_x_min_arr = batch
            .column_by_name("eje_x_min")
            .and_then(|c| c.as_any().downcast_ref::<Float64Array>());
        let rango_x_max_arr = batch
            .column_by_name("eje_x_max")
            .and_then(|c| c.as_any().downcast_ref::<Float64Array>());
        let x_inc_min_arr = batch
            .column_by_name("eje_x_min_inclusivo")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>());
        let x_inc_max_arr = batch
            .column_by_name("eje_x_max_inclusivo")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>());
        let cat_x_arr = batch
            .column_by_name("eje_x_categoria")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());

        let rango_y_min_arr = batch
            .column_by_name("eje_y_min")
            .and_then(|c| c.as_any().downcast_ref::<Float64Array>());
        let rango_y_max_arr = batch
            .column_by_name("eje_y_max")
            .and_then(|c| c.as_any().downcast_ref::<Float64Array>());
        let y_inc_min_arr = batch
            .column_by_name("eje_y_min_inclusivo")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>());
        let y_inc_max_arr = batch
            .column_by_name("eje_y_max_inclusivo")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>());
        let cat_y_arr = batch
            .column_by_name("eje_y_categoria")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());

        let valor_resultado_arr = batch
            .column_by_name("valor_resultado_texto")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| {
                DomainError::StorageError(
                    "Columna valor_resultado_texto no encontrada o inválida".to_string(),
                )
            })?;

        let unidad_arr = batch
            .column_by_name("unidad_medida")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());

        let notas_arr = batch
            .column_by_name("nota_pie_condicion")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());

        let vigente_arr = batch
            .column_by_name("vigente")
            .and_then(|c| c.as_any().downcast_ref::<BooleanArray>())
            .ok_or_else(|| {
                DomainError::StorageError("Columna vigente no encontrada o inválida".to_string())
            })?;

        let estado_arr = batch
            .column_by_name("estado_ingesta")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());

        let mut celdas = Vec::with_capacity(batch.num_rows());

        for i in 0..batch.num_rows() {
            let confirmada =
                estado_arr.is_none_or(|a| a.value(i) == EstadoIngesta::Committed.as_str());
            if !vigente_arr.value(i) || !confirmada {
                continue;
            }

            let celda_id = celda_id_arr.value(i);
            let tabla_id = tabla_id_arr.value(i);
            let doc_id = DocId::new(doc_id_arr.value(i))?;

            // Construir rango X si está presente
            let rango_x = if let (Some(min_arr), Some(max_arr)) = (rango_x_min_arr, rango_x_max_arr)
            {
                let min_val = if min_arr.is_valid(i) {
                    Some(min_arr.value(i))
                } else {
                    None
                };
                let max_val = if max_arr.is_valid(i) {
                    Some(max_arr.value(i))
                } else {
                    None
                };

                let inc_min = x_inc_min_arr.map(|a| a.value(i)).unwrap_or(true);
                let inc_max = x_inc_max_arr.map(|a| a.value(i)).unwrap_or(true);

                Self::construir_rango_opcional(min_val, max_val, inc_min, inc_max)?
            } else {
                None
            };

            // Construir rango Y si está presente
            let rango_y = if let (Some(min_arr), Some(max_arr)) = (rango_y_min_arr, rango_y_max_arr)
            {
                let min_val = if min_arr.is_valid(i) {
                    Some(min_arr.value(i))
                } else {
                    None
                };
                let max_val = if max_arr.is_valid(i) {
                    Some(max_arr.value(i))
                } else {
                    None
                };

                let inc_min = y_inc_min_arr.map(|a| a.value(i)).unwrap_or(true);
                let inc_max = y_inc_max_arr.map(|a| a.value(i)).unwrap_or(true);

                Self::construir_rango_opcional(min_val, max_val, inc_min, inc_max)?
            } else {
                None
            };

            let cat_x = cat_x_arr
                .filter(|a| a.is_valid(i))
                .map(|a| a.value(i).to_string())
                .unwrap_or_default();
            let cat_y = cat_y_arr
                .filter(|a| a.is_valid(i))
                .map(|a| a.value(i).to_string())
                .unwrap_or_default();

            let valor_res = valor_resultado_arr.value(i);
            let unidad = unidad_arr
                .filter(|a| a.is_valid(i))
                .map(|a| a.value(i).to_string());

            let notas = notas_arr
                .filter(|a| a.is_valid(i))
                .map(|a| vec![a.value(i).to_string()])
                .unwrap_or_default();

            let celda = CeldaDobleEntrada::new(
                celda_id, doc_id, tabla_id, cat_x, rango_x, cat_y, rango_y, valor_res, unidad,
                notas,
            )?;

            celdas.push(celda);
        }

        let total = celdas.len();
        self.registrar_celdas(celdas)?;
        Ok(total)
    }

    fn construir_rango_opcional(
        min: Option<f64>,
        max: Option<f64>,
        inc_min: bool,
        inc_max: bool,
    ) -> Result<Option<RangoNumerico>, DomainError> {
        match (min, max) {
            (Some(min_val), Some(max_val)) => {
                let inf = if inc_min {
                    LimiteRango::Inclusivo(min_val)
                } else {
                    LimiteRango::Exclusivo(min_val)
                };
                let sup = if inc_max {
                    LimiteRango::Inclusivo(max_val)
                } else {
                    LimiteRango::Exclusivo(max_val)
                };
                let rango = RangoNumerico::new(inf, sup)?;
                Ok(Some(rango))
            }
            (Some(min_val), None) => {
                let rango = if inc_min {
                    RangoNumerico::mayor_o_igual_que(min_val)
                } else {
                    RangoNumerico::mayor_que(min_val)
                };
                Ok(Some(rango))
            }
            (None, Some(max_val)) => {
                let rango = if inc_max {
                    RangoNumerico::menor_o_igual_que(max_val)
                } else {
                    RangoNumerico::menor_que(max_val)
                };
                Ok(Some(rango))
            }
            (None, None) => Ok(None),
        }
    }
}

#[async_trait]
impl TableLookupPort for TableLookupService {
    async fn buscar_celda(
        &self,
        nombre_tabla: &str,
        etiqueta_fila: Option<&str>,
        valor_fila: Option<f64>,
        etiqueta_columna: Option<&str>,
        valor_columna: Option<f64>,
    ) -> Result<Option<CeldaDobleEntrada>, DomainError> {
        let mapa = self.celdas_por_tabla.read().map_err(|_| {
            DomainError::StorageError("Error de contención en lock de celdas".to_string())
        })?;

        if let Some(celdas) = mapa.get(nombre_tabla) {
            for celda in celdas {
                if celda.coincide(etiqueta_fila, valor_fila, etiqueta_columna, valor_columna) {
                    return Ok(Some(celda.clone()));
                }
            }
        }

        Ok(None)
    }

    async fn listar_celdas_por_tabla(
        &self,
        nombre_tabla: &str,
    ) -> Result<Vec<CeldaDobleEntrada>, DomainError> {
        let mapa = self.celdas_por_tabla.read().map_err(|_| {
            DomainError::StorageError("Error de contención en lock de celdas".to_string())
        })?;

        Ok(mapa.get(nombre_tabla).cloned().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_table_lookup_service_cruce_exacto_y_rango() {
        let service = TableLookupService::new();
        let doc_id = DocId::new("DOC-TUBERIAS-01").unwrap();
        let rango_presion = RangoNumerico::entre_inclusivo(10.0, 50.0).unwrap();

        let celda = CeldaDobleEntrada::new(
            "C-01",
            doc_id,
            "TABLA_ESPESORES",
            "Acero al Carbón",
            None,
            "Media Presión",
            Some(rango_presion),
            "12.7",
            Some("mm".to_string()),
            vec!["Válido para fluidos clase 1".to_string()],
        )
        .unwrap();

        service.registrar_celdas(vec![celda]).unwrap();

        // Consulta exitosa
        let resultado = service
            .buscar_celda(
                "TABLA_ESPESORES",
                Some("Acero al Carbón"),
                None,
                None,
                Some(25.0),
            )
            .await
            .unwrap();

        assert!(resultado.is_some());
        let celda_encontrada = resultado.unwrap();
        assert_eq!(celda_encontrada.valor_resultado, "12.7");
        assert_eq!(celda_encontrada.unidad_medida.as_deref(), Some("mm"));
        assert_eq!(
            celda_encontrada.notas_al_pie,
            vec!["Válido para fluidos clase 1".to_string()]
        );

        // Consulta fallida por valor fuera de rango
        let resultado_fuera = service
            .buscar_celda(
                "TABLA_ESPESORES",
                Some("Acero al Carbón"),
                None,
                None,
                Some(55.0),
            )
            .await
            .unwrap();
        assert!(resultado_fuera.is_none());
    }
}
