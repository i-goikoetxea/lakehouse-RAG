//! Adaptador de almacenamiento columnar transaccional sobre LanceDB (.lance) y Apache Arrow.
//!
//! LanceDB no ofrece transacciones entre tablas, así que la atomicidad de una ingesta se
//! construye con las columnas `ingest_id` y `estado_ingesta`:
//! - las filas se escriben `STAGED` y no vigentes; las lecturas normales solo ven `COMMITTED`;
//! - `confirmar_ingesta` activa primero las tablas 02-07, retira después la vigencia previa y
//!   activa la fila de 01 en último lugar (01 `COMMITTED` es la marca de confirmación);
//! - si la activación falla, lo activado vuelve a `STAGED` antes de devolver el error;
//! - si la retirada falla, se compensa reactivando la revisión previa antes de devolver el error;
//! - `revertir_ingesta` borra todo lo que lleve ese `ingest_id` mientras no esté confirmado.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{Array, BooleanArray, Date32Array, Int32Array, RecordBatch, StringArray};
use async_trait::async_trait;
use chrono::NaiveDate;
use futures::TryStreamExt;
use lancedb::Table;
use lancedb::connection::Connection;
use lancedb::query::{ExecutableQuery, QueryBase, Select};

use crate::application::dtos::lakehouse_filas::{
    ClaveDocumental, LoteIngesta, validar_estructura_stair,
};
use crate::application::ports::lakehouse_store::{
    ConsultaTabla, EstadoIngesta, FilasHuerfanas, HuellaIngerida, InformeIntegridad,
    LIMITE_MAXIMO_LECTURA, LakehouseStorePort, NodosHuerfanos, ResumenConfirmacion,
    TABLA_CHUNKS_NORMATIVA, TABLA_DOCUMENTOS_CATALOGO, TABLA_INDICE_JERARQUICO,
    TABLA_MATRIZ_OPERATIVA, TABLAS_DEPENDIENTES, TODAS_LAS_TABLAS, ValorInvalido,
    VersionesVigentesMultiples,
};
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::document::DocId;
use crate::domain::models::estructura_documental::{EstadoContenido, TipoBloque, TipoNodoIndice};
use crate::domain::models::identificadores::IngestId;
use crate::domain::models::operational_matrix::OrigenResponsable;
use crate::domain::models::vigencia::VentanaVigencia;
use crate::infrastructure::storage::lotes_arrow::{ColumnasTransaccionales, lote_a_record_batch};
use crate::infrastructure::storage::schemas::{
    COLUMNA_CENTRO_ID, COLUMNA_DOC_ID, COLUMNA_DOC_ID_PROVISIONAL, COLUMNA_ESTADO_CONTENIDO,
    COLUMNA_ESTADO_INGESTA, COLUMNA_INGEST_ID, COLUMNA_NODO_ID, COLUMNA_NODO_PADRE_ID,
    COLUMNA_TIPO_BLOQUE, COLUMNA_TIPO_NODO, COLUMNA_VERSION_NUM, COLUMNA_VIGENTE,
    obtener_esquema_por_nombre,
};

/// Longitud de un SHA-256 en hexadecimal
const LONGITUD_SHA256_HEX: usize = 64;

/// Literal SQL entre comillas simples con las comillas internas duplicadas.
/// Los valores ya llegan validados (`DocId`, `CenterId`, `IngestId`); escapar es la segunda barrera.
pub fn literal_sql(valor: &str) -> String {
    format!("'{}'", valor.replace('\'', "''"))
}

/// Revisión del catálogo 01 tal y como está guardada
#[derive(Debug, Clone)]
struct RevisionCatalogo {
    centro_id: String,
    doc_id: String,
    version_num: i32,
    ingest_id: String,
    estado: String,
    vigente: bool,
    fecha_vigor: Option<NaiveDate>,
    doc_id_provisional: Option<String>,
}

/// Columnas comunes leídas de una fila cualquiera (para el diagnóstico de integridad)
#[derive(Debug, Clone)]
struct ColumnasLinaje {
    centro_id: String,
    doc_id: String,
    version_num: i32,
    vigente: bool,
    ingest_id: String,
    estado: String,
}

/// Revisión a la que pertenece una fila: `(centro_id, doc_id, version_num)`
type ClaveRevision = (String, String, i32);
/// Revisión de una fila y valores de las columnas pedidas (`None` si son nulos)
type FilaTextos = (ClaveRevision, Vec<Option<String>>);
/// Tabla, columna y validador de su vocabulario
type ComprobacionVocabulario = (&'static str, &'static str, fn(&str) -> bool);

/// Adaptador que gestiona las 7 tablas columnares en LanceDB
pub struct LanceDbStore {
    base_dir: PathBuf,
    connection: Arc<Connection>,
    init_lock: tokio::sync::Mutex<()>,
    /// Serializa escrituras, confirmaciones y reversiones del mismo almacén
    transaccion_lock: tokio::sync::Mutex<()>,
}

impl LanceDbStore {
    /// Conecta o crea el almacén LanceDB en la ruta de directorio indicada
    pub async fn conectar(base_dir: impl AsRef<Path>) -> Result<Self, DomainError> {
        let path = base_dir.as_ref().to_path_buf();
        let path_str = path.to_str().ok_or_else(|| {
            DomainError::StorageError("Ruta de almacenamiento inválida".to_string())
        })?;

        let connection = lancedb::connect(path_str)
            .execute()
            .await
            .map_err(|e| DomainError::StorageError(format!("Error conectando a LanceDB: {e}")))?;

        Ok(Self {
            base_dir: path,
            connection: Arc::new(connection),
            init_lock: tokio::sync::Mutex::new(()),
            transaccion_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// Obtiene una referencia a la tabla abierta
    pub async fn abrir_tabla(&self, nombre_tabla: &str) -> Result<Table, DomainError> {
        self.connection
            .open_table(nombre_tabla)
            .execute()
            .await
            .map_err(|e| {
                DomainError::StorageError(format!("Error abriendo tabla '{nombre_tabla}': {e}"))
            })
    }

    /// Retorna la ruta base del almacén
    pub fn ruta_base(&self) -> &Path {
        &self.base_dir
    }

    /// Lectura de diagnóstico: todas las filas físicas (cualquier estado) que cumplen el filtro.
    /// Pensada para pruebas, auditoría y `verificar_integridad`; las consultas normales usan `leer_tabla`.
    pub async fn leer_filas_crudas(
        &self,
        nombre_tabla: &str,
        filtro_sql: Option<&str>,
    ) -> Result<Vec<RecordBatch>, DomainError> {
        let limite = self.contar_registros(nombre_tabla).await?.max(1);
        self.leer_lotes(nombre_tabla, filtro_sql, limite, None)
            .await
    }

    /// Inserta filas con columnas transaccionales arbitrarias, sin pasar por staged/commit.
    /// Solo para sembrar fixtures y escenarios de prueba (p. ej. huérfanos).
    pub async fn sembrar_filas(
        &self,
        lote: &LoteIngesta,
        transaccion: &ColumnasTransaccionales,
    ) -> Result<usize, DomainError> {
        self.anadir_lote(lote, transaccion).await
    }

    async fn anadir_lote(
        &self,
        lote: &LoteIngesta,
        transaccion: &ColumnasTransaccionales,
    ) -> Result<usize, DomainError> {
        if lote.is_empty() {
            return Ok(0);
        }
        let nombre_tabla = nombre_tabla_de(lote);
        let batch = lote_a_record_batch(lote, transaccion)?;
        let filas = batch.num_rows();
        let tabla = self.abrir_tabla(nombre_tabla).await?;
        let esquema = batch.schema();
        let iter = arrow_array::RecordBatchIterator::new(vec![Ok(batch)], esquema);
        let reader: Box<dyn arrow_array::RecordBatchReader + Send> = Box::new(iter);
        tabla.add(reader).execute().await.map_err(|e| {
            DomainError::StorageError(format!("Error insertando lote en '{nombre_tabla}': {e}"))
        })?;
        Ok(filas)
    }

    async fn leer_lotes(
        &self,
        nombre_tabla: &str,
        filtro_sql: Option<&str>,
        limite: usize,
        columnas: Option<&[&str]>,
    ) -> Result<Vec<RecordBatch>, DomainError> {
        let tabla = self.abrir_tabla(nombre_tabla).await?;
        // O4: `Query::new` de lancedb 0.15 limita a 10 filas si no se fija `.limit()`.
        let mut consulta = tabla.query().limit(limite);
        if let Some(filtro) = filtro_sql {
            consulta = consulta.only_if(filtro);
        }
        if let Some(seleccion) = columnas {
            consulta = consulta.select(Select::columns(seleccion));
        }
        let flujo = consulta.execute().await.map_err(|e| {
            DomainError::StorageError(format!("Error consultando '{nombre_tabla}': {e}"))
        })?;
        flujo.try_collect().await.map_err(|e| {
            DomainError::StorageError(format!("Error leyendo lotes de '{nombre_tabla}': {e}"))
        })
    }

    async fn contar(&self, nombre_tabla: &str, filtro_sql: &str) -> Result<usize, DomainError> {
        let tabla = self.abrir_tabla(nombre_tabla).await?;
        tabla
            .count_rows(Some(filtro_sql.to_string()))
            .await
            .map_err(|e| {
                DomainError::StorageError(format!("Error contando filas en '{nombre_tabla}': {e}"))
            })
    }

    async fn actualizar(
        &self,
        nombre_tabla: &str,
        filtro_sql: &str,
        asignaciones: &[(&str, String)],
    ) -> Result<u64, DomainError> {
        let tabla = self.abrir_tabla(nombre_tabla).await?;
        let mut actualizacion = tabla.update().only_if(filtro_sql);
        for (columna, expresion) in asignaciones {
            actualizacion = actualizacion.column(*columna, expresion.as_str());
        }
        actualizacion.execute().await.map_err(|e| {
            DomainError::StorageError(format!("Error actualizando '{nombre_tabla}': {e}"))
        })
    }

    async fn leer_catalogo(&self, filtro_sql: &str) -> Result<Vec<RevisionCatalogo>, DomainError> {
        let lotes = self
            .leer_filas_crudas(TABLA_DOCUMENTOS_CATALOGO, Some(filtro_sql))
            .await?;
        let mut revisiones = Vec::new();
        for lote in &lotes {
            let linaje = extraer_linaje(lote)?;
            let fechas = columna::<Date32Array>(lote, "fecha_vigor")?;
            let provisionales = columna::<StringArray>(lote, COLUMNA_DOC_ID_PROVISIONAL)?;
            for (fila, columnas) in linaje.into_iter().enumerate() {
                let fecha_vigor = fechas
                    .is_valid(fila)
                    .then(|| {
                        fechas.value_as_date(fila).ok_or_else(|| {
                            DomainError::StorageError(
                                "fecha_vigor fuera de rango en 01".to_string(),
                            )
                        })
                    })
                    .transpose()?;
                revisiones.push(RevisionCatalogo {
                    centro_id: columnas.centro_id,
                    doc_id: columnas.doc_id,
                    version_num: columnas.version_num,
                    ingest_id: columnas.ingest_id,
                    estado: columnas.estado,
                    vigente: columnas.vigente,
                    fecha_vigor,
                    doc_id_provisional: provisionales
                        .is_valid(fila)
                        .then(|| provisionales.value(fila).to_string()),
                });
            }
        }
        Ok(revisiones)
    }

    /// Fila 01 de la ingesta que se va a confirmar, con sus precondiciones
    async fn revision_staged(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
    ) -> Result<RevisionCatalogo, DomainError> {
        let filtro = format!("{COLUMNA_INGEST_ID} = {}", literal_sql(ingest_id.as_str()));
        let mut revisiones = self.leer_catalogo(&filtro).await?;
        let revision = match revisiones.len() {
            0 => {
                return Err(DomainError::ValidationError(format!(
                    "No existe una ingesta '{ingest_id}' con catálogo en 01"
                )));
            }
            1 => revisiones.remove(0),
            n => {
                return Err(DomainError::StorageError(format!(
                    "La ingesta '{ingest_id}' tiene {n} filas en 01 (se esperaba 1)"
                )));
            }
        };
        if revision.estado == EstadoIngesta::Committed.as_str() {
            return Err(DomainError::ValidationError(format!(
                "La ingesta '{ingest_id}' ya está confirmada"
            )));
        }
        if revision.doc_id != doc_id.as_str() {
            return Err(DomainError::ValidationError(format!(
                "La ingesta '{ingest_id}' pertenece a '{}', no a '{doc_id}'",
                revision.doc_id
            )));
        }
        Ok(revision)
    }

    /// Revisiones confirmadas del mismo documento y centro; valida orden de versiones y fechas.
    /// La revisión sustituida no cuenta: va a desaparecer en esta misma confirmación.
    async fn revisiones_previas_vigentes(
        &self,
        nueva: &RevisionCatalogo,
        sustituida: Option<&RevisionCatalogo>,
    ) -> Result<Vec<RevisionCatalogo>, DomainError> {
        let filtro = format!(
            "{COLUMNA_CENTRO_ID} = {} AND {COLUMNA_DOC_ID} = {} AND {COLUMNA_ESTADO_INGESTA} = {}",
            literal_sql(&nueva.centro_id),
            literal_sql(&nueva.doc_id),
            literal_sql(EstadoIngesta::Committed.as_str())
        );
        let mut confirmadas = self.leer_catalogo(&filtro).await?;
        confirmadas.retain(|r| sustituida.is_none_or(|s| s.ingest_id != r.ingest_id));
        if let Some(maxima) = confirmadas.iter().map(|r| r.version_num).max()
            && maxima >= nueva.version_num
        {
            return Err(DomainError::VersionConflict(format!(
                "{} (versión nueva {} ≤ versión confirmada {maxima})",
                nueva.doc_id, nueva.version_num
            )));
        }
        // Una revisión sin fecha no se compara: el orden entre ellas lo da version_num. Se compara
        // con todas las confirmadas para no saltarse una paradoja a través de una sin fecha.
        if let Some(fecha_nueva) = nueva.fecha_vigor {
            for fecha_previa in confirmadas.iter().filter_map(|r| r.fecha_vigor) {
                VentanaVigencia::new(fecha_previa, Some(fecha_nueva))?;
            }
        }
        Ok(confirmadas.into_iter().filter(|r| r.vigente).collect())
    }

    async fn activar_ingesta(
        &self,
        tablas: &[&str],
        ingest_id: &IngestId,
    ) -> Result<u64, DomainError> {
        let filtro = format!("{COLUMNA_INGEST_ID} = {}", literal_sql(ingest_id.as_str()));
        let asignaciones = [
            (COLUMNA_VIGENTE, "true".to_string()),
            (
                COLUMNA_ESTADO_INGESTA,
                literal_sql(EstadoIngesta::Committed.as_str()),
            ),
        ];
        let mut activadas = 0;
        for tabla in tablas {
            activadas += self.actualizar(tabla, &filtro, &asignaciones).await?;
        }
        Ok(activadas)
    }

    /// Compensación de una activación a medias: las filas vuelven a `STAGED` y no vigentes para
    /// que las lecturas no vean una revisión parcial. Sigue tras un fallo (la tabla rota nunca
    /// llegó a activarse) y devuelve el primero.
    async fn desactivar_ingesta(&self, ingest_id: &IngestId) -> Result<(), DomainError> {
        let filtro = format!("{COLUMNA_INGEST_ID} = {}", literal_sql(ingest_id.as_str()));
        let asignaciones = [
            (COLUMNA_VIGENTE, "false".to_string()),
            (
                COLUMNA_ESTADO_INGESTA,
                literal_sql(EstadoIngesta::Staged.as_str()),
            ),
        ];
        let mut primer_error = None;
        for tabla in TABLAS_DEPENDIENTES {
            if let Err(error) = self.actualizar(tabla, &filtro, &asignaciones).await {
                primer_error.get_or_insert(error);
            }
        }
        primer_error.map_or(Ok(()), Err)
    }

    async fn retirar_vigencia_previa(
        &self,
        nueva: &RevisionCatalogo,
        ingest_id: &IngestId,
    ) -> Result<u64, DomainError> {
        let filtro = format!(
            "{COLUMNA_CENTRO_ID} = {} AND {COLUMNA_DOC_ID} = {} AND {COLUMNA_VIGENTE} = true AND {COLUMNA_INGEST_ID} <> {}",
            literal_sql(&nueva.centro_id),
            literal_sql(&nueva.doc_id),
            literal_sql(ingest_id.as_str())
        );
        let mut retiradas = 0;
        for tabla in TABLAS_DEPENDIENTES {
            retiradas += self
                .actualizar(tabla, &filtro, &[(COLUMNA_VIGENTE, "false".to_string())])
                .await?;
        }
        let derogacion = nueva.fecha_vigor.map_or_else(
            || "CAST(NULL AS DATE)".to_string(),
            |fecha| format!("CAST('{}' AS DATE)", fecha.format("%Y-%m-%d")),
        );
        retiradas += self
            .actualizar(
                TABLA_DOCUMENTOS_CATALOGO,
                &filtro,
                &[
                    (COLUMNA_VIGENTE, "false".to_string()),
                    ("fecha_derogacion", derogacion),
                ],
            )
            .await?;
        Ok(retiradas)
    }

    /// Compensación: devuelve la vigencia a las revisiones previas si la confirmación falla a medias
    async fn restaurar_vigencia(&self, previas: &[RevisionCatalogo]) -> Result<(), DomainError> {
        if previas.is_empty() {
            return Ok(());
        }
        let lista: Vec<String> = previas.iter().map(|r| literal_sql(&r.ingest_id)).collect();
        let filtro = format!("{COLUMNA_INGEST_ID} IN ({})", lista.join(", "));
        for tabla in TABLAS_DEPENDIENTES {
            self.actualizar(tabla, &filtro, &[(COLUMNA_VIGENTE, "true".to_string())])
                .await?;
        }
        self.actualizar(
            TABLA_DOCUMENTOS_CATALOGO,
            &filtro,
            &[
                (COLUMNA_VIGENTE, "true".to_string()),
                ("fecha_derogacion", "CAST(NULL AS DATE)".to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    /// Retira la vigencia de la revisión sustituida, que puede tener otro `doc_id`
    async fn retirar_sustituida(&self, sustituida: &RevisionCatalogo) -> Result<u64, DomainError> {
        let filtro = format!(
            "{COLUMNA_INGEST_ID} = {}",
            literal_sql(&sustituida.ingest_id)
        );
        let mut retiradas = 0;
        for tabla in TODAS_LAS_TABLAS {
            retiradas += self
                .actualizar(tabla, &filtro, &[(COLUMNA_VIGENTE, "false".to_string())])
                .await?;
        }
        Ok(retiradas)
    }

    async fn retirar_y_marcar(
        &self,
        nueva: &RevisionCatalogo,
        ingest_id: &IngestId,
        sustituida: Option<&RevisionCatalogo>,
    ) -> Result<(u64, u64), DomainError> {
        let mut retiradas = self.retirar_vigencia_previa(nueva, ingest_id).await?;
        if let Some(sustituida) = sustituida {
            retiradas += self.retirar_sustituida(sustituida).await?;
        }
        let marca = self
            .activar_ingesta(&[TABLA_DOCUMENTOS_CATALOGO], ingest_id)
            .await?;
        Ok((retiradas, marca))
    }

    /// Borra de todas las tablas las filas de una ingesta. Devuelve las filas borradas.
    async fn borrar_filas_de_ingesta(&self, ingest_id: &str) -> Result<u64, DomainError> {
        let filtro = format!("{COLUMNA_INGEST_ID} = {}", literal_sql(ingest_id));
        let mut borradas = 0u64;
        for nombre_tabla in TODAS_LAS_TABLAS {
            let filas = self.contar(nombre_tabla, &filtro).await?;
            if filas == 0 {
                continue;
            }
            let tabla = self.abrir_tabla(nombre_tabla).await?;
            tabla.delete(&filtro).await.map_err(|e| {
                DomainError::StorageError(format!(
                    "Error revirtiendo '{ingest_id}' en '{nombre_tabla}': {e}"
                ))
            })?;
            borradas += filas as u64;
        }
        Ok(borradas)
    }

    /// Confirmación común; `sustituida` es la revisión provisional que desaparece con esta
    async fn confirmar(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
        sustituida: Option<&RevisionCatalogo>,
    ) -> Result<ResumenConfirmacion, DomainError> {
        let nueva = self.revision_staged(ingest_id, doc_id).await?;
        let previas = self.revisiones_previas_vigentes(&nueva, sustituida).await?;

        let activadas = match self.activar_ingesta(&TABLAS_DEPENDIENTES, ingest_id).await {
            Ok(activadas) => activadas,
            Err(error) => {
                let detalle = match self.desactivar_ingesta(ingest_id).await {
                    Ok(()) => "filas devueltas a STAGED".to_string(),
                    Err(e) => format!("la compensación no pudo completarse: {e}"),
                };
                return Err(DomainError::StorageError(format!(
                    "Error activando la ingesta '{ingest_id}': {error} ({detalle})"
                )));
            }
        };
        let (retiradas, marca) = match self.retirar_y_marcar(&nueva, ingest_id, sustituida).await {
            Ok(resultado) => resultado,
            Err(error) => {
                let a_restaurar: Vec<RevisionCatalogo> =
                    previas.iter().chain(sustituida).cloned().collect();
                let detalle = match self.restaurar_vigencia(&a_restaurar).await {
                    Ok(()) => "vigencia previa restaurada".to_string(),
                    Err(e) => format!("además falló la compensación: {e}"),
                };
                return Err(DomainError::StorageError(format!(
                    "Error confirmando la ingesta '{ingest_id}': {error} ({detalle})"
                )));
            }
        };

        let mut versiones_derogadas: Vec<u32> = previas
            .iter()
            .filter_map(|r| u32::try_from(r.version_num).ok())
            .collect();
        versiones_derogadas.sort_unstable();
        Ok(ResumenConfirmacion {
            filas_activadas: activadas + marca,
            filas_retiradas: retiradas,
            versiones_derogadas,
            aviso_limpieza: None,
        })
    }

    async fn leer_linaje_tabla(
        &self,
        nombre_tabla: &str,
    ) -> Result<Vec<ColumnasLinaje>, DomainError> {
        let limite = self.contar_registros(nombre_tabla).await?.max(1);
        let columnas = [
            COLUMNA_CENTRO_ID,
            COLUMNA_DOC_ID,
            COLUMNA_VERSION_NUM,
            COLUMNA_VIGENTE,
            COLUMNA_INGEST_ID,
            COLUMNA_ESTADO_INGESTA,
        ];
        let lotes = self
            .leer_lotes(nombre_tabla, None, limite, Some(&columnas))
            .await?;
        let mut filas = Vec::new();
        for lote in &lotes {
            filas.extend(extraer_linaje(lote)?);
        }
        Ok(filas)
    }

    /// Revisión de cada fila y valores de las columnas de texto pedidas (`None` si son nulos)
    async fn leer_columnas_texto(
        &self,
        nombre_tabla: &str,
        columnas: &[&str],
    ) -> Result<Vec<FilaTextos>, DomainError> {
        let limite = self.contar_registros(nombre_tabla).await?.max(1);
        let mut seleccion = vec![COLUMNA_CENTRO_ID, COLUMNA_DOC_ID, COLUMNA_VERSION_NUM];
        seleccion.extend_from_slice(columnas);
        let lotes = self
            .leer_lotes(nombre_tabla, None, limite, Some(&seleccion))
            .await?;
        let mut filas = Vec::new();
        for lote in &lotes {
            let centros = columna::<StringArray>(lote, COLUMNA_CENTRO_ID)?;
            let docs = columna::<StringArray>(lote, COLUMNA_DOC_ID)?;
            let versiones = columna::<Int32Array>(lote, COLUMNA_VERSION_NUM)?;
            let textos = columnas
                .iter()
                .map(|c| columna::<StringArray>(lote, c))
                .collect::<Result<Vec<_>, _>>()?;
            for fila in 0..lote.num_rows() {
                let revision = (
                    centros.value(fila).to_string(),
                    docs.value(fila).to_string(),
                    versiones.value(fila),
                );
                let valores = textos
                    .iter()
                    .map(|t| t.is_valid(fila).then(|| t.value(fila).to_string()))
                    .collect();
                filas.push((revision, valores));
            }
        }
        Ok(filas)
    }

    /// Chunks cuyo `nodo_id` falta en 07 y nodos cuyo padre falta, dentro de la misma revisión.
    /// Una revisión sin ninguna fila en 07 también se informa: toda ingesta escribe 07.
    async fn diagnosticar_nodos_huerfanos(&self) -> Result<Vec<NodosHuerfanos>, DomainError> {
        let nodos = self
            .leer_columnas_texto(
                TABLA_INDICE_JERARQUICO,
                &[COLUMNA_NODO_ID, COLUMNA_NODO_PADRE_ID],
            )
            .await?;
        let chunks = self
            .leer_columnas_texto(TABLA_CHUNKS_NORMATIVA, &[COLUMNA_NODO_ID])
            .await?;
        let mut por_revision: HashMap<&ClaveRevision, HashSet<&str>> = HashMap::new();
        for (revision, valores) in &nodos {
            por_revision
                .entry(revision)
                .or_default()
                .extend(valores[0].as_deref());
        }
        let referencias = chunks
            .iter()
            .map(|(r, v)| (TABLA_CHUNKS_NORMATIVA, r, v[0].as_deref()))
            .chain(
                nodos
                    .iter()
                    .map(|(r, v)| (TABLA_INDICE_JERARQUICO, r, v[1].as_deref())),
            );
        let mut ausentes: BTreeMap<(&str, &ClaveRevision, &str), usize> = BTreeMap::new();
        for (tabla, revision, nodo) in referencias {
            let Some(nodo) = nodo else { continue };
            if !por_revision
                .get(revision)
                .is_some_and(|existentes| existentes.contains(nodo))
            {
                *ausentes.entry((tabla, revision, nodo)).or_default() += 1;
            }
        }
        Ok(ausentes
            .into_iter()
            .map(|((tabla, revision, nodo), filas)| NodosHuerfanos {
                tabla: tabla.to_string(),
                centro_id: revision.0.clone(),
                doc_id: revision.1.clone(),
                version_num: revision.2,
                nodo_id: nodo.to_string(),
                filas,
            })
            .collect())
    }

    /// Valores fuera del vocabulario de su columna (un `.lance` manipulado o escrito a mano)
    async fn diagnosticar_valores_invalidos(&self) -> Result<Vec<ValorInvalido>, DomainError> {
        let mut comprobaciones: Vec<ComprobacionVocabulario> = TODAS_LAS_TABLAS
            .iter()
            .map(|t| {
                (
                    *t,
                    COLUMNA_ESTADO_INGESTA,
                    estado_ingesta_valido as fn(&str) -> bool,
                )
            })
            .collect();
        comprobaciones.extend([
            (
                TABLA_CHUNKS_NORMATIVA,
                COLUMNA_ESTADO_CONTENIDO,
                estado_contenido_valido as fn(&str) -> bool,
            ),
            (
                TABLA_CHUNKS_NORMATIVA,
                COLUMNA_TIPO_BLOQUE,
                tipo_bloque_valido,
            ),
            (
                TABLA_INDICE_JERARQUICO,
                COLUMNA_ESTADO_CONTENIDO,
                estado_contenido_valido,
            ),
            (TABLA_INDICE_JERARQUICO, COLUMNA_TIPO_NODO, tipo_nodo_valido),
            (
                TABLA_DOCUMENTOS_CATALOGO,
                COLUMNA_DOC_ID_PROVISIONAL,
                doc_id_valido,
            ),
            (
                TABLA_MATRIZ_OPERATIVA,
                "origen_responsable",
                origen_responsable_valido,
            ),
        ]);
        let mut invalidos = Vec::new();
        for (tabla, nombre_columna, es_valido) in comprobaciones {
            let filas = self.leer_columnas_texto(tabla, &[nombre_columna]).await?;
            let mut cuentas: BTreeMap<String, usize> = BTreeMap::new();
            for valor in filas.into_iter().filter_map(|(_, mut v)| v.remove(0)) {
                if !es_valido(&valor) {
                    *cuentas.entry(valor).or_default() += 1;
                }
            }
            invalidos.extend(cuentas.into_iter().map(|(valor, filas)| ValorInvalido {
                tabla: tabla.to_string(),
                columna: nombre_columna.to_string(),
                valor,
                filas,
            }));
        }
        Ok(invalidos)
    }
}

fn origen_responsable_valido(valor: &str) -> bool {
    [
        OrigenResponsable::Declarado,
        OrigenResponsable::Inferido,
        OrigenResponsable::NoConsta,
    ]
    .iter()
    .any(|origen| origen.as_str() == valor)
}

fn estado_ingesta_valido(valor: &str) -> bool {
    valor == EstadoIngesta::Staged.as_str() || valor == EstadoIngesta::Committed.as_str()
}

fn tipo_nodo_valido(valor: &str) -> bool {
    TipoNodoIndice::parse(valor).is_ok()
}

fn estado_contenido_valido(valor: &str) -> bool {
    EstadoContenido::parse(valor).is_ok()
}

fn tipo_bloque_valido(valor: &str) -> bool {
    TipoBloque::parse(valor).is_ok()
}

/// `DocId::new` recorta espacios: el valor guardado debe coincidir exactamente con el validado
fn doc_id_valido(valor: &str) -> bool {
    DocId::new(valor).is_ok_and(|d| d.as_str() == valor)
}

fn nombre_tabla_de(lote: &LoteIngesta) -> &'static str {
    use crate::application::ports::lakehouse_store::{
        TABLA_CATALOGO_TABLAS, TABLA_CELDAS_DOBLE_ENTRADA, TABLA_GRAFO_REFERENCIAS,
        TABLA_MATRIZ_OPERATIVA,
    };
    match lote {
        LoteIngesta::Catalogo(_) => TABLA_DOCUMENTOS_CATALOGO,
        LoteIngesta::Chunks(_) => TABLA_CHUNKS_NORMATIVA,
        LoteIngesta::Hitos(_) => TABLA_MATRIZ_OPERATIVA,
        LoteIngesta::Tablas(_) => TABLA_CATALOGO_TABLAS,
        LoteIngesta::Celdas(_) => TABLA_CELDAS_DOBLE_ENTRADA,
        LoteIngesta::Aristas(_) => TABLA_GRAFO_REFERENCIAS,
        LoteIngesta::Indice(_) => TABLA_INDICE_JERARQUICO,
    }
}

fn columna<'a, T: 'static>(lote: &'a RecordBatch, nombre: &str) -> Result<&'a T, DomainError> {
    lote.column_by_name(nombre)
        .and_then(|c| c.as_any().downcast_ref::<T>())
        .ok_or_else(|| {
            DomainError::StorageError(format!("Columna '{nombre}' ausente o con tipo inesperado"))
        })
}

fn extraer_linaje(lote: &RecordBatch) -> Result<Vec<ColumnasLinaje>, DomainError> {
    let centros = columna::<StringArray>(lote, COLUMNA_CENTRO_ID)?;
    let docs = columna::<StringArray>(lote, COLUMNA_DOC_ID)?;
    let versiones = columna::<Int32Array>(lote, COLUMNA_VERSION_NUM)?;
    let vigentes = columna::<BooleanArray>(lote, COLUMNA_VIGENTE)?;
    let ingestas = columna::<StringArray>(lote, COLUMNA_INGEST_ID)?;
    let estados = columna::<StringArray>(lote, COLUMNA_ESTADO_INGESTA)?;
    Ok((0..lote.num_rows())
        .map(|fila| ColumnasLinaje {
            centro_id: centros.value(fila).to_string(),
            doc_id: docs.value(fila).to_string(),
            version_num: versiones.value(fila),
            vigente: vigentes.value(fila),
            ingest_id: ingestas.value(fila).to_string(),
            estado: estados.value(fila).to_string(),
        })
        .collect())
}

fn validar_hash(hash_sha256: &str) -> Result<(), DomainError> {
    let es_hex = hash_sha256.chars().all(|c| c.is_ascii_hexdigit());
    if hash_sha256.len() != LONGITUD_SHA256_HEX || !es_hex {
        return Err(DomainError::ValidationError(
            "El hash SHA-256 debe tener 64 caracteres hexadecimales".to_string(),
        ));
    }
    Ok(())
}

fn validar_tabla(nombre_tabla: &str) -> Result<(), DomainError> {
    if TODAS_LAS_TABLAS.contains(&nombre_tabla) {
        Ok(())
    } else {
        Err(DomainError::ValidationError(format!(
            "Tabla desconocida: '{nombre_tabla}'"
        )))
    }
}

/// Todas las filas de una escritura staged deben pertenecer a una única revisión
fn clave_unica(lotes: &[LoteIngesta]) -> Result<ClaveDocumental, DomainError> {
    let claves: HashSet<&ClaveDocumental> = lotes.iter().flat_map(|l| l.claves()).collect();
    match claves.len() {
        0 => Err(DomainError::ValidationError(
            "La escritura staged no contiene filas".to_string(),
        )),
        1 => Ok(claves.into_iter().next().cloned().expect("una clave")),
        n => Err(DomainError::ValidationError(format!(
            "Una ingesta solo puede escribir una revisión; se recibieron {n} claves documentales"
        ))),
    }
}

fn filtro_lectura_normal(consulta: &ConsultaTabla) -> String {
    let mut condiciones = vec![format!(
        "{COLUMNA_ESTADO_INGESTA} = {}",
        literal_sql(EstadoIngesta::Committed.as_str())
    )];
    if consulta.solo_vigentes {
        condiciones.push(format!("{COLUMNA_VIGENTE} = true"));
    }
    if let Some(centro) = &consulta.centro_id {
        condiciones.push(format!(
            "{COLUMNA_CENTRO_ID} = {}",
            literal_sql(centro.as_str())
        ));
    }
    if let Some(doc) = &consulta.doc_id {
        condiciones.push(format!("{COLUMNA_DOC_ID} = {}", literal_sql(doc.as_str())));
    }
    condiciones.join(" AND ")
}

#[async_trait]
impl LakehouseStorePort for LanceDbStore {
    async fn inicializar_tablas(&self) -> Result<(), DomainError> {
        let _guard = self.init_lock.lock().await;
        let tablas_existentes = self.listar_tablas().await?;

        for nombre_tabla in TODAS_LAS_TABLAS {
            if tablas_existentes.iter().any(|t| t == nombre_tabla) {
                continue;
            }
            let esquema = obtener_esquema_por_nombre(nombre_tabla).ok_or_else(|| {
                DomainError::StorageError(format!(
                    "Esquema no definido para la tabla '{nombre_tabla}'"
                ))
            })?;

            let res = self
                .connection
                .create_empty_table(nombre_tabla, esquema)
                .execute()
                .await;

            if let Err(e) = res {
                let msg = e.to_string();
                if !msg.contains("already exists") && !msg.contains("AlreadyExists") {
                    return Err(DomainError::StorageError(format!(
                        "Error creando tabla vacía '{nombre_tabla}': {e}"
                    )));
                }
            }
        }

        Ok(())
    }

    async fn existe_hash(
        &self,
        centro_id: &CenterId,
        hash_sha256: &str,
    ) -> Result<Option<HuellaIngerida>, DomainError> {
        validar_hash(hash_sha256)?;
        let filtro = format!(
            "{COLUMNA_CENTRO_ID} = {} AND hash_sha256 = {} AND {COLUMNA_ESTADO_INGESTA} = {}",
            literal_sql(centro_id.as_str()),
            literal_sql(hash_sha256),
            literal_sql(EstadoIngesta::Committed.as_str())
        );
        let revisiones = self.leer_catalogo(&filtro).await?;
        // Una revisión sustituida cuya limpieza falló queda no vigente: gana la vigente
        let Some(revision) = revisiones
            .into_iter()
            .max_by_key(|r| (r.vigente, r.version_num))
        else {
            return Ok(None);
        };
        let es_provisional = revision.doc_id_provisional.as_deref() == Some(&revision.doc_id);
        Ok(Some(HuellaIngerida {
            es_provisional,
            doc_id: DocId::new(&revision.doc_id)?,
            version_num: u32::try_from(revision.version_num).map_err(|_| {
                DomainError::StorageError(format!(
                    "version_num negativo en 01: {}",
                    revision.version_num
                ))
            })?,
            ingest_id: IngestId::new(&revision.ingest_id)?,
        }))
    }

    async fn escribir_staged(
        &self,
        ingest_id: &IngestId,
        lotes: Vec<LoteIngesta>,
    ) -> Result<usize, DomainError> {
        clave_unica(&lotes)?;
        validar_estructura_stair(&lotes)?;
        let _guard = self.transaccion_lock.lock().await;
        let ya_confirmada = self
            .contar(
                TABLA_DOCUMENTOS_CATALOGO,
                &format!(
                    "{COLUMNA_INGEST_ID} = {} AND {COLUMNA_ESTADO_INGESTA} = {}",
                    literal_sql(ingest_id.as_str()),
                    literal_sql(EstadoIngesta::Committed.as_str())
                ),
            )
            .await?;
        if ya_confirmada > 0 {
            return Err(DomainError::ValidationError(format!(
                "La ingesta '{ingest_id}' ya está confirmada; no admite más filas"
            )));
        }
        let transaccion = ColumnasTransaccionales::staged(ingest_id);
        let mut escritas = 0;
        for lote in &lotes {
            escritas += self.anadir_lote(lote, &transaccion).await?;
        }
        Ok(escritas)
    }

    async fn confirmar_ingesta(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
    ) -> Result<ResumenConfirmacion, DomainError> {
        let _guard = self.transaccion_lock.lock().await;
        self.confirmar(ingest_id, doc_id, None).await
    }

    async fn confirmar_sustitucion(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
        ingest_sustituida: &IngestId,
    ) -> Result<ResumenConfirmacion, DomainError> {
        let _guard = self.transaccion_lock.lock().await;
        let filtro = format!(
            "{COLUMNA_INGEST_ID} = {} AND {COLUMNA_ESTADO_INGESTA} = {}",
            literal_sql(ingest_sustituida.as_str()),
            literal_sql(EstadoIngesta::Committed.as_str())
        );
        let sustituida = self
            .leer_catalogo(&filtro)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                DomainError::ValidationError(format!(
                    "No existe una revisión confirmada con la ingesta '{ingest_sustituida}'"
                ))
            })?;
        let mut resumen = self.confirmar(ingest_id, doc_id, Some(&sustituida)).await?;
        if let Err(error) = self.borrar_filas_de_ingesta(&sustituida.ingest_id).await {
            resumen.aviso_limpieza = Some(format!(
                "Revisión confirmada, pero quedan filas no vigentes de '{ingest_sustituida}': {error}"
            ));
        }
        Ok(resumen)
    }

    async fn revertir_ingesta(&self, ingest_id: &IngestId) -> Result<u64, DomainError> {
        let _guard = self.transaccion_lock.lock().await;
        let filtro = format!("{COLUMNA_INGEST_ID} = {}", literal_sql(ingest_id.as_str()));
        let confirmada = self
            .contar(
                TABLA_DOCUMENTOS_CATALOGO,
                &format!(
                    "{filtro} AND {COLUMNA_ESTADO_INGESTA} = {}",
                    literal_sql(EstadoIngesta::Committed.as_str())
                ),
            )
            .await?;
        if confirmada > 0 {
            return Err(DomainError::ValidationError(format!(
                "La ingesta '{ingest_id}' ya está confirmada y no puede revertirse"
            )));
        }
        self.borrar_filas_de_ingesta(ingest_id.as_str()).await
    }

    async fn leer_tabla(
        &self,
        nombre_tabla: &str,
        consulta: &ConsultaTabla,
    ) -> Result<Vec<RecordBatch>, DomainError> {
        validar_tabla(nombre_tabla)?;
        if consulta.limite > LIMITE_MAXIMO_LECTURA {
            return Err(DomainError::ValidationError(format!(
                "El límite de lectura ({}) supera el máximo de {LIMITE_MAXIMO_LECTURA} filas",
                consulta.limite
            )));
        }
        if consulta.limite == 0 {
            return Ok(Vec::new());
        }
        let filtro = filtro_lectura_normal(consulta);
        self.leer_lotes(nombre_tabla, Some(&filtro), consulta.limite, None)
            .await
    }

    async fn verificar_integridad(&self) -> Result<InformeIntegridad, DomainError> {
        let mut informe = InformeIntegridad::default();
        let mut staged: BTreeSet<String> = BTreeSet::new();
        let catalogo = self.leer_linaje_tabla(TABLA_DOCUMENTOS_CATALOGO).await?;
        let revisiones: HashSet<(String, String, i32)> = catalogo
            .iter()
            .map(|f| (f.centro_id.clone(), f.doc_id.clone(), f.version_num))
            .collect();

        for nombre_tabla in TODAS_LAS_TABLAS {
            let filas = if nombre_tabla == TABLA_DOCUMENTOS_CATALOGO {
                catalogo.clone()
            } else {
                self.leer_linaje_tabla(nombre_tabla).await?
            };
            let mut huerfanas: BTreeMap<(String, String, i32), usize> = BTreeMap::new();
            let mut vigentes: BTreeMap<(String, String), BTreeSet<i32>> = BTreeMap::new();
            for fila in filas {
                if fila.estado == EstadoIngesta::Staged.as_str() {
                    staged.insert(fila.ingest_id.clone());
                }
                let revision = (
                    fila.centro_id.clone(),
                    fila.doc_id.clone(),
                    fila.version_num,
                );
                if nombre_tabla != TABLA_DOCUMENTOS_CATALOGO && !revisiones.contains(&revision) {
                    *huerfanas.entry(revision).or_default() += 1;
                }
                if fila.vigente {
                    vigentes
                        .entry((fila.centro_id, fila.doc_id))
                        .or_default()
                        .insert(fila.version_num);
                }
            }
            informe.huerfanos.extend(huerfanas.into_iter().map(
                |((centro_id, doc_id, version_num), filas)| FilasHuerfanas {
                    tabla: nombre_tabla.to_string(),
                    centro_id,
                    doc_id,
                    version_num,
                    filas,
                },
            ));
            informe.versiones_vigentes_multiples.extend(
                vigentes
                    .into_iter()
                    .filter(|(_, versiones)| versiones.len() > 1)
                    .map(
                        |((centro_id, doc_id), versiones)| VersionesVigentesMultiples {
                            tabla: nombre_tabla.to_string(),
                            centro_id,
                            doc_id,
                            versiones: versiones.into_iter().collect(),
                        },
                    ),
            );
        }
        informe.ingestas_staged = staged.into_iter().collect();
        informe.nodos_huerfanos = self.diagnosticar_nodos_huerfanos().await?;
        informe.valores_invalidos = self.diagnosticar_valores_invalidos().await?;
        Ok(informe)
    }

    async fn contar_registros(&self, nombre_tabla: &str) -> Result<usize, DomainError> {
        let tabla = self.abrir_tabla(nombre_tabla).await?;
        tabla.count_rows(None).await.map_err(|e| {
            DomainError::StorageError(format!(
                "Error contando filas en tabla '{nombre_tabla}': {e}"
            ))
        })
    }

    async fn listar_tablas(&self) -> Result<Vec<String>, DomainError> {
        self.connection
            .table_names()
            .execute()
            .await
            .map_err(|e| DomainError::StorageError(format!("Error listando tablas: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_sql_duplica_comillas() {
        assert_eq!(literal_sql("PO-ALM-03"), "'PO-ALM-03'");
        assert_eq!(literal_sql("x' OR '1'='1"), "'x'' OR ''1''=''1'");
    }

    #[tokio::test]
    async fn test_compensacion_restaura_la_vigencia_previa_y_borra_la_derogacion() {
        use crate::application::dtos::lakehouse_filas::{ClaveDocumental, FilaCatalogo};
        use crate::domain::models::document::TipoDocumento;

        let dir = tempfile::tempdir().unwrap();
        let store = LanceDbStore::conectar(dir.path()).await.unwrap();
        store.inicializar_tablas().await.unwrap();
        let doc = DocId::new("PO-ALM-03").unwrap();
        for (version, mes, hash) in [(3u32, 3u32, "a"), (4, 9, "b")] {
            let id = IngestId::new(format!("ING-V{version}")).unwrap();
            let fila = FilaCatalogo {
                clave: ClaveDocumental {
                    centro_id: CenterId::new("planta").unwrap(),
                    doc_id: doc.clone(),
                    version_num: version,
                },
                titulo: "Recepción".to_string(),
                tipo_documento: TipoDocumento::ProcedimientoObra,
                revision: format!("Rev. {version:02}"),
                vigencia: VentanaVigencia::new(
                    NaiveDate::from_ymd_opt(2026, mes, 1).unwrap(),
                    None,
                )
                .unwrap(),
                hash_sha256: hash.repeat(64),
                doc_id_provisional: None,
                paginas_degradadas: Vec::new(),
            };
            store
                .escribir_staged(&id, vec![LoteIngesta::Catalogo(vec![fila])])
                .await
                .unwrap();
            store.confirmar_ingesta(&id, &doc).await.unwrap();
        }

        let v3 = store.leer_catalogo("version_num = 3").await.unwrap();
        assert!(!v3[0].vigente);
        store.restaurar_vigencia(&v3).await.unwrap();

        let lotes = store
            .leer_filas_crudas(TABLA_DOCUMENTOS_CATALOGO, Some("version_num = 3"))
            .await
            .unwrap();
        assert!(
            columna::<BooleanArray>(&lotes[0], COLUMNA_VIGENTE)
                .unwrap()
                .value(0)
        );
        assert!(
            lotes[0]
                .column_by_name("fecha_derogacion")
                .unwrap()
                .is_null(0)
        );
    }

    #[test]
    fn test_validar_hash_exige_64_hex() {
        assert!(validar_hash(&"a".repeat(64)).is_ok());
        assert!(validar_hash("abc").is_err());
        assert!(validar_hash(&format!("{}'", "a".repeat(63))).is_err());
    }
}
