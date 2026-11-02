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
    NOTA_SEGURIDAD, NodoIndice, RevisionDocumental, campos_de_paginas, explicacion_de_estado,
    rama_de, ruta_en_rama, seccion_real_de, validar_prefijo_ruta,
};
use crate::application::dtos::tools::{
    LakehouseToolName, ModoBusqueda, SearchNormativeContextParams, parsear_argumentos,
};
use crate::application::ports::semantic_index::FragmentoIndexado;
use crate::domain::errors::DomainError;
use crate::domain::models::estructura_documental::EstadoContenido;
use crate::domain::services::arbol_secciones::TITULO_PREAMBULO;
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

/// Fusión RRF de la similitud vectorial y BM25 (BM25 = 0 no aporta rango). Sin `usar_bm25` o sin
/// vector, ese motor no aporta nada: RRF queda como el orden del otro.
pub(crate) fn fusionar<'a>(
    fragmentos: &[&'a FragmentoIndexado],
    consulta: &str,
    vector_consulta: Option<&[f32]>,
    usar_bm25: bool,
    top_k: usize,
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
    puntuados.truncate(top_k);
    puntuados
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
                    json!({
                        "id": f.id,
                        "origen": f.origen.as_str(),
                        "texto": f.texto,
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
        let puntuados = fusionar(&elegibles, consulta, vector.as_deref(), usar_bm25, top_k);
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
            "nota": "Incluye siempre la condicion_aplicacion de cada resultado. Si un tema tiene conflicto_prelacion, responde con el resultado prevalece: true y menciona el supletorio. Cita doc_id, revisión, sección y página si existe. Si un resultado trae aviso_contenido, dilo en la respuesta.",
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
        let puntuados = fusionar(&todos, "acopio", Some(&[1.0, 0.0]), true, 5);
        assert_eq!(puntuados.len(), 1);
        assert_eq!(puntuados[0].fragmento.id, "a");
        assert!(fusionar(&todos, "inexistente", None, true, 5).is_empty());
    }

    #[test]
    fn test_modos_excluyen_al_otro_motor() {
        let a = fragmento("a", "D", 2, "1. Uno", "palabra rara", vec![0.0, 1.0]);
        let b = fragmento("b", "D", 2, "2. Dos", "otro texto", vec![1.0, 0.0]);
        let todos = [&a, &b];
        let solo_vector = fusionar(&todos, "rara", Some(&[1.0, 0.0]), false, 5);
        assert_eq!(
            solo_vector[0].fragmento.id, "b",
            "ignora la coincidencia léxica"
        );
        let solo_bm25 = fusionar(&todos, "rara", None, true, 5);
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
        let puntuados = fusionar(&todos, "ensayo", None, true, 5);
        let mut revisiones = HashMap::new();
        revisiones.insert(
            ("PROV".to_string(), 1),
            DatosRevision {
                revision: "Provisional".into(),
                provisional: true,
                vigencia_no_declarada: false,
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
}
