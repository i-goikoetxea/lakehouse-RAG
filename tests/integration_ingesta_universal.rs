//! Ingesta universal (STAIR Fase 2): cobertura sin pérdidas, árbol ToC, fallback sin Claude y
//! reintento de la destilación.

#[path = "comun_ingesta_universal/mod.rs"]
mod comun;

use std::collections::HashSet;
use std::sync::Arc;

use arrow_array::{Array, FixedSizeListArray, Float32Array};
use async_trait::async_trait;
use comun::*;

use lakehouse_rag::application::dtos::lakehouse_filas::LoteIngesta;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, HuellaIngerida, InformeIntegridad, LakehouseStorePort, ResumenConfirmacion,
    TABLA_CHUNKS_NORMATIVA, TABLA_DOCUMENTOS_CATALOGO, TABLA_INDICE_JERARQUICO,
    TABLA_MATRIZ_OPERATIVA,
};
use lakehouse_rag::application::use_cases::document_ingestion_service::DocumentIngestionService;
use lakehouse_rag::domain::errors::DomainError;
use lakehouse_rag::domain::models::center::CenterId;
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::infrastructure::llm::mock_distiller::MockDistiller;
use lakehouse_rag::infrastructure::parsers::markdown_parser::MarkdownParser;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;

const ARCHIVO: &str = "manual planta.md";
const DOCUMENTO: &str = "Aviso preliminar sobre Zeta.

| Pieza | Par |
| --- | --- |
| Tuerca | 40 Nm |
*Nota: apretar en cruz.*

# 1 Operación

## 1.1 Arranque
Comprobar el nivel de aceite. Abrir la válvula. Registrar la lectura.

### 1.1.1 Condiciones bajo cero (< 0 °C)
Precalentar el motor.

## 1.2 Parada sin texto propio

# 2 Mantenimiento
Revisar los filtros.

| Filtro | Horas |
| --- | --- |
| Aire | 500 |
";

fn palabras(texto: &str) -> HashSet<String> {
    texto
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

async fn ingestar_sin_claude(store: &Arc<LanceDbStore>, contenido: &str) -> String {
    let informe = servicio_con(destilador_caido("Claude no responde"), store.clone())
        .ingestar_documento(&centro(), ARCHIVO, contenido)
        .await
        .unwrap();
    assert_eq!(informe.estado_contenido, "PARCIAL_UNIVERSAL");
    informe.doc_id
}

#[tokio::test]
async fn test_lossless_text_coverage() {
    for destilador in [
        destilador_caido("sin Claude"),
        Arc::new(MockDistiller::new()),
    ] {
        let (_dir, store) = almacen().await;
        let largo = format!(
            "{DOCUMENTO}\n# 3 Anexo\n{}",
            "extenso texto final ".repeat(800)
        );
        servicio_con(destilador, store.clone())
            .ingestar_documento(&centro(), ARCHIVO, &largo)
            .await
            .unwrap();

        let mut union = textos(&store, TABLA_CHUNKS_NORMATIVA, "texto_contenido").await;
        union.extend(textos(&store, TABLA_CHUNKS_NORMATIVA, "breadcrumb_jerarquia").await);
        let cubiertas = palabras(&union.join(" "));
        let perdidas: Vec<String> = palabras(&largo).difference(&cubiertas).cloned().collect();
        assert!(perdidas.is_empty(), "palabras perdidas: {perdidas:?}");

        // Tanto el preámbulo como las tablas están en el texto, no solo en el breadcrumb
        let corpus = textos(&store, TABLA_CHUNKS_NORMATIVA, "texto_contenido")
            .await
            .join("\n");
        for literal in ["Aviso preliminar", "| Tuerca | 40 Nm |", "| Aire | 500 |"] {
            assert!(corpus.contains(literal), "falta {literal}");
        }
        exigir_integridad(&store).await;
    }
}

#[tokio::test]
async fn test_toc_tree_lineage() {
    let (_dir, store) = almacen().await;
    let doc = ingestar_sin_claude(&store, DOCUMENTO).await;
    let lotes = store
        .leer_filas_crudas(TABLA_INDICE_JERARQUICO, None)
        .await
        .unwrap();
    let nodos = columna_texto(&lotes, "nodo_id");
    let padres = columna_texto(&lotes, "nodo_padre_id");
    let rutas = columna_texto(&lotes, "ruta_linaje");
    let condiciones = columna_texto(&lotes, "condiciones_alcance");
    let resumenes = columna_texto(&lotes, "resumen_seccion");
    let fila = |ruta: &str| {
        rutas
            .iter()
            .position(|r| r.as_deref() == Some(ruta))
            .unwrap_or_else(|| panic!("falta la ruta {ruta}: {rutas:?}"))
    };

    assert_eq!(nodos.len(), 6, "incluye preámbulo y sección sin texto");
    let preambulo = fila("/0");
    assert_eq!(
        nodos[preambulo].as_deref(),
        Some(format!("{doc}#sec-preambulo").as_str())
    );
    assert_eq!(padres[preambulo], None);
    let arranque = fila("/1/1.1");
    let bajo_cero = fila("/1/1.1/1.1.1");
    assert_eq!(padres[bajo_cero], nodos[arranque]);
    assert_eq!(
        condiciones[bajo_cero].as_deref(),
        Some("Condiciones bajo cero (< 0 °C)")
    );
    assert_eq!(condiciones[arranque], None);
    assert_eq!(
        resumenes[arranque].as_deref(),
        Some("Arranque. Comprobar el nivel de aceite. Abrir la válvula.")
    );
    fila("/1/1.2");
    fila("/2");

    // Los sub-chunks y los chunks apuntan a nodos existentes (se une por nodo_id)
    let existentes: HashSet<String> = nodos.iter().flatten().cloned().collect();
    for nodo in textos(&store, TABLA_CHUNKS_NORMATIVA, "nodo_id").await {
        assert!(existentes.contains(&nodo), "{nodo}");
    }
    let tipos = textos(&store, TABLA_CHUNKS_NORMATIVA, "tipo_bloque").await;
    assert!(tipos.iter().any(|t| t == "PREAMBULO"));
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_offline_ingestion_fallback() {
    let (_dir, store) = almacen().await;
    let informe = servicio_con(destilador_caido("Timeout de 600 s"), store.clone())
        .ingestar_documento(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap();

    assert!(
        informe.doc_id.starts_with("MANUAL-PLANTA_") && informe.doc_id.len() == 20,
        "{}",
        informe.doc_id
    );
    assert_eq!(informe.estado_contenido, "PARCIAL_UNIVERSAL");
    assert!(informe.pendiente_destilacion);
    assert!(informe.avisos[0].contains("Timeout de 600 s"));
    assert!(informe.total_chunks > 0 && informe.total_nodos == 6);
    assert_eq!(informe.total_tuplas, 0);

    for tabla in [TABLA_CHUNKS_NORMATIVA, TABLA_INDICE_JERARQUICO] {
        let estados = textos(&store, tabla, "estado_contenido").await;
        assert!(estados.iter().all(|e| e == "PARCIAL_UNIVERSAL"), "{tabla}");
        // Transaccionalmente es una fila normal, visible para las lecturas
        let estados = textos(&store, tabla, "estado_ingesta").await;
        assert!(estados.iter().all(|e| e == "COMMITTED"), "{tabla}");
    }
    let provisional = columna(&store, TABLA_DOCUMENTOS_CATALOGO, "doc_id_provisional").await;
    assert_eq!(provisional, vec![Some(informe.doc_id.clone())]);
    assert_eq!(
        textos(&store, TABLA_DOCUMENTOS_CATALOGO, "tipo_documento").await,
        ["FORMATO_REGISTRO"]
    );

    // Reingerir no responde «ya ingerido» sin más: ofrece el reintento y no escribe
    let repetida = servicio_con(destilador_caido("sigue caído"), store.clone())
        .ingestar_documento(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap();
    assert!(repetida.ya_ingerido && repetida.pendiente_destilacion);
    assert!(repetida.avisos[0].contains("reintenta"));
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_con_claude_es_canonico_completo_y_sin_provisional() {
    let (_dir, store) = almacen().await;
    let informe = servicio_con(
        DestiladorFijo::nuevo(paquete("PO-ALM-03", 4)),
        store.clone(),
    )
    .ingestar_documento(&centro(), ARCHIVO, DOCUMENTO)
    .await
    .unwrap();
    assert_eq!(informe.doc_id, "PO-ALM-03");
    assert_eq!(informe.estado_contenido, "COMPLETO");
    assert!(informe.avisos.is_empty() && !informe.pendiente_destilacion);
    assert_eq!(
        columna(&store, TABLA_DOCUMENTOS_CATALOGO, "doc_id_provisional").await,
        vec![None]
    );
    assert!(
        textos(&store, TABLA_INDICE_JERARQUICO, "nodo_id")
            .await
            .iter()
            .all(|n| n.starts_with("PO-ALM-03#sec-"))
    );
}

fn vectores_ordenados(lotes: &[arrow_array::RecordBatch], nombre: &str) -> Vec<Vec<f32>> {
    let mut vectores: Vec<Vec<f32>> = lotes
        .iter()
        .flat_map(|lote| {
            let lista = lote
                .column_by_name(nombre)
                .unwrap()
                .as_any()
                .downcast_ref::<FixedSizeListArray>()
                .unwrap();
            (0..lista.len())
                .map(|i| {
                    let fila = lista.value(i);
                    let fila = fila.as_any().downcast_ref::<Float32Array>().unwrap();
                    fila.values().to_vec()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    vectores.sort_by(|a, b| a.partial_cmp(b).unwrap());
    vectores
}

/// Motor que falla si se le pide vectorizar un texto ya presente en 02 o 07
struct MotorSoloCapa2 {
    vistos: std::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl lakehouse_rag::application::ports::embedding_engine::EmbeddingEnginePort for MotorSoloCapa2 {
    async fn generate_embedding(&self, _: &str) -> Result<Vec<f32>, DomainError> {
        Ok(vec![0.5; self.dimension()])
    }
    async fn generate_batch_embeddings(
        &self,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>, DomainError> {
        self.vistos.lock().unwrap().extend_from_slice(texts);
        Ok(vec![vec![0.5; self.dimension()]; texts.len()])
    }
    fn dimension(&self) -> usize {
        lakehouse_rag::infrastructure::storage::schemas::DIMENSION_EMBEDDINGS as usize
    }
}

#[tokio::test]
async fn test_reintento_exitoso_reescribe_al_canonico_reutilizando_vectores() {
    let (_dir, store) = almacen().await;
    let provisional = ingestar_sin_claude(&store, DOCUMENTO).await;
    let chunks_previos = store
        .leer_filas_crudas(TABLA_CHUNKS_NORMATIVA, None)
        .await
        .unwrap();
    let nodos_previos = store
        .leer_filas_crudas(TABLA_INDICE_JERARQUICO, None)
        .await
        .unwrap();

    let motor = Arc::new(MotorSoloCapa2 {
        vistos: Default::default(),
    });
    let servicio = DocumentIngestionService::new(
        Arc::new(MarkdownParser::new()),
        DestiladorFijo::nuevo(paquete("PO-ALM-03", 4)),
        motor.clone(),
        store.clone(),
    );
    let informe = servicio
        .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap();

    assert_eq!(informe.doc_id, "PO-ALM-03");
    assert_eq!(informe.estado_contenido, "COMPLETO");
    assert!(!informe.pendiente_destilacion && !informe.ya_ingerido);
    assert_eq!(informe.total_tuplas, 1);
    let vistos = motor.vistos.lock().unwrap().clone();
    assert_eq!(vistos.len(), 1, "solo el hito es nuevo: {vistos:?}");

    // 02 y 07 conservan los vectores y cambian todos los identificadores
    let chunks = store
        .leer_filas_crudas(TABLA_CHUNKS_NORMATIVA, None)
        .await
        .unwrap();
    assert_eq!(
        vectores_ordenados(&chunks, "vector"),
        vectores_ordenados(&chunks_previos, "vector")
    );
    let nodos = store
        .leer_filas_crudas(TABLA_INDICE_JERARQUICO, None)
        .await
        .unwrap();
    assert_eq!(
        vectores_ordenados(&nodos, "vector_seccion"),
        vectores_ordenados(&nodos_previos, "vector_seccion")
    );
    for (tabla, nombre) in [
        (TABLA_CHUNKS_NORMATIVA, "chunk_id"),
        (TABLA_CHUNKS_NORMATIVA, "nodo_id"),
        (TABLA_INDICE_JERARQUICO, "nodo_id"),
        (TABLA_INDICE_JERARQUICO, "nodo_padre_id"),
    ] {
        for valor in textos(&store, tabla, nombre).await {
            assert!(valor.starts_with("PO-ALM-03"), "{tabla}.{nombre}: {valor}");
        }
    }
    assert!(
        textos(&store, TABLA_CHUNKS_NORMATIVA, "estado_contenido")
            .await
            .iter()
            .all(|e| e == "COMPLETO")
    );

    // El provisional desaparece y queda solo la trazabilidad en 01
    assert_eq!(
        textos(&store, TABLA_DOCUMENTOS_CATALOGO, "doc_id").await,
        ["PO-ALM-03"]
    );
    assert_eq!(
        columna(&store, TABLA_DOCUMENTOS_CATALOGO, "doc_id_provisional").await,
        vec![Some(provisional)]
    );
    assert_eq!(
        textos(&store, TABLA_MATRIZ_OPERATIVA, "doc_id").await.len(),
        1
    );
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_reintento_es_idempotente_y_no_llama_a_claude_otra_vez() {
    let (_dir, store) = almacen().await;
    ingestar_sin_claude(&store, DOCUMENTO).await;
    let destilador = DestiladorFijo::nuevo(paquete("PO-ALM-03", 4));
    let servicio = servicio_con(destilador.clone(), store.clone());
    servicio
        .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap();
    let foto = foto_de_filas(&store).await;

    let segundo = servicio
        .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap();
    assert!(segundo.ya_ingerido && !segundo.pendiente_destilacion);
    assert_eq!(segundo.doc_id, "PO-ALM-03");
    assert_eq!(destilador.total_llamadas(), 1);
    assert_eq!(foto_de_filas(&store).await, foto);

    let reingesta = servicio
        .ingestar_documento(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap();
    assert!(reingesta.ya_ingerido && !reingesta.pendiente_destilacion);
}

#[tokio::test]
async fn test_reintento_sin_ingesta_previa_es_error_y_no_llama_a_claude() {
    let (_dir, store) = almacen().await;
    let destilador = DestiladorFijo::nuevo(paquete("PO-ALM-03", 4));
    let error = servicio_con(destilador.clone(), store)
        .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no está ingerido"), "{error}");
    assert_eq!(destilador.total_llamadas(), 0);
}

#[tokio::test]
async fn test_reintento_deroga_la_revision_canonica_previa() {
    let (_dir, store) = almacen().await;
    let mut rev3 = paquete("PO-ALM-03", 3);
    rev3.catalogo.fecha_vigor = Some("2025-01-10".to_string());
    servicio_con(DestiladorFijo::nuevo(rev3), store.clone())
        .ingestar_documento(
            &centro(),
            "rev3.md",
            "# 1 Viejo\nTexto de la revisión tres.\n",
        )
        .await
        .unwrap();
    ingestar_sin_claude(&store, DOCUMENTO).await;

    let informe = servicio_con(
        DestiladorFijo::nuevo(paquete("PO-ALM-03", 4)),
        store.clone(),
    )
    .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
    .await
    .unwrap();
    assert_eq!(informe.versiones_derogadas, vec![3]);

    let vigentes = store
        .leer_tabla(
            TABLA_DOCUMENTOS_CATALOGO,
            &ConsultaTabla::confirmadas(10).solo_vigentes(),
        )
        .await
        .unwrap();
    assert_eq!(columna_texto(&vigentes, "doc_id").len(), 1);
    assert_eq!(
        columna_texto(&vigentes, "revision")[0].as_deref(),
        Some("Rev. 04")
    );
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_reintento_con_version_en_conflicto_deja_el_estado_intacto() {
    let (_dir, store) = almacen().await;
    servicio_con(
        DestiladorFijo::nuevo(paquete("PO-ALM-03", 4)),
        store.clone(),
    )
    .ingestar_documento(
        &centro(),
        "rev4.md",
        "# 1 Vigente\nTexto de la revisión cuatro.\n",
    )
    .await
    .unwrap();
    ingestar_sin_claude(&store, DOCUMENTO).await;
    let foto = foto_de_filas(&store).await;

    let error = servicio_con(
        DestiladorFijo::nuevo(paquete("PO-ALM-03", 4)),
        store.clone(),
    )
    .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
    .await
    .unwrap_err();
    assert!(matches!(error, DomainError::VersionConflict(_)), "{error}");
    assert_eq!(foto_de_filas(&store).await, foto);
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_reintento_sin_codigo_valido_consolida_el_provisional() {
    let (_dir, store) = almacen().await;
    let provisional = ingestar_sin_claude(&store, DOCUMENTO).await;
    let informe = servicio_con(DestiladorFijo::nuevo(paquete("¡¡¡", 4)), store.clone())
        .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap();

    assert_eq!(informe.doc_id, provisional);
    assert_eq!(informe.version, 1);
    assert_eq!(informe.estado_contenido, "COMPLETO");
    assert_eq!(informe.total_tuplas, 1);
    assert_eq!(
        columna(&store, TABLA_DOCUMENTOS_CATALOGO, "doc_id_provisional").await,
        vec![None],
        "un consolidado ya no es provisional"
    );
    let estados = textos(&store, TABLA_INDICE_JERARQUICO, "estado_contenido").await;
    assert!(estados.iter().all(|e| e == "COMPLETO"));
    assert_eq!(estados.len(), 6, "sin filas duplicadas");
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_reintento_con_claude_caido_deja_el_estado_intacto() {
    let (_dir, store) = almacen().await;
    ingestar_sin_claude(&store, DOCUMENTO).await;
    let foto = foto_de_filas(&store).await;
    let error = servicio_con(destilador_caido("sigue caído"), store.clone())
        .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("sigue caído"));
    assert_eq!(foto_de_filas(&store).await, foto);
}

/// Almacén que falla en un punto concreto de la sustitución
struct AlmacenConFallo {
    interno: Arc<LanceDbStore>,
    falla_al_escribir_chunks: bool,
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
        id: &IngestId,
        mut lotes: Vec<LoteIngesta>,
    ) -> Result<usize, DomainError> {
        if self.falla_al_escribir_chunks {
            lotes.retain(|l| matches!(l, LoteIngesta::Catalogo(_)));
            self.interno.escribir_staged(id, lotes).await?;
            return Err(DomainError::StorageError("Disco lleno".to_string()));
        }
        self.interno.escribir_staged(id, lotes).await
    }
    async fn confirmar_ingesta(
        &self,
        id: &IngestId,
        doc: &DocId,
    ) -> Result<ResumenConfirmacion, DomainError> {
        self.interno.confirmar_ingesta(id, doc).await
    }
    async fn confirmar_sustitucion(
        &self,
        _: &IngestId,
        _: &DocId,
        _: &IngestId,
    ) -> Result<ResumenConfirmacion, DomainError> {
        Err(DomainError::StorageError("Fallo al confirmar".to_string()))
    }
    async fn revertir_ingesta(&self, id: &IngestId) -> Result<u64, DomainError> {
        self.interno.revertir_ingesta(id).await
    }
    async fn leer_tabla(
        &self,
        tabla: &str,
        consulta: &ConsultaTabla,
    ) -> Result<Vec<arrow_array::RecordBatch>, DomainError> {
        self.interno.leer_tabla(tabla, consulta).await
    }
    async fn verificar_integridad(&self) -> Result<InformeIntegridad, DomainError> {
        self.interno.verificar_integridad().await
    }
    async fn contar_registros(&self, tabla: &str) -> Result<usize, DomainError> {
        self.interno.contar_registros(tabla).await
    }
    async fn listar_tablas(&self) -> Result<Vec<String>, DomainError> {
        self.interno.listar_tablas().await
    }
}

#[tokio::test]
async fn test_reintento_con_fallo_a_mitad_deja_el_estado_intacto_y_admite_otro_intento() {
    let (_dir, store) = almacen().await;
    ingestar_sin_claude(&store, DOCUMENTO).await;
    let foto = foto_de_filas(&store).await;

    for falla_al_escribir_chunks in [true, false] {
        let con_fallo = Arc::new(AlmacenConFallo {
            interno: store.clone(),
            falla_al_escribir_chunks,
        });
        let error = servicio_con(DestiladorFijo::nuevo(paquete("PO-ALM-03", 4)), con_fallo)
            .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
            .await
            .unwrap_err();
        assert!(matches!(error, DomainError::StorageError(_)), "{error}");
        assert_eq!(
            foto_de_filas(&store).await,
            foto,
            "{falla_al_escribir_chunks}"
        );
        exigir_integridad(&store).await;
    }

    let informe = servicio_con(
        DestiladorFijo::nuevo(paquete("PO-ALM-03", 4)),
        store.clone(),
    )
    .reintentar_destilacion(&centro(), ARCHIVO, DOCUMENTO)
    .await
    .unwrap();
    assert_eq!(informe.estado_contenido, "COMPLETO");
    exigir_integridad(&store).await;
}

#[tokio::test]
async fn test_sustitucion_real_con_fallo_en_07_restaura_la_vigencia_del_provisional() {
    let (dir, store) = almacen().await;
    ingestar_sin_claude(&store, DOCUMENTO).await;
    let vigentes_antes = store
        .leer_filas_crudas(TABLA_CHUNKS_NORMATIVA, Some("vigente = true"))
        .await
        .unwrap();
    let total_antes: usize = vigentes_antes.iter().map(|l| l.num_rows()).sum();

    // Escribe la revisión nueva y rompe 07 justo antes de confirmar
    let ingest = IngestId::new("ING-NUEVA").unwrap();
    let huella = store
        .existe_hash(&centro(), &sha_del_documento())
        .await
        .unwrap()
        .unwrap();
    store
        .escribir_staged(&ingest, lotes_minimos_canonicos())
        .await
        .unwrap();
    let ruta_07 = dir.path().join(format!("{TABLA_INDICE_JERARQUICO}.lance"));
    let aparte = dir.path().join("07_retirada");
    std::fs::rename(&ruta_07, &aparte).unwrap();
    let error = store
        .confirmar_sustitucion(
            &ingest,
            &DocId::new("PO-ALM-03").unwrap(),
            &huella.ingest_id,
        )
        .await;
    std::fs::rename(&aparte, &ruta_07).unwrap();
    assert!(error.is_err());
    store.revertir_ingesta(&ingest).await.unwrap();

    let vigentes = store
        .leer_filas_crudas(TABLA_CHUNKS_NORMATIVA, Some("vigente = true"))
        .await
        .unwrap();
    assert_eq!(
        vigentes.iter().map(|l| l.num_rows()).sum::<usize>(),
        total_antes
    );
    exigir_integridad(&store).await;
}

fn sha_del_documento() -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(DOCUMENTO.as_bytes()))
}

fn lotes_minimos_canonicos() -> Vec<LoteIngesta> {
    use lakehouse_rag::application::dtos::lakehouse_filas::{ClaveDocumental, FilaCatalogo};
    use lakehouse_rag::domain::models::document::TipoDocumento;
    use lakehouse_rag::domain::models::vigencia::VentanaVigencia;
    let clave = ClaveDocumental {
        centro_id: centro(),
        doc_id: DocId::new("PO-ALM-03").unwrap(),
        version_num: 4,
    };
    vec![LoteIngesta::Catalogo(vec![FilaCatalogo {
        clave,
        titulo: "Canónico".to_string(),
        tipo_documento: TipoDocumento::ProcedimientoObra,
        revision: "Rev. 04".to_string(),
        vigencia: VentanaVigencia::new(chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(), None)
            .unwrap(),
        hash_sha256: sha_del_documento(),
        doc_id_provisional: None,
    }])]
}
