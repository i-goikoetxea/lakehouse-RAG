# SSD Pieza 5, Fase 5.2: el agente por la API de Messages

**Estado:** implementado en la rama `claude/pieza5-fase52-a1f4cf`, desde `main` `5e88f6e`. Sin merge ni push. Contrato previo: `propuesta_pieza5_api_anthropic.md` y `ssd_pieza5_fase51_procedencia_api.md`. Informe: `benchmarks/informes/pieza5_fase52_20261116.md`.

## 0. Decisiones confirmadas antes de empezar (2026-11-16)

Por la sesión principal y por el usuario:

1. **Referencias jerárquicas.** La regla `N.M` literal se aplica tal cual. c12 («apartado 3 del artículo 7») queda como límite declarado, sin listas.
2. **Cancelación.** Se aborta soltando el future (`Notify` + `select!` de `chat_service`), sin `tokio-util`.
3. **Corrección de omisión.** Solo en modo API, para que el modo CLI quede idéntico en el A/B.

## 1. Referencias compuestas «N.M» (primer commit)

Regla general, sin listas de palabras ni de unidades:

- **Lectura literal.** Un número con algún grupo tras el separador que no tiene tres dígitos (`7.3`, `4.25`, `4.12.1`) es un token literal indivisible (`exige_lectura_literal`). `1.250` sigue siendo de miles.
- **Nivel 2 (`contiene_los_digitos`).** Si la cifra o el número de la subcadena exigen lectura literal, solo vale el mismo token, con sus separadores:
  - «Art. 7.3» ya no respalda «73», ni «7»;
  - «Art. 73» ya no respalda «7.3».
- **Mismo precedente.** Además, una cifra `N.M` solo vale si, en alguna aparición del borrador, la precede la misma palabra (plegada, con su punto de abreviatura) o el mismo marcador (`§`, `#`, `-`) que en la subcadena (`mismo_precedente`). Así «pesa 7.3 kg» no se rescata con «Artículo 7.3».
- **Nivel 1 (`es_referencia_estructural`).** Para `N.M`:
  - no se prueba la variante con la coma;
  - no vale cualquier marcador, solo lo que precede a la cifra en la respuesta.
- **Límite declarado.** La paráfrasis que descompone el identificador (c12, «apartado 3 del artículo 7») no se rescata. Hacerlo exigiría leer los componentes por separado, y eso aprobaría el redondeo de c19 («4 m» con una fuente de «4.25 m»).

## 2. `AnthropicMessagesRunner` (`infrastructure/llm/anthropic_api/agente.rs`)

Implementa `AgentRunnerPort` con `AnthropicHttpClient` y streaming SSE en todos los pasos. Así se mide el `ttft` y se evitan los timeouts de las respuestas largas.

| Parámetro | Valor | Motivo |
|---|---|---|
| `model` | `claude-sonnet-5-5`; los alias `sonnet`/`default`, `haiku` y `opus` se traducen, y un id `claude-*` pasa tal cual | encargo |
| `tool_choice` | `{"type":"auto"}` en todos los pasos | Sonnet 5.5 y Opus 5.5 dan 400 con `any`/`tool` (medido en la 5.1) |
| `temperature` | no se envía | 400 en la versión `2026-10-21` |
| `thinking` | `{"type":"adaptive","block_binding":{"prefix_mismatch_behavior":"drop_block"}}` con la cabecera beta `thinking-binding-controls-2026-08-01` | ver §2.1 |
| `cache_control` | ephemeral en el bloque `system`, en la última tool (encargo) y en el último bloque del historial (añadido: abarata los pasos 2..N; 3 de los 4 puntos permitidos) | encargo y medida del humo |
| `max_tokens` | 32 000 | el pensamiento cuenta; un corte nunca vale |
| `strict` en las tools del agente | no | sus esquemas traen palabras clave que `strict` no admite; los argumentos los valida el servidor de tools |
| `fallbacks` (beta de reserva por rechazo) | no | un cambio silencioso de modelo contaminaría el A/B; `refusal` falla cerrada con error |

### 2.1 Bloques `thinking` y la comprobación de historial

Sonnet 5.5 piensa por defecto, y cada bloque `thinking` queda ligado al prefijo de la conversación que lo produjo.

- **Devolución literal.** El runner guarda el contenido original de cada turno de tools, con su `thinking` y su firma, indexado por los id de `tool_use`. Lo devuelve sin tocar en los pasos siguientes.
- **Texto final.** Del texto final no se devuelve el pensamiento, porque no se guarda. Quitar un bloque de cola no invalida los posteriores.
- **Red para lo imprevisto.** La GUI recorta el historial y la directiva del turno dirigido no está en el historial del orquestador. Para eso, `drop_block` hace que la API descarte los bloques afectados en lugar de dar un 400. Cada descarte se cuenta en la traza (`bloques_thinking_descartados`).
- **Directiva persistente.** La directiva de un turno dirigido se guarda con ese turno y se vuelve a insertar delante de él en cada paso. El historial que ve la API es así de solo añadir.

### 2.2 Conversión del historial

- `User` → mensaje `user` de texto.
- `Assistant` con `tool_calls` → el contenido original guardado o, si no está, bloques `tool_use` reconstruidos.
- `Assistant` de texto → bloque `text`; los vacíos se omiten.
- `Tool` → bloques `tool_result`. Los resultados seguidos van en **un solo** mensaje `user`, y un texto de usuario detrás (un recordatorio del arnés) se añade a ese mismo mensaje.

### 2.3 `ejecutar_turno_dirigido`

Mismo `auto`, con la instrucción explícita como mensaje `role: "system"` intermedio, que Sonnet 5.5 admite sin beta, tras el último turno del usuario. Ese mensaje nombra la tool y exige llamarla en este turno. Un `tool_use` de esa tool da `TurnoDirigido::Llamada`; cualquier otra cosa da `SinLlamada`.

### 2.4 SSE

- `content_block_start` / `content_block_delta` (`text_delta`, `input_json_delta`, `thinking_delta`, `signature_delta`) / `content_block_stop` / `message_delta` / `message_stop`, más `ping` y `error`.
- Los `input_json_delta` se acumulan en silencio y se parsean en `content_block_stop`.
- Solo los `text_delta` llegan al observador, y solo en `ejecutar_turno_en_vivo`, que el orquestador usa sin `retener_texto`.
- Con `retener_texto`, el orquestador llama a `ejecutar_turno`: se acumula en silencio y nada sale antes de validar.
- Si el guard rechaza en `message_stop`, el orquestador emite `TextoEnVivoDescartado`, igual que con la CLI.
- **Detener:** soltar el future cierra el socket. Después no se emite ningún `TextDelta`, y hay un test que lo comprueba.
- **Reintentos de transporte** (429, 529, 5xx, conexión) solo antes del primer evento de contenido. Un error a mitad del flujo falla sin reintentar, porque ya pudo emitirse texto.
- **`stop_reason`:** `max_tokens` → `RespuestaTruncada` (nunca válida); `refusal` → `Rechazo`.

### 2.5 Trazas por paso (`MetricaPasoApi`)

`ttft_ms` (hasta el primer delta de contenido), `latencia_ms`, `intentos`, tools invocadas, `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`, `coste_usd`, `stop_reason` y `bloques_thinking_descartados`.

- El runner las guarda; `tomar_metricas()` las devuelve y vacía el registro.
- También van a `tracing` (nivel `info`), sin texto del usuario ni secretos.
- **Tarifas** (skill `claude-api`, 2026-11-16), en USD por millón de tokens:

| Modelo | Entrada | Escritura de caché (5 min) | Lectura de caché | Salida |
|---|---|---|---|---|
| Sonnet 5.5 | 2,00 | 2,50 | 0,10 | 10,00 |

## 3. Corrección de omisión en el orquestador

- **Declaración del runner:** `AgentRunnerPort::necesita_correccion_de_omision()`, `false` por defecto; el runner de la API devuelve `true`.
- **Cuándo actúa:** la intención no es `Ninguna`, el modelo da un mensaje final y aún no se ha ejecutado ninguna tool en el turno (incluido el `SinLlamada` del turno dirigido).
- **Qué hace:**
  - emite `ArnesIntervino { etapa: OmisionDeHerramienta }`, que cuenta como métrica del A/B;
  - añade a la vista del modelo **un** mensaje que exige consultar (y nombra la tool dirigida, si la había);
  - repite el paso con `auto`.
- **Si vuelve a omitirla:** falla cerrada con `MENSAJE_RESPUESTA_NO_VALIDADA` y `SalidaSegura`, sin gastar el reintento del guard.
- **Modo CLI:** queda idéntico, porque su runner devuelve `false`.

## 4. Configuración y Tauri

- `LAKEHOUSE_RAG_MODO_LLM=api|cli`, con `cli` por defecto. Con `api`, la composición usa `AnthropicMessagesRunner` para el agente y Haiku por la API para la procedencia.
- **Sin key, o si el cliente no se construye,** vuelve a la CLI con un aviso.
- **Comandos Tauri:**
  - `guardar_api_key(key)` escribe en el almacén del sistema (`keyring`) y devuelve solo el estado;
  - `estado_api_key()` devuelve `{ configurada, mascara }`, con una máscara `sk-ant-…` de los 4 últimos caracteres.
- El modo se lee al arrancar: tras guardar la key, la API se usa al reiniciar.
- **El secreto no sale.** Nunca llega al WebView, a los logs ni a la telemetría. Lo comprueba `tests/adversarial_api_key_gui.rs`, que cubre la carga útil de los comandos, la captura de `tracing`, los errores 400/401 que repiten la key y las métricas serializadas.

## 5. Herramienta A/B (`examples/ab_agente.rs`)

Pregunta sobre un centro ya ingerido y activo. No crea, activa ni reingiere centros.

- **Uso:** `cargo run --release --example ab_agente -- --centro <id> --preguntas <jsonl> --modo api|cli --salida <jsonl> [--modelo sonnet] [--repeticiones N]`.
- **Entrada:** JSONL con `{ "id", "pregunta" }`.
- **Salida:** una línea por pregunta y repetición, con:
  - la respuesta, `entregada`, el error y `latencia_ms`;
  - `herramientas` (`nombre`, `argumentos`, `resultado`, `es_error`, el formato de `benchmark_crudas`, que se puede repetir con `tests/repeticion_arnes_guardadas.rs`);
  - `arnes` y `omisiones_de_herramienta`;
  - `pasos_api` y los totales de tokens y coste (vacíos en modo CLI).
