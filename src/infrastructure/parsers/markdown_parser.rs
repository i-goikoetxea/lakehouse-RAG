//! Adaptador de Parser documental para Markdown estructurado y tablas técnicas

use async_trait::async_trait;

use crate::application::dtos::ingestion::{ParsedDocument, ParsedSection, ParsedTableRaw};
use crate::application::ports::document_parser::DocumentParserPort;
use crate::domain::errors::DomainError;

/// Parser para documentos en formato Markdown técnico
pub struct MarkdownParser;

impl MarkdownParser {
    pub fn new() -> Self {
        Self
    }

    /// Extrae el título principal del documento o usa el nombre de archivo como fallback
    fn extraer_titulo_principal(&self, lineas: &[&str], nombre_archivo: &str) -> String {
        for linea in lineas {
            let trim = linea.trim();
            if let Some(resto) = trim.strip_prefix("# ") {
                let titulo = resto.trim();
                if !titulo.is_empty() {
                    return titulo.to_string();
                }
            }
        }
        nombre_archivo
            .trim_end_matches(".md")
            .trim_end_matches(".txt")
            .to_string()
    }

    /// Extrae secciones jerárquicas preservando el nivel de encabezado
    fn extraer_secciones(&self, lineas: &[&str]) -> Vec<ParsedSection> {
        let mut secciones = Vec::new();
        let mut seccion_actual: Option<(u8, String, String, Vec<String>)> = None;

        for linea in lineas {
            let trim = linea.trim();
            if trim.starts_with('#') {
                let nivel = trim.chars().take_while(|&c| c == '#').count() as u8;
                let texto = trim.trim_start_matches('#').trim();

                if let Some((prev_nivel, prev_codigo, prev_titulo, lineas_contenido)) =
                    seccion_actual.take()
                {
                    let contenido = lineas_contenido.join("\n").trim().to_string();
                    if !contenido.is_empty() || !prev_titulo.is_empty() {
                        secciones.push(ParsedSection {
                            codigo: prev_codigo,
                            titulo: prev_titulo,
                            nivel: prev_nivel,
                            contenido,
                        });
                    }
                }

                // Detectar si el texto inicia con numeración ej. "1.1 Criterios de Aceptación"
                let (codigo, titulo) =
                    Self::separar_codigo_y_titulo(texto, nivel, secciones.len() + 1);
                seccion_actual = Some((nivel, codigo, titulo, Vec::new()));
            } else if let Some((_, _, _, ref mut lineas_contenido)) = seccion_actual {
                lineas_contenido.push(linea.to_string());
            }
        }

        if let Some((prev_nivel, prev_codigo, prev_titulo, lineas_contenido)) = seccion_actual {
            let contenido = lineas_contenido.join("\n").trim().to_string();
            secciones.push(ParsedSection {
                codigo: prev_codigo,
                titulo: prev_titulo,
                nivel: prev_nivel,
                contenido,
            });
        }

        secciones
    }

    fn separar_codigo_y_titulo(
        texto: &str,
        _nivel: u8,
        indice_fallback: usize,
    ) -> (String, String) {
        let partes: Vec<&str> = texto.split_whitespace().collect();
        if !partes.is_empty() {
            let primer = partes[0];
            // Verificar si parece código como "1.", "1.2", "4.3.1", "Sección"
            if primer.ends_with('.')
                || primer.contains('.')
                || primer.chars().all(|c| c.is_ascii_digit())
            {
                let codigo = primer.trim_end_matches('.').to_string();
                let titulo = partes[1..].join(" ");
                return (
                    codigo,
                    if titulo.is_empty() {
                        primer.to_string()
                    } else {
                        titulo
                    },
                );
            }
        }
        (format!("SEC-{indice_fallback}"), texto.to_string())
    }

    /// Extrae tablas crudas en formato Markdown (| col1 | col2 |)
    fn extraer_tablas(&self, lineas: &[&str]) -> Vec<ParsedTableRaw> {
        let mut tablas = Vec::new();
        let mut i = 0;

        while i < lineas.len() {
            let linea = lineas[i].trim();
            if linea.starts_with('|') && linea.ends_with('|') && linea.contains('|') {
                let start_idx = i;
                let mut buffer_tabla = Vec::new();

                while i < lineas.len()
                    && lineas[i].trim().starts_with('|')
                    && lineas[i].trim().ends_with('|')
                {
                    buffer_tabla.push(lineas[i].trim());
                    i += 1;
                }

                // Debe tener al menos encabezado y fila separadora (| --- | --- |)
                if buffer_tabla.len() >= 2 {
                    let separador = buffer_tabla[1];
                    if separador.contains("---") {
                        let encabezados = Self::parse_fila_tabla(buffer_tabla[0]);
                        let mut filas = Vec::new();

                        for fila_str in &buffer_tabla[2..] {
                            let celdas = Self::parse_fila_tabla(fila_str);
                            if !celdas.is_empty() {
                                filas.push(celdas);
                            }
                        }

                        // Buscar posible título en la línea previa inmediata
                        let mut titulo_tabla = format!("Tabla {}", tablas.len() + 1);
                        if start_idx > 0 {
                            let linea_previa = lineas[start_idx - 1].trim();
                            if !linea_previa.is_empty() && !linea_previa.starts_with('#') {
                                titulo_tabla = linea_previa
                                    .trim_start_matches("**")
                                    .trim_end_matches("**")
                                    .to_string();
                            }
                        }

                        // Buscar notas al pie inmediatas posteriores (ej. *Nota:* ...)
                        let mut notas = None;
                        if i < lineas.len() {
                            let linea_siguiente = lineas[i].trim();
                            if linea_siguiente.starts_with('*')
                                || linea_siguiente.to_lowercase().starts_with("nota:")
                            {
                                notas = Some(linea_siguiente.to_string());
                                i += 1;
                            }
                        }

                        tablas.push(ParsedTableRaw {
                            titulo: titulo_tabla,
                            encabezados,
                            filas,
                            notas,
                        });
                        continue;
                    }
                }
            }
            i += 1;
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
        let secciones = self.extraer_secciones(&lineas);
        let tablas = self.extraer_tablas(&lineas);

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
        let parser = MarkdownParser::new();
        let parsed = parser.parse("manual_op.md", doc_md).await.unwrap();

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
        let parser = MarkdownParser::new();
        let parsed = parser.parse("calibracion.md", doc_md).await.unwrap();

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
    async fn test_parser_documento_vacio_falla_con_parser_error() {
        let parser = MarkdownParser::new();
        let err = parser.parse("vacio.md", "   \n\t  ").await.unwrap_err();
        assert!(matches!(err, DomainError::ParserError(_)));
    }

    #[tokio::test]
    async fn test_parser_resistencia_a_saltos_linea_crlf_y_espacios_en_blanco() {
        let doc_crlf = "# Título CRLF\r\n\r\n## 1. Sección Uno\r\nLínea con CRLF\r\n";
        let parser = MarkdownParser::new();
        let parsed = parser.parse("test.md", doc_crlf).await.unwrap();
        assert_eq!(parsed.titulo, "Título CRLF");
        assert_eq!(parsed.secciones.len(), 2);
    }
}
