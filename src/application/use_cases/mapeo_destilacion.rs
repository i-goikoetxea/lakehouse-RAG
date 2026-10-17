//! Mapeo puro del paquete destilado v2 (más el documento parseado) a las filas de las 6 tablas.
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

use chrono::{NaiveDate, Utc};

use crate::application::dtos::ingestion::{
    AristaDestilada, CatalogoDestilado, CeldaDestilada, DistilledDocumentPackage, EjeDestilado,
    HitoDestilado, LimitesDestilados, ParsedDocument, ParsedTableRaw, TablaDestilada,
};
use crate::application::dtos::lakehouse_filas::{
    ClaveDocumental, EjeTabla, FilaArista, FilaCatalogo, FilaCelda, FilaChunk, FilaHito, FilaTabla,
    LimitesEje, TipoEjeTabla, TipoNodoGrafo,
};
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;
use crate::domain::models::document::{DocId, NivelPrelacion, TipoDocumento};
use crate::domain::models::identificadores::{TipoEntidad, id_versionado};
use crate::domain::models::operational_matrix::{BorradorHito, FaseOperativa, HitoOperativo};
use crate::domain::models::reference_graph::TipoRelacionNormativa;
use crate::domain::models::vigencia::VentanaVigencia;
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

/// Filas de una revisión sin vectores, con los textos que hay que vectorizar
#[derive(Debug, Clone, PartialEq)]
pub struct RevisionMapeada {
    pub catalogo: FilaCatalogo,
    pub chunks: Vec<FilaChunk>,
    pub hitos: Vec<FilaHito>,
    pub tablas: Vec<FilaTabla>,
    pub celdas: Vec<FilaCelda>,
    pub aristas: Vec<FilaArista>,
    /// `breadcrumb + condición + texto` de cada chunk (mismo orden que `chunks`)
    pub textos_chunks: Vec<String>,
    /// `HitoOperativo::texto_vectorizable` de cada hito
    pub textos_hitos: Vec<String>,
    /// Descripción y variables de cada tabla
    pub textos_tablas: Vec<String>,
}

/// Apartado del documento que se convierte en chunk
#[derive(Debug, Clone, PartialEq)]
struct SeccionIndexable {
    codigo: String,
    breadcrumb: String,
    condicion: Option<String>,
    texto: String,
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

/// Mapea el paquete destilado y el documento parseado a las filas de la revisión
pub fn mapear_revision(
    clave: &ClaveDocumental,
    parsed: &ParsedDocument,
    paquete: &DistilledDocumentPackage,
    hash_sha256: &str,
) -> Result<RevisionMapeada, DomainError> {
    let tipo_documento = TipoDocumento::parse(&paquete.catalogo.tipo_documento)
        .unwrap_or(TIPO_DOCUMENTO_POR_DEFECTO);
    let nivel = tipo_documento.nivel_prelacion();
    let catalogo = fila_catalogo(clave, &paquete.catalogo, tipo_documento, hash_sha256)?;

    let secciones = secciones_indexables(parsed);
    let chunk_ids: Vec<String> = (0..secciones.len())
        .map(|idx| id_versionado(&clave.doc_id, clave.version_num, TipoEntidad::Chunk, idx))
        .collect();

    let (hitos, textos_hitos, seccion_de_hito) =
        filas_hitos(clave, nivel, &paquete.hitos, &secciones, &chunk_ids)?;
    let (tablas, textos_tablas, celdas) =
        filas_tablas_y_celdas(clave, nivel, &paquete.tablas, &parsed.tablas);
    let etiquetas = etiquetas_de_tabla(&paquete.tablas, &tablas);
    let aristas = filas_aristas(clave, &paquete.aristas, &hitos, &etiquetas);

    let mut chunks = filas_chunks_sin_vector(clave, nivel, &secciones, &chunk_ids);
    vincular_hitos(&mut chunks, &hitos, &seccion_de_hito);
    vincular_tablas(
        &mut chunks,
        &tablas,
        &paquete.tablas,
        &secciones,
        &parsed.tablas,
    );
    let textos_chunks = secciones.iter().map(texto_vectorizable_chunk).collect();

    Ok(RevisionMapeada {
        catalogo,
        chunks,
        hitos,
        tablas,
        celdas,
        aristas,
        textos_chunks,
        textos_hitos,
        textos_tablas,
    })
}

/// `fecha_vigor` en ISO (`AAAA-MM-DD…`); si no se interpreta, la fecha de la ingesta
pub fn interpretar_fecha_vigor(fecha: &str) -> NaiveDate {
    let prefijo: String = fecha.trim().chars().take(10).collect();
    NaiveDate::parse_from_str(&prefijo, "%Y-%m-%d").unwrap_or_else(|_| Utc::now().date_naive())
}

fn fila_catalogo(
    clave: &ClaveDocumental,
    catalogo: &CatalogoDestilado,
    tipo_documento: TipoDocumento,
    hash_sha256: &str,
) -> Result<FilaCatalogo, DomainError> {
    let revision = texto_opcional(&catalogo.revision)
        .unwrap_or_else(|| format!("Rev. {:02}", clave.version_num));
    Ok(FilaCatalogo {
        clave: clave.clone(),
        titulo: catalogo.titulo.trim().to_string(),
        tipo_documento,
        revision,
        vigencia: VentanaVigencia::new(interpretar_fecha_vigor(&catalogo.fecha_vigor), None)?,
        hash_sha256: hash_sha256.to_string(),
    })
}

/// Secciones no vacías del documento; si no hay ninguna, el documento entero
fn secciones_indexables(parsed: &ParsedDocument) -> Vec<SeccionIndexable> {
    let secciones: Vec<SeccionIndexable> = parsed
        .secciones
        .iter()
        .filter(|sec| !sec.contenido.trim().is_empty())
        .map(|sec| SeccionIndexable {
            codigo: sec.codigo.clone(),
            breadcrumb: sec.breadcrumb.clone(),
            condicion: sec.condicion_aplicacion.clone(),
            texto: sec.contenido.trim().to_string(),
        })
        .collect();
    if secciones.is_empty() {
        vec![SeccionIndexable {
            codigo: String::new(),
            breadcrumb: parsed.titulo.clone(),
            condicion: None,
            texto: parsed.contenido_completo.trim().to_string(),
        }]
    } else {
        secciones
    }
}

/// Texto que se vectoriza para un chunk: `breadcrumb + condición + texto` (problema 7)
fn texto_vectorizable_chunk(seccion: &SeccionIndexable) -> String {
    let mut partes = vec![seccion.breadcrumb.clone()];
    if let Some(condicion) = &seccion.condicion {
        partes.push(format!("Condición: {condicion}"));
    }
    partes.push(seccion.texto.clone());
    partes.retain(|p| !p.trim().is_empty());
    partes.join("\n")
}

fn filas_chunks_sin_vector(
    clave: &ClaveDocumental,
    nivel: NivelPrelacion,
    secciones: &[SeccionIndexable],
    chunk_ids: &[String],
) -> Vec<FilaChunk> {
    secciones
        .iter()
        .zip(chunk_ids)
        .map(|(seccion, chunk_id)| FilaChunk {
            chunk_id: chunk_id.clone(),
            clave: clave.clone(),
            nivel_prelacion: nivel,
            breadcrumb_jerarquia: seccion.breadcrumb.clone(),
            condicion_aplicacion: seccion.condicion.clone(),
            texto_contenido: seccion.texto.clone(),
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

type HitosMapeados = (Vec<FilaHito>, Vec<String>, Vec<Option<usize>>);

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
    for (idx, destilado) in hitos.iter().enumerate() {
        let seccion = buscar_seccion(&destilado.seccion_origen, secciones);
        let condicion_heredada = seccion.and_then(|i| secciones[i].condicion.clone());
        let hito = hito_de_dominio(clave, idx, destilado, condicion_heredada)?;
        textos.push(hito.texto_vectorizable());
        filas.push(FilaHito {
            hito_id: hito.hito_id.clone(),
            clave: clave.clone(),
            orden_secuencia: hito.orden_secuencia,
            fase_operativa: hito.fase.clone(),
            objeto_alcance: hito.objeto_alcance.clone(),
            rol_ejecutor: hito.rol_ejecutor.clone(),
            rol_responsable: hito.rol_responsable.clone(),
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
    Ok((filas, textos, secciones_de_hito))
}

/// Construye el `HitoOperativo` validado; la condición explícita prevalece sobre la heredada
fn hito_de_dominio(
    clave: &ClaveDocumental,
    idx: usize,
    destilado: &HitoDestilado,
    condicion_heredada: Option<String>,
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
    let titulo = texto_opcional(&tabla.titulo).unwrap_or_else(|| tabla.tabla_id.trim().to_string());
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
    use crate::application::dtos::ingestion::ParsedSection;
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
            fecha_vigor: "2026-09-15".to_string(),
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
            registro_codigo: registro.map(str::to_string),
            registro_nombre: registro.map(|_| "Parte".to_string()),
            criterio_aceptacion: "Cumple".to_string(),
            condicion_aplicacion: None,
            seccion_origen: seccion.to_string(),
        }
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
                    "Custodia.",
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
        assert_eq!(
            interpretar_fecha_vigor("2026-09-15T00:00:00Z"),
            NaiveDate::from_ymd_opt(2026, 9, 15).unwrap()
        );
        assert_eq!(
            interpretar_fecha_vigor("sin fecha"),
            Utc::now().date_naive()
        );
        assert_eq!(acotar("ñandú\0", 3), "ña");
        assert_eq!(acotar("ñandú", 2), "ñ");
        assert_eq!(acotar("x\0y", 9), "xy");
        assert_eq!(
            llamadas_de_nota("150 (1) y (*) (texto largo)"),
            vec!["(1)", "(*)"]
        );
    }
}
