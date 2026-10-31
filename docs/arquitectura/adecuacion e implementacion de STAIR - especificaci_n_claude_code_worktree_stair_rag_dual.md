# Directiva de Ejecución para Claude Code: Tagging, Worktree & Arquitectura Dual (STAIR + RAG Convencional)

> **Destinatario:** Claude Code (CLI / Subagente de desarrollo autónomo).  
> **Objetivo:** Etiquetar el estado actual del repositorio, inicializar un nuevo Git Worktree aislado, e implementar la evolución arquitectónica hacia un **Data Lakehouse RAG en Rust con doble motor**: RAG Convencional Universal (Capa 0 sin pérdida de información) + Navegación Estructural Jerárquica STAIR (Capa 1) + Motores de Precisión SQL/Grafos (Capa 2), con desarrollo guiado por pruebas (TDD) y validación End-to-End (E2E).

---

## 1. Fase 0: Procedimiento Git Obligatorio (Tagging y Creación de Worktree)

Antes de alterar cualquier línea de código del repositorio actual, Claude Code debe ejecutar estrictamente la siguiente secuencia de comandos Bash:

```bash
# 1. Asegurar árbol de trabajo limpio o commitear cambios pendientes del estado de precisión
git status
git add -A
git commit -m "chore: snapshot de precision industrial previa a migracion dual/stair" || true

# 2. Generar tag inmutable del estado actual
git tag -a v1.0.0-precision-lakehouse -m "Estado base: Esquemas de precision (celdas, matrices operativas y grafos) previo a STAIR y RAG dual"
git push origin v1.0.0-precision-lakehouse || true

# 3. Crear nueva rama y su respectivo git worktree aislado
# La estructura de directorios ubicará el worktree como un hermano (../lakehouse-rag-dual)
git worktree add -b feat/stair-rag-dual ../lakehouse-rag-dual v1.0.0-precision-lakehouse

# 4. Navegar al nuevo entorno de trabajo aislado
cd ../lakehouse-rag-dual

# 5. Comprobar compilación inicial de sanidad
cargo check
```

*Restricción:* Todo el trabajo sucesivo debe realizarse exclusivamente dentro del directorio del worktree (`../lakehouse-rag-dual`).

---

## 2. Diagnóstico del Problema y Justificación de la Arquitectura

### 2.1 El Defecto Actual ("Trampa de la Sobreespecialización")
La implementación en `v1.0.0-precision-lakehouse` estructuró los datos de forma tan rígida en torno a:
1. `03_matriz_operativa.lance` (solo hito/fase/responsable/registro).
2. `05_celdas_doble_entrada.lance` (solo coordenadas de matrices numéricas/categóricas).

**Consecuencia:** Si un PDF técnico contiene secciones narrativas (alcances, antecedentes, definiciones generales, advertencias de seguridad o explicaciones conceptuales) que no encajan en una tupla operativa ni en una matriz numérica, el pipeline de extracción las descarta o el motor de consulta no ofrece una vía de recuperación general. El sistema actúa como una calculadora relacional de obra, pero ha dejado de funcionar como un centro de conocimiento o RAG convencional.

### 2.2 La Solución: Modelo Concéntrico en 3 Capas
El nuevo diseño no elimina los motores de precisión, sino que los envuelve en un modelo en tres capas concéntricas con **cero pérdida de información**:

```text
┌────────────────────────────────────────────────────────────────────────┐
│  CAPA 0: RAG Universal Sin Pérdidas (Baseline Exhaustivo)               │
│  - 00_corpus_universal.lance                                           │
│  - Chunking completo de TODO el texto del PDF (sin omisiones)          │
│  - Búsqueda Híbrida: Vector Denso (fastembed) + BM25 Léxico (LanceDB)  │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Linaje Estructural
┌───────────────────────────────────▼────────────────────────────────────┐
│  CAPA 1: Navegación Estructural STAIR (Structure-Aware Retrieval)       │
│  - 07_indice_jerarquico.lance                                          │
│  - Árbol de contenidos (ToC), breadcrumbs normalizados y resúmenes     │
│  - Herencia de condiciones de contorno (nodo padre -> hijos)           │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Restricciones Duras
┌───────────────────────────────────▼────────────────────────────────────┐
│  CAPA 2: Motores de Precisión Deterministas (Especialización)          │
│  - 03_matriz_operativa.lance (Anti-Fusión Roles/Registros)             │
│  - 05_celdas_doble_entrada.lance (Cruce de Tablas por SQL DataFusion)  │
│  - 06_grafo_referencias.lance (GraphRAG Columnar + petgraph)           │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Especificación de los Nuevos Esquemas Apache Arrow / LanceDB

Claude Code debe incorporar en `src/schema/arrow_schemas.rs` los dos nuevos esquemas que integran el RAG universal y el principio STAIR:

### 3.1 Tabla `00_corpus_universal.lance` (Capa 0: Cero Pérdidas)
Garantiza que el 100% del documento crudo se pueda consultar mediante búsqueda tradicional o semántica:

| Campo | Tipo Arrow | Restricciones / Función |
| :--- | :--- | :--- |
| `chunk_id` | `Utf8` | PK (ej. `DOC-01_chk_00014`). |
| `doc_id` | `Utf8` | FK al catálogo de documentos. |
| `centro_id` | `Utf8` | Identificador del centro de conocimiento. |
| `seccion_nodo_id` | `Utf8` | FK al nodo correspondiente en `07_indice_jerarquico.lance`. |
| `pagina_inicio` | `Int32` | Página del PDF donde inicia el bloque. |
| `pagina_fin` | `Int32` | Página del PDF donde finaliza el bloque. |
| `tipo_bloque` | `Utf8` | `NARRATIVA`, `DEFINICION`, `ALCANCE`, `ADVERTENCIA`, `TABLA_RAW_MD`. |
| `texto_contenido` | `Utf8` | Texto íntegro del fragmento sin mutilar. |
| `vector` | `FixedSizeList<Float32, D>` | Vector de embedding denso calculado sobre `texto_contenido`. |
| `vigente` | `Boolean` | `true` si la versión del documento está activa. |

*Índices obligatorios en LanceDB:*
* Índice vectorial: `IVF-PQ` o `HNSW` sobre columna `vector` (métrica: Coseno).
* Índice léxico: `Full-Text Search (BM25)` sobre columna `texto_contenido`.

### 3.2 Tabla `07_indice_jerarquico.lance` (Capa 1: STAIR ToC Graph)
Modela el árbol de contenidos del documento como una jerarquía relacional y semántica:

| Campo | Tipo Arrow | Restricciones / Función |
| :--- | :--- | :--- |
| `nodo_id` | `Utf8` | PK (ej. `PE-OBRA-012#sec-4.2`). |
| `doc_id` | `Utf8` | FK al catálogo de documentos. |
| `centro_id` | `Utf8` | Identificador del centro de conocimiento. |
| `nivel_jerarquico` | `Int8` | `1` (Capítulo / H1), `2` (Sección / H2), `3` (Subsección / H3), etc. |
| `codigo_seccion` | `Utf8` | Ej. `"4.2"`, `"Anexo B"`. |
| `titulo_seccion` | `Utf8` | Texto del encabezado (ej. `"Precalentamiento y Temperaturas Interpasada"`). |
| `nodo_padre_id` | `Utf8` (Nullable) | PK del nodo superior (ej. `PE-OBRA-012#sec-4.0`). `Null` si es raíz. |
| `ruta_linaje` | `Utf8` | Ruta materializada estilo filesystem: `/4/4.2` (permite consultas SQL con `LIKE '/4/%'`). |
| `condiciones_alcance` | `Utf8` (Nullable) | Restricciones heredadas de la cabecera (ej. `"Válido para aceros S355 y ambiente < 0°C"`). |
| `resumen_seccion` | `Utf8` | Síntesis concisa del contenido del nodo (generada en ingesta). |
| `vector_seccion` | `FixedSizeList<Float32, D>` | Embedding de `titulo_seccion + " " + resumen_seccion`. |

---

## 4. Pipeline de Ingesta Dual (No Destructivo)

Claude Code deberá refactorizar `src/ingestion/` para desacoplar el pipeline en dos pasos secuenciales estrictos:

```text
[PDF / Markdown / DOCX]
          │
          ▼
   [doc_parser.rs] ─── Extracción completa estructurada en Markdown/AST
          │
          ├───► 1. PIPELINE UNIVERSAL (Incondicional)
          │        ├── Chunking con solapamiento controlado (512 tokens, 64 overlap)
          │        ├── Inserción directa en `00_corpus_universal.lance` (100% de los datos)
          │        └── Construcción del árbol ToC -> `07_indice_jerarquico.lance` (STAIR)
          │
          └───► 2. PIPELINE DE PRECISIÓN (Condicional / Especializado)
                   ├── Si detecta sección de procedimiento con roles/acciones
                   │   └── Extracción de Tuplas Operativas -> `03_matriz_operativa.lance`
                   └── Si detecta tablas matriciales/doble entrada
                       └── Desnormalización de celdas min/max -> `05_celdas_doble_entrada.lance`
```

**Regla de oro de la ingesta:** La falla o la no aplicabilidad de la extracción de precisión (por ejemplo, en un documento que no tiene tablas ni procedimientos de calidad) **jamás** debe impedir que el documento se indexe al 100% en la Capa 0 y la Capa 1.

---

## 5. Módulo del Agente: Enrutamiento Dinámico y Nuevas Tools

Claude Code deberá actualizar `src/agent/mcp_tools.rs` para dotar a Claude Headless del conjunto completo de herramientas necesarias para operar en las 3 capas:

### 5.1 Nuevas Tools a Registrar

#### Tool A: `universal_knowledge_search` (Búsqueda General / Convencional)
* **Propósito:** Responder preguntas generales, narrativas, definiciones, resúmenes o dudas de cualquier tema.
* **Parámetros:**
  * `query_text` (`String`): Pregunta o términos de búsqueda.
  * `modo` (`Enum`: `HYBRID`, `SEMANTIC_ONLY`, `EXACT_BM25`).
  * `filtro_doc_id` (`Option<String>`).
  * `limit` (`Option<usize>`, default: 5).
* **Comportamiento en Rust:** Ejecuta en LanceDB una búsqueda vectorial combinada con BM25 sobre `00_corpus_universal.lance` aplicando filtro de documentos vigentes.

#### Tool B: `stair_navigate_toc` (Navegación Estructural de Documentos)
* **Propósito:** Explorar el índice y jerarquía de un procedimiento o norma antes de pedir detalles o para responder preguntas de estructura de documento.
* **Parámetros:**
  * `doc_id` (`String`): Identificador del documento.
  * `nodo_padre_id` (`Option<String>`): Si se indica, devuelve las subsecciones hijas; si es `None`, devuelve los capítulos raíz (nivel 1).
* **Comportamiento en Rust:** Lanza una consulta en DataFusion sobre `07_indice_jerarquico.lance` ordenada por `ruta_linaje ASC`.

#### Tool C: `stair_get_subtree_context` (Recuperación en Cascada de Sección Completa)
* **Propósito:** Recuperar todo el contenido de una sección completa (incluyendo subsecciones y condiciones de contorno de los padres) sin perder el contexto global.
* **Parámetros:**
  * `nodo_id` (`String`): Nodo del índice a expandir.
* **Comportamiento en Rust:** Lee la `ruta_linaje` del nodo y recupera todos los chunks de `00_corpus_universal.lance` cuyos nodos asociados tengan `ruta_linaje LIKE '<ruta>%'`.

### 5.2 Estrategia de Enrutamiento del Agente en el System Prompt
El prompt del sistema de Claude Headless debe actualizarse con la siguiente heurística de decisión:
1. **Pregunta Genérica, Narrativa o Conceptual** (*"¿De qué trata este documento?", "¿Qué medidas de seguridad generales se indican?"*):  
   Invocar de inmediato `universal_knowledge_search`.
2. **Pregunta Estructural o de Alcance** (*"¿Qué capítulos tiene esta norma?", "¿Qué apartados tratan sobre soldadura?"*):  
   Invocar `stair_navigate_toc` o `stair_get_subtree_context`.
3. **Pregunta Operativa / Roles / Registros** (*"¿Quién hace la custodia y qué registro firma?"*):  
   Invocar `query_operational_matrix` sobre `03_matriz_operativa.lance`.
4. **Pregunta Numérica / Cruce de Tablas** (*"¿Cuál es el valor para espesor 18 mm y acero S355?"*):  
   Invocar `lookup_double_entry_table` o `execute_athena_sql` sobre `05_celdas_doble_entrada.lance`.

---

## 6. Protocolo de Pruebas TDD y Validación End-to-End (E2E)

Claude Code debe seguir una metodología **TDD estricta (Red-Green-Refactor)**. No se considerará completada la tarea hasta que todos los tests unitarios y de integración pasen con éxito.

### 6.1 Tests Unitarios Requeridos (`tests/unit/`)
* `test_toc_builder_hierarchy`: Validar que al parsear un documento con `H1`, `H2` y `H3` se construyan correctamente las `ruta_linaje` (`/1`, `/1/1.1`, `/1/1.1/1.1.1`).
* `test_universal_chunker_lossless`: Validar que la concatenación de los chunks de `00_corpus_universal` conserve todos los términos de un texto de entrada sin saltarse párrafos.
* `test_condition_inheritance`: Validar que un nodo hijo herede las condiciones de alcance definidas en su nodo padre en `07_indice_jerarquico`.

### 6.2 Batería de Pruebas de Integración y E2E (`tests/e2e/rag_dual_validation.rs`)
Crear un archivo de prueba E2E que ingiera un conjunto de prueba compuesto por **dos documentos sintéticos muy contrastados**:

1. **Documento A (`DOC-GENERICO-01.md`):**  
   Manual de Bienvenida y Políticas Generales de la Empresa (texto puramente narrativo, sin tablas ni responsabilidades formales ni formatos PPI).
2. **Documento B (`DOC-TECNICO-OBRA-02.md`):**  
   Procedimiento de Hormigonado y Control de Calidad (contiene texto general, tabla de tolerancias de doble entrada `Relación A/C vs Resistencia fck` y tres fases operativas explícitas: *Recepción de Camión Hormigonera*, *Custodia de Probetas en Piscina*, e *Inspección de Rotura a 28 Días* con responsables y registros distintos).

### 6.3 Matriz de Validación de los 4 Casos Críticos

| ID Test | Entrada de Consulta | Herramienta que DEBE dispararse | Criterio de Éxito Innegociable |
| :--- | :--- | :--- | :--- |
| **E2E-01** | *"¿Cuál es la política general sobre el uso de EPIs según el manual de bienvenida?"* | `universal_knowledge_search` | Encuentra la respuesta en `DOC-GENERICO-01` con similitud semántica/BM25. **Falla si devuelve que no hay datos.** |
| **E2E-02** | *"¿Qué apartados componen el Capítulo 3 del procedimiento de hormigonado?"* | `stair_navigate_toc` | Devuelve la lista exacta de títulos de las subsecciones del Capítulo 3 sin alucinaciones. |
| **E2E-03** | *"¿Quién custodia las probetas de hormigón y qué formato se rellena?"* | `query_operational_matrix` | Devuelve **exclusivamente** el responsable y registro de la fase de *Custodia*. **Falla si menciona el albarán de recepción o al inspector de rotura.** |
| **E2E-04** | *"Para una relación A/C de 0.45 y árido rodado, ¿cuál es la resistencia mínima fck?"* | `lookup_double_entry_table` o `execute_athena_sql` | Cruce exacto en la tabla `05_celdas_doble_entrada`. **Falla si el LLM intenta interpolar o leer a ojo.** |

---

## 7. Plan de Acción Secuencial para Claude Code

Claude Code deberá ejecutar el plan en los siguientes pasos comprobables:

1. **Paso 1:** Ejecutar comandos de Git de la **Fase 0** (commit snapshot, tag `v1.0.0-precision-lakehouse`, creación y checkout del worktree `feat/stair-rag-dual`).
2. **Paso 2:** Modificar `src/schema/arrow_schemas.rs` para incluir las definiciones de `00_corpus_universal.lance` y `07_indice_jerarquico.lance`.
3. **Paso 3:** Implementar en `src/ingestion/` el parser jerárquico ToC (STAIR) y la ingesta incondicional sin pérdidas hacia la Capa 0.
4. **Paso 4:** Actualizar `src/engine/hybrid_search.rs` y `src/engine/datafusion_ctx.rs` para soportar búsqueda híbrida (vector + BM25) y navegación por árbol en DataFusion.
5. **Paso 5:** Exponer en `src/agent/mcp_tools.rs` las tools `universal_knowledge_search`, `stair_navigate_toc` y `stair_get_subtree_context`, actualizando el prompt del agente.
6. **Paso 6:** Escribir los tests unitarios y la suite E2E en `tests/e2e/rag_dual_validation.rs`.
7. **Paso 7:** Ejecutar `cargo test` asegurando que todos los tests pasen al 100%.
8. **Paso 8:** Generar informe final de validación con los resultados de las 4 pruebas de la matriz.