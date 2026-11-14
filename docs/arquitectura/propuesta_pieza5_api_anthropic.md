# Pieza 5: adaptador de la API de Anthropic en lugar de `claude -p` (propuesta pendiente)

**Estado:** propuesta del mentor técnico (Gemini, 2026-11-14), **sin SSD ni implementación**. Se abre después de cerrar la medición unificada de las Piezas 1 a 4 en el set oculto v2. Esta propuesta **sustituye** la §5 de `ssd_generalidad_4_piezas.md` (`claude -p --mcp-config`), que solo tenía sentido sin API key.

## Motivo (medido o inferido en las Piezas 1-4)

- Latencia de 30-45 s por pregunta, frente a 0,2-0,9 s de búsqueda. Lo atribuimos sobre todo a que **cada paso del bucle lanza un proceso `claude -p`**. Es una inferencia: el desglose no se ha medido.
- El protocolo JSON por texto (`{"tool_calls":[…]}`) es frágil. Hubo una fuga de llamadas en crudo, que hoy tapa la Pieza 3b.
- `ejecutar_turno_dirigido` no garantiza la llamada (hoy usa `--json-schema`).

## Decisiones propuestas

1. **Arquitectura.** Un adaptador HTTP sobre la API de Messages **por cada puerto existente**:
   - `AgentRunnerPort` → `AnthropicMessagesRunner`;
   - `LlmDistillerPort` → `AnthropicDistiller`;
   - `VerificadorProcedenciaPort` → `AnthropicProcedenciaVerifier`.

   Comparten un cliente base, `AnthropicHttpClient`, con autenticación, reintentos y caché. El orquestador **conserva el bucle paso a paso**, y con él `RegistroTurno`, `PreInferenceInterceptor`, `PostInferenceGuard` con su reintento y la telemetría del Studio. Se usa tool use nativo, con bloques `tool_use` y `tool_result`. `claude -p` sigue disponible y se elige por configuración (`modo_llm: Api | Cli`) para la comparación A/B y como respaldo. La guarda de la Pieza 3b queda como defensa en profundidad.
2. **`tool_choice`:**

   | Contexto | `tool_choice` | Motivo |
   |---|---|---|
   | Procedencia y destilación | `{"type":"tool","name":"emitir_json_estructurado"}` | Sustituye a `--json-schema` |
   | Paso 1 con intención detectada | `{"type":"any"}` + directiva del interceptor | Obliga a usar alguna tool, sin elegir cuál |
   | `ejecutar_turno_dirigido` | `{"type":"tool","name":…}` | Solo si la tool no tiene argumentos obligatorios que el modelo tenga que adivinar. **Nunca** `lookup_double_entry_table` en frío (spike de la Fase 0) |
   | Pasos 2..N | `{"type":"auto"}` | El modelo decide |

3. **Streaming SSE:**
   - Con `content_block_start` de tipo `tool_use`, los `input_json_delta` se acumulan en silencio y salen como `ToolCalls`.
   - Con tipo `text`:
     - si `retener_texto == true`, se guarda en un buffer, se valida al llegar `message_stop` y se emite;
     - si `retener_texto == false`, se emite `TextDelta` en vivo y se valida al final. Si el guard rechaza, se envían `TextoEnVivoDescartado` y `ArnesIntervino`, y se reintenta con retención.
   - El botón Detener aborta el socket (`CancellationToken`).
4. **Prompt caching** desde el primer día: `cache_control: {"type":"ephemeral"}` en la última tool del array y en el bloque `system`, y en las instrucciones y el esquema del destilador. Métricas nuevas:
   - `ttft_ms`;
   - `latencia_llm_ms` frente a `latencia_tools_ms`, por paso;
   - `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens` y `output_tokens`;
   - `coste_estimado_usd` por pregunta y por pasada.
5. **Robustez:**
   - **Timeouts:** de conexión, 10 s. De lectura entre eventos, 30 s en procedencia, 60 s por paso en el agente y 180 s en la destilación.
   - **Reintentos de transporte,** sin gastar el reintento del arnés:
     - 429, 529 y 500-504: hasta 3, con backoff exponencial y jitter, respetando `retry-after`;
     - 400, 401 y 403: ninguno, con un mensaje accionable al estilo de `sesion_claude.rs`.
   - **`stop_reason`:** `max_tokens` **nunca** se entrega como respuesta válida. `refusal` y `stop_sequence` se convierten en una salida controlada con aviso.
6. **Secretos:**
   - `ANTHROPIC_API_KEY` por variable de entorno para la CLI y los benchmarks; en la GUI, el almacén de credenciales del sistema (crate `keyring`).
   - El WebView nunca lee la key: los comandos son `guardar_api_key` (solo escritura) y `estado_api_key`, que devuelve `configurada` y una máscara.
   - La key va en un tipo opaco, `SecretApiKey`, cuyos `Debug` y `Display` escriben `[REDACTED]` (con `zeroize` opcional).
   - En disco solo se guardan el proveedor y el mapa de modelos, nunca el secreto.
7. **Rust:** `reqwest` + `serde` propios en `src/infrastructure/llm/anthropic_api/`, sin crates de la comunidad. `base_url` es configurable. Los tests no usan red: un servidor HTTP local (`TcpListener` o `wiremock` en dev-dependencies) con fixtures de:
   - `tool_use` + `tool_result`;
   - SSE partido en trozos TCP arbitrarios;
   - un 429 con `retry-after` seguido de un 200;
   - `max_tokens`.
8. **Medición A/B en el set v2:**
   - el mismo centro v7, sin reingerir;
   - `claude -p` frente a la API con el **mismo identificador de modelo fechado**;
   - comparación pregunta por pregunta de respuestas correctas, bloqueos, latencia, coste y `tools_invocadas_por_pregunta`.
9. **Orden por fases:**
   - **5.1:** procedencia (Haiku, un turno, sin streaming), para validar el cliente, los secretos, la caché y los 429 sin riesgo para el bucle;
   - **5.2:** agente (bucle, SSE y A/B en el set v2);
   - **5.3:** destilación (exige reingerir para comparar).

## A verificar antes del SSD (notas de la sesión principal)

- **Identificadores de modelo:** la propuesta menciona `claude-sonnet-4-…` y `claude-haiku-4-…`. Hay que comprobar los modelos vigentes en la documentación oficial de la API al redactar el SSD y fijar identificadores fechados.
- **`temperature: 0.0`:** comprobar en la documentación vigente que el modelo elegido admite ese parámetro. Aun con 0, no hay determinismo garantizado, así que se mantienen 2 o más pasadas.
- **Cifras de caching** (−90 % del coste de entrada cacheado, −40-60 % del TTFT): son estimaciones del mentor, no medidas nuestras. Hay que medirlas en la Fase 5.1.
- **Equivalencia `claude -p` frente a API:** el alias `sonnet` de la CLI puede resolverse a otra versión. Fijar el mismo identificador en los dos lados antes del A/B.
- **Permisos de la app:** una dependencia nueva (`reqwest`, `keyring`, `wiremock`) exige el SÍ del usuario según `AGENTS.md`.
- **Arnés:** con tool use nativo cambia el formato de `AgentMessage`. Los contratos de `RegistroTurno` y del guard deben seguir intactos, con la misma línea base y los criterios de no retroceso de `CLAUDE.md` §6.
