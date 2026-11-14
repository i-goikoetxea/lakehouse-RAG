# Set oculto v2: piezas 1, 2 y 3c (`ad1fbb3`) frente a la línea base (2026-11-11/12)

`main` local en `ad1fbb3` (sha256 del binario `861e7dc3…`), compilado en un worktree aparte con la DLL de pdfium fijada (`4b0dec92…f994`). No se leyó el código ni los informes de las piezas. Hubo una ingesta nueva en un centro aislado, con destilación con Sonnet.

Comparo con:

- **la línea base**: `main` en `de9cae9`;
- **la rama de arreglos**: `932766c`, medida antes.

Se hicieron 2 pasadas completas de 61 preguntas. Además, repeticiones dirigidas con los dos binarios, piezas y la línea base: V06 hasta 10 ejecuciones; V22 y V53, 5 en piezas; V56, V58 y V60, 10 en cada binario. Sin el texto de las preguntas ni de los documentos.

**Lectura obligada.**

- Pasadas completas:

  | Binario | Correctas |
  |---|---|
  | Piezas | **116/122** (61 + 55) |
  | Línea base | 114/122 |
  | Arreglos | 115/122 |

- **La pieza 1 arregla el error de ingesta más grave.** El documento académico cuyo texto quedaba corrupto ahora se lee bien, y sus preguntas pasan de 0/4 a 4/4 (V50 y V51; V49 ya no necesita descifrar nada).
- **La pieza 3c elimina casi todo el ruido de avisos de página**: de 15-16 avisos por pasada a 0-1, sin que aparezca ninguna página incorrecta.
- **La pieza 2 no se observa en este corpus.** La tabla de doble entrada no devolvió página en ninguna de las 5 tablas consultadas. Desaparece la página falsa que traía la rama de arreglos, pero las respuestas de tabla siguen sin página cuando solo usan esa herramienta.
- **Errores:** los 6 de la pasada 2 se deben a 5 bloqueos del arnés sobre cifras calculadas o números de artículo y a 1 fallo de búsqueda.
- **Retroceso posible, no demostrado:** en las preguntas de cálculo, el arnés bloquea **5 de 30** ejecuciones con piezas frente a **1 de 30** en la línea base (las aciertos son 25/30 frente a 27/30). Lo explico en el apartado 1.2.

## 1. Errores de respuesta

| Id | Categoría | Línea base (p1 / p2) | Piezas (p1 / p2) | Etapa en piezas |
|---|---|---|---|---|
| V22 | dato repartido (legal) | bloqueada / bloqueada | correcta / **bloqueada** | **Arnés** (`salida_segura` por un número de artículo) |
| V53 | tabla con cálculo | bloqueada / incompleta | correcta / **bloqueada** | **Arnés** (diferencia calculada) |
| V56 | tabla con cálculo | correcta / correcta | correcta / **bloqueada** | **Arnés** (diferencia calculada) |
| V58 | dato repartido con suma | correcta / correcta | correcta / **bloqueada** | **Arnés** (suma calculada) |
| V60 | dato repartido con resta | correcta / correcta | correcta / **bloqueada** | **Arnés** (resta calculada) |
| V06 | cruce de dos documentos | correcta / correcta | correcta / incompleta | **Búsqueda**: no recupera el segundo documento; ahora dice que no puede confirmar ni descartar, en vez de afirmar que no consta |
| V50 | cálculo (académico, ES) | abstención / abstención | **correcta / correcta** | arreglado (lectura del PDF) |
| V51 | paráfrasis (académico, ES) | abstención / incompleta | **correcta / correcta** | arreglado |

Entregadas frente a correctas en piezas:

| Pasada | Entregadas | Correctas | Bloqueadas por el arnés |
|---|---|---|---|
| 1 | 61 | 61 | 0 |
| 2 | 56 | 55 | 5 |

### 1.1 Repeticiones: qué es variabilidad y qué no

Correctas sobre el total de ejecuciones (pasadas completas más repeticiones), con los bloqueos del arnés entre paréntesis:

| Id | Línea base | Arreglos | Piezas |
|---|---|---|---|
| V06 | 8/10 (0) | 6/10 (0) | 8/10 (0) |
| V22 | 1/5 (4) | 2/2 (0) | 2/5 (3) |
| V53 | 5/8 (1) | 4/5 (1) | 4/5 (1) |
| V56 | 8/10 (0) | 2/2 (0) | 9/10 (1) |
| V58 | 9/10 (1) | 2/2 (0) | 7/10 (3) |
| V60 | 10/10 (0) | 2/2 (0) | 9/10 (1) |

- **V06** es igual que en la línea base (8/10 en las dos). No es un retroceso; es un defecto previo de búsqueda entre documentos.
- **V22** sigue bloqueándose, como en la línea base. El arreglo que en la rama de arreglos lo dejó en 2/2 no se nota aquí. **Supuesto:** que ese cambio no esté en `ad1fbb3`; no lo he comprobado leyendo el código.
- Los fallos de la línea base en V56 que no son bloqueos se deben a una cifra de otro perímetro (búsqueda), el mismo patrón que V59.

### 1.2 Bloqueos del arnés en cálculos: retroceso posible

- **Medido:**
  - En V56, V58 y V60, el arnés bloqueó 5 de 30 ejecuciones con piezas frente a 1 de 30 en la línea base. Los aciertos son 25/30 frente a 27/30.
  - Todos los bloqueos tienen el mismo motivo: «la respuesta declara que algo no consta y añade un valor», y en todos el valor es una suma o una resta de cifras recuperadas.
  - De los 5 bloqueos de piezas en estas preguntas, 3 cayeron en la misma pasada (la 2), seguidos.
- **No demostrado:**
  - Con 30 ejecuciones por lado, la diferencia de bloqueos (5 frente a 1) no es estadísticamente concluyente (prueba exacta de Fisher, p≈0,2).
  - La agrupación en una pasada sugiere variabilidad del modelo en cómo redacta la respuesta: añade un «no consta» lateral, y eso dispara la regla.
- **Por la regla de CLAUDE.md §6**, lo marco como **posible retroceso** que merece decisión.
- **La regla general propuesta** en los informes anteriores lo elimina en cualquier caso: no tratar como inventado un valor derivable aritméticamente de cifras presentes, ni una referencia estructural (número de artículo). Con eso, esta duda deja de importar.

## 2. Pregunta por pregunta frente a la línea base

Peor veredicto de las 2 pasadas completas. Las 53 preguntas no listadas son correctas en las 2 pasadas de los dos binarios.

| Id | Línea base | Piezas | Cambio | Con repeticiones |
|---|---|---|---|---|
| V50 | abstención indebida | correcta | **mejora** | 0/2 → 2/2 |
| V51 | abstención indebida | correcta | **mejora** | 0/2 → 2/2 |
| V22 | incorrecta (bloqueo) | incorrecta (bloqueo) | igual | 1/5 → 2/5 |
| V53 | incorrecta (bloqueo) | incorrecta (bloqueo) | igual | 5/8 → 4/5 |
| V06 | correcta | incompleta | retroceso candidato, **descartado** | 8/10 → 8/10 |
| V56 | correcta | incorrecta (bloqueo) | **retroceso posible** | 8/10 → 9/10 (bloqueos 0 → 1) |
| V58 | correcta | incorrecta (bloqueo) | **retroceso posible** | 9/10 → 7/10 (bloqueos 1 → 3) |
| V60 | correcta | incorrecta (bloqueo) | **retroceso posible** | 10/10 → 9/10 (bloqueos 0 → 1) |

Retrocesos de página: ninguno. Las páginas que la comparación automática marcó como «sin página» en V56, V58 y V60 son las respuestas bloqueadas, que no traen texto.

## 3. Página

Página en las respuestas correctas con página aplicable:

| | Línea base p1 | Línea base p2 | Arreglos p1 | Arreglos p2 | Piezas p1 | Piezas p2 |
|---|---|---|---|---|---|---|
| Correcta | 45 | 46 | 48 | 44 | 49 | 43 |
| Sin página | 2 | 1 | 1 | 1 | 2 | 2 |
| Incorrecta | 0 | 0 | 0 | 1 | **0** | **0** |

Tablas de doble entrada (`lookup_double_entry_table`):

- **Medido:** en las 2 pasadas, ninguna de las 5 tablas consultadas (dos fichas clínicas, una guía clínica, un documento técnico y un artículo) devolvió un campo de página en la `cita`.
- V10, la tabla que en arreglos daba una página falsa, sale ahora **sin página** en las dos pasadas: el retroceso de arreglos ya no está.
- V05 y V10 siguen sin página cuando el modelo solo usa esa herramienta. Cuando consulta además el texto, la página sale bien (V24, V47).
- **Supuesto:** que la pieza 2 solo rellene la página cuando la localiza con certeza, y que en estas tablas no la localizara. No es comprobable sin leer el código; desde fuera, la mejora no se observa en este corpus.

Avisos de página (pieza 3c):

| Binario | Pasada 1 | Pasada 2 |
|---|---|---|
| Línea base | 15 | 16 |
| Arreglos | 6 | 5 |
| Piezas | **1** | **0** |

- El único aviso que queda es «cita p. 10, pero el texto está en p. 9», sobre una frase que cruza dos páginas: es un aviso de valor dudoso.
- No apareció ninguna página incorrecta sin aviso, así que la reducción no esconde errores en esta muestra.

## 4. Trampas, abstenciones y «no consta»

- **Trampas:** 8/8 en las 2 pasadas, sin respuestas de memoria (igual que la línea base). Las 2 menciones legítimas de un valor real son las mismas de siempre.
- **Sin respuesta:** 9/9 abstenciones correctas en las 2 pasadas. En una, el modelo nombra un valor de memoria solo como término de búsqueda y no lo da como respuesta; lo revisé a mano.
- **Reintentos «no consta + valor»:** 4 en la pasada 1 y 6 en la 2, todos sobre valores legítimos. En 5 casos terminaron en bloqueo (apartado 1.2).

## 5. Compuerta de legibilidad (pieza 1), calibrada con los 15 PDF

- **Páginas sin texto.** La ingesta avisó: «Páginas 1-9, 234 sin texto extraíble» en el informe financiero de 262 páginas. Son **exactamente** las páginas que pypdf ve vacías (menos de 30 caracteres): portadas e imágenes. Ningún otro de los 14 documentos tuvo avisos: **0 falsos rechazos**.
- **`LECTURA_DEGRADADA`:** no la marcó en ninguna página del corpus. Todos los documentos quedan `COMPLETO`.
- **El documento académico corrupto:**
  - Con pdfium ya no está corrupto: su texto se lee bien desde la p. 2 (comprobado por SQL sobre el centro, vía `--mcp-server`).
  - Desaparece el aviso de «22 de 24 textos superan el límite de embeddings», que era el síntoma.
  - La compuerta no tuvo nada que detectar.
- **Prueba sintética** (PDF generados aquí, con texto propio):
  - una página de solo símbolos (`#$%&…`);
  - dos páginas cifradas por sustitución de letras, el mismo síntoma que tenía el documento académico.
  - Las dos quedaron **`COMPLETO`, sin aviso**, y su texto pasa tal cual a la búsqueda.
  - **Medido:** la compuerta no detecta texto ilegible formado por caracteres Unicode válidos.
  - **Supuesto:** que se base en otras señales, como glifos sin mapear o páginas sin texto.
  - Hoy el corpus no lo necesita, pero si otro PDF trae una codificación que ni pdfium resuelva, el texto basura volvería a entrar como `COMPLETO`.
  - Propuesta de regla general: una medida de plausibilidad lingüística por página (proporción de palabras de diccionario o de bigramas frecuentes del idioma detectado).
- **Fallo de ingesta (medido):**
  - El informe financiero de 262 páginas falló una vez: el motor de embeddings (ONNX) no pudo reservar un búfer de 4 GiB.
  - Fue con la CPU al 100 %, mientras otra sesión ejecutaba `cargo test`.
  - Al reintentarlo aislado, con la CPU al 18 % y 27,7 GB libres, se ingirió bien.
  - **Supuesto:** la causa es la presión de memoria del momento. Con un fallo y un éxito no se puede descartar otra causa. En la línea base y en arreglos ese documento nunca falló.

## 6. Carga del equipo y tiempos

No saco conclusiones de los tiempos:

- Al empezar había un `cargo test` de otra sesión, con la CPU al 100 %.
- La CPU previa a cada consulta tuvo medianas del 25-28 %, con picos del 97 %.

| Medida | Piezas | Línea base |
|---|---|---|
| Ingesta | ~106 min más el reintento | ~52 min |
| Latencia mediana por pregunta | 33 y 34 s | 44 y 45 s |

La compilación con dependencias nuevas tardó 23 min.

## 7. Medido y supuesto; ajustes de la puntuación

- **Medido:**
  - Veredictos, con revisión manual de todas las respuestas dudosas, incompletas o fallidas.
  - Páginas y campos de página de la Capa 2.
  - Intervenciones del arnés.
  - 68 ejecuciones extra.
  - Estados de ingesta, avisos y texto en el centro (SQL por MCP).
  - Las pruebas sintéticas de la compuerta.
- **Supuesto:**
  - Las causas internas (página de la Capa 2, criterio de la compuerta, fallo de memoria del ONNX).
  - Que el aumento de bloqueos sea variabilidad.
- **Ejecuciones descartadas:** 30 repeticiones fallaron por el límite semanal de Claude («weekly limit»), no por la app. Se repitieron tras el reinicio y las fallidas no cuentan.
- **Ajustes de la puntuación**, aplicados por igual a la línea base, a arreglos y a piezas, que se repuntuaron:
  - El puntuador lee páginas citadas en columnas «Página» de tablas markdown.
  - Reconoce «no indica» como abstención.
  - Se aceptan dos ubicaciones válidas más, comprobadas en el PDF: el criterio en el glosario y en otro capítulo (V29), y el anexo con la cifra (V57).
  - Se corrigió un carácter de control que se había colado en una regex del puntuador.
  - Ningún resultado de la línea base cambia.
- **El set satura otra vez:** la pasada 1 dio 61/61. Hay que endurecerlo (ver el informe de línea base, apartado 6). La única variación que queda está en el arnés y en la búsqueda entre documentos.

## Propuestas, por prioridad

1. **Arnés: valores derivados y referencias estructurales.** Es la regla general pendiente desde la línea base. Es la causa de los 5 errores de la pasada 2 y del posible retroceso.
2. **Página de la Capa 2.** Medir con un corpus propio de tablas si la pieza 2 rellena la página. En este corpus no apareció en ninguna tabla.
3. **Compuerta de legibilidad:** añadir la plausibilidad lingüística por página. Las pruebas sintéticas pasan sin aviso.
4. **Cruces entre documentos:** antes de decir que algo no consta, buscar en los dos documentos que nombra la pregunta. Es un defecto previo (8/10 en todos los binarios).
5. **Ingesta de documentos grandes:** limitar el tamaño de lote del modelo de embeddings para no pedir búferes de 4 GiB.

## Reproducir

```bash
python -I benchmarks/set_oculto_v2/medir.py --etiqueta piezas --binario <dir>/lakehouse_rag_cli.exe
```

`medir.py` copia también las DLL que haya junto al binario (pdfium). Las consultas SQL de auditoría del centro usan `benchmarks/set_oculto_v2/consulta_sql.py`.
