# Motor FreeLLMAPI: informe de la sesión (2026-11-17)

Rama `claude/motor-freellmapi-4d79f4`, desde `main` `d3986f3` (`v1.0.0`). Sin merge, sin push y sin tag. Contrato: `docs/arquitectura/ssd_motor_freellmapi.md`. No se ha leído el set oculto ni se han tocado los centros.

## 1. Errores y límites (primero)

- **La calidad de respuesta no está medida.** El humo real dio 3/3 correctas, pero con 3 preguntas sintéticas propias y saturadas: solo dice que el circuito funciona. Ningún A/B ni el set v2 respaldan este motor. Los modelos son gratuitos de terceros y cambian según las cuotas.
- **Tope de llamadas del humo superado en 1.** Hubo 10 llamadas del agente, frente a menos de 10. La pregunta abierta («¿Qué es una prueba hidrostática?») encadenó 6 pasos de tools antes de responder. Es un indicio de que el modelo gratuito da más vueltas que Sonnet, pero sale de una sola ejecución.
- **Latencia irregular.** Una pregunta tardó 48 s por un reintento de transporte en el primer paso. Fue una ejecución, con el equipo cargado, así que no se pueden sacar conclusiones.
- **Sin comprobar en real:**
  - **El Nivel 2 (`auto:smart`).** El humo no registra las llamadas de procedencia.
  - **El desplegable en la ventana.** El listado sí se comprobó contra el proxy, con `GET /v1/models?available=true`.
  - **El modal y el guardado real en `keyring`.** La ventana no se lanzó, para no cerrar la app del usuario.
  - **La corrección de omisión con este motor.** No hubo ninguna omisión.
- **La suite completa no se pasó,** por decisión del usuario. Solo los tests dirigidos (§3).
- **TDD no estricto.** Los tests se escribieron antes del código, pero no se vio la fase roja. Hasta tener el SÍ del usuario solo se compiló, y luego se ejecutaron ya en verde.
- **Incidente de proceso.**
  - Un `cargo build --all-targets` enlazó unos 60 binarios a la vez y saturó el disco D: del usuario (100 %).
  - Se paró el `cargo` padre, pero no se pudieron matar los `rustc` y `link` huérfanos: el clasificador lo denegó. Terminaron solos.
  - Desde entonces, solo se compilan targets concretos con `-j 2` y se comprueba con `clippy`, que no enlaza.

## 2. Contra el proxy real (`http://127.0.0.1:31415`, app de escritorio)

| Qué | Resultado |
|---|---|
| `GET /v1/models?available=true` con `anthropic-version` | 200, 36 modelos. Sin `execution_status` ni las variantes `auto:*`, que solo trae el formato OpenAI. |
| Key inválida | 401 |
| Petición completa (`thinking`, `cache_control`, `strict` y beta) | 200: el proxy la acepta y descarta lo que no traduce |
| Segundo paso con `tool_result` y directiva `role: system` | 200, con la cifra exacta |
| Humo del runner real con el perfil FreeLLMAPI | 3/3, 10 llamadas, 78 s, 1 reintento, `stop_reason` y `usage` en todos los pasos, coste desconocido |

Llamadas totales al proxy: 2 sondas a mano y 10 del humo (más las de procedencia que hubiera, que no se registran). No consta que ninguna fallara.

**Solo con dobles (`wiremock`):** el perfil quita los campos, `stop_reason` ausente falla cerrado, el catálogo (orden, ids hostiles, proxy caído y 401), la URL y las keys en el modal.

## 3. Tests ejecutados (dirigidos; la suite completa no se pasó, por decisión del usuario)

- `cargo test --lib -- anthropic_api modo_llm preferencia_modo freellmapi config_llm api_key dtos::agent`: **83 pasan, 0 fallan, 1 ignorado** (el de `keyring` real, que ya existía).
- `--test integration_agente_api`: **12/12**. Incluye los tests del modo API de siempre y el nuevo de FreeLLMAPI.
- `--test adversarial_api_key_gui`: **1/1**, ampliado con la key del proxy.
- `--test gui_config_llm`: **8/8**, con 3 nuevos.
- `cargo clippy --all-targets -- -D warnings`: limpio, así que compilan todos los tests y ejemplos. `cargo fmt`: aplicado.

**El modo API no cambia.** Todo lo nuevo va detrás de `PerfilApi`. Con el perfil Anthropic, la petición, `stop_reason` y el coste son los de `v1.0.0`. Lo cubren tests explícitos, además de los 12 de `integration_agente_api`.

## 4. Decisiones

- **Campos no soportados:** van desactivados de serie, sin reintento. El proxy los acepta hoy, pero no los reenvía (SSD §2).
- **Nivel 2:** FreeLLMAPI con `auto:smart`. Las trazas se comprueban en Rust, así que un modelo flojo solo puede fallar cerrado. Nunca se usa la key de Anthropic (SSD §3.6).
- **`ClaudeModel` admite `:`,** necesario para `auto:smart`.

## 5. Ejecutable

Ejecutable de producción (`cargo tauri build`): `D:\Proyectos\lakehouse RAG\.claude\worktrees\motor-freellmapi-4d79f4	argetelease\lakehouse_rag.exe`, con `pdfium.dll` al lado.

Para probarlo:
1. Abrir el engranaje y elegir FreeLLMAPI.
2. Guardar la key, o tenerla en `FREELLMAPI_API_KEY`.
3. Reiniciar la app.
4. Comprobar que el desplegable muestra `auto` arriba.
