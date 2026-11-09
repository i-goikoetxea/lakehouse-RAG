//! Mapeo puro del paquete destilado v2 (más el documento parseado) a las filas de las tablas 01-07.
//!
//! El corpus 02 y el índice 07 salen solo del documento parseado, sin depender del LLM: si la
//! destilación falla, `mapear_revision_provisional` los indexa igual con un catálogo de respaldo
//! (`docs/arquitectura/decisiones_stair.md`).
//!
//! Es la red de seguridad de la ingesta (problemas 1, 2 y 7):
//! - vocabularios normalizados con los enums del dominio (fase, relación, tipo de documento);
//! - `doc_id` canónico con `separar_revision` + `DocId::desde_codigo_libre`;
//! - `calidad_extraccion` desde `HitoOperativo::alertas_calidad`;
//! - enlaces hito/tabla → chunk de su sección y condición heredada del encabezado;
//! - una arista `EXIGE_LLENAR_REGISTRO` por cada registro aunque el LLM la omita.
//!
//! No calcula vectores: devuelve los textos a vectorizar y el servicio los rellena.

use std::collections::HashSet;

use chrono::Utc;

use crate::application::dtos::ingestion::{
    AristaDestilada, CatalogoDestilado, CeldaDestilada, DistilledDocumentPackage, EjeDestilado,
    HitoDestilado, LimitesDestilados, ParsedDocument, ParsedSection, ParsedTableRaw, SaltoPagina,
    TablaDestilada,
};
use crate::application::dtos::lakehouse_filas::{
    ClaveDocumental, EjeTabla, FilaArista, FilaCatalogo, FilaCelda, FilaChunk, FilaHito,
    FilaIndice, FilaTabla, LimitesEje, TipoEjeTabla, TipoNodoGrafo,
};
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::document::{DocId, NivelPrelacion, TipoDocumento};
use crate::domain::models::estructura_documental::{
    EstadoContenido, NivelJerarquico, NodoId, TipoBloque, TipoNodoIndice, codigo_real_de,
};
use crate::domain::models::identificadores::{TipoEntidad, id_versionado};
use crate::domain::models::operational_matrix::{
    BorradorHito, FaseOperativa, HitoOperativo, OrigenResponsable,
};
use crate::domain::models::reference_graph::TipoRelacionNormativa;
use crate::domain::models::vigencia::{VentanaVigencia, validar_fecha_vigor};
use crate::domain::services::arbol_secciones::{
    CODIGO_PREAMBULO, SeccionDelArbol, TITULO_PREAMBULO, clasificar_bloque, construir_arbol,
    resumen_extractivo,
};
use crate::domain::services::fragmentacion::fragmentar_con_posiciones;
use crate::domain::services::marcas_pagina::saltos_del_tramo;
use crate::domain::services::normalizacion::{
    codigo_canonico, normalizar_token, plegar_mayusculas, separar_revision,
};

/// Tipo asumido cuando el LLM devuelve un `tipo_documento` fuera de vocabulario
const TIPO_DOCUMENTO_POR_DEFECTO: TipoDocumento = TipoDocumento::ProcedimientoGeneral;
/// Fase asignada a un hito sin fase interpretable
const FASE_NO_ESPECIFICADA: &str = "NO_ESPECIFICADA";
/// Objeto asignado a un hito sin objeto (el hito queda en REVISAR por su contexto)
const OBJETO_NO_ESPECIFICADO: &str = "NO_ESPECIFICADO";
/// Nombre de la magnitud de salida cuando el LLM no lo entrega
const VALOR_SALIDA_POR_DEFECTO: &str = "valor_resultado";
/// Prefijo canónico que se descarta al comparar etiquetas de tabla (`TABLA-4-A`)
const PREFIJO_ETIQUETA_TABLA: &str = "TABLA-";
/// Contexto de las aristas que añade la ingesta como red de seguridad
const CONTEXTO_ARISTA_SINTETIZADA: &str =
    "Arista añadida por la ingesta: el hito rellena este registro";

/// Tipo de un documento sin destilar: `FORMATO_REGISTRO` tiene la prelación más baja (5), de modo
/// que un documento sin validar nunca se imponga a uno destilado
const TIPO_DOCUMENTO_PROVISIONAL: TipoDocumento = TipoDocumento::FormatoRegistro;
/// Versión de un documento sin destilar
const VERSION_PROVISIONAL: u32 = 1;
/// Revisión de un documento sin destilar
const REVISION_PROVISIONAL: &str = "Provisional";
/// Título de un encabezado sin texto
const TITULO_SIN_NOMBRE: &str = "(sin título)";
/// Longitud máxima del slug del `doc_id` provisional
const MAX_LONGITUD_SLUG: usize = 64;
/// Slug de un archivo cuyo nombre no contiene ningún alfanumérico
const SLUG_POR_DEFECTO: &str = "DOC";
/// Caracteres del SHA-256 en el `doc_id` provisional
const HEX_HASH_PROVISIONAL: usize = 6;

/// Filas de una revisión sin vectores, con los textos que hay que vectorizar
#[derive(Debug, Clone, PartialEq)]
pub struct RevisionMapeada {
    /// Datos del extractor que se rechazaron o rebajaron al mapear
    pub avisos: Vec<String>,
    /// Hitos sin rol ni registro: no entran en 03 y su texto sigue en la Capa 0
    pub hitos_derivados_capa0: usize,
    pub catalogo: FilaCatalogo,
    pub chunks: Vec<FilaChunk>,
    pub hitos: Vec<FilaHito>,
    pub tablas: Vec<FilaTabla>,
    pub celdas: Vec<FilaCelda>,
    pub aristas: Vec<FilaArista>,
    /// Índice 07 sin `vector_seccion`, una fila por sección
    pub nodos: Vec<FilaIndice>,
    /// `titulo + " " + resumen` de cada nodo (mismo orden que `nodos`)
    pub textos_nodos: Vec<String>,
    /// `breadcrumb + condición + texto` de cada chunk (mismo orden que `chunks`)
    pub textos_chunks: Vec<String>,
    /// `HitoOperativo::texto_vectorizable` de cada hito
    pub textos_hitos: Vec<String>,
    /// Descripción y variables de cada tabla
    pub textos_tablas: Vec<String>,
}

/// Fragmento de una sección que se convierte en chunk (los de una sección comparten `nodo_id`)
#[derive(Debug, Clone, PartialEq)]
struct SeccionIndexable {
    nodo_id: NodoId,
    tipo_bloque: TipoBloque,
    codigo: String,
    breadcrumb: String,
    condicion: Option<String>,
    texto: String,
    pagina_inicio: Option<u32>,
    pagina_fin: Option<u32>,
    saltos_pagina: Option<Vec<SaltoPagina>>,
}

/// Clave de la revisión: `doc_id` canónico sin revisión y versión resuelta
pub fn clave_documental(
    centro_id: &CenterId,
    catalogo: &CatalogoDestilado,
) -> Result<ClaveDocumental, DomainError> {
    let (codigo, revision_en_id) = separar_revision(&catalogo.doc_id);
    let doc_id = DocId::desde_codigo_libre(&codigo)?;
    let version_num = resolver_version(catalogo, revision_en_id)?;
    Ok(ClaveDocumental {
        centro_id: centro_id.clone(),
        doc_id,
        version_num,
    })
}

/// `version_num` si es válido; si no, el número de `revision`; si no, la revisión del `doc_id`
fn resolver_version(
    catalogo: &CatalogoDestilado,
    revision_en_id: Option<u32>,
) -> Result<u32, DomainError> {
    u32::try_from(catalogo.version_num)
        .ok()
        .or_else(|| numero_de_revision(&catalogo.revision))
        .or(revision_en_id)
        .ok_or_else(|| {
            DomainError::IngestionError(format!(
                "Versión destilada inválida ({}) y revisión ilegible ('{}')",
                catalogo.version_num, catalogo.revision
            ))
        })
}

/// Número de una revisión escrita libremente (`Rev. 04` → 4, `4` → 4)
fn numero_de_revision(revision: &str) -> Option<u32> {
    let digitos: String = revision
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digitos.parse().ok()
}

/// `doc_id` provisional: slug del archivo en mayúsculas, `_` y 6 hex del SHA-256
pub fn doc_id_provisional(nombre_archivo: &str, hash_sha256: &str) -> Result<DocId, DomainError> {
    let sin_extension = nombre_archivo
        .rsplit_once('.')
        .map_or(nombre_archivo, |(base, _)| base);
    let slug = codigo_canonico(&sin_extension.replace('.', "-"));
    let slug: String = slug.chars().take(MAX_LONGITUD_SLUG).collect();
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() {
        SLUG_POR_DEFECTO
    } else {
        slug
    };
    let hex: String = hash_sha256.chars().take(HEX_HASH_PROVISIONAL).collect();
    DocId::new(format!("{slug}_{hex}"))
}

/// Revisión de un documento sin destilar: corpus 02 e índice 07 en `PARCIAL_UNIVERSAL` (o
/// `REQUIERE_OCR` si el PDF no tiene texto), sin Capa 2 y con metadatos de respaldo
/// (`decisiones_stair.md`, D4)
pub fn mapear_revision_provisional(
    centro_id: &CenterId,
    parsed: &ParsedDocument,
    hash_sha256: &str,
) -> Result<(ClaveDocumental, RevisionMapeada), DomainError> {
    let clave = ClaveDocumental {
        centro_id: centro_id.clone(),
        doc_id: doc_id_provisional(&parsed.nombre_archivo, hash_sha256)?,
        version_num: VERSION_PROVISIONAL,
    };
    let paquete = DistilledDocumentPackage {
        catalogo: CatalogoDestilado {
            doc_id: clave.doc_id.to_string(),
            titulo: parsed.titulo.clone(),
            tipo_documento: TIPO_DOCUMENTO_PROVISIONAL.as_str().to_string(),
            revision: REVISION_PROVISIONAL.to_string(),
            version_num: i64::from(VERSION_PROVISIONAL),
            fecha_vigor: None,
            emisor: String::new(),
        },
        hitos: Vec::new(),
        tablas: Vec::new(),
        aristas: Vec::new(),
    };
    let mut revision = mapear_con_estado(
        &clave,
        parsed,
        &paquete,
        hash_sha256,
        if parsed.requiere_ocr {
            EstadoContenido::RequiereOcr
        } else {
            EstadoContenido::ParcialUniversal
        },
    )?;
    revision.catalogo.doc_id_provisional = Some(clave.doc_id.clone());
    Ok((clave, revision))
}

/// Mapea el paquete destilado y el documento parseado a las filas de la revisión
pub fn mapear_revision(
    clave: &ClaveDocumental,
    parsed: &ParsedDocument,
    paquete: &DistilledDocumentPackage,
    hash_sha256: &str,
) -> Result<RevisionMapeada, DomainError> {
    mapear_con_estado(
        clave,
        parsed,
        paquete,
        hash_sha256,
        EstadoContenido::Completo,
    )
}

fn mapear_con_estado(
    clave: &ClaveDocumental,
    parsed: &ParsedDocument,
    paquete: &DistilledDocumentPackage,
    hash_sha256: &str,
    estado: EstadoContenido,
) -> Result<RevisionMapeada, DomainError> {
    let tipo_documento = TipoDocumento::parse(&paquete.catalogo.tipo_documento)
        .unwrap_or(TIPO_DOCUMENTO_POR_DEFECTO);
    let nivel = tipo_documento.nivel_prelacion();
    let (catalogo, aviso_fecha) = fila_catalogo(
        clave,
        &paquete.catalogo,
        tipo_documento,
        hash_sha256,
        &parsed.contenido_completo,
    );

    let (nodos, textos_nodos, secciones) = estructura_universal(clave, parsed, estado);
    let chunk_ids: Vec<String> = (0..secciones.len())
        .map(|idx| id_versionado(&clave.doc_id, clave.version_num, TipoEntidad::Chunk, idx))
        .collect();

    let HitosMapeados {
        filas: hitos,
        textos: textos_hitos,
        secciones: seccion_de_hito,
        derivados_capa0,
        origenes_rebajados,
    } = filas_hitos(clave, nivel, &paquete.hitos, &secciones, &chunk_ids)?;
    let (tablas, textos_tablas, celdas) =
        filas_tablas_y_celdas(clave, nivel, &paquete.tablas, &parsed.tablas);
    let etiquetas = etiquetas_de_tabla(&paquete.tablas, &tablas);
    let aristas = filas_aristas(clave, &paquete.aristas, &hitos, &etiquetas);

    let mut chunks = filas_chunks_sin_vector(clave, nivel, estado, &secciones, &chunk_ids);
    vincular_hitos(&mut chunks, &hitos, &seccion_de_hito);
    vincular_tablas(
        &mut chunks,
        &tablas,
        &paquete.tablas,
        &secciones,
        &parsed.tablas,
    );
    let textos_chunks = secciones.iter().map(texto_vectorizable_chunk).collect();
    let avisos = aviso_fecha
        .into_iter()
        .chain(aviso_de_origenes_rebajados(origenes_rebajados))
        .chain(aviso_de_hitos_derivados(derivados_capa0))
        .collect();

    Ok(RevisionMapeada {
        avisos,
        hitos_derivados_capa0: derivados_capa0,
        catalogo,
        chunks,
        hitos,
        tablas,
        celdas,
        aristas,
        nodos,
        textos_nodos,
        textos_chunks,
        textos_hitos,
        textos_tablas,
    })
}

/// Aviso si la red de seguridad rebajó algún responsable que el extractor dio por declarado
fn aviso_de_origenes_rebajados(rebajados: usize) -> Option<String> {
    (rebajados > 0).then(|| {
        format!(
            "{rebajados} responsable(s) dados por declarados no aparecen en el texto de su sección: quedan como INFERIDO o NO_CONSTA."
        )
    })
}

/// Aviso si algún hito no entró en la matriz por no tener rol ni registro
fn aviso_de_hitos_derivados(derivados: usize) -> Option<String> {
    (derivados > 0).then(|| {
        format!(
            "{derivados} hito(s) sin rol ni registro no entran en la matriz: su texto sigue en la Capa 0."
        )
    })
}

fn fila_catalogo(
    clave: &ClaveDocumental,
    catalogo: &CatalogoDestilado,
    tipo_documento: TipoDocumento,
    hash_sha256: &str,
    texto_documento: &str,
) -> (FilaCatalogo, Option<String>) {
    let revision = texto_opcional(&catalogo.revision)
        .unwrap_or_else(|| format!("Rev. {:02}", clave.version_num));
    let validada = validar_fecha_vigor(
        catalogo.fecha_vigor.as_deref(),
        Utc::now().date_naive(),
        texto_documento,
    );
    let vigencia = match validada.fecha {
        Some(fecha) => VentanaVigencia::desde_fecha_declarada(fecha),
        None => VentanaVigencia::sin_fecha_declarada(None),
    };
    (
        FilaCatalogo {
            clave: clave.clone(),
            titulo: catalogo.titulo.trim().to_string(),
            tipo_documento,
            revision,
            vigencia,
            hash_sha256: hash_sha256.to_string(),
            doc_id_provisional: None,
        },
        validada.aviso,
    )
}

type EstructuraUniversal = (Vec<FilaIndice>, Vec<String>, Vec<SeccionIndexable>);

/// Sección única con todo el contenido, para un extractor que no detecte ninguna
fn seccion_unica(parsed: &ParsedDocument) -> ParsedSection {
    ParsedSection {
        codigo: CODIGO_PREAMBULO.to_string(),
        titulo: TITULO_PREAMBULO.to_string(),
        nivel: 0,
        contenido: parsed.contenido_completo.trim().to_string(),
        breadcrumb: parsed.titulo.clone(),
        ..ParsedSection::default()
    }
}

/// Nivel del nodo: solo la primera sección puede ser preámbulo; el resto se acota a H1-H6
fn nivel_de_seccion(indice: usize, seccion: &ParsedSection) -> NivelJerarquico {
    let minimo = u8::from(indice > 0);
    let nivel = seccion.nivel.clamp(minimo, NivelJerarquico::MAXIMO as u8);
    NivelJerarquico::new(nivel as i8).unwrap_or(NivelJerarquico::PREAMBULO)
}

/// Índice 07 (una fila por sección, tenga o no texto) y fragmentos para el corpus 02. Un título
/// hoja sin texto se indexa como chunk para que ninguna palabra quede fuera de 02; los demás
/// títulos ya viajan en el breadcrumb de sus descendientes.
fn estructura_universal(
    clave: &ClaveDocumental,
    parsed: &ParsedDocument,
    estado: EstadoContenido,
) -> EstructuraUniversal {
    let unica;
    let secciones: &[ParsedSection] = if parsed.secciones.is_empty() {
        unica = [seccion_unica(parsed)];
        &unica
    } else {
        &parsed.secciones
    };
    let niveles: Vec<NivelJerarquico> = secciones
        .iter()
        .enumerate()
        .map(|(indice, seccion)| nivel_de_seccion(indice, seccion))
        .collect();
    let entradas: Vec<SeccionDelArbol<'_>> = secciones
        .iter()
        .zip(&niveles)
        .map(|(seccion, nivel)| SeccionDelArbol {
            nivel: *nivel,
            codigo: seccion.clave_nodo.as_deref().unwrap_or(&seccion.codigo),
            // Una parte que agrupa un tramo sin título es de nivel 1: solo los sub-nodos cuelgan
            sintetica: seccion.tipo_nodo == TipoNodoIndice::Sintetico
                && codigo_real_de(&seccion.codigo) != seccion.codigo,
        })
        .collect();
    let arbol = construir_arbol(&clave.doc_id, &entradas);

    let mut nodos = Vec::with_capacity(secciones.len());
    let mut textos_nodos = Vec::with_capacity(secciones.len());
    let mut fragmentos = Vec::with_capacity(secciones.len());
    let mut tipo_de_la_real = TipoBloque::Narrativa;
    for (indice, (seccion, nodo)) in secciones.iter().zip(arbol).enumerate() {
        let nivel = niveles[indice];
        let titulo = texto_opcional(&seccion.titulo).unwrap_or_else(|| TITULO_SIN_NOMBRE.into());
        let resumen = resumen_extractivo(&titulo, &seccion.contenido);
        textos_nodos.push(componer_texto_nodo(&titulo, &resumen));

        let es_hoja = niveles
            .get(indice + 1)
            .is_none_or(|siguiente| *siguiente <= nivel);
        let solo_titulo = seccion.contenido.trim().is_empty() && es_hoja;
        let origen = if solo_titulo {
            seccion.titulo.as_str()
        } else {
            seccion.contenido.as_str()
        };
        let sintetica = codigo_real_de(&seccion.codigo) != seccion.codigo;
        if !sintetica {
            tipo_de_la_real = clasificar_bloque(nivel, &titulo);
        }
        let tipo_bloque = tipo_de_la_real;
        fragmentos.extend(
            fragmentar_con_posiciones(origen)
                .into_iter()
                .map(|fragmento| {
                    let paginas = if solo_titulo {
                        PaginasFragmento::de_la_seccion(seccion)
                    } else {
                        paginas_del_fragmento(seccion, fragmento.inicio, fragmento.fin)
                    };
                    SeccionIndexable {
                        nodo_id: nodo.nodo_id.clone(),
                        tipo_bloque,
                        codigo: codigo_real_de(&seccion.codigo).to_string(),
                        breadcrumb: seccion.breadcrumb.clone(),
                        condicion: seccion.condicion_aplicacion.clone(),
                        texto: fragmento.texto,
                        pagina_inicio: paginas.inicio,
                        pagina_fin: paginas.fin,
                        saltos_pagina: paginas.saltos,
                    }
                }),
        );
        nodos.push(FilaIndice {
            nodo_id: nodo.nodo_id,
            clave: clave.clone(),
            estado_contenido: estado,
            tipo_nodo: seccion.tipo_nodo,
            nivel_jerarquico: nivel,
            codigo_seccion: seccion.codigo.clone(),
            titulo_seccion: titulo,
            nodo_padre_id: nodo.nodo_padre_id,
            ruta_linaje: nodo.ruta_linaje,
            condiciones_alcance: seccion.condicion_aplicacion.clone(),
            resumen_seccion: resumen,
            vector_seccion: Vec::new(),
        });
    }
    (nodos, textos_nodos, fragmentos)
}

/// Páginas de un fragmento y los saltos de página dentro de su texto
struct PaginasFragmento {
    inicio: Option<u32>,
    fin: Option<u32>,
    /// `None` si no se sabe dónde cambia la página (`ssd_citas_pagina.md` §1)
    saltos: Option<Vec<SaltoPagina>>,
}

impl PaginasFragmento {
    /// Rango de la sección. Solo si cabe en una página se sabe que todo el texto está en ella
    fn de_la_seccion(seccion: &ParsedSection) -> Self {
        let saltos = seccion
            .pagina_inicio
            .filter(|inicio| seccion.pagina_fin == Some(*inicio))
            .map(|pagina| {
                vec![SaltoPagina {
                    desplazamiento: 0,
                    pagina,
                }]
            });
        Self {
            inicio: seccion.pagina_inicio,
            fin: seccion.pagina_fin,
            saltos,
        }
    }
}

/// Páginas reales del tramo `inicio..fin` del contenido y sus saltos, relativos a `inicio`. Sin
/// saltos (formatos sin páginas o texto de respaldo) vale el rango de la sección.
fn paginas_del_fragmento(seccion: &ParsedSection, inicio: usize, fin: usize) -> PaginasFragmento {
    if seccion.saltos_pagina.is_empty() {
        return PaginasFragmento::de_la_seccion(seccion);
    }
    let saltos = saltos_del_tramo(&seccion.saltos_pagina, inicio, fin);
    PaginasFragmento {
        inicio: saltos.first().map(|salto| salto.pagina),
        fin: saltos.last().map(|salto| salto.pagina),
        saltos: Some(saltos),
    }
}

/// Texto que se vectoriza para un chunk: `breadcrumb + condición + texto` (problema 7)
fn texto_vectorizable_chunk(seccion: &SeccionIndexable) -> String {
    componer_texto_vectorizable(
        &seccion.breadcrumb,
        seccion.condicion.as_deref(),
        &seccion.texto,
    )
}

/// Composición del texto vectorizable de un chunk a partir de sus columnas guardadas en 02
pub fn componer_texto_vectorizable(
    breadcrumb: &str,
    condicion: Option<&str>,
    texto: &str,
) -> String {
    let mut partes = vec![breadcrumb.to_string()];
    if let Some(condicion) = condicion {
        partes.push(format!("Condición: {condicion}"));
    }
    partes.push(texto.to_string());
    partes.retain(|p| !p.trim().is_empty());
    partes.join("\n")
}

/// Texto que se vectoriza para un nodo de 07: título y resumen
pub fn componer_texto_nodo(titulo: &str, resumen: &str) -> String {
    format!("{titulo} {resumen}")
}

fn filas_chunks_sin_vector(
    clave: &ClaveDocumental,
    nivel: NivelPrelacion,
    estado: EstadoContenido,
    secciones: &[SeccionIndexable],
    chunk_ids: &[String],
) -> Vec<FilaChunk> {
    secciones
        .iter()
        .zip(chunk_ids)
        .map(|(seccion, chunk_id)| FilaChunk {
            chunk_id: chunk_id.clone(),
            clave: clave.clone(),
            nodo_id: seccion.nodo_id.clone(),
            tipo_bloque: seccion.tipo_bloque,
            estado_contenido: estado,
            nivel_prelacion: nivel,
            breadcrumb_jerarquia: seccion.breadcrumb.clone(),
            condicion_aplicacion: seccion.condicion.clone(),
            texto_contenido: seccion.texto.clone(),
            pagina_inicio: seccion.pagina_inicio,
            pagina_fin: seccion.pagina_fin,
            saltos_pagina: seccion.saltos_pagina.clone(),
            tablas_vinculadas: Vec::new(),
            hitos_vinculados: Vec::new(),
            vector: Vec::new(),
        })
        .collect()
}

/// Código de sección al que apunta una referencia libre (`2.2`, `2.2.`, `Sección 2.2 Custodia`)
fn codigo_de_referencia(referencia: &str) -> Option<String> {
    referencia
        .split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .find(|token| token.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

/// Índice de la sección a la que pertenece una referencia: por código exacto o, si no, por
/// coincidencia del texto normalizado dentro del breadcrumb
fn buscar_seccion(referencia: &str, secciones: &[SeccionIndexable]) -> Option<usize> {
    if let Some(codigo) = codigo_de_referencia(referencia)
        && let Some(idx) = secciones.iter().position(|s| s.codigo == codigo)
    {
        return Some(idx);
    }
    let buscado = normalizar_token(referencia);
    if buscado.is_empty() {
        return None;
    }
    secciones
        .iter()
        .position(|s| normalizar_token(&s.breadcrumb).ends_with(&buscado))
}

fn texto_opcional(texto: &str) -> Option<String> {
    let limpio = texto.trim();
    (!limpio.is_empty()).then(|| limpio.to_string())
}

/// Recorta el texto a `max` bytes en frontera de carácter y elimina los bytes nulos
fn acotar(texto: &str, max: usize) -> String {
    let sin_nulos: String = texto.trim().chars().filter(|c| *c != '\0').collect();
    if sin_nulos.len() <= max {
        return sin_nulos;
    }
    let mut corte = max;
    while !sin_nulos.is_char_boundary(corte) {
        corte -= 1;
    }
    sin_nulos[..corte].to_string()
}

fn acotar_opcional(texto: Option<&str>) -> Option<String> {
    texto
        .and_then(texto_opcional)
        .map(|t| acotar(&t, HitoOperativo::MAX_LONGITUD_CAMPO))
}

fn interpretar_fase(fase: &str) -> FaseOperativa {
    FaseOperativa::parse(fase).unwrap_or_else(|_| FaseOperativa::Otra(FASE_NO_ESPECIFICADA.into()))
}

/// Código canónico de un registro (`for alm 01` → `FOR-ALM-01`); `None` si queda vacío
fn registro_canonico(registro: Option<&str>) -> Option<String> {
    registro
        .map(codigo_canonico)
        .filter(|codigo| !codigo.is_empty())
        .map(|codigo| acotar(&codigo, HitoOperativo::MAX_LONGITUD_CAMPO))
}

/// Filas de la matriz, con su texto y sección de origen, y lo que no llegó a ella
struct HitosMapeados {
    filas: Vec<FilaHito>,
    textos: Vec<String>,
    secciones: Vec<Option<usize>>,
    derivados_capa0: usize,
    origenes_rebajados: usize,
}

fn filas_hitos(
    clave: &ClaveDocumental,
    nivel: NivelPrelacion,
    hitos: &[HitoDestilado],
    secciones: &[SeccionIndexable],
    chunk_ids: &[String],
) -> Result<HitosMapeados, DomainError> {
    let mut filas = Vec::with_capacity(hitos.len());
    let mut textos = Vec::with_capacity(hitos.len());
    let mut secciones_de_hito = Vec::with_capacity(hitos.len());
    let mut derivados_capa0 = 0;
    let mut origenes_rebajados = 0;
    for (idx, destilado) in hitos.iter().enumerate() {
        let seccion = buscar_seccion(&destilado.seccion_origen, secciones);
        let condicion_heredada = seccion.and_then(|i| secciones[i].condicion.clone());
        let origen = OrigenResponsable::verificar(
            &destilado.origen_responsable,
            &destilado.rol_responsable,
            &seccion.map_or_else(String::new, |i| texto_de_seccion(&secciones[i], secciones)),
        );
        let hito = match hito_de_dominio(clave, idx, destilado, condicion_heredada, origen) {
            Ok(hito) => hito,
            Err(DomainError::HitoSinTitular(hito_id)) => {
                tracing::debug!(%hito_id, "hito sin rol ni registro: se deriva a la Capa 0");
                derivados_capa0 += 1;
                continue;
            }
            Err(error) => return Err(error),
        };
        if OrigenResponsable::parse(&destilado.origen_responsable)
            == Some(OrigenResponsable::Declarado)
            && hito.origen_responsable != OrigenResponsable::Declarado
        {
            origenes_rebajados += 1;
        }
        textos.push(hito.texto_vectorizable());
        filas.push(FilaHito {
            hito_id: hito.hito_id.clone(),
            clave: clave.clone(),
            orden_secuencia: hito.orden_secuencia,
            fase_operativa: hito.fase.clone(),
            objeto_alcance: hito.objeto_alcance.clone(),
            rol_ejecutor: hito.rol_ejecutor.clone(),
            rol_responsable: hito.rol_responsable.clone(),
            origen_responsable: hito.origen_responsable,
            registro_codigo: hito.registro_codigo.clone(),
            registro_nombre: hito.registro_nombre.clone(),
            criterio_aceptacion: hito.criterio_aceptacion.clone(),
            condicion_aplicacion: hito.condicion_aplicacion.clone(),
            nivel_prelacion: nivel,
            calidad_extraccion: hito.calidad_extraccion(),
            chunk_origen_id: seccion.map(|i| chunk_ids[i].clone()),
            vector_hito: None,
        });
        secciones_de_hito.push(seccion);
    }
    Ok(HitosMapeados {
        filas,
        textos,
        secciones: secciones_de_hito,
        derivados_capa0,
        origenes_rebajados,
    })
}

/// Títulos ancestros y texto íntegro de la sección, reuniendo todas sus ventanas
fn texto_de_seccion(seccion: &SeccionIndexable, secciones: &[SeccionIndexable]) -> String {
    let ventanas = secciones
        .iter()
        .filter(|otra| {
            codigo_real_de(otra.nodo_id.as_str()) == codigo_real_de(seccion.nodo_id.as_str())
        })
        .map(|otra| otra.texto.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} {ventanas}", seccion.breadcrumb)
}

/// Construye el `HitoOperativo` validado; la condición explícita prevalece sobre la heredada
fn hito_de_dominio(
    clave: &ClaveDocumental,
    idx: usize,
    destilado: &HitoDestilado,
    condicion_heredada: Option<String>,
    origen_responsable: OrigenResponsable,
) -> Result<HitoOperativo, DomainError> {
    let max = HitoOperativo::MAX_LONGITUD_CAMPO;
    let orden_secuencia = u32::try_from(destilado.orden_secuencia)
        .ok()
        .filter(|orden| *orden > 0)
        .unwrap_or(idx as u32 + 1);
    let objeto = texto_opcional(&destilado.objeto_alcance)
        .unwrap_or_else(|| OBJETO_NO_ESPECIFICADO.to_string());
    HitoOperativo::new(BorradorHito {
        hito_id: id_versionado(&clave.doc_id, clave.version_num, TipoEntidad::Hito, idx),
        doc_id: clave.doc_id.clone(),
        orden_secuencia,
        fase: interpretar_fase(&destilado.fase_operativa),
        objeto_alcance: acotar(&objeto, max),
        rol_ejecutor: acotar(&destilado.rol_ejecutor, max),
        rol_responsable: acotar(&destilado.rol_responsable, max),
        origen_responsable,
        registro_codigo: registro_canonico(destilado.registro_codigo.as_deref()),
        registro_nombre: acotar_opcional(destilado.registro_nombre.as_deref()),
        criterio_aceptacion: acotar(&destilado.criterio_aceptacion, max),
        condicion_aplicacion: acotar_opcional(destilado.condicion_aplicacion.as_deref())
            .or(condicion_heredada.map(|c| acotar(&c, max))),
    })
}

fn tipo_eje_declarado(eje: &EjeDestilado) -> Option<TipoEjeTabla> {
    match normalizar_token(&eje.tipo).as_str() {
        "RANGO_NUMERICO" | "RANGO" | "NUMERICO" => Some(TipoEjeTabla::RangoNumerico),
        "CATEGORIA_EXACTA" | "CATEGORIA" | "CATEGORICO" => Some(TipoEjeTabla::CategoriaExacta),
        _ => None,
    }
}

/// Tipo del eje: el declarado o, si no es válido, el que se deduce de las celdas
fn tipo_eje(
    eje: &EjeDestilado,
    celdas: &[CeldaDestilada],
    limites: fn(&CeldaDestilada) -> &LimitesDestilados,
) -> TipoEjeTabla {
    tipo_eje_declarado(eje).unwrap_or_else(|| {
        let tiene_rango = celdas
            .iter()
            .map(limites)
            .any(|l| l.min.is_some() || l.max.is_some());
        if tiene_rango {
            TipoEjeTabla::RangoNumerico
        } else {
            TipoEjeTabla::CategoriaExacta
        }
    })
}

fn eje_tabla(
    eje: &EjeDestilado,
    celdas: &[CeldaDestilada],
    limites: fn(&CeldaDestilada) -> &LimitesDestilados,
) -> EjeTabla {
    EjeTabla {
        nombre: eje.nombre.trim().to_string(),
        tipo: tipo_eje(eje, celdas, limites),
        unidad: texto_opcional(eje.unidad.as_deref().unwrap_or_default()),
    }
}

/// Condición SQL de un eje para el valor de entrada `:parametro`, con inclusividad exacta
fn condicion_sql_eje(prefijo: &str, eje: &EjeTabla) -> String {
    let parametro = format!(":{}", normalizar_token(&eje.nombre).to_lowercase());
    match eje.tipo {
        TipoEjeTabla::CategoriaExacta => format!("{prefijo}_categoria = {parametro}"),
        TipoEjeTabla::RangoNumerico => format!(
            "({prefijo}_min IS NULL OR {prefijo}_min < {parametro} OR ({prefijo}_min_inclusivo AND {prefijo}_min = {parametro})) \
             AND ({prefijo}_max IS NULL OR {prefijo}_max > {parametro} OR ({prefijo}_max_inclusivo AND {prefijo}_max = {parametro}))"
        ),
    }
}

fn describir_eje(eje: &EjeTabla) -> String {
    match &eje.unidad {
        Some(unidad) => format!("{} ({}, {unidad})", eje.nombre, eje.tipo.as_str()),
        None => format!("{} ({})", eje.nombre, eje.tipo.as_str()),
    }
}

/// Instrucciones autogeneradas para cruzar la tabla con SQL exacto (Tabla 04)
fn instrucciones_uso_sql(
    tabla_id: &str,
    eje_x: &EjeTabla,
    eje_y: &EjeTabla,
    valor_salida: &str,
) -> String {
    format!(
        "Entrada X = {} ; entrada Y = {} ; salida = {valor_salida}. \
         SELECT valor_resultado_texto, valor_resultado_num, unidad_medida, nota_pie_condicion \
         FROM \"05_celdas_doble_entrada\" WHERE tabla_id = '{tabla_id}' AND vigente = true \
         AND {} AND {}. Devuelve siempre nota_pie_condicion junto al valor.",
        describir_eje(eje_x),
        describir_eje(eje_y),
        condicion_sql_eje("eje_x", eje_x),
        condicion_sql_eje("eje_y", eje_y)
    )
}

fn texto_vectorizable_tabla(fila: &FilaTabla) -> String {
    format!(
        "Tabla: {} | Entrada X: {} | Entrada Y: {} | Salida: {}",
        fila.titulo_tabla,
        describir_eje(&fila.eje_x),
        describir_eje(&fila.eje_y),
        fila.valor_salida_nombre
    )
}

/// Valor numérico inicial de un resultado (`"100 (*)"` → 100, `"1,5 h"` → 1,5, `"No admisible"` → None)
pub fn interpretar_valor_numerico(valor: &str) -> Option<f64> {
    let numero: String = valor
        .trim()
        .chars()
        .enumerate()
        .take_while(|(i, c)| c.is_ascii_digit() || matches!(c, '.' | ',') || (*i == 0 && *c == '-'))
        .map(|(_, c)| if c == ',' { '.' } else { c })
        .collect();
    numero.parse::<f64>().ok()
}

/// Llamadas a nota que aparecen en el texto de una celda (`125 (*)` → `["(*)"]`)
fn llamadas_de_nota(valor: &str) -> Vec<String> {
    let mut llamadas = Vec::new();
    let mut resto = valor;
    while let Some(inicio) = resto.find('(') {
        let tras = &resto[inicio..];
        match tras.find(')') {
            Some(fin) => {
                let llamada = &tras[..=fin];
                let interior = llamada[1..llamada.len() - 1].trim();
                if !interior.is_empty() && interior.chars().count() <= 3 {
                    llamadas.push(llamada.to_string());
                }
                resto = &tras[fin + 1..];
            }
            None => break,
        }
    }
    llamadas
}

/// Nota del parser para las llamadas de la celda, si el LLM no la entregó (H8)
fn nota_desde_parser(valor: &str, tabla_parseada: Option<&ParsedTableRaw>) -> Option<String> {
    let notas = tabla_parseada?.notas.as_deref()?;
    let encontradas: Vec<&str> = llamadas_de_nota(valor)
        .iter()
        .filter_map(|llamada| {
            notas
                .lines()
                .find(|nota| nota.trim().starts_with(llamada.as_str()))
        })
        .collect();
    (!encontradas.is_empty()).then(|| encontradas.join("\n"))
}

/// Tabla del parser que corresponde a la destilada: por etiqueta o título, o la única que haya
fn tabla_parseada<'a>(
    destilada: &TablaDestilada,
    parseadas: &'a [ParsedTableRaw],
    total_destiladas: usize,
) -> Option<&'a ParsedTableRaw> {
    let etiqueta = plegar_mayusculas(destilada.tabla_id.trim());
    let titulo = plegar_mayusculas(destilada.titulo.trim());
    parseadas
        .iter()
        .find(|t| {
            let titulo_parseado = plegar_mayusculas(&t.titulo);
            (!etiqueta.is_empty() && titulo_parseado.contains(&etiqueta))
                || (!titulo.is_empty() && titulo_parseado == titulo)
        })
        .or_else(|| (parseadas.len() == 1 && total_destiladas == 1).then(|| &parseadas[0]))
}

fn limites_eje(limites: &LimitesDestilados) -> LimitesEje {
    LimitesEje {
        categoria: limites.categoria.as_deref().and_then(texto_opcional),
        minimo: limites.min.filter(|v| v.is_finite()),
        minimo_inclusivo: limites.min_inclusivo,
        maximo: limites.max.filter(|v| v.is_finite()),
        maximo_inclusivo: limites.max_inclusivo,
    }
}

fn fila_celda(
    clave: &ClaveDocumental,
    nivel: NivelPrelacion,
    tabla_id: &str,
    celda_id: String,
    celda: &CeldaDestilada,
    tabla_parseada: Option<&ParsedTableRaw>,
) -> FilaCelda {
    let nota = celda
        .nota_pie_condicion
        .as_deref()
        .and_then(texto_opcional)
        .or_else(|| nota_desde_parser(&celda.valor_resultado, tabla_parseada));
    FilaCelda {
        celda_id,
        tabla_id: tabla_id.to_string(),
        clave: clave.clone(),
        eje_x: limites_eje(&celda.eje_x),
        eje_y: limites_eje(&celda.eje_y),
        eje_z_filtro_adicional: celda.eje_z.as_deref().and_then(texto_opcional),
        valor_resultado_texto: celda.valor_resultado.trim().to_string(),
        valor_resultado_num: interpretar_valor_numerico(&celda.valor_resultado),
        unidad_medida: celda.unidad_medida.as_deref().and_then(texto_opcional),
        nota_pie_condicion: nota,
        nivel_prelacion: nivel,
    }
}

type TablasMapeadas = (Vec<FilaTabla>, Vec<String>, Vec<FilaCelda>);

fn filas_tablas_y_celdas(
    clave: &ClaveDocumental,
    nivel: NivelPrelacion,
    tablas: &[TablaDestilada],
    parseadas: &[ParsedTableRaw],
) -> TablasMapeadas {
    let mut filas_tabla = Vec::with_capacity(tablas.len());
    let mut textos = Vec::with_capacity(tablas.len());
    let mut filas_celda = Vec::new();
    for (idx_tabla, tabla) in tablas.iter().enumerate() {
        let tabla_id = id_versionado(
            &clave.doc_id,
            clave.version_num,
            TipoEntidad::Tabla,
            idx_tabla,
        );
        let parseada = tabla_parseada(tabla, parseadas, tablas.len());
        for celda in &tabla.celdas {
            let celda_id = id_versionado(
                &clave.doc_id,
                clave.version_num,
                TipoEntidad::Celda,
                filas_celda.len(),
            );
            filas_celda.push(fila_celda(
                clave, nivel, &tabla_id, celda_id, celda, parseada,
            ));
        }
        let fila = fila_tabla(clave, &tabla_id, tabla);
        textos.push(texto_vectorizable_tabla(&fila));
        filas_tabla.push(fila);
    }
    (filas_tabla, textos, filas_celda)
}

fn fila_tabla(clave: &ClaveDocumental, tabla_id: &str, tabla: &TablaDestilada) -> FilaTabla {
    let eje_x = eje_tabla(&tabla.eje_x, &tabla.celdas, |c| &c.eje_x);
    let eje_y = eje_tabla(&tabla.eje_y, &tabla.celdas, |c| &c.eje_y);
    let valor_salida_nombre = texto_opcional(&tabla.valor_salida_nombre)
        .unwrap_or_else(|| VALOR_SALIDA_POR_DEFECTO.to_string());
    let titulo = titulo_con_etiqueta(&tabla.tabla_id, texto_opcional(&tabla.titulo));
    FilaTabla {
        tabla_id: tabla_id.to_string(),
        clave: clave.clone(),
        titulo_tabla: titulo,
        instrucciones_uso_sql: instrucciones_uso_sql(
            tabla_id,
            &eje_x,
            &eje_y,
            &valor_salida_nombre,
        ),
        eje_x,
        eje_y,
        valor_salida_nombre,
        vector_tabla: None,
    }
}

/// Destino canónico y revisión separada (`PO-ALM-03 Rev. 03` → `PO-ALM-03`, `Some(3)`)
fn destino_canonico(destino: &str) -> (String, Option<u32>) {
    let (codigo, revision) = separar_revision(destino);
    if codigo.is_empty() {
        (destino.trim().to_string(), revision)
    } else {
        (codigo, revision)
    }
}

fn tipo_nodo(valor: &str) -> Option<TipoNodoGrafo> {
    match normalizar_token(valor).as_str() {
        "DOCUMENTO" => Some(TipoNodoGrafo::Documento),
        "HITO_OPERATIVO" | "HITO" => Some(TipoNodoGrafo::HitoOperativo),
        "CHUNK" => Some(TipoNodoGrafo::Chunk),
        "REGISTRO" => Some(TipoNodoGrafo::Registro),
        "TABLA" => Some(TipoNodoGrafo::Tabla),
        _ => None,
    }
}

/// Tipo del destino: el implícito en la relación si lo hay; si no, el declarado o DOCUMENTO
fn tipo_destino(relacion: TipoRelacionNormativa, declarado: &str) -> TipoNodoGrafo {
    match TipoNodoGrafo::destino_de(relacion) {
        TipoNodoGrafo::Documento => tipo_nodo(declarado)
            .filter(|t| {
                matches!(
                    t,
                    TipoNodoGrafo::Documento | TipoNodoGrafo::Registro | TipoNodoGrafo::Tabla
                )
            })
            .unwrap_or(TipoNodoGrafo::Documento),
        implicito => implicito,
    }
}

/// Arista sin `edge_id` (se asigna al final, tras deduplicar)
#[derive(Debug, Clone, PartialEq)]
struct AristaResuelta {
    origen_id: String,
    origen_tipo: TipoNodoGrafo,
    tipo_relacion: TipoRelacionNormativa,
    destino_id: String,
    destino_tipo: TipoNodoGrafo,
    contexto: Option<String>,
}

/// Origen de la arista: el hito que rellena el registro si el LLM declara origen HITO; si no,
/// el documento (el LLM no conoce los `hito_id` ni los `chunk_id`)
fn resolver_origen(
    clave: &ClaveDocumental,
    declarado: &str,
    destino_id: &str,
    hitos: &[FilaHito],
) -> (String, TipoNodoGrafo) {
    let hito = (tipo_nodo(declarado) == Some(TipoNodoGrafo::HitoOperativo))
        .then(|| {
            hitos
                .iter()
                .find(|h| h.registro_codigo.as_deref() == Some(destino_id))
        })
        .flatten();
    match hito {
        Some(hito) => (hito.hito_id.clone(), TipoNodoGrafo::HitoOperativo),
        None => (clave.doc_id.to_string(), TipoNodoGrafo::Documento),
    }
}

/// Título que conserva la etiqueta humana del documento («Tabla 4-A»): el `tabla_id` real es
/// versionado y sin ella el agente no podría pedir la tabla por su código a la primera
fn titulo_con_etiqueta(etiqueta_original: &str, titulo: Option<String>) -> String {
    let etiqueta = etiqueta_canonica(etiqueta_original);
    let rotulo = format!("Tabla {etiqueta}");
    match titulo {
        _ if etiqueta.is_empty() => titulo.unwrap_or_default(),
        Some(titulo) if contiene_etiqueta(&codigo_canonico(&titulo), &etiqueta) => titulo,
        Some(titulo) => format!("{rotulo} — {titulo}"),
        None => rotulo,
    }
}

/// `etiqueta` aparece como segmento completo del código (`4-A` en `TABLA-4-A-X`, no en `14-A`)
fn contiene_etiqueta(codigo: &str, etiqueta: &str) -> bool {
    codigo.match_indices(etiqueta).any(|(inicio, _)| {
        let fin = inicio + etiqueta.len();
        (inicio == 0 || codigo[..inicio].ends_with('-'))
            && (fin == codigo.len() || codigo[fin..].starts_with('-'))
    })
}

/// Etiqueta canónica de una tabla sin el prefijo «Tabla» (`Tabla 4-A` → `4-A`)
fn etiqueta_canonica(etiqueta: &str) -> String {
    let canonica = codigo_canonico(etiqueta);
    canonica
        .strip_prefix(PREFIJO_ETIQUETA_TABLA)
        .map(str::to_string)
        .unwrap_or(canonica)
}

/// Pares (etiqueta canónica, `tabla_id` versionado) de las tablas de la revisión
fn etiquetas_de_tabla(destiladas: &[TablaDestilada], filas: &[FilaTabla]) -> Vec<(String, String)> {
    destiladas
        .iter()
        .zip(filas)
        .map(|(destilada, fila)| {
            (
                etiqueta_canonica(&destilada.tabla_id),
                fila.tabla_id.clone(),
            )
        })
        .filter(|(etiqueta, _)| !etiqueta.is_empty())
        .collect()
}

/// Una remisión a una tabla de la propia revisión apunta a su `tabla_id` versionado
fn destino_de_remision(destino: String, etiquetas: &[(String, String)]) -> String {
    let etiqueta = etiqueta_canonica(&destino);
    etiquetas
        .iter()
        .find(|(candidata, _)| *candidata == etiqueta)
        .map(|(_, tabla_id)| tabla_id.clone())
        .unwrap_or(destino)
}

fn resolver_arista(
    clave: &ClaveDocumental,
    arista: &AristaDestilada,
    hitos: &[FilaHito],
    etiquetas: &[(String, String)],
) -> AristaResuelta {
    let (tipo_relacion, _fue_respaldo) =
        TipoRelacionNormativa::parse_con_respaldo(&arista.tipo_relacion);
    let (destino_id, revision) = destino_canonico(&arista.destino_id);
    let destino_id = if tipo_relacion == TipoRelacionNormativa::RemiteATabla {
        destino_de_remision(destino_id, etiquetas)
    } else {
        destino_id
    };
    let (origen_id, origen_tipo) = resolver_origen(clave, &arista.origen_tipo, &destino_id, hitos);
    let mut contexto = arista.contexto.as_deref().and_then(texto_opcional);
    if let Some(revision) = revision {
        let nota_revision = format!("Rev. {revision:02}");
        contexto = Some(match contexto {
            Some(texto) if texto.contains(&nota_revision) => texto,
            Some(texto) => format!("{texto} ({nota_revision})"),
            None => nota_revision,
        });
    }
    AristaResuelta {
        origen_id,
        origen_tipo,
        tipo_relacion,
        destino_tipo: tipo_destino(tipo_relacion, &arista.destino_tipo),
        destino_id,
        contexto,
    }
}

/// Aristas `EXIGE_LLENAR_REGISTRO` de los hitos cuyo registro no tiene ya una
fn aristas_de_registro_faltantes(
    hitos: &[FilaHito],
    resueltas: &[AristaResuelta],
) -> Vec<AristaResuelta> {
    let mut cubiertos: HashSet<String> = resueltas
        .iter()
        .filter(|a| a.tipo_relacion == TipoRelacionNormativa::ExigeLlenarRegistro)
        .map(|a| a.destino_id.clone())
        .collect();
    hitos
        .iter()
        .filter_map(|hito| {
            let registro = hito.registro_codigo.clone()?;
            cubiertos.insert(registro.clone()).then(|| AristaResuelta {
                origen_id: hito.hito_id.clone(),
                origen_tipo: TipoNodoGrafo::HitoOperativo,
                tipo_relacion: TipoRelacionNormativa::ExigeLlenarRegistro,
                destino_id: registro,
                destino_tipo: TipoNodoGrafo::Registro,
                contexto: Some(CONTEXTO_ARISTA_SINTETIZADA.to_string()),
            })
        })
        .collect()
}

fn filas_aristas(
    clave: &ClaveDocumental,
    aristas: &[AristaDestilada],
    hitos: &[FilaHito],
    etiquetas: &[(String, String)],
) -> Vec<FilaArista> {
    let mut resueltas: Vec<AristaResuelta> = Vec::with_capacity(aristas.len());
    let mut vistas = HashSet::new();
    for arista in aristas {
        let resuelta = resolver_arista(clave, arista, hitos, etiquetas);
        let identidad = (
            resuelta.origen_id.clone(),
            resuelta.tipo_relacion,
            resuelta.destino_id.clone(),
        );
        if !resuelta.destino_id.is_empty() && vistas.insert(identidad) {
            resueltas.push(resuelta);
        }
    }
    let faltantes = aristas_de_registro_faltantes(hitos, &resueltas);
    resueltas.extend(faltantes);

    resueltas
        .into_iter()
        .enumerate()
        .map(|(idx, arista)| FilaArista {
            edge_id: id_versionado(&clave.doc_id, clave.version_num, TipoEntidad::Arista, idx),
            clave: clave.clone(),
            origen_id: arista.origen_id,
            origen_tipo: arista.origen_tipo,
            tipo_relacion: arista.tipo_relacion,
            destino_id: arista.destino_id,
            destino_tipo: arista.destino_tipo,
            contexto_relacion: arista.contexto,
        })
        .collect()
}

fn vincular_hitos(chunks: &mut [FilaChunk], hitos: &[FilaHito], seccion_de_hito: &[Option<usize>]) {
    for (hito, seccion) in hitos.iter().zip(seccion_de_hito) {
        if let Some(chunk) = seccion.and_then(|idx| chunks.get_mut(idx)) {
            chunk.hitos_vinculados.push(hito.hito_id.clone());
        }
    }
}

/// Vincula cada tabla al chunk de su sección y a los chunks que la citan («Tabla 4-A»)
fn vincular_tablas(
    chunks: &mut [FilaChunk],
    filas: &[FilaTabla],
    destiladas: &[TablaDestilada],
    secciones: &[SeccionIndexable],
    parseadas: &[ParsedTableRaw],
) {
    for (fila, destilada) in filas.iter().zip(destiladas) {
        let seccion_declarada = destilada
            .seccion_origen
            .as_deref()
            .and_then(|referencia| buscar_seccion(referencia, secciones));
        let seccion_parser = tabla_parseada(destilada, parseadas, destiladas.len())
            .and_then(|t| t.seccion_codigo.as_deref())
            .and_then(|codigo| buscar_seccion(codigo, secciones));
        let cita = texto_opcional(&destilada.tabla_id)
            .map(|etiqueta| plegar_mayusculas(&format!("Tabla {etiqueta}")));

        for (idx, chunk) in chunks.iter_mut().enumerate() {
            let es_origen = Some(idx) == seccion_declarada || Some(idx) == seccion_parser;
            let la_cita = cita
                .as_ref()
                .is_some_and(|cita| plegar_mayusculas(&chunk.texto_contenido).contains(cita));
            if es_origen || la_cita {
                chunk.tablas_vinculadas.push(fila.tabla_id.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::operational_matrix::CalidadExtraccion;

    fn clave() -> ClaveDocumental {
        ClaveDocumental {
            centro_id: CenterId::new("planta").unwrap(),
            doc_id: DocId::new("PO-ALM-03").unwrap(),
            version_num: 4,
        }
    }

    fn catalogo(doc_id: &str, version_num: i64, revision: &str) -> CatalogoDestilado {
        CatalogoDestilado {
            doc_id: doc_id.to_string(),
            titulo: "Procedimiento".to_string(),
            tipo_documento: "PROCEDIMIENTO_OBRA".to_string(),
            revision: revision.to_string(),
            version_num,
            fecha_vigor: Some("2026-09-15".to_string()),
            emisor: "Calidad".to_string(),
        }
    }

    fn seccion(
        codigo: &str,
        breadcrumb: &str,
        condicion: Option<&str>,
        texto: &str,
    ) -> ParsedSection {
        ParsedSection {
            codigo: codigo.to_string(),
            titulo: breadcrumb.rsplit(" > ").next().unwrap().to_string(),
            nivel: 3,
            contenido: texto.to_string(),
            breadcrumb: breadcrumb.to_string(),
            condicion_aplicacion: condicion.map(str::to_string),
            ..ParsedSection::default()
        }
    }

    fn hito(
        fase: &str,
        ejecutor: &str,
        responsable: &str,
        registro: Option<&str>,
        seccion: &str,
    ) -> HitoDestilado {
        HitoDestilado {
            orden_secuencia: 0,
            fase_operativa: fase.to_string(),
            objeto_alcance: "Tubería de acero".to_string(),
            rol_ejecutor: ejecutor.to_string(),
            rol_responsable: responsable.to_string(),
            origen_responsable: "DECLARADO".to_string(),
            registro_codigo: registro.map(str::to_string),
            registro_nombre: registro.map(|_| "Parte".to_string()),
            criterio_aceptacion: "Cumple".to_string(),
            condicion_aplicacion: None,
            seccion_origen: seccion.to_string(),
        }
    }

    #[test]
    fn test_hito_sin_titular_se_deriva_a_capa_0_y_se_cuenta() {
        let parsed = ParsedDocument {
            secciones: vec![seccion(
                "2.1",
                "2.1 Acopio",
                None,
                "Texto del acopio con su detalle",
            )],
            ..ParsedDocument::default()
        };
        let mut paquete = DistilledDocumentPackage {
            catalogo: catalogo("PO-ALM-03", 4, "Rev. 04"),
            hitos: vec![
                hito("CUSTODIA", "", "", None, "2.1"),
                hito("CUSTODIA", "N/A", "-", None, "2.1"),
                hito("CUSTODIA", "Almacenero", "", None, "2.1"),
            ],
            tablas: Vec::new(),
            aristas: Vec::new(),
        };
        paquete.hitos[2].origen_responsable = "NO_CONSTA".to_string();
        let revision = mapear_revision(&clave(), &parsed, &paquete, "hash").unwrap();
        assert_eq!(revision.hitos.len(), 1);
        assert_eq!(revision.textos_hitos.len(), 1);
        assert_eq!(revision.hitos[0].rol_ejecutor, "Almacenero");
        assert_eq!(revision.hitos_derivados_capa0, 2);
        assert!(
            revision.avisos.iter().any(|a| a.contains("2 hito(s)")),
            "{:?}",
            revision.avisos
        );
        assert!(
            revision
                .chunks
                .iter()
                .any(|c| c.texto_contenido.contains("Texto del acopio"))
        );
        assert_eq!(
            revision.chunks[0].hitos_vinculados,
            vec![revision.hitos[0].hito_id.clone()]
        );
    }

    #[test]
    fn test_aviso_de_origen_rebajado_no_se_desalinea_con_descartes() {
        let parsed = ParsedDocument {
            secciones: vec![seccion(
                "2.1",
                "2.1 Acopio",
                None,
                "El Inspector firma el parte",
            )],
            ..ParsedDocument::default()
        };
        let paquete = DistilledDocumentPackage {
            catalogo: catalogo("PO-ALM-03", 4, "Rev. 04"),
            hitos: vec![
                hito("CUSTODIA", "", "", None, "2.1"),
                hito("CUSTODIA", "", "Inspector", None, "2.1"),
            ],
            tablas: Vec::new(),
            aristas: Vec::new(),
        };
        let revision = mapear_revision(&clave(), &parsed, &paquete, "hash").unwrap();
        assert_eq!(
            revision.hitos[0].origen_responsable,
            OrigenResponsable::Declarado
        );
        assert!(
            !revision
                .avisos
                .iter()
                .any(|a| a.contains("dados por declarados")),
            "{:?}",
            revision.avisos
        );
    }

    fn paquete_vacio() -> DistilledDocumentPackage {
        DistilledDocumentPackage {
            catalogo: catalogo("PO-ALM-03", 4, "Rev. 04"),
            hitos: Vec::new(),
            tablas: Vec::new(),
            aristas: Vec::new(),
        }
    }

    fn con_paginas(paginas: &[(u32, usize)]) -> ParsedSection {
        let mut contenido = String::new();
        let mut saltos = Vec::new();
        for (pagina, palabras) in paginas {
            if !contenido.is_empty() {
                contenido.push('\n');
            }
            saltos.push(crate::application::dtos::ingestion::SaltoPagina {
                desplazamiento: contenido.len(),
                pagina: *pagina,
            });
            contenido.push_str(&vec![format!("pag{pagina}"); *palabras].join(" "));
        }
        ParsedSection {
            pagina_inicio: paginas.first().map(|p| p.0),
            pagina_fin: paginas.last().map(|p| p.0),
            saltos_pagina: saltos,
            ..seccion("2.1", "2.1 Largo", None, &contenido)
        }
    }

    #[test]
    fn test_paginas_de_cada_chunk_salen_de_los_saltos_y_no_de_la_seccion() {
        let parsed = ParsedDocument {
            secciones: vec![con_paginas(&[(7, 400), (8, 400), (9, 400)])],
            ..ParsedDocument::default()
        };
        let revision = mapear_revision(&clave(), &parsed, &paquete_vacio(), "h").unwrap();
        assert!(revision.chunks.len() >= 2, "{}", revision.chunks.len());
        let primero = &revision.chunks[0];
        let ultimo = revision.chunks.last().unwrap();
        assert_eq!(primero.pagina_inicio, Some(7));
        assert!(primero.pagina_fin < Some(9), "{:?}", primero.pagina_fin);
        assert_eq!(ultimo.pagina_fin, Some(9));
        assert!(ultimo.pagina_inicio > Some(7), "{:?}", ultimo.pagina_inicio);
        for chunk in &revision.chunks {
            for pagina in 7..=9 {
                let la_contiene = chunk.texto_contenido.contains(&format!("pag{pagina}"));
                let en_rango =
                    (chunk.pagina_inicio.unwrap()..=chunk.pagina_fin.unwrap()).contains(&pagina);
                assert_eq!(
                    la_contiene, en_rango,
                    "pag{pagina} en {:?}-{:?}",
                    chunk.pagina_inicio, chunk.pagina_fin
                );
            }
        }
    }

    #[test]
    fn test_saltos_de_cada_chunk_son_relativos_a_su_texto() {
        use crate::domain::services::marcas_pagina::{texto_con_marcas, tramos_paginados};
        let parsed = ParsedDocument {
            secciones: vec![con_paginas(&[(7, 400), (8, 400), (9, 400)])],
            ..ParsedDocument::default()
        };
        let revision = mapear_revision(&clave(), &parsed, &paquete_vacio(), "h").unwrap();
        assert!(revision.chunks.len() >= 2);
        for chunk in &revision.chunks {
            let saltos = chunk
                .saltos_pagina
                .as_ref()
                .expect("un PDF guarda sus saltos");
            assert_eq!(saltos[0].desplazamiento, 0);
            assert_eq!(Some(saltos[0].pagina), chunk.pagina_inicio);
            assert_eq!(saltos.last().map(|s| s.pagina), chunk.pagina_fin);
            let marcado = texto_con_marcas(&chunk.texto_contenido, saltos);
            let tramos = tramos_paginados(&marcado);
            assert_eq!(tramos.len(), saltos.len());
            for tramo in tramos {
                let pagina = tramo.pagina.expect("todo el texto queda tras una marca");
                assert!(
                    tramo
                        .texto
                        .split_whitespace()
                        .all(|p| p == format!("pag{pagina}")),
                    "p. {pagina}: {}",
                    tramo.texto
                );
            }
        }
    }

    #[test]
    fn test_saltos_nulos_si_no_se_sabe_donde_cambia_la_pagina() {
        let de = |seccion: ParsedSection| {
            let parsed = ParsedDocument {
                secciones: vec![seccion],
                ..ParsedDocument::default()
            };
            mapear_revision(&clave(), &parsed, &paquete_vacio(), "h")
                .unwrap()
                .chunks
        };
        let markdown = de(seccion("2.1", "2.1 Texto", None, "sin paginas"));
        assert!(markdown.iter().all(|c| c.saltos_pagina.is_none()));

        let una_pagina = de(ParsedSection {
            pagina_inicio: Some(5),
            pagina_fin: Some(5),
            ..seccion("2.1", "2.1 Texto", None, "respaldo en una pagina")
        });
        assert_eq!(
            una_pagina[0].saltos_pagina,
            Some(vec![SaltoPagina {
                desplazamiento: 0,
                pagina: 5
            }])
        );

        let varias = de(ParsedSection {
            pagina_inicio: Some(3),
            pagina_fin: Some(5),
            ..seccion("2.1", "2.1 Texto", None, "respaldo de tres paginas")
        });
        assert_eq!(varias[0].saltos_pagina, None);
        assert_eq!(varias[0].pagina_fin, Some(5));
    }

    #[test]
    fn test_chunk_a_caballo_de_dos_paginas_y_sin_paginas_en_markdown() {
        let parsed = ParsedDocument {
            secciones: vec![con_paginas(&[(3, 10), (4, 10)])],
            ..ParsedDocument::default()
        };
        let revision = mapear_revision(&clave(), &parsed, &paquete_vacio(), "h").unwrap();
        assert_eq!(revision.chunks.len(), 1);
        assert_eq!(
            (
                revision.chunks[0].pagina_inicio,
                revision.chunks[0].pagina_fin
            ),
            (Some(3), Some(4))
        );

        let markdown = ParsedDocument {
            secciones: vec![seccion("2.1", "2.1 Texto", None, &"palabra ".repeat(900))],
            ..ParsedDocument::default()
        };
        let revision = mapear_revision(&clave(), &markdown, &paquete_vacio(), "h").unwrap();
        assert!(revision.chunks.len() > 1);
        assert!(
            revision
                .chunks
                .iter()
                .all(|c| c.pagina_inicio.is_none() && c.pagina_fin.is_none())
        );
    }

    #[test]
    fn test_sub_nodo_sintetico_en_07_y_02_sin_romper_el_enlace_del_hito() {
        let real = ParsedSection {
            nivel: 2,
            ..seccion("2.9", "2.9 Largo", Some("con lluvia"), "")
        };
        let sub = |codigo: &str, texto: &str, pagina| ParsedSection {
            codigo: codigo.to_string(),
            titulo: format!("Páginas de {codigo}"),
            nivel: 3,
            contenido: texto.to_string(),
            breadcrumb: "2.9 Largo".to_string(),
            condicion_aplicacion: Some("con lluvia".to_string()),
            pagina_inicio: Some(pagina),
            pagina_fin: Some(pagina),
            tipo_nodo: TipoNodoIndice::Sintetico,
            ..ParsedSection::default()
        };
        let parsed = ParsedDocument {
            secciones: vec![
                real,
                sub("2.9.PAG1-10", "primer bloque de texto", 1),
                sub("2.9.PAG11-20", "El Inspector firma el parte", 11),
                ParsedSection {
                    nivel: 2,
                    ..seccion("2.10", "2.10 Otro", None, "texto")
                },
            ],
            ..ParsedDocument::default()
        };
        let mut paquete = paquete_vacio();
        paquete.hitos = vec![hito("CUSTODIA", "", "Inspector", None, "2.9")];
        let revision = mapear_revision(&clave(), &parsed, &paquete, "h").unwrap();
        let tipos: Vec<(&str, TipoNodoIndice)> = revision
            .nodos
            .iter()
            .map(|n| (n.codigo_seccion.as_str(), n.tipo_nodo))
            .collect();
        assert_eq!(tipos[0], ("2.9", TipoNodoIndice::Real));
        assert_eq!(tipos[1], ("2.9.PAG1-10", TipoNodoIndice::Sintetico));
        assert_eq!(
            revision.nodos[1].nodo_padre_id.as_ref(),
            Some(&revision.nodos[0].nodo_id)
        );
        assert_eq!(revision.nodos[3].nodo_padre_id, None);
        assert_eq!(
            revision.nodos[1].condiciones_alcance.as_deref(),
            Some("con lluvia")
        );
        // El hito apunta a «2.9», cuyo texto está repartido entre sus sub-nodos
        assert_eq!(
            revision.hitos[0].origen_responsable,
            OrigenResponsable::Declarado
        );
        assert!(revision.hitos[0].chunk_origen_id.is_some());
        assert_eq!(revision.chunks[0].breadcrumb_jerarquia, "2.9 Largo");
    }

    fn eje(nombre: &str, tipo: &str, unidad: Option<&str>) -> EjeDestilado {
        EjeDestilado {
            nombre: nombre.to_string(),
            tipo: tipo.to_string(),
            unidad: unidad.map(str::to_string),
        }
    }

    fn rango(min: Option<f64>, max: Option<f64>) -> LimitesDestilados {
        LimitesDestilados {
            categoria: None,
            min,
            min_inclusivo: false,
            max,
            max_inclusivo: true,
        }
    }

    fn documento() -> ParsedDocument {
        ParsedDocument {
            nombre_archivo: "po.md".to_string(),
            titulo: "PO".to_string(),
            secciones: vec![
                seccion(
                    "2.2",
                    "PO > 2. Secuencia > 2.2 Fase 2 — CUSTODIA",
                    None,
                    "Custodia. Firma el Jefe de Producción.",
                ),
                seccion(
                    "4.2.1",
                    "PO > 4.2 Soldadura > 4.2.1 Bajo cero (< 0 °C)",
                    Some("Bajo cero (< 0 °C)"),
                    "Calentar a 80 °C. Ver Tabla 4-A.",
                ),
                seccion(
                    "3.1",
                    "PO > 3.1 Precalentamiento",
                    None,
                    "Tabla de precalentamiento.",
                ),
            ],
            tablas: vec![ParsedTableRaw {
                titulo: "Tabla 4-A Temperatura".to_string(),
                encabezados: Vec::new(),
                filas: Vec::new(),
                notas: Some(
                    "(*) Requiere aprobación del Inspector QA/QC.\n(1) Interpaso < 250 °C."
                        .to_string(),
                ),
                seccion_codigo: Some("3.1".to_string()),
            }],
            contenido_completo: String::new(),
            avisos_extraccion: Vec::new(),
            requiere_ocr: false,
        }
    }

    fn paquete() -> DistilledDocumentPackage {
        DistilledDocumentPackage {
            catalogo: catalogo("PO-ALM-03", 4, "Rev. 04"),
            hitos: vec![
                hito(
                    "Custodia",
                    "Auxiliar de Almacén",
                    "Jefe de Producción",
                    Some("for alm 05"),
                    "2.2",
                ),
                hito("SOLDEO", "Soldador", "Jefe de Obra", None, "4.2.1"),
            ],
            tablas: vec![TablaDestilada {
                tabla_id: "4-A".to_string(),
                titulo: "Temperatura mínima de precalentamiento".to_string(),
                eje_x: eje("espesor_nominal_mm", "RANGO_NUMERICO", Some("mm")),
                eje_y: eje("carbono_equivalente_pct", "inventado", Some("%")),
                valor_salida_nombre: "temperatura_min_c".to_string(),
                seccion_origen: None,
                celdas: vec![CeldaDestilada {
                    eje_x: rango(Some(20.0), Some(40.0)),
                    eje_y: rango(Some(0.40), Some(0.45)),
                    eje_z: None,
                    valor_resultado: "125 (*)".to_string(),
                    unidad_medida: Some("°C".to_string()),
                    nota_pie_condicion: None,
                }],
            }],
            aristas: vec![AristaDestilada {
                origen_tipo: "DOCUMENTO".to_string(),
                tipo_relacion: "DEROGA".to_string(),
                destino_id: "PO-ALM-03 Rev. 03".to_string(),
                destino_tipo: "DOCUMENTO".to_string(),
                contexto: None,
            }],
        }
    }

    #[test]
    fn test_doc_id_provisional_es_slug_en_mayusculas_con_seis_hex() {
        let hash = "a4f2b9".to_string() + &"0".repeat(58);
        let id = |nombre: &str| doc_id_provisional(nombre, &hash).unwrap().to_string();
        assert_eq!(id("manual válvulas.md"), "MANUAL-VALVULAS_a4f2b9");
        assert_eq!(id("!!!.md"), "DOC_a4f2b9");
        assert_eq!(id("..\\..\\a'; DROP.md"), "A-DROP_a4f2b9");
        assert!(id(&"x".repeat(5000)).len() <= MAX_LONGITUD_SLUG + 7);
    }

    #[test]
    fn test_revision_provisional_lleva_respaldo_parcial_y_sin_capa_2() {
        let centro = CenterId::new("planta").unwrap();
        let hash = "a".repeat(64);
        let (clave, revision) = mapear_revision_provisional(&centro, &documento(), &hash).unwrap();
        assert_eq!(clave.doc_id.as_str(), "PO_aaaaaa");
        assert_eq!(
            revision.catalogo.doc_id_provisional,
            Some(clave.doc_id.clone())
        );
        assert_eq!(
            revision.catalogo.tipo_documento.nivel_prelacion().valor(),
            5
        );
        assert_eq!(revision.catalogo.revision, "Provisional");
        assert!(revision.hitos.is_empty() && revision.tablas.is_empty());
        assert!(
            revision
                .chunks
                .iter()
                .all(|c| c.estado_contenido == EstadoContenido::ParcialUniversal)
        );
        assert!(
            revision
                .nodos
                .iter()
                .all(|n| n.estado_contenido == EstadoContenido::ParcialUniversal)
        );
    }

    #[test]
    fn test_seccion_larga_comparte_nodo_y_hito_apunta_al_primer_fragmento() {
        let mut doc = documento();
        doc.secciones[0].contenido = "palabra ".repeat(1500);
        let revision = mapear_revision(&clave(), &doc, &paquete(), "hash").unwrap();
        let fragmentos: Vec<&FilaChunk> = revision
            .chunks
            .iter()
            .filter(|c| c.nodo_id == revision.nodos[0].nodo_id)
            .collect();
        assert!(fragmentos.len() > 1);
        assert!(
            fragmentos
                .iter()
                .all(|c| { c.breadcrumb_jerarquia == doc.secciones[0].breadcrumb })
        );
        assert_eq!(
            revision.hitos[0].chunk_origen_id.as_deref(),
            Some(fragmentos[0].chunk_id.as_str())
        );
        assert_eq!(revision.textos_chunks.len(), revision.chunks.len());
    }

    #[test]
    fn test_provisional_no_toma_la_fecha_de_la_ingesta() {
        let centro = CenterId::new("planta").unwrap();
        let mut parsed = documento();
        parsed.nombre_archivo = "sin_destilar.md".to_string();
        let (_, revision) = mapear_revision_provisional(&centro, &parsed, "abcdef0123").unwrap();
        assert_eq!(revision.catalogo.vigencia.fecha_vigor(), None);
        assert!(revision.avisos.is_empty());
    }

    #[test]
    fn test_clave_canonica_separa_revision_y_resuelve_version() {
        let centro = CenterId::new("planta").unwrap();
        let clave =
            clave_documental(&centro, &catalogo("po alm 03 Rev. 04", 4, "Rev. 04")).unwrap();
        assert_eq!(clave.doc_id.as_str(), "PO-ALM-03");
        assert_eq!(clave.version_num, 4);
        let desde_revision =
            clave_documental(&centro, &catalogo("PO-ALM-03", -1, "Rev. 07")).unwrap();
        assert_eq!(desde_revision.version_num, 7);
        let desde_id = clave_documental(&centro, &catalogo("PO-ALM-03 Rev. 02", -1, "")).unwrap();
        assert_eq!(desde_id.version_num, 2);
        assert!(clave_documental(&centro, &catalogo("PO-ALM-03", -1, "sin número")).is_err());
    }

    #[test]
    fn test_hitos_normalizados_con_registro_chunk_y_condicion_heredada() {
        let revision = mapear_revision(&clave(), &documento(), &paquete(), "hash").unwrap();
        let custodia = &revision.hitos[0];
        assert_eq!(custodia.fase_operativa, FaseOperativa::Custodia);
        assert_eq!(custodia.orden_secuencia, 1);
        assert_eq!(custodia.registro_codigo.as_deref(), Some("FOR-ALM-05"));
        assert_eq!(custodia.rol_ejecutor, "Auxiliar de Almacén");
        assert_eq!(custodia.calidad_extraccion, CalidadExtraccion::Ok);
        assert_eq!(
            custodia.chunk_origen_id.as_deref(),
            Some(revision.chunks[0].chunk_id.as_str())
        );

        let soldeo = &revision.hitos[1];
        assert_eq!(soldeo.fase_operativa.as_str(), "SOLDEO");
        assert_eq!(
            soldeo.condicion_aplicacion.as_deref(),
            Some("Bajo cero (< 0 °C)")
        );
        assert_eq!(soldeo.calidad_extraccion, CalidadExtraccion::Revisar);
        assert!(revision.textos_hitos[1].contains("Condición: Bajo cero"));
        assert_eq!(
            revision.chunks[1].hitos_vinculados,
            vec![soldeo.hito_id.clone()]
        );
    }

    #[test]
    fn test_chunk_vectoriza_breadcrumb_condicion_y_texto() {
        let revision = mapear_revision(&clave(), &documento(), &paquete(), "hash").unwrap();
        assert_eq!(
            revision.textos_chunks[1],
            "PO > 4.2 Soldadura > 4.2.1 Bajo cero (< 0 °C)\nCondición: Bajo cero (< 0 °C)\n\
             Calentar a 80 °C. Ver Tabla 4-A."
        );
        assert_eq!(
            revision.chunks[1].condicion_aplicacion.as_deref(),
            Some("Bajo cero (< 0 °C)")
        );
    }

    #[test]
    fn test_tabla_con_unidades_sql_autogenerado_vinculos_y_nota_del_parser() {
        let revision = mapear_revision(&clave(), &documento(), &paquete(), "hash").unwrap();
        let tabla = &revision.tablas[0];
        assert_eq!(tabla.eje_x.unidad.as_deref(), Some("mm"));
        assert_eq!(
            tabla.eje_y.tipo,
            TipoEjeTabla::RangoNumerico,
            "tipo deducido de las celdas"
        );
        assert_eq!(tabla.valor_salida_nombre, "temperatura_min_c");
        assert!(
            tabla
                .instrucciones_uso_sql
                .contains("tabla_id = 'PO-ALM-03@v4-TAB-0000'")
        );
        assert!(
            tabla
                .instrucciones_uso_sql
                .contains("eje_x_min_inclusivo AND eje_x_min = :espesor_nominal_mm")
        );
        assert!(revision.textos_tablas[0].contains("espesor_nominal_mm (RANGO_NUMERICO, mm)"));

        let celda = &revision.celdas[0];
        assert_eq!(celda.valor_resultado_num, Some(125.0));
        assert_eq!(
            celda.nota_pie_condicion.as_deref(),
            Some("(*) Requiere aprobación del Inspector QA/QC.")
        );
        // Origen por el parser (3.1) y cita literal en 4.2.1
        assert_eq!(
            revision.chunks[2].tablas_vinculadas,
            vec![tabla.tabla_id.clone()]
        );
        assert_eq!(
            revision.chunks[1].tablas_vinculadas,
            vec![tabla.tabla_id.clone()]
        );
        assert!(revision.chunks[0].tablas_vinculadas.is_empty());
    }

    #[test]
    fn test_titulo_de_tabla_conserva_la_etiqueta_original_del_documento() {
        let revision = mapear_revision(&clave(), &documento(), &paquete(), "hash").unwrap();
        assert_eq!(
            revision.tablas[0].titulo_tabla, "Tabla 4-A — Temperatura mínima de precalentamiento",
            "el código humano «4-A» debe sobrevivir al id versionado"
        );

        let mut con_etiqueta = paquete();
        con_etiqueta.tablas[0].titulo = "Tabla 4-A Temperatura".to_string();
        let revision = mapear_revision(&clave(), &documento(), &con_etiqueta, "hash").unwrap();
        assert_eq!(
            revision.tablas[0].titulo_tabla, "Tabla 4-A Temperatura",
            "si el título ya lleva la etiqueta no se duplica"
        );

        let mut sin_titulo = paquete();
        sin_titulo.tablas[0].titulo = "  ".to_string();
        let revision = mapear_revision(&clave(), &documento(), &sin_titulo, "hash").unwrap();
        assert_eq!(revision.tablas[0].titulo_tabla, "Tabla 4-A");
    }

    #[test]
    fn test_aristas_canonicas_deroga_con_revision_y_registro_sintetizado() {
        let revision = mapear_revision(&clave(), &documento(), &paquete(), "hash").unwrap();
        let deroga = &revision.aristas[0];
        assert_eq!(deroga.tipo_relacion, TipoRelacionNormativa::DerogaA);
        assert_eq!(deroga.destino_id, "PO-ALM-03");
        assert_eq!(deroga.contexto_relacion.as_deref(), Some("Rev. 03"));
        assert_eq!(deroga.edge_id, "PO-ALM-03@v4-ARI-0000");

        let registro = &revision.aristas[1];
        assert_eq!(
            registro.tipo_relacion,
            TipoRelacionNormativa::ExigeLlenarRegistro
        );
        assert_eq!(registro.destino_id, "FOR-ALM-05");
        assert_eq!(registro.destino_tipo, TipoNodoGrafo::Registro);
        assert_eq!(registro.origen_tipo, TipoNodoGrafo::HitoOperativo);
        assert_eq!(registro.origen_id, revision.hitos[0].hito_id);
        assert_eq!(revision.aristas.len(), 2);
    }

    #[test]
    fn test_remision_a_tabla_por_etiqueta_apunta_al_tabla_id_versionado() {
        let mut paquete = paquete();
        for destino in ["4-A", "Tabla 4-A", "Tabla 9-Z"] {
            paquete.aristas.push(AristaDestilada {
                origen_tipo: "DOCUMENTO".to_string(),
                tipo_relacion: "REMITE_A_TABLA".to_string(),
                destino_id: destino.to_string(),
                destino_tipo: "TABLA".to_string(),
                contexto: None,
            });
        }
        let revision = mapear_revision(&clave(), &documento(), &paquete, "hash").unwrap();
        let destinos: Vec<&str> = revision
            .aristas
            .iter()
            .filter(|a| a.tipo_relacion == TipoRelacionNormativa::RemiteATabla)
            .map(|a| a.destino_id.as_str())
            .collect();
        assert_eq!(destinos, ["PO-ALM-03@v4-TAB-0000", "TABLA-9-Z"]);
    }

    #[test]
    fn test_arista_de_registro_del_llm_no_se_duplica_y_toma_el_hito_como_origen() {
        let mut paquete = paquete();
        let arista = AristaDestilada {
            origen_tipo: "HITO_OPERATIVO".to_string(),
            tipo_relacion: "EXIGE_LLENAR_REGISTRO".to_string(),
            destino_id: "FOR-ALM-05".to_string(),
            destino_tipo: "DOCUMENTO".to_string(),
            contexto: None,
        };
        paquete.aristas = vec![arista.clone(), arista];
        let revision = mapear_revision(&clave(), &documento(), &paquete, "hash").unwrap();
        assert_eq!(revision.aristas.len(), 1);
        assert_eq!(revision.aristas[0].origen_id, revision.hitos[0].hito_id);
        assert_eq!(revision.aristas[0].destino_tipo, TipoNodoGrafo::Registro);
    }

    #[test]
    fn test_valor_numerico_fecha_y_acotado() {
        assert_eq!(interpretar_valor_numerico("100 (*)"), Some(100.0));
        assert_eq!(interpretar_valor_numerico(" 1,5 h"), Some(1.5));
        assert_eq!(interpretar_valor_numerico("-10 °C"), Some(-10.0));
        assert_eq!(interpretar_valor_numerico("No admisible"), None);
        assert_eq!(acotar("ñandú\0", 3), "ña");
        assert_eq!(acotar("ñandú", 2), "ñ");
        assert_eq!(acotar("x\0y", 9), "xy");
        assert_eq!(
            llamadas_de_nota("150 (1) y (*) (texto largo)"),
            vec!["(1)", "(*)"]
        );
    }
}
