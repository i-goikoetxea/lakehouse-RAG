//! Catálogo de modelos del desplegable con el motor FreeLLMAPI: `GET /v1/models?available=true`
//! en formato Anthropic. Las rutas `auto` del router van arriba, con `auto` primero; después, el
//! resto ordenado. Cada id pasa por `ClaudeModel::new` (SSD del motor FreeLLMAPI §3.5).

use std::collections::BTreeSet;

use async_trait::async_trait;

use crate::application::dtos::agent::{ClaudeModel, ModelDescriptor};
use crate::application::ports::model_catalog::ModelCatalogPort;
use crate::domain::errors::DomainError;
use crate::infrastructure::llm::anthropic_api::{
    AnthropicHttpClient, ErrorApiAnthropic, ModeloListado,
};

/// Entradas del router que siempre valen, aunque el formato Anthropic no las liste todas
pub const RUTAS_AUTO: [(&str, &str); 6] = [
    ("auto", "Auto: el router elige"),
    ("auto:fast", "Auto · rápido"),
    ("auto:smart", "Auto · más capaz"),
    ("auto:cheap", "Auto · económico"),
    ("auto:reliable", "Auto · fiable"),
    ("auto:balanced", "Auto · equilibrado"),
];
const DESCRIPCION: &str = "FreeLLMAPI · modelo gratuito de terceros; calidad no medida";
const MAXIMO_MODELOS: usize = 500;
const LONGITUD_MAXIMA_NOMBRE: usize = 120;

pub struct FreeLlmApiModelCatalog {
    cliente: AnthropicHttpClient,
    url_base: String,
}

impl FreeLlmApiModelCatalog {
    pub fn new(cliente: AnthropicHttpClient, url_base: impl Into<String>) -> Self {
        Self {
            cliente,
            url_base: url_base.into(),
        }
    }

    fn error_accionable(&self, error: &ErrorApiAnthropic) -> DomainError {
        let motivo = match error {
            ErrorApiAnthropic::Autenticacion => {
                "la key de FreeLLMAPI no es válida (401): revísala en el engranaje".to_string()
            }
            otro => otro.to_string(),
        };
        DomainError::AgentError(format!(
            "No se pudo obtener el catálogo de FreeLLMAPI en {}: {motivo}. ¿Está abierta la app de FreeLLMAPI?",
            self.url_base
        ))
    }
}

/// Rutas `auto` arriba (`auto` primero) y después los demás ids ordenados, sin duplicados. Los
/// ids que no son un identificador seguro se descartan
pub fn ordenar_catalogo(listados: &[ModeloListado]) -> Vec<ModelDescriptor> {
    let mut vistos = BTreeSet::new();
    let mut modelos: Vec<ModelDescriptor> = RUTAS_AUTO
        .iter()
        .filter_map(|(id, nombre)| {
            vistos.insert(id.to_string());
            descriptor(id, nombre)
        })
        .collect();
    let mut resto: Vec<&ModeloListado> = listados
        .iter()
        .filter(|m| vistos.insert(m.id.trim().to_string()))
        .collect();
    resto.sort_by(|a, b| a.id.cmp(&b.id));
    modelos.extend(
        resto
            .into_iter()
            .filter_map(|m| descriptor(&m.id, m.nombre_visible.as_deref().unwrap_or(&m.id)))
            .take(MAXIMO_MODELOS),
    );
    modelos
}

fn descriptor(id: &str, nombre: &str) -> Option<ModelDescriptor> {
    let alias = ClaudeModel::new(id).ok()?;
    let nombre: String = nombre
        .chars()
        .filter(|c| !c.is_control())
        .take(LONGITUD_MAXIMA_NOMBRE)
        .collect();
    Some(ModelDescriptor {
        resolved_model: alias.cli_model_name().to_string(),
        display_name: if nombre.trim().is_empty() {
            alias.cli_model_name().to_string()
        } else {
            nombre
        },
        description: DESCRIPCION.to_string(),
        alias,
    })
}

#[async_trait]
impl ModelCatalogPort for FreeLlmApiModelCatalog {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        let listados = self
            .cliente
            .listar_modelos()
            .await
            .map_err(|e| self.error_accionable(&e))?;
        Ok(ordenar_catalogo(&listados))
    }
}

/// Sin key de FreeLLMAPI no hay catálogo: el desplegable lo dice en vez de quedarse vacío
pub struct CatalogoSinKey;

#[async_trait]
impl ModelCatalogPort for CatalogoSinKey {
    async fn listar_modelos(&self) -> Result<Vec<ModelDescriptor>, DomainError> {
        Err(DomainError::AgentError(
            "FreeLLMAPI no tiene key: guárdala en el engranaje.".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::llm::anthropic_api::{
        ConfigClienteAnthropic, PerfilApi, SecretApiKey,
    };
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn listado(id: &str) -> ModeloListado {
        ModeloListado {
            id: id.into(),
            nombre_visible: Some(format!("Nombre {id}")),
        }
    }

    #[test]
    fn test_auto_arriba_y_el_resto_ordenado_sin_duplicados() {
        let modelos = ordenar_catalogo(&[
            listado("zeta-1"),
            listado("auto"),
            listado("gemini-flash"),
            listado("auto:fast"),
            listado("gemini-flash"),
        ]);
        let ids: Vec<&str> = modelos.iter().map(|m| m.alias.cli_model_name()).collect();
        assert_eq!(
            ids,
            vec![
                "auto",
                "auto:fast",
                "auto:smart",
                "auto:cheap",
                "auto:reliable",
                "auto:balanced",
                "gemini-flash",
                "zeta-1"
            ]
        );
        assert_eq!(modelos[6].display_name, "Nombre gemini-flash");
    }

    #[test]
    fn test_los_ids_hostiles_se_descartan_y_los_nombres_se_limpian() {
        let mut hostil = listado("<img src=x onerror=alert(1)>");
        hostil.nombre_visible = None;
        let largo = ModeloListado {
            id: "modelo-largo".into(),
            nombre_visible: Some(format!("{}\n<script>", "n".repeat(500))),
        };
        let modelos = ordenar_catalogo(&[
            hostil,
            listado("--help"),
            listado("a; calc"),
            listado(""),
            largo,
        ]);
        assert_eq!(modelos.len(), RUTAS_AUTO.len() + 1);
        let ultimo = modelos.last().unwrap();
        assert_eq!(ultimo.alias.cli_model_name(), "modelo-largo");
        assert!(ultimo.display_name.chars().count() <= 120);
        assert!(!ultimo.display_name.contains('\n'));
    }

    #[test]
    fn test_un_catalogo_desmesurado_se_recorta() {
        let muchos: Vec<ModeloListado> = (0..2000).map(|i| listado(&format!("m-{i:05}"))).collect();
        assert_eq!(
            ordenar_catalogo(&muchos).len(),
            RUTAS_AUTO.len() + MAXIMO_MODELOS
        );
    }

    fn cliente(base: &str) -> AnthropicHttpClient {
        AnthropicHttpClient::new(
            SecretApiKey::new("freellmapi-test-NO-ES-REAL-cafe").unwrap(),
            ConfigClienteAnthropic::agente()
                .con_perfil(PerfilApi::FREELLMAPI)
                .con_base_url(base),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_el_puerto_lista_desde_el_proxy() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [{"type":"model","id":"gemini-flash","display_name":"Gemini Flash"}]
            })))
            .mount(&servidor)
            .await;
        let catalogo = FreeLlmApiModelCatalog::new(cliente(&servidor.uri()), servidor.uri());
        let modelos = catalogo.listar_modelos().await.unwrap();
        assert_eq!(modelos[0].alias.cli_model_name(), "auto");
        assert_eq!(modelos.last().unwrap().display_name, "Gemini Flash");
    }

    #[tokio::test]
    async fn test_proxy_caido_da_un_error_claro_sin_la_key() {
        let catalogo =
            FreeLlmApiModelCatalog::new(cliente("http://127.0.0.1:1"), "http://127.0.0.1:1");
        let error = catalogo.listar_modelos().await.unwrap_err().to_string();
        assert!(error.contains("FreeLLMAPI"), "{error}");
        assert!(error.contains("http://127.0.0.1:1"), "{error}");
        assert!(!error.contains("NO-ES-REAL"), "{error}");
        let sin_key = CatalogoSinKey
            .listar_modelos()
            .await
            .unwrap_err()
            .to_string();
        assert!(sin_key.contains("key"), "{sin_key}");
    }
}
