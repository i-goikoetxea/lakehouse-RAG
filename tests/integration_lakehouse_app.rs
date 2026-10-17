//! Integración de la Fase 12: centros en disco, proyección LanceDB → DataFusion/Petgraph/
//! TableLookup y fachada `LakehouseApp` (destilador y agente simulados; disco real).

mod comun_centros;
mod comun_store_v2;

use std::sync::Arc;

use comun_centros::*;
use comun_store_v2::{ARCHIVO_V3, CONTENIDO_V3};
use lakehouse_rag::application::dtos::centros::{
    ARCHIVO_MANIFIESTO, DIRECTORIO_GLUE_CATALOG, DIRECTORIO_LAKEHOUSE_DATA, DIRECTORIO_RAW_DOCS,
};
use lakehouse_rag::application::dtos::tools::ToolCall;
use lakehouse_rag::infrastructure::llm::{MockAgentRunner, MockDistiller};
use lakehouse_rag::infrastructure::query_engine::lakehouse_projection::ARCHIVO_INVENTARIO_GLUE;
use serde_json::json;

const CENTRO: &str = "obra-planta-solar-01";

#[tokio::test]
async fn test_crear_centro_genera_estructura_y_manifiesto_v2_inactivo() {
    let entorno = entorno();
    let centro = entorno
        .app
        .crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), Some("Obra A"))
        .await
        .unwrap();
    assert!(!centro.esta_activo(), "un centro nuevo nace inactivo");

    let ruta = entorno.ruta_centro("a");
    for subdirectorio in [
        DIRECTORIO_RAW_DOCS,
        DIRECTORIO_GLUE_CATALOG,
        DIRECTORIO_LAKEHOUSE_DATA,
    ] {
        assert!(ruta.join(subdirectorio).is_dir(), "{subdirectorio}");
    }
    let manifiesto: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ruta.join(ARCHIVO_MANIFIESTO)).unwrap())
            .unwrap();
    assert_eq!(manifiesto["schema_version"], 2);
    assert_eq!(manifiesto["modelo_embeddings"], MODELO_E5);
    assert_eq!(manifiesto["dimension_embeddings"], 1024);
    assert_eq!(manifiesto["centro_id"], CENTRO);
    assert!(
        ruta.join(DIRECTORIO_LAKEHOUSE_DATA)
            .join("03_matriz_operativa.lance")
            .is_dir()
    );

    let registro: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(entorno.config().join("centers_registry.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(registro["centros"][0]["id"], CENTRO);
    assert_eq!(registro["centros"][0]["activo"], false);
}

#[tokio::test]
async fn test_ciclo_activar_ingerir_consultar_y_desactivar() {
    let entorno = entorno();
    let (v3, v4) = entorno.escribir_revisiones();
    entorno
        .app
        .crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();

    let vacio = entorno.app.activar_centro(CENTRO).await.unwrap();
    assert_eq!(vacio.vistas.len(), 6);
    assert!(vacio.vistas.iter().all(|v| v.filas == 0));
    // Las vistas vacías existen: la agregación devuelve 0, no un error
    assert_eq!(
        escalar(
            &sql(
                &entorno.herramientas,
                CENTRO,
                "SELECT COUNT(*) AS n FROM 03_matriz_operativa"
            )
            .await
        ),
        0
    );

    let r3 = entorno.app.ingestar(&v3, CENTRO).await.unwrap();
    assert!(!r3.informe.ya_ingerido);
    assert!(
        r3.proyeccion.is_some(),
        "centro activo → vistas reconstruidas tras la ingesta"
    );
    let r4 = entorno.app.ingestar(&v4, CENTRO).await.unwrap();
    assert_eq!(r4.informe.versiones_derogadas, vec![3]);

    // raw_docs: copia inmutable con el hash en el nombre y su huella .sha256
    let copia = std::path::Path::new(&r3.copia_original);
    assert!(copia.starts_with(entorno.ruta_centro("a").join(DIRECTORIO_RAW_DOCS)));
    assert_eq!(std::fs::read_to_string(copia).unwrap(), CONTENIDO_V3);
    assert!(
        copia
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&r3.informe.sha256)
    );
    let huella = std::fs::read_to_string(format!("{}.sha256", r3.copia_original)).unwrap();
    assert!(huella.starts_with(&r3.informe.sha256) && huella.contains(ARCHIVO_V3));

    // Vistas: todas las revisiones confirmadas, una sola vigente
    let catalogo = sql(
        &entorno.herramientas,
        CENTRO,
        "SELECT COUNT(*) AS n, SUM(CASE WHEN vigente THEN 1 ELSE 0 END) AS v FROM 01_documentos_catalogo",
    )
    .await;
    assert!(!catalogo.is_error, "{}", catalogo.output);
    assert_eq!(catalogo.output["resultado"][0]["n"], 2);
    assert_eq!(catalogo.output["resultado"][0]["v"], 1);

    // Agregación global sobre la tabla completa (problema 6)
    let agregado = sql(
        &entorno.herramientas,
        CENTRO,
        "SELECT rol_responsable, COUNT(*) AS hitos FROM 03_matriz_operativa WHERE vigente = true GROUP BY rol_responsable ORDER BY rol_responsable",
    )
    .await;
    assert!(!agregado.is_error, "{}", agregado.output);
    let filas = agregado.output["resultado"].as_array().unwrap();
    assert_eq!(filas.len(), 2);
    assert_eq!(filas[1]["rol_responsable"], "Jefe de Producción");

    // Matriz: solo la Rev. 04 vigente y el registro en su columna (problema 1)
    let matriz = entorno
        .herramientas
        .ejecutar_herramienta(&ToolCall {
            id: "m".into(),
            name: "query_operational_matrix".into(),
            arguments: json!({ "centro_id": CENTRO, "fase": "custodia" }),
        })
        .await
        .unwrap();
    assert!(!matriz.is_error, "{}", matriz.output);
    let hitos = matriz.output["filas"].as_array().unwrap();
    assert_eq!(hitos.len(), 1, "{}", matriz.output);
    assert_eq!(hitos[0]["registro_codigo"], "FOR-ALM-05");
    assert_eq!(hitos[0]["cita"]["version_num"], 4);

    // Catálogo de la tool sobre la vista v2
    let inspeccion = entorno
        .herramientas
        .ejecutar_herramienta(&ToolCall {
            id: "c".into(),
            name: "inspect_lakehouse_catalog".into(),
            arguments: json!({ "centro_id": CENTRO }),
        })
        .await
        .unwrap();
    assert!(!inspeccion.is_error, "{}", inspeccion.output);
    assert_eq!(inspeccion.output["documentos"][0]["version_num"], 4);

    // glue_catalog exportado con las vistas y las filas reales
    let inventario: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            entorno
                .ruta_centro("a")
                .join(DIRECTORIO_GLUE_CATALOG)
                .join(ARCHIVO_INVENTARIO_GLUE),
        )
        .unwrap(),
    )
    .unwrap();
    let tablas = inventario["tablas"].as_array().unwrap();
    assert_eq!(tablas.len(), 6);
    assert_eq!(
        tablas[0]["vista_datafusion"],
        format!("{CENTRO}_01_documentos_catalogo")
    );
    assert_eq!(tablas[0]["filas_committed"], 2);

    let integridad = entorno.app.integridad(CENTRO).await.unwrap();
    assert!(integridad.es_integro(), "{integridad:?}");

    // Desactivar → cero acceso
    entorno.app.desactivar_centro(CENTRO).await.unwrap();
    let tras_desactivar = sql(
        &entorno.herramientas,
        CENTRO,
        "SELECT COUNT(*) FROM 03_matriz_operativa",
    )
    .await;
    assert!(tras_desactivar.is_error, "{}", tras_desactivar.output);
    assert!(
        entorno
            .app
            .preguntar("¿Quién custodia?", CENTRO, None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_reingesta_del_mismo_original_es_idempotente() {
    let entorno = entorno();
    let (v3, _) = entorno.escribir_revisiones();
    entorno
        .app
        .crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    entorno.app.ingestar(&v3, CENTRO).await.unwrap();
    let repetida = entorno.app.ingestar(&v3, CENTRO).await.unwrap();
    assert!(repetida.informe.ya_ingerido);
    assert!(repetida.proyeccion.is_none());
    let copias = std::fs::read_dir(entorno.ruta_centro("a").join(DIRECTORIO_RAW_DOCS))
        .unwrap()
        .count();
    assert_eq!(copias, 2, "una copia + su .sha256");
}

#[tokio::test]
async fn test_preguntar_registra_herramientas_y_datos_reales() {
    let consulta = "SELECT COUNT(*) AS hitos FROM 03_matriz_operativa WHERE vigente = true";
    let entorno = entorno_con_agente(agente_sql(CENTRO, consulta, "Hay 2 hitos vigentes."));
    let (v3, _) = entorno.escribir_revisiones();
    entorno
        .app
        .crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    entorno.app.ingestar(&v3, CENTRO).await.unwrap();
    entorno.app.activar_centro(CENTRO).await.unwrap();

    let respuesta = entorno
        .app
        .preguntar("¿Cuántos hitos?", CENTRO, Some("haiku"))
        .await
        .unwrap();
    assert_eq!(respuesta.respuesta, "Hay 2 hitos vigentes.");
    assert_eq!(respuesta.modelo, "haiku");
    assert_eq!(respuesta.herramientas.len(), 1);
    let uso = &respuesta.herramientas[0];
    assert_eq!(uso.nombre, "execute_athena_sql");
    assert!(!uso.es_error, "{}", uso.resultado);
    assert_eq!(uso.resultado["resultado"][0]["hitos"], 2);
}

#[tokio::test]
async fn test_un_proceso_nuevo_proyecta_bajo_demanda_los_centros_activos() {
    let entorno = entorno();
    let (v3, _) = entorno.escribir_revisiones();
    entorno
        .app
        .crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    entorno.app.activar_centro(CENTRO).await.unwrap();
    entorno.app.ingestar(&v3, CENTRO).await.unwrap();

    // Proceso nuevo (como la CLI): motores vacíos; `preguntar` reconstruye las vistas
    let consulta = "SELECT COUNT(*) AS n FROM 01_documentos_catalogo";
    let (app, herramientas) = entorno.reabrir(agente_sql(CENTRO, consulta, "1 documento"));
    assert!(
        sql(&herramientas, CENTRO, consulta).await.is_error,
        "aún sin proyectar"
    );
    let respuesta = app
        .preguntar("¿Cuántos documentos?", CENTRO, None)
        .await
        .unwrap();
    assert_eq!(respuesta.herramientas[0].resultado["resultado"][0]["n"], 1);

    // Y la hidratación de arranque (GUI/MCP) proyecta todos los activos
    let (app, herramientas) = entorno.reabrir(Arc::new(MockAgentRunner::new_scripted(Vec::new())));
    let hidratados = app.hidratar_centros_activos().await.unwrap();
    assert_eq!(hidratados.len(), 1);
    assert!(hidratados[0].resultado.is_ok());
    assert_eq!(escalar(&sql(&herramientas, CENTRO, consulta).await), 1);
}

/// Especificación §9.1: dos centros, solo uno activo; las consultas no ven el otro
#[tokio::test]
async fn test_multi_centro_solo_el_activo_es_visible() {
    let entorno = entorno();
    let (v3, v4) = entorno.escribir_revisiones();
    let app = &entorno.app;
    app.crear_centro(
        "obra-a",
        &entorno.ruta_centro_texto("obra_a"),
        Some("Centro Obra A"),
    )
    .await
    .unwrap();
    app.crear_centro(
        "obra-b",
        &entorno.ruta_centro_texto("obra_b"),
        Some("Centro Obra B"),
    )
    .await
    .unwrap();
    app.ingestar(&v3, "obra-a").await.unwrap();
    app.ingestar(&v3, "obra-b").await.unwrap();
    app.ingestar(&v4, "obra-b").await.unwrap();
    app.activar_centro("obra-a").await.unwrap();

    let consulta = "SELECT COUNT(*) AS n FROM 01_documentos_catalogo";
    assert_eq!(
        escalar(&sql(&entorno.herramientas, "obra-a", consulta).await),
        1
    );
    // B está inactivo: ni por su centro_id ni por el nombre cualificado de su vista
    assert!(
        sql(&entorno.herramientas, "obra-b", consulta)
            .await
            .is_error
    );
    let cruzada = sql(
        &entorno.herramientas,
        "obra-a",
        "SELECT COUNT(*) FROM \"obra-b_01_documentos_catalogo\"",
    )
    .await;
    assert!(cruzada.is_error, "{}", cruzada.output);
    assert!(app.preguntar("hola", "obra-b", None).await.is_err());

    // El centro A solo contiene sus propias filas (centro_id)
    let centros = sql(
        &entorno.herramientas,
        "obra-a",
        "SELECT DISTINCT centro_id FROM 03_matriz_operativa",
    )
    .await;
    assert_eq!(
        centros.output["resultado"],
        json!([{ "centro_id": "obra-a" }])
    );

    // Al activar B aparecen sus 2 revisiones; al desactivar A, A desaparece
    app.activar_centro("obra-b").await.unwrap();
    assert_eq!(
        escalar(&sql(&entorno.herramientas, "obra-b", consulta).await),
        2
    );
    app.desactivar_centro("obra-a").await.unwrap();
    assert!(
        sql(&entorno.herramientas, "obra-a", consulta)
            .await
            .is_error
    );
    assert_eq!(
        escalar(&sql(&entorno.herramientas, "obra-b", consulta).await),
        2
    );

    let centros = app.listar_centros().await.unwrap();
    assert_eq!(centros.len(), 2);
    assert!(!centros[0].esta_activo() && centros[1].esta_activo());
}

#[tokio::test]
async fn test_eliminar_centro_no_borra_el_disco_y_se_puede_readoptar() {
    let entorno = entorno();
    let (v3, _) = entorno.escribir_revisiones();
    let ruta = entorno.ruta_centro_texto("a");
    entorno.app.crear_centro(CENTRO, &ruta, None).await.unwrap();
    entorno.app.ingestar(&v3, CENTRO).await.unwrap();
    entorno.app.activar_centro(CENTRO).await.unwrap();

    entorno.app.eliminar_centro(CENTRO).await.unwrap();
    assert!(entorno.app.listar_centros().await.unwrap().is_empty());
    assert!(
        sql(
            &entorno.herramientas,
            CENTRO,
            "SELECT 1 FROM 01_documentos_catalogo"
        )
        .await
        .is_error
    );
    assert!(entorno.ruta_centro("a").join(ARCHIVO_MANIFIESTO).is_file());

    // Volver a crearlo sobre el mismo directorio lo readopta con sus datos
    entorno.app.crear_centro(CENTRO, &ruta, None).await.unwrap();
    entorno.app.activar_centro(CENTRO).await.unwrap();
    assert_eq!(
        escalar(
            &sql(
                &entorno.herramientas,
                CENTRO,
                "SELECT COUNT(*) FROM 01_documentos_catalogo"
            )
            .await
        ),
        1
    );
}

#[tokio::test]
async fn test_ingesta_fallida_no_deja_filas_ni_toca_las_vistas() {
    let entorno = entorno();
    let (v3, v4) = entorno.escribir_revisiones();
    entorno
        .app
        .crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    entorno.app.ingestar(&v3, CENTRO).await.unwrap();
    entorno.app.activar_centro(CENTRO).await.unwrap();

    // Otro proceso con Claude caído intenta ingerir la Rev. 04
    let destilador = MockDistiller::new();
    destilador.programar_error("Claude no responde");
    let (fallida, _) = construir_app(
        &entorno.config(),
        Arc::new(destilador),
        Arc::new(MockAgentRunner::new_scripted(Vec::new())),
        modelo_e5(),
    );
    let error = fallida.ingestar(&v4, CENTRO).await.unwrap_err().to_string();
    assert!(error.contains("Claude no responde"), "{error}");

    assert!(entorno.app.integridad(CENTRO).await.unwrap().es_integro());
    let vigente = sql(
        &entorno.herramientas,
        CENTRO,
        "SELECT version_num FROM 01_documentos_catalogo WHERE vigente = true",
    )
    .await;
    assert_eq!(vigente.output["resultado"], json!([{ "version_num": 3 }]));
    let _ = (ARCHIVO_V3, CONTENIDO_V3);
}

/// Regresión de la puerta E2E: la Rev. 04 trae `DEROGA_A` hacia su propio código (Rev. 03).
/// En el grafo por documento es un bucle; antes tumbaba la proyección tras confirmar la ingesta.
#[tokio::test]
async fn test_autoderogacion_de_la_rev04_no_impide_reconstruir_las_vistas() {
    let raiz = tempfile::tempdir().unwrap();
    let destilador = MockDistiller::new();
    let mut v4 = comun_store_v2::paquete_revision(4, "2026-09-15", "FOR-ALM-05");
    v4.aristas.push(comun_store_v2::arista_destilada(
        "PO-ALM-03 Rev. 03",
        "DEROGA_A",
    ));
    destilador.registrar_respuesta(
        comun_store_v2::ARCHIVO_V3,
        comun_store_v2::paquete_revision(3, "2026-03-01", "FOR-ALM-02"),
    );
    destilador.registrar_respuesta(comun_store_v2::ARCHIVO_V4, v4);
    let (app, herramientas) = construir_app(
        &raiz.path().join("config"),
        Arc::new(destilador),
        Arc::new(MockAgentRunner::new_scripted(Vec::new())),
        modelo_e5(),
    );
    let entrada = raiz.path().join("entrada");
    std::fs::create_dir_all(&entrada).unwrap();
    let ruta_v3 = entrada.join(comun_store_v2::ARCHIVO_V3);
    let ruta_v4 = entrada.join(comun_store_v2::ARCHIVO_V4);
    std::fs::write(&ruta_v3, comun_store_v2::CONTENIDO_V3).unwrap();
    std::fs::write(&ruta_v4, comun_store_v2::CONTENIDO_V4).unwrap();

    let ruta_centro = raiz.path().join("centro").to_string_lossy().to_string();
    app.crear_centro(CENTRO, &ruta_centro, None).await.unwrap();
    app.activar_centro(CENTRO).await.unwrap();
    app.ingestar(&ruta_v3, CENTRO).await.unwrap();
    let r4 = app.ingestar(&ruta_v4, CENTRO).await.unwrap();
    assert!(r4.aviso_proyeccion.is_none(), "{:?}", r4.aviso_proyeccion);
    let proyeccion = r4.proyeccion.expect("vistas reconstruidas");
    assert!(proyeccion.aristas_grafo >= 2, "{proyeccion:?}");
    let deroga = sql(
        &herramientas,
        CENTRO,
        "SELECT COUNT(*) FROM 06_grafo_referencias WHERE vigente = true AND tipo_relacion = 'DEROGA_A'",
    )
    .await;
    assert_eq!(
        escalar(&deroga),
        1,
        "la arista sigue en DataFusion aunque el grafo la omita"
    );
}
