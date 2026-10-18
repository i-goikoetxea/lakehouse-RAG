# Análisis UI/UX de la ventana (tras la prueba del usuario, Fase 14)

> **Estado:** aprobado (SÍ a D1) y ejecutado en la Sesión 14b. Resultado y capturas `11`–`18` en la bitácora de `PROGRESS.md`.

> Fuentes: capturas del usuario (tema claro, 1920×1032) y mías (`01`–`10`), lectura de `index.html`, `styles.css`, `app.js`, y el corpus `tests/render_markdown_gui_tests.rs` (13 casos: **8 fallan** con el renderizador actual).

## 1. Diagnóstico por severidad

### Críticos (rompen la lectura o la confianza)

| # | Problema | Causa raíz | Corrección propuesta |
|---|---|---|---|
| C1 | «Fuentes:» se parte letra a letra en columnas (`PO-` / `ALM-` / `03` …) | Regla heredada de la Fase 7: `.citation-badge { width:16px; height:16px; vertical-align:super }` (formato superíndice) pisa las reglas nuevas | Rediseñar las citas como **chips** de ancho automático; eliminar la regla antigua |
| C2 | La misma fuente se repite 10 veces (`PO-ALM-03 · Rev. 04` ×10) | `extraer_citas` deduplica por (doc, revisión, apartado, **fragmento**); cada arista del grafo es un fragmento distinto | Agrupar en el backend por **documento + revisión** → un chip por fuente con contador y lista de apartados/fragmentos al desplegar |
| C3 | Markdown mal pintado: `*cursiva*` literal, `> cita` literal, listas `1.` y anidadas como texto plano, bloques ```` ``` ```` rotos, sin párrafos | Renderizador propio línea a línea (solo negrita, código, `##`, `-`, tablas) | Renderizador CommonMark+GFM real en Rust (ver §3, decisión D1), manteniendo el escape de todo HTML |
| C4 | Las tablas del Studio rompen palabras a mitad (`REC/EPCI/ON`, `PO-/ALM/-03`) | Columna derecha fija de 340 px + `overflow-wrap:anywhere` en celdas | Paneles redimensionables (§2) + vista de **ficha** (tarjeta por fila) cuando el panel es estrecho y tabla cuando es ancho |

### Altos (flujo de información poco profesional)

| # | Problema | Corrección |
|---|---|---|
| A1 | Paneles de ancho fijo (310 / flexible / 340 px) | Separadores arrastrables entre paneles, mínimos y máximos, doble clic = restablecer, anchos recordados; botones para plegar Fuentes y Studio |
| A2 | Barras de scroll del sistema (gruesas, con flechas) y varias barras anidadas | Barras finas (6–8 px) y discretas que solo se ven al pasar el ratón, iguales en los 3 paneles y en ambos temas |
| A3 | Scroll vertical **dentro** de los mensajes (JSON de las tools con `max-height:260px`) | Ningún scroll Y dentro del chat: solo se desplaza la conversación. El detalle JSON de una tool se abre en el Studio (pestaña «Detalle») en lugar de en la burbuja |
| A4 | Al llegar una respuesta larga, el chat salta al **final** y hay que subir a buscar el principio | Desplazar hasta el **inicio** de la respuesta nueva; botón flotante «↓ Ir al final» si el usuario no está abajo |
| A5 | Líneas de 1.100 px en el chat (cansan la lectura) | Columna de lectura centrada de ~760 px (≈ 80–90 caracteres), las tablas pueden usar todo el ancho |
| A6 | Avisos del sistema (centro creado, modelo cambiado, ingesta) mezclados con la conversación y la bienvenida fija arriba | Avisos efímeros como *toasts* + registro de actividad en el panel 1; la bienvenida solo se muestra con el chat vacío (estado vacío con ejemplos clicables) |
| A7 | Tool cards sin resumen: solo nombre y duración | Línea de resumen legible por tool (p. ej. «matriz · 1 fila · CUSTODIA», «lookup · resuelto · 125 °C (*)», «grafo · 11 nodos») y estado con icono |
| A8 | El Studio cambia de pestaña solo en cada respuesta (roba el foco) | Marcar la pestaña con novedad (punto) y solo cambiar si el usuario no ha elegido otra |

### Medios / pulido

| # | Problema | Corrección |
|---|---|---|
| M1 | «1 vigentes · 2 rev.» (concordancia) y la píldora se parte en dos líneas | Pluralización correcta y texto compacto («1 vigente · 2 revisiones» en tooltip) |
| M2 | El estado de ingesta («…rev04.md: ingerido») queda pegado al título del catálogo para siempre | Barra de progreso con texto dentro de la zona de drop; desaparece a los pocos segundos al terminar |
| M3 | Espera de 20–40 s solo con «Consultando…» | Cronómetro de segundos y el nombre de la tool en curso si llega; botón de copiar respuesta |
| M4 | El grafo muestra «prelación ¿?» en casi todos los nodos | Ocultar lo desconocido; agrupar nodos por tipo de relación |
| M5 | Las notas al pie y condiciones repiten lo que ya dice el texto | Mantenerlas (son la garantía de que no se pierden), pero compactas y con el documento como chip |
| M6 | No hay «Eliminar centro» en la UI | Menú del centro con eliminar (con confirmación; no borra disco) |
| M7 | El nombre del modelo perdió la descripción | Descripción en tooltip y en el menú desplegable propio |

## 2. Propuesta de diseño (sin cambiar la arquitectura)

- **Layout:** `grid` con variables CSS `--ancho-fuentes` y `--ancho-studio` controladas por dos separadores (`pointer events`, límites 220–520 px y 280–720 px, persistidos en `localStorage`, accesibles con teclado ←/→). Plegar un panel lo reduce a una barra de iconos de 44 px.
- **Chat:** columna centrada de lectura; mensajes del operador alineados a la derecha con ancho máximo; respuesta del asistente sin burbuja pesada (estilo documento) para que tablas y listas respiren; chips de fuente al pie, agrupados.
- **Citas:** chip `PO-ALM-03 · Rev. 04 · 3` (3 = nº de apartados). Clic → panel Studio «Fuentes» con la lista de apartados, fragmentos, condición y nota. Mismo estilo en tema claro y oscuro.
- **Scroll:** `scrollbar-width: thin` + `::-webkit-scrollbar` (WebView2 es Chromium) con pulgar semitransparente que se intensifica al pasar el ratón.
- **Studio:** pestañas «Fuentes», «Matriz / Tabla», «Grafo», «Detalle de tools»; fichas en estrecho, tabla en ancho (`container queries`).

## 3. Decisiones que necesitan aprobación (AGENTS.md §1.2)

- **D1 — Renderizador Markdown:** añadir la dependencia **`pulldown-cmark`** (Rust puro, estándar de facto, CommonMark + tablas GFM). El HTML crudo del modelo se convierte en texto escapado, los enlaces **no navegan** la ventana (se muestran como texto con la URL en el título; solo `http/https`) y las imágenes se reducen a su texto alternativo. Alternativa sin dependencia: ampliar el renderizador propio (más código y más casos sin cubrir).
- **D2 — Interpretación de «el chat no debe tener scroll Y»:** entiendo que no debe haber barras verticales **dentro** de los mensajes ni scroll de página; solo se desplaza la lista de mensajes, con barra discreta.

## 4. Plan de ejecución (TDD)

1. Rojo ya escrito: `tests/render_markdown_gui_tests.rs` (8 casos fallan). Verde con D1; ampliar el corpus con respuestas reales guardadas.
2. Backend: agrupar citas por documento + revisión (test unitario de `extraer_citas`) y resumen por tool en `ToolCardDto`.
3. Frontend: layout redimensionable, scroll, chat (columna, autoscroll, toasts, estado vacío), chips, Studio con fichas y pestaña Detalle.
4. Verificación real en la ventana con un **banco de render** (página de prueba con el corpus) y capturas en claro y oscuro a 1280×720 y 1920×1080; suite completa, clippy y fmt.
