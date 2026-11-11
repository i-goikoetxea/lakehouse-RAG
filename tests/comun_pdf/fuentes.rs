//! PDF sintéticos con codificaciones de fuente: `/Differences` y `ToUnicode`.
//! Una sola página, Helvetica sin incrustar y los códigos de carácter tal cual.

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, Stream, dictionary};

fn pagina_con_codigos(doc: &mut Document, fuente: Object, lineas: &[&[u8]]) -> Vec<u8> {
    let id_paginas = doc.new_object_id();
    let id_fuente = doc.add_object(fuente);
    let recursos = doc.add_object(dictionary! { "Font" => dictionary! { "F1" => id_fuente } });
    let mut operaciones = vec![
        Operation::new("BT", vec![]),
        Operation::new("TL", vec![14.into()]),
        Operation::new("Tf", vec!["F1".into(), 10.into()]),
        Operation::new("Td", vec![50.into(), 780.into()]),
    ];
    for linea in lineas {
        operaciones.push(Operation::new(
            "Tj",
            vec![Object::String(
                linea.to_vec(),
                lopdf::StringFormat::Hexadecimal,
            )],
        ));
        operaciones.push(Operation::new("T*", vec![]));
    }
    operaciones.push(Operation::new("ET", vec![]));
    let contenido = doc.add_object(Stream::new(
        Dictionary::new(),
        Content {
            operations: operaciones,
        }
        .encode()
        .unwrap(),
    ));
    let pagina = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => id_paginas,
        "Contents" => contenido,
        "Resources" => recursos,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    doc.objects.insert(
        id_paginas,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![pagina.into()],
            "Count" => 1,
        }),
    );
    let catalogo = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => id_paginas });
    doc.trailer.set("Root", catalogo);
    let mut salida = Vec::new();
    doc.save_to(&mut salida).unwrap();
    salida
}

/// Helvetica con `WinAnsiEncoding` de base y `/Differences` que renombra códigos
pub fn pdf_con_diferencias(diferencias: Vec<Object>, lineas: &[&[u8]]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let fuente = Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "BaseEncoding" => "WinAnsiEncoding",
            "Differences" => diferencias,
        },
    });
    pagina_con_codigos(&mut doc, fuente, lineas)
}

/// Helvetica con un `ToUnicode` que asigna a cada código de un byte el carácter indicado
pub fn pdf_con_to_unicode(asignaciones: &[(u8, char)], lineas: &[&[u8]]) -> Vec<u8> {
    let mut doc = Document::with_version("1.5");
    let pares: String = asignaciones
        .iter()
        .map(|(codigo, caracter)| {
            let mut utf16 = [0u16; 2];
            let unidades: String = caracter
                .encode_utf16(&mut utf16)
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect();
            format!("<{codigo:02X}> <{unidades}>\n")
        })
        .collect();
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
         {} beginbfchar\n{pares}endbfchar\nendcmap\n\
         CMapName currentdict /CMap defineresource pop\nend\nend\n",
        asignaciones.len()
    );
    let to_unicode = doc.add_object(Stream::new(Dictionary::new(), cmap.into_bytes()));
    let fuente = Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
        "ToUnicode" => to_unicode,
    });
    pagina_con_codigos(&mut doc, fuente, lineas)
}
