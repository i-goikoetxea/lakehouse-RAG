# Regla 15 v2, no traducir la consulta e idioma en el catálogo (Claude real, 2026-11-12)

**Veredicto: no bloquea por corrección, pero no cumple 2 de los umbrales del mentor.**
- Ninguna respuesta empeora: 0 retrocesos.
- Las búsquedas por pregunta de un documento **suben** de 0,93 a 1,33, cuando el objetivo era bajarlas a 1,1-1,2.
- El inglés se conserva en el **94 %** de las partes, no en el 100 % exigido.
- **El set de prueba se ha saturado:** las dos variantes aciertan el 100 % en las 2 pasadas, así que la corrección ya no distingue entre ellas.

## 1. Errores y umbrales no cumplidos (primero)

1. **No hay respuestas incorrectas en ninguna variante**: 0 de 196, puntuadas por reglas y sin falsos negativos que revisar. No hay retrocesos de corrección.
2. **El set está saturado.**
   - `main` también acierta 23/23 y 26/26 en las dos pasadas.
   - El 100 % significa «funciona en estos casos», no que el cambio mejore la corrección: con este set no se puede medir esa mejora.
   - Respecto a la campaña anterior han cambiado dos cosas a la vez: las agujas con sujeto único y el esquema v7 con pdfium. Las abstenciones falsas de entonces no se han reproducido, y no puedo atribuirlo a ninguna de las dos por separado.
3. **Umbral de búsquedas no cumplido** en las preguntas de un documento: `main` 0,93, rama 1,33 (objetivo 1,1-1,2).
   - Sube en 11 de las 23 preguntas: 4 de las 5 de ruido de RGB, 3 de las 4 de integración y 4 léxicas de aguja.
   - Las trazas muestran el patrón: la rama añade una segunda búsqueda con solo el código («L-104», «AGJ-8TFM», «RX-4 humedad relativa curado»).
   - En las preguntas de integración, que piden dos datos, separar las búsquedas es lo que pide la regla. En las de ruido, que piden uno, es una búsqueda redundante.
   - **Inferencia, no medida:** van en el mismo turno, porque la latencia mediana baja de 33,5 a 28,9 s. El registro no guarda el turno de cada llamada.
4. **Umbral de idioma no cumplido.** Las partes en inglés conservan el inglés en la consulta en **67/72 (93 %)** según la métrica y en **68/72 (94 %)** tras revisarlas a mano.
   - Las 4 traducciones reales: `aguja-3-léxica` en las dos pasadas, `cruzada-léxica-1` y `cruzada-léxica-0`, siempre la parte de la aguja traducida al español.
   - Ninguna produjo un error, porque la búsqueda semántica la encontró de todas formas.
5. **Deriva inversa (control en español):** 65/67 según la métrica y **0 casos reales** tras la revisión. Los 2 casos son una paráfrasis en español con una sola palabra igual («inspección» por «inspeccionan») y una parte de tabla cuya única búsqueda era la de la parte inglesa.
6. **Coste incidental:** un primer lanzamiento de la reingesta quedó huérfano y se solapó con el bueno. No tuve permiso para pararlo, así que gastó destilaciones de Sonnet sobre una carpeta descartada (`n3_v7`). El centro medido es `n3_v7b`, ingerido limpio con el binario de `main`.

## 2. Qué se ha cambiado

Rama `claude/regla15-v2-idioma` desde `main` `ad1fbb3`, sin push ni merge. Lleva el cherry-pick de la regla 15 v1 y de las herramientas de medición.

- **Regla 15 v2** (system prompt):
  - una búsqueda por entidad, juntas en el mismo `tool_calls`;
  - antes de «no consta», catálogo (documentos e idiomas) y una segunda búsqueda **abierta, sin `doc_id`**, salvo que se nombre un documento o código del catálogo, cambiando los términos, en el idioma del documento o con `EXACT_BM25`;
  - nunca se restringe por título.
- **Opción (a):** regla 16 y descripción del campo `consulta`: «idioma y términos técnicos de la pregunta o del documento objetivo; nunca la traduzcas al idioma de estas instrucciones».
- **Opción (b), sin esquema ni dependencias:** `inspect_lakehouse_catalog` añade a cada documento:
  - `escritura_dominante`, por bloques Unicode;
  - `muestra_texto`, un prefijo literal de 150 caracteres como máximo de la primera línea de texto corrido.

  Se calcula al vuelo sobre los chunks de 02 ya indexados, sin caché porque no hizo falta. Salta las páginas `LECTURA_DEGRADADA`, los preámbulos, los índices, las tablas y las líneas repetidas (cabeceras y pies). No usa listas de palabras.
  - En MIL-STD, la muestra es «1. This Standard is approved for use by all Military Departments and Defense Agencies».
  - En los fixtures, la frase inicial en español.
  - La salida del catálogo crece de 7088 a 8396 caracteres (+18 %).
- **Set de prueba:** las 5 agujas tienen ahora sujetos distintos (un quinto sujeto solo para N3, sin tocar la pista Aguja).
- **N0:** 1033 aprobados, 0 fallos y 35 ignorados. `clippy --all-targets -D warnings` y `fmt --check` limpios. Base inferida de `main`: 1023 (1015 de las Piezas 1 y 2 más 8 de la 3c). Los 10 tests nuevos se vieron en rojo.

## 3. Antes y después

- Centro aislado v7, ingerido con Sonnet: MIL-STD-882E con 5 agujas de sujeto único, `ciclo_de_vida` y `rgb_industrial`.
- 2 pasadas por binario, con `--modelo sonnet` y el orden de los binarios alternado por pregunta y pasada.
- Binarios: `main` `a0c6e9fa81e7…`, rama `090bc6b300f9…`.
- 196/196 respuestas, sin fallos de ejecución.
- Herramienta: `benchmarks/run_multidoc_claude.py`.

| Métrica | `main` `ad1fbb3` | Rama | Umbral |
|---|---|---|---|
| Un documento, correctas (pasada 1 y 2) | 23 y 23 | 23 y 23 | ≥ 21/23 ✔ |
| Cruzadas, correctas (pasada 1 y 2) | 26 y 26 | 26 y 26 | ≥ 23, objetivo ≥ 25 ✔ |
| Partes en inglés que conservan el inglés | 21/66 (32 %) | 67/72 (93 %), 94 % tras revisión | 100 % ✘ |
| Partes en español que conservan el español | 66/70 | 65/67, 100 % tras revisión | sin deriva ✔ |
| `search_normative_context` por pregunta de un documento | 0,93 | **1,33** | 1,1-1,2 ✘ |
| `search_normative_context` por cruzada | 2,19 | 1,90 | — |
| Tools por pregunta: un documento / cruzadas | 1,50 / 3,25 | 2,00 / 2,56 | — |
| `inspect_lakehouse_catalog` por pregunta: un documento / cruzadas | 0,33 / 0,65 | 0,52 / 0,35 | — |
| Latencia mediana: un documento / cruzadas | 33,5 / 32,8 s | 28,9 / 32,6 s | — |
| CPU media durante las preguntas | 31-40 % | 29-40 % | — |

Los denominadores de la auditoría difieren porque las partes resueltas con `lookup_double_entry_table`, sin búsqueda, se excluyen.

**Latencia:** la CPU estaba cargada por la otra sesión, que medía el set v2. Con 2 pasadas, la latencia es orientativa.

Como referencia, auditando con la misma regla las trazas guardadas de la campaña anterior: `main` conservaba el inglés en 10/32 partes y la regla v1, en 13/32.

## 4. Propuestas

1. **Búsquedas redundantes:** que decida el mentor si 1,33 es aceptable. Las búsquedas extra parecen ir en paralelo y la latencia no sube. Si no lo es, conviene precisar en la regla 15 que la separación es por entidad o condición pedida, no por cada código de la pregunta. Habría que medirlo con datos distintos de estos.
2. **Idioma al 100 %:** las 4 traducciones que quedan son siempre del formato léxico «According to the documentation, what is the … of batch …». No propongo afinar el texto con este set, porque sería ajustar al benchmark.
3. **Un set que vuelva a discriminar**, escrito por la sesión evaluadora y no por esta:
   - documentos en un tercer idioma;
   - un documento cuyo vocabulario solo case en su idioma, por ejemplo términos sin cognados o BM25 exacto;
   - preguntas cruzadas naturales, sin dos `?`.
