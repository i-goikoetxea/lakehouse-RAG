# Fixtures de validación E2E

Las respuestas esperadas se documentan aquí, y no dentro de los `.md`, porque el
destilador envía el documento completo a Claude: un comentario con las respuestas
contaminaría la extracción.

## `procedimiento_recepcion_rev03.md` y `procedimiento_recepcion_rev04.md`

Documento `PO-ALM-03`, tipo `PROCEDIMIENTO_OBRA`, prelación 2. La Rev. 04 sustituye a la Rev. 03.

### Matriz operativa (anti-fusión): 3 filas distintas

| Fase | Ejecutor | Responsable (firma) | Registro Rev. 03 | Registro Rev. 04 |
| --- | --- | --- | --- | --- |
| RECEPCION | Almacenero | Jefe de Almacén | FOR-ALM-01 | FOR-ALM-01 |
| CUSTODIA | Auxiliar de Almacén | Jefe de Producción | **FOR-ALM-02** | **FOR-ALM-05** |
| INSPECCION | Técnico de Calidad | Inspector QA/QC | PPI-MAT-09 | PPI-MAT-09 |

- Pregunta §9.2: «¿Quién es el responsable de la custodia y qué registro llena?»
  - Rev. 03 → **Jefe de Producción, FOR-ALM-02**.
  - Rev. 04 (vigente tras la re-ingesta) → **Jefe de Producción, FOR-ALM-05**.
  - Trampa: el Jefe de Almacén firma la RECEPCIÓN, no la CUSTODIA.

### Tabla 4-A: temperatura mínima de precalentamiento (°C)

- Eje X: espesor nominal `e` (mm): `e ≤ 20`, `20 < e ≤ 40`, `e > 40`.
- Eje Y: carbono equivalente `CE` (%): `CE < 0,40`, `0,40 ≤ CE ≤ 0,45`, `CE > 0,45`.
- 9 celdas. Notas: `(*)` exige aprobación del Inspector QA/QC y ensayo de dureza HV10; `(1)` interpaso < 250 °C (Anexo II).
- Cruce de control §9.3: **e = 30 mm, CE = 0,42 %** → Rev. 03: **100 °C con nota (*)**; Rev. 04: **125 °C con nota (*)**.
- Frontera: e = 20 mm cae en la columna `e ≤ 20` (inclusivo); e = 40 mm en `20 < e ≤ 40`.
- Celda «No admisible (*)»: `CE > 0,45` y `e > 40`.

### Referencias (aristas de grafo esperadas, al menos)

UNE-EN 1090-2, UNE-EN ISO 15614-1, PO-CAL-01, PCC-01, Anexo II, UNE-EN 10204,
y los registros FOR-ALM-01, FOR-ALM-02 (o FOR-ALM-05) y PPI-MAT-09.
La Rev. 04 añade la relación `DEROGA_A` hacia `PO-ALM-03 Rev. 03`.
