//! Adaptador Mock determinista para destilación estructurada sin llamadas externas

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::application::dtos::ingestion::{
    DistilledCatalog, DistilledCell, DistilledDocumentPackage, DistilledDoubleEntryTable,
    DistilledOperationalTuple, DistilledReferenceEdge, ParsedDocument,
};
use crate::application::ports::llm_distiller::LlmDistillerPort;
use crate::domain::errors::DomainError;

/// Destilador Mock configurable y determinista para suites TDD y offline
#[derive(Clone)]
pub struct MockDistiller {
    respuestas_predefinidas: Arc<Mutex<HashMap<String, DistilledDocumentPackage>>>,
    error_programado: Arc<Mutex<Option<String>>>,
}

impl MockDistiller {
    pub fn new() -> Self {
        Self {
            respuestas_predefinidas: Arc::new(Mutex::new(HashMap::new())),
            error_programado: Arc::new(Mutex::new(None)),
        }
    }

    /// Registra un paquete estructurado predefinido asociado al nombre del documento
    pub fn registrar_respuesta(&self, nombre_doc: &str, paquete: DistilledDocumentPackage) {
        self.respuestas_predefinidas
            .lock()
            .unwrap()
            .insert(nombre_doc.to_string(), paquete);
    }

    /// Programa un error forzado para probar manejo de fallos y resiliencia
    pub fn programar_error(&self, mensaje_error: impl Into<String>) {
        *self.error_programado.lock().unwrap() = Some(mensaje_error.into());
    }

    /// Limpia cualquier error programado
    pub fn limpiar_error(&self) {
        *self.error_programado.lock().unwrap() = None;
    }

    /// Generador heurístico canónico si no se configuró respuesta explícita
    fn generar_paquete_heuristico(&self, parsed: &ParsedDocument) -> DistilledDocumentPackage {
        let doc_id = parsed
            .nombre_archivo
            .trim_end_matches(".md")
            .to_uppercase()
            .replace(' ', "-");

        let catalogo = DistilledCatalog {
            doc_id: doc_id.clone(),
            titulo: parsed.titulo.clone(),
            tipo_documental: "NORMA_TECNICA".to_string(),
            emisor: "INGENIERIA_PLANTA".to_string(),
            fecha_emision: "2026-01-15".to_string(),
            nivel_prelacion: 2,
            version: 1,
        };

        let mut tuplas_operativas = Vec::new();
        for sec in &parsed.secciones {
            if sec.contenido.to_lowercase().contains("inspeccionar")
                || sec.contenido.to_lowercase().contains("verificar")
                || sec.contenido.to_lowercase().contains("control")
            {
                tuplas_operativas.push(DistilledOperationalTuple {
                    seccion: sec.codigo.clone(),
                    fase_operativa: "INSPECCION".to_string(),
                    rol_responsable: "OPERADOR_TECNICO".to_string(),
                    accion: "Verificación de parámetro operativo".to_string(),
                    parametro_control: "Presión/Tolerancia".to_string(),
                    tolerancia_criterio: "+/- 5%".to_string(),
                    condicion_aplicacion: "Operación nominal continua".to_string(),
                    referencia_normativa: doc_id.clone(),
                });
            }
        }

        let mut tablas_doble_entrada = Vec::new();
        for t in &parsed.tablas {
            let mut celdas = Vec::new();
            for (idx, fila) in t.filas.iter().enumerate() {
                if fila.len() >= 3 {
                    celdas.push(DistilledCell {
                        rango_x_min: Some((idx as f64) * 5.0),
                        rango_x_max: Some(((idx + 1) as f64) * 5.0),
                        x_inclusivo_min: true,
                        x_inclusivo_max: true,
                        categoria_x: None,
                        rango_y_min: None,
                        rango_y_max: None,
                        y_inclusivo_min: false,
                        y_inclusivo_max: false,
                        categoria_y: Some(fila[2].clone()),
                        valor_resultado: fila[1].clone(),
                        unidad_medida: "mm".to_string(),
                        notas_especificas: t.notas.clone(),
                    });
                }
            }

            let coord_x = t
                .encabezados
                .first()
                .cloned()
                .unwrap_or_else(|| "Eje_X".to_string());
            let coord_y = t
                .encabezados
                .get(2)
                .cloned()
                .unwrap_or_else(|| "Eje_Y".to_string());

            tablas_doble_entrada.push(DistilledDoubleEntryTable {
                nombre_tabla: t.titulo.clone(),
                coordenada_x_nombre: coord_x,
                coordenada_y_nombre: coord_y,
                nota_pie: t.notas.clone(),
                celdas,
            });
        }

        let mut aristas_referencia = Vec::new();
        if parsed.contenido_completo.contains("NOM-STPS-001") {
            aristas_referencia.push(DistilledReferenceEdge {
                doc_destino_id: "NOM-STPS-001".to_string(),
                tipo_relacion: "CITA".to_string(),
                peso_prelacion: 1.0,
                contexto_cita: Some("Cita obligatoria de seguridad general".to_string()),
            });
        }

        DistilledDocumentPackage {
            catalogo,
            tuplas_operativas,
            tablas_doble_entrada,
            aristas_referencia,
        }
    }
}

impl Default for MockDistiller {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmDistillerPort for MockDistiller {
    async fn distill(
        &self,
        parsed: &ParsedDocument,
    ) -> Result<DistilledDocumentPackage, DomainError> {
        if let Some(err_msg) = self.error_programado.lock().unwrap().as_ref() {
            return Err(DomainError::LlmDistillationError(err_msg.clone()));
        }

        if let Some(paquete) = self
            .respuestas_predefinidas
            .lock()
            .unwrap()
            .get(&parsed.nombre_archivo)
        {
            return Ok(paquete.clone());
        }

        Ok(self.generar_paquete_heuristico(parsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dtos::ingestion::ParsedSection;

    #[tokio::test]
    async fn test_mock_distiller_generacion_heuristica() {
        let parsed = ParsedDocument {
            nombre_archivo: "nom_calidad_001.md".to_string(),
            titulo: "Procedimiento de Calidad y Seguridad".to_string(),
            secciones: vec![ParsedSection {
                codigo: "1.1".to_string(),
                titulo: "Control Operativo".to_string(),
                nivel: 2,
                contenido: "Se debe verificar la presión y temperatura en cada turno.".to_string(),
            }],
            tablas: Vec::new(),
            contenido_completo: "Texto con referencia a NOM-STPS-001 para cumplimiento."
                .to_string(),
        };

        let distiller = MockDistiller::new();
        let paquete = distiller.distill(&parsed).await.unwrap();

        assert_eq!(paquete.catalogo.doc_id, "NOM_CALIDAD_001");
        assert_eq!(
            paquete.catalogo.titulo,
            "Procedimiento de Calidad y Seguridad"
        );
        assert_eq!(paquete.tuplas_operativas.len(), 1);
        assert_eq!(paquete.aristas_referencia.len(), 1);
        assert_eq!(paquete.aristas_referencia[0].doc_destino_id, "NOM-STPS-001");
    }

    #[tokio::test]
    async fn test_mock_distiller_error_programado() {
        let distiller = MockDistiller::new();
        distiller.programar_error("Error simulado de timeout en invocación LLM");

        let parsed = ParsedDocument {
            nombre_archivo: "test.md".to_string(),
            titulo: "Test".to_string(),
            secciones: Vec::new(),
            tablas: Vec::new(),
            contenido_completo: "Test".to_string(),
        };

        let err = distiller.distill(&parsed).await.unwrap_err();
        assert!(matches!(err, DomainError::LlmDistillationError(_)));
        assert!(err.to_string().contains("Error simulado"));
    }

    #[test]
    fn test_mock_distiller_genera_json_schema_valido() {
        let distiller = MockDistiller::new();
        let schema_str = distiller.json_schema();
        let json_val: serde_json::Value = serde_json::from_str(&schema_str).unwrap();
        assert!(json_val.get("$schema").is_some() || json_val.get("properties").is_some());
    }
}
