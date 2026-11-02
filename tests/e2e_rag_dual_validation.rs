//! Matriz E2E de los 4 casos críticos (especificación §6.3) con Claude Code y FastEmbed reales.
//! Se ejecuta con autorización del usuario:
//! `cargo test --test e2e_rag_dual_validation -- --ignored --test-threads=1`
//! Oráculo y fixtures: `tests/fixtures/README.md`. Las aserciones son por contenido.

use std::path::PathBuf;
use std::sync::Arc;

use lakehouse_rag::application::use_cases::lakehouse_app::{LakehouseApp, RespuestaLakehouse};
use lakehouse_rag::infrastructure::bootstrap::{ConfiguracionComposicion, componer_lakehouse};

const CENTRO: &str = "e2e-rag-dual";

fn fixture(nombre: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(nombre)
}

/// Compone la aplicación real sobre un directorio temporal e ingiere los dos fixtures
async fn centro_con_fixtures() -> (tempfile::TempDir, Arc<LakehouseApp>) {
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
    for archivo in ["DOC-GENERICO-01.md", "DOC-TECNICO-OBRA-02.md"] {
        let resultado = app.ingestar(&fixture(archivo), CENTRO).await.unwrap();
        assert_eq!(resultado.informe.estado_contenido, "COMPLETO", "{archivo}");
    }
    (raiz, app)
}

fn uso(respuesta: &RespuestaLakehouse, herramienta: &str) -> bool {
    let nombres: Vec<&str> = respuesta
        .herramientas
        .iter()
        .map(|h| h.nombre.as_str())
        .collect();
    println!("TOOLS {nombres:?}\nRESPUESTA {}", respuesta.respuesta);
    respuesta
        .herramientas
        .iter()
        .any(|h| h.nombre == herramienta)
}

fn contiene_todo(texto: &str, terminos: &[&str]) {
    let plegado = texto.to_lowercase();
    for termino in terminos {
        assert!(
            plegado.contains(&termino.to_lowercase()),
            "falta '{termino}' en: {texto}"
        );
    }
}

fn no_contiene(texto: &str, terminos: &[&str]) {
    let plegado = texto.to_lowercase();
    for termino in terminos {
        assert!(
            !plegado.contains(&termino.to_lowercase()),
            "sobra '{termino}' en: {texto}"
        );
    }
}

/// E2E-01: general y narrativo → `search_normative_context` (Capa 0)
#[tokio::test]
#[ignore = "Claude Code y FastEmbed reales: Fase 5, con autorización del usuario"]
async fn e2e_01_politica_de_epis_desde_la_capa_0() {
    let (_raiz, app) = centro_con_fixtures().await;
    let r = app
        .preguntar(
            "¿Cuál es la política general sobre EPIs según el manual de bienvenida?",
            CENTRO,
            None,
        )
        .await
        .unwrap();
    assert!(uso(&r, "search_normative_context"), "{:?}", r.herramientas);
    contiene_todo(&r.respuesta, &["obligatorio", "visitas", "DOC-GENERICO-01"]);
    no_contiene(&r.respuesta, &["no hay datos"]);
}

/// E2E-02: estructura → `stair_navigate_toc` (Capa 1)
#[tokio::test]
#[ignore = "Claude Code y FastEmbed reales: Fase 5, con autorización del usuario"]
async fn e2e_02_apartados_del_capitulo_3_desde_el_indice() {
    let (_raiz, app) = centro_con_fixtures().await;
    let r = app
        .preguntar(
            "¿Qué apartados componen el Capítulo 3 del procedimiento técnico?",
            CENTRO,
            None,
        )
        .await
        .unwrap();
    assert!(uso(&r, "stair_navigate_toc"), "{:?}", r.herramientas);
    contiene_todo(
        &r.respuesta,
        &["3.1", "Recepción", "3.2", "Custodia", "3.3", "Inspección"],
    );
    no_contiene(&r.respuesta, &["3.4"]);
}

/// E2E-03: roles y registros → `query_operational_matrix` (Capa 2), solo la fase CUSTODIA
#[tokio::test]
#[ignore = "Claude Code y FastEmbed reales: Fase 5, con autorización del usuario"]
async fn e2e_03_custodia_sin_contaminacion_de_otras_fases() {
    let (_raiz, app) = centro_con_fixtures().await;
    let r = app
        .preguntar(
            "¿Quién custodia las probetas y qué formato se rellena?",
            CENTRO,
            None,
        )
        .await
        .unwrap();
    assert!(uso(&r, "query_operational_matrix"), "{:?}", r.herramientas);
    contiene_todo(
        &r.respuesta,
        &["Auxiliar de Laboratorio", "Jefe de Calidad", "FOR-PROB-02"],
    );
    no_contiene(
        &r.respuesta,
        &["FOR-PROB-01", "PPI-PROB-03", "Inspector QA/QC"],
    );
}

/// E2E-04: cruce numérico → `lookup_double_entry_table` o SQL (Capa 2), con la nota al pie
#[tokio::test]
#[ignore = "Claude Code y FastEmbed reales: Fase 5, con autorización del usuario"]
async fn e2e_04_cruce_exacto_con_su_nota_al_pie() {
    let (_raiz, app) = centro_con_fixtures().await;
    let r = app
        .preguntar(
            "Para relación A/C de 0.45 y árido rodado, ¿cuál es la resistencia mínima fck?",
            CENTRO,
            None,
        )
        .await
        .unwrap();
    assert!(
        uso(&r, "lookup_double_entry_table") || uso(&r, "execute_athena_sql"),
        "{:?}",
        r.herramientas
    );
    contiene_todo(&r.respuesta, &["30", "CEM I", "Jefe de Calidad"]);
}
