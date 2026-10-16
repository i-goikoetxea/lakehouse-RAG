# PROGRESS.md — Tablero Maestro de Control y Progreso

> **Fuente Única de Verdad:** Este documento refleja el estado en tiempo real del desarrollo de la aplicación **Lakehouse RAG Industrial en Rust**. Cada sesión actualiza su progreso antes de finalizar.

---

## 1. Matriz General de Fases

| Fase | Título de la Fase | Sesión | Estado | Tests Pasando |
| :---: | :--- | :---: | :---: | :---: |
| **0** | Cimientos, Gobernanza y Estructura Cargo | Sesión 0 | `[COMPLETADO]` | 1 / 1 |
| **1** | Dominio Puro y Gestión Multi-Centro | Sesión 1 | `[NO INICIADO]` | - |
| **2** | Adaptador LanceDB y los 6 Esquemas Arrow | Sesión 2 | `[NO INICIADO]` | - |
| **3** | Motor Analítico DataFusion y Grafo Petgraph | Sesión 3 | `[NO INICIADO]` | - |
| **4** | Embeddings Locales y Búsqueda Híbrida | Sesión 4 | `[NO INICIADO]` | - |
| **5** | Pipeline de Ingesta y Destilación Claude Headless | Sesión 5 | `[NO INICIADO]` | - |
| **6** | Servidor de Herramientas Data Lake y Agente | Sesión 6 | `[NO INICIADO]` | - |
| **7** | Interfaz de Usuario TUI (Ratatui) y Chat | Sesión 7 | `[NO INICIADO]` | - |
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
- [ ] Entidades de dominio: `CentroConocimiento`, `CenterManifest`, `DocumentoCatalogo`.
- [ ] Value Objects tipados: `DocId`, `NivelPrelacion` (1 a 5), `RangoNumerico` (evaluación `>=, <=, >, <`), `FaseOperativa`.
- [ ] Modelado de `TuplaOperativa`, `CeldaDobleEntrada` y `AristaGrafo`.
- [ ] Errores de dominio tipados (`DomainError`).
- [ ] Puertos de repositorio para registro de centros (`CenterRegistryPort`).
- [ ] Tests unitarios TDD de dominio al 100% (sin I/O ni dependencias pesadas).

### Fase 2: Adaptador LanceDB y los 6 Esquemas Arrow
- [ ] Definición estricta de esquemas Apache Arrow para las 6 tablas (`01` a `06`).
- [ ] Implementación de repositorios sobre LanceDB (`.lance`).
- [ ] Mecanismo transaccional de control de versiones (`vigente = false` para versiones previas al insertar nueva revisión).
- [ ] Tests de integración TDD en directorio temporal (`tempfile`).

### Fase 3: Motor Analítico DataFusion y Grafo Petgraph
- [ ] Configuración de `SessionContext` de Apache DataFusion con registro dinámico de centros activos.
- [ ] Implementación determinista de `lookup_double_entry_table` para cruce de rangos numéricos y categorías.
- [ ] Servicio de hidratación de subgrafos normativos en memoria con `petgraph`.
- [ ] Tests TDD de consultas SQL deterministas y aislamiento estricto de centros.

### Fase 4: Embeddings Locales y Búsqueda Híbrida
- [ ] Adaptador de embeddings locales con `fastembed` (BGE-M3 / multilingual).
- [ ] Búsqueda híbrida combinada: Vectorial (`IVF-PQ`/`HNSW`) + Filtros Arrow + BM25 Full-Text Search.
- [ ] Tests TDD de vectorización y scoring de relevancia híbrida.

### Fase 5: Pipeline de Ingesta y Destilación Claude Headless
- [ ] Parser documental preservando jerarquía estructural de títulos y tablas.
- [ ] Invocador de Claude Headless (`claude -p`) con salida estructurada JSON Schema para desnormalización.
- [ ] Orquestador de ingesta atómica hacia LanceDB.
- [ ] Tests TDD con fixtures de documentos técnicos de muestra.

### Fase 6: Servidor de Herramientas Data Lake y Agente
- [ ] Implementación de las 5 Tools:
  - `inspect_lakehouse_catalog`
  - `query_operational_matrix`
  - `lookup_double_entry_table`
  - `execute_athena_sql`
  - `traverse_normative_graph`
- [ ] Selector dinámico de modelos Claude (`sonnet`, `opus`, `haiku`) y wrapper asíncrono con streaming.
- [ ] Tests TDD de ejecución de tools y parsing de eventos JSON.

### Fase 7: Interfaz de Usuario TUI (Ratatui) y Chat
- [ ] Estructura de estado de aplicación (`AppState`) reactiva y desacoplada.
- [ ] Vista 1: Gestor de Centros de Conocimiento (CRUD, activación, stats) e Ingesta.
- [ ] Vista 2: Consola de Chat interactiva con conmutador rápido de modelo y visualización en tiempo real de Tools/SQL ejecutadas.
- [ ] Tests de lógica de estado UI.

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
