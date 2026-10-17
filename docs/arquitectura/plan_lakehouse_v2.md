# Plan Lakehouse RAG v2: control de los 8 problemas del RAG de procedimientos

> **Estado:** propuesta pendiente de aprobación (SI/NO) · **Fecha:** 2026-10-17
> **Base:** especificación `docs/especificaci_n_t_cnica_lakehouse_rag_en_rust.md` §3–§9 e informe `docs/validacion/etapa1_ingesta.md`.
> **Documento vivo:** cada fase marca aquí su avance. Es la fuente de verdad para retomar el trabajo en un contexto nuevo.

## 0. Principio rector

**Una sola fuente de verdad: LanceDB por centro.** DataFusion (SQL) y Petgraph (grafo) son *vistas derivadas* de solo lectura que se reconstruyen desde LanceDB al activar un centro o al terminar una ingesta. No hay tres almacenes que sincronizar, sino uno, más proyecciones desechables.

## 1. Matriz problema → control → verificación

| # | Problema | Control en el diseño | Test de aceptación (oráculo en `tests/fixtures/`) |
|---|---|---|---|
| 1 | Contaminación de atributos | Tabla 03 v2: **una fila por hito** con `fase_operativa` (enum canónico), `rol_ejecutor`, `rol_responsable`, `registro_codigo` y `registro_nombre` en columnas separadas; `vector_hito` calculado solo sobre su fila; `query_operational_matrix` filtra por columna exacta. Validación en Rust: si ejecutor = responsable o el registro está vacío, se marca `calidad_extraccion = REVISAR`. | «¿Responsable de CUSTODIA y qué registro llena?» → solo `Jefe de Producción` + `FOR-ALM-02`, nunca el de RECEPCIÓN. |
| 2 | Geometría de tablas | Tablas 04/05 v2: ejes tipados (`RANGO_NUMERICO`/`CATEGORIA_EXACTA`), límites con inclusividad, `valor_resultado_num`, `nota_pie_condicion` por celda. `lookup_double_entry_table` resuelve en Rust con semántica `>=`/`>` exacta (sin vectores ni nodos por celda). | Cruce e = 30, CE = 0,42 → `100 °C (*)`; fronteras e = 20 y e = 20,0001 caen en columnas distintas. |
| 3 | Versiones obsoletas / *time travel* | Tabla 01 v2: `revision`, `version_num`, `fecha_vigor` (Date32), `fecha_derogacion` (Date32 nullable), `vigente`. Las 6 tablas llevan `doc_id` + `version_num`. Los IDs incluyen la versión (`PO-ALM-03@v4-HIT-2`). La retirada de vigencia cubre las 6 tablas (arregla H3), propaga errores y usa parámetros escapados (H9). Todas las tools aceptan `fecha_referencia` opcional: «vigente en D» = `fecha_vigor <= D AND (fecha_derogacion IS NULL OR fecha_derogacion > D)`. | Tras la Rev. 04, la consulta normal da `125`; con `fecha_referencia` anterior a la Rev. 04 da `100`; 0 aristas de la Rev. 03 vigentes. |
| 4 | Supernodos / hairball | El grafo solo modela **documentos, hitos, registros y tablas** (no entidades genéricas como roles o materiales). Recorrido con: solo aristas vigentes, filtro por `tipo_relacion`, máximo 2 saltos, tope de vecinos por nodo (`max_vecinos`, por defecto 15) con poda por grado (los nodos con grado mayor que el umbral se devuelven como «hub resumido» sin expandir) y ranking por prelación. Respuesta acotada (≤ N nodos). | Grafo sintético con un hub de 1.000 aristas: el recorrido de 2 saltos devuelve ≤ 50 nodos e indica `hub_resumido`. |
| 5 | Prelación normativa | `tipo_documento` (enum) → `nivel_prelacion` (1 = cliente … 5 = norma) desnormalizado en las tablas 02, 03 y 05. La búsqueda y las tools devuelven los resultados agrupados por tema con `prevalece: true` en el de menor nivel; el system prompt obliga a citar el que prevalece y a mencionar el supletorio. | Pliego cliente «ensayar 100 %» frente a norma «10 %» → la respuesta dice 100 %, cita el pliego y menciona la norma como supletoria. |
| 6 | Agregación global | `execute_athena_sql` (DataFusion) sobre las **tablas completas** registradas desde LanceDB, no top-K. Solo `SELECT`/`WITH`, validado con el parser de DataFusion, sin DDL/DML, con límite de filas configurable. Plantillas de consulta en el glue catalog. | «Procedimientos vigentes donde interviene el Jefe de Producción y formatos que firma» → lista exhaustiva igual a la del oráculo. |
| 7 | Alcance condicional | El parser mantiene una **pila de encabezados** → `breadcrumb_jerarquia`. La `condicion_aplicacion` de un encabezado (por ejemplo «bajo cero (< 0 °C)») se **hereda** a todos los chunks e hitos descendientes. El embedding se calcula sobre `breadcrumb + condición + texto`. Las tools devuelven la condición junto al dato. | El párrafo «calentar a 80 °C» se devuelve siempre con `condicion_aplicacion = "T ambiente < 0 °C"`. |
| 8 | Desincronización tri-store | Ingesta **transaccional por `ingest_id`**: (1) copia inmutable en `raw_docs/` con SHA-256; (2) idempotencia por hash (si ya existe → «ya ingerido», sin llamar a Claude); (3) escritura de las 6 tablas con `estado_ingesta = STAGED`; (4) *commit*: retirar la vigencia de la versión previa y activar la nueva; (5) ante cualquier error → `delete where ingest_id = X` (rollback); (6) reconstrucción de las vistas DataFusion/Petgraph. Comando `verificar_integridad` que detecta huérfanos. | Fallo inyectado en la tabla 05 → 0 filas residuales del `ingest_id` y la versión previa sigue vigente. |

## 2. Contratos (cambios estructurales que requieren aprobación)

### 2.1 Dominio (`src/domain/`, Rust puro)
- `FaseOperativa` (enum + `normalizar(&str)`: mayúsculas, sin tildes, sinónimos → canónico; valor `OTRA(String)` para las no previstas): `RECEPCION, DESCARGA, CUSTODIA, INSPECCION, ENSAYO, LIBERACION, EJECUCION, …`.
- `TipoRelacion` (enum cerrado): `EXIGE_LLENAR_REGISTRO, DEROGA_A, MODIFICA_APARTADO, REMITE_A_TABLA, SUBORDINADO_A_NORMA, REFERENCIA_A`.
- `TipoDocumento` → `NivelPrelacion` (`ESPECIFICACION_CLIENTE=1, PROCEDIMIENTO_OBRA=2, PROCEDIMIENTO_GENERAL=3, INSTRUCTIVO_TECNICO=4, NORMA_INTERNACIONAL=5, FORMATO_REGISTRO=5`).
- `VentanaVigencia { fecha_vigor, fecha_derogacion }` con `vigente_en(fecha)`.
- `DocId`: conjunto de caracteres `[A-Za-z0-9._-]` (rechaza comillas → H9). `IdVersionado::nuevo(doc, version, tipo, idx)`.
- `Breadcrumb` y `CondicionAlcance` con herencia.
- Evaluador de celdas `RangoEje::contiene(valor)` (ya existe; se amplía con la inclusividad por extremo).

### 2.2 Esquemas Arrow v2 (`schemas.rs`) — se alinean con §4 de la especificación
Las 6 tablas pasan a los nombres de §3.2 (`01_documentos_catalogo` … `06_grafo_referencias`). Columnas comunes a todas: `centro_id`, `doc_id`, `version_num` (Int32), `vigente`, `ingest_id`, `estado_ingesta`. Además de §4 se añaden: `fecha_derogacion` (01), `condicion_aplicacion` (03) y `nivel_prelacion` desnormalizado (02/03/05). `center_manifest.json` guarda `schema_version = 2` y el modelo de embeddings.

### 2.3 Puertos (`src/application/ports/`)
- `LakehouseStorePort`: `existe_hash`, `escribir_staged(ingest_id, lotes)`, `confirmar_ingesta(ingest_id, doc_id)`, `revertir_ingesta(ingest_id)`, `leer_tabla(nombre) -> Vec<RecordBatch>` (con límite explícito, O4) y `verificar_integridad`.
- `CenterRegistryPort` en disco (`centers_registry.json`): crear, activar, desactivar y listar centros.
- `LakehouseProjectionPort` (nuevo): `reconstruir_vistas(centro)` → registra en DataFusion y carga Petgraph desde LanceDB.
- `SemanticSearchPort`: búsqueda híbrida con filtros `vigente_en`, `nivel_prelacion` y `centro`.

### 2.4 Tools del agente (6)
`inspect_lakehouse_catalog`, `query_operational_matrix`, `lookup_double_entry_table`, `execute_athena_sql`, `traverse_normative_graph` y una nueva, **`search_normative_context`** (RAG semántico con breadcrumb, condición, prelación y *time travel*). Todas aceptan `fecha_referencia`.

### 2.5 Destilación
- Prompt y JSON Schema con `enum` para fase, relación y tipo de documento, y separación explícita ejecutor/responsable/registro.
- Invocación `claude -p --output-format json --json-schema` por stdin, con `kill_on_drop`, `CREATE_NO_WINDOW`, `--tools ""` y un timeout de 300 s.
- Normalización y validación en Rust como red de seguridad.

### 2.6 Embeddings
Modelo **multilingüe** de 1024 dims (`MultilingualE5Large` de fastembed; BGE-M3 si está disponible) con caché en el directorio de datos de la app.

## 3. Fases de ejecución (TDD Rojo → Verde → Refactor; suite adversarial al cierre de cada una)

| Fase | Contenido | Problemas | Estado |
|---|---|---|---|
| **9** | Dominio v2: enums canónicos, prelación, vigencia, DocId seguro, IDs versionados, breadcrumb y condición | 1, 3, 5, 7 | Completado (Sesión 9, 2026-10-17; 180 tests) |
| **10** | Esquemas v2 y store transaccional: staged/commit/rollback, idempotencia por hash, retirada de vigencia en las 6 tablas (`raw_docs` pasa a la 12) | 3, 8 | Completado (Sesión 10, 2026-10-17; 214 tests) |
| **11** | Ingesta v2: parser con breadcrumb y herencia de condición; destilador con json-schema y enums; normalización; vectores hito/tabla; embeddings multilingües | 1, 2, 7 | Completado (Sesión 11, 2026-10-17; 246 tests; E2E real 52/52 OK) |
| **12** | Centros en disco + proyección LanceDB → DataFusion/Petgraph (activar/desactivar, reconstrucción tras ingesta) + `raw_docs/` y `center_manifest.json` (`schema_version = 2`) | 6, 8 | Completado (Sesión 12, 2026-10-17; 291 tests; puerta E2E con el binario real) |
| **13** | Tools v2: matriz exacta, lookup con fronteras, SQL de solo lectura, grafo antisupernodos, `search_normative_context` con prelación y *time travel* | 2, 3, 4, 5, 6, 7 | Completado (Sesión 13, 2026-10-17; 322 tests; puerta E2E real 1/1 OK, 6 preguntas) |
| **14** | Cableado de la app: `ingestar_documento` (GUI, MCP y CLI), gestión de centros en la GUI, system prompt de prelación y citas | todos | Pendiente |
| **15** | Aceptación: suite `tests/acceptance_8_problemas.rs` (Claude y FastEmbed reales) + criterios §9 + prueba manual de la GUI | todos | Pendiente |

## 3.1 Criterio de terminado: la app funciona de punta a punta (exigido por el usuario)
- **Fachada única `LakehouseApp`** (composición en un solo punto) que usan la GUI (comandos Tauri), el MCP y la CLI (`--ingest <archivo> --centro <id>`, `--ask "<pregunta>" --centro <id> --modelo <alias>`, `--centros`). La prueba E2E ejecuta el **binario real** con la misma composición que la GUI.
- **Puerta E2E al cierre de cada fase desde la 12:** `tests/e2e_binario_*.rs` (`#[ignore]`, Claude y FastEmbed reales) lanza `target/debug/lakehouse_rag`: crea un centro en un tempdir, ingiere los fixtures, hace las preguntas del oráculo y compara las respuestas. La salida se adjunta al informe de la fase.
- **Fase 15:** prueba manual guiada de la ventana Tauri (arranque, ingesta por *drag & drop* o selector, chat, cambio de modelo en caliente y visor de Studio) con capturas.

## 4. Fuera de alcance de esta iteración (propuesto)
- Parser de PDF/DOCX: se hará en una fase posterior mediante un subproceso `docling` (§5.1). Hasta entonces, entrada Markdown (y PDF convertido externamente).

## 5. Protocolo de continuidad
Al superar el 50 % de contexto, la sesión entrega el trabajo a una sesión nueva. Antes de hacerlo deja `PROGRESS.md` y este documento al día (fase actual, tests, decisiones), con todo commiteado y la suite en verde.
