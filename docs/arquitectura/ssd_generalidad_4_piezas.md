# SSD: generalidad en 4 piezas (PDF, página de Capa 2, búsqueda y arnés)

**Estado: cerrado para aprobación del usuario. No hay código de producción.** Fecha: 11-11-2026. Base: `main` en `62f3010`.

Origen:
- [`docs/estado/situacion_rag_20261111.md`](../estado/situacion_rag_20261111.md) y [`auditoria_generalidad_20261111.md`](../estado/auditoria_generalidad_20261111.md);
- la primera decisión del mentor técnico externo: la rama `claude/arreglos-rag-512d66` no se integra; se descartan `negaciones.rs` y `apartado_tabla.rs`; solo se rescata el silencio de `verificador_paginas.rs`;
- **el resultado del debate con el mentor, vinculante:** pdfium en la Pieza 1, anclaje por ventana de tabla en la 2, protocolo JSON en la 3, verificador híbrido de procedencia en la 4 y tool use nativo como Pieza 5 futura;
- el criterio del usuario para la Pieza 1: **decidir por calidad y seguridad**; la distribución de la DLL no cuenta como inconveniente.

**Regla dura: generalidad absoluta.** Ninguna pieza añade listas de palabras, locuciones o regex de un idioma o dominio. Sí se admiten:
- propiedades de Unicode (`char::is_alphanumeric`, `is_lowercase`, rangos de la PUA y bloques de escritura);
- propiedades de los alfabetos (qué letras son vocales en el latín, el griego y el cirílico; §1.3);
- claves del protocolo de la propia app (`"tool_calls"`);
- tipografía.

Las líneas citadas son de `62f3010` y están comprobadas. Si difieren de la auditoría, vale la de aquí: por ejemplo, el `FinalMessage` de `claude_agent_runner.rs` está en la :143.

---

## 0. Resumen

| Pieza | Fallos del set v2 que ataca | Esquema | Riesgo de retroceso |
|---|---|---|---|
| 1. PDF con pdfium y compuerta por página | 4 (V50 y V51, en 2 pasadas) | v7 (estado nuevo y lista de páginas) | **Alto**: cambia el texto de todos los PDF |
| 2. Página exacta de la Capa 2 | Evita el retroceso de la rama (1 página falsa) y da página en tablas (hoy no hay) | **v7** (columnas de página) | Bajo: hoy el lookup no da página |
| 3. JSON embebido, cuota y verificador | 1 fuga en `main`; multidocumento (no concluyente); 3 o más avisos falsos | No | Medio en la cuota, bajo en el resto |
| 4. Verificador híbrido de procedencia | 4 (V22 y V53, en 2 pasadas) | No | **Alto**: protege las trampas y el «no consta» |
| 5. Tool use nativo por MCP | — | — | **Fuera de esta rama**: solo diseño futuro (§5) |

### Decisiones que tiene que aprobar el usuario

1. **D1. Pieza 1:** pdfium-render como extractor de texto, **en un proceso hijo aislado** (§1.2). Implica una dependencia nueva y una DLL en el bundle de Tauri. Se retira `pdf-extract`; `lopdf` se queda para abrir el PDF, detectar el cifrado y leer los marcadores.
2. **D2. Esquema v7:** **hay que recrear los centros y volver a ingerir**, una sola vez, con las piezas 1 y 2 integradas (§2.5).
3. **D3. Estado nuevo `LECTURA_DEGRADADA`** en las filas de las páginas degradadas, más la lista exacta de páginas en el catálogo (§1.4).
4. **D4. Streaming:** el detector de JSON embebido puede actuar después de que el texto se haya emitido en vivo. Propuesta: el evento `TextoEnVivoDescartado`, que conserva el streaming (§3b).
5. **D5. Pieza 4:** se elimina `PETICIONES_DE_REFERENCIA`. Una petición de «valor de referencia» que el documento no contiene ya no autoriza una cifra de memoria (§4.4).
6. **D6.** `LOCUCIONES_DE_ABSTENCION` **se mantiene de forma provisional** como disparador de `CifraEnAbstencion`. Solo puede endurecer y nunca aprueba nada. Su sustitución depende de la Pieza 5 (§4.4).

---

## 1. Pieza 1: PDF con pdfium-render y compuerta de legibilidad por página

### 1.1 Diagnóstico verificado

- **Extracción actual:** página a página con `pdf_extract::output_doc_page`, con `lopdf::extract_text` como respaldo (`pdf_parser.rs:207-232`).
- `requiere_ocr` solo cuenta caracteres no blancos (`pdf_estructura.rs:145`).
- `normalizar_pagina` **borra los caracteres de control** (`pdf_estructura.rs:116`). La compuerta debe medir el texto crudo.
- El documento queda COMPLETO en cuanto la destilación termina bien (`document_ingestion_service.rs:166`). Los avisos de extracción solo se añaden a la lista (`:184`).

**Defectos de `pdf-extract` 0.12.1, comprobados en su código fuente (`lib.rs`):**
- En las fuentes CFF (`FontFile3/Type1C`, `:406-434`), el mapa de caracteres sale de la **codificación interna** de la fuente. Las entradas de `/Differences` **no la sustituyen**: `Entry::Occupied` solo emite un `warn!` (`:496-505`). Es lo contrario de lo que dice la especificación PDF, y produce letras sustituidas en las fuentes subconjunto.
- Los nombres de glifo se buscan de forma exacta (`glyphnames`, privado, `:28`). No se aplica el algoritmo AGL (`uniXXXX`, sufijos, ligaduras). Un nombre desconocido conserva la letra de la codificación base, que es `PDFDocEncoding` si no hay `/BaseEncoding` (`:471-476`).

**Supuesto:** el informe agregado del set v2 describe el PDF de V50/V51 como «Type1 (CFF) con `/Differences` sin ToUnicode», lo que encaja con estos defectos. El PDF no se ha abierto: no se puede.

### 1.2 Decisión: pdfium-render, por calidad y seguridad

| Criterio | `pdf-extract` + ToUnicode propio | **pdfium-render** |
|---|---|---|
| Calidad de decodificación | Corrige solo el defecto diagnosticado | Motor de PDF de Chrome: precedencia de codificaciones, AGL, CID, Type3, xref dañadas |
| Coordenadas por carácter | No | Sí (banda `y` opcional en la Pieza 2) |
| Seguridad de memoria | Rust: un PDF hostil provoca pánicos, que ya se capturan | C++ sin seguridad de memoria. Muy fuzzeado (OSS-Fuzz), pero en Chrome corre dentro de un sandbox |
| Mantenimiento de seguridad | Crate con poca actividad | Parches al ritmo de Chromium |

**Elección: pdfium**, porque es más robusto, **con el riesgo de memoria contenido así:**
1. **Proceso hijo:** `lakehouse_rag_cli.exe --extraer-texto-pdf <ruta>` escribe por stdout un JSON con el texto crudo de cada página. El padre:
   - aplica el límite de tiempo actual (`TIEMPO_MAXIMO_EXTRACCION`, 300 s, `pdf_parser.rs:27`) y mata el proceso al agotarse;
   - en Windows, lo asocia a un *job object* con límite de memoria.

   Un PDF que explote un fallo de pdfium solo compromete ese proceso, que no tiene acceso al estado de la app. También resuelve que pdfium no admita llamadas concurrentes: hay un proceso por documento.
2. **Carga de la DLL** solo por **ruta absoluta**, junto al ejecutable o en los recursos del bundle, **nunca por la ruta de búsqueda del sistema** (evita el secuestro de DLL). Antes de cargarla se comprueba su **sha256**, fijado en el código.
3. **Versión fijada** de los binarios de pdfium (proyecto `pdfium-binaries`). Se actualiza cuando Chromium publica parches de seguridad de pdfium.

**Si la DLL falta o no valida:** la ingesta de PDF falla con un mensaje accionable. **No hay respaldo silencioso** a otro extractor, porque cambiaría el texto sin avisar. El respaldo por página con `lopdf::extract_text`, que ya existe, se mantiene solo para una página que pdfium no pueda leer.

**A fijar en el spike inicial de la pieza** (no verificado en esta sesión):
- versión de `pdfium-render`;
- API exacta del texto por página y de los límites por carácter;
- tamaño de la DLL;
- `bundle.resources` en `tauri.conf.json`.

### 1.3 Compuerta de legibilidad por página

**Módulo `infrastructure/parsers/legibilidad.rs`:** puro, sin I/O, y aplicado al texto **crudo** de cada página, antes de `normalizar_pagina`.

```rust
pub enum ClasePagina { Legible, Degradada { motivos: Vec<MotivoDegradacion> }, SinTexto }
pub enum MotivoDegradacion {
    ReemplazoOControl { proporcion: f32 },  // U+FFFD + C0/C1 sin \n \r \t
    UsoPrivado { proporcion: f32, dentro_de_palabra: usize },
    Mojibake { proporcion: f32, tokens_evaluados: usize },
}
pub struct MetricasPagina { /* no blancos, proporciones, densidad alfanumérica (solo informativa) */ }
pub fn clasificar_pagina(texto_crudo: &str) -> (ClasePagina, MetricasPagina);
```

| Señal | Se evalúa si | Umbral inicial | Rango de calibración |
|---|---|---|---|
| `U+FFFD` + controles C0/C1 (sin `\n`, `\r`, `\t`) / no blancos | ≥ 50 caracteres no blancos | > 1 % | 0,5-1 % |
| PUA (`U+E000..U+F8FF`) / no blancos | ≥ 50 caracteres no blancos | > 5 % | 5-10 % |
| PUA **dentro de una palabra** (entre dos alfanuméricos) | ≥ 50 caracteres no blancos | ≥ 3 apariciones | a calibrar |
| Mojibake (tokens sin vocal de su alfabeto o con puntuación ASCII anómala dentro) / tokens evaluados | ≥ 80 tokens evaluables | > 30 % | 30-35 % |
| Densidad alfanumérica (`char::is_alphanumeric()`) | Siempre | **Solo se informa**, no clasifica | Se decide tras la calibración |

`SinTexto` se aplica a una página con menos de 50 caracteres no blancos. **No bloquea:** se informa como aviso. La regla de documento entero `REQUIERE_OCR` (`pdf_estructura.rs:145`) no cambia.

**Red contra mojibake: definición estructural.**
- **Token evaluable:** se separa por blancos y se recorta la puntuación de los bordes. Es evaluable si:
  - todas sus letras están en minúscula (`char::is_lowercase`);
  - tiene 4 o más letras;
  - no tiene dígitos;
  - no tiene caracteres CJK (por bloques Han, Hiragana, Katakana y Hangul);
  - no tiene `@`, `/`, `:`, `_` ni `.` interior: así se excluyen correos, URL y códigos;
  - y todas sus letras son del **mismo alfabeto**: latín, griego o cirílico (por bloques Unicode).
- **Página no evaluable** para esta red: la que tiene menos de 80 tokens evaluables, como las de CJK, árabe, devanagari, tablas o fórmulas. Así se evita el falso positivo en esos casos.
- **Token sospechoso:** el que cumple alguna de estas condiciones:
  - a) no contiene ninguna **vocal de su alfabeto**:
    - latín: `a e i o u y` y sus variantes con diacrítico, plegadas con `quitar_diacritico` (`domain/services/normalizacion.rs`);
    - griego: `α ε η ι ο υ ω` con o sin tono;
    - cirílico: `а е ё и о у ы э ю я і ї є`;
  - b) lleva **puntuación ASCII dentro** de la palabra, distinta de `-` y `'`, como en `th#s` o `w;rd`.
- **Por qué no es una lista de palabras:** las vocales son una propiedad del **sistema de escritura**, no de ningún idioma. El conjunto es cerrado y finito por alfabeto, y vale igual para el español, el inglés, el alemán, el polaco, el ruso, el búlgaro o el griego.
  - No se comprueba ninguna palabra de ningún diccionario. Se mide una propiedad de forma: en un texto real casi todas las palabras de 4 o más letras tienen vocal, y en una sustitución de letras muchas la pierden.
  - Las lenguas con palabras sin vocal (checo o eslovaco `vlk`, `krk`) las tienen sobre todo de 3 letras o menos, y son minoría. Por eso el umbral es del 30 %.
  - Las escrituras sin vocales escritas o sin espacios no se evalúan con esta red.

**Prohibido** limitar la longitud de token: no existe esa señal, porque las escrituras CJK, tailandesa o jemer no usan espacios.

#### 1.3.1 Ajustes tras la calibración N1 (11-11-2026, con las decisiones del mentor)

Detalle y márgenes en `benchmarks/informes/pieza_1_20261111.md`.

- **Reemplazo y control:** un control **aislado entre blancos** no cuenta. Los delimitadores de una fuente matemática sin Unicode salen así, en su propia línea, y no dañan ninguna palabra. Un control que es blanco para Unicode (`U+000C`, `U+0085`) no aísla a otro.
- **`SimbolosEnPalabras` (señal nueva):** proporción de tokens con letras que tienen un símbolo anómalo **entre dos letras** (`$ @ # * ~ \ | { } [ ] ^ = + ; ? ! ( )`). Los conectores (`-`, `/`, `_`, `.`, `'`, `:`, `,`) no cuentan. Con al menos 40 tokens con letras:
  - por encima de 0,35, degrada sola;
  - entre 0,15 y 0,35 (zona gris), solo si la confirma otra señal: más del 15 % de palabras sin vocal de su alfabeto (con 20 o más evaluables) o el colapso de la coherencia.
- **`IncoherenciaEstructural` (señal nueva):** una página está degradada si las letras no forman palabras y, además, hay ruido.
  - **Ámbito:** 80 o más no blancos, 20 o más letras fuera de CJK, menos del 50 % de dígitos y menos del 30 % de CJK, Kana o Hangul.
  - **Coherencia:** proporción de las letras no CJK que están en rachas de 3 o más letras seguidas de una misma escritura. Colapsa por debajo de 0,40.
  - **Ruido:** el máximo entre los símbolos sueltos (los anteriores más `< > " % &`) y el ruido de Latin-1, sobre los no blancos, con un mínimo de 0,05. El ruido de Latin-1 son los signos `U+00A1..U+00BF` (salvo `°` y `±`), `×`, `÷` y las letras `U+00C0..U+00FF` sueltas (en tokens de 1 o 2 caracteres o pegadas a un símbolo).
  - **Por qué rachas y no tokens:** el mentor propuso tokens puramente alfabéticos. Así, una página de código fuente (`crate::domain::models`) colapsaba a 0,19 y se degradaba. Con rachas sube a 0,91.
  - Las marcas que no son letras (el virama índico) prolongan la racha si son de la misma escritura. La escritura se toma de los bloques del latín, el griego, el cirílico y el árabe; para el resto, de su bloque de 128 puntos de código.
- **Limitación conocida, aceptada:** en los PDF TeX antiguos, pdfium separa subíndices y superíndices (`r2` → `r 2`). La prosa no cambia. No se reconstruyen líneas.

### 1.4 Efecto en la ingesta y contratos

- **`ParsedDocument`** (`dtos/ingestion.rs:22`):
  - `+ #[serde(default)] paginas_degradadas: Vec<PaginaDegradada>`, con `pagina` y `motivos`;
  - `+ paginas_sin_texto: Vec<u32>`.
- **`EstadoContenido`** (`domain/models/estructura_documental.rs:19`): `+ LecturaDegradada` → `"LECTURA_DEGRADADA"`. Hay que tocar `VALORES_CANONICOS`, `as_str` y `parse`. `lancedb_store.rs:720` lo acepta a través de `parse`.
- **Mapeo:**
  - las filas de 02 (chunks) y 07 (nodos) cuyo rango de páginas corta una página degradada llevan `LECTURA_DEGRADADA`; el resto conserva su estado;
  - el informe de ingesta deja de ser COMPLETO si hay **1 o más** páginas degradadas.
- **Catálogo 01:** `+ paginas_degradadas: List<Int32>` (nulable). Es la lista exacta.
- **`aviso_contenido`:**
  - `explicacion_de_estado` (`lakehouse_tool_service/estructura.rs:121`) recibe la lista y redacta: «Páginas 2-11 con texto degradado (motivo). No concluyas que un dato no consta a partir de ellas».
  - Lo propagan ya `search_normative_context` (`busqueda.rs:308`), el subárbol (`estructura.rs:543`) y el catálogo (`catalogo.rs:60`).
- **Regla 7 del system prompt** (`agent_orchestrator.rs:88`): se añade `LECTURA_DEGRADADA`. Es una instrucción al modelo, no una lista del arnés.
- **Por qué no `PARCIAL_UNIVERSAL`:** ese estado significa «sin Capa 2» y lo usa la lógica de provisionales (`busqueda.rs:652`).

### 1.5 Archivos

- `Cargo.toml` (`pdfium-render`; se retira `pdf-extract`)
- `src/infrastructure/parsers/pdfium_extractor.rs` (nuevo: el lado del proceso hijo)
- `src/infrastructure/parsers/legibilidad.rs` (nuevo)
- `pdf_parser.rs` (lanza el hijo y clasifica)
- `src/presentation/cli/` (subcomando oculto `--extraer-texto-pdf`)
- `tauri.conf.json` (`bundle.resources`)
- `scripts/obtener_pdfium.ps1` (descarga la versión fijada y comprueba su sha256)
- `dtos/ingestion.rs`, `estructura_documental.rs`, `mapeo_destilacion.rs`, `document_ingestion_service.rs`
- `schemas.rs`, `lotes_arrow.rs`
- `lakehouse_tool_service/estructura.rs`, `catalogo.rs`, `busqueda.rs`
- `agent_orchestrator.rs`
- `tests/integration_pdf_parser.rs`, `tests/adversarial_pdf_tests.rs`, `tests/comun_pdf/mod.rs`

**Tests y DLL:** no hay CI en el repositorio (no existe `.github/`). Los tests de PDF **fallan con un mensaje claro** si falta la DLL («ejecuta `scripts/obtener_pdfium.ps1`»). No se marcan `#[ignore]`, para que no se salten en silencio. La ruta se puede fijar con la variable de entorno `LAKEHOUSE_RAG_PDFIUM`.

### 1.6 Plan TDD (datos sintéticos)

**Rojo, `clasificar_pagina`.** Texto escrito en el test; no hace falta pdfium:

| Caso | Contenido | Clase esperada |
|---|---|---|
| Prosa | ES, EN, DE, PL, CS (con `vlk`, `krk`), EL, RU, BG | `Legible` |
| Escrituras sin red de mojibake | JA y ZH sin espacios, AR, HI | `Legible` (red no evaluable) |
| Fórmulas | `∑`, `≤`, `α`, subíndices | `Legible` |
| Tabla financiera | Rejilla numérica con `│─` | `Legible` |
| Iconos | PUA por debajo del 2 %, fuera de palabras | `Legible` |
| Sustitución de letras | Prosa ES y EN cifrada por sustitución (sin vocales en más del 30 % de los tokens) | `Degradada(Mojibake)` |
| Puntuación interior | `th#s w;rd` en más del 30 % de los tokens | `Degradada(Mojibake)` |
| Reemplazo y control | 2 % de `U+FFFD`; 2 % de C1 | `Degradada(ReemplazoOControl)` |
| PUA | 8 %; PUA dentro de 5 palabras | `Degradada(UsoPrivado)` |
| Página corta | 30 caracteres | `SinTexto` |
| Correos, URL y códigos | `abcd@xyzw.qrst`, `pqrs/tvwx` | No cuentan como tokens evaluables |

**Rojo, integración con pdfium.** PDF generados con `tests/comun_pdf`:
- texto WinAnsi;
- `/Differences` con `uni0394`, `afii10017` y `f_i`, que dan «Δ», «А» y «fi»;
- una fuente con ToUnicode.

Los resultados deben coincidir con el texto esperado. **Prueba diferencial del defecto CFF:** se construye en el test un CFF mínimo cuya codificación interna contradice `/Differences`; pdfium debe dar las letras de `/Differences`. Es el trabajo de test más largo, unas 150 líneas.

**Rojo, ingesta:**
- un PDF de 3 páginas con la p. 2 degradada da el informe `LECTURA_DEGRADADA`, `paginas_degradadas = [2]`, el chunk de la p. 2 marcado y los de las pp. 1 y 3 en `COMPLETO`;
- `aviso_contenido` cita la p. 2 en la búsqueda, el subárbol y el catálogo.

**Adversarial y de seguridad:**
- el proceso hijo se cuelga: se mata al agotarse el tiempo y la ingesta falla con un mensaje;
- el hijo muere abruptamente: error controlado, sin pánico en el padre;
- falta la DLL o su sha256 no coincide: error accionable;
- una DLL falsa en la ruta de búsqueda no se carga;
- un PDF de 10 000 páginas o con un stream gigante: límite de memoria del *job object*;
- se mantienen los casos de PDF cifrado y corrupto.

### 1.7 Riesgos de retroceso y medición

**El texto de todos los PDF cambia,** así que el criterio de «texto idéntico» no sirve. Se compara la calidad:

| Riesgo | Medición (sin Claude, §6.2) | Criterio |
|---|---|---|
| Cambian los apartados que detecta `pdf_estructura` (numeración, capítulos, marcadores) | Número y títulos de las secciones por PDF del corpus público, antes y después | Ningún PDF pierde secciones. Cada diferencia se revisa a mano |
| Cambian los saltos de página | Página de las agujas de Aguja; página de 20 frases por PDF localizadas con `pypdf` | 100 % iguales |
| Falsos degradados | Clase por página en el lote de control legible | **0 páginas degradadas** en los PDF validados como legibles |
| El PDF académico sigue mal | Clase por página en un PDF LaTeX público con Type1/CFF | Legible con pdfium |
| Ingesta más lenta (proceso hijo) | Tiempo de parseo por documento, 3 ejecuciones | Informado; el equipo tiene carga |
| Funcionamiento con el modelo | Aguja 5 × 3 | `recall_aguja`, `pagina_exacta_estricta` y `seccion_real` iguales a la línea base (1,00) |

**Calibración de umbrales:** sin Claude, con el lote de control (§6.2) y nuestros PDF públicos. Los 15 PDF del set oculto los calibra **la sesión evaluadora**, no esta.

---

## 2. Pieza 2: página exacta de la Capa 2 por ventana de tabla

### 2.1 Estado verificado

- El parser de PDF **no extrae tablas** (`pdf_parser.rs:142`). Las tablas salen de `TablaDestilada` (`dtos/ingestion.rs:170`), que se **mantiene**.
- `vincular_tablas` (`mapeo_destilacion.rs:1236-1262`) vincula las tablas por la sección declarada, la del parser y el literal **español** `"Tabla {etiqueta}"` (`:1252`).
- `titulo_con_etiqueta` (`:1077`) compone también `"Tabla {etiqueta}"` en el título que ve el modelo.
- Los esquemas 04 (`schemas.rs:177`) y 05 (`:206`) no tienen columnas de página. `lookup.rs` no da página.
- Los ejes de una celda (`LimitesDestilados`, `dtos/ingestion.rs:143`) son una `categoria` de texto o un rango `min`/`max` numérico.

### 2.2 Definiciones

- **Texto paginado:** cada `ParsedSection.contenido` con sus `saltos_pagina` (`dtos/ingestion.rs:58`), mediante `texto_con_marcas` y `tramos_paginados` (`domain/services/marcas_pagina.rs:101` y `:126`). Da una secuencia de líneas con su página. En Markdown no hay páginas, así que todo queda en `None`.
- **Normalización:**
  - `plegar_mayusculas` (`normalizacion.rs:11`);
  - tokens alfanuméricos;
  - en las escrituras sin espacios, bigramas de caracteres;
  - una cifra admite su texto y su variante con la coma y el punto decimal intercambiados.
- **Cabeceras de columna:** los textos de categoría distintos del eje x. Si el eje x es de rangos, sus extremos.
- **Clave de fila:** la categoría del eje y, o sus extremos si es un rango, en el orden de la tabla destilada.
- **Valores de una fila:** los `valor_resultado` de sus celdas.
- **«Aparece»:** la secuencia completa de tokens de la clave está en el texto.

### 2.3 Algoritmo (`domain/services/anclaje_tabla.rs`, puro)

```rust
pub struct LineaPaginada<'a> { pub pagina: u32, pub texto: &'a str }
pub struct TablaParaAnclar { pub cabeceras: Vec<String>, pub filas: Vec<FilaParaAnclar> }
pub struct FilaParaAnclar { pub clave: String, pub valores: Vec<String> }
pub struct AnclajeTabla { pub bloque: Option<(u32, u32)>, pub por_fila: Vec<Option<u32>> }
pub fn anclar_tabla(lineas: &[LineaPaginada], tabla: &TablaParaAnclar) -> AnclajeTabla;
```

**1. Bloque físico `[p_ini, p_fin]`.** Para cada ventana de 1 a `MAX_PAGINAS_BLOQUE` (4) páginas consecutivas se calcula:
- `C`: el número de cabeceras distintas presentes en la ventana;
- `L`: la longitud de la **subsecuencia creciente más larga** de las claves de fila, por su orden en la tabla, según la posición de su primera aparición en el texto de la ventana.

La ventana elegida es la que maximiza `(L, C)` y, a igualdad, la **más corta**. **Es inequívoca** si se cumplen tres condiciones:
- `L ≥ max(2, ⌈filas/2⌉)`;
- `C ≥ max(1, ⌈cabeceras/2⌉)`;
- ninguna otra ventana disjunta alcanza el mismo `(L, C)`.

Si no lo es, `bloque = None` y todas las filas quedan en `None`.

**2. Página de cada fila:**
- `p_ini == p_fin`: todas las filas llevan esa página.
- `p_ini < p_fin`: se buscan dentro del bloque las apariciones de la clave en las que **al menos 2 valores distintos de esa fila** aparecen a ±2 líneas.
  - Si caen en una única página, esa es la de la fila.
  - Si hay varias, se desempata por **monotonía**: la página debe ser mayor o igual que la de la fila anclada anterior y menor o igual que la de la siguiente.
  - Si sigue habiendo empate, o no hay ninguna aparición, la fila queda en `None`.

**3. Nunca** se recurre a la sección padre, a `seccion_origen` ni a un número suelto: fuera del bloque no hay búsqueda, y dentro se exige clave más 2 valores.

**4. Señal opcional:** la banda `y` de pdfium se deja para una versión posterior. Exigiría guardar coordenadas en `ParsedDocument`, y eso complica el contrato de ingesta. **No entra.**

### 2.4 Vinculación y literales

- Se elimina `"Tabla {etiqueta}"` (`:1252`). La tabla se vincula al chunk:
  - por su sección declarada o la del parser, como hoy;
  - por **el chunk cuyo rango de páginas contiene el bloque** anclado;
  - por la **etiqueta distintiva como secuencia completa de tokens**. Es distintiva si mezcla letras y dígitos, o tiene un separador entre alfanuméricos («4-A», «A.2»). Una etiqueta numérica pura («3») no vincula por cita.
- `titulo_con_etiqueta` (`:1077`): el rótulo pasa a ser `"{etiqueta} — {título}"`, sin la palabra «Tabla».

### 2.5 Contratos y esquema v7

- `FilaTabla` (`dtos/lakehouse_filas.rs:113`): `+ paginas: Option<(u32, u32)>`, el bloque.
- `FilaCelda` (`:136`): `+ pagina: Option<u32>`.
- **04:** `+ pagina_inicio`, `+ pagina_fin` (Int32, nulables). **05:** `+ pagina` (Int32, nulable).
- **`lookup_double_entry_table`:** `cita.paginas` con la página de la celda o, si no la tiene, el bloque. Usa el mismo formato que la búsqueda (`"21"` o `"21-22"`). Si no hay ninguna de las dos, la clave no aparece.
- **`VERSION_ESQUEMA_CENTRO` 6 → 7** (`dtos/centros.rs:13`). Un centro v6 se rechaza al abrirlo (`:71`) y no hay migración.

> **El usuario tendrá que recrear sus centros y volver a ingerir los documentos.** Los centros anteriores no se tocan. La v7 incluye también los cambios de la Pieza 1 (`LECTURA_DEGRADADA` y `paginas_degradadas`): **se recrean una sola vez**, con las dos piezas integradas.

### 2.6 Archivos

- `domain/services/anclaje_tabla.rs` (nuevo)
- `mapeo_destilacion.rs`: `vincular_tablas`, `fila_tabla`, celdas y `titulo_con_etiqueta`
- `dtos/lakehouse_filas.rs`, `schemas.rs`, `lotes_arrow.rs`, `dtos/centros.rs`
- `lakehouse_tool_service/lookup.rs`, `dtos/tools.rs`
- `tests/integration_tools_v2.rs`, `tests/adversarial_tools_v2_tests.rs`

### 2.7 Plan TDD (datos sintéticos)

**Unitarios de `anclar_tabla`.** Líneas paginadas escritas en el test:
1. Tabla en una página: bloque `(21, 21)` y todas las filas en la p. 21. Una variante por idioma: ES (material y temperatura), EN (ingresos por trimestre), DE (dosis por peso) y EL.
2. **Una introducción en la p. 6 menciona un valor y una clave de la tabla, que está en las pp. 21-22:** el bloque es `(21, 22)` y ninguna fila cae en la p. 6. Es el defecto de la rama, reproducido con datos inventados.
3. Tabla partida: las filas 1-5 en la p. 21 y la 6-9 en la 22, cada fila en su página.
4. Una fila cuya clave se repite en las pp. 21 y 22 y solo tiene 1 valor cerca: se resuelve por monotonía entre sus vecinas; si las vecinas no lo deciden, `None`.
5. Dos ventanas con el mismo `(L, C)`, como una tabla duplicada en un anexo: `bloque = None`.
6. Claves en desorden en el texto (columnas leídas al revés): la subsecuencia creciente más corta no alcanza el mínimo, y queda `None`.
7. CJK sin espacios: anclaje por bigramas.
8. Markdown: `None`.

**Vinculación:**
- «Table 4-A» en un chunk inglés y «Таблица 4-A» en uno ruso vinculan;
- «Tabla 3», con una etiqueta numérica pura, no vincula por cita.

**Integración:**
- `MockDistiller` y un PDF sintético de 3 páginas: el lookup devuelve `cita.paginas` correcto;
- un centro v6 se rechaza.

**Adversarial:**
- 5000 celdas;
- claves vacías;
- valores de 10 000 caracteres;
- saltos corruptos;
- 1000 páginas (coste lineal en ventanas por páginas).

### 2.8 Riesgos y medición

| Riesgo | Medición (§6.3) | Criterio |
|---|---|---|
| Página falsa | Precisión del anclaje frente a un oráculo `pypdf` en los PDF públicos con tablas | **0 páginas falsas** |
| Menos vínculos tabla-chunk | `tablas_vinculadas` por documento en los fixtures | La recuperación de tablas no baja en RGB ni en `acceptance_8_problemas` |
| Poca cobertura | Porcentaje de filas con página | Se informa; es preferible `None` a una página falsa |

---

## 3. Pieza 3: JSON embebido, cuota por documento y verificador de páginas

Se mantiene el protocolo JSON por texto. Hay 3 subpiezas independientes: **3a**, **3b** y **3c**.

### 3a. Cuota por documento en `fusionar`

**Estado:** `fusionar` (`busqueda.rs:147-189`) puntúa todos los fragmentos elegibles y aplica `truncate(top_k)` (`:187`). `top_k` vale 8 por defecto (`lakehouse_tool_service.rs:68`) y 20 como máximo (`busqueda.rs:31`). La llamada está en `:500`.

**Diseño: cuota con relleno.** Se aplica solo sin filtro de documento (`doc_pedido.is_none()`, `busqueda.rs:423`):
1. El pool es el ranking RRF completo, que ya se calcula.
2. `max_por_documento = top_k − min(2, documentos_en_pool − 1)`: 6 de 8.
3. **Primera pasada:** se acepta cada fragmento en orden de ranking si su documento no ha llenado la cuota.
4. **Relleno:** los huecos que queden se completan con los descartados, por ranking.
5. Salida ordenada por score.

```rust
pub(crate) enum CuotaDocumento { Ninguna, Diversificar { reservados: usize } }
pub(crate) fn fusionar<'a>(…, top_k: usize, cuota: CuotaDocumento) -> Vec<Puntuado<'a>>;
```

**Cómo no empeora las preguntas de un solo documento:**
- con un único documento en el pool, la salida es **idéntica** a la actual;
- el documento dominante conserva sus 6 mejores fragmentos;
- con `doc_id`, no hay cuota.

**TDD:** con fragmentos en ES, EN y FR:
1. Un documento: la salida es igual a la actual (propiedad comprobada con 50 rankings con semilla fija).
2. A domina y B existe: 6 de A y 2 de B.
3. B sin candidatos: 8 de A.
4. Con `doc_id`: sin cuota.
5. `top_k = 1`: el mejor fragmento.

**Criterio de no retroceso:** el recall@8 de las preguntas de un solo documento no baja (§6.4).

### 3b. Guarda estructural en `parsear_salida_claude`

**Estado verificado:**
- `parsear_salida_claude` (`claude_agent_runner.rs:80-144`) solo reconoce la salida **entera** como JSON; si no lo consigue, devuelve `FinalMessage` (`:143`).
- El streaming decide si el texto es visible **por su primer carácter** (`stream_json.rs:80`).
- Con `IntencionTurno::Ninguna`, el guard no revisa nada (`post_inference_guard.rs:191`) y el texto no se retiene (`pre_inference_interceptor.rs:37`).
- El spike de la Fase 0 ya registró la fuga: en `arnes_fase0/bateria.jsonl`, caso `pregunta_ajena`, la respuesta fue prosa con un bloque ```json que contenía `{"tool_calls":…}`.

**Diseño** (`application/use_cases/arnes_turno/llamada_en_crudo.rs`, puro):

```rust
pub enum LlamadaEmbebida { Completa(Vec<ToolCall>), Parcial }
/// Recorre cada '{' con serde_json::Deserializer y acepta un objeto con "tool_calls" (array) o
/// "type": "tool_use". Si no hay ninguno completo, da `Parcial` cuando aparece la clave del
/// protocolo seguida de ':' (también dentro de una valla ```).
pub fn detectar(texto: &str) -> Option<LlamadaEmbebida>;
```

1. **Runner:** antes del `FinalMessage` de `:143`:
   - `Completa` → `ToolCalls`. Se descarta la prosa y se ejecuta lo que el modelo pedía. Un nombre fuera del catálogo da un error de tool, como hoy.
   - `Parcial` → `FinalMessage`, que el guard rechaza en el paso 2.
2. **Guard:** el invariante `ViolacionArnes::LlamadaEnCrudo` se comprueba **antes** del cortocircuito de `Ninguna`, así que vale en todas las intenciones. Provoca un reintento con «responde solo con el JSON del protocolo o solo con texto» y, si se repite, salida segura.
3. **Streaming (D4):** si ya se emitió texto en vivo y luego se rechaza o se convierte en llamada, se emite `AgentExecutionEvent::TextoEnVivoDescartado`. La GUI (`web/app.js`, rama `arnes`, `:673`) vacía la burbuja.

**Archivos:**
- `arnes_turno/llamada_en_crudo.rs` (nuevo)
- `claude_agent_runner.rs`
- `post_inference_guard.rs` (solo este invariante)
- `agent_orchestrator.rs`, `dtos/agent.rs`
- `presentation/gui/chat_service.rs`, `web/app.js`
- `tests/integration_chat_arnes.rs`

**TDD:**
1. Prosa en ES, EN y JA seguida de una valla ```json completa → `ToolCalls`.
2. JSON truncado → `LlamadaEnCrudo`.
3. Dos bloques → dos llamadas.
4. Una respuesta que muestra un JSON sin las claves del protocolo (un ejemplo de configuración) → `FinalMessage`.
5. Con intención `Ninguna`, el invariante actúa.
6. En vivo: `TextoEnVivoDescartado` llega antes del texto nuevo.

**Criterio:** 0 detecciones al repuntuar las respuestas correctas guardadas (§6.5).

### 3c. Verificador de páginas

**Estado verificado:**
- `motivo()` (`verificador_paginas.rs:55`) da nota en los 4 estados que no son `Confirmada`.
- La excepción de título o breadcrumb (`:106-111`) solo actúa con `NoEncontrada`.
- `comprobar` (`:145-179`) da `Discrepante` aunque la página citada sea un rango que contiene la aparición.

**Diseño:**
1. **Rescate de la rama**, y solo esto:
   - a) `motivo()` da `Some` **solo** con `Discrepante`; `Ambigua`, `SinPagina` y `NoEncontrada` quedan en silencio;
   - b) `Confirmada` si alguna aparición **corta** las páginas citadas.

   No se rescata `test_el_contexto_del_apartado_de_un_cruce_es_fuente_con_paginas`, porque depende de `apartado_tabla.rs`.
2. **Excepciones antes de emitir `Discrepante`:**
   - a) **título o metadato:** si el esqueleto del extracto está contenido en algún metadato del turno (`registro.metadatos()`: título y breadcrumb), no se anota;
   - b) **cabecera:** si **todas** las apariciones empiezan justo tras `⟦p. N⟧` y el extracto no supera una línea de la fuente, es un título o una cabecera de página y no se anota. `FuentePaginada` guarda los inicios de página y los finales de línea.
3. **Deuda:** `PALABRAS_DE_PAGINA` (`:29`, español e inglés) decide qué citas se revisan. No bloquea, así que no entra aquí.

**TDD:** con datos en EN, ES y RU:
- los 4 tests de la rama con texto sintético;
- nuevo: el título de la portada (p. 1) citado como p. 22 → sin nota;
- nuevo: una cabecera al inicio de la p. 7 citada como p. 9 → sin nota;
- se mantiene: un extracto del cuerpo citado con una página ajena → `Discrepante` con nota.

**Criterio:** los `Discrepante` verdaderos de los tests se mantienen. Al repuntuar (§6.5), las notas eliminadas no eran errores reales (revisión manual). Aguja: 0 notas nuevas.

---

## 4. Pieza 4: verificador híbrido de procedencia

### 4.1 Estado verificado

- **`PostInferenceGuard::validar`** (`post_inference_guard.rs:186-207`):
  - `CifraSinAnclar` solo con intención tabular o de responsable;
  - `CifraEnAbstencion` depende de `LOCUCIONES_DE_ABSTENCION` (`:18-51`, en español) y queda exenta con `PETICIONES_DE_REFERENCIA` (`:53-72`).
- Hoy una cifra anclada pasa aunque sea de otra entidad (`:232-233`).
- El anclaje literal usa `extraer_cifras` y `lecturas_de_fuente` (`cifras.rs`) sobre la pregunta, las salidas de las tools y la fecha de hoy (`:294-308`).

### 4.2 Principio

**El LLM nunca aprueba nada: solo propone una traza de procedencia. Rust la comprueba sobre los bytes reales de `RegistroTurno` y de la pregunta.** Lo que Rust no puede comprobar no pasa.

```text
borrador ── filtro literal en Rust (el anclaje de hoy) ─ todo anclado ──► sigue el guard actual (0 ms)
                │ alguna cifra sin anclar, o abstención con cifras que no son de la pregunta
                ▼
           Haiku: --tools "" --json-schema estricto; payload = pregunta + salidas del turno + borrador
                ▼
           Rust comprueba cada traza ─ todas válidas ──► la cifra pasa
                │ alguna inválida, timeout o error
                ▼
           comportamiento actual: CifraSinAnclar / CifraEnAbstencion → reintento → salida segura
```

### 4.3 Esquema JSON cerrado y comprobación en Rust

```rust
// application/ports/verificador_procedencia.rs
#[async_trait]
pub trait VerificadorProcedenciaPort: Send + Sync {
    async fn proponer_trazas(&self, solicitud: &SolicitudProcedencia) -> Result<TrazasPropuestas, DomainError>;
}
pub struct SolicitudProcedencia { pub pregunta: String, pub salidas: Vec<SalidaTurno>, pub borrador: String, pub cifras: Vec<String> }
pub struct SalidaTurno { pub id: String /* "t{i}" */, pub herramienta: String, pub texto: String }
pub struct TrazasPropuestas { pub trazas: Vec<Traza> }
pub struct Traza { pub cifra: String, pub tipo: TipoTraza }
pub enum TipoTraza {
    FormatoOEscala { fuente: Literal, factor_potencia_diez: i8 },   // 0,42 = 0.42 (0); 1500 mm = 1.5 m (-3)
    Aritmetica { operacion: Operacion, operandos: Vec<Literal> },    // ≤ 6 operandos
    ReferenciaEstructural { subcadena: Literal },
    ConteoElementos { elementos: Vec<Literal> },
}
pub enum Operacion { Suma, Resta, Producto, Division, Porcentaje }
pub struct Literal { pub origen: String /* id de salida o "pregunta" */, pub texto: String }
```

**Comprobación en Rust** (`arnes_turno/procedencia.rs`, puro). Para todos los tipos, cada `Literal.texto` debe ser una **subcadena exacta** del `origen` citado: en el texto de la salida tal como se registró (`recorrer_textos`, sin marcas `⟦p. N⟧`) o en la pregunta.

| Tipo | Comprobación determinista |
|---|---|
| `formato_o_escala` | El valor de `fuente`, leído con las lecturas canónicas de `cifras.rs`, por `10^factor` (con `factor` entre −9 y 9) es igual al valor de la cifra |
| `aritmetica` | Se recalcula la operación con los valores de los operandos. El resultado coincide con la cifra con una tolerancia de **±0,5·10^-d**, siendo `d` los decimales que muestra la cifra. Porcentaje = `a/b·100` |
| `referencia_estructural` | La subcadena existe y **contiene la secuencia de dígitos** de la cifra (sin separadores): «Art. 21» respalda «artículo 21» |
| `conteo_elementos` | Cada elemento existe. Los elementos son distintos tras normalizarlos (esqueleto). Su número es igual a la cifra |

- Sin traza, con una traza que no valida o con un tipo fuera del esquema: la cifra no pasa.
- **Límite reconocido:** una cifra escrita con palabras en la respuesta («doce») no tiene tipo en el esquema cerrado. Sigue el comportamiento actual.
- **Por qué no se buscan combinaciones aritméticas a ciegas:** con unas 200 cifras en las salidas, casi cualquier número saldría como `a − b` de dos de ellas, y se colarían cifras de memoria. La operación la declara el modelo y sus operandos deben estar en el origen que cita.

### 4.4 `CifraEnAbstencion`

- **En una abstención** siguen prohibidas **las cifras de parámetros, aunque estén ancladas en otro documento recuperado.** Esto endurece la regla actual (`:232-233`).
- Solo se permiten las cifras de la pregunta y las `referencia_estructural` verificadas. En una abstención no se admiten `aritmetica`, `formato_o_escala` ni `conteo_elementos`.
- **Disparador (D6):** sigue siendo `LOCUCIONES_DE_ABSTENCION`, **de forma provisional**.
  - Es la única lista léxica que queda en este camino. Solo endurece: nunca aprueba nada.
  - Limitación declarada, la misma que hoy: una abstención redactada en otro idioma no activa este endurecimiento. Las cifras sin anclar siguen pasando por el verificador.
  - La detección general de la abstención depende de la Pieza 5. No se añade ninguna lista nueva.
- **Se elimina `PETICIONES_DE_REFERENCIA` (D5).** Pedir un «valor de referencia» que el documento no contiene ya no autoriza una cifra de memoria.

### 4.5 Listas tras la Pieza 4

| Lista | Destino |
|---|---|
| `PETICIONES_DE_REFERENCIA` | Se elimina |
| `LOCUCIONES_DE_ABSTENCION`, `LOCUCIONES_DE_RECHAZO`, `IDENTIFICADORES_DE_ABSTENCION` | Se mantienen de forma provisional, como disparador que solo endurece (D6) |
| `PALABRAS_DE_ENLACE`, `UNIDADES`, `PALABRAS_SIN_CIFRA`, `MESES` | Sin cambios en esta pieza. Sus falsos negativos ya no bloquean: llegan al verificador como cifras sin anclar. Su sustitución es deuda declarada |
| `intent_detector.rs` | Sin cambios. Su sustitución por directivas va con la Pieza 5 |

### 4.6 Adaptador, modelo y latencia

- **`infrastructure/llm/claude_headless_procedencia.rs`:** `claude -p --model <haiku> --tools "" --no-session-persistence --strict-mcp-config --json-schema <esquema>`, con un system prompt fijo y corto.
  - Reutiliza `ejecutar_cli` y la lectura de `structured_output` de los turnos dirigidos (`claude_agent_runner.rs:478`).
  - **Payload mínimo:** pregunta, salidas de las tools del turno y borrador. Sin historial.
- **Composición** en `bootstrap.rs`. En los tests, un `VerificadorProcedenciaFalso`.
- **Latencia:**
  - 0 ms si el filtro literal lo resuelve, el caso habitual;
  - entre 3 y 6 s cuando hace falta la microllamada (objetivo del mentor). **No está medido:** en el spike de la Fase 0, Haiku con `--json-schema` tardó entre 6,0 y 9,3 s de reloj, con valores atípicos de 16 a 22 s (`arnes_fase0/bateria.jsonl`; equipo con carga).
- **Timeout de 10 s.** Si hay timeout o error, el comportamiento actual no cambia: la cifra cuenta como no anclada. **Falla cerrada:** las trampas no dependen de que Haiku responda.
- **Como máximo, 1 microllamada por borrador:** se agrupan todas las cifras.

### 4.7 Las 8 trampas y los 9 «no consta» siguen deterministas

| Caso | Resultado |
|---|---|
| Trampa respondida con el valor del documento | Anclada: 0 ms, como hoy |
| Cifra de memoria | Sin traza comprobable → comportamiento actual (reintento y salida segura) |
| «No consta» sin cifras | No se llama a Haiku, como hoy |
| «No consta» + valor de referencia | Rechazada, ahora también si se pidió un «valor de referencia» (D5) |
| «No consta» + cifra anclada de otro documento | **Rechazada (nuevo):** antes pasaba |
| Cálculo correcto a partir de dos celdas (V53) | `aritmetica` comprobada: pasa sin reintento |
| Número de artículo cuya subcadena está en las salidas (V22, supuesto) | `referencia_estructural`: pasa. Si el número sale de memoria, se sigue rechazando |

### 4.8 Archivos

- `application/ports/verificador_procedencia.rs` (nuevo)
- `arnes_turno/procedencia.rs` (nuevo)
- `post_inference_guard.rs`
- `agent_orchestrator.rs` (el guard pasa a ser asíncrono en el punto de la microllamada)
- `infrastructure/llm/claude_headless_procedencia.rs` (nuevo)
- `bootstrap.rs`
- `presentation/gui/intervenciones_arnes.rs` (etapa `VerificacionProcedencia` en la telemetría)
- `tests/adversarial_arnes_tests.rs`, `tests/integration_chat_arnes.rs`

### 4.9 Plan TDD

**Rojo, `procedencia.rs`:**
1. Cada tipo con un caso válido:
   - `0,42` frente a `0.42`;
   - `1500` mm a `1.5` m;
   - resta de dos celdas de una tabla financiera en EN, mostrada con 1 decimal;
   - «Art. 21» en ES y «§ 21» en DE;
   - un conteo de 3 ítems en griego.
2. Cada tipo con un caso inválido:
   - un operando que no está en su origen;
   - un origen inexistente;
   - un resultado fuera de la tolerancia;
   - un factor de 7,3, que no es potencia de 10;
   - una subcadena sin los dígitos;
   - un conteo con ítems duplicados o con un número distinto de elementos.
3. Traza para una cifra que no está en el borrador: se ignora.
4. Más de 6 operandos: inválida.

**Rojo, en el orquestador** con el falso verificador:
1. Trampa: documento 412 y respuesta 400. El falso devuelve una `aritmetica` con operandos inventados → Rust la rechaza y hay reintento.
2. Cálculo válido → se entrega sin intervención.
3. Timeout o error del verificador → comportamiento actual.
4. «No consta» + cifra anclada de otro documento → `CifraEnAbstencion`.
5. «No consta» + artículo verificado → pasa.
6. Todo anclado → 0 llamadas al verificador (contador del falso).

**Regresión:** la suite actual de `post_inference_guard` (`:515-1508`) sigue en verde. Los tests que cambian por D5 o por el endurecimiento de la abstención se reescriben y se justifican uno a uno en el PR.

### 4.10 Riesgos y medición

| Riesgo | Medición | Criterio |
|---|---|---|
| Se cuela una cifra de memoria | Trampas sintéticas y `acceptance_8_problemas` | 100 % rechazadas |
| Abstenciones legítimas rechazadas por el endurecimiento | Repuntuar las respuestas guardadas: abstenciones con cifras ancladas de otro documento (§6.5) | Se informa antes de implementar y se revisa cada caso |
| Frecuencia y latencia de la microllamada | Repuntuar: turnos con cifras sin anclar. Después, 3 o más ejecuciones reales | Se informa |
| Retroceso en «no consta» | Preguntas sin respuesta de los benchmarks propios | Ninguna cifra en una abstención |

---

## 5. Pieza 5 (fuera de esta rama): tool use nativo por MCP

**Solo diseño futuro.** Se hará después de validar las piezas 1 a 4.

- **Un único `claude -p` por turno** con `--mcp-config`, en lugar de un proceso por paso con el protocolo JSON (`claude_agent_runner.rs:156-192`).
- **Servidor MCP dentro de `LakehouseApp`,** ligado al turno (hoy `infrastructure/mcp/mcp_server.rs` solo sirve a clientes externos). Cada llamada alimenta `RegistroTurno` y el Studio.
- **stream-json** con retención del texto final hasta que pasa el guard, y **un reintento**.
- **`PreInferenceInterceptor` solo con directivas.** El guard no depende de la intención léxica.
- **Spike previo en Windows:**
  - los flags de `--mcp-config` y `--strict-mcp-config`;
  - los permisos no interactivos de las tools MCP (`--allowedTools` o equivalente);
  - el transporte stdio o local dentro del proceso de la app;
  - la latencia frente al protocolo actual.
- Hace innecesaria la Pieza 3b y abre la sustitución de `LOCUCIONES_DE_ABSTENCION` (D6).

---

## 6. Protocolo de validación rápida

### 6.1 Niveles

Todo se mide primero **sin inferencia de Claude**, con nuestros datos. **Esta sesión no ha leído `docs/validacion/set_oculto*`.**

| Nivel | Coste de Claude | Qué mide | Piezas |
|---|---|---|---|
| N0 | 0 | `cargo test`, `clippy --all-targets -D warnings`, `fmt --check` | Todas |
| N1 | 0 | Extracción y compuerta por página, en el lote de control y el corpus público | 1 |
| N2 | 1 ingesta por documento, una sola vez | Anclaje de tablas, repetido sin coste | 2 |
| N3 | 0 | Recuperación pura por MCP | 3a |
| N4 | 0 | Repuntuar respuestas guardadas (`--ask --json` guarda las tools, `cli/salida.rs:159-175`) | 3b, 3c y 4 |
| N5 | Aguja 5 × 3 (~78 min) | Con el modelo | 1, 3c y 4 |
| N6 | Sesión evaluadora | Subconjunto del set oculto (§6.7) | Al final |

### 6.2 N1: lote de control y corpus público

- **Lote de control:** griego, cirílico, CJK, un paper LaTeX con fórmulas (Type1/CFF), una tabla financiera numérica y MIL-STD-882E (`aguja_base`).
- **Corpus público:** de 12 a 20 PDF, más árabe, devanagari, iconos en la PUA y un escaneado.
- **Almacenamiento:** en `.cache/datasets/legibilidad/`, registrados con su sha256 en `benchmarks/manifiesto_datasets.json` y **sin versionar su texto**.
- **Exclusión:** se pide a la sesión evaluadora la lista de **sha256** del corpus del set oculto, sin nombres, y se sustituye cualquier PDF que coincida.
- **Herramienta:** `cargo run --example medir_extraccion -- <carpeta>`. Por cada PDF y página da la clase, las métricas, los títulos de sección detectados y 20 frases de muestra con su página, en JSONL y en segundos.
- **Comparación antes y después:**
  - las secciones;
  - la página de cada frase, frente a `pypdf`;
  - las clases.
- **Calibración:** los umbrales se calibran **aquí** y nunca con el set oculto.

### 6.3 N2: anclaje de tablas

- Se crea una vez un **centro de medición** con PDF públicos con tablas: MIL-STD-882E y 2 o 3 del corpus.
- **Herramienta:** `cargo run --example medir_anclaje -- <centro>` lee 02 y 05 en solo lectura, ejecuta `anclar_tabla` y escribe el bloque y la página de cada fila.
- **Oráculo:** un script con `pypdf` que localiza la coincidencia única de la clave y los valores de cada fila. Los casos ambiguos se excluyen y se informan.
- **Métricas:** páginas falsas (criterio: 0) y cobertura.
- Iterar sobre el algoritmo no exige volver a ingerir.

### 6.4 N3: cuota por documento

- Centro con varios documentos: el de §6.3 más los fixtures de `ciclo_de_vida` y `rgb_industrial`.
- **Herramienta:** un script en Python con `benchmarks/lakehouse/mcp.py`, que llama a `search_normative_context` sin Claude (como `run_aguja._recuperacion_directa`).
- **Métricas:**
  - recall@8 del fragmento de referencia en las preguntas de un solo documento (Aguja y RGB). Criterio: **no baja**;
  - porcentaje de respuestas con los dos documentos en el top 8, en preguntas cruzadas sintéticas (A + B). Se informa.

### 6.5 N4: repuntuación de respuestas guardadas

`benchmarks/evaluacion/repuntuar.py` añade un modo que vuelve a ejecutar las reglas puras de Rust sobre los JSON guardados (`respuesta` y `herramientas[].resultado`), con un binario auxiliar o un test `#[ignore]`:
- el verificador de páginas (3c): notas antes y después, revisando a mano las eliminadas;
- el detector de JSON embebido (3b);
- el filtro literal y el endurecimiento de la abstención (4): cuántos turnos irían a Haiku y qué abstenciones cambian.

No usa Claude.

### 6.6 Línea base y criterios de no retroceso (`CLAUDE.md` §6)

**Línea base**, en `main` (`62f3010`) y antes de cada pieza:
- N0 completo, contando aprobados e ignorados;
- los niveles N1 a N4 que correspondan, en `benchmarks/informes/linea_base_generalidad_<pieza>.md`.

**Cualquier empeoramiento bloquea el merge.**

| Pieza | Criterio |
|---|---|
| 1 | N0 en verde. Ninguna sección perdida. 100 % de las frases con la misma página. 0 páginas degradadas en los PDF legibles. El PDF LaTeX Type1/CFF, legible. Aguja igual (1,00) |
| 2 | 0 páginas falsas. La recuperación de tablas en RGB y `acceptance_8_problemas` no baja. Un centro v6 se rechaza |
| 3a | Recall@8 de un solo documento igual o mejor. Con un documento, salida idéntica |
| 3b | 0 detecciones en las respuestas correctas guardadas |
| 3c | Los `Discrepante` verdaderos se mantienen. Las notas eliminadas no eran errores reales. Aguja: 0 notas nuevas |
| 4 | 100 % de las trampas sintéticas rechazadas. 0 cifras inventadas en los benchmarks propios. Abstenciones sin cifras de parámetros |

Los tiempos son orientativos, porque el equipo tiene carga: se dan con 3 o más ejecuciones.

### 6.7 Qué debería medir la sesión evaluadora aislada

- **Reutiliza los centros ya ingeridos** para 3a, 3b, 3c y 4, que solo tocan la recuperación o el arnés. **Las piezas 1 y 2 exigen centros v7 nuevos:** una sola ingesta con las dos integradas.
- **Subconjunto rápido** (propuesta; la sesión evaluadora elige las preguntas sin comunicarlas):
  - las preguntas de las categorías que fallan en `main` (cálculo en tabla, dato repartido legal, PDF académico);
  - las 8 trampas;
  - las 9 sin respuesta;
  - las multidocumento;
  - las respondidas con `lookup_double_entry_table`, para comprobar la página de la Pieza 2.

  Unas 25-30 preguntas en 3 pasadas.
- **Ingesta:** documentos y páginas `LECTURA_DEGRADADA` en su corpus, frente a los ilegibles reales. También la calibración de umbrales con sus 15 PDF.
- **El set v3** (trampas en dos grupos y suites base y de estrés) lo hará la sesión evaluadora más adelante. **No entra en este SSD.**

---

## 7. Independencia y orden

| Pieza | Archivos compartidos | Puede ir en paralelo con |
|---|---|---|
| 1 | `mapeo_destilacion.rs` (`mapear_con_estado`), `estructura_documental.rs`, `schemas.rs` (01), `estructura.rs` | 3a, 3c, 4. **Con la 2 comparte la v7**: una sola sube la versión |
| 2 | `mapeo_destilacion.rs` (`vincular_tablas`, otras funciones), `schemas.rs` (04 y 05), `dtos/centros.rs` | 3a, 3c, 4 y, con coordinación, la 1 |
| 3a | `busqueda.rs` | Todas |
| 3b | `post_inference_guard.rs`, `agent_orchestrator.rs`, `claude_agent_runner.rs`, GUI | 1, 2, 3a, 3c. **Antes de la 4** |
| 3c | `verificador_paginas.rs` | Todas |
| 4 | `post_inference_guard.rs`, `agent_orchestrator.rs` | 1, 2, 3a, 3c. **Después de la 3b** |

**Orden recomendado:**
1. **En paralelo: 3c, 3a y 1.** La 1 empieza por el spike de pdfium y el lote de control (N1). La 3c y la 3a son pequeñas y se miden sin Claude.
2. **2.** Sube a v7 incluyendo los cambios de la 1. Después se recrean los centros una sola vez.
3. **3b.**
4. **4.** Antes del adaptador, la repuntuación N4 da la frecuencia de microllamadas y las abstenciones afectadas.
5. **Pieza 5**, en otra rama, tras validar las piezas 1-4.

## 8. Fuera de alcance

- Las listas de `intent_detector.rs` (Pieza 5).
- `PALABRAS_DE_PAGINA` (`verificador_paginas.rs:29`).
- `INICIOS_DE_NOTA` y `PALABRAS_DE_CAPITULO` (`pdf_estructura.rs:33` y `:48`).
- `PALABRAS_DE_ENLACE`, `UNIDADES`, `PALABRAS_SIN_CIFRA` y `MESES` (§4.5).
- La página impresa (`/PageLabels`).
- La extracción geométrica completa de tablas y la banda `y` de pdfium.
- El set oculto v3.
