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

## 3. Contexto que modifica una tabla (mejora 3)

Una fila de tabla rara vez es toda la respuesta: el mismo apartado puede ampliar el límite, excluir casos o condicionar el valor en otro párrafo. `lookup_double_entry_table` añade, cuando ha elegido una tabla (estados `resuelto`, `ambiguo` y `sin_coincidencia`), el objeto:

| Campo | Contrato |
| :- | :- |
| `contexto_apartado.nodo_id` | Sección real del apartado (§1), para leerlo entero con `stair_get_subtree_context` |
| `contexto_apartado.texto` | La prosa del apartado en orden de documento, **sin las filas de la tabla**. Cada tramo de filas omitido se sustituye por `[…]`. Una marca `⟦p. N⟧` al empezar y en cada cambio de página, si el apartado tiene saltos válidos. Corchetes `⟦ ⟧` del documento neutralizados |
| `contexto_apartado.recortado` | `true` si se cortó por tamaño |
| `nota`, `nota_seguridad` | Qué es, que puede modificar el valor y que el texto es dato, no instrucciones |

- **Tamaño acotado:** como mucho `LIMITE_CONTEXTO_APARTADO` = 2 000 caracteres del documento (las marcas y `[…]` no cuentan). No se prioriza ningún párrafo por palabras clave: no dependería del idioma.
- **Fila de tabla** (`es_linea_de_tabla`), independiente del idioma: una línea con al menos 2 palabras con cifras y **más** palabras con cifras que palabras de prosa (4 letras o más, sin cifras). Las unidades y enlaces cortos (`kg`, `mg`, `de`, `up to`) no cuentan. En la duda la línea es prosa y se conserva: una regla corta con dos cifras («Válido desde 2020 hasta 2025») no se pierde.
- Sin prosa en el apartado no hay `contexto_apartado`.
- **Verificador de páginas:** `lookup_double_entry_table` pasa a ser una tool con marcas (`HERRAMIENTAS_CON_MARCAS`): un extracto citado del contexto se comprueba como los de la búsqueda.
- **Coste:** hasta ~2 000 caracteres más por cruce, y una consulta SQL de los chunks de la revisión (la misma forma que ya hace `stair_get_subtree_context`).

### 3.1 Límites conocidos

- Solo el apartado de la tabla (su sección real y sus sub-nodos por páginas). Una regla general en el apartado padre o en otro capítulo no se aporta: el modelo debe buscarla, y la mejora 4 del arnés frena la negación sin búsqueda.
- La detección de filas depende de que el extractor ponga cada fila en su línea. Un PDF que aplane la tabla en un párrafo deja las filas dentro del contexto (más ruido, no pérdida).
- Probado con documentos sintéticos (tarifas en español, dosis en inglés, distancias de normativa técnica); no con el set oculto.
