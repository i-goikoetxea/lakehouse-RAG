//! Puerto del almacén columnar del Lakehouse (LanceDB / Apache Arrow), v2 transaccional.
//!
//! Ciclo de una ingesta (plan v2, problema 8):
//! 1. `existe_hash` → idempotencia antes de gastar llamadas al LLM.
//! 2. `escribir_staged` → filas con `estado_ingesta = STAGED` y `vigente = false`.
//! 3. `confirmar_ingesta` → activa la revisión nueva y retira la vigencia de la previa en las 7 tablas.
//! 4. Ante cualquier error, `revertir_ingesta` borra todo lo escrito con ese `ingest_id`.

use arrow_array::RecordBatch;
use async_trait::async_trait;

use crate::application::dtos::lakehouse_filas::LoteIngesta;
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::document::DocId;
use crate::domain::models::identificadores::IngestId;

/// Nombres de las 7 tablas (§3.2 de la especificación y `decisiones_stair.md`)
pub const TABLA_DOCUMENTOS_CATALOGO: &str = "01_documentos_catalogo";
pub const TABLA_CHUNKS_NORMATIVA: &str = "02_chunks_normativa";
pub const TABLA_MATRIZ_OPERATIVA: &str = "03_matriz_operativa";
pub const TABLA_CATALOGO_TABLAS: &str = "04_catalogo_tablas";
pub const TABLA_CELDAS_DOBLE_ENTRADA: &str = "05_celdas_doble_entrada";
pub const TABLA_GRAFO_REFERENCIAS: &str = "06_grafo_referencias";
pub const TABLA_INDICE_JERARQUICO: &str = "07_indice_jerarquico";

/// Las 7 tablas en orden de catálogo
pub const TODAS_LAS_TABLAS: [&str; 7] = [
    TABLA_DOCUMENTOS_CATALOGO,
    TABLA_CHUNKS_NORMATIVA,
    TABLA_MATRIZ_OPERATIVA,
    TABLA_CATALOGO_TABLAS,
    TABLA_CELDAS_DOBLE_ENTRADA,
    TABLA_GRAFO_REFERENCIAS,
    TABLA_INDICE_JERARQUICO,
];

/// Tablas que dependen del catálogo 01 (sus filas deben tener `doc_id`/`version_num` en 01)
pub const TABLAS_DEPENDIENTES: [&str; 6] = [
    TABLA_CHUNKS_NORMATIVA,
    TABLA_MATRIZ_OPERATIVA,
    TABLA_CATALOGO_TABLAS,
    TABLA_CELDAS_DOBLE_ENTRADA,
    TABLA_GRAFO_REFERENCIAS,
    TABLA_INDICE_JERARQUICO,
];

/// Tope de filas por lectura: LanceDB 0.15 limita a 10 filas si no se fija `.limit()` (O4)
pub const LIMITE_MAXIMO_LECTURA: usize = 1_000_000;

/// Estado transaccional de una fila
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EstadoIngesta {
    Staged,
    Committed,
}

impl EstadoIngesta {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Staged => "STAGED",
            Self::Committed => "COMMITTED",
        }
    }
}

/// Revisión confirmada que ya contiene un hash (respuesta de `existe_hash`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HuellaIngerida {
    pub doc_id: DocId,
    pub version_num: u32,
    pub ingest_id: IngestId,
    /// `true` si la revisión sigue con identidad provisional (admite `reintentar_destilacion`)
    pub es_provisional: bool,
}

/// Resultado de `confirmar_ingesta`
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResumenConfirmacion {
    /// Filas de la ingesta nueva que pasan a `COMMITTED` y `vigente = true`
    pub filas_activadas: u64,
    /// Filas de revisiones previas del mismo documento que pasan a `vigente = false`
    pub filas_retiradas: u64,
    /// Versiones previas cuya vigencia se retiró
    pub versiones_derogadas: Vec<u32>,
    /// Fallo al borrar la revisión sustituida una vez confirmada la nueva: sus filas quedan no
    /// vigentes y no afectan a las lecturas de vigentes ni a `existe_hash`
    pub aviso_limpieza: Option<String>,
}

/// Parámetros de una lectura normal (siempre solo filas `COMMITTED`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsultaTabla {
    pub limite: usize,
    pub solo_vigentes: bool,
    pub centro_id: Option<CenterId>,
    pub doc_id: Option<DocId>,
}

impl ConsultaTabla {
    /// Todas las filas confirmadas (vigentes e históricas) hasta `limite`
    pub fn confirmadas(limite: usize) -> Self {
        Self {
            limite,
            solo_vigentes: false,
            centro_id: None,
            doc_id: None,
        }
    }

    pub fn solo_vigentes(mut self) -> Self {
        self.solo_vigentes = true;
        self
    }

    pub fn del_centro(mut self, centro_id: &CenterId) -> Self {
        self.centro_id = Some(centro_id.clone());
        self
    }

    pub fn del_documento(mut self, doc_id: &DocId) -> Self {
        self.doc_id = Some(doc_id.clone());
        self
    }
}

/// Filas de una tabla dependiente cuya revisión (`doc_id`, `version_num`) no existe en 01
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilasHuerfanas {
    pub tabla: String,
    pub centro_id: String,
    pub doc_id: String,
    pub version_num: i32,
    pub filas: usize,
}

/// Documento con más de una versión vigente simultánea en alguna tabla
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionesVigentesMultiples {
    pub tabla: String,
    pub centro_id: String,
    pub doc_id: String,
    pub versiones: Vec<i32>,
}

/// Referencia a un nodo que no existe en 07 dentro de la misma revisión: un chunk de 02 cuyo
/// `nodo_id` falta, o un nodo de 07 cuyo padre falta. `nodo_id` es el nodo ausente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodosHuerfanos {
    pub tabla: String,
    pub centro_id: String,
    pub doc_id: String,
    pub version_num: i32,
    pub nodo_id: String,
    pub filas: usize,
}

/// Valor guardado fuera del vocabulario de su columna
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValorInvalido {
    pub tabla: String,
    pub columna: String,
    pub valor: String,
    pub filas: usize,
}

/// Diagnóstico de `verificar_integridad`
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InformeIntegridad {
    pub huerfanos: Vec<FilasHuerfanas>,
    /// `ingest_id` con filas aún en `STAGED` (ingestas colgadas o en curso)
    pub ingestas_staged: Vec<String>,
    pub versiones_vigentes_multiples: Vec<VersionesVigentesMultiples>,
    pub nodos_huerfanos: Vec<NodosHuerfanos>,
    pub valores_invalidos: Vec<ValorInvalido>,
}

impl InformeIntegridad {
    pub fn es_integro(&self) -> bool {
        self.huerfanos.is_empty()
            && self.ingestas_staged.is_empty()
            && self.versiones_vigentes_multiples.is_empty()
            && self.nodos_huerfanos.is_empty()
            && self.valores_invalidos.is_empty()
    }
}

/// Puerto de salida del almacén columnar transaccional
#[async_trait]
pub trait LakehouseStorePort: Send + Sync {
    /// Crea de forma idempotente las 7 tablas si no existen
    async fn inicializar_tablas(&self) -> Result<(), DomainError>;

    /// Busca una revisión confirmada del centro con ese SHA-256 (64 hex)
    async fn existe_hash(
        &self,
        centro_id: &CenterId,
        hash_sha256: &str,
    ) -> Result<Option<HuellaIngerida>, DomainError>;

    /// Escribe los lotes como `STAGED` bajo `ingest_id`. Todas las filas deben pertenecer a la
    /// misma revisión. Devuelve el total de filas escritas.
    async fn escribir_staged(
        &self,
        ingest_id: &IngestId,
        lotes: Vec<LoteIngesta>,
    ) -> Result<usize, DomainError>;

    /// Activa la ingesta (`COMMITTED`, `vigente = true`) y retira la vigencia de las revisiones
    /// previas del mismo documento en las 7 tablas, rellenando `fecha_derogacion` en 01.
    async fn confirmar_ingesta(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
    ) -> Result<ResumenConfirmacion, DomainError>;

    /// `confirmar_ingesta` que además sustituye una revisión confirmada (la provisional): deja de
    /// contar para el orden de versiones, se retira en la misma confirmación y, ya confirmada la
    /// nueva, se borran sus filas. Si falla antes de confirmar, todo vuelve al estado previo.
    async fn confirmar_sustitucion(
        &self,
        ingest_id: &IngestId,
        doc_id: &DocId,
        ingest_sustituida: &IngestId,
    ) -> Result<ResumenConfirmacion, DomainError> {
        let _ = (ingest_id, doc_id, ingest_sustituida);
        Err(DomainError::StorageError(
            "Este almacén no admite sustituir revisiones".to_string(),
        ))
    }

    /// Borra todas las filas de una ingesta no confirmada. Devuelve el número de filas borradas.
    async fn revertir_ingesta(&self, ingest_id: &IngestId) -> Result<u64, DomainError>;

    /// Lee filas `COMMITTED` de una tabla con un límite explícito (≤ `LIMITE_MAXIMO_LECTURA`)
    async fn leer_tabla(
        &self,
        nombre_tabla: &str,
        consulta: &ConsultaTabla,
    ) -> Result<Vec<RecordBatch>, DomainError>;

    /// Detecta huérfanos, ingestas `STAGED` colgadas, documentos con varias versiones vigentes,
    /// nodos ausentes entre 02 y 07 y valores fuera de vocabulario
    async fn verificar_integridad(&self) -> Result<InformeIntegridad, DomainError>;

    /// Cuenta todas las filas físicas de una tabla (cualquier estado)
    async fn contar_registros(&self, nombre_tabla: &str) -> Result<usize, DomainError>;

    /// Lista las tablas existentes
    async fn listar_tablas(&self) -> Result<Vec<String>, DomainError>;
}
