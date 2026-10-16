//! Adaptador para destilación con Claude Headless CLI (`claude -p`)

use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;
use tokio::time::timeout;

use crate::application::dtos::ingestion::{DistilledDocumentPackage, ParsedDocument};
use crate::application::ports::llm_distiller::LlmDistillerPort;
use crate::domain::errors::DomainError;

/// Configuración de ejecución para Claude Headless CLI
#[derive(Debug, Clone)]
pub struct ClaudeHeadlessConfig {
    pub binary_path: String,
    pub model: String,
    pub timeout_seconds: u64,
}

impl Default for ClaudeHeadlessConfig {
    fn default() -> Self {
        Self {
            binary_path: "claude".to_string(),
            model: "sonnet".to_string(),
            timeout_seconds: 60,
        }
    }
}

/// Adaptador que invoca Claude Headless CLI para destilar documentos técnicos
pub struct ClaudeHeadlessDistiller {
    config: ClaudeHeadlessConfig,
}

impl ClaudeHeadlessDistiller {
    pub fn new(config: ClaudeHeadlessConfig) -> Self {
        Self { config }
    }

    /// Limpia formato markdown envolvente (```json ... ```) si el modelo lo incluye
    pub fn limpiar_json_markdown(salida_raw: &str) -> &str {
        let trim = salida_raw.trim();
        if let Some(resto) = trim.strip_prefix("```json") {
            let fin = resto.strip_suffix("```").unwrap_or(resto);
            return fin.trim();
        }
        if let Some(resto) = trim.strip_prefix("```") {
            let fin = resto.strip_suffix("```").unwrap_or(resto);
            return fin.trim();
        }
        trim
    }

    /// Genera el prompt estructurado para la desnormalización
    fn construir_prompt(&self, parsed: &ParsedDocument) -> String {
        let schema = self.json_schema();
        format!(
            "Eres un agente analista de ingeniería industrial experto en estructuración de normativas técnicas.\n\
             Tu tarea es analizar el siguiente documento técnico y extraer TODA su información en un objeto JSON que cumpla EXACTAMENTE con el siguiente JSON Schema.\n\n\
             ### JSON SCHEMA:\n{schema}\n\n\
             ### REGLAS INNEGOCIABLES:\n\
             1. Responde ÚNICAMENTE con el objeto JSON válido. Sin explicaciones, sin texto introductorio, sin bloques de código adicionales.\n\
             2. Para el catálogo, extrae el título exacto, nivel de prelación (1 a 5, siendo 1 máxima jerarquía técnica/legal), emisor y fecha.\n\
             3. Para las tuplas operativas, extrae cada obligación técnica desglosando fase operativa, rol, acción y tolerancias.\n\
             4. Para tablas, desglosa las filas y rangos numéricos.\n\
             5. Para referencias normativas, detecta citas o derogaciones a otras normas (NOM, ISO, etc.).\n\n\
             ### DOCUMENTO A PROCESAR:\n\
             Nombre de archivo: {}\n\
             Título: {}\n\
             Contenido:\n{}\n",
            parsed.nombre_archivo, parsed.titulo, parsed.contenido_completo
        )
    }
}

#[async_trait]
impl LlmDistillerPort for ClaudeHeadlessDistiller {
    async fn distill(
        &self,
        parsed: &ParsedDocument,
    ) -> Result<DistilledDocumentPackage, DomainError> {
        let prompt = self.construir_prompt(parsed);

        let mut cmd = Command::new(&self.config.binary_path);
        cmd.arg("-p").arg(&prompt);

        if !self.config.model.is_empty() {
            cmd.arg("--model").arg(&self.config.model);
        }

        let duracion_timeout = Duration::from_secs(self.config.timeout_seconds);

        let ejecucion = timeout(duracion_timeout, cmd.output()).await.map_err(|_| {
            DomainError::LlmDistillationError(format!(
                "Tiempo de espera agotado ({}s) al invocar Claude Headless CLI",
                self.config.timeout_seconds
            ))
        })?;

        let output = ejecucion.map_err(|e| {
            DomainError::LlmDistillationError(format!(
                "Error al ejecutar binario '{}': {e}",
                self.config.binary_path
            ))
        })?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(DomainError::LlmDistillationError(format!(
                "Claude Headless finalizó con código de error {}: {}",
                output.status.code().unwrap_or(-1),
                err_msg.trim()
            )));
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let json_limpio = Self::limpiar_json_markdown(&stdout_str);

        let paquete: DistilledDocumentPackage =
            serde_json::from_str(json_limpio).map_err(|e| {
                DomainError::LlmDistillationError(format!(
                    "La salida de Claude Headless no cumple con el esquema JSON esperado: {e}. Salida: {json_limpio}"
                ))
            })?;

        Ok(paquete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limpiar_json_markdown_bloque_codigo() {
        let raw = "```json\n{\"test\": 123}\n```";
        let limpio = ClaudeHeadlessDistiller::limpiar_json_markdown(raw);
        assert_eq!(limpio, "{\"test\": 123}");

        let raw_simple = "```\n{\"test\": 123}\n```";
        let limpio_simple = ClaudeHeadlessDistiller::limpiar_json_markdown(raw_simple);
        assert_eq!(limpio_simple, "{\"test\": 123}");

        let raw_puro = "{\"test\": 123}";
        let limpio_puro = ClaudeHeadlessDistiller::limpiar_json_markdown(raw_puro);
        assert_eq!(limpio_puro, "{\"test\": 123}");
    }

    #[tokio::test]
    async fn test_claude_headless_binario_inexistente_retorna_error_controlado() {
        let config = ClaudeHeadlessConfig {
            binary_path: "claude_binario_totalmente_inexistente_999".to_string(),
            model: "sonnet".to_string(),
            timeout_seconds: 5,
        };
        let distiller = ClaudeHeadlessDistiller::new(config);

        let parsed = ParsedDocument {
            nombre_archivo: "doc.md".to_string(),
            titulo: "Doc".to_string(),
            secciones: Vec::new(),
            tablas: Vec::new(),
            contenido_completo: "Contenido".to_string(),
        };

        let err = distiller.distill(&parsed).await.unwrap_err();
        assert!(matches!(err, DomainError::LlmDistillationError(_)));
        assert!(err.to_string().contains("Error al ejecutar binario"));
    }
}
