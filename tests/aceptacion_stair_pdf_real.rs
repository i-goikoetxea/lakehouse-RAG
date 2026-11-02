//! Aceptación STAIR Fase 5 con un PDF real del usuario, Claude Code y FastEmbed reales.
//! El PDF no está en el repositorio y la evidencia se escribe fuera de él:
//! `LAKEHOUSE_PDF_ACEPTACION=<ruta.pdf> LAKEHOUSE_SALIDA_ACEPTACION=<carpeta>
//! LAKEHOUSE_PREGUNTAS_ACEPTACION=<preguntas.json>`
//! `cargo test --test aceptacion_stair_pdf_real -- --ignored --test-threads=1 --nocapture`

#[path = "comun_claude_falso/mod.rs"]
mod comun_claude_falso;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use comun_claude_falso::ClaudeFalso;
use lakehouse_rag::application::dtos::tools::ToolCall;
use lakehouse_rag::application::ports::document_parser::DocumentParserPort;
use lakehouse_rag::application::ports::embedding_engine::EmbeddingEnginePort;
use lakehouse_rag::application::use_cases::lakehouse_app::LakehouseApp;
use lakehouse_rag::domain::services::fragmentacion::estimar_tokens;
use lakehouse_rag::infrastructure::bootstrap::{ConfiguracionComposicion, componer_lakehouse};
use lakehouse_rag::infrastructure::embeddings::fastembed_engine::FastEmbedEngine;
use lakehouse_rag::infrastructure::parsers::ParserPorFormato;
use serde_json::{Value, json};

const CENTRO: &str = "aceptacion-stair";

/// Pregunta de aceptación. Ninguna se versiona, porque revelaría el contenido del documento:
/// están en el archivo JSON que indica `LAKEHOUSE_PREGUNTAS_ACEPTACION`.
/// Formato: `tests/fixtures/preguntas_aceptacion_ejemplo.json`.
#[derive(serde::Deserialize)]
struct PreguntaAceptacion {
    id: String,
    categoria: String,
    pregunta: String,
}

fn interpretar_preguntas(json: &str) -> Vec<PreguntaAceptacion> {
    let preguntas: Vec<PreguntaAceptacion> =
        serde_json::from_str(json).expect("archivo de preguntas con formato inválido");
    assert!(!preguntas.is_empty(), "el archivo de preguntas está vacío");
    preguntas
}

fn preguntas_externas() -> Vec<PreguntaAceptacion> {
    let ruta = std::env::var("LAKEHOUSE_PREGUNTAS_ACEPTACION")
        .expect("LAKEHOUSE_PREGUNTAS_ACEPTACION: ruta del JSON con las preguntas");
    interpretar_preguntas(&std::fs::read_to_string(ruta).unwrap())
}

#[test]
fn el_ejemplo_documenta_el_formato_de_preguntas() {
    let preguntas =
        interpretar_preguntas(include_str!("fixtures/preguntas_aceptacion_ejemplo.json"));
    assert!(
        preguntas
            .iter()
            .all(|p| !p.id.is_empty() && !p.categoria.is_empty() && !p.pregunta.is_empty())
    );
}

fn pdf_real() -> PathBuf {
    PathBuf::from(std::env::var("LAKEHOUSE_PDF_ACEPTACION").expect("LAKEHOUSE_PDF_ACEPTACION"))
}

fn guardar(nombre: &str, contenido: &Value) {
    let carpeta =
        PathBuf::from(std::env::var("LAKEHOUSE_SALIDA_ACEPTACION").expect("salida de evidencia"));
    std::fs::create_dir_all(&carpeta).unwrap();
    std::fs::write(
        carpeta.join(nombre),
        serde_json::to_string_pretty(contenido).unwrap(),
    )
    .unwrap();
}

async fn centro_temporal() -> (tempfile::TempDir, Arc<LakehouseApp>) {
    let raiz = tempfile::tempdir().unwrap();
    let configuracion = ConfiguracionComposicion {
        directorio_config: raiz.path().join("config"),
        directorio_cache_embeddings: ConfiguracionComposicion::desde_entorno()
            .directorio_cache_embeddings,
    };
    let app = componer_lakehouse(&configuracion);
    let ruta = raiz.path().join("centro");
    app.crear_centro(CENTRO, &ruta.to_string_lossy(), None)
        .await
        .unwrap();
    app.activar_centro(CENTRO).await.unwrap();
    (raiz, app)
}

async fn herramienta(app: &LakehouseApp, nombre: &str, argumentos: Value) -> Value {
    let resultado = app
        .servidor_herramientas()
        .ejecutar_herramienta(&ToolCall {
            id: "aceptacion".to_string(),
            name: nombre.to_string(),
            arguments: argumentos,
        })
        .await;
    match resultado {
        Ok(r) => json!({"es_error": r.is_error, "salida": r.output}),
        Err(e) => json!({"es_error": true, "salida": e.to_string()}),
    }
}

fn informe_json(
    r: &lakehouse_rag::application::use_cases::lakehouse_app::ResultadoIngesta,
) -> Value {
    json!({
        "informe": format!("{:#?}", r.informe),
        "doc_id": r.informe.doc_id,
        "estado_contenido": r.informe.estado_contenido,
        "pendiente_destilacion": r.informe.pendiente_destilacion,
        "avisos": r.informe.avisos,
        "total_chunks": r.informe.total_chunks,
        "total_nodos": r.informe.total_nodos,
        "total_tuplas": r.informe.total_tuplas,
        "ya_ingerido": r.informe.ya_ingerido,
        "aviso_proyeccion": r.aviso_proyeccion,
    })
}

/// Árbol completo paginando el ToC desde cada nodo
async fn arbol_completo(app: &LakehouseApp, doc_id: &str) -> Vec<Value> {
    let mut acumulado = Vec::new();
    let mut pendientes = vec![None::<String>];
    while let Some(padre) = pendientes.pop() {
        let mut desplazamiento = 0u64;
        loop {
            let mut argumentos =
                json!({"centro_id": CENTRO, "doc_id": doc_id, "desplazamiento": desplazamiento});
            if let Some(nodo) = &padre {
                argumentos["nodo_padre_id"] = json!(nodo);
            }
            let salida = herramienta(app, "stair_navigate_toc", argumentos).await;
            let nodos = salida["salida"]["nodos"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if nodos.is_empty() {
                if padre.is_none() {
                    acumulado.push(salida);
                }
                break;
            }
            desplazamiento += nodos.len() as u64;
            for nodo in nodos {
                if nodo["tiene_hijos"] == json!(true) {
                    pendientes.push(nodo["nodo_id"].as_str().map(str::to_string));
                }
                let mut fila = nodo;
                fila["nodo_padre_id"] = json!(padre);
                acumulado.push(fila);
            }
        }
    }
    acumulado
}

#[tokio::test]
#[ignore = "PDF real del usuario, Claude Code y FastEmbed reales"]
async fn pdf_real_ingesta_arbol_y_preguntas() {
    let (raiz, app) = centro_temporal().await;
    let inicio = Instant::now();
    let ingesta = app.ingestar(&pdf_real(), CENTRO).await.unwrap();
    let segundos_ingesta = inicio.elapsed().as_secs_f64();
    let doc_id = ingesta.informe.doc_id.clone();
    println!("INGESTA {segundos_ingesta:.1}s: {:#?}", ingesta.informe);

    let integridad = app.integridad(CENTRO).await.unwrap();
    println!(
        "INTEGRIDAD integro={} {:?}",
        integridad.es_integro(),
        integridad.informe
    );

    let arbol = arbol_completo(&app, &doc_id).await;
    let mut raices = Vec::new();
    for nodo in &arbol {
        let id = nodo["nodo_id"].as_str().unwrap_or_default();
        if nodo["nodo_padre_id"].is_null() {
            raices.push(
                herramienta(
                    &app,
                    "stair_get_subtree_context",
                    json!({"centro_id": CENTRO, "doc_id": doc_id, "nodo_id": id}),
                )
                .await,
            );
        }
    }
    guardar(
        "pdf_ingesta.json",
        &json!({
            "segundos": segundos_ingesta,
            "ingesta": informe_json(&ingesta),
            "integro": integridad.es_integro(),
            "integridad": format!("{:#?}", integridad.informe),
            "arbol": arbol,
            "subarboles_raiz": raices,
        }),
    );

    println!("MATRIZ {}", resumen_de_matriz(&app).await);

    let repetida = app.ingestar(&pdf_real(), CENTRO).await.unwrap();
    println!("REINGESTA ya_ingerido={}", repetida.informe.ya_ingerido);

    let mut respuestas = Vec::new();
    for PreguntaAceptacion {
        id,
        categoria,
        pregunta,
    } in preguntas_externas()
    {
        let inicio = Instant::now();
        let resultado = app.preguntar(&pregunta, CENTRO, None).await;
        let segundos = inicio.elapsed().as_secs_f64();
        let fila = match resultado {
            Ok(r) => json!({
                "id": id, "categoria": categoria, "pregunta": pregunta, "segundos": segundos,
                "respuesta": r.respuesta,
                "tools": r.herramientas.iter().map(|h| json!({
                    "nombre": h.nombre, "argumentos": h.argumentos, "es_error": h.es_error,
                    "resultado": h.resultado})).collect::<Vec<_>>(),
            }),
            Err(e) => json!({"id": id, "categoria": categoria, "pregunta": pregunta,
                "segundos": segundos, "error": e.to_string()}),
        };
        println!("{id} ({segundos:.0}s) hecho");
        respuestas.push(fila);
        guardar("pdf_preguntas.json", &json!(respuestas));
    }
    assert!(raiz.path().join("centro").exists());
    assert!(integridad.es_integro());
    assert!(repetida.informe.ya_ingerido);
}

/// Prefijo del texto hasta `objetivo` tokens estimados, cortando en un límite de palabra
/// Solo recuentos: no imprime texto del documento
async fn resumen_de_matriz(app: &LakehouseApp) -> Value {
    let filas = herramienta(
        app,
        "execute_athena_sql",
        json!({"centro_id": CENTRO, "sql_query": "SELECT origen_responsable, calidad_extraccion, COUNT(*) AS filas FROM 03_matriz_operativa WHERE vigente = true GROUP BY origen_responsable, calidad_extraccion ORDER BY 1, 2"}),
    )
    .await;
    let fechas = herramienta(
        app,
        "execute_athena_sql",
        json!({"centro_id": CENTRO, "sql_query": "SELECT COUNT(*) AS documentos, COUNT(fecha_vigor) AS con_fecha FROM 01_documentos_catalogo WHERE vigente = true"}),
    )
    .await;
    json!({"matriz": filas["salida"]["resultado"], "catalogo": fechas["salida"]["resultado"]})
}

/// Estabilidad de la Capa 2 entre ingestas del mismo PDF, sin imprimir contenido. El detalle va a
/// la carpeta de evidencia. `LAKEHOUSE_INGESTAS_ESTABILIDAD` indica cuántas (4 por defecto).
#[tokio::test]
#[ignore = "PDF real del usuario, Claude Code y FastEmbed reales"]
async fn pdf_real_estabilidad_de_la_matriz() {
    let ingestas: usize = std::env::var("LAKEHOUSE_INGESTAS_ESTABILIDAD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let mut conjuntos: Vec<std::collections::BTreeSet<String>> = Vec::new();
    let mut detalle = Vec::new();
    for ingesta in 0..ingestas {
        let (_raiz, app) = centro_temporal().await;
        app.ingestar(&pdf_real(), CENTRO).await.unwrap();
        let matriz = herramienta(
            &app,
            "query_operational_matrix",
            json!({"centro_id": CENTRO}),
        )
        .await;
        let matriz = &matriz["salida"];
        let claves: std::collections::BTreeSet<String> = matriz["filas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                format!(
                    "{}|{}|{}",
                    f["fase_operativa"], f["rol_responsable"], f["origen_responsable"]
                )
            })
            .collect();
        let parecido_con_la_primera = conjuntos.first().map_or(1.0, |primera| {
            primera.intersection(&claves).count() as f64
                / primera.union(&claves).count().max(1) as f64
        });
        println!(
            "ESTABILIDAD ingesta={ingesta} hitos={} combinaciones={} jaccard={parecido_con_la_primera:.2} {}",
            matriz["total"],
            claves.len(),
            resumen_de_matriz(&app).await
        );
        conjuntos.push(claves);
        detalle.push(matriz.clone());
        guardar("pdf_estabilidad.json", &json!(detalle));
    }
}

fn prefijo_estimado(texto: &str, objetivo: usize) -> String {
    let mut prefijo = String::new();
    for palabra in texto.split_inclusive(char::is_whitespace) {
        if estimar_tokens(&prefijo) + estimar_tokens(palabra) > objetivo {
            break;
        }
        prefijo.push_str(palabra);
    }
    prefijo
}

/// Riesgo del recorte a 512 tokens: dónde empieza a truncar el tokenizer real según la estimación
/// determinista de la fragmentación, con y sin un breadcrumb largo como el del PDF real.
#[tokio::test]
#[ignore = "PDF real del usuario y FastEmbed real"]
async fn medir_recorte_de_512_tokens() {
    const BREADCRUMB: &str = "IX CONTENIDO > 4 FASES DEL PROCESO > 4.18 Si no se dispone de espacio propio para el acopio de equipos y materiales, el Inspector";
    let motor = FastEmbedEngine::try_new(None).unwrap();
    let bytes = std::fs::read(pdf_real()).unwrap();
    let pdf = ParserPorFormato::new()
        .parse_bytes("real.pdf", &bytes)
        .await
        .unwrap()
        .contenido_completo;
    let directorio = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut fixtures = String::new();
    for entrada in std::fs::read_dir(&directorio).unwrap() {
        let ruta = entrada.unwrap().path();
        if ruta.extension().is_some_and(|e| e == "md") {
            fixtures.push_str(&std::fs::read_to_string(ruta).unwrap());
            fixtures.push('\n');
        }
    }
    let mut filas = Vec::new();
    for (corpus, texto) in [("pdf_real", pdf.as_str()), ("fixtures", fixtures.as_str())] {
        println!("{corpus}: {} tokens estimados", estimar_tokens(texto));
        for objetivo in [256, 300, 350, 384, 400, 420, 448, 480, 512, 544, 600] {
            let prefijo = prefijo_estimado(texto, objetivo);
            let con_miga = format!("{BREADCRUMB}\nCondición: en obra\n{prefijo}");
            let solo = motor
                .contar_truncados(std::slice::from_ref(&prefijo))
                .await
                .unwrap();
            let miga = motor.contar_truncados(&[con_miga]).await.unwrap();
            println!(
                "{corpus} objetivo={objetivo} estimado={} truncado={solo} con_miga={miga}",
                estimar_tokens(&prefijo)
            );
            filas.push(json!({"corpus": corpus, "objetivo": objetivo,
                "estimado": estimar_tokens(&prefijo), "truncado": solo, "truncado_con_miga": miga}));
        }
    }
    guardar("recorte_512.json", &json!(filas));
}

/// Avisos de truncado que el informe de ingesta da con cada fixture Markdown, sin Claude real
#[tokio::test]
#[ignore = "FastEmbed real; altera el PATH de este proceso"]
async fn fixtures_avisos_de_truncado() {
    let falso = ClaudeFalso::compilar(r#"fn main() { std::process::exit(1); }"#, &[]);
    let path_original = std::env::var_os("PATH").unwrap_or_default();
    let mut rutas = vec![falso.binario.parent().unwrap().to_path_buf()];
    rutas.extend(std::env::split_paths(&path_original));
    // SAFETY: suite ejecutada con --test-threads=1; ningún otro hilo lee el entorno ahora.
    unsafe { std::env::set_var("PATH", std::env::join_paths(rutas).unwrap()) };
    let (_raiz, app) = centro_temporal().await;
    let directorio = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut filas = Vec::new();
    for entrada in std::fs::read_dir(&directorio).unwrap() {
        let ruta = entrada.unwrap().path();
        let nombre = ruta.file_name().unwrap().to_string_lossy().to_string();
        if ruta.extension().is_some_and(|e| e == "md") && nombre != "README.md" {
            let r = app.ingestar(&ruta, CENTRO).await.unwrap();
            let truncado = r
                .informe
                .avisos
                .iter()
                .any(|a| a.contains("superan el límite"));
            println!(
                "{nombre}: chunks={} truncado={truncado}",
                r.informe.total_chunks
            );
            filas.push(
                json!({"archivo": nombre, "chunks": r.informe.total_chunks, "truncado": truncado}),
            );
        }
    }
    // SAFETY: como arriba.
    unsafe { std::env::set_var("PATH", &path_original) };
    guardar("recorte_fixtures.json", &json!(filas));
}

/// Claude no disponible en este proceso: la ingesta y la búsqueda siguen; el reintento con el
/// binario real completa la Capa 2. La sesión real del usuario no se toca.
#[tokio::test]
#[ignore = "PDF real del usuario, Claude Code y FastEmbed reales; altera el PATH de este proceso"]
async fn pdf_real_fallback_sin_claude_y_reintento() {
    let falso = ClaudeFalso::compilar(
        r#"fn main() {
            eprintln!("Claude falso: no autenticado");
            std::process::exit(1);
        }"#,
        &[],
    );
    let path_original = std::env::var_os("PATH").unwrap_or_default();
    let directorio_falso = falso.binario.parent().unwrap().to_path_buf();
    let mut rutas = vec![directorio_falso];
    rutas.extend(std::env::split_paths(&path_original));
    // SAFETY: suite ejecutada con --test-threads=1; ningún otro hilo lee el entorno ahora.
    unsafe { std::env::set_var("PATH", std::env::join_paths(rutas).unwrap()) };

    let (_raiz, app) = centro_temporal().await;
    let ingesta = app.ingestar(&pdf_real(), CENTRO).await.unwrap();
    let provisional = ingesta.informe.doc_id.clone();
    println!("FALLBACK: {:#?}", ingesta.informe);
    let integridad_provisional = app.integridad(CENTRO).await.unwrap().es_integro();

    let busqueda = herramienta(
        &app,
        "search_normative_context",
        json!({"centro_id": CENTRO, "consulta": "acta de cese de equipos en custodia"}),
    )
    .await;
    let toc = herramienta(
        &app,
        "stair_navigate_toc",
        json!({"centro_id": CENTRO, "doc_id": provisional}),
    )
    .await;
    let error_chat = app.preguntar("¿Cuál es el objeto?", CENTRO, None).await;
    let reingesta = app.ingestar(&pdf_real(), CENTRO).await.unwrap();
    let reintento_fallido = app.reintentar_documento(&provisional, CENTRO).await;
    let sigue = herramienta(
        &app,
        "stair_navigate_toc",
        json!({"centro_id": CENTRO, "doc_id": provisional}),
    )
    .await;

    // SAFETY: como arriba.
    unsafe { std::env::set_var("PATH", &path_original) };
    let reintento = app.reintentar_documento(&provisional, CENTRO).await;
    let reintento_json = match &reintento {
        Ok(r) => informe_json(r),
        Err(e) => json!({"error": e.to_string()}),
    };
    let canonico = reintento
        .as_ref()
        .map(|r| r.informe.doc_id.clone())
        .unwrap_or_default();
    let integridad_final = app.integridad(CENTRO).await.unwrap().es_integro();
    let toc_alias = herramienta(
        &app,
        "stair_navigate_toc",
        json!({"centro_id": CENTRO, "doc_id": provisional}),
    )
    .await;
    let toc_canonico = herramienta(
        &app,
        "stair_navigate_toc",
        json!({"centro_id": CENTRO, "doc_id": canonico}),
    )
    .await;
    let matriz = herramienta(
        &app,
        "query_operational_matrix",
        json!({"centro_id": CENTRO, "doc_id": canonico}),
    )
    .await;
    let segundo_reintento = app.reintentar_documento(&canonico, CENTRO).await;
    let ya_final = segundo_reintento
        .as_ref()
        .map(|r| r.informe.ya_ingerido)
        .unwrap_or(false);
    guardar(
        "pdf_fallback.json",
        &json!({
            "ingesta": informe_json(&ingesta),
            "integro_provisional": integridad_provisional,
            "busqueda": busqueda,
            "toc_provisional": toc,
            "error_chat": error_chat.as_ref().err().map(|e| e.to_string()),
            "chat_ok": error_chat.is_ok(),
            "reingesta": informe_json(&reingesta),
            "reintento_fallido": reintento_fallido.as_ref().err().map(|e| e.to_string()),
            "reintento_fallido_ok": reintento_fallido.is_ok(),
            "provisional_intacto": sigue,
            "reintento": reintento_json,
            "integro_final": integridad_final,
            "toc_alias": toc_alias,
            "toc_canonico": toc_canonico,
            "matriz_capa2": matriz,
            "segundo_reintento_ya_ingerido": ya_final,
        }),
    );
    assert_eq!(ingesta.informe.estado_contenido, "PARCIAL_UNIVERSAL");
    assert!(reintento.is_ok(), "{reintento_json}");
    assert!(integridad_provisional && integridad_final);
}
