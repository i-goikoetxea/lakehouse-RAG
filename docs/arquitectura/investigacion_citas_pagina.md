# Investigación: citas exactas de página y de fragmento

**Fecha:** 2026-11-09. **Estado:** propuesta para decidir; no hay nada implementado.
**Punto de partida:** la [campaña Aguja 5 × 3](../../benchmarks/informes/aguja_campana_20261109.md) da 1,00 en `recall_aguja`, en `pagina_exacta` (que acepta el rango) y en `seccion_real`, pero 0,00 en `pagina_exacta_estricta`.

## La idea en una analogía

Imagina que preguntas a un bibliotecario dónde está una frase. Él encuentra la frase, pero en su ficha solo pone «entre las páginas 52 y 54». Puede decirte el capítulo y el rango, pero no la página, porque nunca apuntó dónde se pasaba la hoja.

Hay dos arreglos:
- **Apuntar el paso de hoja en la ficha.** Al leer, el bibliotecario ve «— pág. 53 —» en medio del texto y sabe en qué página está cada frase.
- **Comprobar después.** El bibliotecario copia la frase literal y un ayudante, con el libro delante, busca en qué página está.

La propuesta combina los dos: el modelo **ve** dónde cambia la página y el código **comprueba** la página que cita.

---

## 1. Inventario del estado actual

### 1.1 Dónde se pierde la página exacta

1. **El parser sí sabe la página de cada línea.**
   - `pdf-extract` saca el texto página a página (`src/infrastructure/parsers/pdf_parser.rs:2`).
   - Cada `LineaPaginada` lleva su página (`src/infrastructure/parsers/pdf_estructura.rs:75`).
   - `saltos_de` convierte esas líneas en `saltos_pagina`, es decir, desplazamiento en bytes → página, por sección (`pdf_estructura.rs:1101`).
   - El DTO `SaltoPagina { desplazamiento, pagina }` está en `src/application/dtos/ingestion.rs:39`, y `ParsedSection::saltos_pagina` en `ingestion.rs:62`.
2. **La fragmentación conserva posiciones.** `fragmentar_con_posiciones` devuelve `inicio` y `fin` en bytes (`src/domain/services/fragmentacion.rs:130`). Usa ventanas de 512 tokens estimados con 64 de solape (`fragmentacion.rs:11-15`).
3. **Aquí se colapsa a un rango.**
   - `paginas_del_fragmento` usa los saltos solo para calcular la primera y la última página del fragmento (`src/application/use_cases/mapeo_destilacion.rs:472-490`).
   - `SeccionIndexable` y `FilaChunk` guardan únicamente `pagina_inicio` y `pagina_fin` (`mapeo_destilacion.rs:114-115` y `538-542`).
   - **Los saltos internos del chunk se descartan.**
4. **El esquema no tiene dónde guardarlos.** La tabla `02_chunks_normativa` solo tiene `pagina_inicio` y `pagina_fin` (`src/infrastructure/storage/schemas.rs:118-119`).
5. **El texto guardado no lleva marca de página.** `texto_contenido` es el tramo de bytes tal cual (`fragmentacion.rs:133-137`). Una vez ingerido, no se puede reconstruir dónde cambia la página.

Conclusión: la información existe durante la ingesta y se tira en el paso 3. Cualquier solución exige **reingerir**, porque los centros actuales no la tienen.

### 1.2 Metadata de cada chunk: qué se guarda y qué ve el modelo

«Llega al modelo» se refiere a la salida de `search_normative_context`, construida en `src/application/use_cases/lakehouse_tool_service/busqueda.rs:259-293`.

| Dato | ¿Se guarda? (dónde) | ¿Llega al modelo en `search_normative_context`? |
| :- | :- | :- |
| `chunk_id` | Sí, clave de 02 (`schemas.rs:109`) | Sí, como `id` (`busqueda.rs:277`) |
| `doc_id` | Sí, columna común (`schemas.rs:71`) | Sí, en `cita.doc_id` (`busqueda.rs:260`) |
| `version_num` | Sí (`schemas.rs:72`) | Sí (`busqueda.rs:262`) |
| **Revisión** (`revision`, texto como «Rev. 2») | Sí, en `01_documentos_catalogo` (`schemas.rs:97`) | **Sí**, en `cita.revision` (`busqueda.rs:261`). Es nula si la cadena está vacía |
| **Título del documento** | Sí, en 01 (`schemas.rs:94`) | **No** en los resultados. Solo aparece en `documentos_no_definitivos`, y solo para revisiones no definitivas (`busqueda.rs:318-321`). Sí llega en la cabecera de las tools STAIR (`estructura.rs:480`) |
| `tipo_documento` | Sí, en 01 (`schemas.rs:95`) | No |
| `fecha_vigor` / `fecha_derogacion` | Sí, en 01 (`schemas.rs:98-99`) | **No** como fecha. Solo llegan el indicador `vigencia_no_declarada` (`busqueda.rs:290`) y `fecha_referencia` del contexto (`busqueda.rs:485`) |
| `hash_sha256`, `ingest_id`, `estado_ingesta`, `vigente` | Sí (`schemas.rs:73-76` y `100`) | No (no hace falta para citar) |
| `nodo_id` | Sí (`schemas.rs:111`) | Sí, convertido a sección real con `seccion_real_de` (`busqueda.rs:265`) |
| Breadcrumb | Sí (`schemas.rs:115`) | Sí, dos veces: `breadcrumb_jerarquia` y `cita.breadcrumb` (`busqueda.rs:264` y `280`) |
| `condicion_aplicacion` | Sí (`schemas.rs:116`) | Sí (`busqueda.rs:281`) |
| `tipo_bloque`, `estado_contenido`, `nivel_prelacion` | Sí (`schemas.rs:112-114`) | Sí (`busqueda.rs:282` y `286-287`) |
| `pagina_inicio`, `pagina_fin` | Sí (`schemas.rs:118-119`) | Sí, junto con `paginas` legible («52-54») (`estructura.rs:205-219`, `busqueda.rs:267-275`) |
| **Saltos de página dentro del chunk** | **No** (se descartan en `mapeo_destilacion.rs:472-490`) | **No** |
| Página impresa (etiqueta del pie, `/PageLabels`) | **No**: la app usa la página física, empezando en 1 | No |
| Desplazamiento del chunk dentro de la sección | No (se calcula y se descarta) | No |
| `tablas_vinculadas`, `hitos_vinculados` | Sí (`schemas.rs:120-121`) | No en la búsqueda |
| Vector | Sí (`schemas.rs:122`) | No (no aplica) |

Notas:
- **Proyección.** El índice semántico en memoria (`FragmentoIndexado`, `src/application/ports/semantic_index.rs:29-53`) copia de 02 solo lo que la búsqueda necesita. No incluye título ni fechas de 01 (`src/infrastructure/query_engine/lakehouse_projection.rs:193-213`).
- **Otras vías.** El modelo puede llegar a todo lo de 01 con `inspect_lakehouse_catalog` o con SQL. La descripción de `execute_athena_sql` enumera las columnas de 02 (`src/application/dtos/tools.rs:313`). Pero eso cuesta otra llamada.
- **`stair_get_subtree_context`.** Concatena chunks y da el rango de páginas de **toda la sección** (`estructura.rs:602-656` y `702`), que es aún más grueso que el del chunk.

**Sobre lo que cree el usuario:**
- La **revisión sí llega** al modelo.
- El **nombre (título) del documento no llega** en la búsqueda.
- Tampoco llegan las **fechas de vigor y derogación**.
- Lo que de verdad impide la página exacta no es nada de lo anterior: son los **saltos de página internos del chunk**, que ni se guardan.

### 1.3 Un efecto lateral del arnés

El arnés exige que toda cifra de la respuesta aparezca en una salida de tool del turno. Una página tomada del historial se rechaza (`docs/arquitectura/ssd_arnes_determinista.md:177` y `204`).

Si un chunk trae `"paginas": "52-54"`, las cifras disponibles son 52 y 54. **Según esa regla, si el modelo dedujera bien que el dato está en la 53, el arnés no la vería anclada.** Es una inferencia de la especificación, no medida: la campaña no tuvo intervenciones porque el modelo siempre citó el rango.

Cualquier solución debe poner en la salida de la tool el número de página concreto, no solo los extremos.

---

## 2. Investigación externa

Las fuentes se leyeron el 2026-11-09, solo como páginas web. Las cifras se copian tal como aparecen. Cuando una fuente no da cifras, se dice.

### 2.1 Citas a nivel de frase o de span, y cómo se evalúan

| Fuente | Qué afirma | Evidencia |
| :- | :- | :- |
| Rashkin et al., *Measuring Attribution in NLG Models* (AIS). [arXiv 2112.12870](https://arxiv.org/abs/2112.12870), 2021, rev. 2022 | Define «Attributable to Identified Sources»: un texto es atribuible si una persona puede verificarlo en la fuente citada | Anotación humana en dos etapas. El resumen no da cifras |
| Gao et al., **ALCE**, *Enabling LLMs to Generate Text with Citations*. [arXiv 2305.14627](https://arxiv.org/abs/2305.14627), EMNLP 2023 | Mide la cita automáticamente con un NLI (TRUE, T5-11B). **Recall:** el pasaje citado implica el enunciado. **Precisión:** cada cita es necesaria | «even the best models lack complete citation support 50% of the time» en ELI5. ChatGPT con 5 pasajes, recall/precisión de cita: ASQA 73,6/72,5; ELI5 51,1/50,0 |
| Zhang et al., **LongCite**. [arXiv 2409.02897](https://arxiv.org/abs/2409.02897), sept. 2024 | Las citas a nivel de frase localizan mejor la evidencia que las de chunk. El contexto se parte en frases numeradas (`<C i>`) y el modelo cita los números | Citation F1 en LongBench-Cite: GPT-4o 65,6 (cita media de 220 tokens), Claude-3-sonnet 67,2 (132), LongCite-8B 72,0 (85). Ablación con GLM-4: frase post-hoc 65,8 frente a chunk de 128 tokens post-hoc 63,8 y chunk en una pasada 51,6 |
| Huang et al., **FRONT**. [ACL Findings 2024](https://aclanthology.org/2024.findings-acl.838) | Primero se extraen citas literales y luego se genera guiado por ellas | +14,21 % de calidad de cita de media en ALCE con LLaMA-2-7B. La cifra viene del resumen y de fichas secundarias; no se comprobó en el PDF |
| Li et al., **AttributionBench**. [arXiv 2402.15089](https://arxiv.org/abs/2402.15089), feb. 2024 | Verificar la atribución automáticamente es difícil | Incluso GPT-3.5 ajustado se queda «around 80% macro-F1» |
| Buchmann et al., *Attribute or Abstain*. [arXiv 2407.07799](https://arxiv.org/abs/2407.07799), EMNLP 2024 | En documentos largos, citar en el mismo paso que se genera funciona mejor en modelos grandes. No observan «lost in the middle» en la atribución | 6 tareas y 5 LLM. El resumen no da cifras |
| Ma et al., **CiteVQA**. [arXiv 2605.12882](https://arxiv.org/abs/2605.12882), mayo 2026 | Es el caso más parecido al nuestro: PDF de 40,6 páginas de media y cita por elemento. Los modelos aciertan la respuesta y fallan la fuente | Métrica SAA (respuesta y cita correctas a la vez): el mejor, Gemini-3.1-Pro-Preview, saca 76,0, y el mejor modelo abierto, 22,5. Según la prensa ([The Decoder](https://the-decoder.com/ai-models-often-give-the-right-answers-but-point-to-the-wrong-sources/), 25-5-2026), GPT-5.4 baja de 87,1 a 59 cuando se exige la cita. Esas dos cifras no se comprobaron en el paper |

### 2.2 Inline frente a post-hoc: ¿marcar o resolver después?

| Fuente | Qué afirma | Evidencia |
| :- | :- | :- |
| ALCE (la misma de 2.1), variante PostCite | Generar sin fuentes y asignar la cita después rinde mucho peor | ASQA 26,7/26,7 frente a unos 73 de la cita en línea. No es una comparación controlada: la respuesta *closed book* no se parece a los pasajes |
| LongCite, tabla 4 | Extraer después las frases que apoyan una respuesta **ya basada en el contexto** funciona bien a nivel de frase | 65,8 a nivel de frase frente a 63,8 por chunk |
| Gao et al., **RARR**. [arXiv 2210.08726](https://arxiv.org/abs/2210.08726), ACL 2023 | Atribuye y revisa después de generar | El resumen solo dice «significantly improves attribution», sin cifras |
| Slobodkin et al., *Attribute First, then Generate*. [arXiv 2403.17104](https://arxiv.org/abs/2403.17104), ACL 2024 | Si se eligen primero los spans, la atribución queda fijada por construcción. Las citas son más concisas y cuesta menos verificarlas | El resumen no da cifras |
| Maheshwari et al., **CiteFix**. [arXiv 2504.15629](https://arxiv.org/abs/2504.15629), ACL 2025 Industry | Un posprocesado ligero, léxico o híbrido, reasigna cada afirmación a su documento | +15,46 % relativo de precisión global del RAG. El mejor método depende del LLM. No se revisaron las tablas por modelo |

No se encontró ningún estudio que compare marcar en línea y resolver después **midiendo la página** como unidad.

### 2.3 Citations de la API de Anthropic

- **Documentación oficial:** <https://platform.claude.com/docs/en/build-with-claude/citations>
  - Los PDF y el texto plano se parten automáticamente en frases. En los documentos *custom content* «no further chunking is done», así que el integrador controla la granularidad.
  - **Tipos de ubicación:**
    - `page_location`: `start_page_number` empieza en 1 y `end_page_number` es exclusivo.
    - `char_location`: índices de carácter que empiezan en 0, con fin exclusivo.
    - `content_block_location`: índices de bloque.
  - El `cited_text` lo **extrae la API** del documento; no lo escribe el modelo. Por eso las citas son «guaranteed to contain valid pointers». Lo que no se garantiza es que haya una cita.
  - `cited_text` no cuenta como tokens de salida. Las citas no se pueden combinar con *structured output*: según el resumen de búsqueda, devuelve un error 400; no se vio en el texto de la página.
- **Anuncio:** <https://claude.com/blog/introducing-citations-api>. La página muestra el 23-6-2025, con una actualización del 30-6-2025.
  - Evaluación interna: «increasing recall accuracy by up to 15%».
  - Cliente Endex: alucinaciones de fuente y errores de formato del 10 % al 0 %, y un 20 % más de referencias por respuesta.
- **¿Se puede usar aquí? No directamente.**
  - La documentación de Claude Code headless (<https://code.claude.com/docs/en/headless>) no menciona citations ni bloques `document`. `claude -p` solo ofrece `--output-format text|json|stream-json`.
  - Además, la app pasa el contexto como **salida de tools MCP** (JSON), no como bloques `document`.
  - **Es una inspiración, no una dependencia.** La idea a copiar: el modelo señala un texto literal y el **sistema**, no el modelo, resuelve dónde está.

### 2.4 Chunking con metadata de ubicación

| Patrón y fuente | Efecto en la recuperación | Efecto en la cita |
| :- | :- | :- |
| **Chunk por página.** NVIDIA, *Finding the Best Chunking Strategy* ([blog](https://developer.nvidia.com/blog/finding-the-best-chunking-strategy-for-accurate-ai-responses/), 18-6-2025) | El mejor de media: 0,648 de precisión de respuesta, con la menor desviación (0,107). Por tokens queda entre 0,603 y 0,645. En 3 de los 5 datasets lo supera algún tamaño fijo, por ejemplo FinanceBench 0,579 con 1.024 tokens frente a 0,566 | No mide la cita. La página queda implícita porque cada chunk tiene una sola |
| **Tamaño del chunk.** Chroma, *Evaluating Chunking Strategies* ([informe](https://www.trychroma.com/research/evaluating-chunking), 3-7-2024) | Con chunks más pequeños el recall se mantiene (200 tokens sin solape, 88,1; 800/400, 85,4) y suben mucho la precisión y el IoU (7,0 frente a 1,5) | Indirecto: cuanto menor el chunk, más acotado el origen |
| **Tamaño según la tarea.** Bhat et al. ([arXiv 2505.21700](https://arxiv.org/abs/2505.21700), 2025) | 64–128 tokens para respuestas factuales cortas y 512–1.024 si hace falta contexto. Depende del modelo de embeddings | No mide la cita |
| **Ventana de frase.** LlamaIndex `SentenceWindowNodeParser` ([docs](https://developers.llamaindex.ai/python/framework/module_guides/loading/node_parsers/modules/)) | Indexa frases y entrega al LLM la ventana que las rodea. La documentación no da cifras. ARAGOG ([arXiv 2404.01037](https://arxiv.org/abs/2404.01037)) la sitúa como la mejor en precisión de recuperación, pero el resumen no da cifras | Cada frase podría llevar su página |
| **Padre-hijo.** LlamaIndex `HierarchicalNodeParser` + `AutoMergingRetriever` | Recupera hijos pequeños y sube al padre si coinciden varios. La documentación no da cifras | El hijo da la ubicación fina y el padre, el contexto |
| **Metadata de nodo** (LlamaIndex) | Por defecto la metadata entra en el texto del embedding y en el del LLM. `excluded_embed_metadata_keys` y `excluded_llm_metadata_keys` la separan | **Patrón clave:** decidir por separado qué ve el embedding y qué ve el LLM |
| **Loaders con página.** LangChain `PyPDFLoader` ([referencia v0.3](https://reference.langchain.com/v0.3/python/community/document_loaders/langchain_community.document_loaders.pdf.PyPDFLoader.html)) | En modo `page` hay un documento por página, con `page` (física, desde 0) y `page_label` (impresa). En modo `single` hay un solo documento con delimitador entre páginas, y **se pierde la página** | Distingue la página física de la impresa |
| **Contextual Retrieval.** Anthropic ([artículo](https://www.anthropic.com/news/contextual-retrieval), 19-9-2024) | La tasa de fallo con top-20 baja del 5,7 % al 3,7 % con contextual embeddings (−35 %), al 2,9 % añadiendo BM25 contextual (−49 %) y al 1,9 % con reranking (−67 %) | No trata la cita. Mejora el recall de chunks pequeños, que pierden contexto |
| **Late chunking.** Günther et al. ([arXiv 2409.04701](https://arxiv.org/abs/2409.04701), 2024–2025) | nDCG@10 +3,63 % relativo de media con límites de frase. Ayuda sobre todo con chunks pequeños y aporta poco en Needle y Passkey | No trata la cita |

**Lo que no se encontró:** ningún benchmark mide el acierto de la página exacta en la cita en función de la estrategia de chunking. Las fuentes miden la recuperación o la respuesta.

### 2.5 Extractores que conservan la ubicación

| Herramienta | Qué ubicación da |
| :- | :- |
| **Docling** ([docs](https://docling-project.github.io/docling/reference/docling_document/)) | Cada elemento lleva `prov` con `page_no`, `bbox` y `charspan`. El `HybridChunker` fusiona elementos vecinos, que conservan su `prov` |
| **Unstructured** ([docs](https://docs.unstructured.io/open-source/concepts/document-elements)) | `page_number` y `coordinates` por elemento. Sus chunks guardan `orig_elements`. Con `multipage_sections=False`, un chunk no cruza de página |
| **LlamaParse** ([docs](https://developers.llamaindex.ai/llamaparse/parse/features/layout-and-bounding-boxes/)) | Bbox por elemento, línea o palabra, y `printed_page_number`, que la documentación recomienda para citar. Es un servicio en la nube |
| **PyMuPDF** ([docs](https://pymupdf.readthedocs.io/en/latest/app1.html)) | `get_text("words"/"blocks"/"rawdict")` da el bbox por palabra, bloque o carácter |
| **Marker** ([repo](https://github.com/datalab-to/marker)) | Bloques con polígono y página. `paginate_output` inserta un separador `{N}` entre páginas, es decir, marcas en el texto |
| **Rust: `pdf-extract` 0.12**, ya en uso | Texto por página. El trait `OutputDev` recibe `begin_page` y la posición de cada carácter |
| **Rust: `lopdf`** | Texto por página. No documenta coordenadas |
| **Rust: `pdfium-render`** | Caracteres con sus límites. Necesita la biblioteca nativa Pdfium |
| **Rust: `mupdf-rs`** | Bloques, líneas y caracteres. Licencia AGPL-3.0 |

Todas las herramientas serias conservan la ubicación **por elemento** y la dejan viajar junto al chunk. En Rust, la página por línea ya la tenemos con `pdf-extract`. El bbox sería posible, pero no hace falta para citar la página.

---

## 3. Comparación de opciones

**Supuestos comunes:**
- Todas las opciones exigen reingerir, porque los saltos no se guardan hoy (§1.1).
- En Markdown no hay páginas: `saltos_pagina` está vacío y `pagina_*` es nulo (`ingestion.rs:55-62`).

| Criterio | **A.** Columna de saltos por chunk | **B.** Marcas `[p. N]` en el texto que ve el modelo | **C.** Chunks que no cruzan página | **D.** Frase o ventana con página por frase | **E.** Tool o paso determinista: cita literal → página |
| :- | :- | :- | :- | :- | :- |
| **Exactitud de página esperada** | Ninguna por sí sola: el modelo no ve dónde cae cada byte. Es la **materia prima** de B y E | Alta, si el modelo copia bien el número más cercano antes del dato. Depende del modelo. LongCite muestra que los modelos citan bien IDs en línea, pero no al 100 % | Total por construcción: el chunk tiene una sola página | Alta: la página va con la frase | **Exacta y verificable** cuando la cita literal aparece una sola vez en el chunk. Inspirada en el `cited_text` de Anthropic |
| **Impacto en el recall (hoy 1,00)** | Ninguno | Ninguno si las marcas **solo se añaden a la salida** y no entran en el vector ni en BM25 (`busqueda.rs:81-88` indexa `texto`) | Riesgo: los párrafos y las tablas partidos por la página pierden contexto. NVIDIA da por página la mejor media (0,648), pero no gana en todos los datasets | Riesgo alto: cambia toda la recuperación. ARAGOG la ve fuerte, pero sin cifras verificadas aquí | Ninguno |
| **Coste de ingesta** | Mínimo: el dato ya se calcula (`mapeo_destilacion.rs:472-490`) | Ninguno extra, si se generan a partir de A | Moderado: más chunks y más vectores | Alto: un vector por frase, o de 5 a 10 veces más filas | Ninguno extra, si usa A |
| **Coste de tokens** | Ninguno, si no se expone | Bajo: de 1 a 3 marcas por chunk (unos 5 tokens cada una) | Similar o algo mayor: más resultados por respuesta | Similar, con ventanas de tamaño parecido | Una llamada más solo cuando se cita, o ninguna si lo hace el arnés |
| **Cambio de esquema** | Una columna nueva en 02 (lista de pares) → esquema v6 y recrear centros | Ninguno, si se deriva de A al responder. Si se guardan las marcas dentro de `texto_contenido`, no cambia el esquema, pero se contaminan el vector y BM25 | Ninguno en 02, pero cambia la fragmentación y, con ella, la decisión del umbral de 512 (`decisiones_stair.md` §10) | Tabla o columnas nuevas, y otro modelo de recuperación | Ninguno más allá de A |
| **Encaje hexagonal y arnés** | Bueno: el dominio ya tiene `SaltoPagina`. Infraestructura solo serializa | Bueno: es presentación de la tool, en aplicación. **Resuelve el anclaje del arnés (§1.3)**, porque el número sale en la salida | Neutro | Costoso: toca dominio, aplicación e infraestructura | Muy bueno: es el mismo espíritu del arnés (verificar, no confiar), y puede vivir como regla del guard posterior a la inferencia |
| **Markdown** | Lista vacía | Sin marcas: se cita la sección | No aplica | Funciona, sin página | Puede devolver la sección y el desplazamiento de carácter en lugar de la página |
| **Riesgos** | Ninguno propio | Que el modelo copie el número equivocado. Marcas confundidas con texto del documento: usar un formato inequívoco y documentarlo en la `nota` | Peor recall en preguntas que cruzan página. Rompe la equivalencia con la campaña actual | Mucho trabajo, recall en riesgo y beneficio no medido para la página | La cita literal puede no coincidir exactamente (espacios, guiones partidos, ligaduras). Hace falta normalizar y, si falla, devolver el rango. Un texto repetido en dos páginas es ambiguo y debe decirse así |

**Otras opciones vistas:**
- **F.** Citations nativas de la API: no aplica con `claude -p` (§2.3).
- **G.** Re-parsear el PDF original al citar: exige conservar el fichero y rehacer la extracción en tiempo de consulta. Es más frágil que guardar los saltos.
- **H.** Ventana de frase al estilo LlamaIndex, pero solo para la cita: es D en pequeño. No mejora a A+B+E.

---

## 4. Recomendación

**A + B + E por fases, sin tocar la fragmentación ni la recuperación.**

1. **Fase 1. A + B, guardar y mostrar.**
   - **Guardar.** Persistir en `02_chunks_normativa` los saltos de página relativos al chunk. Por ejemplo, una columna `saltos_pagina` de tipo lista de `{desplazamiento, pagina}`, nula en Markdown.
     - Supone esquema v6 y recrear centros, como ya se hizo en STAIR (sin migración).
     - Los datos salen de `paginas_del_fragmento`, que ya los recorre.
   - **Mostrar.** Al construir la salida de `search_normative_context`, intercalar marcas de página en `texto` **solo en la respuesta**: una al inicio (`[p. 52]`) y otra en cada salto (`[p. 53]`). No se guardan en `texto_contenido`, así que el vector y BM25 no cambian, y el recall de 1,00 no debería moverse.
     - Hacer lo mismo en `stair_get_subtree_context`.
     - Cambiar la `nota` de la tool (`busqueda.rs:492`): «cita la página de la marca [p. N] que precede al dato; si el dato cruza un salto, cita las dos».
   - Con esto, el número de página concreto aparece en la salida de una tool y **el arnés lo puede anclar** (§1.3).
2. **Fase 2. E, comprobar.** Una regla determinista en el arnés: si la respuesta trae una cita con página y un extracto literal, se busca el extracto, normalizado, en los chunks del turno y se confirma su página con la columna de A.
   - Si no coincide, se corrige al rango o se marca como no verificada.
   - Es la idea del `cited_text` de Anthropic y de CiteFix, pero hecha por nuestro código.
   - Es opcional: decidir tras medir la fase 1.
3. **Descartar por ahora C y D.** Cambian la recuperación, que ya da 1,00, a cambio de un beneficio en la página que A+B consigue sin tocarla. La evidencia externa sobre chunk por página es de precisión de respuesta, no de cita, y no es concluyente (NVIDIA: gana de media, pero pierde en 3 de los 5 datasets frente a algún tamaño fijo).

**Metadata que conviene exponer o añadir:**
- **Añadir:** los saltos de página por chunk, con la fase 1.
- **Exponer:** el **título del documento** en `cita`. Ya está en 01 (`schemas.rs:94`) y en `RevisionDocumental`, que la búsqueda carga (`busqueda.rs:388`). Basta con añadirlo a `DatosRevision`. Así el modelo cita «Título (doc_id, Rev. X), sección, p. N» sin otra llamada.
- **Exponer (opcional):** `fecha_vigor`, cuando exista, junto a `vigencia_no_declarada`. Nunca inventarla (regla STAIR).
- **Más adelante:** la **página impresa** (`/PageLabels` de lopdf). La app usa la física, y un usuario que busca «p. 12» en el PDF impreso puede ver otro número. Como mínimo, documentar en la `nota` que la página es la física del fichero.

## 5. Cómo validarlo

1. **Pista Aguja actual.**
   - Tras la fase 1, repetir la campaña 5 × 3.
   - **Éxito:** `pagina_exacta_estricta` sube de 0,00 a 1,00 (o cerca), y `recall_aguja`, `pagina_exacta` y `seccion_real` siguen en 1,00.
   - Añadir el conteo de intervenciones del arnés por página no anclada (debe ser 0).
2. **Dos agujas casi iguales (precisión).**
   - Insertar dos frases con el mismo sujeto y distinto lote o valor, en **páginas consecutivas que caigan en el mismo chunk**: el caso que hoy es indistinguible.
   - Preguntar por cada una.
   - **Métrica conjunta (a la manera de la SAA de CiteVQA):** acierta solo si el valor **y** la página son los de esa aguja.
   - Variante: una aguja que **cruce** el salto de página. La cita correcta son las dos páginas.
3. **Fase 2 (E).** Test adversarial con extractos alterados (espacios, guiones partidos, comillas tipográficas) y con un extracto repetido en dos páginas: debe responder «ambiguo», no elegir una.
4. **Markdown.** Comprobar que no aparece ninguna marca y que la cita sigue siendo la sección.
5. **Coste.** Medir los tokens medios de la salida de `search_normative_context` antes y después (se espera un aumento inferior al 1 %).

## 6. Límites de esta investigación

- No hay evidencia publicada que mida la **página exacta** según la estrategia: las cifras externas miden recall, precisión de respuesta o calidad de cita a nivel de frase.
- La previsión de que B lleve `pagina_exacta_estricta` cerca de 1,00 es un supuesto, no una medida. Lo decidirá la campaña.
- Algunas cifras vienen de resúmenes o de prensa y se marcan así en las tablas (FRONT, CiteFix, cifras de prensa de CiteVQA, error 400 de Citations).
- No se ha ejecutado nada ni se ha ingerido ningún documento.
