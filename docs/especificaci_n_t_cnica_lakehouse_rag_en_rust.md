# Especificación Técnica del Sistema (SRS): Lakehouse RAG Industrial en Rust

> **Propósito del documento:** Este documento constituye la especificación de ingeniería exhaustiva para que **Antigravity** elabore el plan de acción y genere el código fuente completo de una aplicación nativa en **Rust**. No debe omitirse ninguna restricción arquitectónica ni tecnológica aquí definida.

---

## 1. Visión General y Restricciones Innegociables de Arquitectura

El objetivo es construir una aplicación local en **Rust** que implemente un sistema **Data Lakehouse RAG (Schema-on-Read + Vector + SQL + Graph)** orientado a documentación técnica de ingeniería y obra (procedimientos operativos, instructivos, normativas UNE/ISO/ASME, planes de puntos de inspección y tablas técnicas de doble entrada).

### 1.1 Restricciones Arquitectónicas Estrictas (PROHIBICIONES)
1. **Cero servidores externos en memoria:** Queda estrictamente prohibido levantar bases de datos vectoriales cliente-servidor (Qdrant, Milvus, Chroma) o motores de grafos dedicados (Neo4j, Memgraph).
2. **Almacenamiento 100% basado en disco (Single Source of Truth):** Todo centro de conocimiento debe residir físicamente en una carpeta del sistema de archivos local utilizando el formato columnar **Lance (`.lance`)** sobre **Apache Arrow**, permitiendo acceso aleatorio (*random access*) sub-milisegundo sin cargar la base de datos en RAM.
3. **Prohibición de Chunking Ciego para Tablas y Matrices de Responsabilidad:**
   * Las **tablas de doble entrada** jamás se consultarán mediante embeddings de texto plano; deben desnormalizarse durante la ingesta y consultarse exclusivamente vía **SQL determinista (`DataFusion`)**.
   * Las **fases operativas** (ej. *Recepción*, *Custodia*, *Inspección*) jamás deben fusionarse en un mismo registro semántico; deben aislarse en tuplas estructuradas para evitar contaminación de atributos (*Attribute Bleeding*).
4. **Motor de IA y Chat mediante Claude Headless:** La interacción con el LLM se realizará invocando **Claude en modo headless** (vía subproceso `claude -p ... --output-format stream-json` / `json` o SDK/API directa con soporte de *Tools / MCP*), permitiendo elegir dinámicamente el modelo desde la interfaz.

---

## 2. Stack Tecnológico y Crates de Rust Obligatorios (`Cargo.toml`)

Antigravity deberá estructurar el proyecto en Rust (Edición 2021/2024) empleando los siguientes crates:

* **Runtime Asíncrono:** `tokio` (features: `full`), `futures`, `async-trait`.
* **Motor Lakehouse, Vectorial y Esquemas:**
  * `lancedb`: Almacenamiento columnar en disco `.lance`, índices vectoriales (`IVF-PQ` / `HNSW`) e índice léxico Full-Text Search (`BM25`).
  * `arrow`, `arrow-array`, `arrow-schema`: Definición estricta de esquemas en memoria y disco (equivalente local a *AWS Glue Data Catalog*).
  * `datafusion`: Motor de ejecución SQL analítico y relacional in-process (equivalente local a *Amazon Athena*).
  * `parquet`: Lectura/escritura auxiliar de archivos Parquet.
* **Grafo Ligero e Hidratación Bajo Demanda:**
  * `petgraph`: Para hidratar subgrafos en memoria tras filtrar nodos/aristas por SQL en `DataFusion`.
* **Embeddings Locales / Híbridos:**
  * `fastembed` (u `ort` / ONNX Runtime en Rust): Generación local de vectores de embeddings sin depender de APIs externas de vectorización (ej. `BGM-M3` o `multilingual-e5-large`).
* **Serialización y Validación de Esquemas:**
  * `serde`, `serde_json`, `schemars` (para forzar JSON Schema estricto durante la extracción en la ingesta).
* **Ejecución de Claude Headless y Subprocesos:**
  * `tokio::process::Command` para invocar el binario CLI `claude` en modo headless (`-p`, `--model`, `--output-format stream-json`, `--allowedTools`), o integración MCP local donde el propio binario Rust expone las herramientas del Data Lake.
* **Interfaz de Usuario en Rust:**
  * Arquitectura limpia desacoplada (`core` + `ui`), implementando una interfaz nativa en Rust (mediante **`ratatui` + `crossterm`** para TUI multipanel de alto rendimiento o **`egui` / `eframe`** para GUI de escritorio en un único binario nativo) con dos vistas principales: **1) Gestor de Centros e Ingesta** y **2) Consola de Chat con Selector de Modelo Claude**.

---

## 3. Módulo 1: Gestión Multi-Centro de Conocimiento (Multi-Tenant Lakehouse)

La aplicación debe permitir crear, registrar, activar/desactivar y eliminar múltiples **Centros de Conocimiento** independientes en disco.

### 3.1 Archivo Maestro de Configuración (`~/.config/lakehouse_rag/centers_registry.json`)
```json
{
  "centros": [
    {
      "id": "obra-planta-solar-01",
      "nombre": "Proyecto Planta Solar - Normativa y Procedimientos",
      "ruta_disco": "/datos/centros_conocimiento/obra_solar_01",
      "activo": true,
      "fecha_creacion": "2026-10-16T18:00:00Z",
      "estadisticas": {
        "documentos_procesados": 42,
        "tuplas_operativas": 318,
        "tablas_doble_entrada": 27
      }
    }
  ]
}
```

### 3.2 Estructura Física en Disco de cada Centro de Conocimiento
Cada `ruta_disco` seleccionada por el usuario contendrá la siguiente estructura autónoma:
```text
<ruta_disco_centro>/
├── center_manifest.json              # Metadatos del centro, modelo de embedding y versión del esquema
├── raw_docs/                         # Copia inmutable de los PDFs/DOCX/MD originales + hash SHA-256
├── glue_catalog/                     # Definiciones Arrow Schema exportadas e inventario de tablas para el LLM
└── lakehouse_data/                   # Tablas nativas de LanceDB (.lance)
    ├── 01_documentos_catalogo.lance/
    ├── 02_chunks_normativa.lance/
    ├── 03_matriz_operativa.lance/
    ├── 04_catalogo_tablas.lance/
    ├── 05_celdas_doble_entrada.lance/
    └── 06_grafo_referencias.lance/
```

### 3.3 Comportamiento de Activación / Desactivación
* El usuario puede marcar **uno o varios centros como `ACTIVO`** en tiempo real desde la interfaz.
* Cuando un centro pasa a estado `ACTIVO`, el motor `SessionContext` de **Apache DataFusion** registra automáticamente sus tablas `.lance` bajo un esquema o prefijo de catálogo (o crea una vista unificada `UNION ALL` entre los centros activos).
* Cuando un centro se marca como `INACTIVO`, se desregistra inmediatamente del `SessionContext` de DataFusion, garantizando que el LLM tenga **cero acceso** a esos datos sin borrar nada del disco.

---

## 4. Modelo de Datos Exacto (Esquemas Apache Arrow / LanceDB)

Para resolver los 8 problemas críticos del RAG tradicional (fusión de responsabilidades, tablas de doble entrada, prelación normativa, versiones y grafos), **cada Centro de Conocimiento DEBE inicializar estas 6 tablas con estos esquemas Arrow exactos**:

### Tabla 1: `01_documentos_catalogo.lance` (Control Documental y Prelación)
Controla versiones, vigencia y jerarquía normativa.
* `doc_id` (`Utf8`, PK): Código único del documento (ej. `PO-MAT-02`).
* `titulo` (`Utf8`): Título oficial del documento.
* `tipo_documento` (`Utf8`): Enum lógico (`ESPECIFICACION_CLIENTE`, `PROCEDIMIENTO_OBRA`, `INSTRUCTIVO_TECNICO`, `NORMA_INTERNACIONAL`, `FORMATO_REGISTRO`).
* `nivel_prelacion` (`Int8`): Prioridad normativa (`1` = máxima prioridad / pliego cliente; `5` = norma general supletoria).
* `revision` (`Utf8`): Ej. `Rev. 03`.
* `vigente` (`Boolean`): `true` si es la versión activa; `false` si está derogada/histórica.
* `fecha_vigor` (`Date32`): Fecha de entrada en vigor (permite *Time Travel* y auditorías por fecha).
* `hash_sha256` (`Utf8`): Para evitar re-ingestas duplicadas.

### Tabla 2: `02_chunks_normativa.lance` (Contexto Semántico con Linaje)
* `chunk_id` (`Utf8`, PK)
* `doc_id` (`Utf8`, FK)
* `vigente` (`Boolean`)
* `nivel_prelacion` (`Int8`)
* `breadcrumb_jerarquia` (`Utf8`): Ruta completa de títulos padres (ej. `Cap 4. Ejecución > 4.2 Soldadura > 4.2.1 Condiciones bajo cero`). Evita perder el alcance condicional.
* `condicion_aplicacion` (`Utf8`, Nullable): Condición de contorno explícita extraída en la ingesta (ej. `"Solo aplicable cuando T ambiente < 0 °C"`).
* `texto_contenido` (`Utf8`): Texto íntegro del apartado.
* `tablas_vinculadas` (`List<Utf8>`): Lista de `tabla_id` citadas en este apartado.
* `hitos_vinculados` (`List<Utf8>`): Lista de `hito_id` de la matriz operativa.
* `vector` (`FixedSizeList<Float32, D>`): Embedding semántico del apartado + `breadcrumb_jerarquia`.

### Tabla 3: `03_matriz_operativa.lance` (Anti-Fusión de Fase / Responsable / Registro)
**Tabla crítica** que impide que el LLM mezcle quién hace la *Recepción*, quién la *Custodia* y quién la *Inspección*, o qué registro se llena en cada una.
* `hito_id` (`Utf8`, PK)
* `doc_id` (`Utf8`, FK)
* `vigente` (`Boolean`)
* `orden_secuencia` (`Int32`): Orden cronológico dentro del procedimiento (1, 2, 3...).
* `fase_operativa` (`Utf8`): Categoría normalizada o específica (ej. `RECEPCION`, `DESCARGA`, `CUSTODIA`, `INSPECCION`, `LIBERACION`, `ENSAYO`).
* `objeto_alcance` (`Utf8`): Material, equipo o elemento sobre el que recae la acción (ej. `Tubería Acero al Carbono`, `Hormigón HA-30`).
* `rol_ejecutor` (`Utf8`): Quién ejecuta físicamente la acción (ej. `Almacenero`, `Operario Especialista`).
* `rol_responsable` (`Utf8`): De quién es la responsabilidad formal / quién firma (ej. `Jefe de Almacén`, `Jefe de Producción`, `Inspector QA/QC`).
* `registro_codigo` (`Utf8`): Código exacto del formato/documento a rellenar (ej. `FOR-ALM-01`, `PPI-MAT-09`).
* `registro_nombre` (`Utf8`): Nombre descriptivo del registro.
* `criterio_aceptacion` (`Utf8`): Regla, requisito o acción obligatoria en esta fase.
* `chunk_origen_id` (`Utf8`, FK): Referencia al texto literal.
* `vector_hito` (`FixedSizeList<Float32, D>`): Embedding calculado **exclusivamente** sobre la concatenación limpia de esta fila (`fase_operativa + objeto_alcance + rol_responsable + registro_codigo + criterio_aceptacion`).

### Tabla 4: `04_catalogo_tablas.lance` (El "Glue Catalog" de Tablas de Doble Entrada)
Permite al LLM descubrir qué tablas existen y con qué variables debe consultar el SQL.
* `tabla_id` (`Utf8`, PK): Ej. `TABLA_PO_04_A`.
* `doc_id` (`Utf8`, FK)
* `titulo_tabla` (`Utf8`): Ej. `Espesores de recubrimiento y temperaturas de precalentamiento`.
* `eje_x_nombre` (`Utf8`): Nombre de la variable de entrada 1 (ej. `espesor_nominal_mm`).
* `eje_x_tipo` (`Utf8`): `RANGO_NUMERICO` o `CATEGORIA_EXACTA`.
* `eje_x_unidad` (`Utf8`, Nullable): Ej. `mm`, `°C`, `tm`.
* `eje_y_nombre` (`Utf8`): Nombre de la variable de entrada 2 (ej. `diametro_tuberia_mm` o `tipo_acero`).
* `eje_y_tipo` (`Utf8`): `RANGO_NUMERICO` o `CATEGORIA_EXACTA`.
* `eje_y_unidad` (`Utf8`, Nullable)
* `valor_salida_nombre` (`Utf8`): Qué dato entrega la intersección (ej. `temperatura_minima_c`, `numero_pasadas`).
* `instrucciones_uso_sql` (`Utf8`): Explicación autogenerada en la ingesta de cómo lanzar el `SELECT` sobre esta tabla.
* `vector_tabla` (`FixedSizeList<Float32, D>`): Embedding de la descripción y variables de la tabla.

### Tabla 5: `05_celdas_doble_entrada.lance` (Datos Desnormalizados para Cruce SQL Exacto)
Cada intersección (celda) de una tabla de doble entrada o multivariable se guarda como una fila atómica.
* `celda_id` (`Utf8`, PK)
* `tabla_id` (`Utf8`, FK)
* `doc_id` (`Utf8`, FK)
* `eje_x_categoria` (`Utf8`, Nullable): Valor si el eje X es discreto/texto (ej. `"Acero S355J2"`).
* `eje_x_min` (`Float64`, Nullable): Límite inferior del rango X (si es `> 10`, guardar `10.0`).
* `eje_x_min_inclusivo` (`Boolean`): `true` para `>=`, `false` para `>`.
* `eje_x_max` (`Float64`, Nullable): Límite superior del rango X.
* `eje_x_max_inclusivo` (`Boolean`): `true` para `<=`, `false` para `<`.
* `eje_y_categoria` (`Utf8`, Nullable): Valor si el eje Y es discreto/texto.
* `eje_y_min` (`Float64`, Nullable): Límite inferior del rango Y.
* `eje_y_min_inclusivo` (`Boolean`)
* `eje_y_max` (`Float64`, Nullable): Límite superior del rango Y.
* `eje_y_max_inclusivo` (`Boolean`)
* `eje_z_filtro_adicional` (`Utf8`, Nullable): Para tablas de triple entrada o sub-tablas.
* `valor_resultado_texto` (`Utf8`): Resultado exacto de la celda (ej. `"150 °C"`, `"8 pasadas"`, `"No admisible"`).
* `valor_resultado_num` (`Float64`, Nullable): Valor numérico parseado si aplica.
* `nota_pie_condicion` (`Utf8`, Nullable): **Obligatorio** si la celda tiene llamada a nota al pie `(*)` o `(1)` que altera el criterio.

### Tabla 6: `06_grafo_referencias.lance` (Aristas del GraphRAG Columnar)
Modela las relaciones inter-documentales y de formatos sin usar servidores de grafos externos.
* `edge_id` (`Utf8`, PK)
* `origen_id` (`Utf8`): ID del documento, chunk o hito origen.
* `origen_tipo` (`Utf8`): `DOCUMENTO`, `HITO_OPERATIVO`, `CHUNK`.
* `tipo_relacion` (`Utf8`): `EXIGE_LLENAR_REGISTRO`, `DEROGA_A`, `MODIFICA_APARTADO`, `REMITE_A_TABLA`, `SUBORDINADO_A_NORMA`.
* `destino_id` (`Utf8`): Código del documento/registro/tabla destino (ej. `FOR-ALM-02`, `UNE-EN-1090`).
* `contexto_relacion` (`Utf8`): Explicación breve de por qué están conectados.

---

## 5. Módulo 2: El PIPE de Ingesta (De Documento Técnico a Lakehouse)

Cuando el usuario añade un documento o carpeta al Centro de Conocimiento activo, la aplicación ejecutará el siguiente pipeline en Rust:

1. **Extracción Estructural:**
   * Lectura del documento (PDF, DOCX o Markdown). Si es PDF complejo con tablas, invocar parser estructural (ej. `docling` CLI/subproceso o extractor de tablas a Markdown/HTML conservando cabeceras y notas al pie) respetando la jerarquía de títulos (`H1 > H2 > H3`).
2. **Destilación Semántica con Claude Headless (Offline en Ingesta):**
   * El motor de ingesta en Rust invoca a Claude en modo headless (`claude -p --output-format json --json-schema <SCHEMA>`) pasándole cada sección lógica del documento.
   * **Prompt de extracción estricta:** Se le ordena a Claude devolver un JSON tipado que separe:
     1. Los metadatos de prelación y versión del documento.
     2. La lista de **Tuplas de la Matriz Operativa**, obligándole a crear objetos separados por cada fase (`RECEPCION` vs `CUSTODIA` vs `INSPECCION`) con su `rol_ejecutor`, `rol_responsable` y `registro_codigo`.
     3. La **Desnormalización de Tablas de Doble Entrada**, convirtiendo cada celda visual de la tabla en un objeto con `(eje_x_min, eje_x_max, eje_y_min, eje_y_max, valor_resultado, nota_pie_condicion)`.
     4. Las **Aristas del Grafo** (referencias a otros procedimientos, normas o anexos de registros).
3. **Vectorización y Escritura Atómica en LanceDB:**
   * El código Rust deserializa la respuesta con `serde`, genera los vectores de embeddings con `fastembed` para las columnas vectoriales (`vector`, `vector_hito`, `vector_tabla`), construye los `RecordBatch` de Apache Arrow e inserta los datos de forma transaccional en las 6 tablas `.lance`.
   * Si se sube una nueva revisión (`Rev. 04`) de un documento existente (`Rev. 03`), el pipeline marca automáticamente las filas de la `Rev. 03` con `vigente = false` en todas las tablas antes de insertar la `Rev. 04` con `vigente = true`.

---

## 6. Módulo 3: Motor de Chat con Claude Headless, Selector de Modelos y Tools del Data Lake

El módulo de Chat en Rust no usará un RAG ingenuo de "buscar Top-5 chunks y pegarlos en el prompt". Funcionará como un **Agente Híbrido con acceso al Catálogo de Datos (estilo AWS Glue + Athena)** ejecutado por **Claude Headless**.

### 6.1 Selector Dinámico de Modelos Claude
La interfaz de usuario debe incluir un selector desplegable o conmutador rápido en vivo que permita elegir el modelo que usará el comando headless en cada turno (y también qué modelo usar para la ingesta):
* `claude-sonnet-4-5` / `sonnet` (Por defecto para chat ágil y generación SQL).
* `claude-opus-4-5` / `opus` (Para auditorías complejas, cruces multi-normativa o extracción de tablas difíciles en ingesta).
* `claude-haiku-4-5` / `haiku` (Para clasificación rápida o enrutamiento).
* Soporte para introducir un identificador de modelo personalizado desde configuración.

### 6.2 Integración Técnica con Claude Headless + MCP Local
El binario en Rust puede operar en modo dual (o subcomando `--mcp-server`):
1. Cuando el usuario escribe en el Chat de la app en Rust, la app invoca el proceso `claude` en modo headless:
   ```bash
   claude -p "<pregunta_del_usuario>" \
     --model <modelo_seleccionado> \
     --system-prompt "<prompt_arquitectura_lakehouse>" \
     --mcp-config <ruta_mcp_local_rust.json> \
     --output-format stream-json
   ```
2. El proceso de Claude Headless tiene conectadas **5 Herramientas (Tools)** expuestas directamente por nuestro motor en Rust (`LanceDB` + `DataFusion`) sobre los **Centros de Conocimiento Activos**:

#### Tool 1: `inspect_lakehouse_catalog` (Equivalente a AWS Glue Catalog)
* **Qué hace:** Devuelve el inventario de documentos vigentes en los centros activos, la lista de fases operativas existentes y el catálogo de tablas de doble entrada (`04_catalogo_tablas.lance`) con los nombres exactos de sus ejes X e Y.
* **Cuándo la usa Claude:** Siempre al inicio para conocer el esquema antes de formular consultas.

#### Tool 2: `query_operational_matrix` (Consulta Anti-Fusión de Responsabilidades y Registros)
* **Parámetros:** `fase_operativa` (opcional), `objeto_alcance` (opcional), `rol_responsable` (opcional), `registro_codigo` (opcional), `semantic_query` (texto).
* **Qué hace en Rust:** Ejecuta una búsqueda híbrida o filtro SQL estricto sobre `03_matriz_operativa.lance` (`WHERE vigente = true AND ...`). Devuelve tuplas atómicas aisladas donde jamás se mezcla Recepción con Custodia o Inspección.

#### Tool 3: `lookup_double_entry_table` (Cruce Determinista de Tablas Técnicas)
* **Parámetros:** `tabla_id`, `val_x_num` (opcional), `val_x_cat` (opcional), `val_y_num` (opcional), `val_y_cat` (opcional).
* **Qué hace en Rust:** Ejecuta en `DataFusion` la consulta parametrizada sobre `05_celdas_doble_entrada.lance` verificando los límites `eje_x_min`/`eje_x_max` (respetando si el límite es inclusivo `<=` o estricto `<`) y devuelve el `valor_resultado_texto` junto con su `nota_pie_condicion`.

#### Tool 4: `execute_athena_sql` (Consultas Analíticas y de Auditoría Global)
* **Parámetros:** `sql_query` (String).
* **Qué hace en Rust:** Valida que la consulta sea de solo lectura (`SELECT`) y la ejecuta directamente en el `SessionContext` de **Apache DataFusion** sobre las 6 tablas `.lance` de los centros activos.
* **Cuándo la usa Claude:** Cuando el usuario pide auditorías globales (*"¿Qué registros tiene que firmar el Jefe de Almacén en todos los procedimientos vigentes?"*) o cruces complejos.

#### Tool 5: `traverse_normative_graph` (GraphRAG Columnar + `petgraph`)
* **Parámetros:** `nodo_origen_id`, `max_saltos` (1..3), `filtro_tipo_relacion` (opcional).
* **Qué hace en Rust:** Realiza joins en `DataFusion` sobre `06_grafo_referencias.lance`, hidrata el subgrafo en `petgraph` y devuelve todas las normas superiores, anexos o formatos (FOR/PPI) conectados al procedimiento consultado, ordenados por `nivel_prelacion ASC`.

---

## 7. Reglas del System Prompt del Agente de Chat

El System Prompt inyectado a Claude Headless deberá imponer el siguiente protocolo de respuesta:
1. **Verificación de Centro Activo:** Informar siempre sobre qué Centro(s) de Conocimiento activo(s) se está realizando la consulta.
2. **Cero Cálculos a Ojo en Tablas:** Si la pregunta involucra valores en tablas técnicas o rangos numéricos, **está prohibido responder solo con texto de chunks**; es obligatorio invocar `lookup_double_entry_table` o `execute_athena_sql` y reportar cualquier `nota_pie_condicion`.
3. **Aislamiento de Fases Operativas:** Si se pregunta por responsables o registros de una etapa (Recepción, Custodia, Inspección, etc.), consultar `query_operational_matrix` y responder citando explícitamente la tupla: `[Procedimiento | Fase | Rol Ejecutor | Rol Responsable | Código de Registro]`.
4. **Respeto de Prelación Normativa:** Si dos documentos vigentes entran en conflicto, priorizar siempre el de menor `nivel_prelacion` (ej. Especificación del Cliente pisa a Norma General) y advertirlo en la respuesta.

---

## 8. Estructura de Módulos del Proyecto Rust a Generar por Antigravity

Antigravity deberá organizar el repositorio con la siguiente arquitectura modular:

```text
lakehouse-rag-rs/
├── Cargo.toml
├── src/
│   ├── main.rs                      # Punto de entrada (inicia UI o modo servidor MCP interno)
│   ├── config/
│   │   ├── mod.rs
│   │   └── centers_manager.rs       # Gestión CRUD de Centros de Conocimiento, rutas en disco y estado Activo/Inactivo
│   ├── schema/
│   │   ├── mod.rs
│   │   └── arrow_schemas.rs         # Definición estática de los 6 esquemas Apache Arrow (.lance)
│   ├── ingestion/
│   │   ├── mod.rs
│   │   ├── doc_parser.rs            # Extracción de PDF/MD/DOCX respetando jerarquía y tablas
│   │   ├── claude_extractor.rs      # Invocación a Claude Headless con JSON Schema para extraer tuplas y celdas
│   │   ├── embedder.rs              # Generación de embeddings locales con fastembed
│   │   └── lance_writer.rs          # Escritura transaccional en LanceDB y control de versiones (vigente=true/false)
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── datafusion_ctx.rs        # SessionContext de DataFusion (registro dinámico de centros activos)
│   │   ├── hybrid_search.rs         # Búsqueda combinada Vector (LanceDB) + SQL Filter + BM25
│   │   ├── table_lookup.rs          # Motor determinista para tablas de doble entrada (rangos min/max)
│   │   └── graph_traversal.rs       # Recorrido de aristas sobre 06_grafo_referencias + petgraph
│   ├── agent/
│   │   ├── mod.rs
│   │   ├── claude_headless.rs       # Wrapper asíncrono para CLI `claude` headless con selector de modelos y streaming
│   │   └── mcp_tools.rs             # Servidor/Handler de las 5 herramientas del Data Lake para Claude
│   └── ui/
│       ├── mod.rs
│       ├── app_state.rs             # Estado global (Centro activo, modelo Claude seleccionado, progreso de ingesta)
│       ├── view_centers.rs          # Vista 1: Administración de Centros en disco e Ingesta de documentos
│       └── view_chat.rs             # Vista 2: Chat interactivo con trazabilidad de Tools SQL/Vector y selector de modelo
```

---

## 9. Criterios de Aceptación (Definition of Done para el Plan de Acción)

1. **Prueba de Multi-Centro:** El usuario puede crear el *"Centro Obra A"* en `/tmp/obra_a` y el *"Centro Obra B"* en `/tmp/obra_b`, activar solo *"Obra A"* y verificar en el chat que las consultas SQL de DataFusion solo leen datos de `/tmp/obra_a`.
2. **Prueba Anti-Fusión (Recepción vs Custodia vs Inspección):** Al ingerir un procedimiento que contenga las tres fases, la tabla `03_matriz_operativa.lance` debe contener al menos 3 filas distintas, y al preguntar en el chat por *"¿Quién es el responsable de la custodia y qué registro llena?"*, el sistema debe devolver únicamente el responsable y registro de la fila `CUSTODIA`.
3. **Prueba de Tabla de Doble Entrada:** Al ingerir una tabla con rangos numéricos en X e Y y notas al pie, la consulta en el chat debe invocar `lookup_double_entry_table` o `execute_athena_sql` y devolver el valor exacto del cruce junto con la nota al pie aplicable.
4. **Prueba de Selector Claude Headless:** El usuario puede cambiar en caliente entre `sonnet`, `opus` y `haiku` desde la interfaz del chat en Rust y observar en el log de ejecución cómo el subproceso headless utiliza el flag `--model` correspondiente mostrando en vivo qué herramientas del Data Lake (`inspect_lakehouse_catalog`, `query_operational_matrix`, `lookup_double_entry_table`, etc.) está ejecutando.