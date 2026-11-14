//! Suite adversarial del anclaje de tablas por ventana (`ssd_generalidad_4_piezas.md` §2.7):
//! tamaño, claves vacías, valores enormes, saltos corruptos, coste lineal y páginas falsas.

use std::time::{Duration, Instant};

use lakehouse_rag::domain::services::anclaje_tabla::{
    FilaParaAnclar, LineaPaginada, TablaParaAnclar, anclar_tabla,
};

fn fila(clave: &str, valores: &[String]) -> FilaParaAnclar {
    FilaParaAnclar {
        clave: vec![clave.to_string()],
        valores: valores.to_vec(),
    }
}

/// Texto de `paginas` páginas de relleno con números que se repiten en todas
fn relleno(paginas: u32) -> Vec<(u32, String)> {
    (1..=paginas)
        .flat_map(|pagina| {
            (0..40).map(move |linea| {
                (
                    pagina,
                    format!("Párrafo {linea} con cifras 10 20 30 {pagina} y texto corriente"),
                )
            })
        })
        .collect()
}

fn lineas(texto: &[(u32, String)]) -> Vec<LineaPaginada<'_>> {
    texto
        .iter()
        .map(|(pagina, linea)| LineaPaginada {
            pagina: *pagina,
            texto: linea,
        })
        .collect()
}

#[test]
fn test_5000_celdas_en_1000_paginas_se_anclan_en_tiempo_lineal_y_sin_paginas_falsas() {
    let mut texto = relleno(1000);
    let columnas: Vec<String> = (0..50).map(|c| format!("Col{c}")).collect();
    texto.push((500, columnas.join(" ")));
    let mut filas = Vec::new();
    for f in 0..100 {
        let valores: Vec<String> = (0..50).map(|c| format!("{}", f * 1000 + c)).collect();
        let pagina = 500 + (f / 50) as u32;
        texto.push((pagina, format!("Fila{f} {}", valores.join(" "))));
        filas.push(fila(&format!("Fila{f}"), &valores));
    }
    texto.sort_by_key(|(pagina, _)| *pagina);
    let tabla = TablaParaAnclar {
        cabeceras: columnas,
        filas,
    };
    let inicio = Instant::now();
    let anclaje = anclar_tabla(&lineas(&texto), &tabla);
    assert!(
        inicio.elapsed() < Duration::from_secs(20),
        "{:?}",
        inicio.elapsed()
    );
    assert_eq!(anclaje.bloque, Some((500, 501)));
    for (f, pagina) in anclaje.por_fila.iter().enumerate() {
        let correcta = 500 + (f / 50) as u32;
        assert!(
            pagina.is_none() || *pagina == Some(correcta),
            "fila {f}: {pagina:?}"
        );
    }
    let con_pagina = anclaje.por_fila.iter().flatten().count();
    assert!(con_pagina >= 90, "cobertura {con_pagina}/100");
}

#[test]
fn test_claves_vacias_y_de_solo_simbolos_nunca_anclan() {
    let texto = vec![(1, "— — —".to_string()), (1, "* * *".to_string())];
    let tabla = TablaParaAnclar {
        cabeceras: vec![String::new(), "—".to_string()],
        filas: vec![
            fila("", &[]),
            fila("—", &["*".to_string()]),
            fila("   ", &[]),
        ],
    };
    let anclaje = anclar_tabla(&lineas(&texto), &tabla);
    assert_eq!(anclaje.bloque, None);
    assert_eq!(anclaje.por_fila, vec![None, None, None]);
}

#[test]
fn test_valores_de_10000_caracteres_no_rompen_ni_anclan_por_casualidad() {
    let enorme = "9".repeat(10_000);
    let ausente = "x".repeat(10_000);
    let texto = vec![
        (4, "Zona Valor".to_string()),
        (4, format!("Norte {enorme} 1")),
        (4, format!("Sur {enorme} 2")),
        (4, format!("Este {enorme}")),
    ];
    let tabla = TablaParaAnclar {
        cabeceras: vec!["Valor".to_string()],
        filas: vec![
            fila("Norte", &[enorme.clone(), "1".to_string()]),
            fila("Sur", &[enorme.clone(), "2".to_string()]),
            // Uno de sus dos valores (10 000 caracteres) no está en el texto: no es su fila
            fila("Este", &[enorme.clone(), ausente]),
        ],
    };
    let anclaje = anclar_tabla(&lineas(&texto), &tabla);
    assert_eq!(anclaje.bloque, Some((4, 4)));
    assert_eq!(anclaje.por_fila, vec![Some(4), Some(4), None]);
}

#[test]
fn test_saltos_corruptos_paginas_desordenadas_y_extremas_no_dan_pagina_falsa() {
    // Páginas que retroceden y valores frontera de u32
    let texto = vec![
        (9, "Clase Mín Máx".to_string()),
        (3, "A1 1 2".to_string()),
        (u32::MAX, "A2 3 4".to_string()),
        (9, "A3 5 6".to_string()),
        (0, "A4 7 8".to_string()),
    ];
    let tabla = TablaParaAnclar {
        cabeceras: vec!["Mín".to_string(), "Máx".to_string()],
        filas: (1..=4)
            .map(|i| {
                fila(
                    &format!("A{i}"),
                    &[(2 * i - 1).to_string(), (2 * i).to_string()],
                )
            })
            .collect(),
    };
    let anclaje = anclar_tabla(&lineas(&texto), &tabla);
    let reales = [3, u32::MAX, 9, 0];
    for (pagina, real) in anclaje.por_fila.iter().zip(reales) {
        assert!(pagina.is_none() || *pagina == Some(real), "{anclaje:?}");
    }
}

#[test]
fn test_tabla_repetida_en_dos_anexos_lejanos_queda_sin_bloque() {
    let mut texto = relleno(30);
    for pagina in [5, 25] {
        texto.push((pagina, "Clase Invierno Verano".to_string()));
        for (clase, a, b) in [("Norte", 5, 18), ("Centro", 8, 24), ("Sur", 12, 30)] {
            texto.push((pagina, format!("{clase} {a} {b}")));
        }
    }
    texto.sort_by_key(|(pagina, _)| *pagina);
    let tabla = TablaParaAnclar {
        cabeceras: vec!["Invierno".to_string(), "Verano".to_string()],
        filas: [("Norte", 5, 18), ("Centro", 8, 24), ("Sur", 12, 30)]
            .iter()
            .map(|(c, a, b)| fila(c, &[a.to_string(), b.to_string()]))
            .collect(),
    };
    let anclaje = anclar_tabla(&lineas(&texto), &tabla);
    assert_eq!(anclaje.bloque, None);
}

#[test]
fn test_cabeceras_solo_de_conectores_o_simbolos_no_cuentan_como_compuestas() {
    // Sin tokens no triviales, una cabecera no puede estar «presente» por composición. Sus letras
    // están en la línea previa, pero no contiguas: solo la composición podría contarlas
    let texto: Vec<(u32, String)> = vec![
        (3, "e y u o".to_string()),
        (3, "Lote 1 10 11".to_string()),
        (3, "Lote 2 20 21".to_string()),
        (3, "Lote 3 30 31".to_string()),
    ];
    let tabla = TablaParaAnclar {
        cabeceras: vec!["y e".into(), "o u".into(), "— ·".into(), String::new()],
        filas: (1..=3)
            .map(|n| fila(&format!("Lote {n}"), &[format!("{n}0"), format!("{n}1")]))
            .collect(),
    };
    assert_eq!(anclar_tabla(&lineas(&texto), &tabla).bloque, None);
}

#[test]
fn test_2000_cabeceras_compuestas_en_1000_paginas_terminan_pronto() {
    let mut texto = relleno(1000);
    let grupos: Vec<String> = (0..40).map(|g| format!("Grupo{g}")).collect();
    let subcolumnas: Vec<String> = (0..50).map(|s| format!("Sub{s}")).collect();
    texto.push((700, grupos.join(" ")));
    texto.push((700, subcolumnas.join(" ")));
    let mut filas = Vec::new();
    for f in 0..60 {
        let valores: Vec<String> = (0..4).map(|c| format!("{}", f * 1000 + c)).collect();
        texto.push((
            700 + (f / 30) as u32,
            format!("Fila{f} {}", valores.join(" ")),
        ));
        filas.push(fila(&format!("Fila{f}"), &valores));
    }
    texto.sort_by_key(|(pagina, _)| *pagina);
    let cabeceras = grupos
        .iter()
        .flat_map(|g| subcolumnas.iter().map(move |s| format!("{g} {s}")))
        .collect();
    let tabla = TablaParaAnclar { cabeceras, filas };
    let inicio = Instant::now();
    let anclaje = anclar_tabla(&lineas(&texto), &tabla);
    assert!(
        inicio.elapsed() < Duration::from_secs(3),
        "{:?}",
        inicio.elapsed()
    );
    assert_eq!(anclaje.bloque, Some((700, 701)));
    for (f, pagina) in anclaje.por_fila.iter().enumerate() {
        assert!(pagina.is_none() || *pagina == Some(700 + (f / 30) as u32));
    }
}
