//! `execute_athena_sql`: SQL de solo lectura, exhaustiva y acotada (problema 6)

use chrono::NaiveDate;
use serde_json::{Value, json};

use super::{ContextoConsulta, LakehouseToolService, lotes_a_filas, predicado_ventana};
use crate::application::dtos::tools::{
    ExecuteAthenaSqlParams, LakehouseToolName, parsear_argumentos,
};
use crate::application::ports::analytical_engine::EsquemaTabla;
use crate::application::ports::lakehouse_projection::nombre_vista;
use crate::application::ports::lakehouse_store::{TABLA_DOCUMENTOS_CATALOGO, TODAS_LAS_TABLAS};
use crate::domain::errors::DomainError;
use crate::domain::models::center::CenterId;

/// Longitud máxima de la SQL del agente
const MAX_LONGITUD_SQL: usize = 20_000;

fn es_caracter_de_identificador(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

/// Sustituye cada aparición aislada de `tabla` (sin distinguir mayúsculas) por `destino`
/// entrecomillado; respeta las ya entrecomilladas y las que forman parte de otro identificador.
fn reemplazar_identificador(sql: &str, tabla: &str, destino: &str) -> String {
    let minusculas = sql.to_ascii_lowercase();
    let mut salida = String::with_capacity(sql.len() + destino.len());
    let mut cursor = 0;
    while let Some(relativo) = minusculas[cursor..].find(tabla) {
        let inicio = cursor + relativo;
        let fin = inicio + tabla.len();
        let previo = sql[..inicio].chars().last();
        let siguiente = sql[fin..].chars().next();
        salida.push_str(&sql[cursor..inicio]);
        if previo.is_some_and(es_caracter_de_identificador)
            || siguiente.is_some_and(es_caracter_de_identificador)
        {
            salida.push_str(&sql[inicio..fin]);
        } else if previo == Some('"') && siguiente == Some('"') {
            salida.push_str(destino);
        } else {
            salida.push('"');
            salida.push_str(destino);
            salida.push('"');
        }
        cursor = fin;
    }
    salida.push_str(&sql[cursor..]);
    salida
}

/// Traduce las 7 tablas canónicas (`03_matriz_operativa`…) a las vistas del centro
pub fn reescribir_tablas_canonicas(sql: &str, centro: &CenterId) -> String {
    TODAS_LAS_TABLAS
        .iter()
        .fold(sql.to_string(), |acumulado, tabla| {
            reemplazar_identificador(&acumulado, tabla, &nombre_vista(centro, tabla))
        })
}

/// Con `fecha_referencia`, cada tabla canónica se sustituye por una CTE con su contenido
/// vigente en esa fecha (ventana de `01_documentos_catalogo`)
fn envolver_con_fecha(
    sql: &str,
    centro: &CenterId,
    fecha: NaiveDate,
) -> Result<String, DomainError> {
    let entrecomillada = TODAS_LAS_TABLAS
        .iter()
        .fold(sql.to_string(), |acumulado, tabla| {
            reemplazar_identificador(&acumulado, tabla, tabla)
        });
    let catalogo = format!("\"{}\"", nombre_vista(centro, TABLA_DOCUMENTOS_CATALOGO));
    let ctes: Vec<String> = TODAS_LAS_TABLAS
        .iter()
        .map(|tabla| {
            let vista = format!("\"{}\"", nombre_vista(centro, tabla));
            let cuerpo = if *tabla == TABLA_DOCUMENTOS_CATALOGO {
                format!("SELECT d.* FROM {vista} d WHERE {}", predicado_ventana("d", fecha))
            } else {
                format!(
                    "SELECT t.* FROM {vista} t JOIN {catalogo} d ON t.doc_id = d.doc_id AND t.version_num = d.version_num WHERE {}",
                    predicado_ventana("d", fecha)
                )
            };
            format!("\"{tabla}\" AS ({cuerpo})")
        })
        .collect();
    let recortada = entrecomillada.trim_start();
    let cabecera = recortada.get(..5).map(str::to_ascii_uppercase);
    if cabecera.as_deref() == Some("WITH ") || cabecera.as_deref() == Some("WITH\n") {
        let resto = recortada[4..].trim_start();
        if resto.to_ascii_uppercase().starts_with("RECURSIVE") {
            return Err(DomainError::QueryError(
                "WITH RECURSIVE no se admite junto con fecha_referencia".to_string(),
            ));
        }
        return Ok(format!("WITH {}, {resto}", ctes.join(", ")));
    }
    Ok(format!("WITH {} {recortada}", ctes.join(", ")))
}

impl LakehouseToolService {
    pub(super) async fn ejecutar_sql_agente(
        &self,
        argumentos: &Value,
    ) -> Result<Value, DomainError> {
        let params: ExecuteAthenaSqlParams =
            parsear_argumentos(LakehouseToolName::ExecuteAthenaSql, argumentos)?;
        let contexto = self
            .contexto(&params.centro_id, params.fecha_referencia.as_deref())
            .await?;
        let sql = preparar_sql(&params.sql_query, &contexto)?;
        let politica = self.politica(&contexto, self.config.limite_filas_sql);
        let resultado = match self.motor.ejecutar_consulta_lectura(&sql, &politica).await {
            Ok(resultado) => resultado,
            Err(e @ DomainError::SqlNoInterpretable(_)) => {
                let esquemas = self
                    .motor
                    .esquemas_de_tablas(&politica.tablas_permitidas)
                    .await;
                let diagnostico =
                    diagnostico_de_esquema(&params.sql_query, &contexto.centro, esquemas);
                return Err(DomainError::QueryError(format!(
                    "{e} | SQL ejecutada: {sql}{diagnostico}"
                )));
            }
            Err(e) => {
                return Err(DomainError::QueryError(format!(
                    "{e} | SQL ejecutada: {sql}"
                )));
            }
        };
        let filas = lotes_a_filas(&resultado.lotes)?;
        let mut salida = json!({
            "centro_id": contexto.centro.as_str(),
            "fecha_referencia": contexto.fecha_json(),
            "filas": resultado.filas,
            "truncado": resultado.truncado,
            "limite_filas": politica.limite_filas,
            "resultado": filas,
            "query_ejecutada": sql,
        });
        if resultado.truncado {
            salida["aviso"] = json!(format!(
                "Resultado truncado a {} filas: agrega (COUNT, GROUP BY) o filtra más para un listado exhaustivo",
                politica.limite_filas
            ));
        }
        Ok(salida)
    }
}

/// Tablas y columnas por tabla como máximo en el diagnóstico de esquema
const MAX_TABLAS_DIAGNOSTICO: usize = 8;
const MAX_COLUMNAS_DIAGNOSTICO: usize = 40;
/// Fragmentos de un nombre de tabla más cortos que esto no cuentan como parecido con el SQL
const MIN_FRAGMENTO_PARECIDO: usize = 4;

/// Parecido de una tabla con el SQL del modelo: nombre completo citado (2), algún fragmento
/// significativo del nombre citado (1) o ninguno (0). Sin listas de términos: solo el nombre
fn parecido_con_sql(tabla: &str, sql: &str) -> u8 {
    let tabla = tabla.to_lowercase();
    if sql.contains(&tabla) {
        return 2;
    }
    let cita_fragmento = tabla
        .split(|c: char| !c.is_alphanumeric())
        .filter(|f| {
            f.chars().count() >= MIN_FRAGMENTO_PARECIDO && !f.chars().all(|c| c.is_ascii_digit())
        })
        .any(|f| sql.contains(f));
    u8::from(cita_fragmento)
}

/// Bloque que se añade a un error de parseo o planificación: las tablas que el agente puede
/// leer con sus columnas y tipos reales, leídos del motor. Primero las que se parecen a las del
/// SQL; con tope de tablas y de columnas
fn diagnostico_de_esquema(sql: &str, centro: &CenterId, esquemas: Vec<EsquemaTabla>) -> String {
    if esquemas.is_empty() {
        return String::new();
    }
    let sql = sql.to_lowercase();
    let prefijo = nombre_vista(centro, "");
    let mut tablas: Vec<(u8, EsquemaTabla)> = esquemas
        .into_iter()
        .map(|esquema| {
            let nombre = esquema
                .nombre
                .strip_prefix(&prefijo)
                .unwrap_or(&esquema.nombre)
                .to_string();
            let parecido = parecido_con_sql(&nombre, &sql);
            (
                parecido,
                EsquemaTabla {
                    nombre,
                    columnas: esquema.columnas,
                },
            )
        })
        .collect();
    tablas.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.nombre.cmp(&b.1.nombre)));
    let total = tablas.len();
    let descritas: Vec<String> = tablas
        .into_iter()
        .take(MAX_TABLAS_DIAGNOSTICO)
        .map(|(_, EsquemaTabla { nombre, columnas })| {
            let mut lista: Vec<String> = columnas
                .iter()
                .take(MAX_COLUMNAS_DIAGNOSTICO)
                .map(|(columna, tipo)| format!("{columna} {tipo}"))
                .collect();
            if columnas.len() > MAX_COLUMNAS_DIAGNOSTICO {
                lista.push(format!(
                    "… y {} columnas más",
                    columnas.len() - MAX_COLUMNAS_DIAGNOSTICO
                ));
            }
            format!("{nombre}({})", lista.join(", "))
        })
        .collect();
    let omitidas = total.saturating_sub(MAX_TABLAS_DIAGNOSTICO);
    let resto = if omitidas > 0 {
        format!("; … y {omitidas} tablas más")
    } else {
        String::new()
    };
    format!(
        " | Esquema disponible (tablas que puede leer la consulta, con sus columnas y tipos): {}{resto}",
        descritas.join("; ")
    )
}

fn preparar_sql(sql: &str, contexto: &ContextoConsulta) -> Result<String, DomainError> {
    let sql = sql.trim().trim_end_matches(';').trim();
    if sql.is_empty() {
        return Err(DomainError::QueryError(
            "La consulta SQL está vacía".to_string(),
        ));
    }
    if sql.len() > MAX_LONGITUD_SQL {
        return Err(DomainError::QueryError(format!(
            "La consulta SQL supera {MAX_LONGITUD_SQL} caracteres"
        )));
    }
    match contexto.fecha {
        None => Ok(reescribir_tablas_canonicas(sql, &contexto.centro)),
        Some(fecha) => envolver_con_fecha(sql, &contexto.centro, fecha),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn esquema(nombre: &str, columnas: usize) -> EsquemaTabla {
        EsquemaTabla {
            nombre: nombre.to_string(),
            columnas: (0..columnas)
                .map(|i| (format!("col_{i}"), "Utf8".to_string()))
                .collect(),
        }
    }

    #[test]
    fn test_diagnostico_de_esquema_quita_el_prefijo_del_centro_y_lista_tipos() {
        let centro = CenterId::new("obra-a").unwrap();
        let texto = diagnostico_de_esquema(
            "SELECT * FROM inexistente",
            &centro,
            vec![EsquemaTabla {
                nombre: "obra-a_alfa".to_string(),
                columnas: vec![
                    ("id".to_string(), "Utf8".to_string()),
                    ("valor".to_string(), "Int32".to_string()),
                ],
            }],
        );
        assert!(texto.contains("alfa(id Utf8, valor Int32)"), "{texto}");
        assert!(!texto.contains("obra-a_alfa"), "{texto}");
    }

    #[test]
    fn test_diagnostico_de_esquema_prioriza_las_tablas_parecidas_al_sql() {
        let centro = CenterId::new("c").unwrap();
        let esquemas = vec![
            esquema("c_aaa_primera", 1),
            esquema("c_bbb_registros_turno", 1),
            esquema("c_ccc_medidas", 1),
        ];
        // «medidas» citada entera; «registros» como fragmento de otra tabla
        let texto = diagnostico_de_esquema(
            "SELECT * FROM ccc_medidas JOIN registros ON 1=1",
            &centro,
            esquemas,
        );
        let pos = |t: &str| texto.find(t).unwrap();
        assert!(pos("ccc_medidas(") < pos("bbb_registros_turno("), "{texto}");
        assert!(pos("bbb_registros_turno(") < pos("aaa_primera("), "{texto}");
    }

    #[test]
    fn test_diagnostico_de_esquema_acota_tablas_y_columnas() {
        let centro = CenterId::new("c").unwrap();
        let esquemas: Vec<EsquemaTabla> = (0..MAX_TABLAS_DIAGNOSTICO + 3)
            .map(|i| esquema(&format!("c_t{i:02}"), MAX_COLUMNAS_DIAGNOSTICO + 5))
            .collect();
        let texto = diagnostico_de_esquema("SELECT 1", &centro, esquemas);
        assert_eq!(texto.matches("(col_0 ").count(), MAX_TABLAS_DIAGNOSTICO);
        assert!(texto.contains("… y 3 tablas más"), "{texto}");
        assert!(texto.contains("… y 5 columnas más"), "{texto}");
    }

    #[test]
    fn test_diagnostico_de_esquema_vacio_sin_esquemas() {
        let centro = CenterId::new("c").unwrap();
        assert_eq!(diagnostico_de_esquema("SELECT 1", &centro, vec![]), "");
    }

    #[test]
    fn test_reescritura_traduce_tablas_canonicas_a_vistas_del_centro() {
        let centro = CenterId::new("obra-a").unwrap();
        let sql = "SELECT rol_responsable, COUNT(*) FROM 03_matriz_operativa m JOIN \"01_documentos_catalogo\" d ON m.doc_id = d.doc_id GROUP BY 1";
        let reescrita = reescribir_tablas_canonicas(sql, &centro);
        assert!(
            reescrita.contains("FROM \"obra-a_03_matriz_operativa\" m"),
            "{reescrita}"
        );
        assert!(
            reescrita.contains("JOIN \"obra-a_01_documentos_catalogo\" d"),
            "{reescrita}"
        );
    }

    #[test]
    fn test_reescritura_respeta_identificadores_que_contienen_el_nombre() {
        let centro = CenterId::new("obra-a").unwrap();
        for intacta in [
            "SELECT * FROM \"obra-a_03_matriz_operativa\"",
            "SELECT * FROM x03_matriz_operativa",
            "SELECT * FROM 03_matriz_operativa_bis",
        ] {
            assert_eq!(reescribir_tablas_canonicas(intacta, &centro), intacta);
        }
        assert_eq!(
            reescribir_tablas_canonicas("select * from 06_GRAFO_REFERENCIAS", &centro),
            "select * from \"obra-a_06_grafo_referencias\""
        );
    }

    #[test]
    fn test_fecha_referencia_envuelve_las_tablas_en_ctes_con_la_ventana() {
        let centro = CenterId::new("obra-a").unwrap();
        let fecha = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let sql =
            envolver_con_fecha("SELECT COUNT(*) FROM 03_matriz_operativa", &centro, fecha).unwrap();
        assert!(sql.starts_with("WITH \"01_documentos_catalogo\" AS (SELECT d.* FROM \"obra-a_01_documentos_catalogo\" d WHERE d.fecha_vigor <= DATE '2026-06-01'"), "{sql}");
        assert!(
            sql.ends_with("SELECT COUNT(*) FROM \"03_matriz_operativa\""),
            "{sql}"
        );
        let con_with =
            envolver_con_fecha("WITH x AS (SELECT 1) SELECT * FROM x", &centro, fecha).unwrap();
        assert!(
            con_with.contains("), x AS (SELECT 1) SELECT * FROM x"),
            "{con_with}"
        );
        assert!(
            envolver_con_fecha("WITH RECURSIVE r AS (SELECT 1) SELECT 1", &centro, fecha).is_err()
        );
    }
}
