//! Página de origen y contexto del apartado en las salidas de Capa 2
//! (`docs/arquitectura/ssd_capa2_paginas_contexto.md`). PDF sintéticos de varias páginas, en
//! dominios e idiomas distintos, ingeridos de verdad (parser, LanceDB, proyección) con un
//! destilador doble. Todo el texto es sintético.

#[path = "comun_centros/mod.rs"]
mod comun_centros;
#[path = "comun_pdf/mod.rs"]
mod comun_pdf;
#[path = "comun_store_v2/mod.rs"]
mod comun_store_v2;

use std::sync::Arc;

use lakehouse_rag::application::dtos::ingestion::{
    CatalogoDestilado, CeldaDestilada, DistilledDocumentPackage, EjeDestilado, HitoDestilado,
    LimitesDestilados, TablaDestilada,
};
use lakehouse_rag::application::dtos::tools::ToolCall;
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::infrastructure::llm::{MockAgentRunner, MockDistiller};
use serde_json::{Value, json};

const CENTRO: &str = "capa2-paginas";
const RELLENO_ES: &str =
    "Texto suficiente para que la página cuente como extraíble por el extractor";
const RELLENO_EN: &str =
    "Enough text so that the extractor treats this page as extractable content";

fn categoria(valor: &str) -> LimitesDestilados {
    LimitesDestilados {
        categoria: Some(valor.to_string()),
        min: None,
        min_inclusivo: true,
        max: None,
        max_inclusivo: true,
    }
}

fn rango(min: Option<f64>, max: Option<f64>) -> LimitesDestilados {
    LimitesDestilados {
        categoria: None,
        min,
        min_inclusivo: false,
        max,
        max_inclusivo: true,
    }
}

fn celda(x: LimitesDestilados, y: LimitesDestilados, valor: &str) -> CeldaDestilada {
    CeldaDestilada {
        eje_x: x,
        eje_y: y,
        eje_z: None,
        valor_resultado: valor.to_string(),
        unidad_medida: None,
        nota_pie_condicion: None,
    }
}

fn eje(nombre: &str, tipo: &str) -> EjeDestilado {
    EjeDestilado {
        nombre: nombre.to_string(),
        tipo: tipo.to_string(),
        unidad: None,
    }
}

fn catalogo(doc_id: &str) -> CatalogoDestilado {
    CatalogoDestilado {
        doc_id: doc_id.to_string(),
        titulo: format!("Documento {doc_id}"),
        tipo_documento: "PROCEDIMIENTO_OBRA".to_string(),
        revision: "Rev. 01".to_string(),
        version_num: 1,
        fecha_vigor: Some("2026-01-01".to_string()),
        emisor: "Calidad".to_string(),
    }
}

// --- Tarifas, en español: la tabla en la página 2 y la regla general en la 3 ---

const TARIFAS: &str = "TAR-ENV-01";
const ARCHIVO_TARIFAS: &str = "tarifas_envio.pdf";
const REGLA_TEMPORADA: &str =
    "En temporada alta las tarifas de esta tabla se incrementan un veinte por ciento.";
const EXCLUSION_ISLAS: &str = "Los envíos a las islas quedan excluidos de esta tabla.";

fn pdf_tarifas() -> Vec<u8> {
    comun_pdf::pdf(&[
        &[
            "1 Objeto",
            "Este documento fija las tarifas de envío de la empresa.",
            RELLENO_ES,
        ],
        &[
            "2 Tarifas de envío",
            "Tabla 1. Tarifa por peso y zona",
            "Peso Zona A Zona B",
            "Hasta 5 kg 4,50 6,20",
            "Más de 5 kg 7,80 9,90",
        ],
        &[REGLA_TEMPORADA, EXCLUSION_ISLAS, RELLENO_ES],
        &[
            "3 Plazos",
            "El plazo de entrega es de dos días laborables.",
            RELLENO_ES,
        ],
    ])
}

fn paquete_tarifas() -> DistilledDocumentPackage {
    let peso = |p: &str| categoria(p);
    DistilledDocumentPackage {
        catalogo: catalogo(TARIFAS),
        hitos: vec![HitoDestilado {
            orden_secuencia: 0,
            fase_operativa: "RECEPCION".to_string(),
            objeto_alcance: "Plazo de entrega".to_string(),
            rol_ejecutor: "Transportista".to_string(),
            rol_responsable: "Jefe de almacén".to_string(),
            origen_responsable: "DECLARADO".to_string(),
            registro_codigo: None,
            registro_nombre: None,
            criterio_aceptacion: "Entrega en plazo".to_string(),
            condicion_aplicacion: None,
            seccion_origen: "3".to_string(),
        }],
        tablas: vec![TablaDestilada {
            tabla_id: "1".to_string(),
            titulo: "Tabla 1. Tarifa por peso y zona".to_string(),
            eje_x: eje("peso", "CATEGORIA_EXACTA"),
            eje_y: eje("zona", "CATEGORIA_EXACTA"),
            valor_salida_nombre: "tarifa_eur".to_string(),
            seccion_origen: Some("2".to_string()),
            celdas: vec![
                celda(peso("Hasta 5 kg"), categoria("Zona A"), "4,50"),
                celda(peso("Hasta 5 kg"), categoria("Zona B"), "6,20"),
                celda(peso("Más de 5 kg"), categoria("Zona A"), "7,80"),
                celda(peso("Más de 5 kg"), categoria("Zona B"), "9,90"),
            ],
        }],
        aristas: Vec::new(),
    }
}

// --- Dosis, en inglés: la tabla cruza de la página 2 a la 3 ---

const DOSIS: &str = "DOSE-GUIDE-02";
const ARCHIVO_DOSIS: &str = "dose_guide.pdf";
const EXCEPCION_RENAL: &str =
    "Patients with renal impairment must not exceed half of the doses given in this table.";

fn pdf_dosis() -> Vec<u8> {
    comun_pdf::pdf(&[
        &[
            "1 Scope",
            "This guide sets the daily dose of the reference product.",
            RELLENO_EN,
        ],
        &[
            "2 Daily dose",
            "Table 2. Daily dose by body weight and age group",
            "Weight Child Adult",
            "up to 20 kg 5 mg 10 mg",
        ],
        &["over 20 kg 8 mg 15 mg", EXCEPCION_RENAL, RELLENO_EN],
        &[
            "3 Storage",
            "Keep the product below room temperature.",
            RELLENO_EN,
        ],
    ])
}

fn paquete_dosis() -> DistilledDocumentPackage {
    DistilledDocumentPackage {
        catalogo: catalogo(DOSIS),
        hitos: Vec::new(),
        tablas: vec![TablaDestilada {
            tabla_id: "2".to_string(),
            titulo: "Table 2. Daily dose by body weight and age group".to_string(),
            eje_x: eje("body_weight_kg", "RANGO_NUMERICO"),
            eje_y: eje("age_group", "CATEGORIA_EXACTA"),
            valor_salida_nombre: "daily_dose".to_string(),
            seccion_origen: Some("2".to_string()),
            celdas: vec![
                celda(rango(None, Some(20.0)), categoria("Child"), "5 mg"),
                celda(rango(None, Some(20.0)), categoria("Adult"), "10 mg"),
                celda(rango(Some(20.0), None), categoria("Child"), "8 mg"),
                celda(rango(Some(20.0), None), categoria("Adult"), "15 mg"),
            ],
        }],
        aristas: Vec::new(),
    }
}

struct Escenario {
    _raiz: tempfile::TempDir,
    herramientas: Arc<dyn ToolServerPort>,
}

async fn escenario() -> Escenario {
    let raiz = tempfile::tempdir().unwrap();
    let destilador = MockDistiller::new();
    destilador.registrar_respuesta(ARCHIVO_TARIFAS, paquete_tarifas());
    destilador.registrar_respuesta(ARCHIVO_DOSIS, paquete_dosis());
    let (app, herramientas) = comun_centros::construir_app(
        &raiz.path().join("config"),
        Arc::new(destilador),
        Arc::new(MockAgentRunner::new_scripted(Vec::new())),
        comun_centros::modelo_e5(),
    );
    let centro = raiz.path().join("centros").join(CENTRO);
    app.crear_centro(CENTRO, &centro.to_string_lossy(), None)
        .await
        .unwrap();
    app.activar_centro(CENTRO).await.unwrap();
    let entrada = raiz.path().join("entrada");
    std::fs::create_dir_all(&entrada).unwrap();
    for (nombre, bytes) in [
        (ARCHIVO_TARIFAS, pdf_tarifas()),
        (ARCHIVO_DOSIS, pdf_dosis()),
    ] {
        let ruta = entrada.join(nombre);
        std::fs::write(&ruta, bytes).unwrap();
        app.ingestar(&ruta, CENTRO).await.unwrap();
    }
    Escenario {
        _raiz: raiz,
        herramientas,
    }
}

async fn invocar(escenario: &Escenario, nombre: &str, argumentos: Value) -> Value {
    let mut argumentos = argumentos;
    argumentos["centro_id"] = json!(CENTRO);
    let resultado = escenario
        .herramientas
        .ejecutar_herramienta(&ToolCall {
            id: "c".into(),
            name: nombre.into(),
            arguments: argumentos,
        })
        .await
        .unwrap();
    assert!(!resultado.is_error, "{}", resultado.output);
    resultado.output
}

async fn cruce(escenario: &Escenario, tabla: &str, x: Value, y: Value) -> Value {
    invocar(
        escenario,
        "lookup_double_entry_table",
        json!({ "busqueda_tabla": tabla, "valor_x": x, "valor_y": y }),
    )
    .await
}

#[tokio::test]
async fn test_celda_resuelta_trae_la_pagina_de_su_fila_con_marca() {
    let e = escenario().await;
    let tarifa = cruce(&e, "Tarifa por peso", json!("Hasta 5 kg"), json!("Zona B")).await;
    assert_eq!(tarifa["estado"], "resuelto", "{tarifa}");
    assert_eq!(tarifa["valor_resultado_texto"], "6,20");
    assert_eq!(tarifa["cita"]["pagina"], "⟦p. 2⟧", "{tarifa}");
    assert_eq!(tarifa["cita"]["paginas"], "2-3", "{tarifa}");

    // La tabla cruza de página: la segunda fila está en la 3
    let dosis = cruce(&e, "Daily dose", json!(35), json!("Adult")).await;
    assert_eq!(dosis["valor_resultado_texto"], "15 mg", "{dosis}");
    assert_eq!(dosis["cita"]["pagina"], "⟦p. 3⟧", "{dosis}");
    let nino = cruce(&e, "Daily dose", json!(12), json!("Child")).await;
    assert_eq!(nino["cita"]["pagina"], "⟦p. 2⟧", "{nino}");
}

#[tokio::test]
async fn test_fila_de_la_matriz_trae_la_pagina_de_su_seccion() {
    let e = escenario().await;
    let matriz = invocar(
        &e,
        "query_operational_matrix",
        json!({ "doc_id": TARIFAS, "solo_declarados": false }),
    )
    .await;
    let fila = &matriz["filas"][0];
    assert_eq!(fila["cita"]["pagina"], "⟦p. 4⟧", "{matriz}");
    assert_eq!(fila["cita"]["paginas"], "4", "{matriz}");
}
