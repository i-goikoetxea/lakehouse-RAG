//! Adaptador de PDF digital, sin OCR (`docs/arquitectura/decisiones_stair.md` §5 y
//! `ssd_generalidad_4_piezas.md` §1):
//! - lopdf abre el archivo, rechaza el cifrado y lee `/Outlines`. No se usa `get_toc` ni
//!   `get_outlines` de lopdf: un `/Next` cíclico los deja en un bucle y descartan títulos repetidos.
//! - pdfium extrae el texto de cada página en un proceso hijo aislado (`proceso_aislado.rs`). Si el
//!   extractor o la DLL faltan, la ingesta falla con un mensaje: no hay respaldo silencioso a otro
//!   extractor. lopdf solo cubre una página concreta que pdfium no pudo leer.
//! - La compuerta de legibilidad (`legibilidad.rs`) mide el texto crudo de cada página antes de
//!   normalizarlo.
//! - lopdf corre en un hilo propio con pila grande, bajo `catch_unwind` y con límite de tiempo.
//! - Un PDF protegido se rechaza siempre y sin abrirlo, aunque baste la contraseña vacía.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use async_trait::async_trait;
use lopdf::{Dictionary, Document, Object, ObjectId, decode_text_string};
use tokio::sync::oneshot;

use crate::application::dtos::ingestion::{MotivoDegradacion, PaginaDegradada, ParsedDocument};
use crate::application::ports::document_parser::DocumentParserPort;
use crate::domain::errors::DomainError;
use crate::domain::services::marcas_pagina::describir_paginas;
use crate::infrastructure::parsers::legibilidad::{ClasePagina, clasificar_pagina};
use crate::infrastructure::parsers::pdf_estructura::{
    EntradaMarcador, dividir_secciones_largas, estructurar_por_marcadores,
    estructurar_por_numeracion, normalizar_pagina, requiere_ocr, seccion_contenido_general,
};
use crate::infrastructure::parsers::pdfium_extractor::PaginaExtraida;
use crate::infrastructure::parsers::proceso_aislado::{ProcesoExtractor, TIEMPO_MAXIMO_EXTRACCION};

/// Las bibliotecas recorren el PDF de forma recursiva: un PDF hostil no debe agotar la pila
const TAMANO_PILA_EXTRACCION: usize = 32 * 1024 * 1024;
const BYTES_DE_CABECERA: usize = 1024;
const MAX_MARCADORES: usize = 10_000;
const MAX_PROFUNDIDAD_MARCADORES: u8 = 32;
const MAX_NIVEL_MARCADOR: u8 = 6;
const MAX_REFERENCIAS_DESTINO: u8 = 8;
const MAX_DESTINOS_CON_NOMBRE: usize = 100_000;

/// Parser para PDF con capa de texto
#[derive(Debug, Clone, Default)]
pub struct PdfParser {
    extractor: ProcesoExtractor,
}

impl PdfParser {
    /// Con el extractor `lakehouse_rag_cli` junto al ejecutable
    pub fn new() -> Self {
        Self::default()
    }

    pub fn con_extractor(extractor: ProcesoExtractor) -> Self {
        Self { extractor }
    }
}

#[async_trait]
impl DocumentParserPort for PdfParser {
    async fn parse(
        &self,
        nombre_archivo: &str,
        contenido: &str,
    ) -> Result<ParsedDocument, DomainError> {
        self.parse_bytes(nombre_archivo, contenido.as_bytes()).await
    }

    async fn parse_bytes(
        &self,
        nombre_archivo: &str,
        contenido: &[u8],
    ) -> Result<ParsedDocument, DomainError> {
        let nombre = nombre_archivo.to_string();
        let bytes = contenido.to_vec();
        let abierto = en_hilo_protegido(nombre_archivo, move || {
            abrir_y_leer_marcadores(&nombre, &bytes)
        })
        .await?;
        let extraidas = self
            .extractor
            .extraer(contenido)
            .await
            .map_err(|motivo| error_pdf(nombre_archivo, &motivo))?;
        let nombre = nombre_archivo.to_string();
        let bytes = contenido.to_vec();
        en_hilo_protegido(nombre_archivo, move || {
            Ok(componer_documento(&nombre, &bytes, abierto, extraidas))
        })
        .await
    }
}

/// Ejecuta la parte de lopdf en un hilo con pila grande, sin pánicos y con límite de tiempo
async fn en_hilo_protegido<T: Send + 'static>(
    nombre_archivo: &str,
    tarea: impl FnOnce() -> Result<T, DomainError> + Send + 'static,
) -> Result<T, DomainError> {
    let (emisor, receptor) = oneshot::channel();
    let nombre = nombre_archivo.to_string();
    std::thread::Builder::new()
        .name("extraccion-pdf".to_string())
        .stack_size(TAMANO_PILA_EXTRACCION)
        .spawn(move || {
            let resultado = catch_unwind(AssertUnwindSafe(tarea)).unwrap_or_else(|_| {
                Err(error_pdf(
                    &nombre,
                    "fallo interno al leerlo: PDF no compatible",
                ))
            });
            let _ = emisor.send(resultado);
        })
        .map_err(|e| {
            error_pdf(
                nombre_archivo,
                &format!("no se pudo iniciar la lectura: {e}"),
            )
        })?;
    match tokio::time::timeout(TIEMPO_MAXIMO_EXTRACCION, receptor).await {
        Ok(Ok(resultado)) => resultado,
        Ok(Err(_)) => Err(error_pdf(
            nombre_archivo,
            "la lectura terminó sin resultado",
        )),
        Err(_) => Err(error_pdf(
            nombre_archivo,
            "la lectura superó el tiempo máximo; el PDF puede estar dañado",
        )),
    }
}

fn error_pdf(nombre_archivo: &str, motivo: &str) -> DomainError {
    DomainError::IngestionError(format!("PDF '{nombre_archivo}': {motivo}"))
}

/// Lo que lopdf aporta antes de extraer el texto
struct PdfAbierto {
    total_paginas: usize,
    marcadores: Vec<EntradaMarcador>,
}

fn abrir_y_leer_marcadores(nombre_archivo: &str, bytes: &[u8]) -> Result<PdfAbierto, DomainError> {
    let documento = abrir_sin_proteccion(nombre_archivo, bytes)?;
    let total_paginas = documento.get_pages().len();
    if total_paginas == 0 {
        return Err(error_pdf(nombre_archivo, "no tiene páginas (corrupto)"));
    }
    // Unos marcadores ilegibles no invalidan el documento: se recurre a la numeración
    let marcadores =
        catch_unwind(AssertUnwindSafe(|| leer_marcadores(&documento))).unwrap_or_default();
    Ok(PdfAbierto {
        total_paginas,
        marcadores,
    })
}

/// Clase de cada página según la compuerta
#[derive(Default)]
struct Legibilidad {
    degradadas: Vec<PaginaDegradada>,
    sin_texto: Vec<u32>,
}

fn componer_documento(
    nombre_archivo: &str,
    bytes: &[u8],
    abierto: PdfAbierto,
    extraidas: Vec<PaginaExtraida>,
) -> ParsedDocument {
    let (crudas, mut avisos) = textos_crudos(bytes, abierto.total_paginas, extraidas);
    let legibilidad = clasificar_paginas(&crudas);
    let paginas: Vec<String> = crudas.iter().map(|t| normalizar_pagina(t)).collect();
    let titulo = nombre_sin_extension(nombre_archivo);
    let total_paginas = paginas.len();

    if requiere_ocr(&paginas) {
        avisos.push(format!(
            "Menos de 50 caracteres por página de media en {total_paginas} páginas: requiere OCR"
        ));
        let respaldo = format!("[Sin texto extraíble; requiere OCR] {nombre_archivo}");
        let seccion = seccion_contenido_general(&paginas, &respaldo);
        return ParsedDocument {
            nombre_archivo: nombre_archivo.to_string(),
            titulo,
            contenido_completo: seccion.contenido.clone(),
            secciones: vec![seccion],
            tablas: Vec::new(),
            avisos_extraccion: avisos,
            requiere_ocr: true,
            paginas_degradadas: legibilidad.degradadas,
            paginas_sin_texto: legibilidad.sin_texto,
        };
    }

    avisos.extend(avisos_de_legibilidad(&legibilidad));
    let secciones = if abierto.marcadores.is_empty() {
        estructurar_por_numeracion(&paginas)
            .unwrap_or_else(|| vec![seccion_contenido_general(&paginas, "")])
    } else {
        estructurar_por_marcadores(&paginas, &abierto.marcadores)
    };
    ParsedDocument {
        nombre_archivo: nombre_archivo.to_string(),
        titulo,
        secciones: dividir_secciones_largas(secciones),
        tablas: Vec::new(),
        contenido_completo: paginas.join("\n\n"),
        avisos_extraccion: avisos,
        requiere_ocr: false,
        paginas_degradadas: legibilidad.degradadas,
        paginas_sin_texto: legibilidad.sin_texto,
    }
}

/// Texto crudo de cada página de lopdf (base 1). Una página que pdfium no leyó se intenta con
/// lopdf; si tampoco, queda vacía y constatada, sin impedir el resto
fn textos_crudos(
    bytes: &[u8],
    total_paginas: usize,
    extraidas: Vec<PaginaExtraida>,
) -> (Vec<String>, Vec<String>) {
    let mut por_pagina: HashMap<u32, String> = extraidas
        .into_iter()
        .filter_map(|p| Some((p.pagina, p.texto?)))
        .collect();
    let mut respaldo: Option<Option<Document>> = None;
    let mut avisos = Vec::new();
    let mut crudas = Vec::with_capacity(total_paginas);
    for pagina in 1..=u32::try_from(total_paginas).unwrap_or(u32::MAX) {
        let texto = por_pagina.remove(&pagina).or_else(|| {
            let documento = respaldo.get_or_insert_with(|| Document::load_mem(bytes).ok());
            let texto = documento.as_ref().and_then(|d| texto_con_lopdf(d, pagina));
            avisos.push(match texto {
                Some(_) => format!(
                    "Página {pagina}: extraída con el método de respaldo; el orden de sus líneas \
                     puede variar"
                ),
                None => {
                    format!("Página {pagina}: no se pudo extraer su texto y falta en el corpus")
                }
            });
            texto
        });
        crudas.push(texto.unwrap_or_default());
    }
    (crudas, avisos)
}

fn clasificar_paginas(crudas: &[String]) -> Legibilidad {
    let mut legibilidad = Legibilidad::default();
    for (pagina, texto) in (1u32..).zip(crudas) {
        match clasificar_pagina(texto).0 {
            ClasePagina::Legible => {}
            ClasePagina::SinTexto => legibilidad.sin_texto.push(pagina),
            ClasePagina::Degradada { motivos } => legibilidad
                .degradadas
                .push(PaginaDegradada { pagina, motivos }),
        }
    }
    legibilidad
}

/// Un aviso por clase, con las páginas agrupadas en tramos
fn avisos_de_legibilidad(legibilidad: &Legibilidad) -> Vec<String> {
    let mut avisos = Vec::new();
    if !legibilidad.degradadas.is_empty() {
        let paginas: Vec<u32> = legibilidad.degradadas.iter().map(|p| p.pagina).collect();
        let mut motivos: Vec<&str> = legibilidad
            .degradadas
            .iter()
            .flat_map(|p| p.motivos.iter().map(MotivoDegradacion::descripcion))
            .collect();
        motivos.sort_unstable();
        motivos.dedup();
        avisos.push(format!(
            "{}: texto degradado ({}). Sus fragmentos quedan como LECTURA_DEGRADADA: la \
             búsqueda puede no encontrar lo que contienen. Revisa el PDF o pásalo por un OCR.",
            describir_paginas(&paginas),
            motivos.join(", ")
        ));
    }
    if !legibilidad.sin_texto.is_empty() {
        avisos.push(format!(
            "{} sin texto extraíble (imagen o página en blanco).",
            describir_paginas(&legibilidad.sin_texto)
        ));
    }
    avisos
}

fn nombre_sin_extension(nombre_archivo: &str) -> String {
    nombre_archivo
        .rsplit_once('.')
        .map_or(nombre_archivo, |(base, _)| base)
        .to_string()
}

fn abrir_sin_proteccion(nombre_archivo: &str, bytes: &[u8]) -> Result<Document, DomainError> {
    if bytes.is_empty() {
        return Err(error_pdf(nombre_archivo, "el archivo está vacío"));
    }
    let cabecera = &bytes[..bytes.len().min(BYTES_DE_CABECERA)];
    if !cabecera.windows(5).any(|ventana| ventana == b"%PDF-") {
        return Err(error_pdf(
            nombre_archivo,
            "no es un PDF: le falta la cabecera, aunque la extensión lo diga",
        ));
    }
    let protegido = || {
        error_pdf(
            nombre_archivo,
            "está protegido con contraseña o cifrado. Retira la protección con su propietario y              vuelve a ingerirlo",
        )
    };
    // Antes de abrirlo: lopdf prueba por sí mismo la contraseña vacía y descifra sin avisar
    if declara_cifrado(bytes) {
        return Err(protegido());
    }
    match Document::load_mem(bytes) {
        Ok(documento) if documento.is_encrypted() || documento.trailer.has(b"Encrypt") => {
            Err(protegido())
        }
        Ok(documento) => Ok(documento),
        Err(error) => Err(error_pdf(
            nombre_archivo,
            &format!("está corrupto o truncado ({error})"),
        )),
    }
}

/// `/Encrypt` seguido de una referencia o un diccionario, como en un tráiler; un texto sin
/// comprimir que solo mencione esa palabra no cuenta
fn declara_cifrado(bytes: &[u8]) -> bool {
    const CLAVE: &[u8] = b"/Encrypt";
    bytes
        .windows(CLAVE.len())
        .enumerate()
        .any(|(posicion, ventana)| {
            ventana == CLAVE && {
                let resto = &bytes[posicion + CLAVE.len()..];
                let inicio = resto
                    .iter()
                    .position(|b| !b.is_ascii_whitespace())
                    .unwrap_or(resto.len());
                resto[inicio..].starts_with(b"<<")
                    || resto.get(inicio).is_some_and(u8::is_ascii_digit)
            }
        })
}

fn texto_con_lopdf(documento: &Document, pagina: u32) -> Option<String> {
    catch_unwind(AssertUnwindSafe(|| documento.extract_text(&[pagina]).ok()))
        .ok()
        .flatten()
}

// --- Marcadores ---

/// Recorrido de `/Outlines` que tolera ciclos y destinos inválidos
struct LectorDeMarcadores<'a> {
    documento: &'a Document,
    paginas: HashMap<ObjectId, u32>,
    destinos: HashMap<Vec<u8>, Object>,
    visitados: HashSet<ObjectId>,
    entradas: Vec<EntradaMarcador>,
}

fn leer_marcadores(documento: &Document) -> Vec<EntradaMarcador> {
    let Ok(catalogo) = documento.catalog() else {
        return Vec::new();
    };
    let Some(raiz) = catalogo
        .get(b"Outlines")
        .ok()
        .and_then(|o| diccionario(documento, o))
    else {
        return Vec::new();
    };
    let mut lector = LectorDeMarcadores {
        documento,
        paginas: documento
            .get_pages()
            .into_iter()
            .map(|(n, id)| (id, n))
            .collect(),
        destinos: destinos_con_nombre(documento, catalogo),
        visitados: HashSet::new(),
        entradas: Vec::new(),
    };
    lector.recorrer_hermanos(raiz, 1);
    lector.entradas
}

impl LectorDeMarcadores<'_> {
    fn recorrer_hermanos(&mut self, padre: &Dictionary, nivel: u8) {
        let mut actual = referencia(padre, b"First");
        while let Some(id) = actual {
            if self.entradas.len() >= MAX_MARCADORES || !self.visitados.insert(id) {
                return;
            }
            let Some(nodo) = self
                .documento
                .get_object(id)
                .ok()
                .and_then(|o| o.as_dict().ok())
            else {
                return;
            };
            self.entradas.push(EntradaMarcador {
                titulo: titulo_de(self.documento, nodo),
                nivel: nivel.min(MAX_NIVEL_MARCADOR),
                pagina: self.pagina_de(nodo),
            });
            if nivel < MAX_PROFUNDIDAD_MARCADORES {
                self.recorrer_hermanos(nodo, nivel + 1);
            }
            actual = referencia(nodo, b"Next");
        }
    }

    fn pagina_de(&self, nodo: &Dictionary) -> Option<u32> {
        let destino = nodo.get(b"Dest").ok().or_else(|| {
            let accion = nodo
                .get(b"A")
                .ok()
                .and_then(|a| diccionario(self.documento, a))?;
            (accion.get(b"S").ok()?.as_name().ok()? == b"GoTo")
                .then(|| accion.get(b"D").ok())
                .flatten()
        })?;
        self.resolver_destino(destino, 0)
    }

    fn resolver_destino(&self, destino: &Object, saltos: u8) -> Option<u32> {
        if saltos > MAX_REFERENCIAS_DESTINO {
            return None;
        }
        match destino {
            Object::Reference(id) => self.paginas.get(id).copied().or_else(|| {
                self.resolver_destino(self.documento.get_object(*id).ok()?, saltos + 1)
            }),
            Object::Array(partes) => match partes.first()? {
                Object::Reference(id) => self.paginas.get(id).copied(),
                Object::Integer(indice) => {
                    let pagina = u32::try_from(*indice).ok()?.checked_add(1)?;
                    self.paginas
                        .values()
                        .any(|p| *p == pagina)
                        .then_some(pagina)
                }
                _ => None,
            },
            Object::Name(nombre) | Object::String(nombre, _) => {
                self.resolver_destino(self.destinos.get(nombre)?, saltos + 1)
            }
            Object::Dictionary(entrada) => {
                self.resolver_destino(entrada.get(b"D").ok()?, saltos + 1)
            }
            _ => None,
        }
    }
}

fn referencia(nodo: &Dictionary, clave: &[u8]) -> Option<ObjectId> {
    nodo.get(clave).ok()?.as_reference().ok()
}

fn diccionario<'a>(documento: &'a Document, objeto: &'a Object) -> Option<&'a Dictionary> {
    match objeto {
        Object::Reference(id) => documento.get_object(*id).ok()?.as_dict().ok(),
        otro => otro.as_dict().ok(),
    }
}

fn titulo_de(documento: &Document, nodo: &Dictionary) -> String {
    let Ok(titulo) = nodo.get(b"Title") else {
        return String::new();
    };
    let titulo = match titulo {
        Object::Reference(id) => match documento.get_object(*id) {
            Ok(objeto) => objeto,
            Err(_) => return String::new(),
        },
        directo => directo,
    };
    decode_text_string(titulo).unwrap_or_else(|_| match titulo.as_str() {
        Ok(crudo) => String::from_utf8_lossy(crudo).to_string(),
        Err(_) => String::new(),
    })
}

/// Destinos de `/Dests` y del árbol de nombres `/Names /Dests`
fn destinos_con_nombre(documento: &Document, catalogo: &Dictionary) -> HashMap<Vec<u8>, Object> {
    let mut destinos = HashMap::new();
    if let Some(directos) = catalogo
        .get(b"Dests")
        .ok()
        .and_then(|o| diccionario(documento, o))
    {
        for (nombre, valor) in directos.iter() {
            destinos.insert(nombre.clone(), valor.clone());
        }
    }
    let arbol = catalogo
        .get(b"Names")
        .ok()
        .and_then(|o| diccionario(documento, o))
        .and_then(|nombres| nombres.get(b"Dests").ok());
    if let Some(arbol) = arbol {
        recoger_arbol_de_nombres(documento, arbol, &mut destinos, &mut HashSet::new());
    }
    destinos
}

fn recoger_arbol_de_nombres(
    documento: &Document,
    nodo: &Object,
    destinos: &mut HashMap<Vec<u8>, Object>,
    visitados: &mut HashSet<ObjectId>,
) {
    if let Object::Reference(id) = nodo
        && !visitados.insert(*id)
    {
        return;
    }
    let Some(nodo) = diccionario(documento, nodo) else {
        return;
    };
    if let Some(pares) = nodo.get(b"Names").ok().and_then(|o| lista(documento, o)) {
        for par in pares.chunks_exact(2) {
            if destinos.len() >= MAX_DESTINOS_CON_NOMBRE {
                return;
            }
            if let Ok(nombre) = par[0].as_str() {
                destinos.insert(nombre.to_vec(), par[1].clone());
            }
        }
    }
    if let Some(hijos) = nodo.get(b"Kids").ok().and_then(|o| lista(documento, o)) {
        for hijo in hijos {
            recoger_arbol_de_nombres(documento, hijo, destinos, visitados);
        }
    }
}

fn lista<'a>(documento: &'a Document, objeto: &'a Object) -> Option<&'a Vec<Object>> {
    match objeto {
        Object::Reference(id) => documento.get_object(*id).ok()?.as_array().ok(),
        otro => otro.as_array().ok(),
    }
}
