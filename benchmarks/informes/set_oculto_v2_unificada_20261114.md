# Set oculto v2: medición unificada de `main` `da66519` (2026-11-14)

## Qué se midió

- **Código:** `main` local en `da66519`, que integra las piezas 1, 2, 3b, 3c y 4, el ajuste de cabeceras multinivel y la Regla 15 v2.
- **Binario:** sha256 `fe70dd0e…0a23`, más la DLL de pdfium (`4b0dec92…f994`). Se compiló en un checkout aparte; no se leyó el código ni los informes de las piezas.
- **Centro:** ingesta nueva en un centro aislado, con destilación con Sonnet. **El centro se conserva** para la comparación A/B con el adaptador de la API.
- **Preguntas:** 2 pasadas completas de 61, con Claude real y `--modelo sonnet`. Además, repeticiones dirigidas: V56, V58 y V60 ×8 y V59 ×4.
- **Comparación:** pregunta por pregunta frente a la línea base (`de9cae9`) y frente a `ad1fbb3` (piezas 1, 2 y 3c).
- **Aislamiento:** sin el texto de las preguntas ni de los documentos.

**Lectura obligada.**

- **120/122 correctas** en las 2 pasadas (59 + 61). Es el mejor resultado hasta ahora:

  | Versión | Correctas |
  |---|---|
  | `da66519` (unificada) | **120/122** |
  | `ad1fbb3` | 116/122 |
  | arreglos | 115/122 |
  | `de9cae9` (línea base) | 114/122 |

- **Condición dura de aceptación: se cumple** en las 2 pasadas: trampas 8/8, «no consta» 9/9 y 0 páginas falsas.
- **Los 2 errores** (V58 y V59, solo en la pasada 1) son defectos que ya tenía la línea base:
  - un bloqueo del arnés sobre una suma calculada;
  - la búsqueda que elige la cifra de otro perímetro.
- **Las repeticiones no muestran retroceso**: en las preguntas de cálculo, 27/30, igual que la línea base.
- **El replay ciego no se hizo:** no encontré un punto de entrada para pasar respuestas guardadas por el arnés sin leer el código. Lo pregunté a la sesión principal y no tuve respuesta durante la medición.
- **El set satura otra vez** (98 %). Hace falta uno más duro para que discrimine.

## 1. Errores de respuesta

| Id | Categoría | Pasada 1 | Pasada 2 | Etapa |
|---|---|---|---|---|
| V58 | dato repartido con suma (financiero) | **bloqueada** | correcta | **Arnés**: `salida_segura` sobre una suma calculada. En la misma consulta, el verificador de procedencia agotó su límite de 10 s |
| V59 | cruce de dos documentos (financiero) | incompleta | correcta | **Búsqueda**: toma una cifra de otro perímetro (resultado antes de impuestos). La conclusión es correcta y lo advierte |

Entregadas frente a correctas:

| Pasada | Entregadas | Correctas |
|---|---|---|
| 1 | 60 | 59 |
| 2 | 61 | 61 |

### 1.1 Repeticiones dirigidas: variabilidad frente a retroceso

Correctas sobre el total de ejecuciones (pasadas completas más repeticiones), con los bloqueos del arnés entre paréntesis:

| Id | Línea base `de9cae9` | `ad1fbb3` | `da66519` |
|---|---|---|---|
| V56 | 8/10 (0) | 9/10 (1) | **10/10 (0)** |
| V58 | 9/10 (1) | 7/10 (3) | 8/10 (2) |
| V60 | 10/10 (0) | 9/10 (1) | 9/10 (1) |
| **Las tres** | **27/30 (1)** | 25/30 (5) | **27/30 (3)** |
| V59 | 3/5 (0) | 2/2 (0) | 4/6 (0) |

- **Cálculos:** las mismas aciertos que la línea base. Los bloqueos son 3 frente a 1, una diferencia no concluyente con 30 ejecuciones por lado. El posible retroceso que señaló `ad1fbb3` (25/30, con 5 bloqueos) no se repite.
- **V59:** es el defecto de búsqueda que ya existía, con tasas parecidas.
- **No hay retrocesos de veredicto atribuibles a `da66519`.**

## 2. Pregunta por pregunta

Peor veredicto de las 2 pasadas completas. Las preguntas no listadas son correctas, o abstenciones correctas, en todas las comparaciones.

| Id | `de9cae9` | `ad1fbb3` | `da66519` | Cambio |
|---|---|---|---|---|
| V50 | abstención indebida | correcta | correcta | **mejora** sobre la base (pieza 1) |
| V51 | abstención indebida | correcta | correcta | **mejora** sobre la base (pieza 1) |
| V22 | bloqueada | bloqueada | **correcta** | **mejora** (2/2) |
| V53 | bloqueada | bloqueada | **correcta** | **mejora** (2/2) |
| V06 | correcta | incompleta | **correcta** | igual que la base |
| V56 | correcta | bloqueada | correcta | igual que la base |
| V60 | correcta | bloqueada | correcta | igual que la base |
| V58 | correcta | bloqueada | bloqueada (1 de 2) | **retroceso candidato, descartado** (8/10 frente a 9/10) |
| V59 | correcta | correcta | incompleta (1 de 2) | **retroceso candidato, descartado** (defecto previo: base 3/5) |

## 3. Página

Página en las respuestas correctas con página aplicable:

| | `de9cae9` p1 / p2 | `ad1fbb3` p1 / p2 | `da66519` p1 / p2 |
|---|---|---|---|
| Correcta | 45 / 46 | 49 / 43 | **49 / 50** |
| Sin página | 2 / 1 | 2 / 2 | **0 / 0** |
| Parcial | 0 / 0 | 0 / 0 | 0 / 1 |
| **Incorrecta** | 0 / 0 | 0 / 0 | **0 / 0** |

La respuesta parcial (V31) cita una de las dos páginas del dato; no es una página falsa.

**Tablas de la Capa 2.** Se consultaron 5 tablas: dos fichas clínicas, una guía clínica, un documento técnico y un artículo.

- **Medido:** **0 de 5** devuelven página en la salida de `lookup_double_entry_table`. No hay ningún campo de página ni marcas `⟦p. N⟧` en la `cita` ni en ninguna otra parte de la respuesta.
- Aun así, las respuestas de tabla salen ahora todas con su página correcta (V05 y V10 pasan de «sin página» a «correcta»). La página viene de `search_normative_context`, que el modelo consulta junto a la tabla y que sí trae `pagina_inicio`/`pagina_fin` y marcas de página.
- **Supuesto:** que ese cambio de comportamiento venga de la Regla 15 v2 o de las instrucciones del arnés, y no de la pieza 2.

**Avisos de página** (`verificacion_pagina`): 3 en la pasada 1 y 4 en la 2, frente a 1 y 0 en `ad1fbb3` y a 15 y 16 en la línea base.

- Revisados, 2 son falsos: la cita lleva su página bien y el aviso señala otra aparición de la misma frase.
- Al menos 3 son plausibles: una cita atribuida a la página de al lado.

## 4. Arnés: intervenciones por tipo

| Intervención | Pasada 1 | Pasada 2 |
|---|---|---|
| Consultas con alguna intervención | 11 | 10 |
| `salida_segura` (bloqueo) | 1 | 0 |
| Reintento «no consta + valor nuevo» | 5 | 3 |
| Reintento «no se consultó la Capa 2» | 2 | 2 |
| `directiva` Capa 2 | 1 | 1 |
| `verificacion_pagina` | 3 | 4 |
| `verificacion_procedencia`, timeout de la microllamada (10 s) | 3 | 1 |
| `verificacion_procedencia` con resultado («k de n cifras con procedencia comprobada») | 1 | 2 |

- El **reintento «no consta + valor»** sigue saltando sobre valores legítimos: números de artículo, cifras calculadas y una fecha. Solo una vez acabó en bloqueo (V58, en la pasada 1).
- La **guarda contra llamadas en crudo** (pieza 3b) no intervino. En estas 122 consultas no apareció ninguna respuesta con llamadas en crudo; antes se había visto una vez en la línea base.

### 4.1 Microllamadas a Haiku (pieza 4, Nivel 2)

| | Pasada 1 | Pasada 2 |
|---|---|---|
| Intentos (completadas más agotadas) | 4 | 3 |
| Completadas | 1 | 2 |
| Agotadas a los 10 s | 3 | 1 |
| Latencia de las completadas | 9,3 s | 7,9 y 9,0 s |

- El modelo resuelto fue `claude-haiku-5-5`.
- **Medido:**
  - El límite de espera del verificador (**10 s**) está casi en la latencia real de una microllamada `claude -p --model haiku`, que es de unos 8-9 s por el arranque del proceso. **4 de 7 intentos** agotaron el límite.
  - Las 3 completadas dieron «0 de 2», «0 de 2» y «1 de 2 cifras con procedencia comprobada», las tres sobre respuestas correctas. El verificador no bloqueó nada, pero su señal no distingue esas respuestas de una errónea.
- **Supuesto:** que las agotadas correspondan cada una a un proceso cortado. Mi envoltorio de registro solo guarda los procesos que terminan, así que esas no constan en la traza.

Propuesta de regla general: medir el tiempo de arranque de `claude -p` y fijar el límite con margen (por ejemplo, el percentil 95 de la latencia observada). Si el verificador no responde, que no cuente como evidencia en ningún sentido. Conviene revisar el Nivel 2 con un set donde haya cifras realmente erróneas.

## 5. Trampas y «no consta»

- **Trampas:** 8/8 en las 2 pasadas, sin respuestas de memoria. En la pasada 2, una respuesta añade de forma explícita que «su recuerdo de la publicación original es otro» y no lo da como respuesta. Es el comportamiento correcto.
- **«No consta»:** 9/9 abstenciones correctas en las 2 pasadas. En una, un valor conocido aparece solo como término de búsqueda (revisado a mano).

## 6. Replay ciego

- **No ejecutado.** La CLI de `da66519` solo expone `--ask`, `--ingest` y `--mcp-server`. No encontré, sin leer el código, una forma de pasar una respuesta guardada por el arnés sin llamar al modelo.
- Pregunté a la sesión principal cómo hacerlo. Si existe el punto de entrada, el replay de V56, V58 y V60, de las 8 trampas y de los 9 «no consta» puede ejecutarse sobre las respuestas crudas ya guardadas sin gastar Claude.
- Lo medido en su lugar, con respuestas nuevas:
  - cálculos 27/30, con 3 bloqueos;
  - trampas 16/16;
  - «no consta» 18/18;
  - 0 falsos aceptados en trampas y «no consta».

## 7. Línea base para la comparación con la API

### 7.1 Qué se guardó

Todo está en `docs/validacion/set_oculto_v2/`, ignorado por git. El manifiesto es `manifiesto_unificada.json`. Se guardó:

- **Respuestas crudas completas** (`crudas_unificada_p1.jsonl` y `_p2.jsonl`). Cada fila trae:
  - la respuesta;
  - las herramientas, con argumentos, salidas y errores;
  - las intervenciones del arnés;
  - la latencia;
  - la CPU antes y después;
  - el sobre JSON completo de la CLI.
- **Trazas de cada `claude -p`** (`crudas_unificada_p{1,2}/<id>/*.json`). Las registra un envoltorio de medición (`benchmarks/set_oculto_v2/claude_shim/`) que se antepone en el PATH y reenvía todo al `claude.exe` real. Por proceso guarda los argumentos (sin los prompts largos), la duración, el código de salida y la salida `stream-json` completa: eventos, modelo resuelto, `usage` y `total_cost_usd`.
- **Métricas por pregunta** (`metricas_unificada_p{1,2}.json`):
  - latencia total;
  - pasos (procesos `claude -p`) y procesos por alias;
  - modelos resueltos;
  - tokens y coste;
  - herramientas;
  - reintentos y bloqueos;
  - microllamadas a Haiku y su latencia;
  - CPU.
- **Contexto de la ejecución:**
  - el commit;
  - los sha256 del binario y de la DLL;
  - la ruta del centro (`aislado_unificada/centros/set-oculto`);
  - el `doc_id` y el sha256 de los 15 PDF ingeridos.

### 7.2 Métricas guardadas

Este es el patrón contra el que comparar el adaptador de la API:

| Métrica | Pasada 1 | Pasada 2 |
|---|---|---|
| Correctas | 59/61 | 61/61 |
| Procesos `claude -p` (total) | 162 | 162 |
| Procesos por pregunta (mediana) | 2 | 2 |
| Modelos resueltos | `claude-sonnet-5-5`, `claude-haiku-5-5` | ídem |
| Tokens de entrada (sin caché) | 204 | 218 |
| Tokens de salida | 81.481 | 84.680 |
| Tokens de escritura en caché | 2.098.700 | 1.932.809 |
| Tokens de lectura de caché | 433.139 | 998.394 |
| Coste declarado por `claude -p` (`total_cost_usd`) | 9,18 USD | 8,55 USD |
| Microllamadas a Haiku: completadas / agotadas | 1 / 3 | 2 / 1 |
| Reintentos / bloqueos | 7 / 1 | 5 / 0 |
| Latencia por pregunta: mediana | 31,2 s | 29,9 s |
| Latencia: total | 37,5 min | 36,3 min |
| CPU previa: mediana (máx.) | 21 % (71 %) | 26 % (69 %) |

- **El coste** es el que declara `claude -p` en su sobre. No es una factura; con suscripción puede no corresponder a un cargo real.
- **El caché pesa mucho:** cada paso es un proceso nuevo que reescribe en caché el prompt de sistema y el contexto. Por eso los tokens de escritura en caché superan en varios órdenes de magnitud a los de entrada sin caché. Es justo lo que un adaptador de la API con sesión persistente podría reducir. **Supuesto**, a comprobar en el A/B.
- **Latencias:** no saco conclusiones absolutas; el equipo se usaba en paralelo.

### 7.3 Repetir con otro adaptador sobre el mismo centro (sin reingerir)

```bash
python -I benchmarks/set_oculto_v2/ejecutar.py preguntar --binario <nuevo>/lakehouse_rag_cli.exe --aislado docs/validacion/set_oculto_v2/aislado_unificada --set docs/validacion/set_oculto_v2/preguntas.json --salida docs/validacion/set_oculto_v2/crudas_api_p1.jsonl --modelo sonnet --envoltorio-claude docs/validacion/set_oculto_v2/envoltorio_claude
```

Repetir con `--salida …crudas_api_p2.jsonl`. Si el adaptador nuevo no usa `claude -p`, se quita `--envoltorio-claude`; el uso tendrá que salir de la propia API. Después:

```bash
python -I benchmarks/set_oculto_v2/metricas_claude.py docs/validacion/set_oculto_v2/crudas_api_p1.jsonl --salida docs/validacion/set_oculto_v2/metricas_api_p1.json
```

```bash
python -I benchmarks/set_oculto_v2/medir.py --etiqueta api --solo-puntuar --base unificada
```

```bash
python -I benchmarks/set_oculto_v2/manifiesto.py --etiqueta api --commit <sha>
```

El centro de `aislado_unificada` debe usarse tal cual; `--solo-puntuar` no reingiere. Para `--solo-puntuar` con la etiqueta `api`, primero hay que copiar `bin_unificada` a `bin_api` y crear `ingesta_api.jsonl` como enlace a `ingesta_unificada.jsonl`, o usar `ejecutar.py` y `puntuar.py` directamente.

## 8. Medido y supuesto; incidencias

- **Medido:**
  - Veredictos, con revisión manual de 12 respuestas dudosas o fallidas.
  - Páginas.
  - Campos de página de la Capa 2.
  - Intervenciones del arnés.
  - Trazas de `claude -p`: procesos, modelos, tokens y coste.
  - 28 ejecuciones extra.
- **Supuesto:**
  - El origen de las páginas de tabla (búsqueda de texto, no Capa 2).
  - Las causas de los timeouts de Haiku.
  - Que los procesos cortados no consten en la traza.
- **Ingesta:** 15/15, sin fallos, ~54 min. Las páginas sin texto que marca la compuerta son las mismas que en `ad1fbb3` (exactas frente a pypdf).
- **Sin ajustes de la puntuación** en esta medición. Se usaron las mismas reglas que en las anteriores, ya aplicadas a todas las ramas.

## Propuestas, por prioridad

1. **Verificador de procedencia (Nivel 2):** subir el límite de 10 s o medirlo contra la latencia real de arranque. Además, validar su señal con un set que tenga cifras erróneas: hoy da «0 de 2» en respuestas correctas.
2. **Reintento «no consta + valor»:** excluir los valores derivables y las referencias estructurales. Queda como la última fuente de bloqueos falsos (3 en 30 cálculos).
3. **Búsqueda entre perímetros** (V59): ante varias cifras del mismo concepto (entidad frente a grupo, antes frente a después de impuestos), que la búsqueda las traiga juntas y la respuesta las distinga.
4. **Replay ciego:** exponer un punto de entrada documentado que pase respuestas guardadas por el arnés. Permitiría repuntuar el arnés sin gastar Claude.
5. **Set v3**, más difícil: este satura al 98 %.
