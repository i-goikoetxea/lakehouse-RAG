# Pista Aguja tras las citas de página exacta (2026-11-10)

Comparación antes y después de las fases 1 y 2 de [`ssd_citas_pagina.md`](../../docs/arquitectura/ssd_citas_pagina.md): saltos de página por chunk y marcas `⟦p. N⟧` en la salida de las tools (fase 1), y verificación de la página de los extractos literales (fase 2).

| | Antes | Después |
| :- | :- | :- |
| Informe | [`aguja_campana_20261109.md`](aguja_campana_20261109.md) | este |
| Binario release (sha256) | `8f2440b7…26a9` (`main`, esquema v5) | `4dc641b5…49ef` (rama `claude/citas-pagina-209ed5`, esquema v6) |
| Commit de la app | `0f7c2ae` | `dd65d2e` |
| Modelo, semilla y repeticiones | Sonnet, 42, 5 × 3 | Sonnet, 42, 5 × 3 |
| Duración | 61,4 min | 77,67 min |
| Consultas | 30, ninguna fallida | 30, 0 fallidas |
| Campaña detenida | no | no |

La base es el mismo PDF de `manifiesto_datasets.json` (sha256 `ba0fa533…`, 106 páginas), con las mismas agujas: la semilla y el generador no cambian. Cada caso usa un centro nuevo en un directorio temporal, con `LAKEHOUSE_RAG_CONFIG_DIR` propio.

## 1. Campaña 5 × 3: antes y después (medido)

Media por profundidad sobre 6 consultas (3 ingestas para `ingesta_s`). «Antes» sale del informe del 2026-11-09.

| Prof. | Métrica | Antes | Después | n | Mín. | Máx. |
| -: | :- | -: | -: | -: | -: | -: |
| 0,10 | recall_aguja | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,10 | pagina_exacta | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,10 | pagina_exacta_estricta | 0,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,10 | pagina_exacta_estricta_modelo | no existía | 1,00 | 6 | 1,00 | 1,00 |
| 0,10 | seccion_real | n/d | n/d | 0 | n/d | n/d |
| 0,10 | pagina_mcp | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,10 | latencia_respuesta_s | 30,25 | 27,64 | 6 | 25,02 | 33,01 |
| 0,10 | ingesta_s | 168,87 | 231,26 | 3 | 167,15 | 352,67 |
| 0,28 | recall_aguja | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,28 | pagina_exacta | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,28 | pagina_exacta_estricta | 0,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,28 | pagina_exacta_estricta_modelo | no existía | 1,00 | 6 | 1,00 | 1,00 |
| 0,28 | seccion_real | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,28 | pagina_mcp | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,28 | latencia_respuesta_s | 29,08 | 29,14 | 6 | 25,06 | 33,36 |
| 0,28 | ingesta_s | 171,08 | 290,32 | 3 | 165,01 | 393,84 |
| 0,50 | recall_aguja | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,50 | pagina_exacta | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,50 | pagina_exacta_estricta | 0,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,50 | pagina_exacta_estricta_modelo | no existía | 1,00 | 6 | 1,00 | 1,00 |
| 0,50 | seccion_real | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,50 | pagina_mcp | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,50 | latencia_respuesta_s | 29,78 | 27,51 | 6 | 24,48 | 34,60 |
| 0,50 | ingesta_s | 170,81 | 225,46 | 3 | 179,44 | 311,73 |
| 0,72 | recall_aguja | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,72 | pagina_exacta | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,72 | pagina_exacta_estricta | 0,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,72 | pagina_exacta_estricta_modelo | no existía | 1,00 | 6 | 1,00 | 1,00 |
| 0,72 | seccion_real | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,72 | pagina_mcp | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,72 | latencia_respuesta_s | 29,02 | 28,72 | 6 | 26,81 | 33,69 |
| 0,72 | ingesta_s | 229,91 | 238,88 | 3 | 186,35 | 340,37 |
| 0,90 | recall_aguja | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,90 | pagina_exacta | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,90 | pagina_exacta_estricta | 0,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,90 | pagina_exacta_estricta_modelo | no existía | 1,00 | 6 | 1,00 | 1,00 |
| 0,90 | seccion_real | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,90 | pagina_mcp | 1,00 | 1,00 | 6 | 1,00 | 1,00 |
| 0,90 | latencia_respuesta_s | 30,22 | 26,96 | 6 | 23,18 | 33,57 |
| 0,90 | ingesta_s | 174,63 | 264,40 | 3 | 163,29 | 385,04 |

**Global (30 consultas):**

| Métrica | Antes | Después |
| :- | -: | -: |
| recall_aguja | 1,00 | 1,00 |
| pagina_exacta | 1,00 | 1,00 |
| pagina_exacta_estricta | 0,00 | 1,00 |
| pagina_exacta_estricta_modelo | no existía | 1,00 |
| pagina_tool_agente | 1,00 | 1,00 |
| seccion_real | 1,00 | 1,00 |
| pagina_mcp | 1,00 | 1,00 |
| seccion_mcp | 1,00 | 1,00 |
| recall_lexica | 1,00 | 1,00 |
| recall_semantica | 1,00 | 1,00 |
| latencia_respuesta_s (media) | 29,67 | 27,99 |
| ingesta_s (media) | 183,06 | 250,06 |

- `pagina_exacta_estricta`: 1,0 solo si la respuesta cita **sola** la página de la aguja; un rango no cuenta.
- `pagina_exacta_estricta_modelo`: la misma métrica sobre el texto del modelo, sin el bloque «Verificación automática de páginas» que añade la app. Separa el efecto de las marcas (fase 1) del de la corrección (fase 2).

### Páginas que cita cada respuesta

| Rep. | Prof. | Pág. esperada | Pregunta | Páginas citadas | Estricta | Estricta (modelo) | Arnés |
| -: | -: | -: | :- | :- | -: | -: | :- |
| 1 | 0,10 | 12 | lexica | 11, 12, 13 | 1,00 | 1,00 | — |
| 1 | 0,10 | 12 | semantica | 11, 12, 13 | 1,00 | 1,00 | — |
| 1 | 0,28 | 31 | lexica | 30, 31 | 1,00 | 1,00 | — |
| 1 | 0,28 | 31 | semantica | 31 | 1,00 | 1,00 | — |
| 1 | 0,50 | 54 | lexica | 52, 53, 54 | 1,00 | 1,00 | — |
| 1 | 0,50 | 54 | semantica | 52, 53, 54 | 1,00 | 1,00 | — |
| 1 | 0,72 | 77 | lexica | 76, 77, 78 | 1,00 | 1,00 | — |
| 1 | 0,72 | 77 | semantica | 77, 78 | 1,00 | 1,00 | — |
| 1 | 0,90 | 96 | lexica | 94, 95, 96, 97 | 1,00 | 1,00 | — |
| 1 | 0,90 | 96 | semantica | 94, 95, 96, 97 | 1,00 | 1,00 | — |
| 2 | 0,10 | 12 | lexica | 11, 12, 13 | 1,00 | 1,00 | — |
| 2 | 0,10 | 12 | semantica | 11, 12, 13 | 1,00 | 1,00 | — |
| 2 | 0,28 | 31 | lexica | 31 | 1,00 | 1,00 | — |
| 2 | 0,28 | 31 | semantica | 30, 31, 32, 33 | 1,00 | 1,00 | — |
| 2 | 0,50 | 54 | lexica | 52, 53, 54 | 1,00 | 1,00 | — |
| 2 | 0,50 | 54 | semantica | 52, 53, 54 | 1,00 | 1,00 | — |
| 2 | 0,72 | 77 | lexica | 76, 77, 78 | 1,00 | 1,00 | — |
| 2 | 0,72 | 77 | semantica | 77 | 1,00 | 1,00 | — |
| 2 | 0,90 | 96 | lexica | 94, 95, 96, 97 | 1,00 | 1,00 | — |
| 2 | 0,90 | 96 | semantica | 94, 95, 96, 97 | 1,00 | 1,00 | — |
| 3 | 0,10 | 12 | lexica | 11, 12, 13 | 1,00 | 1,00 | — |
| 3 | 0,10 | 12 | semantica | 11, 12, 13 | 1,00 | 1,00 | — |
| 3 | 0,28 | 31 | lexica | 31 | 1,00 | 1,00 | — |
| 3 | 0,28 | 31 | semantica | 30, 31, 32, 33 | 1,00 | 1,00 | — |
| 3 | 0,50 | 54 | lexica | 2, 54 | 1,00 | 1,00 | — |
| 3 | 0,50 | 54 | semantica | 52, 53, 54 | 1,00 | 1,00 | — |
| 3 | 0,72 | 77 | lexica | 76, 77, 78 | 1,00 | 1,00 | — |
| 3 | 0,72 | 77 | semantica | 77 | 1,00 | 1,00 | — |
| 3 | 0,90 | 96 | lexica | 94, 95, 96, 97 | 1,00 | 1,00 | — |
| 3 | 0,90 | 96 | semantica | 94, 95, 96, 97 | 1,00 | 1,00 | — |

## 2. Dos agujas casi iguales en páginas consecutivas (medido)

Variante nueva (`run_aguja.py --dos-agujas`, §5.2 de la investigación): dos páginas insertadas seguidas al 0,50, con el mismo sujeto y la misma unidad y distinto lote y valor. Una pregunta léxica por aguja. Sonnet, semilla 42, 3 repeticiones, 17,85 min, binario `4dc641b5…`. No hay «antes»: la variante no existía, y con el esquema v5 las dos agujas compartían chunk y rango de páginas, así que eran indistinguibles por página.

| Métrica | n | Media | Mín. | Máx. |
| :- | -: | -: | -: | -: |
| recall_aguja | 6 | 1,00 | 1,00 | 1,00 |
| valor_y_pagina | 6 | 1,00 | 1,00 | 1,00 |
| pagina_exacta_estricta | 6 | 1,00 | 1,00 | 1,00 |
| pagina_exacta_estricta_modelo | 6 | 1,00 | 1,00 | 1,00 |
| mismo_chunk | 6 | 1,00 | 1,00 | 1,00 |
| seccion_real | 6 | 1,00 | 1,00 | 1,00 |
| pagina_mcp | 6 | 1,00 | 1,00 | 1,00 |
| latencia_respuesta_s | 6 | 25,18 | 23,89 | 27,38 |
| ingesta_s | 3 | 302,63 | 165,02 | 424,43 |

- `valor_y_pagina` (conjunta, a la manera de la SAA de CiteVQA): 1,0 solo si la respuesta da el valor de esa aguja y cita sola su página. El valor o la página de la otra aguja solo se admiten en una línea que nombre el lote de la otra (mencionarla como descartada). Se mide sobre el texto del modelo.
- `mismo_chunk`: 1,0 si algún texto devuelto por las tools contiene los dos lotes. Confirma que el caso es el que antes era indistinguible.

| Rep. | Páginas | Aguja | Lote y valor | valor_y_pagina | Páginas citadas | Mismo chunk | Arnés |
| -: | :- | :- | :- | -: | :- | -: | :- |
| 1 | 54-55 | aguja_1 (p. 54) | AGJ-GKU2, 392.4 mm | 1,00 | 52, 53, 54, 55 | 1,00 | — |
| 1 | 54-55 | aguja_2 (p. 55) | AGJ-L9A8, 215.7 mm | 1,00 | 52, 53, 54, 55 | 1,00 | verificacion_pagina |
| 2 | 54-55 | aguja_1 (p. 54) | AGJ-NZFH, 981.0 kPa | 1,00 | 54 | 1,00 | — |
| 2 | 54-55 | aguja_2 (p. 55) | AGJ-NNBA, 832.2 kPa | 1,00 | 52, 53, 54, 55 | 1,00 | verificacion_pagina |
| 3 | 54-55 | aguja_1 (p. 54) | AGJ-HKPK, 203.6 kN | 1,00 | 54, 55 | 1,00 | verificacion_pagina |
| 3 | 54-55 | aguja_2 (p. 55) | AGJ-S8Q9, 365.9 kN | 1,00 | 52, 53, 54, 55 | 1,00 | verificacion_pagina |

La ingesta de esta tanda (de 165 a 424 s) coincidió con la suite de aceptación en la misma máquina; no es comparable con la de la campaña.

## 3. Arnés (medido)

| Medida | Antes | Campaña | Dos agujas |
| :- | -: | -: | -: |
| Reintentos, salidas seguras y directivas | 0 | 0 | 0 |
| Consultas con alguna de esas intervenciones | 0 de 30 | 0 de 30 | 0 de 6 |
| Falsos bloqueos probables | 0 | 0 | 0 |
| Consultas con nota de verificación de páginas (fase 2) | no existía | 0 de 30 | 4 de 6 |

Notas de verificación emitidas (motivo literal):

| Tanda | Rep. | Prof. | Pregunta | Motivo |
| :- | -: | -: | :- | :- |
| dos agujas | 1 | 0,50 | aguja_2 | «Inserted page (benchmark test); not part of the original document.»: aparece en p. 54 y en p. 55; no se puede confirmar si es p. 55. |
| dos agujas | 2 | 0,50 | aguja_2 | «Inserted page (benchmark test); not part of the original document.»: aparece en p. 54 y en p. 55; no se puede confirmar si es p. 55. |
| dos agujas | 3 | 0,50 | aguja_1 | «Inserted page (benchmark test); not part of the original document»: aparece en p. 54 y en p. 55; no se puede confirmar si es p. 54. |
| dos agujas | 3 | 0,50 | aguja_2 | «Inserted page (benchmark test); not part of the original document.»: aparece en p. 54 y en p. 55; no se puede confirmar si es p. 55. |

## 4. Coste de las marcas (medido)

Sobre la salida de `search_normative_context` por MCP directo, con la pregunta tal cual:

| Tanda | Llamadas | Caracteres de la salida | Caracteres de marcas | Proporción |
| :- | -: | -: | -: | -: |
| Campaña | 30 | 601029 | 3452 | 0,57 % |
| Dos agujas | 6 | 117828 | 673 | 0,57 % |


## 5. Aceptación con Claude real (medido)

`acceptance_8_problemas`, una pasada con Sonnet sobre el commit `08809ce` (el ajuste posterior `dd65d2e` solo afecta a qué entrecomillados revisa el verificador): **10 de 11 pasan**.

- Falla `p2_tabla_cruce_exacto_con_nota_y_fronteras`, y vuelve a fallar al repetirlo solo sobre `dd65d2e`. En las dos pasadas la respuesta da 125 °C (\*) con su nota y descarta el orden invertido de los ejes. El oráculo exige que la frase que menciona «100 °C» contenga «invert…» o «descart…», y las respuestas lo dicen en otra frase («Orden invertido: … . Da 100 °C» y «Ese orden daría … (100 °C), que no aplica a este caso»).
- El test usa `lookup_double_entry_table`, que no tiene páginas ni marcas, y este cambio no toca el prompt del sistema. Es la fragilidad del oráculo de P2 ya anotada en `PROGRESS.md` (Sesión Arnés 4b), no una regresión medida de las citas de página. No se ha tocado el oráculo.
- El arnés no emitió ninguna nota de verificación de páginas en la suite (los fixtures son Markdown, sin páginas).

## 6. Lectura de los resultados

**Medido:**
- `pagina_exacta_estricta` pasa de 0,00 a 1,00 en las 30 consultas de la campaña, y `pagina_exacta_estricta_modelo` también vale 1,00: el acierto viene del texto del modelo, es decir, de las marcas (fase 1), no del bloque de corrección.
- `recall_aguja`, `pagina_exacta`, `seccion_real` y la recuperación por MCP siguen en 1,00. La latencia media de respuesta no sube (27,99 s frente a 29,67 s).
- Dos agujas: `valor_y_pagina` 1,00 en las 6 consultas, con las dos agujas en el mismo chunk en todas (`mismo_chunk` 1,00).
- Arnés: 0 reintentos, 0 salidas seguras y 0 falsos bloqueos en las 36 consultas. Las marcas no provocaron ningún bloqueo.
- Fase 2: 0 notas en la campaña y 4 en las dos agujas, todas por la misma frase, el aviso de página insertada, que está literalmente en las dos páginas. Es el caso «ambiguo» del SSD: la app lo dice y no elige. Ninguna nota contradice una página correcta.
- Las marcas ocupan el 0,57 % de los caracteres de la salida de `search_normative_context`.
- La campaña duró 77,7 min frente a 61,4 min. La ingesta media sube de 183 s a 250 s, con el mismo mínimo que antes (163 s) y 6 de 15 ingestas por encima de 300 s.

**Estimado fuera de la app** (script de análisis sobre las respuestas guardadas, con una regla aproximada a la del verificador): las 36 respuestas traen un extracto literal de la aguja. En 21 de 30 (campaña) y 3 de 6 (dos agujas) el extracto lleva la página en la misma línea, que es lo que la fase 2 comprueba. En las otras 12 la página va en una línea («Página del dato: 54») y el extracto en otra, así que no se comprobó.

**Supuesto, no medido:**
- Que la ingesta más lenta se deba a la variabilidad de la destilación con Claude y no a la columna nueva. Lo apoya que el mínimo no cambia y que la campaña anterior ya tuvo una ingesta de 344 s, pero no se ha medido por separado.
- Que el resultado se mantenga con otros modelos, con documentos escaneados o con páginas de mucho texto. La aguja es una página corta con un dato único, y las preguntas son fáciles.
- Que la fase 2 corrija bien en uso real: en estas tandas el modelo no citó ninguna página equivocada, así que el caso «discrepante» solo está cubierto por los tests.

## 7. Límites

1. La métrica estricta acepta una respuesta que cite la página sola aunque además cite el rango del fragmento. Casi todas las respuestas hacen las dos cosas.
2. La página es la física del fichero, no la impresa.
3. La fase 2 solo asocia una página a un extracto si están en la misma línea y a 80 caracteres como mucho. El formato «Página del dato» y «Extracto literal» en líneas distintas queda sin comprobar.
4. Los datos crudos (respuestas y salidas de tools) no se versionan: contienen texto del documento base.
