//! Pruebas de integración para el pipeline de ingesta atómica hacia LanceDB

use std::sync::Arc;

use lakehouse_rag::application::dtos::ingestion::{
    DistilledCatalog, DistilledCell, DistilledDocumentPackage, DistilledDoubleEntryTable,
    DistilledOperationalTuple, DistilledReferenceEdge,
};
use lakehouse_rag::application::ports::lakehouse_store::{
    LakehouseStorePort, TABLA_CATALOGO_TABLAS, TABLA_CELDAS_DOBLE_ENTRADA, TABLA_CHUNKS_NORMATIVA,
    TABLA_DOCUMENTOS_CATALOGO, TABLA_GRAFO_REFERENCIAS, TABLA_MATRIZ_OPERATIVA,
};
use lakehouse_rag::application::use_cases::document_ingestion_service::DocumentIngestionService;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::llm::mock_distiller::MockDistiller;
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::schemas::DIMENSION_EMBEDDINGS;

/// Cuenta las filas físicas (cualquier estado) de una tabla que cumplen el filtro
async fn contar_filas(store: &LanceDbStore, tabla: &str, filtro: Option<&str>) -> usize {
    store
        .leer_filas_crudas(tabla, filtro)
        .await
        .expect("Leer filas crudas")
        .iter()
        .map(|lote| lote.num_rows())
        .sum()
}

#[tokio::test]
async fn test_integration_ingesta_e2e_persiste_en_las_6_tablas() {
    let temp_dir = tempfile::tempdir().expect("Crear tempdir");
    let store = LanceDbStore::conectar(temp_dir.path())
        .await
        .expect("Conectar store");
    store
        .inicializar_tablas()
        .await
        .expect("Inicializar 6 tablas");

    let parser = Arc::new(MarkdownParser::new());
    let distiller = Arc::new(MockDistiller::new());
    let embedding_engine = Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )));
    let store_arc = Arc::new(store);

    // Documento fixture industrial completo
    let doc_md = r#"
# Procedimiento de Mantenimiento de Turbogeneradores

## 1. Alcance y Seguridad Operativa
Este procedimiento rige para todas las cuadrillas de electromecánica en parada de planta.

### 1.1 Recepción de Componentes
El supervisor debe verificar los precintos de calibración en fábrica antes del montaje.

### 1.2 Inspección de Alineación Axial
Medir la desviación axial con reloj comparador micrométrico.

**Tabla de Tolerancias de Desalineación Dinámica**
| Rango RPM | Tolerancia Máxima (um) | Categoría Crítica |
| --- | --- | --- |
| 0.0 - 1500.0 | 50.0 um | Clase I |
| 1500.1 - 3600.0 | 25.0 um | Clase II |
*Nota: Lecturas compensadas a 20 grados Celsius.*

Este procedimiento complementa a la norma NOM-STPS-001 en materia de protección laboral.
"#;

    let paquete = DistilledDocumentPackage {
        catalogo: DistilledCatalog {
            doc_id: "PROC-TURBO-001".to_string(),
            titulo: "Procedimiento de Mantenimiento de Turbogeneradores".to_string(),
            tipo_documental: "PROCEDIMIENTO_TECNICO".to_string(),
            emisor: "INGENIERIA_ELECTROMECANICA".to_string(),
            fecha_emision: "2026-03-01".to_string(),
            nivel_prelacion: 2,
            version: 1,
        },
        tuplas_operativas: vec![
            DistilledOperationalTuple {
                seccion: "1.1".to_string(),
                fase_operativa: "RECEPCION".to_string(),
                rol_responsable: "SUPERVISOR".to_string(),
                accion: "Verificar precintos de calibración".to_string(),
                parametro_control: "Integridad de precinto".to_string(),
                tolerancia_criterio: "100% intacto".to_string(),
                condicion_aplicacion: "Previo al montaje".to_string(),
                referencia_normativa: "PROC-TURBO-001".to_string(),
            },
            DistilledOperationalTuple {
                seccion: "1.2".to_string(),
                fase_operativa: "INSPECCION".to_string(),
                rol_responsable: "TECNICO_MECANICO".to_string(),
                accion: "Medir desviación axial".to_string(),
                parametro_control: "Alineación axial".to_string(),
                tolerancia_criterio: "Según tabla RPM".to_string(),
                condicion_aplicacion: "Eje rotando en vacío".to_string(),
                referencia_normativa: "PROC-TURBO-001".to_string(),
            },
        ],
        tablas_doble_entrada: vec![DistilledDoubleEntryTable {
            nombre_tabla: "Tabla de Tolerancias de Desalineación Dinámica".to_string(),
            coordenada_x_nombre: "Rango RPM".to_string(),
            coordenada_y_nombre: "Categoría Crítica".to_string(),
            nota_pie: Some("Lecturas compensadas a 20 grados Celsius.".to_string()),
            celdas: vec![
                DistilledCell {
                    rango_x_min: Some(0.0),
                    rango_x_max: Some(1500.0),
                    x_inclusivo_min: true,
                    x_inclusivo_max: true,
                    categoria_x: None,
                    rango_y_min: None,
                    rango_y_max: None,
                    y_inclusivo_min: false,
                    y_inclusivo_max: false,
                    categoria_y: Some("Clase I".to_string()),
                    valor_resultado: "50.0".to_string(),
                    unidad_medida: "um".to_string(),
                    notas_especificas: None,
                },
                DistilledCell {
                    rango_x_min: Some(1500.1),
                    rango_x_max: Some(3600.0),
                    x_inclusivo_min: true,
                    x_inclusivo_max: true,
                    categoria_x: None,
                    rango_y_min: None,
                    rango_y_max: None,
                    y_inclusivo_min: false,
                    y_inclusivo_max: false,
                    categoria_y: Some("Clase II".to_string()),
                    valor_resultado: "25.0".to_string(),
                    unidad_medida: "um".to_string(),
                    notas_especificas: None,
                },
            ],
        }],
        aristas_referencia: vec![DistilledReferenceEdge {
            doc_destino_id: "NOM-STPS-001".to_string(),
            tipo_relacion: "COMPLEMENTA".to_string(),
            peso_prelacion: 0.75,
            contexto_cita: Some("Protección laboral complementaria".to_string()),
        }],
    };

    distiller.registrar_respuesta("proc_turbos.md", paquete);

    let service =
        DocumentIngestionService::new(parser, distiller, embedding_engine, store_arc.clone());

    let centro = CenterId::new("planta-hidro").unwrap();
    let reporte = service
        .ingestar_documento(&centro, "proc_turbos.md", doc_md)
        .await
        .expect("Ingesta E2E exitosa");

    assert_eq!(reporte.doc_id, "PROC-TURBO-001");
    assert_eq!(reporte.centro_id, "planta-hidro");
    assert_eq!(reporte.total_tuplas, 2);
    assert_eq!(reporte.total_tablas, 1);
    assert_eq!(reporte.total_celdas, 2);
    assert_eq!(reporte.total_aristas, 1);
    assert!(reporte.total_chunks >= 3);

    // Verificar en LanceDB cada una de las 6 tablas
    let count_cat = store_arc
        .contar_registros(TABLA_DOCUMENTOS_CATALOGO)
        .await
        .unwrap();
    assert_eq!(count_cat, 1);

    let count_chk = store_arc
        .contar_registros(TABLA_CHUNKS_NORMATIVA)
        .await
        .unwrap();
    assert_eq!(count_chk, reporte.total_chunks);

    let count_mat = store_arc
        .contar_registros(TABLA_MATRIZ_OPERATIVA)
        .await
        .unwrap();
    assert_eq!(count_mat, 2);

    let count_tbl = store_arc
        .contar_registros(TABLA_CATALOGO_TABLAS)
        .await
        .unwrap();
    assert_eq!(count_tbl, 1);

    let count_cld = store_arc
        .contar_registros(TABLA_CELDAS_DOBLE_ENTRADA)
        .await
        .unwrap();
    assert_eq!(count_cld, 2);

    let count_grf = store_arc
        .contar_registros(TABLA_GRAFO_REFERENCIAS)
        .await
        .unwrap();
    assert_eq!(count_grf, 1);
}

#[tokio::test]
async fn test_integration_ingesta_control_de_versiones_invalida_previa() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();
    store.inicializar_tablas().await.unwrap();

    let parser = Arc::new(MarkdownParser::new());
    let distiller = Arc::new(MockDistiller::new());
    let embedding_engine = Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )));
    let store_arc = Arc::new(store);

    let service = DocumentIngestionService::new(
        parser,
        distiller.clone(),
        embedding_engine,
        store_arc.clone(),
    );

    let centro = CenterId::new("planta-gas").unwrap();

    // 1. Ingestar Versión 1
    let doc_v1 = "# Norma de Presión V1\n## 1. Alcance\nPresión máxima 10 bar.";
    let mut paquete_v1 = DistilledDocumentPackage {
        catalogo: DistilledCatalog {
            doc_id: "NORMA-PRESION".to_string(),
            titulo: "Norma de Presión".to_string(),
            tipo_documental: "NORMA".to_string(),
            emisor: "OPERACIONES".to_string(),
            fecha_emision: "2025-01-01".to_string(),
            nivel_prelacion: 1,
            version: 1,
        },
        tuplas_operativas: Vec::new(),
        tablas_doble_entrada: Vec::new(),
        aristas_referencia: vec![DistilledReferenceEdge {
            doc_destino_id: "NOM-STPS-001".to_string(),
            tipo_relacion: "SUBORDINADO_A_NORMA".to_string(),
            peso_prelacion: 1.0,
            contexto_cita: None,
        }],
    };
    distiller.registrar_respuesta("norma_v1.md", paquete_v1.clone());

    service
        .ingestar_documento(&centro, "norma_v1.md", doc_v1)
        .await
        .unwrap();

    // 2. Ingestar Versión 2 del mismo documento
    let doc_v2 = "# Norma de Presión V2\n## 1. Alcance Actualizado\nPresión máxima 12 bar.";
    paquete_v1.catalogo.version = 2;
    paquete_v1.catalogo.fecha_emision = "2026-01-01".to_string();
    distiller.registrar_respuesta("norma_v2.md", paquete_v1);

    service
        .ingestar_documento(&centro, "norma_v2.md", doc_v2)
        .await
        .unwrap();

    // 3. Verificar que solo hay 1 registro vigente para NORMA-PRESION en el catálogo y en el grafo
    let filtro_vigentes = "doc_id = 'NORMA-PRESION' AND vigente = true";
    let vigentes_catalogo =
        contar_filas(&store_arc, TABLA_DOCUMENTOS_CATALOGO, Some(filtro_vigentes)).await;
    assert_eq!(
        vigentes_catalogo, 1,
        "Solo la versión más reciente debe figurar como vigente"
    );

    let vigentes_grafo =
        contar_filas(&store_arc, TABLA_GRAFO_REFERENCIAS, Some(filtro_vigentes)).await;
    assert_eq!(
        vigentes_grafo, 1,
        "La revisión nueva también retira la vigencia de las aristas previas"
    );

    let total_historico = contar_filas(&store_arc, TABLA_DOCUMENTOS_CATALOGO, None).await;
    assert_eq!(
        total_historico, 2,
        "Debe conservarse el registro histórico inmutable"
    );
    assert_eq!(
        contar_filas(&store_arc, TABLA_GRAFO_REFERENCIAS, None).await,
        2,
        "Debe conservarse el histórico de aristas"
    );
}

#[tokio::test]
async fn test_integration_ingesta_coexistencia_multi_centro() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = LanceDbStore::conectar(temp_dir.path()).await.unwrap();
    store.inicializar_tablas().await.unwrap();

    let parser = Arc::new(MarkdownParser::new());
    let distiller = Arc::new(MockDistiller::new());
    let embedding_engine = Arc::new(MockEmbeddingEngine::new(Some(
        DIMENSION_EMBEDDINGS as usize,
    )));
    let store_arc = Arc::new(store);

    let service =
        DocumentIngestionService::new(parser, distiller, embedding_engine, store_arc.clone());

    let centro_alfa = CenterId::new("planta-alfa").unwrap();
    let centro_beta = CenterId::new("planta-beta").unwrap();

    let doc_alfa = "# Protocolo Planta Alfa\n## 1. Reglas\nReglas exclusivas de Alfa.";
    let doc_beta = "# Protocolo Planta Beta\n## 1. Reglas\nReglas exclusivas de Beta.";

    service
        .ingestar_documento(&centro_alfa, "alfa.md", doc_alfa)
        .await
        .unwrap();
    service
        .ingestar_documento(&centro_beta, "beta.md", doc_beta)
        .await
        .unwrap();

    // Verificar en tabla de chunks el conteo filtrado por centro
    let rows_alfa = contar_filas(
        &store_arc,
        TABLA_CHUNKS_NORMATIVA,
        Some("centro_id = 'planta-alfa'"),
    )
    .await;
    let rows_beta = contar_filas(
        &store_arc,
        TABLA_CHUNKS_NORMATIVA,
        Some("centro_id = 'planta-beta'"),
    )
    .await;

    assert!(rows_alfa >= 1);
    assert!(rows_beta >= 1);
}
