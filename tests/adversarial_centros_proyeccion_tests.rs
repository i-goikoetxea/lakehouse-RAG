//! Suite adversarial de la Fase 12: centros en disco y proyección.
//! Rutas con `..`, registro corrupto, centro inexistente, activación doble, manifiesto de
//! otro modelo, `.lance` borrado a mano y otros abusos del registro y de los nombres.

mod comun_centros;
mod comun_store_v2;

use std::sync::Arc;

use comun_centros::*;
use lakehouse_rag::application::dtos::centros::{
    ARCHIVO_MANIFIESTO, DIRECTORIO_LAKEHOUSE_DATA, DIRECTORIO_RAW_DOCS, ModeloEmbeddingsActivo,
};
use lakehouse_rag::infrastructure::llm::MockAgentRunner;

const CENTRO: &str = "obra-a";

#[tokio::test]
async fn test_rutas_con_directorio_padre_se_rechazan_sin_tocar_el_registro() {
    let entorno = entorno();
    let base = entorno.ruta_centro_texto("a");
    for ruta in [
        format!("{base}/../escape"),
        "../fuera_del_proyecto".to_string(),
        format!("{base}\\..\\..\\windows"),
        String::new(),
        "   ".to_string(),
        "con\0nulo".to_string(),
    ] {
        let error = entorno.app.crear_centro(CENTRO, &ruta, None).await;
        assert!(error.is_err(), "ruta aceptada: {ruta:?}");
    }
    assert!(entorno.app.listar_centros().await.unwrap().is_empty());
    assert!(!entorno.ruta_centro("escape").exists());
}

#[tokio::test]
async fn test_identificadores_de_centro_maliciosos_se_rechazan() {
    let entorno = entorno();
    for id in ["../a", "a/b", "a;DROP", "a'b", "", "a\"b", "a b"] {
        assert!(
            entorno
                .app
                .crear_centro(id, &entorno.ruta_centro_texto("x"), None)
                .await
                .is_err(),
            "id aceptado: {id:?}"
        );
        assert!(entorno.app.activar_centro(id).await.is_err());
    }
}

#[tokio::test]
async fn test_registro_corrupto_bloquea_operaciones_sin_sobrescribirlo() {
    let entorno = entorno();
    std::fs::create_dir_all(entorno.config()).unwrap();
    let ruta_registro = entorno.config().join("centers_registry.json");
    for corrupto in [
        "{ no es json",
        "{\"centros\": 5}",
        "{\"centros\": [{\"id\": \"../x\", \"nombre\": \"x\", \"ruta_disco\": \"x\", \"activo\": true, \"fecha_creacion\": \"2026-10-17T00:00:00Z\"}]}",
        "{\"centros\": [{\"id\": \"a\"}]}",
    ] {
        std::fs::write(&ruta_registro, corrupto).unwrap();
        let error = entorno.app.listar_centros().await.unwrap_err().to_string();
        assert!(error.contains("corrupto"), "{error}");
        assert!(
            entorno
                .app
                .crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
                .await
                .is_err()
        );
        assert!(entorno.app.hidratar_centros_activos().await.is_err());
        assert_eq!(std::fs::read_to_string(&ruta_registro).unwrap(), corrupto);
    }
}

#[tokio::test]
async fn test_centro_inexistente_da_error_claro_en_todas_las_operaciones() {
    let entorno = entorno();
    let (v3, _) = entorno.escribir_revisiones();
    let app = &entorno.app;
    let errores = [
        app.activar_centro("fantasma")
            .await
            .err()
            .map(|e| e.to_string()),
        app.desactivar_centro("fantasma")
            .await
            .err()
            .map(|e| e.to_string()),
        app.eliminar_centro("fantasma")
            .await
            .err()
            .map(|e| e.to_string()),
        app.ingestar(&v3, "fantasma")
            .await
            .err()
            .map(|e| e.to_string()),
        app.preguntar("hola", "fantasma", None)
            .await
            .err()
            .map(|e| e.to_string()),
        app.integridad("fantasma")
            .await
            .err()
            .map(|e| e.to_string()),
    ];
    for error in errores {
        let error = error.expect("debe fallar");
        assert!(error.contains("no está registrado"), "{error}");
    }
}

#[tokio::test]
async fn test_activacion_doble_es_idempotente_y_no_duplica_vistas_ni_grafo() {
    let entorno = entorno();
    let (v3, v4) = entorno.escribir_revisiones();
    let app = &entorno.app;
    app.crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    app.ingestar(&v3, CENTRO).await.unwrap();
    app.ingestar(&v4, CENTRO).await.unwrap();

    let primera = app.activar_centro(CENTRO).await.unwrap();
    let segunda = app.activar_centro(CENTRO).await.unwrap();
    assert_eq!(primera, segunda);
    assert!(
        primera.aristas_grafo > 0 && primera.celdas_lookup > 0,
        "{primera:?}"
    );
    assert_eq!(
        escalar(
            &sql(
                &entorno.herramientas,
                CENTRO,
                "SELECT COUNT(*) FROM 01_documentos_catalogo"
            )
            .await
        ),
        2
    );
    // Concurrencia: activaciones simultáneas no corrompen el registro ni las vistas
    let (a, b) = tokio::join!(app.activar_centro(CENTRO), app.activar_centro(CENTRO));
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(app.listar_centros().await.unwrap().len(), 1);

    // Doble desactivación tampoco falla
    app.desactivar_centro(CENTRO).await.unwrap();
    app.desactivar_centro(CENTRO).await.unwrap();
}

#[tokio::test]
async fn test_alta_duplicada_por_id_o_por_ruta_se_rechaza() {
    let entorno = entorno();
    let ruta = entorno.ruta_centro_texto("a");
    entorno.app.crear_centro(CENTRO, &ruta, None).await.unwrap();
    assert!(
        entorno
            .app
            .crear_centro(CENTRO, &entorno.ruta_centro_texto("b"), None)
            .await
            .is_err()
    );
    let misma_ruta = entorno
        .app
        .crear_centro("obra-b", &ruta.to_uppercase(), None)
        .await;
    assert!(misma_ruta.is_err(), "dos centros no comparten directorio");
    assert_eq!(entorno.app.listar_centros().await.unwrap().len(), 1);
}

#[tokio::test]
async fn test_directorio_ajeno_no_vacio_o_archivo_se_rechazan() {
    let entorno = entorno();
    let ajeno = entorno.ruta_centro("documentos_personales");
    std::fs::create_dir_all(&ajeno).unwrap();
    std::fs::write(ajeno.join("nomina.xlsx"), "privado").unwrap();
    let error = entorno
        .app
        .crear_centro(CENTRO, &ajeno.to_string_lossy(), None)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("no está vacío"), "{error}");
    assert!(
        !ajeno.join(DIRECTORIO_RAW_DOCS).exists(),
        "no se crea nada dentro"
    );

    let archivo = entorno.raiz.path().join("soy_un_archivo");
    std::fs::write(&archivo, "x").unwrap();
    assert!(
        entorno
            .app
            .crear_centro(CENTRO, &archivo.to_string_lossy(), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn test_manifiesto_de_otro_modelo_impide_abrir_e_ingerir() {
    let entorno = entorno();
    let (v3, _) = entorno.escribir_revisiones();
    entorno
        .app
        .crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();

    // Una app con BGE no puede abrir ni ingerir un centro E5 (vectores incomparables)
    let (bge, _) = construir_app(
        &entorno.config(),
        comun_store_v2::distiller_revisiones(),
        Arc::new(MockAgentRunner::new_scripted(Vec::new())),
        ModeloEmbeddingsActivo::new("BGELargeENV15", 1024),
    );
    for error in [
        bge.activar_centro(CENTRO).await.unwrap_err().to_string(),
        bge.ingestar(&v3, CENTRO).await.unwrap_err().to_string(),
        bge.integridad(CENTRO).await.unwrap_err().to_string(),
    ] {
        assert!(
            error.contains("BGELargeENV15") && error.contains(MODELO_E5),
            "{error}"
        );
    }
    assert!(
        !entorno.app.listar_centros().await.unwrap()[0].esta_activo(),
        "un intento fallido no activa el centro"
    );

    // Manifiesto manipulado en disco: dimensión y versión de esquema distintas
    let ruta_manifiesto = entorno.ruta_centro("a").join(ARCHIVO_MANIFIESTO);
    let original = std::fs::read_to_string(&ruta_manifiesto).unwrap();
    for (de, a) in [
        (
            "\"dimension_embeddings\": 1024",
            "\"dimension_embeddings\": 384",
        ),
        ("\"schema_version\": 2", "\"schema_version\": 1"),
    ] {
        std::fs::write(&ruta_manifiesto, original.replace(de, a)).unwrap();
        assert!(entorno.app.activar_centro(CENTRO).await.is_err(), "{a}");
    }
    std::fs::write(&ruta_manifiesto, "{ corrupto").unwrap();
    let error = entorno
        .app
        .activar_centro(CENTRO)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("corrupto"), "{error}");
    std::fs::remove_file(&ruta_manifiesto).unwrap();
    let error = entorno
        .app
        .activar_centro(CENTRO)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(ARCHIVO_MANIFIESTO), "{error}");
}

#[tokio::test]
async fn test_lance_borrado_a_mano_se_detecta_sin_romper_las_vistas_vigentes() {
    let entorno = entorno();
    let (v3, v4) = entorno.escribir_revisiones();
    let app = &entorno.app;
    app.crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    app.ingestar(&v3, CENTRO).await.unwrap();
    app.activar_centro(CENTRO).await.unwrap();

    let tabla = entorno
        .ruta_centro("a")
        .join(DIRECTORIO_LAKEHOUSE_DATA)
        .join("03_matriz_operativa.lance");
    std::fs::remove_dir_all(&tabla).unwrap();

    let integridad = app.integridad(CENTRO).await.unwrap();
    assert!(!integridad.es_integro());
    assert_eq!(
        integridad.tablas_ausentes,
        vec!["03_matriz_operativa".to_string()]
    );

    // La reactivación falla nombrando la tabla y deja intactas las vistas anteriores
    let error = app.activar_centro(CENTRO).await.unwrap_err().to_string();
    assert!(
        error.contains("03_matriz_operativa") && error.contains("--integridad"),
        "{error}"
    );
    assert_eq!(
        escalar(
            &sql(
                &entorno.herramientas,
                CENTRO,
                "SELECT COUNT(*) FROM 03_matriz_operativa"
            )
            .await
        ),
        2
    );

    // La ingesta sobre el centro dañado falla sin dejar filas STAGED en las demás tablas
    assert!(app.ingestar(&v4, CENTRO).await.is_err());
    let integridad = app.integridad(CENTRO).await.unwrap();
    assert!(
        integridad.informe.ingestas_staged.is_empty(),
        "{integridad:?}"
    );
}

#[tokio::test]
async fn test_nombre_de_archivo_hostil_queda_neutralizado_en_raw_docs() {
    let entorno = entorno();
    let app = &entorno.app;
    app.crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    let hostil = entorno.raiz.path().join("..rev 03;rm -rf $HOME.md");
    std::fs::write(&hostil, comun_store_v2::CONTENIDO_V3).unwrap();
    // El destilador simulado no conoce este nombre: la ingesta puede fallar, pero la copia
    // inmutable se escribe antes y debe quedar dentro de raw_docs/ con un nombre seguro
    let _ = app.ingestar(&hostil, CENTRO).await;
    let raw_docs = entorno.ruta_centro("a").join(DIRECTORIO_RAW_DOCS);
    let nombres: Vec<String> = std::fs::read_dir(&raw_docs)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(nombres.len(), 2, "{nombres:?}");
    for nombre in &nombres {
        assert!(
            nombre
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
            "{nombre}"
        );
    }

    // Directorios, binarios y archivos inexistentes se rechazan antes de llamar a Claude
    let binario = entorno.raiz.path().join("binario.md");
    std::fs::write(&binario, [0xff, 0xfe, 0x00, 0x81]).unwrap();
    for ruta in [
        entorno.raiz.path().to_path_buf(),
        binario,
        entorno.raiz.path().join("no_existe.md"),
    ] {
        assert!(
            app.ingestar(&ruta, CENTRO).await.is_err(),
            "{}",
            ruta.display()
        );
    }
}

#[tokio::test]
async fn test_sql_hostil_sobre_las_vistas_reconstruidas_no_escapa_del_centro() {
    let entorno = entorno();
    let (v3, _) = entorno.escribir_revisiones();
    let app = &entorno.app;
    app.crear_centro(CENTRO, &entorno.ruta_centro_texto("a"), None)
        .await
        .unwrap();
    app.ingestar(&v3, CENTRO).await.unwrap();
    app.activar_centro(CENTRO).await.unwrap();

    for hostil in [
        "DROP TABLE 03_matriz_operativa",
        "SELECT * FROM 03_matriz_operativa; DROP TABLE 01_documentos_catalogo",
        "INSERT INTO 01_documentos_catalogo SELECT * FROM 01_documentos_catalogo",
        "CREATE TABLE x AS SELECT 1",
    ] {
        assert!(
            sql(&entorno.herramientas, CENTRO, hostil).await.is_error,
            "{hostil}"
        );
    }
    // centro_id inyectado en la herramienta
    assert!(
        entorno
            .herramientas
            .ejecutar_herramienta(&lakehouse_rag::application::dtos::tools::ToolCall {
                id: "x".into(),
                name: "execute_athena_sql".into(),
                arguments: serde_json::json!({
                    "centro_id": "obra-a\" OR 1=1 --",
                    "sql_query": "SELECT 1"
                }),
            })
            .await
            .unwrap()
            .is_error
    );
    assert_eq!(
        escalar(
            &sql(
                &entorno.herramientas,
                CENTRO,
                "SELECT COUNT(*) FROM 01_documentos_catalogo"
            )
            .await
        ),
        1,
        "las vistas siguen intactas"
    );
}
