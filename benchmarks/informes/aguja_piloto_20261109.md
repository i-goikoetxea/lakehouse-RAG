# Pista Aguja: piloto a una profundidad (2026-11-09)

Una repetición al 50 % con Sonnet, sobre el binario release del código de `main` (`src` idéntico en `68af7d2` y `5735101`; sha256 `8f2440b7…26a9`). Diseño: [`propuesta_aguja_documento_base.md`](../../docs/arquitectura/propuesta_aguja_documento_base.md). Comando:

```bash
python run_aguja.py --binario <lakehouse_rag_cli.exe> --profundidad 0.5 --repeticiones 1 --semilla 42 --modelo sonnet --salida <carpeta>
```

## Documento base (verificación §3)

| Punto | Resultado |
| :--- | :--- |
| Páginas | 106 (107 con la aguja) |
| `/Encrypt` | No |
| Texto | 2.160 caracteres por página de media; ninguna página con menos de 50 |
| `/Outlines` | 48 marcadores, de calidad irregular (ver Hallazgos) |
| `/PageLabels` | No. La página impresa no coincide con la física: la física 53 lleva el número impreso 46 |
| Leyenda | «DISTRIBUTION A: Approved for public release: distribution unlimited.» |
| Fijado | `manifiesto_datasets.json`: sha256 de esta copia y sha256 del texto sin sello |

**Reproducibilidad:** dos descargas seguidas dieron el mismo texto y bytes distintos (sha256 `ba0fa533…` y `d6b8c7b4…`). Cada página lleva el sello `Source: https://assist.dla.mil -- Downloaded: <fecha>`. Por eso el manifiesto fija la copia guardada en `.cache/datasets/` y añade una huella del texto sin sello para validar descargas nuevas.

## Resultado (medido)

| Medida | Valor |
| :--- | :--- |
| Aguja | Lote `AGJ-GKU2`, `392.4 mm`, página física 54 |
| Sección esperada | `TASK 201 PRELIMINARY HAZARD LIST` (marcador que envuelve la inserción) |
| Ingesta | **183,6 s**, `COMPLETO`: la destilación terminó, no quedó provisional |
| Avisos de ingesta | 1 de 224 textos supera el límite del modelo de embeddings |
| Pregunta léxica (con lote) | recall 1, página en respuesta 1, página y sección por la tool del agente 1, por MCP directo 1; 25,6 s |
| Pregunta semántica (sin lote) | recall 1, página en respuesta 1, página y sección por la tool del agente 1, por MCP directo 1; 28,1 s |
| Recuperación MCP directa | 2,04 s la primera llamada (arranque), 0,05 s la segunda |
| Duración total | 4 min 03 s (18:42:37 a 18:46:40 UTC) |

Las dos respuestas dan el valor con coma decimal (`392,4 mm`), citan `TASK 201 PRELIMINARY HAZARD LIST`, «páginas 52-54» y el chunk `CHK-0094`. Las dos señalan que la página dice no formar parte del documento original, sin abstenerse por ello.

## Hallazgos

1. **`pagina_exacta` es indulgente.** El chunk con la aguja abarca las páginas 52-54, y la respuesta cita ese rango. La métrica actual da 1 si el rango cubre la página esperada, así que un 1 no prueba que la app sepa la página exacta. Propuesta: añadir `pagina_unica` (inicio = fin = esperada) sin cambiar la actual.
2. **Los marcadores del PDF fallan en algunas profundidades.** Con la semilla 42, la sección esperada sería `PARAGRAPH PAGE` al 10 % (índice) y `FIGURE B-1. Assessing software…` al 75 % (marcador duplicado en cuatro páginas). Al 25 % sería `TABLE VI…`. Las secciones 1 a 4 del cuerpo no tienen marcador. En esas profundidades, `seccion_real` mediría la fidelidad a un esquema roto, no a la estructura real.
3. **La pregunta semántica es fácil:** «supplementary technical note» coincide con el título de la página insertada, y solo hay una aguja por centro.
4. **El aviso de página insertada es visible** para el modelo, que lo menciona en ambas respuestas. No provocó abstenciones en este piloto.

## Estimación revisada de la campaña completa

| | Medido | Supuesto |
| :--- | :--- | :--- |
| Ingesta | 183,6 s (una muestra) | Similar en las otras profundidades: mismo texto más una página |
| Preguntas | 25,6 s y 28,1 s | Entre 25 y 60 s, con reintentos del arnés |
| Por profundidad | 4,05 min | 4-5 min |
| Por repetición (5 profundidades) | — | **20-25 min** |
| 3 repeticiones | — | **1-1,25 h** |
| Tokens de Claude | No medidos | ~60 k de entrada por destilación (229 k caracteres / ~4) |

La estimación previa (30-45 min por repetición) era pesimista: la destilación terminó y la ingesta tardó la mitad de lo supuesto.

## Lo que no cubre

- Una sola muestra: no hay varianza.
- El tiempo de ingesta no se desglosa en destilación y embeddings.
- No se midió el coste en tokens ni en dinero.
