# AGENTS.md — Protocolo Operativo y Contrato de Desarrollo para Agentes

> **Propósito:** Este documento define el estándar innegociable de trabajo para cualquier agente de IA o desarrollador que abra una sesión en este repositorio. Su objetivo es garantizar la continuidad absoluta del desarrollo, el respeto irrestricto a la arquitectura y la ejecución metódica bajo **TDD** y **Clean Code**.

---

## 1. Reglas Globales de Comportamiento e Interacción

1. **Idioma y Tono:** Responde siempre en español neutro, con tono profesional y orientado a producto.
2. **Seguridad y Control de Backend:**
   * **NUNCA** generes código de backend o adaptadores sin antes validar la estructura y los contratos con el usuario.
   * Antes de cualquier cambio estructural en módulos o dependencias, explica el **"por qué"** y espera confirmación explícita (**SI / NO**).
3. **Didáctica y Analogías:** Si el usuario plantea dudas conceptuales, explica mediante situaciones cotidianas y ejemplos sencillos, evitando jerga excesiva a menos que lo pida expresamente.

---

## 2. Principios de Arquitectura Hexagonal y Clean Code

El sistema implementa **Arquitectura Hexagonal (Ports & Adapters)** pura para aislar el núcleo analítico de cualquier tecnología externa:

```text
src/
├── domain/                  # 1. NÚCLEO PURO (Reglas de Negocio)
│   ├── models/              # Entidades y Value Objects (inmutables, tipados)
│   ├── errors/              # DomainError tipado con thiserror
│   └── services/            # Servicios de dominio y algoritmos puros
│
├── application/             # 2. CASOS DE USO Y PUERTOS (Orquestación)
│   ├── ports/               # Traits de entrada y salida (Traits de Repositorios, Motores, LLM)
│   ├── use_cases/           # Flujos de negocio coordinando el dominio
│   └── dtos/                # Data Transfer Objects
│
├── infrastructure/          # 3. ADAPTADORES (Mundo Exterior e I/O)
│   ├── storage/             # Adaptador LanceDB (.lance) y esquemas Arrow
│   ├── query_engine/        # Adaptador Apache DataFusion y Petgraph
│   ├── embeddings/          # Adaptador FastEmbed local
│   ├── llm/                 # Adaptador Claude Headless CLI / MCP
│   └── parsers/             # Adaptadores de extracción de PDFs/Markdown
│
└── presentation/            # 4. INTERFAZ DE USUARIO Y CLI
    ├── tui/                 # Terminal UI con Ratatui + Crossterm
    └── cli/                 # Subcomandos de consola (ej. --mcp-server)
```

### Reglas Innegociables de Código:
* **Dominio Puro:** Prohibido importar crates de I/O (`tokio`, `lancedb`, `datafusion`, etc.) en `src/domain/`. El dominio solo utiliza Rust estándar, validaciones y tipos propios.
* **Inversión de Dependencias:** La aplicación define los traits (puertos); la infraestructura los implementa.
* **Nombres con Intención:** Prohibidos nombres genéricos como `data`, `info`, `manager`, `temp`. Usar nombres semánticos como `DocumentMetadata`, `RangeEvaluator`, `ActiveCentersCatalog`.
* **Funciones Monotarea:** Cada función resuelve una única responsabilidad.

---

## 3. Metodología TDD Estricta (Test-Driven Development)

Toda funcionalidad debe construirse siguiendo el ciclo **Rojo $\rightarrow$ Verde $\rightarrow$ Refactor**:
1. **Rojo (Red):** Escribir primero el test que describa el comportamiento esperado y verificar que falle (`cargo test`).
2. **Verde (Green):** Implementar la cantidad mínima de código necesaria para que el test pase.
3. **Refactor:** Limpiar duplicaciones, mejorar nombres y optimizar sin romper la suite de pruebas. **Nunca omitir el refactor.**

### Pirámide de Pruebas:
* **Unitarias (`src/domain/` y `src/application/`):** Rápidas, en memoria, sin tocar disco.
* **Integración (`tests/`):** Pruebas de adaptadores (`LanceDB`, `DataFusion`, `FastEmbed`) usando directorios temporales (`tempfile`).
* **Pruebas Adversariales de Seguridad y Resiliencia (`tests/adversarial_*`):** Al cierre de cada fase, es obligatorio diseñar y ejecutar una suite adversarial específica (inyecciones, caos de tipos, valores frontera extremos, paradojas lógicas y DoS) para garantizar la robustez del componente antes de su aprobación.
* **End-to-End (`tests/acceptance_*`):** Validación de los criterios de aceptación completos.

---

## 4. Protocolo de Inicio de Sesión (Onboarding del Agente)

Al iniciar una nueva sesión, el agente **DEBE**:
1. Leer íntegramente `PROGRESS.md` para conocer el estado exacto del proyecto.
2. Identificar la **Fase Actual** asignada para la sesión.
3. Ejecutar `cargo test` para verificar que la suite heredada pasa al 100%.
4. Notificar al usuario la fase a desarrollar y los criterios de aceptación de la sesión antes de comenzar el ciclo TDD.

---

## 5. Protocolo de Cierre de Sesión (Hand-off entre Agentes)

Antes de finalizar la sesión, el agente **DEBE**:
1. Ejecutar la suite completa de pruebas:
   ```powershell
   cargo test
   ```
2. Ejecutar el linter para garantizar código limpio:
   ```powershell
   cargo clippy -- -D warnings
   cargo fmt --check
   ```
3. Actualizar `PROGRESS.md`:
   * Marcar las tareas completadas (`- [x]`).
   * Actualizar el estado de la fase (`[COMPLETADO]`).
   * Añadir una entrada a la **Bitácora de Sesiones** indicando: cambios realizados, tests añadidos y notas explícitas para la siguiente sesión.
4. Informar al usuario del estado alcanzado y solicitar aprobación explícita de la fase.
5. **Git Commit, Merge y Push Remoto (Innegociable):**
   * Tras la aprobación explícita del usuario sobre la fase, realizar commit semántico (`git commit -m "feat(phase-X): ..."`).
   * Si se trabajó en rama temática de fase, realizar el merge correspondiente.
   * Realizar push inmediato al repositorio remoto en GitHub:
     ```powershell
     git push origin main
     ```

---

## 6. Estado actual (Fase 15: aceptación final) y guía de continuación

### Estado general del código (2026-10-19)
- **367 tests pasando, más 19 `#[ignore]`.** Los ignorados son las suites reales con Claude Code y FastEmbed, más un generador visual.
- `cargo clippy --all-targets -- -D warnings`: 0 avisos. `cargo fmt --check`: OK. `cargo build --release`: OK.
- **Aceptación real** `tests/acceptance_8_problemas.rs`:
  - 10/10 en 2 ejecuciones: los 8 problemas del plan v2 §1, más §9.1 y §9.4 de la especificación.
  - También pasan las suites reales de la etapa 1 y de las fases 12 y 13.
  - Informe para el usuario: `docs/validacion/aceptacion_final.md`.
- **Fase 15 aprobada por el usuario** tras la prueba guiada (demo en `docs/validacion/fase15/demo/`). El chat responde en streaming (`stream-json`) y tiene botón Detener. El plan v2 está completo.

### Componentes y puntos de entrada
1. **Fachada única `LakehouseApp`**, compuesta en `infrastructure/bootstrap.rs` (`composicion_real` / `componer_lakehouse`). La usan la GUI, el MCP y la CLI.
2. **Binarios:**
   - `lakehouse_rag.exe`: ventana Tauri sin consola en release. También acepta subcomandos.
   - `lakehouse_rag_cli.exe`: subsistema consola, para terminales, scripts y `--mcp-server`, con `$LASTEXITCODE` fiable.
3. **MCP stdio** con las 6 tools v2 (`MCP_GUIDE.md`).
4. **GUI** (`src/presentation/gui/web/`): 3 paneles redimensionables, centros (crear, activar y quitar con confirmación), ingesta por diálogo nativo o arrastre, chat con selector de modelo en caliente y Studio.
5. **Sesión de Claude Code ausente o caducada:** mensaje accionable (`infrastructure/llm/sesion_claude.rs`).

### Diagnóstico Técnico del Despliegue de GUI (Tauri 2 en Windows) para Claude Code:
1. **Supresión de Terminal en Windows:**
   - La aplicación gráfica debe ejecutarse sin abrir ventana de consola `cmd.exe`.
   - Se debe mantener `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` (o `#![windows_subsystem = "windows"]`) en `src/main.rs`.
2. **Bucle de Eventos (Event Loop) en el Hilo Principal:**
   - `tauri::Builder::default().run(...)` requiere ejecutarse en un hilo principal síncrono. No debe envolverse `main()` en `#[tokio::main]`; Tokio debe instanciarse bajo demanda solo cuando se active el flag `--mcp-server`.
3. **Configuración de Tauri 2 (`tauri.conf.json` y `capabilities/`):**
   - La ventana en `tauri.conf.json` debe tener explícitamente `"label": "main"`, `"url": "index.html"`, `"decorations": false`, `"width": 1280`, `"height": 720`.
   - Las `capabilities` en Tauri 2 deben residir en `capabilities/default.json` con los permisos `core:window:allow-minimize`, `core:window:allow-maximize`, `core:window:allow-toggle-maximize`, `core:window:allow-close`. No deben declararse propiedades no reconocidas en `"security"` dentro de `tauri.conf.json`.
4. **Comprobación de WebView2 Runtime:**
   - Si la ventana se cierra de inmediato en el entorno local, verificar que el runtime de WebView2 de Windows no falle en el arranque o lanzar en modo dev con `cargo tauri dev`.


