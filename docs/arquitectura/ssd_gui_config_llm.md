# SSD: configuración del motor del LLM y de la API key desde la GUI

**Estado:** implementado en la rama `claude/gui-config-api-c2009b`, desde `main` `4eca889`. Sin merge ni push. Parte de la Fase 5.2 (`ssd_pieza5_fase52_agente_api.md`).

## 1. Qué se añade

- **Botón de engranaje** (`#btn-config`), un SVG monocromo con `currentColor`. Está en la barra superior derecha, después del botón de tema y antes de los controles de ventana. Usa las variables de tema, así que funciona en oscuro y en claro.
- **Modal `#dialogo-config`** con dos secciones:
  1. **Motor del LLM.**
     - El motor *en uso en esta sesión*: el efectivo, tras el respaldo a CLI.
     - Un selector CLI/API con el modo *elegido*.
     - Una línea «Al reiniciar: … (origen)».
     - Los avisos.
  2. **API key.**
     - El estado (configurada o sin key), la máscara `sk-ant-…XXXX` y el origen (variable `ANTHROPIC_API_KEY` o almacén del sistema).
     - Un campo `type="password"` para guardarla o actualizarla.
- **Comandos de Tauri nuevos:** `estado_modo_llm` y `guardar_modo_llm`. `estado_api_key` y `guardar_api_key` ya existían. Los cuatro están en el `invoke_handler`; lo comprueba `tests/gui_config_llm.rs`.

## 2. Persistencia del modo

- **Fichero:** `llm_config.json` en el directorio de configuración de la app (`LAKEHOUSE_RAG_CONFIG_DIR` > `%APPDATA%\lakehouse_rag`). Es el mismo directorio que `centers_registry.json`, pero el registro no se toca.
  - Contenido: solo `{"modo_llm": "cli" | "api"}`.
  - Escritura atómica: temporal + `rename`.
  - Al guardar, el fichero se reescribe entero, así que desaparece cualquier campo ajeno.
- **Prelación al arrancar** (`infrastructure/llm/preferencia_modo.rs::resolver_modo`):
  1. `LAKEHOUSE_RAG_MODO_LLM`, si está definida y no vacía. Manda, y el modal lo dice.
  2. `llm_config.json`.
  3. `Cli`. **Sin configuración, el valor por defecto sigue siendo `Cli`.**
- **Tolerancia:** un fichero ausente, corrupto, con un modo desconocido o de más de 4 KB se ignora con un aviso que no repite su contenido.
- **Alcance:** `bootstrap::composicion_real` resuelve el modo así para la GUI, la CLI y el MCP. Si se elige API en la ventana, también la usan la CLI y el MCP con el mismo directorio de configuración.
  - El ejemplo `ab_agente` no cambia: sustituye el agente y la procedencia por los de `--modo`.
  - Los tests que componen con un directorio temporal siguen en `Cli`.

## 3. Decisión: «se aplica al reiniciar», sin cambio en caliente

El modo **no** se cambia en caliente. Por qué:

1. **La composición es inmutable por diseño.** `LakehouseApp` recibe `agente: Arc<dyn AgentRunnerPort>` y `verificador_procedencia` en `DependenciasLakehouse`.
   - Para cambiarlos en caliente habría que meter un runner conmutable (`RwLock<Arc<…>>`) en el núcleo de la composición, que comparten la GUI, la CLI, el MCP y las suites de aceptación.
   - Sería un cambio estructural que AGENTS.md §1.2 obliga a consultar antes.
2. **El motor no es una pieza aislada.** El modo API activa la corrección de omisión del arnés (`necesita_correccion_de_omision`) y la procedencia por la API.
   - Cambiar solo el runner dejaría una mezcla (agente API con procedencia CLI) que el A/B 121/122 no ha medido.
   - Cambiar los dos sin un turno en curso exige coordinar el `GuiChatService` con la fachada.
   - Es más superficie de error que beneficio para un laboratorio.
3. **Reiniciar es barato y explícito.** El modal distingue el motor **en uso** del **elegido** y del **del próximo arranque**. Además marca «pendiente de reinicio». No hay ambigüedad sobre qué motor contesta.

Si más adelante hace falta en caliente, el sitio es un `AgentRunnerPort` conmutable en `infrastructure/llm`, que se cambia entre turnos con el `GuiChatService` como guardián. Requiere un SSD propio.

**«En uso»** lo calcula la composición, no el entorno: es `Api` solo si se construyó el runner de la API (`ComposicionReal::modo_en_uso`). Si se pidió API sin key, el modal muestra el respaldo a CLI (`respaldoCli`) con un aviso. `estado_api_key.modoLlm` ahora devuelve también el motor en uso; antes devolvía el de la variable.

**«Pendiente de reinicio»** compara el motor en uso con el que de verdad habrá al reiniciar. Por eso «API elegida sin key» no promete un cambio: seguirá en CLI.

## 4. Seguridad de la key

- La key va del campo al comando sin variable intermedia: `invoke('guardar_api_key', { key: campoKey.value })`.
- El campo se vacía en el `finally` de cada intento (éxito o error) y al cerrar el modal.
- Lo que devuelve el backend:
  - el estado, solo con la máscara;
  - errores con texto fijo, que no repiten la entrada.
- El fichero de preferencias solo guarda el modo. El test adversarial cubre estos casos:
  - un fichero envenenado con `api_key`;
  - la key pegada en el selector de modo.

  En ninguno de los dos la key llega al fichero, al DTO ni a los logs.
- **Fuga corregida de paso:** `ModoLlm::desde_valor` registraba en el log el valor desconocido de `LAKEHOUSE_RAG_MODO_LLM`. Una key pegada en la variable equivocada habría llegado al log. Ahora solo registra su longitud.

## 5. Pruebas

- Unitarias:
  - `preferencia_modo`: 6 tests, sobre prelación, ida y vuelta del fichero, corrupción y modo estricto.
  - `config_llm`: 7 tests, sobre por defecto, pendiente de reinicio, sin key, entorno que manda, respaldo, vuelta a CLI y modo hostil.
- Estáticas del frontend (`tests/gui_config_llm.rs`, 5 tests): posición del botón, SVG sin emoji, campo password, la key no se retiene en JS y los comandos están registrados. El frontend no tiene runner de JS; se sigue el patrón de `adversarial_gui_v2_tests.rs`.
- Adversarial (`tests/adversarial_api_key_gui.rs`): se amplía con el flujo del modal y el fichero de configuración.
- **Manual (no automatizable aquí):** el aspecto del modal en oscuro y en claro, y el flujo real con `keyring`. Ver el informe de la sesión.
