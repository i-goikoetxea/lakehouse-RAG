# Línea base del benchmark (Fase 2)

Mide un binario **anterior a la regla 12 y al corte por ANEXO, y sin arnés determinista**. No representa el commit actual de `main`. Su función es servir de punto de comparación antes del arnés.

| Dato | Valor |
| :--- | :--- |
| Binario | `target/release-stair/lakehouse_rag_cli.exe`, compilado el 2026-11-01 |
| sha256 del binario | `38dd01e75b1a1a5b0411ff7a45646b22d058c7fc8d3ce5e6e17d619f41de0ce8` |
| Modelo | `sonnet` |
| Ejecución | 3 repeticiones, semilla 42, 2026-11-02 |
| sha256 de fixtures `ciclo-de-vida` | `a573d99eb04f3a5a6d8fa57c7407074d542bb6d7f62404d1c4760b37c2cc18be` |
| sha256 de fixtures `rgb-industrial` | `ceff254f2f396674e5386c04c6f57177a400eb9be8aabada665ab9c115dd1174` |
| Informe crudo | `benchmarks/resultados/campana/benchmark_report_20261102T181222_582801.*` (ignorado por git) |
| Informe repuntuado | El mismo nombre con el sufijo `_repuntuado` (ignorado por git) |

**Aviso sobre `app_commit`:** en el informe crudo vale `0d10d5e`, que es el commit del checkout del benchmark, no el del binario. El binario se identifica solo por su hash.

## Resultados

Hubo 75 consultas y 27 ingestas, sin ningún fallo de ejecución ni de recuperación por MCP. Todas las ingestas quedaron definitivas (`COMPLETO`).

- **Estricta:** la nota del puntuador automático.
- **Revisada:** la nota tras leer a mano las 22 respuestas dudosas.

| Pista | Métrica | Consultas | Estricta | Revisada |
| :--- | :--- | :--- | :--- | :--- |
| Ciclo de vida | `vigente` | 9 | 1,000 | 1,000 |
| Ciclo de vida | `historico_por_fecha` | 9 | 1,000 | 1,000 |
| Ciclo de vida | `historico_por_revision` | 9 | 0,556 | 1,000 |
| Ciclo de vida | **Precisión global** (media; mín. y máx. por repetición) | 27 | **0,852** (0,778 a 0,889) | **1,000** |
| RGB industrial | `robustez_ruido` | 15 | 0,867 | 1,000 |
| RGB industrial | `rechazo_estricto` | 15 | 0,000 | 0,000 |
| RGB industrial | `rechazo_semantico` | 15 | sin nota: 15 pendientes | 1,000 |
| RGB industrial | `integracion` | 12 | 1,000 | 1,000 |
| RGB industrial | `exactitud_tabular` | 6 | 0,833 | 1,000 |
| RGB industrial | **Precisión global** (igual en las 3 repeticiones) | 48 | **0,625** | **0,688** |

La precisión global usa siempre `rechazo_estricto`. Con `rechazo_semantico` en su lugar, la global revisada de RGB industrial sería 1,000.

Las pistas no se mezclan entre sí y ninguna es comparable con los artículos de RGB o CRUD-RAG.

## Las dos notas de rechazo

Los resultados se repuntuaron a partir de los crudos guardados, sin lanzar la app ni llamar a Claude. Las demás categorías no cambiaron.

- **`rechazo_estricto`:** la regla original del SSD. Vale 0,0 si aparece cualquier cifra ausente de la pregunta.
- **`rechazo_semantico`:** vale 1,0 si se abstiene y no atribuye ningún valor a la entidad inexistente, aunque cite cifras claramente atribuidas a otras entidades.

**La atribución no se puede resolver con reglas fiables**, porque «No consta; la línea L-204 tiene 14,5 bar» y «No consta, pero suele ser 7,5 bar» tienen la misma forma. Por eso las reglas solo deciden dos casos: 1,0 si se abstiene sin cifras nuevas y 0,0 si no se abstiene. Si se abstiene y cita cifras, la respuesta queda pendiente. Mientras haya alguna pendiente, la métrica no da nota.

Las reglas dejaron pendientes las 15 respuestas. Las leí completas y las 15 valen 1,0; esto es **juicio manual, no medición automática**. Cada decisión está en [`revision_rechazo_linea_base.json`](revision_rechazo_linea_base.json), con su justificación y el hash de la respuesta, de modo que no se aplica a otra respuesta distinta.

Una campaña nueva volverá a dejar pendientes las abstenciones con cifras, hasta que alguien las revise.

Para reproducirlo:

```bash
python benchmarks/evaluacion/repuntuar.py --informe benchmarks/resultados/campana/benchmark_report_20261102T181222_582801.json --revision benchmarks/informes/revision_rechazo_linea_base.json
```

## Correcciones de la revisión manual

Se corrigieron 7 respuestas de 0,0 a 1,0. En todas, el error está en el puntuador: un valor prohibido vale 0,0 aunque la respuesta lo atribuya correctamente a otro documento o revisión.

| Consulta | Repeticiones | Por qué es correcta |
| :--- | :--- | :--- |
| `revision-temperatura` | 1 y 2 | Responde 72 °C y Rev. 00, y cita 85 °C como valor de la Rev. 01 vigente |
| `revision-presion` | 2 y 3 | Responde 6,5 bar y Rev. 00, y cita 8,2 bar como valor de la Rev. 01 vigente |
| `ruido-temperatura` | 1 | Responde 38 °C y avisa de que los demás valores son de otras líneas |
| `ruido-espesor` | 3 | Responde 3,2 mm y avisa de que los demás valores son de otras líneas |
| `tabular-esquina` | 2 | Responde 175 °C y descarta de forma explícita los 110 °C de una llamada con los ejes invertidos |

No se cambió el puntuador para esto: distinguir una atribución correcta de una incorrecta requeriría interpretar la frase. Estos casos quedan como dudosos y la nota estricta se conserva en el informe crudo.

## Hallazgos del producto

1. **Rechazo: se abstiene bien, pero añade cifras que nadie pidió.** Las 15 respuestas declaran que el dato no consta y ninguna atribuye un valor a lo inexistente (`rechazo_semantico` 1,000). Aun así, las 15 añaden valores de otros documentos o parámetros «como referencia» (`rechazo_estricto` 0,000). La distancia entre ambas notas mide ese exceso. El cambio de comportamiento del prompt queda para otra sesión, tras la Fase 3 del arnés, y entonces se repetirá la prueba real.
2. **Lista incompleta presentada como completa.** En `rechazo-linea` de la repetición 3, la respuesta afirma que las líneas existentes son L-104, L-204, L-304, L-504 y L-604, y omite L-404, que sí está en el centro. Esa respuesta no consultó el catálogo. Otras tablas también omiten una línea por el límite de resultados: `rechazo-linea` (rep. 2) y `rechazo-norma` (reps. 2 y 3). Solo una de ellas declara la omisión.
3. **Las preguntas por número de revisión funcionan, pero por un camino improvisado.** Las 9 respuestas son correctas. Ninguna herramienta filtra por revisión, así que el agente lo resuelve de dos formas: con SQL sobre `version_num` o con una `fecha_referencia` deducida del catálogo (6 de 9). Cuesta más que preguntar por fecha: 32,6 s frente a 25,9 s de media. El SSD anticipaba aquí un posible fallo, y no se ha producido con estos fixtures.
4. **Tabla de doble entrada con los ejes invertidos.** En `tabular-esquina` de la repetición 2, el agente llamó primero a `lookup_double_entry_table` con los ejes al revés, recibió un valor de otra celda (110 °C), lo detectó y repitió la llamada. Acertó, pero la herramienta no impidió el cruce erróneo, y la corrección dependió del modelo.
5. **Sin página en la cita.** Es lo esperable con Markdown. No se ha medido en PDF.

## Tiempo y coste

| Pista | Ingesta, media por documento | Respuesta, media (mín. a máx.) | Recuperación por MCP, mediana (máx.) |
| :--- | :--- | :--- | :--- |
| Ciclo de vida | 14,6 s | 27,0 s (20 a 36 s) | 95 ms (1.871 ms) |
| RGB industrial | 20,2 s | 33,0 s (21 a 55 s) | 84 ms (3.955 ms) |

- **Latencia de recuperación:** el máximo corresponde a la primera llamada de cada servidor MCP, que carga el modelo de embeddings. Por eso la mediana es más representativa que la media que da el informe crudo.
- **Duración:** unos 47 minutos sumando ingestas y respuestas.
- **Rechazo:** es la categoría más lenta, con 41,3 s de media, porque el agente sigue buscando antes de abstenerse.
- **Coste:** **no se midió en dinero ni en tokens**, porque la app no lo expone y lanza a Claude sin persistir la sesión. Lo único medido es el volumen: 102 ejecuciones de `sonnet` (27 destilaciones y 75 respuestas). Antes de la campaña hubo un piloto de 7 consultas y 9 ingestas, en `benchmarks/resultados/piloto/`.

## Limitaciones

- **Fixtures fáciles:** son Markdown breves, escritos a mano y con códigos inequívocos. Cada pregunta nombra la línea o el procedimiento. Los valores perfectos en ciclo de vida no prueban nada sobre documentos largos, ni sobre PDF.
- **Ciclo de vida acotado:** hay un solo documento y dos revisiones, con fechas declaradas. La Rev. 01 no incluye control de cambios con los valores anteriores. No se prueba una fecha anterior a la primera revisión, ni revisiones sin fecha.
- **Muestra pequeña:** 3 repeticiones y 25 consultas distintas no permiten sacar conclusiones estadísticas. En RGB industrial, los fixtures no varían entre repeticiones.
- **Fuera de alcance:** multi-turno, aguja, RGB público y calidad de los hitos.
- **Revisión manual:** solo se leyeron las 22 dudosas. Las respuestas con 1,0 sin marca de duda no se leyeron una a una: un acierto automático solo garantiza que aparecen los valores esperados y ninguno prohibido.
- **Tabular añadido:** el SSD §4.3 no lista esta categoría, pero el informe de §5 incluye su métrica, así que se añadieron 2 consultas.

## Cliente MCP

El cliente de la Fase 1 funcionó contra el `--mcp-server` real sin cambios. Se comprobaron `initialize`, `tools/list` (9 herramientas), `tools/call` y los errores de herramienta (centro inexistente, argumento ausente, herramienta desconocida). El centro temporal usado estaba vacío, y después la campaña hizo más de 75 llamadas reales sin fallos.

## Anexo (2026-11-03): repuntuación con el puntuador corregido

Lo anterior se conserva tal como se publicó. El puntuador ya distingue un valor prohibido dado como respuesta de uno mencionado como descartado o de otra revisión (SSD §5.1). Además, `rechazo_estricto` ignora los dígitos de identificadores y el recuento «0 resultados» (SSD §5.2).

Repuntuando los crudos guardados de esta línea base, sin llamar a Claude, las 7 respuestas de «Correcciones de la revisión manual» pasan de 0 a 1 por reglas:

| Pista | Métrica | Estricta publicada | Estricta repuntuada |
| :--- | :--- | :--- | :--- |
| Ciclo de vida | `historico_por_revision` | 0,556 | 1,000 |
| Ciclo de vida | **Precisión global** | **0,852** | **1,000** |
| RGB industrial | `robustez_ruido` | 0,867 | 1,000 |
| RGB industrial | `exactitud_tabular` | 0,833 | 1,000 |
| RGB industrial | **Precisión global** | **0,625** | **0,688** |

`rechazo_estricto` sigue en 0,000, porque las 15 respuestas añaden valores de otros documentos. Las demás métricas no cambian. El detalle, junto con la Fase 4 y la Fase 4b del arnés, está en [`repuntuacion_20261103.md`](repuntuacion_20261103.md).
