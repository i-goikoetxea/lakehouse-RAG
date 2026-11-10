# Línea base de `claude/arreglos-rag-512d66` (2026-11-10, commit de partida `de9cae9`)

Medido en este worktree, `target` propio, perfil debug. CPU del equipo ~27 % antes de empezar (Docker y Chrome activos).

| Medida | Valor |
| :- | :- |
| `cargo test --no-fail-fast` | 912 pasan, 1 falla, 34 ignorados |
| Fallo | `adversarial_arnes_tests::test_adv_mega_de_fechas_termina_pronto` (5,74 s; límite de tiempo). Aislado pasa 3/3 (2,2-2,4 s): inestable por carga, no regresión |
| Python `benchmarks/` `python -m unittest` | 215 OK |
| Repuntuación RGB industrial (`repuntuar.py --valores` + revisión manual) | global 0,833; `rechazo_estricto` 0,533; `exactitud_tabular` 0,833; `rechazo_semantico` 1,000 |
| Repuntuación ciclo de vida | global 1,000 (27/27) |
| Repuntuación Aguja campaña (30) | recall 1,00; `pagina_exacta_estricta` 1,00 (modelo 1,00) |
| Repuntuación dos agujas (6) | recall 1,00; estricta 1,00 (modelo 0,833) |
| Repetición por el guard (111 respuestas) | 0 rechazos (reintentos) |
| Avisos de página en la repetición | 49 en 29 respuestas, todos en Aguja |

Notas:
- Las respuestas guardadas no estaban en `D:\Proyectos\lakehouse RAG\benchmarks\resultados` (no existe). Se usaron las crudas de los scratchpads de las sesiones `citas-pagina` (Aguja), `benchmark-rgb-arnes` (RGB) y `arnes-falsos-bloqueos` (ciclo de vida).
- Aguja no guarda la salida de las tools: la fuente se reconstruye con la frase de la aguja tras la marca de su página. Los 49 avisos son títulos o breadcrumbs entre comillas «no encontrados» en esa reconstrucción (artefacto), no citas erróneas.
- RGB y ciclo de vida son Markdown: no citan páginas, así que no ejercitan el verificador.
- Herramienta: `tests/repeticion_arnes_guardadas.rs` (ignorada). Toma la fecha de hoy del sello del nombre de las crudas, como el reloj de la app en la ejecución real.
