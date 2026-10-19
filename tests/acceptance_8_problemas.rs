//! Fase 15 — Aceptación final de los 8 problemas del RAG de procedimientos (plan v2 §1) y de
//! los criterios §9 de la especificación, con **Claude Code y FastEmbed reales**.
//!
//! La fachada `LakehouseApp` se construye con `bootstrap::composicion_real`, la MISMA que usan la
//! GUI, el MCP y la CLI. Solo se sustituye una pieza cuando el problema lo exige, y se dice:
//! - P4 (supernodos): los dos documentos sintéticos `sintetico_*.md` (una norma «hub» con 1.000
//!   remisiones y un procedimiento que la cita) se destilan con un paquete fijo. Pedir a Claude
//!   que extraiga 1.000 aristas no es viable (timeout de 300 s, salida enorme y no determinista)
//!   y el control que se valida es el recorrido acotado, no la extracción. Parser, FastEmbed,
//!   LanceDB, proyección, tools y el agente Claude siguen siendo reales.
//! - P8 (tri-store): el almacén LanceDB real va envuelto en un decorador que falla al escribir
//!   el lote de la tabla 05 cuando se activa el interruptor.
//! - §9.4 (selector de modelos): el agente real va envuelto en un registrador de modelos.
//!
//! Oráculo: `tests/fixtures/README.md`. Las respuestas del LLM se validan por contenido (cifras,
//! códigos y citas presentes; valores incorrectos ausentes) y por las tools que las sustentan.
//!
//! ```text
//! cargo test --test acceptance_8_problemas -- --ignored --nocapture --test-threads=1
//! ```

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use arrow_array::{Array, RecordBatch, StringArray};
use async_trait::async_trait;
use lakehouse_rag::application::dtos::agent::{
    AgentExecutionEvent, AgentMessage, AgentTurnOutput, ClaudeModel,
};
use lakehouse_rag::application::dtos::centros::{
    DocumentoOriginal, ManifiestoCentroDisco, ModeloEmbeddingsActivo,
};
use lakehouse_rag::application::dtos::ingestion::{
    AristaDestilada, CatalogoDestilado, DistilledDocumentPackage, HitoDestilado, ParsedDocument,
};
use lakehouse_rag::application::dtos::lakehouse_filas::LoteIngesta;
use lakehouse_rag::application::dtos::tools::{ToolCall, ToolDefinition};
use lakehouse_rag::application::ports::agent_runner::AgentRunnerPort;
use lakehouse_rag::application::ports::center_workspace::{CenterWorkspacePort, CentroAbierto};
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, HuellaIngerida, InformeIntegridad, LakehouseStorePort, ResumenConfirmacion,
    TODAS_LAS_TABLAS,
};
use lakehouse_rag::application::ports::llm_distiller::LlmDistillerPort;
use lakehouse_rag::application::ports::semantic_index::IndiceSemanticoPort;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::application::use_cases::lakehouse_app::{
    AsistenteLakehousePort, LakehouseApp, RespuestaLakehouse,
};
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::infrastructure::bootstrap::{
    ComposicionReal, ConfiguracionComposicion, composicion_real,
};
use lakehouse_rag::infrastructure::llm::{ClaudeAgentConfig, ClaudeAgentRunner, MockDistiller};
use serde_json::{Value, json};
use tokio::runtime::Runtime;

const CENTRO_A: &str = "obra-aceptacion-a";
const CENTRO_B: &str = "obra-aceptacion-b";
const CENTRO_HUB: &str = "obra-aceptacion-hub";
const CENTRO_P8: &str = "obra-aceptacion-p8";
const FECHA_ANTERIOR_REV04: &str = "2026-06-01";
const ARISTAS_HUB: usize = 1000;
const ARCHIVO_HUB: &str = "sintetico_norma_hub.md";
const ARCHIVO_PO_SINT: &str = "sintetico_po_sint_01.md";
const ARCHIVO_OBRA_B: &str = "sintetico_obra_b.md";

// ---------------------------------------------------------------------------------------------
// Infraestructura compartida
// ---------------------------------------------------------------------------------------------

fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    })
}

fn cache_fastembed() -> PathBuf {
    std::env::var_os("FASTEMBED_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("fastembed_cache")
        })
}

fn fixture(nombre: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(nombre)
}

fn composicion(raiz: &Path) -> ComposicionReal {
    composicion_real(&ConfiguracionComposicion {
        directorio_config: raiz.join("config"),
        directorio_cache_embeddings: Some(cache_fastembed()),
    })
}

fn recortar(texto: &str, maximo: usize) -> String {
    if texto.chars().count() <= maximo {
        texto.to_string()
    } else {
        format!("{}…", texto.chars().take(maximo).collect::<String>())
    }
}

/// Destilador real (Claude) salvo para los documentos `sintetico_*.md` (paquete fijo)
struct DestiladorEnrutado {
    real: Arc<dyn LlmDistillerPort>,
    sinteticos: MockDistiller,
}

#[async_trait]
impl LlmDistillerPort for DestiladorEnrutado {
    async fn distill(
        &self,
        parsed: &ParsedDocument,
    ) -> Result<DistilledDocumentPackage, DomainError> {
        if parsed.nombre_archivo.contains("sintetico_") {
            self.sinteticos.distill(parsed).await
        } else {
            self.real.distill(parsed).await
        }
    }
}

/// Agente real que además registra el modelo de cada invocación del subproceso
struct AgenteRegistrador {
    interno: Arc<dyn AgentRunnerPort>,
    modelos: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl AgentRunnerPort for AgenteRegistrador {
    async fn ejecutar_turno(
        &self,
        modelo: &ClaudeModel,
        system_prompt: &str,
        historial: &[AgentMessage],
        herramientas: &[ToolDefinition],
    ) -> Result<AgentTurnOutput, DomainError> {
        self.modelos
            .lock()
            .unwrap()
            .push(modelo.cli_model_name().to_string());
        self.interno
            .ejecutar_turno(modelo, system_prompt, historial, herramientas)
            .await
    }
}

struct Lakehouse {
    raiz: tempfile::TempDir,
    app: Arc<LakehouseApp>,
    herramientas: Arc<dyn ToolServerPort>,
    modelos_usados: Arc<Mutex<Vec<String>>>,
}

impl Lakehouse {
    fn ruta_centro(&self, id: &str) -> String {
        self.raiz.path().join(id).to_string_lossy().to_string()
    }

    async fn crear(&self, id: &str, activar: bool) {
        self.app
            .crear_centro(id, &self.ruta_centro(id), Some(id))
            .await
            .unwrap();
        if activar {
            self.app.activar_centro(id).await.unwrap();
        }
    }

    async fn ingestar(&self, ruta: &Path, centro: &str) {
        let inicio = Instant::now();
        let resultado = self
            .app
            .ingestar(ruta, centro)
            .await
            .unwrap_or_else(|e| panic!("ingesta de {ruta:?}: {e}"));
        let informe = &resultado.informe;
        println!(
            "  ingesta {} → {} v{} [{:.1} s] chunks {} · hitos {} · tablas {} · celdas {} · aristas {} · deroga {:?}",
            ruta.file_name().unwrap().to_string_lossy(),
            informe.doc_id,
            informe.version,
            inicio.elapsed().as_secs_f64(),
            informe.total_chunks,
            informe.total_tuplas,
            informe.total_tablas,
            informe.total_celdas,
            informe.total_aristas,
            informe.versiones_derogadas
        );
        assert!(resultado.aviso_proyeccion.is_none(), "{resultado:?}");
    }

    fn escribir_sintetico(&self, nombre: &str, contenido: &str) -> PathBuf {
        let directorio = self.raiz.path().join("entrada");
        std::fs::create_dir_all(&directorio).unwrap();
        let ruta = directorio.join(nombre);
        std::fs::write(&ruta, contenido).unwrap();
        ruta
    }

    async fn preguntar(&self, centro: &str, texto: &str, modelo: &str) -> RespuestaLakehouse {
        let inicio = Instant::now();
        let respuesta = self
            .app
            .preguntar(texto, centro, Some(modelo))
            .await
            .unwrap_or_else(|e| panic!("pregunta «{texto}»: {e}"));
        println!(
            "\n  [{modelo}, {:.1} s] P: {texto}\n  R: {}",
            inicio.elapsed().as_secs_f64(),
            recortar(&respuesta.respuesta, 1200)
        );
        for uso in &respuesta.herramientas {
            println!(
                "    tool {} [{}] args={} → {}",
                uso.nombre,
                if uso.es_error { "error" } else { "ok" },
                uso.argumentos,
                recortar(&uso.resultado.to_string(), 260)
            );
        }
        respuesta
    }

    async fn tool(&self, nombre: &str, argumentos: Value) -> Value {
        let resultado = self
            .herramientas
            .ejecutar_herramienta(&ToolCall {
                id: format!("acept_{nombre}"),
                name: nombre.to_string(),
                arguments: argumentos,
            })
            .await
            .expect("la herramienta devuelve un ToolResult");
        assert!(!resultado.is_error, "{nombre} falló: {}", resultado.output);
        resultado.output
    }

    async fn tool_error(&self, nombre: &str, argumentos: Value) -> Value {
        let resultado = self
            .herramientas
            .ejecutar_herramienta(&ToolCall {
                id: format!("acept_{nombre}"),
                name: nombre.to_string(),
                arguments: argumentos,
            })
            .await;
        match resultado {
            Ok(r) => {
                assert!(r.is_error, "{nombre} debía fallar: {}", r.output);
                r.output
            }
            Err(e) => json!(e.to_string()),
        }
    }

    async fn sql(&self, centro: &str, consulta: &str) -> Vec<Value> {
        let salida = self
            .tool(
                "execute_athena_sql",
                json!({ "centro_id": centro, "sql_query": consulta }),
            )
            .await;
        salida["resultado"].as_array().cloned().unwrap_or_default()
    }
}

fn nueva_lakehouse(
    personalizar: impl FnOnce(&mut ComposicionReal),
) -> (Lakehouse, Arc<dyn IndiceSemanticoPort>) {
    let raiz = tempfile::tempdir().unwrap();
    let mut comp = composicion(raiz.path());
    let modelos_usados: Arc<Mutex<Vec<String>>> = Arc::default();
    comp.dependencias.agente = Arc::new(AgenteRegistrador {
        interno: comp.dependencias.agente.clone(),
        modelos: modelos_usados.clone(),
    });
    personalizar(&mut comp);
    let herramientas = comp.dependencias.herramientas.clone();
    let indice = comp.indice.clone();
    let app = Arc::new(LakehouseApp::new(comp.dependencias));
    (
        Lakehouse {
            raiz,
            app,
            herramientas,
            modelos_usados,
        },
        indice,
    )
}

/// Centro A (activo, fixtures reales), centro HUB (activo, sintéticos) y centro B (inactivo)
fn compartido() -> &'static Lakehouse {
    static COMPARTIDO: OnceLock<Lakehouse> = OnceLock::new();
    COMPARTIDO.get_or_init(|| {
        let (lakehouse, _) = nueva_lakehouse(|comp| {
            comp.dependencias.destilador = Arc::new(DestiladorEnrutado {
                real: comp.dependencias.destilador.clone(),
                sinteticos: destilador_sintetico(),
            });
        });
        rt().block_on(async {
            let inicio = Instant::now();
            println!("\n=== Preparación: centros A (real), HUB (sintético) y B (inactivo) ===");
            lakehouse.crear(CENTRO_A, true).await;
            for archivo in [
                "procedimiento_recepcion_rev03.md",
                "procedimiento_recepcion_rev04.md",
                "pliego_cliente_pcc01.md",
                "norma_soldeo_nt_sol_01.md",
            ] {
                lakehouse.ingestar(&fixture(archivo), CENTRO_A).await;
            }
            lakehouse.crear(CENTRO_HUB, true).await;
            let hub = lakehouse.escribir_sintetico(ARCHIVO_HUB, &markdown_hub());
            lakehouse.ingestar(&hub, CENTRO_HUB).await;
            let po = lakehouse.escribir_sintetico(ARCHIVO_PO_SINT, MARKDOWN_PO_SINT);
            lakehouse.ingestar(&po, CENTRO_HUB).await;
            lakehouse.crear(CENTRO_B, true).await;
            let obra_b = lakehouse.escribir_sintetico(ARCHIVO_OBRA_B, MARKDOWN_OBRA_B);
            lakehouse.ingestar(&obra_b, CENTRO_B).await;
            lakehouse.app.desactivar_centro(CENTRO_B).await.unwrap();
            println!(
                "=== Preparación completa en {:.1} s ===",
                inicio.elapsed().as_secs_f64()
            );
        });
        lakehouse
    })
}

fn usadas(respuesta: &RespuestaLakehouse) -> Vec<&str> {
    respuesta
        .herramientas
        .iter()
        .filter(|h| !h.es_error)
        .map(|h| h.nombre.as_str())
        .collect()
}

fn tool_con(
    respuesta: &RespuestaLakehouse,
    nombre: &str,
    condicion: impl Fn(&Value, &Value) -> bool,
) -> bool {
    respuesta
        .herramientas
        .iter()
        .any(|h| !h.es_error && h.nombre == nombre && condicion(&h.argumentos, &h.resultado))
}

fn contiene_alguno(texto: &str, opciones: &[&str]) -> bool {
    opciones.iter().any(|o| texto.contains(o))
}

fn resultado(etiqueta: &str, inicio: Instant) {
    println!(
        "\nRESULTADO {etiqueta}: OK en {:.1} s",
        inicio.elapsed().as_secs_f64()
    );
}

// ---------------------------------------------------------------------------------------------
// Documentos sintéticos (P4 y §9.1)
// ---------------------------------------------------------------------------------------------

fn catalogo(doc_id: &str, titulo: &str, tipo: &str) -> CatalogoDestilado {
    CatalogoDestilado {
        doc_id: doc_id.to_string(),
        titulo: titulo.to_string(),
        tipo_documento: tipo.to_string(),
        revision: "Rev. 01".to_string(),
        version_num: 1,
        fecha_vigor: "2026-01-01".to_string(),
        emisor: "Calidad".to_string(),
    }
}

fn arista(destino: &str, relacion: &str, tipo_destino: &str) -> AristaDestilada {
    AristaDestilada {
        origen_tipo: "DOCUMENTO".to_string(),
        tipo_relacion: relacion.to_string(),
        destino_id: destino.to_string(),
        destino_tipo: tipo_destino.to_string(),
        contexto: None,
    }
}

fn paquete(
    catalogo: CatalogoDestilado,
    hitos: Vec<HitoDestilado>,
    aristas: Vec<AristaDestilada>,
) -> DistilledDocumentPackage {
    DistilledDocumentPackage {
        catalogo,
        hitos,
        tablas: Vec::new(),
        aristas,
    }
}

fn destilador_sintetico() -> MockDistiller {
    let destilador = MockDistiller::new();
    let remisiones = (1..=ARISTAS_HUB)
        .map(|i| arista(&format!("REG-{i:04}"), "REFERENCIA_A", "REGISTRO"))
        .collect();
    destilador.registrar_respuesta(
        ARCHIVO_HUB,
        paquete(
            catalogo(
                "NORMA-HUB",
                "Índice general de registros",
                "NORMA_INTERNACIONAL",
            ),
            Vec::new(),
            remisiones,
        ),
    );
    destilador.registrar_respuesta(
        ARCHIVO_PO_SINT,
        paquete(
            catalogo("PO-SINT-01", "Montaje de soportes", "PROCEDIMIENTO_OBRA"),
            Vec::new(),
            vec![
                arista("NORMA-HUB", "SUBORDINADO_A_NORMA", "DOCUMENTO"),
                arista("FOR-SINT-01", "EXIGE_LLENAR_REGISTRO", "REGISTRO"),
                arista("PCC-SINT", "SUBORDINADO_A_NORMA", "DOCUMENTO"),
            ],
        ),
    );
    destilador.registrar_respuesta(
        ARCHIVO_OBRA_B,
        paquete(
            catalogo(
                "PO-OBRA-B-01",
                "Custodia en la obra B",
                "PROCEDIMIENTO_OBRA",
            ),
            vec![HitoDestilado {
                orden_secuencia: 1,
                fase_operativa: "CUSTODIA".to_string(),
                objeto_alcance: "Tubería".to_string(),
                rol_ejecutor: "Peón de Obra B".to_string(),
                rol_responsable: "Encargado de Obra B".to_string(),
                registro_codigo: Some("FOR-B-99".to_string()),
                registro_nombre: Some("Parte de custodia B".to_string()),
                criterio_aceptacion: "Conforme".to_string(),
                condicion_aplicacion: None,
                seccion_origen: "1".to_string(),
            }],
            Vec::new(),
        ),
    );
    destilador
}

fn markdown_hub() -> String {
    let mut texto = String::from(
        "# NORMA-HUB Índice general de registros (Rev. 01)\n\n## 1. Objeto\n\
         Esta norma remite a los 1.000 registros de calidad de la organización.\n\n## 2. Remisiones\n",
    );
    for bloque in 0..10 {
        let codigos: Vec<String> = (1..=100)
            .map(|i| format!("REG-{:04}", bloque * 100 + i))
            .collect();
        texto.push_str(&format!(
            "Bloque {}: {}.\n\n",
            bloque + 1,
            codigos.join(", ")
        ));
    }
    texto
}

const MARKDOWN_PO_SINT: &str = "# PO-SINT-01 Montaje de soportes (Rev. 01)\n\n## 1. Alcance\n\
    El montaje de soportes se rige por NORMA-HUB y por el pliego PCC-SINT.\n\n## 2. Registros\n\
    El encargado cumplimenta el registro FOR-SINT-01.\n";

const MARKDOWN_OBRA_B: &str = "# PO-OBRA-B-01 Custodia en la obra B (Rev. 01)\n\n## 1. Custodia\n\
    El Peón de Obra B custodia la tubería y el Encargado de Obra B firma el registro FOR-B-99.\n";

// ---------------------------------------------------------------------------------------------
// P1 — Contaminación de atributos (anti-fusión) · especificación §9.2
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn p1_anti_fusion_la_custodia_solo_devuelve_su_responsable_y_registro() {
    let lh = compartido();
    let inicio = Instant::now();
    rt().block_on(async {
        // §9.2: al menos 3 filas distintas en 03 para el procedimiento vigente
        let filas = lh
            .sql(
                CENTRO_A,
                "SELECT fase_operativa, rol_ejecutor, rol_responsable, registro_codigo \
                 FROM \"03_matriz_operativa\" WHERE doc_id = 'PO-ALM-03' AND vigente = true \
                 ORDER BY fase_operativa",
            )
            .await;
        println!("  03 vigente PO-ALM-03: {}", Value::Array(filas.clone()));
        let fases: BTreeSet<String> = filas
            .iter()
            .map(|f| f["fase_operativa"].as_str().unwrap_or_default().to_string())
            .collect();
        for fase in ["RECEPCION", "CUSTODIA", "INSPECCION"] {
            assert!(fases.contains(fase), "falta la fila {fase}: {fases:?}");
        }

        // Tool exacta: una sola fila de CUSTODIA con su responsable y su registro
        let matriz = lh
            .tool(
                "query_operational_matrix",
                json!({ "centro_id": CENTRO_A, "fase": "CUSTODIA" }),
            )
            .await;
        let custodia = matriz["filas"].as_array().unwrap();
        assert_eq!(custodia.len(), 1, "{matriz}");
        assert_eq!(custodia[0]["rol_responsable"], "Jefe de Producción");
        assert_eq!(custodia[0]["registro_codigo"], "FOR-ALM-05");

        // Chat (pregunta literal de §9.2)
        let r = lh
            .preguntar(
                CENTRO_A,
                "¿Quién es el responsable de la custodia y qué registro llena?",
                "sonnet",
            )
            .await;
        let texto = &r.respuesta;
        assert!(texto.contains("Jefe de Producción"), "{texto}");
        assert!(texto.contains("FOR-ALM-05"), "{texto}");
        for ajeno in ["Jefe de Almacén", "FOR-ALM-01", "PPI-MAT-09", "FOR-ALM-02"] {
            assert!(!texto.contains(ajeno), "fusiona «{ajeno}»: {texto}");
        }
        assert!(
            usadas(&r)
                .iter()
                .any(|t| *t == "query_operational_matrix" || *t == "execute_athena_sql"),
            "{:?}",
            usadas(&r)
        );
    });
    resultado("P1 anti-fusión", inicio);
}

// ---------------------------------------------------------------------------------------------
// P2 — Geometría de tablas · especificación §9.3
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn p2_tabla_cruce_exacto_con_nota_y_fronteras() {
    let lh = compartido();
    let inicio = Instant::now();
    rt().block_on(async {
        let cruce = |x: Value| {
            json!({ "centro_id": CENTRO_A, "tabla_id": "4-A", "valor_x": x, "valor_y": 0.42 })
        };
        let central = lh.tool("lookup_double_entry_table", cruce(json!(30))).await;
        assert_eq!(central["estado"], "resuelto", "{central}");
        assert!(central["valor_resultado_texto"].as_str().unwrap().contains("125"));
        assert!(
            central.to_string().contains("HV10"),
            "nota (*) obligatoria: {central}"
        );
        let frontera = lh.tool("lookup_double_entry_table", cruce(json!(20))).await;
        let pasada = lh
            .tool("lookup_double_entry_table", cruce(json!(20.0001)))
            .await;
        println!(
            "  e = 20 → {} · e = 20,0001 → {}",
            frontera["valor_resultado_texto"], pasada["valor_resultado_texto"]
        );
        assert_eq!(frontera["valor_resultado_texto"], "50", "e ≤ 20 es inclusivo");
        assert!(pasada["valor_resultado_texto"].as_str().unwrap().contains("125"));

        let r = lh
            .preguntar(
                CENTRO_A,
                "Según la Tabla 4-A, ¿cuál es la temperatura mínima de precalentamiento para un espesor de 30 mm y un carbono equivalente de 0,42 %?",
                "sonnet",
            )
            .await;
        let texto = &r.respuesta;
        assert!(texto.contains("125"), "{texto}");
        assert!(
            contiene_alguno(texto, &["HV10", "QA/QC"]),
            "falta la nota (*): {texto}"
        );
        assert!(!texto.contains("100 °C"), "valor de la Rev. 03: {texto}");
        assert!(
            tool_con(&r, "lookup_double_entry_table", |_, res| res["estado"]
                == "resuelto")
                || tool_con(&r, "execute_athena_sql", |_, _| true),
            "§9.3: lookup o SQL"
        );

        let r = lh
            .preguntar(
                CENTRO_A,
                "En la Tabla 4-A, ¿qué temperatura mínima de precalentamiento corresponde a un espesor de exactamente 20 mm con CE = 0,42 %?",
                "sonnet",
            )
            .await;
        assert!(r.respuesta.contains("50"), "{}", r.respuesta);
        assert!(
            tool_con(&r, "lookup_double_entry_table", |args, res| args["valor_x"]
                .as_f64()
                == Some(20.0)
                && res["valor_resultado_texto"] == "50"),
            "la frontera la resuelve el lookup: {:?}",
            usadas(&r)
        );
    });
    resultado("P2 tabla", inicio);
}

// ---------------------------------------------------------------------------------------------
// P3 — Versiones obsoletas / time travel
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn p3_versiones_vigente_125_a_fecha_100_y_cero_aristas_v3() {
    let lh = compartido();
    let inicio = Instant::now();
    rt().block_on(async {
        let catalogo = lh
            .sql(
                CENTRO_A,
                "SELECT version_num, vigente, CAST(fecha_vigor AS VARCHAR) AS vigor, \
                 CAST(fecha_derogacion AS VARCHAR) AS derogacion \
                 FROM \"01_documentos_catalogo\" WHERE doc_id = 'PO-ALM-03' ORDER BY version_num",
            )
            .await;
        println!("  01 PO-ALM-03: {}", Value::Array(catalogo.clone()));
        assert_eq!(catalogo.len(), 2);
        assert_eq!(catalogo[0]["vigente"], false);
        assert_eq!(catalogo[0]["derogacion"], "2026-09-15");
        assert_eq!(catalogo[1]["vigente"], true);

        let aristas_v3 = lh
            .sql(
                CENTRO_A,
                "SELECT COUNT(*) AS n FROM \"06_grafo_referencias\" \
                 WHERE doc_id = 'PO-ALM-03' AND version_num = 3 AND vigente = true",
            )
            .await;
        assert_eq!(aristas_v3[0]["n"], 0, "aristas de la Rev. 03 vigentes");
        let grafo = lh
            .tool(
                "traverse_normative_graph",
                json!({ "centro_id": CENTRO_A, "nodo_origen": "PO-ALM-03", "max_saltos": 1 }),
            )
            .await;
        let aristas = grafo["aristas"].as_array().unwrap();
        assert!(!aristas.is_empty());
        assert!(
            aristas.iter().all(|a| a["cita"]["version_num"] == 4),
            "0 aristas de la v3 en el recorrido: {grafo}"
        );

        let antes = lh
            .tool(
                "lookup_double_entry_table",
                json!({ "centro_id": CENTRO_A, "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42, "fecha_referencia": FECHA_ANTERIOR_REV04 }),
            )
            .await;
        assert!(antes["valor_resultado_texto"].as_str().unwrap().contains("100"));

        let r = lh
            .preguntar(
                CENTRO_A,
                "¿Cuál es hoy la temperatura mínima de precalentamiento de la Tabla 4-A para 30 mm y CE 0,42 %, y cuál era el 1 de junio de 2026, antes de que entrara en vigor la Rev. 04?",
                "sonnet",
            )
            .await;
        assert!(r.respuesta.contains("125"), "{}", r.respuesta);
        assert!(r.respuesta.contains("100"), "{}", r.respuesta);
        assert!(
            tool_con(&r, "lookup_double_entry_table", |args, res| args
                ["fecha_referencia"]
                .is_string()
                && res["valor_resultado_texto"]
                    .as_str()
                    .is_some_and(|v| v.contains("100")))
                || tool_con(&r, "execute_athena_sql", |args, _| args
                    ["fecha_referencia"]
                    .is_string()
                    || args["sql_query"].to_string().contains("2026-06-01")),
            "la cifra antigua sale de una consulta a fecha"
        );
    });
    resultado("P3 versiones", inicio);
}

// ---------------------------------------------------------------------------------------------
// P4 — Supernodos / hairball
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn p4_supernodos_dos_saltos_acotados_con_hub_resumido() {
    let lh = compartido();
    let inicio = Instant::now();
    rt().block_on(async {
        let aristas_hub = lh
            .sql(
                CENTRO_HUB,
                "SELECT COUNT(*) AS n FROM \"06_grafo_referencias\" WHERE doc_id = 'NORMA-HUB' AND vigente = true",
            )
            .await;
        assert_eq!(aristas_hub[0]["n"], ARISTAS_HUB as i64);

        for origen in ["PO-SINT-01", "NORMA-HUB"] {
            let grafo = lh
                .tool(
                    "traverse_normative_graph",
                    json!({ "centro_id": CENTRO_HUB, "nodo_origen": origen, "max_saltos": 2 }),
                )
                .await;
            let nodos = grafo["nodos"].as_array().unwrap();
            println!(
                "  desde {origen}: {} nodos, {} aristas",
                nodos.len(),
                grafo["aristas"].as_array().unwrap().len()
            );
            assert!(nodos.len() <= 50, "{origen}: {} nodos", nodos.len());
            if origen == "PO-SINT-01" {
                let hub = nodos
                    .iter()
                    .find(|n| n["id"] == "NORMA-HUB")
                    .expect("el hub aparece");
                assert_eq!(hub["hub_resumido"], true, "{hub}");
                assert!(hub["grado"].as_u64().unwrap() > ARISTAS_HUB as u64);
                assert!(
                    nodos
                        .iter()
                        .all(|n| !n["id"].as_str().unwrap().starts_with("REG-")),
                    "el hub no se expande"
                );
            }
        }

        let r = lh
            .preguntar(
                CENTRO_HUB,
                "Recorre el grafo normativo desde el procedimiento PO-SINT-01 hasta dos saltos: ¿con qué documentos y registros se relaciona?",
                "sonnet",
            )
            .await;
        assert!(r.respuesta.contains("NORMA-HUB"), "{}", r.respuesta);
        assert!(
            tool_con(&r, "traverse_normative_graph", |_, res| {
                let nodos = res["nodos"].as_array().map(Vec::len).unwrap_or(99);
                nodos <= 50 && res.to_string().contains("\"hub_resumido\":true")
            }),
            "el agente recibe un recorrido acotado con el hub resumido"
        );
        let menciones_reg = r.respuesta.matches("REG-").count();
        assert!(menciones_reg <= 50, "lista {menciones_reg} registros del hub");
    });
    resultado("P4 supernodos", inicio);
}

// ---------------------------------------------------------------------------------------------
// P5 — Prelación normativa
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn p5_prelacion_el_pliego_prevalece_sobre_la_norma() {
    let lh = compartido();
    let inicio = Instant::now();
    rt().block_on(async {
        let busqueda = lh
            .tool(
                "search_normative_context",
                json!({ "centro_id": CENTRO_A, "consulta": "porcentaje de uniones soldadas a tope a ensayar con radiografía" }),
            )
            .await;
        let texto_busqueda = busqueda.to_string();
        assert!(texto_busqueda.contains("PCC-01") && texto_busqueda.contains("NT-SOL-01"));

        let r = lh
            .preguntar(
                CENTRO_A,
                "¿Qué porcentaje de las uniones soldadas a tope hay que ensayar con radiografía en esta obra?",
                "sonnet",
            )
            .await;
        let texto = &r.respuesta;
        assert!(contiene_alguno(texto, &["100 %", "100%"]), "{texto}");
        assert!(texto.contains("PCC-01"), "cita el pliego: {texto}");
        assert!(
            texto.contains("NT-SOL-01") || contiene_alguno(texto, &["10 %", "10%"]),
            "menciona el supletorio: {texto}"
        );
        assert!(
            contiene_alguno(
                &texto.to_lowercase(),
                &["prevalece", "prelación", "prima", "supletori"]
            ),
            "explica la prelación: {texto}"
        );
    });
    resultado("P5 prelación", inicio);
}

// ---------------------------------------------------------------------------------------------
// P6 — Agregación global exhaustiva (SQL sobre tablas completas, no top-K)
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn p6_agregacion_exhaustiva_coincide_con_el_oraculo() {
    let lh = compartido();
    let inicio = Instant::now();
    let oraculo: BTreeSet<(String, String)> = [
        ("FOR-ALM-01", "Jefe de Almacén"),
        ("FOR-ALM-05", "Jefe de Producción"),
        ("PPI-MAT-09", "Inspector QA/QC"),
    ]
    .iter()
    .map(|(c, r)| (c.to_string(), r.to_string()))
    .collect();
    rt().block_on(async {
        let filas = lh
            .sql(
                CENTRO_A,
                "SELECT DISTINCT m.registro_codigo, m.rol_responsable \
                 FROM \"03_matriz_operativa\" m JOIN \"01_documentos_catalogo\" d \
                 ON m.doc_id = d.doc_id AND m.version_num = d.version_num \
                 WHERE d.vigente = true AND m.doc_id = 'PO-ALM-03'",
            )
            .await;
        let calculado: BTreeSet<(String, String)> = filas
            .iter()
            .map(|f| {
                (
                    f["registro_codigo"].as_str().unwrap_or_default().to_string(),
                    f["rol_responsable"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        assert_eq!(calculado, oraculo, "la SQL coincide al 100 % con el oráculo");

        let r = lh
            .preguntar(
                CENTRO_A,
                "Dame el listado exhaustivo de todos los registros que exige el procedimiento PO-ALM-03 vigente, con la fase y el responsable que firma cada uno.",
                "sonnet",
            )
            .await;
        for (codigo, responsable) in &oraculo {
            assert!(r.respuesta.contains(codigo.as_str()), "falta {codigo}");
            assert!(r.respuesta.contains(responsable.as_str()), "falta {responsable}");
        }
        assert!(!r.respuesta.contains("FOR-ALM-02"), "registro derogado");
        let exhaustivas = ["execute_athena_sql", "query_operational_matrix"];
        assert!(
            usadas(&r).iter().any(|t| exhaustivas.contains(t)),
            "lo calcula una consulta sobre la tabla completa, no el top-K: {:?}",
            usadas(&r)
        );

        let r = lh
            .preguntar(
                CENTRO_A,
                "¿En qué procedimientos vigentes interviene el Jefe de Producción y qué formatos firma? Quiero la lista completa.",
                "sonnet",
            )
            .await;
        assert!(r.respuesta.contains("PO-ALM-03"), "{}", r.respuesta);
        assert!(r.respuesta.contains("FOR-ALM-05"), "{}", r.respuesta);
        assert!(!r.respuesta.contains("FOR-ALM-02"), "{}", r.respuesta);
        assert!(
            usadas(&r).iter().any(|t| exhaustivas.contains(t)),
            "{:?}",
            usadas(&r)
        );
        println!("  tools: {:?}", usadas(&r));
    });
    resultado("P6 agregación", inicio);
}

// ---------------------------------------------------------------------------------------------
// P7 — Alcance condicional (condición heredada del encabezado)
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn p7_condicion_heredada_80_grados_siempre_con_bajo_cero() {
    let lh = compartido();
    let inicio = Instant::now();
    rt().block_on(async {
        let busqueda = lh
            .tool(
                "search_normative_context",
                json!({ "centro_id": CENTRO_A, "consulta": "calentar la zona de soldadura a 80 °C", "doc_id": "NT-SOL-01" }),
            )
            .await;
        let mut con_80 = 0;
        for grupo in busqueda["grupos"].as_array().unwrap() {
            for fragmento in grupo["resultados"].as_array().unwrap() {
                if fragmento.to_string().contains("80 °C") {
                    con_80 += 1;
                    assert!(
                        fragmento["condicion_aplicacion"]
                            .as_str()
                            .is_some_and(|c| c.contains("< 0")),
                        "80 °C sin su condición: {fragmento}"
                    );
                }
            }
        }
        assert!(con_80 > 0, "{busqueda}");

        for pregunta in [
            "Según la norma NT-SOL-01, ¿a qué temperatura hay que calentar la zona de soldadura antes de soldar y en qué situación se aplica?",
            "¿A cuántos grados hay que precalentar la zona de soldadura según la NT-SOL-01?",
        ] {
            let r = lh.preguntar(CENTRO_A, pregunta, "sonnet").await;
            assert!(r.respuesta.contains("80"), "{}", r.respuesta);
            assert!(
                r.respuesta.contains("< 0") || r.respuesta.to_lowercase().contains("bajo cero"),
                "«80 °C» llega sin «< 0 °C»: {}",
                r.respuesta
            );
        }
    });
    resultado("P7 condición heredada", inicio);
}

// ---------------------------------------------------------------------------------------------
// P8 — Desincronización tri-store (fallo inyectado a mitad de ingesta)
// ---------------------------------------------------------------------------------------------

/// Almacén real con un interruptor que hace fallar la escritura de la tabla 05
struct AlmacenConFallo {
    interno: Arc<dyn LakehouseStorePort>,
    fallar: Arc<AtomicBool>,
}

#[async_trait]
impl LakehouseStorePort for AlmacenConFallo {
    async fn inicializar_tablas(&self) -> Result<(), DomainError> {
        self.interno.inicializar_tablas().await
    }
    async fn existe_hash(
        &self,
        centro_id: &CenterId,
        hash: &str,
    ) -> Result<Option<HuellaIngerida>, DomainError> {
        self.interno.existe_hash(centro_id, hash).await
    }
    async fn escribir_staged(
        &self,
        ingest_id: &IngestId,
        lotes: Vec<LoteIngesta>,
    ) -> Result<usize, DomainError> {
        if !self.fallar.load(Ordering::SeqCst) {
            return self.interno.escribir_staged(ingest_id, lotes).await;
        }
        let mut escritas = 0;
        for lote in lotes {
            if matches!(lote, LoteIngesta::Celdas(_)) {
                return Err(DomainError::StorageError(
                    "Fallo inyectado al escribir 05_celdas_doble_entrada".to_string(),
                ));
            }
            escritas += self.interno.escribir_staged(ingest_id, vec![lote]).await?;
        }
        Ok(escritas)
    }
    async fn confirmar_ingesta(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
    ) -> Result<ResumenConfirmacion, DomainError> {
        self.interno.confirmar_ingesta(ingest_id, doc_id).await
    }
    async fn revertir_ingesta(&self, ingest_id: &IngestId) -> Result<u64, DomainError> {
        self.interno.revertir_ingesta(ingest_id).await
    }
    async fn leer_tabla(
        &self,
        nombre_tabla: &str,
        consulta: &ConsultaTabla,
    ) -> Result<Vec<RecordBatch>, DomainError> {
        self.interno.leer_tabla(nombre_tabla, consulta).await
    }
    async fn verificar_integridad(&self) -> Result<InformeIntegridad, DomainError> {
        self.interno.verificar_integridad().await
    }
    async fn contar_registros(&self, nombre_tabla: &str) -> Result<usize, DomainError> {
        self.interno.contar_registros(nombre_tabla).await
    }
    async fn listar_tablas(&self) -> Result<Vec<String>, DomainError> {
        self.interno.listar_tablas().await
    }
}

/// Espacio de centros real cuyo almacén va envuelto en `AlmacenConFallo`
struct EspacioConFallo {
    interno: Arc<dyn CenterWorkspacePort>,
    fallar: Arc<AtomicBool>,
    almacen_real: Arc<Mutex<Option<Arc<dyn LakehouseStorePort>>>>,
}

#[async_trait]
impl CenterWorkspacePort for EspacioConFallo {
    fn ruta_absoluta(&self, ruta: &Path) -> Result<PathBuf, DomainError> {
        self.interno.ruta_absoluta(ruta)
    }
    async fn preparar(
        &self,
        ruta: &Path,
        manifiesto: &ManifiestoCentroDisco,
        modelo_activo: &ModeloEmbeddingsActivo,
    ) -> Result<ManifiestoCentroDisco, DomainError> {
        self.interno.preparar(ruta, manifiesto, modelo_activo).await
    }
    async fn abrir(
        &self,
        id: &CenterId,
        ruta: &Path,
        modelo_activo: &ModeloEmbeddingsActivo,
    ) -> Result<CentroAbierto, DomainError> {
        let mut abierto = self.interno.abrir(id, ruta, modelo_activo).await?;
        *self.almacen_real.lock().unwrap() = Some(abierto.store.clone());
        abierto.store = Arc::new(AlmacenConFallo {
            interno: abierto.store,
            fallar: self.fallar.clone(),
        });
        Ok(abierto)
    }
    async fn guardar_original(
        &self,
        ruta_centro: &Path,
        ruta_archivo: &Path,
    ) -> Result<DocumentoOriginal, DomainError> {
        self.interno
            .guardar_original(ruta_centro, ruta_archivo)
            .await
    }
}

async fn filas_lance(
    almacen: &Arc<dyn LakehouseStorePort>,
    tabla: &str,
    solo_vigentes: bool,
) -> Vec<RecordBatch> {
    let mut consulta = ConsultaTabla::confirmadas(100_000);
    if solo_vigentes {
        consulta = consulta.solo_vigentes();
    }
    almacen.leer_tabla(tabla, &consulta).await.unwrap()
}

fn columna_texto(lotes: &[RecordBatch], columna: &str) -> BTreeSet<String> {
    let mut valores = BTreeSet::new();
    for lote in lotes {
        let Some(array) = lote.column_by_name(columna) else {
            continue;
        };
        let textos = array.as_any().downcast_ref::<StringArray>().unwrap();
        for i in 0..textos.len() {
            if !textos.is_null(i) {
                valores.insert(textos.value(i).to_string());
            }
        }
    }
    valores
}

fn total_filas(lotes: &[RecordBatch]) -> usize {
    lotes.iter().map(RecordBatch::num_rows).sum()
}

/// Las vistas derivadas (DataFusion, grafo e índice semántico) coinciden con LanceDB
async fn verificar_vistas_coinciden(
    lh: &Lakehouse,
    indice: &Arc<dyn IndiceSemanticoPort>,
    almacen: &Arc<dyn LakehouseStorePort>,
) {
    for tabla in TODAS_LAS_TABLAS {
        let total = total_filas(&filas_lance(almacen, tabla, false).await);
        let vigentes = total_filas(&filas_lance(almacen, tabla, true).await);
        let vista = lh
            .sql(
                CENTRO_P8,
                &format!(
                    "SELECT COUNT(*) AS total, SUM(CASE WHEN vigente THEN 1 ELSE 0 END) AS vigentes FROM \"{tabla}\""
                ),
            )
            .await;
        println!(
            "  {tabla}: LanceDB {total} ({vigentes} vigentes) · DataFusion {} ({})",
            vista[0]["total"], vista[0]["vigentes"]
        );
        assert_eq!(vista[0]["total"], total as i64, "{tabla}");
        assert_eq!(
            vista[0]["vigentes"].as_i64().unwrap_or(0),
            vigentes as i64,
            "{tabla}"
        );
    }

    // Índice semántico = chunks (02) + hitos (03) confirmados de LanceDB
    let mut esperados = columna_texto(
        &filas_lance(almacen, "02_chunks_normativa", false).await,
        "chunk_id",
    );
    esperados.extend(columna_texto(
        &filas_lance(almacen, "03_matriz_operativa", false).await,
        "hito_id",
    ));
    let indexados: BTreeSet<String> = indice
        .fragmentos(&CenterId::new(CENTRO_P8).unwrap())
        .await
        .into_iter()
        .map(|f| f.id)
        .collect();
    println!(
        "  índice semántico: {} fragmentos · LanceDB 02+03: {}",
        indexados.len(),
        esperados.len()
    );
    assert_eq!(indexados, esperados, "índice semántico ≠ LanceDB");

    // Grafo: las aristas vigentes de 06 son exactamente las que recorre la tool
    let aristas_lance = columna_texto(
        &filas_lance(almacen, "06_grafo_referencias", true).await,
        "edge_id",
    );
    let grafo = lh
        .tool(
            "traverse_normative_graph",
            json!({ "centro_id": CENTRO_P8, "nodo_origen": "PO-ALM-03", "max_saltos": 1, "max_vecinos": 50 }),
        )
        .await;
    let aristas_grafo: BTreeSet<String> = grafo["aristas"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["cita"]["edge_id"].as_str().map(str::to_string))
        .collect();
    let faltan: Vec<&String> = aristas_lance.difference(&aristas_grafo).collect();
    println!(
        "  grafo: {} aristas vigentes en 06 · {} en el recorrido · no recorridas {:?}",
        aristas_lance.len(),
        aristas_grafo.len(),
        faltan
    );
    assert!(faltan.is_empty(), "aristas de 06 ausentes del grafo");
}

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn p8_tri_store_fallo_inyectado_sin_residuos_y_vistas_coherentes() {
    let fallar = Arc::new(AtomicBool::new(false));
    let almacen_real: Arc<Mutex<Option<Arc<dyn LakehouseStorePort>>>> = Arc::default();
    let (lh, indice) = nueva_lakehouse(|comp| {
        comp.dependencias.espacio = Arc::new(EspacioConFallo {
            interno: comp.dependencias.espacio.clone(),
            fallar: fallar.clone(),
            almacen_real: almacen_real.clone(),
        });
    });
    let inicio = Instant::now();
    rt().block_on(async {
        lh.crear(CENTRO_P8, true).await;
        lh.ingestar(&fixture("procedimiento_recepcion_rev03.md"), CENTRO_P8)
            .await;
        let almacen = almacen_real.lock().unwrap().clone().expect("centro abierto");
        let antes: Vec<usize> = {
            let mut cuentas = Vec::new();
            for tabla in TODAS_LAS_TABLAS {
                cuentas.push(almacen.contar_registros(tabla).await.unwrap());
            }
            cuentas
        };

        // Fallo inyectado en la tabla 05 durante la ingesta de la Rev. 04
        fallar.store(true, Ordering::SeqCst);
        let error = lh
            .app
            .ingestar(&fixture("procedimiento_recepcion_rev04.md"), CENTRO_P8)
            .await
            .expect_err("la ingesta debe fallar");
        println!("  error esperado: {error}");
        assert!(error.to_string().contains("Fallo inyectado"), "{error}");
        fallar.store(false, Ordering::SeqCst);

        for (tabla, filas_antes) in TODAS_LAS_TABLAS.iter().zip(&antes) {
            let filas_despues = almacen.contar_registros(tabla).await.unwrap();
            println!("  {tabla}: {filas_antes} → {filas_despues} filas físicas");
            assert_eq!(filas_despues, *filas_antes, "residuos en {tabla}");
        }
        let integridad = lh.app.integridad(CENTRO_P8).await.unwrap();
        assert!(integridad.es_integro(), "{integridad:?}");
        let vigente = lh
            .tool(
                "lookup_double_entry_table",
                json!({ "centro_id": CENTRO_P8, "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42 }),
            )
            .await;
        assert!(
            vigente["valor_resultado_texto"].as_str().unwrap().contains("100"),
            "la Rev. 03 sigue vigente: {vigente}"
        );

        // Reconstrucción desde LanceDB: las tres vistas coinciden con el almacén
        lh.app.desactivar_centro(CENTRO_P8).await.unwrap();
        lh.app.activar_centro(CENTRO_P8).await.unwrap();
        verificar_vistas_coinciden(&lh, &indice, &almacen).await;

        // Reintento sin fallo: la Rev. 04 entra y las vistas siguen coincidiendo
        lh.ingestar(&fixture("procedimiento_recepcion_rev04.md"), CENTRO_P8)
            .await;
        let nuevo = lh
            .tool(
                "lookup_double_entry_table",
                json!({ "centro_id": CENTRO_P8, "tabla_id": "4-A", "valor_x": 30, "valor_y": 0.42 }),
            )
            .await;
        assert!(nuevo["valor_resultado_texto"].as_str().unwrap().contains("125"));
        assert!(lh.app.integridad(CENTRO_P8).await.unwrap().es_integro());
        verificar_vistas_coinciden(&lh, &indice, &almacen).await;
    });
    resultado("P8 tri-store", inicio);
}

// ---------------------------------------------------------------------------------------------
// §9.1 — Multi-centro: solo el centro activo es legible
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn s9_1_multicentro_solo_se_leen_los_datos_del_centro_activo() {
    let lh = compartido();
    let inicio = Instant::now();
    rt().block_on(async {
        let centros = lh.app.listar_centros().await.unwrap();
        let b = centros
            .iter()
            .find(|c| c.id().as_str() == CENTRO_B)
            .unwrap();
        assert!(!b.esta_activo(), "B queda inactivo");
        assert!(
            Path::new(&lh.ruta_centro(CENTRO_B))
                .join("raw_docs")
                .exists(),
            "B tiene sus datos en su propia carpeta"
        );

        let documentos = lh
            .sql(
                CENTRO_A,
                "SELECT DISTINCT doc_id FROM \"01_documentos_catalogo\"",
            )
            .await;
        let ids: BTreeSet<String> = documentos
            .iter()
            .map(|d| d["doc_id"].as_str().unwrap().to_string())
            .collect();
        println!("  documentos visibles desde A: {ids:?}");
        assert!(ids.contains("PO-ALM-03") && !ids.contains("PO-OBRA-B-01"));

        let cruzada = lh
            .tool_error(
                "execute_athena_sql",
                json!({ "centro_id": CENTRO_A, "sql_query": format!("SELECT * FROM \"{CENTRO_B}_01_documentos_catalogo\"") }),
            )
            .await;
        println!("  SQL de A sobre la vista de B → {cruzada}");
        let inactivo = lh
            .tool_error(
                "query_operational_matrix",
                json!({ "centro_id": CENTRO_B, "fase": "CUSTODIA" }),
            )
            .await;
        println!("  tool sobre B inactivo → {inactivo}");

        let r = lh
            .preguntar(
                CENTRO_A,
                "Lista todos los documentos del catálogo de este centro con su revisión.",
                "sonnet",
            )
            .await;
        assert!(r.respuesta.contains("PO-ALM-03"), "{}", r.respuesta);
        for ajeno in ["PO-OBRA-B-01", "FOR-B-99", "Obra B"] {
            assert!(!r.respuesta.contains(ajeno), "lee datos de B: {}", r.respuesta);
        }
    });
    resultado("§9.1 multi-centro", inicio);
}

// ---------------------------------------------------------------------------------------------
// §9.4 — Selector de modelos en caliente con las tools en vivo
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "real: Claude Code headless + FastEmbed E5-large"]
fn s9_4_selector_de_modelos_en_caliente_con_tools_en_vivo() {
    let lh = compartido();
    let inicio = Instant::now();
    rt().block_on(async {
        let catalogo = lh.app.listar_modelos().await.unwrap();
        let alias: Vec<String> = catalogo
            .iter()
            .map(|m| m.alias.cli_model_name().to_string())
            .collect();
        println!("  catálogo real de Claude Code: {alias:?}");
        for esperado in ["sonnet", "opus", "haiku"] {
            assert!(
                alias.iter().any(|a| a.contains(esperado)),
                "falta {esperado}"
            );
        }

        let centro = CenterId::new(CENTRO_A).unwrap();
        let mut historial = Vec::new();
        let turnos = [
            ("haiku", "¿Quién es el responsable de la custodia y qué registro llena?", ["Jefe de Producción", "FOR-ALM-05"]),
            ("sonnet", "¿Y quién firma la recepción y con qué registro?", ["Jefe de Almacén", "FOR-ALM-01"]),
            ("opus", "¿Y la inspección? Responsable y registro.", ["Inspector QA/QC", "PPI-MAT-09"]),
        ];
        let runner = ClaudeAgentRunner::new(ClaudeAgentConfig::default());
        for (alias, pregunta, esperados) in turnos {
            lh.modelos_usados.lock().unwrap().clear();
            let eventos: Arc<Mutex<Vec<String>>> = Arc::default();
            let destino = eventos.clone();
            let modelo = ClaudeModel::new(alias).unwrap();
            historial.push(AgentMessage::user(pregunta));
            let t = Instant::now();
            let respuesta = lh
                .app
                .conversar(
                    &modelo,
                    &centro,
                    &mut historial,
                    Some(Arc::new(move |evento| {
                        if let AgentExecutionEvent::CallingTool { name, .. } = evento {
                            destino.lock().unwrap().push(name);
                        }
                    })),
                )
                .await
                .unwrap();
            let usados = lh.modelos_usados.lock().unwrap().clone();
            let args = runner.construir_argumentos(&modelo, "…");
            let flag = &args[args.iter().position(|a| a == "--model").unwrap() + 1];
            println!(
                "\n  [{alias} → --model {flag}, {:.1} s] P: {pregunta}\n  subprocesos: {usados:?}\n  tools en vivo: {:?}\n  R: {}",
                t.elapsed().as_secs_f64(),
                eventos.lock().unwrap(),
                recortar(&respuesta, 600)
            );
            assert!(!usados.is_empty() && usados.iter().all(|m| m == alias), "{usados:?}");
            assert_eq!(flag, alias);
            assert!(!eventos.lock().unwrap().is_empty(), "sin tools en vivo");
            for esperado in esperados {
                assert!(respuesta.contains(esperado), "[{alias}] falta {esperado}: {respuesta}");
            }
        }
    });
    resultado("§9.4 selector de modelos", inicio);
}
