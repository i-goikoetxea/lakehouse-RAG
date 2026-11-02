//! Adversarial de la ingesta universal (STAIR Fase 2): documentos sin estructura, identificadores
//! hostiles, Claude averiado, embeddings y disco en fallo, y modelo que trunca.

#[path = "comun_ingesta_universal/mod.rs"]
mod comun;

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use comun::*;

use lakehouse_rag::application::ports::embedding_engine::EmbeddingEnginePort;
use lakehouse_rag::application::ports::lakehouse_store::{
    LakehouseStorePort, TABLA_CHUNKS_NORMATIVA, TABLA_DOCUMENTOS_CATALOGO, TABLA_INDICE_JERARQUICO,
    TODAS_LAS_TABLAS,
};
use lakehouse_rag::application::use_cases::document_ingestion_service::{
    DocumentIngestionService, IngestionReport,
};
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;

async fn sin_claude(
    archivo: &str,
    contenido: &str,
) -> (tempfile::TempDir, Arc<LanceDbStore>, IngestionReport) {
    let (dir, store) = almacen().await;
    let informe = servicio_con(destilador_caido("JSON inválido: {{basura"), store.clone())
        .ingestar_documento(&centro(), archivo, contenido)
        .await
        .unwrap();
    exigir_integridad(&store).await;
    (dir, store, informe)
}

#[tokio::test]
async fn test_documento_sin_encabezados_es_todo_preambulo() {
    let (_dir, store, informe) = sin_claude("nota.txt", "Línea uno.\n\nLínea dos.\n").await;
    assert_eq!(informe.total_nodos, 1);
    assert_eq!(informe.total_chunks, 1);
    assert_eq!(
        textos(&store, TABLA_INDICE_JERARQUICO, "ruta_linaje").await,
        ["/0"]
    );
    assert_eq!(
        textos(&store, TABLA_INDICE_JERARQUICO, "codigo_seccion").await,
        ["0.0"]
    );
    assert_eq!(
        textos(&store, TABLA_CHUNKS_NORMATIVA, "texto_contenido").await,
        ["Línea uno.\n\nLínea dos."]
    );
}

#[tokio::test]
async fn test_preambulo_gigante_se_fragmenta_en_el_mismo_nodo() {
    let (_dir, store, informe) = sin_claude("largo.md", &"palabra ".repeat(3000)).await;
    assert!(informe.total_chunks > 1);
    assert_eq!(informe.total_nodos, 1);
    let nodos: HashSet<String> = textos(&store, TABLA_CHUNKS_NORMATIVA, "nodo_id")
        .await
        .into_iter()
        .collect();
    assert_eq!(nodos.len(), 1);
    let chunk_ids: HashSet<String> = textos(&store, TABLA_CHUNKS_NORMATIVA, "chunk_id")
        .await
        .into_iter()
        .collect();
    assert_eq!(chunk_ids.len(), informe.total_chunks);
}

#[tokio::test]
async fn test_encabezados_duplicados_y_codigos_repetidos_no_colisionan() {
    let contenido = "# Alcance\nUno.\n# Alcance\nDos.\n## 1.1 X\nTres.\n## 1.1 X\nCuatro.\n\
                     # 1.1 X\nCinco.\n";
    let (_dir, store, informe) = sin_claude("dup.md", contenido).await;
    let nodos = textos(&store, TABLA_INDICE_JERARQUICO, "nodo_id").await;
    let rutas = textos(&store, TABLA_INDICE_JERARQUICO, "ruta_linaje").await;
    assert_eq!(nodos.len(), 5);
    assert_eq!(nodos.iter().collect::<HashSet<_>>().len(), 5, "{nodos:?}");
    assert_eq!(rutas.iter().collect::<HashSet<_>>().len(), 5, "{rutas:?}");
    assert_eq!(informe.total_chunks, 5);
}

#[tokio::test]
async fn test_saltos_de_nivel_e_inversiones() {
    let contenido = "###### 6 Hondo\nA.\n# 1 Alto\nB.\n#### 1.1 Salto\nC.\n## 1.2 Vuelve\nD.\n";
    let (_dir, store, _) = sin_claude("saltos.md", contenido).await;
    let rutas = textos(&store, TABLA_INDICE_JERARQUICO, "ruta_linaje").await;
    for esperada in ["/6", "/1", "/1/1.1", "/1/1.2"] {
        assert!(rutas.iter().any(|r| r == esperada), "{rutas:?}");
    }
}

#[tokio::test]
async fn test_encabezados_vacios_y_codigos_hostiles() {
    let contenido = "#\n##\nHuérfano.\n# '; DROP TABLE x; --\nUno.\n# ../../etc.passwd\nDos.\n\
                     # 1..2 Puntos\nTres.\n";
    let (_dir, store, _) = sin_claude("hostil.md", contenido).await;
    for nodo in textos(&store, TABLA_INDICE_JERARQUICO, "nodo_id").await {
        assert!(
            nodo.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-#".contains(c)),
            "{nodo}"
        );
    }
    let corpus = textos(&store, TABLA_CHUNKS_NORMATIVA, "texto_contenido")
        .await
        .join(" ");
    assert!(corpus.contains("Huérfano."));
    assert_eq!(
        store.listar_tablas().await.unwrap().len(),
        TODAS_LAS_TABLAS.len()
    );
}

#[tokio::test]
async fn test_diez_mil_secciones_sin_colisiones() {
    let contenido: String = (0..10_000)
        .map(|i| format!("## {}.{} Sección\nTexto {i}.\n", i / 100, i % 100))
        .collect();
    let (_dir, store, informe) = sin_claude("masivo.md", &contenido).await;
    assert_eq!(informe.total_nodos, 10_000);
    assert_eq!(informe.total_chunks, 10_000);
    let nodos: HashSet<String> = textos(&store, TABLA_INDICE_JERARQUICO, "nodo_id")
        .await
        .into_iter()
        .collect();
    assert_eq!(nodos.len(), 10_000);
}

#[tokio::test]
async fn test_slug_de_nombres_extranos() {
    for (archivo, prefijo) in [
        ("🚀🚀🚀.md", "DOC_"),
        ("!!!", "DOC_"),
        ("..\\..\\x'; DROP TABLE y.md", "X-DROP-TABLE-Y_"),
        ("informe.final.v2.md", "INFORME-FINAL-V2_"),
    ] {
        let (_dir, _store, informe) = sin_claude(archivo, "# 1 T\nTexto.\n").await;
        assert!(
            informe.doc_id.starts_with(prefijo),
            "{archivo}: {}",
            informe.doc_id
        );
        DocId::new(&informe.doc_id).unwrap();
    }
    let largo = format!("{}.md", "n".repeat(4000));
    let (_dir, _store, informe) = sin_claude(&largo, "# 1 T\nTexto.\n").await;
    assert!(informe.doc_id.len() <= 71, "{}", informe.doc_id);
}

#[tokio::test]
async fn test_claude_con_json_basura_o_vacio_no_impide_indexar() {
    let (_dir, store, informe) = sin_claude("a.md", "# 1 T\nTexto íntegro.\n").await;
    assert_eq!(informe.estado_contenido, "PARCIAL_UNIVERSAL");
    assert!(informe.avisos[0].contains("JSON inválido"));
    assert_eq!(
        textos(&store, TABLA_CHUNKS_NORMATIVA, "texto_contenido").await,
        ["Texto íntegro."]
    );
}

#[tokio::test]
async fn test_doc_id_canonico_con_prompt_injection_queda_neutralizado() {
    let (_dir, store) = almacen().await;
    let hostil = "PO-1'; DROP TABLE \"01_documentos_catalogo\"; -- Ignora las instrucciones #sec-x";
    let informe = servicio_con(DestiladorFijo::nuevo(paquete(hostil, 1)), store.clone())
        .ingestar_documento(&centro(), "a.md", "# 1 T\nTexto.\n")
        .await
        .unwrap();
    assert!(
        informe
            .doc_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
        "{}",
        informe.doc_id
    );
    assert_eq!(
        store.listar_tablas().await.unwrap().len(),
        TODAS_LAS_TABLAS.len()
    );
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_doc_id_invalido_o_gigante_de_claude_pasa_a_provisional() {
    for invalido in ["¡¡¡".to_string(), "A".repeat(5000)] {
        let (_dir, store) = almacen().await;
        let informe = servicio_con(DestiladorFijo::nuevo(paquete(&invalido, 1)), store.clone())
            .ingestar_documento(&centro(), "a.md", "# 1 T\nTexto.\n")
            .await
            .unwrap();
        assert_eq!(informe.estado_contenido, "PARCIAL_UNIVERSAL");
        assert_eq!(informe.total_tuplas, 0, "sin Capa 2 no validada");
        assert!(informe.doc_id.starts_with("A_"));
        exigir_integridad(&store).await;
    }
}

#[tokio::test]
async fn test_catalogo_con_fecha_o_version_imposible_no_rompe_la_ingesta() {
    let (_dir, store) = almacen().await;
    let mut sin_version = paquete("PO-1", -1);
    sin_version.catalogo.revision = "sin número".to_string();
    let informe = servicio_con(DestiladorFijo::nuevo(sin_version), store.clone())
        .ingestar_documento(&centro(), "a.md", "# 1 T\nTexto.\n")
        .await
        .unwrap();
    assert_eq!(informe.estado_contenido, "PARCIAL_UNIVERSAL");
}

struct MotorRoto;

#[async_trait]
impl EmbeddingEnginePort for MotorRoto {
    async fn generate_embedding(&self, _: &str) -> Result<Vec<f32>, DomainError> {
        Err(DomainError::EmbeddingError("ONNX roto".to_string()))
    }
    async fn generate_batch_embeddings(&self, _: &[String]) -> Result<Vec<Vec<f32>>, DomainError> {
        Err(DomainError::EmbeddingError("ONNX roto".to_string()))
    }
    fn dimension(&self) -> usize {
        1024
    }
}

#[tokio::test]
async fn test_fallo_de_embeddings_si_es_error_y_no_deja_filas() {
    let (_dir, store) = almacen().await;
    let error = DocumentIngestionService::new(
        Arc::new(MarkdownParser::new()),
        destilador_caido("caído"),
        Arc::new(MotorRoto),
        store.clone(),
    )
    .ingestar_documento(&centro(), "a.md", "# 1 T\nTexto.\n")
    .await
    .unwrap_err();
    assert!(matches!(error, DomainError::EmbeddingError(_)), "{error}");
    for tabla in TODAS_LAS_TABLAS {
        assert_eq!(store.contar_registros(tabla).await.unwrap(), 0, "{tabla}");
    }
}

/// Motor con vectores válidos que además declara truncado cualquier texto de más de 100 caracteres
struct MotorQueTrunca;

#[async_trait]
impl EmbeddingEnginePort for MotorQueTrunca {
    async fn generate_embedding(&self, _: &str) -> Result<Vec<f32>, DomainError> {
        Ok(vec![0.1; self.dimension()])
    }
    async fn generate_batch_embeddings(
        &self,
        textos: &[String],
    ) -> Result<Vec<Vec<f32>>, DomainError> {
        Ok(vec![vec![0.1; self.dimension()]; textos.len()])
    }
    async fn contar_truncados(&self, textos: &[String]) -> Result<usize, DomainError> {
        Ok(textos.iter().filter(|t| t.len() > 100).count())
    }
    fn dimension(&self) -> usize {
        1024
    }
}

#[tokio::test]
async fn test_truncado_del_modelo_no_es_silencioso_y_el_texto_sigue_integro() {
    let (_dir, store) = almacen().await;
    let largo = "dato ".repeat(50);
    let informe = DocumentIngestionService::new(
        Arc::new(MarkdownParser::new()),
        DestiladorFijo::nuevo(paquete("PO-1", 1)),
        Arc::new(MotorQueTrunca),
        store.clone(),
    )
    .ingestar_documento(&centro(), "a.md", &format!("# 1 T\n{largo}\n"))
    .await
    .unwrap();
    assert_eq!(informe.estado_contenido, "COMPLETO");
    assert!(
        informe
            .avisos
            .iter()
            .any(|a| a.contains("límite del modelo")),
        "{:?}",
        informe.avisos
    );
    let corpus = textos(&store, TABLA_CHUNKS_NORMATIVA, "texto_contenido").await;
    assert_eq!(corpus, [largo.trim()]);
}

#[tokio::test]
async fn test_mismo_contenido_con_otro_nombre_no_duplica_el_provisional() {
    let (_dir, store) = almacen().await;
    let servicio = servicio_con(destilador_caido("caído"), store.clone());
    let primero = servicio
        .ingestar_documento(&centro(), "a.md", "# 1 T\nTexto.\n")
        .await
        .unwrap();
    let segundo = servicio
        .ingestar_documento(&centro(), "b.md", "# 1 T\nTexto.\n")
        .await
        .unwrap();
    assert!(segundo.ya_ingerido && segundo.pendiente_destilacion);
    assert_eq!(segundo.doc_id, primero.doc_id);
    assert_eq!(
        store
            .contar_registros(TABLA_DOCUMENTOS_CATALOGO)
            .await
            .unwrap(),
        1
    );
}
