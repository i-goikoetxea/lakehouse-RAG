//! `search_normative_context`: búsqueda sobre los chunks (02, corpus sin pérdidas) y los hitos
//! (03) del centro, con vigencia, prelación y condición heredada (problemas 5 y 7). `modo`
//! elige coseno + BM25 fusionados con RRF, solo coseno o solo BM25, siempre en memoria. La
//! búsqueda se puede restringir a una sección real del índice (07): si no existe, es un error.
//! Los resultados se agrupan por tema (título de la sección) y, si en un tema hay documentos de
//! distinta prelación, `prevalece` marca el de menor nivel. Un provisional nunca prevalece.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::LakehouseToolService;
use super::estructura::{
    NOTA_MARCAS_DE_PAGINA, NOTA_SEGURIDAD, NodoIndice, RevisionDocumental, anadir_fecha_vigor,
    campos_de_paginas, explicacion_de_estado, rama_de, ruta_en_rama, seccion_real_de,
    validar_prefijo_ruta,
};
use crate::application::dtos::tools::{
    LakehouseToolName, ModoBusqueda, SearchNormativeContextParams, parsear_argumentos,
};
use crate::application::ports::semantic_index::FragmentoIndexado;
use crate::domain::errors::DomainError;
use crate::domain::models::estructura_documental::EstadoContenido;
use crate::domain::services::arbol_secciones::TITULO_PREAMBULO;
use crate::domain::services::marcas_pagina::texto_con_marcas;
use crate::domain::services::normalizacion::plegar_mayusculas;

/// Longitud máxima de la consulta
const MAX_LONGITUD_CONSULTA: usize = 2_000;
/// Resultados máximos
const MAX_TOP_K: usize = 20;
/// Constante de Reciprocal Rank Fusion
const RRF_K: f32 = 60.0;
const BM25_K1: f32 = 1.2;
const BM25_B: f32 = 0.75;
/// Documentos no definitivos que se listan como máximo en una respuesta
const MAX_DOCUMENTOS_NO_DEFINITIVOS: usize = 20;
/// Título de la sección única de un documento sin estructura
const TITULO_CONTENIDO_GENERAL: &str = "Contenido General";

/// Datos de la revisión de un fragmento que la respuesta necesita
#[derive(Debug, Clone, Default)]
pub(crate) struct DatosRevision {
    pub revision: String,
    /// Título del documento en 01
    pub titulo: String,
    /// `AAAA-MM-DD` si 01 la declara; nunca se inventa
    pub fecha_vigor: Option<String>,
    pub provisional: bool,
    pub vigencia_no_declarada: bool,
}

impl DatosRevision {
    /// Ni un provisional ni una revisión sin vigencia afirmable deciden un conflicto
    fn puede_prevalecer(&self) -> bool {
        !self.provisional && !self.vigencia_no_declarada
    }
}

/// Fragmento puntuado
#[derive(Debug, Clone)]
pub(crate) struct Puntuado<'a> {
    pub fragmento: &'a FragmentoIndexado,
    pub score: f32,
}

pub(crate) fn tokenizar(texto: &str) -> Vec<String> {
    plegar_mayusculas(texto)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() > 1)
        .map(str::to_string)
        .collect()
}

fn coseno(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let producto: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norma =
        a.iter().map(|x| x * x).sum::<f32>().sqrt() * b.iter().map(|x| x * x).sum::<f32>().sqrt();
    (norma > f32::EPSILON)
        .then(|| producto / norma)
        .filter(|s| s.is_finite())
}

fn texto_buscable(f: &FragmentoIndexado) -> String {
    format!(
        "{} {} {}",
        f.breadcrumb_jerarquia,
        f.condicion_aplicacion.as_deref().unwrap_or_default(),
        f.texto
    )
}

/// BM25 de cada fragmento frente a la consulta
fn puntuar_bm25(consulta: &[String], fragmentos: &[&FragmentoIndexado]) -> Vec<f32> {
    let documentos: Vec<Vec<String>> = fragmentos
        .iter()
        .map(|f| tokenizar(&texto_buscable(f)))
        .collect();
    let n = documentos.len().max(1) as f32;
    let media = documentos.iter().map(Vec::len).sum::<usize>() as f32 / n;
    let terminos: HashSet<&String> = consulta.iter().collect();
    let df: HashMap<&String, usize> = terminos
        .iter()
        .map(|t| (*t, documentos.iter().filter(|d| d.contains(t)).count()))
        .collect();
    documentos
        .iter()
        .map(|doc| {
            terminos
                .iter()
                .map(|t| {
                    let tf = doc.iter().filter(|p| p == t).count() as f32;
                    if tf == 0.0 {
                        return 0.0;
                    }
                    let d = df[t] as f32;
                    let idf = ((n - d + 0.5) / (d + 0.5) + 1.0).ln();
                    let normal =
                        BM25_K1 * (1.0 - BM25_B + BM25_B * doc.len() as f32 / media.max(1.0));
                    idf * tf * (BM25_K1 + 1.0) / (tf + normal)
                })
                .sum()
        })
        .collect()
}

/// Posición (1-based) de cada fragmento ordenado por score descendente; sin score → None
fn rangos(scores: &[Option<f32>]) -> Vec<Option<usize>> {
    let mut indices: Vec<usize> = (0..scores.len()).filter(|i| scores[*i].is_some()).collect();
    indices.sort_by(|a, b| {
        scores[*b]
            .partial_cmp(&scores[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut rango = vec![None; scores.len()];
    for (posicion, i) in indices.into_iter().enumerate() {
        rango[i] = Some(posicion + 1);
    }
    rango
}

/// Cómo se reparte el top-k entre documentos
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CuotaDocumento {
    /// Orden RRF puro
    Ninguna,
    /// Con dos o más documentos en el pool, ninguno ocupa más de `top_k − reservados` puestos
    /// (al menos 1). Los huecos que los demás no llenan vuelven al ranking. Con uno, como `Ninguna`
    Diversificar { reservados: usize },
}

/// Puestos del top-k que se dejan a otros documentos cuando la búsqueda no filtra por uno
const PUESTOS_RESERVADOS_A_OTROS_DOCUMENTOS: usize = 2;

/// Con un documento pedido no hay nada que repartir; sin él, se diversifica
pub(crate) fn cuota_de_busqueda(doc_pedido: Option<&str>) -> CuotaDocumento {
    match doc_pedido {
        Some(_) => CuotaDocumento::Ninguna,
        None => CuotaDocumento::Diversificar {
            reservados: PUESTOS_RESERVADOS_A_OTROS_DOCUMENTOS,
        },
    }
}

/// Fusión RRF de la similitud vectorial y BM25 (BM25 = 0 no aporta rango). Sin `usar_bm25` o sin
/// vector, ese motor no aporta nada: RRF queda como el orden del otro. La cuota se aplica sobre el
/// ranking RRF completo, antes de truncar a `top_k`.
pub(crate) fn fusionar<'a>(
    fragmentos: &[&'a FragmentoIndexado],
    consulta: &str,
    vector_consulta: Option<&[f32]>,
    usar_bm25: bool,
    top_k: usize,
    cuota: CuotaDocumento,
) -> Vec<Puntuado<'a>> {
    let ranking = ranking_rrf(fragmentos, consulta, vector_consulta, usar_bm25);
    match cuota {
        CuotaDocumento::Ninguna => ranking.into_iter().take(top_k).collect(),
        CuotaDocumento::Diversificar { reservados } => {
            seleccionar_con_cuota(ranking, top_k, reservados)
        }
    }
}

/// Primera pasada por ranking con el tope por documento; después, relleno con los descartados.
/// Como ambas listas conservan el orden del ranking, la salida sigue ordenada por score
fn seleccionar_con_cuota(ranking: Vec<Puntuado>, top_k: usize, reservados: usize) -> Vec<Puntuado> {
    let documentos_en_pool = ranking
        .iter()
        .map(|p| p.fragmento.doc_id.as_str())
        .collect::<HashSet<_>>()
        .len();
    if documentos_en_pool < 2 {
        return ranking.into_iter().take(top_k).collect();
    }
    let maximo_por_documento = top_k.saturating_sub(reservados).max(1);
    let mut ocupados: HashMap<&str, usize> = HashMap::new();
    let mut aceptados: Vec<usize> = Vec::with_capacity(top_k);
    let mut descartados: Vec<usize> = Vec::new();
    for (posicion, puntuado) in ranking.iter().enumerate() {
        if aceptados.len() == top_k {
            break;
        }
        let ocupados_del_documento = ocupados.entry(&puntuado.fragmento.doc_id).or_default();
        if *ocupados_del_documento < maximo_por_documento {
            *ocupados_del_documento += 1;
            aceptados.push(posicion);
        } else {
            descartados.push(posicion);
        }
    }
    let huecos = top_k - aceptados.len();
    aceptados.extend(descartados.into_iter().take(huecos));
    aceptados.sort_unstable();
    let mut elegidos: Vec<Option<Puntuado>> = ranking.into_iter().map(Some).collect();
    aceptados
        .into_iter()
        .filter_map(|posicion| elegidos[posicion].take())
        .collect()
}

/// Todos los fragmentos con score RRF positivo, por score descendente y, a igualdad, por id
fn ranking_rrf<'a>(
    fragmentos: &[&'a FragmentoIndexado],
    consulta: &str,
    vector_consulta: Option<&[f32]>,
    usar_bm25: bool,
) -> Vec<Puntuado<'a>> {
    let tokens = tokenizar(consulta);
    let bm25: Vec<Option<f32>> = if usar_bm25 {
        puntuar_bm25(&tokens, fragmentos)
            .into_iter()
            .map(|s| (s > 0.0).then_some(s))
            .collect()
    } else {
        vec![None; fragmentos.len()]
    };
    let vectoriales: Vec<Option<f32>> = fragmentos
        .iter()
        .map(|f| vector_consulta.and_then(|v| coseno(v, &f.vector)))
        .collect();
    let rango_bm25 = rangos(&bm25);
    let rango_vector = rangos(&vectoriales);
    let mut puntuados: Vec<Puntuado> = fragmentos
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let aporte = |r: Option<usize>| r.map_or(0.0, |r| 1.0 / (RRF_K + r as f32));
            let score = aporte(rango_bm25[i]) + aporte(rango_vector[i]);
            (score > 0.0).then_some(Puntuado {
                fragmento: f,
                score,
            })
        })
        .collect();
    puntuados.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.fragmento.id.cmp(&b.fragmento.id))
    });
    puntuados
}

/// Texto de un fragmento para la respuesta: con marcas `⟦p. N⟧` si tiene saltos de página. Las
/// marcas no existen en el índice, así que no afectan al ranking
fn texto_de_salida(fragmento: &FragmentoIndexado) -> String {
    texto_con_marcas(
        &fragmento.texto,
        fragmento.saltos_pagina.as_deref().unwrap_or_default(),
    )
}

/// Tema de un fragmento: título de su sección más profunda, sin numeración
pub(crate) fn tema(fragmento: &FragmentoIndexado) -> String {
    let ultimo = fragmento
        .breadcrumb_jerarquia
        .rsplit('>')
        .next()
        .unwrap_or_default();
    let sin_numero = ultimo.trim().trim_start_matches(|c: char| {
        c.is_ascii_digit() || c == '.' || c.is_whitespace() || c == '-'
    });
    let titulo = plegar_mayusculas(sin_numero).trim().to_string();
    if titulo.is_empty() {
        format!("{}#{}", fragmento.doc_id, fragmento.id)
    } else if es_titulo_generico(&titulo) {
        // Dos documentos sin relación comparten estos títulos: no son un conflicto de prelación
        format!("{titulo} · {}", fragmento.doc_id)
    } else {
        titulo
    }
}

/// Títulos que la ingesta asigna sin que los escriba el documento
fn es_titulo_generico(titulo: &str) -> bool {
    [TITULO_CONTENIDO_GENERAL, TITULO_PREAMBULO]
        .iter()
        .any(|generico| plegar_mayusculas(generico) == titulo)
}

/// Agrupa por tema; en cada tema con varios niveles de prelación marca el menor como prevalente
pub(crate) fn agrupar_por_tema(
    puntuados: &[Puntuado],
    revision_de: &HashMap<(String, i64), DatosRevision>,
) -> Vec<Value> {
    let mut orden: Vec<String> = Vec::new();
    let mut grupos: HashMap<String, Vec<&Puntuado>> = HashMap::new();
    for p in puntuados {
        let clave = tema(p.fragmento);
        if !grupos.contains_key(&clave) {
            orden.push(clave.clone());
        }
        grupos.entry(clave).or_default().push(p);
    }
    orden
        .into_iter()
        .map(|clave| {
            let miembros = &grupos[&clave];
            let datos = |f: &FragmentoIndexado| {
                revision_de
                    .get(&(f.doc_id.clone(), i64::from(f.version_num)))
                    .cloned()
                    .unwrap_or_default()
            };
            let minimo = miembros
                .iter()
                .filter(|p| datos(p.fragmento).puede_prevalecer())
                .map(|p| p.fragmento.nivel_prelacion)
                .min();
            let conflicto = minimo.is_some()
                && miembros.iter().any(|p| {
                    !datos(p.fragmento).puede_prevalecer()
                        || minimo != Some(p.fragmento.nivel_prelacion)
                });
            let resultados: Vec<Value> = miembros
                .iter()
                .map(|p| {
                    let f = p.fragmento;
                    let revision = datos(f);
                    let prevalece =
                        revision.puede_prevalecer() && minimo == Some(f.nivel_prelacion);
                    // Un hito solo existe si el documento tiene Capa 2
                    let estado = f
                        .estado_contenido
                        .unwrap_or(EstadoContenido::Completo)
                        .as_str();
                    let mut cita = json!({
                        "doc_id": f.doc_id,
                        "titulo": revision.titulo,
                        "revision": (!revision.revision.is_empty()).then_some(&revision.revision),
                        "version_num": f.version_num,
                        "chunk_origen_id": f.chunk_origen_id,
                        "breadcrumb": f.breadcrumb_jerarquia,
                        "nodo_id": f.nodo_id.as_deref().map(seccion_real_de),
                    });
                    if let (Some(destino), Value::Object(paginas)) = (
                        cita.as_object_mut(),
                        campos_de_paginas(
                            f.pagina_inicio.map(i64::from),
                            f.pagina_fin.map(i64::from),
                        ),
                    ) {
                        destino.extend(paginas);
                    }
                    anadir_fecha_vigor(&mut cita, revision.fecha_vigor.as_deref());
                    json!({
                        "id": f.id,
                        "origen": f.origen.as_str(),
                        "texto": texto_de_salida(f),
                        "breadcrumb_jerarquia": f.breadcrumb_jerarquia,
                        "condicion_aplicacion": f.condicion_aplicacion,
                        "nivel_prelacion": f.nivel_prelacion,
                        "prevalece": prevalece,
                        "supletorio": conflicto && !prevalece,
                        "score": p.score,
                        "tipo_bloque": f.tipo_bloque.map(|t| t.as_str()),
                        "estado_contenido": estado,
                        "tiene_capa_2": estado == EstadoContenido::Completo.as_str(),
                        "provisional": revision.provisional,
                        "vigencia_no_declarada": revision.vigencia_no_declarada,
                        "aviso_contenido": explicacion_de_estado(estado, revision.provisional),
                        "cita": cita,
                    })
                })
                .collect();
            json!({ "tema": clave, "conflicto_prelacion": conflicto, "resultados": resultados })
        })
        .collect()
}

/// Documentos elegibles que no son definitivos: visibles aunque ningún fragmento coincida
fn documentos_no_definitivos(
    revisiones: &[RevisionDocumental],
    fragmentos: &[FragmentoIndexado],
) -> Vec<Value> {
    let mut estados: HashMap<(&str, i32), &'static str> = HashMap::new();
    for f in fragmentos {
        if let Some(estado) = f.estado_contenido {
            estados.insert((&f.doc_id, f.version_num), estado.as_str());
        }
    }
    revisiones
        .iter()
        .filter_map(|r| {
            let version = i32::try_from(r.version_num).ok()?;
            let estado = estados.get(&(r.doc_id.as_str(), version)).copied()?;
            let explicacion = explicacion_de_estado(estado, r.es_provisional())?;
            Some(json!({
                "doc_id": r.doc_id,
                "titulo": r.titulo,
                "revision": r.revision,
                "estado_contenido": estado,
                "explicacion": explicacion,
            }))
        })
        .take(MAX_DOCUMENTOS_NO_DEFINITIVOS)
        .collect()
}

/// Secciones a las que se restringe la búsqueda
fn seccion_permitida(
    nodos: &[NodoIndice],
    revision: &RevisionDocumental,
    nodo_id: Option<&str>,
    prefijo: Option<&str>,
) -> Result<HashSet<String>, DomainError> {
    let mut permitidas = match nodo_id {
        Some(pedido) => {
            let raiz = LakehouseToolService::nodo_existente(nodos, revision, pedido)?;
            rama_de(nodos, &raiz.nodo_id)
        }
        None => nodos.iter().map(|n| n.nodo_id.clone()).collect(),
    };
    if let Some(prefijo) = prefijo {
        permitidas.retain(|id| {
            nodos
                .iter()
                .any(|n| &n.nodo_id == id && ruta_en_rama(&n.ruta, prefijo))
        });
        if permitidas.is_empty() {
            return Err(DomainError::ValidationError(format!(
                "La rama '{prefijo}' no existe en {} {}. Consulta las secciones reales con stair_navigate_toc.",
                revision.doc_id, revision.revision
            )));
        }
    }
    Ok(permitidas)
}

impl LakehouseToolService {
    pub(super) async fn buscar_contexto(&self, argumentos: &Value) -> Result<Value, DomainError> {
        let params: SearchNormativeContextParams =
            parsear_argumentos(LakehouseToolName::SearchNormativeContext, argumentos)?;
        let contexto = self
            .contexto(&params.centro_id, params.fecha_referencia.as_deref())
            .await?;
        let consulta = params.consulta.trim();
        if consulta.is_empty() || consulta.chars().count() > MAX_LONGITUD_CONSULTA {
            return Err(DomainError::ValidationError(format!(
                "La consulta debe tener entre 1 y {MAX_LONGITUD_CONSULTA} caracteres"
            )));
        }
        let top_k = params
            .top_k
            .map_or(self.config.top_k, usize::from)
            .clamp(1, MAX_TOP_K);
        if let Some(prefijo) = params.ruta_linaje_prefijo.as_deref() {
            validar_prefijo_ruta(prefijo)?;
            if params.doc_id.is_none() && params.nodo_id.is_none() {
                return Err(DomainError::ValidationError(
                    "ruta_linaje_prefijo requiere doc_id: cada documento tiene sus propias rutas"
                        .to_string(),
                ));
            }
        }

        // Revisiones elegibles (vigentes o en la fecha) y sus datos para la cita
        let revisiones = self.revisiones_elegibles(&contexto).await?;
        let revision_de: HashMap<(String, i64), DatosRevision> = revisiones
            .iter()
            .map(|r| {
                (
                    (r.doc_id.clone(), r.version_num),
                    DatosRevision {
                        revision: r.revision.clone(),
                        titulo: r.titulo.clone(),
                        fecha_vigor: r.fecha_vigor.clone(),
                        provisional: r.es_provisional(),
                        vigencia_no_declarada: r.vigencia_no_declarada,
                    },
                )
            })
            .collect();

        // El documento del filtro: el pedido o, si solo hay sección, el de su `nodo_id`
        let doc_pedido = params.doc_id.as_deref().or_else(|| {
            params
                .nodo_id
                .as_deref()
                .map(|n| n.split_once('#').map_or(n, |(doc, _)| doc))
        });
        let con_seccion = params.nodo_id.is_some() || params.ruta_linaje_prefijo.is_some();
        let doc_resuelto = doc_pedido
            .map(|doc| Self::resolver_documento(&revisiones, doc))
            .transpose()?
            .flatten();
        if let (Some(doc), None, true) = (doc_pedido, doc_resuelto, con_seccion) {
            return Err(DomainError::ValidationError(format!(
                "La sección pedida no existe: no hay ninguna revisión de '{}' en el centro '{}'. Consulta los documentos con inspect_lakehouse_catalog.",
                doc.escape_default(),
                contexto.centro
            )));
        }
        let aviso_documento = (doc_pedido.is_some() && doc_resuelto.is_none()).then(|| {
            format!(
                "El documento '{}' no existe en este centro para esta fecha: consulta inspect_lakehouse_catalog.",
                doc_pedido.unwrap_or_default().escape_default()
            )
        });
        let secciones = match doc_resuelto.filter(|_| con_seccion) {
            Some(revision) => {
                let nodos = self.nodos_de_revision(&contexto, revision).await?;
                Some(seccion_permitida(
                    &nodos,
                    revision,
                    params.nodo_id.as_deref(),
                    params.ruta_linaje_prefijo.as_deref(),
                )?)
            }
            None => None,
        };

        let indexados = self.indice.fragmentos(&contexto.centro).await;
        let elegibles: Vec<&FragmentoIndexado> = indexados
            .iter()
            .filter(|f| revision_de.contains_key(&(f.doc_id.clone(), i64::from(f.version_num))))
            .filter(|f| {
                params
                    .nivel_prelacion_max
                    .is_none_or(|n| f.nivel_prelacion <= n)
            })
            .filter(|f| {
                doc_pedido.is_none()
                    || doc_resuelto.is_some_and(|r| {
                        r.doc_id == f.doc_id && r.version_num == i64::from(f.version_num)
                    })
            })
            .filter(|f| {
                secciones.as_ref().is_none_or(|permitidas| {
                    f.nodo_id.as_ref().is_some_and(|n| permitidas.contains(n))
                })
            })
            .collect();

        let (vector, aviso_vector) = match params.modo {
            ModoBusqueda::SoloBm25 => (None, None),
            ModoBusqueda::SoloSemantico => {
                // Sin fallback: quien pide solo semántica debe saber que no la ha obtenido
                (
                    Some(self.embeddings.generate_embedding(consulta).await?),
                    None,
                )
            }
            ModoBusqueda::Hibrido => match self.embeddings.generate_embedding(consulta).await {
                Ok(v) => (Some(v), None),
                Err(e) => (
                    None,
                    Some(format!("Sin búsqueda vectorial ({e}): solo léxica")),
                ),
            },
        };
        let usar_bm25 = params.modo != ModoBusqueda::SoloSemantico;
        let puntuados = fusionar(
            &elegibles,
            consulta,
            vector.as_deref(),
            usar_bm25,
            top_k,
            cuota_de_busqueda(doc_pedido),
        );
        let aviso = aviso_documento.or(aviso_vector);
        Ok(json!({
            "centro_id": contexto.centro.as_str(),
            "fecha_referencia": contexto.fecha_json(),
            "consulta": consulta,
            "modo": params.modo.as_str(),
            "total": puntuados.len(),
            "grupos": agrupar_por_tema(&puntuados, &revision_de),
            "aviso": aviso,
            "documentos_no_definitivos": documentos_no_definitivos(&revisiones, &indexados),
            "nota": "Incluye siempre la condicion_aplicacion de cada resultado. Si un tema tiene conflicto_prelacion, responde con el resultado prevalece: true y menciona el supletorio. Cita el título o el doc_id, la revisión, la sección y la página si existe. Si un resultado trae aviso_contenido, dilo en la respuesta."
                .to_string()
                + " "
                + NOTA_MARCAS_DE_PAGINA,
            "nota_seguridad": NOTA_SEGURIDAD,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::semantic_index::OrigenFragmento;

    fn fragmento(
        id: &str,
        doc: &str,
        nivel: i8,
        breadcrumb: &str,
        texto: &str,
        vector: Vec<f32>,
    ) -> FragmentoIndexado {
        FragmentoIndexado {
            id: id.into(),
            origen: OrigenFragmento::Chunk,
            doc_id: doc.into(),
            version_num: 1,
            nivel_prelacion: nivel,
            breadcrumb_jerarquia: breadcrumb.into(),
            condicion_aplicacion: None,
            texto: texto.into(),
            chunk_origen_id: None,
            nodo_id: None,
            tipo_bloque: None,
            estado_contenido: None,
            pagina_inicio: None,
            pagina_fin: None,
            saltos_pagina: None,
            vector,
        }
    }

    #[test]
    fn test_prevalece_el_menor_nivel_en_el_mismo_tema() {
        let pliego = fragmento(
            "p1",
            "PCC-01",
            1,
            "PCC-01 > 5. Ensayos no destructivos",
            "Ensayar el 100 % de las soldaduras",
            vec![1.0, 0.0],
        );
        let norma = fragmento(
            "n1",
            "UNE-EN-1090-2",
            5,
            "UNE > 12.4 Ensayos no destructivos",
            "Ensayar el 10 % de las soldaduras",
            vec![0.9, 0.1],
        );
        let otro = fragmento(
            "o1",
            "PO-ALM-03",
            2,
            "PO > 3. Recepción",
            "Descarga de tubería",
            vec![0.0, 1.0],
        );
        let todos = [&pliego, &norma, &otro];
        let puntuados = fusionar(
            &todos,
            "porcentaje de soldaduras a ensayar",
            Some(&[1.0, 0.0]),
            true,
            10,
            CuotaDocumento::Ninguna,
        );
        let grupos = agrupar_por_tema(&puntuados, &HashMap::new());
        let ensayos = grupos
            .iter()
            .find(|g| g["tema"] == "ENSAYOS NO DESTRUCTIVOS")
            .unwrap();
        assert_eq!(ensayos["conflicto_prelacion"], true);
        let resultados = ensayos["resultados"].as_array().unwrap();
        let del = |doc: &str| {
            resultados
                .iter()
                .find(|r| r["cita"]["doc_id"] == doc)
                .unwrap()
                .clone()
        };
        assert_eq!(del("PCC-01")["prevalece"], true);
        assert_eq!(del("UNE-EN-1090-2")["prevalece"], false);
        assert_eq!(del("UNE-EN-1090-2")["supletorio"], true);
    }

    #[test]
    fn test_bm25_sin_vectores_y_vectores_de_otra_dimension() {
        let a = fragmento(
            "a",
            "D",
            2,
            "1. Custodia",
            "acopio superior a siete dias",
            vec![1.0, 0.0, 0.0],
        );
        let b = fragmento("b", "D", 2, "2. Recepcion", "descarga del camion", vec![]);
        let todos = [&a, &b];
        let puntuados = fusionar(
            &todos,
            "acopio",
            Some(&[1.0, 0.0]),
            true,
            5,
            CuotaDocumento::Ninguna,
        );
        assert_eq!(puntuados.len(), 1);
        assert_eq!(puntuados[0].fragmento.id, "a");
        assert!(
            fusionar(
                &todos,
                "inexistente",
                None,
                true,
                5,
                CuotaDocumento::Ninguna
            )
            .is_empty()
        );
    }

    #[test]
    fn test_modos_excluyen_al_otro_motor() {
        let a = fragmento("a", "D", 2, "1. Uno", "palabra rara", vec![0.0, 1.0]);
        let b = fragmento("b", "D", 2, "2. Dos", "otro texto", vec![1.0, 0.0]);
        let todos = [&a, &b];
        let solo_vector = fusionar(
            &todos,
            "rara",
            Some(&[1.0, 0.0]),
            false,
            5,
            CuotaDocumento::Ninguna,
        );
        assert_eq!(
            solo_vector[0].fragmento.id, "b",
            "ignora la coincidencia léxica"
        );
        let solo_bm25 = fusionar(&todos, "rara", None, true, 5, CuotaDocumento::Ninguna);
        assert_eq!(solo_bm25.len(), 1);
        assert_eq!(solo_bm25[0].fragmento.id, "a");
    }

    #[test]
    fn test_titulos_generados_no_crean_conflicto_entre_documentos() {
        let uno = fragmento("u", "A", 1, "Contenido General", "cemento", vec![]);
        let dos = fragmento("d", "B", 5, "Contenido General", "cemento", vec![]);
        assert_ne!(tema(&uno), tema(&dos));
        let pre = fragmento("p", "A", 1, "Preámbulo / Introducción", "", vec![]);
        assert!(tema(&pre).ends_with("· A"));
    }

    #[test]
    fn test_provisional_nunca_prevalece_y_marca_conflicto() {
        let vigente = fragmento("v", "NORMA", 5, "4. Ensayos", "ensayo", vec![]);
        let mut provisional = fragmento("p", "PROV", 5, "4. Ensayos", "ensayo", vec![]);
        provisional.estado_contenido = Some(EstadoContenido::ParcialUniversal);
        let todos = [&vigente, &provisional];
        let puntuados = fusionar(&todos, "ensayo", None, true, 5, CuotaDocumento::Ninguna);
        let mut revisiones = HashMap::new();
        revisiones.insert(
            ("PROV".to_string(), 1),
            DatosRevision {
                revision: "Provisional".into(),
                provisional: true,
                ..DatosRevision::default()
            },
        );
        let grupos = agrupar_por_tema(&puntuados, &revisiones);
        assert_eq!(grupos[0]["conflicto_prelacion"], true);
        let resultados = grupos[0]["resultados"].as_array().unwrap();
        let de = |doc: &str| {
            resultados
                .iter()
                .find(|r| r["cita"]["doc_id"] == doc)
                .unwrap()
        };
        assert_eq!(de("NORMA")["prevalece"], true);
        assert_eq!(de("PROV")["prevalece"], false);
        assert_eq!(de("PROV")["supletorio"], true);
        assert_eq!(de("PROV")["tiene_capa_2"], false);
    }

    fn resultado_unico(f: &FragmentoIndexado, datos: DatosRevision) -> Value {
        let puntuados = fusionar(&[f], "presion", None, true, 5, CuotaDocumento::Ninguna);
        let mut revisiones = HashMap::new();
        revisiones.insert((f.doc_id.clone(), 1), datos);
        agrupar_por_tema(&puntuados, &revisiones)[0]["resultados"][0].clone()
    }

    #[test]
    fn test_texto_lleva_marcas_de_pagina_solo_en_la_salida() {
        use crate::domain::services::marcas_pagina::SaltoPagina;
        let mut f = fragmento(
            "c1",
            "DOC",
            2,
            "4. Ensayos",
            "presion de prueba\nvalor de la presion",
            vec![],
        );
        f.pagina_inicio = Some(52);
        f.pagina_fin = Some(53);
        f.saltos_pagina = Some(vec![
            SaltoPagina {
                desplazamiento: 0,
                pagina: 52,
            },
            SaltoPagina {
                desplazamiento: 18,
                pagina: 53,
            },
        ]);
        let resultado = resultado_unico(&f, DatosRevision::default());
        assert_eq!(
            resultado["texto"],
            "⟦p. 52⟧ presion de prueba\n⟦p. 53⟧ valor de la presion"
        );
        assert_eq!(resultado["cita"]["paginas"], "52-53");
        assert_eq!(f.texto, "presion de prueba\nvalor de la presion");
        assert!(!texto_buscable(&f).contains('⟦'), "BM25 no ve las marcas");
    }

    #[test]
    fn test_sin_saltos_no_hay_marcas_y_las_falsas_se_neutralizan() {
        let f = fragmento(
            "c1",
            "DOC",
            2,
            "4. Ensayos",
            "la presion ⟦p. 99⟧ de prueba",
            vec![],
        );
        let resultado = resultado_unico(&f, DatosRevision::default());
        assert_eq!(resultado["texto"], "la presion [p. 99] de prueba");
    }

    #[test]
    fn test_cita_lleva_titulo_y_fecha_de_vigor_solo_si_existe() {
        let f = fragmento("c1", "DOC", 2, "4. Ensayos", "presion", vec![]);
        let con_fecha = resultado_unico(
            &f,
            DatosRevision {
                revision: "Rev. 02".into(),
                titulo: "Procedimiento de ensayos".into(),
                fecha_vigor: Some("2026-03-01".into()),
                ..DatosRevision::default()
            },
        );
        assert_eq!(con_fecha["cita"]["titulo"], "Procedimiento de ensayos");
        assert_eq!(con_fecha["cita"]["fecha_vigor"], "2026-03-01");
        let sin_fecha = resultado_unico(
            &f,
            DatosRevision {
                titulo: "Procedimiento de ensayos".into(),
                ..DatosRevision::default()
            },
        );
        assert_eq!(sin_fecha["cita"]["titulo"], "Procedimiento de ensayos");
        assert!(
            sin_fecha["cita"]
                .as_object()
                .unwrap()
                .get("fecha_vigor")
                .is_none(),
            "una fecha nula no aparece: {}",
            sin_fecha["cita"]
        );
    }

    #[test]
    fn test_nota_explica_las_marcas_y_la_pagina_fisica() {
        for frase in [
            "⟦p. N⟧",
            "página física",
            "cruza una marca",
            "extracto literal",
        ] {
            assert!(NOTA_MARCAS_DE_PAGINA.contains(frase), "{frase}");
        }
    }

    #[test]
    fn test_tema_quita_numeracion_y_tildes() {
        let f = fragmento(
            "x",
            "D",
            2,
            "PO-ALM-03 > 4. Inspección > 4.2 Ensayos",
            "",
            vec![],
        );
        assert_eq!(tema(&f), "ENSAYOS");
        let sin = fragmento("y", "D", 2, "", "", vec![]);
        assert_eq!(tema(&sin), "D#y");
    }

    // --- Cuota por documento (SSD de generalidad, Pieza 3a) ---

    const CUOTA_POR_DEFECTO: CuotaDocumento = CuotaDocumento::Diversificar { reservados: 2 };

    /// Fragmentos de un documento cuya similitud con `[1, 0]` decrece con el índice
    fn serie(doc: &str, textos: &[&str], similitud_maxima: f32) -> Vec<FragmentoIndexado> {
        textos
            .iter()
            .enumerate()
            .map(|(i, texto)| {
                let x = similitud_maxima - 0.05 * i as f32;
                fragmento(
                    &format!("{doc}-{i:02}"),
                    doc,
                    2,
                    "1. Apartado",
                    texto,
                    vec![x, (1.0 - x * x).max(0.0).sqrt()],
                )
            })
            .collect()
    }

    fn ids(puntuados: &[Puntuado]) -> Vec<String> {
        puntuados.iter().map(|p| p.fragmento.id.clone()).collect()
    }

    const TEXTOS_ES: [&str; 10] = [
        "presión de prueba hidrostática",
        "tiempo de mantenimiento de la presión",
        "temperatura del agua de prueba",
        "espesor de la junta de brida",
        "pérdida de presión admisible",
        "registro de la prueba",
        "responsable de la inspección",
        "equipos de medida calibrados",
        "condiciones de seguridad",
        "criterio de aceptación",
    ];
    const TEXTOS_EN: [&str; 3] = [
        "hydrostatic test pressure",
        "holding time of the test",
        "acceptance criteria for leaks",
    ];
    const TEXTOS_FR: [&str; 3] = [
        "pression d'essai hydrostatique",
        "durée de maintien de l'essai",
        "critères d'acceptation",
    ];

    #[test]
    fn test_cuota_documento_dominante_conserva_sus_seis_mejores_y_deja_dos_al_otro() {
        let es = serie("ES-DOC", &TEXTOS_ES, 0.99);
        let en = serie("EN-DOC", &TEXTOS_EN, 0.50);
        let todos: Vec<&FragmentoIndexado> = es.iter().chain(&en).collect();
        let sin_cuota = fusionar(
            &todos,
            "pressure",
            Some(&[1.0, 0.0]),
            false,
            8,
            CuotaDocumento::Ninguna,
        );
        assert!(sin_cuota.iter().all(|p| p.fragmento.doc_id == "ES-DOC"));
        let con_cuota = fusionar(
            &todos,
            "pressure",
            Some(&[1.0, 0.0]),
            false,
            8,
            CUOTA_POR_DEFECTO,
        );
        assert_eq!(
            ids(&con_cuota),
            [
                "ES-DOC-00",
                "ES-DOC-01",
                "ES-DOC-02",
                "ES-DOC-03",
                "ES-DOC-04",
                "ES-DOC-05",
                "EN-DOC-00",
                "EN-DOC-01"
            ],
            "los 6 mejores del dominante, después los 2 mejores del otro, por score"
        );
        assert!(
            con_cuota.windows(2).all(|par| par[0].score >= par[1].score),
            "salida ordenada por score"
        );
    }

    #[test]
    fn test_cuota_con_tres_documentos_reparte_los_huecos_por_ranking() {
        let es = serie("ES-DOC", &TEXTOS_ES, 0.99);
        let en = serie("EN-DOC", &TEXTOS_EN, 0.70);
        let fr = serie("FR-DOC", &TEXTOS_FR, 0.80);
        let todos: Vec<&FragmentoIndexado> = es.iter().chain(&en).chain(&fr).collect();
        let con_cuota = fusionar(&todos, "x", Some(&[1.0, 0.0]), false, 8, CUOTA_POR_DEFECTO);
        let de = |doc: &str| {
            con_cuota
                .iter()
                .filter(|p| p.fragmento.doc_id == doc)
                .count()
        };
        assert_eq!((de("ES-DOC"), de("FR-DOC"), de("EN-DOC")), (6, 2, 0));
    }

    #[test]
    fn test_cuota_sin_candidatos_del_otro_documento_rellena_con_el_dominante() {
        let es = serie("ES-DOC", &TEXTOS_ES, 0.99);
        // Sin vector y sin coincidencia léxica: el otro documento no entra en el pool
        let en: Vec<FragmentoIndexado> = serie("EN-DOC", &TEXTOS_EN, 0.80)
            .into_iter()
            .map(|mut f| {
                f.vector = vec![];
                f
            })
            .collect();
        let todos: Vec<&FragmentoIndexado> = es.iter().chain(&en).collect();
        let con_cuota = fusionar(&todos, "zzz", Some(&[1.0, 0.0]), true, 8, CUOTA_POR_DEFECTO);
        assert_eq!(con_cuota.len(), 8);
        assert!(con_cuota.iter().all(|p| p.fragmento.doc_id == "ES-DOC"));
    }

    #[test]
    fn test_cuota_con_un_solo_candidato_ajeno_rellena_el_hueco_restante() {
        let es = serie("ES-DOC", &TEXTOS_ES, 0.99);
        let fr = serie("FR-DOC", &TEXTOS_FR[..1], 0.30);
        let todos: Vec<&FragmentoIndexado> = es.iter().chain(&fr).collect();
        let con_cuota = fusionar(&todos, "x", Some(&[1.0, 0.0]), false, 8, CUOTA_POR_DEFECTO);
        let de = |doc: &str| {
            con_cuota
                .iter()
                .filter(|p| p.fragmento.doc_id == doc)
                .count()
        };
        assert_eq!((de("ES-DOC"), de("FR-DOC")), (7, 1));
    }

    #[test]
    fn test_cuota_con_top_k_uno_devuelve_el_mejor_fragmento() {
        let es = serie("ES-DOC", &TEXTOS_ES, 0.99);
        let en = serie("EN-DOC", &TEXTOS_EN, 0.80);
        let todos: Vec<&FragmentoIndexado> = es.iter().chain(&en).collect();
        let con_cuota = fusionar(&todos, "x", Some(&[1.0, 0.0]), false, 1, CUOTA_POR_DEFECTO);
        assert_eq!(ids(&con_cuota), ["ES-DOC-00"]);
    }

    #[test]
    fn test_cuota_con_top_k_menor_que_los_reservados_conserva_el_mejor() {
        let es = serie("ES-DOC", &TEXTOS_ES, 0.99);
        let en = serie("EN-DOC", &TEXTOS_EN, 0.80);
        let fr = serie("FR-DOC", &TEXTOS_FR, 0.70);
        let todos: Vec<&FragmentoIndexado> = es.iter().chain(&en).chain(&fr).collect();
        let con_cuota = fusionar(&todos, "x", Some(&[1.0, 0.0]), false, 2, CUOTA_POR_DEFECTO);
        assert_eq!(ids(&con_cuota), ["ES-DOC-00", "EN-DOC-00"]);
    }

    #[test]
    fn test_cuota_solo_sin_filtro_de_documento() {
        assert_eq!(cuota_de_busqueda(Some("DOC")), CuotaDocumento::Ninguna);
        assert_eq!(cuota_de_busqueda(None), CUOTA_POR_DEFECTO);
    }

    /// Generador congruencial: rankings reproducibles sin dependencias nuevas
    struct Semilla(u64);

    impl Semilla {
        fn siguiente(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }

        fn real(&mut self) -> f32 {
            (self.siguiente() % 10_000) as f32 / 10_000.0
        }
    }

    #[test]
    fn test_cuota_con_un_solo_documento_no_cambia_la_salida() {
        let palabras: Vec<&str> = TEXTOS_ES
            .iter()
            .chain(&TEXTOS_EN)
            .chain(&TEXTOS_FR)
            .flat_map(|t| t.split(' '))
            .collect();
        let mut semilla = Semilla(20_261_111);
        for ronda in 0..50 {
            let cantidad = 1 + (semilla.siguiente() % 30) as usize;
            let fragmentos: Vec<FragmentoIndexado> = (0..cantidad)
                .map(|i| {
                    let texto: Vec<&str> = (0..6)
                        .map(|_| palabras[semilla.siguiente() as usize % palabras.len()])
                        .collect();
                    let vector = vec![semilla.real(), semilla.real(), semilla.real()];
                    fragmento(
                        &format!("f{i:02}"),
                        "UNICO",
                        2,
                        "1. A",
                        &texto.join(" "),
                        vector,
                    )
                })
                .collect();
            let todos: Vec<&FragmentoIndexado> = fragmentos.iter().collect();
            let consulta = palabras[semilla.siguiente() as usize % palabras.len()];
            let vector = [semilla.real(), semilla.real(), semilla.real()];
            let top_k = 1 + (semilla.siguiente() % 20) as usize;
            let sin = fusionar(
                &todos,
                consulta,
                Some(&vector),
                true,
                top_k,
                CuotaDocumento::Ninguna,
            );
            let con = fusionar(
                &todos,
                consulta,
                Some(&vector),
                true,
                top_k,
                CUOTA_POR_DEFECTO,
            );
            assert_eq!(ids(&sin), ids(&con), "ronda {ronda}");
        }
    }
}
