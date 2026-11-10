//! Apartado del documento donde está una tabla de Capa 2 y página de origen de cada celda
//! (`docs/arquitectura/ssd_capa2_paginas_contexto.md`). Sin cambiar el esquema: se deriva de
//! `tablas_vinculadas` y `saltos_pagina` de `02_chunks_normativa` (esquema v6).
//!
//! La ingesta vincula la tabla al chunk de su sección y a los chunks que la citan («Tabla 4-A»),
//! así que entre las secciones vinculadas se elige la que contiene más valores de sus celdas:
//! es donde están sus filas. Una celda tiene página solo si su valor aparece en líneas de una
//! única página de ese apartado; si no, se da el rango del apartado y nunca una página supuesta.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::estructura::seccion_real_de;
use super::texto;
use crate::domain::services::arbol_secciones::comparar_natural;
use crate::domain::services::fragmentacion::reconstruir_con_saltos;
use crate::domain::services::marcas_pagina::{SaltoPagina, pagina_en, saltos_validos};
use crate::domain::services::normalizacion::plegar_mayusculas;

/// Columnas de 02 que necesita la localización
pub(super) const COLUMNAS_CHUNK_APARTADO: &str =
    "c.chunk_id, c.nodo_id, c.texto_contenido, c.saltos_pagina, c.tablas_vinculadas";

/// Un chunk de la revisión de la tabla
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChunkApartado {
    pub chunk_id: String,
    pub nodo_id: String,
    pub texto: String,
    pub saltos: Option<Vec<SaltoPagina>>,
    pub tablas_vinculadas: Vec<String>,
}

impl ChunkApartado {
    /// Fila de 02 leída por SQL (`saltos_pagina` y `tablas_vinculadas` llegan como listas JSON)
    pub(super) fn desde_fila(fila: &Map<String, Value>) -> Self {
        Self {
            chunk_id: texto(fila, "chunk_id"),
            nodo_id: texto(fila, "nodo_id"),
            texto: texto(fila, "texto_contenido"),
            saltos: saltos_de_valor(fila.get("saltos_pagina")),
            tablas_vinculadas: fila
                .get("tablas_vinculadas")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
        }
    }
}

fn saltos_de_valor(valor: Option<&Value>) -> Option<Vec<SaltoPagina>> {
    valor?
        .as_array()?
        .iter()
        .map(|salto| {
            let entero = |campo: &str| salto.get(campo).and_then(Value::as_u64);
            Some(SaltoPagina {
                desplazamiento: usize::try_from(entero("desplazamiento")?).ok()?,
                pagina: u32::try_from(entero("pagina")?).ok()?,
            })
        })
        .collect()
}

/// Texto íntegro de la sección real que contiene la tabla, con sus saltos de página
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ApartadoTabla {
    /// Sección real (un sub-nodo por páginas cuenta como su sección)
    pub nodo_id: String,
    pub texto: String,
    /// `None` si algún chunk no tiene saltos válidos (Markdown, PDF sin saltos)
    pub saltos: Option<Vec<SaltoPagina>>,
}

/// Una línea del apartado y su página
pub(super) struct LineaApartado<'a> {
    pub texto: &'a str,
    pub pagina: Option<u32>,
}

impl ApartadoTabla {
    /// Sección vinculada a `tabla_id` que contiene más valores de la tabla. Sin ningún valor,
    /// solo se acepta si la tabla está vinculada a una única sección
    pub(super) fn localizar(
        chunks: &[ChunkApartado],
        tabla_id: &str,
        valores: &[String],
    ) -> Option<Self> {
        let vinculadas: BTreeSet<&str> = chunks
            .iter()
            .filter(|c| c.tablas_vinculadas.iter().any(|t| t == tabla_id))
            .map(|c| seccion_real_de(&c.nodo_id))
            .collect();
        let candidatos: Vec<Self> = vinculadas
            .iter()
            .map(|seccion| Self::reconstruir(chunks, seccion))
            .collect();
        let secuencias: Vec<Vec<String>> = valores
            .iter()
            .map(|v| palabras(v))
            .filter(|p| !p.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let puntuados: Vec<(usize, Self)> = candidatos
            .into_iter()
            .map(|apartado| (apartado.valores_presentes(&secuencias), apartado))
            .collect();
        let maximo = puntuados.iter().map(|(p, _)| *p).max()?;
        if maximo == 0 && puntuados.len() > 1 {
            return None;
        }
        puntuados
            .into_iter()
            .find(|(puntos, _)| *puntos == maximo)
            .map(|(_, apartado)| apartado)
    }

    fn reconstruir(chunks: &[ChunkApartado], seccion: &str) -> Self {
        let mut propios: Vec<&ChunkApartado> = chunks
            .iter()
            .filter(|c| seccion_real_de(&c.nodo_id) == seccion)
            .collect();
        propios.sort_by(|a, b| comparar_natural(&a.chunk_id, &b.chunk_id));
        let ventanas: Vec<(&str, Option<&[SaltoPagina]>)> = propios
            .iter()
            .map(|c| (c.texto.as_str(), c.saltos.as_deref()))
            .collect();
        let (texto, saltos) = reconstruir_con_saltos(&ventanas);
        let saltos = saltos.filter(|s| saltos_validos(&texto, s));
        Self {
            nodo_id: seccion.to_string(),
            texto,
            saltos,
        }
    }

    fn valores_presentes(&self, secuencias: &[Vec<String>]) -> usize {
        let lineas: Vec<Vec<String>> = self.lineas().iter().map(|l| palabras(l.texto)).collect();
        secuencias
            .iter()
            .filter(|valor| lineas.iter().any(|linea| contiene_secuencia(linea, valor)))
            .count()
    }

    /// Líneas no vacías con la página de su primer carácter
    pub(super) fn lineas(&self) -> Vec<LineaApartado<'_>> {
        let mut lineas = Vec::new();
        let mut inicio = 0;
        for linea in self.texto.split_inclusive('\n') {
            let limpia = linea.trim();
            if !limpia.is_empty() {
                let desplazamiento = inicio + (linea.len() - linea.trim_start().len());
                lineas.push(LineaApartado {
                    texto: limpia,
                    pagina: self
                        .saltos
                        .as_deref()
                        .and_then(|s| pagina_en(s, desplazamiento)),
                });
            }
            inicio += linea.len();
        }
        lineas
    }

    /// Primera y última página del apartado, si tiene saltos
    pub(super) fn paginas(&self) -> Option<(u32, u32)> {
        let saltos = self.saltos.as_deref()?;
        let primera = saltos.iter().map(|s| s.pagina).min()?;
        let ultima = saltos.iter().map(|s| s.pagina).max()?;
        Some((primera, ultima))
    }

    /// Página del valor de una celda: la única página de las líneas que lo contienen. Si alguna
    /// de esas líneas lleva también una etiqueta categórica de la celda (fila o columna), solo
    /// cuentan esas. Con varias páginas posibles o sin páginas, `None`
    pub(super) fn pagina_de_valor(&self, valor: &str, etiquetas: &[String]) -> Option<u32> {
        let buscado = palabras(valor);
        if buscado.is_empty() {
            return None;
        }
        let etiquetas: Vec<Vec<String>> = etiquetas
            .iter()
            .map(|e| palabras(e))
            .filter(|e| !e.is_empty())
            .collect();
        let con_valor: Vec<(Vec<String>, Option<u32>)> = self
            .lineas()
            .into_iter()
            .map(|l| (palabras(l.texto), l.pagina))
            .filter(|(p, _)| contiene_secuencia(p, &buscado))
            .collect();
        let con_etiqueta: Vec<&(Vec<String>, Option<u32>)> = con_valor
            .iter()
            .filter(|(p, _)| etiquetas.iter().any(|e| contiene_secuencia(p, e)))
            .collect();
        let elegidas: Vec<&(Vec<String>, Option<u32>)> = if con_etiqueta.is_empty() {
            con_valor.iter().collect()
        } else {
            con_etiqueta
        };
        let paginas: BTreeSet<Option<u32>> = elegidas.iter().map(|(_, p)| *p).collect();
        match paginas.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(unica)] => Some(*unica),
            _ => None,
        }
    }
}

/// Apartados de las revisiones que intervienen en un cruce, por `(doc_id, version_num)`
#[derive(Debug, Default)]
pub(super) struct ApartadosDeTabla {
    pub por_revision: BTreeMap<(String, i64), ApartadoTabla>,
}

impl ApartadosDeTabla {
    pub(super) fn de_celda(&self, celda: &Map<String, Value>) -> Option<&ApartadoTabla> {
        let clave = (
            texto(celda, "doc_id"),
            super::entero(celda, "version_num").unwrap_or_default(),
        );
        self.por_revision.get(&clave)
    }

    /// Página de origen de una celda, si se puede determinar sin suponer
    pub(super) fn pagina_de_celda(&self, celda: &Map<String, Value>) -> Option<u32> {
        let etiquetas: Vec<String> = ["eje_x_categoria", "eje_y_categoria"]
            .iter()
            .filter_map(|columna| super::texto_opcional(celda, columna))
            .collect();
        self.de_celda(celda)?
            .pagina_de_valor(&texto(celda, "valor_resultado_texto"), &etiquetas)
    }
}

/// Palabras de un texto en mayúsculas y sin tildes: letras y dígitos (`0,42` son dos palabras)
pub(super) fn palabras(texto: &str) -> Vec<String> {
    plegar_mayusculas(texto)
        .split(|letra: char| !letra.is_alphanumeric())
        .filter(|palabra| !palabra.is_empty())
        .map(str::to_string)
        .collect()
}

/// `linea` contiene `buscada` como palabras consecutivas completas
pub(super) fn contiene_secuencia(linea: &[String], buscada: &[String]) -> bool {
    !buscada.is_empty()
        && linea
            .windows(buscada.len())
            .any(|ventana| ventana == buscada)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salto(desplazamiento: usize, pagina: u32) -> SaltoPagina {
        SaltoPagina {
            desplazamiento,
            pagina,
        }
    }

    fn chunk(id: &str, nodo: &str, texto: &str, saltos: Option<Vec<SaltoPagina>>) -> ChunkApartado {
        ChunkApartado {
            chunk_id: id.into(),
            nodo_id: nodo.into(),
            texto: texto.into(),
            saltos,
            tablas_vinculadas: Vec::new(),
        }
    }

    fn vinculado(mut chunk: ChunkApartado, tabla: &str) -> ChunkApartado {
        chunk.tablas_vinculadas.push(tabla.into());
        chunk
    }

    const TARIFAS_P2: &str = "2 Tarifas\nTabla 1. Tarifa por peso\nHasta 5 kg 4,50 6,20\n";
    const TARIFAS_P3: &str = "Más de 5 kg 7,80 9,90\nEn temporada alta se incrementan un 20 %.\n";

    fn seccion_tarifas() -> Vec<ChunkApartado> {
        let texto = format!("{TARIFAS_P2}{TARIFAS_P3}");
        vec![
            chunk(
                "D@v1-C-0001",
                "D#1",
                "1 Objeto\nTexto.",
                Some(vec![salto(0, 1)]),
            ),
            vinculado(
                chunk(
                    "D@v1-C-0002",
                    "D#2",
                    &texto,
                    Some(vec![salto(0, 2), salto(TARIFAS_P2.len(), 3)]),
                ),
                "D@v1-TAB-0000",
            ),
            // La sección 3 cita la tabla y repite una cifra: no es donde están sus filas
            vinculado(
                chunk(
                    "D@v1-C-0003",
                    "D#3",
                    "3 Plazos\nSegún la Tabla 1, el envío de 4,50 sale en 48 h.",
                    Some(vec![salto(0, 4)]),
                ),
                "D@v1-TAB-0000",
            ),
        ]
    }

    fn valores() -> Vec<String> {
        ["4,50", "6,20", "7,80", "9,90"].map(String::from).to_vec()
    }

    #[test]
    fn test_elige_la_seccion_con_las_filas_y_no_la_que_solo_la_cita() {
        let apartado =
            ApartadoTabla::localizar(&seccion_tarifas(), "D@v1-TAB-0000", &valores()).unwrap();
        assert_eq!(apartado.nodo_id, "D#2");
        assert_eq!(apartado.paginas(), Some((2, 3)));
    }

    #[test]
    fn test_pagina_de_la_celda_sale_de_la_linea_de_su_valor() {
        let apartado =
            ApartadoTabla::localizar(&seccion_tarifas(), "D@v1-TAB-0000", &valores()).unwrap();
        assert_eq!(apartado.pagina_de_valor("6,20", &[]), Some(2));
        assert_eq!(apartado.pagina_de_valor("9,90", &[]), Some(3));
        // El valor no figura: no se supone ninguna
        assert_eq!(apartado.pagina_de_valor("1,00", &[]), None);
    }

    #[test]
    fn test_valor_repetido_en_dos_paginas_se_desempata_por_la_etiqueta_o_no_se_da() {
        let p2 = "Clase I 150 200\n";
        let p3 = "Clase II 150 250\n";
        let chunks = vec![vinculado(
            chunk(
                "D@v1-C-0001",
                "D#4",
                &format!("{p2}{p3}"),
                Some(vec![salto(0, 2), salto(p2.len(), 3)]),
            ),
            "T",
        )];
        let apartado = ApartadoTabla::localizar(&chunks, "T", &["150".into()]).unwrap();
        assert_eq!(apartado.pagina_de_valor("150", &[]), None);
        assert_eq!(
            apartado.pagina_de_valor("150", &["Clase II".into()]),
            Some(3)
        );
        // «Clase I» no está dentro de «Clase II» como palabras completas
        assert_eq!(
            apartado.pagina_de_valor("150", &["Clase I".into()]),
            Some(2)
        );
    }

    #[test]
    fn test_sin_saltos_no_hay_pagina_ni_rango() {
        let chunks = vec![vinculado(
            chunk("D@v1-C-0001", "D#2", "Hasta 5 kg 4,50 6,20", None),
            "T",
        )];
        let apartado = ApartadoTabla::localizar(&chunks, "T", &valores()).unwrap();
        assert_eq!(apartado.paginas(), None);
        assert_eq!(apartado.pagina_de_valor("6,20", &[]), None);
    }

    #[test]
    fn test_tabla_sin_vinculos_o_ambigua_sin_valores_no_tiene_apartado() {
        let mut chunks = seccion_tarifas();
        assert!(ApartadoTabla::localizar(&chunks, "OTRA", &valores()).is_none());
        // Dos secciones vinculadas y ningún valor en ninguna: no se adivina
        chunks[1].texto = "Texto sin cifras".into();
        chunks[2].texto = "Otro texto".into();
        chunks[1].saltos = Some(vec![salto(0, 2)]);
        assert!(ApartadoTabla::localizar(&chunks, "D@v1-TAB-0000", &valores()).is_none());
    }

    #[test]
    fn test_sub_nodos_por_paginas_cuentan_como_su_seccion_real() {
        let chunks = vec![
            vinculado(
                chunk(
                    "D@v1-C-0001",
                    "D#2.PAG2-2",
                    "Hasta 5 kg 4,50 6,20\n",
                    Some(vec![salto(0, 2)]),
                ),
                "T",
            ),
            chunk(
                "D@v1-C-0002",
                "D#2.PAG3-3",
                "En temporada alta se incrementan un 20 %.\n",
                Some(vec![salto(0, 3)]),
            ),
        ];
        let apartado = ApartadoTabla::localizar(&chunks, "T", &valores()).unwrap();
        assert_eq!(apartado.nodo_id, "D#2");
        assert!(apartado.texto.contains("temporada alta"));
        assert_eq!(apartado.paginas(), Some((2, 3)));
    }

    #[test]
    fn test_fila_sql_con_listas_json() {
        let fila = serde_json::json!({
            "chunk_id": "c", "nodo_id": "n", "texto_contenido": "t",
            "saltos_pagina": [{ "desplazamiento": 0, "pagina": 7 }],
            "tablas_vinculadas": ["T1", "T2"],
        });
        let chunk = ChunkApartado::desde_fila(fila.as_object().unwrap());
        assert_eq!(chunk.saltos, Some(vec![salto(0, 7)]));
        assert_eq!(chunk.tablas_vinculadas, vec!["T1", "T2"]);
        let nula = serde_json::json!({ "saltos_pagina": null, "tablas_vinculadas": null });
        let chunk = ChunkApartado::desde_fila(nula.as_object().unwrap());
        assert_eq!(chunk.saltos, None);
        assert!(chunk.tablas_vinculadas.is_empty());
    }
}
