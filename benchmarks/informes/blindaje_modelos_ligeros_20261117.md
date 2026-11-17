# Blindaje del flujo para modelos ligeros: informe de cierre (2026-11-17)

Rama `claude/circuit-breaker-herramientas-6c3d9a`, que sale de `claude/motor-freellmapi-4d79f4` (`d075b3a`). Al fusionarla en `main` entran también el motor FreeLLMAPI (`motor_freellmapi_20261117.md`) y las tres etapas de blindaje. No se ha leído el set oculto.

## 1. Errores y límites (primero)

- **Calidad sin medir con un set.** La validación en vivo la hizo el usuario en la ventana, con modelos locales o gratuitos (Gemini Flash Lite, Nemotron 3 Nano 30B, `auto`) y con Haiku. Según su informe, no hubo regresión. No hay A/B ni pasada por el set oculto con estos cambios: «sin regresión» describe esas pruebas en vivo, no el caso general.
- **Hallazgo pendiente en las sesiones de varios turnos** (§4): la procedencia solo contrasta las cifras con las herramientas del turno actual. Está documentado, no implementado.
- **Cambios de comportamiento que también pueden afectar a Claude:**
  - **Aterrizaje forzoso.** En la iteración 8 de 10 pide el cierre sin herramientas. En 24 respuestas guardadas de Claude (STAIR fase 6 y 7), ninguna llegó a 8 iteraciones; el máximo fue de 7 llamadas a herramientas. Es una muestra pequeña y cuenta llamadas, no iteraciones.
  - **Deduplicación.** Si Claude repite exactamente la misma llamada en un turno, recibe un aviso en lugar de los datos, que ya están en el historial. Esto incluye los errores pasajeros, como un timeout: una consulta idéntica no se reintenta tal cual.
  - **Cambio de motor o de key.** El runner de la API se crea de nuevo y se pierde la memoria de los bloques `thinking` de los turnos anteriores, que se reenvían reconstruidos.
- **Sin llamada real a Anthropic** que confirme que acepta un historial con `tool_use` de una herramienta retirada del array `tools`. Se apoya en la documentación (`prefix_mismatch_behavior: drop_block`). FreeLLMAPI sí se comprobó con una llamada real (HTTP 200).
- **Texto de salida segura impreciso en el cierre.** Si el cierre forzoso no supera el guard, el operador ve «La segunda respuesta tampoco se mostró…» aunque no hubiera una primera.
- **Tests de integración ajustados.** Dos esperaban el error «límite máximo de iteraciones» y ahora esperan el cierre con el mensaje fijo de respuesta no validada. Las aserciones de seguridad no cambian: el turno no entrega una respuesta sin validar y no hay bucle. `integration_chat_en_vivo` añade el campo `redirigida`.

## 2. Qué cambia (un commit por ajuste)

| Commit | Ajuste |
|---|---|
| `f67f3b8` | Circuit breaker por herramienta: con 2 errores consecutivos en el turno, la siguiente llamada no se ejecuta |
| `6ddd9fc` | Errores de parseo y planificación de `execute_athena_sql` con las tablas del centro, sus columnas y tipos, tomados de DataFusion (hasta 8 tablas y 40 columnas) |
| `7bf1b3c` | Deduplicación de llamadas idénticas en el turno (JSON canónico). La repetición hereda el resultado original para el circuito |
| `0cdd03c` | Cambio de motor CLI/API/FreeLLMAPI sin reiniciar; el motor se fija al empezar cada consulta |
| `958d210` | Las herramientas con el circuito abierto se retiran del array `tools` el resto del turno. Sin fallos, el catálogo y la caché no cambian |
| `23d220c` | Aterrizaje forzoso: en la iteración 8 de 10, cierre sin herramientas (`tool_choice: none`; en CLI, catálogo vacío), una sola pasada por el guard y salida segura si no lo supera |
| `cba7aab` | Avisos de herramienta recuperables como pill ámbar colapsado («Vía alternativa» / «Consulta redirigida»), con el detalle al desplegar |

Todas son reglas generales: no dependen de nombres de herramienta, documentos, dominios ni idiomas.

## 3. Verificación

- Tests dirigidos de cada etapa, en verde: Etapa 1, 10/10; Etapa 2, 100/100; Etapa 3, 98 unitarios más 4 de integración.
- Suite completa en la rama antes del merge: **1.323 tests en verde, 0 fallos y 36 ignorados**, en 69 binarios. La primera pasada dio 3 fallos. Uno era de entorno: faltaba `pdfium.dll` en `target\debug` del worktree. Los otros dos eran `gui_config_llm`, que buscaba un texto de `app.js` que el cambio de motor había reemplazado. Solo se ajustó el texto que delimita el bloque; las aserciones sobre la key no cambian (commit `b21195b`). Tras el arreglo, ambos binarios pasan.
- Replay ciego de las respuestas guardadas (sin coste de Claude) frente a `main`: **0 cambios en 278 filas** (rechazos, avisos de página, mutadas y pendientes de nivel 2). Límite: el replay repite el guard y la procedencia sobre respuestas guardadas; no ejercita el bucle de herramientas, así que no cubre el circuito, la deduplicación ni el aterrizaje forzoso.

## 4. Hallazgo pendiente: procedencia en sesiones de varios turnos

Observado por el usuario en vivo. El usuario pide un resumen acumulado de los turnos anteriores, junto con un «no consta». El verificador de procedencia solo contrasta las cifras con los resultados de herramienta del turno actual, no con las evidencias ya verificadas en turnos anteriores de la misma sesión. Por eso, las cifras correctas que se repiten de turnos previos pueden quedar sin validar o provocar una corrección innecesaria.

Propuesta, sin implementar: llevar por sesión un conjunto de evidencias verificadas (cifra, fuente y cita) que el verificador consulte junto con el turno actual. Así una cifra ya contrastada en un turno previo no exige otra consulta, y una cifra nueva sigue exigiendo su herramienta. Hay que medirlo con conversaciones de varios turnos y documentos distintos de los que lo revelaron.
