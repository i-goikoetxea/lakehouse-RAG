//! PDF sintéticos generados con lopdf: sin datos reales ni fixtures binarios.
#![allow(dead_code)]

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

/// Marcador con su página de destino (base 1) y sus hijos
pub struct Marcador {
    pub titulo: &'static str,
    pub pagina: u32,
    pub hijos: Vec<Marcador>,
}

pub fn marcador(titulo: &'static str, pagina: u32, hijos: Vec<Marcador>) -> Marcador {
    Marcador {
        titulo,
        pagina,
        hijos,
    }
}

/// Bytes en Latin-1, que Helvetica con `WinAnsiEncoding` muestra correctamente
fn latin1(linea: &str) -> Vec<u8> {
    linea
        .chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

fn contenido_de_pagina(lineas: &[&str]) -> Vec<u8> {
    let mut operaciones = vec![
        Operation::new("BT", vec![]),
        Operation::new("TL", vec![14.into()]),
        Operation::new("Tf", vec!["F1".into(), 10.into()]),
        Operation::new("Td", vec![50.into(), 780.into()]),
    ];
    for linea in lineas {
        operaciones.push(Operation::new(
            "Tj",
            vec![Object::string_literal(latin1(linea))],
        ));
        operaciones.push(Operation::new("T*", vec![]));
    }
    operaciones.push(Operation::new("ET", vec![]));
    Content {
        operations: operaciones,
    }
    .encode()
    .unwrap()
}

/// Documento con una lista de líneas por página
pub fn documento(paginas: &[&[&str]]) -> Document {
    let mut doc = Document::with_version("1.5");
    let id_paginas = doc.new_object_id();
    let id_fuente = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let recursos = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => id_fuente },
    });
    let hijos: Vec<Object> = paginas
        .iter()
        .map(|lineas| {
            let contenido =
                doc.add_object(Stream::new(Dictionary::new(), contenido_de_pagina(lineas)));
            doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => id_paginas,
                "Contents" => contenido,
                "Resources" => recursos,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            })
            .into()
        })
        .collect();
    let total = hijos.len() as i64;
    doc.objects.insert(
        id_paginas,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => hijos,
            "Count" => total,
        }),
    );
    let catalogo = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => id_paginas,
    });
    doc.trailer.set("Root", catalogo);
    doc
}

pub fn bytes_de(mut doc: Document) -> Vec<u8> {
    let mut salida = Vec::new();
    doc.save_to(&mut salida).unwrap();
    salida
}

pub fn pdf(paginas: &[&[&str]]) -> Vec<u8> {
    bytes_de(documento(paginas))
}

fn id_de_pagina(doc: &Document, pagina: u32) -> ObjectId {
    doc.get_pages()[&pagina]
}

/// Asigna `/Outlines` al catálogo
pub fn fijar_raiz_de_marcadores(doc: &mut Document, raiz: ObjectId) {
    let id_catalogo = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_object_mut(id_catalogo)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Outlines", raiz);
}

fn escribir_hermanos(
    doc: &mut Document,
    marcadores: &[Marcador],
    padre: ObjectId,
) -> Vec<ObjectId> {
    let ids: Vec<ObjectId> = marcadores.iter().map(|_| doc.new_object_id()).collect();
    for (indice, marcador) in marcadores.iter().enumerate() {
        let pagina = id_de_pagina(doc, marcador.pagina);
        let mut nodo = dictionary! {
            "Title" => Object::string_literal(latin1(marcador.titulo)),
            "Parent" => padre,
            "Dest" => vec![pagina.into(), "Fit".into()],
        };
        if indice > 0 {
            nodo.set("Prev", ids[indice - 1]);
        }
        if let Some(siguiente) = ids.get(indice + 1) {
            nodo.set("Next", *siguiente);
        }
        let hijos = escribir_hermanos(doc, &marcador.hijos, ids[indice]);
        if let (Some(primero), Some(ultimo)) = (hijos.first(), hijos.last()) {
            nodo.set("First", *primero);
            nodo.set("Last", *ultimo);
            nodo.set("Count", hijos.len() as i64);
        }
        doc.objects.insert(ids[indice], Object::Dictionary(nodo));
    }
    ids
}

pub fn con_marcadores(paginas: &[&[&str]], marcadores: &[Marcador]) -> Vec<u8> {
    let mut doc = documento(paginas);
    let raiz = doc.new_object_id();
    let hermanos = escribir_hermanos(&mut doc, marcadores, raiz);
    doc.objects.insert(
        raiz,
        Object::Dictionary(dictionary! {
            "Type" => "Outlines",
            "First" => hermanos[0],
            "Last" => *hermanos.last().unwrap(),
            "Count" => hermanos.len() as i64,
        }),
    );
    fijar_raiz_de_marcadores(&mut doc, raiz);
    bytes_de(doc)
}

/// Palabras únicas de la entrada para comprobar cobertura sin pérdidas
pub fn palabras(texto: &str) -> Vec<String> {
    texto
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}
