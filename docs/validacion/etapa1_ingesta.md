# Validación E2E — Etapa 1: Ingesta real de un documento

> **Fecha:** 2026-10-17 · **Rama:** `main` (sin commits) · **Alcance:** diagnóstico. No se ha tocado código de producción en `src/`.
> **Referencias:** especificación §3–§5 y §9 (`docs/especificaci_n_t_cnica_lakehouse_rag_en_rust.md`).

## 1. Qué se probó

Flujo real completo, sin mocks:

```
Markdown (fixture) → MarkdownParser → ClaudeHeadlessDistiller (claude -p, Sonnet)
                   → FastEmbedEngine (BGE-Large-EN v1.5, ONNX local, 1024 dims)
                   → LanceDbStore en disco (tempdir) → lectura y comprobación de las 6 tablas
```

Se ingiere primero la **Rev. 03** y después la **Rev. 04** del mismo procedimiento, para validar el versionado (`vigente = false`).

**Artefactos nuevos (ninguno en `src/`):**

| Archivo | Contenido |
| --- | --- |
| `tests/fixtures/procedimiento_recepcion_rev03.md` | Procedimiento `PO-ALM-03` con H1 > H2 > H3, fases RECEPCIÓN / CUSTODIA / INSPECCIÓN (ejecutor, responsable y registro distintos), tabla 4-A de doble entrada (rangos en X e Y, notas `(*)` y `(1)`) y referencias a normas y anexos. |
| `tests/fixtures/procedimiento_recepcion_rev04.md` | Rev. 04: el registro de custodia cambia a FOR-ALM-05, el cruce (30 mm, CE 0,42) pasa de 100 a 125 °C y se añade la derogación de la Rev. 03. |
| `tests/fixtures/README.md` | Respuestas correctas esperadas. Están fuera de los `.md` porque el destilador envía el documento entero a Claude y las contaminaría. |
| `tests/acceptance_e2e_etapa1_ingesta.rs` | 12 tests: 2 con mocks (suite normal), 3 reales `#[ignore]` y 7 `hallazgo_*` `#[ignore]` en rojo a propósito. |

**Cómo se ejecuta:**

```bash
cargo test --test acceptance_e2e_etapa1_ingesta
```

```bash
cargo test --test acceptance_e2e_etapa1_ingesta -- --ignored --nocapture --test-threads=1
```

Los paquetes JSON que devuelve Claude se vuelcan en `target/validacion_etapa1/`. El modelo de FastEmbed (1,3 GB) se descarga en `target/fastembed_cache/`.

El test usa decoradores de los 4 puertos (`ParserCronometrado`, `DestiladorCronometrado`, `EmbeddingsCronometrados`, `AlmacenCronometrado`) para medir cada paso dentro del `DocumentIngestionService` real sin modificarlo.

## 2. Resultados por comprobación

Columnas: **Mock** = destilación «oro» + embeddings mock sobre LanceDB real; **Real** = Claude + FastEmbed + LanceDB.

| # | Comprobación | Mock | Real | Detalle (ejecución real) |
| --- | --- | :---: | :---: | --- |
| 1 | Se crean las 6 tablas `.lance` | OK | OK | `01_doc_catalog` … `06_reference_graph` |
| 2 | La ingesta Rev. 03 termina sin error | OK | OK | 9 chunks, 9 o 5 tuplas (según la ejecución), 1 tabla, 9 celdas, 6 aristas |
| 3 | `doc_id` canónico (`PO-ALM-03`) | OK | OK | Claude lo extrae bien en todas las ejecuciones |
| 4 | Filas por tabla = reporte | OK | OK | 1 / 9 / 9 / 1 / 9 / 6 (ejecución 1); 1 / 9 / 5 / 1 / 9 / 6 (ejecución 2) |
| 5 | Matriz operativa ≥ 3 filas | OK | OK | Ejecución 1: 9 filas (RECEPCIÓN ×2, CUSTODIA, INSPECCIÓN, SOLDEO ×4, REGISTROS). Ejecución 2: 5 filas |
| 6 | Anti-fusión: una fila por fase, ninguna mezclada | OK | OK | Cada fase en su fila |
| 7 | Responsable de CUSTODIA = Jefe de Producción | OK | OK | Aparece, pero junto al ejecutor (ver #8) |
| 8 | `rol_responsable` sin mezclar al ejecutor | OK | **FALLA** | `"Auxiliar de Almacén (ejecutor); Jefe de Producción (firma)"` → H2 |
| 9 | Registro de CUSTODIA (FOR-ALM-02 / -05) en su fila | OK | OK* | *Solo como texto libre en `referencia_normativa` / `accion`: no existe `registro_codigo` → H1 |
| 10 | Fases con valor SQL exacto (`RECEPCION`, `INSPECCION`) | OK | **FALLA** | Claude guarda `RECEPCIÓN`, `INSPECCIÓN` o `SOLDEO (REQUISITOS POSTERIORES)` → H2 |
| 11 | Tabla 4-A: 9 celdas con rangos numéricos en X e Y | OK | OK | 9/9 con min/max e inclusividad correctos |
| 12 | Cruce e = 30 mm, CE = 0,42 → **100** (Rev. 03) | OK | OK | Una sola celda: `100` |
| 13 | Nota al pie `(*)` en la celda del cruce | OK | OK | `"(*) Requiere aprobación previa del Inspector QA/QC y ensayo de dureza HV10…"` |
| 14 | Frontera inclusiva (e = 20 mm → columna `e ≤ 20`) | OK | OK | `x_inclusivo_max = true` |
| 15 | Aristas del grafo (≥ 3, incluye normas) | OK | OK | UNE-EN 1090-2, UNE-EN ISO 15614-1, PO-CAL-01, PCC-01, UNE-EN 10204, Anexo II |
| 16 | `tipo_relacion` dentro del vocabulario de §4 | OK | **FALLA** | Claude inventa `REFERENCIA`, `SUBORDINADO_A`, `DEROGA` → H2 |
| 17 | Aristas `EXIGE_LLENAR_REGISTRO` a los 3 registros | OK | **FALLA** | Ninguna arista hacia FOR-ALM-01/02 ni PPI-MAT-09 → H2 |
| 18 | Vectores 1024 dims, sin nulos ni ceros | OK | OK | 9 vectores; columna = motor = 1024 |
| 19 | `doc_id` estable entre Rev. 03 y Rev. 04 | OK | OK | `PO-ALM-03` → `PO-ALM-03` |
| 20 | Vigencia `01_doc_catalog` | OK | OK | `(3, false), (4, true)` |
| 21 | Vigencia `02_doc_chunks` | OK | OK | false 9/9, true 9/9 |
| 22 | Vigencia `03_operational_matrix` | OK | OK | false 9/9, true 11/11 (ejecución 1); false 5/5, true 5/5 (ejecución 2) |
| 23 | Vigencia `04_double_entry_tables` | OK | OK | false 1/1, true 1/1 |
| 24 | Vigencia `05_double_entry_cells` | OK | OK | false 9/9, true 9/9 |
| 25 | Vigencia `06_reference_graph` | **FALLA** | **FALLA** | false **0**/6: las aristas de la Rev. 03 siguen vigentes → H3 |
| 26 | IDs únicos entre revisiones | **FALLA** | **FALLA** | `chunk_id`: 18 filas, 9 distintos → H4 |
| 27 | Rev. 04: CUSTODIA → FOR-ALM-05 y cruce → **125** | OK | OK | Solo la Rev. 04 queda vigente en 03 y 05 |
| 28 | Rev. 04: arista `DEROGA_A` hacia la Rev. 03 | OK | **FALLA** | 0 aristas `DEROGA_A`; en la ejecución 1, Claude la emitió como `DEROGA` con destino `"PO-ALM-03 Rev. 03"` → H2 |
| 29 | Esquema 03 con los campos de §4 | NO EXISTE | NO EXISTE | Faltan `hito_id`, `orden_secuencia`, `objeto_alcance`, `rol_ejecutor`, `registro_codigo`, `registro_nombre`, `criterio_aceptacion`, `chunk_origen_id` → H1 |
| 30 | `vector_hito` (03) y `vector_tabla` (04) | NO EXISTE | NO EXISTE | Solo `02_doc_chunks` tiene vector → H1 |
| 31 | Esquema 01 con `revision`, `fecha_vigor` (Date32) y `tipo_documento` | NO EXISTE | NO EXISTE | Hay `version` Int32 y `fecha_emision` Utf8 → H1 |
| 32 | La re-ingesta del mismo archivo no duplica filas | FALLA | — | H5 (`hallazgo_reingesta_…`) |
| 33 | Chunks con breadcrumb H1 > H2 > H3 | FALLA | — | Se guarda `"2.2 Fase 2 — CUSTODIA"` sin sus padres → H7 |
| 34 | El parser capta el título y las notas de la tabla | FALLA | — | Título → `"Tabla 1"`; notas → vacías → H8 |
| 35 | Un `doc_id` con comilla simple retira la vigencia | FALLA | — | Ambas revisiones quedan vigentes → H9 |

> Se hicieron dos ejecuciones reales completas (Rev. 03 + Rev. 04) y una de destilación aislada. Las comprobaciones 8, 10, 16, 17 y 28 se añadieron tras ver la salida de la primera; su columna «Real» procede de la segunda. Las demás dieron el mismo resultado en ambas.

## 3. Hallazgos y causa raíz

Ordenados por impacto en los criterios de §9.

### H1 — Los esquemas Arrow y los DTO no siguen el modelo de datos de §4 (crítico para §9.2)
- **Dónde:** `src/infrastructure/storage/schemas.rs:16` (01), `:50` (03), `:66` (04); `src/application/dtos/ingestion.rs:45` y `:57`.
- **Causa raíz:** la Fase 2 definió un modelo propio (`accion`, `parametro_control`, `tolerancia_criterio`…) en lugar del de §4. `DistilledOperationalTuple` no tiene `rol_ejecutor`, `registro_codigo` ni `registro_nombre`, así que Claude no puede separarlos aunque quiera.
- **Efecto:** la pregunta de §9.2 («¿quién es el responsable de la custodia y qué registro llena?») no tiene columna que consultar. El registro solo aparece como texto libre en `referencia_normativa` o `accion`. Tampoco existen `vector_hito` ni `vector_tabla`, ni `fecha_vigor` (*time travel*).

### H2 — El prompt y el esquema de destilación no fijan vocabularios ni separación de roles
- **Dónde:** `src/infrastructure/llm/claude_headless_distiller.rs:56-74` (prompt); `src/application/dtos/ingestion.rs:57-66` y `:98-103` (campos `String` libres, sin `enum`).
- **Causa raíz:** el prompt no enumera las fases canónicas (`RECEPCION`, `CUSTODIA`, `INSPECCION`…) ni los tipos de relación de §4 (`EXIGE_LLENAR_REGISTRO`, `DEROGA_A`…), y el JSON Schema generado con `schemars` no los restringe. Tampoco se invoca con `--output-format json --json-schema`, como pide §5.2: se parsea texto libre.
- **Efecto observado:**
  - Fases con tilde o calificadores (`RECEPCIÓN`, `SOLDEO (REQUISITOS POSTERIORES)`): un `WHERE fase_operativa = 'RECEPCION'` no encuentra nada.
  - `rol_responsable` mezcla ejecutor y firmante.
  - `tipo_relacion` inventado (`REFERENCIA`, `DEROGA`).
  - No hay aristas hacia los registros.
  - La salida no es determinista: la misma Rev. 03 dio 7, 9 y 5 tuplas en tres ejecuciones. Las fases auxiliares varían de nombre (`SOLDEO`, `SOLDEO - PRECALENTAMIENTO`, `SOLDEO (REQUISITOS POSTERIORES)`); las tres fases objetivo, en cambio, salieron siempre separadas.

### H3 — La Rev. 04 no retira la vigencia de las aristas del grafo (bug)
- **Dónde:** `src/infrastructure/storage/lancedb_store.rs:118` (filtro `doc_id = '…'`) y `:121-133` (errores descartados con `if let Ok`). La tabla 06 usa `doc_origen_id` (`schemas.rs:109`).
- **Causa raíz:** el mismo filtro SQL se aplica a las 6 tablas; en la 06 la columna `doc_id` no existe, el `update` falla y el error se ignora en silencio.
- **Efecto:** tras la Rev. 04 quedan vigentes 13 aristas (6 antiguas y 7 nuevas). `traverse_normative_graph` mezclará relaciones derogadas.
- **Test:** `hallazgo_rev04_retira_vigencia_de_las_aristas_del_grafo`.

### H4 — Los IDs primarios se repiten entre revisiones (bug)
- **Dónde:** `src/application/use_cases/document_ingestion_service.rs:171` (`{doc}-CHK-{idx}`), `:291` (`-TPL-`), `:358` (`-TBL-`), `:369` (`-CLD-`), `:455` (`-REF-`).
- **Causa raíz:** los IDs solo dependen de `doc_id` y de la posición, no de la versión ni del hash.
- **Efecto:** la Rev. 03 y la Rev. 04 comparten `PO-ALM-03-CHK-3`, `PO-ALM-03-TPL-0`… Cualquier citación, join o `chunk_origen_id` por ID es ambiguo.
- **Test:** `hallazgo_ids_no_colisionan_entre_revisiones`.

### H5 — `hash_sha256` se calcula pero no evita re-ingestas
- **Dónde:** `document_ingestion_service.rs:92-94`.
- **Causa raíz:** no se consulta el catálogo antes de ingerir. Re-ingerir el mismo archivo vuelve a llamar a Claude (≈ 35 s y coste) y duplica filas; la segunda copia invalida a la primera.
- **Test:** `hallazgo_reingesta_del_mismo_documento_no_duplica_filas`.

### H6 — La ingesta no está cableada en la aplicación (pieza que falta)
- **Dónde:** `grep` sobre `src/`: `DocumentIngestionService`, `ClaudeHeadlessDistiller::new`, `FastEmbedEngine::try_new` e `inicializar_tablas` no se usan fuera de los tests.
- **Efecto:** la ingesta funciona como biblioteca, pero ni la GUI ni el CLI ni el MCP pueden ejecutarla, y nadie crea las 6 tablas en el directorio de un centro (`lakehouse_data/`, §3.2). Esto encaja con la nota de la Sesión 8: «pendiente el comando `ingestar_documento`».

### H7 — Los chunks no guardan el breadcrumb jerárquico
- **Dónde:** `document_ingestion_service.rs:173` y `markdown_parser.rs:35-79`.
- **Causa raíz:** cada sección guarda solo su código y título. No se mantiene la pila de encabezados padre que §4 exige en `breadcrumb_jerarquia`, y el texto que se vectoriza no incluye el breadcrumb.
- **Test:** `hallazgo_chunks_guardan_breadcrumb_jerarquico`.

### H8 — El parser pierde el título y las notas de la tabla (impacto bajo hoy)
- **Dónde:** `markdown_parser.rs:144-152` (título solo si la línea anterior no está vacía) y `:156-164` (una sola línea de nota, y solo si empieza por `*` o `Nota:`).
- **Efecto actual:** nulo, porque el destilador envía `contenido_completo` a Claude y `ParsedDocument.tablas` solo lo usa el `MockDistiller`. Pasará a importar si la destilación se hace por secciones o tablas (§5.2: «cada sección lógica»).

### H9 — `doc_id` se interpola en SQL sin escapar
- **Dónde:** `lancedb_store.rs:118`. `DocId::new` acepta comillas (`src/domain/models/document.rs:16-39`).
- **Efecto:** con un `doc_id` como `PO-ALM-03'B`, el `update` falla en silencio y las dos revisiones quedan vigentes. Es además un vector de inyección, porque el `doc_id` lo decide el LLM.
- **Test:** `hallazgo_doc_id_con_comilla_retira_vigencia`.

### Observaciones menores (no bloquean la Etapa 1)
- **O1 — `claude -p` recibe el prompt como argumento** (`claude_headless_distiller.rs:86`). Con el fixture son unos 9 KB y funciona, pero en Windows el límite de la línea de comandos es de unos 32 KB: un documento real lo superará. `ClaudeAgentRunner` ya resolvió esto pasando el texto por stdin (Sesión 8).
- **O2 — Timeout por defecto de 60 s** (`:26`). Basta para este fixture (33-39 s), pero va justo para documentos mayores. Además no hay `kill_on_drop`: si salta el timeout, el proceso `claude` queda vivo. Tampoco se usa `CREATE_NO_WINDOW` (en la GUI abriría consolas) ni `--tools ""` / `--no-session-persistence` / `--strict-mcp-config`, como sí hace el runner.
- **O3 — Modelo de embeddings solo en inglés.** `FastEmbedEngine::try_new(None)` usa `BGELargeENV15` (`fastembed_engine.rs:20`), no BGE-M3 multilingüe, y los documentos están en español. Afecta a la calidad de la Etapa 4. La caché por defecto es `.fastembed_cache` relativa al directorio de trabajo (1,3 GB), configurable con `FASTEMBED_CACHE_DIR`.
- **O4 — Trampa de LanceDB 0.15.** `table.query()` sin `.limit()` devuelve como máximo 10 filas (`Query::new` fija `limit: Some(DEFAULT_TOP_K)`, aunque la documentación dice «sin límite»). El primer borrador de este test cayó en ella. Cualquier cargador de la Etapa 2 (LanceDB → DataFusion/Petgraph) debe fijar el límite o usar `count_rows`.
- **O5 — `retirar_vigencia_documento` no filtra por `centro_id`.** Hoy no importa, porque cada centro tiene su propio directorio y su propio `LanceDbStore`, pero sí importaría si se compartiera el almacén.

## 4. Tiempos medidos

Máquina local Windows 11, perfil `dev` (sin optimizar), Claude Code 2.1.296 con el modelo `sonnet`. Donde hay dos cifras, corresponden a la ejecución 1 / ejecución 2.

| Paso | Rev. 03 | Rev. 04 |
| --- | ---: | ---: |
| Carga del modelo FastEmbed (1.ª vez, con descarga de 1,3 GB) | 20,9 s | — |
| Carga del modelo FastEmbed (en caché) | 1,4–4,5 s | — |
| 1. Parseo Markdown | < 1 ms | < 1 ms |
| 2. Destilación con Claude (`claude -p`) | 35,0 / 34,2 s | 38,8 / 28,6 s |
| 3. Embeddings (9 chunks, CPU) | 6,8 / 6,4 s | 5,6 / 6,3 s |
| 4. Retirar vigencia previa (6 `update`) | 0,04 s | 0,08 s |
| 4. Escrituras LanceDB (6 tablas) | 0,09 s | 0,08 s |
| **Total `ingestar_documento`** | **41,9 / 40,8 s** | **44,6 / 35,1 s** |
| Destilación aislada con la config. por defecto (timeout 60 s) | 32,7 s | — |
| Pipeline con mocks (Rev. 03 + Rev. 04) | 0,10 s | 0,09 s |

**Conclusión de tiempos:** Claude supone más del 80 % del tiempo y FastEmbed en CPU alrededor del 15 %. LanceDB es despreciable.

## 5. Conclusión

**Sí: la ingesta real de punta a punta funciona como biblioteca.** Con Claude real, FastEmbed real y LanceDB en disco, el documento de prueba llega a las 6 tablas sin errores en 35-45 s por revisión.

Lo más difícil sale muy bien:

- La tabla de doble entrada se desnormaliza perfectamente: rangos, inclusividad, valor exacto 100 → 125 y nota al pie `(*)` correcta.
- Las tres fases quedan en filas separadas.
- El versionado Rev. 03 → Rev. 04 funciona en las tablas 01 a 05.

Pero **no cumple todavía la especificación** en cuatro puntos que afectan a las etapas siguientes:

1. **El modelo de datos de la matriz no es el de §4 (H1).** Sin `rol_ejecutor` ni `registro_codigo`, la respuesta a §9.2 depende de que el LLM del chat interprete texto libre como `"Auxiliar de Almacén (ejecutor); Jefe de Producción (firma)"`. Así no hay garantía de anti-fusión a nivel de columna.
2. **Los vocabularios no son canónicos (H2).** Las consultas SQL por fase (`'RECEPCION'`) y por relación (`'DEROGA_A'`) fallarán en la Etapa 3.
3. **El grafo arrastra aristas derogadas (H3) y los IDs colisionan entre revisiones (H4).**
4. **Nada en la aplicación invoca la ingesta (H6).** El usuario no puede ingerir desde la GUI ni el MCP. Por eso «la app solo da errores»: los motores arrancan sobre tablas vacías.

## 6. Arreglos propuestos (priorizados; NO aplicados)

| Prioridad | Arreglo | Hallazgo | Esfuerzo |
| :---: | --- | :---: | :---: |
| P0 | `retirar_vigencia_documento`: usar la columna de documento de cada tabla (`doc_origen_id` en la 06), **propagar el error** en lugar de `if let Ok` y escapar la comilla (`'` → `''`) o validar el `DocId` con un charset `[A-Za-z0-9._ -]`. | H3, H9 | S |
| P0 | Cablear la ingesta en la composición (`main.rs`): comando Tauri `ingestar_documento` y/o tool MCP que creen `<centro>/lakehouse_data`, llamen a `inicializar_tablas` y ejecuten el servicio con `ClaudeHeadlessDistiller` + `FastEmbedEngine`. | H6 | M |
| P1 | Alinear el DTO de destilación y el esquema 03 con §4: `rol_ejecutor`, `rol_responsable`, `registro_codigo`, `registro_nombre`, `orden_secuencia`, `objeto_alcance`, `criterio_aceptacion`, `chunk_origen_id`. Alinear también el 01 (`revision`, `fecha_vigor`, `tipo_documento`). Es un cambio estructural, así que requiere SI/NO del usuario y afecta a las tools de la Fase 6. | H1 | M-L |
| P1 | Prompt y JSON Schema con `enum` para `fase_operativa` y `tipo_relacion` (vocabulario de §4), instrucción explícita «una tupla por fase con ejecutor y responsable separados» y aristas `EXIGE_LLENAR_REGISTRO` por cada registro. Invocar `claude -p --output-format json --json-schema` y normalizar las fases en Rust (mayúsculas, sin tildes) como red de seguridad. | H2 | S-M |
| P1 | IDs estables por revisión: `{doc_id}@v{version}-CHK-{idx}` o un prefijo con el hash corto. | H4 | S |
| P2 | Comprobar `hash_sha256` en `01_doc_catalog` antes de llamar a Claude y devolver `IngestionReport` «ya ingerido». | H5 | S |
| P2 | Distiller: prompt por stdin, `kill_on_drop(true)`, `CREATE_NO_WINDOW`, `--tools ""`, `--no-session-persistence`, `--strict-mcp-config` y un timeout de 180 s o más (reutilizar el patrón de `ClaudeAgentRunner`). | O1, O2 | S |
| P2 | Breadcrumb jerárquico en el parser y en los chunks; vectorizar `breadcrumb + texto`. | H7 | S |
| P3 | Modelo de embeddings multilingüe (BGE-M3 o `MultilingualE5Large`, 1024 dims) y `FASTEMBED_CACHE_DIR` bajo el directorio de datos de la app. | O3 | S |
| P3 | Parser: título de tabla con línea en blanco previa y notas multilínea `(*)`, `(1)`, `(a)`. | H8 | S |

## 7. Notas para la Etapa 2 y siguientes

- Reutilizar los fixtures y `tests/fixtures/README.md` como oráculo.
- Para no gastar llamadas a Claude, los JSON destilados reales de la Rev. 03 y la Rev. 04 están en `target/validacion_etapa1/*.destilado.json` (no versionados) y pueden reinyectarse con `MockDistiller::registrar_respuesta`.
- Al cargar LanceDB en DataFusion/Petgraph, fijar `.limit()` explícito (O4) y filtrar `vigente = true` en el grafo, ya que H3 deja aristas derogadas vigentes.
- Cruce de control para `lookup_double_entry_table`: `espesor = 30`, `CE = 0.42` → `100 °C (*)` (Rev. 03) o `125 °C (*)` (Rev. 04).
