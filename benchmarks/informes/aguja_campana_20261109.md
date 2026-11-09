# Pista Aguja: campaña 5 × 3 (2026-11-09)

Cinco profundidades por tres repeticiones con Sonnet y semilla 42, sobre el binario release de `main` (sha256 `8f2440b7…26a9`, el mismo del [piloto](aguja_piloto_20261109.md)). La base es el PDF de `manifiesto_datasets.json` (sha256 `ba0fa533…`), con 106 páginas. El arnés está en el commit `0f7c2ae`. Comando:

```bash
python run_aguja.py --binario <lakehouse_rag_cli.exe> --repeticiones 3 --semilla 42 --modelo sonnet --parar-si-provisional --tiempo-maximo-min 100 --salida <carpeta>
```

- **Duración:** 61,4 min.
- **Paradas:** ninguna. Las 15 ingestas quedaron `COMPLETO`, sin destilación pendiente, y no se pasó del límite de 100 min.
- **Consultas:** 30, una léxica y una semántica por caso. Ninguna falló.

## Profundidades

| Profundidad | Página física de la aguja | Sección esperada (marcador) |
| -: | -: | :- |
| 0,10 | 12 | excluida: no hay marcador fiable cerca |
| 0,28 | 31 | TASK 101 |
| 0,50 | 54 | TASK 201 PRELIMINARY HAZARD LIST |
| 0,72 | 77 | TASK 208 FUNCTIONAL HAZARD ANALYSIS |
| 0,90 | 96 | TASK 401 SAFETY VERIFICATION |

## Corrección del evaluador antes de agregar

El informe automático de la campaña daba `pagina_exacta` de 0,50 a 0,83. Esos ceros venían de un fallo del evaluador, no de la app.

**Causa:** la sección «Fuente» de las respuestas cita «Páginas: 52-54» o «**Páginas:** 76-77», pero `paginas_citadas` no admitía dos puntos ni `**` entre la palabra y el número.

**Corrección:**
- Se arregló la expresión regular, con su test previo: `test_paginas_citadas_con_dos_puntos_y_negrita_markdown`.
- Se repuntuaron sin red las 30 respuestas guardadas. No se repitió ninguna llamada a Claude.
- Cambiaron 8 valores, todos de 0 a 1. Ninguna otra métrica se ve afectada.

## Agregados por profundidad (medido)

Media, mínimo y máximo sobre las 6 consultas por profundidad. `ingesta_s` se calcula sobre 3 ingestas.

| Prof. | Métrica | n | Media | Mín. | Máx. |
| -: | :- | -: | -: | -: | -: |
| 0.10 | recall_aguja | 6 | 1.00 | 1.00 | 1.00 |
| 0.10 | pagina_exacta | 6 | 1.00 | 1.00 | 1.00 |
| 0.10 | pagina_exacta_estricta | 6 | 0.00 | 0.00 | 0.00 |
| 0.10 | pagina_tool_agente | 6 | 1.00 | 1.00 | 1.00 |
| 0.10 | seccion_real | 0 | n/d | n/d | n/d |
| 0.10 | pagina_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.10 | seccion_mcp | 0 | n/d | n/d | n/d |
| 0.10 | latencia_respuesta_s | 6 | 30.25 | 25.75 | 42.17 |
| 0.10 | ingesta_s | 3 | 168.87 | 162.96 | 178.38 |
| 0.28 | recall_aguja | 6 | 1.00 | 1.00 | 1.00 |
| 0.28 | pagina_exacta | 6 | 1.00 | 1.00 | 1.00 |
| 0.28 | pagina_exacta_estricta | 6 | 0.00 | 0.00 | 0.00 |
| 0.28 | pagina_tool_agente | 6 | 1.00 | 1.00 | 1.00 |
| 0.28 | seccion_real | 6 | 1.00 | 1.00 | 1.00 |
| 0.28 | pagina_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.28 | seccion_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.28 | latencia_respuesta_s | 6 | 29.08 | 25.32 | 34.47 |
| 0.28 | ingesta_s | 3 | 171.08 | 165.30 | 175.65 |
| 0.50 | recall_aguja | 6 | 1.00 | 1.00 | 1.00 |
| 0.50 | pagina_exacta | 6 | 1.00 | 1.00 | 1.00 |
| 0.50 | pagina_exacta_estricta | 6 | 0.00 | 0.00 | 0.00 |
| 0.50 | pagina_tool_agente | 6 | 1.00 | 1.00 | 1.00 |
| 0.50 | seccion_real | 6 | 1.00 | 1.00 | 1.00 |
| 0.50 | pagina_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.50 | seccion_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.50 | latencia_respuesta_s | 6 | 29.78 | 24.28 | 40.79 |
| 0.50 | ingesta_s | 3 | 170.81 | 162.76 | 186.84 |
| 0.72 | recall_aguja | 6 | 1.00 | 1.00 | 1.00 |
| 0.72 | pagina_exacta | 6 | 1.00 | 1.00 | 1.00 |
| 0.72 | pagina_exacta_estricta | 6 | 0.00 | 0.00 | 0.00 |
| 0.72 | pagina_tool_agente | 6 | 1.00 | 1.00 | 1.00 |
| 0.72 | seccion_real | 6 | 1.00 | 1.00 | 1.00 |
| 0.72 | pagina_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.72 | seccion_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.72 | latencia_respuesta_s | 6 | 29.02 | 25.70 | 32.27 |
| 0.72 | ingesta_s | 3 | 229.91 | 166.94 | 343.91 |
| 0.90 | recall_aguja | 6 | 1.00 | 1.00 | 1.00 |
| 0.90 | pagina_exacta | 6 | 1.00 | 1.00 | 1.00 |
| 0.90 | pagina_exacta_estricta | 6 | 0.00 | 0.00 | 0.00 |
| 0.90 | pagina_tool_agente | 6 | 1.00 | 1.00 | 1.00 |
| 0.90 | seccion_real | 6 | 1.00 | 1.00 | 1.00 |
| 0.90 | pagina_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.90 | seccion_mcp | 6 | 1.00 | 1.00 | 1.00 |
| 0.90 | latencia_respuesta_s | 6 | 30.22 | 26.56 | 33.05 |
| 0.90 | ingesta_s | 3 | 174.63 | 174.15 | 175.56 |

`recall_lexica` y `recall_semantica` valen 1,00 en todas las profundidades (n = 3 cada una), así que se omiten de la tabla.

## Telemetría del arnés (medido)

| Medida | Valor |
| :- | -: |
| Intervenciones, por etapa | ninguna |
| Reintentos | 0 |
| Salidas seguras | 0 |
| Consultas con alguna intervención | 0 de 30 |
| Falsos bloqueos probables | 0 |

Con un dato único y anclado en un fragmento recuperado, el arnés no tuvo que intervenir. Esta campaña no prueba nada sobre su comportamiento con datos ambiguos o ausentes.

## Otras observaciones (medido)

- **Herramientas:** el agente usó `search_normative_context` en las 30 consultas, y además `inspect_lakehouse_catalog` en 3.
- **Fragmentos con la aguja:** al 0,28 y al 0,72 se recuperaron dos fragmentos contiguos con la aguja; en el resto, uno. Cubren de 2 a 4 páginas.
- **Recuperación MCP directa:** de 0,04 s a 3,6 s; la llamada más lenta es la primera de cada proceso.
- **Ingesta:** de 163 s a 187 s, con una excepción de 344 s (repetición 3, profundidad 0,72). Esa ingesta también quedó `COMPLETO`, y la causa del retraso no se midió.
- **Aviso de página insertada:** las 30 respuestas lo mencionan, y ninguna se abstiene por ello.
- **Marcador espurio al 0,10:** allí el fragmento lleva el breadcrumb `PARAGRAPH PAGE`, un encabezado del índice convertido en marcador. Confirma que excluir esa profundidad de `seccion_real` era lo correcto.

## Interpretación (supuesto, no medido)

- **Recall:** la app encuentra un dato único e inequívoco a cualquier profundidad de un documento de 106 páginas, con la pregunta léxica y con la semántica. En esta escala, la profundidad no influye en el recall.
- **`pagina_exacta_estricta` = 0:** mide la granularidad del chunk, no un error del modelo. La app cita siempre el rango del fragmento, de 2 a 4 páginas, y nunca la página sola. Para acertarla, la app tendría que registrar la página de cada frase dentro del chunk, lo que exigiría cambiar el código Rust, fuera del alcance de esta tarea.
- **Sección:** con marcadores fiables, el breadcrumb de la app coincide con la sección que envuelve la aguja en todas las profundidades medidas.

## Límites

1. **Aviso de página insertada.** La página insertada se declara a sí misma como página de prueba ajena al documento, porque la licencia exige declarar el cambio. El modelo lo sabe, y eso puede facilitar la recuperación o, con otro modelo, provocar abstenciones.
2. **Chunk de varias páginas.** `pagina_exacta` acepta el rango del chunk, y `pagina_exacta_estricta` no puede valer 1 con la app actual. Ninguna de las dos mide si la app sabe la página exacta.
3. **El 0,10 no tiene sección medida.** Los marcadores del PDF no cubren los apartados 1 a 6 del cuerpo.
4. **Preguntas fáciles.** La léxica lleva el lote, que es único. La semántica no repite palabras del título de la página insertada, pero sí del sujeto de la frase. Además, hay una sola aguja por centro y no hay distractores.
5. **Muestra pequeña.** Son un documento, un modelo y tres repeticiones con la misma semilla base. Un 1,00 con n = 6 no excluye fallos poco frecuentes.
6. **Repuntuación.** `pagina_exacta` y `pagina_exacta_estricta` se recalcularon sin red con el evaluador corregido. El resto de métricas son las del informe original.

## Siguientes pasos posibles

- **Distractores:** añadir agujas casi iguales, con el mismo sujeto y otro lote o valor, para medir precisión además de recall.
- **Más páginas:** probar un documento base más largo, o concatenar copias, para buscar el punto en que la profundidad empieza a influir.
- **Página dentro del chunk:** decidir si la app debe registrar la página exacta de cada frase. Afecta al código Rust y requiere aprobación.
