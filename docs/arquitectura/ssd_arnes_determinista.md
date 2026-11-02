# SSD: Arnés determinista del turno de chat

Estado: **propuesta, sin implementar.** Requiere aprobación (AGENTS.md §1.2).

## 1. Qué cambia respecto a la especificación recibida

| Especificación | Realidad del código | Decisión |
| :--- | :--- | :--- |
| Forzar `tool_choice` de la API de Anthropic | No se usa la API. El agente lanza `claude -p --tools ""` y las herramientas son un protocolo de texto (`claude_agent_runner.rs:144`). `tool_choice` no existe en este camino | Forzar con `--json-schema`, ya usado en la destilación (`claude_headless_distiller.rs:150`). Se valida con un spike antes de implementar (§6, paso 0). Pasar a la API exigiría una clave y abandonar la sesión de Claude Code |
| Forzar `lookup_double_entry_table` si hay patrón tabular | La tool exige `valor_x` y `valor_y` (`dtos/tools.rs:149`). Con un solo valor, forzarla empuja al modelo a inventar el otro eje | Tres niveles de intención (§3.1): solo se fuerza con tabla y dos ejes con valor |
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
  paso 1: EleccionHerramienta::Obligatoria(tool), solo si la intención es forzable
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
    /// Tabla o magnitud y dos ejes con valor: se fuerza el lookup
    TabularCruzable { tabla_mencionada: Option<String> },
    /// Pregunta por quién es responsable, firma o ejecuta
    Responsable,
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

Solo analiza el mensaje del operador, nunca el texto de los documentos. Así un documento malicioso no puede activar el arnés.

### 3.2 `PreInferenceInterceptor` y cambio de puerto

```rust
pub enum EleccionHerramienta {
    Libre,
    Obligatoria(LakehouseToolName),
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
| `TabularCruzable` | `Obligatoria(LookupDoubleEntryTable)` | Sí |
| `Responsable` | `Obligatoria(QueryOperationalMatrix)` | Sí |

**Cambio estructural en `AgentRunnerPort`:** nuevo método `ejecutar_turno_forzado(modelo, system_prompt, historial, herramienta)`, con implementación por defecto que devuelve error. `ClaudeAgentRunner` lo implementa con `--output-format json --json-schema`, con un esquema que fija `name` a la tool y toma `arguments` de su `input_schema`.

El turno forzado obliga a **llamar** a la tool, pero el modelo sigue eligiendo los argumentos. El arnés no los extrae por su cuenta. La tool valida la tabla y los ejes, y un error o `sin_coincidencia` vuelve al modelo, que puede seguir en Capa 0.

### 3.3 `PostInferenceGuard`

```rust
pub struct RegistroTurno { /* tools y salidas del turno actual */ }

pub enum ViolacionArnes {
    CapaDosNoIntentada,
    CifraSinAnclar { cifra: String },
    RolSinRespaldo,
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

- **Capa 2 intentada:** con intención tabular debe haberse ejecutado `lookup_double_entry_table` o `execute_athena_sql` en este turno. Vale aunque devuelva `sin_coincidencia`.
- **Cifra anclada:** toda cifra de la respuesta debe aparecer en una salida de tool del turno actual o en la pregunta. Se normaliza `0,42` = `0.42`. El historial **no** cuenta: es justo la pereza que se quiere evitar.
- **Rol con respaldo:** si la matriz devolvió `sin_responsable_declarado: true`, la respuesta debe decir `no consta` o `no declarado`, o citar un fragmento literal de una salida de tool.

**Límites conocidos:**
- `no consta, pero lo hace el jefe de obra` pasa el invariante de roles: detectar roles exigiría un vocabulario que no existe.
- Una cifra copiada de otra fila de la misma tabla pasa el anclaje: prueba el origen, no la interpretación.
- Los turnos sin intención detectada no se validan.

### 3.4 Streaming

Un turno con `retener_texto` llama a `ejecutar_turno` sin observador y emite el texto validado de una vez. Se pierde el efecto de escritura en esos turnos; el botón Detener sigue funcionando, porque cancela el proceso.

### 3.5 Reintento y salida segura

- Un solo reintento por turno, con un recordatorio que nombra la violación.
- El recordatorio y la respuesta rechazada **no** se guardan en el historial de la GUI, para no contaminar los turnos siguientes.
- Si vuelve a fallar: mensaje fijo que indica que la respuesta no pudo validarse, y remite a los resultados del Studio. Nunca se emite la respuesta inválida.
- Nuevo evento `AgentExecutionEvent::ArnesIntervino { etapa, motivo }`, visible en el Studio y en el `--json`.

**Coste:** cada turno forzado y cada reintento lanzan un proceso `claude` más, con su latencia y consumo.

## 4. Relación con la regla 12 del prompt

La regla 12 se conserva como primera línea de defensa. El arnés es lo que la hace verificable, y evita tener que demostrar que el modelo obedece.

## 5. Batería de tests TDD

**Unitarios, en memoria:**
- Clasificación de cada fila de §3.1, más: unidades, `0,42`/`0.42`, ejes categóricos (`clase III`), mensaje vacío, 1 MB de texto, inyección en el mensaje.
- `test_cruzable_fuerza_lookup_en_el_primer_paso`
- `test_cruzable_con_tabla_en_historial_sigue_forzando_lookup`
- `test_un_solo_eje_no_fuerza_lookup`
- `test_forzado_solo_afecta_al_primer_paso`
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
| 0 | Spike real de `--json-schema` con esquema de tool, con Haiku y Sonnet | Se sabe si fuerza la llamada. Si no, se usa directiva + validación + reintento, y se documenta que no es un forzado duro |
| 1 | `IntentDetector` y `PostInferenceGuard` puros | Tests unitarios en verde |
| 2 | Puerto, runner, orquestador y evento | Tests de adaptador y adversariales en verde |
| 3 | Streaming y Studio | Comprobado en la ventana |
| 4 | Suites reales | P2 multi-turno y aceptación en verde, con falsos bloqueos medidos |
