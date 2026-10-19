# Informe de referencia: maqueta del chat de la app de escritorio de ChatGPT (Windows 11)

> Fase 15 · Investigación de solo lectura · 2026-10-19
> Objetivo: documentar cómo se ve y cómo se comporta el chat de la app de ChatGPT para replicar el patrón en la GUI Tauri de Lakehouse RAG (`src/presentation/gui/web/`).

## 0. Identificación del objetivo y método

| Dato | Valor observado |
|---|---|
| Proceso | `ChatGPT.exe` (PID 432744, ventana «ChatGPT»), con ~13 procesos hijos Chromium (gpu, renderer, network…) |
| Paquete | `C:\Program Files\WindowsApps\OpenAI.Codex_26.1007.9446.0_x64__2p2nqsd0c76g0\app\` (la app «ChatGPT» de escritorio es la superapp Codex) |
| Tecnología | **Electron** (Chromium 155, `resources\app.asar` de 567 MB, esquemas `app://` y `codex-sandbox://`). No es WebView2. |
| Frontend | React + Tailwind v4 + CSS Modules, en `app.asar:/webview/` (`index.html` + `assets/*.css|js`) |
| CDP / depuración | **No disponible**: el proceso principal se lanzó sin `--remote-debugging-port` y no hay puertos en escucha propiedad de `ChatGPT`. Por eso no se pudo usar `list_electron_targets` / `inspect_electron_page` ni leer estilos calculados en vivo. `observe_native_ui` solo aplica a Mach-O (macOS). |

Método aplicado (solo observación, sin clics ni escritura en el chat):

1. **Captura pasiva** de la ventana con `PrintWindow` (Win32), ventana 1280×820 px a 96 DPI (escala 100 %). Medidas en píxeles y muestreo de colores sobre esa captura.
2. **Análisis estático** del `app.asar`: lectura de la cabecera ASAR y extracción (en el scratchpad, fuera del repo) de los 15 091 ficheros CSS/HTML/JS de `/webview/`; búsqueda de variables CSS, clases Tailwind y lógica de scroll en los bundles.

Leyenda: **[O]** observado en la captura · **[E]** extraído del código empaquetado (valor real, pero no verificado en ejecución) · **[I]** inferencia.

Capturas en `docs/validacion/fase15/referencia_chat/`:
- `01_ventana_actual.png` — ventana completa (1280×820).
- `02_burbuja_y_trabajado_x2.png` — burbuja de usuario y fila «Ha trabajado durante…» (×2).
- `03_composer_x2.png` — composer (×2).
- `04_respuesta_asistente_x2.png` — respuesta del asistente (×2).

---

## 1. Columna de conversación: ancho y centrado

Layout de la ventana [O]: rail de iconos 0–52 px · barra lateral de chats 52–340 px · área de hilo 341–1280 px (939 px útiles) · cabecera de hilo de ~40 px con título.

| Propiedad | Valor |
|---|---|
| Ancho máximo de contenido | `--thread-content-max-width: 48rem` (768 px) [E], aplicado como `mx-auto w-full max-w-[var(--thread-content-max-width)]` [E] |
| Padding interno de la columna | `--thread-content-margin: calc(var(--spacing)*4)` = 16 px (variantes de 12 px) [E] → texto visible de **736 px** (x 441→1177) [O] = 768 − 2×16 ✔ |
| Centrado | `margin-inline: auto`; centro de columna x≈809 vs centro del área 810 [O] |
| Gutter adaptable | `--thread-gutter: clamp(12px, 3cqi, 24px)` (unidades de *container query*) [E] |
| Ancho del cuerpo | `--thread-body-max-width: calc(var(--thread-content-max-width) + var(--thread-body-inline-padding)*2)` [E] |
| Margen superior | `--thread-content-top-inset: calc(var(--spacing)*8)` = 32 px (+ altura de toolbar si flota) [E] |
| Hueco entre turnos | `gap-8` (32 px) en el contenedor de turnos; `--conversation-item-gap: 16px` [E] |
| Base de espaciado | `--spacing: .25rem` (4 px) [E] |

Con ventanas estrechas la columna simplemente ocupa el 100 % menos el gutter; no hay saltos de breakpoint, se usa `@container` [E].

## 2. Mensajes

### 2.1 Usuario (burbuja)

| Propiedad | Valor |
|---|---|
| Alineación | derecha: contenedor `group/user-message flex flex-col items-end gap-2` [E]; borde derecho de la burbuja = borde derecho de la columna (x 1177) [O] |
| Ancho máximo | `max-w-(--user-chat-width)` con `--user-chat-width: 70%` [E]; medido 516 px de 736 = 70 % [O] (otras variantes: `min(456px,100%)`) |
| Fondo | `bg-user-message` → `--color-background-user-message` (en tema por defecto `--color-surface-tertiary`: `#f3f3f3` claro / `#414141` oscuro) [E]. En la captura es **#873e60** porque el usuario tiene un acento rosa personalizado [O] |
| Texto | `--color-text-user-message: var(--color-text)` [E]; en la captura texto claro rosado #d1b3c1 sobre #873e60 [O] |
| Radio | `rounded-2xl` = 16 px (`--radius-2xl-base: 1rem` × `--corner-radius-scale: 1`) [E]; perfil de esquina medido ≈16–18 px [O] |
| Padding | `px-(--thread-content-margin)` (12–16 px) y ~10 px vertical [E/O]; alto de burbuja de 2 líneas = 68 px [O] |
| Otros | `min-w-0 overflow-hidden break-words` [E]. Mensajes largos se truncan con «…» y se expanden (se observó una fila del usuario truncada) [O] |

### 2.2 Asistente (sin burbuja)

| Propiedad | Valor |
|---|---|
| Contenedor | sin fondo ni borde, a ancho completo de la columna (x 441→1177) [O] |
| Fuente | `--font-sans-default: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif` → **Segoe UI** en Windows [E] |
| Tamaño | `--codex-chat-font-size` (por defecto `--font-ui-size`, fallback 13 px; texto medido ≈14 px) [E/O] |
| Interlineado | `--markdown-line-height: calc(font-size * 1.625)` (`--leading-relaxed: 1.625`) [E]; medido **22 px** entre líneas [O] |
| Espacio entre párrafos | `--markdown-space: font-size/4`; párrafo+párrafo = `margin-block: calc(var(--markdown-space)*4)` = 1 em [E]; medido ≈15 px [O] |
| Encabezados | h1 1.75em/600, h2 1.5em (mt 2rem, mb 1rem), h3 1.25em (mt 1.125rem, mb .625rem), h4 1em [E] |
| Listas | `padding-inline-start: 1.625em`, `li { margin: .5em 0 }`, marcadores en `--color-text-secondary` [E] |
| Negritas | `font-weight: 600` (semibold) [E]; en la captura las cifras clave van en negrita blanca [O] |
| Enlaces a ficheros | chip inline con icono y color de acento (`space-free.exe` en rosa) [O] |
| Tablas | `width:100%; margin:2em 0; font-size:.875em; line-height:1.714`; sin bordes verticales; `th` con `border-bottom: 1px solid var(--color-border-strong)`, `td` con `border-bottom: 1px solid var(--color-border)`; padding 8 px/10 px, primera y última columna sin padding lateral; última fila sin borde [E] |
| Código inline | fondo `alpha-10` (oscuro) / `alpha-04` (claro), `box-shadow: inset 0 0 0 1px alpha-08`, `border-radius:.25rem`, `padding:1px .3rem`, `font-size:.875em`, `font-weight:500` [E] |
| Bloques de código | `border:1px solid var(--color-border)`, `border-radius: var(--radius-md)`, `padding: 14px 40px 14px 16px`, `font-size:.875em`, `line-height:1.5em`, `overflow:auto`; botón copiar absoluto arriba-derecha (`padding:10px 10px 0 0`) [E]. Fuente mono: `ui-monospace, "SFMono-Regular", "SF Mono", Menlo, Consolas, …` a 12 px (`--codex-chat-code-font-size`) [E] |
| Citas | barra izquierda de 4 px con `alpha-20`, `padding-inline-start: 24px` [E] |

### 2.3 Turnos de trabajo (agente)

Entre el mensaje del usuario y la respuesta final aparece una fila plegable **«Ha trabajado durante 18m 7s ›»** en texto secundario (#8a8a8a aprox.) con un separador inferior de 1 px (#2b2b2b) [O]. Clave i18n `localConversation.workedFor` [E]. Al hacer clic despliega las acciones intermedias (no se pulsó).

## 3. Barra de acciones bajo los mensajes

- **Usuario**: los controles (`turn-action-controls`) están en `opacity-0` y aparecen con `group-hover/user-message:opacity-100` y `group-focus-within:opacity-100`; en pantallas táctiles `[@media(hover:none)]:opacity-100` (siempre visibles) [E].
- **Asistente**: mismo patrón `opacity-0 group-hover:opacity-100 group-focus-within:opacity-100` [E]. En el chat de ChatGPT la barra entra con animación `initial {opacity:0, y:8}` → `{opacity:1, y:0}`, `duration .3s`, `delay .25s` tras completarse la respuesta [E, chunk `landing`].
- **Copiar**: botón solo icono con dos estados `icons: {copy, copied}` (portapapeles → check), tooltip «Copiar» / «Copiar respuesta» (`assistantMessageContent.copyResponseTooltip`) [E]. Feedback = cambio de icono a check y texto «Copiado» [E]. La duración exacta del estado «copiado» no se pudo extraer.
- En la captura no se ven acciones porque no hay hover [O]. No se observó «regenerar» en la vista de Codex (es un hilo de agente, no ChatGPT clásico).

## 4. Composer (input)

| Propiedad | Valor |
|---|---|
| Posición | fijo abajo, dentro de la misma columna centrada (x 441→1177, mismo ancho que el texto) [O]; separación inferior `--thread-composer-bottom-inset: 16–24 px` [E]; medido 20 px hasta el borde de ventana [O] |
| Radio | `--composer-radius: calc(var(--spacing)*5.5)` = **22 px** (variantes 28 px / `--radius-4xl`) [E]; perfil medido ≈20–22 px [O] |
| Fondo | `--composer-layout-surface-background: var(--color-surface-secondary)` [E]; medido **#363636** sobre fondo de hilo **#181818**, borde 1 px **#3a3a3a/#3e3e3e** [O] |
| Sombra | claro: `0 0 0 1px #0000000a, 0 2px 8px #0000000a, 0 4px 80px 8px #00000006`; oscuro: `inset 0 0 1px 0 #fff3` (solo un filo interior) [E] |
| Altura mínima | `--min-height-composer: calc(var(--spacing)*11)` = 44 px [E] |
| Line-height editor | 20–24 px (`--line-height-composer: calc(var(--spacing)*5)`) [E] |
| Padding editor | `calc(spacing*3.75) calc(spacing*4.5) calc(spacing*4)` = 15/18/16 px [E] |
| Crecimiento | auto-crece con el contenido hasta `max-height: min(25dvh, var(--composer-editor-max-height, 25dvh))`; a partir de ahí **scroll interno** con scrollbar propia [E/O: en la captura el editor está en su altura máxima, 259 px incl. pie, con scrollbar a la derecha] |
| Modo expandido | existe `[data-composer-expand-toggle]` que pasa a `max-h-none`/`h-full` [E] |
| Placeholder | opacidad `.5` (`--opacity-composer-placeholder`) [E]; textos contextuales: «Pedir cambios de seguimiento», «Trabajando…» mientras corre [E] |
| Pie (footer) | izquierda: **+** (adjuntar/menú), etiqueta de permisos «⚠ Acceso completo» en naranja; derecha: selector «**GPT-6.1 Sol** Medio ⌄» (modelo + esfuerzo en gris), **micrófono**, y botón circular de 32 px rosa (#f077af) que con el editor vacío muestra icono de voz (ondas) y pasa a enviar/detener [O]; tamaño de botón `--spacing-token-button-composer: 32–36 px` [E] |
| Atajos | ajuste «Atajo para enviar» con 3 modos: `Enter`, «{Ctrl} + Enter para mensajes multilínea», «Siempre {Ctrl} + Enter» [E]. Por defecto Enter envía y Shift+Enter inserta salto [I]. |

## 5. Selector de modelo/herramientas y estado «pensando»

- El selector de modelo vive en el pie del composer como texto plano + chevron (sin borde), con el esfuerzo de razonamiento en color secundario («Medio») [O].
- Estado en curso: placeholder «Trabajando…», etiquetas «Pensando» [E]. El texto de actividad usa un **shimmer** de gradiente que barre el texto: `@keyframes activity-shimmer {0%{background-position:-100% 0} 40%,to{background-position:250% 0}}`, `animation: 5s steps(48,end) infinite` [E]. Duraciones/placeholders de carga usan `duration-shimmer` 2.2 s ease-in-out alternado [E].
- Al terminar, el bloque de trabajo se colapsa en «Ha trabajado durante X ›» [O].

## 6. Streaming y scroll

### 6.1 Aparición del texto
- **Sin cursor parpadeante.** Cada palabra nueva se envuelve en `[data-d-stream-word]` y entra con fundido: `animation: .7s backwards d-word-fade-in`, con `@keyframes {0%{opacity:0} 6%{opacity:.18} 12%{opacity:.72} 15%,to{opacity:1}}` (≈105 ms efectivos de fundido) [E].
- Marcadores de lista: fundido de 110 ms; iconos 180 ms; medios 450 ms [E]. Todo bajo `@media (prefers-reduced-motion:no-preference)` [E].

### 6.2 Auto-scroll (máquina de estados de «follow mode») [E, `local-conversation-thread-*.js`]
- Umbral de «pegado al final»: **24 px** de distancia al fondo.
- Modos: `user_follow` (sigue el final), `prework_follow`, `prework_watch`, `static`.
- `scroll_distance_changed`: si distancia ≤ 24 px se mantiene; si el usuario sube más de 24 px, pasa a `static` (**se suelta**).
- `scroll_to_bottom` (pulsar el botón) → vuelve a `user_follow` y salta con `scrollToDistanceFromBottomPx(0, 'instant')`.
- Al terminar el turno (`idle`) → `static`.
- **Anclaje del nuevo mensaje del usuario**: evento `latest_turn_placed` + un `responseSpacer` (espaciador de altura dinámica bajo el último turno) que permite colocar el último mensaje del usuario arriba del viewport y que la respuesta crezca debajo sin empujarlo; el espaciador se consume (`consumeGrowth`) según crece la respuesta [E].

### 6.3 Botón «bajar al final»
- Etiqueta «Ir al final» (`localConversation.scrollToBottomButton`) [E].
- Botón circular flotante centrado horizontalmente en la columna, justo encima del composer, con flecha ↓ (x≈809, y≈500) [O]. Patrón del bundle: 28×28 px, `border-radius:999px`, `border:1px solid var(--color-border)`, `background: var(--color-surface-elevated)`, `box-shadow: var(--shadow-200)`, aparece/desaparece con `transition: opacity .15s` [E].

## 7. Scrollbar y estado vacío

- Scrollbar del hilo **visible y fina** a la derecha del área (x≈1270), pulgar gris claro sobre pista oscura, estilo Chromium overlay-like [O]; tokens `--color-token-scrollbar-slider-background` / `-hover-background` (= `--app-color-border-heavy`) [E]. El composer y la barra lateral tienen su propia scrollbar [O].
- Estado vacío: no observado (la conversación abierta tiene contenido). En el código existe una home con composer centrado (`--home-composer-inline-inset`, `composer-home-top-menu`) [E].

## 8. Paleta medida (tema oscuro, captura) [O]

| Elemento | Color |
|---|---|
| Fondo del hilo | `#181818` |
| Barra lateral | `#1a1b1e` · rail `#1e2024` · ítem activo `#2c2d30` |
| Separador panel lateral | `#2e323c` |
| Composer | `#363636`, borde `#3a3a3a`, scrollbar `#474747` |
| Burbuja usuario (acento personalizado) | `#873e60`; texto `#d1b3c1` |
| Botón enviar/voz | `#f077af` |
| Texto asistente | blanco ~#e8e8e8, secundario ~#a8a8a8 |
| Separador fila de trabajo | `#2b2b2b` |

---

## Recomendaciones para Lakehouse RAG

Requisitos concretos para `src/presentation/gui/web/` (`styles.css`, `app.js`):

**Layout**
1. `--chat-max-width: 48rem; --chat-pad-x: 16px;` columna `.chat-thread__content { max-width: var(--chat-max-width); margin-inline: auto; padding-inline: var(--chat-pad-x); }` → 736 px de texto.
2. El composer usa **el mismo contenedor centrado** que los mensajes (mismo ancho y bordes alineados).
3. Separación vertical entre turnos: `gap: 32px`; margen superior del hilo 32 px.

**Mensajes**
4. Usuario: `align-self:flex-end; max-width:70%; border-radius:16px; padding:10px 16px; background:var(--user-bubble-bg)` (oscuro `#414141` neutro o un acento atenuado); `overflow-wrap:anywhere`.
5. Asistente: sin burbuja; `font: 14px/1.625 "Segoe UI", system-ui, sans-serif` (≈22–23 px de línea); `p + p { margin-top: 1em }`; `strong { font-weight: 600 }`.
6. Tablas: `width:100%; margin:2em 0; font-size:.875em;` solo bordes horizontales (`th` borde fuerte, `td` borde suave, última fila sin borde, primera/última columna sin padding lateral).
7. Código: inline con fondo alfa + `box-shadow: inset 0 0 0 1px` y radio 4 px; bloques con borde 1 px, radio 8 px, `padding:14px 40px 14px 16px`, mono 12–13 px, `line-height:1.5`, botón copiar absoluto arriba-derecha.

**Acciones**
8. Barra de acciones bajo cada mensaje con `opacity:0` y `.msg:hover .actions, .msg:focus-within .actions { opacity:1 }`; `@media (hover:none) { .actions { opacity:1 } }`.
9. En la respuesta del asistente, mostrar la barra solo al terminar el streaming, con entrada `opacity 0→1, translateY(8px→0)`, 300 ms, delay 250 ms.
10. Copiar: swap de icono portapapeles→check + tooltip «Copiado», volviendo al estado inicial a los ~2 s (valor propio; el de ChatGPT no se pudo extraer).

**Composer**
11. Fijo abajo (fuera del contenedor que hace scroll), `bottom: 16–24px`, `border-radius:22px`, fondo `#363636`/superficie secundaria, borde 1 px sutil; en oscuro `box-shadow: inset 0 0 1px #fff3`, en claro sombra difusa `0 2px 8px #0000000a, 0 4px 80px 8px #00000006`.
12. `textarea` autoajustable: `min-height:44px; max-height:min(25dvh, 240px); overflow-y:auto`; recalcular `height = scrollHeight` en cada `input`.
13. Placeholder con `opacity:.5`; cambiar a «Generando…» mientras hay respuesta en curso.
14. Pie con: botón adjuntar (+) a la izquierda; a la derecha selector de herramienta/modelo como texto + chevron sin borde, y botón circular de 32 px que alterna **Enviar ↔ Detener** (cuadrado) durante el streaming.
15. Teclado: `Enter` envía, `Shift+Enter` salto de línea; ignorar `Enter` con `event.isComposing` (IME); opcional ajuste «Ctrl+Enter para enviar».

**Streaming y scroll**
16. Sin cursor: envolver cada palabra nueva en `<span class="stream-word">` con `animation: word-in .7s backwards` y keyframes `0%{opacity:0} 6%{opacity:.18} 12%{opacity:.72} 15%,100%{opacity:1}`; respetar `prefers-reduced-motion`.
17. Indicador «Pensando…/Consultando herramienta» con shimmer de texto: `background: linear-gradient(90deg, var(--text-2) 0%, #fff8 50%, var(--text-2) 100%); background-size:200% 100%; -webkit-background-clip:text; color:transparent; animation: shimmer 2–5s infinite`.
18. Auto-scroll con estado `follow` / `static`: `const STICK_PX = 24`; en `scroll` del usuario, si `scrollHeight - scrollTop - clientHeight > 24` → `static`; mientras `follow`, al llegar cada token hacer `scrollTop = scrollHeight` (instantáneo, no smooth). Al terminar la respuesta → `static`.
19. Botón «Ir al final»: circular 28–32 px, centrado sobre el composer, `opacity` con transición 150 ms; visible solo si distancia > 24 px; al pulsar → salto instantáneo y vuelta a `follow`.
20. Al enviar un mensaje: anclar el mensaje del usuario arriba del viewport añadiendo un espaciador al final (`min-height` = alto del viewport − alto del mensaje) que se va reduciendo a medida que crece la respuesta.
21. Scrollbar fina (`scrollbar-width: thin; scrollbar-color: #474747 transparent`) en el hilo, en el textarea y en la barra lateral.
22. Estado vacío: composer centrado verticalmente con el mismo ancho máximo (patrón de la home de ChatGPT, no observado en vivo).

## Lo que no se pudo observar

- Estilos **calculados** en vivo y DOM real: sin CDP (app lanzada sin `--remote-debugging-port`); todos los valores CSS provienen del bundle empaquetado y de la captura.
- Streaming en ejecución, hover de acciones, feedback «copiado», estado vacío y comportamiento real del scroll: no se interactuó con la app (requisito de solo lectura). Lo descrito proviene del código.
- Duración exacta del estado «copiado» y lógica de `Enter` por defecto (solo se hallaron las opciones del ajuste).
- El contenido del editor del composer en la captura aparece vacío pero con scrollbar en altura máxima; no se determinó si contiene líneas en blanco.
- Para observar en vivo, el usuario podría relanzar la app con `--remote-debugging-port=9223` y repetir con `inspect_electron_page`.
