# SSD: tercer motor del LLM, FreeLLMAPI

**Estado:** implementado en la rama `claude/motor-freellmapi-4d79f4`, desde `main` `d3986f3` (`v1.0.0`). Sin merge ni push. Amplía `ssd_gui_config_llm.md` y `ssd_pieza5_fase52_agente_api.md`.

## 0. Qué es y qué no es

FreeLLMAPI es un proxy autoalojado que reparte las peticiones entre los niveles gratuitos de varios proveedores. Habla la API de Messages de Anthropic (`POST <origen>/v1/messages`) y por detrás la traduce a formato OpenAI.

- **No es Claude.** Los modelos son de terceros, sin calidad de frontera, con latencia variable. La «inteligencia» baja cuando se agotan las cuotas diarias.
- **Los prompts, las preguntas y los extractos de los documentos se envían a esos proveedores.**
- **Calidad no medida.** Ningún A/B respalda este motor. Es un experimento de laboratorio.

## 1. Documentación oficial comprobada (2026-11-17)

Leída en `tashfeenahmed/freellmapi`: `README.md`, `docs/en/api/01-rest-api.md` y `docs/en/clients/01-agent-clients.md`.

- Auth con `x-api-key` o `Authorization: Bearer`, y `anthropic-version: 2023-06-01`.
- Traduce streaming, `system`, tools e imágenes. No documenta `thinking` ni `cache_control`. Acepta `anthropic-beta`, pero **no lo reenvía**: los proveedores «pueden rechazar campos no admitidos».
- `GET /v1/models` responde en formato Anthropic si se envía `anthropic-version`. `?available=true` filtra los disponibles.
- Router: `auto`, `auto:fast`, `auto:smart`, `auto:cheap`, `auto:reliable` y `auto:balanced`.
- La app de escritorio escucha en `http://127.0.0.1:31415` (el servidor de fuentes, en `3001`).

## 2. Medido contra el proxy real (2026-11-17, una ejecución)

Proxy de escritorio del usuario en `127.0.0.1:31415`, con tools sintéticas y sin centros.

| Prueba | Resultado |
|---|---|
| `GET /v1/models?available=true` con `anthropic-version` | 200. 36 entradas `{type,id,display_name,created_at}`, con `auto` y las ranuras `claude-*-4-5`. **No trae `execution_status` ni las variantes `auto:*`.** |
| El mismo `GET` sin `anthropic-version` (formato OpenAI) | Trae `execution_status`. Varios modelos `exhausted` aparecen igualmente con `available=true`. |
| Key inválida | 401 |
| `POST /v1/messages` en streaming con `thinking` adaptativo, `cache_control` en `system`, tool y mensaje, `strict: true`, `anthropic-beta` y `anthropic-version: 2026-10-21` | **200.** `tool_use` correcto, `stop_reason: tool_use` y `usage` con `input_tokens` y `output_tokens`. No devolvió bloques `thinking` ni tokens de caché. |
| Segundo paso con `tool_result` y una directiva `role: "system"` a mitad de conversación (turno dirigido) | 200. Texto con la cifra exacta de la tool y `stop_reason: end_turn`. |

**Conclusión.** Hoy el proxy acepta y descarta en silencio los campos que no traduce. No hace falta reintentar sin ellos. Aun así **van desactivados de serie**:
1. no aportan nada, porque el proxy no los reenvía;
2. la documentación avisa de que un proveedor puede rechazarlos;
3. una petición más corta es más fácil de depurar.

Así no se añade lógica de reintento, que sería ruido (CLAUDE.md §6).

## 3. Diseño

### 3.1 Perfil de compatibilidad, en un único sitio

`ConfigClienteAnthropic` lleva un `PerfilApi`:

| Campo | Anthropic | FreeLLMAPI |
|---|---|---|
| `version_api` (cabecera `anthropic-version`) | `2026-10-21` | `2023-06-01` (la documentada) |
| `thinking` | sí | no |
| `cache_control` | sí | no |
| `strict` en las tools | sí | no |
| `betas` (`anthropic-beta`) | sí | no |
| alias de la CLI (`sonnet` → `claude-sonnet-5-5`) | sí | no: el id del desplegable va tal cual |
| tarifas (coste en USD) | sí | no: coste **desconocido** (`None`), nunca 0 |
| `stop_reason` ausente | se acepta, como hasta ahora | **falla cerrado** (`RespuestaNoValida`) |

`AnthropicHttpClient` aplica el perfil a la petición justo antes de enviarla (`PerfilApi::adaptar`). El runner y la procedencia no se duplican ni cambian su construcción: el perfil quita lo que no toca.

### 3.2 El modo API no cambia

Por indicación de la sesión coordinadora, todo lo nuevo va detrás del perfil. Con `PerfilApi::ANTHROPIC`, la petición, la validación de `stop_reason` y el coste son idénticos a `v1.0.0`. Lo comprueban `test_el_perfil_anthropic_no_toca_la_peticion` y el caso Anthropic de `test_sin_stop_reason_falla_cerrada_con_el_perfil_freellmapi_y_anthropic_no_cambia`.

- `max_tokens` y `refusal` fallan en los dos perfiles, como siempre.
- **Descartado:** se probó a hacer generales «`stop_reason` ausente falla cerrado» y «sin `usage`, coste desconocido», pero cambiaban el modo API. El coste de FreeLLMAPI ya es desconocido por perfil, tenga o no `usage`.

### 3.3 Modo y configuración

- `ModoLlm::FreeLlmApi` (`freellmapi` en el fichero, en `LAKEHOUSE_RAG_MODO_LLM` y en la ventana). **El valor por defecto sigue siendo `Cli`.**
- `llm_config.json` pasa a `{"modo_llm": "...", "freellmapi_url": "..."}`. La URL es opcional (por defecto, `http://127.0.0.1:31415`) y no es secreta. Guardar el modo conserva la URL, y al revés.
- **Validación de la URL:**
  - esquema `http` o `https`, con host;
  - sin usuario ni contraseña, sin query ni fragmento, y sin más ruta que `/`;
  - como mucho 256 caracteres.

  Un valor inválido se rechaza con un error fijo que no lo repite. Si no es de loopback, el modal avisa de que la key y los prompts viajan por la red.
- **Composición:**
  - con la key resuelta, `AnthropicMessagesRunner` sobre un cliente con la URL y el perfil de FreeLLMAPI;
  - sin key, respaldo a `claude -p` con un aviso, igual que el modo API;
  - `modo_en_uso` distingue los tres motores;
  - la destilación de la ingesta sigue en `claude -p` en todos los modos, como en el modo API.
- La corrección de omisión del arnés sigue activa: la declara el runner.

### 3.4 La key

- Entrada propia en `keyring`: servicio `lakehouse_rag`, usuario `freellmapi_api_key`, distinta de `anthropic_api_key`.
- La variable `FREELLMAPI_API_KEY` manda sobre el almacén.
- Comandos `estado_freellmapi_key` y `guardar_freellmapi_key`, sobre las mismas funciones que la key de Anthropic. La máscara conoce el prefijo público `freellmapi-` (`freellmapi-…XXXX`).
- **Una key nunca va al otro motor.** El modo FreeLLMAPI solo resuelve `FREELLMAPI_API_KEY` o su entrada, y el modo API solo la de Anthropic.

### 3.5 Modelos del desplegable

`FreeLlmApiModelCatalog` implementa `ModelCatalogPort` y se compone en lugar del catálogo de Claude Code cuando el motor en uso es FreeLLMAPI.

- **Petición:** `GET /v1/models?available=true` con `anthropic-version` y la key.
- **Orden:**
  1. Arriba, `auto`, `auto:fast`, `auto:smart`, `auto:cheap`, `auto:reliable` y `auto:balanced`. Son entradas del router y siempre valen: la documentación dice que se listan siempre, aunque el formato Anthropic no traiga las variantes.
  2. Después, el resto de los ids, ordenados y sin duplicados.
- **Validación:**
  - cada id pasa por `ClaudeModel::new`;
  - los que no la pasan se descartan;
  - el nombre visible se recorta a 120 caracteres;
  - como mucho 500 entradas;
  - la respuesta, como mucho 1 MB.
- **`ClaudeModel` admite ahora `:`.** Hace falta para `auto:smart`. Va como argumento de `Command`, sin shell, así que `:` no abre ninguna inyección. Siguen rechazados los espacios, `;`, `&`, `$`, los saltos de línea y el `-` inicial.
- **Frontend:** el texto va con `textContent`. Si el modelo preferido guardado no está en la lista, se elige el primero, que es `auto`. Si el proxy no responde, el desplegable queda desactivado con «Catálogo de modelos no disponible», y el registro dice que no se pudo conectar con FreeLLMAPI. La GUI sigue operativa.

### 3.6 Nivel 2 de procedencia: FreeLLMAPI con `auto:smart`

**Decisión:** en este motor el Nivel 2 usa el mismo proxy con `auto:smart`, sin streaming y con `tool_choice: auto` (como Haiku).

**Por qué:**
- **El Nivel 2 solo propone.** `arnes_turno::procedencia` comprueba en Rust cada traza (literales copiados de las salidas, aritmética recalculada). Un modelo flojo solo puede **no** rescatar una cifra, y eso falla cerrado. No puede aprobar una cifra falsa.
- **Desactivarlo** también fallaría cerrado, pero bloquearía todas las cifras derivadas (escalas, sumas, conteos). La experiencia sería peor sin ganar seguridad.
- **La key de Anthropic no se usa nunca** sin que el usuario elija el modo API. Mezclar motores tampoco está medido.
- **`auto:smart`** porque el Nivel 2 premia la exactitud sobre la velocidad.
- **Exposición de datos:** la procedencia manda a los mismos proveedores lo que el agente ya les ha mandado (la pregunta, las salidas de tools y el borrador). No añade exposición nueva.

### 3.7 Aviso en el modal

Una línea fija bajo la opción: «Modelos gratuitos de terceros; los prompts y extractos se envían a esos proveedores; calidad no medida».

## 4. Pruebas

- **Con dobles (`wiremock`):**
  - el perfil quita `thinking`, `cache_control`, `strict` y `anthropic-beta`, y envía `anthropic-version: 2023-06-01`;
  - `stop_reason` ausente falla cerrado;
  - el coste queda desconocido sin `usage`;
  - el catálogo: orden, `auto` arriba, ids hostiles descartados y proxy caído.
- **Unitarias:** modo y fichero (con la URL), estado del modal con tres motores, máscara `freellmapi-` y `ClaudeModel` con `:`.
- **Adversarial (`tests/adversarial_api_key_gui.rs`):** la key de FreeLLMAPI no llega al estado, a los errores, a los logs ni al fichero. Las dos keys no se cruzan.
- **Humo real** (2026-11-17, una ejecución, equipo con carga): `examples/humo_agente_api.rs --motor freellmapi`, con las tools sintéticas del humo de la 5.2, modelo `auto` y 3 preguntas.
  - **3/3 correctas por fragmentos** («16», «125»).
  - **10 llamadas del agente,** una por encima del tope de menos de 10 del encargo: la pregunta abierta encadenó 6 pasos de tools.
  - 78 s en total. Una pregunta tardó 48 s por **1 reintento de transporte**.
  - `stop_reason` y `usage` en todos los pasos, y coste desconocido.
  - **El Nivel 2 con `auto:smart` no está comprobado en real.** El humo no registra las llamadas de procedencia, así que no consta que se llegara a hacer alguna. Solo está probado con dobles.
- **Manual:** el aspecto del modal y el desplegable con el proxy real en la ventana. La ventana no se lanza para no interferir con la app abierta del usuario.
