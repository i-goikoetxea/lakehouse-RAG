# Demo guiada — Hormigonado del Edificio Norte

Estos documentos son sintéticos, cortos y de un dominio distinto al de los tests: el hormigonado. Cada uno contiene a propósito los datos que provocan los problemas del RAG de procedimientos. Su validación real con la CLI (Claude + FastEmbed) del 2026-10-19 está en `target/aceptacion/validar_demo.txt`.

| Archivo | Qué es | Trampa que contiene |
| --- | --- | --- |
| `PO-HOR-07_rev01.md` | Procedimiento de obra, Rev. 01 (vigor 2026-02-01) | 3 fases con ejecutor, responsable y registro distintos; Tabla 7-B con rangos, categorías y notas; condición «tiempo frío < 5 °C» en un encabezado |
| `PO-HOR-07_rev02.md` | Rev. 02 (vigor 2026-09-01), deroga la Rev. 01 | Cambia el registro del vertido (FOR-HOR-03 → **FOR-HOR-08**) y la celda CEM II / 10–20 °C (5 → **7** días) |
| `PCC-02_pliego_cliente.md` | Pliego del cliente, prelación 1 | Probetas cada **25 m³** |
| `NT-HOR-02_norma.md` | Norma, prelación 5 | Probetas cada 100 m³ (supletoria) |

**Orden de subida:**
1. `PO-HOR-07_rev01.md`, con el diálogo nativo (clic en la zona de arrastre).
2. `PO-HOR-07_rev02.md`, **arrastrándolo desde el Explorador**.
3. `PCC-02_pliego_cliente.md` y `NT-HOR-02_norma.md`, con cualquiera de los dos métodos.
4. Vuelve a subir `PO-HOR-07_rev02.md`: debe responder «ya ingerido», sin llamar a Claude.

## Preguntas y respuesta esperada

| # | Problema | Pregunta (modelo sugerido) | Respuesta correcta | Lo que NO debe salir |
| --- | --- | --- | --- | --- |
| 1 | Anti-fusión | ¿Quién es el responsable del vertido y vibrado y qué registro firma? (sonnet) | **Encargado General**, **FOR-HOR-08** | Jefe de Obra, FOR-HOR-01, FOR-HOR-03 |
| 2 | Tabla | Según la Tabla 7-B, ¿cuántos días de curado mínimo hacen falta con una temperatura media de 15 °C y cemento CEM II? (**cambia a haiku**) | **7 días (\*)** + nota «aprobación del Director de Ejecución y control de madurez» | 5 días |
| 3 | Frontera | Tabla 7-B: ¿curado mínimo con una temperatura media de exactamente 20 °C y cemento CEM III? (sonnet) | **7 días (1)**: el 20 cae en «10 ≤ T ≤ 20» | 5 días (columna T > 20) |
| 4 | Versiones / time travel | Según la Tabla 7-B, ¿cuántos días de curado mínimo hacían falta el 1 de mayo de 2026 con 15 °C y cemento CEM II? (**cambia a opus**) | **5 días (\*)**, Rev. 01 | 7 días |
| 5 | Prelación | ¿Cada cuántos metros cúbicos hay que tomar probetas de hormigón en esta obra? | **25 m³ (PCC-02 prevalece)**; NT-HOR-02 (100 m³) como supletoria | 100 m³ como respuesta |
| 6 | Agregación exhaustiva | Dame el listado completo de los registros que exige el PO-HOR-07 vigente, con la fase y quién firma cada uno. | FOR-HOR-01 / Jefe de Obra · FOR-HOR-08 / Encargado General · PPI-HOR-11 / Responsable de Calidad | FOR-HOR-03 (derogado) |
| 7 | Condición heredada | ¿Durante cuántas horas hay que mantener las mantas térmicas sobre la losa? | **72 h**, **solo en tiempo frío (< 5 °C)** | 72 h sin la condición |
| 8 | Grafo / tri-store | Muéstrame el grafo de referencias del procedimiento PO-HOR-07. | 7 aristas: 3 registros, PCC-02, NT-HOR-02, PO-CAL-01 y DEROGA_A, todas de la Rev. 02 | Aristas de la Rev. 01 |

El supernodo (1.000 aristas) y el fallo inyectado a mitad de ingesta no se pueden reproducir a mano con documentos normales. Los cubren los tests automáticos P4 y P8 (`tests/acceptance_8_problemas.rs`), que pasaron 2 de 2 veces.
