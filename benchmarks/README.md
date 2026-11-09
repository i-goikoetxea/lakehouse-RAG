# Benchmark de caja negra

Mide Lakehouse RAG solo por sus binarios (CLI y MCP). No importa ni modifica el core. Diseño: `docs/arquitectura/ssd_benchmark_runner.md`.

Estado: **Fase 3 en piloto**. Pista `aguja` (`run_aguja.py`) sobre el PDF de `manifiesto_datasets.json`, que se descarga a mano en `.cache/datasets/` y se verifica por sha256; piloto en [`informes/aguja_piloto_20261109.md`](informes/aguja_piloto_20261109.md). Fase 2: Pistas `ciclo-de-vida` y `rgb-industrial`, con fixtures propios en `fixtures/`. La pista `humo` es inventada y solo corre contra la CLI simulada, de modo que no gasta Claude. Primer informe real: [`informes/linea_base.md`](informes/linea_base.md), repuntuado con el puntuador corregido en [`informes/repuntuacion_20261103.md`](informes/repuntuacion_20261103.md).

```bash
cd benchmarks
python -m unittest
python run.py --binario tests/cli_simulada.py --pista humo --modelo haiku --repeticiones 2 --semilla 42
```

Con un binario real, primero se mide el coste con un piloto. `--max-consultas-por-categoria` reduce las consultas, no las ingestas:

```bash
python run.py --binario <lakehouse_rag_cli.exe> --pista ciclo-de-vida --pista rgb-industrial --modelo sonnet --repeticiones 1 --max-consultas-por-categoria 1
```

Los informes se escriben en `resultados/`, ignorada por git: `benchmark_report_<TIMESTAMP>.json` y `.md`, y `benchmark_crudas_<TIMESTAMP>.jsonl`, que guarda cada respuesta al llegar.

Códigos de salida: `0` informe sin fallos, `2` uso inválido, `3` informe con fallos de ejecución.

## Garantías

- **Aislamiento:** cada repetición usa una carpeta temporal propia como `LAKEHOUSE_RAG_CONFIG_DIR`. El cliente rechaza el directorio real de la app y cualquier centro fuera de la carpeta aislada.
- **Sin shell:** los argumentos viajan como lista. Un valor que la app leería como bandera (`--help`, `-m`) se rechaza antes de lanzar el proceso.
- **Fallos aparte:** timeout, sesión de Claude caducada, JSON roto o ingesta fallida se cuentan en `fallos_de_ejecucion`, con puntuación nula. Si una métrica no tiene consultas puntuadas vale `null`, nunca 0. La precisión solo cubre lo puntuado, así que siempre debe leerse junto a los fallos.
- **Latencia de recuperación:** un fallo por MCP se cuenta en `fallos_de_recuperacion` y no invalida la respuesta ya puntuada.

## Reglas de puntuación

- **Números:** `0,42` = `0.42` = `0,420`. `1.234,5` y `1,234.5` son el mismo valor. `1.234` conserva las dos lecturas y marca la consulta como dudosa si la ambigüedad decide el resultado.
- **`rechazo_estricto`:** vale 1.0 con abstención explícita y sin cifras nuevas. Vale 0.0 con cualquier cifra ausente de la pregunta, también en una cita (`página 12`, `apartado 2`) o en un código sin separador (`M12`). No cuentan los dígitos de códigos con separador (`PTF-HID-10`, `bench-…-r1`, `EXACT_BM25`), de citas de revisión o de capa (`Rev. 02`, `Capa 0`) ni el recuento `0 resultados` (SSD §5.2). Una cifra escrita en letra no se puede detectar con seguridad: no cambia la nota, pero marca la consulta como dudosa.
- **`rechazo_semantico`:** vale 1.0 si se abstiene sin atribuir ningún valor a la entidad inexistente. Las reglas no pueden saber a quién se atribuye una cifra, así que solo deciden 1.0 (abstención sin cifras nuevas) y 0.0 (sin abstención). El resto queda pendiente y, mientras haya pendientes, la métrica no da nota. Se resuelve con una revisión manual atada al hash de la respuesta, sin llamar a Claude: `python evaluacion/repuntuar.py --informe <informe.json> --revision <revision.json>`. Con `--valores` repuntúa también las demás categorías.
- **Valores:** deben aparecer todos los esperados y ningún prohibido. La coincidencia es numérica, no por subcadena. Si se pide unidad, se exige y se convierte dentro de su magnitud (`0,42 MPa` = `420 kPa` = `4,2 bar`). Un prohibido solo es fallo si se da como respuesta. Mencionado como descartado, con los ejes invertidos, no aplicable o de otra revisión, y con el esperado como respuesta, vale 1.0 (`acierto_con_mencion`). Un prohibido negado sin más («no es 0,55») sigue valiendo 0.0, y una mención ambigua vale 0.0 y queda dudosa (`mencion_dudosa`). Reglas y límites: SSD §5.1. Una abstención con cifras ajenas también vale 0.0 y queda como dudosa.
- **Revisiones:** `ValorEsperado.revision(1)` acepta `Rev. 01`, `rev 1`, `revisión n.º 1` y `R01`, pero no `Rev. 10`.
- **Dudosas:** se listan en `consultas_dudosas` para revisarlas a mano.
