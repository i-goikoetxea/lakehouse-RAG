//! Filas tipadas de las 7 tablas del Lakehouse (§4 de la especificación).
//!
//! El caso de uso construye estas filas con tipos de dominio ya validados y el adaptador de
//! almacenamiento las convierte a Arrow. Las columnas transaccionales (`ingest_id`,
//! `estado_ingesta` y `vigente`) no forman parte de la fila: las fija el almacén, de modo que
//! una fila nunca puede «colarse» como confirmada o vigente.

use crate::domain::models::center::CenterId;
use std::collections::HashMap;

use crate::domain::errors::DomainError;
use crate::domain::models::document::{DocId, NivelPrelacion, TipoDocumento};
use crate::domain::models::estructura_documental::{
    EstadoContenido, NivelJerarquico, NodoId, TipoBloque, TipoNodoIndice,
};
use crate::domain::models::operational_matrix::{
    CalidadExtraccion, FaseOperativa, OrigenResponsable, tiene_rol_o_registro,
};
use crate::domain::models::reference_graph::TipoRelacionNormativa;
use crate::domain::models::vigencia::VentanaVigencia;

/// Columnas comunes que identifican la revisión a la que pertenece una fila
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClaveDocumental {
    pub centro_id: CenterId,
    pub doc_id: DocId,
    pub version_num: u32,
}

/// Tabla 01: una fila por revisión de documento
#[derive(Debug, Clone, PartialEq)]
pub struct FilaCatalogo {
    pub clave: ClaveDocumental,
    pub titulo: String,
    pub tipo_documento: TipoDocumento,
    pub revision: String,
    pub vigencia: VentanaVigencia,
    pub hash_sha256: String,
    /// Identidad provisional (ver `docs/arquitectura/decisiones_stair.md`, D4)
    pub doc_id_provisional: Option<DocId>,
    /// Páginas con texto degradado según la compuerta de legibilidad; vacío se guarda como nulo
    pub paginas_degradadas: Vec<u32>,
}

/// Tabla 02: apartado de texto con su linaje jerárquico y su embedding
#[derive(Debug, Clone, PartialEq)]
pub struct FilaChunk {
    pub chunk_id: String,
    pub clave: ClaveDocumental,
    /// Sección en la tabla 07; los sub-chunks de una sección lo comparten
    pub nodo_id: NodoId,
    pub tipo_bloque: TipoBloque,
    pub estado_contenido: EstadoContenido,
    pub nivel_prelacion: NivelPrelacion,
    pub breadcrumb_jerarquia: String,
    pub condicion_aplicacion: Option<String>,
    pub texto_contenido: String,
    /// Páginas de la sección de origen (para citas); `None` en formatos sin páginas
    pub pagina_inicio: Option<u32>,
    pub pagina_fin: Option<u32>,
    /// Saltos de página dentro de `texto_contenido` (`ssd_citas_pagina.md` §1); `None` si no se conocen
    pub saltos_pagina: Option<Vec<crate::domain::services::marcas_pagina::SaltoPagina>>,
    pub tablas_vinculadas: Vec<String>,
    pub hitos_vinculados: Vec<String>,
    pub vector: Vec<f32>,
}

/// Tabla 03: un hito operativo por fila (anti-fusión de fase, ejecutor, responsable y registro)
#[derive(Debug, Clone, PartialEq)]
pub struct FilaHito {
    pub hito_id: String,
    pub clave: ClaveDocumental,
    pub orden_secuencia: u32,
    pub fase_operativa: FaseOperativa,
    pub objeto_alcance: String,
    pub rol_ejecutor: String,
    pub rol_responsable: String,
    pub origen_responsable: OrigenResponsable,
    pub registro_codigo: Option<String>,
    pub registro_nombre: Option<String>,
    pub criterio_aceptacion: String,
    pub condicion_aplicacion: Option<String>,
    pub nivel_prelacion: NivelPrelacion,
    pub calidad_extraccion: CalidadExtraccion,
    pub chunk_origen_id: Option<String>,
    pub vector_hito: Option<Vec<f32>>,
}

/// Naturaleza de un eje de tabla de doble entrada
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoEjeTabla {
    RangoNumerico,
    CategoriaExacta,
}

impl TipoEjeTabla {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RangoNumerico => "RANGO_NUMERICO",
            Self::CategoriaExacta => "CATEGORIA_EXACTA",
        }
    }
}

/// Descripción de una variable de entrada de la tabla (Tabla 04)
#[derive(Debug, Clone, PartialEq)]
pub struct EjeTabla {
    pub nombre: String,
    pub tipo: TipoEjeTabla,
    pub unidad: Option<String>,
}

/// Tabla 04: glue catalog de tablas de doble entrada
#[derive(Debug, Clone, PartialEq)]
pub struct FilaTabla {
    pub tabla_id: String,
    pub clave: ClaveDocumental,
    pub titulo_tabla: String,
    pub eje_x: EjeTabla,
    pub eje_y: EjeTabla,
    pub valor_salida_nombre: String,
    pub instrucciones_uso_sql: String,
    pub vector_tabla: Option<Vec<f32>>,
    /// Bloque físico `(página inicial, página final)` anclado por ventana de tabla; `None` si no es
    /// inequívoco o el formato no tiene páginas (`ssd_generalidad_4_piezas.md` §2)
    pub paginas: Option<(u32, u32)>,
}

/// Límites de una celda en un eje: categoría exacta o rango con inclusividad por extremo
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LimitesEje {
    pub categoria: Option<String>,
    pub minimo: Option<f64>,
    pub minimo_inclusivo: bool,
    pub maximo: Option<f64>,
    pub maximo_inclusivo: bool,
}

/// Tabla 05: una celda desnormalizada por fila
#[derive(Debug, Clone, PartialEq)]
pub struct FilaCelda {
    pub celda_id: String,
    pub tabla_id: String,
    pub clave: ClaveDocumental,
    pub eje_x: LimitesEje,
    pub eje_y: LimitesEje,
    pub eje_z_filtro_adicional: Option<String>,
    pub valor_resultado_texto: String,
    pub valor_resultado_num: Option<f64>,
    pub unidad_medida: Option<String>,
    pub nota_pie_condicion: Option<String>,
    pub nivel_prelacion: NivelPrelacion,
    /// Página física de la fila de la celda; `None` antes que una página dudosa
    pub pagina: Option<u32>,
}

/// Tipo de nodo en los extremos de una arista del grafo (§4, Tabla 6)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoNodoGrafo {
    Documento,
    HitoOperativo,
    Chunk,
    Registro,
    Tabla,
}

impl TipoNodoGrafo {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Documento => "DOCUMENTO",
            Self::HitoOperativo => "HITO_OPERATIVO",
            Self::Chunk => "CHUNK",
            Self::Registro => "REGISTRO",
            Self::Tabla => "TABLA",
        }
    }

    /// Tipo de destino implícito en la relación (`EXIGE_LLENAR_REGISTRO` → `REGISTRO`)
    pub fn destino_de(relacion: TipoRelacionNormativa) -> Self {
        match relacion {
            TipoRelacionNormativa::ExigeLlenarRegistro => Self::Registro,
            TipoRelacionNormativa::RemiteATabla => Self::Tabla,
            _ => Self::Documento,
        }
    }
}

/// Tabla 06: arista del grafo de referencias; `clave.doc_id` es el documento que la declara
#[derive(Debug, Clone, PartialEq)]
pub struct FilaArista {
    pub edge_id: String,
    pub clave: ClaveDocumental,
    pub origen_id: String,
    pub origen_tipo: TipoNodoGrafo,
    pub tipo_relacion: TipoRelacionNormativa,
    pub destino_id: String,
    pub destino_tipo: TipoNodoGrafo,
    pub contexto_relacion: Option<String>,
}

/// Tabla 07: un nodo del índice jerárquico (ToC) de la revisión
#[derive(Debug, Clone, PartialEq)]
pub struct FilaIndice {
    pub nodo_id: NodoId,
    pub clave: ClaveDocumental,
    pub estado_contenido: EstadoContenido,
    pub tipo_nodo: TipoNodoIndice,
    pub nivel_jerarquico: NivelJerarquico,
    pub codigo_seccion: String,
    pub titulo_seccion: String,
    pub nodo_padre_id: Option<NodoId>,
    pub ruta_linaje: String,
    pub condiciones_alcance: Option<String>,
    pub resumen_seccion: String,
    pub vector_seccion: Vec<f32>,
}

/// Lote homogéneo de filas destinado a una de las 7 tablas
#[derive(Debug, Clone, PartialEq)]
pub enum LoteIngesta {
    Catalogo(Vec<FilaCatalogo>),
    Chunks(Vec<FilaChunk>),
    Hitos(Vec<FilaHito>),
    Tablas(Vec<FilaTabla>),
    Celdas(Vec<FilaCelda>),
    Aristas(Vec<FilaArista>),
    Indice(Vec<FilaIndice>),
}

impl LoteIngesta {
    pub fn len(&self) -> usize {
        match self {
            Self::Catalogo(filas) => filas.len(),
            Self::Chunks(filas) => filas.len(),
            Self::Hitos(filas) => filas.len(),
            Self::Tablas(filas) => filas.len(),
            Self::Celdas(filas) => filas.len(),
            Self::Aristas(filas) => filas.len(),
            Self::Indice(filas) => filas.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Claves documentales de todas las filas del lote (para validar que pertenecen a una sola revisión)
    pub fn claves(&self) -> Vec<&ClaveDocumental> {
        match self {
            Self::Catalogo(filas) => filas.iter().map(|f| &f.clave).collect(),
            Self::Chunks(filas) => filas.iter().map(|f| &f.clave).collect(),
            Self::Hitos(filas) => filas.iter().map(|f| &f.clave).collect(),
            Self::Tablas(filas) => filas.iter().map(|f| &f.clave).collect(),
            Self::Celdas(filas) => filas.iter().map(|f| &f.clave).collect(),
            Self::Aristas(filas) => filas.iter().map(|f| &f.clave).collect(),
            Self::Indice(filas) => filas.iter().map(|f| &f.clave).collect(),
        }
    }
}

/// Coherencia entre 02 y 07 de una escritura; las dos tablas deben ir en la misma llamada.
/// Una escritura sin 07 solo comprueba la pertenencia de los `nodo_id` a su documento.
pub fn validar_estructura_stair(lotes: &[LoteIngesta]) -> Result<(), DomainError> {
    let chunks: Vec<&FilaChunk> = lotes
        .iter()
        .filter_map(|lote| match lote {
            LoteIngesta::Chunks(filas) => Some(filas),
            _ => None,
        })
        .flatten()
        .collect();
    let nodos: Vec<&FilaIndice> = lotes
        .iter()
        .filter_map(|lote| match lote {
            LoteIngesta::Indice(filas) => Some(filas),
            _ => None,
        })
        .flatten()
        .collect();

    for chunk in &chunks {
        exigir_pertenencia(&chunk.nodo_id, &chunk.clave)?;
    }
    for hito in lotes
        .iter()
        .filter_map(|lote| match lote {
            LoteIngesta::Hitos(filas) => Some(filas),
            _ => None,
        })
        .flatten()
    {
        exigir_rol_o_registro(hito)?;
    }
    let niveles = niveles_por_nodo(&nodos)?;
    for nodo in &nodos {
        validar_nodo(nodo, &niveles)?;
    }
    if !nodos.is_empty()
        && let Some(chunk) = chunks.iter().find(|c| !niveles.contains_key(&c.nodo_id))
    {
        return Err(DomainError::ValidationError(format!(
            "El chunk '{}' apunta al nodo '{}', que no está en 07",
            chunk.chunk_id, chunk.nodo_id
        )));
    }
    Ok(())
}

/// Última barrera de la invariante de `HitoOperativo`: 03 nunca guarda un hito sin titular
fn exigir_rol_o_registro(hito: &FilaHito) -> Result<(), DomainError> {
    if tiene_rol_o_registro(
        &hito.rol_ejecutor,
        &hito.rol_responsable,
        hito.registro_codigo.as_deref(),
        hito.registro_nombre.as_deref(),
    ) {
        return Ok(());
    }
    Err(DomainError::HitoSinTitular(hito.hito_id.clone()))
}

fn niveles_por_nodo<'a>(
    nodos: &[&'a FilaIndice],
) -> Result<HashMap<&'a NodoId, NivelJerarquico>, DomainError> {
    let mut niveles = HashMap::with_capacity(nodos.len());
    for nodo in nodos {
        if niveles
            .insert(&nodo.nodo_id, nodo.nivel_jerarquico)
            .is_some()
        {
            return Err(DomainError::ValidationError(format!(
                "nodo_id duplicado en 07: '{}'",
                nodo.nodo_id
            )));
        }
    }
    Ok(niveles)
}

/// El padre debe existir y tener un nivel estrictamente menor, lo que también impide los ciclos
fn validar_nodo(
    nodo: &FilaIndice,
    niveles: &HashMap<&NodoId, NivelJerarquico>,
) -> Result<(), DomainError> {
    exigir_pertenencia(&nodo.nodo_id, &nodo.clave)?;
    if !nodo.ruta_linaje.starts_with('/') {
        return Err(DomainError::ValidationError(format!(
            "ruta_linaje de '{}' debe empezar por '/': {:?}",
            nodo.nodo_id, nodo.ruta_linaje
        )));
    }
    let Some(padre) = &nodo.nodo_padre_id else {
        return Ok(());
    };
    exigir_pertenencia(padre, &nodo.clave)?;
    match niveles.get(padre) {
        Some(nivel) if *nivel < nodo.nivel_jerarquico => Ok(()),
        Some(_) => Err(DomainError::ValidationError(format!(
            "El padre '{padre}' de '{}' debe tener un nivel menor",
            nodo.nodo_id
        ))),
        None => Err(DomainError::ValidationError(format!(
            "El padre '{padre}' de '{}' no está en 07",
            nodo.nodo_id
        ))),
    }
}

fn exigir_pertenencia(nodo: &NodoId, clave: &ClaveDocumental) -> Result<(), DomainError> {
    if nodo.pertenece_a(&clave.doc_id) {
        Ok(())
    } else {
        Err(DomainError::ValidationError(format!(
            "El nodo '{nodo}' no pertenece al documento '{}'",
            clave.doc_id
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destino_de_la_relacion_y_vocabularios_canonicos() {
        assert_eq!(
            TipoNodoGrafo::destino_de(TipoRelacionNormativa::ExigeLlenarRegistro).as_str(),
            "REGISTRO"
        );
        assert_eq!(
            TipoNodoGrafo::destino_de(TipoRelacionNormativa::RemiteATabla).as_str(),
            "TABLA"
        );
        assert_eq!(
            TipoNodoGrafo::destino_de(TipoRelacionNormativa::DerogaA).as_str(),
            "DOCUMENTO"
        );
        assert_eq!(TipoEjeTabla::RangoNumerico.as_str(), "RANGO_NUMERICO");
        assert!(LoteIngesta::Celdas(Vec::new()).is_empty());
    }
}
