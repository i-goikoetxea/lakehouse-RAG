//! Suite adversarial de las citas de página exacta (`docs/arquitectura/ssd_citas_pagina.md` §6):
//! extractos alterados, repetidos o falsos, marcas falsas dentro del documento, chunks sin
//! saltos o con saltos corruptos, Markdown, valores frontera y entradas desmesuradas.
//! Todo el texto es sintético.

#[path = "comun_centros/mod.rs"]
mod comun_centros;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;
#[path = "comun_store_v2/mod.rs"]
mod comun_store_v2;
#[path = "comun_tools_stair/mod.rs"]
mod comun_tools_stair;

use std::time::{Duration, Instant};

use comun_tools_stair::*;
use lakehouse_rag::application::dtos::tools::ToolResult;
use lakehouse_rag::application::use_cases::arnes_turno::{
    ENCABEZADO_VERIFICACION_PAGINAS, Paginas, RegistroTurno, ResultadoCita, VerificadorPaginas,
};
use lakehouse_rag::domain::services::fragmentacion::reconstruir_con_saltos;
use lakehouse_rag::domain::services::marcas_pagina::{
    SaltoPagina, quitar_marcas, saltos_del_tramo, texto_con_marcas, tramos_paginados,
};
use serde_json::{Value, json};

const BUSCAR: &str = "search_normative_context";
const SUBARBOL: &str = "stair_get_subtree_context";
const FRASE: &str = "El inspector de turno firma el acta de recepción del lote de válvulas";

fn salto(desplazamiento: usize, pagina: u32) -> SaltoPagina {
    SaltoPagina {
        desplazamiento,
        pagina,
    }
}

fn paginas(lista: &[u32]) -> Paginas {
    lista.iter().copied().collect()
}

fn registro(herramienta: &str, textos: &[&str]) -> RegistroTurno {
    let resultados: Vec<Value> = textos.iter().map(|t| json!({ "texto": t })).collect();
    let mut registro = RegistroTurno::default();
    registro.registrar(ToolResult {
        call_id: "c".into(),
        tool_name: herramienta.into(),
        output: json!({ "grupos": [{ "resultados": resultados }] }),
        is_error: false,
    });
    registro
}

fn busqueda(textos: &[&str]) -> RegistroTurno {
    registro(BUSCAR, textos)
}

fn resultado_unico(respuesta: &str, registro: &RegistroTurno) -> ResultadoCita {
    let revisadas = VerificadorPaginas::revisar(respuesta, registro);
    assert_eq!(revisadas.len(), 1, "{revisadas:?}");
    revisadas[0].resultado.clone()
}

// --- Extractos alterados ---

#[test]
fn test_adv_espacios_y_guiones_partidos_no_impiden_confirmar_la_pagina() {
    let fuente = "⟦p. 40⟧ Otra cosa.\n⟦p. 41⟧ El   inspec-\ntor de tur no firma el ac\u{00AD}ta de recep-\n ción del lote de vál vulas.";
    let respuesta = format!("«{FRASE}» (p. 41)");
    assert_eq!(
        resultado_unico(&respuesta, &busqueda(&[fuente])),
        ResultadoCita::Confirmada
    );
    // Y al revés: el modelo copia el guion partido y la fuente no lo tiene
    let fuente = format!("⟦p. 41⟧ {FRASE}.");
    let partido =
        "«El inspec- tor de turno firma el acta de recep-\nción del lote de válvulas» (p. 41)";
    assert_eq!(
        resultado_unico(partido, &busqueda(&[&fuente])),
        ResultadoCita::Confirmada
    );
}

#[test]
fn test_adv_comillas_tipograficas_ligaduras_tildes_y_mayusculas() {
    let fuente = "⟦p. 7⟧ El “inspector” de turno ﬁrma el acta ‘oﬁcial’ de recepción — lote Nº 12 — sin más trámite.";
    for respuesta in [
        "«El \"inspector\" de turno firma el acta 'oficial' de recepción - lote Nº 12 - sin más trámite» (p. 7)",
        "“EL INSPECTOR DE TURNO FIRMA EL ACTA OFICIAL DE RECEPCION, LOTE Nº 12, SIN MAS TRAMITE” (pág. 7)",
        "\"el inspector de turno firma el acta oficial de recepción lote nº 12 sin más trámite\" [página 7]",
    ] {
        assert_eq!(
            resultado_unico(respuesta, &busqueda(&[fuente])),
            ResultadoCita::Confirmada,
            "{respuesta}"
        );
    }
}

// --- Ambigüedad ---

#[test]
fn test_adv_extracto_repetido_en_dos_paginas_es_ambiguo_y_se_dice() {
    let fuente = format!("⟦p. 12⟧ {FRASE}.\n⟦p. 13⟧ {FRASE}.");
    let registro = busqueda(&[&fuente]);
    for citada in ["p. 12", "p. 13", "pp. 12-13"] {
        let revisadas = VerificadorPaginas::revisar(&format!("«{FRASE}» ({citada})"), &registro);
        assert_eq!(
            revisadas[0].resultado,
            ResultadoCita::Ambigua {
                apariciones: vec![paginas(&[12]), paginas(&[13])]
            },
            "{citada}: no se elige una"
        );
        let nota = VerificadorPaginas::nota(&revisadas).unwrap();
        assert!(nota.contains("p. 12") && nota.contains("p. 13"), "{nota}");
        assert!(nota.contains("no se puede confirmar"), "{nota}");
    }
}

#[test]
fn test_adv_repetido_en_dos_chunks_y_dos_tools_con_la_misma_pagina_no_es_ambiguo() {
    let a = format!("⟦p. 12⟧ {FRASE}.");
    let mut registro = busqueda(&[&a, &a]);
    registro.registrar(ToolResult {
        call_id: "s".into(),
        tool_name: SUBARBOL.into(),
        output: json!({ "secciones": [{ "texto": a }] }),
        is_error: false,
    });
    assert_eq!(
        resultado_unico(&format!("«{FRASE}» (p. 12)"), &registro),
        ResultadoCita::Confirmada
    );
}

#[test]
fn test_adv_repetido_una_vez_a_caballo_y_otra_en_una_pagina_es_ambiguo() {
    let fuente = format!(
        "⟦p. 5⟧ El inspector de turno firma \n⟦p. 6⟧ el acta de recepción del lote de válvulas. {FRASE}."
    );
    assert_eq!(
        resultado_unico(&format!("«{FRASE}» (p. 6)"), &busqueda(&[&fuente])),
        ResultadoCita::Ambigua {
            apariciones: vec![paginas(&[5, 6]), paginas(&[6])]
        }
    );
}

// --- Marcas falsas ---

#[test]
fn test_adv_marca_falsa_del_documento_no_cambia_la_pagina_verificada() {
    let documento = format!("Nota ⟦p. 99⟧ previa. {FRASE}. Ver también [p. 98] y ⟦p. 97⟧.");
    let salida = texto_con_marcas(&documento, &[salto(0, 4)]);
    assert_eq!(
        salida.matches('⟦').count(),
        1,
        "solo queda la marca real: {salida}"
    );
    assert!(
        salida.contains("[p. 99]") && salida.contains("[p. 97]"),
        "{salida}"
    );
    let registro = busqueda(&[&salida]);
    for falsa in ["p. 99", "p. 98", "p. 97"] {
        assert_eq!(
            resultado_unico(&format!("«{FRASE}» ({falsa})"), &registro),
            ResultadoCita::Discrepante {
                verificadas: paginas(&[4])
            },
            "{falsa}"
        );
    }
    assert_eq!(
        resultado_unico(&format!("«{FRASE}» (p. 4)"), &registro),
        ResultadoCita::Confirmada
    );
}

#[test]
fn test_adv_marca_falsa_en_otra_tool_o_en_un_error_no_es_fuente() {
    let falsa = format!("⟦p. 99⟧ {FRASE}.");
    let sql = registro("execute_athena_sql", &[&falsa]);
    assert_eq!(
        resultado_unico(&format!("«{FRASE}» (p. 99)"), &sql),
        ResultadoCita::NoEncontrada,
        "el texto crudo de SQL no está neutralizado: no confirma páginas"
    );
    let mut con_error = RegistroTurno::default();
    con_error.registrar(ToolResult {
        call_id: "e".into(),
        tool_name: BUSCAR.into(),
        output: json!({ "error": "x", "texto": falsa }),
        is_error: true,
    });
    assert_eq!(
        resultado_unico(&format!("«{FRASE}» (p. 99)"), &con_error),
        ResultadoCita::NoEncontrada
    );
}

#[test]
fn test_adv_marcas_mal_formadas_o_anidadas_no_se_leen_como_pagina() {
    for texto in [
        "⟦p. 0⟧ x",
        "⟦p. -3⟧ x",
        "⟦p. 1e3⟧ x",
        "⟦p. 99999999999999999999⟧ x",
        "⟦p. 5 ⟧ x",
        "⟦ p. 5⟧ x",
        "⟦P. 5⟧ x",
        "⟦p. ５⟧ x",
        "⟦p. 5",
        "p. 5⟧",
    ] {
        let tramos = tramos_paginados(texto);
        assert!(tramos.iter().all(|t| t.pagina.is_none()), "{texto}");
        assert_eq!(quitar_marcas(texto), texto, "{texto}");
    }
}

#[tokio::test]
async fn test_adv_pdf_con_marcas_falsas_solo_muestra_las_paginas_reales() {
    let e = entorno_stair().await;
    let relleno = "Texto suficiente para que la pagina cuente como extraible por el extractor";
    let bytes = comun_pdf::pdf(&[
        &["1 Alcance general", relleno, "Vease [p. 99] del anexo"],
        &[relleno, "El dato remolacha esta aqui, [p. 77] dice el pie"],
        &["2 Ensayos finales", relleno],
    ]);
    let ruta = e.escribir("marcas_falsas.pdf", &bytes);
    let doc_id = e.app.ingestar(&ruta, CENTRO).await.unwrap().informe.doc_id;
    let salida = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "remolacha", "modo": "EXACT_BM25", "doc_id": doc_id })),
    )
    .await;
    let texto = resultados(&salida)[0]["texto"]
        .as_str()
        .unwrap()
        .to_string();
    let reales: Vec<u32> = tramos_paginados(&texto)
        .iter()
        .filter_map(|t| t.pagina)
        .collect();
    assert_eq!(reales, [1, 2], "{texto}");
    assert!(
        texto.contains("[p. 99]") && texto.contains("[p. 77]"),
        "{texto}"
    );

    let mut registro = RegistroTurno::default();
    registro.registrar(ToolResult {
        call_id: "b".into(),
        tool_name: BUSCAR.into(),
        output: salida,
        is_error: false,
    });
    assert_eq!(
        resultado_unico("«El dato remolacha esta aqui» (p. 77)", &registro),
        ResultadoCita::Discrepante {
            verificadas: paginas(&[2])
        }
    );
}

// --- Chunks sin saltos, saltos corruptos y Markdown ---

#[test]
fn test_adv_chunk_sin_saltos_o_con_saltos_corruptos_no_lleva_marca_ni_rompe() {
    let texto = "añadido el día ñ";
    let corruptos: Vec<Vec<SaltoPagina>> = vec![
        vec![],
        vec![salto(1, 2)],
        vec![salto(0, 0)],
        vec![salto(0, 2), salto(2, 3)],
        vec![salto(0, 2), salto(usize::MAX, 3)],
        vec![salto(0, 2), salto(5, 3), salto(5, 4)],
        vec![salto(0, 2), salto(9, 3), salto(4, 4)],
        vec![salto(0, 2), salto(texto.len(), 3)],
    ];
    for saltos in corruptos {
        assert_eq!(texto_con_marcas(texto, &saltos), texto, "{saltos:?}");
        // El tramo de unos saltos corruptos tampoco provoca un pánico
        let _ = saltos_del_tramo(&saltos, 3, 9);
        let (unido, unidos) = reconstruir_con_saltos(&[(texto, Some(&saltos)), ("otro", None)]);
        assert!(unido.starts_with(texto));
        assert_eq!(unidos, None);
    }
    assert_eq!(texto_con_marcas("", &[salto(0, 1)]), "⟦p. 1⟧ ");
}

#[test]
fn test_adv_cita_sobre_texto_sin_paginas_se_declara_no_comprobable() {
    let registro = busqueda(&[&format!("{FRASE}.")]);
    let revisadas = VerificadorPaginas::revisar(&format!("«{FRASE}» (p. 3)"), &registro);
    assert_eq!(revisadas[0].resultado, ResultadoCita::SinPagina);
    let nota = VerificadorPaginas::nota(&revisadas).unwrap();
    assert!(
        nota.starts_with(&format!("\n\n{ENCABEZADO_VERIFICACION_PAGINAS}")),
        "{nota}"
    );
    assert!(nota.contains("no se puede comprobar"), "{nota}");
}

#[tokio::test]
async fn test_adv_markdown_real_nunca_muestra_marcas_ni_confirma_paginas() {
    let e = entorno_stair().await;
    let salida = ok(
        &e,
        BUSCAR,
        args(json!({ "consulta": "durmientes de madera tratada", "modo": "EXACT_BM25" })),
    )
    .await;
    assert!(!salida["grupos"].to_string().contains('⟦'));
    let capitulo_custodia = capitulo(&e, "Custodia").await;
    let seccion = ok(
        &e,
        SUBARBOL,
        args(json!({ "doc_id": DOC, "nodo_id": capitulo_custodia })),
    )
    .await;
    assert!(!seccion["secciones"].to_string().contains('⟦'));

    let mut registro = RegistroTurno::default();
    for (nombre, output) in [(BUSCAR, salida), (SUBARBOL, seccion)] {
        registro.registrar(ToolResult {
            call_id: nombre.into(),
            tool_name: nombre.into(),
            output,
            is_error: false,
        });
    }
    assert_eq!(
        resultado_unico(&format!("«{FRASE_ACOPIO}» (p. 2)"), &registro),
        ResultadoCita::SinPagina
    );
}

#[tokio::test]
async fn test_adv_subarbol_paginado_por_caracteres_no_pierde_texto_ni_marcas() {
    let e = entorno_stair_con(
        lakehouse_rag::application::use_cases::lakehouse_tool_service::ConfiguracionHerramientas {
            max_caracteres_subarbol: 400,
            ..Default::default()
        },
    )
    .await;
    let linea =
        |n: usize| format!("Línea número {n} con eñes, tildes y años: verificación técnica");
    let paginas_pdf: Vec<Vec<String>> = (0..4)
        .map(|p| (0..6).map(|l| linea(p * 10 + l)).collect())
        .collect();
    let referencias: Vec<Vec<&str>> = paginas_pdf
        .iter()
        .map(|p| p.iter().map(String::as_str).collect())
        .collect();
    let paginas_ref: Vec<&[&str]> = referencias.iter().map(Vec::as_slice).collect();
    let ruta = e.escribir("largo.pdf", &comun_pdf::pdf(&paginas_ref));
    let doc_id = e.app.ingestar(&ruta, CENTRO).await.unwrap().informe.doc_id;
    let indice = ok(&e, "stair_navigate_toc", args(json!({ "doc_id": doc_id }))).await;
    let nodo = indice["nodos"][0]["nodo_id"].as_str().unwrap().to_string();

    let mut desde = 0u64;
    let mut reunido = String::new();
    let mut vistas = Vec::new();
    for _ in 0..200 {
        let parte = ok(
            &e,
            SUBARBOL,
            args(json!({ "doc_id": doc_id, "nodo_id": nodo, "desde_caracter": desde })),
        )
        .await;
        for seccion in parte["secciones"].as_array().unwrap() {
            let texto = seccion["texto"].as_str().unwrap();
            if texto.is_empty() {
                continue;
            }
            assert!(
                texto.starts_with("⟦p. "),
                "cada tramo empieza con su página: {texto}"
            );
            for tramo in tramos_paginados(texto) {
                vistas.push(tramo.pagina.unwrap());
                reunido.push_str(tramo.texto);
            }
        }
        if parte["truncado"] != true {
            break;
        }
        desde = parte["como_continuar"]["siguiente_desde_caracter"]
            .as_u64()
            .unwrap();
    }
    for (pagina, lineas) in paginas_pdf.iter().enumerate() {
        for linea in lineas {
            assert!(
                reunido.contains(linea.as_str()),
                "falta «{linea}» (p. {})",
                pagina + 1
            );
        }
    }
    assert!(vistas.windows(2).all(|par| par[0] <= par[1]), "{vistas:?}");
    assert_eq!(vistas.first(), Some(&1));
    assert_eq!(vistas.last(), Some(&4));
}

// --- Valores frontera de la referencia de página ---

#[test]
fn test_adv_referencias_de_pagina_frontera() {
    let fuente = format!("⟦p. 12⟧ {FRASE}.");
    let registro = busqueda(&[&fuente]);
    // No son referencias: el extracto no se revisa
    for sin_referencia in [
        format!("«{FRASE}» (p. 0)"),
        format!("«{FRASE}» (p. 99999999999)"),
        format!("«{FRASE}» (cap. 12)"),
        format!("«{FRASE}» (grupo 12)"),
        format!("«{FRASE}» (pp. )"),
        format!("«{FRASE}»\n(p. 12)"),
        format!("«{FRASE}» {} (p. 12)", "x".repeat(90)),
    ] {
        assert!(
            VerificadorPaginas::revisar(&sin_referencia, &registro).is_empty(),
            "{sin_referencia}"
        );
    }
    // Rango invertido o desmesurado: cuentan los extremos y no se expande
    let invertido = VerificadorPaginas::revisar(&format!("«{FRASE}» (pp. 14-12)"), &registro);
    assert_eq!(invertido[0].citadas, paginas(&[12, 14]));
    let enorme = VerificadorPaginas::revisar(&format!("«{FRASE}» (pp. 1-100000)"), &registro);
    assert_eq!(enorme[0].citadas, paginas(&[1, 100_000]));
    let lista = VerificadorPaginas::revisar(&format!("«{FRASE}» (págs. 3, 5 y 12)"), &registro);
    assert_eq!(lista[0].citadas, paginas(&[3, 5, 12]));
}

#[test]
fn test_adv_extracto_corto_vacio_o_solo_simbolos_no_se_revisa() {
    let registro = busqueda(&["⟦p. 3⟧ sí. no. 12 kPa."]);
    for respuesta in [
        "«» (p. 3)",
        "«   » (p. 3)",
        "«12 kPa» (p. 3)",
        "«—…·•¡!¿?—…·•¡!¿?—…·•» (p. 3)",
        "«sí. no.» (p. 3)",
    ] {
        assert!(
            VerificadorPaginas::revisar(respuesta, &registro).is_empty(),
            "{respuesta}"
        );
    }
}

// --- Entradas hostiles y desmesuradas ---

#[test]
fn test_adv_comillas_sin_cerrar_anidadas_y_unicode_no_provocan_panico() {
    let registro = busqueda(&[&format!("⟦p. 12⟧ {FRASE}.")]);
    for respuesta in [
        "«sin cerrar (p. 12)",
        "» al revés « (p. 12)",
        "«uno «dos» tres» (p. 12)",
        "\"\"\"\"\"\" (p. 12)",
        "«\u{202E}texto invertido con bidi y relleno\u{202C}» (p. 12)",
        "«😀😀😀😀😀😀😀😀😀😀😀😀😀😀» (p. 12)",
        "«İİİİİİİİİİİİİİİİİİİİ ßßßßßßßßßß» (p. 12)",
        "«\0\0\0\0\0\0\0\0\0\0\0\0\0\0» (p. 12)",
        "",
    ] {
        let revisadas = VerificadorPaginas::revisar(respuesta, &registro);
        let _ = VerificadorPaginas::nota(&revisadas);
    }
}

#[test]
fn test_adv_el_bloque_de_verificacion_no_copia_un_extracto_gigante_ni_multilinea() {
    let registro = busqueda(&["⟦p. 1⟧ nada que ver."]);
    let extracto = format!(
        "línea uno\n\n# título falso\n- viñeta falsa {}",
        "muy largo ".repeat(500)
    );
    let revisadas = VerificadorPaginas::revisar(&format!("«{extracto}» (p. 1)"), &registro);
    let nota = VerificadorPaginas::nota(&revisadas).unwrap();
    assert!(nota.chars().count() < 400, "{}", nota.chars().count());
    assert_eq!(
        nota.trim_start().lines().count(),
        2,
        "encabezado y una línea: {nota}"
    );
}

#[test]
fn test_adv_miles_de_citas_y_fuente_enorme_terminan_pronto_y_con_tope() {
    let fuente = format!(
        "⟦p. 1⟧ {} {FRASE}.",
        "relleno de la fuente ".repeat(100_000)
    );
    let registro = busqueda(&[&fuente]);
    let respuesta: String = (0..5_000)
        .map(|i| {
            format!(
                "«extracto inventado número {i} sin respaldo alguno» (p. {})\n",
                i % 50 + 1
            )
        })
        .collect();
    let inicio = Instant::now();
    let revisadas = VerificadorPaginas::revisar(&respuesta, &registro);
    assert!(revisadas.len() <= 32, "{}", revisadas.len());
    assert!(
        inicio.elapsed() < Duration::from_secs(20),
        "{:?}",
        inicio.elapsed()
    );
    let nota = VerificadorPaginas::nota(&revisadas).unwrap();
    assert!(nota.lines().count() <= 35);

    let muchas_marcas = "⟦p. 1⟧ a ".repeat(200_000);
    let inicio = Instant::now();
    assert_eq!(tramos_paginados(&muchas_marcas).len(), 200_000);
    assert!(inicio.elapsed() < Duration::from_secs(10));
}

#[test]
fn test_adv_la_respuesta_no_puede_fabricar_su_propia_fuente() {
    // El extracto aparece en la respuesta y en la pregunta, pero no en ninguna tool del turno
    let registro = RegistroTurno::default();
    assert_eq!(
        resultado_unico(
            &format!("⟦p. 9⟧ {FRASE}. Lo dice «{FRASE}» (p. 9)"),
            &registro
        ),
        ResultadoCita::NoEncontrada
    );
}

#[test]
fn test_adv_titulo_entre_comillas_no_se_anota_pero_el_texto_crudo_de_sql_si() {
    let mut registro = RegistroTurno::default();
    registro.registrar(ToolResult {
        call_id: "t".into(),
        tool_name: "stair_navigate_toc".into(),
        output: json!({ "nodos": [{ "titulo_seccion": "Recepción de válvulas y accesorios" }] }),
        is_error: false,
    });
    registro.registrar(ToolResult {
        call_id: "q".into(),
        tool_name: "execute_athena_sql".into(),
        output: json!({ "resultado": [{ "texto_contenido": format!("{FRASE}.") }] }),
        is_error: false,
    });
    assert!(
        VerificadorPaginas::revisar(
            "Apartado «Recepción de válvulas y accesorios» (p. 4)",
            &registro
        )
        .is_empty(),
        "un título no es un extracto"
    );
    assert_eq!(
        resultado_unico(&format!("«{FRASE}» (p. 4)"), &registro),
        ResultadoCita::NoEncontrada,
        "el texto del documento leído por SQL no tiene página que confirmar"
    );
}
