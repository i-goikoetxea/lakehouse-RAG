//! `lookup_double_entry_table`: cruce exacto en Rust con la inclusividad de cada extremo
//! (problema 2). Nunca interpola: sin celda o con varias candidatas lo dice explícitamente.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::apartado_tabla::{
    ApartadoTabla, ApartadosDeTabla, COLUMNAS_CHUNK_APARTADO, ChunkApartado,
    LIMITE_CONTEXTO_APARTADO,
};
use super::estructura::campos_de_paginas;
use super::estructura::{NOTA_SEGURIDAD, seccion_real_de};
use super::{
    ContextoConsulta, LakehouseToolService, booleano, decimal, entero, literal_sql, texto,
    texto_opcional,
};
use crate::application::dtos::tools::{
    LakehouseToolName, LookupDoubleEntryTableParams, ValorEje, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::{
    TABLA_CATALOGO_TABLAS, TABLA_CELDAS_DOBLE_ENTRADA, TABLA_CHUNKS_NORMATIVA,
};
use crate::domain::errors::DomainError;
use crate::domain::models::double_entry_table::{LimiteRango, RangoNumerico};
use crate::domain::services::marcas_pagina::marca_de_pagina;
use crate::domain::services::normalizacion::{codigo_canonico, plegar_mayusculas};

/// Valor de entrada ya validado: número finito y/o su forma textual normalizada
#[derive(Debug, Clone, PartialEq)]
struct ValorEntrada {
    numero: Option<f64>,
    categoria: String,
}

impl ValorEntrada {
    fn desde(eje: &str, valor: &ValorEje) -> Result<Self, DomainError> {
        let (numero, original) = match valor {
            ValorEje::Numero(n) => (Some(*n), n.to_string()),
            ValorEje::Texto(t) => {
                let limpio = t.trim();
                if limpio.is_empty() {
                    return Err(DomainError::ValidationError(format!(
                        "valor_{eje} está vacío"
                    )));
                }
                (
                    limpio.replace(',', ".").parse::<f64>().ok(),
                    limpio.to_string(),
                )
            }
        };
        if numero.is_some_and(|n| !n.is_finite()) {
            return Err(DomainError::ValidationError(format!(
                "valor_{eje} debe ser un número finito (se recibió '{original}')"
            )));
        }
        Ok(Self {
            numero,
            categoria: plegar_mayusculas(&original).trim().to_string(),
        })
    }

    fn a_json(&self) -> Value {
        self.numero.map_or(json!(self.categoria), |n| json!(n))
    }
}

fn limite(valor: Option<f64>, inclusivo: bool) -> LimiteRango {
    match valor {
        None => LimiteRango::Infinito,
        Some(v) if inclusivo => LimiteRango::Inclusivo(v),
        Some(v) => LimiteRango::Exclusivo(v),
    }
}

/// Evalúa un eje de una celda: categoría exacta o rango con la inclusividad de cada extremo
fn eje_contiene(celda: &Map<String, Value>, eje: &str, valor: &ValorEntrada) -> bool {
    if let Some(categoria) = texto_opcional(celda, &format!("eje_{eje}_categoria")) {
        return plegar_mayusculas(&categoria).trim() == valor.categoria;
    }
    let Some(numero) = valor.numero else {
        return false;
    };
    RangoNumerico::new(
        limite(
            decimal(celda, &format!("eje_{eje}_min")),
            booleano(celda, &format!("eje_{eje}_min_inclusivo")),
        ),
        limite(
            decimal(celda, &format!("eje_{eje}_max")),
            booleano(celda, &format!("eje_{eje}_max_inclusivo")),
        ),
    )
    .is_ok_and(|rango| rango.contiene(numero))
}

/// Descripción legible del rango de un eje (`x <= 20`, `20 < x <= 40`, `x > 40`)
fn describir_eje(celda: &Map<String, Value>, eje: &str) -> String {
    if let Some(categoria) = texto_opcional(celda, &format!("eje_{eje}_categoria")) {
        return format!("{eje} = {categoria}");
    }
    let operador = |extremo: &str, inclusivo: &str, estricto: &str| {
        if booleano(celda, &format!("eje_{eje}_{extremo}_inclusivo")) {
            inclusivo.to_string()
        } else {
            estricto.to_string()
        }
    };
    match (
        decimal(celda, &format!("eje_{eje}_min")),
        decimal(celda, &format!("eje_{eje}_max")),
    ) {
        (None, None) => format!("{eje} cualquiera"),
        (Some(min), None) => format!("{eje} {} {min}", operador("min", ">=", ">")),
        (None, Some(max)) => format!("{eje} {} {max}", operador("max", "<=", "<")),
        (Some(min), Some(max)) => format!(
            "{min} {} {eje} {} {max}",
            operador("min", "<=", "<"),
            operador("max", "<=", "<")
        ),
    }
}

/// Filtra por el tercer eje: si se indica, gana la coincidencia exacta sobre las celdas sin eje z
fn filtrar_eje_z<'a>(
    candidatas: Vec<&'a Map<String, Value>>,
    eje_z: Option<&str>,
) -> Vec<&'a Map<String, Value>> {
    let Some(z) = eje_z.map(|z| plegar_mayusculas(z).trim().to_string()) else {
        return candidatas;
    };
    let exactas: Vec<_> = candidatas
        .iter()
        .copied()
        .filter(|c| {
            texto_opcional(c, "eje_z_filtro_adicional")
                .is_some_and(|v| plegar_mayusculas(&v).trim() == z)
        })
        .collect();
    if !exactas.is_empty() {
        return exactas;
    }
    candidatas
        .into_iter()
        .filter(|c| texto_opcional(c, "eje_z_filtro_adicional").is_none())
        .collect()
}

fn celdas_candidatas<'a>(
    celdas: &'a [Map<String, Value>],
    x: &ValorEntrada,
    y: &ValorEntrada,
    eje_z: Option<&str>,
) -> Vec<&'a Map<String, Value>> {
    let por_ejes = celdas
        .iter()
        .filter(|c| eje_contiene(c, "x", x) && eje_contiene(c, "y", y))
        .collect();
    filtrar_eje_z(por_ejes, eje_z)
}

/// `codigo` aparece en `texto` como segmento completo (`4-A` en `TABLA-4-A-TEMPERATURA`, no en `14-A`)
fn contiene_segmento(texto: &str, codigo: &str) -> bool {
    !codigo.is_empty()
        && texto.match_indices(codigo).any(|(inicio, _)| {
            let fin = inicio + codigo.len();
            (inicio == 0 || texto[..inicio].ends_with('-'))
                && (fin == texto.len() || texto[fin..].starts_with('-'))
        })
}

/// El agente puede nombrar la tabla por su id versionado (`PO-ALM-03@v4-TAB-0000`) o por el
/// código del documento (`4-A`, `Tabla 4-A`), que figura en el título
fn tabla_coincide(fila: &Map<String, Value>, tabla_id: &str) -> bool {
    let buscado = codigo_canonico(tabla_id);
    let buscado = buscado.strip_prefix("TABLA-").unwrap_or(&buscado);
    codigo_canonico(&texto(fila, "tabla_id")) == buscado
        || contiene_segmento(&codigo_canonico(&texto(fila, "titulo_tabla")), buscado)
}

fn tabla_contiene_texto(fila: &Map<String, Value>, busqueda: &str) -> bool {
    let pajar = plegar_mayusculas(&format!(
        "{} {} {} {} {}",
        texto(fila, "tabla_id"),
        texto(fila, "titulo_tabla"),
        texto(fila, "eje_x_nombre"),
        texto(fila, "eje_y_nombre"),
        texto(fila, "valor_salida_nombre")
    ));
    plegar_mayusculas(busqueda)
        .split_whitespace()
        .all(|palabra| pajar.contains(palabra))
}

/// Nota del contexto del apartado para el agente
const NOTA_CONTEXTO_APARTADO: &str = "Texto del mismo apartado que la tabla, sin sus filas ([…] marca las filas omitidas). Puede fijar excepciones, ampliaciones, condiciones o notas que modifican el valor de la celda: tenlo en cuenta antes de responder y cítalo si aplica. Si recortado es true, lee el apartado completo con stair_get_subtree_context y este nodo_id. Las marcas ⟦p. N⟧ indican la página física.";

/// Prosa del apartado de la tabla que acompaña al cruce (mejora 3 del SSD de Capa 2)
fn contexto_del_apartado(apartado: &ApartadoTabla) -> Option<Value> {
    let contexto = apartado.contexto(LIMITE_CONTEXTO_APARTADO)?;
    Some(json!({
        "nodo_id": seccion_real_de(&apartado.nodo_id),
        "texto": contexto.texto,
        "recortado": contexto.recortado,
        "nota": NOTA_CONTEXTO_APARTADO,
        "nota_seguridad": NOTA_SEGURIDAD,
    }))
}

/// Cita de una celda: su revisión y, si se conocen, las páginas del apartado de la tabla y la
/// página de su fila (`⟦p. N⟧`, como las marcas de las demás tools)
fn cita_de_celda(celda: &Map<String, Value>, apartados: &ApartadosDeTabla) -> Value {
    let mut cita = json!({
        "doc_id": texto(celda, "doc_id"),
        "revision": texto(celda, "revision"),
        "version_num": entero(celda, "version_num"),
        "tabla_id": texto(celda, "tabla_id"),
        "celda_id": texto(celda, "celda_id"),
    });
    if let Some((primera, ultima)) = apartados.de_celda(celda).and_then(ApartadoTabla::paginas) {
        if let (Some(destino), Value::Object(paginas)) = (
            cita.as_object_mut(),
            campos_de_paginas(Some(i64::from(primera)), Some(i64::from(ultima))),
        ) {
            destino.extend(paginas);
        }
    }
    if let Some(pagina) = apartados.pagina_de_celda(celda) {
        cita["pagina"] = json!(marca_de_pagina(pagina));
    }
    cita
}

fn celda_salida(celda: &Map<String, Value>, apartados: &ApartadosDeTabla) -> Value {
    let nota = texto_opcional(celda, "nota_pie_condicion");
    let valor_texto = texto(celda, "valor_resultado_texto");
    let llamada_sin_nota = nota.is_none() && valor_texto.contains('(');
    json!({
        "valor_resultado_texto": valor_texto,
        "valor_resultado_num": decimal(celda, "valor_resultado_num"),
        "unidad_medida": texto_opcional(celda, "unidad_medida"),
        "nota_pie_condicion": nota,
        "nota_obligatoria": nota.is_some(),
        "aviso_nota": if llamada_sin_nota {
            Some("El valor lleva una llamada a nota pero la celda no trae su texto: búscala con search_normative_context antes de responder")
        } else { None },
        "rango_x": describir_eje(celda, "x"),
        "rango_y": describir_eje(celda, "y"),
        "eje_z": texto_opcional(celda, "eje_z_filtro_adicional"),
        "nivel_prelacion": entero(celda, "nivel_prelacion"),
        "cita": cita_de_celda(celda, apartados),
    })
}

/// Marca `prevalece` en las candidatas del nivel de prelación más bajo si hay varios niveles
fn marcar_prelacion(
    candidatas: &[&Map<String, Value>],
    apartados: &ApartadosDeTabla,
) -> Vec<Value> {
    let niveles: BTreeSet<i64> = candidatas
        .iter()
        .filter_map(|c| entero(c, "nivel_prelacion"))
        .collect();
    let minimo = niveles.first().copied();
    let hay_conflicto = niveles.len() > 1;
    candidatas
        .iter()
        .map(|c| {
            let mut salida = celda_salida(c, apartados);
            salida["prevalece"] = json!(hay_conflicto && entero(c, "nivel_prelacion") == minimo);
            salida
        })
        .collect()
}

fn identidades(candidatas: &[&Map<String, Value>]) -> BTreeSet<(String, Option<i64>, String)> {
    candidatas
        .iter()
        .map(|c| {
            (
                texto(c, "doc_id"),
                entero(c, "version_num"),
                texto(c, "celda_id"),
            )
        })
        .collect()
}

fn lado_del_aviso(
    x: &ValorEntrada,
    y: &ValorEntrada,
    candidatas: &[&Map<String, Value>],
    apartados: &ApartadosDeTabla,
) -> Value {
    json!({
        "valor_x": x.a_json(),
        "valor_y": y.a_json(),
        "celdas": marcar_prelacion(candidatas, apartados),
    })
}

/// La tool no sabe qué eje es cuál: repite el cruce invertido y avisa si eso cambia el resultado.
/// `estado` y el valor del orden recibido no se tocan, para no romper a quien ya los lee.
fn anotar_orden_invertido(
    salida: &mut Value,
    celdas: &[Map<String, Value>],
    recibidas: &[&Map<String, Value>],
    x: &ValorEntrada,
    y: &ValorEntrada,
    eje_z: Option<&str>,
    apartados: &ApartadosDeTabla,
) {
    let invertidas = celdas_candidatas(celdas, y, x, eje_z);
    if invertidas.is_empty() || identidades(recibidas) == identidades(&invertidas) {
        return;
    }
    if recibidas.is_empty() {
        // Sin la celda: el valor solo existe si el modelo repite la llamada con los ejes correctos.
        salida["orden_invertido_resolveria"] = json!({
            "valor_x": y.a_json(),
            "valor_y": x.a_json(),
            "mensaje": "Con los ejes invertidos sí hay celda. Comprueba con los nombres y unidades de la tabla qué eje es cada valor: si estaban cambiados, repite la llamada con ese orden; si no, la tabla no cubre el caso.",
        });
        return;
    }
    salida["ejes_ambiguos"] = json!({
        "mensaje": "Los dos órdenes de los ejes resuelven con celdas distintas. Decide por las unidades o los nombres de los ejes; si no lo distinguen, muestra ambos valores o pregunta. Nunca elijas uno en silencio.",
        "eje_x": salida["tabla"]["eje_x"].clone(),
        "eje_y": salida["tabla"]["eje_y"].clone(),
        "orden_recibido": lado_del_aviso(x, y, recibidas, apartados),
        "orden_invertido": lado_del_aviso(y, x, &invertidas, apartados),
    });
}

impl LakehouseToolService {
    /// Apartado de la tabla en cada revisión elegida (puede no haberlo: tabla sin vínculos)
    async fn apartados_de_tabla(
        &self,
        contexto: &ContextoConsulta,
        elegidas: &[&Map<String, Value>],
        celdas: &[Map<String, Value>],
    ) -> Result<ApartadosDeTabla, DomainError> {
        let mut apartados = ApartadosDeTabla::default();
        for tabla in elegidas {
            let doc_id = texto(tabla, "doc_id");
            let version = entero(tabla, "version_num").unwrap_or_default();
            let chunks: Vec<ChunkApartado> = self
                .consultar(
                    contexto,
                    &format!(
                        "SELECT {COLUMNAS_CHUNK_APARTADO} FROM {} c WHERE c.doc_id = {} AND c.version_num = {version}",
                        contexto.vista(TABLA_CHUNKS_NORMATIVA),
                        literal_sql(&doc_id),
                    ),
                )
                .await?
                .iter()
                .map(ChunkApartado::desde_fila)
                .collect();
            let valores: Vec<String> = celdas
                .iter()
                .filter(|c| {
                    texto(c, "doc_id") == doc_id
                        && entero(c, "version_num").unwrap_or_default() == version
                })
                .map(|c| texto(c, "valor_resultado_texto"))
                .collect();
            if let Some(apartado) =
                ApartadoTabla::localizar(&chunks, &texto(tabla, "tabla_id"), &valores)
            {
                apartados.por_revision.insert((doc_id, version), apartado);
            }
        }
        Ok(apartados)
    }

    pub(super) async fn cruzar_tabla(&self, argumentos: &Value) -> Result<Value, DomainError> {
        let params: LookupDoubleEntryTableParams =
            parsear_argumentos(LakehouseToolName::LookupDoubleEntryTable, argumentos)?;
        let contexto = self
            .contexto(&params.centro_id, params.fecha_referencia.as_deref())
            .await?;
        let x = ValorEntrada::desde("x", &params.valor_x)?;
        let y = ValorEntrada::desde("y", &params.valor_y)?;

        let tablas = self
            .consultar(
                &contexto,
                &format!(
                    "SELECT t.tabla_id, t.titulo_tabla, t.doc_id, t.version_num, d.revision, t.eje_x_nombre, t.eje_x_unidad, t.eje_y_nombre, t.eje_y_unidad, t.valor_salida_nombre FROM {} t {} WHERE {} ORDER BY d.nivel_prelacion, t.tabla_id",
                    contexto.vista(TABLA_CATALOGO_TABLAS),
                    contexto.union_catalogo("t", "d"),
                    contexto.predicado_firme("t", "d")
                ),
            )
            .await?;
        let elegidas: Vec<&Map<String, Value>> = match (
            params
                .tabla_id
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty()),
            params
                .busqueda_tabla
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty()),
        ) {
            (Some(tabla_id), _) => tablas
                .iter()
                .filter(|t| tabla_coincide(t, tabla_id))
                .collect(),
            (None, Some(busqueda)) => tablas
                .iter()
                .filter(|t| tabla_contiene_texto(t, busqueda))
                .collect(),
            (None, None) => {
                return Err(DomainError::ValidationError(
                    "Indica tabla_id o busqueda_tabla".to_string(),
                ));
            }
        };
        let base = json!({
            "centro_id": contexto.centro.as_str(),
            "fecha_referencia": contexto.fecha_json(),
            "entrada": { "valor_x": x.a_json(), "valor_y": y.a_json(), "eje_z": params.eje_z },
        });
        let ids_distintos: BTreeSet<String> = elegidas
            .iter()
            .map(|t| codigo_canonico(&texto(t, "tabla_id")))
            .collect();
        if elegidas.is_empty() || ids_distintos.len() > 1 {
            let disponibles: Vec<Value> = (if elegidas.is_empty() { tablas.iter().collect() } else { elegidas.clone() })
                .iter()
                .map(|t| json!({ "tabla_id": texto(t, "tabla_id"), "titulo_tabla": texto(t, "titulo_tabla"), "doc_id": texto(t, "doc_id") }))
                .collect();
            let (estado, mensaje) = if elegidas.is_empty() {
                (
                    "tabla_no_encontrada",
                    "Ninguna tabla vigente coincide: elige un tabla_id de la lista",
                )
            } else {
                (
                    "tabla_ambigua",
                    "Varias tablas coinciden con la búsqueda: repite con un tabla_id concreto",
                )
            };
            let mut salida = base;
            salida["estado"] = json!(estado);
            salida["mensaje"] = json!(mensaje);
            salida["tablas"] = json!(disponibles);
            return Ok(salida);
        }

        let tabla = elegidas[0];
        let versiones: Vec<(String, i64)> = elegidas
            .iter()
            .map(|t| {
                (
                    texto(t, "doc_id"),
                    entero(t, "version_num").unwrap_or_default(),
                )
            })
            .collect();
        let celdas: Vec<Map<String, Value>> = self
            .consultar(
                &contexto,
                &format!(
                    "SELECT e.*, d.revision FROM {} e {} WHERE {} AND e.tabla_id = {}",
                    contexto.vista(TABLA_CELDAS_DOBLE_ENTRADA),
                    contexto.union_catalogo("e", "d"),
                    contexto.predicado_firme("e", "d"),
                    literal_sql(&texto(tabla, "tabla_id"))
                ),
            )
            .await?
            .into_iter()
            .filter(|c| {
                versiones.contains(&(
                    texto(c, "doc_id"),
                    entero(c, "version_num").unwrap_or_default(),
                ))
            })
            .collect();

        let apartados = self
            .apartados_de_tabla(&contexto, &elegidas, &celdas)
            .await?;
        let candidatas = celdas_candidatas(&celdas, &x, &y, params.eje_z.as_deref());
        let mut salida = base;
        let clave_tabla = (
            texto(tabla, "doc_id"),
            entero(tabla, "version_num").unwrap_or_default(),
        );
        if let Some(contexto) = apartados
            .por_revision
            .get(&clave_tabla)
            .and_then(contexto_del_apartado)
        {
            salida["contexto_apartado"] = contexto;
        }
        salida["tabla"] = json!({
            "tabla_id": texto(tabla, "tabla_id"),
            "titulo_tabla": texto(tabla, "titulo_tabla"),
            "eje_x": { "nombre": texto(tabla, "eje_x_nombre"), "unidad": texto_opcional(tabla, "eje_x_unidad") },
            "eje_y": { "nombre": texto(tabla, "eje_y_nombre"), "unidad": texto_opcional(tabla, "eje_y_unidad") },
            "valor_salida_nombre": texto(tabla, "valor_salida_nombre"),
        });
        match candidatas.as_slice() {
            [] => {
                let rangos: BTreeSet<String> = celdas
                    .iter()
                    .map(|c| format!("{} | {}", describir_eje(c, "x"), describir_eje(c, "y")))
                    .collect();
                salida["estado"] = json!("sin_coincidencia");
                salida["mensaje"] = json!(
                    "Ninguna celda contiene esos valores. No se interpola: informa de que la tabla no cubre el caso."
                );
                salida["rangos_disponibles"] = json!(rangos);
            }
            [unica] => {
                let celda = celda_salida(unica, &apartados);
                for (clave, valor) in celda.as_object().into_iter().flatten() {
                    salida[clave] = valor.clone();
                }
                salida["estado"] = json!("resuelto");
            }
            varias => {
                salida["estado"] = json!("ambiguo");
                salida["mensaje"] = json!(format!(
                    "{} celdas candidatas: no elijas una arbitrariamente. Si difieren en eje_z, pide ese dato; si difieren en prelación, prevalece la marcada.",
                    varias.len()
                ));
                salida["candidatas"] = json!(marcar_prelacion(varias, &apartados));
            }
        }
        anotar_orden_invertido(
            &mut salida,
            &celdas,
            &candidatas,
            &x,
            &y,
            params.eje_z.as_deref(),
            &apartados,
        );
        Ok(salida)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn celda(
        x: (Option<f64>, bool, Option<f64>, bool),
        valor: &str,
        z: Option<&str>,
    ) -> Map<String, Value> {
        json!({
            "eje_x_min": x.0, "eje_x_min_inclusivo": x.1, "eje_x_max": x.2, "eje_x_max_inclusivo": x.3,
            "eje_y_min": 0.40, "eje_y_min_inclusivo": true, "eje_y_max": 0.45, "eje_y_max_inclusivo": true,
            "valor_resultado_texto": valor, "eje_z_filtro_adicional": z
        })
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
    }

    fn tabla_4a() -> Vec<Map<String, Value>> {
        vec![
            celda((None, false, Some(20.0), true), "75", None),
            celda((Some(20.0), false, Some(40.0), true), "125 (*)", None),
            celda((Some(40.0), false, None, false), "150", None),
        ]
    }

    fn valor(n: f64) -> ValorEntrada {
        ValorEntrada::desde("x", &ValorEje::Numero(n)).unwrap()
    }

    fn resolver(x: f64) -> Vec<String> {
        let celdas = tabla_4a();
        celdas_candidatas(&celdas, &valor(x), &valor(0.42), None)
            .iter()
            .map(|c| texto(c, "valor_resultado_texto"))
            .collect()
    }

    #[test]
    fn test_fronteras_respetan_la_inclusividad_de_cada_extremo() {
        assert_eq!(resolver(20.0), vec!["75"]);
        assert_eq!(resolver(20.0001), vec!["125 (*)"]);
        assert_eq!(resolver(30.0), vec!["125 (*)"]);
        assert_eq!(resolver(40.0), vec!["125 (*)"]);
        assert_eq!(resolver(40.0001), vec!["150"]);
        let celdas = tabla_4a();
        assert!(celdas_candidatas(&celdas, &valor(30.0), &valor(0.46), None).is_empty());
    }

    type Limites = (f64, bool, f64, bool);

    fn celda_cruzada(
        celda_id: &str,
        x: Limites,
        y: Limites,
        resultado: &str,
    ) -> Map<String, Value> {
        json!({
            "celda_id": celda_id, "doc_id": "DOC-1", "version_num": 1,
            "eje_x_min": x.0, "eje_x_min_inclusivo": x.1, "eje_x_max": x.2, "eje_x_max_inclusivo": x.3,
            "eje_y_min": y.0, "eje_y_min_inclusivo": y.1, "eje_y_max": y.2, "eje_y_max_inclusivo": y.3,
            "valor_resultado_texto": resultado,
        })
        .as_object()
        .unwrap()
        .clone()
    }

    fn celda_categorica(celda_id: &str, x: &str, y: &str, resultado: &str) -> Map<String, Value> {
        json!({
            "celda_id": celda_id, "doc_id": "DOC-1", "version_num": 1,
            "eje_x_categoria": x, "eje_y_categoria": y,
            "valor_resultado_texto": resultado,
        })
        .as_object()
        .unwrap()
        .clone()
    }

    /// `A` y `B` tienen los rangos cruzados: con ejes solapados, el intercambio cambia de celda.
    fn tabla_solapada() -> Vec<Map<String, Value>> {
        vec![
            celda_cruzada(
                "A",
                (0.0, true, 20.0, false),
                (20.0, true, 40.0, false),
                "A",
            ),
            celda_cruzada(
                "B",
                (20.0, true, 40.0, false),
                (0.0, true, 20.0, false),
                "B",
            ),
        ]
    }

    fn entrada(valor_eje: ValorEje) -> ValorEntrada {
        ValorEntrada::desde("x", &valor_eje).unwrap()
    }

    /// Resultado de anotar un cruce sobre una salida que solo trae los ejes de la tabla
    fn salida_anotada(
        celdas: &[Map<String, Value>],
        x: ValorEje,
        y: ValorEje,
        eje_z: Option<&str>,
        unidades: (&str, &str),
    ) -> Value {
        let (x, y) = (entrada(x), entrada(y));
        let recibidas = celdas_candidatas(celdas, &x, &y, eje_z);
        let mut salida = json!({ "tabla": {
            "eje_x": { "nombre": "espesor", "unidad": unidades.0 },
            "eje_y": { "nombre": "ancho", "unidad": unidades.1 },
        }});
        anotar_orden_invertido(
            &mut salida,
            celdas,
            &recibidas,
            &x,
            &y,
            eje_z,
            &ApartadosDeTabla::default(),
        );
        salida
    }

    fn numeros(celdas: &[Map<String, Value>], x: f64, y: f64) -> Value {
        salida_anotada(
            celdas,
            ValorEje::Numero(x),
            ValorEje::Numero(y),
            None,
            ("mm", "mm"),
        )
    }

    fn sin_anotaciones(salida: &Value) -> bool {
        salida.get("ejes_ambiguos").is_none() && salida.get("orden_invertido_resolveria").is_none()
    }

    fn valor_de_la_primera_celda(salida: &Value, orden: &str) -> String {
        salida["ejes_ambiguos"][orden]["celdas"][0]["valor_resultado_texto"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

    /// Spike de la Fase 0 (§4.4): con rangos disjuntos el orden invertido es el único que resuelve,
    /// y se dice sin devolver su valor, para que el modelo repita el cruce en vez de copiarlo.
    #[test]
    fn test_ejes_intercambiados_con_rangos_disjuntos_indican_que_el_invertido_resolveria() {
        let celdas = tabla_4a();
        assert!(celdas_candidatas(&celdas, &valor(0.42), &valor(30.0), None).is_empty());
        let salida = numeros(&celdas, 0.42, 30.0);
        assert!(salida.get("ejes_ambiguos").is_none(), "{salida}");
        let aviso = &salida["orden_invertido_resolveria"];
        assert_eq!(aviso["valor_x"], 30.0, "{salida}");
        assert_eq!(aviso["valor_y"], 0.42, "{salida}");
        assert!(aviso.get("celdas").is_none(), "{salida}");
        assert!(!salida.to_string().contains("125"), "{salida}");
    }

    /// El orden recibido se mantiene: la tool no sabe cuál es el bueno, lo avisa con ambas celdas.
    #[test]
    fn test_ejes_intercambiados_con_rangos_solapados_avisan_de_ejes_ambiguos() {
        let celdas = tabla_solapada();
        let solo_ejes = |x, y| -> Vec<String> {
            celdas_candidatas(&celdas, &valor(x), &valor(y), None)
                .iter()
                .map(|c| texto(c, "valor_resultado_texto"))
                .collect()
        };
        assert_eq!(solo_ejes(10.0, 30.0), vec!["A"]);
        assert_eq!(solo_ejes(30.0, 10.0), vec!["B"]);

        let salida = numeros(&celdas, 10.0, 30.0);
        let aviso = &salida["ejes_ambiguos"];
        assert_eq!(aviso["eje_x"]["nombre"], "espesor", "{salida}");
        assert_eq!(aviso["eje_y"]["nombre"], "ancho", "{salida}");
        assert_eq!(aviso["eje_x"]["unidad"], "mm", "{salida}");
        assert_eq!(aviso["orden_recibido"]["valor_x"], 10.0);
        assert_eq!(aviso["orden_recibido"]["valor_y"], 30.0);
        assert_eq!(aviso["orden_invertido"]["valor_x"], 30.0);
        assert_eq!(aviso["orden_invertido"]["valor_y"], 10.0);
        assert_eq!(valor_de_la_primera_celda(&salida, "orden_recibido"), "A");
        assert_eq!(valor_de_la_primera_celda(&salida, "orden_invertido"), "B");
        assert_eq!(
            aviso["orden_invertido"]["celdas"][0]["cita"]["celda_id"],
            "B"
        );
    }

    #[test]
    fn test_rangos_solapados_con_unidades_distintas_las_exponen_en_el_aviso() {
        let salida = salida_anotada(
            &tabla_solapada(),
            ValorEje::Numero(10.0),
            ValorEje::Numero(30.0),
            None,
            ("mm", "%"),
        );
        assert_eq!(salida["ejes_ambiguos"]["eje_x"]["unidad"], "mm", "{salida}");
        assert_eq!(salida["ejes_ambiguos"]["eje_y"]["unidad"], "%", "{salida}");
    }

    #[test]
    fn test_tabla_simetrica_con_la_misma_celda_no_cambia_la_salida() {
        let celdas = vec![celda_cruzada(
            "UNICA",
            (0.0, true, 40.0, true),
            (0.0, true, 40.0, true),
            "V",
        )];
        assert!(sin_anotaciones(&numeros(&celdas, 10.0, 30.0)));
    }

    /// Dos celdas con el mismo valor siguen siendo celdas distintas: pueden diferir en su cita.
    #[test]
    fn test_celdas_distintas_con_el_mismo_valor_tambien_avisan() {
        let celdas = vec![
            celda_cruzada(
                "A",
                (0.0, true, 20.0, false),
                (20.0, true, 40.0, false),
                "V",
            ),
            celda_cruzada(
                "B",
                (20.0, true, 40.0, false),
                (0.0, true, 20.0, false),
                "V",
            ),
        ];
        assert!(numeros(&celdas, 10.0, 30.0).get("ejes_ambiguos").is_some());
    }

    #[test]
    fn test_mismo_valor_en_ambos_ejes_no_avisa() {
        let celdas = vec![
            celda_cruzada("D", (0.0, true, 20.0, false), (0.0, true, 20.0, false), "D"),
            celda_cruzada(
                "A",
                (0.0, true, 20.0, false),
                (20.0, true, 40.0, false),
                "A",
            ),
        ];
        assert!(sin_anotaciones(&numeros(&celdas, 10.0, 10.0)));
    }

    #[test]
    fn test_solo_el_orden_recibido_resuelve_no_cambia_la_salida() {
        let celdas = vec![celda_cruzada(
            "A",
            (0.0, true, 20.0, false),
            (20.0, true, 40.0, false),
            "A",
        )];
        assert!(sin_anotaciones(&numeros(&celdas, 10.0, 30.0)));
    }

    #[test]
    fn test_ningun_orden_resuelve_no_anota_nada() {
        assert!(sin_anotaciones(&numeros(&tabla_solapada(), 100.0, 200.0)));
    }

    #[test]
    fn test_ejes_categoricos_solapados_avisan_con_ambas_celdas() {
        let celdas = vec![
            celda_categorica("A", "Clase I", "Clase II", "A"),
            celda_categorica("B", "Clase II", "Clase I", "B"),
        ];
        let salida = salida_anotada(
            &celdas,
            ValorEje::Texto("clase i".into()),
            ValorEje::Texto("CLASE II".into()),
            None,
            ("", ""),
        );
        assert_eq!(valor_de_la_primera_celda(&salida, "orden_recibido"), "A");
        assert_eq!(valor_de_la_primera_celda(&salida, "orden_invertido"), "B");
    }

    #[test]
    fn test_categoria_frente_a_numero_solo_admite_un_orden() {
        let celdas = vec![json!({
            "celda_id": "A", "doc_id": "DOC-1", "version_num": 1,
            "eje_x_categoria": "Clase I",
            "eje_y_min": 0.0, "eje_y_min_inclusivo": true, "eje_y_max": 40.0, "eje_y_max_inclusivo": true,
            "valor_resultado_texto": "A",
        })
        .as_object()
        .unwrap()
        .clone()];
        let bien = salida_anotada(
            &celdas,
            ValorEje::Texto("clase i".into()),
            ValorEje::Numero(30.0),
            None,
            ("", "mm"),
        );
        assert!(sin_anotaciones(&bien), "{bien}");
        let invertida = salida_anotada(
            &celdas,
            ValorEje::Numero(30.0),
            ValorEje::Texto("clase i".into()),
            None,
            ("", "mm"),
        );
        assert_eq!(
            invertida["orden_invertido_resolveria"]["valor_y"], 30.0,
            "{invertida}"
        );
    }

    #[test]
    fn test_frontera_inclusiva_decide_si_el_orden_invertido_resuelve() {
        let celdas = vec![
            celda_cruzada("A", (0.0, true, 20.0, true), (20.0, false, 40.0, true), "A"),
            celda_cruzada("B", (20.0, false, 40.0, true), (0.0, true, 20.0, true), "B"),
        ];
        // (20, 20) cae fuera en los dos órdenes: 20 está excluido del extremo inferior.
        assert!(sin_anotaciones(&numeros(&celdas, 20.0, 20.0)));
        // Justo encima de la frontera, cada orden cae en una celda.
        let encima = numeros(&celdas, 20.0, 20.0001);
        assert_eq!(
            valor_de_la_primera_celda(&encima, "orden_recibido"),
            "A",
            "{encima}"
        );
        assert_eq!(
            valor_de_la_primera_celda(&encima, "orden_invertido"),
            "B",
            "{encima}"
        );
        // El extremo superior inclusivo 40 resuelve; un paso más allá, solo queda sin celda.
        assert!(numeros(&celdas, 20.0, 40.0).get("ejes_ambiguos").is_some());
        assert!(sin_anotaciones(&numeros(&celdas, 20.0, 40.0001)));
    }

    #[test]
    fn test_orden_invertido_respeta_el_eje_z() {
        let mut solo_z = celda_cruzada(
            "B",
            (20.0, true, 40.0, false),
            (0.0, true, 20.0, false),
            "B",
        );
        solo_z.insert("eje_z_filtro_adicional".to_string(), json!("Proceso 135"));
        let celdas = vec![
            celda_cruzada(
                "A",
                (0.0, true, 20.0, false),
                (20.0, true, 40.0, false),
                "A",
            ),
            solo_z,
        ];
        let sin_ese_proceso = salida_anotada(
            &celdas,
            ValorEje::Numero(10.0),
            ValorEje::Numero(30.0),
            Some("Proceso 111"),
            ("mm", "mm"),
        );
        assert!(sin_anotaciones(&sin_ese_proceso), "{sin_ese_proceso}");
        let con_ese_proceso = salida_anotada(
            &celdas,
            ValorEje::Numero(10.0),
            ValorEje::Numero(30.0),
            Some("Proceso 135"),
            ("mm", "mm"),
        );
        assert_eq!(
            valor_de_la_primera_celda(&con_ese_proceso, "orden_invertido"),
            "B"
        );
    }

    #[test]
    fn test_orden_recibido_ambiguo_lista_todas_sus_candidatas_en_el_aviso() {
        let mut celdas = tabla_solapada();
        celdas.push(celda_cruzada(
            "A2",
            (0.0, true, 20.0, false),
            (20.0, true, 40.0, false),
            "A2",
        ));
        let salida = numeros(&celdas, 10.0, 30.0);
        assert_eq!(
            salida["ejes_ambiguos"]["orden_recibido"]["celdas"]
                .as_array()
                .map(Vec::len),
            Some(2),
            "{salida}"
        );
    }

    #[test]
    fn test_valores_textuales_con_coma_y_no_finitos() {
        let coma = ValorEntrada::desde("y", &ValorEje::Texto("0,42".into())).unwrap();
        assert_eq!(coma.numero, Some(0.42));
        for no_finito in ["NaN", "inf", "-Infinity", "1e999"] {
            assert!(
                ValorEntrada::desde("x", &ValorEje::Texto(no_finito.into())).is_err(),
                "{no_finito}"
            );
        }
        assert!(ValorEntrada::desde("x", &ValorEje::Texto("  ".into())).is_err());
        let categoria = ValorEntrada::desde("x", &ValorEje::Texto("Acero S355".into())).unwrap();
        assert_eq!(categoria.numero, None);
        assert_eq!(categoria.categoria, "ACERO S355");
    }

    #[test]
    fn test_eje_z_prefiere_coincidencia_exacta_y_sin_z_es_ambiguo() {
        let celdas = vec![
            celda(
                (Some(20.0), false, Some(40.0), true),
                "100",
                Some("Proceso 111"),
            ),
            celda(
                (Some(20.0), false, Some(40.0), true),
                "110",
                Some("Proceso 135"),
            ),
        ];
        assert_eq!(
            celdas_candidatas(&celdas, &valor(30.0), &valor(0.42), None).len(),
            2
        );
        let con_z = celdas_candidatas(&celdas, &valor(30.0), &valor(0.42), Some("proceso 135"));
        assert_eq!(con_z.len(), 1);
        assert_eq!(texto(con_z[0], "valor_resultado_texto"), "110");
    }

    #[test]
    fn test_tabla_por_id_versionado_o_por_codigo_del_titulo() {
        let fila = json!({ "tabla_id": "PO-ALM-03@v4-TAB-0000", "titulo_tabla": "Tabla 4-A Temperatura mínima" })
            .as_object()
            .unwrap()
            .clone();
        for nombre in ["4-A", "Tabla 4-A", "tabla 4 a", "PO-ALM-03@v4-TAB-0000"] {
            assert!(tabla_coincide(&fila, nombre), "{nombre}");
        }
        for nombre in ["14-A", "4-AB", "B", ""] {
            assert!(!tabla_coincide(&fila, nombre), "{nombre}");
        }
    }

    #[test]
    fn test_descripcion_de_rangos() {
        let celdas = tabla_4a();
        assert_eq!(describir_eje(&celdas[0], "x"), "x <= 20");
        assert_eq!(describir_eje(&celdas[1], "x"), "20 < x <= 40");
        assert_eq!(describir_eje(&celdas[2], "x"), "x > 40");
        assert_eq!(describir_eje(&celdas[1], "y"), "0.4 <= y <= 0.45");
    }
}
