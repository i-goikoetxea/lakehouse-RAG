# SSD corto · Pieza 5, Fase 5.1: procedencia por la API de Messages

**Contrato de origen:** `propuesta_pieza5_api_anthropic.md` (decisiones 1, 2, 4, 5, 6, 7 y 9). Esta fase solo sustituye el **transporte** del Nivel 2 de procedencia (SSD de generalidad §4.6). El puerto `VerificadorProcedenciaPort`, el esquema cerrado de 4 tipos y la comprobación en Rust (`arnes_turno/procedencia.rs`) no cambian: **el modelo propone, Rust comprueba**.

## 1. Verificación de la documentación oficial (15-11-2026)

Fuente: la referencia oficial de la API de Claude que acompaña a Claude Code (tabla de modelos, guía de migración a Claude Haiku 5.5, guía de la versión de API `2026-10-21`, prompt caching y tool use).

| Punto «A verificar» | Resultado | Consecuencia en el diseño |
|---|---|---|
| Identificador de Haiku | `claude-haiku-5-5`. Es un identificador **fijo**, sin sufijo de fecha y sin alias aparte | Se fija tal cual en `MODELO_PROCEDENCIA_API`. El alias `haiku` de la CLI puede resolverse a otra versión: el A/B de la 5.2 debe fijar el mismo identificador en los dos lados |
| `temperature: 0` | **No admitido.** En Claude Haiku 5.5 los parámetros de muestreo no por defecto devuelven 400. Con la versión `2023-06-01` solo se acepta `temperature: 1`; con `2026-10-21` cualquier valor de `temperature`, `top_p` o `top_k` da 400 en todos los modelos | **Desviación del encargo:** la petición **no envía `temperature`**. El encargo pedía `temperature 0`; enviarlo rompería cada llamada. El determinismo que se buscaba lo dan la tool forzada y el esquema cerrado; la falta de determinismo se sigue cubriendo con 2 o más pasadas, como ya decía la propuesta |
| `tool_choice` | `{"type": "tool", "name": "<tool>"}`. Claude Haiku 5.5 **acepta** la tool forzada, también con el pensamiento activo; en ese caso la respuesta empieza por el `tool_use` y no trae bloque `thinking` | Se fuerza `emitir_trazas_procedencia`. **Aviso para la 5.2:** Claude Sonnet 5.5 y Claude Opus 5.5 **rechazan** `tool_choice` `any`/`tool` con 400. La tabla de la decisión 2 de la propuesta no vale tal cual para el agente: allí habrá que usar `auto` + `strict: true` + instrucción en el prompt |
| `cache_control` | `{"type": "ephemeral"}` en el último bloque del prefijo estable. Orden de render: `tools` → `system` → `messages`. Máximo 4 puntos. Prefijo mínimo cacheable en Claude Haiku 5.5: **512 tokens**; por debajo, no se cachea y no da error (`cache_creation_input_tokens: 0`) | Dos puntos: en la tool (cachea el esquema) y en el bloque `system` (cachea tools + system). Si el prefijo queda por debajo de 512 tokens, la caché no actúa; se mide en la calibración |
| `strict: true` | Va en la definición de la tool. Admite `enum`, `const`, `anyOf`, `allOf`; **no** admite `minimum`/`maximum` ni restricciones complejas de arrays | El esquema de la CLI se adapta: `oneOf` → `anyOf` y se quitan `minimum`, `maximum` y `maxItems`. Esos límites ya los comprueba Rust (factor entre −9 y 9, ≤ 6 operandos) |
| Versión de API | Cabecera `anthropic-version: 2026-10-21`; autenticación con `x-api-key` | Se fija `2026-10-21` |
| Errores | 429 `rate_limit_error` con `retry-after` (segundos); 529 `overloaded_error`; 500 reintentables; 400, 401 y 403 no | Política de reintentos de §3 |
| Precio de Claude Haiku 5.5 | Con prompts ≤ 100 K tokens: 0,10 $/MTok de entrada y 0,50 $/MTok de salida. Lectura de caché: 0,1×; escritura de caché de 5 min: 1,25× | `coste_estimado_usd` = `input·0,10 + cache_creation·0,125 + cache_read·0,01 + output·0,50`, por millón |
| Pensamiento | Claude Haiku 5.5 piensa por defecto (adaptativo, esfuerzo `medium`); el pensamiento cuenta para `max_tokens`. La respuesta puede traer bloques `thinking` | Se leen los bloques por `type`, nunca por posición. `max_tokens` holgado (4096) |

## 2. Componentes

```text
src/infrastructure/llm/anthropic_api/
├── mod.rs
├── secreto.rs        SecretApiKey (opaco, [REDACTED]) · lectura de ANTHROPIC_API_KEY · AlmacenKeyring
├── mensajes.rs       tipos serde propios de la petición y la respuesta de /v1/messages
├── cliente.rs        AnthropicHttpClient: autenticación, timeouts, reintentos, stop_reason
├── error.rs          ErrorApiAnthropic con mensajes accionables sin la key
└── procedencia.rs    AnthropicProcedenciaVerifier: VerificadorProcedenciaPort
src/infrastructure/llm/modo_llm.rs   ModoLlm { Cli, Api } (por defecto Cli)
```

- **Sin SDK de la comunidad:** `reqwest` 0.12 con `rustls-tls-native-roots` (lo que ya hay en el árbol), `json` y `stream`.
- **Sin streaming** en la 5.1: un turno, respuesta completa.
- `claude_headless_procedencia.rs` **no se toca** (otra sesión cambia su timeout). Del adaptador CLI solo se reutiliza el esquema público `esquema()`.

## 3. Contratos

### `AnthropicHttpClient`

| Aspecto | Regla |
|---|---|
| `base_url` | Configurable (por defecto `https://api.anthropic.com`). Los tests apuntan a un servidor local |
| Timeouts | Conexión 10 s; lectura 30 s en procedencia (`read_timeout` de reqwest, entre lecturas del socket) |
| Reintentos de transporte | 429, 529 y 500-504, y fallos de conexión o timeout: hasta **3 reintentos** (4 intentos). Espera = `retry-after` si viene (con tope de 60 s); si no, backoff exponencial `base·2^n` con tope y jitter completo. No gastan el reintento del arnés |
| Sin reintento | 400, 401, 403 y cualquier otro 4xx. Mensaje accionable: 401 → «revisa `ANTHROPIC_API_KEY`»; 403 → permisos de la organización o del modelo; 400 → el mensaje de la API |
| `stop_reason` | `max_tokens` → error `RespuestaTruncada`, **nunca** resultado válido. `refusal` → error `Rechazo` (salida controlada). `end_turn`, `tool_use` y `stop_sequence` se entregan; el adaptador decide |
| Secretos | La key solo vive en `SecretApiKey`. `Debug` y `Display` escriben `[REDACTED]`. Ningún error ni log incluye la key, ni las cabeceras, ni el cuerpo de la petición |
| Métricas | Cada respuesta trae `Uso` (`input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`), la latencia de reloj y el número de intentos |

### `AnthropicProcedenciaVerifier`

- Modelo `claude-haiku-5-5`, `max_tokens` 4096, **sin `temperature`** (§1).
- `system`: el prompt de procedencia como bloque de texto con `cache_control: ephemeral`.
- `tools`: una sola, `emitir_trazas_procedencia`, con el esquema cerrado adaptado a `strict` y `cache_control: ephemeral`.
- `tool_choice`: **`{"type": "auto"}` por defecto**, con la tool `strict` y la instrucción de responder solo con ella. Es un ajuste tras la calibración (informe `pieza5_fase51_20261115.md` §1): con `{"type": "tool", ...}`, Haiku 5.5 no piensa antes de llamar e invertía el signo del factor de escala en los cambios de unidad. La tool forzada queda como opción, `con_tool_forzada()`. Sin llamada, la cifra falla cerrada igual.
- `messages`: un turno `user` con la `SolicitudProcedencia` en JSON. Sin historial.
- Lectura: el primer bloque `tool_use` con ese nombre → `TrazasPropuestas` (con `deny_unknown_fields`). Sin `tool_use`, con otra tool o fuera del esquema → error: **falla cerrada**, la cifra no pasa, como hoy.

### `ModoLlm`

- `ModoLlm::Cli` (por defecto) o `ModoLlm::Api`, leído de `LAKEHOUSE_RAG_MODO_LLM` (`api` / `cli`, sin distinguir mayúsculas). Un valor desconocido → `Cli` con aviso.
- Con `Api` y sin key disponible, la composición vuelve a `Cli` y lo avisa por `tracing::warn!`: nunca se cae la app por la key.
- La key: variable de entorno `ANTHROPIC_API_KEY` en la CLI y los tests. `AlmacenKeyring` (crate `keyring`, `windows-native`) lee, guarda y borra la key en el almacén de credenciales del sistema; **sin comandos de GUI** en esta fase. Orden de búsqueda: entorno, luego keyring.

## 4. Plan TDD (sin red)

1. `SecretApiKey`: `Debug`/`Display` = `[REDACTED]`; vacía o solo espacios → error; `desde_valor_de_entorno` con ausente.
2. Cliente contra `wiremock`:
   - un 200 con `tool_use` → bloques, `stop_reason` y `Uso` leídos; cabeceras `x-api-key`, `anthropic-version` y `content-type` enviadas;
   - 429 con `retry-after: 0` y luego 200 → éxito con 2 intentos;
   - 529 y 503 repetidos → error tras 4 intentos;
   - 400, 401 y 403 → 1 solo intento y mensaje accionable;
   - `stop_reason: max_tokens` → `RespuestaTruncada`; `refusal` → `Rechazo`;
   - la key no aparece en el `Debug`/`Display` de ningún error, ni en los logs capturados.
3. Procedencia contra `wiremock`:
   - el cuerpo enviado lleva `tool_choice` forzado, `cache_control` en el system y en la tool, el modelo fijo y **no** lleva `temperature`;
   - un `tool_use` conforme → `TrazasPropuestas`;
   - sin `tool_use`, otra tool o un tipo fuera del esquema → error;
   - el esquema adaptado no contiene `oneOf`, `minimum`, `maximum` ni `maxItems`, y conserva los 4 tipos.
4. `ModoLlm`: por defecto `Cli`; `api`/`API` → `Api`; desconocido → `Cli`.

## 5. Calibración con Haiku real (paso 4 del encargo)

Batería sintética propia de 15 a 20 casos (extractos de tools + borrador), con cálculos de 2 a 4 operandos, fechas, artículos y cifras falsas sembradas, en varios idiomas y formatos numéricos. **Puerta:** ninguna cifra falsa aprobada. Se mide latencia p50/p95, tokens (input, `cache_creation`, `cache_read`, output) y coste en USD. Requiere `ANTHROPIC_API_KEY` en el entorno de la sesión; si no está, no se ejecuta.

## 6. Riesgos

| Riesgo | Mitigación |
|---|---|
| `strict: true` con un esquema que la API no acepte (400) | El 400 no se reintenta y el error es accionable; la calibración real lo detecta en la primera llamada. Sin `strict`, Rust sigue validando con `deny_unknown_fields` |
| Prefijo < 512 tokens: la caché no actúa | Se mide; si no actúa, se informa (no se rellena el prompt para forzarla) |
| Fuga de la key | Tipo opaco, errores sin cabeceras ni cuerpo, test que captura los logs |
| Retroceso en la ruta CLI | `Cli` sigue por defecto; la suite completa se compara con la línea base |
