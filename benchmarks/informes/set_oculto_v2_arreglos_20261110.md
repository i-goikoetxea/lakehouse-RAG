# Set oculto v2: rama de arreglos frente a `main` (2026-11-10)

## Qué se comparó

| | Línea base | Arreglos |
|---|---|---|
| Rama | `main` | `claude/arreglos-rag-512d66` |
| Commit | `de9cae9` | `932766c` |
| Binario (sha256) | `6d4f4d27…` | `7aef90b4…` |

- El binario de arreglos se compiló en un worktree aparte, desacoplado de la rama. No se leyó ni su código ni sus informes.
- Mismo set: 61 preguntas sobre 15 PDF. Mismo modelo (Sonnet). Cada binario usó su propio centro aislado con una ingesta nueva.
- Se hicieron **2 pasadas completas** con cada binario.
- Además, **repeticiones dirigidas** sobre las preguntas que cambiaron (en `main` y en arreglos):
  - 3 más de V53 y V59;
  - 8 más de V06.
- Sin el texto de las preguntas ni de los documentos. El set sigue en `docs/validacion/set_oculto_v2/`, ignorado por git.

**Lectura obligada.** En las 2 pasadas, arreglos da **115/122 correctas** y `main` **114/122**. La diferencia está dentro de la variabilidad del modelo.

Arreglos **elimina un error** de forma estable:
- V22: un falso bloqueo del arnés, de 0/2 a 2/2.

Arreglos **reduce otro**:
- V53: el arnés bloqueaba un cálculo; pasa de 2/5 a 4/5 correctas.

Arreglos **no arregla** el texto corrupto de la ingesta: V50 y V51 siguen fallando en 0 de 4 ejecuciones.

Introduce **un retroceso de página**, que bloquea el merge según CLAUDE.md §6:
- La tabla de doble entrada devuelve ahora una página **falsa** para una tabla.

No hay retrocesos de veredicto demostrados:
- Los dos candidatos que marcó la comparación automática (V06 y V59) también fallan en `main` cuando se repiten.

## 1. Errores de respuesta

Errores en las 2 pasadas completas (61 × 2 por binario):

| Id | Categoría | `main` p1 / p2 | Arreglos p1 / p2 | Etapa |
|---|---|---|---|---|
| V50 | cálculo (académico, ES) | abstención / abstención | abstención / abstención | **Ingesta**: texto corrupto marcado COMPLETO. Sin cambios |
| V51 | paráfrasis (académico, ES) | abstención / incompleta | incompleta / abstención | **Ingesta**: igual |
| V53 | tabla con cálculo (financiero, EN) | **bloqueada** / incompleta | correcta / **bloqueada** | **Arnés**: `salida_segura` sobre una diferencia calculada |
| V22 | dato repartido (legal) | **bloqueada** / **bloqueada** | correcta / correcta | **Arnés** en `main`; arreglado |
| V06 | cruce de dos documentos (clínico) | correcta / correcta | correcta / **incorrecta** | **Búsqueda más respuesta**: no recupera el segundo documento y afirma que el dato no consta. Es una negación sin evidencia |
| V59 | cruce de dos documentos (financiero) | correcta / correcta | correcta / incompleta | **Búsqueda**: toma la cifra de otro perímetro (grupo en lugar de entidad) y lo advierte |

Entregadas frente a correctas:

| Pasada | `main` entregadas | `main` correctas | Arreglos entregadas | Arreglos correctas |
|---|---|---|---|---|
| 1 | 59 | 57 | 61 | 59 |
| 2 | 60 | 57 | 60 | 56 |

### 1.1 Retrocesos candidatos: ¿son del cambio?

La comparación automática pregunta a pregunta (`resumen.py --base`) marcó V06 y V59 como retrocesos, porque eran correctas en `main` y fallaron una vez en arreglos. Con 2 pasadas no se puede separar la variabilidad del modelo de un efecto del cambio, así que repetí esas preguntas con **los dos binarios**:

| Id | `main` correctas | Arreglos correctas | Conclusión |
|---|---|---|---|
| V06 | **8/10** | **6/10** | El mismo fallo aparece en `main` (2 veces), con la misma negación. Es un defecto previo de recuperación entre documentos. La diferencia de 8 a 6 no es concluyente con 10 ejecuciones. **Vigilar** |
| V59 | **3/5** | **4/5** | Falla también en `main`, con el mismo cambio de perímetro. No es un retroceso |
| V53 | **2/5** | **4/5** | Mejora, pero el falso bloqueo **persiste** (1 de 5) |
| V22 | 0/2 | 2/2 | Mejora |

Medido, de pasada, en `main`: en una de las repeticiones de V59, `main` **entregó como respuesta el texto en crudo de llamadas a herramientas**, en 8 s y sin ejecutar ninguna herramienta. El arnés no lo detuvo. No se observó en arreglos, pero con una sola ocurrencia no se puede decir si está corregido.

## 2. Pregunta por pregunta frente a la línea base

Peor veredicto de las 2 pasadas completas. Las 55 preguntas no listadas son correctas, o abstenciones correctas, en las 4 ejecuciones.

| Id | Categoría | `main` | Arreglos | Cambio | Con repeticiones |
|---|---|---|---|---|---|
| V22 | dato repartido | incorrecta (bloqueo) | correcta | **mejora** | 0/2 → 2/2 |
| V53 | tabla, cálculo | incorrecta (bloqueo) | incorrecta (bloqueo) | igual en el peor caso | 2/5 → 4/5 |
| V50 | cálculo | abstención indebida | abstención indebida | igual | 0/2 → 0/2 |
| V51 | paráfrasis | abstención indebida | abstención indebida | igual | 0/2 → 0/2 |
| V06 | cruce | correcta | incorrecta | **retroceso candidato, no confirmado** | 8/10 → 6/10 |
| V59 | cruce | correcta | incompleta | **retroceso candidato, descartado** | 3/5 → 4/5 |

## 3. Retrocesos

- **Página, confirmado (bloquea el merge según §6): V10, una tabla clínica.**
  - **Medido:** en arreglos, `lookup_double_entry_table` devuelve ahora la página de la celda. Para esta tabla devuelve la **p. 6**, cuando la tabla está en las pp. 21-22.
  - En la pasada 1 el modelo detectó la contradicción y citó bien la p. 22. En la pasada 2 confió en la herramienta y citó la p. 6. En `main` la herramienta no daba página: la respuesta salía sin página (pasada 1) o con la correcta, por el texto (pasada 2).
  - Una página falsa engaña más que no dar página.
  - **Supuesto:** que la página procede del apartado de contexto que la herramienta adjunta (apartado 1.1, p. 6), y no de la tabla. El propio modelo lo señaló en una respuesta.
  - **Etapa:** ingesta o troceo de tablas (la página asignada a la celda).
- **Veredicto:** ninguno confirmado (ver el apartado 1.1). V06 queda en vigilancia.

## 4. Página

Página en las respuestas correctas con página aplicable:

| | `main` p1 | `main` p2 | Arreglos p1 | Arreglos p2 |
|---|---|---|---|---|
| Correcta | 45 | 46 | 48 | 44 |
| Sin página | 2 | 1 | 1 | 1 |
| **Incorrecta** | 0 | 0 | 0 | **1** (V10) |

Las páginas que devuelve la Capa 2 en arreglos, medidas en las 6 tablas que se consultaron:

| Tablas consultadas | Con página | Página correcta | Página incorrecta | Sin página |
|---|---|---|---|---|
| 6 | 2 | 1 (p. 8) | 1 (p. 6 en lugar de 21-22) | 4 (dos fichas clínicas, una tabla financiera y la de variantes de un artículo) |

La mejora prometida (que las tablas tengan página) solo se ve en 1 de 6 tablas, y en otra introduce un error. Propuesta de regla general:

- Tomar la página de la propia celda o fila, la del chunk de donde se extrajo.
- No usar el apartado de contexto.
- Si no hay certeza, devolver «sin página» antes que una página dudosa.

## 5. Trampas

8 de 8 en las 4 ejecuciones completas, en `main` y en arreglos, con 0 respuestas de memoria. No hay cambios. Las 2 menciones legítimas de un valor real son las mismas que en la base (un umbral de otra ley del corpus y el umbral alterado aplicado bien).

## 6. Arnés

| Intervención | `main` p1 | `main` p2 | Arreglos p1 | Arreglos p2 |
|---|---|---|---|---|
| Consultas con alguna intervención | 20 | 24 | **12** | **12** |
| Intervenciones totales | 23 | 27 | **12** | **13** |
| `salida_segura` (bloqueo) | 2 | 1 | **0** | 1 |
| Reintento «no consta + valor nuevo» | 7 | 7 | 4 | 5 |
| `verificacion_pagina` «aparece en varias páginas» | 8 | 8 | **0** | **0** |
| `verificacion_pagina` «cita p. X, pero está en p. Y» | 3 | 6 | 6 | 5 |
| `verificacion_pagina` «no aparece literalmente» | 0 | 2 | 0 | 0 |
| Reintento «no se consultó la Capa 2» | 2 | 2 | 1 | 1 |
| `directiva` Capa 2 | 1 | 1 | 1 | 1 |

- **Mejora medida:**
  - Las intervenciones bajan aproximadamente a la mitad.
  - Desaparecen los avisos «no se puede confirmar» de títulos repetidos.
  - Los bloqueos bajan de 3 a 1.
- **Persiste:** el reintento «no consta + valor nuevo» sigue saltando sobre valores legítimos (números de artículo y cifras calculadas). Ya no suele dañar la respuesta, pero en 1 de 61 terminó en bloqueo (V53).
- **Ruido que queda:** los avisos «cita p. X, pero está en p. Y» siguen dándose sobre el título del documento, que está en la p. 1. Al menos 3 de los revisados en arreglos son falsos avisos.

## 7. Ingesta y tiempos

- **Ingesta:** el texto corrupto del documento académico sigue igual (con el mismo aviso, «22 de 24 textos superan el límite de embeddings», y marcado COMPLETO). El documento cifrado sigue rechazado. La ingesta duró ~65 min, frente a ~52 min en `main`.
- **Latencia:** mediana por pregunta de 37 s y 36 s en arreglos, frente a 45 s y 44 s en `main`. **No concluyo nada de esto**:
  - La CPU previa a cada consulta varió mucho entre ejecuciones (medianas del 9 % al 51 %, con picos del 100 %).
  - El usuario y otras sesiones usaban el equipo.
  - Es plausible que el menor número de reintentos explique parte de la diferencia, pero no está medido.

## 8. Medido y supuesto

- **Medido:**
  - Veredictos: puntuador determinista y revisión manual de cada respuesta dudosa, incompleta o fallida.
  - Páginas citadas y páginas devueltas por la Capa 2.
  - Intervenciones del arnés.
  - Repeticiones (28 consultas extra).
- **Supuesto:**
  - El origen de la página errónea de la Capa 2.
  - Que la diferencia en V06 sea solo variabilidad: 8/10 frente a 6/10 no lo prueba ni lo descarta.
- **Ajustes de la puntuación** hechos en esta medición y aplicados por igual a la base, que se repuntuó:
  - V06: la clave aceptaba un «10 años» de la otra ficha; ahora exige que el segundo producto figure como autorizado para esa edad.
  - V29: se acepta el criterio de «más de tres sistemas».
  - Una página incorrecta pesa más que no dar página.
  - Ninguno cambia el resultado de la base.
- **El set sigue cerca de saturar** (93-97 %). Las propuestas del informe de línea base para endurecerlo siguen vigentes.

## Propuestas, por prioridad

1. **Página de la Capa 2.** Tomarla de la celda o del chunk de origen, nunca del apartado de contexto. Ante la duda, no dar página. Es un retroceso confirmado.
2. **Texto corrupto en la ingesta.** Sin cambios respecto a la base: decodificar las fuentes con tabla de diferencias y comprobar la legibilidad antes de marcar un documento como COMPLETO.
3. **Reintento «no consta + valor nuevo».** Excluir las referencias estructurales y los valores derivables de cifras presentes. Ya no es grave, pero bloquea en 1 de 61.
4. **Cruces entre documentos con negación.** Cuando la pregunta nombra dos productos o documentos, buscar en los dos antes de afirmar que algo no consta. Si no se busca, decir «no lo he encontrado» en lugar de «no consta». Es un defecto previo; medido con 10 repeticiones por binario.
5. **Respuestas con llamadas a herramientas en crudo.** El arnés debería detectarlas antes de entregar. Se vio una vez en `main`.

## Reproducir

```bash
python -I benchmarks/set_oculto_v2/medir.py --etiqueta arreglos --compilar <worktree>
```

Usa `CARGO_TARGET_DIR` si está definido. Para repuntuar sin gastar Claude:

```bash
python -I benchmarks/set_oculto_v2/medir.py --etiqueta arreglos --solo-puntuar
```

Las repeticiones dirigidas usan `ejecutar.py preguntar` con un subconjunto del set y una `--salida` por repetición.
