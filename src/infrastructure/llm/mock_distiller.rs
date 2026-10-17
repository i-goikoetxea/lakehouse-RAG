//! Adaptador Mock determinista para destilación estructurada sin llamadas externas

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::application::dtos::ingestion::{
    AristaDestilada, CatalogoDestilado, CeldaDestilada, DistilledDocumentPackage, EjeDestilado,
    HitoDestilado, LimitesDestilados, ParsedDocument, ParsedTableRaw, TablaDestilada,
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

        let catalogo = CatalogoDestilado {
            doc_id: doc_id.clone(),
            titulo: parsed.titulo.clone(),
            tipo_documento: "INSTRUCTIVO_TECNICO".to_string(),
            revision: "Rev. 01".to_string(),
            version_num: 1,
            fecha_vigor: "2026-01-15".to_string(),
            emisor: "INGENIERIA_PLANTA".to_string(),
        };

        let hitos: Vec<HitoDestilado> = parsed
            .secciones
            .iter()
            .filter(|sec| {
                let texto = sec.contenido.to_lowercase();
                ["inspeccionar", "verificar", "control"]
                    .iter()
                    .any(|verbo| texto.contains(verbo))
            })
            .enumerate()
            .map(|(idx, sec)| HitoDestilado {
                orden_secuencia: idx as i64 + 1,
                fase_operativa: "INSPECCION".to_string(),
                objeto_alcance: sec.titulo.clone(),
                rol_ejecutor: "Operador Técnico".to_string(),
                rol_responsable: "Supervisor de Calidad".to_string(),
                registro_codigo: None,
                registro_nombre: None,
                criterio_aceptacion: "Verificación de parámetro operativo +/- 5%".to_string(),
                condicion_aplicacion: sec.condicion_aplicacion.clone(),
                seccion_origen: sec.codigo.clone(),
            })
            .collect();

        let tablas = parsed
            .tablas
            .iter()
            .enumerate()
            .map(|(idx, tabla)| Self::tabla_heuristica(idx, tabla))
            .collect();

        let mut aristas = Vec::new();
        if parsed.contenido_completo.contains("NOM-STPS-001") {
            aristas.push(AristaDestilada {
                origen_tipo: "DOCUMENTO".to_string(),
                tipo_relacion: "SUBORDINADO_A_NORMA".to_string(),
                destino_id: "NOM-STPS-001".to_string(),
                destino_tipo: "DOCUMENTO".to_string(),
                contexto: Some("Cita obligatoria de seguridad general".to_string()),
            });
        }

        DistilledDocumentPackage {
            catalogo,
            hitos,
            tablas,
            aristas,
        }
    }

    /// Eje X numérico por fila y eje Y categórico (tercera columna) por cada fila con 3 celdas
    fn tabla_heuristica(idx: usize, tabla: &ParsedTableRaw) -> TablaDestilada {
        let eje = |nombre: Option<&String>, defecto: &str, tipo: &str| EjeDestilado {
            nombre: nombre.cloned().unwrap_or_else(|| defecto.to_string()),
            tipo: tipo.to_string(),
            unidad: None,
        };
        let celdas = tabla
            .filas
            .iter()
            .enumerate()
            .filter(|(_, fila)| fila.len() >= 3)
            .map(|(fila_idx, fila)| CeldaDestilada {
                eje_x: LimitesDestilados {
                    categoria: None,
                    min: Some(fila_idx as f64 * 5.0),
                    min_inclusivo: true,
                    max: Some((fila_idx + 1) as f64 * 5.0),
                    max_inclusivo: true,
                },
                eje_y: LimitesDestilados {
                    categoria: Some(fila[2].clone()),
                    min: None,
                    min_inclusivo: false,
                    max: None,
                    max_inclusivo: false,
                },
                eje_z: None,
                valor_resultado: fila[1].clone(),
                unidad_medida: Some("mm".to_string()),
                nota_pie_condicion: tabla.notas.clone(),
            })
            .collect();

        TablaDestilada {
            tabla_id: format!("T{}", idx + 1),
            titulo: tabla.titulo.clone(),
            eje_x: eje(tabla.encabezados.first(), "Eje_X", "RANGO_NUMERICO"),
            eje_y: eje(tabla.encabezados.get(2), "Eje_Y", "CATEGORIA_EXACTA"),
            valor_salida_nombre: tabla
                .encabezados
                .get(1)
                .cloned()
                .unwrap_or_else(|| "valor".to_string()),
            seccion_origen: tabla.seccion_codigo.clone(),
            celdas,
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
                breadcrumb: "Procedimiento > 1.1 Control Operativo".to_string(),
                condicion_aplicacion: None,
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
        assert_eq!(paquete.hitos.len(), 1);
        assert_eq!(paquete.hitos[0].seccion_origen, "1.1");
        assert_eq!(paquete.aristas.len(), 1);
        assert_eq!(paquete.aristas[0].destino_id, "NOM-STPS-001");
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
