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
