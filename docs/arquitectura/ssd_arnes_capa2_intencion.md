# SSD: el reintento de Capa 2 solo con intención tabular clara

**Fecha:** 2026-11-10. **Rama:** `claude/arreglos-rag-512d66`. **Código:** `post_inference_guard.rs` (`exige_capa_dos`), `intent_detector.rs` (`menciona_tabla`), `busqueda.rs`, `estructura.rs`, `lakehouse_projection.rs`.

## 1. Problema

`CapaDosNoIntentada` («no se consultó la Capa 2») salta en toda pregunta `TabularIncompleta`: una magnitud del vocabulario (temperatura, presión, tensión, velocidad…) y un valor. Muchas preguntas narrativas cumplen eso («¿qué medidas se toman si la temperatura supera 40 °C?»). En un set oculto hubo 6 reintentos así en preguntas que no eran de tabla (dato del informe del set, no visto aquí).

## 2. Regla

Se sigue exigiendo un intento de Capa 2 (`lookup_double_entry_table` o `execute_athena_sql`) como hasta ahora, **salvo** en este caso:

- intención `TabularIncompleta`,
- la pregunta **no nombra una tabla** (`IntentDetector::menciona_tabla`: «tabla» o «tablas» como palabra completa),
- el turno **leyó texto** del documento (`search_normative_context` o `stair_get_subtree_context`), y
- **nada de lo leído está vinculado a una tabla**: ninguna salida de esas tools trae `tablas_vinculadas` con alguna tabla.

| Pregunta | Turno | Antes | Ahora |
| :- | :- | :- | :- |
| Cruzable (dos ejes con valor) | cualquiera sin Capa 2 | reintento | reintento |
| Incompleta que nombra una tabla | cualquiera sin Capa 2 | reintento | reintento |
| Incompleta sin tabla | no leyó texto | reintento | reintento |
| Incompleta sin tabla | leyó texto vinculado a una tabla | reintento | reintento |
| Incompleta sin tabla | leyó texto sin tablas | reintento | **acepta** |
| Responsable | sin matriz ni SQL | reintento | reintento |

Las demás comprobaciones del turno tabular (cifras ancladas, rol, ejes) siguen igual: no exigir la Capa 2 no relaja el anclaje de cifras.

## 3. `tablas_vinculadas` en las tools de texto

La columna ya existía en `02_chunks_normativa` (la ingesta vincula cada tabla al chunk de su sección y a los que la citan). Ahora:

- `FragmentoIndexado` la lleva (la proyección la lee del lote Arrow; vacía en hitos).
- `search_normative_context` añade `tablas_vinculadas` a un resultado **solo si no está vacía**. Así el agente también sabe que ese texto tiene Capa 2 y puede usar el `tabla_id` en el cruce.
- `stair_get_subtree_context` añade `tablas_vinculadas` a una sección, sin repetir, **solo si no está vacía**.

Sin cambio de esquema ni de centros.

## 4. Límites

- El vocabulario de magnitudes y la palabra «tabla» son españoles, como todo el detector.
- Un documento cuya tabla no se destiló a Capa 2 no tiene vínculos: una pregunta incompleta sobre ella, respondida desde el texto, ya no se reintenta. Es el comportamiento buscado (no hay Capa 2 que consultar), pero el lookup tampoco se intentará.
- Medido con tests sintéticos y sin cambios en las 111 respuestas guardadas (ninguna era incompleta sin tabla con reintento). El efecto en los 6 reintentos del set oculto no está medido: lo medirá el set v2.
