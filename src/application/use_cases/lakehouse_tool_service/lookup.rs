//! `lookup_double_entry_table`: cruce exacto en Rust con la inclusividad de cada extremo
//! (problema 2). Nunca interpola: sin celda o con varias candidatas lo dice explícitamente.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::{LakehouseToolService, booleano, decimal, entero, literal_sql, texto, texto_opcional};
use crate::application::dtos::tools::{
    LakehouseToolName, LookupDoubleEntryTableParams, ValorEje, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::{
    TABLA_CATALOGO_TABLAS, TABLA_CELDAS_DOBLE_ENTRADA,
};
use crate::domain::errors::DomainError;
use crate::domain::models::double_entry_table::{LimiteRango, RangoNumerico};
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

fn celda_salida(celda: &Map<String, Value>) -> Value {
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
        "cita": {
            "doc_id": texto(celda, "doc_id"),
            "revision": texto(celda, "revision"),
            "version_num": entero(celda, "version_num"),
            "tabla_id": texto(celda, "tabla_id"),
            "celda_id": texto(celda, "celda_id"),
        },
    })
}

/// Marca `prevalece` en las candidatas del nivel de prelación más bajo si hay varios niveles
fn marcar_prelacion(candidatas: &[&Map<String, Value>]) -> Vec<Value> {
    let niveles: BTreeSet<i64> = candidatas
        .iter()
        .filter_map(|c| entero(c, "nivel_prelacion"))
        .collect();
    let minimo = niveles.first().copied();
    let hay_conflicto = niveles.len() > 1;
    candidatas
        .iter()
        .map(|c| {
            let mut salida = celda_salida(c);
            salida["prevalece"] = json!(hay_conflicto && entero(c, "nivel_prelacion") == minimo);
            salida
        })
        .collect()
}

impl LakehouseToolService {
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
                    contexto.predicado_vigencia("t", "d")
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
            "entrada": { "valor_x": x.numero.map_or(json!(x.categoria), |n| json!(n)), "valor_y": y.numero.map_or(json!(y.categoria), |n| json!(n)), "eje_z": params.eje_z },
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
                    contexto.predicado_vigencia("e", "d"),
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

        let candidatas = celdas_candidatas(&celdas, &x, &y, params.eje_z.as_deref());
        let mut salida = base;
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
                let celda = celda_salida(unica);
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
                salida["candidatas"] = json!(marcar_prelacion(varias));
            }
        }
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
