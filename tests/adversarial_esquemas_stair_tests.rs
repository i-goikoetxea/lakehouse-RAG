//! Suite adversarial de los esquemas STAIR (Fase 1): `nodo_id` malicioso o vacío, estados
//! inválidos, huérfanos entre 02 y 07, paradojas del árbol, rollback con fallo inyectado en 07
//! y `doc_id_provisional` malicioso.

mod comun_store_v2;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{ArrayRef, RecordBatch, RecordBatchIterator, StringArray};

use comun_store_v2::*;
use lakehouse_rag::application::dtos::centros::{
    ManifiestoCentroDisco, ModeloEmbeddingsActivo, VERSION_ESQUEMA_CENTRO,
};
use lakehouse_rag::application::dtos::lakehouse_filas::LoteIngesta;
use lakehouse_rag::application::ports::lakehouse_store::{
    ConsultaTabla, LakehouseStorePort, TABLA_CHUNKS_NORMATIVA, TABLA_DOCUMENTOS_CATALOGO,
    TABLA_INDICE_JERARQUICO, TODAS_LAS_TABLAS,
};
use lakehouse_rag::domain::models::document::DocId;
use lakehouse_rag::domain::models::estructura_documental::{
    EstadoContenido, NivelJerarquico, NodoId, TipoBloque,
};
use lakehouse_rag::domain::models::identificadores::IngestId;
use lakehouse_rag::infrastructure::storage::lancedb_store::LanceDbStore;
use lakehouse_rag::infrastructure::storage::lotes_arrow::{
    ColumnasTransaccionales, lote_a_record_batch,
};

const CARGAS_MALICIOSAS: [&str; 7] = [
    "x' OR '1'='1",
    "PO-ALM-03#sec-1'; DROP TABLE t; --",
    "PO-ALM-03#sec-\"1\"",
    "../../etc/passwd",
    "PO-ALM-03#sec-1\0",
    "PO-ALM-03#sec-1%",
    "PO-ALM-03#sec-..",
];

fn ingesta(valor: &str) -> IngestId {
    IngestId::new(valor).unwrap()
}

fn filas(lotes: &[RecordBatch]) -> usize {
    lotes.iter().map(RecordBatch::num_rows).sum()
}

async fn conteos(store: &LanceDbStore) -> Vec<usize> {
    let mut totales = Vec::new();
    for tabla in TODAS_LAS_TABLAS {
        totales.push(store.contar_registros(tabla).await.unwrap());
    }
    totales
}

fn ruta_tabla_07(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join(format!("{TABLA_INDICE_JERARQUICO}.lance"))
}

/// Inyecta un fallo solo en 07 retirando su directorio; devuelve la ruta de restauración
fn romper_tabla_07(dir: &tempfile::TempDir) -> PathBuf {
    let apartada = dir.path().join("indice_apartado");
    std::fs::rename(ruta_tabla_07(dir), &apartada).unwrap();
    apartada
}

fn restaurar_tabla_07(dir: &tempfile::TempDir, apartada: &Path) {
    std::fs::rename(apartada, ruta_tabla_07(dir)).unwrap();
}

/// Inserta una fila saltándose los tipos, como haría un `.lance` manipulado a mano
async fn insertar_con_columna_manipulada(
    store: &LanceDbStore,
    tabla: &str,
    lote: &LoteIngesta,
    columna: &str,
    valor: &str,
) {
    let transaccion = ColumnasTransaccionales {
        ingest_id: ingesta("ING-MANIPULADA"),
        estado: lakehouse_rag::application::ports::lakehouse_store::EstadoIngesta::Committed,
        vigente: true,
    };
    let original = lote_a_record_batch(lote, &transaccion).unwrap();
    let esquema = original.schema();
    let indice = esquema.index_of(columna).unwrap();
    let mut columnas: Vec<ArrayRef> = original.columns().to_vec();
    columnas[indice] = Arc::new(StringArray::from(vec![valor; original.num_rows()]));
    let manipulado = RecordBatch::try_new(esquema.clone(), columnas).unwrap();
    let lector: Box<dyn arrow_array::RecordBatchReader + Send> =
        Box::new(RecordBatchIterator::new(vec![Ok(manipulado)], esquema));
    store
        .abrir_tabla(tabla)
        .await
        .unwrap()
        .add(lector)
        .execute()
        .await
        .unwrap();
}

// ---------- nodo_id ----------

#[test]
fn test_nodo_id_vacio_o_con_comillas_se_rechaza() {
    for valor in [
        "",
        "   ",
        "PO-ALM-03",
        "#sec-1",
        "PO-ALM-03#sec-",
        "A#sec-1#sec-2",
    ] {
        assert!(NodoId::new(valor).is_err(), "{valor:?}");
    }
    for carga in CARGAS_MALICIOSAS {
        assert!(NodoId::new(carga).is_err(), "{carga:?}");
    }
    assert!(NodoId::new(format!("A#sec-{}", "9".repeat(600))).is_err());
    assert_eq!(
        NodoId::new("PO-ALM-03#sec-4.2").unwrap().as_str(),
        "PO-ALM-03#sec-4.2"
    );
}

#[test]
fn test_nodo_de_seccion_neutraliza_codigos_maliciosos() {
    let doc = doc();
    for carga in CARGAS_MALICIOSAS {
        let nodo = NodoId::de_seccion(&doc, carga, 7);
        assert!(NodoId::new(nodo.as_str()).is_ok(), "{carga:?} → {nodo}");
        assert!(nodo.pertenece_a(&doc), "{carga:?} → {nodo}");
        assert!(!nodo.as_str().contains('\''), "{nodo}");
    }
    assert_eq!(
        NodoId::de_seccion(&doc, "", 3).as_str(),
        "PO-ALM-03#sec-idx-3"
    );
    assert_eq!(
        NodoId::de_seccion(&doc, "4.2", 3).as_str(),
        "PO-ALM-03#sec-4.2"
    );
    assert_eq!(NodoId::preambulo(&doc).as_str(), "PO-ALM-03#sec-preambulo");
}

#[tokio::test]
async fn test_chunk_con_nodo_de_otro_documento_no_se_escribe() {
    let (_dir, store) = store_temporal().await;
    let mut chunk = fila_chunk(DOC, 3, 0);
    chunk.nodo_id = NodoId::new("OTRO-DOC#sec-1").unwrap();
    let error = store
        .escribir_staged(
            &ingesta("ING-AJENO"),
            vec![
                LoteIngesta::Catalogo(vec![fila_catalogo(DOC, 3, fecha(2026, 1, 1), HASH_A)]),
                LoteIngesta::Chunks(vec![chunk]),
            ],
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("OTRO-DOC#sec-1"), "{error}");
    assert_eq!(conteos(&store).await.iter().sum::<usize>(), 0);
}

// ---------- estados ----------

#[test]
fn test_vocabularios_rechazan_valores_cruzados_y_variantes() {
    for valor in [
        "STAGED",
        "COMMITTED",
        "completo",
        " COMPLETO",
        "",
        "COMPLETO'--",
    ] {
        assert!(EstadoContenido::parse(valor).is_err(), "{valor:?}");
    }
    for valor in ["COMPLETO", "PARCIAL_UNIVERSAL", "REQUIERE_OCR"] {
        assert_eq!(EstadoContenido::parse(valor).unwrap().as_str(), valor);
    }
    for valor in ["TABLA", "narrativa", "", "NARRATIVA;"] {
        assert!(TipoBloque::parse(valor).is_err(), "{valor:?}");
    }
    for valor in [
        "PREAMBULO",
        "NARRATIVA",
        "DEFINICION",
        "ALCANCE",
        "PROCEDIMIENTO",
    ] {
        assert_eq!(TipoBloque::parse(valor).unwrap().as_str(), valor);
    }
}

#[tokio::test]
async fn test_estado_de_contenido_en_estado_ingesta_es_invisible_y_se_diagnostica() {
    let (_dir, store) = store_temporal().await;
    insertar_con_columna_manipulada(
        &store,
        TABLA_CHUNKS_NORMATIVA,
        &LoteIngesta::Chunks(vec![fila_chunk(DOC, 3, 0)]),
        "estado_ingesta",
        "PARCIAL_UNIVERSAL",
    )
    .await;

    let lectura = store
        .leer_tabla(TABLA_CHUNKS_NORMATIVA, &ConsultaTabla::confirmadas(100))
        .await
        .unwrap();
    assert_eq!(filas(&lectura), 0);

    let informe = store.verificar_integridad().await.unwrap();
    assert!(!informe.es_integro());
    assert!(
        informe
            .valores_invalidos
            .iter()
            .any(|v| v.tabla == TABLA_CHUNKS_NORMATIVA
                && v.columna == "estado_ingesta"
                && v.valor == "PARCIAL_UNIVERSAL"),
        "{:?}",
        informe.valores_invalidos
    );
}

#[tokio::test]
async fn test_vocabularios_manipulados_en_disco_se_diagnostican() {
    let (_dir, store) = store_temporal().await;
    let casos = [
        (TABLA_CHUNKS_NORMATIVA, "estado_contenido", "COMMITTED"),
        (TABLA_CHUNKS_NORMATIVA, "tipo_bloque", "TABLA'--"),
        (TABLA_INDICE_JERARQUICO, "estado_contenido", "STAGED"),
    ];
    for (tabla, columna, valor) in casos {
        let lote = if tabla == TABLA_CHUNKS_NORMATIVA {
            LoteIngesta::Chunks(vec![fila_chunk(DOC, 3, 0)])
        } else {
            LoteIngesta::Indice(vec![fila_indice(DOC, 3, 0)])
        };
        insertar_con_columna_manipulada(&store, tabla, &lote, columna, valor).await;
    }
    let informe = store.verificar_integridad().await.unwrap();
    for (tabla, columna, valor) in casos {
        assert!(
            informe
                .valores_invalidos
                .iter()
                .any(|v| v.tabla == tabla && v.columna == columna && v.valor == valor),
            "{tabla}.{columna}: {:?}",
            informe.valores_invalidos
        );
    }
}

// ---------- huérfanos entre 02 y 07 ----------

#[tokio::test]
async fn test_chunk_sin_nodo_en_la_misma_escritura_se_rechaza_entera() {
    let (_dir, store) = store_temporal().await;
    let error = store
        .escribir_staged(
            &ingesta("ING-HUERFANO"),
            vec![
                LoteIngesta::Catalogo(vec![fila_catalogo(DOC, 3, fecha(2026, 1, 1), HASH_A)]),
                LoteIngesta::Chunks(vec![fila_chunk(DOC, 3, 0), fila_chunk(DOC, 3, 1)]),
                LoteIngesta::Indice(vec![fila_indice(DOC, 3, 0)]),
            ],
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(&nodo_de(DOC, 1)), "{error}");
    assert_eq!(conteos(&store).await.iter().sum::<usize>(), 0);
}

#[tokio::test]
async fn test_integridad_detecta_chunk_huerfano_y_padre_inexistente() {
    let (_dir, store) = store_temporal().await;
    let transaccion = ColumnasTransaccionales {
        ingest_id: ingesta("ING-SIEMBRA"),
        estado: lakehouse_rag::application::ports::lakehouse_store::EstadoIngesta::Committed,
        vigente: true,
    };
    let mut hijo = fila_indice(DOC, 3, 0);
    hijo.nodo_padre_id = Some(NodoId::new(format!("{DOC}#sec-fantasma")).unwrap());
    hijo.nivel_jerarquico = NivelJerarquico::new(2).unwrap();
    for lote in [
        LoteIngesta::Catalogo(vec![fila_catalogo(DOC, 3, fecha(2026, 1, 1), HASH_A)]),
        LoteIngesta::Chunks(vec![fila_chunk(DOC, 3, 0), fila_chunk(DOC, 3, 1)]),
        LoteIngesta::Indice(vec![hijo]),
    ] {
        store.sembrar_filas(&lote, &transaccion).await.unwrap();
    }

    let informe = store.verificar_integridad().await.unwrap();
    assert!(!informe.es_integro());
    let chunk = informe
        .nodos_huerfanos
        .iter()
        .find(|h| h.tabla == TABLA_CHUNKS_NORMATIVA)
        .expect("chunk huérfano");
    assert_eq!(chunk.nodo_id, nodo_de(DOC, 1));
    assert_eq!(chunk.version_num, 3);
    assert!(
        informe
            .nodos_huerfanos
            .iter()
            .any(|h| h.tabla == TABLA_INDICE_JERARQUICO && h.nodo_id == "PO-ALM-03#sec-fantasma"),
        "{:?}",
        informe.nodos_huerfanos
    );
}

#[tokio::test]
async fn test_el_nodo_de_otra_revision_no_salva_al_chunk_huerfano() {
    let (_dir, store) = store_temporal().await;
    let transaccion = ColumnasTransaccionales {
        ingest_id: ingesta("ING-CRUZADA"),
        estado: lakehouse_rag::application::ports::lakehouse_store::EstadoIngesta::Committed,
        vigente: false,
    };
    for lote in [
        LoteIngesta::Catalogo(vec![
            fila_catalogo(DOC, 3, fecha(2026, 1, 1), HASH_A),
            fila_catalogo(DOC, 4, fecha(2026, 6, 1), HASH_B),
        ]),
        // v3 tiene el nodo 1; v4 tiene índice, pero no contiene ese nodo
        LoteIngesta::Indice(vec![fila_indice(DOC, 3, 1), fila_indice(DOC, 4, 0)]),
        LoteIngesta::Chunks(vec![fila_chunk(DOC, 4, 1)]),
    ] {
        store.sembrar_filas(&lote, &transaccion).await.unwrap();
    }
    let informe = store.verificar_integridad().await.unwrap();
    assert_eq!(informe.nodos_huerfanos.len(), 1, "{informe:?}");
    assert_eq!(informe.nodos_huerfanos[0].version_num, 4);
}

#[tokio::test]
async fn test_revision_sin_07_es_huerfana_y_nodo_sin_chunks_no() {
    let (_dir, store) = store_temporal().await;
    // Toda ingesta escribe 07 desde la Fase 2: chunks sin índice son daño, no un estado normal
    let id = ingesta("ING-SIN-07");
    store
        .escribir_staged(&id, revision_con_chunks(DOC, 3, 2))
        .await
        .unwrap();
    store.confirmar_ingesta(&id, &doc()).await.unwrap();
    let informe = store.verificar_integridad().await.unwrap();
    assert_eq!(informe.nodos_huerfanos.len(), 2, "{informe:?}");
    let (_dir, store) = store_temporal().await;
    // Un título puede no tener texto propio: nodo en 07 sin chunks
    let id = ingesta("ING-TITULO");
    store
        .escribir_staged(
            &id,
            vec![
                LoteIngesta::Catalogo(vec![fila_catalogo("DOC-B", 1, fecha(2026, 1, 1), HASH_B)]),
                LoteIngesta::Indice(vec![fila_indice("DOC-B", 1, 0)]),
            ],
        )
        .await
        .unwrap();
    store
        .confirmar_ingesta(&id, &DocId::new("DOC-B").unwrap())
        .await
        .unwrap();

    let informe = store.verificar_integridad().await.unwrap();
    assert!(informe.es_integro(), "{informe:?}");
}

// ---------- paradojas del árbol y valores frontera ----------

#[tokio::test]
async fn test_paradojas_del_arbol_se_rechazan_sin_escribir() {
    let (_dir, store) = store_temporal().await;
    let a = fila_indice(DOC, 3, 0);
    let nodo_a = a.nodo_id.clone();
    let b = fila_indice(DOC, 3, 1);
    let nodo_b = b.nodo_id.clone();

    let mut auto_padre = a.clone();
    auto_padre.nodo_padre_id = Some(nodo_a.clone());

    let mut ciclo_a = a.clone();
    ciclo_a.nodo_padre_id = Some(nodo_b);
    let mut ciclo_b = b.clone();
    ciclo_b.nodo_padre_id = Some(nodo_a.clone());

    let mut padre_ajeno = b.clone();
    padre_ajeno.nodo_padre_id = Some(NodoId::new("OTRO-DOC#sec-1").unwrap());

    let mut sin_barra = a.clone();
    sin_barra.ruta_linaje = "4/4.2".to_string();

    let mut vector_corto = a.clone();
    vector_corto.vector_seccion = vec![0.1, 0.2];

    let casos = [
        ("auto-padre", vec![auto_padre]),
        ("ciclo", vec![ciclo_a, ciclo_b]),
        ("duplicado", vec![a.clone(), a.clone()]),
        ("padre ajeno", vec![a.clone(), padre_ajeno]),
        ("ruta sin barra", vec![sin_barra]),
        ("vector corto", vec![vector_corto]),
    ];
    for (caso, nodos) in casos {
        let resultado = store
            .escribir_staged(
                &ingesta("ING-PARADOJA"),
                vec![
                    LoteIngesta::Catalogo(vec![fila_catalogo(DOC, 3, fecha(2026, 1, 1), HASH_A)]),
                    LoteIngesta::Indice(nodos),
                ],
            )
            .await;
        assert!(resultado.is_err(), "{caso}");
        store
            .revertir_ingesta(&ingesta("ING-PARADOJA"))
            .await
            .unwrap();
        assert_eq!(conteos(&store).await.iter().sum::<usize>(), 0, "{caso}");
    }
}

#[test]
fn test_nivel_jerarquico_rechaza_la_frontera() {
    for valor in [-1, 7, i8::MIN, i8::MAX] {
        assert!(NivelJerarquico::new(valor).is_err(), "{valor}");
    }
    for valor in 0..=6 {
        assert_eq!(NivelJerarquico::new(valor).unwrap().valor(), valor);
    }
}

// ---------- rollback con fallo inyectado en 07 ----------

#[tokio::test]
async fn test_fallo_en_07_al_confirmar_no_expone_nada_y_conserva_la_revision_previa() {
    let (dir, store) = store_temporal().await;
    confirmar_revision(&store, "ING-V3", 3, fecha(2026, 1, 1), HASH_A).await;
    let previos = conteos(&store).await;

    let id = ingesta("ING-V4");
    store
        .escribir_staged(&id, revision(DOC, 4, fecha(2026, 6, 1), HASH_B))
        .await
        .unwrap();

    let apartada = romper_tabla_07(&dir);
    assert!(store.confirmar_ingesta(&id, &doc()).await.is_err());

    // Durante el fallo, las lecturas solo ven la v3 en las demás tablas
    let lectura = store
        .leer_tabla(TABLA_CHUNKS_NORMATIVA, &ConsultaTabla::confirmadas(100))
        .await
        .unwrap();
    assert_eq!(filas(&lectura), FILAS_POR_REVISION[1], "02 no expone la v4");
    restaurar_tabla_07(&dir, &apartada);

    store.revertir_ingesta(&id).await.unwrap();
    assert_eq!(conteos(&store).await, previos);
    for tabla in TODAS_LAS_TABLAS {
        let vigentes = store
            .leer_tabla(tabla, &ConsultaTabla::confirmadas(100).solo_vigentes())
            .await
            .unwrap();
        assert!(filas(&vigentes) > 0, "{tabla} perdió la v3 vigente");
    }
    let informe = store.verificar_integridad().await.unwrap();
    assert!(informe.es_integro(), "{informe:?}");
}

#[tokio::test]
async fn test_fallo_en_07_al_escribir_y_al_revertir_admite_reintento() {
    let (dir, store) = store_temporal().await;
    let id = ingesta("ING-ROTA");
    let apartada = romper_tabla_07(&dir);
    assert!(
        store
            .escribir_staged(&id, revision(DOC, 3, fecha(2026, 1, 1), HASH_A))
            .await
            .is_err()
    );
    // El rollback también falla en 07, pero el reintento termina de limpiar
    assert!(store.revertir_ingesta(&id).await.is_err());
    restaurar_tabla_07(&dir, &apartada);
    store.revertir_ingesta(&id).await.unwrap();

    assert_eq!(conteos(&store).await.iter().sum::<usize>(), 0);
    assert!(store.verificar_integridad().await.unwrap().es_integro());
}

#[tokio::test]
async fn test_07_cumple_staged_commit_retirada_y_time_travel() {
    let (_dir, store) = store_temporal().await;
    let id = ingesta("ING-V3");
    store
        .escribir_staged(&id, revision(DOC, 3, fecha(2026, 1, 1), HASH_A))
        .await
        .unwrap();
    let consulta = ConsultaTabla::confirmadas(100);
    let staged = store
        .leer_tabla(TABLA_INDICE_JERARQUICO, &consulta)
        .await
        .unwrap();
    assert_eq!(filas(&staged), 0, "las filas STAGED de 07 no se leen");
    store.confirmar_ingesta(&id, &doc()).await.unwrap();
    confirmar_revision(&store, "ING-V4", 4, fecha(2026, 6, 1), HASH_B).await;

    let historico = store
        .leer_tabla(TABLA_INDICE_JERARQUICO, &consulta)
        .await
        .unwrap();
    assert_eq!(filas(&historico), FILAS_POR_REVISION[6] * 2);
    let vigentes = store
        .leer_tabla(TABLA_INDICE_JERARQUICO, &consulta.clone().solo_vigentes())
        .await
        .unwrap();
    assert_eq!(filas(&vigentes), FILAS_POR_REVISION[6]);
    let informe = store.verificar_integridad().await.unwrap();
    assert!(informe.es_integro(), "{informe:?}");
}

// ---------- doc_id_provisional ----------

#[test]
fn test_doc_id_provisional_malicioso_no_se_puede_construir() {
    for carga in ["x' OR '1'='1", "../../etc/passwd", "DOC;DROP", "DOC\0", ""] {
        assert!(DocId::new(carga).is_err(), "{carga:?}");
    }
    assert!(DocId::new("PROC_SOLDADURA_A4F2C1").is_ok());
}

#[tokio::test]
async fn test_doc_id_provisional_se_guarda_y_el_manipulado_se_diagnostica() {
    let (_dir, store) = store_temporal().await;
    let id = ingesta("ING-PROV");
    let provisional = DocId::new("PROC_SOLDADURA_A4F2C1").unwrap();
    let mut catalogo = fila_catalogo(provisional.as_str(), 1, fecha(2026, 1, 1), HASH_A);
    catalogo.doc_id_provisional = Some(provisional.clone());
    store
        .escribir_staged(&id, vec![LoteIngesta::Catalogo(vec![catalogo.clone()])])
        .await
        .unwrap();
    store.confirmar_ingesta(&id, &provisional).await.unwrap();
    let lotes = store
        .leer_tabla(TABLA_DOCUMENTOS_CATALOGO, &ConsultaTabla::confirmadas(10))
        .await
        .unwrap();
    let columna = lotes[0]
        .column_by_name("doc_id_provisional")
        .expect("columna en 01");
    let textos = columna.as_any().downcast_ref::<StringArray>().unwrap();
    assert_eq!(textos.value(0), provisional.as_str());
    assert!(store.verificar_integridad().await.unwrap().es_integro());

    let carga = "x' OR '1'='1";
    catalogo.clave.version_num = 2;
    insertar_con_columna_manipulada(
        &store,
        TABLA_DOCUMENTOS_CATALOGO,
        &LoteIngesta::Catalogo(vec![catalogo]),
        "doc_id_provisional",
        carga,
    )
    .await;
    let informe = store.verificar_integridad().await.unwrap();
    assert!(
        informe
            .valores_invalidos
            .iter()
            .any(|v| v.columna == "doc_id_provisional" && v.valor == carga),
        "{:?}",
        informe.valores_invalidos
    );
    // La carga no llega a ejecutarse: todas las tablas siguen legibles
    assert!(
        store
            .contar_registros(TABLA_DOCUMENTOS_CATALOGO)
            .await
            .is_ok()
    );
}

// ---------- manifiesto ----------

#[test]
fn test_centro_con_esquema_anterior_se_rechaza_sin_migracion() {
    let modelo = ModeloEmbeddingsActivo::new("MultilingualE5Large", 1024);
    let mut manifiesto = ManifiestoCentroDisco::nuevo("obra-a", "Obra A", &modelo, "x".into());
    assert_eq!(manifiesto.schema_version, VERSION_ESQUEMA_CENTRO);
    assert!(manifiesto.validar_compatibilidad(&modelo).is_ok());
    for version in [0, 1, 2, VERSION_ESQUEMA_CENTRO + 1] {
        manifiesto.schema_version = version;
        let error = manifiesto
            .validar_compatibilidad(&modelo)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(&format!("schema_version = {version}")),
            "{error}"
        );
        assert!(error.contains("no hay migración"), "{error}");
        assert!(error.contains("reingiere"), "{error}");
    }
}
