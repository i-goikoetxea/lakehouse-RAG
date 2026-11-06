# SSD: Arnés determinista del turno de chat

Estado: **aprobado.** Fase 0 (spike) hecha; Fases 1, 2, 3 y 3b implementadas; Fase 4 pendiente.

## 1. Qué cambia respecto a la especificación recibida

| Especificación | Realidad del código | Decisión |
| :--- | :--- | :--- |
| Forzar `tool_choice` de la API de Anthropic | No se usa la API. El agente lanza `claude -p --tools ""` y las herramientas son un protocolo de texto (`claude_agent_runner.rs:144`). `tool_choice` no existe en este camino | Dirigir con `--json-schema`, ya usado en la destilación (`claude_headless_distiller.rs:150`). **Es una directiva, no un forzado duro** (spike de la Fase 0: 8 de 60 ejecuciones sin llamada y con sobre de éxito, `ssd_arnes_fase0_spike.md`). La garantía la da el guard de §3.3. Pasar a la API exigiría una clave y abandonar la sesión de Claude Code |
| Forzar `lookup_double_entry_table` si hay patrón tabular | La tool exige `valor_x` y `valor_y` (`dtos/tools.rs:149`). Con un solo valor, dirigirla empuja al modelo a inventar el otro eje | Tres niveles de intención (§3.1): solo se dirige con tabla y dos ejes con valor |
| Bloquear cifras sobre tolerancias si no se ejecutó el lookup | Contradice el enrutamiento vigente: «Un documento sin Capa 2 se responde desde aquí» (`agent_orchestrator.rs:49`). Bloquearía respuestas correctas de documentos sin destilar | Se exige haber **intentado** la Capa 2, y que cada cifra de la respuesta esté anclada (§3.3) |
| Interceptar la respuesta antes de emitirla | La GUI recibe el texto en streaming (`TextDelta`) antes de que termine el turno | Los turnos con intención detectada no hacen streaming: el texto sale ya validado. El resto sigue igual (§3.4) |
| Reescribir la respuesta inválida | Reescribir texto generado es otra fuente de errores | No se reescribe: un reintento y, si falla, mensaje fijo con los datos de las tools (§3.5) |
| Regex | `regex` no es dependencia del proyecto | Normalización y coincidencia de tokens en Rust estándar, sin dependencia nueva |

**Alcance:** cubre GUI y CLI, que pasan por `AgentOrchestrator`. **No cubre MCP:** un cliente externo decide sus propias llamadas y redacta su propia respuesta.

## 2. Ciclo de vida del turno

```text
Mensaje del operador (GUI / CLI)
        │
        ▼
IntentDetector::clasificar(mensaje) ─► IntencionTurno
        │
        ├─ Ninguna ───────────────► flujo actual, con streaming, sin guard
        │
        ▼
PreInferenceInterceptor::plan(intención) ─► PlanTurno { primer_paso, buffer: true }
        │
        ▼
Bucle del orquestador (máx. 10 pasos)
  paso 1: EleccionHerramienta::Dirigida(tool), solo si la intención es cruzable o de responsable
          (sin `structured_output`, el paso no cuenta como llamada y actúa el guard)
  resto:  EleccionHerramienta::Libre
  cada resultado ─► RegistroTurno (tool, estado, salida; solo del turno actual)
        │
        ▼
FinalMessage ─► PostInferenceGuard::validar(respuesta, intención, registro, pregunta)
        │
        ├─ Valida ─────────────────► Completed(respuesta) + telemetría
        ├─ Violación, sin reintento gastado ─► recordatorio ─► vuelve al bucle
        └─ Violación, reintento gastado ─────► mensaje fijo + telemetría
```

## 3. Componentes

Viven en `src/application/use_cases/arnes_turno/`: son lógica pura, sin I/O ni dependencias nuevas, y se prueban en memoria. Los casos de uso de dominio no cambian.

### 3.1 `IntentDetector`

```rust
pub enum IntencionTurno {
    Ninguna,
    /// Tabla o magnitud sin dos ejes con valor: no se puede cruzar aún
    TabularIncompleta,
    /// Tabla o magnitud y dos ejes con valor: se dirige el lookup
    TabularCruzable { tabla_mencionada: Option<String> },
    /// Pregunta por quién es responsable, firma o ejecuta
    Responsable,
    /// Pide un valor o un código sin ejes que cruzar: se retiene para validar la abstención
    DatoConcreto,
}

pub struct IntentDetector;
impl IntentDetector {
    pub fn clasificar(mensaje: &str) -> IntencionTurno;
}
```

| Mensaje | Intención |
| :--- | :--- |
| `Según la Tabla 4-A, ¿temperatura para 30 mm y CE 0,42 %?` | `TabularCruzable` |
| `¿Qué tolerancia corresponde a 30 mm?` | `TabularIncompleta` |
| `¿Qué es el refuerzo de soldadura?` | `Ninguna`: no hay valor |
| `¿Cuántas tablas hay?` | `Ninguna`: es agregación y va a SQL |
| `¿Quién firma el registro de recepción?` | `Responsable` |
| `¿Qué presión tiene la línea L-999?` | `DatoConcreto` |
| `Dime el código del registro` | `DatoConcreto` |
| `¿Qué es un valor nominal?` | `Ninguna`: no pide el dato |
| `¿Cuál era la temperatura… el 11 de septiembre de 2025?` | `DatoConcreto`: la fecha no es un eje |
| `Según la Rev. 00…, ¿cuál es la temperatura…?` | `DatoConcreto`: la revisión no es un eje |

`DatoConcreto` es la última opción: agregación, responsable y tabular tienen prioridad. Se activa con `qué` + dato, `cuál` + dato en las 4 palabras siguientes, `cuánto`/`cuánta`, o una orden (`dime`, `indica`, `dame`…) + dato en las 3 siguientes. Dato es `valor`, `código`, `plazo`, `límite` y similares, o una magnitud. Una pregunta sin esa forma (`¿Qué dice la norma X sobre la presión?`) sigue siendo `Ninguna`, con streaming y sin guard. Un acierto de más solo cuesta el efecto de escritura.

**Fechas y revisiones no son ejes** (corrección tras la Fase 4): no cuentan como valor de eje una fecha (`11 de septiembre de 2025`, `2025-09-11`, `16/06/2025`, `16-06-2025`) ni el número de una revisión (`Rev. 00`, `Rev.01`, `revisión 3`). Una pregunta histórica por fecha o revisión se trata por sus ejes reales: sin ellos no es tabular. El día de una fecha textual se reconoce por `de` + nombre de mes; `11 de espesor` sigue siendo un valor.

Solo analiza el mensaje del operador, nunca el texto de los documentos. Así un documento malicioso no puede activar el arnés.

### 3.2 `PreInferenceInterceptor` y cambio de puerto

```rust
pub enum EleccionHerramienta {
    Libre,
    /// Se pide la tool con `--json-schema`. Es una directiva: el modelo puede no llamarla
    Dirigida(LakehouseToolName),
}

pub struct PlanTurno {
    pub primer_paso: EleccionHerramienta,
    pub retener_texto: bool,
}

pub struct PreInferenceInterceptor;
impl PreInferenceInterceptor {
    pub fn plan(intencion: &IntencionTurno) -> PlanTurno;
}
```

| Intención | Primer paso | Retener texto |
| :--- | :--- | :--- |
| `Ninguna` | `Libre` | No |
| `TabularIncompleta` | `Libre` | Sí |
| `DatoConcreto` | `Libre` | Sí |
| `TabularCruzable` | `Dirigida(LookupDoubleEntryTable)` | Sí |
| `Responsable` | `Dirigida(QueryOperationalMatrix)` | Sí |

**Cambio estructural en `AgentRunnerPort`:** nuevo método `ejecutar_turno_dirigido(modelo, system_prompt, historial, herramientas, dirigida)`, con implementación por defecto que devuelve error. Recibe también el catálogo porque el spike midió con las 8 tools a la vista: enviar solo la dirigida sería una configuración sin medir. Devuelve `TurnoDirigido::Llamada(ToolCall)` o `TurnoDirigido::SinLlamada`. `ClaudeAgentRunner` lo implementa con `--output-format json --json-schema`, con un esquema que fija `name` a la tool y toma `arguments` del campo `parameters` de `ToolDefinition`. El adaptador no confía en el esquema: un `structured_output` con otro `name` o con `arguments` que no sea un objeto también es `SinLlamada`. Un `is_error: true` o una salida que no sea JSON son errores del proceso.

El turno dirigido solicita la llamada a la tool, pero **no la garantiza**, y el modelo sigue eligiendo los argumentos. El arnés no los extrae por su cuenta. La tool valida la tabla y los ejes, y un error o `sin_coincidencia` vuelve al modelo, que puede seguir en Capa 0.

**Señal de llamada:** la única señal fiable es la presencia de `structured_output`. Ni el código de salida ni `subtype` distinguen los dos desenlaces: sin llamada, el sobre también indica éxito (`exit 0`, `subtype: success`, `is_error: false`). Sin `structured_output`, el turno no cuenta como llamada, `result` no se usa como respuesta y actúa el guard de §3.3 (`CapaDosNoIntentada`), con el reintento único de §3.5.

**Riesgo de ejes intercambiados, resuelto en la salida de la tool y sin garantía sobre la decisión del modelo:** Haiku y Sonnet intercambiaron los ejes en algunas ejecuciones del spike (`ssd_arnes_fase0_spike.md` §4.4). `lookup_double_entry_table` no sabe qué eje es cuál, así que ahora ejecuta el cruce también con los ejes invertidos, sobre las mismas celdas ya filtradas por vigencia o `fecha_referencia` y con el mismo `eje_z`:

| Orden recibido | Orden invertido | Salida | Test (`lookup.rs`) |
| :--- | :--- | :--- | :--- |
| Resuelve | No resuelve, o da las mismas celdas | Sin cambios | `test_solo_el_orden_recibido_resuelve_no_cambia_la_salida`, `test_tabla_simetrica_con_la_misma_celda_no_cambia_la_salida` |
| Resuelve | Resuelve con otras celdas | `estado` y valor del orden recibido, más `ejes_ambiguos`: ambos ejes con unidad y, por cada orden, valores y celdas | `test_ejes_intercambiados_con_rangos_solapados_avisan_de_ejes_ambiguos` |
| No resuelve | Resuelve | `sin_coincidencia`, más `orden_invertido_resolveria` con los valores en ese orden, **sin la celda** | `test_ejes_intercambiados_con_rangos_disjuntos_indican_que_el_invertido_resolveria` |
| No resuelve | No resuelve | Sin cambios | `test_ningun_orden_resuelve_no_anota_nada` |

- `estado` no cambia porque la tool no puede saber qué orden es el bueno, y los consumidores actuales ya leen el valor del orden recibido.
- `orden_invertido_resolveria` no trae la celda para que su valor solo se pueda obtener repitiendo la llamada, que queda registrada y es lo único que ancla esa cifra.
- Dos celdas distintas con el mismo valor también avisan, porque su cita es distinta. El guard no exige aclarar nada en ese caso, ya que el valor mencionado cuenta para los dos órdenes.
- La regla 13 del prompt obliga a decidir por unidades o nombres de eje y, si no lo distinguen, a mostrar ambos valores o preguntar. La descripción de la tool y `MCP_GUIDE.md` lo repiten, porque un cliente MCP no recibe el prompt.

**Comprobación en el guard (`EjesAmbiguosSinAclarar`, §3.3):** se añadió porque no necesita vocabulario propio: compara con los nombres, las unidades y los valores que trae el propio aviso. Un fallo por exceso cuesta un reintento con un recordatorio que dice cómo corregirlo.

**El aviso también sale en cruces correctos:** con extremos abiertos (`x <= 20`, `y > 0,45`), el orden invertido suele resolver aunque el modelo haya acertado. En la tabla 4-A del fixture, 30 mm y CE 0,42 % avisan porque 0,42 cae en `x <= 20` y 30 en `y > 0,45`. Por eso no basta con mirar si los rangos se solapan, y esos turnos deben nombrar los ejes o sus unidades. Falta medir en la Fase 4 con qué frecuencia ocurre y cuántos reintentos provoca.

**Lo que sigue sin garantizarse:**
- Que el modelo asigne bien los ejes: el guard comprueba que el lector ve la asignación, no que sea correcta (`espesor 0,42 y carbono 30` pasa).
- Un turno sin intención detectada no se valida, y en él el texto ya se ha emitido. Allí solo actúan el aviso y la regla 13.
- `execute_athena_sql` no pasa por este contraste.
- Un valor de celda igual a un dato de entrada parece mencionado (`Para 30 son 125` con celdas 125 y 30), así que esa respuesta pasa sin aclarar.
- Las cifras de la celda invertida quedan ancladas por salir en una salida válida: el anclaje no impide elegir esa celda.
- Nombrar el eje con un sinónimo o abreviatura sin su unidad (`grosor`, `CE`) se rechaza una vez: es el coste del reintento.
- Nada de esto se ha probado con Claude real.

### 3.3 `PostInferenceGuard`

```rust
pub struct RegistroTurno { /* tools y salidas del turno actual */ }

pub enum ViolacionArnes {
    CapaDosNoIntentada,
    CifraSinAnclar { cifra: String },
    RolSinRespaldo,
    EjesAmbiguosSinAclarar,
    CifraEnAbstencion { cifra: String },
}

pub struct PostInferenceGuard;
impl PostInferenceGuard {
    pub fn validar(
        respuesta: &str,
        pregunta: &str,
        intencion: &IntencionTurno,
        registro: &RegistroTurno,
    ) -> Result<(), ViolacionArnes>;
}
```

- **Capa 2 intentada:** con intención tabular debe haberse ejecutado `lookup_double_entry_table` o `execute_athena_sql` en este turno; con `Responsable`, `query_operational_matrix` o `execute_athena_sql`. Vale aunque devuelva `sin_coincidencia` o un error de la tool. Como `--json-schema` no garantiza la llamada, este invariante es el que cubre el turno dirigido sin `structured_output`.
- **Abstención sin cifras ajenas (`CifraEnAbstencion`):** en cualquier intención salvo `Ninguna`, si la respuesta contiene una locución de abstención y una cifra que no está en la pregunta ni en una salida válida del turno, se rechaza. La excepción es que el operador pida referencias (`valor de referencia`, `orientativo`, `aproximado`, `valor típico`…). Reutiliza el reintento único y el mensaje fijo de §3.5. Con `DatoConcreto` es la **única** comprobación.
- **Cifra anclada:** toda cifra de la respuesta debe aparecer en una salida de tool válida del turno actual o en la pregunta. Se normaliza `0,42` = `0.42`. El historial **no** cuenta: es justo la pereza que se quiere evitar. Una salida con error no ancla, porque puede repetir los argumentos del modelo. No son cifras los identificadores (`4-A`, `S355`, `5.2.1`), los marcadores de lista, también con Markdown (`**1.**`, `### 2.`), ni `Capa 0`.
- **Qué ancla una fuente:** la pregunta y las salidas se leen con más amplitud que la respuesta (`lecturas_de_fuente`). `125ºC`, `7días` y `30mm²` anclan su número, `Rev02` y `REV-2` anclan el 2, y `5.2.1` ancla `5` y `5.2`. Un token con más de dos tramos de dígitos (`8f3a12c4`) se considera un hash y no ancla nada.
- **Fechas enteras** (corrección tras la Fase 4, módulo `fechas.rs`): en la respuesta, una fecha ISO (`2026-11-03`, también con guiones tipográficos) o textual (`3 de noviembre de 2026`) es **una sola cifra** con lectura `AAAA-MM-DD`, y solo la ancla esa misma fecha. En una fuente, la fecha ancla la fecha entera y también sus partes, así que la pregunta `el 11 de septiembre de 2025` ancla `2025-09-11`, `11` y `2025`.
- **Fecha de hoy:** Claude Code da la fecha al modelo. El orquestador la recibe de `RelojPort` (adaptador `RelojDelSistema`, fecha local) y la fija en el `RegistroTurno`. Ancla solo la fecha entera: con ella, `Hoy (2026-11-03)` pasa y un `11` suelto no. Sin reloj inyectado, la fecha de hoy no ancla.
- **Rol con respaldo:** si la matriz devolvió `sin_responsable_declarado: true`, la respuesta debe decir `no consta` o `no declarado`, o citar entre comillas un fragmento literal (12 caracteres o más) de una salida de tool. La propia salida de la matriz con esa señal no sirve de cita.
- **Ejes aclarados:** si un lookup válido del turno trae `ejes_ambiguos` y la respuesta menciona el valor de un solo orden, debe nombrar ambos ejes con una seña que los distinga (palabra de su nombre o su unidad). Mencionar los dos valores, o ninguno porque pregunta, es válido. Límites en §3.2.

**Límites conocidos:**
- `TabularIncompleta` exige el intento: pedir el eje que falta sin llamar a ninguna tool se rechaza una vez. Es lo que dice este SSD, pero puede ser un falso bloqueo. La Fase 4 lo midió con fechas tomadas por ejes, ya corregido (§3.1).
- Un recuento redactado por el modelo (`hay 3 filas`) sin esa cifra en la salida se rechaza. También se mide en la Fase 4.
- `1.000` se lee como 1 o como 1000, porque la notación es ambigua: puede anclar con cualquiera de los dos.
- Un argumento inventado que la tool repita en una salida válida queda anclado.
- `no consta, pero lo hace el jefe de obra` pasa el invariante de roles: detectar roles exigiría un vocabulario que no existe.
- Una cifra copiada de otra fila de la misma tabla pasa el anclaje: prueba el origen, no la interpretación.
- Los turnos sin intención detectada no se validan.

**Límites del guard de abstención:**
- **No frena lo que más se vio en la línea base.** `No consta; la línea L-204 tiene 14,5 bar` pasa si ese 14,5 salió de una tool del turno, porque tiene la misma forma que una respuesta parcial legítima y la atribución no se puede resolver con reglas. Contra eso solo actúa la regla 14 del prompt. El guard frena las cifras que no salen de ninguna fuente del turno (`No consta; suele ser 7,5 bar`). Por tanto, no se puede prometer que `rechazo_estricto` suba por el guard: depende del prompt y se mide en la Fase 4.
- **Identificadores con `_`:** las locuciones se buscan por palabras, y un identificador con `_` es una sola palabra: `vigencia_no_declarada` no es «no declarada». La excepción es `NO_CONSTA`, que la regla 9 del prompt lee como «no consta» (`IDENTIFICADORES_DE_ABSTENCION`). Los `_` de los extremos se recortan, así que la cursiva Markdown (`_No consta_`) sigue contando.
- **La lista de locuciones es cerrada:** una abstención redactada de otro modo (`queda fuera del corpus`) no se detecta. La lista está en `LOCUCIONES_DE_ABSTENCION`.
- `no aparece` o `no indica` también activan el guard si hablan de otra cosa, y una cifra sin anclar (un año, un recuento) cuesta entonces un reintento.
- Con `DatoConcreto`, una respuesta sin abstención no se ancla: `Son 999 bar` pasa. Ampliar el anclaje a esta intención no forma parte de lo aprobado.
- En un turno tabular o de responsable, pedir referencias no exime del anclaje: la cifra ajena sigue dando `CifraSinAnclar`.
- La excepción mira frases concretas de la pregunta: `dame algo parecido` no cuenta como pedir referencias.

**Límites del anclaje, tras la revisión de formatos:**
- Por leer las fuentes con más amplitud, el número de un identificador ancla: con `S355J2` en una salida, `Son 355` pasa.
- Siguen rechazándose, y cuestan un reintento: una página, una fecha o un dato tomados del historial (la `cita` del lookup no trae página ni fecha) y los recuentos redactados (`los 2 órdenes`).
- Una fecha con día imposible (`31 de febrero`) se trata como fecha. `16/06/2025` en la respuesta son tres cifras, no una fecha. Un identificador nuevo que contenga `no_consta` (`origen_no_consta_aun`) no cuenta como abstención.
- Con la fecha de hoy fijada en la zona del equipo, una respuesta cerca de medianoche que cite la fecha de otra zona se rechaza una vez.
- `1 000` con espacio normal son dos cifras; con espacio duro es 1000. `20 -30` lee `-30`. `125,150` se lee como un solo número. `[1]` es una cifra.
- Se escapan del control en la respuesta, porque se leen como identificador: un sufijo desconocido pegado (`999mm²`, `7días`) y un número de hasta dos dígitos al inicio de línea con `.` o `)` (`30. Es el valor`).

### 3.4 Streaming

Un turno con `retener_texto` llama a `ejecutar_turno` sin observador y emite el texto validado de una vez. Se pierde el efecto de escritura en esos turnos; el botón Detener sigue funcionando, porque cancela el proceso.

### 3.5 Reintento y salida segura

- Un solo reintento por turno, con un recordatorio que nombra la violación.
- El recordatorio y la respuesta rechazada **no** se guardan en el historial de la GUI, para no contaminar los turnos siguientes.
- Si vuelve a fallar: mensaje fijo que indica que la respuesta no pudo validarse, y remite a los resultados del Studio. Nunca se emite la respuesta inválida.
- Nuevo evento `AgentExecutionEvent::ArnesIntervino { etapa, motivo }`, con `etapa` entre `directiva`, `llamada_no_obtenida`, `reintento` y `salida_segura`. Visible en el `--json` (campo `arnes`) y, tras la Fase 3, en el Studio.
- **Telemetría de la cifra rechazada:** el `motivo` lleva el literal rechazado (`«999»`). Se puede leer en el `--json` y en el registro, con `tracing::warn!` en cada intervención. El registro necesita `RUST_LOG=warn`, y la ventana en release no tiene consola, así que hay que redirigir su stderr. La ventana sigue sin recibir el `motivo`: solo le llega el texto de `explicar_intervencion`, que no incluye la cifra.

**Coste:** cada turno dirigido y cada reintento lanzan un proceso `claude` más, con su latencia y consumo. Un turno dirigido además usa unos 3 turnos internos.

## 4. Relación con la regla 12 del prompt

La regla 12 se conserva como primera línea de defensa. El arnés es lo que la hace verificable, y evita tener que demostrar que el modelo obedece.

La regla 14 pide abstenerse sin ofrecer valores de referencia, salvo que el operador los pida. A diferencia de la 12, el guard solo la verifica en parte (§3.3).

## 5. Batería de tests TDD

**Unitarios, en memoria:**
- Clasificación de cada fila de §3.1, más: unidades, `0,42`/`0.42`, ejes categóricos (`clase III`), mensaje vacío, 1 MB de texto, inyección en el mensaje.
- `test_cruzable_dirige_lookup_en_el_primer_paso`
- `test_cruzable_con_tabla_en_historial_sigue_dirigiendo_lookup`
- `test_un_solo_eje_no_dirige_lookup`
- `test_directiva_solo_afecta_al_primer_paso`
- `test_turno_dirigido_sin_structured_output_no_cuenta_como_llamada`
- `test_cifra_presente_solo_en_historial_se_rechaza`
- `test_cifra_de_la_pregunta_o_de_la_tool_se_acepta`
- `test_sin_coincidencia_cuenta_como_capa_dos_intentada`
- `test_documento_sin_capa_dos_responde_desde_capa_cero_tras_el_intento`
- `test_sin_responsable_exige_locucion_de_rechazo`
- `test_reintento_unico_y_luego_mensaje_fijo`
- `test_recordatorio_no_queda_en_el_historial`
- `test_turno_retenido_no_emite_deltas_antes_de_validar`
- `test_turno_sin_intencion_mantiene_streaming`

**Adaptador:** argumentos de `--json-schema` y lectura de `structured_output`, con un binario simulado.

**Adversarial** (`tests/adversarial_arnes_tests.rs`): esquema incumplido por el modelo, tool inexistente, argumentos inválidos, salida de tool que contiene órdenes, cifras con formatos extremos, bucle de reintentos.

**Real** (`#[ignore]`): `p2_cruce_con_la_tabla_ya_en_el_historial_invoca_lookup` y `acceptance_8_problemas`, con Haiku y Sonnet, porque el fallo original se observó con Haiku.

## 6. Plan de fases

| Fase | Contenido | Hecho cuando |
| :--- | :--- | :--- |
| 0 | Spike real de `--json-schema` con esquema de tool, con Haiku y Sonnet | **Hecho:** no es un forzado duro. Se usa directiva + validación + reintento (`ssd_arnes_fase0_spike.md`) |
| 1 | `IntentDetector` y `PostInferenceGuard` puros | Tests unitarios en verde |
| 2 | Puerto, runner, orquestador y evento | Tests de adaptador y adversariales en verde |
| 3 | Streaming y Studio | Comprobado en la ventana |
| 3b | Guard de abstención, `DatoConcreto`, regla 14 y revisión del anclaje | Tests sintéticos en verde; sin probar con Claude real |
| 4 | Suites reales | P2 multi-turno y aceptación en verde, con falsos bloqueos medidos. **Medido** (`ssd_arnes_fase4_suites_reales.md`): P2 multi-turno en verde; aceptación 10/11 con Haiku y 9/11 con Sonnet; tres falsos bloqueos pendientes de corregir |
| 4b | Corrección de los tres falsos bloqueos y nuevo oráculo de P2 | **Medido** (`ssd_arnes_fase4_suites_reales.md` §7): aceptación 11/11 con Haiku y con Sonnet; ciclo de vida 0,852 estricta y 1,000 revisada, sin salidas seguras ni reintentos |
