//! `execute_athena_sql`: SQL de solo lectura, exhaustiva y acotada (problema 6)

use chrono::NaiveDate;
use serde_json::{Value, json};

use super::{ContextoConsulta, LakehouseToolService, lotes_a_filas, predicado_ventana};
use crate::application::dtos::tools::{
    ExecuteAthenaSqlParams, LakehouseToolName, parsear_argumentos,
};
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

/// Traduce las 6 tablas canónicas v2 (`03_matriz_operativa`…) a las vistas del centro
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
        let resultado = self
            .motor
            .ejecutar_consulta_lectura(&sql, &politica)
            .await
            .map_err(|e| DomainError::QueryError(format!("{e} | SQL ejecutada: {sql}")))?;
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
