# Directiva de Ejecución para Claude Code: Tagging, Worktree & Evolución Arquitectónica (STAIR + RAG Universal)

> **Destinatario:** Claude Code (CLI / Subagente de desarrollo autónomo).
>
> **Repositorio de referencia:** [i-goikoetxea/lakehouse-RAG](https://github.com/i-goikoetxea/lakehouse-RAG)
>
> **Objetivo:** Etiquetar el estado actual del repositorio, inicializar un nuevo Git Worktree aislado e implementar la evolución hacia un **Data Lakehouse RAG en Rust con doble motor**: RAG Universal Sin Pérdidas (Capa 0 evolucionando `02_chunks_normativa`) + Navegación Estructural Jerárquica STAIR (Capa 1 con `07_indice_jerarquico`) + Motores de Precisión SQL/Grafos existentes (Capa 2), con TDD y validación End-to-End (E2E).

---

## 1. Fase 0: Procedimiento Git Obligatorio (Tagging y Creación de Worktree)

Antes de alterar cualquier línea de código del repositorio actual, Claude Code debe ejecutar estrictamente la siguiente secuencia de comandos:

```bash
# 1. Asegurar árbol de trabajo limpio o commitear cambios pendientes del estado de precisión
git status
git add -A
git commit -m "chore: snapshot de precision industrial previa a migracion dual/stair" || true

# 2. Generar tag inmutable del estado actual
git tag -a v1.0.0-precision-lakehouse -m "Estado base: Esquemas de precision (celdas, matrices operativas y grafos) previo a STAIR y RAG dual"
git push origin v1.0.0-precision-lakehouse || true

# 3. Crear nueva rama y su respectivo git worktree aislado
# Ubicar el worktree como hermano (../lakehouse-rag-dual) para no interferir con el workspace actual
git worktree add -b feat/stair-rag-dual ../lakehouse-rag-dual v1.0.0-precision-lakehouse

# 4. Navegar al nuevo entorno de trabajo aislado
cd ../lakehouse-rag-dual

# 5. Comprobar compilación inicial de sanidad
cargo check
```

*Restricción:* Todo el trabajo sucesivo debe realizarse exclusivamente dentro del directorio del worktree (`../lakehouse-rag-dual`).

---

## 2. Diagnóstico del Estado Real y Justificación de las Decisiones

A partir de la revisión del código en `i-goikoetxea/lakehouse-RAG`, se han tomado **8 decisiones arquitectónicas firmes** que sustituyen y corrigen cualquier borrador previo:

1. **Cero Duplicidad (Capa 0 en `02_chunks_normativa`):** **NO** se creará una tabla `00_corpus_universal`. Se evoluciona la tabla existente `02_chunks_normativa` para que sea el corpus universal sin pérdidas.
2. **Búsqueda Híbrida:** Se reutiliza y extiende `search_normative_context`. **NO** se exigen índices nativos `IVF-PQ/FTS` de LanceDB; se mantiene el índice híbrido en memoria actual (Coseno + BM25 + RRF), que ofrece latencias sub-10 ms en datasets locales y evita problemas de compilación en Windows.
3. **Ingesta Desconectada / Fallback sin Claude:** Si la destilación falla o se ejecuta offline, el `doc_id` se genera de forma determinista mediante el slug del fichero + hash corto SHA-256. El documento se indexa al 100% en Capa 0 (`02`) y Capa 1 (`07`) con `estado_ingesta = "PARCIAL_UNIVERSAL"`, habilitando un reintento posterior para Capa 2.
4. **Resúmenes ToC Extractivos:** `resumen_seccion` en la tabla `07` se genera de forma determinista (extractiva: título + primeras dos frases del primer párrafo, máx. 250 caracteres), eliminando la dependencia y coste de decenas de llamadas a la API durante la ingesta.
5. **Esquema de Auditoría Unificado:** La nueva tabla `07_indice_jerarquico.lance` **DEBE** incluir obligatoriamente las mismas columnas de control de ciclo de vida que el resto: `centro_id`, `version_num`, `vigente`, `ingest_id`, `estado_ingesta`.
6. **Extracción PDF en Windows:** Soporte para PDFs digitales nativos sin OCR pesado en C++ en este worktree. Si un PDF tiene menos de 50 caracteres por página, marcarlo con `estado_ingesta = "REQUIERE_OCR"`.
7. **Chunking Jerárquico Flexible:** Límite orientativo de 512 tokens con 64 de solapamiento solo si una sección supera 600 tokens. Secciones cortas se mantienen como un único chunk atómico.
8. **Técnicas de STAIR (IBM):** Se integran 3 técnicas: ToC como unidad de recuperación, búsqueda restringida por Trie de secciones reales y herencia estricta de condiciones de contorno (ancestros).

---

## 3. Especificación de Esquemas Apache Arrow

Claude Code debe localizar el módulo donde residen los esquemas Arrow del proyecto (adaptándose a la estructura real del repositorio) y aplicar los siguientes cambios:

### 3.1 Evolución de `02_chunks_normativa.lance` (Capa 0: Corpus Universal Sin Pérdidas)

La tabla `02` ya cuenta con auditoría. Se asegura que su esquema contenga:

| Campo | Tipo Arrow | Restricciones / Función |
| :--- | :--- | :--- |
| `chunk_id` | `Utf8` | PK (ej. `DOC-01_chk_00014` o `PROC-SOLD-04#sec-4.2_p1`). |
| `doc_id` | `Utf8` | FK al catálogo de documentos. |
| `centro_id` | `Utf8` | Identificador del centro de conocimiento activo. |
| `version_num` | `Int32` | Número de revisión documental. |
| `vigente` | `Boolean` | `true` si es la versión activa en el Data Lake. |
| `ingest_id` | `Utf8` | UUID del lote de ingesta. |
| `estado_ingesta` | `Utf8` | `COMPLETO`, `PARCIAL_UNIVERSAL`, `REQUIERE_OCR`. |
| `breadcrumb_jerarquia`| `Utf8` | Ruta completa de títulos padres (ej. `Preámbulo` o `Cap 4 > 4.2 Soldadura`). |
| `condicion_aplicacion`| `Utf8` (Nullable) | Restricción heredada del nodo padre o detectada en el texto. |
| `nivel_prelacion` | `Int8` | `1` (Cliente/Particular) a `5` (Norma general supletoria). |
| `tipo_bloque` | `Utf8` | `PREAMBULO`, `NARRATIVA`, `DEFINICION`, `ALCANCE`, `PROCEDIMIENTO`. |
| `texto_contenido` | `Utf8` | **Texto íntegro sin mutilar**. Incluye el texto previo al primer H1. |
| `tablas_vinculadas` | `List<Utf8>` | IDs de tablas asociadas (`04`/`05`). Vacío si no aplican. |
| `hitos_vinculados` | `List<Utf8>` | IDs de hitos operativos asociados (`03`). Vacío si no aplican. |
| `vector` | `FixedSizeList<Float32, D>` | Embedding denso (`fastembed`) sobre `breadcrumb + " " + texto_contenido`. |

### 3.2 Nueva Tabla: `07_indice_jerarquico.lance` (Capa 1: STAIR ToC Graph)

Almacena el árbol estructural de contenidos (ToC) con las columnas de auditoría estándar:

| Campo | Tipo Arrow | Restricciones / Función |
| :--- | :--- | :--- |
| `nodo_id` | `Utf8` | PK (ej. `PROC-SOLD-04#sec-4.2` o `MANUAL-CAL-01#sec-preambulo`). |
| `doc_id` | `Utf8` | FK al documento. |
| `centro_id` | `Utf8` | FK al centro de conocimiento. |
| `version_num` | `Int32` | Número de revisión documental. |
| `vigente` | `Boolean` | Control de versión activa. |
| `ingest_id` | `Utf8` | UUID del lote de ingesta. |
| `estado_ingesta` | `Utf8` | Estado del procesamiento del nodo. |
| `nivel_jerarquico` | `Int8` | `0` (Preámbulo/Raíz), `1` (H1), `2` (H2), `3` (H3). |
| `codigo_seccion` | `Utf8` | Ej. `"0.0"`, `"4.2"`, `"Anexo B"`. |
| `titulo_seccion` | `Utf8` | Texto literal del encabezado. |
| `nodo_padre_id` | `Utf8` (Nullable) | PK del nodo padre. `Null` si es nivel raíz o preámbulo. |
| `ruta_linaje` | `Utf8` | Ruta jerárquica: `/4/4.2` o `/0` para preámbulo. |
| `condiciones_alcance` | `Utf8` (Nullable) | Restricción general aplicable a todo este subárbol. |
| `resumen_seccion` | `Utf8` | **Generación extractiva:** Título + 2 primeras frases del párrafo (máx. 250 car.). |
| `vector_seccion` | `FixedSizeList<Float32, D>` | Embedding sobre `titulo_seccion + " " + resumen_seccion`. |

---

## 4. Pipeline de Ingesta Dual (No Destructivo)

Claude Code debe ajustar el flujo de ingesta para que opere en **dos etapas estrictas**:

```text
[ Documento Técnico: PDF / Markdown ]
                 │
                 ▼
     [ Extractor Estructural ]
                 │
                 ├──► ETAPA 1: INGESTA UNIVERSAL (Garantizada / Offline / Sin Claude)
                 │    ├── 1. Extraer texto previo al primer H1 -> Chunk de "Preámbulo / General"
                 │    ├── 2. Construir árbol ToC con Trie -> Insertar en `07_indice_jerarquico.lance`
                 │    │      (Resumen extractivo determinista, cero coste LLM)
                 │    ├── 3. Chunking jerárquico inteligente (solo trocear si > 600 tokens)
                 │    └── 4. Inserción del 100% del texto en `02_chunks_normativa.lance`
                 │           (estado_ingesta = "PARCIAL_UNIVERSAL")
                 │
                 └──► ETAPA 2: DESTILACIÓN DE PRECISIÓN (Asíncrona / Condicional con Claude)
                      ├── Si Claude está disponible y la sección tiene tablas/matrices:
                      │   ├── Extraer Tuplas Operativas -> `03_matriz_operativa.lance`
                      │   ├── Desnormalizar Tablas Doble Entrada -> `05_celdas_doble_entrada.lance`
                      │   └── Extraer Referencias Cruzadas -> `06_grafo_referencias.lance`
                      │   └── Actualizar estado_ingesta = "COMPLETO"
                      └── Si Claude falla o está offline:
                          └── Registrar advertencia y mantener estado_ingesta = "PARCIAL_UNIVERSAL"
                              (El documento ya es 100% consultable en Capa 0 y Capa 1)
```

### Reglas de Implementación en Ingesta:
1. **Captura del Preámbulo:** El texto previo al primer encabezado se indexa con `nodo_id = "<doc_id>#sec-preambulo"`, `codigo_seccion = "0.0"`, `titulo_seccion = "Preámbulo / Introducción"`, `ruta_linaje = "/0"`.
2. **Determinismo de `doc_id`:** Si Claude no destila el documento, el `doc_id` se calcula sanitizando el nombre de archivo a mayúsculas y agregando los primeros 6 caracteres del hash SHA-256 (ej. `PROC_SOLDADURA_A4F2`).
3. **Chunking Inteligente:** Si una sección mide $\le 600$ tokens, se emite un único chunk en `02`. Si supera los 600 tokens, se sub-fragmenta con ventana de 512 tokens y solapamiento de 64 tokens, manteniendo en todos los sub-chunks el mismo `breadcrumb_jerarquia` y `nodo_id`.

---

## 5. Módulo del Motor de Búsqueda y Tools de Claude Headless

### 5.1 Evolución del Motor de Búsqueda Híbrida
* Mantener la implementación actual de **búsqueda híbrida en memoria** (Coseno + BM25 + RRF) sobre `02_chunks_normativa`.
* Al arrancar o registrar un centro activo, el motor carga los textos y vectores de `02` en la estructura en memoria.
* Añadir soporte para filtrar por `doc_id` y por `nodo_id` / prefijo de `ruta_linaje`.

### 5.2 Actualización de Tools para Claude Headless
Claude Code debe actualizar el registro de herramientas en el módulo correspondiente:

1. **`search_normative_context` (Evolucionada):**
   * Pasa a consultar la totalidad de `02_chunks_normativa` (Capa 0).
   * Añadir parámetro opcional `modo`: `"HYBRID"` (default), `"SEMANTIC_ONLY"`, `"EXACT_BM25"`.
   * Parámetro opcional `doc_id` para acotar la búsqueda a un manual o norma específica.
2. **`stair_navigate_toc` (Nueva):**
   * Permite a Claude explorar el índice del documento antes de pedir el texto completo.
   * Parámetros: `doc_id` (String), `nodo_padre_id` (Option<String>).
   * Consulta `07_indice_jerarquico.lance` en DataFusion filtrando por `centro_id` activo y `vigente = true`, ordenado por `ruta_linaje ASC`.
3. **`stair_get_subtree_context` (Nueva):**
   * Recupera todos los chunks de una sección completa y sus subsecciones.
   * Parámetros: `doc_id` (String), `nodo_id` (String).
   * Lee la `ruta_linaje` del nodo en `07` y recupera todos los registros de `02` donde `breadcrumb_jerarquia` o referencia estructural coincida con el subárbol, inyectando las `condiciones_alcance` del padre.
4. **Tools Existentes Preservadas:**
   * `query_operational_matrix` (sobre `03`): Se mantiene para consultas estrictas de roles y registros.
   * `lookup_double_entry_table` / SQL (sobre `05`): Se mantiene para cruces numéricos exactos de tablas.

---

## 6. Protocolo de Pruebas TDD y Validación E2E

Claude Code debe aplicar **TDD estricto** en el worktree `feat/stair-rag-dual`.

### 6.1 Tests Unitarios Requeridos
* `test_lossless_text_coverage`: Validar matemáticamente que el 100% de los tokens únicos del texto de entrada original están contenidos en la unión de los chunks generados para `02_chunks_normativa` (incluyendo preámbulo).
* `test_toc_tree_lineage`: Validar que el parser jerárquico construya las rutas de linaje correctas (`/0`, `/1`, `/1/1.1`, `/1/1.1/1.1.1`) y asigne los `nodo_padre_id` correspondientes.
* `test_offline_ingestion_fallback`: Validar que ante un error simulado en Claude, el documento quede indexado en `02` y `07` con `estado_ingesta = "PARCIAL_UNIVERSAL"` y `doc_id` determinista.

### 6.2 Test E2E de Validación Dual (`tests/e2e/rag_dual_validation.rs`)
La suite E2E debe ingerir dos documentos de prueba:
1. `DOC-GENERICO-01.md`: Texto puramente narrativo (política de empresa, preámbulo, recomendaciones generales, sin tablas ni roles).
2. `DOC-TECNICO-OBRA-02.md`: Procedimiento técnico completo (preámbulo, tabla de doble entrada de tolerancias y matriz operativa con fases de *Recepción*, *Custodia* e *Inspección* claramente diferenciadas).

### 6.3 Matriz de Validación de los 4 Casos Críticos

| ID Test | Entrada de Consulta | Herramienta que DEBE dispararse | Criterio de Éxito Innegociable |
| :--- | :--- | :--- | :--- |
| **E2E-01** | *"¿Cuál es la política general sobre EPIs según el manual de bienvenida?"* | `search_normative_context` (Capa 0) | Encuentra la respuesta en el texto narrativo de `DOC-GENERICO-01`. **Falla si devuelve que no hay datos.** |
| **E2E-02** | *"¿Qué apartados componen el Capítulo 3 del procedimiento técnico?"* | `stair_navigate_toc` (Capa 1) | Devuelve los títulos y códigos exactos de las subsecciones del Capítulo 3 desde `07_indice_jerarquico`. |
| **E2E-03** | *"¿Quién custodia las probetas y qué formato se rellena?"* | `query_operational_matrix` (Capa 2) | Devuelve **únicamente** la tupla de la fase `CUSTODIA`. **Falla si contamina con datos de Recepción o Inspección.** |
| **E2E-04** | *"Para relación A/C de 0.45 y árido rodado, ¿cuál es la resistencia mínima fck?"* | `lookup_double_entry_table` o SQL (Capa 2) | Devuelve el valor exacto del cruce y la nota al pie aplicable desde `05_celdas_doble_entrada`. |

---

## 7. Plan de Acción Secuencial para Claude Code

1. **Paso 1:** Ejecutar los comandos de la **Fase 0** (commit snapshot, crear tag `v1.0.0-precision-lakehouse`, crear worktree `feat/stair-rag-dual`).
2. **Paso 2:** Modificar la definición de esquemas para incorporar `07_indice_jerarquico.lance` (con columnas de auditoría estándar) y ajustar `02_chunks_normativa.lance`.
3. **Paso 3:** Adaptar el parser de ingesta para capturar el preámbulo, construir el árbol ToC extractivo (STAIR) y garantizar chunking sin pérdidas hacia la tabla `02`.
4. **Paso 4:** Implementar el fallback determinista de `doc_id` y metadatos cuando Claude no intervenga en la ingesta.
5. **Paso 5:** Exponer las nuevas tools (`stair_navigate_toc`, `stair_get_subtree_context`) y actualizar `search_normative_context` con el parámetro de modo de búsqueda.
6. **Paso 6:** Implementar la suite de tests unitarios y el test E2E (`rag_dual_validation.rs`).
7. **Paso 7:** Ejecutar `cargo test` asegurando que todos los tests pasen al 100%.
8. **Paso 8:** Generar reporte de validación certificando el cumplimiento de la matriz de 4 casos.