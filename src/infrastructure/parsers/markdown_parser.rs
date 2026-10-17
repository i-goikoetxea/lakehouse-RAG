//! Adaptador de Parser documental para Markdown estructurado y tablas técnicas.
//!
//! - Cada sección guarda su breadcrumb completo (`H1 > H2 > H3`) y la condición de aplicación
//!   heredada de sus encabezados ancestros (`PilaEncabezados`, H7 y problema 7).
//! - Las tablas capturan su título aunque haya una línea en blanco entre ambos y las notas al
//!   pie multilínea con llamadas `(*)`, `(1)`, `(a)`, `*` o `Nota:` (H8).

use async_trait::async_trait;

use crate::application::dtos::ingestion::{ParsedDocument, ParsedSection, ParsedTableRaw};
use crate::application::ports::document_parser::DocumentParserPort;
use crate::domain::errors::DomainError;
use crate::domain::models::alcance::PilaEncabezados;

/// Líneas en blanco que pueden separar una tabla de su título o de sus notas
const MAX_LINEAS_EN_BLANCO_TOLERADAS: usize = 2;
/// Profundidad máxima de encabezado Markdown reconocida
const MAX_NIVEL_ENCABEZADO: usize = 6;

/// Parser para documentos en formato Markdown técnico
pub struct MarkdownParser;

/// Sección en construcción durante el recorrido del documento
struct SeccionAbierta {
    codigo: String,
    titulo: String,
    nivel: u8,
    breadcrumb: String,
    condicion_aplicacion: Option<String>,
    lineas: Vec<String>,
}

impl SeccionAbierta {
    fn cerrar(self) -> ParsedSection {
        ParsedSection {
            codigo: self.codigo,
            titulo: self.titulo,
            nivel: self.nivel,
            contenido: self.lineas.join("\n").trim().to_string(),
            breadcrumb: self.breadcrumb,
            condicion_aplicacion: self.condicion_aplicacion,
        }
    }
}

impl MarkdownParser {
    pub fn new() -> Self {
        Self
    }

    /// Extrae el título principal del documento o usa el nombre de archivo como fallback
    fn extraer_titulo_principal(&self, lineas: &[&str], nombre_archivo: &str) -> String {
        lineas
            .iter()
            .filter_map(|linea| linea.trim().strip_prefix("# "))
            .map(str::trim)
            .find(|titulo| !titulo.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                nombre_archivo
                    .trim_end_matches(".md")
                    .trim_end_matches(".txt")
                    .to_string()
            })
    }

    /// Nivel y texto de un encabezado ATX (`## 2. Título`); `None` si la línea no lo es
    fn interpretar_encabezado(linea: &str) -> Option<(u8, &str)> {
        let trim = linea.trim();
        let nivel = trim.chars().take_while(|&c| c == '#').count();
        if nivel == 0 || nivel > MAX_NIVEL_ENCABEZADO {
            return None;
        }
        let resto = &trim[nivel..];
        if !resto.is_empty() && !resto.starts_with(char::is_whitespace) {
            return None;
        }
        Some((nivel as u8, resto.trim()))
    }

    /// Secciones con breadcrumb y condición heredada; devuelve también el código de la
    /// sección abierta en cada línea (para ubicar las tablas)
    fn extraer_secciones(&self, lineas: &[&str]) -> (Vec<ParsedSection>, Vec<Option<String>>) {
        let mut secciones = Vec::new();
        let mut seccion_por_linea = Vec::with_capacity(lineas.len());
        let mut pila = PilaEncabezados::default();
        let mut abierta: Option<SeccionAbierta> = None;

        for linea in lineas {
            if let Some((nivel, texto)) = Self::interpretar_encabezado(linea) {
                if let Some(previa) = abierta.take() {
                    secciones.push(previa.cerrar());
                }
                pila.apilar(nivel, texto);
                let (codigo, titulo) = Self::separar_codigo_y_titulo(texto, secciones.len() + 1);
                abierta = Some(SeccionAbierta {
                    codigo,
                    titulo,
                    nivel,
                    breadcrumb: pila.breadcrumb(),
                    condicion_aplicacion: pila.condicion_heredada(),
                    lineas: Vec::new(),
                });
            } else if let Some(seccion) = abierta.as_mut() {
                seccion.lineas.push(linea.to_string());
            }
            seccion_por_linea.push(abierta.as_ref().map(|s| s.codigo.clone()));
        }
        if let Some(ultima) = abierta {
            secciones.push(ultima.cerrar());
        }
        (secciones, seccion_por_linea)
    }

    fn separar_codigo_y_titulo(texto: &str, indice_fallback: usize) -> (String, String) {
        let partes: Vec<&str> = texto.split_whitespace().collect();
        if let Some(primer) = partes.first() {
            // Numeración tipo "1.", "1.2", "4.3.1" o "4"
            if primer.ends_with('.')
                || primer.contains('.')
                || primer.chars().all(|c| c.is_ascii_digit())
            {
                let codigo = primer.trim_end_matches('.').to_string();
                let titulo = partes[1..].join(" ");
                let titulo = if titulo.is_empty() {
                    primer.to_string()
                } else {
                    titulo
                };
                return (codigo, titulo);
            }
        }
        (format!("SEC-{indice_fallback}"), texto.to_string())
    }

    fn es_fila_tabla(linea: &str) -> bool {
        let trim = linea.trim();
        trim.len() > 1 && trim.starts_with('|') && trim.ends_with('|')
    }

    /// Llamada de nota al pie: `(*)`, `(**)`, `(1)`, `(a)`, `*texto`, `Nota:` o `Notas:`
    fn es_inicio_de_nota(linea: &str) -> bool {
        let trim = linea.trim();
        if let Some(resto) = trim.strip_prefix('(')
            && let Some(cierre) = resto.find(')')
        {
            let llamada = resto[..cierre].trim();
            let es_llamada = !llamada.is_empty()
                && llamada.chars().count() <= 3
                && (llamada.chars().all(|c| c == '*')
                    || llamada.chars().all(|c| c.is_ascii_digit())
                    || llamada.chars().all(|c| c.is_ascii_alphabetic()));
            return es_llamada;
        }
        let es_asterisco = trim.starts_with('*') && !trim.starts_with("**");
        let minusculas = trim.to_lowercase();
        es_asterisco || minusculas.starts_with("nota:") || minusculas.starts_with("notas:")
    }

    /// Título de la tabla que empieza en `inicio`: la línea previa no vacía. Si hay líneas en
    /// blanco por medio, solo se acepta si parece un título (negrita o empieza por «Tabla»).
    fn buscar_titulo_tabla(lineas: &[&str], inicio: usize) -> Option<String> {
        let mut blancos = 0;
        let mut idx = inicio;
        while idx > 0 {
            idx -= 1;
            let previa = lineas[idx].trim();
            if previa.is_empty() {
                blancos += 1;
                if blancos > MAX_LINEAS_EN_BLANCO_TOLERADAS {
                    return None;
                }
                continue;
            }
            if Self::interpretar_encabezado(previa).is_some()
                || Self::es_fila_tabla(previa)
                || Self::es_inicio_de_nota(previa)
            {
                return None;
            }
            let es_negrita = previa.starts_with("**") && previa.ends_with("**");
            let parece_titulo = es_negrita || previa.to_lowercase().starts_with("tabla");
            if blancos > 0 && !parece_titulo {
                return None;
            }
            let titulo = previa.trim_matches('*').trim();
            return (!titulo.is_empty()).then(|| titulo.to_string());
        }
        None
    }

    /// Notas al pie tras la tabla (desde `inicio`): devuelve las notas (una por línea, con sus
    /// continuaciones unidas) y el índice de la primera línea no consumida
    fn extraer_notas(lineas: &[&str], inicio: usize) -> (Option<String>, usize) {
        let mut notas: Vec<String> = Vec::new();
        let mut idx = inicio;
        let mut blancos = 0;
        let mut consumido = inicio;
        while idx < lineas.len() {
            let linea = lineas[idx].trim();
            if linea.is_empty() {
                blancos += 1;
                if blancos > MAX_LINEAS_EN_BLANCO_TOLERADAS {
                    break;
                }
                idx += 1;
                continue;
            }
            if Self::es_inicio_de_nota(linea) {
                notas.push(linea.to_string());
            } else if blancos == 0
                && !notas.is_empty()
                && Self::interpretar_encabezado(linea).is_none()
                && !Self::es_fila_tabla(linea)
            {
                // Continuación de la nota anterior (nota multilínea)
                if let Some(ultima) = notas.last_mut() {
                    ultima.push(' ');
                    ultima.push_str(linea);
                }
            } else {
                break;
            }
            blancos = 0;
            idx += 1;
            consumido = idx;
        }
        let notas = (!notas.is_empty()).then(|| notas.join("\n"));
        (notas, consumido)
    }

    /// Extrae tablas crudas en formato Markdown (| col1 | col2 |)
    fn extraer_tablas(
        &self,
        lineas: &[&str],
        seccion_por_linea: &[Option<String>],
    ) -> Vec<ParsedTableRaw> {
        let mut tablas = Vec::new();
        let mut i = 0;

        while i < lineas.len() {
            if !Self::es_fila_tabla(lineas[i]) {
                i += 1;
                continue;
            }
            let inicio = i;
            while i < lineas.len() && Self::es_fila_tabla(lineas[i]) {
                i += 1;
            }
            let bloque = &lineas[inicio..i];
            // Debe tener al menos encabezado y fila separadora (| --- | --- |)
            if bloque.len() < 2 || !bloque[1].contains("---") {
                continue;
            }
            let filas = bloque[2..]
                .iter()
                .map(|fila| Self::parse_fila_tabla(fila))
                .filter(|celdas| !celdas.is_empty())
                .collect();
            let titulo = Self::buscar_titulo_tabla(lineas, inicio)
                .unwrap_or_else(|| format!("Tabla {}", tablas.len() + 1));
            let (notas, siguiente) = Self::extraer_notas(lineas, i);
            i = siguiente;

            tablas.push(ParsedTableRaw {
                titulo,
                encabezados: Self::parse_fila_tabla(bloque[0]),
                filas,
                notas,
                seccion_codigo: seccion_por_linea.get(inicio).cloned().flatten(),
            });
        }
        tablas
    }

    fn parse_fila_tabla(fila: &str) -> Vec<String> {
        let contenido = fila.trim();
        let sin_bordes = contenido
            .strip_prefix('|')
            .unwrap_or(contenido)
            .strip_suffix('|')
            .unwrap_or(contenido);

        sin_bordes
            .split('|')
            .map(|s| s.trim().to_string())
            .collect()
    }
}

impl Default for MarkdownParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DocumentParserPort for MarkdownParser {
    async fn parse(
        &self,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<ParsedDocument, DomainError> {
        let contenido_limpio = contenido.trim();
        if contenido_limpio.is_empty() {
            return Err(DomainError::ParserError(
                "El contenido del documento a parsear está vacío".to_string(),
            ));
        }

        let normalizado = contenido.replace("\r\n", "\n");
        let lineas: Vec<&str> = normalizado.lines().collect();

        let titulo = self.extraer_titulo_principal(&lineas, nombre_archivo);
        let (secciones, seccion_por_linea) = self.extraer_secciones(&lineas);
        let tablas = self.extraer_tablas(&lineas, &seccion_por_linea);

        Ok(ParsedDocument {
            nombre_archivo: nombre_archivo.to_string(),
            titulo,
            secciones,
            tablas,
            contenido_completo: normalizado,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn parsear(markdown: &str) -> ParsedDocument {
        MarkdownParser::new()
            .parse("doc.md", markdown)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_parser_markdown_extrae_titulo_y_jerarquia_secciones() {
        let doc_md = r#"
# Manual de Operación y Seguridad Planta Norte

## 1. Introducción y Alcance
Este procedimiento establece los requerimientos mínimos de seguridad industrial.

### 1.1 Recepción de Materiales
El personal debe inspeccionar los sellos de seguridad antes de permitir el ingreso.

## 2. Inspección Técnica
Verificar presión manométrica en línea troncal.
"#;
        let parsed = parsear(doc_md).await;

        assert_eq!(
            parsed.titulo,
            "Manual de Operación y Seguridad Planta Norte"
        );
        assert_eq!(parsed.secciones.len(), 4);

        let sec_intro = &parsed.secciones[1];
        assert_eq!(sec_intro.codigo, "1");
        assert_eq!(sec_intro.titulo, "Introducción y Alcance");
        assert_eq!(sec_intro.nivel, 2);
        assert!(sec_intro.contenido.contains("requerimientos mínimos"));

        let sec_recepcion = &parsed.secciones[2];
        assert_eq!(sec_recepcion.codigo, "1.1");
        assert_eq!(sec_recepcion.titulo, "Recepción de Materiales");
        assert_eq!(sec_recepcion.nivel, 3);
    }

    #[tokio::test]
    async fn test_breadcrumb_completo_y_condicion_heredada_por_los_descendientes() {
        let doc_md = "# Cap 4. Ejecución\n\n## 4.2 Soldadura\nTexto general.\n\n\
                      ### 4.2.1 Condiciones bajo cero (< 0 °C)\nIntro.\n\n\
                      #### 4.2.1.1 Precalentamiento\nCalentar a 80 °C.\n\n\
                      ## 4.3 Pintura\nSin condición.\n";
        let parsed = parsear(doc_md).await;
        let precalentamiento = parsed
            .secciones
            .iter()
            .find(|s| s.codigo == "4.2.1.1")
            .unwrap();
        assert_eq!(
            precalentamiento.breadcrumb,
            "Cap 4. Ejecución > 4.2 Soldadura > 4.2.1 Condiciones bajo cero (< 0 °C) > 4.2.1.1 Precalentamiento"
        );
        assert_eq!(
            precalentamiento.condicion_aplicacion.as_deref(),
            Some("Condiciones bajo cero (< 0 °C)")
        );
        let pintura = parsed.secciones.iter().find(|s| s.codigo == "4.3").unwrap();
        assert_eq!(pintura.breadcrumb, "Cap 4. Ejecución > 4.3 Pintura");
        assert_eq!(pintura.condicion_aplicacion, None);
    }

    #[tokio::test]
    async fn test_parser_markdown_extrae_tabla_con_encabezados_y_filas() {
        let doc_md = r#"
# Procedimiento de Calibración

**Matriz de Tolerancias de Espesor**
| Rango Espesor (mm) | Tolerancia Admisible | Categoría Inspección |
| --- | --- | --- |
| 0.0 - 5.0 | +/- 0.05 mm | Nivel A |
| 5.1 - 20.0 | +/- 0.12 mm | Nivel B |
*Nota: Mediciones con calibrador vernier calibrado.*

## 3. Fin del Proceso
"#;
        let parsed = parsear(doc_md).await;

        assert_eq!(parsed.tablas.len(), 1);
        let tabla = &parsed.tablas[0];
        assert_eq!(tabla.titulo, "Matriz de Tolerancias de Espesor");
        assert_eq!(
            tabla.encabezados,
            vec![
                "Rango Espesor (mm)",
                "Tolerancia Admisible",
                "Categoría Inspección"
            ]
        );
        assert_eq!(tabla.filas.len(), 2);
        assert_eq!(tabla.filas[0], vec!["0.0 - 5.0", "+/- 0.05 mm", "Nivel A"]);
        assert_eq!(
            tabla.notas,
            Some("*Nota: Mediciones con calibrador vernier calibrado.*".to_string())
        );
    }

    #[tokio::test]
    async fn test_titulo_tras_linea_en_blanco_y_notas_multilinea() {
        let doc_md = "# Doc\n\n## 3.1 Precalentamiento\n\n**Tabla 4-A Temperatura (°C)**\n\n\
                      | CE | e ≤ 20 |\n| --- | --- |\n| CE < 0,40 | 10 (*) |\n\n\
                      (*) Requiere aprobación del Inspector QA/QC\n    y ensayo de dureza HV10.\n\
                      (1) Interpaso por debajo de 250 °C.\n(a) Solo acero S355J2.\n\n\
                      Texto posterior que no es nota.\n";
        let parsed = parsear(doc_md).await;
        let tabla = &parsed.tablas[0];
        assert_eq!(tabla.titulo, "Tabla 4-A Temperatura (°C)");
        assert_eq!(tabla.seccion_codigo.as_deref(), Some("3.1"));
        let notas = tabla.notas.as_deref().unwrap();
        let lineas: Vec<&str> = notas.lines().collect();
        assert_eq!(lineas.len(), 3, "{notas}");
        assert_eq!(
            lineas[0],
            "(*) Requiere aprobación del Inspector QA/QC y ensayo de dureza HV10."
        );
        assert!(lineas[1].starts_with("(1)") && lineas[2].starts_with("(a)"));
        assert!(!notas.contains("Texto posterior"));
    }

    #[tokio::test]
    async fn test_parrafo_separado_por_blanco_no_es_titulo_y_tabla_sin_notas() {
        let doc_md = "# Doc\nEste párrafo describe el ensayo.\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\nOtro párrafo.\n";
        let parsed = parsear(doc_md).await;
        assert_eq!(parsed.tablas[0].titulo, "Tabla 1");
        assert_eq!(parsed.tablas[0].notas, None);
    }

    #[tokio::test]
    async fn test_parser_documento_vacio_falla_con_parser_error() {
        let parser = MarkdownParser::new();
        let err = parser.parse("vacio.md", "   \n\t  ").await.unwrap_err();
        assert!(matches!(err, DomainError::ParserError(_)));
    }

    #[tokio::test]
    async fn test_parser_resistencia_a_saltos_linea_crlf_y_espacios_en_blanco() {
        let doc_crlf = "# Título CRLF\r\n\r\n## 1. Sección Uno\r\nLínea con CRLF\r\n";
        let parsed = parsear(doc_crlf).await;
        assert_eq!(parsed.titulo, "Título CRLF");
        assert_eq!(parsed.secciones.len(), 2);
        assert_eq!(
            parsed.secciones[1].breadcrumb,
            "Título CRLF > 1. Sección Uno"
        );
    }
}
