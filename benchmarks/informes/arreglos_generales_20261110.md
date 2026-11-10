# Cinco arreglos generales del RAG: antes y después (2026-11-10)

Rama `claude/arreglos-rag-512d66` desde `de9cae9`. Un commit por mejora:

| Commit | Mejora |
| :- | :- |
| `8bffe9b` | Herramienta de repetición del arnés sobre respuestas guardadas y línea base |
| `1a5860c` | 1. El verificador de páginas solo avisa con evidencia positiva de error |
| `954f5f6` | 2. Página de origen en `lookup_double_entry_table` y `query_operational_matrix` |
| `fb6b711` | 3. El cruce de tabla trae la prosa de su apartado |
| `cc64ffb` | 4. El arnés frena la ausencia afirmada sin haber buscado |
| `0f54c98` | 5. Reintento de Capa 2 solo con intención tabular clara |
| `932766c` | Ajuste de estilo que pide clippy (sin cambio de conducta) |

Contratos: [`ssd_citas_pagina.md`](../../docs/arquitectura/ssd_citas_pagina.md) §5.3 (mejora 1), [`ssd_capa2_paginas_contexto.md`](../../docs/arquitectura/ssd_capa2_paginas_contexto.md) (2 y 3), [`ssd_arnes_negaciones.md`](../../docs/arquitectura/ssd_arnes_negaciones.md) (4) y [`ssd_arnes_capa2_intencion.md`](../../docs/arquitectura/ssd_arnes_capa2_intencion.md) (5). Sin cambio de esquema: no hay que recrear centros.

## 1. Errores y retrocesos encontrados (lo primero)

### 1.1 En las respuestas de la ejecución real

- **`rechazo_estricto` de RGB industrial baja de 0,533 a 0,333 (de 8/15 a 5/15), y la precisión global de 0,833 a 0,792.** Las 10 respuestas de rechazo con 0 se abstienen bien; la nota estricta cae porque citan cifras que no están en la pregunta:
  - 21 de 48 respuestas citan la fecha de vigor «2025-03-14» (antes, 6). Medido: la `cita` de la búsqueda trae `fecha_vigor` en 42 salidas de tool (antes, 20). Ese campo lo añadió la rama de citas de página (`08809ce`), que ya estaba en la base de esta rama; la referencia de RGB se midió con un binario anterior (`8f2440b7`). Ninguno de los cinco cambios toca `fecha_vigor` (comprobado en el diff).
  - El resto son los parámetros hermanos de la L-104 y los códigos `PTF-HID-20…60`, que ya aparecían en la referencia.
  - **Atribución: supuesta, no medida.** No hay ejecución de RGB con el binario de la base (`de9cae9`) que separe el efecto de la fecha (base), de `tablas_vinculadas` en la búsqueda (mejora 5) y de la variación del modelo con n = 15. Propuesta: 1 ejecución de RGB con el binario de `de9cae9` (≈ 35 min de Claude), o volver a puntuar sin contar una fecha que esté en la cita de la tool.
- **`rechazo_semantico`, la métrica de fondo, sigue en 1,000.** Las reglas dejaron 11 pendientes; las revisé a mano: las 11 se abstienen y ninguna atribuye un valor a la entidad inexistente. Es juicio manual mío, no una revisión versionada con hash.
- **1 reintento en RGB** (antes, 2): el falso bloqueo ya conocido de la referencia («-20» por `CifraEnAbstencion`, lista abreviada de códigos). No lo causa ningún cambio de esta rama y sigue sin corregir.
- Aguja, dos agujas y ciclo de vida: ninguna respuesta incorrecta.

### 1.2 Retrocesos encontrados durante el trabajo (resueltos)

- **Mejora 4, primera versión: 9 de 111 respuestas guardadas (todas aceptadas en su día) habrían pasado a reintento.** Eran afirmaciones sobre el propio resultado: «no hay conflicto de prelación», «el resultado no incluye página», «los valores de otras líneas no aplican a la L-104», «Nota al pie: no hay ninguna asociada». Se bloqueó y se rediseñó (§2.4). La versión final: 0 de 111.
  - **Sesgo:** la versión 2 se diseñó después de ver esos falsos positivos en un benchmark propio. Sus reglas son estructurales (si se leyó texto, vocabulario de las instrucciones de la app, etiqueta antes de dos puntos) y no contienen palabras de esos documentos, pero se ajustaron con ellos. Debe medirla el set v2.
- **Mejora 3:** el primer criterio de «fila de tabla» (proporción de palabras con cifras) dejaba filas con unidades dentro del contexto (`up to 20 kg 5 mg 10 mg`). Se cambió por uno más conservador antes del commit (§2.3).
- **Línea base:** `test_adv_mega_de_fechas_termina_pronto` falló una vez por tiempo (5,7 s) en la suite completa. Aislado pasa 3 de 3 (2,2-2,4 s) y en las 5 regresiones siguientes pasó. Es un test de tiempo sensible a la carga, no una regresión.
- **Compilación:** una regresión falló con «crate required to be available in rlib format» y pasó al repetirla sin cambios (fallo transitorio del `target`, ya visto en otras sesiones).

## 2. Qué cambia (resumen)

1. **Verificador de páginas.** Solo anota si el extracto aparece en un único lugar de lo consultado y ninguna de sus páginas es la citada. Se callan: el rango que contiene la página, el extracto a caballo con una de sus páginas citada, el repetido en varias páginas, el no encontrado (paráfrasis, títulos) y el que no tiene página. Cambio de contrato deliberado: citar el rango del chunk ya no se anota (es impreciso, no erróneo; `pagina_exacta_estricta` lo sigue midiendo).
2. **Página de Capa 2.** La celda trae `paginas` (rango del apartado de su tabla) y `pagina` `⟦p. N⟧` solo si su valor está en una única página de ese apartado (con la etiqueta de fila o columna para desempatar). La fila de la matriz trae las páginas de su chunk de origen. Se deriva de `tablas_vinculadas` y `saltos_pagina` del esquema v6.
3. **Contexto del apartado.** El cruce añade `contexto_apartado`: la prosa del mismo apartado sin las filas de la tabla, con marcas de página, hasta 2 000 caracteres. La fila se detecta sin mirar el idioma.
4. **Negaciones sin búsqueda.** En un turno retenido que no leyó texto del documento, «no hay X» / «X no se aplica» / «there is no X» sobre un X que no aparece en lo consultado ni en el vocabulario de la app provoca el reintento y, si insiste, el mensaje fijo.
5. **Capa 2 solo con intención clara.** Una pregunta incompleta que no nombra una tabla ya no exige la Capa 2 si el turno leyó texto y nada de lo leído está vinculado a una tabla. La búsqueda y el subárbol exponen `tablas_vinculadas` cuando las hay.

## 3. Antes y después

### 3.1 Sin Claude (medido)

| Métrica | Línea base (`de9cae9`) | Final (`932766c`) |
| :- | -: | -: |
| `cargo test --no-fail-fast` | 912 pasan, 1 falla (tiempo, inestable), 34 ignorados | **953 pasan, 0 fallan**, 35 ignorados |
| Tests nuevos | — | 41 (más la herramienta ignorada) |
| `cargo clippy --all-targets -- -D warnings` / `cargo fmt --check` | — | limpio / limpio |
| Python `benchmarks/` | 215 OK | OK (sin cambios en Python) |
| Repuntuación de respuestas guardadas (RGB 0,833 global; ciclo 1,000; Aguja recall y estricta 1,00) | igual | igual (el evaluador no cambió) |
| Repetición por el guard: reintentos sobre 111 respuestas | 0 | **0** |
| Repetición: avisos de página (Aguja, fuente reconstruida) | 49 en 29 respuestas | **0** |
| Avisos reales guardados en dos agujas («ambigua» sobre citas correctas) | 4 | 0 al repetirlas |

Los 49 avisos de la línea base son artefacto de la reconstrucción (títulos y breadcrumbs «no encontrados»), no citas erróneas. RGB y ciclo de vida son Markdown y no citan páginas: no ejercitan el verificador. Tras cada mejora se repitieron la suite completa y la repetición; ninguna empeoró nada (línea base, 915, 925, 933, 948, 953 tests).

### 3.2 Con Claude: una ejecución (medido)

Sonnet, semilla 42, binario release sha256 `f845d65d…e111` (commit `932766c`), CLI aislada con `LAKEHOUSE_RAG_CONFIG_DIR` temporal por repetición. De 14:30 a 16:42 UTC. Carga de CPU del equipo, 128 muestras de 1 min: media 40 %, ≥ 50 % en 34 y ≥ 80 % en 30 (picos de ingesta y otras sesiones en paralelo). Los tiempos no son comparables con seguridad entre ejecuciones.

| Pista | Métrica | Referencia | Ahora |
| :- | :- | -: | -: |
| Aguja 5 × 3 (30 consultas) | recall / `pagina_exacta` / `pagina_exacta_estricta` / `…_modelo` | 1,00 / 1,00 / 1,00 / 1,00 | 1,00 / 1,00 / 1,00 / 1,00 |
| | `seccion_real`, página por MCP | 1,00 | 1,00 |
| | Reintentos / bloqueos / avisos de página | 0 / 0 / 0 | 0 / 0 / 0 |
| | Duración | 77,7 min | 57,9 min |
| Dos agujas (6 consultas) | `valor_y_pagina` / `mismo_chunk` | 1,00 / 1,00 | 1,00 / 1,00 |
| | `pagina_exacta_estricta_modelo` | 0,833 | 1,00 |
| | **Avisos de página (todos «ambigua» sobre citas correctas)** | **4** | **0** |
| RGB industrial × 3 (48) | Precisión global (estricta) | 0,833 | **0,792** |
| | `rechazo_estricto` | 0,533 | **0,333** (§1.1) |
| | `rechazo_semantico` | 1,000 (7 manuales) | 1,000 (11 manuales, mías) |
| | `exactitud_tabular` (estricta) | 0,833 (1,000 revisada) | 1,000 |
| | `robustez_ruido` / `integracion` | 1,000 / 1,000 | 1,000 / 1,000 |
| | Reintentos / salidas seguras / directivas | 2 / 0 / 6 | 1 / 0 / 6 |
| | Cruces con `contexto_apartado` | — | 6 de 6 |
| | Latencia media por pregunta | 33,9 s | 33,2 s |
| Ciclo de vida × 3 (27) | Precisión global | 1,000 (repuntuada) | 1,000 |
| | Reintentos / salidas seguras | 0 / 0 | 0 / 0 |
| | Latencia media por pregunta | 32,2 s | 33,9 s |

Referencias: Aguja y dos agujas, [`aguja_citas_pagina_20261110.md`](aguja_citas_pagina_20261110.md) (binario `4dc641b5…`, misma base de código que esta rama salvo los cinco cambios). RGB, [`rgb_arnes_corregido_20261109.md`](rgb_arnes_corregido_20261109.md) (binario `8f2440b7…`, **anterior a las citas de página**). Ciclo de vida, ejecución de la sesión Arnés 4b (2026-11-03, `ssd_arnes_fase4_suites_reales.md` §7). Evidencia cruda fuera del repo (contiene texto de un documento real en Aguja).

## 4. Qué quedó sin verificar

- **Los cuatro defectos del set oculto.** No se ha leído el set y no se ha medido con él. Que (a) la negación sin búsqueda, (b) las 2 respuestas de tabla sin página, (c) los 18 avisos y (d) los 6 reintentos de Capa 2 desaparezcan es una **suposición** apoyada en tests sintéticos. Lo medirá el set v2.
- **Mejora 4:** solo actúa si el turno no leyó texto. Si en el defecto (a) el turno hizo alguna búsqueda, la regla no lo habría frenado (no consta cómo fue ese turno). La mejora 3 aporta la otra mitad: el contexto del apartado trae la regla general si está en el mismo apartado.
- **Mejora 5:** ninguna respuesta guardada ni de la ejecución real era una pregunta incompleta sin tabla con reintento; el efecto solo está en tests.
- **Mejora 2:** la página exacta de la celda depende de que el extractor ponga cada fila en su línea; con PDF reales de tablas complejas no está medido. RGB y ciclo de vida son Markdown: la página no se ejercitó con Claude.
- **Mejora 1:** el caso «discrepante» con Claude real sigue sin darse (el modelo no citó ninguna página errónea); solo hay tests.
- La causa de la bajada de `rechazo_estricto` (§1.1).
- `acceptance_8_problemas` con Claude real no se ejecutó en esta sesión.

## 5. Sesgo y alcance

Los tests usan documentos sintéticos de tres dominios y dos idiomas (tarifas en español, dosis en inglés, distancias de normativa técnica), ninguno del set oculto. Las respuestas guardadas y la ejecución real son de nuestros benchmarks (un documento militar en inglés, fixtures industriales en español): un 100 % aquí significa «funciona en los casos conocidos», no «funciona en general». Las listas de locuciones de la mejora 4 y el detector de la mejora 5 son de español (y algo de inglés).
