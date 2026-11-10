# SSD: página y contexto del apartado en las salidas de Capa 2

**Fecha:** 2026-11-10. **Rama:** `claude/arreglos-rag-512d66`. **Sin cambio de esquema:** se deriva de las columnas de `02_chunks_normativa` del esquema v6 (`tablas_vinculadas`, `saltos_pagina`, `nodo_id`). No hay que recrear centros.

Informe de medición: [`benchmarks/informes/arreglos_generales_20261110.md`](../../benchmarks/informes/arreglos_generales_20261110.md).

## 1. Apartado de una tabla (`apartado_tabla.rs`)

1. Se leen los chunks de la revisión de la tabla (`doc_id`, `version_num` de `04`).
2. Secciones candidatas: la **sección real** (`seccion_real_de`: un sub-nodo por páginas `.PAGn-m` cuenta como su sección) de cada chunk cuyo `tablas_vinculadas` contiene el `tabla_id`. La ingesta vincula la tabla a su sección y también a los chunks que la citan («Tabla 4-A»), así que puede haber varias.
3. Cada candidata se reconstruye entera (`reconstruir_con_saltos`, chunks en orden natural de `chunk_id`), con sus saltos de página.
4. Se elige la que contiene **más valores distintos** de las celdas de la tabla (`valor_resultado_texto` como palabras consecutivas completas, sin tildes ni mayúsculas, en alguna línea). Empate: la primera. Si ninguna contiene ningún valor y hay más de una candidata, no hay apartado: no se adivina.

## 2. Página de la celda y de la fila (mejora 2)

### 2.1 `lookup_double_entry_table`

En la `cita` de cada celda devuelta (resuelta, candidatas de un cruce ambiguo y celdas de `ejes_ambiguos`):

| Campo | Contrato |
| :- | :- |
| `pagina_inicio`, `pagina_fin`, `paginas` | Primera y última página del apartado de la tabla (como en las demás tools). Solo si el apartado tiene saltos válidos |
| `pagina` | `⟦p. N⟧`, el mismo formato que las marcas. **Solo si** todas las líneas del apartado que contienen el valor de la celda están en la página N. Si alguna de esas líneas lleva además una etiqueta categórica de la celda (`eje_x_categoria`, `eje_y_categoria`), solo cuentan esas: así se separan dos filas con el mismo valor en páginas distintas |

Sin página única (valor repetido en varias páginas sin etiqueta que las separe, valor ausente del texto, Markdown, saltos inválidos) no hay `pagina`: mejor sin página que con una supuesta.

### 2.2 `query_operational_matrix`

La consulta ya unía el hito con su chunk de origen. La `cita` de cada fila añade `pagina_inicio`, `pagina_fin` y `paginas` de ese chunk, y `pagina` (`⟦p. N⟧`) solo si el chunk está entero en una página: el hito es una destilación y no se puede situar dentro del chunk.

### 2.3 Efecto en el arnés

Los números de página de estas salidas pasan a anclar cifras en el guard, como ya ocurría con las páginas de `search_normative_context`. Es un debilitamiento menor y conocido de `CifraSinAnclar`: una cifra pequeña que coincida con un número de página queda anclada.
