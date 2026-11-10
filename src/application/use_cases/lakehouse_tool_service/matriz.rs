//! `query_operational_matrix`: filas atómicas con filtros exactos (problema 1)

use serde_json::{Map, Value, json};

use super::estructura::campos_de_paginas;
use super::{LakehouseToolService, entero, texto, texto_opcional};
use crate::application::dtos::tools::{
    LakehouseToolName, QueryOperationalMatrixParams, RolBuscado, parsear_argumentos,
};
use crate::application::ports::lakehouse_store::{TABLA_CHUNKS_NORMATIVA, TABLA_MATRIZ_OPERATIVA};
use crate::domain::errors::DomainError;
use crate::domain::models::operational_matrix::{FaseOperativa, OrigenResponsable};
use crate::domain::services::marcas_pagina::marca_de_pagina;
use crate::domain::services::normalizacion::{
    codigo_canonico, normalizar_token, plegar_mayusculas,
};

/// Acción sugerida cuando ningún hito de la consulta tiene responsable declarado
const ACCION_SIN_RESPONSABLE: &str = "Ningún hito de esta consulta tiene responsable declarado. Busca con search_normative_context, cita el párrafo literal y di que no consta responsable. Si la pregunta no trata del responsable, repite con solo_declarados=false.";

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

/// Cita de un hito: su revisión, su chunk de origen y las páginas de ese chunk. La página exacta
/// (`⟦p. N⟧`) solo si el chunk está entero en una página: el texto del hito es una destilación y
/// no se puede situar dentro del chunk
fn cita_de_hito(fila: &Map<String, Value>) -> Value {
    let mut cita = json!({
        "doc_id": texto(fila, "doc_id"),
        "revision": texto(fila, "revision"),
        "version_num": entero(fila, "version_num"),
        "origen_responsable": texto(fila, "origen_responsable"),
        "chunk_origen_id": texto_opcional(fila, "chunk_origen_id"),
        "breadcrumb": texto_opcional(fila, "breadcrumb_jerarquia"),
    });
    let (inicio, fin) = (entero(fila, "pagina_inicio"), entero(fila, "pagina_fin"));
    if inicio.is_some() {
        if let (Some(destino), Value::Object(paginas)) =
            (cita.as_object_mut(), campos_de_paginas(inicio, fin))
        {
            destino.extend(paginas);
        }
    }
    if let Some(pagina) = inicio
        .filter(|i| fin.is_none_or(|f| f == *i))
        .and_then(|i| u32::try_from(i).ok())
        .filter(|p| *p > 0)
    {
        cita["pagina"] = json!(marca_de_pagina(pagina));
    }
    cita
}

fn fila_salida(fila: &Map<String, Value>) -> Value {
    json!({
        "hito_id": texto(fila, "hito_id"),
        "fase_operativa": texto(fila, "fase_operativa"),
        "orden_secuencia": entero(fila, "orden_secuencia"),
        "objeto_alcance": texto(fila, "objeto_alcance"),
        "rol_ejecutor": texto(fila, "rol_ejecutor"),
        "rol_responsable": texto(fila, "rol_responsable"),
        "origen_responsable": texto(fila, "origen_responsable"),
        "vigencia_no_declarada": fila.get("vigencia_no_declarada").and_then(Value::as_bool).unwrap_or(false),
        "registro_codigo": texto_opcional(fila, "registro_codigo"),
        "registro_nombre": texto_opcional(fila, "registro_nombre"),
        "criterio_aceptacion": texto(fila, "criterio_aceptacion"),
        "condicion_aplicacion": texto_opcional(fila, "condicion_aplicacion"),
        "nivel_prelacion": entero(fila, "nivel_prelacion"),
        "calidad_extraccion": texto(fila, "calidad_extraccion"),
        "cita": cita_de_hito(fila),
    })
}

/// Filas que pasan el filtro de origen y los hitos que deja fuera
fn separar_por_origen(filas: Vec<Value>, solo_declarados: bool) -> (Vec<Value>, usize) {
    if !solo_declarados {
        return (filas, 0);
    }
    let total = filas.len();
    let declaradas: Vec<Value> = filas
        .into_iter()
        .filter(tiene_responsable_declarado)
        .collect();
    let excluidos = total - declaradas.len();
    (declaradas, excluidos)
}

fn tiene_responsable_declarado(fila: &Value) -> bool {
    fila["origen_responsable"] == OrigenResponsable::Declarado.as_str()
}

/// Campos que dejan a la vista lo filtrado y qué hacer si no queda ningún responsable declarado
fn senales_de_origen(filas: &[Value], excluidos: usize, solo_declarados: bool) -> Value {
    let sin_responsable = !filas.iter().any(tiene_responsable_declarado);
    let mut senales = json!({
        "solo_declarados": solo_declarados,
        "excluidos_no_declarados": excluidos,
        "sin_responsable_declarado": sin_responsable,
    });
    let accion = if sin_responsable {
        Some(ACCION_SIN_RESPONSABLE.to_string())
    } else if excluidos > 0 {
        Some(format!(
            "{excluidos} hito(s) con responsable INFERIDO o NO_CONSTA quedan fuera. Si la pregunta no trata del responsable, repite con solo_declarados=false: cada fila lleva su origen."
        ))
    } else {
        None
    };
    if let Some(accion) = accion {
        senales["accion_sugerida"] = Value::String(accion);
    }
    senales
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
            "SELECT m.hito_id, m.doc_id, m.version_num, m.orden_secuencia, m.fase_operativa, m.objeto_alcance, m.rol_ejecutor, m.rol_responsable, m.origen_responsable, m.registro_codigo, m.registro_nombre, m.criterio_aceptacion, m.condicion_aplicacion, m.nivel_prelacion, m.calidad_extraccion, m.chunk_origen_id, d.revision, c.breadcrumb_jerarquia, c.pagina_inicio, c.pagina_fin, {} AS vigencia_no_declarada FROM {} m {} LEFT JOIN {} c ON c.chunk_id = m.chunk_origen_id WHERE {} ORDER BY m.nivel_prelacion ASC, m.doc_id ASC, m.orden_secuencia ASC, m.hito_id ASC",
            contexto.marca_no_declarada("d"),
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
        let (filas, excluidos) = separar_por_origen(filas, params.solo_declarados);
        let senales = senales_de_origen(&filas, excluidos, params.solo_declarados);

        let mut salida = json!({
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
            "nota": "Cada fila es un hito independiente: no combines el responsable de una fila con el registro de otra. Cita doc_id + revisión e incluye la condicion_aplicacion. origen_responsable: solo DECLARADO es dato del documento; INFERIDO es una deducción y NO_CONSTA significa que no figura, y hay que decirlo.",
        });
        if let (Some(destino), Value::Object(campos)) = (salida.as_object_mut(), senales) {
            destino.extend(campos);
        }
        Ok(salida)
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

    fn salida(origen: &str, responsable: &str) -> Value {
        json!({ "origen_responsable": origen, "rol_responsable": responsable, "fase_operativa": "CUSTODIA" })
    }

    #[test]
    fn test_solo_declarados_excluye_y_cuenta_el_resto() {
        let filas = vec![
            salida("DECLARADO", "Jefe"),
            salida("INFERIDO", "Inspector"),
            salida("NO_CONSTA", ""),
        ];
        let (declaradas, excluidos) = separar_por_origen(filas.clone(), true);
        assert_eq!(declaradas.len(), 1);
        assert_eq!(excluidos, 2);
        let senales = senales_de_origen(&declaradas, excluidos, true);
        assert_eq!(senales["excluidos_no_declarados"], 2);
        assert_eq!(senales["sin_responsable_declarado"], false);
        assert!(
            senales["accion_sugerida"]
                .as_str()
                .unwrap()
                .contains("solo_declarados")
        );

        let (todas, excluidos) = separar_por_origen(filas, false);
        assert_eq!((todas.len(), excluidos), (3, 0));
        assert_eq!(todas[1]["origen_responsable"], "INFERIDO");
    }

    #[test]
    fn test_sin_responsable_declarado_sugiere_buscar_en_la_capa_0() {
        let (filas, excluidos) = separar_por_origen(vec![salida("INFERIDO", "Inspector")], true);
        assert!(filas.is_empty());
        let senales = senales_de_origen(&filas, excluidos, true);
        assert_eq!(senales["sin_responsable_declarado"], true);
        assert_eq!(senales["excluidos_no_declarados"], 1);
        assert!(
            senales["accion_sugerida"]
                .as_str()
                .unwrap()
                .contains("search_normative_context")
        );

        let todas = vec![salida("INFERIDO", "Inspector")];
        assert_eq!(
            senales_de_origen(&todas, 0, false)["sin_responsable_declarado"],
            true
        );
    }

    #[test]
    fn test_con_responsable_declarado_y_sin_excluidos_no_hay_accion() {
        let filas = vec![salida("DECLARADO", "Jefe")];
        let senales = senales_de_origen(&filas, 0, true);
        assert_eq!(senales["sin_responsable_declarado"], false);
        assert!(senales.get("accion_sugerida").is_none());
    }

    #[test]
    fn test_solo_declarados_es_el_valor_por_defecto() {
        let params: QueryOperationalMatrixParams =
            serde_json::from_value(json!({ "centro_id": "c" })).unwrap();
        assert!(params.solo_declarados);
        let params: QueryOperationalMatrixParams =
            serde_json::from_value(json!({ "centro_id": "c", "solo_declarados": false })).unwrap();
        assert!(!params.solo_declarados);
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
