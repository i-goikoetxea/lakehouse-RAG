//! Fixtures de las tools v2 (Fase 13): un centro real en disco (LanceDB + proyección) con la
//! Rev. 03 y la Rev. 04 de `PO-ALM-03` (tabla 4-A completa), un pliego del cliente (nivel 1),
//! una norma (nivel 5) y una norma «hub» con 1.000 aristas. Oráculo: `tests/fixtures/README.md`.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use lakehouse_rag::application::dtos::ingestion::{
    AristaDestilada, CatalogoDestilado, CeldaDestilada, DistilledDocumentPackage, EjeDestilado,
    HitoDestilado, LimitesDestilados, TablaDestilada,
};
use lakehouse_rag::application::dtos::tools::{ToolCall, ToolResult};
use lakehouse_rag::application::ports::tool_server::ToolServerPort;
use lakehouse_rag::infrastructure::llm::{MockAgentRunner, MockDistiller};
use serde_json::{Value, json};

use crate::comun_centros::{Entorno, construir_app, modelo_e5};

pub const CENTRO: &str = "obra-tools-v2";
pub const CENTRO_VACIO: &str = "obra-vacia";
pub const FECHA_ANTERIOR_REV04: &str = "2026-06-01";
pub const NOTA_ASTERISCO: &str =
    "(*) Requiere aprobación previa del Inspector QA/QC y ensayo de dureza HV10 sobre probeta.";
pub const ARISTAS_HUB: usize = 1000;

pub const ARCHIVO_REV03: &str = "po_alm_03_rev03_tools.md";
pub const ARCHIVO_REV04: &str = "po_alm_03_rev04_tools.md";
pub const ARCHIVO_PLIEGO: &str = "pcc_01_pliego.md";
pub const ARCHIVO_NORMA: &str = "une_en_1090_2.md";
pub const ARCHIVO_HUB: &str = "norma_hub.md";

fn procedimiento(revision: &str) -> String {
    format!(
        "# PO-ALM-03 Recepción y custodia de tubería ({revision})\n\n\
         ## 1. Recepción\nEl Almacenero recibe la tubería y la verifica contra el albarán.\n\n\
         ## 2. Custodia\n### 2.1 Acopio prolongado (cuando el acopio supera 7 días)\n\
         La tubería se almacena sobre durmientes a más de 15 cm del suelo.\n\n\
         ## 3. Soldadura\n### 3.1 Soldadura bajo cero (< 0 °C)\n\
         Calentar la zona de soldadura a 80 °C antes de soldar.\n\n\
         ### 3.2 Tabla 4-A\nTemperatura mínima de precalentamiento según espesor y carbono equivalente.\n"
    )
}

const PLIEGO: &str = "# PCC-01 Pliego de condiciones del cliente\n\n## 5. Ensayos no destructivos\nSe debe ensayar el 100 % de las soldaduras a tope mediante radiografía.\n";
const NORMA: &str = "# UNE-EN 1090-2 Ejecución de estructuras de acero\n\n## 12. Ensayos no destructivos\nSe debe ensayar el 10 % de las soldaduras a tope.\n";
const HUB: &str =
    "# NORMA-HUB Índice general de normas\n\n## 1. Remisiones\nRelación de documentos citados.\n";

fn catalogo(doc_id: &str, tipo: &str, version: i64, fecha_vigor: &str) -> CatalogoDestilado {
    CatalogoDestilado {
        doc_id: doc_id.to_string(),
        titulo: format!("Documento {doc_id}"),
        tipo_documento: tipo.to_string(),
        revision: format!("Rev. {version:02}"),
        version_num: version,
        fecha_vigor: fecha_vigor.to_string(),
        emisor: "Calidad".to_string(),
    }
}

fn hito(
    fase: &str,
    ejecutor: &str,
    responsable: &str,
    registro: &str,
    condicion: Option<&str>,
    seccion: &str,
) -> HitoDestilado {
    HitoDestilado {
        orden_secuencia: 0,
        fase_operativa: fase.to_string(),
        objeto_alcance: "Tubería de acero al carbono".to_string(),
        rol_ejecutor: ejecutor.to_string(),
        rol_responsable: responsable.to_string(),
        registro_codigo: Some(registro.to_string()),
        registro_nombre: Some(format!("Registro {registro}")),
        criterio_aceptacion: "Conforme al procedimiento".to_string(),
        condicion_aplicacion: condicion.map(str::to_string),
        seccion_origen: seccion.to_string(),
    }
}

fn limites(min: Option<f64>, min_inc: bool, max: Option<f64>, max_inc: bool) -> LimitesDestilados {
    LimitesDestilados {
        categoria: None,
        min,
        min_inclusivo: min_inc,
        max,
        max_inclusivo: max_inc,
    }
}

fn eje(nombre: &str, unidad: &str) -> EjeDestilado {
    EjeDestilado {
        nombre: nombre.to_string(),
        tipo: "RANGO_NUMERICO".to_string(),
        unidad: Some(unidad.to_string()),
    }
}

/// Tabla 4-A (9 celdas). La celda central vale 100 (*) en la Rev. 03 y 125 (*) en la Rev. 04
fn tabla_4a(valor_central: &str) -> TablaDestilada {
    let espesores = [
        limites(None, false, Some(20.0), true),
        limites(Some(20.0), false, Some(40.0), true),
        limites(Some(40.0), false, None, false),
    ];
    let carbonos = [
        limites(None, false, Some(0.40), false),
        limites(Some(0.40), true, Some(0.45), true),
        limites(Some(0.45), false, None, false),
    ];
    let valores = [
        ["20", "50", "75"],
        ["50", valor_central, "150 (*)"],
        ["75", "150", "No admisible (*)"],
    ];
    let mut celdas = Vec::new();
    for (i, x) in espesores.iter().enumerate() {
        for (j, y) in carbonos.iter().enumerate() {
            let valor = valores[i][j];
            celdas.push(CeldaDestilada {
                eje_x: x.clone(),
                eje_y: y.clone(),
                eje_z: None,
                valor_resultado: valor.to_string(),
                unidad_medida: Some("°C".to_string()),
                nota_pie_condicion: valor.contains("(*)").then(|| NOTA_ASTERISCO.to_string()),
            });
        }
    }
    TablaDestilada {
        tabla_id: "4-A".to_string(),
        titulo: "Tabla 4-A Temperatura mínima de precalentamiento".to_string(),
        eje_x: eje("espesor_nominal_mm", "mm"),
        eje_y: eje("carbono_equivalente_pct", "%"),
        valor_salida_nombre: "temperatura_min_c".to_string(),
        seccion_origen: Some("3.2".to_string()),
        celdas,
    }
}

fn arista(destino: &str, relacion: &str, tipo_destino: &str) -> AristaDestilada {
    AristaDestilada {
        origen_tipo: "DOCUMENTO".to_string(),
        tipo_relacion: relacion.to_string(),
        destino_id: destino.to_string(),
        destino_tipo: tipo_destino.to_string(),
        contexto: None,
    }
}

fn paquete_procedimiento(
    version: i64,
    fecha: &str,
    registro: &str,
    central: &str,
) -> DistilledDocumentPackage {
    let mut aristas = vec![
        arista("UNE-EN 1090-2", "SUBORDINADO_A_NORMA", "DOCUMENTO"),
        arista("PCC-01", "SUBORDINADO_A_NORMA", "DOCUMENTO"),
        arista("NORMA-HUB", "REFERENCIA_A", "DOCUMENTO"),
        arista(registro, "EXIGE_LLENAR_REGISTRO", "REGISTRO"),
    ];
    if version == 4 {
        aristas.push(arista("PO-ALM-03 Rev. 03", "DEROGA_A", "DOCUMENTO"));
    }
    DistilledDocumentPackage {
        catalogo: catalogo("PO-ALM-03", "PROCEDIMIENTO_OBRA", version, fecha),
        hitos: vec![
            hito(
                "RECEPCIÓN",
                "Almacenero",
                "Jefe de Almacén",
                "FOR-ALM-01",
                None,
                "1",
            ),
            hito(
                "CUSTODIA",
                "Auxiliar de Almacén",
                "Jefe de Producción",
                registro,
                Some("Cuando el acopio supera 7 días"),
                "2.1",
            ),
            hito(
                "INSPECCIÓN",
                "Técnico de Calidad",
                "Inspector QA/QC",
                "PPI-MAT-09",
                None,
                "3.2",
            ),
        ],
        tablas: vec![tabla_4a(central)],
        aristas,
    }
}

fn paquete_simple(
    doc_id: &str,
    tipo: &str,
    aristas: Vec<AristaDestilada>,
) -> DistilledDocumentPackage {
    DistilledDocumentPackage {
        catalogo: catalogo(doc_id, tipo, 1, "2025-01-01"),
        hitos: Vec::new(),
        tablas: Vec::new(),
        aristas,
    }
}

pub fn distiller_tools_v2() -> Arc<MockDistiller> {
    let distiller = MockDistiller::new();
    distiller.registrar_respuesta(
        ARCHIVO_REV03,
        paquete_procedimiento(3, "2026-03-01", "FOR-ALM-02", "100 (*)"),
    );
    distiller.registrar_respuesta(
        ARCHIVO_REV04,
        paquete_procedimiento(4, "2026-09-15", "FOR-ALM-05", "125 (*)"),
    );
    distiller.registrar_respuesta(
        ARCHIVO_PLIEGO,
        paquete_simple("PCC-01", "ESPECIFICACION_CLIENTE", Vec::new()),
    );
    distiller.registrar_respuesta(
        ARCHIVO_NORMA,
        paquete_simple("UNE-EN-1090-2", "NORMA_INTERNACIONAL", Vec::new()),
    );
    let hub: Vec<AristaDestilada> = (1..=ARISTAS_HUB)
        .map(|i| arista(&format!("REG-{i:04}"), "REFERENCIA_A", "REGISTRO"))
        .collect();
    distiller.registrar_respuesta(
        ARCHIVO_HUB,
        paquete_simple("NORMA-HUB", "NORMA_INTERNACIONAL", hub),
    );
    Arc::new(distiller)
}

fn escribir(entorno: &Entorno, nombre: &str, contenido: &str) -> PathBuf {
    let directorio = entorno.raiz.path().join("entrada");
    std::fs::create_dir_all(&directorio).unwrap();
    let ruta = directorio.join(nombre);
    std::fs::write(&ruta, contenido).unwrap();
    ruta
}

/// Centro activo con los 5 documentos ingeridos (Rev. 03 antes que Rev. 04) y un centro vacío activo
pub async fn entorno_sembrado() -> Entorno {
    let raiz = tempfile::tempdir().unwrap();
    let (app, herramientas) = construir_app(
        &raiz.path().join("config"),
        distiller_tools_v2(),
        Arc::new(MockAgentRunner::new_scripted(Vec::new())),
        modelo_e5(),
    );
    let entorno = Entorno {
        raiz,
        app,
        herramientas,
    };
    for (centro, nombre) in [(CENTRO, "Obra tools v2"), (CENTRO_VACIO, "Obra vacía")] {
        entorno
            .app
            .crear_centro(centro, &entorno.ruta_centro_texto(centro), Some(nombre))
            .await
            .unwrap();
        entorno.app.activar_centro(centro).await.unwrap();
    }
    for (archivo, contenido) in [
        (ARCHIVO_REV03, procedimiento("Rev. 03")),
        (ARCHIVO_REV04, procedimiento("Rev. 04")),
        (ARCHIVO_PLIEGO, PLIEGO.to_string()),
        (ARCHIVO_NORMA, NORMA.to_string()),
        (ARCHIVO_HUB, HUB.to_string()),
    ] {
        let ruta = escribir(&entorno, archivo, &contenido);
        entorno.app.ingestar(&ruta, CENTRO).await.unwrap();
    }
    entorno
}

pub async fn invocar(
    herramientas: &Arc<dyn ToolServerPort>,
    nombre: &str,
    argumentos: Value,
) -> ToolResult {
    herramientas
        .ejecutar_herramienta(&ToolCall {
            id: format!("call_{nombre}"),
            name: nombre.to_string(),
            arguments: argumentos,
        })
        .await
        .expect("la herramienta devuelve un ToolResult")
}

/// Invoca y exige éxito
pub async fn ok(herramientas: &Arc<dyn ToolServerPort>, nombre: &str, argumentos: Value) -> Value {
    let resultado = invocar(herramientas, nombre, argumentos).await;
    assert!(!resultado.is_error, "{nombre} falló: {}", resultado.output);
    resultado.output
}

pub fn args(extra: Value) -> Value {
    let mut base = json!({ "centro_id": CENTRO });
    base.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    base
}
