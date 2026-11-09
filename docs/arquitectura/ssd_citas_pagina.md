# SSD: citas de página exacta (fases 1 y 2)

**Fecha:** 2026-11-09. **Origen:** §4 de [investigacion_citas_pagina.md](investigacion_citas_pagina.md), aprobado por el usuario: A + B (fase 1) y E (fase 2), con esquema v6 y recreación de centros, sin migración.

## 0. La idea en una analogía

El bibliotecario apunta en su ficha cada vez que se pasa la hoja (A). Cuando te lee la ficha, dice «— pág. 53 —» en voz alta en el punto exacto (B). Y si tú le repites una frase diciendo «esto estaba en la 52», un ayudante busca la frase en la ficha y te corrige si no es así (E).

## 1. Columna nueva en `02_chunks_normativa` (fase 1, A)

| Campo | Contrato |
| :- | :- |
| Nombre | `saltos_pagina` |
| Tipo Arrow | `List<Struct<desplazamiento: Int32 no nulo, pagina: Int32 no nulo>>`, **nulable** |
| `desplazamiento` | Bytes UTF-8 desde el inicio de `texto_contenido` **de ese chunk** (no de la sección) |
| `pagina` | Página **física** del fichero, base 1 (la misma que `pagina_inicio`) |
| Primer elemento | Siempre `{0, pagina_inicio}` |
| Siguientes | Uno por cada página nueva dentro del chunk, en orden estrictamente creciente de desplazamiento y nunca dos seguidos con la misma página |
| Invariante | `pagina_inicio` = primer `pagina`; `pagina_fin` = último `pagina` |
| Nulo | Formatos sin páginas (Markdown, texto) o PDF sin saltos conocidos cuya sección abarca más de una página: no se sabe dónde cambia y no se inventa |
| Un solo elemento | Chunk entero en una página |

- Los datos salen de `paginas_del_fragmento` (`mapeo_destilacion.rs`), que ya recorría los saltos de la sección: ahora devuelve también los saltos del tramo, rebasados al inicio del chunk.
- **Esquema v6** (`VERSION_ESQUEMA_CENTRO = 6`). Sin migración: un centro v5 se rechaza al abrirlo con el mensaje de siempre («crea un centro nuevo y reingiere sus documentos»), que ahora nombra el motivo (los saltos de página de la v6).
- `FilaChunk` y `FragmentoIndexado` llevan `saltos_pagina: Option<Vec<SaltoPagina>>`. `SaltoPagina` pasa al dominio (`domain/services/marcas_pagina.rs`) y el DTO de ingesta lo reexporta. Los hitos no tienen saltos (su texto lo compone la app).
- `execute_athena_sql` ve la columna como lista de objetos JSON; la descripción de las vistas la enumera.

## 2. Marcas de página en la salida (fase 1, B)

### 2.1 Formato

`⟦p. N⟧`: U+27E6, `p.`, un espacio, N en decimal sin ceros a la izquierda, U+27E7. Ejemplo: `⟦p. 53⟧`.

- Se eligen los corchetes blancos matemáticos porque no aparecen en documentos técnicos; `[p. 53]` sí puede aparecer.
- **Neutralización:** antes de insertar las marcas, cualquier `⟦` o `⟧` del texto del documento se sustituye por `[` o `]`. Así, en la salida de estas dos tools, **toda** secuencia `⟦p. N⟧` es una marca de la app. Una marca falsa del documento llega como `[p. N]`, que la `nota` declara que no es una marca.

### 2.2 Dónde y cuándo

| Tool | Campo | Marcas |
| :- | :- | :- |
| `search_normative_context` | `resultados[].texto` de un chunk con `saltos_pagina` válidos | Una al inicio (`⟦p. pagina_inicio⟧`) y otra justo antes del byte de cada salto |
| `stair_get_subtree_context` | `secciones[].texto` | Una al inicio del tramo entregado (también si empieza en `desde_caracter`) con la página de su primer carácter, y otra en cada salto dentro del tramo |
| Ambas | Hitos, Markdown, chunks sin saltos o con saltos inválidos | Ninguna |

- **Nunca** se escriben en `texto_contenido`, en el vector ni en el texto de BM25: se añaden al construir el JSON de respuesta. El ranking y el recall no cambian.
- **Subárbol:** al reconstruir la sección sin el solape (`reconstruir_sin_solape`), los saltos de cada ventana se trasladan al texto unido (`reconstruir_con_saltos`). `caracteres_totales`, `caracteres_entregados`, `desde_caracter` y el límite de tamaño siguen contando **solo el texto del documento**: las marcas no desplazan la paginación.
- **Saltos inválidos** (desplazamiento fuera del texto o fuera de frontera UTF-8, no crecientes, página 0, o primer salto distinto de 0): no se inserta ninguna marca y el resultado conserva `pagina_inicio`/`pagina_fin`. Mejor sin marca que con una falsa.

### 2.3 `nota` de las dos tools

Se añade a la nota actual:

> Las marcas ⟦p. N⟧ del texto las inserta el sistema e indican que lo que sigue está en la página física N del fichero; no forman parte del documento, y un [p. N] es texto del documento. Cita la página de la marca que precede al dato y, si el dato cruza una marca, cita las dos. Al citar una página concreta, acompáñala de un extracto literal breve entre «», que se comprueba automáticamente.

La última frase es la única adición respecto al texto de §4: la regla de la fase 2 solo actúa si hay un extracto literal junto a la página.

## 3. Campos nuevos en `cita` (fase 1)

En `search_normative_context` y en las dos `cita` de `stair_get_subtree_context`:

| Campo | Contrato |
| :- | :- |
| `titulo` | Título del documento de `01_documentos_catalogo`, el de la revisión del fragmento |
| `fecha_vigor` | `AAAA-MM-DD`, **solo si existe** en 01. Si es nula, la clave no aparece: nunca se inventa ni se rellena. `vigencia_no_declarada` sigue igual |

## 4. Arnés: anclaje (fase 1)

- La página concreta aparece ahora en la salida de la tool (`⟦p. 53⟧`), así que «p. 53» queda anclada por la regla de cifras sin cambiarla.
- **Falso bloqueo evitado:** el respaldo por cita literal (`cita_una_tool`, regla 9) busca la cita en las salidas del turno **sin las marcas**; si no, una cita que cruzara un salto no se encontraría.
- Las marcas solo añaden números a las fuentes del turno: no pueden provocar `CifraSinAnclar` ni `CifraEnAbstencion`.

## 5. Arnés: verificación de páginas (fase 2, E)

`VerificadorPaginas` (`arnes_turno/verificador_paginas.rs`), lógica pura. Se ejecuta **después** del guard, en todas las intenciones, salvo si la respuesta es `MENSAJE_RESPUESTA_NO_VALIDADA`. No rechaza ni reintenta: **anota**.

### 5.1 Qué se comprueba

1. **Extractos:** textos entre `«»`, `“”` o `""` de la respuesta cuyo esqueleto (§5.2) tiene al menos 12 caracteres. Como máximo 32 por respuesta.
2. **Página asociada:** la referencia de página más cercana **después** del cierre de comillas, en la misma línea, a 80 caracteres como mucho y antes de la siguiente apertura de comillas. Si no hay, la más cercana **antes** de la apertura, en las mismas condiciones. Formas: `p. N`, `pp. N-M`, `pág. N`, `págs. N y M`, `página(s) N`, `page(s) N`, `⟦p. N⟧`; listas con `,`, `y`, `and`, `-` o `–`. Un rango de más de 50 páginas cuenta solo por sus extremos. Sin página asociada, el extracto no se comprueba.
3. **Fuentes:** los campos `texto` (en cualquier anidamiento) de las salidas sin error de `search_normative_context` y `stair_get_subtree_context` **del turno actual**. Las páginas salen de las marcas `⟦p. N⟧`, que se derivan de `saltos_pagina` (§2), así que la página se confirma con los saltos guardados. Un texto sin marcas es una fuente «sin página».
4. **Metadatos:** un entrecomillado que no aparece en ningún texto pero sí en otra cadena de las salidas del turno (título del documento, breadcrumb, título de sección, código) no es un extracto del documento y no se revisa. Así, «TÍTULO DE LA SECCIÓN» (págs. 52-55) no genera una nota.

### 5.2 Normalización (esqueleto)

Se aplica igual al extracto y a las fuentes (estas, tras quitar las marcas y recordar la página de cada carácter):
1. Ligaduras a letras (`ﬁ ﬂ ﬀ ﬃ ﬄ ﬅ ﬆ`).
2. Minúsculas y sin tildes.
3. Solo se conservan letras y dígitos: desaparecen espacios, saltos de línea, guiones (también los partidos a final de línea y el guion blando), comillas tipográficas y demás puntuación.

### 5.3 Resultado por extracto

Con C = páginas citadas y V = páginas que cubre cada aparición del esqueleto en las fuentes con página:

| Caso | Condición | Qué hace |
| :- | :- | :- |
| Confirmada | Todas las apariciones dan el mismo V y V = C | Nada |
| Discrepante | Todas dan el mismo V y V ≠ C (incluye citar el rango del chunk cuando el extracto está en una página) | Corrige: «la respuesta cita C; el extracto está en V» |
| Ambigua | Dos o más V distintos | Lo dice: «aparece en V1 y V2: no se puede confirmar la página» |
| Sin página | Solo aparece en fuentes sin marcas | «el texto no tiene página registrada (p. ej., Markdown): la página citada no se puede comprobar» |
| No encontrada | No aparece | «no aparece literalmente en lo consultado en este turno: la página no está verificada» |

Dos apariciones con el mismo V (el solape entre chunks, o el mismo texto en la búsqueda y en el subárbol) son una sola.

### 5.4 Salida

- Si algún extracto no está confirmado, se **añade** al final de la respuesta un bloque fijo que empieza por `ENCABEZADO_VERIFICACION_PAGINAS` («**Verificación automática de páginas**») con una línea por extracto, el extracto recortado a 80 caracteres. El texto del modelo no se reescribe: en una respuesta que ya se ha emitido en streaming no se puede cambiar lo mostrado.
- En streaming el bloque se emite como un `TextDelta` más; en un turno retenido, junto con el resto. El `Completed` y el historial llevan la respuesta con el bloque.
- Telemetría: un `ArnesIntervino` con la etapa nueva `VerificacionPagina` y un motivo por extracto no confirmado. La GUI la explica como «comprobación de páginas», sin jerga.

### 5.5 Límites conocidos

- Un extracto parafraseado no se encuentra: queda «no verificada», no se corrige.
- Una página citada lejos de su extracto (otra línea o más de 80 caracteres) no se comprueba.
- La página es la física del fichero, no la impresa (`/PageLabels`).

## 6. Pruebas

- **Unitarias:** saltos por chunk en la ingesta, escritura y lectura Arrow, marcas, reconstrucción con saltos, `cita`, verificador, orquestador.
- **Integración:** ida y vuelta LanceDB → proyección → tools con un PDF sintético de varias páginas.
- **Adversariales** (`tests/adversarial_citas_pagina_tests.rs`): espacios y guiones partidos; comillas tipográficas y ligaduras; extracto repetido en dos páginas; marcas falsas `⟦p. 99⟧` y `[p. 99]` dentro del documento; chunks sin saltos; Markdown; saltos corruptos; muchas comillas y textos enormes.

## 7. Validación con Claude real

- `acceptance_8_problemas`, una pasada con Sonnet.
- Campaña Aguja 5 × 3 con el binario release de esta rama. `pagina_exacta_estricta` se mide sobre la respuesta completa y también sobre el texto del modelo sin el bloque de verificación (`pagina_exacta_estricta_modelo`), para separar el efecto de las marcas (fase 1) del de la corrección (fase 2).
- Variante de dos agujas casi iguales en páginas consecutivas del mismo chunk, con métrica conjunta valor + página (`valor_y_pagina`), 3 repeticiones.
