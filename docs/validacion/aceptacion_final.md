# Aceptación final — Lakehouse RAG Industrial (Fase 15)

> **Fecha:** 2026-10-19 · **Plan:** `docs/arquitectura/plan_lakehouse_v2.md` §1 y §3.1 · **Especificación:** §9
> **Suite:** `tests/acceptance_8_problemas.rs` (10 tests `#[ignore]`, Claude Code y FastEmbed reales)
> **Cómo se reproduce:** `cargo test --test acceptance_8_problemas -- --ignored --nocapture --test-threads=1`

## 1. Resumen

La aplicación controla los 8 problemas del RAG de procedimientos y cumple los 4 criterios §9 de la
especificación, con evidencia de punta a punta: Claude Code real (destilación y agente), FastEmbed
E5-large real, LanceDB en disco, proyección a DataFusion, grafo e índice semántico, y la misma
composición que la ventana (`bootstrap::composicion_real`).

| Ejecución | Resultado | Duración |
| --- | --- | ---: |
| 1 | **10/10 OK** | 605 s |
| 2 | **10/10 OK** | 609 s |

**Cómo se monta la prueba:** cada test usa la fachada `LakehouseApp` con los adaptadores reales.
Solo se sustituye una pieza cuando el problema lo exige, y se dice en cada caso:

- **P4 (supernodos):** dos documentos **sintéticos** se destilan con un paquete fijo:
  - una norma «hub» con 1.000 remisiones;
  - un procedimiento que la cita.

  Que Claude extraiga 1.000 aristas no es viable: supera el timeout de 300 s y la salida sería enorme y no determinista. Además, lo que se valida aquí es el recorrido acotado, no la extracción. El parser, FastEmbed, LanceDB, la proyección, las tools y el agente siguen siendo los reales.
- **P8 (tri-store):** el almacén LanceDB real va envuelto en un decorador. Cuando se activa un interruptor, falla al escribir el lote de la tabla 05.
- **§9.4:** el agente real va envuelto en un registrador que anota el modelo de cada subproceso.

## 2. Problema → control → evidencia → estado

Las salidas son literales de la ejecución 1 (log completo en `target/aceptacion/ejecucion1.txt`, regenerable).

| # | Problema | Control | Evidencia (test → salida real) | Estado |
| --- | --- | --- | --- | --- |
| 1 | Contaminación de atributos | Una fila por hito en `03_matriz_operativa`, con ejecutor, responsable y registro en columnas separadas; `query_operational_matrix` filtra por columna exacta | `p1_anti_fusion…`: la tabla 03 vigente tiene 3 filas distintas (RECEPCION, CUSTODIA, INSPECCION). La tool devuelve **1 fila** de CUSTODIA. Chat (sonnet, 21 s): «**Responsable:** Jefe de Producción · **Registro:** FOR-ALM-05». No aparecen Jefe de Almacén, FOR-ALM-01, PPI-MAT-09 ni FOR-ALM-02 | ✅ |
| 2 | Geometría de tablas | Ejes tipados con inclusividad; el lookup cruza en Rust, con nota obligatoria | `p2_tabla…`: cruce e = 30 / CE = 0,42 → `resuelto`, «**125 °C (\*)**: Requiere aprobación previa del Inspector QA/QC y ensayo de dureza HV10». Fronteras: e = 20 → «50», e = 20,0001 → «125 (\*)». Chat e = 20 mm → «**50 °C** … rango `x <= 20` (el límite de 20 mm queda incluido)» | ✅ |
| 3 | Versiones obsoletas / time travel | `vigente`, `fecha_vigor` y `fecha_derogacion` en las 6 tablas; `fecha_referencia` en todas las tools | `p3_versiones…`: la Rev. 03 tiene `vigente=false` y derogación 2026-09-15. Hay **0 aristas v3 vigentes** en 06 y todas las aristas del recorrido son de la v4. Chat: «Hoy … Rev. 04 … **125 °C (\*)**; 1 de junio de 2026 … Rev. 03 … **100 °C (\*)**», con la consulta a fecha | ✅ |
| 4 | Supernodos / hairball | Recorrido de 2 saltos como máximo, `max_vecinos` 15, `hub_resumido` (grado > 25) sin expandir y ≤ 50 nodos | `p4_supernodos…`: la tabla 06 contiene 1.000 aristas del hub. Desde PO-SINT-01: 4 nodos; desde NORMA-HUB: 18 nodos (≤ 50). Chat: «NORMA-HUB tiene grado 1001, por lo que se devuelve como *hub resumido* y no se expande» | ✅ |
| 5 | Prelación normativa | `nivel_prelacion` desnormalizado; búsqueda agrupada con `prevalece` / `supletorio`; system prompt | `p5_prelacion…`: «Se debe ensayar … el **100 %** … Norma que prevalece (nivel 1): **PCC-01** … Norma supletoria (nivel 5): **NT-SOL-01** … 10 %» | ✅ |
| 6 | Agregación global | `execute_athena_sql` sobre las tablas completas (DataFusion), solo lectura | `p6_agregacion…`: la SQL del test da {FOR-ALM-01/Jefe de Almacén, FOR-ALM-05/Jefe de Producción, PPI-MAT-09/Inspector QA/QC}, **igual al 100 % al oráculo**. Chat: «El procedimiento exige **3 registros**. La consulta devolvió 3 filas, sin truncamiento». Jefe de Producción → solo PO-ALM-03 / FOR-ALM-05 (tool `execute_athena_sql`) | ✅ |
| 7 | Alcance condicional | Pila de encabezados → breadcrumb; la condición del encabezado se hereda en chunks e hitos | `p7_condicion…`: todo fragmento con «80 °C» trae `condicion_aplicacion` con «< 0». Incluso a la pregunta que no menciona la condición («¿A cuántos grados…?»), la respuesta es: «**80 °C como mínimo** … solo cuando se suelda con temperatura ambiente bajo cero (< 0 °C)» | ✅ |
| 8 | Desincronización tri-store | Ingesta transaccional STAGED → commit; rollback por `ingest_id`; proyección reconstruida desde LanceDB | `p8_tri_store…`: «Fallo inyectado al escribir 05_celdas_doble_entrada». Filas físicas antes = después en las 6 tablas (1, 9, 3, 1, 9, 10), integridad OK y el lookup sigue en **100** (Rev. 03). Tras reconstruir, DataFusion = LanceDB en las 6 tablas, índice semántico 12 = 12 fragmentos y grafo 10 = 10 aristas. El reintento sin fallo da v4 → 125 y las vistas siguen coincidiendo | ✅ |

## 3. Criterios §9 de la especificación

| § | Criterio | Evidencia | Estado |
| --- | --- | --- | --- |
| 9.1 | Multi-centro: solo se leen los datos del centro activo | `s9_1_multicentro…`: el centro B está inactivo y sus datos viven en su propia carpeta. Desde A solo son visibles {NT-SOL-01, PCC-01, PO-ALM-03}. Una SQL de A sobre la vista de B falla («table … not found»), una tool sobre B inactivo da error y el chat no menciona nada de B | ✅ |
| 9.2 | Anti-fusión Recepción / Custodia / Inspección | P1 | ✅ |
| 9.3 | Tabla de doble entrada con nota al pie (lookup o SQL) | P2 | ✅ |
| 9.4 | Selector de modelos en caliente con las tools en vivo | `s9_4_selector…`: catálogo real con `sonnet`, `opus` y `haiku`. En **la misma conversación**: haiku → `--model haiku` (subprocesos `[haiku, haiku]`), tool `query_operational_matrix` → Jefe de Producción / FOR-ALM-05; sonnet → `--model sonnet` → Jefe de Almacén / FOR-ALM-01; opus → `--model opus` → Inspector QA/QC / PPI-MAT-09 | ✅ |

## 4. Pendientes de la Fase 14 cerrados

| Pendiente | Resolución | Evidencia |
| --- | --- | --- |
| `petgraph` sin uso | Retirado de `Cargo.toml` (el grafo se resuelve en `traverse_normative_graph`) | `cargo build` / `cargo test` |
| Botón «Eliminar centro» | Ya existía desde la Sesión 14b, con diálogo de confirmación: quita el centro del registro (y lo desactiva antes si está activo) **sin tocar el disco** | Checklist §7, paso 5 |
| Sesión de Claude Code caducada | `infrastructure/llm/sesion_claude.rs`: el agente y el destilador traducen la salida real del CLI («Not logged in · Please run /login», o el sobre JSON con `is_error`) a un mensaje accionable: «Claude Code no tiene una sesión válida … ejecuta `claude` y escribe `/login` …». El catálogo de modelos no lo necesita: el CLI real lo sirve sin sesión | `tests/adversarial_sesion_claude_tests.rs`: un `claude` falso compilado al vuelo con la salida real capturada con un `CLAUDE_CONFIG_DIR` vacío, sin tocar la sesión del usuario |
| CLI de consola en release | Binario aparte **`lakehouse_rag_cli.exe`** (`src/bin/lakehouse_rag_cli.rs`, subsistema consola) con los mismos subcomandos y `--mcp-server`. Comparte el arranque (`presentation/cli/proceso.rs`) con `lakehouse_rag.exe`, que sigue abriendo la ventana sin consola. Es la opción más simple y robusta: PowerShell lo espera, la salida sale en orden y `$LASTEXITCODE` es fiable | `tests/integration_cli_consola.rs` y prueba release (§6) |

## 5. Límites conocidos

- **Rendimiento:**
  - Una ingesta tarda unos 40–50 s por documento con tabla (Claude destila el documento completo) y 12–21 s por uno sin tablas.
  - Una pregunta tarda unos 20–35 s con una tool y hasta unos 60 s si el agente encadena varias (catálogo + lookup).
- **Primera carga del modelo:** FastEmbed descarga MultilingualE5Large (unos 2,2 GB) la primera vez. La ventana lo avisa.
- **Formatos:** solo Markdown (`.md`, `.markdown`, `.txt`). PDF y DOCX quedan **fuera de alcance**: conviértelos antes, o espera a la fase de `docling` (plan §4).
- **Dependencia de Claude Code:** la destilación y el chat necesitan el CLI `claude` instalado y con sesión. Sin sesión se muestra el mensaje de §4.
- **Agrupado por tema de la prelación:** depende de que los documentos titulen igual la sección. Si no, el system prompt sigue obligando a aplicar la prelación, pero la tool no marca el conflicto.
- **Variabilidad del LLM:** las aserciones de la suite son por contenido y por tool. La redacción cambia entre ejecuciones; las cifras, los códigos y las citas no (ver estabilidad en §6).

## 6. Estabilidad y auditoría

**Estabilidad de la suite de aceptación:** 20/20 en dos ejecuciones consecutivas (logs en
`docs/validacion/fase15/acceptance_8_problemas_ejecucion{1,2}.log`).

| Test | Ejecución 1 | Ejecución 2 |
| --- | ---: | ---: |
| Preparación (4 ingestas reales + 3 sintéticas) | 128 s | 113 s |
| P1 anti-fusión | 21,0 s | 20,9 s |
| P2 tabla | 52,6 s | 66,6 s |
| P3 versiones | 35,6 s | 32,4 s |
| P4 supernodos | 23,7 s | 28,9 s |
| P5 prelación | 22,6 s | 22,8 s |
| P6 agregación | 51,9 s | 51,9 s |
| P7 condición heredada | 57,3 s | 59,0 s |
| P8 tri-store (2 ingestas reales) | 125,4 s | 122,9 s |
| §9.1 multi-centro | 20,7 s | 19,6 s |
| §9.4 selector (haiku → sonnet → opus) | 65,7 s | 71,1 s |

**Auditoría final:**

| Comprobación | Resultado | Duración |
| --- | --- | ---: |
| `cargo build --release` (ambos binarios) | OK | 79 s |
| `cargo test` | 349 OK, 0 fallos, 19 ignorados | — |
| `cargo clippy --all-targets -- -D warnings` | 0 avisos | — |
| `cargo fmt --check` | OK | — |
| `acceptance_e2e_etapa1_ingesta` (real) | 3/3 OK | 122 s |
| `e2e_binario_fase12` (real, binario) | 2/2 OK | 186 s |
| `e2e_binario_fase13` (real, binario) | 1/1 OK | 272 s |
| `acceptance_gui_chat_claude_headless` (real) | 1/1 OK | 19 s |
| `integration_claude_model_catalog` (real) | 1/1 OK | 8 s |
| `lakehouse_rag_cli.exe` release en PowerShell | `--centros` → 0; centro inexistente → 1; `--gui` → 2; salida en orden | — |

## 7. Prueba guiada de la ventana (checklist para el usuario)

Lanza la app en release (sin consola):

```powershell
& "D:\Proyectos\lakehouse RAG\target\release\lakehouse_rag.exe"
```

Usa un centro de prueba en una carpeta vacía, por ejemplo `D:\lh_fase15`. Los fixtures están en `D:\Proyectos\lakehouse RAG\tests\fixtures\`.

| # | Paso | Qué debe pasar | ✔/✘ |
| --- | --- | --- | --- |
| 1 | **Ventana:** minimiza, restaura, maximiza y vuelve a restaurar con los botones de la barra; arrastra la ventana por la barra de título | Responde sin parpadeos y no se abre ninguna consola | |
| 2 | **Redimensionado:** con el ratón, arrastra los dos separadores de paneles; pliega y despliega los paneles laterales; cierra y vuelve a abrir la app | Los anchos se respetan (220–520 px y 280–760 px) y se recuerdan | |
| 3 | **Tema:** alterna claro / oscuro | Todo legible en ambos temas | |
| 4 | **Crear centro:** «+ Nuevo» → id `obra-fase15` (un id que no exista ya), botón «Elegir…» (**diálogo nativo de carpetas**) → `D:\lh_fase15` → «Crear y activar» | El centro queda activo y la carpeta se crea con `raw_docs`, `lakehouse_data`, etc. | |
| 5 | **Ingesta por diálogo nativo:** pulsa la zona «Arrastra procedimientos aquí» y elige `procedimiento_recepcion_rev03.md` | Barra por etapas; al terminar, el catálogo muestra PO-ALM-03 Rev. 03 vigente | |
| 6 | **Ingesta por arrastre desde el Explorador:** arrastra `procedimiento_recepcion_rev04.md` a la zona | Rev. 04 vigente desde 2026-09-15 y Rev. 03 derogada | |
| 7 | **Pregunta (sonnet):** «¿Quién es el responsable de la custodia y qué registro llena?» | **Jefe de Producción**, **FOR-ALM-05**, cita PO-ALM-03 Rev. 04; nunca FOR-ALM-01 ni Jefe de Almacén | |
| 8 | **Cambio de modelo en caliente a haiku:** «Según la Tabla 4-A, ¿temperatura mínima para 30 mm y CE 0,42 %?» | **125 °C (\*)** con la nota (QA/QC + HV10); el Studio muestra la celda | |
| 9 | **Time travel (opus):** «¿Y cuál era el 1 de junio de 2026?» | **100 °C (\*)** (Rev. 03) | |
| 10 | **Grafo:** «Muéstrame el grafo de referencias de PO-ALM-03» | Pestaña Grafo con registros, normas y DEROGA_A | |
| 11 | **Eliminar centro:** crea otro centro `obra-borrar`, selecciónalo y pulsa el botón de quitar (papelera); prueba primero «No» y luego «Sí» | Con «No» no pasa nada. Con «Sí» desaparece de la lista, pero su carpeta **sigue en disco** | |
| 12 | **(Opcional) Sesión caducada:** solo si te viene bien cerrar sesión en Claude Code (`claude` → `/logout`) y hacer una pregunta; después, `/login` de nuevo | Mensaje «Claude Code no tiene una sesión válida … `/login` …» en lugar del JSON | |

Anota ✔/✘ y, si algo falla, una captura o el texto del aviso del registro «Actividad». Los fallos se corrigen con TDD antes de cerrar la fase.

## 7 bis. Documentos de demostración y chat en vivo (iteración tras la prueba guiada)

- **Demo con documentos nuevos:** `docs/validacion/fase15/demo/GUIA_DEMO.md`. Son 4 documentos de hormigonado y 8 preguntas con su respuesta esperada, validadas con Claude real: 8/8.
- **Chat en vivo:** la respuesta llega en streaming (`stream-json`). Las tools se ven mientras se ejecutan, hay botón **Detener** y el diseño sigue la referencia `informe_chat_referencia.md`: columna de 768 px, burbuja del usuario, composer que crece y scroll pegado al final con «↓». En la ventana real, el texto aparece de forma progresiva (unos 2.300 caracteres en 3 s), sin saltos de scroll. Capturas `docs/validacion/fase15/19`–`22`.

## 8. Recomendaciones para siguientes iteraciones

1. **PDF/DOCX** mediante un subproceso `docling` (plan §4), con OCR opcional y sin cambiar el contrato de ingesta, que trabaja por ruta.
2. **Rendimiento de la ingesta:**
   - Destilar por secciones en paralelo y cachear por hash de sección.
   - Usar `haiku` para documentos sin tablas.
   - Objetivo: menos de 20 s por documento.
3. **Rendimiento del chat:**
   - Llevar el catálogo de tablas en el system prompt para ahorrar el `inspect_lakehouse_catalog` previo (unos 10 s).
   - Respuestas en streaming en la ventana.
4. **Prelación por tema:** usar el código de requisito o un embedding del encabezado, en lugar del título literal, para agrupar temas con títulos distintos.
5. **Instalador:** `cargo tauri build` con los dos binarios (`lakehouse_rag.exe` y `lakehouse_rag_cli.exe`) y la comprobación de WebView2.
6. **Observabilidad:** un registro de cada invocación a Claude (modelo, duración y coste) visible en «Actividad».
7. **Disco:** `target/` llega a más de 30 GB; conviene limpiar `target/debug/incremental` de forma periódica.
