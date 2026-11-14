# Regla multidocumento en el agente: consultas separadas (Claude real, 2026-11-11)

**Veredicto: BLOQUEADA por ahora.**
- En el recuento bruto, una pregunta de un solo documento empeora: 21/23 → 20/23. `CLAUDE.md` §6 no admite ningún empeoramiento.
- Con una sola pasada no se puede separar ese retroceso del ruido del modelo (§3).
- Las cruzadas mejoran: 19/26 → 23/26 tras la revisión manual.

**Qué se ha cambiado:** la regla 15 del system prompt (`agent_orchestrator.rs`, commit `9da16fb`). Pide una `search_normative_context` por cada entidad, documento o condición y, antes de decir que una parte no consta, consultar `inspect_lakehouse_catalog` y hacer una segunda búsqueda dirigida (con `doc_id` o en modo `EXACT_BM25`). No hay detección léxica en Rust. El apoyo por `doc_id` literal no se ha implementado.

## 1. Errores (primero)

**Todos los errores reales son de la misma clase y aparecen en las dos variantes:** el agente dice «no consta» de una aguja que sí está en el centro. Todas son preguntas semánticas de aguja, en inglés y sin el lote.
- En los registros, el agente traduce la pregunta al español y busca en español sobre un documento en inglés.
- Es una abstención falsa, el peor tipo de error para un RAG.
- **Ningún fragmento de fixture salió mal en ninguna respuesta**, en ES y tras la revisión manual.

| Pregunta (fallos reales) | `main` | Rama |
|---|---|---|
| aguja-0-semántica ¹ | «no consta» | «no consta» |
| aguja-1-semántica | correcta | **«no consta»** |
| aguja-2-semántica | **«no consta»** | correcta |
| aguja-4-semántica ¹ | correcta (da los dos lotes) | **«no consta»** |
| cruzada-semántica-0 + ruido-presión ¹ | «no consta» (aguja) | correcta |
| cruzada-semántica-0 + integración-par ¹ | «no consta» (aguja) | correcta |
| cruzada-semántica-0 + tabular-esquina ¹ | «no consta» (aguja) | «no consta» (aguja) |
| cruzada-semántica-1 + integración-precalentamiento | «no consta» (aguja) | correcta |
| cruzada-semántica-2 + ruido-temperatura | «no consta» (aguja) | correcta |
| cruzada-semántica-2 + integración-curado | «no consta» (aguja) | correcta |
| cruzada-semántica-3 + integración-inspección | correcta | **«no consta» (aguja)** |
| cruzada-semántica-4 + tabular-centro ¹ | «no consta» (aguja) | «no consta» (aguja) |

¹ **Defecto de mi set de prueba:** las agujas 0 y 4 tienen el mismo sujeto («maximum test pressure for the relief valves»), con lotes y valores distintos. Sus preguntas semánticas son ambiguas: piden un valor y en el centro hay dos. Hay que generar el centro con sujetos únicos.

**Falsos negativos del puntuador, en la rama, corregidos a mano:**
- `cruzada-léxica-1 + ruido-tiempo`: responde 45 min y cita los tiempos de las otras líneas como no aplicables. Es correcta, pero añade cifras ajenas que no se pidieron, lo que roza la regla 14.
- `cruzada-semántica-0 + tabular-esquina`: la parte de la tabla (175 °C, descarta 110 °C) es correcta; la «mención dudosa» viene de ahí. La parte de la aguja sí falla (fila de la tabla).

**Mecanismo observado en los 2 retrocesos de un documento** (aguja-1 y aguja-4, en la rama):
1. La primera búsqueda, en español, no encuentra la aguja.
2. El agente aplica la regla: consulta el catálogo y dirige la segunda búsqueda con `doc_id` a documentos equivocados (`PTF-MON-31` y `PTF-PUR-05`). El título del catálogo de MIL-STD («System Safety») no delata la nota insertada.
3. Concluye «no consta».

En `main`, la primera consulta de aguja-1 llevaba el término inglés «proof load» y acertó: lo que cambia es la redacción de la consulta, que varía entre pasadas. **Supuesto, no medido:** la regla no causa el fallo, pero tampoco lo rescata cuando el candidato no se reconoce por su título.

## 2. Antes y después

Mismo centro aislado que en N3: MIL-STD-882E con 5 agujas más los fixtures de `ciclo_de_vida` y `rgb_industrial`, ingerido con Sonnet. Preguntas con `--modelo sonnet`.

- Binario de `main`: `e0f9f15d3193…` (`734a263`, con la 3c).
- Binario de la rama: `abc07cf32cd4…` (`9da16fb`).

Una pasada de cada uno. El orden de los dos binarios se alternó en cada pregunta para repartir la carga. Herramienta: `benchmarks/run_multidoc_claude.py`. No hubo ningún fallo de ejecución (98/98).

| Métrica | `main` | Rama |
|---|---|---|
| Un documento: correctas, puntuador | 21/23 | 20/23 |
| Un documento: correctas, revisión manual | 21/23 | **20/23** |
| Un documento sin las 2 ambiguas | 20/21 | 20/21 |
| Cruzadas: correctas, puntuador | 19/26 | 22/26 |
| Cruzadas: correctas, revisión manual | 19/26 | **23/26** |
| Cruzadas sin las 4 ambiguas | 19/22 | 21/22 |
| Tools por pregunta, un documento (media) | 1,91 | 2,09 |
| `search_normative_context` por pregunta, un documento | 1,17 | 1,65 |
| Tools por pregunta, cruzadas (media) | 3,31 | 3,81 |
| `search_normative_context` por pregunta, cruzadas | 2,35 | 2,65 |
| `inspect_lakehouse_catalog` por pregunta, cruzadas | 0,62 | 0,69 |
| Latencia mediana, un documento | 34,9 s | 30,3 s |
| Latencia mediana, cruzadas | 35,0 s | 42,8 s |
| CPU media del equipo durante las preguntas | 30-39 % | 28-37 % |
| N0 | 921 aprobados, 0 fallos y 35 ignorados (informe de la 3c) | 922 aprobados (1 test nuevo), 0 fallos y 35 ignorados; `clippy` y `fmt` limpios |

**Latencia:** es una sola pasada, con la CPU entre el 28 % y el 39 % (otras sesiones activas). No permite sacar conclusiones.

## 3. Por qué no se puede decidir con esta pasada

- Los cambios de corrección van en las dos direcciones y siempre en las mismas preguntas semánticas de aguja: en un documento, 2 empeoran y 1 mejora; en las cruzadas, tras la revisión manual, 1 empeora y 5 mejoran. Es lo que se espera si la redacción de la primera consulta varía de una pasada a otra.
- Sin las preguntas ambiguas, el recuento de un documento queda igual (20/21) y las cruzadas mejoran (19/22 → 21/22). Aun así, es una sola muestra.
- Las 5 cruzadas que mejoran coinciden con lo que predecía la medición sin Claude (separar las búsquedas recupera la parte perdida). Lo apoyan los registros: la rama lanza una búsqueda por parte.

## 4. Propuestas

1. **Repetir para separar señal de ruido**, previa autorización, porque gasta Claude: 3 pasadas por binario solo de las 10 preguntas semánticas de aguja y sus 13 cruzadas, unas 140 respuestas, en un centro nuevo con agujas de sujeto único. Criterio: la rama no puede tener más abstenciones falsas en un documento que `main`.
2. **El error dominante es otro y está en las dos variantes:** abstención falsa al buscar en un idioma distinto del documento. Una regla general candidata, no implementada: si una búsqueda no encuentra nada y el catálogo tiene documentos en otro idioma, repetirla con los términos de la pregunta original sin traducir. Se mediría igual.
3. **El puntuador** da falsos negativos cuando la respuesta enumera los valores de otros documentos como no aplicables, y eso merece revisarse aparte. Además, esa enumeración la provoca la propia respuesta y choca con la regla 14.
