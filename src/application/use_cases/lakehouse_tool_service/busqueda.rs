//! `search_normative_context`: búsqueda híbrida (coseno + BM25 fusionados con RRF) sobre los
//! chunks (02) y los hitos (03) del centro, con vigencia, prelación y condición heredada
//! (problemas 5 y 7). Los resultados se agrupan por tema (título de la sección) y, si en un
//! tema hay documentos de distinta prelación, `prevalece` marca el de menor nivel.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::{LakehouseToolService, entero, texto};
use crate::application::dtos::tools::{
    LakehouseToolName, SearchNormativeContextParams, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::TABLA_DOCUMENTOS_CATALOGO;
use crate::application::ports::semantic_index::FragmentoIndexado;
use crate::domain::errors::DomainError;
use crate::domain::services::normalizacion::{codigo_canonico, plegar_mayusculas};

/// Longitud máxima de la consulta
const MAX_LONGITUD_CONSULTA: usize = 2_000;
/// Resultados máximos
const MAX_TOP_K: usize = 20;
/// Constante de Reciprocal Rank Fusion
const RRF_K: f32 = 60.0;
const BM25_K1: f32 = 1.2;
const BM25_B: f32 = 0.75;

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

/// Fusión RRF de la similitud vectorial y BM25 (BM25 = 0 no aporta rango)
pub(crate) fn fusionar<'a>(
    fragmentos: &[&'a FragmentoIndexado],
    consulta: &str,
    vector_consulta: Option<&[f32]>,
    top_k: usize,
) -> Vec<Puntuado<'a>> {
    let tokens = tokenizar(consulta);
    let bm25: Vec<Option<f32>> = puntuar_bm25(&tokens, fragmentos)
        .into_iter()
        .map(|s| (s > 0.0).then_some(s))
        .collect();
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
    } else {
        titulo
    }
}

/// Agrupa por tema; en cada tema con varios niveles de prelación marca el menor como prevalente
pub(crate) fn agrupar_por_tema(
    puntuados: &[Puntuado],
    revision_de: &HashMap<(String, i64), String>,
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
            let minimo = miembros.iter().map(|p| p.fragmento.nivel_prelacion).min().unwrap_or(5);
            let conflicto = miembros.iter().any(|p| p.fragmento.nivel_prelacion != minimo);
            let resultados: Vec<Value> = miembros
                .iter()
                .map(|p| {
                    let f = p.fragmento;
                    let prevalece = f.nivel_prelacion == minimo;
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
                        "cita": {
                            "doc_id": f.doc_id,
                            "revision": revision_de.get(&(f.doc_id.clone(), i64::from(f.version_num))),
                            "version_num": f.version_num,
                            "chunk_origen_id": f.chunk_origen_id,
                        },
                    })
                })
                .collect();
            json!({ "tema": clave, "conflicto_prelacion": conflicto, "resultados": resultados })
        })
        .collect()
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
        let doc_filtro = params
            .doc_id
            .as_deref()
            .map(codigo_canonico)
            .filter(|d| !d.is_empty());

        // Revisiones elegibles (vigentes o en la fecha) y su revisión legible para la cita
        let revisiones = self
            .consultar(
                &contexto,
                &format!(
                    "SELECT d.doc_id, d.version_num, d.revision FROM {} d WHERE {}",
                    contexto.vista(TABLA_DOCUMENTOS_CATALOGO),
                    contexto.predicado_vigencia("d", "d")
                ),
            )
            .await?;
        let revision_de: HashMap<(String, i64), String> = revisiones
            .iter()
            .map(|r| {
                (
                    (
                        texto(r, "doc_id"),
                        entero(r, "version_num").unwrap_or_default(),
                    ),
                    texto(r, "revision"),
                )
            })
            .collect();

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
                doc_filtro
                    .as_ref()
                    .is_none_or(|d| codigo_canonico(&f.doc_id) == *d)
            })
            .collect();

        let (vector, aviso) = match self.embeddings.generate_embedding(consulta).await {
            Ok(v) => (Some(v), None),
            Err(e) => (
                None,
                Some(format!("Sin búsqueda vectorial ({e}): solo léxica")),
            ),
        };
        let puntuados = fusionar(&elegibles, consulta, vector.as_deref(), top_k);
        Ok(json!({
            "centro_id": contexto.centro.as_str(),
            "fecha_referencia": contexto.fecha_json(),
            "consulta": consulta,
            "total": puntuados.len(),
            "grupos": agrupar_por_tema(&puntuados, &revision_de),
            "aviso": aviso,
            "nota": "Incluye siempre la condicion_aplicacion de cada resultado. Si un tema tiene conflicto_prelacion, responde con el resultado prevalece: true y menciona el supletorio.",
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
        let puntuados = fusionar(&todos, "acopio", Some(&[1.0, 0.0]), 5);
        assert_eq!(puntuados.len(), 1);
        assert_eq!(puntuados[0].fragmento.id, "a");
        assert!(fusionar(&todos, "inexistente", None, 5).is_empty());
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
