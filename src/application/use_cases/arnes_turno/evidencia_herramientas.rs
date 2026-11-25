//! Qué campos de la salida de cada herramienta son evidencia documental (F-1a de la auditoría de
//! la v1.1.0). Las anclas de procedencia del arnés salen solo de aquí.
//!
//! Un campo es evidencia si su valor procede del contenido de un documento o de una fila de una
//! vista del centro. No lo es el eco de un argumento del modelo (`consulta`, `query_ejecutada`,
//! `filtros`, `entrada`), un contador o parámetro (`total`, `limite_filas`, `score`) ni un texto
//! que genera la app (`nota`, `aviso`, `mensaje`, `estado`).
//!
//! Es una lista de permitidos por herramienta y por ruta de clave: lo que no se declara no ancla,
//! y una herramienta que no se reconoce no aporta nada. Los nombres de clave nunca son evidencia.
//! La forma del JSON que ve el modelo no cambia: aquí solo se decide qué partes lee el arnés.

use serde_json::{Map, Value};

use crate::application::dtos::tools::{LakehouseToolName, ToolResult};

/// Niveles de anidamiento que se copian de un subárbol libre (`Regla::Todo`). Una fila de una
/// vista tiene dos o tres; más allá no es una fila y no se lee.
const PROFUNDIDAD_MAXIMA_DE_FILA: usize = 32;

/// Cómo se lee un valor de la salida de una herramienta
enum Regla {
    /// El valor entero es evidencia: un escalar, o las filas de una vista cuyas columnas elige
    /// la consulta
    Todo,
    /// Un objeto, o una lista de objetos, con cada clave clasificada. `ajenos` no ancla; una
    /// clave que no está en ninguna de las dos listas tampoco, y la delata el test de
    /// exhaustividad
    Campos {
        evidencia: &'static [(&'static str, Regla)],
        ajenos: &'static [&'static str],
    },
}

use Regla::{Campos, Todo};

/// Cita de un resultado: identificadores, revisión, sección y páginas leídos de las vistas
const CITA: Regla = Campos {
    evidencia: &[
        ("doc_id", Todo),
        ("titulo", Todo),
        ("revision", Todo),
        ("version_num", Todo),
        ("chunk_origen_id", Todo),
        ("breadcrumb", Todo),
        ("nodo_id", Todo),
        ("pagina_inicio", Todo),
        ("pagina_fin", Todo),
        ("paginas", Todo),
        ("fecha_vigor", Todo),
        ("tabla_id", Todo),
        ("celda_id", Todo),
        ("origen_responsable", Todo),
        ("edge_id", Todo),
    ],
    ajenos: &[],
};

/// Campos del primer nivel que llevan todas las herramientas. Con `fecha_referencia`, el servicio
/// añade las revisiones de `01` cuya vigencia no se puede afirmar y un aviso fijo
const COMUNES: Regla = Campos {
    evidencia: &[(
        "vigencia_no_declarada",
        Campos {
            evidencia: &[("doc_id", Todo), ("version_num", Todo), ("revision", Todo)],
            ajenos: &[],
        },
    )],
    ajenos: &["centro_id", "fecha_referencia", "aviso_vigencia"],
};

const CATALOGO: Regla = Campos {
    evidencia: &[
        (
            "documentos",
            Campos {
                evidencia: &[
                    ("doc_id", Todo),
                    ("titulo", Todo),
                    ("tipo_documento", Todo),
                    ("nivel_prelacion", Todo),
                    ("revision", Todo),
                    ("version_num", Todo),
                    ("fecha_vigor", Todo),
                    ("fecha_derogacion", Todo),
                    ("vigente", Todo),
                    ("doc_id_provisional", Todo),
                    ("paginas_degradadas", Todo),
                    ("estado_contenido", Todo),
                    ("muestra_texto", Todo),
                ],
                ajenos: &[
                    "vigencia_no_declarada",
                    "fecha_vigor_texto",
                    "provisional",
                    "aviso_contenido",
                    "escritura_dominante",
                ],
            },
        ),
        (
            "tablas_doble_entrada",
            Campos {
                evidencia: &[
                    ("tabla_id", Todo),
                    ("titulo_tabla", Todo),
                    ("doc_id", Todo),
                    ("revision", Todo),
                    ("version_num", Todo),
                    ("nivel_prelacion", Todo),
                    ("eje_x_nombre", Todo),
                    ("eje_x_tipo", Todo),
                    ("eje_x_unidad", Todo),
                    ("eje_y_nombre", Todo),
                    ("eje_y_tipo", Todo),
                    ("eje_y_unidad", Todo),
                    ("valor_salida_nombre", Todo),
                    ("instrucciones_uso_sql", Todo),
                ],
                ajenos: &[],
            },
        ),
    ],
    ajenos: &["vistas_sql", "nota"],
};

const MATRIZ: Regla = Campos {
    evidencia: &[(
        "filas",
        Campos {
            evidencia: &[
                ("hito_id", Todo),
                ("fase_operativa", Todo),
                ("orden_secuencia", Todo),
                ("objeto_alcance", Todo),
                ("rol_ejecutor", Todo),
                ("rol_responsable", Todo),
                ("origen_responsable", Todo),
                ("registro_codigo", Todo),
                ("registro_nombre", Todo),
                ("criterio_aceptacion", Todo),
                ("condicion_aplicacion", Todo),
                ("nivel_prelacion", Todo),
                ("calidad_extraccion", Todo),
                ("cita", CITA),
            ],
            ajenos: &["vigencia_no_declarada"],
        },
    )],
    ajenos: &[
        "filtros",
        "total",
        "nota",
        "solo_declarados",
        "excluidos_no_declarados",
        "sin_responsable_declarado",
        "accion_sugerida",
    ],
};

const EJE: Regla = Campos {
    evidencia: &[("nombre", Todo), ("unidad", Todo)],
    ajenos: &[],
};

/// Celda de una tabla de doble entrada. Con un único resultado, estos mismos campos van en el
/// primer nivel de la salida (`LOOKUP`): las dos listas deben coincidir
const CELDA: Regla = Campos {
    evidencia: &[
        ("valor_resultado_texto", Todo),
        ("valor_resultado_num", Todo),
        ("unidad_medida", Todo),
        ("nota_pie_condicion", Todo),
        ("rango_x", Todo),
        ("rango_y", Todo),
        ("eje_z", Todo),
        ("nivel_prelacion", Todo),
        ("cita", CITA),
    ],
    ajenos: &["nota_obligatoria", "aviso_nota", "prevalece"],
};

/// Un orden de los ejes en el aviso `ejes_ambiguos`: sus valores son el eco de la entrada
const LADO_DEL_AVISO: Regla = Campos {
    evidencia: &[("celdas", CELDA)],
    ajenos: &["valor_x", "valor_y"],
};

const LOOKUP: Regla = Campos {
    evidencia: &[
        (
            "tabla",
            Campos {
                evidencia: &[
                    ("tabla_id", Todo),
                    ("titulo_tabla", Todo),
                    ("eje_x", EJE),
                    ("eje_y", EJE),
                    ("valor_salida_nombre", Todo),
                ],
                ajenos: &[],
            },
        ),
        (
            "tablas",
            Campos {
                evidencia: &[("tabla_id", Todo), ("titulo_tabla", Todo), ("doc_id", Todo)],
                ajenos: &[],
            },
        ),
        ("rangos_disponibles", Todo),
        ("valor_resultado_texto", Todo),
        ("valor_resultado_num", Todo),
        ("unidad_medida", Todo),
        ("nota_pie_condicion", Todo),
        ("rango_x", Todo),
        ("rango_y", Todo),
        ("eje_z", Todo),
        ("nivel_prelacion", Todo),
        ("cita", CITA),
        ("candidatas", CELDA),
        (
            "ejes_ambiguos",
            Campos {
                evidencia: &[
                    ("eje_x", EJE),
                    ("eje_y", EJE),
                    ("orden_recibido", LADO_DEL_AVISO),
                    ("orden_invertido", LADO_DEL_AVISO),
                ],
                ajenos: &["mensaje"],
            },
        ),
    ],
    ajenos: &[
        "entrada",
        "estado",
        "mensaje",
        "nota_obligatoria",
        "aviso_nota",
        "orden_invertido_resolveria",
    ],
};

/// Las columnas de `resultado` las elige la consulta: se lee la fila entera. Una columna calculada
/// o literal (`SELECT 437`) sigue anclando: es la parte 1b de F-1
const SQL: Regla = Campos {
    evidencia: &[("resultado", Todo)],
    ajenos: &[
        "filas",
        "truncado",
        "limite_filas",
        "query_ejecutada",
        "aviso",
    ],
};

const GRAFO: Regla = Campos {
    evidencia: &[
        ("sugerencias", Todo),
        (
            "nodos",
            Campos {
                evidencia: &[
                    ("id", Todo),
                    ("nivel_prelacion", Todo),
                    (
                        "resumen_hub",
                        Campos {
                            evidencia: &[(
                                "top_relaciones",
                                Campos {
                                    evidencia: &[("tipo_relacion", Todo)],
                                    ajenos: &["aristas"],
                                },
                            )],
                            ajenos: &["grado"],
                        },
                    ),
                ],
                ajenos: &["salto", "grado", "hub_resumido", "vecinos_omitidos"],
            },
        ),
        (
            "aristas",
            Campos {
                evidencia: &[
                    ("origen", Todo),
                    ("tipo_relacion", Todo),
                    ("destino", Todo),
                    ("contexto", Todo),
                    ("cita", CITA),
                ],
                ajenos: &[],
            },
        ),
    ],
    ajenos: &["nodo_origen", "limites", "estado", "mensaje", "truncado"],
};

const BUSQUEDA: Regla = Campos {
    evidencia: &[
        (
            "grupos",
            Campos {
                evidencia: &[
                    ("tema", Todo),
                    (
                        "resultados",
                        Campos {
                            evidencia: &[
                                ("id", Todo),
                                ("texto", Todo),
                                ("breadcrumb_jerarquia", Todo),
                                ("condicion_aplicacion", Todo),
                                ("nivel_prelacion", Todo),
                                ("tipo_bloque", Todo),
                                ("estado_contenido", Todo),
                                ("cita", CITA),
                            ],
                            ajenos: &[
                                "origen",
                                "prevalece",
                                "supletorio",
                                "score",
                                "tiene_capa_2",
                                "provisional",
                                "vigencia_no_declarada",
                                "aviso_contenido",
                            ],
                        },
                    ),
                ],
                ajenos: &["conflicto_prelacion"],
            },
        ),
        (
            "documentos_no_definitivos",
            Campos {
                evidencia: &[
                    ("doc_id", Todo),
                    ("titulo", Todo),
                    ("revision", Todo),
                    ("estado_contenido", Todo),
                ],
                ajenos: &["explicacion"],
            },
        ),
    ],
    ajenos: &[
        "consulta",
        "modo",
        "total",
        "aviso",
        "nota",
        "nota_seguridad",
    ],
};

const CONDICIONES_HEREDADAS: Regla = Campos {
    evidencia: &[
        ("nodo_id", Todo),
        ("titulo_seccion", Todo),
        ("condicion", Todo),
    ],
    ajenos: &[],
};

const INDICE: Regla = Campos {
    evidencia: &[
        ("doc_id", Todo),
        ("titulo", Todo),
        ("revision", Todo),
        ("version_num", Todo),
        ("estado_contenido", Todo),
        (
            "nodos",
            Campos {
                evidencia: &[
                    ("nodo_id", Todo),
                    ("codigo_seccion", Todo),
                    ("titulo_seccion", Todo),
                    ("nivel_jerarquico", Todo),
                    ("ruta_linaje", Todo),
                    ("tipo_nodo", Todo),
                    ("resumen_seccion", Todo),
                    ("condiciones_alcance", Todo),
                ],
                ajenos: &["tiene_hijos", "total_hijos"],
            },
        ),
        ("cita", CITA),
    ],
    ajenos: &[
        "alias_resuelto_de",
        "provisional",
        "revision_vigencia_no_declarada",
        "aviso_contenido",
        "nodo_padre_id",
        "total",
        "total_nodos_documento",
        "truncado",
        "siguiente_desplazamiento",
        "nota",
        "aviso_nodos_sinteticos",
        "nota_seguridad",
    ],
};

const SUBARBOL: Regla = Campos {
    evidencia: &[
        ("doc_id", Todo),
        ("titulo", Todo),
        ("revision", Todo),
        ("version_num", Todo),
        ("estado_contenido", Todo),
        (
            "nodo",
            Campos {
                evidencia: &[
                    ("nodo_id", Todo),
                    ("codigo_seccion", Todo),
                    ("titulo_seccion", Todo),
                    ("ruta_linaje", Todo),
                    ("tipo_nodo", Todo),
                    ("condiciones_alcance", Todo),
                ],
                ajenos: &[],
            },
        ),
        ("condiciones_heredadas", CONDICIONES_HEREDADAS),
        (
            "secciones",
            Campos {
                evidencia: &[
                    ("nodo_id", Todo),
                    ("codigo_seccion", Todo),
                    ("titulo_seccion", Todo),
                    ("nivel_jerarquico", Todo),
                    ("ruta_linaje", Todo),
                    ("tipo_nodo", Todo),
                    ("tipo_bloque", Todo),
                    ("condicion_aplicacion", Todo),
                    ("condiciones_heredadas", CONDICIONES_HEREDADAS),
                    ("chunks", Todo),
                    ("texto", Todo),
                    ("cita", CITA),
                ],
                ajenos: &["texto_recortado"],
            },
        ),
        ("cita", CITA),
    ],
    ajenos: &[
        "alias_resuelto_de",
        "provisional",
        "revision_vigencia_no_declarada",
        "aviso_contenido",
        "total_secciones",
        "caracteres_totales",
        "caracteres_entregados",
        "truncado",
        "secciones_omitidas",
        "como_continuar",
        "nota",
        "nota_seguridad",
    ],
};

fn regla_de(herramienta: LakehouseToolName) -> &'static Regla {
    match herramienta {
        LakehouseToolName::InspectLakehouseCatalog => &CATALOGO,
        LakehouseToolName::QueryOperationalMatrix => &MATRIZ,
        LakehouseToolName::LookupDoubleEntryTable => &LOOKUP,
        LakehouseToolName::ExecuteAthenaSql => &SQL,
        LakehouseToolName::TraverseNormativeGraph => &GRAFO,
        LakehouseToolName::SearchNormativeContext => &BUSQUEDA,
        LakehouseToolName::StairNavigateToc => &INDICE,
        LakehouseToolName::StairGetSubtreeContext => &SUBARBOL,
    }
}

/// Nombre exacto: un nombre parecido (`Lookup_…`, con espacios) no es la herramienta
fn herramienta_de(nombre: &str) -> Option<LakehouseToolName> {
    LakehouseToolName::TODAS
        .into_iter()
        .find(|herramienta| herramienta.as_str() == nombre)
}

/// La salida de una herramienta reducida a sus campos de evidencia, con la misma forma. Un error
/// (puede repetir los argumentos del modelo) o una herramienta que no se reconoce dan `Null`
pub fn evidencia_de(resultado: &ToolResult) -> Value {
    if resultado.is_error {
        return Value::Null;
    }
    let Some(herramienta) = herramienta_de(&resultado.tool_name) else {
        return Value::Null;
    };
    let propia = podar(regla_de(herramienta), &resultado.output);
    match (propia, podar(&COMUNES, &resultado.output)) {
        (Some(Value::Object(mut campos)), Some(Value::Object(comunes))) => {
            campos.extend(comunes);
            Value::Object(campos)
        }
        (propia, _) => propia.unwrap_or(Value::Null),
    }
}

fn podar(regla: &Regla, valor: &Value) -> Option<Value> {
    match (regla, valor) {
        (_, Value::Null) => None,
        (Todo, _) => copiar_acotado(valor, PROFUNDIDAD_MAXIMA_DE_FILA),
        (Campos { .. }, Value::Array(_)) => Some(Value::Array(
            objetos_de_la_lista(valor)
                .into_iter()
                .filter_map(|objeto| podar(regla, objeto))
                .collect(),
        )),
        (Campos { evidencia, .. }, Value::Object(campos)) => {
            let podados: Map<String, Value> = evidencia
                .iter()
                .filter_map(|(clave, regla_del_campo)| {
                    let podado = podar(regla_del_campo, campos.get(*clave)?)?;
                    Some((clave.to_string(), podado))
                })
                .collect();
            Some(Value::Object(podados))
        }
        // Donde se espera un objeto, un escalar no tiene clave que lo clasifique
        (Campos { .. }, _) => None,
    }
}

/// Los objetos de una lista, en orden, también si vienen en listas dentro de listas. Con pila
/// propia: una salida muy anidada no agota la pila del hilo
fn objetos_de_la_lista(lista: &Value) -> Vec<&Value> {
    let mut objetos = Vec::new();
    let mut pendientes = vec![lista];
    while let Some(valor) = pendientes.pop() {
        match valor {
            Value::Array(elementos) => pendientes.extend(elementos.iter().rev()),
            Value::Object(_) => objetos.push(valor),
            _ => {}
        }
    }
    objetos
}

/// Copia de un subárbol libre hasta `niveles` de anidamiento; lo más profundo no se lee
fn copiar_acotado(valor: &Value, niveles: usize) -> Option<Value> {
    match valor {
        Value::Array(elementos) => (niveles > 0).then(|| {
            Value::Array(
                elementos
                    .iter()
                    .filter_map(|elemento| copiar_acotado(elemento, niveles - 1))
                    .collect(),
            )
        }),
        Value::Object(campos) => (niveles > 0).then(|| {
            Value::Object(
                campos
                    .iter()
                    .filter_map(|(clave, contenido)| {
                        Some((clave.clone(), copiar_acotado(contenido, niveles - 1)?))
                    })
                    .collect(),
            )
        }),
        escalar => Some(escalar.clone()),
    }
}

/// Un valor escalar de la evidencia
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Escalar {
    Texto(String),
    /// Un número, tal como lo escribe el JSON
    Numero(String),
}

impl Escalar {
    pub(super) fn como_texto(&self) -> &str {
        match self {
            Self::Texto(texto) | Self::Numero(texto) => texto,
        }
    }
}

/// Textos y números de una evidencia ya podada, en orden de documento. Nunca los nombres de
/// clave: los fija el esquema o, en una fila SQL, el alias que escribió el modelo
pub(super) fn escalares_de(evidencia: &Value) -> Vec<Escalar> {
    let mut escalares = Vec::new();
    let mut pendientes = vec![evidencia];
    while let Some(valor) = pendientes.pop() {
        match valor {
            Value::String(texto) => escalares.push(Escalar::Texto(texto.clone())),
            Value::Number(numero) => escalares.push(Escalar::Numero(numero.to_string())),
            Value::Array(elementos) => pendientes.extend(elementos.iter().rev()),
            Value::Object(campos) => pendientes.extend(campos.values().rev()),
            Value::Null | Value::Bool(_) => {}
        }
    }
    escalares
}

/// Rutas de clave de una salida que ninguna lista clasifica (`grupos[].resultados[].nueva`), o
/// que no tienen la forma declarada. Vacío si la herramienta no añade campos sin decidir si son
/// evidencia: es lo que comprueba el test de exhaustividad con las salidas reales
pub fn claves_sin_clasificar(herramienta: LakehouseToolName, salida: &Value) -> Vec<String> {
    let mut sin_clasificar = Vec::new();
    anotar_sin_clasificar(regla_de(herramienta), salida, "", &mut sin_clasificar);
    // Una clave del primer nivel puede estar clasificada en los campos comunes
    if let (Value::Object(campos), Campos { evidencia, ajenos }) = (salida, &COMUNES) {
        sin_clasificar.retain(|ruta| {
            !ajenos.contains(&ruta.as_str()) && !evidencia.iter().any(|(clave, _)| clave == ruta)
        });
        for (clave, regla_del_campo) in *evidencia {
            if let Some(contenido) = campos.get(*clave) {
                anotar_sin_clasificar(regla_del_campo, contenido, clave, &mut sin_clasificar);
            }
        }
    }
    sin_clasificar.sort();
    sin_clasificar.dedup();
    sin_clasificar
}

fn anotar_sin_clasificar(regla: &Regla, valor: &Value, ruta: &str, hallazgos: &mut Vec<String>) {
    let Campos { evidencia, ajenos } = regla else {
        return;
    };
    match valor {
        Value::Null => {}
        Value::Array(_) => {
            let ruta = format!("{ruta}[]");
            for objeto in objetos_de_la_lista(valor) {
                anotar_sin_clasificar(regla, objeto, &ruta, hallazgos);
            }
            if tiene_escalares_sueltos(valor) {
                hallazgos.push(format!("{ruta} (escalar donde se declara un objeto)"));
            }
        }
        Value::Object(campos) => {
            for (clave, contenido) in campos {
                let ruta_del_campo = if ruta.is_empty() {
                    clave.clone()
                } else {
                    format!("{ruta}.{clave}")
                };
                match evidencia.iter().find(|(declarada, _)| declarada == clave) {
                    Some((_, regla_del_campo)) => anotar_sin_clasificar(
                        regla_del_campo,
                        contenido,
                        &ruta_del_campo,
                        hallazgos,
                    ),
                    None if ajenos.contains(&clave.as_str()) => {}
                    None => hallazgos.push(ruta_del_campo),
                }
            }
        }
        _ => hallazgos.push(format!("{ruta} (escalar donde se declara un objeto)")),
    }
}

fn tiene_escalares_sueltos(lista: &Value) -> bool {
    let mut pendientes = vec![lista];
    while let Some(valor) = pendientes.pop() {
        match valor {
            Value::Array(elementos) => pendientes.extend(elementos),
            Value::Object(_) | Value::Null => {}
            _ => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::*;

    /// Cifra que no aparece en ningún otro campo de las salidas de prueba
    const SENUELO: i64 = 43_719;

    fn resultado(herramienta: &str, salida: Value) -> ToolResult {
        ToolResult {
            call_id: "c".into(),
            tool_name: herramienta.into(),
            output: salida,
            is_error: false,
        }
    }

    fn contiene_el_senuelo(herramienta: LakehouseToolName, salida: Value) -> bool {
        let evidencia = evidencia_de(&resultado(herramienta.as_str(), salida));
        escalares_de(&evidencia)
            .iter()
            .any(|escalar| escalar.como_texto().contains(&SENUELO.to_string()))
    }

    /// Escribe `valor` en una ruta de claves; cada objeto intermedio se crea, y un tramo acabado
    /// en `[]` es una lista de un elemento
    fn con(ruta: &str, valor: Value) -> Value {
        ruta.rsplit('.')
            .fold(valor, |interior, tramo| match tramo.strip_suffix("[]") {
                Some(clave) => json!({ clave: [interior] }),
                None => json!({ tramo: interior }),
            })
    }

    /// Por herramienta: (rutas de eco, metadato o texto de la app), (rutas de evidencia)
    fn casos() -> Vec<(LakehouseToolName, Vec<&'static str>, Vec<&'static str>)> {
        vec![
            (
                LakehouseToolName::InspectLakehouseCatalog,
                vec![
                    "centro_id",
                    "fecha_referencia",
                    "vistas_sql",
                    "nota",
                    "documentos[].aviso_contenido",
                    "documentos[].fecha_vigor_texto",
                ],
                vec![
                    "documentos[].titulo",
                    "documentos[].muestra_texto",
                    "documentos[].version_num",
                    "tablas_doble_entrada[].titulo_tabla",
                ],
            ),
            (
                LakehouseToolName::QueryOperationalMatrix,
                vec![
                    "filtros.objeto",
                    "filtros.registro",
                    "total",
                    "excluidos_no_declarados",
                    "accion_sugerida",
                    "nota",
                ],
                vec![
                    "filas[].criterio_aceptacion",
                    "filas[].condicion_aplicacion",
                    "filas[].registro_codigo",
                    "filas[].cita.revision",
                ],
            ),
            (
                LakehouseToolName::LookupDoubleEntryTable,
                vec![
                    "entrada.valor_x",
                    "entrada.valor_y",
                    "entrada.eje_z",
                    "estado",
                    "mensaje",
                    "aviso_nota",
                    "orden_invertido_resolveria.valor_x",
                    "orden_invertido_resolveria.mensaje",
                    "ejes_ambiguos.mensaje",
                    "ejes_ambiguos.orden_recibido.valor_x",
                    "ejes_ambiguos.orden_invertido.valor_y",
                    "candidatas[].aviso_nota",
                ],
                vec![
                    "valor_resultado_texto",
                    "valor_resultado_num",
                    "nota_pie_condicion",
                    "rango_x",
                    "cita.paginas",
                    "tabla.titulo_tabla",
                    "tabla.eje_x.nombre",
                    "tablas[].titulo_tabla",
                    "rangos_disponibles",
                    "candidatas[].valor_resultado_texto",
                    "ejes_ambiguos.orden_invertido.celdas[].valor_resultado_texto",
                ],
            ),
            (
                LakehouseToolName::ExecuteAthenaSql,
                vec![
                    "query_ejecutada",
                    "filas",
                    "limite_filas",
                    "aviso",
                    "fecha_referencia",
                ],
                vec!["resultado[].valor", "resultado[].texto_contenido"],
            ),
            (
                LakehouseToolName::TraverseNormativeGraph,
                vec![
                    "nodo_origen",
                    "limites.max_vecinos",
                    "limites.max_nodos",
                    "mensaje",
                    "nodos[].grado",
                    "nodos[].salto",
                    "nodos[].vecinos_omitidos",
                    "nodos[].resumen_hub.grado",
                    "nodos[].resumen_hub.top_relaciones[].aristas",
                ],
                vec![
                    "nodos[].id",
                    "aristas[].destino",
                    "aristas[].contexto",
                    "aristas[].cita.revision",
                    "sugerencias",
                ],
            ),
            (
                LakehouseToolName::SearchNormativeContext,
                vec![
                    "consulta",
                    "modo",
                    "total",
                    "aviso",
                    "nota",
                    "nota_seguridad",
                    "grupos[].resultados[].score",
                    "grupos[].resultados[].aviso_contenido",
                    "documentos_no_definitivos[].explicacion",
                ],
                vec![
                    "grupos[].resultados[].texto",
                    "grupos[].resultados[].breadcrumb_jerarquia",
                    "grupos[].resultados[].condicion_aplicacion",
                    "grupos[].resultados[].cita.paginas",
                    "grupos[].tema",
                    "documentos_no_definitivos[].titulo",
                ],
            ),
            (
                LakehouseToolName::StairNavigateToc,
                vec![
                    "nodo_padre_id",
                    "alias_resuelto_de",
                    "total",
                    "total_nodos_documento",
                    "siguiente_desplazamiento",
                    "nodos[].total_hijos",
                    "aviso_contenido",
                    "nota",
                ],
                vec![
                    "titulo",
                    "nodos[].titulo_seccion",
                    "nodos[].resumen_seccion",
                    "nodos[].codigo_seccion",
                    "cita.revision",
                ],
            ),
            (
                LakehouseToolName::StairGetSubtreeContext,
                vec![
                    "alias_resuelto_de",
                    "total_secciones",
                    "caracteres_totales",
                    "caracteres_entregados",
                    "secciones_omitidas",
                    "como_continuar.siguiente_desde_caracter",
                    "como_continuar.indicacion",
                    "nota",
                ],
                vec![
                    "secciones[].texto",
                    "secciones[].titulo_seccion",
                    "secciones[].condiciones_heredadas[].condicion",
                    "secciones[].cita.paginas",
                    "nodo.titulo_seccion",
                    "condiciones_heredadas[].condicion",
                ],
            ),
        ]
    }

    #[test]
    fn test_los_campos_comunes_valen_para_todas_las_herramientas() {
        for herramienta in LakehouseToolName::TODAS {
            for ruta in ["centro_id", "fecha_referencia", "aviso_vigencia"] {
                assert!(
                    !contiene_el_senuelo(herramienta, con(ruta, json!(format!("eco {SENUELO}")))),
                    "{herramienta:?} · {ruta}"
                );
                assert!(claves_sin_clasificar(herramienta, &con(ruta, json!("x"))).is_empty());
            }
            let revisiones = con(
                "vigencia_no_declarada[].revision",
                json!(format!("Rev. {SENUELO}")),
            );
            assert!(
                contiene_el_senuelo(herramienta, revisiones.clone()),
                "{herramienta:?}"
            );
            assert!(claves_sin_clasificar(herramienta, &revisiones).is_empty());
            assert_eq!(
                claves_sin_clasificar(herramienta, &con("vigencia_no_declarada[].nueva", json!(1))),
                vec!["vigencia_no_declarada[].nueva".to_string()]
            );
        }
    }

    #[test]
    fn test_cada_herramienta_tiene_casos_de_eco_y_de_evidencia() {
        let cubiertas: HashSet<LakehouseToolName> = casos().iter().map(|caso| caso.0).collect();
        assert_eq!(cubiertas.len(), LakehouseToolName::TODAS.len());
    }

    #[test]
    fn test_una_cifra_solo_en_un_eco_un_contador_o_un_texto_de_la_app_no_es_evidencia() {
        for (herramienta, ajenas, _) in casos() {
            for ruta in ajenas {
                for valor in [json!(SENUELO), json!(format!("texto con {SENUELO} dentro"))] {
                    assert!(
                        !contiene_el_senuelo(herramienta, con(ruta, valor)),
                        "{herramienta:?} · {ruta}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_la_misma_cifra_en_un_campo_de_evidencia_si_lo_es() {
        for (herramienta, _, evidencias) in casos() {
            for ruta in evidencias {
                for valor in [json!(SENUELO), json!(format!("texto con {SENUELO} dentro"))] {
                    assert!(
                        contiene_el_senuelo(herramienta, con(ruta, valor)),
                        "{herramienta:?} · {ruta}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_las_rutas_de_los_casos_estan_clasificadas() {
        for (herramienta, ajenas, evidencias) in casos() {
            for ruta in ajenas.into_iter().chain(evidencias) {
                assert_eq!(
                    claves_sin_clasificar(herramienta, &con(ruta, json!("x"))),
                    Vec::<String>::new(),
                    "{herramienta:?} · {ruta}"
                );
            }
        }
    }

    #[test]
    fn test_los_nombres_de_clave_nunca_son_evidencia() {
        // El alias de una columna lo escribe el modelo: `SELECT x AS total_43719`
        let alias = format!("total_{SENUELO}");
        assert!(!contiene_el_senuelo(
            LakehouseToolName::ExecuteAthenaSql,
            json!({ "resultado": [{ alias: "sin cifras" }] }),
        ));
        // Una clave que no existe en el esquema, con la cifra en el nombre y en el valor
        let inventada = format!("campo_{SENUELO}");
        for herramienta in LakehouseToolName::TODAS {
            assert!(
                !contiene_el_senuelo(herramienta, json!({ inventada.clone(): SENUELO })),
                "{herramienta:?}"
            );
        }
    }

    #[test]
    fn test_una_clave_sin_declarar_no_ancla_y_el_inventario_la_delata() {
        let salida = json!({
            "grupos": [{ "resultados": [{ "texto": "texto", "campo_nuevo": SENUELO }] }],
            "otro_nuevo": SENUELO,
        });
        assert!(!contiene_el_senuelo(
            LakehouseToolName::SearchNormativeContext,
            salida.clone()
        ));
        assert_eq!(
            claves_sin_clasificar(LakehouseToolName::SearchNormativeContext, &salida),
            vec![
                "grupos[].resultados[].campo_nuevo".to_string(),
                "otro_nuevo".to_string()
            ]
        );
    }

    #[test]
    fn test_herramienta_desconocida_o_con_error_no_aporta_evidencia() {
        let salida = json!({ "resultado": [{ "valor": SENUELO }], "texto": SENUELO });
        for nombre in [
            "tool_inventada",
            "Execute_Athena_Sql",
            "execute_athena_sql ",
            "",
        ] {
            assert_eq!(
                evidencia_de(&resultado(nombre, salida.clone())),
                Value::Null,
                "{nombre:?}"
            );
        }
        let mut con_error = resultado("execute_athena_sql", salida);
        con_error.is_error = true;
        assert_eq!(evidencia_de(&con_error), Value::Null);
    }

    #[test]
    fn test_la_evidencia_conserva_la_forma_de_la_salida() {
        let salida = json!({
            "consulta": "eco 7",
            "total": 1,
            "grupos": [{
                "tema": "TEMA",
                "conflicto_prelacion": false,
                "resultados": [{
                    "texto": "cuerpo 12",
                    "score": 0.5,
                    "cita": { "doc_id": "DOC-1", "paginas": "3-4" },
                }],
            }],
            "nota": "nota de la app",
        });
        assert_eq!(
            evidencia_de(&resultado("search_normative_context", salida)),
            json!({ "grupos": [{
                "tema": "TEMA",
                "resultados": [{ "texto": "cuerpo 12", "cita": { "doc_id": "DOC-1", "paginas": "3-4" } }],
            }] })
        );
    }

    #[test]
    fn test_un_escalar_donde_se_declara_un_objeto_no_es_evidencia() {
        let salida = json!({ "grupos": [SENUELO, "suelto"], "documentos_no_definitivos": SENUELO });
        assert!(!contiene_el_senuelo(
            LakehouseToolName::SearchNormativeContext,
            salida.clone()
        ));
        assert_eq!(
            claves_sin_clasificar(LakehouseToolName::SearchNormativeContext, &salida).len(),
            2
        );
    }

    #[test]
    fn test_los_escalares_salen_en_orden_de_documento_y_sin_claves() {
        let evidencia =
            json!({ "a": "uno", "b": [2, { "c": "tres", "d": null, "e": true }], "f": 4.5 });
        let textos: Vec<String> = escalares_de(&evidencia)
            .iter()
            .map(|e| e.como_texto().to_string())
            .collect();
        assert_eq!(textos, vec!["uno", "2", "tres", "4.5"]);
    }

    #[test]
    fn test_salida_muy_anidada_no_agota_la_pila_y_lo_demasiado_profundo_no_se_lee() {
        let mut listas = json!({ "valor_resultado_texto": SENUELO.to_string() });
        let mut filas = json!(SENUELO);
        for _ in 0..50_000 {
            listas = Value::Array(vec![listas]);
            filas = Value::Array(vec![filas]);
        }
        // Sin `json!`: interpolar un valor lo serializa con recursión
        let objeto =
            |clave: &str, valor: Value| Value::Object(Map::from_iter([(clave.to_string(), valor)]));
        let lookup = resultado("lookup_double_entry_table", objeto("candidatas", listas));
        let sql = resultado("execute_athena_sql", objeto("resultado", filas));
        let leidos = |resultado: &ToolResult| {
            escalares_de(&evidencia_de(resultado))
                .iter()
                .any(|escalar| escalar.como_texto() == SENUELO.to_string())
        };
        assert!(leidos(&lookup));
        assert!(!leidos(&sql));
        // El destructor recursivo de `Value` no es parte del módulo
        std::mem::forget((lookup, sql));
    }
}
