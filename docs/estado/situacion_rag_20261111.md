# Lakehouse RAG: situación actual (11-11-2026)

Este documento sirve para que otro asistente se ponga al día.

- Lo medido aparece como medido.
- Lo que es opinión o suposición se marca como tal.
- No contiene texto de documentos reales.

- **Repositorio (público):** https://github.com/i-goikoetxea/lakehouse-RAG. El `main` publicado es el `f47505e`.
- **Auditoría técnica con ficheros y líneas:** [`auditoria_generalidad_20261111.md`](auditoria_generalidad_20261111.md).
- **Ramas** `claude/arreglos-rag-512d66` y `claude/set-oculto-v2-f844c1`: ya publicadas en el mismo repositorio.

---

## 1. Qué es y qué buscamos

**Lakehouse RAG** es una aplicación de escritorio en Rust con Tauri 2. Responde preguntas sobre los documentos de quien la use: normativa, manuales, informes, guías clínicas, leyes y artículos. Admite PDF y Markdown.

Es un **laboratorio de aprendizaje**, sin uso comercial. El objetivo es un RAG genérico que:

- **extraiga el conocimiento exacto** de cualquier documento, en cualquier dominio e idioma;
- lo **exprese** con su cita: documento, revisión, apartado y **página exacta**;
- diga «no consta» cuando el dato no está, en lugar de rellenar de memoria.

No tiene fine-tuning. La mejora tiene que salir de la ingesta, la recuperación, las herramientas y el control de la respuesta.

## 2. Arquitectura (resumen)

### Organización del código y forma de trabajar

- Arquitectura hexagonal: `domain`, `application`, `infrastructure` y `presentation`.
- TDD estricto, con unos 950 tests y suites adversariales.
- **Fachada única** `LakehouseApp`, que usan la GUI, la CLI (`lakehouse_rag_cli.exe --crear-centro / --activar / --ingest / --ask --json`) y un servidor MCP por stdio.
- **Centro de conocimiento:** cada corpus aislado con su almacenamiento. El esquema actual es el **v6**, y un centro anterior hay que recrearlo.

### Modelo de respuesta

El modelo es Claude, a través de `claude -p` (Claude Code), en modo agente con herramientas MCP. Un *system prompt* largo, de 14 reglas, le dice qué herramienta usar.

### Capas de datos

| Capa | Qué guarda | Herramienta |
|---|---|---|
| Capa 0 | Texto troceado (chunks) con embeddings (FastEmbed) y BM25, búsqueda híbrida, saltos de página por chunk y marcas `⟦p. N⟧` en la salida | `search_normative_context` |
| Índice STAIR | Árbol de apartados del documento (navegación por índice, al estilo de PageIndex) | `stair_navigate_toc`, `stair_get_subtree_context` |
| Capa 2 | Datos destilados: tablas de doble entrada, matriz operativa (roles y registros) y SQL de solo lectura | `lookup_double_entry_table`, `query_operational_matrix`, `execute_athena_sql` |
| Grafo y catálogo | Remisiones o derogaciones entre documentos; inventario, prelación y vigencia | `traverse_normative_graph`, `inspect_lakehouse_catalog` |

### Arnés determinista

El arnés (`src/application/use_cases/arnes_turno/`) envuelve cada turno del modelo.

**Antes de la inferencia:**

- `IntentDetector` clasifica la pregunta.
- `PreInferenceInterceptor` añade directivas, por ejemplo «consulta la Capa 2».

**Después de la inferencia,** `PostInferenceGuard` revisa la respuesta:

- **anclaje de cifras:** toda cifra de la respuesta debe estar en la pregunta o en lo que devolvieron las herramientas;
- **`CifraEnAbstencion`:** si dice «no consta», no puede añadir valores;
- **fechas y revisiones;**
- **verificación de páginas:** si el extracto citado está en otra página, añade un aviso;
- **«no se consultó la Capa 2»;**
- **negaciones sin búsqueda.**

Si algo falla, hay **un reintento** con la indicación. Si vuelve a fallar, el usuario recibe un **mensaje fijo** (`salida_segura`).

## 3. Cómo medimos

1. **Benchmarks propios** (`benchmarks/`, Python): ciclo de vida, RGB industrial y Aguja (documento militar en inglés, con páginas). **Saturan al 100 %** y los diseñamos nosotros, así que solo dicen «funciona en los casos conocidos».
2. **Set oculto (held-out).** Lo escribe una sesión que no toca la app. Las sesiones que corrigen no lo leen.
   - **v1:** 5 PDF y 48 preguntas. Resultado: 47/48. Se usó para orientar arreglos, así que ya no sirve como prueba.
   - **v2:** 15 PDF públicos reales y 61 preguntas.
     - Seis dominios: técnico, clínico, legal, financiero, académico y manual.
     - Dos idiomas: ES y EN.
     - Las preguntas están parafraseadas, con otro vocabulario que el del documento.
     - Incluye datos repartidos, cruces entre documentos, preguntas sin respuesta y premisas falsas.
     - Incluye **8 trampas**: PDF modificados con un valor distinto del que el modelo conoce de memoria.
     - Las respuestas correctas llevan página y extracto **verificados por script**.
     - El set vive fuera de git. Los informes solo dan ids y categorías.
3. **Puntuación determinista** más revisión manual de los casos dudosos. Se distingue entre «entregada» y «correcta».

## 4. Resultados medidos

### 4.1 Set oculto v2 (2 pasadas por binario; Sonnet)

| | `main` (`de9cae9`) | Rama de arreglos (`932766c`) |
|---|---|---|
| Correctas | **114/122** (57 + 57) | **115/122** (59 + 56) |
| Entregadas | 119/122 | 121/122 |
| Trampas | 8/8 y 8/8, sin respuestas de memoria | 8/8 y 8/8 |
| Preguntas sin respuesta | 9/9 «no consta», sin cifras | igual |
| Página incorrecta | 0 | **1** (retroceso) |
| Intervenciones del arnés | 23 y 27 | 12 y 13 |
| Bloqueos (`salida_segura`) | 2 y 1 | 0 y 1 |

La diferencia 115 frente a 114 **está dentro de la variabilidad del modelo**: no demuestra una mejora global.

### 4.2 Benchmarks propios con la rama de arreglos

- Aguja: 1,00 en todo. Avisos de página: de 4 a 0.
- Ciclo de vida: 1,000.
- RGB industrial: 0,792 frente a 0,833.
  - La bajada viene de `rechazo_estricto`, que castiga cualquier cifra.
  - Medido: **0 cifras inventadas** antes y después. Todas son eco de cifras que dieron las herramientas.
  - No es un error de respuesta.
- Tests: 953 pasan y 0 fallan.

## 5. Qué funciona (y hay que proteger)

- **No inventa.** Las 8 trampas se respondieron siempre con el valor del documento, también en apéndices y en otro idioma. Todas las preguntas sin respuesta acabaron en «no consta» sin cifras.
- **Los datos repartidos y los cruces** salen bien en la gran mayoría de los casos.
- **Las preguntas parafraseadas,** con vocabulario coloquial o en otro idioma, se responden bien.
- **La página es exacta en el texto corrido:** unas 45 de 47 respuestas con página, y 0 páginas incorrectas en `main`.
- **Buscar y razonar** no causó ningún error en el set v2 con `main`. Todos los errores vinieron del arnés o de la ingesta.

## 6. Qué está mal

### 6.1 Errores de respuesta abiertos, por causa

| # | Defecto | Dónde se rompe | Evidencia |
|---|---|---|---|
| 1 | **Página falsa en una tabla.** La Capa 2 da la p. 6 a una tabla que está en las pp. 21-22. Solo 2 de 6 tablas consultadas traen página, y una de esas 2 es falsa. | Rama de arreglos: la página se deriva del apartado y no de la celda (causa supuesta) | Retroceso confirmado. Bloquea el merge. |
| 2 | **Texto corrupto marcado como COMPLETO.** Un PDF académico con fuentes Type1 sin ToUnicode (codificación con `/Differences`) queda ilegible en las pp. 2-11. pypdf lo lee bien. | Ingesta: el lector de PDF de la app | 2 preguntas fallan siempre, en 4 de 4 ejecuciones |
| 3 | **El arnés castiga valores legítimos.** Toma por inventados los números de artículo y las cifras calculadas (restas, sumas), reintenta y a veces bloquea o el modelo borra el cálculo correcto. | `PostInferenceGuard` (regla «no consta + valor nuevo») | 1 de 61 bloqueada en arreglos; 3 en `main` |
| 4 | **Cruce entre dos documentos con negación.** No recupera el segundo documento y afirma que el dato «no consta». | Búsqueda y respuesta. Defecto previo: también ocurre en `main` | 8/10 en `main` y 6/10 en arreglos; no concluyente |
| 5 | **Respuesta con llamadas a herramientas en crudo.** Una vez entregó texto de *tool calls* sin ejecutarlas, y el arnés no lo detuvo. | Arnés o adaptador de Claude | 1 vez en `main` |
| 6 | **Avisos de página falsos sobre el título del documento** («cita p. X, pero está en p. Y»), porque el título está en la p. 1. | Verificador de páginas | Al menos 3 en arreglos |

### 6.2 Problemas de fondo

Esto es opinión, basada en lo que vimos en estas iteraciones.

1. **El arnés crece a base de reglas léxicas:**
   - listas de locuciones en español,
   - expresiones regulares sobre cifras,
   - heurísticas de «intención tabular».

   Cada regla tapa un fallo y abre otro: bloquea respuestas correctas o mete ruido. Varias rondas fueron así: arreglar un falso bloqueo, ver aparecer otro. **No converge**, porque intenta validar semántica con reglas superficiales.
2. **La página depende de la calidad de la extracción del PDF.** El lector de PDF propio falla con fuentes no estándar y con tablas complejas, y aguas abajo (página de celda, verificación) se construye sobre esa base frágil.
3. **La señal de medición es pequeña.** El set v2 ya está en el 93-97 %, así que una mejora o un retroceso equivale a 1 o 2 preguntas, dentro del ruido del modelo. Hacen falta más repeticiones o un set más duro, y eso cuesta tiempo y Claude.
4. **Los arreglos se prueban con datos sintéticos** y los mide después el set oculto. Es lo correcto contra el sesgo, pero el ciclo es largo: unas 2,5 h por medición en un equipo que el usuario también usa.
5. **Proceso:**
   - Se paralelizó en muchas sesiones de agentes; algunas se pararon esperando compilaciones.
   - Los informes son largos, y al usuario le cuesta saber dónde estamos.
   - Ha habido regresiones introducidas por arreglos, por ejemplo un comprobador que subió los avisos falsos de 1 a 51 y se descartó.

## 7. Qué hay que corregir (propuesta, por prioridad)

1. **Página de la Capa 2.** Tomarla de la celda o del chunk de origen de la fila. Nunca del apartado de contexto. Si no hay certeza, no dar página: una página falsa es peor que ninguna.
2. **Ingesta de PDF.**
   - Decodificar las fuentes simples con `/Differences` por nombre de glifo cuando falte ToUnicode, o usar un extractor más robusto como alternativa.
   - **Medir la legibilidad** del texto extraído (proporción de palabras plausibles por idioma) antes de marcar un documento como COMPLETO. Si falla, marcarlo con aviso.
3. **Regla «no consta + valor nuevo» del arnés.** No contar como inventados:
   - las referencias estructurales (artículo, apartado);
   - los valores derivables por aritmética de cifras recuperadas.

   Que solo dispare si la negación afecta al dato que se pregunta.
4. **Cruces entre documentos.** Si la pregunta nombra dos entidades o documentos, buscar en los dos antes de concluir. Sin búsqueda, decir «no lo he encontrado», no «no consta».
5. **Detectar *tool calls* en crudo** en la respuesta antes de entregarla.
6. **Verificador de páginas:** ignorar el título y la cabecera del documento.
7. **Set oculto v3,** más duro:
   - trampas que **no** se recuperen fácilmente, para medir si rellena de memoria;
   - más tablas complejas y PDF problemáticos;
   - 3 o más pasadas.

## 8. Estado de las ramas (nada integrado sin el OK del usuario)

| Rama | Contenido | Estado |
|---|---|---|
| `main` (`de9cae9`, local; origin en `f47505e`) | Arnés, citas de página v6, benchmarks, `CLAUDE.md` | 2 commits sin subir (solo documentación) |
| `claude/arreglos-rag-512d66` | 5 mejoras: verificador de páginas silencioso, página en Capa 2, contexto de apartado en tablas, negaciones sin búsqueda, Capa 2 solo con intención clara | **No integrar** hasta corregir la página falsa (punto 1) |
| `claude/set-oculto-v2-f844c1` | Scripts genéricos e informes del set v2, sin datos | Pendiente de integrar (solo informes y scripts) |
| `claude/comprobador-paginas-96dd29` | Comprobador por unidad de cita | **Descartado:** subió los avisos falsos de 1 a 51 |

## 9. Reglas del proyecto (de `CLAUDE.md`)

- **App genérica:** ningún arreglo puede depender de un documento, una pregunta o una cifra concretos.
- **Set oculto aislado:** quien corrige no lo lee.
- **Sin retrocesos:**
  - línea base antes del cambio y comparación después;
  - cualquier empeoramiento bloquea el merge;
  - el ruido nuevo cuenta como retroceso.
- **Errores primero** en los informes. Distinguir «entregada» de «correcta» y lo medido de lo supuesto.
- **No versionar** texto de documentos reales.

## 10. Preguntas abiertas para quien nos ayude

1. ¿Conviene sustituir parte del arnés léxico por un verificador semántico? Por ejemplo, una segunda llamada barata que compruebe que cada afirmación está soportada por los extractos (estilo *claim verification* o NLI). ¿O hay otro patrón más robusto para no inventar sin bloquear respuestas correctas?
2. Para la página exacta en tablas de PDF: ¿qué patrón recomendáis? ¿Extracción con coordenadas por celda, detección de tablas con layout, otra cosa?
3. ¿Extractor de PDF propio o una librería probada (pdfium, mupdf, docling…) con un control de legibilidad?
4. ¿Cómo medir con más señal sin gastar horas por iteración? ¿Más repeticiones de un subconjunto, *judge* automático, otra cosa?
5. Viendo la arquitectura de capas (Capa 0, STAIR, Capa 2), ¿qué simplificaríais?
