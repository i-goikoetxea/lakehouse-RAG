# SSD: el arnés frena las negaciones sin evidencia

**Fecha:** 2026-11-10. **Rama:** `claude/arreglos-rag-512d66`. **Código:** `arnes_turno/negaciones.rs`, `post_inference_guard.rs` (`ViolacionArnes::NegacionSinBusqueda`).

## 1. Problema

Una respuesta a una pregunta de límite recuperó solo la fila de la tabla y añadió «no hay ampliación», sin haber leído el apartado. El arnés no lo frenó: ninguna regla miraba las afirmaciones de ausencia.

## 2. Regla

En un turno con intención distinta de `Ninguna` (los turnos retenidos; en streaming el texto ya se mostró), el guard rechaza la respuesta con `NegacionSinBusqueda { aspecto }` y aplica el mecanismo existente (un reintento con recordatorio y, si insiste, `MENSAJE_RESPUESTA_NO_VALIDADA`) si se cumplen **todas**:

1. **El turno no leyó texto del documento:** ninguna llamada ni salida de `search_normative_context` o `stair_get_subtree_context`. Si el modelo leyó texto, su negación no es «sin buscar» (puede ser errónea, pero no es lo que esta regla detecta).
2. **La respuesta afirma con rotundidad una ausencia sobre el mundo**, fuera de las comillas (lo entrecomillado es texto del documento). Lista cerrada de locuciones:
   - existenciales (el aspecto es la primera palabra de contenido que sigue): «no hay», «no existe(n)», «no se contempla(n)», «no se prevé(n)», «no se establece(n)», «no se admite(n)», «there is/are no», «there is/are not any»;
   - verbales (el aspecto es la palabra de contenido que precede): «no (se) aplica(n)», «does/do not apply».
   - No entran las abstenciones sobre la búsqueda («no consta», «no he encontrado», «no aparece en lo consultado») ni las descripciones del resultado («no figura», «no incluye»).
   - Tras la locución, «que», «nada», «nadie» anulan la negación («no hay que…», «no hay nada más»). Se saltan artículos, cuantificadores y palabras genéricas de abstención («datos», «información», «referencia»…). Un aspecto necesita 4 letras o más y ninguna cifra.
3. **El aspecto no está buscado:** su raíz (5 primeras letras) no aparece en lo consultado: los argumentos de las llamadas del turno, el texto y las claves de las salidas válidas, y el vocabulario de las instrucciones de la app (el system prompt: «no hay conflicto de prelación» lee un campo que la app define, no afirma nada del mundo). Si el aspecto está en la pregunta y el turno consultó alguna tool, cuenta como buscado con otras palabras.
4. **Etiqueta:** si la negación abre lo que sigue a dos puntos («Nota al pie (`nota_pie_condicion`): no hay ninguna asociada»), las palabras de la etiqueta también nombran el aspecto; si alguna está buscada, pasa.

Coste: lo consultado solo se indexa si la respuesta contiene alguna negación; como mucho 16 negaciones por respuesta.

## 3. Condición de no retroceso

| Caso | Por qué no se ve afectado |
| :- | :- |
| «No consta X» tras una búsqueda vacía | «no consta» no es una locución de la regla; además, buscar es leer texto (regla 1) |
| «No hay X» tras buscar X con la búsqueda | Regla 1 |
| «No hay X» tras un cruce o SQL que devolvió X o sus palabras | Regla 3 |
| Descripciones del resultado («no hay conflicto», «no hay nota al pie», «el resultado no incluye página») | Vocabulario de la app, etiquetas y lista cerrada |
| Respuestas de las suites | Tests unitarios, del guard y del orquestador; repetición de 111 respuestas guardadas: 0 rechazos, igual que la línea base |

## 4. Límites conocidos

- La lista de locuciones es española e inglesa, como el detector de intención. Otros idiomas no se frenan.
- Un turno que buscó texto de otro tema y luego niega un aspecto no buscado no se frena (regla 1, elegida para no retroceder).
- No entiende la frase: «Carga máxima: 12 t, no hay ampliación» se frena aunque la ampliación figure en un documento no consultado; es lo buscado. Pero una negación con el aspecto elidido («no hay ninguna») toma la palabra siguiente como aspecto.
- **Sesgo:** la versión 1 de la regla daba 9 falsos rechazos en las respuestas guardadas de RGB y ciclo de vida (afirmaciones sobre el propio resultado). La versión 2 (reglas 1, 3 con el vocabulario de la app y 4) se diseñó después de verlos. Son reglas estructurales, sin palabras de esos documentos, pero se ajustaron con un benchmark propio: el set v2 debe medirlas.
