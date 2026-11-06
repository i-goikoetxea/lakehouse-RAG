# Fase 4 del arnés: suites reales y benchmark con Claude

Fecha: 2026-11-03. Commit del código: `7dc887d` (arnés hasta la Fase 3b). CLI: Claude Code 2.1.302, alias `haiku` y `sonnet`. La resolución del alias a un modelo concreto no se registró.

Todos los datos son de fixtures sintéticos: `tests/fixtures/` y los fixtures propios del benchmark. La evidencia cruda (logs, informes y telemetría) quedó fuera del repositorio.

## 1. Veredicto

**La Fase 4 no se cierra.** El cruce con la tabla en el historial pasa con los dos modelos, pero el arnés bloquea respuestas correctas en tres casos reproducibles:

1. **Fechas tomadas por ejes.** Una pregunta con magnitud y fecha se clasifica como `TabularIncompleta` («temperatura… el 11 de septiembre de 2025»). El arnés exige entonces `lookup_double_entry_table` o SQL, aunque el dato esté en el texto. Esto dio 6 salidas seguras en el benchmark, todas en `historico_por_fecha`.
2. **Locución de abstención dentro de un identificador.** `vigencia_no_declarada` se lee como «no declarada», así que la respuesta cuenta como abstención.
3. **Fecha de hoy sin anclar.** Claude Code da la fecha al modelo y la respuesta escribe «Hoy (2026-11-03)». El guard trocea la fecha y el «11» no está en ninguna fuente.

Juntos, 2 y 3 producen `CifraEnAbstencion` con la cifra «11» en respuestas correctas.

**Actualización (anexo, §7):** los tres están corregidos en el commit `2edde3a`. Con Claude real no queda ningún falso bloqueo medido. Ninguno se corrigió en la sesión original. El punto 2 no es trivial: separar `_` de las palabras también hace que `NO_CONSTA` cuente como abstención. Las propuestas están en el §5.

## 2. Suites reales de Rust

Una pasada por modelo de `tests/acceptance_8_problemas.rs`. El test `p2_cruce_con_la_tabla_ya_en_el_historial_invoca_lookup` es uno de sus 11 tests. Las demás suites `#[ignore]` no son del arnés. El modelo se elige con `LAKEHOUSE_MODELO_ACEPTACION` (por defecto `sonnet`), salvo en §9.4, que fija los suyos. Con `LAKEHOUSE_ACEPTACION_VOLCAR` se imprime la salida bruta del modelo, también la que el arnés rechaza.

| Test | Haiku | Sonnet |
| :--- | :--- | :--- |
| P1 anti-fusión | OK | OK |
| **P2 cruce con la tabla en el historial** | **OK** (125 °C, llamó al lookup) | **OK** (125 °C, llamó al lookup) |
| P2 cruce exacto con nota y fronteras | Falla: oráculo del test | Falla: oráculo del test |
| P3 versiones | OK | **Falla: falso bloqueo** (salida segura) |
| P4 a P8, §9.1 y §9.4 | OK | OK |
| **Total** | **10/11** | **9/11** |
| Intervenciones del arnés | 4 directivas | 4 directivas, 1 reintento, 1 salida segura |
| Duración | 640 s | 802 s |

**P2 cruce exacto: fallo del test, no del modelo ni del arnés.** La respuesta da 125 °C con la nota HV10 y la cita. Además explica, como pide la regla 13, que el orden invertido de los ejes daría 100 °C. El test rechaza cualquier «100 °C» porque lo toma por el valor de la Rev. 03. Falló en 3 de 4 pasadas con Haiku (la completa y 2 de 3 repeticiones aisladas) y en la única con Sonnet.

**P3: falso bloqueo, reproducido 2 veces de 2.** La respuesta rechazada era correcta: 125 °C hoy, 100 °C el 2026-06-01, con la nota, las dos celdas y la vigencia. Motivo:

> la respuesta declara que algo no consta y añade «11», que no aparece en la pregunta ni en ninguna salida de herramienta de este turno

Las dos piezas están en el texto rechazado: `` `vigencia_no_declarada` vino vacío `` activa la abstención y `| Hoy (2026-11-03) |` aporta el «11». En la primera pasada, el reintento también se rechazó y acabó en salida segura. En la repetición, el reintento pasó.

## 3. Caso pendiente de la Fase 3

«Tabla 4-A, 30 mm y CE 0,42 %» con la tabla en el historial: **no se reprodujo** en 5 pasadas (4 con Haiku y 1 con Sonnet). Las 5 llamaron al lookup y dieron 125 °C, sin reintento ni salida segura.

La cifra de la Fase 3 sigue sin conocerse. **Hipótesis, no medida:** fue una cifra que el modelo añade y que ninguna fuente contiene. Esta fase ha visto dos de esas cifras en respuestas tabulares correctas: la fecha de hoy y una diferencia calculada («25 °C más que la Rev. 03» en P3). Ninguna se rechazó aquí como `CifraSinAnclar`, porque el «11» saltó antes.

## 4. Benchmark contra el binario con el arnés

- **Binario:** `lakehouse_rag_cli.exe` release de esta rama, sha256 `f26f5972f03c6891491c6d6fe9cde5378bd92ca2acace0370983d05c9a513915`.
- **Runner:** el de la rama `claude/benchmark-fase-2-97ce34`, sin modificar. Pistas `ciclo-de-vida` y `rgb-industrial`, `sonnet`, 3 repeticiones y semilla 42, como la línea base.
- **Envoltorio:** el binario se lanzó a través de un envoltorio Python que guarda la salida `--json` completa de cada `--ask`, porque el runner descarta el campo `arnes`. Por eso el hash del informe es el del envoltorio, no el del binario.

Hubo 75 consultas y 27 ingestas, sin ningún fallo de ejecución ni de recuperación por MCP. La revisión manual, igual que en la línea base, se hizo leyendo las respuestas marcadas como dudosas y las de rechazo.

| Pista | Métrica | Línea base estricta | Arnés estricta | Línea base revisada | Arnés revisada |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Ciclo de vida | `vigente` | 1,000 | 1,000 | 1,000 | 1,000 |
| Ciclo de vida | `historico_por_fecha` | 1,000 | **0,333** | 1,000 | **0,333** |
| Ciclo de vida | `historico_por_revision` | 0,556 | 0,556 | 1,000 | 1,000 |
| Ciclo de vida | **Precisión global** | **0,852** | **0,630** (0,556 a 0,667) | **1,000** | **0,778** |
| RGB industrial | `robustez_ruido` | 0,867 | 1,000 | 1,000 | 1,000 |
| RGB industrial | `rechazo_estricto` | 0,000 | 0,000 | 0,000 | 0,000 |
| RGB industrial | `rechazo_semantico` | sin nota | sin nota (15 pendientes) | 1,000 | 1,000 |
| RGB industrial | `integracion` | 1,000 | 1,000 | 1,000 | 1,000 |
| RGB industrial | `exactitud_tabular` | 0,833 | **0,500** | 1,000 | 1,000 |
| RGB industrial | **Precisión global** | **0,625** | **0,625** | **0,688** | **0,688** |

Los dos tipos de revisión son distintos:
- **Ciclo de vida:** la revisión no recupera las salidas seguras, porque el operador no recibió ninguna respuesta.
- **Rechazo:** la revisión es juicio manual, atada al hash de cada respuesta, como en la línea base.

### 4.1 Intervenciones del arnés en el benchmark

| Tipo | Casos | Desenlace |
| :--- | :--- | :--- |
| Directiva (tabulares cruzables) | 6 | Todas obtuvieron la llamada |
| Reintento por `CapaDosNoIntentada` → **salida segura** | 6 | `fecha-temperatura` y `fecha-presion`, en las 3 repeticiones |
| Reintento por `CifraEnAbstencion` con «11» | 3 | El reintento pasó: 2 aciertos y 1 acierto dudoso |
| Fallos de ejecución | 0 | — |

- **Salidas seguras (6): falsos bloqueos probables.** Medido: las dos preguntas con fecha se clasifican como `TabularIncompleta`, y lo mismo ocurre con «Según la Rev. 00…». El modelo usó `inspect_lakehouse_catalog`, `search_normative_context` con `fecha_referencia` y `query_operational_matrix`, que en la línea base dieron 6 aciertos de 6. Supuesto: que la respuesta rechazada era correcta. El texto rechazado no se guarda, así que no se pudo leer.
- **Reintentos por «11» (3): falsos bloqueos recuperados por el reintento.** Supuesto: que el «11» es otra vez la fecha de hoy. Así ocurrió en P3, pero aquí no se guardó el texto.
- **Coste:** sin medir en dinero ni en tokens. La latencia media de respuesta subió de 27,0 s a 39,9 s en ciclo de vida, y de 33,0 s a 34,4 s en RGB industrial. Las preguntas con salida segura tardaron entre 46 y 64 s.

### 4.2 Lo que no es del arnés

- **`exactitud_tabular` 0,500 y `historico_por_revision` 0,556 estrictas: error del puntuador.** Pasa en 7 respuestas correctas que mencionan un valor prohibido atribuido a otra cosa:
  - el orden de ejes descartado, «110 °C… lo descarto», en las 3 repeticiones de `tabular-esquina`;
  - la revisión vigente, «la Rev. 01 fija 85 °C», en 4 respuestas de `revision-*`.

  En la línea base pasaba en 1 de 3 repeticiones de `tabular-esquina`. Con el aviso `ejes_ambiguos` y la regla 13, el modelo consulta los dos órdenes y explica el descartado en todas.
- **Rechazo:** las 15 respuestas se abstienen y ninguna atribuye un valor a lo inexistente, así que `rechazo_semantico` vale 1,000 (revisión manual). Respecto a la línea base:
  - 10 de 15 ya no ofrecen valores de otros documentos, y varias lo rechazan expresamente. Aun así, `rechazo_estricto` sigue en 0,000, porque cuenta los dígitos de los identificadores: el centro `…-r1`, `EXACT_BM25` y «0 resultados».
  - Las otras 5 (`rechazo-parametro-caudal` ×3 y `rechazo-parametro-dureza` ×2) listan los demás parámetros de la L-104. Esto contradice la regla 14. El guard no lo frena porque esas cifras están ancladas en una tool del turno, como preveía el §3.3 del SSD.
  - En la línea base, las 15 añadían valores.

## 5. Propuestas (sin implementar)

1. **Detector:**
   - Que una fecha («11 de septiembre de 2025», `AAAA-MM-DD`) y una revisión («Rev. 00») no cuenten como valor de eje en `ejes_con_valor`.
   - Tests: las tres preguntas del benchmark deben clasificarse como `Ninguna` o `DatoConcreto`, nunca como `TabularIncompleta`.
2. **Locuciones de abstención:**
   - Buscarlas en el texto sin los identificadores en `snake_case` ni el código entre comillas invertidas.
   - `NO_CONSTA` debe seguir contando, porque la regla 9 lo trata como «no consta». Se puede incluir de forma explícita.
   - Test: el texto rechazado en P3, reducido a la frase de `vigencia_no_declarada` y la fila de la fecha.
3. **Fechas en el anclaje:**
   - Tratar una fecha ISO como una sola cifra que se ancla entera.
   - Anclar también la fecha de hoy, inyectada en el `RegistroTurno` desde fuera del dominio (un reloj como puerto) para que el núcleo siga siendo puro.
   - Alternativa: que el prompt dé la fecha de hoy y la ancle como entrada del turno.
4. **Oráculo de P2** (`acceptance_8_problemas.rs`): admitir «100 °C» si aparece como orden descartado, con «invertido» o «descarto» en la respuesta. Cambia un criterio de aceptación, así que la decisión es del usuario.
5. **Puntuador del benchmark** (no se ha tocado): el mismo error del §4.2 es ahora sistemático. Corresponde a la sesión del benchmark.

Tras 1 a 3, hay que repetir P3 con Sonnet y el benchmark.

## 6. Medido y supuesto

**Medido:**
- Resultados por test y modelo.
- Intervenciones del arnés con su etapa y su `motivo`.
- El texto rechazado de P3 y su causa.
- La clasificación del detector para las tres preguntas del benchmark.
- Las métricas del runner.
- El repuntuado del rechazo con revisión manual, atada al hash de cada respuesta.

**Supuesto:**
- Que las 6 respuestas de las salidas seguras del benchmark eran correctas.
- Que el «11» de los 3 reintentos del benchmark es la fecha de hoy.
- La causa del caso de la Fase 3.
- La nota de `rechazo_semantico`, que es juicio manual.

## 7. Anexo: corrección de los falsos bloqueos

Fecha: 2026-11-03. Commit del código: `2edde3a`, rama `claude/arnes-falsos-bloqueos-ea4f1c`. Mismo CLI de Claude Code y mismos alias que en §2. Solo fixtures sintéticos; la evidencia cruda quedó fuera del repositorio.

### 7.1 Qué se cambió

1. **Detector** (`intent_detector.rs`, `fechas.rs`): no cuentan como valor de eje una fecha (`11 de septiembre de 2025`, `2025-09-11`, `16/06/2025`, `16-06-2025`) ni el número de una revisión (`Rev. 00`, `Rev.01`). Las tres preguntas del benchmark pasan a `DatoConcreto`. Los casos tabulares con test se mantienen, también P3 («hoy… y el 1 de junio de 2026, antes de la Rev. 04»), que sigue siendo `TabularCruzable`.
2. **Abstención** (`post_inference_guard.rs`): un identificador con `_` es una sola palabra, así que `vigencia_no_declarada` no es «no declarada». `NO_CONSTA` se traduce de forma explícita a «no consta» (regla 9). Los `_` de los extremos se recortan, así que `_No consta_` sigue contando.
3. **Fechas en el anclaje** (`cifras.rs`, `fechas.rs`, `RelojPort`): en la respuesta, una fecha ISO o textual es una sola cifra, y solo la ancla esa fecha. En una fuente, la fecha ancla la fecha entera y sus partes. La fecha de hoy llega por el puerto `RelojPort`, con el adaptador `RelojDelSistema` (fecha local), y ancla solo la fecha entera. El núcleo no lee el reloj.
4. **Oráculo de P2** (`acceptance_8_problemas.rs`, módulo `oraculo_p2`): **cambio de criterio aprobado por el usuario.** Antes rechazaba cualquier `100 °C`. Ahora parte la respuesta en líneas y frases, y rechaza un fragmento con `100 °C` (o `100°C`, `100 ºC`) salvo que nombre los ejes invertidos o el descarte (`invert`, `invirt`, `descart`, `al revés`, `inverso`, `intercambi`, `permut`) y no lo presente como vigente, de la Rev. 03 ni como respuesta. Límite conocido: una frase que da 100 °C como valor y además menciona el orden invertido de otra cosa se acepta. Tiene 3 tests unitarios que no necesitan Claude.

**Tests:** 842 pasando, 0 fallos, 32 `#[ignore]` no ejecutados en `cargo test`. `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: OK. Rojo verificado antes de implementar: 14 tests unitarios del arnés y 3 del oráculo. La suite `adversarial_arnes_tests` añade 6 casos: fechas que intentan colar cifras, guiones tipográficos, `_` que intenta esconder una abstención, fechas y revisiones que no deben esconder ejes, y un mega de fechas.

### 7.2 Suites reales de Rust

Una pasada por modelo, con `LAKEHOUSE_ACEPTACION_VOLCAR`.

| Test | Haiku | Sonnet |
| :--- | :--- | :--- |
| P1 anti-fusión | OK | OK |
| **P2 cruce con la tabla en el historial** | **OK** (125 °C, llamó al lookup) | **OK** (125 °C, llamó al lookup, 1 reintento) |
| P2 cruce exacto con nota y fronteras | **OK** (100 °C citado como orden invertido descartado, 2 de 2 respuestas) | **OK** (ídem, 2 de 2) |
| P3 versiones | OK | **OK, sin intervención** |
| P4 a P8, §9.1 y §9.4 | OK | OK |
| **Total** | **11/11** | **11/11** |
| Intervenciones del arnés | 5 directivas | 5 directivas, 1 reintento, 0 salidas seguras |
| Duración | 769 s | 780 s |

- **P3 con Sonnet:** la respuesta aceptada contiene las dos piezas que la bloqueaban en la Fase 4: `| Hoy (2026-11-03) |` y `` `vigencia_no_declarada` vacío ``. Con Haiku, la respuesta lleva además «Dato no consta» y la fecha de hoy, y pasó sin reintento.
- **Reintento de Sonnet en P2 con la tabla en el historial: no es uno de los tres falsos bloqueos.** El motivo fue `CifraSinAnclar` con «50», que es la celda de 20 mm que el test deja en el historial y que el lookup del turno no devuelve. El SSD §3.3 rechaza a propósito un dato tomado del historial. **Supuesto:** que la respuesta rechazada citaba esa celda. Este test usa el orquestador directamente y no vuelca el texto rechazado. El reintento pasó.

### 7.3 Benchmark, pista ciclo de vida

- **Binario:** `lakehouse_rag_cli.exe` release de esta rama, sha256 `5e90bc8e633c971855d36d3713eff4f2347dfd10449d80d8fb451787083a1976`.
- **Runner:** el de `claude/benchmark-fase-2-97ce34` (`8b88bf9`), sin modificar. `sonnet`, 3 repeticiones y semilla 42.
- **Envoltorio:** el mismo de la Fase 4 (sha256 `d6aeae9e…`, el que figura en el informe del runner).

Hubo 27 consultas, sin fallos de ejecución ni de recuperación por MCP.

| Pista | Métrica | Línea base estricta | Fase 4 estricta | Corrección estricta | Línea base revisada | Fase 4 revisada | Corrección revisada |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| Ciclo de vida | `vigente` | 1,000 | 1,000 | 1,000 | 1,000 | 1,000 | 1,000 |
| Ciclo de vida | `historico_por_fecha` | 1,000 | 0,333 | **1,000** | 1,000 | 0,333 | **1,000** |
| Ciclo de vida | `historico_por_revision` | 0,556 | 0,556 | 0,556 | 1,000 | 1,000 | 1,000 |
| Ciclo de vida | **Precisión global** | **0,852** | **0,630** | **0,852** (0,778 a 0,889) | **1,000** | **0,778** | **1,000** |

- **Revisión manual:** las 4 respuestas marcadas `valor_prohibido` (`revision-temperatura` ×3 y `revision-duracion` ×1, todas dudosas) son correctas. Dan el valor de la Rev. 00 y citan el de la Rev. 01 atribuido a la Rev. 01. Es el error del puntuador del §4.2, sin cambios.
- **Latencia media de respuesta:** 32,4 s (línea base 27,0 s y Fase 4 39,9 s).

### 7.4 Intervenciones del arnés en el benchmark

| Tipo | Casos | Desenlace |
| :--- | :--- | :--- |
| Directiva | 0 | — (ninguna pregunta de la pista es tabular cruzable) |
| Reintento | 0 | — |
| Salida segura | 0 | — |
| Fallos de ejecución | 0 | — |

En la Fase 4 hubo 6 salidas seguras y 3 reintentos por «11». Ahora no hay ninguno. Una respuesta de `revision-temperatura` cita «a fecha de hoy (2026-11-03)», y otra de `revision-duracion` dice «origen NO_CONSTA» junto a cifras ancladas. Las dos pasaron sin intervención.

### 7.5 Recuento

| | Aceptación Haiku | Aceptación Sonnet | Benchmark ciclo de vida |
| :--- | :--- | :--- | :--- |
| Reintentos | 0 | 1 («50», dato del historial, por diseño) | 0 |
| Salidas seguras | 0 | 0 | 0 |
| Falsos bloqueos | 0 | 0 de los tres de la Fase 4. El reintento por «50» es el rechazo previsto por el SSD | 0 |

**Pendiente, fuera de alcance:**
- El puntuador del benchmark sigue contando como fallo citar un valor atribuido a otra revisión o a un orden de ejes descartado (§4.2). Corresponde a la sesión del benchmark.
- No se repitió la pista RGB industrial, que no tenía falsos bloqueos atribuidos al arnés.

### 7.6 Medido y supuesto

**Medido:** resultados por test y modelo, intervenciones con su `motivo`, las respuestas aceptadas de P2 y P3, las métricas del runner, la telemetría `arnes` de las 27 consultas y la revisión manual de las 4 marcadas.

**Supuesto:** que el reintento por «50» rechazó una cita de la celda del historial; no se volcó el texto. Que una sola pasada por modelo y 3 repeticiones del benchmark bastan para afirmar que no quedan falsos bloqueos de estas causas: el resultado es de esta muestra.

