//! Fixtures compartidas por las suites del almacén v2 (integración y adversarial)
#![allow(dead_code)]

use std::sync::Arc;

use chrono::NaiveDate;

use lakehouse_rag::application::dtos::ingestion::{
    AristaDestilada, CatalogoDestilado, CeldaDestilada, DistilledDocumentPackage, EjeDestilado,
    HitoDestilado, LimitesDestilados, TablaDestilada,
};
use lakehouse_rag::application::dtos::lakehouse_filas::{
    ClaveDocumental, EjeTabla, FilaArista, FilaCatalogo, FilaCelda, FilaChunk, FilaHito, FilaTabla,
    LimitesEje, LoteIngesta, TipoEjeTabla, TipoNodoGrafo,
};
use lakehouse_rag::application::ports::lakehouse_store::{LakehouseStorePort, ResumenConfirmacion};
use lakehouse_rag::application::ports::llm_distiller::LlmDistillerPort;
use lakehouse_rag::application::use_cases::document_ingestion_service::DocumentIngestionService;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::{DocId, TipoDocumento};
use lakehouse_rag::domain::models::identificadores::{IngestId, TipoEntidad, id_versionado};
use lakehouse_rag::domain::models::operational_matrix::{CalidadExtraccion, FaseOperativa};
use lakehouse_rag::domain::models::reference_graph::TipoRelacionNormativa;
use lakehouse_rag::domain::models::vigencia::VentanaVigencia;
use lakehouse_rag::infrastructure::embeddings::mock_engine::MockEmbeddingEngine;
use lakehouse_rag::infrastructure::llm::MockDistiller;
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::schemas::DIMENSION_EMBEDDINGS;

pub const DOC: &str = "PO-ALM-03";
pub const CENTRO: &str = "obra-planta-solar-01";
pub const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub const HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

/// Filas que `revision` escribe en cada tabla, en el orden de `TODAS_LAS_TABLAS`
pub const FILAS_POR_REVISION: [usize; 6] = [1, 2, 2, 1, 2, 2];

pub const ARCHIVO_V3: &str = "po_alm_03_rev03.md";
pub const ARCHIVO_V4: &str = "po_alm_03_rev04.md";
pub const CONTENIDO_V3: &str = "# PO-ALM-03 Rev. 03\n\n## 1. Recepción\nEl Almacenero recibe la tubería.\n\n## 2. Custodia\nRegistro FOR-ALM-02.\n";
pub const CONTENIDO_V4: &str = "# PO-ALM-03 Rev. 04\n\n## 1. Recepción\nEl Almacenero recibe la tubería.\n\n## 2. Custodia\nRegistro FOR-ALM-05.\n";

pub fn centro() -> CenterId {
    CenterId::new(CENTRO).unwrap()
}

pub fn doc() -> DocId {
    DocId::new(DOC).unwrap()
}

pub fn fecha(anio: i32, mes: u32, dia: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(anio, mes, dia).unwrap()
}

pub async fn store_temporal() -> (tempfile::TempDir, Arc<LanceDbStore>) {
    let dir = tempfile::tempdir().unwrap();
    let store = LanceDbStore::conectar(dir.path()).await.unwrap();
    store.inicializar_tablas().await.unwrap();
    (dir, Arc::new(store))
}

fn clave(doc_id: &str, version: u32) -> ClaveDocumental {
    ClaveDocumental {
        centro_id: centro(),
        doc_id: DocId::new(doc_id).unwrap(),
        version_num: version,
    }
}

fn vector(semilla: usize) -> Vec<f32> {
    (0..DIMENSION_EMBEDDINGS as usize)
        .map(|i| ((i + semilla) % 7) as f32 + 0.5)
        .collect()
}

pub fn fila_catalogo(doc_id: &str, version: u32, vigor: NaiveDate, hash: &str) -> FilaCatalogo {
    FilaCatalogo {
        clave: clave(doc_id, version),
        titulo: format!("{doc_id} Rev. {version:02}"),
        tipo_documento: TipoDocumento::ProcedimientoObra,
        revision: format!("Rev. {version:02}"),
        vigencia: VentanaVigencia::new(vigor, None).unwrap(),
        hash_sha256: hash.to_string(),
    }
}

pub fn fila_chunk(doc_id: &str, version: u32, idx: usize) -> FilaChunk {
    let c = clave(doc_id, version);
    FilaChunk {
        chunk_id: id_versionado(&c.doc_id, version, TipoEntidad::Chunk, idx),
        nivel_prelacion: TipoDocumento::ProcedimientoObra.nivel_prelacion(),
        breadcrumb_jerarquia: format!("{doc_id} > Apartado {idx}"),
        condicion_aplicacion: (idx % 2 == 1).then(|| "T ambiente < 0 °C".to_string()),
        texto_contenido: format!("Texto del apartado {idx} de la versión {version}"),
        tablas_vinculadas: vec![id_versionado(&c.doc_id, version, TipoEntidad::Tabla, 0)],
        hitos_vinculados: Vec::new(),
        vector: vector(idx),
        clave: c,
    }
}

fn fila_hito(doc_id: &str, version: u32, idx: usize, fase: FaseOperativa) -> FilaHito {
    let c = clave(doc_id, version);
    FilaHito {
        hito_id: id_versionado(&c.doc_id, version, TipoEntidad::Hito, idx),
        orden_secuencia: idx as u32 + 1,
        fase_operativa: fase,
        objeto_alcance: "Tubería de acero al carbono".to_string(),
        rol_ejecutor: "Auxiliar de Almacén".to_string(),
        rol_responsable: "Jefe de Producción".to_string(),
        registro_codigo: Some(format!("FOR-ALM-0{}", idx + 1)),
        registro_nombre: None,
        criterio_aceptacion: "Sin daños visibles".to_string(),
        condicion_aplicacion: None,
        nivel_prelacion: TipoDocumento::ProcedimientoObra.nivel_prelacion(),
        calidad_extraccion: CalidadExtraccion::Ok,
        chunk_origen_id: Some(id_versionado(&c.doc_id, version, TipoEntidad::Chunk, 0)),
        vector_hito: (idx == 0).then(|| vector(99)),
        clave: c,
    }
}

fn fila_tabla(doc_id: &str, version: u32) -> FilaTabla {
    let c = clave(doc_id, version);
    FilaTabla {
        tabla_id: id_versionado(&c.doc_id, version, TipoEntidad::Tabla, 0),
        titulo_tabla: "Tabla 4-A".to_string(),
        eje_x: EjeTabla {
            nombre: "espesor_nominal_mm".to_string(),
            tipo: TipoEjeTabla::RangoNumerico,
            unidad: Some("mm".to_string()),
        },
        eje_y: EjeTabla {
            nombre: "carbono_equivalente_pct".to_string(),
            tipo: TipoEjeTabla::RangoNumerico,
            unidad: None,
        },
        valor_salida_nombre: "temperatura_minima_c".to_string(),
        instrucciones_uso_sql: "SELECT ...".to_string(),
        vector_tabla: None,
        clave: c,
    }
}

fn fila_celda(doc_id: &str, version: u32, idx: usize) -> FilaCelda {
    let c = clave(doc_id, version);
    FilaCelda {
        celda_id: id_versionado(&c.doc_id, version, TipoEntidad::Celda, idx),
        tabla_id: id_versionado(&c.doc_id, version, TipoEntidad::Tabla, 0),
        eje_x: LimitesEje {
            minimo: Some(20.0 * idx as f64),
            maximo: Some(20.0 * (idx + 1) as f64),
            maximo_inclusivo: true,
            ..LimitesEje::default()
        },
        eje_y: LimitesEje {
            categoria: Some("S355J2".to_string()),
            ..LimitesEje::default()
        },
        eje_z_filtro_adicional: None,
        valor_resultado_texto: format!("{} °C", 100 + idx),
        valor_resultado_num: Some(100.0 + idx as f64),
        unidad_medida: Some("°C".to_string()),
        nota_pie_condicion: (idx == 1).then(|| "(*) Requiere QA/QC".to_string()),
        nivel_prelacion: TipoDocumento::ProcedimientoObra.nivel_prelacion(),
        clave: c,
    }
}

fn fila_arista(
    doc_id: &str,
    version: u32,
    idx: usize,
    relacion: TipoRelacionNormativa,
    destino: &str,
) -> FilaArista {
    let c = clave(doc_id, version);
    FilaArista {
        edge_id: id_versionado(&c.doc_id, version, TipoEntidad::Arista, idx),
        origen_id: doc_id.to_string(),
        origen_tipo: TipoNodoGrafo::Documento,
        tipo_relacion: relacion,
        destino_id: destino.to_string(),
        destino_tipo: TipoNodoGrafo::destino_de(relacion),
        contexto_relacion: None,
        clave: c,
    }
}

/// Revisión completa: 1 / 2 / 2 / 1 / 2 / 2 filas (ver `FILAS_POR_REVISION`)
pub fn revision(doc_id: &str, version: u32, vigor: NaiveDate, hash: &str) -> Vec<LoteIngesta> {
    vec![
        LoteIngesta::Catalogo(vec![fila_catalogo(doc_id, version, vigor, hash)]),
        LoteIngesta::Chunks(vec![
            fila_chunk(doc_id, version, 0),
            fila_chunk(doc_id, version, 1),
        ]),
        LoteIngesta::Hitos(vec![
            fila_hito(doc_id, version, 0, FaseOperativa::Recepcion),
            fila_hito(doc_id, version, 1, FaseOperativa::Custodia),
        ]),
        LoteIngesta::Tablas(vec![fila_tabla(doc_id, version)]),
        LoteIngesta::Celdas(vec![
            fila_celda(doc_id, version, 0),
            fila_celda(doc_id, version, 1),
        ]),
        LoteIngesta::Aristas(vec![
            fila_arista(
                doc_id,
                version,
                0,
                TipoRelacionNormativa::SubordinadoANorma,
                "UNE-EN-1090-2",
            ),
            fila_arista(
                doc_id,
                version,
                1,
                TipoRelacionNormativa::ExigeLlenarRegistro,
                "FOR-ALM-02",
            ),
        ]),
    ]
}

/// Catálogo + `chunks` apartados (para probar lecturas de más de 10 filas)
pub fn revision_con_chunks(doc_id: &str, version: u32, chunks: usize) -> Vec<LoteIngesta> {
    vec![
        LoteIngesta::Catalogo(vec![fila_catalogo(
            doc_id,
            version,
            fecha(2026, 1, 1),
            HASH_C,
        )]),
        LoteIngesta::Chunks(
            (0..chunks)
                .map(|i| fila_chunk(doc_id, version, i))
                .collect(),
        ),
    ]
}

pub async fn confirmar_revision(
    store: &LanceDbStore,
    ingest: &str,
    version: u32,
    vigor: NaiveDate,
    hash: &str,
) -> ResumenConfirmacion {
    let ingest_id = IngestId::new(ingest).unwrap();
    store
        .escribir_staged(&ingest_id, revision(DOC, version, vigor, hash))
        .await
        .unwrap();
    store.confirmar_ingesta(&ingest_id, &doc()).await.unwrap()
}

pub fn hito_destilado(
    fase: &str,
    ejecutor: &str,
    responsable: &str,
    registro: Option<&str>,
) -> HitoDestilado {
    HitoDestilado {
        orden_secuencia: 0,
        fase_operativa: fase.to_string(),
        objeto_alcance: "Tubería de acero al carbono".to_string(),
        rol_ejecutor: ejecutor.to_string(),
        rol_responsable: responsable.to_string(),
        registro_codigo: registro.map(str::to_string),
        registro_nombre: registro.map(|_| "Registro de la fase".to_string()),
        criterio_aceptacion: "Sin daños".to_string(),
        condicion_aplicacion: None,
        seccion_origen: "2".to_string(),
    }
}

pub fn eje_destilado(nombre: &str, unidad: &str) -> EjeDestilado {
    EjeDestilado {
        nombre: nombre.to_string(),
        tipo: "RANGO_NUMERICO".to_string(),
        unidad: Some(unidad.to_string()),
    }
}

pub fn limites(
    min: Option<f64>,
    max: Option<f64>,
    min_inc: bool,
    max_inc: bool,
) -> LimitesDestilados {
    LimitesDestilados {
        categoria: None,
        min,
        min_inclusivo: min_inc,
        max,
        max_inclusivo: max_inc,
    }
}

pub fn arista_destilada(destino: &str, relacion: &str) -> AristaDestilada {
    AristaDestilada {
        origen_tipo: "DOCUMENTO".to_string(),
        tipo_relacion: relacion.to_string(),
        destino_id: destino.to_string(),
        destino_tipo: "DOCUMENTO".to_string(),
        contexto: None,
    }
}

pub fn catalogo_destilado(version: i64, fecha_vigor: &str) -> CatalogoDestilado {
    CatalogoDestilado {
        doc_id: DOC.to_string(),
        titulo: "Recepción de tubería".to_string(),
        tipo_documento: "PROCEDIMIENTO_OBRA".to_string(),
        revision: format!("Rev. {version:02}"),
        version_num: version,
        fecha_vigor: fecha_vigor.to_string(),
        emisor: "Calidad".to_string(),
    }
}

pub fn paquete_revision(
    version: i64,
    fecha_vigor: &str,
    registro_custodia: &str,
) -> DistilledDocumentPackage {
    let celda = |min: f64, max: f64, valor: &str| CeldaDestilada {
        eje_x: limites(Some(min), Some(max), false, true),
        eje_y: limites(Some(0.40), Some(0.45), true, true),
        eje_z: None,
        valor_resultado: valor.to_string(),
        unidad_medida: Some("°C".to_string()),
        nota_pie_condicion: None,
    };
    DistilledDocumentPackage {
        catalogo: catalogo_destilado(version, fecha_vigor),
        hitos: vec![
            hito_destilado(
                "RECEPCIÓN",
                "Almacenero",
                "Jefe de Almacén",
                Some("FOR-ALM-01"),
            ),
            hito_destilado(
                "Fase 2 — CUSTODIA",
                "Auxiliar de Almacén",
                "Jefe de Producción",
                Some(registro_custodia),
            ),
        ],
        tablas: vec![TablaDestilada {
            tabla_id: "4-A".to_string(),
            titulo: "Tabla 4-A".to_string(),
            eje_x: eje_destilado("espesor_nominal_mm", "mm"),
            eje_y: eje_destilado("carbono_equivalente_pct", "%"),
            valor_salida_nombre: "temperatura_min_c".to_string(),
            seccion_origen: None,
            celdas: vec![celda(0.0, 20.0, "50"), celda(20.0, 40.0, "100 (*)")],
        }],
        aristas: vec![
            arista_destilada("UNE-EN 1090-2", "SUBORDINADO_A_NORMA"),
            arista_destilada(registro_custodia, "EXIGE_LLENAR_REGISTRO"),
        ],
    }
}

/// Destilador mock con las respuestas de la Rev. 03 y la Rev. 04 de `PO-ALM-03`
pub fn distiller_revisiones() -> Arc<MockDistiller> {
    let distiller = MockDistiller::new();
    distiller.registrar_respuesta(ARCHIVO_V3, paquete_revision(3, "2026-03-01", "FOR-ALM-02"));
    distiller.registrar_respuesta(ARCHIVO_V4, paquete_revision(4, "2026-09-15", "FOR-ALM-05"));
    Arc::new(distiller)
}

pub fn servicio(
    distiller: Arc<dyn LlmDistillerPort>,
    store: Arc<dyn LakehouseStorePort>,
) -> DocumentIngestionService {
    DocumentIngestionService::new(
        Arc::new(MarkdownParser::new()),
        distiller,
        Arc::new(MockEmbeddingEngine::new(Some(
            DIMENSION_EMBEDDINGS as usize,
        ))),
        store,
    )
}
