# SSD: Benchmark Runner independiente

Estado: **propuesta, sin implementar.** Requiere aprobación (AGENTS.md §1.2).

## 1. Qué cambia respecto a la especificación recibida

| Especificación | Realidad | Decisión |
| :--- | :--- | :--- |
| `lakehouse-rag ingest --center --file` y `query --center --query` | No existen. La CLI es `lakehouse_rag_cli.exe --ingest <archivo> --centro <id>` y `--ask "<texto>" --centro <id> --modelo <alias> --json` | Se usa la CLI real, sin tocar el core |
| Solo datasets públicos, sin preguntas sintéticas | La aguja, los distractores «del mismo dominio técnico» y las revisiones no existen en ningún dataset público localizado | Dos pistas: **pública** (RGB original) e **industrial** (fixtures estáticos versionados, escritos a mano, nunca generados al ejecutar) |
| CRUD-RAG como referencia de versionado | Es un benchmark de noticias en chino, con tareas de generación y recuperación. No trae revisiones de procedimientos | No se descarga. La prueba se llama `Ciclo de vida`, inspirada en CRUD, y **no es comparable** con el artículo |
| `RGB-Industrial` con métricas de RGB | RGB son preguntas sobre noticias, en inglés y chino | La pista pública usa `en_refine.json` y `en_int.json`. La industrial solo adapta la metodología, y el informe nunca mezcla ambas |
| `retrieval_latency_ms_avg` | `--ask` solo permite medir el tiempo total, que incluye a Claude | Dos medidas: `latencia_respuesta_ms` (`--ask`) y `latencia_recuperacion_ms` (llamada directa a `search_normative_context` por MCP) |
| Multi-turno | `--ask` es de un solo turno (`lakehouse_app.rs:501`) | La pereza multi-turno **no se mide aquí**. Sigue cubierta por el test real de Rust |

## 2. Estructura

Python 3, solo biblioteca estándar más `pypdf` para insertar la aguja. Nada de `benchmarks/` lo importa el core, ni al revés.

```text
benchmarks/
├── README.md
├── requirements.txt
├── manifiesto_datasets.json     # URL, commit fijado, sha256 y licencia
├── run.py                       # punto de entrada
├── lakehouse/
│   ├── cli.py                   # --crear-centro, --activar, --ingest, --ask (sin shell)
│   └── mcp.py                   # cliente stdio para medir recuperación
├── datasets/
│   ├── descargar.py             # descarga explícita y verifica sha256
│   └── rgb.py                   # lector de RGB
├── pistas/
│   ├── rgb_publico.py
│   ├── aguja.py
│   ├── rgb_industrial.py
│   └── ciclo_de_vida.py
├── fixtures/                    # Markdown propios, sin datos privados
├── evaluacion/
│   └── puntuar.py               # reglas deterministas, sin LLM juez
├── tests/                       # puntuación y lectores, con CLI simulada
└── .cache/  resultados/         # ignorados por git
```

## 3. Aislamiento y reproducibilidad

- Cada ejecución usa `LAKEHOUSE_RAG_CONFIG_DIR` y centros propios en una carpeta temporal. **Nunca toca tus centros reales.**
- El informe registra commit de la app, hash del binario, modelo, semilla y hash de cada dataset.
- Las respuestas varían entre ejecuciones: cada pista corre 3 veces y se informa media y extremos.
- Se guarda cada respuesta cruda con su `herramientas`, para poder auditar cualquier puntuación.

```bash
python benchmarks/datasets/descargar.py --dataset rgb
python benchmarks/run.py --binario target/release/lakehouse_rag_cli.exe --pista rgb-industrial --modelo sonnet --repeticiones 3 --semilla 42
```

## 4. Pistas

Un fallo de ejecución (timeout, sesión de Claude caducada) se cuenta aparte y **nunca** como acierto ni como abstención.

### 4.1 RGB público
Referencia: Chen et al., AAAI 2024. Una muestra con semilla de `en_refine.json` (ruido y rechazo) y `en_int.json` (integración). Cada pregunta trae sus propios pasajes, que se ingieren como Markdown en un centro propio.

- **Advertencia de coste:** cada documento pasa por la destilación con Claude. Por eso la muestra empieza en 10 preguntas y solo crece tras medir el coste real.
- **Validez:** los esquemas de los JSON aún no están inspeccionados. El primer paso de esta pista es leerlos y confirmar los campos.

### 4.2 Aguja
1. Documento base de 100-200 páginas, **público y de licencia verificada**. Sin PDF privados.
2. Se inserta una página con la aguja al 10, 25, 50, 75 y 90 %, con pypdf, y se conoce así la página física esperada.
3. Se ingiere y se pregunta. Un documento por profundidad: 5 ingestas por repetición, cada una de varios minutos.

Métricas: `recall_aguja` (valor correcto en la respuesta), `pagina_exacta` y `seccion_real`, comparando la cita y el resultado de la tool con lo esperado. Para no aprenderse la aguja, se cambian el lote y el valor por profundidad.

### 4.3 RGB industrial
- **Ruido:** el dato en un documento más 5 distractores que usan la misma terminología con otros valores.
- **Rechazo:** preguntas por normas, códigos o valores inexistentes. Vale 1.0 con abstención explícita y sin cifras nuevas; 0.0 si aparece cualquier cifra que no esté en la pregunta.
- **Integración:** el dato en una sección y su condición en otra. Solo acierta si aparecen ambos.

### 4.4 Ciclo de vida
1. Se ingiere `Rev. 00` con el valor X y después `Rev. 01` con el valor Y.
2. Vigente: debe responder Y, citar `Rev. 01` y no presentar X como vigente.
3. Histórico: debe responder X y citar `Rev. 00`, sin Y como valor de esa revisión.

La app viaja en el tiempo con `fecha_referencia`, así que se prueban dos formas de preguntar: por fecha y por número de revisión. Un fallo en esta última sería un hallazgo real del producto.

## 5. Puntuación e informe

Reglas deterministas en `evaluacion/puntuar.py`: normalización numérica y de unidades, valores esperados y prohibidos, y locuciones de abstención. Sin LLM juez, porque añadiría otra fuente de ruido. Se revisan a mano las respuestas clasificadas como dudosas.

Salida: `benchmark_report_<TIMESTAMP>.json` y `.md`, con una sección por pista:

```json
{
  "benchmark": "RGB-Industrial",
  "comparable_con_articulo": false,
  "app_commit": "…",
  "modelo": "sonnet",
  "repeticiones": 3,
  "total_consultas": 50,
  "fallos_de_ejecucion": 0,
  "precision_global": { "media": 0.0, "min": 0.0, "max": 0.0 },
  "metricas": {
    "robustez_ruido": 0.0,
    "rechazo_negativo": 0.0,
    "exactitud_tabular": 0.0,
    "latencia_respuesta_ms_media": 0,
    "latencia_recuperacion_ms_media": 0
  }
}
```

## 6. Plan de fases

| Fase | Contenido | Hecho cuando |
| :--- | :--- | :--- |
| 1 | Runner, cliente CLI y MCP, puntuación con tests y CLI simulada | Tests en verde, sin gasto en Claude |
| 2 | `Ciclo de vida` y `RGB industrial` con fixtures | Primer informe real |
| 3 | `Aguja` con el documento base aprobado | Informe por profundidad |
| 4 | Descarga y `RGB público`, tras revisar la licencia | Muestra de 10 y coste medido |

**Licencias:** se debe leer el `LICENSE` de RGB antes de descargarlo; una fuente secundaria indica que prohíbe el uso comercial. Los datasets no se versionan.

**Fuentes:** [RGB](https://github.com/chen700564/RGB), [artículo RGB](https://arxiv.org/html/2309.01431v2), [CRUD-RAG](https://github.com/IAAR-Shanghai/CRUD_RAG), [artículo CRUD-RAG](https://arxiv.org/html/2401.17043v2).
