# Repuntuación con el puntuador corregido (2026-11-03)

Se volvieron a puntuar las respuestas guardadas de tres ejecuciones, **sin lanzar la app ni llamar a Claude**, con las reglas nuevas del SSD §5.1 (valores prohibidos mencionados) y §5.2 (identificadores en `rechazo_estricto`). Las notas publicadas en cada informe no se tocan: esto es un anexo.

| Ejecución | Informe crudo (ignorado por git o fuera del repo) | sha256 del JSON crudo | Revisión manual de rechazo |
| :--- | :--- | :--- | :--- |
| Línea base | `benchmark_report_20261102T181222_582801.json`, en `benchmarks/resultados/campana/` de la rama `claude/benchmark-fase-2-97ce34` | `d03927b5…2edb1` | [`revision_rechazo_linea_base.json`](revision_rechazo_linea_base.json) |
| Arnés, Fase 4 | `benchmark_report_20261103T112413_573073.json`, en el scratchpad de la sesión `arnes-fase-4` | `1d5ef7fa…612aa` | `revision_rechazo_fase4.json` (`51853fd3…e357f`), mismo scratchpad |
| Arnés, Fase 4b | `benchmark_report_20261103T142358_678400.json`, en el scratchpad de la sesión `arnes-falsos-bloqueos`. Solo la pista ciclo de vida | `d28933bc…4878d` | No hay: la pista RGB no se ejecutó |

Se encontraron las respuestas de las tres. Las rutas completas están en las notas de traspaso de cada sesión. Las respuestas crudas no se versionan.

Para reproducirlo, por ejemplo con la línea base:

```bash
python benchmarks/evaluacion/repuntuar.py --informe <crudo.json> --revision benchmarks/informes/revision_rechazo_linea_base.json --valores --salida <carpeta>
```

## Notas anteriores y nuevas

- **Anterior:** la nota estricta publicada para cada ejecución (en la línea base, el repuntuado del rechazo).
- **Nueva:** la que da el puntuador corregido.
- **Revisada:** la publicada tras la lectura manual. No cambia.

| Ejecución | Pista | Métrica | Anterior | Nueva | Revisada publicada |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Línea base | Ciclo de vida | `vigente` | 1,000 | 1,000 | 1,000 |
| Línea base | Ciclo de vida | `historico_por_fecha` | 1,000 | 1,000 | 1,000 |
| Línea base | Ciclo de vida | `historico_por_revision` | 0,556 | **1,000** | 1,000 |
| Línea base | Ciclo de vida | **Precisión global** | 0,852 | **1,000** (1,000 a 1,000) | 1,000 |
| Línea base | RGB industrial | `robustez_ruido` | 0,867 | **1,000** | 1,000 |
| Línea base | RGB industrial | `rechazo_estricto` | 0,000 | 0,000 | 0,000 |
| Línea base | RGB industrial | `rechazo_semantico` | 1,000 (manual) | 1,000 (manual) | 1,000 |
| Línea base | RGB industrial | `integracion` | 1,000 | 1,000 | 1,000 |
| Línea base | RGB industrial | `exactitud_tabular` | 0,833 | **1,000** | 1,000 |
| Línea base | RGB industrial | **Precisión global** | 0,625 | **0,688** | 0,688 |
| Fase 4 | Ciclo de vida | `vigente` | 1,000 | 1,000 | 1,000 |
| Fase 4 | Ciclo de vida | `historico_por_fecha` | 0,333 | 0,333 | 0,333 |
| Fase 4 | Ciclo de vida | `historico_por_revision` | 0,556 | **1,000** | 1,000 |
| Fase 4 | Ciclo de vida | **Precisión global** | 0,630 | **0,778** (0,778 a 0,778) | 0,778 |
| Fase 4 | RGB industrial | `robustez_ruido` | 1,000 | 1,000 | 1,000 |
| Fase 4 | RGB industrial | `rechazo_estricto` | 0,000 | **0,533** | 0,000 |
| Fase 4 | RGB industrial | `rechazo_semantico` | 1,000 (manual) | 1,000 (8 por reglas y 7 manuales) | 1,000 |
| Fase 4 | RGB industrial | `integracion` | 1,000 | 1,000 | 1,000 |
| Fase 4 | RGB industrial | `exactitud_tabular` | 0,500 | **1,000** | 1,000 |
| Fase 4 | RGB industrial | **Precisión global** | 0,625 | **0,854** (0,813 a 0,875) | 0,688 |
| Fase 4b | Ciclo de vida | `vigente` | 1,000 | 1,000 | 1,000 |
| Fase 4b | Ciclo de vida | `historico_por_fecha` | 1,000 | 1,000 | 1,000 |
| Fase 4b | Ciclo de vida | `historico_por_revision` | 0,556 | **1,000** | 1,000 |
| Fase 4b | Ciclo de vida | **Precisión global** | 0,852 | **1,000** (1,000 a 1,000) | 1,000 |

Las demás métricas no cambian. No hay ningún fallo de ejecución nuevo, porque no se ejecutó nada.

## Qué cambió y por qué

### Valores prohibidos (SSD §5.1): 18 respuestas de 0 a 1

Todas pasan a `acierto_con_mencion`, sin marca de duda. Son exactamente las 18 que la revisión manual de cada informe ya había dado por correctas: 7 en la línea base, 7 en la Fase 4 y 4 en la Fase 4b. Ninguna otra respuesta cambió de nota.

| Ejecución | Consulta | Repeticiones | Regla que la atribuye |
| :--- | :--- | :--- | :--- |
| Línea base | `revision-temperatura` | 1 y 2 | Otra revisión: la frase del 85 °C cita la Rev. 01 |
| Línea base | `revision-presion` | 2 y 3 | Otra revisión. En la 2 también hay una tabla con columnas «Rev. 00» y «Rev. 01», y una frase que hereda la Rev. 01 («La Rev. 01 la sustituye. Fija 8,2 bar») |
| Línea base | `ruido-temperatura` | 1 | Descarte: la línea de los valores de otras líneas dice que «no aplican» a la L-104 |
| Línea base | `ruido-espesor` | 3 | Descarte: «no es aplicable a la L-104» |
| Línea base | `tabular-esquina` | 2 | Descarte: «Consulta descartada», «invertí los ejes» |
| Fase 4 | `revision-temperatura` | 1, 2 y 3 | Otra revisión, por herencia («En esa revisión…»), por la fila «Rev. 01» de una tabla o tras ignorar «sustituye a la Rev. 00» |
| Fase 4 | `revision-duracion` | 3 | Otra revisión: «Rev. 01 (vigente): … sustituye a la Rev. 00. Fija 36 min» |
| Fase 4 | `tabular-esquina` | 1, 2 y 3 | Descarte: «orden inverso», «lo descarto» |
| Fase 4b | `revision-duracion` | 1 y 2 | Otra revisión. En la 2, «Esa cifra de 36 min es la que rige ahora, no la de la Rev. 00» hereda la Rev. 01 de la frase anterior |
| Fase 4b | `revision-temperatura` | 1 y 3 | Otra revisión |

Efecto en las métricas:
- **`historico_por_revision`** pasa de 0,556 a 1,000 en las tres ejecuciones (4 respuestas de 9 en cada una).
- **`exactitud_tabular`** sube de 0,833 a 1,000 en la línea base (1 de 6) y de 0,500 a 1,000 en la Fase 4 (3 de 6). La caída a 0,500 de la Fase 4 era del puntuador, no del arnés. Con el aviso `ejes_ambiguos` y la regla 13, el modelo explica el orden descartado en todas las repeticiones.
- **`robustez_ruido`** pasa de 0,867 a 1,000 en la línea base (2 de 15). En la Fase 4 ya valía 1,000.

La nota estricta coincide ahora con la revisada en todas las métricas de valores. **Es una coincidencia con la muestra, no una garantía:** las reglas son deterministas y tienen los límites del SSD §5.1.

### Identificadores en `rechazo_estricto` (SSD §5.2): 8 respuestas de 0 a 1, solo en la Fase 4

- **Fase 4:** 8 de 15 respuestas ya no ofrecían ningún valor. Contaban como fallo por el centro `bench-rgb-industrial-rN`, `EXACT_BM25`, «Capa 0», códigos como `PTF-HID-10` o `L-104`, «Rev. 02» y «0 resultados». Ahora valen 1,0, y su `rechazo_semantico` lo resuelven las reglas. Las 8 coinciden con la revisión manual anterior, que queda como contraste (`revision_manual_concordante`).
- **Las 7 restantes siguen en 0:**
  - 5 listan parámetros de otro documento: `rechazo-parametro-caudal` ×3 y `rechazo-parametro-dureza` reps. 1 y 3. Es el comportamiento que la métrica debe castigar.
  - 2 no dan valores, pero citan «nivel de prelación 2» (`rechazo-linea`, rep. 1) o «apartados 1 y 2» (`rechazo-parametro-dureza`, rep. 2). La exclusión no cubre apartados ni niveles a propósito, para que sea acotada. Por eso la revisión del arnés contó 10 respuestas sin valores y la regla nueva da 8.
- **Línea base:** sigue en 0,000. Las 15 respuestas añaden valores de otros documentos.
- **Precisión global de RGB en la Fase 4:** sube de 0,625 a 0,854. Ahora es mayor que la revisada publicada (0,688), porque aquella seguía usando el `rechazo_estricto` antiguo.

## Límites de esta repuntuación

- Solo se puntuó lo que ya estaba guardado. No mide ningún binario nuevo.
- En la Fase 4b no hay respuestas de RGB industrial: esa pista no se ejecutó.
- Las 6 salidas seguras de `historico_por_fecha` en la Fase 4 siguen en 0. No son del puntuador: el operador no recibió ninguna respuesta.
- La tabla compara contra la nota estricta publicada. Para las métricas de valores, la línea base solo publicó el informe crudo y el repuntuado del rechazo, que coinciden en esas métricas.
