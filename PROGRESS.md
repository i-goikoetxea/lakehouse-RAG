# PROGRESS.md — Tablero Maestro de Control y Progreso

> **Fuente Única de Verdad:** Este documento refleja el estado en tiempo real del desarrollo de la aplicación **Lakehouse RAG Industrial en Rust**. Cada sesión actualiza su progreso antes de finalizar.

---

## 1. Matriz General de Fases

| Fase | Título de la Fase | Sesión | Estado | Tests Pasando |
| :---: | :--- | :---: | :---: | :---: |
| **0** | Cimientos, Gobernanza y Estructura Cargo | Sesión 0 | `[COMPLETADO]` | 1 / 1 |
| **1** | Dominio Puro y Gestión Multi-Centro | Sesión 1 | `[COMPLETADO]` | 34 / 34 |
| **2** | Adaptador LanceDB y los 6 Esquemas Arrow | Sesión 2 | `[COMPLETADO]` | 46 / 46 |
| **3** | Motor Analítico DataFusion y Grafo Petgraph | Sesión 3 | `[COMPLETADO]` | 61 / 61 |
| **4** | Embeddings Locales y Búsqueda Híbrida | Sesión 4 | `[COMPLETADO]` | 70 / 70 |
| **5** | Pipeline de Ingesta y Destilación Claude Headless | Sesión 5 | `[COMPLETADO]` | 88 / 88 |
| **6** | Servidor de Herramientas Data Lake y Agente | Sesión 6 | `[COMPLETADO]` | 108 / 108 |
| **7** | Servidor MCP y App Gráfica (Linear + NotebookLM 16:9) | Sesión 7 | `[EN PROGRESO]` | 124 / 124 |
| **8** | Auditoría Adversarial E2E y Criterios DoD | Sesión 8 | `[NO INICIADO]` | - |
| **9** | Dominio v2: enums canónicos, prelación, vigencia, DocId seguro, IDs versionados, breadcrumb y condición | Sesión 9 | `[COMPLETADO]` | 180 / 180 |
| **10** | Esquemas v2 y store transaccional (staged/commit/rollback, idempotencia por hash) | Sesión 10 | `[COMPLETADO]` | 214 / 214 |
| **11** | Ingesta v2: breadcrumb, herencia de condición, destilador con json-schema y enums | Sesión 11 | `[COMPLETADO]` | 246 / 246 |
| **12** | Centros en disco y proyección LanceDB → DataFusion/Petgraph | Sesión 12 | `[COMPLETADO]` | 291 / 291 |
| **13** | Tools v2: matriz exacta, lookup, SQL de solo lectura, grafo antisupernodos, `search_normative_context` | Sesión 13 | `[COMPLETADO]` | 322 / 322 |
| **14** | Cableado de la app: `ingestar_documento`, gestión de centros, system prompt de prelación | Sesión 14 | `[COMPLETADO]` | 341 / 341 |
| **15** | Aceptación: `tests/acceptance_8_problemas.rs` y prueba manual de la GUI | Sesión 15 | `[COMPLETADO]` | 367 / 367 + 19 ignorados (aceptación real 30/30 en 3 ejecuciones) |



### Proyecto STAIR (rama `feat/stair-rag-dual`; contrato en `docs/arquitectura/decisiones_stair.md`)

| Fase STAIR | Título | Estado | Tests |
| :---: | :--- | :---: | :---: |
| **1** | Esquemas: tabla 07, columnas de 02 y 01, `schema_version` 3 | `[COMPLETADO]` (pendiente de commit por la sesión orquestadora) | Solo suites específicas; ver Bitácora |
| **2** | Ingesta universal | `[COMPLETADO]` (pendiente de commit por la sesión orquestadora) | Solo suites específicas; ver Bitácora |
| **3** | PDF | `[COMPLETADO]` | Solo suites específicas; ver Bitácora |
| **4** | Búsqueda y tools | `[COMPLETADO]` (pendiente de commit por la sesión orquestadora) | Solo suites específicas; ver Bitácora |
| **5** | Aceptación | `[COMPLETADO]` (pendiente de commit; merge congelado hasta la revisión del usuario) | 522 pasan, 29 ignorados; E2E real 4/4 en 3 ejecuciones |
| **6** | Calidad de datos: fecha nula, apartados, origen del responsable | `[COMPLETADO]` | 566 pasan, 31 ignorados |
| **7** | Hitos, roles e índice para documentos largos; esquema v5 | `[COMPLETADO]` (pendiente de commit; merge congelado hasta la revisión del usuario) | 600 pasan, 31 ignorados; aceptación real 10/10 en 5 pasadas seguidas |

### Citas de página exacta (rama `claude/citas-pagina-209ed5`; contrato en `docs/arquitectura/ssd_citas_pagina.md`)

| Fase | Título | Estado | Tests |
| :---: | :--- | :---: | :---: |
| **1** | Guardar y mostrar: `saltos_pagina` en 02 (esquema v6), marcas `⟦p. N⟧`, `titulo` y `fecha_vigor` en `cita` | `[COMPLETADO]` (pendiente de aprobación del usuario; sin push ni merge) | 913 pasan, 34 ignorados |
| **2** | Comprobar: `VerificadorPaginas` en el arnés | `[COMPLETADO]` (pendiente de aprobación del usuario; sin push ni merge) | 913 pasan, 34 ignorados; Aguja 5 × 3 con `pagina_exacta_estricta` 1,00 |

*Estados posibles:* `[NO INICIADO]`, `[EN PROGRESO]`, `[COMPLETADO]`, `[BLOQUEADO]`

---

## 2. Desglose Detallado por Fases

### Fase 0: Cimientos, Gobernanza y Estructura Cargo
- [x] Creación de `AGENTS.md` con directivas de Clean Code, Hexagonal y TDD.
- [x] Creación de `PROGRESS.md` con seguimiento granular.
- [x] Configuración del manifiesto `Cargo.toml` con dependencias estrictas del stack (`arrow`, `datafusion`, `lancedb`, `fastembed`, `petgraph`, `ratatui`, `tokio`).
- [x] Creación de la estructura de módulos hexagonal (`src/domain/`, `src/application/`, `src/infrastructure/`, `src/presentation/`).
- [x] Verificación de compilación base (`cargo check`), tests (`cargo test`), linter (`cargo clippy -- -D warnings`) y formato (`cargo fmt --check`).

### Fase 1: Dominio Puro y Gestión Multi-Centro
- [x] Entidades de dominio: `CentroConocimiento`, `CenterManifest`, `DocumentoCatalogo`.
- [x] Value Objects tipados: `DocId`, `NivelPrelacion` (1 a 5), `RangoNumerico` (evaluación `>=, <=, >, <`), `FaseOperativa`.
- [x] Modelado de `TuplaOperativa`, `CeldaDobleEntrada` y `AristaGrafo`.
- [x] Errores de dominio tipados (`DomainError`).
- [x] Puertos de repositorio para registro de centros (`CenterRegistryPort`).
- [x] Tests unitarios TDD de dominio al 100% (sin I/O ni dependencias pesadas).

### Fase 2: Adaptador LanceDB y los 6 Esquemas Arrow
- [x] Definición estricta de esquemas Apache Arrow para las 6 tablas (`01` a `06`).
- [x] Implementación de repositorios sobre LanceDB (`.lance`).
- [x] Mecanismo transaccional de control de versiones (`vigente = false` para versiones previas al insertar nueva revisión).
- [x] Tests de integración TDD en directorio temporal (`tempfile`).

### Fase 3: Motor Analítico DataFusion y Grafo Petgraph
- [x] Configuración de `SessionContext` de Apache DataFusion con registro dinámico de centros activos.
- [x] Implementación determinista de `lookup_double_entry_table` para cruce de rangos numéricos y categorías.
- [x] Servicio de hidratación de subgrafos normativos en memoria con `petgraph`.
- [x] Tests TDD de consultas SQL deterministas y aislamiento estricto de centros.

### Fase 4: Embeddings Locales y Búsqueda Híbrida
- [x] Adaptador de embeddings locales con `fastembed` (BGE-M3 / multilingual).
- [x] Búsqueda híbrida combinada: Vectorial (`IVF-PQ`/`HNSW`) + Filtros Arrow + BM25 Full-Text Search.
- [x] Tests TDD de vectorización y scoring de relevancia híbrida.

### Fase 5: Pipeline de Ingesta y Destilación Claude Headless
- [x] Parser documental preservando jerarquía estructural de títulos y tablas.
- [x] Invocador de Claude Headless (`claude -p`) con salida estructurada JSON Schema para desnormalización.
- [x] Orquestador de ingesta atómica hacia LanceDB.
- [x] Tests TDD con fixtures de documentos técnicos de muestra.

### Fase 6: Servidor de Herramientas Data Lake y Agente
- [x] Implementación de las 5 Tools:
  - `inspect_lakehouse_catalog`
  - `query_operational_matrix`
  - `lookup_double_entry_table`
  - `execute_athena_sql`
  - `traverse_normative_graph`
- [x] Selector dinámico de modelos Claude (`sonnet`, `opus`, `haiku`) y wrapper asíncrono con streaming.
- [x] Tests TDD de ejecución de tools y parsing de eventos JSON.

### Fase 7: Servidor MCP y Aplicación Gráfica (Linear + NotebookLM Frameless 16:9)
- [x] Servidor MCP Stdio (`--mcp-server`) con protocolo JSON-RPC 2.0 compatible con Claude Desktop, Claude Code y Gemini.
- [x] Configuración de ventana de escritorio frameless (sin bordes del SO, aspecto 16:9 inicial de 1280x720, controles propios de minimizar/maximizar/cerrar y tema claro/oscuro).
- [x] Panel 1 (Fuentes & Ingesta): Selector de centros y zona de Drag & Drop interactiva con progreso en vivo del pipeline.
- [x] Panel 2 (Chat & Grounding): Consola conversacional con tarjetas visuales colapsables de Tool Use y citas normativas al pie.
- [x] Panel 3 (Studio & Lakehouse Explorer): Visor interactivo de los 6 esquemas Arrow (catálogo, matriz, tablas cruzadas y grafo).
- [x] Tests de integración del protocolo MCP y lógica UI.
- [x] Herramientas de autoinstalación en la raíz: `MCP_GUIDE.md`, `install_mcp.ps1` y comando CLI `--mcp-info`.
- [x] Chat de la GUI conectado a Claude Code headless mediante comandos Tauri (`enviar_mensaje_chat`, `listar_modelos_claude`) con catálogo dinámico de modelos.
- [ ] Comando Tauri `ingestar_documento` (el frontend aún solo envía el nombre del archivo, no su ruta/contenido).

### Fase 8: Auditoría Adversarial E2E y Criterios DoD
- [ ] Prueba de Multi-Centro: Verificación de aislamiento estricto al alternar centros activos.
- [ ] Prueba Anti-Fusión: Verificación de aislamiento entre Recepción, Custodia e Inspección.
- [ ] Prueba de Tabla de Doble Entrada: Verificación de cruce numérico exacto y notas al pie.
- [ ] Prueba de Selector Claude Headless: Conmutación de modelos y observabilidad de herramientas.
- [ ] Reporte final de cierre y documentación técnica.

### Fase 9: Dominio v2 (plan `docs/arquitectura/plan_lakehouse_v2.md` §2.1)
- [x] `services/normalizacion.rs`: `plegar_mayusculas`, `normalizar_token`, `codigo_canonico`, `separar_revision`.
- [x] `models/alcance.rs`: `extraer_condicion` y `PilaEncabezados` (breadcrumb y condición heredada).
- [x] `models/identificadores.rs`: `TipoEntidad` e `id_versionado` (`PO-ALM-03@v3-HIT-0002`).
- [x] `models/vigencia.rs`: `VentanaVigencia` (intervalo semiabierto, derogación inmutable).
- [x] `FaseOperativa` canónica en MAYÚSCULAS con `Otra(String)` normalizada.
- [x] `TipoRelacionNormativa` cerrado (§4) con `parse` estricto y `parse_con_respaldo`.
- [x] `TipoDocumento` → `NivelPrelacion`.
- [x] `DocId` con conjunto `[A-Za-z0-9._-]` (H9) y `DocId::desde_codigo_libre`.
- [x] `HitoOperativo` (sustituye a `TuplaOperativa`) con `alertas_calidad`, `calidad_extraccion` y `texto_vectorizable`.
- [x] Suite adversarial `tests/adversarial_dominio_v2_tests.rs`.

### Fase 10: Esquemas v2 y store transaccional (plan v2 §1 filas 3 y 8, §2.2, §2.3)
- [x] Esquemas Arrow v2 (`schemas.rs`): tablas `01_documentos_catalogo` … `06_grafo_referencias`, columnas comunes (`centro_id`, `doc_id`, `version_num`, `vigente`, `ingest_id`, `estado_ingesta`) y `VERSION_ESQUEMA = 2`.
- [x] Filas tipadas `application/dtos/lakehouse_filas.rs` (`LoteIngesta`) y conversión a Arrow `infrastructure/storage/lotes_arrow.rs`.
- [x] `LakehouseStorePort` v2: `existe_hash`, `escribir_staged`, `confirmar_ingesta`, `revertir_ingesta`, `leer_tabla` (límite explícito, O4) y `verificar_integridad`.
- [x] `LanceDbStore` transaccional: errores propagados, literales escapados, compensación si la retirada de vigencia falla a medias y `fecha_derogacion` en 01 (H3, H9, O5).
- [x] `IngestId` validado en el dominio.
- [x] `DocumentIngestionService`: idempotencia por hash (H5), staged → commit con rollback, IDs versionados (H4) y enums canónicos.
- [x] Consumidores adaptados: la búsqueda híbrida solo lee `COMMITTED`; `TableLookupService` y `PetgraphEngine` leen columnas v2.
- [x] Suites `tests/integration_store_v2_transaccional.rs` y `tests/adversarial_store_v2_tests.rs`; los hallazgos H1 (esquemas), H3, H4, H5 y H9 corren sin `#[ignore]`.
- [x] `raw_docs/` (copia inmutable del original): hecho en la Fase 12 (centros en disco).

### Fase 11: Ingesta v2 (plan v2 §1 filas 1, 2 y 7; §2.5 y §2.6)
- [x] DTO de destilación v2 (`application/dtos/ingestion.rs`) alineado con §4; JSON Schema con `enum` para tipo de documento, fase, relación, tipo de eje y tipo de nodo.
- [x] `ClaudeHeadlessDistiller` v2: reglas en `--system-prompt`, documento por stdin delimitado, `--output-format json --json-schema`, `--tools ""`, `--no-session-persistence`, `--strict-mcp-config`, `kill_on_drop`, sin consola en Windows y timeout de 300 s.
- [x] Red de seguridad en Rust (`application/use_cases/mapeo_destilacion.rs`): enums del dominio, `separar_revision` + `DocId::desde_codigo_libre`, `calidad_extraccion` desde `HitoOperativo::alertas_calidad` y aristas `EXIGE_LLENAR_REGISTRO` sintetizadas.
- [x] Parser Markdown: breadcrumb completo y condición heredada (H7); título de tabla tras línea en blanco y notas multilínea `(*)`, `(1)`, `(a)` (H8).
- [x] Columnas v2 rellenas: `rol_ejecutor`, `registro_*`, `objeto_alcance`, `chunk_origen_id`, `tablas_vinculadas`, `hitos_vinculados`, unidades, `valor_salida_nombre`, `instrucciones_uso_sql`, `vector_hito` y `vector_tabla`.
- [x] Embeddings multilingües `MultilingualE5Large` (1024 dims) con caché fuera del cwd.
- [x] Suite adversarial `tests/adversarial_ingesta_v2_tests.rs` y E2E real de la Etapa 1 con todas las comprobaciones en OK.

### Fase 12: Centros en disco, proyección y fachada (plan v2 §1 filas 6 y 8, §2.3, §3.1)
- [x] `CenterRegistryPort` + `JsonCenterRegistry` (`centers_registry.json` en el directorio de configuración, inyectable; escritura atómica; registro corrupto → error sin sobrescribir; `eliminar` no toca el disco).
- [x] `CenterWorkspacePort` + `DiskCenterWorkspace`: `center_manifest.json` (`schema_version = 2`, modelo y dimensión de embeddings), `raw_docs/` (copia `<sha256>_<nombre>` + `.sha256`), `glue_catalog/` y `lakehouse_data/`; el manifiesto se valida contra el modelo activo al abrir.
- [x] `LakehouseProjectionPort` + `LanceProjection`: filas `COMMITTED` (vigentes e históricas, sin columnas vectoriales) registradas en DataFusion con prefijo de centro; Petgraph y TableLookup con lo vigente de los centros proyectados; inventario en `glue_catalog/inventario_tablas.json`; desregistro al desactivar.
- [x] Fachada `LakehouseApp` (`application/use_cases/lakehouse_app.rs`) y composición única `infrastructure/bootstrap.rs`; `main.rs`, GUI (`GuiChatService::desde_asistente` y comandos Tauri) y MCP la usan.
- [x] CLI: `--centros`, `--crear-centro`, `--activar`, `--desactivar`, `--eliminar-centro`, `--ingest`, `--ask`, `--integridad` (legible o `--json`).
- [x] Tools: lo mínimo para leer las vistas v2 (nombres canónicos `01_…`–`06_…`, columnas v2 en catálogo y matriz) e inventario v2 en el system prompt.
- [x] Suites `tests/integration_lakehouse_app.rs` (9) y `tests/adversarial_centros_proyeccion_tests.rs` (11).
- [x] Puerta E2E `tests/e2e_binario_fase12.rs` (2 tests `#[ignore]`, binario real).

### Fase 13: Tools v2 (plan v2 §1 filas 2, 3, 4, 5, 6 y 7; §2.4)
- [x] Contrato de las 6 tools (`application/dtos/tools.rs`): JSON Schema estricto escrito a mano (`additionalProperties: false`), parámetros `deny_unknown_fields`, `centro_id` obligatorio y `fecha_referencia` ISO opcional en todas (1900–9999).
- [x] Todas las tools leen las vistas DataFusion **del centro activo** (historial completo) con la ventana `fecha_vigor <= D < fecha_derogacion` de `01_documentos_catalogo`; los filtros del agente se aplican en Rust, nunca se interpolan en SQL.
- [x] `inspect_lakehouse_catalog`: documentos (tipo, prelación, revisión y vigencia) + tablas de doble entrada (ejes, unidades, `instrucciones_uso_sql`).
- [x] `query_operational_matrix`: fase con `FaseOperativa::parse`, rol exacto (ejecutor/responsable/cualquiera, sin tildes ni mayúsculas), registro, doc y objeto; filas atómicas con cita (doc_id, revisión, chunk y breadcrumb).
- [x] `lookup_double_entry_table`: tabla por id versionado o por código del título (`4-A`) o por búsqueda; cruce en Rust con la inclusividad de cada extremo; nota al pie obligatoria; `resuelto` / `sin_coincidencia` / `ambiguo` / `tabla_no_encontrada` / `tabla_ambigua`; rechaza NaN e infinitos.
- [x] `execute_athena_sql`: puerto `ejecutar_consulta_lectura` (parser de DataFusion: una sola `SELECT`/`WITH`; tablas del plan ⊆ vistas del centro; límite 1000 filas con `truncado`; timeout 30 s en un runtime dedicado y cancelable); `fecha_referencia` envuelve las tablas en CTE con la ventana.
- [x] `traverse_normative_graph`: grafo por llamada desde `06` vigente (o a fecha), filtro por `tipo_relacion`, ≤ 2 saltos, `max_vecinos` 15, `hub_resumido` (grado > 25, top relaciones) sin expandir, ≤ 50 nodos, orden por prelación.
- [x] `search_normative_context` (nueva): índice semántico en memoria por centro (`IndiceSemanticoPort`, alimentado por la proyección con chunks e hitos y sus vectores) + coseno/BM25/RRF; filtros de vigencia, prelación y documento; agrupación por tema con `prevalece` / `supletorio`.
- [x] System prompt v2 del `AgentOrchestrator` (elección de tool, citas doc_id + revisión, notas, condiciones, prelación, no inventar) y MCP con las 6 tools (`MCP_GUIDE.md` actualizado).
- [x] Suites `tests/integration_tools_v2.rs` (6) y `tests/adversarial_tools_v2_tests.rs` (10); fixtures `tests/comun_tools_v2/`.
- [x] Puerta E2E `tests/e2e_binario_fase13.rs` (`#[ignore]`, binario real) con dos fixtures nuevos (`pliego_cliente_pcc01.md`, `norma_soldeo_nt_sol_01.md`).

### Fase 14: Cableado de la ventana, MCP y CLI (plan v2 §2.4, §3.1)
- [x] Comandos Tauri finos sobre `LakehouseApp` (`presentation/gui/comandos.rs`): listar, crear, activar, desactivar y eliminar centros; `ingestar_documento` por **ruta**; `catalogo_centro`; `integridad_centro`; modelos y chat.
- [x] Ingesta desde el diálogo nativo (`tauri-plugin-dialog`, llamado desde Rust: no hace falta permiso de capability) o desde el drop de la ventana (`WindowEvent::DragDrop` reemitido como `documentos-soltados`); progreso por etapas (`EtapaIngesta`, evento `ingesta-progreso`) y aviso de la primera carga del modelo de embeddings.
- [x] Frontend sin simulaciones: selector de centros real con alta y activación, catálogo real (doc_id, tipo, prelación, revisión y vigencia), tool cards, citas (doc_id + revisión + breadcrumb), notas al pie y condiciones visibles, y Studio con `matrixResult` y `graphResult`.
- [x] `GuiChatService` rellena `citations`, `matrixResult` y `graphResult` (`presentation/gui/resultados_tools.rs`); la respuesta se escapa en Rust antes de pintar el Markdown (encabezados, viñetas, tablas, negrita y código).
- [x] Arreglo de la Fase 13: `fila_tabla` conserva «Tabla 4-A» en `titulo_tabla` y el lookup por «4-A» resuelve a la primera.
- [x] Limpieza: retirados `TableLookupService`, `PetgraphEngine`, `HybridSearchService` y sus puertos (18 tests retirados; ver bitácora).
- [x] Release: `cargo build --release`; `--centros` y `--ask` escriben en la consola padre; la GUI release no abre consolas al lanzar `claude`.
- [x] Verificación real de la ventana con capturas en `docs/validacion/fase14/`.
- [x] Suites `tests/integration_gui_v2.rs` (3) y `tests/adversarial_gui_v2_tests.rs` (8).

### Fase 15: Aceptación final (plan v2 §1, §3.1; especificación §9)
- [x] `tests/acceptance_8_problemas.rs`: un test por problema (P1–P8) más §9.1 (multi-centro) y §9.4 (selector de modelos en caliente), con Claude Code y FastEmbed reales sobre `bootstrap::composicion_real` (la misma composición que la GUI). **10/10 en 2 ejecuciones** (605 s y 609 s).
- [x] `bootstrap::composicion_real` / `ComposicionReal` (los adaptadores reales antes de construir la fachada) y `MotoresConsulta.indice`.
- [x] Mensaje accionable cuando Claude Code no tiene sesión o ha caducado (`infrastructure/llm/sesion_claude.rs`, en el agente y el destilador) + `tests/adversarial_sesion_claude_tests.rs` (un `claude` falso con la salida real).
- [x] Binario de consola `lakehouse_rag_cli` (`src/bin/`) + `presentation/cli/proceso.rs` compartido + `tests/integration_cli_consola.rs`; `default-run = "lakehouse_rag"`.
- [x] `petgraph` retirado de `Cargo.toml`.
- [x] Botón «Eliminar centro» con confirmación: ya existía (Sesión 14b), así que se incluye en el checklist.
- [x] Auditoría: `cargo build --release`, suite completa, clippy, fmt y todas las suites `#[ignore]` reales.
- [x] Informe `docs/validacion/aceptacion_final.md` y logs en `docs/validacion/fase15/`.
- [x] Prueba guiada de la ventana con el usuario (checklist §7 y demo `GUIA_DEMO.md`); aprobada el 2026-10-19.
- [x] Chat en vivo (streaming `stream-json`, Detener, rediseño según la referencia REA). La aceptación real repetida con streaming da 10/10 (620 s).

---

### STAIR Fase 1: Esquemas
- [x] Contrato en `docs/arquitectura/decisiones_stair.md`.
- [x] Dominio: `EstadoContenido`, `TipoBloque`, `NodoId`, `NivelJerarquico`.
- [x] Tabla `07_indice_jerarquico`; `nodo_id`, `tipo_bloque` y `estado_contenido` en 02; `doc_id_provisional` en 01.
- [x] `FilaIndice`, `LoteIngesta::Indice`, `FragmentoIndexado` y proyección actualizados.
- [x] 07 en staged, commit, rollback, retirada de vigencia, integridad, lecturas `COMMITTED` y time travel.
- [x] `schema_version` 3: un centro anterior se rechaza con error claro. No hay migración.
- [x] Ingesta actual con valores mínimos y sin escribir 07 (hasta la Fase 2).
- [x] Suite `tests/adversarial_esquemas_stair_tests.rs`.

---

### STAIR Fase 2: Ingesta universal
- [x] Preámbulo, secciones sin texto y BOM en `markdown_parser.rs`.
- [x] Árbol ToC y resumen extractivo (`domain/services/arbol_secciones.rs`); filas de 07 en la ingesta.
- [x] Fragmentación 600/512/64 (umbral bajado a 512 en la sesión Umbral 512) con estimación determinista (`domain/services/fragmentacion.rs`).
- [x] Tablas Markdown íntegras en 02.
- [x] Fallback sin Claude: `doc_id` provisional, `PARCIAL_UNIVERSAL` y avisos en el informe, CLI y GUI.
- [x] `reintentar_destilacion` en el servicio y en `LakehouseApp`; `confirmar_sustitucion` en el store.
- [x] Retirados el `continue` de 07 y la tolerancia «revisión sin 07».
- [x] Comando de reintento en GUI, CLI y MCP (Fase 4).

### STAIR Fase 4: Búsqueda y tools
- [x] Páginas en `FragmentoIndexado` (los hitos las heredan) y citas con sección y página.
- [x] `search_normative_context`: `modo`, `nodo_id`, `ruta_linaje_prefijo`, estado de contenido y `documentos_no_definitivos`.
- [x] `stair_navigate_toc` y `stair_get_subtree_context`; alias provisional.
- [x] System prompt con los 4 casos; MCP con 8 tools más el reintento administrativo; `MCP_GUIDE.md`.
- [x] Reintento en GUI, CLI y MCP; estado en catálogo e informe; `app.js`.
- [x] Fixtures `DOC-GENERICO-01.md` y `DOC-TECNICO-OBRA-02.md`, oráculo y esqueleto `#[ignore]` de `tests/e2e_rag_dual_validation.rs`.
- [x] Ejecutar la matriz E2E con Claude real y probar la GUI (Fase 5).

### STAIR Fase 5: Aceptación
- [x] Suite completa, matriz E2E (3 veces) y suites `#[ignore]` reales.
- [x] PDF real: ingesta, 7 preguntas en 2 ejecuciones, fallback sin Claude y reintento.
- [x] Medida del recorte a 512 tokens y valoración del límite de 40.000 caracteres.
- [x] GUI en ventana real por DevTools, con capturas en `docs/validacion/stair/`.
- [ ] Comprobar a mano el diálogo nativo y el arrastre desde el Explorador.
- [ ] Decisión del usuario sobre el merge y los fallos abiertos.

---

## 3. Bitácora de Sesiones

### Sesión 0 (2026-10-16)
* **Objetivo:** Fase 0 — Cimientos, Marco Metodológico y Estructura Cargo.
* **Acciones realizadas:**
  * Generación y aprobación del plan maestro de desarrollo en artefacto `plan_lakehouse_rag.md`.
  * Redacción y formalización de `AGENTS.md` (contrato operativo de Clean Code, Hexagonal y TDD).
  * Creación y formalización de `PROGRESS.md`.
  * Inicialización del paquete Cargo con dependencias verificadas (`lancedb 0.15`, `arrow 54`, `datafusion 45`, `fastembed 4`, `petgraph 0.7`, `ratatui 0.29`, `tokio 1.43`).
  * Estructura modular de capas hexagonales: `src/domain/`, `src/application/`, `src/infrastructure/`, `src/presentation/`.
  * Validación total: `cargo test` (1 test pasando), `cargo clippy -- -D warnings` (0 advertencias) y `cargo fmt --check` (código 100% formateado).
* **Notas para la Sesión 1:**
  * Comenzar con la **Fase 1: Dominio Puro y Gestión Multi-Centro**.
  * Desarrollar bajo TDD estricto las entidades y Value Objects: `CentroConocimiento`, `CenterManifest`, `DocId`, `NivelPrelacion`, `RangoNumerico`, `TuplaOperativa`, `CeldaDobleEntrada` y `AristaGrafo`.

### Sesión 1 (2026-10-16)
* **Objetivo:** Fase 1 — Dominio Puro y Gestión Multi-Centro.
* **Acciones realizadas:**
  * Implementación de Value Objects y entidades de Centro de Conocimiento: `CenterId`, `CenterManifest`, `CentroConocimiento`.
  * Implementación de catálogo de documentos: `DocId`, `NivelPrelacion` (validación 1..=5, jerarquía de prelación) y `DocumentoCatalogo` con control de vigencia.
  * Modelado de matriz operacional técnica: `FaseOperativa` y `TuplaOperativa`.
  * Modelado de tablas cruzadas y rangos numéricos: `LimiteRango`, `RangoNumerico` (evaluación determinista con límites abiertos, cerrados y semiabiertos) y `CeldaDobleEntrada` con matching bidireccional.
  * Modelado de grafo normativo: `TipoRelacionNormativa` y `AristaGrafo` con peso de prelación.
  * Definición del puerto de aplicación `CenterRegistryPort` y verificación de contrato en memoria con `InMemoryCenterRegistry`.
  * Generación del plan adversarial e implementación de la suite en `tests/adversarial_domain_tests.rs`: blindaje de `DocId` contra path traversal/caracteres nulos/DoS, blindaje de `RangoNumerico` contra `NaN` e infinitos con evaluación precisa en epsilon, blindaje de `AristaGrafo` contra autoderogación y pesos no finitos, y protección contra strings abusivos en `TuplaOperativa`.
  * Verificación rigurosa: 34 tests pasando (25 unitarios + 9 adversariales al 100%), `cargo clippy -- -D warnings` (0 advertencias) y `cargo fmt --check` (100% formateado).
* **Notas para la Sesión 2:**
  * Proceder con la **Fase 2: Adaptador LanceDB y los 6 Esquemas Arrow**.
  * Diseñar e implementar los esquemas de Apache Arrow para las 6 tablas (`01_doc_catalog`, `02_doc_chunks`, `03_operational_matrix`, `04_double_entry_tables`, `05_hierarchical_sections`, `06_reference_graph`).
  * Implementar el adaptador `LanceDbCenterRegistry` y repositorios sobre LanceDB (`.lance`) con tests de integración temporales (`tempfile`).

### Sesión 2 (2026-10-16)
* **Objetivo:** Fase 2 — Adaptador LanceDB y los 6 Esquemas Arrow.
* **Acciones realizadas:**
  * Definición tipada y rigurosa de los 6 esquemas Apache Arrow en `src/infrastructure/storage/schemas.rs` (`01_doc_catalog`, `02_doc_chunks`, `03_operational_matrix`, `04_double_entry_tables`, `05_double_entry_cells`, `06_reference_graph`), con dimensión de embeddings en 1024 (`FixedSizeList`) y campo de versionado `vigente: Boolean`.
  * Definición del puerto de aplicación `LakehouseStorePort` en `src/application/ports/lakehouse_store.rs`.
  * Implementación del adaptador `LanceDbStore` sobre LanceDB (`.lance`) en `src/infrastructure/storage/lancedb_store.rs`: inicialización idempotente, inserción atómica de lotes `RecordBatch` y control transaccional de versiones (`retirar_vigencia_documento`).
  * Sincronización asíncrona de inicialización mediante `tokio::sync::Mutex` y control de existencia ante llamadas concurrentes multihilo.
  * Implementación de la suite de pruebas de integración en `tests/integration_lancedb_store.rs`: inicialización de las 6 tablas, inserción de lotes heterogéneos, conteo y verificación de invalidación selectiva de vigencia.
  * Implementación de la suite de pruebas adversariales en `tests/adversarial_storage_tests.rs`: rechazo de esquemas corruptos, rechazo de dimensiones vectoriales incompatibles, control de errores en tablas inexistentes, idempotencia de invalidación y resistencia a carreras concurrentes multihilo.
  * Alineación de dependencias de Arrow a la versión 53 unificando LanceDB y DataFusion sin duplicidad de crates.
  * Verificación rigurosa: 46 / 46 tests pasando (28 unitarios + 9 adversariales de dominio + 5 adversariales de almacenamiento + 4 de integración al 100%), `cargo clippy -- -D warnings` (0 advertencias) y `cargo fmt --check` (100% formateado).
* **Notas para la Sesión 3:**
  * Proceder con la **Fase 3: Motor Analítico DataFusion y Grafo Petgraph**.
  * Configurar `SessionContext` dinámico con registro y desregistro de tablas según centros activos.
  * Implementar el servicio determinista `TableLookupService` para resolución de celdas cruzadas (rangos numéricos y categorías).
  * Implementar el servicio `GraphTraversalService` para hidratación y linaje normativo en memoria con `petgraph`.

### Sesión 3 (2026-10-16)
* **Objetivo:** Fase 3 — Motor Analítico DataFusion y Grafo Petgraph.
* **Acciones realizadas:**
  * Definición de los contratos y puertos de aplicación: `AnalyticalEnginePort` (consultas SQL deterministas y aislamiento por centro), `TableLookupPort` (resolución de tablas cruzadas) y `GraphEnginePort` (recorridos y linaje normativo).
  * Implementación del caso de uso `TableLookupService` en `src/application/use_cases/table_lookup_service.rs` con soporte de hidratación directa desde `RecordBatch` de Apache Arrow (`05_double_entry_cells`) y evaluación de límites inclusivos/exclusivos y categorías.
  * Implementación del adaptador `PetgraphEngine` en `src/infrastructure/query_engine/petgraph_engine.rs` con grafo dirigido en memoria `DiGraph<DocId, AristaGrafo>`, hidratación desde `RecordBatch` (`06_reference_graph`), cálculo de norma prevalente por derogación sucesiva, extracción de linaje y detección de ciclos vía algoritmo de Tarjan.
  * Implementación del adaptador `DataFusionEngine` en `src/infrastructure/query_engine/datafusion_engine.rs` encapsulando `SessionContext` de Apache DataFusion 44, validación estricta de seguridad contra sentencias DDL/DML mutables, y aislamiento estricto de tablas por centro de conocimiento.
  * Configuración de perfiles `[profile.dev]` y `[profile.test]` con `debug = 1` en `Cargo.toml` para resolver el límite de PDB de MSVC en Windows y optimizar el tiempo de enlace.
  * Implementación de la suite de pruebas de integración en `tests/integration_query_engine.rs`: agregaciones SQL y filtros WHERE sobre la matriz operativa, aislamiento entre centros `planta_alfa` y `planta_beta`, resolución exacta de celdas en límites frontera continuos, y resolución de prevalencia normativa y linaje documental.
  * Implementación de la suite de pruebas adversariales en `tests/adversarial_query_tests.rs`: neutralización de 15 variantes de inyecciones SQL destructivas y sintaxis inválida, verificación de contención anti-cross-contamination, resistencia de tablas de doble entrada a `NaN`, infinitos y extremos de punto flotante, y contención segura de paradojas de derogación circular reportando `DomainError::GraphError` sin desbordamiento de pila.
  * Verificación rigurosa: 61 / 61 tests pasando (34 unitarios + 9 adversariales de dominio + 5 adversariales de almacenamiento + 5 adversariales de query + 4 de integración LanceDB + 4 de integración DataFusion/Petgraph al 100%), `cargo clippy -- -D warnings` (0 advertencias) y `cargo fmt --check` (100% formateado).
* **Notas para la Sesión 4:**
  * Proceder con la **Fase 4: Embeddings Locales y Búsqueda Híbrida**.

### Sesión 4 (2026-10-16)
* **Objetivo:** Fase 4 — Embeddings Locales y Búsqueda Híbrida.
* **Acciones realizadas:**
  * Ampliación de errores tipados en `DomainError`: variantes `EmbeddingError` y `SearchError`.
  * Definición de contratos y puertos en `src/application/ports/`: `EmbeddingEnginePort` y `HybridSearchPort` junto a sus DTOs de transferencia (`SearchHit`, `HybridSearchParams`).
  * Implementación del motor en memoria determinista `MockEmbeddingEngine` con hash criptográfico y normalización L2 para ejecución de suites ultrarrápidas y offline.
  * Implementación del adaptador de infraestructura `FastEmbedEngine` encapsulando `fastembed::TextEmbedding` (ONNX Runtime) y offload seguro a hilos bloqueantes con `tokio::task::spawn_blocking`.
  * Implementación del orquestador de búsqueda híbrida `HybridSearchService` en `src/application/use_cases/hybrid_search_service.rs`: búsqueda vectorial KNN/L2 en LanceDB (`_distance`), extracción léxica BM25 (Robertson-Spärck Jones IDF con normalización min-max) y fusión balanceada vía Reciprocal Rank Fusion (RRF) con filtrado estricto de `centro_id` y `vigente`.
  * Implementación de la suite de pruebas de integración en `tests/integration_hybrid_search.rs`: recuperación y ordenamiento RRF sobre LanceDB temporal, aislamiento estricto entre centros (`planta_norte` vs `planta_sur`), filtrado de vigencia documental y modulación de ponderación de pesos vectorial/BM25.
  * Implementación de la suite de pruebas adversariales en `tests/adversarial_embeddings_tests.rs`: rechazo de `centro_id` vacío o de espacios en blanco, manejo seguro de consultas vacías, neutralización de 8 vectores de inyección hostil (SQL, bytes nulos, path traversal, scripts), resistencia ante extremos de `top_k` (0 y 10,000) y rechazo de vectores con dimensiones incongruentes.
  * Verificación rigurosa: 70 / 70 tests pasando (37 unitarios + 9 adversariales de dominio + 5 adversariales de almacenamiento + 5 adversariales de query + 5 adversariales de embeddings + 4 integración LanceDB + 4 integración DataFusion/Petgraph + 4 integración híbrida al 100%), `cargo clippy --all-targets -- -D warnings` (0 advertencias) y `cargo fmt --check` (100% formateado).
### Sesión 5 (2026-10-16)
* **Objetivo:** Fase 5 — Pipeline de Ingesta y Destilación Claude Headless.
* **Acciones realizadas:**
  * Ampliación de errores de dominio en `DomainError`: `ParserError`, `LlmDistillationError`, `IngestionError`.
  * Definición de DTOs tipados en `src/application/dtos/ingestion.rs` con soporte de serialización, deserialización y generación automática de JSON Schema vía `schemars` (`DistilledDocumentPackage`, `DistilledCatalog`, `DistilledOperationalTuple`, `DistilledDoubleEntryTable`, `DistilledCell`, `DistilledReferenceEdge`, `ChunkToIngest`).
  * Definición de puertos en `src/application/ports/`: `DocumentParserPort` y `LlmDistillerPort` con validador de esquema JSON.
  * Implementación del adaptador `MarkdownParser` en `src/infrastructure/parsers/markdown_parser.rs`: extracción de jerarquía estructural (H1-H4), códigos de sección normalizados, tablas Markdown completas con encabezados, filas y notas al pie, y resistencia ante formatos Windows CRLF.
  * Implementación del adaptador mock determinista `MockDistiller` en `src/infrastructure/llm/mock_distiller.rs` para pruebas offline continuas de alta velocidad y programación de fallos.
  * Implementación del adaptador `ClaudeHeadlessDistiller` en `src/infrastructure/llm/claude_headless_distiller.rs`: invocación asíncrona mediante subproceso con `claude -p`, inyección de esquema JSON estricto, limpieza de wrappers markdown (````json...````) y manejo robusto de errores/timeouts.
  * Implementación del orquestador atómico `DocumentIngestionService` en `src/application/use_cases/document_ingestion_service.rs`: parseo estructural, destilación LLM, cálculo SHA-256, invalidación previa de vigencia por `DocId`, vectorización de fragmentos con `EmbeddingEnginePort` y persistencia transaccional de lotes `RecordBatch` hacia las tablas correspondientes de `LanceDbStore`.
  * Implementación de la suite de integración en `tests/integration_ingestion_pipeline.rs`: persistencia E2E en las 6 tablas con fixture industrial complejo, control de revisiones e invalidación de versiones anteriores y coexistencia multi-centro.
  * Implementación de la suite adversarial en `tests/adversarial_ingestion_tests.rs`: neutralización de inyecciones de escape de prompt / scripts hostiles, manejo controlado de fallos en LLM, absorción de tablas rotas/asimétricas, resistencia a DoS con payloads masivos y rechazo estricto de dimensiones incompatibles de embeddings.
  * Verificación rigurosa: 88 / 88 tests pasando (48 unitarios + 9 adversariales de dominio + 5 adversariales de almacenamiento + 5 adversariales de query + 5 adversariales de embeddings + 5 adversariales de ingesta + 4 integración LanceDB + 4 integración DataFusion/Petgraph + 4 integración híbrida + 3 integración ingesta al 100%), `cargo clippy --all-targets -- -D warnings` (0 advertencias) y `cargo fmt --check` (100% formateado).
### Sesión 6 (2026-10-16)
* **Objetivo:** Fase 6 — Servidor de Herramientas Data Lake y Agente.
* **Acciones realizadas:**
  * Ampliación de errores de dominio en `DomainError`: variantes `ToolExecutionError` y `AgentError`.
  * Definición tipada de DTOs en `src/application/dtos/tools.rs` y `src/application/dtos/agent.rs` con esquemas JSON Schema (`InspectCatalogParams`, `QueryOperationalMatrixParams`, `LookupDoubleEntryTableParams`, `ExecuteAthenaSqlParams`, `TraverseNormativeGraphParams`, `ToolCall`, `ToolResult`, `ClaudeModel`, `AgentMessage`, `AgentTurnOutput`, `AgentExecutionEvent`).
  * Definición de contratos y puertos en `src/application/ports/`: `ToolServerPort` y `AgentRunnerPort`.
  * Implementación del servicio de herramientas operativas `LakehouseToolService` en `src/application/use_cases/lakehouse_tool_service.rs` resolviendo de forma determinista las 5 herramientas operativas sobre `DataFusionEngine`, `TableLookupService` y `PetgraphEngine` con serialización Arrow `ArrayWriter` a JSON y calificación de aislamiento por centro.
  * Implementación del orquestador del ciclo de vida del agente `AgentOrchestrator` en `src/application/use_cases/agent_orchestrator.rs`: bucle de razonamiento y herramientas con protección estricta contra bucles infinitos (`max_steps`), inyección de System Prompt con aislamiento de centro y emisión reactiva de eventos para UI (`EventCallback`).
  * Implementación del adaptador mock determinista `MockAgentRunner` en `src/infrastructure/llm/mock_agent_runner.rs` para pruebas de alta velocidad, simulación de respuestas programadas, heurística basada en intenciones y control de errores.
  * Implementación del adaptador de producción `ClaudeAgentRunner` en `src/infrastructure/llm/claude_agent_runner.rs`: integración con subproceso CLI `claude -p`, selector dinámico de modelos (`sonnet`, `opus`, `haiku`), extracción y parseo seguro de solicitudes de herramientas (`tool_calls` o `tool_use`) y manejo de timeouts.
  * Implementación de la suite de integración en `tests/integration_agent_tools.rs`: ejecución E2E de las 5 herramientas sobre datos reales en memoria y orquestación completa del ciclo agente -> herramienta -> síntesis final.
  * Implementación de la suite adversarial en `tests/adversarial_agent_tools_tests.rs`: neutralización de inyecciones SQL destructivas (`DROP`, `DELETE`, `INSERT`), bloqueo estricto anti-cross-center en consultas `execute_athena_sql`, tolerancia a argumentos JSON corruptos o mal tipados, sanitización ante inyecciones en filtros WHERE de la matriz operativa y corte seguro ante bucles infinitos de herramientas (`max_steps`).
  * Verificación rigurosa: 108 / 108 tests pasando (61 unitarios + 9 adversariales de dominio + 5 adversariales de almacenamiento + 5 adversariales de query + 5 adversariales de embeddings + 5 adversariales de ingesta + 5 adversariales de agent/tools + 4 integración LanceDB + 4 integración DataFusion/Petgraph + 4 integración híbrida + 3 integración ingesta + 2 integración agent/tools al 100%), `cargo clippy --all-targets -- -D warnings` (0 advertencias) y `cargo fmt --check` (100% formateado).
* **Notas para la Sesión 7:**
  * Proceder con la **Fase 7: Servidor MCP y Aplicación Gráfica (Linear + NotebookLM Frameless 16:9)**.

### Sesión 7 (2026-10-17)
* **Objetivo:** Fase 7 — Servidor MCP Stdio y Aplicación Gráfica Moderna (Linear + Google NotebookLM Frameless 16:9).
* **Acciones realizadas:**
  * Reorientación arquitectónica autorizada: implementación dual de Servidor MCP Universal y Aplicación Gráfica Frameless moderna inspirada en Linear y NotebookLM.
  * Implementación del Servidor MCP Stdio en `src/infrastructure/mcp/mcp_server.rs` y `src/infrastructure/mcp/mod.rs`: protocolo JSON-RPC 2.0 sobre `stdin`/`stdout`, respuestas deterministas a `initialize`, `tools/list` y `tools/call` invocando directamente a `LakehouseToolService`.
  * Implementación de suite adversarial para el servidor MCP en `tests/adversarial_mcp_tests.rs`: tolerancia a JSON corrupto (`-32700`), manejo de notificaciones sin ID, validación estricta de parámetros de herramientas (`-32602`) y captura de errores sin pánico (`isError: true`).
  * Implementación de los activos de presentación GUI en `src/presentation/gui/web/`:
    * `index.html`: barra de título personalizada frameless, controles de ventana propios, selector de plantas/centros activos, selector de modelos Claude, Panel 1 de Fuentes & Drag & Drop, Panel 2 de Chat interactivo con observabilidad de herramientas y Panel 3 de Lakehouse Studio con los 6 esquemas Arrow y grafo.
    * `styles.css`: paleta de colores nativa de Linear (Dark `#08090a` y Light theme), escala de superficies (*Surface Ladder*), acento índigo `#5e6ad2`, bordes sutiles de 1px y componentes interactivos.
    * `app.js`: lógica de cliente completa con control de ventana frameless (minimizar, maximizar, restaurar, cerrar), conmutador de tema con persistencia `localStorage`, ingesta interactiva Drag & Drop, renderizado reactivo de tarjetas colapsables de Tool Use, citas interactivas y visor dinámico de matrices y grafo.
  * Implementación de contratos de ventana y DTOs IPC en `src/presentation/gui/mod.rs` (validación de relación 16:9 1280x720, frameless y serialización).
  * Implementación de comandos CLI en `src/presentation/cli/mod.rs` con soporte de flags `--mcp-server`, `--mcp-info`, `--gui`, `--help`.
  * Integración del punto de entrada en `src/main.rs`: despacho entre servidor MCP stdio con redirección de logs a stderr, generador de configuración JSON y lanzador de GUI.
  * Creación de herramientas de autoinstalación en la raíz del repositorio:
    * `MCP_GUIDE.md`: guía paso a paso para Claude Desktop, Claude Code, Cursor y Gemini.
    * `install_mcp.ps1`: script interactivo PowerShell para registro en un clic.
  * Verificación rigurosa: 124 / 124 tests pasando (73 unitarios + 9 adversariales de dominio + 5 adversariales de almacenamiento + 5 adversariales de query + 5 adversariales de embeddings + 5 adversariales de ingesta + 5 adversariales de agent/tools + 4 adversariales MCP + 4 integración LanceDB + 4 integración DataFusion/Petgraph + 4 integración híbrida + 3 integración ingesta + 2 integración agent/tools al 100%), `cargo clippy --all-targets -- -D warnings` (0 advertencias) y `cargo fmt --check` (100% formateado).

### Traspaso Técnico para Claude Code (Resolución de Despliegue de GUI Tauri 2):
* **Estado Actual:**
  * Toda la lógica de negocio, los 6 esquemas Arrow, LanceDB, DataFusion, FastEmbed, Petgraph y el servidor MCP por Stdio están 100% operativos y probados.
  * El frontend (`src/presentation/gui/web/`) está completo con diseño Linear + Google NotebookLM (16:9 frameless).
  * Las dependencias de `tauri = "2"` y `tauri-build = "2"` ya están añadidas y compilan limpiamente (`cargo build --release` compila con éxito).
  * Se crearon los iconos de Windows en `icons/icon.ico` y el favicon para WebView2.
* **Diagnóstico de los Desafíos Encontrados en Windows:**
  1. **Consola de Terminal no Deseada:** Al ejecutarse en Windows, el binario por defecto lanza una ventana `cmd.exe`. Se configuró `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` en `src/main.rs`. Al compilar en `--release`, no debe aparecer consola.
  2. **Bucle de Eventos (Event Loop) de Tauri:** Tauri 2 requiere un hilo principal síncrono. No debe usarse `#[tokio::main]` sobre `main()`. En `src/main.rs`, `main()` debe ser síncrono y Tokio debe instanciarse mediante `tokio::runtime::Runtime::new()` únicamente cuando se detecte el flag `--mcp-server`.
  3. **Esquema de `tauri.conf.json`:** En Tauri 2, la propiedad `"capabilities"` no va dentro de `"security"`. La configuración debe declarar `url: "index.html"`, `label: "main"` y los permisos residen en `capabilities/default.json`.
  4. **Instrucción para Claude Code:** Para levantar la GUI interactivamente en desarrollo y depurar cualquier excepción de WebView2, ejecutar directamente `cargo tauri dev` o revisar el ciclo de vida de la ventana en `src/presentation/gui/mod.rs`.

### Sesión 8 (2026-10-17)
* **Objetivo:** Reparar el chat de la GUI (no funcionaba) y listar dinámicamente los modelos disponibles en Claude Code headless.
* **Diagnóstico:**
  * `iniciar_app_tauri()` no registraba `invoke_handler`: toda llamada `invoke('enviar_mensaje_chat')` fallaba con "command not found".
  * El selector de modelos estaba fijo en `index.html` con modelos obsoletos (Claude 3.x) y `ClaudeModel` era un enum cerrado de 3 variantes.
  * `ChatResponsePayload` se serializaba en snake_case mientras `app.js` lee camelCase (`replyHtml`, `toolsUsed`).
  * `ClaudeAgentRunner` pasaba toda la conversación como argumento de línea de comandos (límite de 32 KB en Windows), sin protocolo de tools explícito y con las herramientas nativas de Claude Code activas.
* **Acciones realizadas:**
  * `ClaudeModel` pasa a ser un newtype validado (`ClaudeModel::new`) que admite cualquier alias o ID (`opus`, `fable`, `claude-opus-5-5`, `sonnet[1m]`) y rechaza flags (`--…`), metacaracteres y longitudes > 128. Serde valida al deserializar. Nuevo DTO `ModelDescriptor` (camelCase).
  * Nuevo puerto `application/ports/model_catalog.rs` (`ModelCatalogPort`).
  * Nuevo adaptador `infrastructure/llm/claude_model_catalog.rs` (`ClaudeHeadlessModelCatalog`): envía `control_request {subtype: initialize}` por stdin a `claude -p --input-format stream-json --output-format stream-json --verbose` y extrae `response.models`; descarta entradas inválidas y duplicadas, con timeout y límite de líneas.
  * `ClaudeAgentRunner`: conversación por stdin, `--system-prompt`, `--tools ""`, `--no-session-persistence`, `--strict-mcp-config`, protocolo `tool_calls` explícito, timeout 180 s. Helper `ocultar_consola_en_windows` (`CREATE_NO_WINDOW`) en ambos adaptadores.
  * Nuevo `presentation/gui/chat_service.rs` (`GuiChatService`): valida mensaje/centro/modelo, historial por centro acotado a 40 mensajes con rollback ante error, tool cards con duración y escape HTML de la respuesta (`texto_a_html_seguro`).
  * Comandos Tauri `listar_modelos_claude` y `enviar_mensaje_chat` registrados; composición en `main.rs` (`construir_servicio_chat_gui`).
  * Frontend: selector de modelos poblado desde `listar_modelos_claude` (respaldo estático solo sin Tauri) y validación de modelo vacío.
* **Tests añadidos:** 4 unitarios de `ClaudeModel`/`ModelDescriptor`, 6 del catálogo, 2 del runner, 1 de escape HTML, 6 adversariales en `tests/adversarial_gui_chat_tests.rs`, y 2 `#[ignore]` contra la CLI real (`integration_claude_model_catalog`, `acceptance_gui_chat_claude_headless`), ambos verificados en verde en local.
* **Verificación:** 146 tests pasando + 2 ignorados (reales, ejecutados manualmente con éxito), `cargo clippy --all-targets -- -D warnings` sin avisos, `cargo fmt --check` OK.
* **Notas para la siguiente sesión:**
  * No se ha verificado visualmente la ventana Tauri; probar con `cargo tauri dev` el chat y el selector.
  * Pendiente el comando `ingestar_documento` (requiere enviar ruta o bytes desde el frontend).
  * Las citas (`citations`), `matrixResult` y `graphResult` aún se devuelven vacíos desde el backend.

### Sesión 9 (2026-10-17) — Fase 9: Dominio v2
* **Objetivo:** Implementar el contrato de dominio del plan v2 (§2.1) en `src/domain/`, sin I/O, con TDD.
* **Acciones realizadas:**
  * Registrados y llevados a Verde los módulos Rojo de la sesión orquestadora: `services/normalizacion.rs`, `models/alcance.rs`, `models/identificadores.rs` y `models/vigencia.rs`.
  * `FaseOperativa`: variantes `Recepcion, Descarga, Custodia, Inspeccion, Ensayo, Liberacion, Ejecucion, Despacho, Mantenimiento, Otra(String)`; `parse` quita paréntesis/corchetes, tildes y prefijos (`FASE`, `ETAPA`, `PASO`, numeración) y decide por la primera palabra. `VALORES_CANONICOS`, `es_canonica()`, `MAX_LONGITUD_OTRA = 64`.
  * `TipoRelacionNormativa`: vocabulario cerrado §4 (`ExigeLlenarRegistro, DerogaA, ModificaApartado, RemiteATabla, SubordinadoANorma, ReferenciaA`), ahora `Copy`. `parse` estricto con sinónimos y `parse_con_respaldo -> (Self, bool)`. `petgraph_engine` lee la Tabla 06 con `parse_con_respaldo` (filas heredadas como `Complementa` o `CITA` se degradan a `REFERENCIA_A`).
  * `TipoDocumento` (nuevo, en `document.rs`) con `parse` por la primera palabra, `as_str`, `VALORES_CANONICOS` y `nivel_prelacion()`.
  * `DocId::new` solo admite `[A-Za-z0-9._-]`, exige un alfanumérico y rechaza `..`. Nuevo `DocId::desde_codigo_libre`.
  * `HitoOperativo` + `BorradorHito` (entrada del extractor) sustituyen a `TuplaOperativa`. Alertas: `RolesFusionados`, `EjecutorVacio`, `ResponsableVacio`, `SinRegistro` y `RegistroNoCanonico`; `CalidadExtraccion {Ok, Revisar}`.
  * Actualizados los usos en `src/` (petgraph_engine, lakehouse_tool_service) y `tests/` (adversarial_domain, adversarial_query, integration_agent_tools, integration_query_engine).
* **Tests añadidos:** 20 unitarios de dominio (normalización 3, alcance 4, identificadores 1, vigencia 2, documento 4, relación +1 neto, matriz +5 neto) y 12 adversariales en `tests/adversarial_dominio_v2_tests.rs` (Unicode/emoji/homoglifos, cadenas de 1 MB, inyección SQL, 29-feb y año 9999/`NaiveDate::MAX`, niveles 0-255 y roles fusionados camuflados con falsos positivos controlados).
* **Verificación:** 180 tests pasando + 12 ignorados, `cargo clippy --all-targets -- -D warnings` sin avisos, `cargo fmt --check` OK.
* **Desviaciones del contrato:**
  * `texto_vectorizable()` añade `Condición: …` cuando el hito tiene `condicion_aplicacion` (problema 7); el ejecutor no se vectoriza.
  * Se añade la alerta `EjecutorVacio`. Solo `hito_id` y `objeto_alcance` son obligatorios; los roles y el criterio vacíos generan alertas en lugar de errores, para no perder filas en la ingesta.
  * `VentanaVigencia::derogar` rechaza una segunda derogación (inmutable para la auditoría). Sus campos son privados con getters, para preservar la invariante.
  * `HitoOperativo` conserva `registro_codigo` tal cual (recortado); la canonicalización la decide la ingesta (Fase 11) y la alerta `RegistroNoCanonico` la señala (también si el código no tiene dígitos).
* **Notas para la Fase 10:**
  * Columnas en texto: usar `FaseOperativa::as_str`, `TipoRelacionNormativa::as_str`, `TipoDocumento::as_str`, `AlertaExtraccion::as_str` y `CalidadExtraccion::as_str` (MAYÚSCULAS canónicas). `nivel_prelacion` = `TipoDocumento::nivel_prelacion().valor()` (i8).
  * Fechas: `VentanaVigencia::fecha_vigor()` / `fecha_derogacion()` → Date32. `vigente_en` define la semántica del filtro de *time travel*.
  * IDs primarios: `id_versionado(&doc_id, version, TipoEntidad::…, idx)`. `DocId` ya no admite comillas, así que las interpolaciones en filtros LanceDB dejan de ser inyectables (H9), aunque conviene seguir escapando.
  * `DocumentoCatalogo` aún no tiene `tipo_documento` ni `VentanaVigencia`: añadirlos junto con el esquema 01 v2.
  * `DocumentIngestionService` sigue usando `DocId::new` sobre el `doc_id` destilado: si Claude devuelve espacios o tildes fallará; la Fase 11 debe pasar a `DocId::desde_codigo_libre` + `separar_revision`.

### Sesión 10 (2026-10-17) — Fase 10: Esquemas v2 y store transaccional
* **Objetivo:** plan v2 §1 (filas 3 y 8), §2.2 y §2.3 (`LakehouseStorePort`).
* **Contratos nuevos:**
  * `application/ports/lakehouse_store.rs`:
    * Constantes `TABLA_DOCUMENTOS_CATALOGO`, `TABLA_CHUNKS_NORMATIVA`, `TABLA_MATRIZ_OPERATIVA`, `TABLA_CATALOGO_TABLAS`, `TABLA_CELDAS_DOBLE_ENTRADA`, `TABLA_GRAFO_REFERENCIAS`, `TODAS_LAS_TABLAS`, `TABLAS_DEPENDIENTES` y `LIMITE_MAXIMO_LECTURA = 1_000_000`.
    * Tipos `EstadoIngesta {Staged, Committed}`, `HuellaIngerida`, `ResumenConfirmacion`, `ConsultaTabla` e `InformeIntegridad`.
    * `ConsultaTabla` se construye con `confirmadas(limite)` y se acota con `solo_vigentes()`, `del_centro()` y `del_documento()`.
    * `InformeIntegridad` expone `huerfanos`, `ingestas_staged`, `versiones_vigentes_multiples` y `es_integro()`.
  * Puerto:
    * Métodos: `inicializar_tablas`, `existe_hash(&CenterId, hash)`, `escribir_staged(&IngestId, Vec<LoteIngesta>)`, `confirmar_ingesta(&IngestId, &DocId)`, `revertir_ingesta(&IngestId)`, `leer_tabla(nombre, &ConsultaTabla)` (solo `COMMITTED`), `verificar_integridad`, `contar_registros` y `listar_tablas`.
    * Se eliminan `insertar_lote` y `retirar_vigencia_documento`.
  * `application/dtos/lakehouse_filas.rs`:
    * Filas: `ClaveDocumental`, `FilaCatalogo`, `FilaChunk`, `FilaHito`, `FilaTabla`, `FilaCelda` y `FilaArista`, agrupadas en `LoteIngesta`.
    * Auxiliares: `EjeTabla`/`TipoEjeTabla`, `LimitesEje` y `TipoNodoGrafo` (con `destino_de(relación)`).
    * Las columnas transaccionales no forman parte de la fila: las fija el almacén.
  * `domain/models/identificadores.rs`: `IngestId` (`[A-Za-z0-9_-]`, ≤ 64).
  * `infrastructure/storage/lotes_arrow.rs`: `lote_a_record_batch`, `ColumnasTransaccionales` y `fecha_a_date32`.
  * `LanceDbStore` añade `leer_filas_crudas` (diagnóstico, cualquier estado), `sembrar_filas` (solo fixtures) y `literal_sql`.
* **Semántica de la confirmación:**
  * Orden:
    1. Activa la ingesta en 02-06.
    2. Retira la vigencia de las filas vigentes del mismo `centro_id` + `doc_id` con otro `ingest_id`, en 02-06 y en 01 (con `fecha_derogacion = fecha_vigor` nueva).
    3. Activa 01, que actúa como marca de commit.
  * Si la retirada falla, reactiva la revisión previa antes de devolver el error.
  * Rechaza la confirmación si:
    * la ingesta no existe o ya está confirmada;
    * el `doc_id` es ajeno;
    * `version_num` es ≤ la máxima confirmada (`VersionConflict`);
    * la `fecha_vigor` no es posterior a la vigente (`ValidationError`).
  * `revertir_ingesta` rechaza las ingestas ya confirmadas (01 `COMMITTED`) y es idempotente (`Ok(0)`).
* **Ingesta (`DocumentIngestionService`):**
  * Flujo:
    1. SHA-256 y `existe_hash`, por centro. Si el hash ya existe, devuelve `IngestionReport { ya_ingerido: true }` sin parsear ni destilar.
    2. Parseo y destilación.
    3. Filas con `id_versionado`.
    4. `escribir_staged` y `confirmar_ingesta`.
  * Cualquier error posterior a la primera escritura llama a `revertir_ingesta`.
  * `IngestionReport` añade `ingest_id`, `ya_ingerido` y `versiones_derogadas`.
  * `DocId::new` es estricto: una comilla devuelve `InvalidDocId` antes de escribir nada.
* **Mapeo mínimo del DTO v1 (lo completa la Fase 11):**
  * `tipo_documental` → `TipoDocumento::parse`, con respaldo `PROCEDIMIENTO_GENERAL`.
  * `fecha_emision` ISO → `fecha_vigor`, con respaldo en la fecha de hoy.
  * `revision = "Rev. NN"`.
  * Fase → `FaseOperativa::parse`, con respaldo `NO_ESPECIFICADA`.
  * `criterio_aceptacion` = `accion | parametro | tolerancia | referencia`; ahí queda hoy el código del registro.
  * `tipo_relacion` → `parse_con_respaldo`.
  * Destino → `codigo_canonico`.
  * `valor_resultado_num` se toma del prefijo numérico del resultado.
  * Quedan vacíos o nulos: `objeto_alcance`, `rol_ejecutor`, `registro_codigo`, `registro_nombre`, `chunk_origen_id`, `vector_hito`, `vector_tabla`, `valor_salida_nombre`, `eje_*_unidad`, `eje_z_filtro_adicional`, `condicion_aplicacion` y los vínculos de los chunks.
  * `breadcrumb_jerarquia` es solo «código título» de la sección (H7).
  * Todos los hitos salen con `calidad_extraccion = REVISAR`.
* **Consumidores:**
  * `HybridSearchService` lee la 02 v2, filtra `estado_ingesta = 'COMMITTED'` y escapa el centro.
  * `TableLookupService` y `PetgraphEngine` leen las columnas v2 y saltan las filas no vigentes o no confirmadas (`peso_prelacion` es opcional, 1,0 por defecto).
  * `LakehouseToolService` no cambia: usa sus propias tablas en memoria (las tools v2 son de la Fase 13).
* **Tests añadidos:**
  * Unitarios: esquemas v2 ×5, `IngestId`, filas, `literal_sql`, hash, compensación con `CAST(NULL AS DATE)` y mapeos de ingesta ×3.
  * `tests/integration_store_v2_transaccional.rs` (8):
    * staged invisible hasta el commit;
    * Rev.03 → Rev.04 en las 6 tablas, con `fecha_derogacion`;
    * IDs únicos entre revisiones;
    * rollback ante un fallo inyectado en la 05;
    * idempotencia por hash, con contador de llamadas al LLM;
    * grafo de la v3 retirado;
    * `verificar_integridad` con huérfanos, STAGED y versiones múltiples;
    * lectura de más de 10 filas.
  * `tests/adversarial_store_v2_tests.rs` (9):
    * ingest_id malicioso y doc_id con comilla;
    * confirmación doble y rollback de un ingest_id inexistente;
    * lotes vacíos o mezclados;
    * regresión de versión o de fecha y doc_id ajeno;
    * límites frontera y tablas desconocidas;
    * confirmaciones concurrentes.
  * Fixtures compartidas en `tests/comun_store_v2/`.
  * Adaptados a v2: `integration_lancedb_store`, `adversarial_storage_tests`, `integration_hybrid_search` (+1 test de STAGED invisible), `adversarial_embeddings_tests`, `integration_ingestion_pipeline`, `adversarial_ingestion_tests`, `integration_query_engine` y `acceptance_e2e_etapa1_ingesta`.
* **Etapa 1:**
  * Pasan sin `#[ignore]`:
    * `hallazgo_esquemas_cumplen_la_especificacion` (H1);
    * `hallazgo_rev04_retira_vigencia_de_las_aristas_del_grafo` (H3);
    * `hallazgo_ids_no_colisionan_entre_revisiones` (H4);
    * `hallazgo_reingesta_del_mismo_documento_no_duplica_filas` (H5);
    * `hallazgo_doc_id_con_comilla_se_rechaza_sin_escribir_nada` (H9, reescrito: el doc_id con comilla se rechaza).
  * Siguen ignorados H7 y H8.
  * Reales ejecutados (`-- --ignored real_`):
    * Destilador con la configuración por defecto: OK (42,6 s, 5 tuplas, 6 aristas).
    * FastEmbed: OK (1024 dims).
    * E2E Rev.03 → Rev.04 con Claude + FastEmbed + LanceDB: las dos ingestas terminan bien (`versiones_derogadas = [3]`), la vigencia es correcta en las 6 tablas, los IDs son únicos y aparece `DEROGA_A`.
    * Fallan solo `grafo_registros` (Claude no emite `EXIGE_LLENAR_REGISTRO`) y `custodia_responsable_sin_ejecutor` (mezcla ejecutor y firmante). Ambas pertenecen al hallazgo H2 (Fase 11).
* **Verificación:** 214 tests pasando + 7 ignorados; `cargo clippy --all-targets -- -D warnings` sin avisos; `cargo fmt --check` OK.
* **Desviaciones del plan:**
  * `escribir_staged` recibe filas tipadas (`LoteIngesta`) en lugar de `RecordBatch`. Así el caso de uso deja de construir Arrow y de depender de `infrastructure::storage::schemas` (solo lo importan los tests unitarios).
  * `existe_hash` recibe también el centro: la idempotencia y la retirada de vigencia son por centro (O5).
  * `raw_docs/` y `center_manifest.json` con `schema_version` pasan a la Fase 12 (centros en disco). Aquí solo existe la constante `VERSION_ESQUEMA = 2`.
  * TDD: los tests de esquemas e `IngestId` se vieron en rojo antes de implementar. Las suites de integración y adversarial del store se escribieron justo después de la implementación; su rojo previo eran los `hallazgo_*` de la Etapa 1.
  * No hay migración de datos v1: un centro creado con los nombres antiguos (`01_doc_catalog`…) no se lee, y `inicializar_tablas` crea las tablas v2 al lado.
  * LanceDB no tiene transacciones entre tablas:
    * Durante la confirmación hay una ventana breve con dos versiones vigentes en 02-06. Hay un único escritor por almacén, serializado con un mutex.
    * Si el proceso muere a mitad, `verificar_integridad` lo detecta como STAGED colgados o versiones vigentes múltiples.
* **Notas para la Fase 11:**
  * Sustituir el DTO de destilación por el v2 y rellenar, en `document_ingestion_service.rs`, las columnas hoy vacías listadas arriba. Están en `filas_hitos`, `fila_tabla`, `fila_celda` y `filas_chunks`.
  * Calcular `calidad_extraccion` con `HitoOperativo::new(...).calidad_extraccion()` y `vector_hito` con `texto_vectorizable()`.
  * Construir `breadcrumb_jerarquia` y `condicion_aplicacion` con `PilaEncabezados` (H7) y vectorizar `breadcrumb + condición + texto`.
  * Pasar a `DocId::desde_codigo_libre` + `separar_revision` sobre el doc_id destilado: hoy un espacio o una tilde abortan la ingesta con `InvalidDocId`.
  * La fecha de vigor sale de `fecha_emision`; si el DTO v2 trae `fecha_vigor`, usar esa. Una revisión nueva exige `version_num` mayor y `fecha_vigor` posterior a la vigente.
  * Los fallos reales pendientes de H2 son `grafo_registros` y `custodia_responsable_sin_ejecutor`.

### Sesión 11 (2026-10-17) — Fase 11: Ingesta v2
* **Objetivo:** plan v2 §1 (problemas 1, 2 y 7), §2.5 (destilación) y §2.6 (embeddings).
* **Contratos nuevos:**
  * `application/dtos/ingestion.rs` (DTO v2, sustituye al v1): `DistilledDocumentPackage { catalogo, hitos, tablas, aristas }`.
    * `CatalogoDestilado`: `doc_id`, `titulo`, `tipo_documento`, `revision`, `version_num`, `fecha_vigor`, `emisor`.
    * `HitoDestilado`: `orden_secuencia`, `fase_operativa`, `objeto_alcance`, `rol_ejecutor`, `rol_responsable`, `registro_codigo`, `registro_nombre`, `criterio_aceptacion`, `condicion_aplicacion`, `seccion_origen`.
    * `TablaDestilada`: `tabla_id`, `titulo`, `eje_x`/`eje_y` (`EjeDestilado {nombre, tipo, unidad}`), `valor_salida_nombre`, `seccion_origen`, `celdas`.
    * `CeldaDestilada`: `LimitesDestilados` por eje, `eje_z`, `valor_resultado`, `unidad_medida`, `nota_pie_condicion`.
    * `AristaDestilada`: `origen_tipo`, `tipo_relacion`, `destino_id`, `destino_tipo`, `contexto`.
    * `json_schema_destilacion()` genera el esquema compacto. Los campos de vocabulario se deserializan como `String` (un valor fuera de vocabulario no tumba el paquete) y el esquema los restringe con `enum`.
  * `ParsedSection` añade `breadcrumb` y `condicion_aplicacion`; `ParsedTableRaw` añade `seccion_codigo`.
  * `application/use_cases/mapeo_destilacion.rs` (nuevo, puro): `clave_documental`, `mapear_revision -> RevisionMapeada` (filas sin vectores + textos a vectorizar), `interpretar_fecha_vigor` e `interpretar_valor_numerico`. `DocumentIngestionService` solo orquesta y vectoriza chunks, hitos y tablas en una única llamada al motor.
  * `ClaudeHeadlessDistiller`: `construir_argumentos`, `construir_entrada`, `reglas_destilacion` y `extraer_paquete` (sobre de la CLI → `structured_output`; si no, `result` como texto; o el paquete desnudo). `TIMEOUT_DESTILACION_SEGUNDOS = 300`.
  * `FastEmbedEngine`: `MODELO_POR_DEFECTO = MultilingualE5Large`, `con_cache(modelo, Option<&Path>)`, `resolver_directorio_cache`, `directorio_cache_efectivo` y `dimension_del_modelo`.
* **Decisiones:**
  * **Modelo de embeddings:** `MultilingualE5Large` (1024 dims, `Qdrant/multilingual-e5-large-onnx`, ≈ 2,2 GB). fastembed 4.9.1 (Cargo.lock) no incluye BGE-M3, y E5-large es su único modelo multilingüe de 1024 dims, así que no cambia el esquema (`DIMENSION_EMBEDDINGS = 1024`). Se aplican los prefijos E5: `query: ` en `generate_embedding` (consultas) y `passage: ` en `generate_batch_embeddings` (indexación).
  * **Caché:** parámetro explícito > `FASTEMBED_CACHE_DIR` > `%LOCALAPPDATA%\lakehouse_rag\fastembed_cache` (Windows) o `$XDG_DATA_HOME` / `~/.local/share` (Unix). Sin dependencias nuevas; nunca el cwd.
  * **Fase:** el esquema solo admite los 9 valores canónicos y el prompt pide el más cercano (soldeo → `EJECUCION`). Lo no interpretable queda como `NO_ESPECIFICADA`.
  * **Prompt:** reglas en `--system-prompt` (sustituye el prompt por defecto de Claude Code) y documento por stdin entre `<documento>…</documento>`; un `</documento>` incrustado se neutraliza.
  * **Versión:** `version_num` si es ≥ 0; si no, el número de `revision`; si no, la revisión incrustada en el `doc_id`.
  * **Origen de las aristas:** el LLM no conoce los IDs internos. Con `origen_tipo = HITO_OPERATIVO`, el origen es el hito cuyo `registro_codigo` es el destino; en otro caso, el documento. `REMITE_A_TABLA` hacia una etiqueta de la propia revisión («4-A») apunta al `tabla_id` versionado. `DEROGA_A` guarda el código sin revisión y la revisión en `contexto_relacion`.
  * **Red de seguridad:**
    * Si un hito tiene registro y falta su arista `EXIGE_LLENAR_REGISTRO`, se sintetiza.
    * Si la celda trae una llamada `(*)` sin nota, la nota se toma del parser.
    * Si el hito no trae condición, hereda la del chunk de su sección.
    * Los campos de más de 16 KiB se recortan y se eliminan los `\0`.
  * **Dominio (bug de la Fase 9):** «Inspector QA/QC» se marcaba como rol fusionado por la `/`. Ahora una sigla compuesta (`QA/QC`, `I+D`) no cuenta como separador de roles.
* **Tests añadidos:**
  * Unitarios: DTO (3), destilador (+6), parser (+3), mapeo (9), FastEmbed (2) y dominio (+1).
  * `tests/adversarial_ingesta_v2_tests.rs` (9): enums inválidos y caos de tipos, JSON roto, roles fusionados, documento sin hitos, tabla sin notas, breadcrumbs profundos/largos/masivos (3.000 secciones), prompt injection, inyección SQL en campos y 1.500 hitos con campos de 16 KiB.
  * Adaptados: `comun_store_v2`, `integration_ingestion_pipeline`, `adversarial_store_v2_tests` (el doc_id con comilla ahora se canonicaliza; el que no tiene alfanuméricos se rechaza) y `acceptance_e2e_etapa1_ingesta` (paquete oro v2, 6 comprobaciones nuevas, H7 y H8 sin `#[ignore]`).
* **Verificación:** 246 tests pasando + 5 ignorados (los reales); `cargo clippy --all-targets -- -D warnings` sin avisos; `cargo fmt --check` OK.
* **E2E real** (Claude Code 2.1.296 con `sonnet`; FastEmbed E5-large; LanceDB en tempdir; perfil `dev`):
  * Comando: `cargo test --test acceptance_e2e_etapa1_ingesta -- --ignored --nocapture --test-threads=1` → 3/3 tests OK.
  * **52/52 comprobaciones en OK** (Rev. 03 + Rev. 04) en las dos ejecuciones completas. La segunda se hizo con el código final.

  | # Etapa 1 | Comprobación (clave del test) | Antes | Fase 11 | Evidencia (ejecución real) |
  | :---: | --- | :---: | :---: | --- |
  | 8 | `custodia_responsable_sin_ejecutor` | FALLA | OK | CUSTODIA → `rol_responsable = "Jefe de Producción"` (Rev. 03 y 04) |
  | 9 | `custodia_registro_columna` (nueva) | OK* | OK | `registro_codigo = FOR-ALM-02` (Rev. 03) / `FOR-ALM-05` (Rev. 04) |
  | 10 | `fases_sql_exactas` | FALLA | OK | `["RECEPCION", "CUSTODIA", "INSPECCION"]` |
  | 16 | `grafo_vocabulario_spec` | FALLA | OK | 0 relaciones fuera de §4 |
  | 17 | `grafo_registros` | FALLA | OK | `EXIGE_LLENAR_REGISTRO` → FOR-ALM-01, FOR-ALM-02, PPI-MAT-09 |
  | 28 | `grafo_deroga_a_rev03` | FALLA | OK | 1 arista `DEROGA_A` vigente en la Rev. 04 |
  | 29 | `esquema_03_campos_spec` + `matriz_columnas_v2_pobladas` (nueva) | NO EXISTE | OK | 3/3 hitos con `rol_ejecutor`, `objeto_alcance` y `chunk_origen_id` |
  | 30 | `esquema_vectores_hito_tabla` + `vectores_hito_tabla_poblados` (nueva) | NO EXISTE | OK | `vector_hito` 3/3, `vector_tabla` 1/1 |
  | — | `tabla_04_unidades_salida_sql` (nueva) | — | OK | `eje_x_unidad` 1/1; `valor_salida_nombre = temperatura_precalentamiento_min_c` |
  | 33 | `chunks_breadcrumb_jerarquico` (nueva) | FALLA | OK | `PO-ALM-03 Procedimiento… > 2. Secuencia Operativa > 2.2 Fase 2 — CUSTODIA` |
  | — | `chunks_vinculos_hitos_tablas` (nueva) | — | OK | 3 chunks con `hitos_vinculados` y 2 con `tablas_vinculadas` |
  | 34 | `parser_tabla_titulo_y_notas` (nueva) | FALLA | OK | Título «Tabla 4-A Temperatura mínima…» y notas `(*)` y `(1)` |
  | 12/27 | `celda_cruce_valor` | OK | OK | `100 (*)` (Rev. 03) → `125 (*)` (Rev. 04), con la nota QA/QC + HV10 |

  * Grafo de la Rev. 03 (ejecución 2): FOR-ALM-01, FOR-ALM-02, PPI-MAT-09, PCC-01, `PO-ALM-03@v3-TAB-0000` (remisión a la Tabla 4-A), UNE-EN-1090-2, UNE-EN-ISO-15614-1, PO-CAL-01, UNE-EN-10204 y ANEXO-II.

  Tiempos (ejecución 1 / ejecución 2):

  | Paso | Rev. 03 | Rev. 04 |
  | --- | ---: | ---: |
  | Carga de FastEmbed E5-large (1.ª vez, con descarga de ≈ 2,2 GB) | 100,3 s | — |
  | Carga de FastEmbed E5-large (en caché) | 7,0 / 5,7 s | — |
  | Destilación aislada con la configuración por defecto | 33,8 s | — |
  | 2. Destilación (`claude -p --json-schema`) | 26,6 / 26,7 s | 27,7 / 26,3 s |
  | 3. Embeddings (13 textos: 9 chunks + 3 hitos + 1 tabla, CPU) | 10,3 / 9,8 s | 8,9 / 8,3 s |
  | 4-5. Escritura STAGED + commit | 0,22 s | 0,21 s |
  | **Total `ingestar_documento`** | **37,1 / 36,7 s** | **36,9 / 34,8 s** |
* **Desviaciones del plan:**
  * Se añaden `seccion_origen` a la tabla destilada y `destino_tipo` a la arista (no estaban en la lista de la tarea) para enlazar la tabla con su chunk y fijar el tipo de nodo.
  * El caso adversarial «doc_id con comilla» cambia de semántica: ya no se rechaza, sino que se canonicaliza (`PO-ALM-03'B` → `PO-ALM-03-B`), como pide `desde_codigo_libre`. Solo se rechaza el que queda vacío.
  * La destilación sigue siendo del documento entero, no por sección (§5.2): con el fixture (≈ 9 KB) sobra margen. El parser ya entrega las secciones con breadcrumb para hacerlo cuando haga falta.
  * BGE-Large-EN deja de ser el modelo por defecto. Un centro ingerido con BGE tiene vectores incompatibles (mismas dimensiones, distinto espacio): la Fase 12 debe guardar el modelo en `center_manifest.json` y no mezclarlos.
  * TDD: el DTO, el destilador, el parser y el mapeo se escribieron junto con sus tests (rojo por compilación). El fallo de siglas `QA/QC` y la remisión a tabla se vieron en rojo antes de corregirlos.
* **Notas para la Fase 12:**
  * `FastEmbedEngine::try_new(None)` descarga E5-large en `%LOCALAPPDATA%\lakehouse_rag\fastembed_cache` la primera vez (≈ 100 s). La GUI debe avisar o precargar el modelo en segundo plano. Para el manifiesto, usar `MODELO_POR_DEFECTO`.
  * `hitos_vinculados`, `tablas_vinculadas` (List<Utf8>) y `chunk_origen_id` ya permiten proyectar el grafo hito → chunk → tabla.
  * Las aristas `EXIGE_LLENAR_REGISTRO` salen con `origen_tipo = HITO_OPERATIVO` y `origen_id = hito_id`; el resto, con el documento como origen.
  * Los tests reales usan `target/fastembed_cache` (`FASTEMBED_CACHE_DIR`), donde ya está E5-large. Paquetes destilados reales: `target/validacion_etapa1/*.destilado.json`, reinyectables con `MockDistiller::registrar_respuesta`.

### Sesión 12 (2026-10-17) — Fase 12: Centros en disco, proyección y fachada
* **Objetivo:** plan v2 §1 (problemas 6 y 8), §2.3 (`CenterRegistryPort`, `LakehouseProjectionPort`) y §3.1 (fachada única y primera puerta E2E con el binario real).
* **Contratos nuevos:**
  * `application/dtos/centros.rs`: `ModeloEmbeddingsActivo`, `ManifiestoCentroDisco` (`schema_version = 2`, `modelo_embeddings`, `dimension_embeddings`) con `validar_compatibilidad`, `DocumentoOriginal` y `validar_ruta_centro` (rechaza `..`, `\0` y rutas vacías).
  * `CenterRegistryPort` añade `eliminar` (solo quita la entrada del registro). Adaptador `infrastructure/storage/center_registry_json.rs`: `JsonCenterRegistry::new(dir)`, formato §3.1 (`id`, `nombre`, `ruta_disco`, `activo`, `fecha_creacion`, `estadisticas`). Directorio: `LAKEHOUSE_RAG_CONFIG_DIR` > `%APPDATA%\lakehouse_rag` (XDG o `~/.config` en Unix).
  * `CenterWorkspacePort` (`ruta_absoluta`, `preparar`, `abrir`, `guardar_original`) → `CentroAbierto { id, ruta, manifiesto, store }`. Adaptador `center_workspace_disk.rs` (`DiskCenterWorkspace`).
  * `LakehouseProjectionPort` (`reconstruir_vistas`, `retirar_vistas`, `centros_proyectados`) y `nombre_vista(centro, tabla) = "{centro}_{tabla}"`. Adaptador `query_engine/lakehouse_projection.rs` (`LanceProjection`). Nuevos `PetgraphEngine::vaciar` y `TableLookupService::vaciar`.
  * Fachada `application/use_cases/lakehouse_app.rs`: `LakehouseApp::new(DependenciasLakehouse)` con:
    * Centros: `crear_centro`, `listar_centros`, `activar_centro`, `desactivar_centro`, `eliminar_centro` e `hidratar_centros_activos`.
    * Ingesta: `ingestar(ruta, centro) -> ResultadoIngesta`.
    * Consulta: `preguntar(texto, centro, modelo) -> RespuestaLakehouse { respuesta, herramientas: Vec<UsoHerramienta> }`.
    * Diagnóstico: `integridad(centro) -> InformeIntegridadCentro` (incluye `tablas_ausentes`).
    * `servidor_herramientas()`.
    * Trait `AsistenteLakehousePort` (`listar_modelos`, `conversar`), implementado por `LakehouseApp` y por `AsistenteDirecto`.
  * Composición única `infrastructure/bootstrap.rs`: `componer_lakehouse(&ConfiguracionComposicion)` y `motores_consulta()`. `LazyFastEmbedEngine` (`embeddings/lazy_engine.rs`) carga E5-large solo al vectorizar.
  * CLI `presentation/cli/{mod,ejecutor,salida}.rs`: `ModoEjecucion::Comando(ComandoCli, FormatoSalida)` y `ErrorArgumentos`. Códigos de salida: 0 OK, 1 error, 2 argumentos y 3 centro no íntegro.
  * GUI: `iniciar_app_tauri(Arc<LakehouseApp>)`, `GuiChatService::desde_asistente` y comandos `listar_centros`, `activar_centro`, `desactivar_centro` e `ingestar_documento`. El frontend aún no los usa (Fase 14).
  * MCP: hidrata los centros activos y sirve `app.servidor_herramientas()`.
* **Decisiones:**
  * **Proyección:**
    * DataFusion recibe **todas las filas `COMMITTED`**, vigentes e históricas (con `vigente`, `version_num`, `fecha_vigor` y `fecha_derogacion`), para el *time travel* SQL de la Fase 13. Nunca recibe filas `STAGED`. Las tools filtran `vigente = true`.
    * Las columnas vectoriales no se proyectan.
    * Petgraph y TableLookup se recargan con lo **vigente** de todos los centros proyectados.
    * Las 6 tablas se leen antes de tocar DataFusion: si una falla, las vistas previas quedan intactas.
  * **Ciclo de vida:**
    * Un centro nuevo nace **inactivo**. `activar` valida el manifiesto, proyecta y solo entonces marca el centro como `activo`.
    * Un proceso nuevo (CLI) proyecta bajo demanda en `preguntar`; la GUI y el MCP hidratan los centros activos al arrancar.
    * Preguntar a un centro inactivo o inexistente es un error explícito.
  * **Disco:**
    * `preparar` solo acepta un directorio inexistente, vacío o que ya sea un centro (readopción tras `eliminar`).
    * Dos centros no comparten ruta.
    * Copia en `raw_docs/<sha256>_<nombre_seguro>` + `.sha256`. Si ya existe, no se reescribe. No se marca como solo lectura, para no romper los borrados de tempdir en Windows.
    * La copia se hace antes de la ingesta. Si la ingesta falla, la copia queda: es inmutable y está indexada por hash.
  * **Ingesta confirmada con proyección fallida:** devuelve OK con `aviso_proyeccion`, porque los datos ya están confirmados en LanceDB.
  * **Tools (lo mínimo para leer v2):**
    * Los nombres canónicos `01_…`–`06_…` se reescriben a la vista entrecomillada del centro (`"obra-a_03_matriz_operativa"`): el `-` del id obliga a usar comillas. Los identificadores ya cualificados se respetan.
    * `inspect_lakehouse_catalog` y `query_operational_matrix` leen columnas v2 (`parametro` → `objeto_alcance`).
    * El system prompt incluye `INVENTARIO_TABLAS_V2`.
  * **Petgraph:**
    * El grafo es por documento (sin versión). Una arista que el dominio rechaza se omite con `warn` en vez de tumbar la proyección. Es el caso de la `DEROGA_A` de la Rev. 04 hacia `PO-ALM-03`, que aquí es un bucle.
    * Los orígenes versionados (`…@v4-HIT-…`) se canonicalizan con `desde_codigo_libre`.
  * **Consola en release:**
    * Se mantiene `windows_subsystem = "windows"`.
    * Los modos que no son GUI llaman a `AttachConsole(ATTACH_PARENT_PROCESS)` solo si `GetStdHandle(STD_OUTPUT_HANDLE)` es nulo. Si stdout está redirigido (tubería o archivo, como en la E2E), se escribe ahí.
    * Verificado en debug (subsistema de consola). Release **no se ha probado** en esta sesión: si en una consola interactiva el texto no aparece tras `AttachConsole`, habrá que reabrir `CONOUT$`.
* **Tests añadidos (+45; 291 pasando + 7 ignorados):**
  * Unitarios: DTO de centros (4), registro JSON (4), espacio en disco (2), proyección (2), `nombre_vista` (1), fachada (2), FastEmbed diferido (2), reescritura SQL (2), Petgraph v2 (1), CLI (3) y salida (2).
  * `tests/integration_lakehouse_app.rs` (9):
    * Estructura del centro y manifiesto.
    * Ciclo completo activar → ingerir → consultar → desactivar.
    * Idempotencia de la reingesta.
    * `preguntar` con tools.
    * Proceso nuevo e hidratación.
    * Multi-centro §9.1.
    * Eliminar y readoptar.
    * Ingesta fallida sin residuos.
    * Regresión de la autoderogación.
  * `tests/adversarial_centros_proyeccion_tests.rs` (11):
    * Rutas con `..` e ids hostiles.
    * Registro corrupto (4 variantes, sin sobrescribirlo).
    * Centro inexistente (6 operaciones).
    * Activación doble y concurrente.
    * Alta duplicada por id o por ruta, y directorio ajeno no vacío.
    * Manifiesto de otro modelo, otra dimensión u otro esquema, corrupto o ausente.
    * `.lance` borrado a mano.
    * Nombre de archivo hostil.
    * SQL hostil sobre las vistas.
  * Adaptado: `tests/integration_agent_tools.rs` (lotes con forma v2).
  * TDD: antes de corregir se vieron en rojo la autoderogación (en la puerta E2E) y la ingesta fallida (un mock que no fallaba). El resto se escribió junto con su test (rojo por compilación).
* **Verificación:** `cargo test` con 291 OK + 7 ignorados; `cargo clippy --all-targets -- -D warnings` sin avisos; `cargo fmt --check` OK.
* **Puerta E2E real:**
  * Comando: `cargo test --test e2e_binario_fase12 -- --ignored --nocapture --test-threads=1`.
  * Entorno: binario `target/debug/lakehouse_rag`, Claude Code con `sonnet` y FastEmbed E5-large en `target/fastembed_cache`.
  * Resultado: **2/2 OK en 302 s**.
  * 1.ª ejecución: **FALLÓ** la ingesta de la Rev. 04 (código 1) con «Paradoja lógica: una sección normativa no puede derogarse a sí misma». El fallo ocurría al cargar Petgraph tras el commit. Se corrigió (las aristas inválidas se omiten y se informa `aviso_proyeccion`) y se añadió la regresión.
  * 2.ª ejecución:

  | Paso (binario, `--json`) | Tiempo | Resultado |
  | --- | ---: | --- |
  | `--crear-centro obra-planta-solar-01 <tmp> "Planta Solar"` | 0,2 s | Centro inactivo; estructura y manifiesto v2 |
  | `--activar` | 0,1 s | 6 vistas vacías |
  | `--ingest procedimiento_recepcion_rev03.md` | 39,4 s | `PO-ALM-03` v3: 9 chunks, 3 hitos, 1 tabla, 9 celdas, 10 aristas |
  | `--ingest procedimiento_recepcion_rev04.md` | 52,9 s | v4, derogadas `[3]`; 01: 2 filas (1 vigente); 02: 18 filas (9 vigentes) |
  | `--integridad` | 66,6 s* | `integro: true` |
  | `--ask "¿Quién es el responsable de la custodia y qué registro llena?"` | 57,9 s | Tool `query_operational_matrix(fase=CUSTODIA)` → **Jefe de Producción**, **FOR-ALM-05** (Registro de Custodia, Preservación y Trazabilidad), con la condición «acopio > 7 días» |
  | `--ask` con agregación SQL | 23,6 s | `execute_athena_sql`: `SELECT fase_operativa, COUNT(*) … WHERE vigente = true GROUP BY fase_operativa` → CUSTODIA 1, INSPECCION 1, RECEPCION 1 |
  | Multi-centro: A con la Rev. 03 (inactivo), B vacío y activo | — | `--ask … --centro obra-a` → código 1, «está inactivo». `--ask … --centro obra-b` → `SELECT COUNT(*) … 01_documentos_catalogo` = **0**; ningún resultado contiene `PO-ALM-03` |

  \* El tiempo de `--integridad` coincidió con un `cargo test` y un `cargo clippy` lanzados en paralelo en la misma máquina. No se ha medido de forma aislada.
* **Qué falta en las tools (para la Fase 13; no se reescriben aquí):**
  * `lookup_double_entry_table`:
    * Usa `TableLookupService` con la semántica previa: no distingue `>=`/`>` en cada extremo y no admite `fecha_referencia`.
    * Busca por `tabla_id` sin filtrar el centro: con varios centros activos, dos `tabla_id` iguales se mezclarían.
  * `query_operational_matrix`:
    * Compara la fase con `LOWER()`, así que «Recepción» con tilde no casa con `RECEPCION`.
    * Falta normalizar con `FaseOperativa` y filtrar por `rol_*` y `registro_codigo`.
  * `traverse_normative_graph`:
    * Trabaja sobre un grafo por documento y pierde la versión (la autoderogación v4 → v3 se omite).
    * No tiene poda antisupernodos.
  * `execute_athena_sql`:
    * Sigue validando por prefijos de texto, no con el parser de DataFusion.
    * No tiene límite de filas.
  * Ninguna tool acepta `fecha_referencia`; las vistas ya contienen el histórico para implementarlo.
* **Notas para la Fase 13:**
  * Las vistas se llaman `"{centro}_{tabla}"` y hay que entrecomillarlas: usa `nombre_vista` y `reescribir_tablas_canonicas` (`lakehouse_tool_service.rs`).
  * `glue_catalog/inventario_tablas.json` tiene las columnas reales de cada vista, por si `inspect_lakehouse_catalog` debe devolverlas.
  * La E2E puede reutilizar `tests/e2e_binario_fase12.rs` como plantilla: `Sesion::ejecutar` lanza el binario con `--json`.
  * La GUI envía un `centro_id` fijo desde `app.js`. Usar los nuevos comandos de centros es trabajo de la Fase 14.

### Sesión 13 (2026-10-17) — Fase 13: Tools v2
* **Objetivo:** plan v2 §1 (problemas 2, 3, 4, 5, 6 y 7) y §2.4: las 6 tools sobre las vistas que reconstruye la Fase 12.
* **Contratos nuevos:**
  * `application/dtos/tools.rs`: `LakehouseToolName::TODAS` (6), JSON Schema escrito a mano (`additionalProperties: false`), parámetros `deny_unknown_fields`, `ValorEje` (número o texto), `parsear_fecha_referencia` (ISO estricto, 1900–9999) y `DESCRIPCION_VISTAS_SQL` (vistas y columnas; también es `INVENTARIO_TABLAS_V2`).
  * `AnalyticalEnginePort::ejecutar_consulta_lectura(sql, &PoliticaConsultaLectura { tablas_permitidas, limite_filas, timeout })` → `ResultadoConsultaLectura { lotes, filas, truncado }`. En DataFusion: `DFParser` (exactamente una sentencia y solo `Statement::Query`), plan lógico con `apply_with_subqueries` (todo `TableScan` debe ser una vista del centro: bloquea otros centros, `read_csv`/`generate_series`…), `LIMIT limite+1` y ejecución en un runtime dedicado (hilo propio) con timeout y cancelación.
  * `ports/semantic_index.rs` (`IndiceSemanticoPort`, `FragmentoIndexado`) + `query_engine/indice_semantico_memoria.rs`. `LanceProjection::con_indice_semantico` lo rellena con chunks (02) e hitos (03) **con vectores** de todas las revisiones confirmadas.
  * `LakehouseToolService::new(motor, indice, embeddings)` + `ConfiguracionHerramientas` (SQL 1000 filas / 30 s, `max_vecinos` 15, `umbral_hub` 25, `max_nodos` 50, `top_k` 8). Submódulos `lakehouse_tool_service/{catalogo,matriz,lookup,sql_lectura,grafo,busqueda}.rs`.
  * `bootstrap::motores_consulta(embeddings)`.
* **Decisiones:**
  * Todas las tools leen solo las vistas del centro (las exigen proyectadas: centro inactivo → error) y aplican la vigencia con `vigente = true` o con la ventana de `01` a `fecha_referencia`. Los filtros del agente se evalúan en Rust (normalizados), nunca se interpolan en SQL.
  * `TableLookupService` y `PetgraphEngine` (globales, sin centro ni fecha) dejan de usarse en las tools; la proyección aún los carga.
  * Lookup: acepta el id versionado (`PO-ALM-03@v4-TAB-0000`) o el código del título (`4-A`, por segmentos: no casa `14-A`); con eje z prefiere la coincidencia exacta; varias candidatas → `ambiguo` (con `prevalece` si difieren en prelación); nunca interpola.
  * SQL con `fecha_referencia`: las 6 tablas canónicas se sustituyen por CTE con la ventana (`WITH RECURSIVE` + fecha → rechazo).
  * Grafo: nodos = códigos canónicos (origen = `doc_id` de la arista; destino sin revisión). El origen siempre se expande con tope de vecinos; los demás nodos con grado > 25 son `hub_resumido`.
  * Búsqueda: tema = último segmento del breadcrumb sin numeración; `prevalece` en el nivel menor y `supletorio` en el resto cuando hay varios niveles.
  * System prompt v2: tabla de elección de tool, cita doc_id + revisión, nota al pie literal, condición de aplicación, prelación con supletorio y prohibición de inventar o interpolar.
* **Tests añadidos (+31; 322 pasando + 8 ignorados):** unitarios (DTOs 4, matriz 1, lookup 6, SQL 3, grafo 3, búsqueda 3, índice 1, prompt 1; MCP ampliado a 6 tools); `tests/integration_tools_v2.rs` (6: fronteras e = 20 / 20,0001, nota (*), time travel 125 → 100, matriz exacta y a fecha, agregación exhaustiva y truncado a 1000, hub de 1.000 aristas ≤ 50 nodos, prelación pliego 100 % / norma 10 %, condición heredada, catálogo y aislamiento de centro en las 6 tools); `tests/adversarial_tools_v2_tests.rs` (10). Adaptados al contrato v2: `tests/adversarial_agent_tools_tests.rs`, `tests/integration_agent_tools.rs` y `tests/acceptance_gui_chat_claude_headless.rs`. La infraestructura E2E pasa a `tests/comun_e2e/` (la usan las Fases 12 y 13).
* **TDD:** en rojo antes de corregir: el timeout (vencía a los 7,8 s: la agregación cartesiana saturaba el runtime → runtime dedicado), el `tabla_id` versionado (el lookup por «4-A» no encontraba la tabla) y la expectativa del grafo (los hitos también generan aristas `EXIGE_LLENAR_REGISTRO`). El resto se escribió con su test (rojo por compilación).
* **Verificación:** `cargo test` 322 OK + 8 ignorados; `cargo clippy --all-targets -- -D warnings` sin avisos; `cargo fmt --check` OK. Además se adaptaron a la salida-objeto de las tools `tests/comun_centros` (`escalar`), `tests/integration_lakehouse_app.rs`, `tests/adversarial_centros_proyeccion_tests.rs` (un `centro_id` hostil ahora es `ToolResult` con `is_error`, no `Err`), la E2E de la Fase 12 y los argumentos del modo heurístico de `MockAgentRunner`.
* **Puerta E2E real** (`cargo test --test e2e_binario_fase13 -- --ignored --nocapture --test-threads=1`; binario debug, Claude `sonnet`, FastEmbed E5-large): **1/1 OK en 387 s**, a la primera.

  | Paso | Tiempo | Tools usadas | Resultado |
  | --- | ---: | --- | --- |
  | Ingesta Rev. 03 / Rev. 04 / PCC-01 / NT-SOL-01 | 47,6 / 40,6 / 19,6 / 21,9 s | — | v3 (9 chunks, 3 hitos, 9 celdas, 10 aristas); v4 (11 aristas); PCC-01 v1 (3 chunks); NT-SOL-01 v2 (5 chunks, 3 hitos) |
  | (a) Responsable y registro de la custodia | 32,7 s | `query_operational_matrix(fase=CUSTODIA)` | **Jefe de Producción**, **FOR-ALM-05** (Rev. 04, breadcrumb «2.2 Fase 2 — CUSTODIA») |
  | (b) Tabla 4-A, e = 30 mm, CE = 0,42 | 44,4 s | `lookup` ×2 | 1.ª con `tabla_id: "4-A"` → `tabla_no_encontrada` + lista; 2.ª con `PO-ALM-03@v4-TAB-0000` → `resuelto`: **125 °C (*)** con la nota |
  | (c) Igual a 2026-06-01 | 62,4 s | `lookup` ×2 con `fecha_referencia` | Mismo patrón; `PO-ALM-03@v3-TAB-0000` → **100 °C (*)** (Rev. 03) |
  | (d) Registros que firma el Jefe de Producción | 56,4 s | `execute_athena_sql` ×2 (JOIN 03–01, `vigente`) | Solo **FOR-ALM-05**; no cita FOR-ALM-02 |
  | (e) NT-SOL-01: temperatura y cuándo | 34,4 s | `search_normative_context(doc_id=NT-SOL-01)` | **80 °C** con la condición heredada **«bajo cero (< 0 °C)»** |
  | (f) % de radiografía en uniones a tope | 26,8 s | `search_normative_context` | `conflicto_prelacion: true` → **100 % (PCC-01, prevalece)** y NT-SOL-01 10 % como supletorio |

* **Desviaciones y pendientes:**
  * El destilador guarda el `tabla_id` versionado y el título sin el código «4-A» (`mapeo_destilacion.rs`, `fila_tabla`: el título sustituye al id humano). El lookup lo compensa con la lista de tablas, pero cuesta una llamada extra. Propuesta: conservar el código humano en `titulo_tabla` (o una columna `codigo_tabla`) en la ingesta.
  * El agrupado por tema de la búsqueda depende de que las secciones se titulen igual; documentos con títulos distintos para el mismo tema no se marcan como conflicto (el system prompt sigue obligando a aplicar la prelación).
  * `TableLookupService`, `PetgraphEngine` y `HybridSearchService` (LanceDB directo) ya no los usan las tools: candidatos a retirarse.
  * La consulta SQL que vence el timeout sigue consumiendo CPU en su runtime dedicado hasta el siguiente punto de cesión (la respuesta vuelve a tiempo).
* **Notas para la Fase 14:**
  * La salida de cada tool es ahora un objeto: matriz → `filas[]` con `cita { doc_id, revision, version_num, chunk_origen_id, breadcrumb }`; lookup → `estado`, `valor_resultado_texto`, `nota_pie_condicion`, `cita`; grafo → `nodos[]` (`hub_resumido`, `resumen_hub`) y `aristas[]` con `cita` (graphResult); búsqueda → `grupos[].resultados[]` con `prevalece`/`supletorio` y `cita`; SQL → `resultado[]`, `truncado`. `app.js` (citas, matrixResult, graphResult) debe leer estas formas.
  * El MCP expone las mismas 6 tools (`MCP_GUIDE.md` §5 actualizado).
  * Notas de traspaso en la carpeta compartida de sesiones: `fase13_tools_v2_handover.md`.

### Sesión 14 (2026-10-17) — Fase 14: Cableado de la ventana, MCP y CLI
* **Objetivo:** plan v2 §3.1: la ventana Tauri, el MCP y la CLI usan `LakehouseApp` con datos reales y sin simulaciones.
* **Contratos nuevos:**
  * `application/dtos/ingestion.rs`: `EtapaIngesta` (`copia`, `parseo`, `destilacion`, `embeddings`, `escritura`, `proyeccion`, con `descripcion` y `porcentaje`) y `ObservadorIngesta`.
  * `DocumentIngestionService::con_observador` y `LakehouseApp::ingestar_con_progreso(ruta, centro, observador)`; `ingestar` delega en ella.
  * `application/dtos/centros.rs`: `validar_ruta_documento` (sin `..` ni `\0`, extensión `.md`/`.markdown`/`.txt`) y `EXTENSIONES_INGERIBLES`. Se aplica en `LakehouseApp`, así que también afecta a la CLI.
  * `LakehouseApp` serializa las ingestas con un cerrojo propio: dos drops simultáneos se encolan y la copia repetida devuelve «ya ingerido».
  * `presentation/gui/comandos.rs`: DTOs `CentroDto`, `ActivacionDto`, `IngestaDto`, `DocumentoCatalogoDto`, `IntegridadDto` y `ProgresoIngestaDto` (camelCase), más funciones probadas sin ventana. `catalogo_centro` usa `execute_athena_sql` sobre `01_documentos_catalogo`, así que exige el centro activo.
  * `presentation/gui/resultados_tools.rs`: `CitationDto` (doc_id, revisión, versión, breadcrumb, fragmento, tool, condición y nota) y `extraer_citas` (como mucho 30, sin duplicados), `extraer_matrix_result` (`tipo: matriz | lookup`) y `extraer_graph_result`.
  * Comandos Tauri: `listar_centros`, `crear_centro`, `activar_centro`, `desactivar_centro`, `eliminar_centro`, `seleccionar_documentos`, `seleccionar_directorio`, `ingestar_documento`, `catalogo_centro`, `integridad_centro`, `listar_modelos_claude` y `enviar_mensaje_chat`.
  * Eventos: `ingesta-progreso`, `documentos-soltados` y `documentos-arrastre`.
* **Decisiones:**
  * **Drag & drop:** en WebView2, el `drop` HTML no trae rutas. Lo fiable es `WindowEvent::DragDrop`, que se reemite desde Rust con las rutas, y la ingesta siempre va por ruta.
  * **Diálogos:** el diálogo nativo se invoca desde Rust (`DialogExt`), sin la API JS del plugin. Las capabilities no cambian: los eventos entran en `core:default` y los comandos propios no requieren permisos ACL.
  * **Centro inactivo:** la ingesta se permite, como en la CLI. Se devuelve `proyectado: false` con un aviso y los datos no son consultables hasta activar el centro.
  * **XSS:** `app.js` solo usa `innerHTML` para `replyHtml`; el resto se pinta con `textContent`. `texto_a_html_seguro` escapa cada línea o celda y solo después pinta el Markdown: encabezados → `<strong>`, `- ` → `•`, tablas → `<table class="md-tabla">`, `\*` → `*`, `**` y `` ` ``.
  * **Modelos:** el cambio de modelo en caliente usa el alias elegido en cada turno, con el mismo historial por centro.
  * **Dependencias:** se añade `tauri-plugin-dialog` 2.8.3, lo que regenera `gen/schemas/*`. `petgraph` queda declarado en `Cargo.toml` sin uso; retirarlo es un cambio de dependencias aparte.
* **Limpieza (Fase 13):**
  * Retirados `TableLookupService`, `PetgraphEngine`, `HybridSearchService`, los puertos `table_lookup`, `graph_engine` y `hybrid_search`, y los campos `aristas_grafo` y `celdas_lookup` de `ResumenProyeccion` (salida CLI incluida).
  * `LanceProjection::new(datafusion)` recibe ahora un solo argumento.
  * **Tests retirados (18), porque solo cubrían el código retirado:**
    * `tests/integration_hybrid_search.rs` completo (5: RRF, aislamiento, vigencia, pesos y STAGED). Las tools v2 lo cubren con las vistas del centro y el índice semántico.
    * `adversarial_embeddings_tests` (4: centro vacío, consulta vacía, inyecciones y `top_k` en `HybridSearchService`).
    * `adversarial_query_tests` (3: NaN/inf de `TableLookupService` y bucles o grafo vacío de `PetgraphEngine`).
    * `integration_query_engine` (2: hidratación de TableLookup y Petgraph).
    * Unitarios de los módulos borrados (4).
  * **Tests adaptados:** la autoderogación y la activación doble comprueban ahora las filas vigentes de las vistas 05 y 06 en lugar de los contadores retirados.
* **Tests añadidos (+12 nuevos; 326 pasando + 8 ignorados):**
  * Unitarios:
    * Etapas del observador.
    * Título de tabla con etiqueta.
    * Ruta de documento.
    * DTOs de progreso y catálogo.
    * Citas, matriz y grafo (4).
    * Render seguro (2).
  * `tests/integration_gui_v2.rs` (3):
    * Ciclo completo de comandos con etapas, catálogo vigente/derogado, integridad y eliminación.
    * Lookup «4-A» a la primera con el título destilado sin código.
    * Chat de la ventana con citas, `matrixResult` y `graphResult` desde tools reales.
  * `tests/adversarial_gui_v2_tests.rs` (8):
    * Rutas con `..`.
    * Archivo inexistente, directorio y binario.
    * Extensiones no soportadas.
    * Centro inactivo.
    * HTML/JS en el documento y en la respuesta.
    * Análisis estático de `app.js` (un solo `innerHTML`, sin `onclick=` ni simulaciones).
    * Ingestas concurrentes.
    * Ids y rutas de centro hostiles.
  * TDD: en rojo antes de implementar se vieron el título «4-A», el render de negrita y código, y el de encabezados y tablas, además de la cita del breadcrumb (mayúsculas del fixture). El resto se escribió con su test (rojo por compilación). Las suites adversariales pasaron a la primera.
* **Verificación:** `cargo test` 326 OK + 8 ignorados; `cargo clippy --all-targets -- -D warnings` sin avisos; `cargo fmt --check` OK; `node --check app.js` OK.
* **Verificación REAL de la ventana** (binario debug con `--gui`; registro en `D:\lh_fase14\config`; Claude Code y FastEmbed reales):
  * **Cómo se condujo:** WebView2 con `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`. El guion `docs/validacion/fase14/cdp_ventana.mjs` (CDP) hace clics y rellena el DOM de la ventana real, y las capturas `Page.captureScreenshot` son del contenido de esa ventana. Esta sesión no tenía herramienta de *computer use*.

  | Paso | Captura | Qué se vio |
  | --- | --- | --- |
  | Arranque | `01_arranque.png` | Ventana frameless 1280×720, «Sin centros: crea uno», catálogo real de 13 modelos de Claude Code |
  | Alta de centro (formulario + «Crear y activar») | `02`, `03` | Centro `obra-planta-solar-01` creado y activo, con estructura en `D:\lh_fase14\obra-planta-solar-01` |
  | Ingesta Rev. 03 + Rev. 04 (evento `documentos-soltados`) | `04`, `05` | Barra por etapas («Destilando con Claude… (≈ 30–60 s)», «Calculando embeddings…»); aviso de los ~2,2 GB; v3: 9 chunks, 3 hitos, 1 tabla, 9 celdas, 10 aristas; v4: 11 aristas, «deroga v3». Catálogo: Rev. 04 vigente desde 2026-09-15 y Rev. 03 derogada (2026-03-01 → 2026-09-15) |
  | Custodia con **sonnet** | `06` | 21 s, `query_operational_matrix` → **Jefe de Producción**, **FOR-ALM-05**, condición «acopio > 7 días» en notas y cita PO-ALM-03 Rev. 04 «2.2 Fase 2 — CUSTODIA»; Studio con la fila de la matriz |
  | Cambio en caliente a **haiku**: Tabla 4-A, e = 30 / CE = 0,42 | `07` | 36 s, `inspect_lakehouse_catalog` + **un solo** `lookup` → **125 °C (*)** con la nota obligatoria (QA/QC + HV10); Studio: «resuelto», rango 20 < x ≤ 40, celda `@v4-CEL-0004` |
  | Vuelta a **sonnet**: grafo desde PO-ALM-03 | `08` | `traverse_normative_graph` → 11 nodos y 11 aristas en Studio (registros, PCC-01, normas, DEROGA_A) |
  | Cierre con el botón ✕ y relanzado | `09` | Rehidratación «Centro proyectado (6 vistas)»; catálogo intacto; tabla Markdown del modelo pintada como tabla real |
  | GUI **release** + pregunta con haiku | `10` | Respuesta correcta; durante la llamada a `claude` no apareció ninguna ventana de consola nueva (`ventanas_visibles.ps1`: solo la app, Claude Desktop y la terminal previa) |

  * **No verificado visualmente:**
    * El diálogo nativo de archivos y carpetas (CDP no puede manejar diálogos del sistema).
    * El arrastre real desde el Explorador. El frontend se ejercitó emitiendo el mismo evento que reemite Rust; `WindowEvent::DragDrop` → emisión solo se ha comprobado por compilación y lectura de código.
    * Minimizar y maximizar (sí se usó cerrar).
    * El tema claro.
* **CLI release:**
  * `cargo build --release` OK (1 min 51 s). Por tubería, `--centros` escribe la salida y devuelve código 0.
  * En una consola PowerShell interactiva (`consola_release.ps1`), `--centros` y `--ask` **sí escriben en la consola padre** (`AttachConsole`), pero PowerShell **no espera** a un ejecutable de subsistema Windows: el texto aparece después del prompt y `$LASTEXITCODE` no es fiable.
  * Para scripts, usar tubería (`… | Out-Host`) o `Start-Process -Wait`.
  * Propuesta: un binario de consola aparte (`[[bin]]` `lakehouse_rag_cli`).
* **Notas para la Fase 15:**
  * Hay que probar con el usuario el diálogo nativo, el arrastre real desde el Explorador, minimizar y maximizar, y el tema claro. Son los puntos no vistos.
  * Fixtures: `tests/fixtures/*.md`. El centro de esta prueba quedó en `D:\lh_fase14` (registro aislado con `LAKEHOUSE_RAG_CONFIG_DIR`); se puede borrar.
  * El agente a veces llama primero a `inspect_lakehouse_catalog` antes del lookup. Es correcto, pero cuesta unos 10 s más.
  * Pendientes menores:
    * Retirar `petgraph` de `Cargo.toml`.
    * Binario CLI de consola.
    * Botón «Eliminar centro» en la UI (hoy solo existe el comando IPC).

### Sesión 14b (2026-10-17) — Fase 14: iteración UI/UX tras la prueba del usuario
* **Origen:** el usuario probó la ventana y encontró problemas de diseño y de flujo de la información: «Fuentes» partidas letra a letra, Markdown mal pintado, paneles fijos y barras de scroll del sistema. El análisis completo, con causa raíz y corrección de cada punto, está en `docs/validacion/fase14/analisis_ui_ux.md`. El usuario aprobó (SÍ) la dependencia `pulldown-cmark` y el plan.
* **Cambios en el backend:**
  * `presentation/gui/markdown_seguro.rs`: CommonMark + tablas GFM con `pulldown-cmark` 0.12.
    * El HTML crudo pasa a texto escapado.
    * Los enlaces se pintan como `<span class="md-enlace" title=url>` (solo `http`/`https`): nunca navegan la ventana.
    * Las imágenes se reducen a su texto alternativo.
    * El salto simple es visible y las tablas van envueltas con scroll horizontal.
    * `texto_a_html_seguro` delega aquí; el renderizador propio línea a línea queda retirado.
  * `resultados_tools.rs`:
    * `FuenteCitadaDto` y `agrupar_fuentes`: un chip por documento + revisión, con sus apartados y citas. Arregla las 10 «Fuentes» repetidas.
    * `resumen_tool`: una línea legible por tool, por ejemplo «resuelto · 125 (*) °C».
    * `ChatResponsePayload.fuentes` y `ToolCardDto.resumen` son campos nuevos.
* **Frontend reescrito** (`index.html`, `styles.css` sin las reglas heredadas de la Fase 7, `app.js`):
  * **Paneles:** separadores arrastrables (220–520 px y 280–760 px, con teclado ←/→ y doble clic para restablecer), plegado a un riel de 40 px y anchos recordados.
  * **Scroll:** barras finas sin flechas mediante `::-webkit-scrollbar`. No se usa `scrollbar-width`, que en Chromium ≥ 121 anula esos estilos.
  * **Chat:** sin scroll vertical dentro de los mensajes; columna de lectura de 780 px; al llegar una respuesta se desplaza hasta su inicio, con botón «↓ Ir al final»; estado vacío con ejemplos clicables; entrada multilínea (Intro envía, Mayús+Intro salta de línea); cronómetro y botón Copiar.
  * **Tools y fuentes:** cada tool ocupa una línea con resumen y su detalle JSON se abre en el Studio. Las fuentes son chips agrupados, y las notas y condiciones van en un bloque compacto.
  * **Avisos:** salen como *toasts* y quedan en el registro «Actividad» del panel 1, fuera de la conversación.
  * **Studio:** pestañas Fuentes, Matriz · Tabla, Grafo, Tools y Centro. Una pestaña con novedad se marca con un punto y solo se activa sola si el usuario no ha elegido otra. Las fichas sustituyen a las tablas comprimidas; el grafo se agrupa por relación; las fuentes se agrupan por apartado, sin repetir el título del documento.
  * **Centros:** botón para quitar un centro del registro, con diálogo de confirmación; el disco no se toca.
* **Tests (+18, −3; 341 pasando + 9 ignorados, el nuevo es el generador del banco):**
  * `tests/render_markdown_gui_tests.rs` (13 casos).
    * En rojo antes de introducir `pulldown-cmark` fallaban 8: cursiva, anidado, cita en bloque, listas ordenadas y anidadas, viñetas con `*`, código con vallas, párrafos y regla, y encabezados h1–h4.
    * También cubre enlaces y HTML crudo, incluidos `javascript:` y `<details ontoggle>`.
    * Incluye además un generador `#[ignore]` del banco visual `docs/validacion/fase14/banco_render.html` (renderizador y CSS reales).
  * Unitarios: `markdown_seguro` (2), agrupación de fuentes (1) y resumen por tool (1, con 8 casos).
  * Adaptadas a `<p>…`: la integración y la adversarial de la GUI. La comprobación de HTML vivo pasa a hacerse por etiquetas y atributos permitidos.
  * Retirados los 3 tests del renderizador antiguo, que cubrían código eliminado.
* **Verificación real** (ventana debug, Claude real, capturas `11`–`18` en `docs/validacion/fase14/`):
  * Arranque en tema claro con estado vacío (`11`).
  * Recepción con Sonnet (`12`, `13`): matriz y búsqueda, salto al inicio de la respuesta, ficha en el Studio, un único chip `PO-ALM-03 Rev. 04 · 6` y notas compactas.
  * Grafo en tema oscuro (`14`): aristas agrupadas por relación.
  * Tabla 4-A con Haiku (`15`): «resuelto · 125 (*) °C» y nota obligatoria.
  * Redimensionado de paneles (`16`; 565 px y 240 px recordados).
  * Plegado y fuentes agrupadas (`17`).
  * Ventana maximizada a 1920×1032 con el panel plegado y la columna centrada (`18`).
  * La comprobación automática no encontró ningún elemento con scroll vertical dentro de los mensajes.
  * **Fallos encontrados y corregidos durante la verificación:**
    * Flechas en las barras de scroll (`scrollbar-width`).
    * Panel plegado que conservaba su ancho (el estilo en línea pisaba la clase).
    * Placeholder partido en dos líneas.
    * Un bloque que empieza por HTML se tragaba los saltos de línea. Lo detectó `adversarial_gui_chat_tests`; ahora se convierte en texto línea a línea con su salto.
* **Incidencia de entorno:** el disco D: llegó al 100 % (fallo `LNK1285`, PDB dañados). Se liberaron 51 GB borrando solo artefactos regenerables del proyecto: `target/debug/incremental` y los binarios y PDB de test del crate. Conviene vigilar el espacio: `target/debug` llegó a 61 GB.
* **Pendiente para la Fase 15:**
  * Probar con el usuario el arrastre real desde el Explorador, el diálogo nativo y el redimensionado con ratón real. El redimensionado se verificó con eventos de puntero sintéticos.
  * Mensaje claro cuando la sesión de Claude Code ha caducado. Hoy se muestra el JSON del CLI; está propuesto, sin hacer.

### Sesión 15 (2026-10-19) — Fase 15: Aceptación final
* **Objetivo:** demostrar con evidencia real que la app controla los 8 problemas (plan v2 §1) y cumple §9 de la especificación; cerrar los pendientes de la Fase 14.
* **Contratos nuevos o cambiados:**
  * `infrastructure/bootstrap.rs`:
    * `ComposicionReal { dependencias, indice }` y `composicion_real(config)`. `componer_lakehouse` delega en esta función.
    * `MotoresConsulta` expone `indice`.
  * `infrastructure/llm/sesion_claude.rs`: `es_fallo_de_sesion(salida)` y `MENSAJE_SESION_CLAUDE`.
    * `ClaudeAgentRunner` lo aplica con código ≠ 0 y con un aviso breve con código 0.
    * `ClaudeHeadlessDistiller` lo aplica con código ≠ 0 y con el sobre `is_error`.
    * El catálogo de modelos no lo necesita: el CLI real lo sirve sin sesión, comprobado con un `CLAUDE_CONFIG_DIR` vacío.
  * `presentation/cli/proceso.rs`: arranque compartido por `main.rs` y el nuevo binario `src/bin/lakehouse_rag_cli.rs`, que es de subsistema consola. Funciones: `ejecutar_comando`, `informar_error_argumentos`, `imprimir_mcp_info` y `servir_mcp`.
  * `Cargo.toml`: `default-run = "lakehouse_rag"`; se retira `petgraph`.
* **Decisiones:**
  * **CLI:** un binario de consola aparte es lo más simple y robusto. PowerShell lo espera y `$LASTEXITCODE` es fiable, como se verificó en release: códigos 0, 1 y 2. `lakehouse_rag.exe` sigue siendo la ventana sin consola.
  * **P4:** los documentos sintéticos se destilan con un paquete fijo (`DestiladorEnrutado` en el test). Extraer 1.000 aristas con Claude no es viable, y lo que se valida es el recorrido acotado. El resto de la tubería es real.
  * **P8:** un decorador del almacén real falla en la tabla 05. Después se comprueba que las vistas DataFusion, el índice semántico y el grafo coinciden con LanceDB.
  * **§9.4:** un decorador del agente real registra el modelo de cada subproceso. Los tres modelos se usan en la misma conversación.
* **TDD:**
  * En rojo antes de implementar se vieron los 3 tests de sesión: salían la salida cruda y el JSON del CLI.
  * El binario de consola y la suite de aceptación fallaban por compilación.
  * La suite de aceptación pasó a la primera.
* **Tests añadidos (+8 que corren, +10 ignorados reales):**
  * Unitarios de `sesion_claude` (3).
  * `adversarial_sesion_claude_tests` (3).
  * `integration_cli_consola` (2).
  * `acceptance_8_problemas` (10, `#[ignore]`).
* **Verificación:** `cargo test` 349 OK + 19 ignorados; `cargo clippy --all-targets -- -D warnings` sin avisos; `cargo fmt --check` OK; `cargo build --release` OK (79 s).
* **Suites reales (`--ignored --test-threads=1`, binario debug, Claude Code 2.1.302, FastEmbed E5-large):**

  | Suite | Resultado | Duración |
  | --- | --- | ---: |
  | `acceptance_8_problemas` (ejecución 1) | 10/10 OK | 605 s |
  | `acceptance_8_problemas` (ejecución 2) | 10/10 OK | 609 s |
  | `acceptance_e2e_etapa1_ingesta` | 3/3 OK | 122 s |
  | `e2e_binario_fase12` | 2/2 OK | 186 s |
  | `e2e_binario_fase13` | 1/1 OK | 272 s |
  | `acceptance_gui_chat_claude_headless` | 1/1 OK | 19 s |
  | `integration_claude_model_catalog` | 1/1 OK | 8 s |

  No se ejecutó el generador `render_markdown_gui_tests::…banco_render` (`#[ignore]`): no es una suite, sino que regenera un HTML de revisión visual.
* **Pendiente (requiere al usuario):**
  * La prueba guiada de la ventana: checklist en `docs/validacion/aceptacion_final.md` §7, con arrastre real, diálogo nativo, redimensionado, centros, ingestas, preguntas del oráculo y cambio de modelo.
  * Cuando el usuario la apruebe: commit `feat(phase-15): …`. El push solo con su aprobación explícita.
* **Notas para la siguiente sesión:**
  * Disco D: al 99 % (17 GB libres) y `target/` > 30 GB. Si hace falta espacio, borra `target/debug/incremental`.
  * Los logs de la aceptación se regeneran en `target/aceptacion/`. Las copias de evidencia están en `docs/validacion/fase15/`.

### Sesión 15b (2026-10-19) — Fase 15: prueba guiada, documentos de demostración y chat en vivo
* **Origen:** comentarios del usuario en la prueba guiada:
  * La barra de título solo se arrastraba por la izquierda.
  * El icono del tema era de colores.
  * Un id de centro repetido (ya existía `obra-prueba`): no era un fallo.
  * Pidió documentos de demostración y un chat con streaming y un diseño de referencia (ChatGPT, analizado con REA).
  * Aprobó (SÍ) el diseño de streaming.
* **Arreglos de la ventana (TDD):**
  * `data-tauri-drag-region` en toda la barra.
  * Iconos de tema monocromos en SVG.
  * Tests nuevos en `adversarial_gui_v2_tests` (2).
* **Sesión caducada real:** el CLI devolvió «Failed to authenticate: OAuth session expired and could not be refreshed». El detector no la reconocía; se añade con su test, que primero falló.
* **Documentos de demostración:** `docs/validacion/fase15/demo/`, con 4 documentos de hormigonado y `GUIA_DEMO.md` (8 preguntas con su respuesta esperada). Validados con la CLI real: 8/8 correctas, y la segunda subida de la Rev. 02 se detecta como «ya ingerido».
* **Referencia de UX:** `docs/validacion/fase15/informe_chat_referencia.md` (REA sobre la app de escritorio de ChatGPT, que es Electron; capturas en `referencia_chat/`).
* **Contratos nuevos (aprobados):**
  * `AgentRunnerPort::ejecutar_turno_en_vivo(…, ObservadorTexto)`, con una implementación por defecto que delega en `ejecutar_turno`.
  * `infrastructure/llm/stream_json.rs`: `interpretar_linea` y `FiltroTextoVisible`. Un turno que empieza por `{` o por una valla de código es una petición de tool y no se muestra.
  * `ClaudeAgentRunner` usa `--output-format stream-json --include-partial-messages --verbose`.
  * El orquestador emite `TextDelta`.
  * `GuiChatService`: `ChatStreamEvento` (`texto`, `herramientaInicio`, `herramientaFin`; camelCase), `enviar_mensaje_en_vivo` y `detener(turno_id)`. Detener suelta el futuro, así que el subproceso muere por `kill_on_drop` y no queda rastro en el historial.
  * Comandos Tauri: `enviar_mensaje_chat` (con `turnoId`, emite `chat-stream`), `detener_respuesta` y `renderizar_markdown`.
* **Frontend:**
  * Columna de 768 px y burbuja del usuario (70 % de ancho, radio 18 px).
  * Asistente sin burbuja, con interlineado de 1,625.
  * Composer con radio de 22 px que crece hasta el 25 % de la altura (25vh). El pie lleva el modelo, la ayuda y el botón circular enviar/detener.
  * «Pensando / Consultando *tool*…» con brillo; tools en vivo (● → ✓).
  * Markdown renderizado en Rust cada 120 ms, con un punto latiendo al final. Se mantiene un único `innerHTML` (`pintarHtmlSeguro`).
  * Scroll pegado al final con un umbral de 24 px, botón «↓» y pregunta anclada arriba mediante un espaciador.
  * Acciones visibles al pasar el ratón: «Copiar» → «✓ Copiado».
* **Tests nuevos:**
  * Unitarios: `stream_json` (4) y orquestador en vivo (1).
  * `integration_streaming_claude` (3, con un `claude` falso que reproduce la salida real de stream-json, con pausas).
  * `integration_chat_en_vivo` (3).
  * `ux_chat_tests` (5).
  * Módulo común `tests/comun_claude_falso/`.
  * Fixtures `tests/fixtures/claude_stream_json/`.
* **Verificación real en la ventana** (CDP, `docs/validacion/fase15/cdp_chat_en_vivo.js`; capturas `19`–`22`):
  * El texto aparece de forma progresiva (de 205 a 2.306 caracteres en unos 3 s) con `dist=0`, es decir, pegado al final.
  * Las tools se ven en vivo.
  * La pregunta queda anclada a 116–124 px del borde superior.
* **Fallos encontrados en la verificación real y corregidos con su test:**
  * Colapsar el espaciador para medirlo recortaba el scroll.
  * La respuesta se insertaba delante del espaciador del turno anterior, así que quedaba encima de la nueva pregunta.
* **Observado una vez y no reproducido:** en un segundo turno inmediato, la vista quedó a 400 px del final hasta terminar la respuesta.
* **Cierre:** el usuario probó la demo y el chat nuevo y aprobó la Fase 15. Con el runner en streaming, la aceptación real da 10/10 (620 s; `docs/validacion/fase15/acceptance_8_problemas_ejecucion3_streaming.log`). La suite queda en 367 OK + 19 ignorados, con clippy y fmt limpios. Commit `feat(phase-15)` sin push, pendiente de la aprobación explícita del usuario.


### Sesión STAIR 1 (2026-10-31) — Fase 1: Esquemas

**Cambios:**
- Contrato: `docs/arquitectura/decisiones_stair.md`. Prevalece sobre la especificación de Gemini.
- Nuevo `domain/models/estructura_documental.rs` y validación `validar_estructura_stair`, que `escribir_staged` aplica antes de escribir.
- `verificar_integridad` añade `nodos_huerfanos` y `valores_invalidos`, visibles en CLI y GUI.
- `confirmar_ingesta` devuelve lo activado a `STAGED` si la activación falla a medias. Antes, un fallo así exponía la revisión parcial hasta el rollback.
- `VERSION_ESQUEMA_CENTRO = 3`; `VERSION_ESQUEMA` y el `schema_version` del glue catalog se derivan de ella.

**Tests:** se añade `adversarial_esquemas_stair_tests` (18). Se adaptan fixtures y recuentos de 6 a 7 tablas. Por regla del usuario no se ejecutó `cargo test` completo ni las suites `#[ignore]`.

**Notas para la Fase 2:**
- Leer `decisiones_stair.md` §4 y §5.
- Enviar 02 y 07 en la misma llamada a `escribir_staged`.
- Hay un `continue` temporal para la tabla 07 en `tests/integration_store_v2_transaccional.rs` (`servicio_revierte_todo_...`): debe retirarse cuando la ingesta escriba 07.

### Sesión STAIR 2 (2026-10-31) — Fase 2: Ingesta universal

**Cambios:** los contratos están en `decisiones_stair.md` §4.
- Ingesta en dos etapas y una transacción: un fallo de Claude deja el documento indexado como provisional, con aviso. Solo revierten los fallos de embeddings o de disco.
- `reintentar_destilacion` sustituye el provisional reutilizando vectores, o lo consolida si no hay código válido.
- Puertos: `confirmar_sustitucion` en el store, `contar_truncados` en embeddings y `es_provisional` en `HuellaIngerida`.
- `verificar_integridad` informa las revisiones con chunks y sin 07.

**Tests:** nuevas `integration_ingesta_universal` (13) y `adversarial_ingesta_universal_tests` (14), más unitarios de fragmentación, árbol, parser y mapeo.
- Cambian de contrato, porque un fallo de destilación ya no es error: `adversarial_store_v2_tests`, `adversarial_ingestion_tests`, `integration_lakehouse_app` y `acceptance_e2e_etapa1_ingesta`. `adversarial_esquemas_stair_tests` cambia porque una revisión sin 07 ya es huérfana.
- `integration_gui_v2` esperaba 6 vistas y son 7: quedó pendiente de la Fase 1.
- Por regla del usuario no se ejecutó `cargo test` completo, ni las suites `#[ignore]`, ni Claude real, ni el modelo real.

**Notas para las siguientes fases:**
- **Fase 3:** ver `decisiones_stair.md` §5.
- **Fase 4:** exponer el reintento en GUI, CLI y MCP, y resolver el alias provisional.
- **Fase 5:** ejecutar el test `#[ignore]` de `contar_truncados` y medir el truncado real de E5, limitado a 512 tokens. Puede obligar a revisar el umbral de 600.

### Sesión STAIR 5 (2026-10-31) — Fase 5: Aceptación

**Resultado:** `docs/validacion/stair/aceptacion_stair.md`. Funciona como RAG general con límites; se recomienda integrar tras decidir sobre los centros existentes. Merge congelado.

**Cambios:**
- `estructura.rs`: el límite del subárbol cuenta toda la sección serializada.
- `app.js`: «Valores inválidos: undefined» y botón de reintento ilegible.

**Tests:**
- Nueva `tests/aceptacion_stair_pdf_real.rs` (`#[ignore]`; ruta del PDF por variable de entorno).
- Tres tests de regresión, uno por cada fallo corregido.
- `e2e_rag_dual_validation`: aserciones reforzadas con el oráculo.
- `e2e_binario_fase12`: de 6 a 7 vistas, porque el test estaba obsoleto.
- Final: 522 pasan, 0 fallan, 29 ignorados. Clippy y fmt limpios.

**Notas para la siguiente sesión:**
- Decidir los fallos abiertos del informe, empezando por la fecha de vigor inventada.
- Falta comprobar a mano el diálogo nativo y el arrastre desde el Explorador.
- Lanzar las suites reales con `FASTEMBED_CACHE_DIR` apuntando a la caché existente; si no, la etapa 1 descarga de nuevo el modelo en `target/`.
- Tras corregir el límite solo se repitieron la matriz E2E y la suite normal.
### Sesión STAIR 3 (2026-10-31) — Fase 3: PDF digital

**Cambios:** los contratos están en `decisiones_stair.md` §5.
- Nuevos `PdfParser`, `pdf_estructura` y `ParserPorFormato`, compuesto en `bootstrap`. GUI y CLI aceptan `.pdf`.
- `parse_bytes`, `ingestar_bytes` y `reintentar_destilacion_bytes`: el hash se calcula sobre los bytes del original.
- 02 añade `pagina_inicio` y `pagina_fin`. Los centros v3 creados antes de esta fase deben recrearse.
- `REQUIERE_OCR` ya se produce: indexa sin destilar y no admite reintento.

**Tests:** nuevas `integration_pdf_parser` (11, más 1 `#[ignore]` con PDF reales por `LAKEHOUSE_PDF_REALES`) y `adversarial_pdf_tests` (21), más unitarios de `pdf_estructura`. Por regla del usuario no se ejecutó `cargo test` completo, ni otras suites `#[ignore]`, ni Claude real, ni el modelo real.

**Notas para las siguientes fases:**
- **Fase 4:** ver `decisiones_stair.md` §6 (páginas en citas y explicación de `REQUIERE_OCR`).
- **Fase 5:** ingerir un PDF real completo con Claude y modelo reales.

### Sesión STAIR 4 (2026-10-31) — Fase 4: Búsqueda y tools

**Cambios:** los contratos están en `decisiones_stair.md` §6.
- Nuevo `lakehouse_tool_service/estructura.rs` con las dos tools STAIR y la resolución de documento, alias y sección.
- `busqueda.rs` evoluciona con modos y filtros por sección. Un provisional nunca prevalece.
- Dominio: `comparar_rutas_linaje`, `comparar_natural` y `reconstruir_sin_solape`.
- `LakehouseApp::reintentar_documento` y `CenterWorkspacePort::localizar_original`; CLI `--reintentar`, comando GUI y botón, `McpServer::con_reintento`.
- `FragmentoIndexado` añade páginas; `IngestaDto` y `DocumentoCatalogoDto` añaden estado; `CitationDto` añade `paginas`.

**Tests:** nuevas `integration_tools_stair` (19) y `adversarial_tools_stair_tests` (7), más unitarios en dominio, `estructura.rs`, `busqueda.rs` y `resultados_tools.rs`. Pasan `--lib` (242, con 1 `#[ignore]` sin ejecutar) y las suites afectadas de tools, agente, GUI, MCP, CLI, app e ingesta universal. Por regla del usuario no se ejecutó `cargo test` completo, ni `#[ignore]`, ni Claude real, ni el modelo real. El esqueleto E2E solo se compiló.

**Notas para la Fase 5:** ver `decisiones_stair.md` §7. El enrutamiento del agente **no está verificado**: los dobles no lo pueden comprobar.

### Sesión STAIR 6 (2026-10-31) — Fase 6: Calidad de datos

**Cambios:** los contratos están en `decisiones_stair.md` §8. Esquema v4, sin migración.
- `fecha_vigor` nula: `validar_fecha_vigor`, `VentanaVigencia::estado_en`, `vigencia_no_declarada` en las tools; la confirmación compara con todas las fechas confirmadas.
- Parser de PDF: títulos dudosos (puntuación final o más de 16 palabras) solo en secuencia exacta.
- `origen_responsable` en 03 con `OrigenResponsable::verificar`; calidad `REVISAR` si es inferido o ausente; expuesto en matriz, SQL, GUI y system prompt.
- Privacidad: preguntas de aceptación por `LAKEHOUSE_PREGUNTAS_ACEPTACION`; sin referencias a documentos reales en lo versionado.

**TDD:** vistos en rojo: 8 unitarios de dominio, 17 de `adversarial_fase6_stair_tests` y los de numeración. Excepciones: `test_la_gui_muestra_no_declarada_y_el_origen` se añadió junto a la implementación, y `test_provisional_no_toma_la_fecha_de_la_ingesta` y los dos del esquema JSON, con ella; no se vieron en rojo por comportamiento. 13 tests adversariales pasaron desde el principio porque son guardas (falsos positivos).

**Tests:**
- Nueva `adversarial_fase6_stair_tests`.
- Tests heredados adaptados a los tipos nuevos; `interpretar_fecha_vigor` se eliminó con su aserción, que exigía usar la fecha de hoy, contraria al contrato. Algunos fixtures ahora nombran el rol o usan `INFERIDO`; no se rebajó ninguna aserción.
- Final: 566 pasan, 0 fallan, 31 ignorados. Clippy y fmt limpios.
- Reales: e2e 4/4 y PDF real 4/4. `acceptance_8_problemas` **9/10**: `p6_agregacion` falló 1 de 4 veces.

**Notas para la siguiente sesión:**
- Resultados en el informe local `docs/validacion/stair/aceptacion_stair.md`.
- Abierto: fallo intermitente de P6 (un hito sin rol ni registro), con propuesta sin aplicar.
- Abierto: numeración que se reinicia en documentos largos, y umbral 600→512 (recomendado, no aplicado).
- Lanzar las suites reales con `FASTEMBED_CACHE_DIR` apuntando a la caché existente.

### Sesión STAIR 7 (2026-11-01) — Fase 7: Hitos, roles e índice largo

**Cambios:** contratos en `decisiones_stair.md` §9. Esquema v5, sin migración.
- `HitoOperativo` exige rol o registro; los descartados se cuentan en `hitos_derivados_capa0` y su texto sigue en la Capa 0. Barrera también al escribir.
- `query_operational_matrix`: `solo_declarados`, `excluidos_no_declarados`, `sin_responsable_declarado`, `accion_sugerida`; reglas nuevas en el system prompt.
- PDF: tramos por capítulo o reinicio, sub-nodos `SINTETICO` por páginas y páginas reales por chunk. `tipo_nodo` en 07, ToC, subárbol, integridad y GUI; las citas apuntan a la sección real.

**Tests:** nueva `adversarial_fase7_stair_tests`. Final: 600 pasan, 0 fallan, 31 ignorados. Clippy y fmt limpios. Reales sobre el código final: e2e 4/4, PDF real, documento largo y `acceptance_8_problemas` 10/10 en 5 pasadas seguidas. Excepciones de TDD y tests heredados adaptados: informe local.

**Notas para la siguiente sesión:**
- Resultados, checklist manual y borrador de changelog: `docs/validacion/stair/aceptacion_stair.md`, sección Fase 7.
- Abierto: umbral 600→512 (datos aportados, sin aplicar); `pattern` no válido en un campo del alta de centro (propuesta sin aplicar); pasos manuales de la GUI.
- Límite aceptado: la matriz no es fuente fiable de roles en documentos en voz pasiva.

### Sesión STAIR 8 (2026-11-02) — Ajustes tras la validación manual

**Cambios:**
- Regla 12 del system prompt (auditoría): un cruce tabular exige `lookup_double_entry_table` en el turno actual aunque la tabla ya esté en el historial.
- PDF: una línea `ANEXO A`, `ANEXO 3` o `ANEXO IV`, con título opcional, abre un nodo raíz `REAL`; sus apartados cuelgan de él y el cuerpo conserva sus niveles. Una mención en mitad del cuerpo o la última entrada de un índice no cortan.

**Tests:** 608 pasan, 0 fallan, 32 ignorados. Clippy y fmt limpios. Nuevos tests de parser y orquestador, `test_adv_cruce_tabular_con_tabla_en_historial...` con runner simulado y `p2_cruce_con_la_tabla_ya_en_el_historial_invoca_lookup` real e ignorado.

**Notas para la siguiente sesión:**
- El test real de la regla 12 no se ejecutó: el cumplimiento por Claude real queda sin verificar.
- `Anexo A` en minúsculas no abre nodo, a propósito: suele ser una remisión.
- Abierto: umbral 600→512; `pattern` no válido en un campo del alta de centro; pasos manuales de la GUI.

### Sesión Arnés 1 (2026-11-02) — Fase 1: `IntentDetector` y `PostInferenceGuard`

**Cambios:**
- `ssd_arnes_determinista.md` ajustado al spike: `--json-schema` es una directiva, `Obligatoria` pasa a `Dirigida`, la señal de llamada es `structured_output` y queda como riesgo de la Fase 2 el intercambio de ejes.
- Nuevo `use_cases/arnes_turno`, puro y sin dependencias: `IntentDetector`, `PreInferenceInterceptor::plan` y `PostInferenceGuard` con `RegistroTurno`. Sin cablear: no se tocó puerto, runner, orquestador, GUI ni eventos.
- Decisión añadida al SSD: `Responsable` también exige el intento (matriz o SQL), para que el guard cubra el turno dirigido sin llamada.

**Tests:** 52 unitarios (`cargo test --lib arnes_turno`), vistos en rojo con implementaciones vacías (38 fallaban; 14 pasaban por comprobar casos que la implementación vacía ya cumplía), y la nueva `adversarial_arnes_tests` (16), escrita junto a la implementación; encontró un fallo real (la unidad de un valor se tomaba como etiqueta del siguiente). No se ejecutó la suite completa: no estaba autorizada.

**Notas para la siguiente sesión (Fase 2):**
- Pendientes de §5, por depender del orquestador o del adaptador: `test_directiva_solo_afecta_al_primer_paso`, `test_turno_dirigido_sin_structured_output_no_cuenta_como_llamada`, reintento, historial y streaming.
- Comprobar qué hace la tool con ejes intercambiados o vacíos.
- Medir en la Fase 4 los falsos bloqueos: `TabularIncompleta` que pide el eje que falta y recuentos redactados por el modelo.

### Sesión Arnés 2 (2026-11-02) — Fase 2: puerto, runner, orquestador y evento

**Cambios:**
- `AgentRunnerPort::ejecutar_turno_dirigido`, que devuelve `TurnoDirigido::{Llamada, SinLlamada}`. Respecto al §3.2 original añade el parámetro `herramientas`: el spike midió con las 8 tools a la vista. SSD actualizado.
- `ClaudeAgentRunner`: `--output-format json --json-schema`. Solo un `structured_output` conforme cuenta como llamada; `result` nunca se usa. `is_error` y una salida que no sea JSON son errores.
- `AgentOrchestrator` conectado al arnés: clasifica, dirige solo el primer paso, registra las tools del turno, valida, hace un reintento y luego da el mensaje fijo. El recordatorio solo existe en la vista del modelo. Los turnos retenidos no pasan observador y emiten un único `TextDelta` ya validado; los turnos sin intención conservan el streaming.
- `AgentExecutionEvent::ArnesIntervino { etapa, motivo }`, expuesto en el campo `arnes` del `--json`. La GUI no necesitó cambios: su `match` ya tiene `_`.
- Ejes intercambiados: `lookup_double_entry_table` **no los detecta**. Con rangos solapados devuelve otra celda sin aviso, y el guard la acepta. Evidencia y propuesta en el SSD §3.2; no implementado en esta sesión (ver Sesión Arnés 2b).

**Tests:** 711 pasando, 0 fallos, 32 `#[ignore]` no ejecutados. `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: OK. Los tests del orquestador y del adaptador se vieron en rojo. Los adversariales nuevos (8) se escribieron después de la implementación y pasaron a la primera, así que no hay rojo que los respalde.

**Test heredado adaptado:** `test_chat_de_la_ventana_rellena_citas_matriz_y_grafo_desde_las_tools_reales` preguntaba «¿Quién custodia?» con una respuesta guionizada que nombra un rol, y la matriz devolvía `sin_responsable_declarado`: el guard la rechaza con razón. Ahora pregunta «Resume la custodia», sin cambiar ninguna aserción, y el caso original pasa a `test_chat_de_la_ventana_no_muestra_un_responsable_sin_respaldo`. `acceptance_8_problemas` (ignorado) solo gana el reenvío del turno dirigido en su `AgenteRegistrador`.

**Notas para la siguiente sesión:**
- Nada de esto se ha probado con Claude real: es la Fase 4. Allí conviene medir los ejes intercambiados y qué ocurre si la CLI agota sus reintentos de esquema, porque hoy un `is_error` termina el turno con error en vez de pasar por el guard.
- Fase 3: mostrar `ArnesIntervino` en el Studio; `evento_en_vivo` de la GUI aún lo ignora. El efecto de escritura se pierde en los turnos retenidos.
- Decisión pendiente del usuario: la propuesta de `ejes_ambiguos` (aprobada e implementada en la Sesión Arnés 2b).

### Sesión Arnés 2b (2026-11-02) — Ejes intercambiados en `lookup_double_entry_table`

**Cambios:**
- `lookup_double_entry_table` repite el cruce con los ejes invertidos, con las mismas celdas ya filtradas por fecha y el mismo `eje_z`. Si ambos órdenes dan celdas distintas añade `ejes_ambiguos`, sin cambiar `estado` ni valor. Si solo resuelve el invertido, conserva `sin_coincidencia` y añade `orden_invertido_resolveria`, sin la celda.
- Regla 13 en `build_system_prompt`. También se avisa en la descripción de la tool y en `MCP_GUIDE.md`, porque un cliente MCP no recibe el prompt.
- `PostInferenceGuard`: nueva `ViolacionArnes::EjesAmbiguosSinAclarar`. Rechaza el valor de un solo orden si la respuesta no nombra ambos ejes por su nombre o unidad. Usa solo datos del propio aviso, sin vocabulario añadido.
- SSD §3.2 y §3.3 actualizados: riesgo resuelto en la salida de la tool, con lo que queda sin garantizar.

**Hallazgo:** el aviso también sale en cruces correctos cuando la tabla tiene extremos abiertos. En la tabla 4-A del fixture, 30 mm y CE 0,42 % avisan, porque 0,42 cae en `x <= 20` y 30 en `y > 0,45`. El SSD suponía que el riesgo solo existía con rangos solapados.

**Tests:** 744 pasando, 0 fallos, 32 `#[ignore]` no ejecutados. `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: OK. Los unitarios de `lookup.rs` y del guard se vieron en rojo con stubs (13 fallaban). Los `test_ejes_intercambiados_*` documentaban el fallo y ahora exigen el aviso: `test_ejes_intercambiados_con_rangos_disjuntos_indican_que_el_invertido_resolveria` y `test_ejes_intercambiados_con_rangos_solapados_avisan_de_ejes_ambiguos`. Los 2 de integración y los 12 adversariales nuevos se escribieron después de la implementación, así que no hay rojo que los respalde. No se ejecutó ningún `#[ignore]` ni Claude real.

**Notas para la siguiente sesión:**
- Fase 4: medir la frecuencia del aviso en tablas reales, los reintentos que causa `EjesAmbiguosSinAclarar` y si el modelo decide bien por unidades.
- Límites conocidos en el SSD §3.2: el guard no prueba que la asignación sea correcta, y los turnos sin intención y `execute_athena_sql` quedan fuera.

### Sesión Arnés 3 (2026-11-03) — Fase 3: streaming y Studio

**Cambios:**
- `ChatStreamEvento` gana `TurnoRetenido` y `Arnes { etapa, mensaje }`. La retención se decide con las mismas funciones puras que usa el orquestador, sin tocar el puerto ni el evento de la aplicación.
- Nuevo `presentation/gui/intervenciones_arnes.rs`: traduce etapa y `motivo` a lenguaje del operador. El `motivo` nunca se muestra; uno desconocido da una causa genérica. `ChatResponsePayload` añade `arnes` y `salida_segura`.
- Ventana: estado «Validando…» y «preparando otra…», pestaña Validación en el Studio, mensaje fijo resaltado con botón al Studio, y avisos `ejes_ambiguos` y `orden_invertido_resolveria` en la tarjeta y en el resumen del lookup. Con salida segura se limpian los resultados de turnos anteriores, para que el mensaje no remita a datos ajenos.

**Tests:** 759 pasando, 0 fallos, 32 `#[ignore]` no ejecutados. `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` y `node --check`: OK. Nueva `tests/integration_chat_arnes.rs` (9) y 6 unitarios. Vistos en rojo con stubs, salvo tres que ya pasaban porque describen lo hecho en la Fase 2: serialización, historial con reintento e historial con salida segura. Los casos adversariales de la fase (motivo con HTML u órdenes, Detener en paso dirigido y en reintento) están en esos tests; no hay fichero `adversarial_*` propio. El JavaScript no tiene tests automáticos: se comprobó en la ventana.

**Comprobado en la ventana real** (release del worktree, CDP, config y centro temporales, Markdown sintético, Haiku):
- Turno sin intención: el texto crece en 8 pasos, sin estado de validación.
- Turno tabular: «Validando…», luego «preparando otra…»; tarjeta con el chip y el aviso de ejes ambiguos.
- Salida segura real, provocada por Haiku: mensaje fijo resaltado, Studio en Matriz · Tabla y pestaña Validación legible.
- Detener en paso dirigido y en reintento: el `claude.exe` hijo (PID 482916 y 39848) desaparece, y tras el primero el turno siguiente responde con normalidad.
- Ventana cerrada (PID 193028): sin procesos del worktree ni puerto de depuración.

**No comprobado con un turno real:**
- `orden_invertido_resolveria`: el fixture no lo provoca. Se llamó a `pintarTabla` en la ventana con datos sintéticos, incluido HTML, que queda escapado. El botón «Ver los resultados en el Studio» no se pulsó.
- Un turno tabular retenido que acabe en respuesta válida: en las dos preguntas tabulares completadas o reintentadas Haiku fue rechazado; la entrega válida solo está cubierta por tests.

**Notas para la siguiente sesión (Fase 4):**
- Posible falso bloqueo a medir: «Tabla 4-A, 30 mm y CE 0,42 %» con Haiku devolvió el lookup correcto (125 °C) y acabó en salida segura por `CifraSinAnclar`, con dos llamadas iguales. La GUI no guarda la cifra rechazada, así que no se sabe cuál fue. En otro intento apareció una línea de lookup con error antes del reintento.
- El aviso de ejes ambiguos aparece en este cruce correcto (ver Sesión Arnés 2b) y puede sumar rechazos.
- `gen/schemas/*.json` se regeneran al compilar y no forman parte de esta fase.

### Sesión Arnés 3b (2026-11-03) — Guard de abstención y revisión del anclaje

**Cambios:**
- Regla 14 del prompt: ante lo que no consta, decirlo sin ofrecer valores de referencia, salvo que se pidan. Sí se da lo preguntado que consta.
- Nueva violación `CifraEnAbstencion`: abstención con una cifra que no está en la pregunta ni en una salida válida del turno, salvo que el operador pida referencias. Usa el mismo reintento único y el mensaje fijo.
- Nueva intención `DatoConcreto` (valor o código): retiene el texto, no dirige ninguna tool y solo valida la abstención.
- Anclaje: las fuentes se leen con `lecturas_de_fuente`, más amplia que la lectura de la respuesta. También se admiten marcadores de lista con Markdown, guiones tipográficos en identificadores y miles con espacio duro. `125ºC` en una respuesta ya es una cifra, y `999.` al inicio de línea ya no pasa por marcador.
- Telemetría: `tracing::warn!` en cada intervención, con el `motivo` que lleva el literal rechazado. La ventana sigue sin recibirlo.
- GUI, en commit aparte: `pattern` del alta de centro válido con el indicador `v`.

**Tests:** 811 pasando, 0 fallos, 32 `#[ignore]` no ejecutados. `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: OK. Rojo verificado contra stubs: 24 unitarios y 9 adversariales, más el del `pattern`. Los demás tests nuevos ya pasaban porque fijan comportamiento que no debía cambiar o límites declarados. La suite adversarial se amplió en `tests/adversarial_arnes_tests.rs`.

**Hallazgo de `CifraSinAnclar` (Fase 3): no reproducido.** No se guardó la cifra, así que no se sabe cuál fue. `125 °C`, `125°C`, `0,42 %`, `0.42%`, rangos y `**125 °C**` ya se anclaban bien con una salida de la forma real del lookup. Sí se hallaron y corrigieron rechazos falsos de otros formatos: `**1.**`, `### 1.`, `Tabla 4‑A`, `125ºC` o `Rev02` en la fuente, y `apartado 5.2` con `5.2.1` en la fuente. Cualquiera pudo ser la causa, o ninguna. Otra hipótesis sigue abierta y no es un fallo de formato: la `cita` del lookup no trae página ni fecha, y citarlas desde el historial se sigue rechazando.

**No comprobado con Claude real:** todo lo de esta sesión. Ni la regla 14, ni la tasa de falsos bloqueos, ni el efecto en la línea base.

**Notas para la siguiente sesión (Fase 4):**
- El guard **no frena** la forma dominante de la línea base (`No consta; la línea L-204 tiene 14,5 bar`) si esa cifra salió de una tool del turno: contra eso solo actúa la regla 14. Ver SSD §3.3. Hay que medir `rechazo_estricto` tras este cambio.
- Para leer la cifra rechazada: `--json` (campo `arnes`) o `RUST_LOG=warn` con stderr redirigido en la ventana.
- `DatoConcreto` no ancla una respuesta sin abstención: `Son 999 bar` pasa.
- Durante esta sesión se llenó el disco D:. Con aprobación del usuario se borraron los `target` de `arnes-ejes-ambiguos` y `arnes-fase-2`, que hay que recompilar si se vuelven a usar.

### Sesión Arnés 4 (2026-11-03) — Fase 4: suites reales y benchmark con Claude

**Estado de la Fase 4: medida, no completada.** Informe: `docs/arquitectura/ssd_arnes_fase4_suites_reales.md`.

**Cambios:**
- `tests/acceptance_8_problemas.rs`: el modelo de los 8 problemas se elige con `LAKEHOUSE_MODELO_ACEPTACION` (por defecto `sonnet`; §9.4 conserva los suyos). Cada intervención del arnés se imprime con su `motivo`, y `LAKEHOUSE_ACEPTACION_VOLCAR` vuelca la salida bruta del modelo, también la rechazada.
- No se tocó código de producción ni `benchmarks/`.

**Medido con Claude real:**
- `acceptance_8_problemas`: 10/11 con Haiku y 9/11 con Sonnet. `p2_cruce_con_la_tabla_ya_en_el_historial_invoca_lookup` pasa con los dos (125 °C, con lookup).
- P2 cruce exacto falla con los dos modelos por el oráculo del test: prohíbe «100 °C», y la respuesta correcta lo cita como orden de ejes descartado (regla 13).
- P3 con Sonnet: **falso bloqueo** por `CifraEnAbstencion` con «11». La causa está en el texto rechazado: `vigencia_no_declarada` activa la locución «no declarada», y «Hoy (2026-11-03)» aporta el «11».
- Benchmark (binario release sha256 `f26f5972…`, Sonnet, 3 repeticiones, semilla 42):
  - Ciclo de vida: 0,852 → 0,630 estricta. `historico_por_fecha` pasa de 1,000 a 0,333 por 6 salidas seguras: el detector toma la fecha de la pregunta por valor de eje (`TabularIncompleta`) y exige Capa 2.
  - RGB industrial: 0,625 → 0,625 estricta y 0,688 → 0,688 revisada. `rechazo_estricto` sigue en 0,000 y `rechazo_semantico` en 1,000 (revisión manual).
  - Hubo 3 reintentos más por «11», recuperados por el reintento, y 0 fallos de ejecución.
- Caso «Tabla 4-A, 30 mm y CE 0,42 %» de la Fase 3: no reproducido en 5 pasadas.

**Notas para la siguiente sesión:**
- Hay tres propuestas para el arnés pendientes de aprobación: fechas y revisiones que no cuenten como ejes, locuciones fuera de los identificadores y fechas ISO ancladas enteras con la fecha de hoy. Están en el §5 del informe. Después hay que repetir P3 con Sonnet y el benchmark.
- El oráculo de P2 y el puntuador del benchmark penalizan el valor descartado que la regla 13 obliga a explicar. Son decisiones del usuario y de la sesión del benchmark.
- La compilación se hizo con `CARGO_TARGET_DIR` en los `target` de `arnes-guard-rechazo` (tests) y `arnes-fase-3` (release), sin borrar nada. Este worktree no tiene `target`.

### Sesión Arnés 4b (2026-11-03) — Corrección de los falsos bloqueos de la Fase 4

**Estado: corregida y medida con Claude real. Pendiente de aprobación del usuario.** Informe: `docs/arquitectura/ssd_arnes_fase4_suites_reales.md` §7.

**Cambios:**
- [x] Detector: una fecha (textual, ISO, `d/m/a`, `d-m-a`) o el número de una revisión (`Rev. 00`, `Rev.01`) no cuentan como valor de eje. Nuevo `arnes_turno/fechas.rs`.
- [x] Abstención: un identificador con `_` es una sola palabra (`vigencia_no_declarada` no es «no declarada»); `NO_CONSTA` sigue contando como «no consta» (regla 9).
- [x] Anclaje: una fecha ISO o textual de la respuesta es una sola cifra. La fecha de hoy entra por el nuevo puerto `RelojPort` (adaptador `infrastructure/reloj_sistema.rs`, campo `reloj` de `DependenciasLakehouse`, `AgentOrchestrator::with_reloj`) y ancla solo la fecha entera.
- [x] Oráculo de P2 en `acceptance_8_problemas.rs` (`oraculo_p2`): «100 °C» solo se admite como lectura con los ejes invertidos o descartada. Es un cambio de criterio aprobado por el usuario, documentado en el informe §7.1.
- No se tocó `benchmarks/`.

**Tests:** 842 pasando, 0 fallos, 32 `#[ignore]` no ejecutados. `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: OK. Rojo verificado: 14 unitarios del arnés y 3 del oráculo. Se añadieron 6 casos a `tests/adversarial_arnes_tests.rs`.

**Medido con Claude real:**
- `acceptance_8_problemas`: 11/11 con Haiku (0 reintentos) y 11/11 con Sonnet (1 reintento por «50», un dato del historial que el SSD rechaza a propósito). P3 con Sonnet pasa sin intervención, con «Hoy (2026-11-03)» y `vigencia_no_declarada` en la respuesta.
- Benchmark ciclo de vida (binario sha256 `5e90bc8e…`, Sonnet, 3 repeticiones, semilla 42): 0,852 estricta y 1,000 revisada, igual que la línea base (Fase 4: 0,630 y 0,778). `historico_por_fecha` vuelve a 1,000. Hubo 0 directivas, 0 reintentos y 0 salidas seguras.
- No queda ningún falso bloqueo medido.

**Notas para la siguiente sesión:**
- Pendiente fuera de alcance: el puntuador del benchmark cuenta como fallo citar un valor atribuido a otra revisión o a un orden descartado. Deja `historico_por_revision` en 0,556 estricta.
- No se repitió la pista RGB industrial.
- Compilación con `CARGO_TARGET_DIR` en los `target` de `arnes-guard-rechazo` (tests) y `arnes-fase-3` (release), sin borrar nada.

### Sesión Umbral 512 (2026-11-03) — Umbral de chunk único de 600 a 512

**Estado: [COMPLETADO]. Aprobada por el usuario (2026-11-03), sin reserva de contexto en la ventana.** Contrato: `docs/arquitectura/decisiones_stair.md` §10.

**Cambios:**
- [x] `UMBRAL_CHUNK_UNICO` vale `TOKENS_VENTANA` (512). Ventana y solapamiento sin cambios. Sin cambio de esquema.
- [x] Presupuesto de las ventanas revisado con números: no se reserva espacio para el contexto (§10).
- [x] Nuevo `tests/medicion_truncado_sintetico.rs` (`#[ignore]`, FastEmbed real, sin Claude) con un documento sintético.

**Tests:** 849 pasando, 0 fallos, 34 `#[ignore]` no ejecutados salvo los de FastEmbed citados. `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: OK. Casos: frontera 511, 512 y 513, ninguna ventana por encima de 512, solape que cubre la cola sin pérdidas justo sobre el umbral. Rojo verificado: 3 unitarios y 4 adversariales. Nueva suite `tests/adversarial_umbral_512_tests.rs` con 5 casos: sección de 200.000 tokens, breadcrumb de 6 títulos largos, texto sin espacios e ideogramas en la frontera.

**Medido con FastEmbed real (documento sintético):** con 600 salían 34 chunks y 5 truncados; con 512, 43 chunks y 0 truncados. En el límite real de 512 caben 699 tokens estimados en prosa, 656 con códigos, 812 en ideogramas y 380 sin espacios.

**Notas para la siguiente sesión:**
- Los centros existentes no se recrean. Lo ya ingerido conserva los cortes de 600 hasta que se reingiera en un centro nuevo; un provisional que se reintenta ya fragmenta con 512.
- Riesgo restante, visible en el aviso de truncado: texto sin espacios, porque la estimación subcuenta, y breadcrumbs de más de unos 144 tokens estimados.
- Compilación con `CARGO_TARGET_DIR` en el `target` de `arnes-guard-rechazo` y `FASTEMBED_CACHE_DIR` en `target/fastembed_cache` del checkout principal, sin borrar nada.

### Sesión Citas de página (2026-11-09 y 2026-11-10) — Fases 1 y 2: saltos de página, marcas y verificación

**Estado: implementado y medido con Claude real. Pendiente de aprobación del usuario; sin push ni merge.** Contrato: `docs/arquitectura/ssd_citas_pagina.md`. Origen: §4 de `docs/arquitectura/investigacion_citas_pagina.md`. Informe: `benchmarks/informes/aguja_citas_pagina_20261110.md`.

**Aviso para el usuario: el esquema pasa a v6 y no hay migración. Los centros v5 se rechazan al abrirlos: hay que crear centros nuevos y reingerir los documentos. Los datos de los centros anteriores no se tocan.**

**Fase 1 (guardar y mostrar):**
- [x] `02_chunks_normativa.saltos_pagina`: `List<Struct<desplazamiento, pagina>>` nulable, en bytes desde el inicio de `texto_contenido`. `VERSION_ESQUEMA_CENTRO = 6`. `SaltoPagina` pasa al dominio (`domain/services/marcas_pagina.rs`).
- [x] Marcas `⟦p. N⟧` en `texto` de `search_normative_context` y `stair_get_subtree_context`, solo en la respuesta. `⟦ ⟧` del documento se neutralizan a `[ ]`. En el subárbol no cuentan para `desde_caracter`.
- [x] `cita` añade `titulo` y, solo si existe, `fecha_vigor`. La `nota` de las dos tools explica las marcas y pide un extracto literal junto a la página.
- [x] Un PDF sin estructura («Contenido General») guarda ya sus saltos: antes todos sus chunks citaban el rango del documento entero.
- [x] Arnés: la página de una marca queda anclada; el respaldo por cita literal ignora las marcas y los saltos de línea.

**Fase 2 (comprobar):**
- [x] `arnes_turno/verificador_paginas.rs`: extracto entre comillas con una página en la misma línea → esqueleto normalizado → búsqueda en los textos marcados del turno. Resultados: confirmada, discrepante (se corrige), ambigua (se dice), sin página y no encontrada. No rechaza: añade el bloque «Verificación automática de páginas» y la etapa `VerificacionPagina`. Un título o breadcrumb entre comillas no se revisa.

**Benchmark:** `run_aguja.py --dos-agujas`, `valor_y_pagina`, `pagina_exacta_estricta_modelo`, `mismo_chunk` y coste de las marcas. El evaluador lee «Página del dato: N».

**Tests:** 913 pasando, 0 fallos, 34 `#[ignore]`. `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: OK. 215 tests de Python en `benchmarks/`. Rojo verificado: 30 unitarios en una pasada (dominio, ingesta, Arrow, tools, guard, verificador y orquestador), 1 del verificador (metadatos) y los de Python. Nuevas suites: `tests/adversarial_citas_pagina_tests.rs` (20 casos: espacios y guiones partidos, comillas tipográficas y ligaduras, extracto repetido en dos páginas, marcas falsas, chunks sin saltos o con saltos corruptos, Markdown, valores frontera y entradas desmesuradas) y `tests/integration_citas_pagina.rs` (4 casos sobre LanceDB real).

**Medido con Claude real (Sonnet, binario release sha256 `4dc641b5…`, commit `dd65d2e`):**
- Campaña Aguja 5 × 3 (77,7 min, 30 consultas, 0 fallos): `pagina_exacta_estricta` pasa de 0,00 a 1,00, también sobre el texto del modelo sin el bloque de verificación. `recall_aguja`, `pagina_exacta`, `seccion_real` y la recuperación por MCP siguen en 1,00. Arnés: 0 reintentos, 0 salidas seguras, 0 notas de verificación.
- Dos agujas en páginas consecutivas del mismo chunk (3 repeticiones, 6 consultas): `valor_y_pagina` 1,00 y `mismo_chunk` 1,00. 4 notas de verificación, todas «ambigua» y correctas: la frase citada está en las dos páginas.
- Las marcas ocupan el 0,57 % de la salida de `search_normative_context`.
- `acceptance_8_problemas` con Sonnet: 10/11. Falla `p2_tabla_cruce_exacto_con_nota_y_fronteras` en dos pasadas por su oráculo: la respuesta da 125 °C y descarta el orden invertido, pero menciona «100 °C» en una frase sin «invert…» ni «descart…». No usa páginas ni marcas.

**No medido:**
- La suite base no se pudo contar entera antes de empezar: se editó `src/` mientras compilaba y los doc-tests fallaron por esa edición. Los 31 binarios cuyo resultado quedó en el registro pasaron sin fallos.
- La ingesta media de la campaña sube de 183 s a 250 s (mínimo igual, 6 de 15 por encima de 300 s). La causa no se midió.
- El caso «discrepante» de la fase 2 solo está cubierto por tests: el modelo no citó ninguna página equivocada.

**Notas para la siguiente sesión:**
- Oráculo de P2: frágil ante redacciones correctas. Decisión del usuario.
- Fase 2: la página y el extracto en líneas distintas («Página del dato» y «Extracto literal») no se asocian; ocurre en 12 de 36 respuestas medidas (estimación fuera de la app).
- Pendiente de la investigación: página impresa (`/PageLabels`). La app cita la física.
- Compilación con `CARGO_TARGET_DIR` en el `target` de `benchmark-rgb-arnes-d142b4` (ahora también con el perfil debug), sin borrar nada. La aceptación descargó el modelo E5 (2,2 GB) a `target/fastembed_cache` de este worktree. `gen/schemas/*.json` quedan fuera de los commits.

### Sesión SSD de generalidad (2026-11-11): 4 piezas, solo diseño

**Estado: borrador para aprobación del usuario. Sin código de producción, sin push ni merge.** Documento: `docs/arquitectura/ssd_generalidad_4_piezas.md`. Origen: la decisión del mentor externo sobre `docs/estado/situacion_rag_20261111.md` y `auditoria_generalidad_20261111.md`. La rama `claude/arreglos-rag-512d66` no se integra; de ella solo se rescata el silencio de `verificador_paginas.rs`.

- [x] **Pieza 1: PDF.** Se recomienda la opción (b): ToUnicode propio inyectado en memoria, frente a pdfium. Compuerta de legibilidad sin léxico y estado nuevo `LECTURA_DUDOSA` por fila.
  - **Hallazgo verificado en `pdf-extract` 0.12.1** (`lib.rs:406-434` y `:496-505`): en las fuentes CFF, la codificación interna gana a `/Differences`.
  - La AGL solo se aplica por búsqueda exacta.
- [x] **Pieza 2: página de la Capa 2.** Anclaje textual por coincidencia de 2 o más tokens con al menos uno distintivo; `None` si es ambiguo. Se eliminan los literales `"Tabla {id}"` (`mapeo_destilacion.rs:1252` y `:1077`). **Esquema v7: hay que recrear los centros.**
- [x] **Pieza 3:**
  - 3a: cuota con relleno en `fusionar`;
  - 3b: detector estructural de *tool calls* embebidas y evento `TextoEnVivoDescartado` (el streaming filtra solo por el primer carácter, `stream_json.rs:80`);
  - 3c: rescate y excepciones de metadatos y de posición de cabecera antes de `Discrepante`.
- [x] **Pieza 4:** cascada de invariantes estructurales y microllamada a Haiku. El LLM propone una justificación y Rust la comprueba. Falla cerrada. Tabla de listas que se eliminan y que quedan.
- [x] **Protocolo de validación rápida:** niveles N0 a N6, criterios de no retroceso por pieza y subconjunto propuesto para la sesión evaluadora.

**Tests:** no se han ejecutado porque no hay cambios de código; solo documentación. **No se ha leído `docs/validacion/set_oculto*`.**

**Notas para la siguiente sesión:**
- Las decisiones D1 a D7 del §0 del SSD son del usuario.
- Orden recomendado: 3c, 3a y 1 en paralelo; después la 2; después la 3b; y por último la 4.
- La latencia de Haiku sale del spike de la Fase 0 (6-9 s): no está medida con el prompt del verificador.
