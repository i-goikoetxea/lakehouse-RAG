# RGB industrial con el arnés corregido (2026-11-09)

Mide la pista `rgb-industrial` con el código de `main` en `68af7d2`. Ese código incluye el arnés determinista sin los falsos bloqueos de la Fase 4 (`9911bed`) y el puntuador corregido. Completa la serie formada por la [línea base](linea_base.md) y la [repuntuación del 2026-11-03](repuntuacion_20261103.md), sin reescribirlas. Solo se usaron fixtures sintéticos.

## Condiciones

| Dato | Valor |
| :--- | :--- |
| Código | `main` en `68af7d2`, compilado en release el 2026-11-09 (`cargo build --release --bin lakehouse_rag_cli`) |
| sha256 del binario | `8f2440b73f3b9aa4114e80e144bbce5ff1844d5afa7b31036fefdffb4d9526a9` |
| Envoltorio | El mismo de la Fase 4, que guarda el `--json` completo de cada `--ask` (campo `arnes`). Su sha256, `d6aeae9e…0d400`, es el que figura como «binario» en el informe del runner |
| Runner | `benchmarks/run.py` sin modificar. Pista `rgb-industrial`, `sonnet`, 3 repeticiones y semilla 42 |
| Fixtures `rgb-industrial` | sha256 `ceff254f…1174`, el mismo que en la línea base |
| Embeddings | Caché de FastEmbed existente (`%LOCALAPPDATA%\lakehouse_rag\fastembed_cache`) |
| Claude Code CLI | 2.1.302 |
| Informe crudo | `benchmark_report_20261109T165955_501075.json`, sha256 `f348c9f9…8a66`, en el scratchpad de la sesión (no versionado) |
| Revisión manual de rechazo | [`revision_rechazo_arnes_corregido_20261109.json`](revision_rechazo_arnes_corregido_20261109.json) |
| Duración | 35,5 min (16:59:54 a 17:35:23 UTC), solo esta pista |

El `app_commit` del informe crudo es `68af7d2`. Esta vez coincide con el binario, porque se compiló desde el mismo checkout.

Para reproducir la repuntuación:

```bash
python benchmarks/evaluacion/repuntuar.py --informe <crudo.json> --revision benchmarks/informes/revision_rechazo_arnes_corregido_20261109.json --valores --salida <carpeta>
```

## Resultados

Hubo 48 consultas y 21 ingestas (7 por repetición), sin ningún fallo de ejecución ni de recuperación por MCP. Todas las ingestas quedaron definitivas (`COMPLETO`).

En las columnas anteriores va la nota «Nueva» de la [repuntuación del 2026-11-03](repuntuacion_20261103.md), que usa el mismo puntuador que hoy. La columna «Ahora» da la nota estricta del puntuador y, si cambia, la revisada a mano.

| Métrica | Consultas | Línea base | Fase 4 (falsos bloqueos) | Ahora, estricta | Ahora, revisada |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `robustez_ruido` | 15 | 1,000 | 1,000 | 1,000 | 1,000 |
| `rechazo_estricto` | 15 | 0,000 | 0,533 | **0,533** | 0,533 |
| `rechazo_semantico` | 15 | 1,000 (manual) | 1,000 (8 por reglas y 7 manuales) | sin nota (7 pendientes) | **1,000** (8 por reglas y 7 manuales) |
| `integracion` | 12 | 1,000 | 1,000 | 1,000 | 1,000 |
| `exactitud_tabular` | 6 | 1,000 | 1,000 | **0,833** | 1,000 |
| **Precisión global** (media; mín. y máx. por repetición) | 48 | **0,688** | **0,854** (0,813 a 0,875) | **0,833** (0,750 a 0,875) | **0,854** (0,813 a 0,875) |
| Latencia media de respuesta | 48 | 33,0 s | 34,4 s | 34,1 s | — |
| Latencia media de recuperación (MCP) | 48 | 0,28 s | 0,91 s | 0,21 s | — |

La precisión global usa siempre `rechazo_estricto`. Las pistas no se mezclan, y nada de esto es comparable con los artículos de RGB.

### Rechazo: respuestas que siguen ofreciendo valores

| | Línea base | Fase 4 | Ahora |
| :--- | :--- | :--- | :--- |
| Atribuyen un valor a lo inexistente | 0 de 15 | 0 de 15 | **0 de 15** |
| Ofrecen valores que nadie pidió | 15 de 15 (de otros documentos o parámetros) | 5 de 15 | **5 de 15** |
| De ellas, con valores de **otros documentos** (otras líneas) | sin desglosar | 0 | **0** |
| No ofrecen ningún valor | 0 de 15 | 10 de 15 | **10 de 15** |

- **Las 5 que ofrecen valores** son las mismas consultas que en la Fase 4: `rechazo-parametro-caudal` en las 3 repeticiones y `rechazo-parametro-dureza` en las repeticiones 1 y 3. Todas listan los demás parámetros de la L-104, del mismo documento (PTF-HID-10), cada uno con su nombre, y ninguna los presenta como el parámetro pedido. Ninguna da cifras de otra línea. Siguen contradiciendo la regla 14 («sin ofrecer valores de referencia»), y es lo que `rechazo_estricto` debe castigar.
- **Las otras 10 no dan ningún valor.** 8 valen 1,0 por reglas. Las 2 restantes valen 0,0 solo por dígitos de códigos que la exclusión del SSD §5.2 no cubre:
  - `rechazo-procedimiento`, rep. 1: escribe la lista abreviada «PTF-HID-10, 20, 30, 40, 50 y 60», y el puntuador ve «20, 30, 40, 50, 60».
  - `rechazo-parametro-dureza`, rep. 2: el puntuador ve un «2». Lo más probable es que venga de `02_chunks_normativa`, el nombre de la tabla consultada por SQL. Es una inferencia, no una comprobación.

  En la Fase 4 pasó lo mismo con 2 respuestas, por «nivel de prelación 2» y «apartados 1 y 2».
- **El arnés no frena las 5 con valores**, y no es un fallo: esas cifras están ancladas en una tool del turno, como ya preveía el §3.3 del SSD del arnés.

## Intervenciones del arnés

Salen de la telemetría del envoltorio, con 48 registros, uno por `--ask`.

| Tipo | Fase 4, solo RGB | Ahora | Desenlace ahora |
| :--- | :--- | :--- | :--- |
| Directiva (tabulares cruzables) | 6 | 6 | Las 6 obtuvieron la llamada al lookup |
| Reintento | 0 | **2** | Los 2 reintentos pasaron, y las 2 respuestas finales son correctas (1,0) |
| Salida segura | 0 | **0** | — |
| Falso bloqueo | 0 | **2**, recuperados por el reintento | Ver abajo |
| Fallo de ejecución | 0 | 0 | — |

En la Fase 4, los reintentos y las salidas seguras se dieron en la pista de ciclo de vida, no en RGB.

**Los 2 reintentos son falsos bloqueos.** Las 2 respuestas que llegaron al operador son correctas. Lo que se desconoce es la respuesta rechazada, porque su texto no se guarda.

1. **`rechazo-procedimiento`, rep. 2 (60,5 s).**
   - Motivo: `CifraEnAbstencion` con «-20».
   - La respuesta final es una abstención limpia y lista los códigos completos, de «PTF-HID-10» a «PTF-HID-60».
2. **`ruido-temperatura`, rep. 3 (44,1 s).**
   - Motivo: `CifraEnAbstencion` con «-30».
   - Es una pregunta con respuesta: la final da 38 °C y es correcta.
   - Activa el modo abstención porque la respuesta dice «Página: no consta en el registro recuperado».

**Causa probable, supuesta y no medida.** El modelo abrevió una lista de códigos, por ejemplo «PTF-HID-10/-20» o «PTF-HID-10, -20, -30». Hay tres indicios:
- `cifras.rs::tiene_signo` lee un guion como signo negativo si el carácter anterior no forma parte de un identificador.
- En las fuentes, el «20» de `PTF-HID-20` va tras una letra y un guion, así que se ancla como «20», nunca como «-20». Las salidas de tools de ambos turnos contienen `PTF-HID-20` y `PTF-HID-30`. Se ha comprobado leyendo la telemetría.
- El modelo sí abrevia listas de códigos: lo hace en `rechazo-procedimiento`, rep. 1 («PTF-HID-10, 20, 30…»).

En el caso 2 se suma otra pieza: una abstención parcial de un metadato («Página: no consta») basta para que el guard trate como abstención una respuesta con dato.

**Coste:** cada reintento añade otra inferencia completa. Las dos preguntas tardaron 60,5 s y 44,1 s, y la latencia media apenas cambia.

## Revisión manual

Se siguió el mismo criterio que en la línea base: leer completas las respuestas que las reglas dejan pendientes y las dudosas.

- **`rechazo_semantico`:** las reglas dejaron pendientes 7 respuestas. Las 7 se abstienen y ninguna atribuye un valor a lo inexistente, así que las 7 valen 1,0. Cada decisión está en la revisión versionada, atada al hash de la respuesta. `repuntuar.py --valores` la aplicó sin contradicciones con las reglas.
- **`tabular-esquina`, rep. 1:** el puntuador da 0,0 (`valor_prohibido`, 110 °C, dudosa). La respuesta acierta, con 175 °C, y descarta expresamente el 110 °C: «salió de asignar 55 al CE… así que lo descarto». Revisada vale 1,0. La regla del SSD §5.1 no la salva por la frase anterior, «daba celdas distintas: 110 °C o 175 °C», que no lleva ninguna palabra de descarte. Es el límite conocido de la mención ambigua. Esa revisión no tiene mecanismo en `repuntuar.py`, que solo cubre el rechazo, así que figura solo en la columna «revisada».
- **`ruido-perdida`, rep. 3, y `tabular-esquina`, reps. 2 y 3:** el puntuador ya las da por buenas (`acierto_con_mencion`).

## Hallazgos y propuestas (sin implementar)

1. **Arnés, signo negativo tras una lista abreviada de códigos** (`src/application/use_cases/arnes_turno/cifras.rs`). Propuesta: que `tiene_signo` no lea el guion como signo cuando lo precede un separador de lista (`/`, `,`, `y`) justo después de un identificador con guion (`PTF-HID-10/-20`). Habría que escribir antes el test rojo con ese texto, y guardar el texto rechazado en la telemetría para confirmar la causa: hoy solo queda el `motivo`.
2. **Arnés, abstención parcial.** «No consta» aplicado a un metadato (página, condición de aplicación), en una respuesta que sí da el dato pedido, activa la comprobación de abstención. Propuesta: no tratarlo como abstención cuando la locución va en una línea de campo (`Página:`, `Condición de aplicación:`), con tests de ambos sentidos. Es menos urgente que el punto 1, porque sin una cifra mal leída no se habría producido el reintento.
3. **Puntuador, `rechazo_estricto` y listas abreviadas de códigos.** «PTF-HID-10, 20, 30» y `02_chunks_normativa` cuentan como cifras. Ampliar la exclusión del SSD §5.2 a esos casos es discutible, porque la haría menos acotada. La decisión corresponde a la sesión del puntuador. No se ha tocado.
4. **Producto, regla 14.** Las 5 respuestas que listan parámetros hermanos son estables entre la Fase 4 y ahora, y siempre en las mismas dos consultas. Es un cambio de prompt, no del arnés.

## Medido y supuesto

**Medido:**
- Las métricas del runner y su repuntuación con `repuntuar.py`.
- Las latencias.
- La duración de la campaña.
- Las intervenciones del arnés, con su etapa y su `motivo`.
- Que las respuestas finales de los 2 reintentos son correctas.
- Que las fuentes de esos turnos contienen `PTF-HID-20` y `PTF-HID-30`.
- El conteo de respuestas de rechazo con valores.

**Supuesto o juicio:**
- La nota de `rechazo_semantico` de las 7 pendientes y la de `tabular-esquina`, rep. 1, que son juicio manual.
- La causa de los dos reintentos: el texto rechazado no se guarda.
- Que el «2» de `rechazo-parametro-dureza`, rep. 2, venga de `02_chunks_normativa`.

## Límites

- Una sola campaña de 3 repeticiones. La diferencia entre 0,833 y 0,854 depende de una sola respuesta (`tabular-esquina`, rep. 1), y los 2 reintentos pueden no repetirse.
- La línea base y la Fase 4 se ejecutaron con otros binarios y en otros días. Además, la línea base es anterior a la regla 12 y al corte por ANEXO. El modelo `sonnet` es un alias, así que su versión concreta puede haber cambiado entre campañas.
- No se midió el coste en tokens ni en dinero, solo el tiempo. La campaña cubrió solo RGB, 35,5 min. La línea base tardó 48 min con las dos pistas, así que la duración de RGB no se puede separar exactamente. Por la latencia media, el coste por consulta es el mismo que en la línea base y en la Fase 4.
- El arnés corregido ya se midió en ciclo de vida el 2026-11-03 (Fase 4b, 1,000). No se repitió aquí.
