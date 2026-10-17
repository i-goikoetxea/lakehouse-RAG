//! `query_operational_matrix`: filas atómicas con filtros exactos (problema 1)

use serde_json::{Map, Value, json};

use super::{LakehouseToolService, entero, texto, texto_opcional};
use crate::application::dtos::tools::{
    LakehouseToolName, QueryOperationalMatrixParams, RolBuscado, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::{TABLA_CHUNKS_NORMATIVA, TABLA_MATRIZ_OPERATIVA};
use crate::domain::errors::DomainError;
use crate::domain::models::operational_matrix::FaseOperativa;
use crate::domain::services::normalizacion::{
    codigo_canonico, normalizar_token, plegar_mayusculas,
};

/// Filtros ya normalizados (comparación exacta sobre la forma canónica)
#[derive(Debug, Default)]
struct FiltrosMatriz {
    fase: Option<String>,
    rol: Option<String>,
    rol_en: RolBuscado,
    registro: Option<String>,
    doc_id: Option<String>,
    objeto: Option<String>,
}

fn no_vacio(valor: &Option<String>) -> Option<&str> {
    valor.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

impl FiltrosMatriz {
    fn desde(params: &QueryOperationalMatrixParams) -> Result<Self, DomainError> {
        Ok(Self {
            fase: no_vacio(&params.fase)
                .map(|f| FaseOperativa::parse(f).map(|f| f.as_str().to_string()))
                .transpose()?,
            rol: no_vacio(&params.rol).map(normalizar_token),
            rol_en: params.rol_en,
            registro: no_vacio(&params.registro).map(codigo_canonico),
            doc_id: no_vacio(&params.doc_id).map(codigo_canonico),
            objeto: no_vacio(&params.objeto).map(plegar_mayusculas),
        })
    }

    fn fase_canonica(fila: &Map<String, Value>) -> String {
        FaseOperativa::parse(texto(fila, "fase_operativa"))
            .map(|f| f.as_str().to_string())
            .unwrap_or_default()
    }

    fn rol_coincide(&self, fila: &Map<String, Value>, rol: &str) -> bool {
        let igual = |columna: &str| normalizar_token(&texto(fila, columna)) == rol;
        match self.rol_en {
            RolBuscado::Ejecutor => igual("rol_ejecutor"),
            RolBuscado::Responsable => igual("rol_responsable"),
            RolBuscado::Cualquiera => igual("rol_ejecutor") || igual("rol_responsable"),
        }
    }

    fn admite(&self, fila: &Map<String, Value>) -> bool {
        self.fase
            .as_ref()
            .is_none_or(|f| Self::fase_canonica(fila) == *f)
            && self.rol.as_ref().is_none_or(|r| self.rol_coincide(fila, r))
            && self
                .registro
                .as_ref()
                .is_none_or(|r| codigo_canonico(&texto(fila, "registro_codigo")) == *r)
            && self
                .doc_id
                .as_ref()
                .is_none_or(|d| codigo_canonico(&texto(fila, "doc_id")) == *d)
            && self
                .objeto
                .as_ref()
                .is_none_or(|o| plegar_mayusculas(&texto(fila, "objeto_alcance")).contains(o))
    }
}

fn fila_salida(fila: &Map<String, Value>) -> Value {
    json!({
        "hito_id": texto(fila, "hito_id"),
        "fase_operativa": texto(fila, "fase_operativa"),
        "orden_secuencia": entero(fila, "orden_secuencia"),
        "objeto_alcance": texto(fila, "objeto_alcance"),
        "rol_ejecutor": texto(fila, "rol_ejecutor"),
        "rol_responsable": texto(fila, "rol_responsable"),
        "registro_codigo": texto_opcional(fila, "registro_codigo"),
        "registro_nombre": texto_opcional(fila, "registro_nombre"),
        "criterio_aceptacion": texto(fila, "criterio_aceptacion"),
        "condicion_aplicacion": texto_opcional(fila, "condicion_aplicacion"),
        "nivel_prelacion": entero(fila, "nivel_prelacion"),
        "calidad_extraccion": texto(fila, "calidad_extraccion"),
        "cita": {
            "doc_id": texto(fila, "doc_id"),
            "revision": texto(fila, "revision"),
            "version_num": entero(fila, "version_num"),
            "chunk_origen_id": texto_opcional(fila, "chunk_origen_id"),
            "breadcrumb": texto_opcional(fila, "breadcrumb_jerarquia"),
        },
    })
}

impl LakehouseToolService {
    pub(super) async fn consultar_matriz(&self, argumentos: &Value) -> Result<Value, DomainError> {
        let params: QueryOperationalMatrixParams =
            parsear_argumentos(LakehouseToolName::QueryOperationalMatrix, argumentos)?;
        let contexto = self
            .contexto(&params.centro_id, params.fecha_referencia.as_deref())
            .await?;
        let filtros = FiltrosMatriz::desde(&params)?;

        let sql = format!(
            "SELECT m.hito_id, m.doc_id, m.version_num, m.orden_secuencia, m.fase_operativa, m.objeto_alcance, m.rol_ejecutor, m.rol_responsable, m.registro_codigo, m.registro_nombre, m.criterio_aceptacion, m.condicion_aplicacion, m.nivel_prelacion, m.calidad_extraccion, m.chunk_origen_id, d.revision, c.breadcrumb_jerarquia FROM {} m {} LEFT JOIN {} c ON c.chunk_id = m.chunk_origen_id WHERE {} ORDER BY m.nivel_prelacion ASC, m.doc_id ASC, m.orden_secuencia ASC, m.hito_id ASC",
            contexto.vista(TABLA_MATRIZ_OPERATIVA),
            contexto.union_catalogo("m", "d"),
            contexto.vista(TABLA_CHUNKS_NORMATIVA),
            contexto.predicado_vigencia("m", "d"),
        );
        let filas: Vec<Value> = self
            .consultar(&contexto, &sql)
            .await?
            .iter()
            .filter(|f| filtros.admite(f))
            .map(fila_salida)
            .collect();

        Ok(json!({
            "centro_id": contexto.centro.as_str(),
            "fecha_referencia": contexto.fecha_json(),
            "filtros": {
                "fase": filtros.fase,
                "rol": filtros.rol,
                "rol_en": format!("{:?}", filtros.rol_en).to_lowercase(),
                "registro": filtros.registro,
                "doc_id": filtros.doc_id,
                "objeto": filtros.objeto,
            },
            "total": filas.len(),
            "filas": filas,
            "nota": "Cada fila es un hito independiente: no combines el responsable de una fila con el registro de otra. Cita doc_id + revisión e incluye la condicion_aplicacion.",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fila(fase: &str, ejecutor: &str, responsable: &str, registro: &str) -> Map<String, Value> {
        json!({
            "fase_operativa": fase, "rol_ejecutor": ejecutor, "rol_responsable": responsable,
            "registro_codigo": registro, "doc_id": "PO-ALM-03", "objeto_alcance": "Tubería de acero"
        })
        .as_object()
        .unwrap()
        .clone()
    }

    fn filtros(json_params: Value) -> FiltrosMatriz {
        let mut base = json!({ "centro_id": "c" });
        base.as_object_mut()
            .unwrap()
            .extend(json_params.as_object().unwrap().clone());
        FiltrosMatriz::desde(&serde_json::from_value(base).unwrap()).unwrap()
    }

    #[test]
    fn test_filtros_exactos_normalizan_tildes_mayusculas_y_rol() {
        let custodia = fila(
            "CUSTODIA",
            "Auxiliar de Almacén",
            "Jefe de Producción",
            "FOR-ALM-05",
        );
        let recepcion = fila("RECEPCION", "Almacenero", "Jefe de Almacén", "FOR-ALM-01");
        let f = filtros(
            json!({ "fase": "Custodia", "rol": "jefe de produccion", "rol_en": "responsable" }),
        );
        assert!(f.admite(&custodia));
        assert!(!f.admite(&recepcion));
        let por_ejecutor = filtros(json!({ "rol": "Jefe de Producción", "rol_en": "ejecutor" }));
        assert!(!por_ejecutor.admite(&custodia));
        assert!(filtros(json!({ "fase": "Recepción" })).admite(&recepcion));
        assert!(filtros(json!({ "registro": "for alm 05" })).admite(&custodia));
        assert!(!filtros(json!({ "registro": "FOR-ALM-0" })).admite(&custodia));
        assert!(filtros(json!({ "objeto": "tuberia" })).admite(&custodia));
        assert!(
            !filtros(json!({ "rol": "Jefe" })).admite(&custodia),
            "el rol es exacto, no subcadena"
        );
    }
}
