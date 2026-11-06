# Spike de la Fase 0 del arnés: ¿`--json-schema` fuerza una llamada a tool?

Fecha: 2026-11-02. CLI: Claude Code 2.1.302. Modelos: los alias `haiku` y `sonnet` resolvieron a `claude-haiku-5-5` y `claude-sonnet-5-5`.

## 1. Veredicto

**`--json-schema` no es un forzado duro.** El camino viable es **directiva + validación + reintento**, con `--json-schema` como directiva. Esto no garantiza la llamada: la garantía la da la validación del arnés.

- En 8 de 60 ejecuciones con esquema, el modelo terminó sin llamar a la tool. El sobre indicó éxito en todas ellas: `exit 0`, `subtype: success`, `is_error: false`.
- Las 8 fueron con Haiku y en casos no cruzables, los que el arnés no debe forzar. Con Sonnet no hubo ninguna, pero en esos casos la llamada llegó con argumentos inventados o vacíos.
- En los casos cruzables hubo 40 llamadas válidas de 40, con los valores de los ejes correctos. Con esa muestra, el límite superior aproximado de fallo es del 7 % (95 %), así que no se puede descartar un fallo también aquí.

**El spike no reprodujo la pereza.** Sin esquema, el modelo también llamó a `lookup_double_entry_table` en 20 de 20 ejecuciones, incluso con la tabla en el historial y con la orden expresa de no usar herramientas. Por tanto, aquí no queda demostrado que el esquema aporte algo en los casos cruzables. La pereza sigue pendiente de medir en la suite real de la Fase 4.

## 2. Método

El script lee del código, sin modificarlo:
- el system prompt real (`build_system_prompt`, con la regla 12);
- las 8 definiciones de tools (`lakehouse_tools()`).

Reproduce el formato de `construir_entrada_conversacion`. La tabla es sintética. El historial incluye un turno previo en el que una tool devolvió la tabla y el asistente la mostró al operador.

Cada ejecución es equivalente a:

```bash
claude -p --output-format json --model <haiku|sonnet> --system-prompt "<prompt>" --tools "" --no-session-persistence --strict-mcp-config --json-schema "<esquema>" < entrada.txt
```

El control es el mismo comando sin `--json-schema`. El esquema es:

```json
{
  "type": "object",
  "properties": {
    "name": { "const": "lookup_double_entry_table" },
    "arguments": "<campo parameters de ToolDefinition>"
  },
  "required": ["name", "arguments"],
  "additionalProperties": false
}
```

Campaña: 5 repeticiones por caso y modelo, con 3 procesos en paralelo, desde un directorio vacío. En total, 80 ejecuciones: 60 con esquema y 20 de control. Hubo además una ejecución de humo por separado, citada en §4.

## 3. Resultados

«Llamada» significa que llegó `structured_output` y que cumple el esquema y los campos que admite la tool. Esta tabla no juzga si los argumentos tienen sentido; eso se indica en la última columna.

| Caso | Pregunta | Modelo | Llamada | Argumentos |
| :--- | :--- | :--- | :--- | :--- |
| `dos_ejes_con_historial` | Tabla 4-A, 30 mm y CE 0,42 %, con la tabla en el historial | Haiku | 5/5 | Correctos; 1 con los ejes intercambiados |
| | | Sonnet | 5/5 | Correctos |
| `dos_ejes_sin_historial` | La misma, sin historial | Haiku | 5/5 | Correctos |
| | | Sonnet | 5/5 | Correctos |
| `tabla_inexistente` | Tabla 9-Z, con la 4-A en el historial | Haiku | 5/5 | Conserva `9-Z`; no la sustituye por `4-A` |
| | | Sonnet | 5/5 | Conserva `9-Z`; no la sustituye por `4-A` |
| `orden_de_no_usar_tools` | Pide leer la tabla y no usar herramientas | Haiku | 5/5 | Correctos; 2 con los ejes intercambiados |
| | | Sonnet | 5/5 | Correctos |
| `un_solo_eje` | Tabla 4-A y 30 mm, sin CE | Haiku | **2/5** | Inventa `"CE"` y `"CE no especificado"`; las otras 3 piden el dato en texto |
| | | Sonnet | 5/5 | **Inválidos en 5/5:** `""` (3), `0.3`, `"CE <= 0,35"` |
| `pregunta_ajena` | ¿Qué es el refuerzo de soldadura? | Haiku | **0/5** | Sin llamada: 4 en texto, 1 con `tool_calls` a otra tool dentro de `result` |
| | | Sonnet | 5/5 | **Sin sentido en 5/5:** `valor_x` y `valor_y` salen de las palabras de la pregunta |

Controles sin esquema:

| Control | Haiku | Sonnet |
| :--- | :--- | :--- |
| La pregunta de `dos_ejes_con_historial` | 5/5 llama a lookup | 5/5 llama a lookup; 3 con los ejes intercambiados |
| La pregunta de `orden_de_no_usar_tools` | 5/5 llama a lookup | 5/5 llama a lookup; 1 con los ejes intercambiados |

### Dónde llegan los datos

| Desenlace | `structured_output` | `result` | `stop_reason` | `exit` / `subtype` / `is_error` |
| :--- | :--- | :--- | :--- | :--- |
| Con llamada (52/60) | Objeto con la llamada | El mismo JSON como texto (52/52) | `tool_use` | `0` / `success` / `false` |
| Sin llamada (8/60) | La clave no existe | Texto libre o JSON de otra tool | `end_turn` | `0` / `success` / `false` |

**La única señal fiable es la presencia de `structured_output`.** Ni el código de salida ni `subtype` distinguen los dos desenlaces.

### Latencia y coste

Medianas por ejecución. Con esquema se incluyen los 4 casos cruzables (n = 20 por modelo). El control es `control_sin_esquema` (n = 5 por modelo).

| Modelo | Modo | Pared (s) | API (ms) | Turnos internos | Coste (USD) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Haiku | Normal | 8,7 | 3027 | 1 | 0,0005 |
| Haiku | Con esquema | 8,7 (6,0–16,2) | 5133 | 3 | 0,0010 |
| Sonnet | Normal | 8,5 | 2898 | 1 | 0,0052 |
| Sonnet | Con esquema | 10,3 (6,3–18,5) | 5289 | 3 | 0,0138 |

El turno con esquema tarda entre 0 y 2 s más en tiempo de pared, y casi el doble en tiempo de API. Cuesta entre 2 y 2,7 veces más. El esquema usa de 2 a 5 turnos internos (`num_turns`), frente a 1 sin él. Estas cifras son orientativas: el control tiene pocas muestras, se ejecutó en paralelo y gran parte del tiempo de pared es el arranque del proceso.

## 4. Hallazgos para las fases siguientes

1. **Falta de llamada:** es un resultado normal del comando, no un error del proceso. No debe usarse `result` como respuesta en un turno forzado, porque puede traer una negativa o `tool_calls` de otra tool.
2. **El detector es lo que protege:** en los casos no cruzables, el esquema empujó a inventar argumentos, sobre todo a Sonnet. Un falso positivo de `IntentDetector` produce una llamada inválida con aspecto de válida. Esto respalda la decisión de §1 del SSD de forzar solo con dos ejes con valor. Queda por comprobar qué hace la tool con `""` o con ejes sin sentido.
3. **Tabla omitida:** el esquema real no exige `tabla_id` ni `busqueda_tabla`. En el humo, Haiku los omitió en una llamada que cumplía el esquema. No se repitió en las 40 de la campaña. La tool respondería `Indica tabla_id o busqueda_tabla` (`lookup.rs:270`), a costa de un paso más.
4. **Ejes intercambiados:** ocurrió en 3 de 20 llamadas forzadas de Haiku y en 4 de 10 controles de Sonnet, así que no lo causa el esquema. La entrada del spike no declaraba qué eje es X, lo que favorece el fallo. No encontré en `lakehouse_tool_service` ningún tratamiento del intercambio. Si no lo hay, en una tabla como la del spike se podría obtener una celda equivocada sin aviso (no se ejecutó la tool). Es un riesgo aparte del forzado, y el anclaje de cifras de §3.3 no lo detectaría.

## 5. Impacto en §3.2 del SSD

**Sí cambia §3.2.** No se ha editado el SSD; estos son los cambios propuestos:

- **Corregir la afirmación:** «El turno forzado obliga a **llamar** a la tool» es falsa. Debe decir que lo solicita con fuerza, y que el arnés verifica si ocurrió.
- **Contrato de `ejecutar_turno_forzado`:** el adaptador lee solo `structured_output`. Si falta, devuelve un resultado explícito de «llamada no obtenida», distinto de un error de proceso y de un `FinalMessage`. También valida `name` y `arguments`, sin confiar en el esquema.
- **Reintento:** definir qué ocurre tras «llamada no obtenida». Propuesta: usar el reintento único de §3.5 y, si vuelve a fallar, la salida segura. `CapaDosNoIntentada` de §3.3 ya cubre un turno que termine sin lookup.
- **Nombre del campo:** el esquema sale de `ToolDefinition.parameters`; `input_schema` no existe en el código.
- **Exigir la tabla (opcional y sin probar):** añadir al esquema forzado la exigencia de `tabla_id` o `busqueda_tabla`, por ejemplo con `anyOf`. Hay que probar que la CLI lo acepta.
- **Coste:** la nota de §3.5 («un proceso `claude` más») se queda corta. Un turno forzado también usa unos 3 turnos internos.

La arquitectura general se mantiene: el guard de §3.3 pasa de ser una segunda defensa a ser la garantía real.

## 6. Límites

- **Muestra pequeña:** 5 repeticiones por caso y modelo. Las proporciones son indicativas, no tasas.
- **Modelos:** solo se probaron las versiones 5.5 actuales. El fallo original se observó con Haiku, y este spike no determina con qué versión.
- **Entrada sintética:** un solo historial corto, en un centro inexistente. No se ejecutó ninguna tool ni el bucle completo, solo el primer paso.
- **CLI:** el comportamiento pertenece a esta versión y puede cambiar; conviene cubrirlo con el test real de la Fase 4.
