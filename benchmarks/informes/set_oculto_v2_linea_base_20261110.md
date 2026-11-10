# Set oculto v2: línea base de `main` (2026-11-10)

Binario release de `main` (`de9cae9`, sha256 `6d4f4d27…`), Sonnet, **2 pasadas** de 61 preguntas sobre un centro aislado (`LAKEHOUSE_RAG_CONFIG_DIR` propio). Hubo 0 fallos de ejecución.

El set (preguntas, extractos, PDF originales y modificados, respuestas crudas y revisiones manuales) está en `docs/validacion/set_oculto_v2/`, que no se versiona: tiene un `.gitignore` propio con `*`, y `git check-ignore` lo confirma. Este informe solo da agregados, ids y categorías. No incluye el texto de las preguntas ni de los documentos, ni los títulos del corpus, para que una sesión de arreglos no pueda afinar contra él.

**Lectura obligada.** En cada pasada, **57 de 61 respuestas son correctas (93 %) y hay 4 errores**. Los errores caen en las mismas 4 preguntas en las dos pasadas. **Ninguno se debe a la búsqueda ni al razonamiento del modelo**: 2 los provoca el propio arnés y 2 la lectura del PDF en la ingesta. Lo que mide el modelo (datos repartidos, cruces, trampas, premisas falsas y abstención) **satura al 100 %** en las dos pasadas. Eso significa «funciona en estos casos», no «funciona en general». En el apartado 6 propongo cómo endurecerlo.

## 1. Errores de respuesta

| Id | Categoría | Pasada 1 | Pasada 2 | Etapa donde se rompe |
|---|---|---|---|---|
| V22 | dato repartido (regla más excepción, legal) | **bloqueada**: no se entrega | **bloqueada** | **Arnés** (`salida_segura`). El extracto llegó al modelo y la respuesta era correcta. Se bloqueó porque citaba números de artículo que no figuraban literalmente en las salidas de las herramientas. |
| V53 | tabla con cálculo (financiero, EN) | **bloqueada** | **incompleta**: da las dos cifras, pero no la diferencia que se pedía | **Arnés**. La diferencia calculada entre dos celdas no aparece literal en las herramientas. En la pasada 1 el arnés bloqueó la respuesta. En la 2 forzó un reintento y el modelo **eliminó el cálculo correcto**. |
| V50 | cálculo (académico, ES) | abstención indebida | abstención indebida | **Ingesta: lectura del PDF**. El texto del cuerpo del documento quedó corrupto (caracteres sustituidos). |
| V51 | paráfrasis (académico, ES) | abstención indebida | incompleta | **Ingesta: lectura del PDF**. Mismo documento. |

Entregadas frente a correctas: en la pasada 1 se entregaron 59 de 61 (2 bloqueadas) y fueron correctas 57. En la pasada 2 se entregaron 60 y fueron correctas 57.

### 1.1 El arnés bloquea o mutila respuestas correctas (V22, V53)

- Lo que dispara la intervención es la regla «la respuesta declara que algo no consta y añade un valor que no aparece en la pregunta ni en ninguna salida de herramienta». Saltó **17 veces en 122 consultas** (9 en la pasada 1 y 8 en la 2): 14 reintentos y 3 bloqueos (`salida_segura`).
- **Medido:** de los valores señalados que he comprobado, ninguno era un dato inventado. Eran números de artículo, como el de la propia norma que se cita, o cifras calculadas a partir de datos recuperados (diferencias, sumas y redondeos de celdas). Las respuestas finales de esos casos fueron correctas, salvo las 4 dañadas (V22 dos veces y V53 dos veces).
- **Supuesto:** en un caso (`1.700` en V01) infiero que era un producto de dos datos de la pregunta. No lo comprobé de forma literal.
- **Efecto:** 4 de los 8 errores de las dos pasadas son pérdidas de respuestas correctas: 3 no entregadas y 1 recortada. La regla también añade latencia en los 13 casos restantes, en que el reintento no cambió nada sustancial.
- **Regla general propuesta** (no es un parche para estos casos):
  - Antes de tratar un valor como inventado, reconocer las referencias estructurales (números de artículo, apartado o recomendación) y los valores derivables aritméticamente de cifras presentes en las salidas (suma, resta, cociente y redondeo).
  - Además, que el disparador «declara que no consta» exija que la negación afecte al dato que se pregunta, y no a un matiz lateral de la respuesta.

### 1.2 Texto corrupto aceptado como «COMPLETO» (V50, V51)

- **Medido:**
  - Un artículo académico en español (11 páginas) quedó en el corpus con el texto de las páginas 2 a 11 ilegible, aunque la ingesta lo marcó con `estado_contenido` COMPLETO y sin `aviso_contenido`.
  - Esas páginas usan fuentes Type1 (CFF) con codificación propia (`/Encoding` con diferencias) y **sin** mapa ToUnicode. pypdf extrae el mismo PDF sin problemas.
  - La ingesta solo avisó de que 22 de 24 trozos superaban el límite de embeddings, que es un síntoma de texto sin espacios ni palabras.
- **Supuesto:** que el lector de PDF de la app no aplique la tabla de diferencias de la fuente. No he mirado el código (no toco `src/`).
- Un tercer caso del mismo documento (V49) salió correcto solo porque el modelo descifró a mano la sustitución de letras. Es frágil y no debe contarse como robustez.
- **Reglas generales propuestas:**
  1. Decodificar las fuentes simples con `/Differences` por nombre de glifo cuando falte ToUnicode.
  2. Antes de marcar un documento como COMPLETO, medir la legibilidad del texto extraído: proporción de palabras de diccionario o de bigramas plausibles por idioma. Si falla, marcarlo como no definitivo con aviso, que la búsqueda ya expone.

## 2. Página

Página en las respuestas correctas que tienen página aplicable:

| Pasada | Página correcta | Sin página |
|---|---|---|
| 1 | 45 | 2 |
| 2 | 46 | 1 |

Las respuestas sin página son tablas de doble entrada resueltas con `lookup_double_entry_table`, que no devuelve página. Es el mismo defecto que vio el set v1, ahora reproducido con otro documento y otro dominio (clínico). Cuando el modelo consultó además el texto, sí citó la página (V10 en la pasada 2).

No hubo ninguna página incorrecta. Dos ubicaciones alternativas válidas del mismo dato (informe de gestión frente a estados financieros) se aceptaron tras comprobarlas en el PDF.

## 3. Trampas

**8 de 8 respondidas con el valor del documento modificado, en las 2 pasadas. 0 respuestas de memoria.**

- Las 8 trampas cambian un valor conocido por el modelo en 8 documentos de 6 dominios y en los 2 idiomas (ES/EN).
- Varias están en apéndices, en notas o en el interior de documentos largos, y la pregunta usa otro vocabulario y a veces otro idioma.
- En 2 casos la respuesta menciona además un valor de memoria, y en los dos de forma legítima:
  - Una distingue correctamente una regla parecida de **otra** ley del corpus, que conserva el valor real.
  - Otra cita el umbral alterado y aplica bien la consecuencia.
- **Método:**
  - `trampas_pdf.py` edita los literales de texto en los flujos de contenido. Ahora admite también fuentes Type0 Identity-H con ToUnicode, que es el formato del BOE.
  - Un script comprobó, para cada trampa, que el texto original está en el PDF real y no está en el modificado, y que el nuevo sí está.
- **Límite:**
  - En las 8 trampas, la búsqueda recuperó el fragmento trucado: el extracto llegó al modelo.
  - Sigue sin medirse el caso más peligroso, en que el dato trucado **no** se recupera y el modelo rellena de memoria (ver el apartado 6).

## 4. Aciertos por categoría

Los números son iguales en las dos pasadas, salvo donde se indica.

| Categoría | n | Correctas |
|---|---|---|
| Paráfrasis (vocabulario coloquial o sinónimos, a veces en otro idioma) | 15 | 14 (falla V51, ingesta) |
| Dato repartido (regla más excepción, tabla más nota, varias páginas) | 10 | 9 (falla V22, arnés) |
| Sin respuesta en el corpus | 9 | 9 abstenciones, 0 cifras de memoria |
| Trampa | 8 | 8 |
| Tabla (con página) | 7 | 6 (falla V53, arnés) |
| Premisa falsa | 6 | 6 (una se abstiene sin rebatir la premisa; el documento tampoco la desmiente de forma explícita) |
| Cruce de dos documentos | 4 | 4 |
| Nota al pie | 1 | 1 |
| Cálculo | 1 | 0 (V50, ingesta) |

Sobre las preguntas sin respuesta:

- Hay 3 sobre un documento que la ingesta rechazó (ver el apartado 5). Son datos muy conocidos por el modelo, como una dirección de bus o una precisión de un sensor, y aun así el modelo no rellenó ninguno de memoria.
- Otra pregunta pedía una cuantía legal que el modelo conoce y que la norma del corpus no fija. Tampoco la dio.

## 5. Arnés, variación, ingesta y tiempos

Intervenciones del arnés (consultas con alguna intervención: 20 en la pasada 1 y 24 en la 2):

| Motivo | Pasada 1 | Pasada 2 | Valoración |
|---|---|---|---|
| Reintento «no consta + valor nuevo» | 7 | 7 | Todas sobre valores legítimos (ver el apartado 1.1): ruido |
| `salida_segura` (bloqueo) | 2 | 1 | Falsos bloqueos: errores de respuesta |
| `verificacion_pagina` «aparece en varias páginas; no se puede confirmar» | 8 | 8 | Casi siempre sobre títulos de sección o frases repetidas: ruido |
| `verificacion_pagina` «cita p. X, pero está en p. Y» | 3 | 6 | Al menos 4 falsos avisos comprobados (títulos del documento en la p. 1; una cita bien paginada atribuida a otra página) |
| `verificacion_pagina` «no aparece literalmente» | 0 | 2 | Citas abreviadas con «[…]» |
| Reintento «no se consultó la Capa 2» | 2 | 2 | Innecesarios: una pregunta no era de tabla y la otra era una abstención correcta |
| `directiva` Capa 2 | 1 | 1 | Correcta |

**Variación entre pasadas:** 2 de 61 preguntas cambian de veredicto, y las dos siguen siendo fallos con la misma causa. V51 pasa de abstención a incompleta y V53 de bloqueada a incompleta. El resto es estable. Dos pasadas son poca muestra para separar la variabilidad del modelo; tres serían mejor.

**Ingesta:**

- Corpus de 15 PDF, ~1.070 páginas, en ES y EN:
  - académico (3);
  - técnico o regulatorio (3, uno de ~300 páginas);
  - legal (3);
  - clínico (3);
  - financiero (2, uno de ~260 páginas);
  - manual de producto (1).
- Hay tablas, listas, anexos, notas al pie y figuras con texto.
- Tardó ~52 min en total, entre 39 y 516 s por documento.
- Un 16.º PDF (hoja de datos de producto) **estaba cifrado** con contraseña de propietario. La ingesta lo rechazó con un mensaje claro, que es el comportamiento correcto. No le quité la protección, quedó fuera del corpus y sus preguntas pasaron a la categoría «sin respuesta».

**Tiempos.** Las cifras de esta tabla **no permiten conclusiones**:

- La CPU estaba cargada: el usuario usaba el equipo y otras sesiones compilaban y ejecutaban benchmarks.
- En la pasada 2 hubo picos del 100 %.
- Son solo 2 ejecuciones.

| Pasada | Mediana | Rango | Total | CPU previa, mediana (máx.) |
|---|---|---|---|---|
| 1 | 44,6 s | 25–110 s | 51 min | 22 % (71 %) |
| 2 | 44,0 s | 24–110 s | 53 min | 51 % (100 %) |

## 6. El set casi satura: cómo endurecerlo

El 93 % viene de fallos de tubería (arnés e ingesta). En las capacidades del modelo y de la búsqueda, el set **no discrimina**: datos repartidos, cruces, trampas, premisas falsas y abstenciones salieron al 100 %. Propuestas para un v3, que debería escribir otra sesión exclusiva:

1. **Trampas no recuperables.**
   - Poner el valor trucado en un sitio al que la búsqueda difícilmente llega: una tabla en imagen, con una leyenda o un pie que lo declare en otra página, o una fila con un rótulo distinto del de la pregunta.
   - Así se mide de verdad si el modelo rellena de memoria cuando no ve el dato. Hoy, 8 de 8 trampas se recuperaron.
2. **Sin respuesta «casi»:** el corpus contiene el dato para una condición vecina (otra edad, otro tramo u otro año), pero no para la que se pregunta. Las 9 abstenciones actuales eran fáciles: el tema no estaba.
3. **Cruces de 3 o más documentos y cálculos encadenados** (conversión de unidades, varias celdas, ratios). Además, los cálculos son justo donde el arnés falla, así que conviene medirlos más.
4. **Contradicciones internas y entre ediciones del mismo documento.** Este corpus traía una sin buscarla (dos cifras distintas del mismo concepto en dos partes de un informe), y el modelo eligió una sin mencionar la otra.
5. **Más variedad de PDF problemático:** escaneados con OCR, fuentes sin ToUnicode, varias columnas, tablas giradas y documentos cifrados con permiso de lectura. La ingesta es la etapa que más falla aquí.
6. **3 pasadas** y más preguntas por categoría: hoy algunas categorías tienen n = 1.

## 7. Medido y supuesto; defectos del propio set

- **Medido:**
  - Veredictos: puntuador determinista y revisión manual de todas las respuestas marcadas como dudosas, fallidas o con claves sospechosas (21 revisiones en total).
  - Páginas citadas.
  - Si el extracto llegó a las herramientas.
  - Las intervenciones del arnés y sus valores.
  - Las fuentes del PDF corrupto.
  - Tiempos y CPU.
- **Supuesto:**
  - La causa interna del texto corrupto (la app ignoraría la tabla de diferencias).
  - Que el ruido de los avisos de página reste confianza al usuario.
  - Un valor (V01) que atribuyo a un cálculo.
- **Defectos del set corregidos tras la pasada 1**, solo en la puntuación y aplicados igual a las dos pasadas:
  - V57: el documento da dos cifras del mismo concepto, y ahora se aceptan ambas.
  - V50: la clave aceptaba un «6» suelto, y se endureció.
  - V56 y V59: se añadieron ubicaciones alternativas válidas, comprobadas en el PDF.
  - Ninguno cambia la app ni el contenido de las preguntas.
- **Sesgo del propio set:**
  - Lo escribió esta sesión leyendo los documentos con pypdf. Por eso eligió datos localizables por un extractor razonable, y los dos fallos de ingesta los encontró de forma incidental, no por diseño.
  - Las trampas se colocaron en datos conocidos por el modelo, pero en lugares que la búsqueda acabó recuperando.

## Reproducir sobre otra rama (un comando)

Desde el worktree que guarda el set (`claude/set-oculto-v2-f844c1`):

```bash
python -I benchmarks/set_oculto_v2/medir.py --etiqueta arreglos --compilar <worktree_de_la_rama>
```

El comando:

1. Compila y copia el binario.
2. Crea un centro aislado e ingiere el corpus.
3. Hace 2 pasadas y guarda las respuestas crudas.
4. Puntúa y compara con `resumen_main.json`, pregunta a pregunta. Sale con **código 1 si algo empeora** frente a `main`, en veredicto o en página (CLAUDE.md §6).

Las respuestas «dudosa», «incompleta» o «correcta_menciona_memoria» necesitan revisión manual: se apunta en `revision_<etiqueta>_p<k>.json` y se repuntúa sin gastar Claude:

```bash
python -I benchmarks/set_oculto_v2/medir.py --etiqueta arreglos --solo-puntuar
```

Los scripts están en `benchmarks/set_oculto_v2/`: `trampas_pdf.py`, `verificar_extractos.py`, `ejecutar.py`, `puntuar.py`, `resumen.py` y `medir.py`. Ninguno contiene texto de documentos ni de preguntas.
