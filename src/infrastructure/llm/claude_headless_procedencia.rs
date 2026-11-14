//! Adaptador del Nivel 2 del verificador de procedencia: una microllamada a Haiku, sin tools y con
//! `--json-schema` estricto. Solo propone trazas; las comprueba `arnes_turno::procedencia`.

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::application::dtos::agent::ClaudeModel;
use crate::application::dtos::procedencia::{SolicitudProcedencia, TrazasPropuestas};
use crate::application::ports::verificador_procedencia::VerificadorProcedenciaPort;
use crate::domain::errors::DomainError;
use crate::infrastructure::llm::claude_agent_runner::{ClaudeAgentConfig, ClaudeAgentRunner};

/// Más que esto y la respuesta se trata como hoy (la cifra no pasa)
pub const TIEMPO_MAXIMO_PROCEDENCIA: Duration = Duration::from_secs(10);
const MODELO_PROCEDENCIA: &str = "haiku";
const PROMPT_PROCEDENCIA: &str = "Eres un comprobador de procedencia. Recibes una pregunta, las salidas de herramientas de un turno (con id t0, t1…) y un borrador de respuesta. Para cada cifra de la lista `cifras`, indica de dónde sale con uno de los 4 tipos del esquema. Cada `texto` debe copiarse EXACTAMENTE, carácter a carácter, de su `origen` (un id de salida o \"pregunta\"). Si una cifra no sale de esos textos, no propongas traza para ella. No inventes operandos.";

pub struct ClaudeHeadlessProcedencia {
    runner: ClaudeAgentRunner,
    modelo: ClaudeModel,
}

impl ClaudeHeadlessProcedencia {
    pub fn new(binario: PathBuf) -> Self {
        Self {
            runner: ClaudeAgentRunner::new(ClaudeAgentConfig {
                binary_path: binario,
                timeout_duration: TIEMPO_MAXIMO_PROCEDENCIA,
                extra_args: Vec::new(),
            }),
            modelo: ClaudeModel::new(MODELO_PROCEDENCIA).expect("alias de modelo válido"),
        }
    }

    pub fn con_config(config: ClaudeAgentConfig) -> Self {
        Self {
            runner: ClaudeAgentRunner::new(config),
            modelo: ClaudeModel::new(MODELO_PROCEDENCIA).expect("alias de modelo válido"),
        }
    }

    /// Esquema cerrado de `--json-schema`: los 4 tipos del SSD §4.3, sin campos extra
    pub fn esquema() -> Value {
        let literal = json!({
            "type": "object",
            "properties": { "origen": { "type": "string" }, "texto": { "type": "string" } },
            "required": ["origen", "texto"],
            "additionalProperties": false,
        });
        let tipo = |nombre: &str, propiedades: Value, requeridas: Vec<&str>| {
            let mut props = propiedades.as_object().cloned().unwrap_or_default();
            props.insert("tipo".into(), json!({ "const": nombre }));
            let mut requeridas: Vec<Value> = requeridas.into_iter().map(Value::from).collect();
            requeridas.push(json!("tipo"));
            json!({
                "type": "object",
                "properties": props,
                "required": requeridas,
                "additionalProperties": false,
            })
        };
        let lista =
            |maximo: usize| json!({ "type": "array", "items": literal, "maxItems": maximo });
        json!({
            "type": "object",
            "properties": {
                "trazas": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "cifra": { "type": "string" },
                            "tipo": { "oneOf": [
                                tipo("formato_o_escala", json!({
                                    "fuente": literal,
                                    "factor_potencia_diez": { "type": "integer", "minimum": -9, "maximum": 9 },
                                }), vec!["fuente", "factor_potencia_diez"]),
                                tipo("aritmetica", json!({
                                    "operacion": { "enum": ["suma", "resta", "producto", "division", "porcentaje"] },
                                    "operandos": lista(6),
                                }), vec!["operacion", "operandos"]),
                                tipo("referencia_estructural", json!({ "subcadena": literal }), vec!["subcadena"]),
                                tipo("conteo_elementos", json!({ "elementos": lista(64) }), vec!["elementos"]),
                            ]},
                        },
                        "required": ["cifra", "tipo"],
                        "additionalProperties": false,
                    },
                },
            },
            "required": ["trazas"],
            "additionalProperties": false,
        })
    }

    /// Lee `structured_output` del sobre de la CLI. Sin él, o fuera del esquema, es un error: la
    /// cifra no pasa
    pub fn interpretar_sobre(stdout: &str) -> Result<TrazasPropuestas, DomainError> {
        let sobre: Value = serde_json::from_str(stdout.trim())
            .map_err(|e| DomainError::AgentError(format!("Sobre de procedencia no válido: {e}")))?;
        if sobre.get("is_error").and_then(Value::as_bool) == Some(true) {
            return Err(DomainError::AgentError(
                "La microllamada de procedencia terminó en error".to_string(),
            ));
        }
        let salida = sobre.get("structured_output").cloned().ok_or_else(|| {
            DomainError::AgentError("La microllamada de procedencia no dio trazas".to_string())
        })?;
        serde_json::from_value(salida).map_err(|e| {
            DomainError::AgentError(format!("Trazas de procedencia fuera del esquema: {e}"))
        })
    }
}

#[async_trait]
impl VerificadorProcedenciaPort for ClaudeHeadlessProcedencia {
    async fn proponer_trazas(
        &self,
        solicitud: &SolicitudProcedencia,
    ) -> Result<TrazasPropuestas, DomainError> {
        let entrada = serde_json::to_string_pretty(solicitud).map_err(|e| {
            DomainError::AgentError(format!("No se pudo serializar la solicitud: {e}"))
        })?;
        let argumentos = self.runner.construir_argumentos_dirigidos(
            &self.modelo,
            PROMPT_PROCEDENCIA,
            &Self::esquema(),
        );
        let stdout = self.runner.ejecutar_cli(argumentos, entrada).await?;
        Self::interpretar_sobre(&stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sobre_con_trazas_conformes_se_lee() {
        let sobre = json!({
            "is_error": false,
            "structured_output": { "trazas": [{ "cifra": "21", "tipo": {
                "tipo": "referencia_estructural", "subcadena": { "origen": "t0", "texto": "Art. 21" } } }] },
        });
        let trazas = ClaudeHeadlessProcedencia::interpretar_sobre(&sobre.to_string()).unwrap();
        assert_eq!(trazas.trazas.len(), 1);
    }

    #[test]
    fn test_sobre_sin_trazas_o_en_error_falla_cerrado() {
        for sobre in [
            json!({ "is_error": true, "structured_output": { "trazas": [] } }),
            json!({ "is_error": false, "result": "texto" }),
            json!({ "is_error": false, "structured_output": { "trazas": [{ "cifra": "1", "tipo": { "tipo": "aprobada" } }] } }),
        ] {
            assert!(
                ClaudeHeadlessProcedencia::interpretar_sobre(&sobre.to_string()).is_err(),
                "{sobre}"
            );
        }
        assert!(ClaudeHeadlessProcedencia::interpretar_sobre("no json").is_err());
    }

    #[test]
    fn test_esquema_es_cerrado_y_sin_tools() {
        let esquema = ClaudeHeadlessProcedencia::esquema();
        assert_eq!(esquema["additionalProperties"], false);
        assert_eq!(
            esquema["properties"]["trazas"]["items"]["properties"]["tipo"]["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        let adaptador = ClaudeHeadlessProcedencia::new(PathBuf::from("claude"));
        let args = adaptador.runner.construir_argumentos_dirigidos(
            &adaptador.modelo,
            PROMPT_PROCEDENCIA,
            &esquema,
        );
        let posicion = |flag: &str| args.iter().position(|a| a == flag).expect(flag);
        assert_eq!(args[posicion("--tools") + 1], "");
        assert_eq!(args[posicion("--model") + 1], MODELO_PROCEDENCIA);
        assert!(args.contains(&"--json-schema".to_string()));
    }

    #[tokio::test]
    async fn test_binario_inexistente_da_error_controlado() {
        let adaptador = ClaudeHeadlessProcedencia::new(PathBuf::from("binario_inexistente_xyz"));
        let solicitud = SolicitudProcedencia {
            pregunta: String::new(),
            salidas: Vec::new(),
            borrador: String::new(),
            cifras: vec!["7".into()],
        };
        assert!(adaptador.proponer_trazas(&solicitud).await.is_err());
    }
}
