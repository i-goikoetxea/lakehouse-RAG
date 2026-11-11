# Auditoría de generalidad (solo lectura), 11-11-2026

Complementa [`situacion_rag_20261111.md`](situacion_rag_20261111.md).

- La auditoría se hizo sin modificar código y sin acceder al set oculto.
- Las rutas sin marca son de `main`.
- Las marcadas con **[rama]** son de `claude/arreglos-rag-512d66`.

## 0. Repositorio y ficheros de referencia

- **Repositorio (público):** https://github.com/i-goikoetxea/lakehouse-RAG
- **`main` publicado:** commit `f47505e`.
  - El `main` local (`de9cae9`) solo añade documentación (`CLAUDE.md`). El código auditado es idéntico en los dos.
  - Enlaces permanentes: `https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/<ruta>#L<línea>`.
- **Ramas publicadas:**
  - [`claude/arreglos-rag-512d66`](https://github.com/i-goikoetxea/lakehouse-RAG/tree/cc77256) (`cc77256`): los 5 arreglos. [Diff contra `main`](https://github.com/i-goikoetxea/lakehouse-RAG/compare/f47505e...cc77256).
  - [`claude/set-oculto-v2-f844c1`](https://github.com/i-goikoetxea/lakehouse-RAG/tree/239c9e4) (`239c9e4`): scripts e informes del set v2, sin datos.

### Ficheros clave (`main`, `f47505e`)

**Arnés:**
- [intent_detector.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/arnes_turno/intent_detector.rs)
- [post_inference_guard.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/arnes_turno/post_inference_guard.rs)
- [verificador_paginas.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/arnes_turno/verificador_paginas.rs)
- [cifras.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/arnes_turno/cifras.rs)
- [fechas.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/arnes_turno/fechas.rs)

**Orquestador y *system prompt*:**
- [agent_orchestrator.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/agent_orchestrator.rs)

**Adaptador de Claude:**
- [claude_agent_runner.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/infrastructure/llm/claude_agent_runner.rs)
- [claude_headless_distiller.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/infrastructure/llm/claude_headless_distiller.rs)

**Búsqueda:**
- [busqueda.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/lakehouse_tool_service/busqueda.rs)
- [lakehouse_tool_service.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/lakehouse_tool_service.rs)

**Tablas (Capa 2):**
- [lookup.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/lakehouse_tool_service/lookup.rs)
- [mapeo_destilacion.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/use_cases/mapeo_destilacion.rs)
- [dtos/ingestion.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/application/dtos/ingestion.rs)

**PDF:**
- [pdf_parser.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/infrastructure/parsers/pdf_parser.rs)
- [pdf_estructura.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/src/infrastructure/parsers/pdf_estructura.rs)
- [Cargo.toml](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/Cargo.toml)

**Documentación:**
- Contrato de desarrollo: [AGENTS.md](https://github.com/i-goikoetxea/lakehouse-RAG/blob/f47505e/AGENTS.md)
- Diseño: [docs/arquitectura/](https://github.com/i-goikoetxea/lakehouse-RAG/tree/f47505e/docs/arquitectura)
- Benchmarks: [benchmarks/](https://github.com/i-goikoetxea/lakehouse-RAG/tree/f47505e/benchmarks)

**Solo en las ramas:**
- [negaciones.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/cc77256/src/application/use_cases/arnes_turno/negaciones.rs)
- [apartado_tabla.rs](https://github.com/i-goikoetxea/lakehouse-RAG/blob/cc77256/src/application/use_cases/lakehouse_tool_service/apartado_tabla.rs)
- [arreglos_generales_20261110.md](https://github.com/i-goikoetxea/lakehouse-RAG/blob/cc77256/benchmarks/informes/arreglos_generales_20261110.md)
- [set_oculto_v2_linea_base_20261110.md](https://github.com/i-goikoetxea/lakehouse-RAG/blob/239c9e4/benchmarks/informes/set_oculto_v2_linea_base_20261110.md)
- [set_oculto_v2_arreglos_20261110.md](https://github.com/i-goikoetxea/lakehouse-RAG/blob/239c9e4/benchmarks/informes/set_oculto_v2_arreglos_20261110.md)

---

## Resumen

1. El arnés depende sobre todo de listas léxicas en español y de dominio industrial.
2. La página falsa sale de una función que acepta un apartado aunque no contenga ningún valor de la tabla.
3. El problema de las «llamadas en crudo» no tiene que ver con MCP. El bucle del agente usa un protocolo JSON textual y trata como respuesta final todo lo que no se pueda parsear como JSON.

## 1. Arnés

### Listas léxicas en `main`

**`intent_detector.rs`:**

| Línea | Constante | Contenido | Idioma o dominio |
|---|---|---|---|
| 11 | `MAGNITUDES` | `temperatur`, `presion`, `espesor`, `dureza`… | **español, industrial** |
| 30 | `ETIQUETAS_CATEGORICAS` | `clase`, `grupo`, `material`, `proceso`… | español |
| 42 | `REFERENCIAS` | `tabla`, `articulo`, `iso`, `une`, `asme`, `astm`, `din`, `aws`… | español y **normas industriales** |
| 50 | `PALABRAS_VACIAS` | — | español |
| 81 | `PREFIJOS_NUMERO_DE_TABLA` | — | — |
| 82 | `ENTIDADES_CONTABLES` | — | español |
| 96 | `VERBOS_DE_RESPONSABILIDAD` | `firm`, `aprueb`, `supervis`… | español |
| 116 y 134 | `ORDENES_DE_IDENTIFICAR`, `ORDENES_DE_DAR_DATO` | `dime`, `indica`, `dame`… | español |
| 117 | `DATOS_CONCRETOS` | — | español |

Literales sueltos:

| Línea | Función | Literal |
|---|---|---|
| 220 | `pide_agregacion` | `"cuantos"`, `"cuantas"` |
| 241 | `pregunta_por_responsable` | `"es"`, `"son"` |
| 280 | — | `"responsable"` |
| 308 | `es_palabra_tabla` | `"tabla"`, `"tablas"` |

**`post_inference_guard.rs`:**

| Línea | Constante | Contenido |
|---|---|---|
| 16 | `LOCUCIONES_DE_RECHAZO` | `no consta`, `no declarado` |
| 18 | `LOCUCIONES_DE_ABSTENCION` | 33 locuciones, **todas en español**. No detecta una abstención en inglés («not stated», «not found»). |
| 53 | `PETICIONES_DE_REFERENCIA` | español |
| 83 | `PALABRAS_DE_ENLACE` | español |

**Otros ficheros:**

| Fichero | Línea | Constante | Contenido |
|---|---|---|---|
| `cifras.rs` | 13 | `UNIDADES` | Unidades físicas e industriales; sin monedas ni unidades clínicas |
| `cifras.rs` | 19 | `PALABRAS_SIN_CIFRA` | `["capa","capas"]` |
| `fechas.rs` | 6 | `MESES` | **Solo español.** No reconoce «March 14, 2025». |
| `verificador_paginas.rs` | 29 | `PALABRAS_DE_PAGINA` | español e inglés |

### Lo que añade la rama

**`negaciones.rs`** (fichero nuevo):

| Línea | Elemento | Contenido |
|---|---|---|
| 18 | `NEGACIONES_EXISTENCIALES` | 11 en español y 4 en inglés |
| 36 | `NEGACIONES_VERBALES` | — |
| 45 | `PALABRAS_SIN_ASPECTO` | ~95, en español e inglés |
| 144 | `PALABRAS_QUE_ANULAN` | — |
| 148 | Heurística | Raíz de 5 letras |
| 150 | Heurística | Ventana de 4 palabras |
| 156 | Heurística | Etiqueta con `:` |

**Otros cambios del arnés:**
- `intent_detector.rs:190`, `menciona_tabla`: reutiliza `es_palabra_tabla`, así que **solo reconoce «tabla» en español**.
- `post_inference_guard.rs:111`, `fijar_vocabulario_de_la_app`: usa como exclusión las palabras del *system prompt*, que está en español.

### `git diff main..claude/arreglos-rag-512d66 --stat`

27 ficheros, +2789 y −114 líneas. En el código fuente:

| Fichero | Líneas | Qué cambia |
|---|---|---|
| `arnes_turno/negaciones.rs` | +524 | Regla nueva: una negación rotunda sobre un aspecto que no aparece en lo consultado provoca un reintento. |
| `arnes_turno/post_inference_guard.rs` | +312 | `RegistroTurno` guarda las llamadas, si se leyó texto y el vocabulario de la app. `exige_capa_dos` (:302): si la pregunta es `TabularIncompleta` y no dice «tabla», solo se exige la Capa 2 cuando no se leyó texto o cuando lo leído enlaza a una tabla. Añade `comprobar_negaciones_buscadas` (:356). |
| `arnes_turno/verificador_paginas.rs` | +157 | Solo `Discrepante` genera nota. Se callan `Ambigua`, `NoEncontrada` y `SinPagina`. |
| `arnes_turno/intent_detector.rs` | +15 | `menciona_tabla` |
| `lakehouse_tool_service/apartado_tabla.rs` | +625 | Nuevo: localiza el apartado de la tabla, da la página por celda y añade `contexto_apartado` (hasta 2000 caracteres de prosa). |
| `lakehouse_tool_service/lookup.rs` | +154 | Añade `paginas`, `pagina` y `contexto_apartado` a la salida del lookup. |
| `lakehouse_tool_service/matriz.rs` | +42 | Páginas del chunk de origen en la fila de la matriz. |
| `busqueda.rs` y `estructura.rs` | +10 y +18 | Exponen `tablas_vinculadas`. |
| `agent_orchestrator.rs` | +47 | Registra las llamadas en el guard y le pasa el vocabulario de la app. |
| `semantic_index.rs`, `lakehouse_projection.rs`, `indice_semantico_memoria.rs`, `intervenciones_arnes.rs` | +1 a +19 | Conexiones internas y GUI. |

## 2. PDF y páginas

### Crates

`lopdf 0.42` y `pdf-extract 0.12` (`Cargo.toml:45-46`).

- **Extracción por página** (`pdf_parser.rs:213-232`): usa `pdf_extract::output_doc_page` y, si falla, recurre a `lopdf::extract_text`.
- **No hay control de legibilidad.** `requiere_ocr` (`pdf_estructura.rs:145`) solo cuenta los caracteres no blancos por página. Un texto con letras sustituidas pasa como COMPLETO.
- **El parser de PDF no extrae tablas:** devuelve `tablas: Vec::new()` (`pdf_parser.rs:142`). Las tablas de Capa 2 de un PDF salen solo de la **destilación por LLM** (`TablaDestilada`, `dtos/ingestion.rs:169`, con la `seccion_origen` que declara el LLM).

### Página en la Capa 2

**En `main`** el lookup no da página: `lookup.rs` no la contiene.

**En la rama**, la cadena es esta:

1. **Ingesta.** `mapeo_destilacion.rs:1236-1262` (`vincular_tablas`) vincula la tabla:
   - al chunk de la `seccion_origen` **que declara el LLM**;
   - a todo chunk cuyo texto contenga el literal `"Tabla {id}"` (:1252). Ese literal es **español**: «Table 3» nunca coincide.

   En un PDF no hay sección de parser, porque el parser no extrae tablas.
2. **`ApartadoTabla::localizar`** ([rama] `apartado_tabla.rs:107`). Elige, entre las secciones vinculadas, la que contiene más valores de celdas.
3. **El defecto** (:133): `if maximo == 0 && puntuados.len() > 1 { return None }`. Con **una sola** sección vinculada, la acepta **aunque no contenga ningún valor de la tabla**.
4. **`pagina_de_valor`** (:248). Busca el texto de la celda en las líneas de ese apartado y, si aparece en una única página, la devuelve (:275). Un valor corto, como un número suelto, puede coincidir por azar con la prosa.

**Causa probable de la p. 6 en lugar de las pp. 21-22.** Es una inferencia del código, no está comprobada con el documento:
- El LLM declaró como `seccion_origen` un apartado introductorio (p. 6), o era el único chunk vinculado.
- `localizar` lo aceptó sin valores.
- El número de la celda aparece una vez en esa prosa.

### Avisos falsos sobre el título (p. 1)

En [rama] `verificador_paginas.rs`:

- **`revisar`** (:83) extrae cada texto entre comillas que va junto a una referencia de página y lo busca en los textos de las tools.
- **`comprobar`** (:141) devuelve `Discrepante` cuando el extracto aparece en **una única** ubicación y esa ubicación no incluye la página citada (:167).
- **La excepción de títulos** (:103-108) solo se aplica si el resultado es `NoEncontrada`.

Por eso este caso da `Discrepante` y un aviso: el modelo escribe «Título del documento», p. 22, y el chunk recuperado incluye la portada con ese título en la p. 1.

## 3. Búsqueda y adaptador de Claude

### ¿Límite de fragmentos por documento en `search_normative_context`?

**No existe.**

- **`fusionar`** (`busqueda.rs:147-189`): fusión RRF con `RRF_K = 60` (:33), orden por puntuación y `truncate(top_k)` (:187).
- **`top_k`:** 8 por defecto (`lakehouse_tool_service.rs:68`) y 20 como máximo (`dtos/tools.rs:412`).
- **Sin cupo ni diversificación por `doc_id`:** un documento puede ocupar los 8 resultados. `agrupar_por_tema` (:508) solo agrupa la presentación.

### Fin de turno y llamadas en crudo

**La premisa no se cumple: el bucle del agente no usa MCP ni el *tool use* nativo.**

- **Arranque:** se lanza `claude -p … --tools "" --strict-mcp-config` (`claude_agent_runner.rs:175-192`), que desactiva las herramientas nativas.
- **Protocolo:** es textual. El modelo debe responder **solo** con `{"tool_calls":[…]}` (:143-150).
- **Un proceso por paso:** cada paso lanza un `claude -p` nuevo, con toda la conversación por stdin (:156-173). El MCP solo sirve a clientes externos.
- **`parsear_salida_claude`** (:80-141):
  - quita una valla ``` inicial (:61-78);
  - intenta parsear **la salida entera** como JSON (`tool_calls` o `type: tool_use`);
  - **trata todo lo demás como `FinalMessage`** (:140).
- **Consecuencia:** basta con texto antes del JSON, un JSON incompleto o dos bloques para que la llamada se entregue al usuario como respuesta. El guard no comprueba si un `FinalMessage` tiene forma de llamada a herramienta.

## 4. Fallos por causa técnica

Datos agregados del set v2, sin texto de las preguntas.

| Causa | `main` (8 fallos) | Arreglos (7 fallos) |
|---|---|---|
| Bloqueo o reintento del arnés con valores legítimos (artículos, cálculos) | **4** | **1** |
| Texto corrupto del PDF marcado COMPLETO | **4** | **4** |
| Recuperación multidocumento: falta el segundo documento y se niega sin evidencia | 0 en las pasadas (2 de 10 en repeticiones) | **1** |
| Recuperación multidocumento: cifra de otro perímetro | 0 en las pasadas (2 de 5 en repeticiones) | **1** |
| Error de cruce tabular | 0 | 0 |

Además, en la rama de arreglos hay **1 respuesta correcta con página falsa**, que viene de la Capa 2.

### Dónde se van las ~2,5 h

**Medido en `main`:**

| Fase | Duración |
|---|---|
| Ingesta | ~52 min (39-516 s por documento) |
| Pasada 1 | 51 min (mediana 44,6 s por pregunta) |
| Pasada 2 | 53 min |

En la rama de arreglos, la ingesta tardó ~65 min.

**Sin medir o supuesto:**
- **La ingesta también es inferencia:** la Capa 2 se destila con `claude -p` (`claude_headless_distiller.rs:147`). No se ha medido qué parte es destilación y qué parte embeddings.
- **Por pregunta:** la búsqueda tarda 0,2-0,9 s (medido antes). Casi todo el resto es el modelo más un arranque de `claude -p` por paso. El reparto exacto no está medido.
- **CPU:** estuvo cargada, con picos del 100 %. Los tiempos son orientativos.
