# Decisiones STAIR: contrato de las fases 1 a 7

Este documento es el contrato vigente del proyecto STAIR. Si contradice a la especificación de partida (`2 - adecuacion e implementacion de STAIR - especificaci_n_claude_code_worktree_stair_rag_dual.md`), prevalece este documento.

**Objetivo:** que la aplicación funcione como RAG general sin perder texto y añada navegación por índice (ToC) sobre los motores de precisión actuales. No hay fine-tuning ni nada preparado para ello.

**Fases:** 1 esquemas · 2 ingesta universal · 3 PDF · 4 búsqueda y tools · 5 aceptación · 6 calidad de datos · 7 hitos, roles e índice largo.

## 1. Decisiones consensuadas

### D1. Dos estados independientes
- `estado_ingesta` sigue siendo solo transaccional: `STAGED` o `COMMITTED`. No se toca.
- `estado_contenido` es una columna nueva de `02` y `07`: `COMPLETO`, `PARCIAL_UNIVERSAL` o `REQUIERE_OCR`.
- Donde la especificación escribe `estado_ingesta = "PARCIAL_UNIVERSAL"` o `"REQUIERE_OCR"`, léase `estado_contenido`. Un valor de contenido en `estado_ingesta` dejaría la fila invisible para las lecturas normales.

### D2. Vector de chunks sin cambios
`texto_vectorizable_chunk` (`mapeo_destilacion.rs`) no cambia: `breadcrumb + "Condición: ..." + texto`, omitiendo la condición si no existe. La fórmula y el fragmento de código de la especificación quedan descartados.

### D3. `nodo_id` une 02 y 07
- `nodo_id` es `Utf8` no nulo en `02`. Los sub-chunks de una sección comparten `nodo_id` y se diferencian por `chunk_id`.
- 02 y 07 se unen por `nodo_id`, **nunca por el breadcrumb**.
- **Formato:** `<doc_id>#sec-<segmento>`, y `<doc_id>#sec-preambulo` para el preámbulo. Solo admite `[A-Za-z0-9._-]` más el `#` separador. `NodoId` lo valida en el dominio.
- **La clave única es `(centro_id, doc_id, version_num, nodo_id)`.** A diferencia de `chunk_id`, el `nodo_id` no lleva versión: la misma sección conserva su identificador entre revisiones. Por eso toda unión debe incluir también `centro_id`, `doc_id` y `version_num`, o mezclará revisiones.

### D4. Identidad del documento
- **Destilación correcta:** `doc_id` canónico como hoy (por ejemplo, `PO-ALM-03`). La derogación entre revisiones y el grafo dependen de ello.
- **Claude falla:** `doc_id` provisional = slug del archivo + 6 hex del SHA-256, con `PARCIAL_UNIVERSAL`.
- **Reintento:** en una transacción reescribe `02` y `07` al canónico reutilizando vectores, escribe la Capa 2 y retira la versión provisional. Si Claude no da un código válido, se mantiene el provisional.
- **`doc_id_provisional`** (`Utf8` nulable) está en `01_documentos_catalogo`; no existe ninguna tabla `01_documentos_metadata`. Valores:
  - `NULL`: el documento nunca fue provisional.
  - Igual a `doc_id`: el documento sigue siendo provisional.
  - Distinto de `doc_id`: fila canónica que sustituyó a ese provisional (trazabilidad).

### D5. Tabla `07_indice_jerarquico`
Lleva auditoría completa: `centro_id`, `version_num`, `vigente`, `ingest_id`, `estado_ingesta`, `estado_contenido`. Además: `nodo_id`, `doc_id`, `nivel_jerarquico` (0 = preámbulo), `codigo_seccion`, `titulo_seccion`, `nodo_padre_id` (nulable), `ruta_linaje`, `condiciones_alcance` (nulable), `resumen_seccion`, `vector_seccion`.

No se crea `00_corpus_universal`: `02` evoluciona a corpus sin pérdidas.

### D6. `tipo_bloque` en 02
Valores: `PREAMBULO`, `NARRATIVA`, `DEFINICION`, `ALCANCE`, `PROCEDIMIENTO`.

## 2. Decisiones de la especificación que siguen vigentes

| Tema | Decisión |
| :--- | :--- |
| Búsqueda | Índice híbrido en memoria (coseno + BM25 + RRF), sin índices nativos de LanceDB (IVF-PQ/FTS). |
| Resumen ToC | Extractivo y determinista: título + 2 primeras frases, máximo 250 caracteres. Sin LLM. |
| Chunking | Una sección de hasta 600 tokens es un único chunk. Por encima, ventana de 512 tokens con solapamiento de 64. |
| Preámbulo | `nodo_id = <doc_id>#sec-preambulo`, `codigo_seccion = "0.0"`, `titulo_seccion = "Preámbulo / Introducción"`, `ruta_linaje = "/0"`, nivel 0. |
| Vector de sección | Embedding de `titulo_seccion + " " + resumen_seccion`. |
| PDF | `lopdf` (`/Outlines`) y `pdf-extract` (texto); detalle en §5. Sin OCR: menos de 50 caracteres por página marca `REQUIERE_OCR`. |
| Hash del provisional | 6 hex del SHA-256. El ejemplo `_A4F2` de la especificación, de 4 hex, queda descartado. |
| E2E | `tests/e2e_rag_dual_validation.rs`, no `tests/e2e/rag_dual_validation.rs`: Cargo solo descubre suites en la raíz de `tests/`. |
| Tools | `search_normative_context` añade `modo`, `nodo_id` y `ruta_linaje_prefijo` (`doc_id` ya existía). Nuevas: `stair_navigate_toc` y `stair_get_subtree_context`. |

## 3. Contratos de la Fase 1

### Versión de esquema y limitación sin migración
- `VERSION_ESQUEMA_CENTRO = 5` desde la Fase 7 (§9). `schemas::VERSION_ESQUEMA` se deriva de ella.
- **No hay migración.** Un centro con `schema_version` distinta se rechaza al abrirlo, con un mensaje que indica crear un centro nuevo y reingerir los documentos. Sus datos en disco no se tocan.

### Tipos
- **Dominio** (`domain/models/estructura_documental.rs`): `EstadoContenido`, `TipoBloque`, `NodoId`, `NivelJerarquico`.
- **`NivelJerarquico` admite 0 a 6:** la especificación cita de 0 a 3, pero Markdown tiene hasta H6 y limitar a H3 impediría indexar sin pérdidas.
- **Filas:** `FilaChunk` añade `nodo_id`, `tipo_bloque` y `estado_contenido`. `FilaCatalogo` añade `doc_id_provisional: Option<DocId>`. `FilaIndice` y `LoteIngesta::Indice` son nuevos.
- **`FragmentoIndexado`** añade `nodo_id`, `tipo_bloque` y `estado_contenido`, todos `Option`. Los hitos heredan el `nodo_id` de su chunk de origen, sin los otros dos.

### Garantías de la tabla 07
Está en `TODAS_LAS_TABLAS` y `TABLAS_DEPENDIENTES`, así que entra en staged, commit, rollback, retirada de vigencia, `verificar_integridad`, lecturas solo `COMMITTED`, proyección y time travel. `vector_seccion` no se proyecta a DataFusion.

### Validación al escribir (`validar_estructura_stair`, en `escribir_staged`)
Se rechaza la escritura completa si:
- un `nodo_id` o `nodo_padre_id` no pertenece al `doc_id` de la fila;
- hay un `nodo_id` duplicado en `07`;
- un nodo apunta a un padre inexistente, o a un padre de nivel igual o mayor (esto también excluye los ciclos);
- `ruta_linaje` no empieza por `/`;
- la escritura incluye `07` y algún chunk apunta a un nodo que no está en ella.

**Requisito para la Fase 2:** 02 y 07 deben ir en la misma llamada a `escribir_staged`.

### Diagnóstico (`verificar_integridad`)
- `nodos_huerfanos`: chunks cuyo `nodo_id` no existe en `07`, y nodos cuyo padre no existe, siempre dentro de la misma revisión.
- `valores_invalidos`: valores fuera de vocabulario en `estado_ingesta`, `estado_contenido`, `tipo_bloque` y `doc_id_provisional`.
- Un nodo de `07` sin chunks **no** es huérfano: un título puede no tener texto propio.

### Confirmación con fallo
Si la activación falla a medias (por ejemplo, en `07`), `confirmar_ingesta` devuelve lo activado a `STAGED` y no vigente antes de devolver el error. Así las lecturas no ven una revisión parcial. Después, `revertir_ingesta` limpia las filas, y puede reintentarse si también falló.

## 4. Contratos de la Fase 2

### Flujo
Se destila **antes** de escribir, porque el `doc_id` canónico depende de Claude, y todo va en una sola transacción.

| Resultado de la etapa 2 | `doc_id` | `estado_contenido` | Capa 2 | `doc_id_provisional` |
| :--- | :--- | :--- | :--- | :--- |
| Correcta | Canónico | `COMPLETO` | Sí | `NULL` |
| Claude falla o agota el tiempo, JSON inválido, código sin alfanuméricos, versión ilegible o Capa 2 no mapeable | `SLUG_6hex` | `PARCIAL_UNIVERSAL` | No | Igual a `doc_id` |

- Un fallo de embeddings o de disco **sí** es error y revierte. `VersionConflict` de un código válido también.
- El informe añade `estado_contenido`, `pendiente_destilacion`, `avisos` y `total_nodos`. CLI y GUI muestran los avisos.

### Metadatos de respaldo del provisional
- **Tipo:** `FORMATO_REGISTRO`, de prelación 5, la más baja: un documento sin validar no debe imponerse a uno destilado. La especificación no fijaba el valor.
- **Revisión:** `Provisional`. **Versión:** 1. **Fecha de vigor:** nula desde la Fase 6 (§8).
- **Título:** el del documento parseado.
- **Slug:** nombre sin extensión en `codigo_canonico`, con `.` como `-`, hasta 64 caracteres; `DOC` si no queda ningún alfanumérico.

### Árbol y corpus
- 07 tiene una fila por sección, tenga o no texto. El árbol se calcula solo con nivel y código, así que sirve para cualquier extractor (Fase 3).
- `ruta_linaje` usa el segmento del `nodo_id`, de modo que un código repetido (`idx-N`) tampoco colisiona en la ruta.
- **Sin pérdidas:** toda palabra de la entrada está en la unión de `texto_contenido` y `breadcrumb_jerarquia` de 02. Un título hoja sin texto genera un chunk con su título. Los demás títulos viajan en el breadcrumb de sus descendientes. Un título de más de 512 caracteres, que la pila recorta, se copia también al texto de su sección.
- Las tablas Markdown están íntegras en 02 porque sus líneas forman parte del texto de la sección o del preámbulo.
- `tipo_bloque` se deduce del título; ante la duda, `NARRATIVA`.

### Tokens y truncado del modelo
- **Estimación determinista** (`domain/services/fragmentacion.rs`): 1 token por signo o ideograma y 1 por cada 4 caracteres alfanuméricos.
- **No se usa el tokenizer de E5 para cortar:** exigiría cargar un modelo de 2,2 GB en una capa pura y haría depender los cortes del modelo activo.
- **Hallazgo:** fastembed 4.9.1 trunca en silencio a 512 tokens y E5 no admite más. Por eso una sección de 513 a 600 tokens, que el contrato deja en un chunk, y una ventana de 512 con `passage: `, breadcrumb y condición pueden exceder el límite. El texto queda íntegro en 02; el vector solo representa el inicio. En las ventanas, el solapamiento de 64 cubre la cola salvo con breadcrumbs muy largos.
- **Medida:** `EmbeddingEnginePort::contar_truncados` usa el tokenizer real después de vectorizar, y el informe avisa si hay truncado. **Pendiente de la Fase 5:** medir con el modelo real si hace falta bajar los umbrales, con el test `#[ignore]` de `fastembed_engine.rs`.

### Reintento (`reintentar_destilacion`)
Caso de uso y método de `LakehouseApp`, que recibe la ruta del archivo. Es idempotente: sobre un documento definitivo devuelve `ya_ingerido` sin llamar a Claude.

| Respuesta de Claude | Efecto |
| :--- | :--- |
| Error | Devuelve el error; el provisional queda intacto |
| Código válido | Reescribe al canónico; `doc_id_provisional` guarda el provisional; deroga revisiones previas |
| Código válido con versión ≤ la confirmada | `VersionConflict`; el provisional queda intacto |
| Sin código válido | **Consolida:** mismo `doc_id` y versión, `COMPLETO`, con Capa 2 y `doc_id_provisional = NULL` |

- **D4, aclaración:** `NULL` también significa «consolidado», porque ya no es provisional ni admite otro reintento.
- **Vectores:** 02 y 07 se regeneran con el mismo mapeo determinista y reutilizan el vector cuyo texto vectorizado coincide exactamente. Solo se calcula lo nuevo.
- **Transacción:** `confirmar_sustitucion` activa la revisión nueva, retira la previa y la provisional, y marca 01. Si falla, compensa y el servicio revierte. **Una vez confirmada**, borra físicamente las filas provisionales: no son historia documental, y dejarlas confundiría a `existe_hash` y al time travel. Si ese borrado falla, las filas quedan no vigentes y el informe lo avisa.
- `existe_hash` prefiere la revisión vigente. Reingerir un provisional devuelve `ya_ingerido` con `pendiente_destilacion` y un aviso.

### Integridad
Una revisión confirmada con chunks y sin 07 **pasa a ser** `nodos_huerfanos`. No hay centros heredados que tolerar, porque el esquema v3 no migra.

## 5. Contratos de la Fase 3

### Dependencias (verificadas en Windows/MSVC con 75 PDF reales)
- **`pdf-extract` 0.12 y `lopdf` 0.42**. lopdf va en la versión que usa pdf-extract: la última (0.46) duplicaría el crate y sus tipos `Document` no serían compatibles.
- **No se usan** `get_toc`/`get_outlines` de lopdf (un `/Next` cíclico los deja en bucle y `get_toc` descarta títulos repetidos), ni `extract_text_from_mem_by_pages` de pdf-extract (se detiene en la primera página con error). Se recorre `/Outlines` con control de ciclos y se extrae con `output_doc_page`.
- pdf-extract **entra en pánico con fuentes reales** («missing width», 1 de 75 PDF). Cada página va bajo `catch_unwind`, con `Document::extract_text` de respaldo y aviso en el informe. Si ambos fallan, la página queda vacía y el aviso lo dice.
- La extracción corre en un hilo propio (pila de 32 MB, máximo de 300 s). Si se agota el tiempo, el hilo no se puede cancelar y sigue hasta terminar.

### Contrato
- `DocumentParserPort::parse_bytes`: por defecto interpreta UTF-8; `PdfParser` lo sustituye. `ParserPorFormato` elige por extensión y es el parser de `bootstrap`.
- `DocumentoOriginal.contenido` pasa a `Vec<u8>`, y el servicio añade `ingestar_bytes` y `reintentar_destilacion_bytes`. El hash es el SHA-256 de los bytes del archivo; en texto no cambia.
- `guardar_original` solo exige UTF-8 a lo que no es `.pdf`. Un PDF corrupto o cifrado **deja su copia en `raw_docs/`** antes de fallar, porque la copia precede al parseo.
- `ParsedDocument` añade `avisos_extraccion` y `requiere_ocr`; `ParsedSection`, `pagina_inicio` y `pagina_fin`.
- **02 añade `pagina_inicio` y `pagina_fin`** (`Int32` nulables). Se justifica para las citas de la Fase 4 y cuesta poco, porque el esquema v3 aún no se ha publicado. La granularidad era la **sección**; desde la Fase 7 cada chunk lleva sus páginas reales (§9). `VERSION_ESQUEMA_CENTRO` sigue en 3, así que **un centro v3 creado antes de esta fase debe recrearse**. 07 no cambia.

### Estrategia de estructura
1. **`/Outlines` con entradas:** títulos y jerarquía de los marcadores (nivel máximo 6). El título se busca en su página; si la línea es exactamente el título, se omite porque ya viaja en el breadcrumb. Si no aparece, la sección abre al inicio de la página. Con destino inválido, abre en el punto del recorrido. El código es la numeración del título o `M-n`.
2. **Sin marcadores, numeración:** línea con `1`, `1.`, `1.2.3.`, `A.1` o romano con punto (`IX.`), seguida de inicial en mayúscula, en un título de hasta 120 caracteres y 16 palabras, sin `.,;:` final. La Fase 6 amplía este patrón para los subapartados en secuencia exacta (§8). No se exige doble salto de línea, porque pdf-extract no lo conserva de forma fiable. Filtros, en este orden:
   - cabeceras y pies (línea presente en al menos la mitad de las páginas, con los dígitos neutralizados): no cuentan como encabezado, pero **siguen en el corpus**;
   - índice: un encabezado decimal de las primeras páginas (3 o 1/5 del total) que reaparece descarta lo que hay entre ambos;
   - capítulo contradicho: un `3` seguido del subapartado `2.2.2` no era capítulo;
   - secuencia creciente con salto máximo de 3 (`1.4` → `1.4.2` existe en documentos reales). Un `1.` fuera de secuencia abre una lista, y sus continuaciones a 15 líneas o menos se ignoran. Los romanos exigen paso exacto de 1, y los decimales cuelgan de ellos.
   - Con menos de 2 encabezados no hay estructura.
3. **Sin estructura:** una sección «Contenido General», `<doc_id>#sec-cuerpo`, nivel 1.

La numeración de romanos no estaba en el patrón acordado. Se añade porque un procedimiento real tiene sus apartados generales numerados así, y sin ello quedarían en el preámbulo.

**Falsos observados (la heurística no pierde texto, solo afina el árbol):**
- **Positivos:** un ítem de lista largo que empieza por el número siguiente se toma como capítulo si no lo contradice ningún subapartado (`4 Resultado: Al ejecutar...`). Las cláusulas numeradas de un procedimiento (`3.1.1 Recibe los materiales...`) son nodos, lo cual es correcto, pero su título es una frase.
- **Negativos:** un título que termina en punto fuera de la secuencia exacta (§8), que ocupa más de 120 caracteres o que usa numeración sin mayúscula queda como texto. Una referencia suelta `1.5 Algo` tras el capítulo `2` descarta ese capítulo. `regresion_lineal.pdf` (61 páginas) queda en Contenido General.

### PDF sin texto (`REQUIERE_OCR`)
Media inferior a 50 caracteres por página (sin espacios), comprobada antes que los marcadores.
- **Se indexa:** catálogo con `doc_id` provisional, un único nodo `#sec-cuerpo` y un chunk con el poco texto extraído o, si no hay ninguno, `[Sin texto extraíble; requiere OCR] <archivo>`. Queda localizable por nombre.
- **No se destila**, ni al ingerir ni al reintentar: `reintentar_destilacion` devuelve `ya_ingerido` y `pendiente_destilacion = false`. Pasarle OCR cambia los bytes, así que entra como documento nuevo.
- **El usuario ve** `estado_contenido = REQUIERE_OCR` y un aviso que le indica pasarlo por OCR.

### Errores controlados
Vacío, sin cabecera `%PDF-`, corrupto, sin páginas, tiempo agotado o pánico interno devuelven `IngestionError` con el nombre del archivo.

**PDF protegido:** se rechaza **antes de abrirlo** si declara `/Encrypt`, aunque baste la contraseña vacía. `load_mem` de lopdf la prueba sola y descifra sin indicarlo: `is_encrypted()` no es fiable tras cargar. Esto deja fuera a los PDF que solo restringen permisos, por decisión de no sortear ninguna protección.

### Destilación
Claude recibe `contenido_completo` por stdin, sin límite propio. Si un documento grande agota el tiempo o falla, se aplica el provisional de la Fase 2.

## 6. Contratos de la Fase 4

### Lectura y seguridad
- Lo que escribe el agente **nunca entra en SQL**. Las consultas solo llevan valores leídos de las vistas (`centro_id`, `doc_id`, `version_num`); documento, sección y ruta se comparan en Rust. Por eso no se usa `LIKE` y el problema de sus comodines desaparece.
- La revisión la elige siempre 01: vigente o ventana de `fecha_referencia`. 02 y 07 se filtran por esa revisión y se unen por `nodo_id`, nunca por breadcrumb.
- Un `doc_id`, `nodo_id` o ruta inexistente o malformado es un **error útil** en las tools STAIR. No hay contenido inventado.
- `search_normative_context` con un `doc_id` inexistente y sin sección conserva su forma: `total = 0` y un `aviso`.
- Toda respuesta con texto de documento lleva `nota_seguridad`, y el system prompt ordena tratarlo como datos. El texto se entrega literal, sin censurar.

### `search_normative_context`
- `modo`: `HYBRID` por defecto, `SEMANTIC_ONLY` o `EXACT_BM25`. `HYBRID` conserva el recurso léxico si fallan los embeddings. **`SEMANTIC_ONLY` devuelve error:** quien pide solo semántica debe saber que no la ha obtenido. `EXACT_BM25` no llama a embeddings.
- `nodo_id` restringe a la sección y sus descendientes, siguiendo `nodo_padre_id`. `ruta_linaje_prefijo` exige `doc_id`, valida `[A-Za-z0-9._-]` por segmento, rechaza `..` y solo captura la ruta exacta o sus hijos: `/4/4.2` no captura `/4/4.20`.
- Cada resultado añade `estado_contenido`, `tiene_capa_2`, `provisional`, `aviso_contenido` y `tipo_bloque`. La cita añade `breadcrumb`, `nodo_id`, `pagina_inicio`, `pagina_fin` y `paginas`.

### Provisional y revisión vigente (caso señalado por la Fase 2)
Un identificador provisional no permite saber qué documento real es, así que no se fusiona ni se oculta:
- **Un provisional nunca prevalece**, aunque empate en nivel con una norma: un documento sin validar no decide un conflicto. Si comparte tema con uno definitivo, el grupo marca `conflicto_prelacion` y el provisional queda como `supletorio`.
- **`documentos_no_definitivos`** lista los provisionales y `REQUIERE_OCR` del centro en cada búsqueda, aunque ningún fragmento suyo coincida (máximo 20).
- El system prompt obliga a decirlo y a no mezclarlo en silencio.

**Corrección de paso:** `Contenido General` y `Preámbulo / Introducción` los genera la ingesta, no el autor. El tema de esos títulos incluye el documento, porque si no dos documentos sin relación daban un falso conflicto de prelación.

### `stair_navigate_toc` y `stair_get_subtree_context`
- **Orden:** comparación natural de `ruta_linaje` por segmento (`/2/2.2` antes que `/2/2.10`). Los segmentos con solo `I`, `V` y `X` se leen como romanos; `C`, `D` y `L` no, porque suelen ser letras de anexo.
- **Límites** en `ConfiguracionHerramientas`: `max_nodos_toc` (100) y `max_caracteres_subarbol` (40.000, texto y metadatos). La primera sección siempre avanza al menos un carácter, aunque sus metadatos agoten el límite.
- **Dos campos más que el encargo:** `desplazamiento` en el ToC y `desde_caracter` en el subárbol. Sin ellos, los hijos de un nodo muy ancho y una hoja grande serían inalcanzables. Es el caso normal de un PDF sin estructura, que es un solo nodo `Contenido General`.
- **Solape:** `reconstruir_sin_solape` (dominio) quita el prefijo de cada ventana que repite el final de la anterior, hasta `TOKENS_SOLAPE` tokens, el máximo que genera la fragmentación. Así un texto repetitivo no pierde contenido.
- **Condiciones:** `condiciones_heredadas` recorre los ancestros por `nodo_padre_id`, de la raíz hacia abajo y sin repetir. La condición de un hijo no se atribuye a su padre.
- **Alias:** `doc_id` y el prefijo de `nodo_id` aceptan el provisional anterior (`alias_resuelto_de`). Tras la sustitución el segmento de la sección se conserva.

### Enrutamiento
El system prompt añade los 4 casos, citar sección y página, y no decir que no hay datos sin haber buscado en la Capa 0. Las reglas de prelación, condiciones, notas al pie y no inventar valores siguen intactas.

### Reintento y MCP
- **`LakehouseApp::reintentar_documento(doc_id, centro)`** localiza el original por su SHA-256 en `raw_docs/` (`CenterWorkspacePort::localizar_original`) y usa el mismo flujo que `reintentar_destilacion(ruta)`, que se mantiene. GUI, CLI y MCP usan este método, porque el usuario conoce el documento y no la ruta. Verifica el hash antes de usar la copia.
- **CLI:** `--reintentar <doc_id> --centro <id>`. **GUI:** comando `reintentar_destilacion` y botón en el catálogo, solo con `pendienteDestilacion`.
- **MCP:** 8 tools de consulta y, en `--mcp-server`, una novena administrativa `reintentar_destilacion` añadida con `McpServer::con_reintento`. **No está en `lakehouse_tools()`:** el agente solo lee, y el texto de un documento podría inducirle a escribir. Por eso `McpServer::new` sigue listando 8.

### Recortes respecto al encargo
- **07 no entra en el índice semántico en memoria;** solo en las vistas, donde ya estaba. La búsqueda usa 02, que es sin pérdidas e incluye el breadcrumb con los títulos, así que indexar `vector_seccion` no añadía recuperación. Si la Fase 5 detecta que falta, se añade al índice.
- **`--reintentar` no admite una ruta:** la copia en `raw_docs/` siempre precede al parseo.

## 7. Resultado de la Fase 5
Informe completo: `docs/validacion/stair/aceptacion_stair.md`.

- **Enrutamiento real:** matriz E2E, 4/4 en 3 ejecuciones.
- **`contar_truncados`:** medido. Sin recorte en el PDF real ni en los fixtures. **Propuesta no aplicada:** bajar el umbral de chunk único de 600 a 512 tras medir con un documento largo.
- **Límite del subárbol:** se mantiene en 40.000. **Corrección del contrato de §6:** los metadatos que consumen el límite son toda la sección serializada, no solo título y condición. El primer texto avanza al menos 1/4 del límite. El escapado JSON del texto no se descuenta, así que el límite es aproximado.
- **07 fuera del índice semántico (§6):** no se detectó ninguna carencia de recuperación.
- **GUI:** verificada en ventana por DevTools. Faltan el diálogo nativo y el arrastre desde el Explorador.

**Pendiente de decisión (no aplicado):** páginas en 07 para citar desde el ToC. Los otros tres fallos abiertos de esta fase se tratan en §8.

## 8. Contratos de la Fase 6

Aprobados por el usuario. `VERSION_ESQUEMA_CENTRO` pasa a 4 y sigue sin haber migración: un centro anterior se rechaza con el mensaje de §3.

### Fecha de vigor
- `fecha_vigor` es nula en 01 y `Option` en `CatalogoDestilado` y `VentanaVigencia`. Nunca se inventa, ni en el prompt ni en Rust. Un provisional queda sin fecha.
- `validar_fecha_vigor` (dominio) es la red de seguridad. Da `NULL`, con aviso en el informe, para:
  - lo que no es ISO (`03-04-2026` es ambiguo y adivinarlo sería inventar);
  - fechas imposibles, años fuera de `[1900, 2100)`, `1900-01-01` y `1970-01-01`;
  - la fecha del día de la ingesta, salvo que figure en el texto del documento: es lo que hacía Claude.
- Un valor ausente, vacío, `null`, `N/A` o `s/f` da `NULL` sin aviso: es lo esperado.
- **Sin `fecha_referencia`** nada cambia: la vigencia es la columna `vigente`, y el orden de revisiones, `version_num`.

**Con `fecha_referencia` D**, cada revisión está en uno de tres estados (`VentanaVigencia::estado_en`, y en SQL `predicado_ventana` y `predicado_no_declarada`):

| Estado | Condición |
| :--- | :--- |
| Vigente | `fecha_vigor <= D` y (`D < fecha_derogacion`, o sin derogación y `vigente = true`) |
| No vigente | `D < fecha_vigor` o `fecha_derogacion <= D` |
| `vigencia_no_declarada` | Sin `fecha_vigor` y no derogada antes de D, o con inicio conocido y fin desconocido |

**Transiciones al confirmar una revisión nueva:**

| Previa → nueva | `fecha_derogacion` de la previa | En una D |
| :--- | :--- | :--- |
| fecha → fecha | La fecha nueva, que debe ser posterior | Ventana exacta, como antes |
| `NULL` → fecha | La fecha nueva | Antes de ella, la previa queda sin declarar; desde ella, derogada |
| fecha → `NULL` | `NULL`, con `vigente = false` | Antes de su inicio, no vigente; después, sin declarar, y la nueva también |
| `NULL` → `NULL` | `NULL`, con `vigente = false` | Ambas sin declarar; el orden lo da `version_num` |

- **Paradojas:** una fecha nueva se compara con todas las revisiones confirmadas con fecha, no solo con la vigente, para que no se cuele a través de una sin fecha.
- **Limitación aceptada:** en `F1 → NULL → F3`, la revisión 1 sigue marcada sin declarar para una D posterior a F3, porque nada fija su fin. Queda marcada, no afirmada. Resolverlo exigiría subconsultas en todas las lecturas.

**Lo que devuelven las tools con `fecha_referencia`:**
- Todas añaden `vigencia_no_declarada` (lista de revisiones) y, si no está vacía, `aviso_vigencia`.
- Catálogo, matriz y búsqueda incluyen las filas, cada una con `vigencia_no_declarada`. El ToC y el subárbol añaden `revision_vigencia_no_declarada`. Una revisión así no prevalece en un conflicto, igual que un provisional, y una firme se elige antes.
- SQL, grafo y `lookup_double_entry_table` solo devuelven lo firme: no pueden marcar fila a fila y son los motores de precisión. Lo excluido figura en la lista, de modo que no queda oculto.
- El catálogo añade `fecha_vigor_texto` (`no declarada`), que la GUI muestra igual. La CLI no tiene listado de catálogo: los avisos de fecha descartada salen en el informe de ingesta.

### Apartados en PDF
Un título **dudoso** (termina en `.`, `,` o `:`, o supera las 16 palabras) solo abre sección si cumple todo esto:
- tiene dos o más niveles (`4.17`): `4 Algo.` nunca es capítulo;
- sigue la **secuencia exacta**: primer hijo del anterior o siguiente, sin huecos, de alguno de sus ancestros. No se aplica el salto de 3 de §5;
- no acaba en puntos guía ni empieza por `Nota`, `Ver`, `Véase`, `Fuente`, `Figura`, `Tabla` o `Ejemplo`.

Los títulos dudosos no cuentan para detectar índices ni capítulos contradichos, y el límite de 120 caracteres se mantiene. `;` final sigue sin abrir sección. **Falso positivo posible:** una línea de cifra con la numeración exacta siguiente y con mayúscula (`1.2 Veces el espesor.` justo tras `1.1`). Solo afina el árbol: el texto no se pierde.

### Origen del responsable
- 03 añade `origen_responsable`, y `OrigenResponsable` (dominio) es `DECLARADO`, `INFERIDO` o `NO_CONSTA`.
- El destilador lo devuelve por hito, con enum en el JSON Schema, y el prompt prohíbe deducir.
- `OrigenResponsable::verificar` es la red de seguridad, aplicada a lo que devuelve Claude:
  - un responsable vacío es `NO_CONSTA`;
  - `DECLARADO` exige que el rol aparezca como **secuencia de palabras completas**, sin tildes ni mayúsculas, en el título con sus ancestros o en el texto íntegro de la sección de origen. Si no, pasa a `INFERIDO` y el informe avisa;
  - un valor desconocido o `NO_CONSTA` con rol pasa a `INFERIDO`.
- **Límite:** comprueba que el rol se nombra, no que sea el responsable. Un texto malicioso **en la misma sección** puede superarla. Por eso el prompt lo excluye y el system prompt trata el documento como datos.
- `calidad_extraccion` da `REVISAR` con un responsable inferido (`RESPONSABLE_INFERIDO`) o ausente.
- La matriz, su cita, el SQL y la GUI exponen el origen; el texto de hito de la búsqueda lo incluye. El system prompt prohíbe presentar `INFERIDO` o `NO_CONSTA` como hecho.
- **La matriz no es determinista:** ver la variación medida en el informe local de aceptación.

### Privacidad
- Ningún test ni documento versionado lleva nombres ni contenido de documentos reales. Las preguntas de `tests/aceptacion_stair_pdf_real.rs` se leen de `LAKEHOUSE_PREGUNTAS_ACEPTACION` (formato: `tests/fixtures/preguntas_aceptacion_ejemplo.json`). El oráculo de apartados recibe PDF y códigos por `LAKEHOUSE_PDF_ORACULO` y `LAKEHOUSE_CODIGOS_ORACULO`.
- La evidencia con contenido real va a `docs/validacion/stair/`, ignorada por git.

## 9. Contratos de la Fase 7

Autorizados por el usuario con alcance cerrado. `VERSION_ESQUEMA_CENTRO` pasa a 5 y sigue sin haber migración: un centro anterior se rechaza con el mensaje de §3.

### Invariante de hitos
- `HitoOperativo::new` exige al menos un rol (ejecutor o responsable) o un registro (código o nombre). Si no, devuelve `DomainError::HitoSinTitular`. El tipo es `#[non_exhaustive]` y ya no implementa `Deserialize`, de modo que fuera del crate no hay otra forma de construirlo.
- **Relleno:** un campo sin alfanuméricos o igual a un relleno habitual, completo (`N/A`, `-`, `No aplica`, `No consta`, `Ninguno`, `s/d`…), se trata como vacío. Un responsable vacío siempre es `NO_CONSTA`.
- `mapeo_destilacion` deriva esos hitos a la Capa 0: emite `tracing::debug!`, los cuenta en `hitos_derivados_capa0` del informe de ingesta y añade un aviso. Su texto sigue en 02, porque 02 sale del parser y no del destilador.
- `validar_estructura_stair` rechaza además una `FilaHito` sin titular al escribir: es la última barrera, para quien construya la fila a mano.

### Filtro de origen en `query_operational_matrix`
- `solo_declarados` vale `true` por defecto. Devuelve solo los hitos con `origen_responsable = DECLARADO`, y el filtro se aplica después de los demás: `excluidos_no_declarados` cuenta solo hitos que coincidían con la consulta.
- Con `false` devuelve todos, cada uno con su origen. **El filtro mira el responsable aunque se pregunte por el ejecutor**, por fase, por registro o por objeto: por eso el contador y la acción sugerida lo dejan a la vista, y el system prompt ordena repetir con `false` si la pregunta no trata del responsable.
- `sin_responsable_declarado` es `true` si ninguna fila devuelta tiene responsable declarado, también con 0 filas. Entonces `accion_sugerida` indica buscar con `search_normative_context`, citar el párrafo literal y decir que no consta responsable. La tool no llama a otras tools.
- La tarjeta de la GUI y el resumen de la tool muestran los excluidos y la señal.
- **Límite aceptado:** en documentos en voz pasiva la matriz no es fuente fiable de roles (§8: varía entre ingestas). El filtro evita presentar inferencias como hechos, pero no hace que la extracción sea estable.

### Numeración reiniciada en PDF
`dividir_en_tramos` (`pdf_estructura.rs`) corta el documento antes de aplicar la secuencia. Cada tramo tiene su propia numeración y su filtro de capítulos contradichos.

| Apertura | Condición | Nodo |
| :--- | :--- | :--- |
| Línea `Capítulo N: Título` (`N` en cifras o romanos, también `Chapter`) | Le sigue en 60 líneas o menos un apartado que abre numeración (`1`, `2.1`), con otro encabezado encadenado, y hay subapartados o separación propia de secciones. El título no se había visto antes | `REAL`, nivel 1, código `Cap-N` |
| Reinicio sin línea de capítulo | Vuelve atrás en la numeración, abre numeración, al menos 4 encabezados encadenados, 2 con subapartado, separados más de 15 líneas en algún punto y continúa en otra página | `SINTETICO`, nivel 1, «Parte N (numeración reiniciada)», sin texto ni línea propios |

- Con capítulos, los decimales bajan un nivel, como ya ocurría con los romanos. El `nodo_id` de cada tramo lleva el prefijo `C<n>-` (`ParsedSection::clave_nodo`), de modo que los códigos repetidos no colisionan y el ToC conserva el orden. El `codigo_seccion` visible no cambia.
- Una parte generada no entra en el breadcrumb de sus apartados: es la columna que se vectoriza y se cita, y su título no está en el documento.
- **Descartes:** listas, notas al pie y referencias (planas o apretadas), una remisión `Capítulo 7: ...` sin apartados detrás y las entradas de un índice inicial. Un ítem que parece capítulo pero contradice a lo siguiente no adelanta la numeración.
- **Falso positivo posible:** una lista de dos niveles larga y dispersa puede abrir una parte. Solo afina el árbol: no se pierde texto.

### Nodos sintéticos por páginas
- `tipo_nodo` en 07: `REAL` o `SINTETICO`. Vocabulario comprobado por `verificar_integridad`.
- `dividir_secciones_largas` actúa en cualquier estrategia si la sección abarca **más de 15 páginas y más de 40.000 caracteres** (umbrales estrictos). Genera sub-nodos de hasta 10 páginas. La sección real queda como padre sin texto, con su rango; una sola página o un nivel 6 no se dividen.
- Los sub-nodos cuelgan siempre de la sección real, también del preámbulo (`construir_arbol`, `SeccionDelArbol::sintetica`), y heredan breadcrumb, condición y tipo de bloque. Código: `<real>.PAG<ini>-<fin>`; título: «Páginas ini-fin de «título»». Ninguna palabra se pierde.
- **Citas:** búsqueda y subárbol devuelven el `nodo_id` de la sección real (`seccion_real_de`) y el rango de páginas del fragmento; el breadcrumb es el real. El ToC añade `tipo_nodo` y `aviso_nodos_sinteticos`, y el system prompt prohíbe presentarlos como apartados.
- Los hitos que apuntan a la sección real se enlazan a su primer chunk, y `OrigenResponsable::verificar` lee el texto de todos sus sub-nodos.

### Páginas por chunk
- `ParsedSection::saltos_pagina` guarda dónde empieza cada página. `fragmentar_con_posiciones` devuelve la posición de cada fragmento, y `pagina_inicio` y `pagina_fin` de 02 salen de ambos. Un chunk a caballo cita las dos páginas.
- Markdown sigue con `NULL`. Un título sin texto o el respaldo de un PDF sin líneas heredan el rango de la sección.
- El subárbol da el rango de la menor a la mayor página de sus chunks.
