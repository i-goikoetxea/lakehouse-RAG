# SSD: generalidad en 4 piezas (PDF, página de Capa 2, búsqueda y arnés)

**Estado: borrador para aprobación. No hay código de producción.** Fecha: 11-11-2026. Base: `main` en `62f3010`.

Origen: [`docs/estado/situacion_rag_20261111.md`](../estado/situacion_rag_20261111.md), [`docs/estado/auditoria_generalidad_20261111.md`](../estado/auditoria_generalidad_20261111.md) y la decisión del mentor externo:
- la rama `claude/arreglos-rag-512d66` **no se integra**;
- se descartan `negaciones.rs` y `apartado_tabla.rs`;
- solo se rescata el silencio de los estados ambiguos de `verificador_paginas.rs`.

**Regla dura de este SSD: generalidad absoluta.** Ninguna pieza añade listas de palabras, locuciones o regex de un idioma o dominio. Sí se admiten:
- propiedades de Unicode (`char::is_alphanumeric`, rangos de la zona de uso privado, bloques de escrituras sin espacios);
- claves del propio protocolo de la app (`"tool_calls"`);
- tipografía (pares de comillas).

Las líneas citadas son de `62f3010` y están comprobadas. Donde la auditoría daba otra línea, vale la de aquí: por ejemplo, el `FinalMessage` de `claude_agent_runner.rs` está en la :143, no en la :140.

---

## 0. Resumen y decisiones que pide

| Pieza | Fallos del set v2 que ataca | Cambia el esquema | Riesgo de retroceso |
|---|---|---|---|
| 1. PDF y compuerta de legibilidad | 4 (V50 y V51, en 2 pasadas) | Sí: valor nuevo de `estado_contenido` (va con la v7) | Medio: toca el texto de todos los PDF |
| 2. Página exacta de la Capa 2 | 0 del set; evita el retroceso de la rama (1 página falsa) | **Sí: v6 → v7** | Bajo: hoy el lookup no da página |
| 3. Búsqueda, fuga de *tool calls* y verificador | Multidocumento (no concluyente), 1 fuga en `main`, 3 o más avisos falsos | No | Medio en la cuota; bajo en el resto |
| 4. Arnés estructural y verificador semántico | 4 (V22 y V53, en 2 pasadas) | No | **Alto**: protege las trampas y el «no consta» |

**Decisiones para el usuario (detalle en cada pieza):**
1. **D1. Extractor:** mantener `pdf-extract`/`lopdf` e inyectar un ToUnicode propio (opción b, recomendada), o `pdfium-render` (opción a). §1.1.
2. **D2. Esquema v7:** hay que **recrear los centros** una sola vez, cuando las piezas 1 y 2 estén integradas. §2.4.
3. **D3. Estado nuevo `LECTURA_DUDOSA`:** marca las filas de las páginas que fallan la compuerta, en lugar de rebajar todo el documento. §1.3.
4. **D4. Streaming:** si el arnés valida todos los turnos, el texto emitido en vivo puede tener que retirarse. Propuesta: un evento `TextoEnVivoDescartado`, en lugar de retener siempre el texto. §3.2.
5. **D5. Arnés:** se quitan del guard las reglas con disparador léxico (`CapaDosNoIntentada` y `CifraEnAbstencion`). El `IntentDetector` queda solo como pista previa, que nunca bloquea. §4.3.
6. **D6. Microllamada a Haiku:** añade unos 6-9 s cuando se dispara, medidos en el spike de la Fase 0. Si falla, la regla actúa como hoy (falla cerrada). §4.4 y §4.5.
7. **D7. Invariante «conclusión sin consulta»:** es nuevo. Puede meter reintentos en saludos o preguntas sobre la app. §4.2.

---

## 1. Pieza 1: ingesta de PDF robusta y compuerta de legibilidad

### 1.1 Diagnóstico verificado y elección del extractor

**Comprobado en el código:**
- La extracción es por página: `pdf_parser.rs:207-232` (`extraer_paginas`), con `pdf_extract::output_doc_page` y `lopdf::extract_text` como respaldo.
- `requiere_ocr` (`pdf_estructura.rs:145`) solo cuenta caracteres no blancos.
- `normalizar_pagina` (`pdf_estructura.rs:116`) **borra los caracteres de control** antes de cualquier medición. La compuerta debe medir el texto crudo.
- El documento queda COMPLETO si la destilación tiene éxito (`document_ingestion_service.rs:166`). Los avisos de extracción solo se añaden a la lista (`:184`).

**Comprobado en `pdf-extract` 0.12.1** (`~/.cargo/registry/.../pdf-extract-0.12.1/src/lib.rs`):
1. **`/Differences` ya se decodifica por nombre de glifo** (`:477-535`), pero `glyphnames::name_to_unicode` es una **búsqueda exacta** en una tabla fija. No aplica el algoritmo AGL: `uniXXXX`, `uXXXX[XX]`, ligaduras `f_f_i`, sufijos `.sc`/`.alt`.
   - Un nombre desconocido solo produce un `warn!`, y el código **conserva la letra de la codificación base**.
   - Sin `/BaseEncoding`, esa base es `PDFDocEncoding` (`:471-476`), no la codificación interna de la fuente que exige la especificación.
2. **Fuentes CFF (`FontFile3/Type1C`, `:406-434`):** el `unicode_map` se construye con la **codificación interna** de la fuente. Después, en el bucle de `/Differences`, una entrada ya ocupada **no se sustituye** (`Entry::Occupied` solo avisa, `:496-505`).
   - Así, la codificación interna gana a `/Differences`, al revés de lo que dice la especificación PDF.
   - En una fuente subconjunto, cuyos códigos internos son arbitrarios, eso da **letras sustituidas**.
3. Las entradas de un `/ToUnicode` **sí ganan** a las demás: `unicode_map.extend(get_unicode_map(...))` (`:452-455`).

**Supuesto, no comprobado con el documento:** el informe agregado del set v2 describe el PDF de V50/V51 como Type1 (CFF) con `/Differences` y sin ToUnicode. Encaja con los defectos 1 y 2, pero no he abierto ese PDF: no se puede.

**Elección: opción (b).** Mantener `pdf-extract`/`lopdf` y, antes de extraer, **inyectar en memoria un `/ToUnicode` propio** en cada fuente simple que no lo tenga. Por el punto 3, `pdf-extract` lo usará sin tocar el crate.

| Criterio | (a) `pdfium-render` | (b) ToUnicode propio |
|---|---|---|
| Arregla el defecto diagnosticado | Sí (el motor de Chrome aplica la especificación) | Sí, de forma directa |
| Radio de cambio | **Todo el texto de todos los PDF** cambia: espacios, orden de líneas, guiones. Afecta a `pdf_estructura` (numeración, capítulos), los chunks, los saltos y los benchmarks de página | Solo las fuentes sin ToUnicode. El resto del texto queda igual byte a byte, y eso se puede comprobar |
| Distribución en Windows | `pdfium.dll` aparte, cargada en tiempo de ejecución (`Pdfium::bind_to_library`), o compilación estática de pdfium (pesada). Hay que llevarla en `bundle.resources` de Tauri y localizarla en la CLI y en los tests | Nada nuevo |
| Tamaño | Varios MB por la DLL (no medido) | 0 |
| Tests | Necesitan la DLL. Sin CI en el repo (no hay `.github/`), cada máquina debe descargarla, o los tests se marcan `#[ignore]` | Unitarios e integración con PDF sintéticos de `lopdf` (`tests/comun_pdf`) |
| Coordenadas por carácter | **Sí**: base para tablas por regiones | No |
| Licencia (sin peso) | BSD/Apache | — |

**Por qué (b):** el defecto está localizado y es corregible en el punto exacto. La opción (a) cambia el texto de todo el corpus, y con él se mueven todas las líneas base de página, de estructura y de chunk. Ese es el retroceso que `CLAUDE.md` §6 obliga a evitar. Las coordenadas de (a) solo hacen falta para la vía geométrica de la Pieza 2, y la vía textual no las necesita (§2).

**Revisable:** si tras (b) la compuerta sigue marcando páginas en el corpus público (§5), se evalúa pdfium **solo como respaldo por página**, detrás de una *feature* de cargo. Sería otro SSD.

### 1.2 Diseño

**Módulo nuevo `infrastructure/parsers/pdf_unicode.rs`.** Construye el ToUnicode de las fuentes simples (`Type1`, `Type1C`, `TrueType`, `MMType1`) que no lo tienen.

Precedencia de la especificación PDF:

```text
/Differences  >  /BaseEncoding  >  codificación interna (FontFile/FontFile3)  >  StandardEncoding
```

Algoritmo AGL completo para cada nombre de glifo:
- búsqueda en la tabla de la AGL. El módulo `glyphnames` de `pdf-extract` es privado (`lib.rs:28`), así que se incorpora la tabla pública de la AGL como dato estático;
- `uniXXXX[XXXX…]`;
- `uXXXX` a `uXXXXXX`;
- componentes separados por `_`;
- sufijo tras `.` descartado.

Un nombre no resoluble (`/g13`, `/C0`) **no se adivina**: se cuenta.

```rust
pub struct InformeUnicodeFuente {
    pub fuente: ObjectId,
    pub codigos_mapeados: usize,
    pub nombres_no_resueltos: usize, // nombres de glifo fuera de la AGL y sin forma uniXXXX
}
/// Inyecta /ToUnicode en memoria en las fuentes simples que no lo tienen. Nunca toca una fuente que ya lo tiene.
pub fn completar_to_unicode(documento: &mut Document) -> Vec<InformeUnicodeFuente>;
```

Se llama en `extraer_documento` (`pdf_parser.rs:102`), entre `abrir_sin_proteccion` y `extraer_paginas`, bajo el `catch_unwind` que ya existe. Para la codificación interna de CFF basta el `charset` y la `Encoding` del CFF, sin rasterizar. `cff-parser` ya es dependencia de `pdf-extract` (su `Cargo.toml`). Si expone lo necesario, se declara como dependencia directa con la misma versión, un cambio de dependencias que pide el SÍ del usuario (AGENTS.md §1.2); si no, un lector mínimo propio.

**Módulo nuevo `infrastructure/parsers/legibilidad.rs` (compuerta), puro y sin I/O.** Se ejecuta sobre el texto **crudo** de cada página, antes de `normalizar_pagina`.

```rust
pub struct MetricasPagina {
    pub caracteres_visibles: usize,        // sin espacios
    pub proporcion_reemplazo: f32,         // U+FFFD / visibles
    pub proporcion_control_pua: f32,       // control (sin \t \n \r) + PUA (E000–F8FF, planos 15-16) / visibles
    pub densidad_alfanumerica: f32,        // alfanuméricos / (visibles − símbolos en rachas de ≥3 iguales)
    pub proporcion_tokens_anomalos: f32,   // letras en tokens de > 40 caracteres / letras
    pub usa_fuente_no_fiable: bool,        // la página usa una fuente con ≥25 % de nombres no resueltos
}
pub enum VeredictoPagina { Legible, NoEvaluable, Dudosa { motivos: Vec<MotivoIlegible> } }
pub fn evaluar_pagina(texto_crudo: &str, usa_fuente_no_fiable: bool) -> VeredictoPagina;
```

- **No evaluable:** menos de 200 caracteres visibles, por ejemplo una portada o una página de figura. Nunca falla.
- **Tokens:** se separan por espacios. No cuentan como anómalos los tokens que contienen caracteres de escrituras sin espacios: Han, Hiragana, Katakana, tailandés, lao, jemer, birmano y tibetano, por rangos de bloque Unicode.
- **Rachas de símbolos:** se excluyen del denominador las de 3 o más símbolos iguales, como los puntos guía de un índice («……… 3») o las líneas de tabla («────»).

**Umbrales iniciales, que se ajustan con el corpus público (§5) y nunca con el set oculto.** Una página es `Dudosa` si cumple cualquiera de estas condiciones:

| Métrica | Umbral | Lo que caza |
|---|---|---|
| `proporcion_reemplazo` | > 1 % | Códigos sin mapa |
| `proporcion_control_pua` | > 5 % | Glifos sin Unicode (los iconos de viñeta legítimos suelen quedar por debajo del 3 %; a medir) |
| `densidad_alfanumerica` | < 40 % | Basura de símbolos |
| `proporcion_tokens_anomalos` | > 25 % | Espacios perdidos: el síntoma del set v2 (22 de 24 trozos superaron el límite de embeddings) |
| `usa_fuente_no_fiable` | verdadero | Sustitución de letras por letras, que las demás métricas no ven |

**Por qué no rechazan textos legítimos:**
- `is_alphanumeric` es Unicode: el griego, el cirílico, el árabe, el devanagari (con sus matras alfabéticas) y el CJK cuentan como alfanuméricos.
- El CJK sin espacios queda fuera de los tokens anómalos.
- En fórmulas, las letras griegas y los dígitos son alfanuméricos. Una página con muchas ecuaciones se mantiene en torno al 50 % (lo fija un test).
- En tablas, las cifras son alfanuméricas y las líneas de tabla se descuentan por racha.

**Respaldo por página:** si una página sale `Dudosa`, se extrae con `lopdf::extract_text`, se vuelve a evaluar y se queda la versión `Legible`, si la hay. Si las dos fallan, la página conserva el texto (puede tener cifras útiles), pero queda marcada.

### 1.3 Contratos y efecto en la ingesta

- **`ParsedDocument`** (`dtos/ingestion.rs:22`): nuevo `#[serde(default)] pub paginas_dudosas: Vec<PaginaDudosa>`, con `pagina` y `motivos`.
- **`EstadoContenido`** (`domain/models/estructura_documental.rs:19`): nuevo valor `LecturaDuda` → `"LECTURA_DUDOSA"` (**D3**).
  - Hay que tocar `VALORES_CANONICOS`, `as_str` y `parse`.
  - `lancedb_store.rs:720` lo acepta a través de `EstadoContenido::parse`.
- **Mapeo:**
  - las filas de 02 (chunks) y 07 (nodos) cuyo rango de páginas corta una página dudosa llevan `LECTURA_DUDOSA`;
  - el resto conserva su estado;
  - el informe de ingesta da `LECTURA_DUDOSA` si hay al menos una página dudosa, con este aviso: «Páginas N–M: el texto extraído puede ser ilegible (motivo). Revisa el PDF o pásalo por OCR».
  - Se toca `mapear_con_estado` (`mapeo_destilacion.rs:233` y siguientes) y `document_ingestion_service.rs:166-184`.
- **System prompt, regla 7** (`agent_orchestrator.rs:88`): se añade `LECTURA_DUDOSA` («el texto de esas páginas puede estar corrupto: dilo y no concluyas "no consta" a partir de él»). No es una lista léxica del arnés: es una instrucción al modelo.
- **Por qué no `PARCIAL_UNIVERSAL`:** ese estado significa «sin Capa 2» y lo usa la lógica de provisionales (`busqueda.rs:652`). Mezclarlos rompería esa semántica.

### 1.4 Archivos

- `src/infrastructure/parsers/pdf_unicode.rs` (nuevo)
- `src/infrastructure/parsers/legibilidad.rs` (nuevo)
- `pdf_parser.rs`
- `pdf_estructura.rs` (solo para exportar el texto crudo; `requiere_ocr` no cambia)
- `dtos/ingestion.rs`
- `estructura_documental.rs`
- `mapeo_destilacion.rs`
- `document_ingestion_service.rs`
- `agent_orchestrator.rs` (regla 7)
- `tests/adversarial_pdf_tests.rs`
- `tests/integration_pdf_parser.rs`
- `tests/comun_pdf/mod.rs` (generadores)

### 1.5 Plan TDD (datos sintéticos)

**Generador:** se amplía `tests/comun_pdf` con fuentes cuya `/Encoding` es un diccionario con `/Differences`, y con un CFF mínimo construido en el test (cabecera, `charset` y `Encoding`) para el caso de precedencia. **Coste: el generador de CFF es el trabajo más largo de la pieza** (unas 150 líneas de test).

**Rojo, ToUnicode:**
1. `/Differences` con `uni0394`, `afii10017`, `f_i`, `a.sc` y `Alpha`: produce «Δ», «А», «fi», «a» y «Α».
2. CFF cuya codificación interna contradice `/Differences`: gana `/Differences`.
3. Fuente con ToUnicode propio: **no se toca**. Su texto queda igual byte a byte.
4. Nombres `/g13`: se cuentan en `nombres_no_resueltos`. No se inventa ningún carácter.
5. Fuente sin `/Differences`: nada cambia.

**Rojo, compuerta.** Una página por caso:

| Caso | Contenido sintético | Veredicto esperado |
|---|---|---|
| Legible | Prosa en ES, EN, DE, EL (griego), RU (cirílico), JA, ZH (CJK sin espacios), AR (árabe), HI (devanagari) | `Legible` |
| Fórmulas | Ecuaciones densas (`∑`, `≤`, `α`, subíndices) | `Legible` |
| Tablas | Rejilla de cifras con líneas `│─` y puntos guía | `Legible` |
| Iconos | Viñetas en la PUA por debajo del 3 % | `Legible` |
| Corrupta | 5 % de `U+FFFD`, 10 % de PUA, símbolos sueltos, palabras pegadas | `Dudosa` (cada métrica por separado) |
| Corta | Portada de 40 caracteres | `NoEvaluable` |

**Integración:**
- un PDF de 3 páginas con la página 2 corrupta da el informe `LECTURA_DUDOSA`, el chunk de esa página lleva `LECTURA_DUDOSA` y los chunks de las páginas 1 y 3 quedan `COMPLETO`;
- el respaldo `lopdf` sustituye una página dudosa cuando su texto es legible.

**Adversarial:**
- `Differences` con un código mayor que 255;
- nombres de 10 000 caracteres;
- un CFF truncado;
- fuentes con 10 000 entradas.

Ninguno debe producir un pánico que salga fuera del `catch_unwind`, y el resto de páginas se conserva.

### 1.6 Riesgos de retroceso y medición

| Riesgo | Medición | Criterio |
|---|---|---|
| Cambia el texto de PDF que ya funcionaban | Hash del texto por página antes y después en el corpus público (§5.2) | Solo pueden cambiar páginas con fuentes sin ToUnicode. Cualquier otro cambio bloquea |
| Falso «dudoso» en otros alfabetos, fórmulas o tablas | Proporción de páginas dudosas en el corpus legible | **0** páginas dudosas en los PDF que un humano valida como legibles |
| El estado nuevo confunde al modelo | Aguja y ciclo de vida sin cambios en la puntuación | Iguales a la línea base |
| Coste de la ingesta | Tiempo de parseo por documento | < 5 % más (orientativo: el equipo tiene carga) |

---

## 2. Pieza 2: página exacta de la Capa 2 en la ingesta

### 2.1 Estado verificado

- **El parser de PDF no extrae tablas** (`pdf_parser.rs:142`). Las tablas de un PDF salen solo de `TablaDestilada` (`dtos/ingestion.rs:170`), con la `seccion_origen` que declara el LLM.
- **`vincular_tablas`** (`mapeo_destilacion.rs:1236-1262`) vincula la tabla:
  - a la sección declarada;
  - a la sección del parser;
  - a todo chunk que contenga `"Tabla {etiqueta}"` (`:1252`), un literal en **español**.
- `titulo_con_etiqueta` (`:1077`) también compone `"Tabla {etiqueta}"` para el título que ve el modelo. **Mismo acoplamiento al español.** Se incluye aquí porque es el mismo archivo y la misma regla.
- Los esquemas 04 (`schemas.rs:177`) y 05 (`schemas.rs:206`) **no tienen columnas de página**. `lookup.rs` no devuelve página.

### 2.2 Diseño

**Vía elegida: anclaje textual en Rust** (la vía geométrica requiere pdfium; ver D1).

**Servicio de dominio nuevo `domain/services/anclaje_paginas.rs`**, puro:

```rust
pub struct PaginaTexto<'a> { pub pagina: u32, pub texto: &'a str }
pub struct FilaParaAnclar { pub rotulos: Vec<String>, pub valores: Vec<String> } // etiqueta del eje y + textos de celda
pub struct TablaParaAnclar { pub cabecera: Vec<String>, pub filas: Vec<FilaParaAnclar> } // título + categorías y nombre del eje x
pub struct AnclajeTabla { pub por_fila: Vec<Option<u32>>, pub rango: Option<(u32, u32)> }
pub fn anclar_tabla(paginas: &[PaginaTexto], tabla: &TablaParaAnclar) -> AnclajeTabla;
```

**Algoritmo,** independiente del idioma:
1. **Tokens:** `plegar_mayusculas` (`domain/services/normalizacion.rs:11`), corte por no alfanuméricos y tokens de 2 o más caracteres.
   - En las escrituras sin espacios, los tokens son **bigramas de caracteres**.
   - Las cifras se tokenizan con su texto, y además con la coma y el punto decimal intercambiados.
2. **Rareza:** `df(t)` es el número de páginas del documento que contienen `t`. Un token es **distintivo** si `df(t) ≤ max(2, ⌈N/10⌉)`.
3. **Anclaje de una fila:** una página `p` es candidata si, en una ventana de 400 caracteres de esa página, aparecen juntos:
   - al menos **2 tokens distintos de la fila** (rótulos o valores);
   - al menos **1 token de la cabecera o del rótulo**;
   - y al menos uno de todos ellos es distintivo.

   La puntuación es el número de tokens distintos que coinciden. **Solo se acepta una página con la puntuación máxima única.** Si hay empate, o ninguna candidata, la fila queda en `None`.
4. **Rango de la tabla:** se calcula con el mínimo y el máximo de las filas ancladas, solo si se anclan al menos la mitad de las filas y el rango ocupa como mucho las páginas consecutivas que justifican sus filas. Si no, queda en `None`.
5. **Celda:** toma la página de su fila.
6. **Nunca** se recurre a la sección padre, a `seccion_origen` ni a un número suelto: el anclaje exige que coincidan 2 o más tokens en la misma ventana y que alguno sea distintivo.

**Texto paginado en la ingesta:** se reconstruye con `ParsedSection.contenido` y `saltos_pagina` (`dtos/ingestion.rs:58`), mediante `texto_con_marcas` y `tramos_paginados` (`domain/services/marcas_pagina.rs:101` y `:126`). No hace falta cambiar el parser. Un Markdown no tiene saltos, así que su página es siempre `None`.

**Literal `"Tabla {id}"` (`:1252`):** se elimina. La vinculación por cita pasa a ser estructural:
- a) **un chunk cuyo rango de páginas contiene una página anclada** de la tabla;
- b) **la etiqueta distintiva como secuencia de tokens completa** en el chunk.

Una etiqueta es distintiva si mezcla letras y dígitos, o si tiene un separador entre alfanuméricos: «4-A» y «A.2» lo son; «3» no. Así se reconoce «Tabla 4-A», «Table 4-A» o «Таблица 4-A», y una etiqueta numérica pura no vincula por cita.

**`titulo_con_etiqueta` (`:1077`):** el rótulo neutro es `"{etiqueta} — {título}"`, sin la palabra «Tabla».

### 2.3 Contratos

- **`FilaTabla`** (`dtos/lakehouse_filas.rs:113`): `+ paginas: Option<(u32, u32)>`.
- **`FilaCelda`** (`:136`): `+ pagina: Option<u32>`.
- **Esquema 04:** `+ pagina_inicio Int32 null`, `+ pagina_fin Int32 null`. **Esquema 05:** `+ pagina Int32 null`. Se escriben en `lotes_arrow.rs`.
- **`lookup_double_entry_table`:** `cita.paginas` (`"21-22"` o `"21"`), solo si existe, con el mismo formato que `cita.paginas` de la búsqueda. Si es `None`, la clave no aparece.
- **`VERSION_ESQUEMA_CENTRO`** (`dtos/centros.rs:13`): **6 → 7.**

### 2.4 Esquema: v6 → v7 (D2)

**Sí hace falta cambiarlo.** Un centro v6 se rechaza al abrirlo (`centros.rs:71`) y no hay migración, como en la v5 → v6.

> **El usuario tendrá que recrear sus centros y volver a ingerir los documentos.** Los datos de los centros anteriores no se tocan.

El valor `LECTURA_DUDOSA` de la Pieza 1 viaja en la misma v7: un binario v6 no lo reconocería. **Hay que recrear los centros una sola vez, con las dos piezas integradas.**

### 2.5 Archivos

- `src/domain/services/anclaje_paginas.rs` (nuevo)
- `mapeo_destilacion.rs` (`vincular_tablas`, `fila_tabla`, celdas, `titulo_con_etiqueta`)
- `dtos/lakehouse_filas.rs`
- `infrastructure/storage/schemas.rs`
- `lotes_arrow.rs`
- `dtos/centros.rs`
- `lakehouse_tool_service/lookup.rs`
- `dtos/tools.rs` (descripción de la tool)
- `tests/integration_tools_v2.rs`
- `tests/adversarial_*` (nuevo caso)

### 2.6 Plan TDD (datos sintéticos)

**Rojo, unitarios** de `anclar_tabla`, con texto paginado escrito en el test:
1. Tabla de ES (rótulos de material y temperatura), EN (finanzas), DE (dosis clínica) y EL (griego): la fila se ancla en su página.
2. **La tabla está en la página 21, y la introducción de la página 6 menciona un valor de la tabla:** la fila se ancla en la 21, nunca en la 6. Es el patrón del defecto de la rama, reproducido con datos inventados.
3. Valor repetido en dos páginas con los mismos rótulos: `None`.
4. Solo coincide 1 token: `None`.
5. Tabla partida entre las páginas 21 y 22: rango `(21, 22)`.
6. CJK sin espacios: anclaje por bigramas.
7. Markdown sin saltos: `None`.

**Rojo, vinculación:**
- «Table 4-A» en un chunk inglés vincula;
- «Tabla 3», con la etiqueta numérica pura «3», **no** vincula por cita;
- un chunk que contiene la página anclada vincula.

**Integración:**
- ingesta con `MockDistiller` y un PDF sintético de 3 páginas: el lookup devuelve `cita.paginas` correcto;
- un centro v6 se rechaza con el mensaje actual.

**Adversarial:**
- tablas de 5000 celdas;
- celdas vacías;
- textos de celda de 10 000 caracteres;
- saltos corruptos (que `saltos_validos` ya filtra).

### 2.7 Riesgos de retroceso y medición

| Riesgo | Medición | Criterio |
|---|---|---|
| **Página falsa** (el retroceso de la rama) | Precisión del anclaje en un corpus con páginas de tabla verificadas por script (§5.3) | **0 páginas falsas.** Una sola bloquea |
| Menos vínculos tabla-chunk en documentos en español | Número de `tablas_vinculadas` por documento en los fixtures, antes y después | No baja la recuperación de tablas en RGB industrial ni en `acceptance_8_problemas` |
| Coste de la ingesta | Tiempo de `vincular_tablas` | Despreciable frente a la destilación (a medir) |

---

## 3. Pieza 3: búsqueda multidocumento, fuga de *tool calls* y verificador de páginas

Son 3 subpiezas independientes entre sí: **3a**, **3b** y **3c**.

### 3a. Cuota por documento en `fusionar`

**Estado:** `fusionar` (`busqueda.rs:147-189`) puntúa **todos** los fragmentos elegibles, los ordena y aplica `truncate(top_k)` (`:187`). `top_k` vale 8 por defecto (`lakehouse_tool_service.rs:68`) y 20 como máximo (`busqueda.rs:31`). La llamada está en `:500`.

**Diseño: cuota con relleno.** Se aplica solo sin filtro de documento, es decir, con `doc_pedido.is_none()` (`busqueda.rs:423`):
1. El pool es el ranking RRF completo, que ya existe.
2. `max_por_documento = top_k − min(2, documentos_en_pool − 1)`: 6 de 8 con dos o más documentos.
3. **Primera pasada:** se recorre el ranking y se acepta cada fragmento si su documento no ha llenado la cuota.
4. **Relleno:** si quedan huecos, se completan con los descartados, en orden de ranking.
5. **Orden de salida:** el del score RRF.

**Por qué no empeora una pregunta de un solo documento:**
- en un centro con un documento, o con un único documento en el pool, la salida es **idéntica** a la actual;
- con varios documentos, el documento dominante conserva sus **6 mejores** fragmentos, y solo pierde los puestos 7 y 8 si otro documento tiene candidatos;
- con `doc_id` en la petición, no hay cuota.

```rust
pub(crate) fn fusionar<'a>(…, top_k: usize, cuota: CuotaDocumento) -> Vec<Puntuado<'a>>;
pub(crate) enum CuotaDocumento { Ninguna, Diversificar { reservados: usize } } // reservados = 2
```

**Archivos:** `lakehouse_tool_service/busqueda.rs` y sus tests.

**TDD:**
1. Centro con un documento: salida igual a la de hoy (la propiedad se comprueba con 50 rankings aleatorios con semilla fija).
2. Dos documentos, con 10 fragmentos de A mejor puntuados que el mejor de B: salida de 6 de A y 2 de B.
3. B sin candidatos: 8 de A.
4. Con `doc_id`: sin cuota.
5. Tres documentos con `top_k = 1`: 1 resultado, el mejor.

Los fragmentos de prueba están en ES, EN y FR.

**Riesgo y medición:** el fragmento de referencia de una pregunta de un solo documento puede caer del puesto 7 u 8. Se mide en recuperación pura (§5.4). **Criterio:** el recall@8 del fragmento de referencia de las preguntas de un solo documento no baja.

### 3b. Fuga de *tool calls* en crudo

**Estado verificado:**
- `parsear_salida_claude` (`claude_agent_runner.rs:80-144`) solo reconoce como llamada **la salida entera** parseada como JSON. Si no lo consigue, devuelve `FinalMessage` (`:143`).
- El filtro de streaming decide si el texto es visible **por su primer carácter** (`stream_json.rs:80`: `'{'` o `` '`' ``). Un texto que empieza en prosa y sigue con el JSON se emite en vivo.
- Con `IntencionTurno::Ninguna`, el guard devuelve `Ok` sin revisar nada (`post_inference_guard.rs:191`), y el texto no se retiene (`pre_inference_interceptor.rs:37`).

**Evidencia previa:** en el spike de la Fase 0 (`arnes_fase0/bateria.jsonl`, caso `pregunta_ajena`), Haiku respondió con prosa y un bloque ```json con `{"tool_calls": …}`. Es exactamente esta fuga.

**Diseño:** es una regla estructural sobre las claves del protocolo de la propia app, no sobre el idioma.

```rust
// application/use_cases/arnes_turno/llamada_en_crudo.rs
pub enum LlamadaEmbebida { Completa(Vec<ToolCall>), Parcial }
/// Busca objetos JSON (serde_json::Deserializer sobre cada '{') con la clave "tool_calls" (array)
/// o "type":"tool_use". Si no hay ninguno completo, `Parcial` si aparece la clave del
/// protocolo seguida de ':' (`"tool_calls"\s*:`), también dentro de una valla ```.
pub fn detectar(texto: &str) -> Option<LlamadaEmbebida>;
```

1. **Runner** (`parsear_salida_claude`): antes de devolver `FinalMessage`, aplica `detectar`.
   - Con `Completa`, devuelve `ToolCalls` y descarta la prosa: el modelo quería llamar a una herramienta, y ejecutarla ahorra un reintento.
   - Un nombre que no está en el catálogo ya devuelve un resultado de error de la tool.
2. **Guard:** nuevo invariante `ViolacionArnes::LlamadaEnCrudo`, para el caso `Parcial`. Se aplica **en todas las intenciones, también `Ninguna`**. Produce un reintento con el recordatorio «responde solo con el JSON del protocolo o solo con texto» y, si se repite, salida segura.
3. **Streaming (D4):** si ya se emitió texto en vivo y la respuesta se rechaza o se reinterpreta como llamada, el orquestador emite `AgentExecutionEvent::TextoEnVivoDescartado`.
   - La GUI (`web/app.js`, rama `arnes`, `:673`) vacía la burbuja y muestra el estado «revisando».
   - **Alternativa:** retener siempre el texto, sin streaming. Es más simple, pero pierde una función que el usuario aprobó en la Fase 15.

**Archivos:**
- `arnes_turno/llamada_en_crudo.rs` (nuevo)
- `claude_agent_runner.rs`
- `post_inference_guard.rs` (solo el invariante)
- `agent_orchestrator.rs` (evento)
- `dtos/agent.rs` (evento)
- `presentation/gui/chat_service.rs`
- `web/app.js`
- `tests/integration_chat_arnes.rs`

**TDD:**
1. Prosa + valla ```json con `tool_calls` completo: `ToolCalls`.
2. JSON truncado: guard → `LlamadaEnCrudo`.
3. Dos bloques JSON: las dos llamadas.
4. Respuesta legítima que **muestra** un JSON sin las claves del protocolo, como un ejemplo de configuración: `FinalMessage`.
5. La prosa del caso 1 en ES, EN y JA.
6. En vivo: tras el rechazo, la GUI recibe `TextoEnVivoDescartado` antes del texto nuevo.

**Riesgo y medición:** un falso positivo, si un documento contiene la cadena `"tool_calls":`, es improbable fuera de documentación técnica de LLM. Se mide repuntuando las respuestas guardadas (`--ask --json` guarda `herramientas[].resultado` y `respuesta`, `cli/salida.rs:159-175`). **Criterio:** 0 detecciones en las respuestas correctas guardadas.

### 3c. Verificador de páginas

**Estado verificado:**
- `CitaRevisada::motivo` (`verificador_paginas.rs:55`) genera nota para `Discrepante`, `Ambigua`, `SinPagina` y `NoEncontrada`.
- La excepción de título o breadcrumb (`:106-111`) se aplica **solo** con `NoEncontrada`.
- `comprobar` (`:145-179`) da `Discrepante` cuando hay una única aparición distinta de las páginas citadas, aunque estas la contengan dentro de un rango.

**Diseño:**
1. **Rescate de la rama**, solo esto (`git show claude/arreglos-rag-512d66:…/verificador_paginas.rs`):
   - a) `motivo()` devuelve `Some` **solo** con `Discrepante`;
   - b) `Confirmada` si alguna aparición **corta** las páginas citadas (`!paginas.is_disjoint(citadas)`), antes del `match`.

   **No** se rescata el test `test_el_contexto_del_apartado_de_un_cruce_es_fuente_con_paginas`: depende de `contexto_apartado`, que viene de `apartado_tabla.rs`, descartado.
2. **Excepciones antes de emitir `Discrepante`:**
   - a) **metadatos:** si el esqueleto del extracto está contenido en algún metadato del turno (`titulo`, `breadcrumb`, que ya recoge `registro.metadatos()`), no se revisa, **sea cual sea el resultado**;
   - b) **posición de cabecera:** si **todas** las apariciones empiezan en el primer carácter del esqueleto de una página (justo tras `⟦p. N⟧`) y el extracto no pasa de una línea del texto fuente, el extracto es un título o una cabecera y no genera nota.

   `FuentePaginada` guarda para ello los inicios de página y los finales de línea en el esqueleto.
3. **Deuda que queda:** `PALABRAS_DE_PAGINA` (`:29`) es una lista de abreviaturas de «página» en español e inglés. No se toca aquí, porque no bloquea: solo decide qué se revisa. La opción general sería pedir al modelo que cite con la misma notación `⟦p. N⟧`. Es otro cambio, con efecto en la experiencia de usuario.

**Archivos:** `arnes_turno/verificador_paginas.rs` y `tests/adversarial_citas_pagina_tests.rs`.

**TDD:**
- los tests de la rama `test_en_caso_de_duda_se_guarda_silencio`, `test_rango_que_contiene_…`, `test_extracto_que_cruza_el_salto_…` y `test_parafrasis_no_se_anota`, con su texto sintético;
- nuevo: título del documento en la p. 1 del chunk, citado como «…», p. 22: sin nota;
- nuevo: cabecera repetida al inicio de la p. 7, citada como p. 9: sin nota;
- se mantiene: un extracto del cuerpo citado en una página ajena da `Discrepante` con nota.

Los datos van en EN, ES y RU.

**Riesgo:** callar un error real. **Medición:** campaña Aguja (§5.5) y repuntuación de las respuestas guardadas.
- **Criterio:** las notas `Discrepante` verdaderas de los tests siguen saliendo;
- en las respuestas guardadas, los avisos bajan y ninguno de los eliminados era un error de página real (revisión manual de cada uno).

---

## 4. Pieza 4: arnés estructural y verificador semántico en cascada

### 4.1 Estado verificado

- **`PostInferenceGuard::validar`** (`post_inference_guard.rs:186-207`):
  - sale con `Ok` si la intención es `Ninguna`;
  - `CapaDosNoIntentada` depende de la intención **léxica** (`intent_detector.rs`);
  - `CifraEnAbstencion` depende de `LOCUCIONES_DE_ABSTENCION` (`:18-51`, 33 locuciones en español) y `PETICIONES_DE_REFERENCIA` (`:53-72`);
  - el anclaje de cifras (`CifraSinAnclar`) **solo** se aplica con intención tabular o de responsable (`:198-205`).
- **Listas léxicas:**
  - `LOCUCIONES_DE_RECHAZO` (`:16`)
  - `IDENTIFICADORES_DE_ABSTENCION` (`:74`)
  - `PALABRAS_DE_ENLACE` (`:83`)
  - `cifras.rs:13` `UNIDADES` y `:19` `PALABRAS_SIN_CIFRA`
  - `fechas.rs:6` `MESES` (solo español)

### 4.2 Diseño: cascada

```text
respuesta candidata
  │
  ├─ R1 LlamadaEnCrudo (3b) ─────────────────────────────── estructural, Rust
  ├─ R2 anclaje literal de cifras, EN TODAS las intenciones ─ estructural, Rust
  │     cifras sin anclar = sospechas
  ├─ R3 conclusión sin consulta: 0 tools de contenido en el turno y respuesta no vacía
  │     → sospecha `ConclusionSinConsulta` ──────────────── estructural, Rust
  ├─ R4 ejes ambiguos (señal de la tool) y rol sin respaldo (señal de la matriz)
  │     → igual que hoy, sin listas (§4.3) ──────────────── estructural, Rust
  │
  └─ ¿hay sospechas? ── no → se entrega
         │ sí
         ▼
     microverificación (1 llamada, Haiku, sin tools, salida JSON)
         ▼
     Rust COMPRUEBA cada justificación (§4.6)
         ├─ todas válidas → se entrega sin intervención
         └─ alguna no válida → reintento (como hoy) → salida segura al segundo fallo
```

- **R2:** extrae las cifras con `extraer_cifras`. Las anclas son:
  - las cifras de la pregunta;
  - las de las salidas de las tools;
  - la fecha de hoy;
  - **las del system prompt y las descripciones de las tools**, el vocabulario de la app. Esto sustituye a `PALABRAS_SIN_CIFRA`: el «2» de «Capa 2» queda anclado en cualquier idioma del prompt.
- **R3** es el invariante «negación con 0 búsquedas», sin léxico. El disparador es estructural: no hubo ninguna llamada a `search_normative_context`, `stair_*`, `lookup_*`, `query_*`, `execute_athena_sql`, `traverse_*` ni `inspect_*`. El juicio (si la respuesta afirma o niega hechos de los documentos) lo hace la microllamada. **D7:** es nuevo y puede dar reintentos en saludos.
- Como máximo hay **1 microllamada por respuesta candidata**, que agrupa todas las sospechas: 2 por turno, contando el reintento.

### 4.3 Listas: qué se elimina y qué queda

| Elemento | Destino | Motivo |
|---|---|---|
| `LOCUCIONES_DE_ABSTENCION`, `PETICIONES_DE_REFERENCIA`, `LOCUCIONES_DE_RECHAZO`, `IDENTIFICADORES_DE_ABSTENCION` | **Se eliminan** | `CifraEnAbstencion` queda dentro de R2, que vale para cualquier respuesta: una cifra nueva tras un «no consta» (o tras un «not stated») está sin anclar y la juzga la cascada |
| Regla `CapaDosNoIntentada` | **Se elimina del guard** (D5) | El disparador es léxico: en otro idioma, o con la palabra equivocada, bloquea o no actúa. La pista hacia la Capa 2 sigue en `PreInferenceInterceptor`, que dirige el primer paso y **nunca bloquea** |
| `RolSinRespaldo` | Se mantiene sin `LOCUCIONES_DE_RECHAZO` | El disparador es la señal `sin_responsable_declarado` de la matriz. Pasa si la respuesta cita literalmente una tool o si no da ningún rol no anclado: el rol inventado es una afirmación sin respaldo, y su detección pasa a la microllamada como sospecha `RolNoDeclarado` |
| `PALABRAS_DE_ENLACE` | Se sustituye | Una seña de eje es un token del nombre del eje que no aparece en el otro eje (ya se calcula así) y que tiene 3 o más caracteres alfabéticos, o 1 o más ideográficos, o la unidad. Es una regla de longitud por escritura, no de vocabulario |
| `UNIDADES` (`cifras.rs:13`) | Se sustituye | Un token de dígitos con un sufijo de 1 a 4 letras y sin más dígitos es una cifra. La extracción es simétrica en la fuente y en la respuesta, así que extraer de más solo añade sospechas que la cascada resuelve |
| `PALABRAS_SIN_CIFRA` (`cifras.rs:19`) | Se elimina | Lo cubre el anclaje con el vocabulario de la app (R2) |
| `MESES` (`fechas.rs:6`) | Se elimina la parte textual | Quedan las fechas numéricas e ISO. Una fecha con el mes en palabras, en cualquier idioma, se ancla por sus componentes numéricos o se juzga en la cascada como `EquivalenciaTextual` |
| `PARES_DE_COMILLAS` | Se mantiene y se amplía (`„“`, `‹›`, `「」`, `『』`) | Es tipografía, no vocabulario |
| `intent_detector.rs` (todas sus listas) | **Quedan, como deuda declarada** | Ya no pueden bloquear una respuesta: solo eligen el primer paso dirigido. Sustituirlas (por ejemplo, con un enrutado por descripción de las tools) sería otro SSD |

`retener_texto` deja de depender de la intención: el texto se emite en vivo siempre, y se descarta con `TextoEnVivoDescartado` (D4).

### 4.4 Puerto hexagonal y adaptador

```rust
// application/ports/verificador_afirmaciones.rs
#[async_trait]
pub trait VerificadorAfirmacionesPort: Send + Sync {
    async fn verificar(&self, solicitud: &SolicitudVerificacion) -> Result<Dictamen, DomainError>;
}
pub struct SolicitudVerificacion {
    pub pregunta: String,
    pub extractos: Vec<ExtractoTurno>,   // id "r{i}" = salida i de las tools; texto sin marcas
    pub borrador: String,
    pub sospechas: Vec<Sospecha>,
}
pub enum Sospecha {
    CifraSinAnclar { literal: String, contexto: String }, // ±120 caracteres del borrador
    ConclusionSinConsulta,
    RolNoDeclarado { fragmento: String },
}
pub struct Dictamen { pub cifras: Vec<DictamenCifra>, pub afirma_sobre_documentos: Option<bool>, pub rol_respaldado: Option<CitaLiteral> }
pub struct DictamenCifra { pub literal: String, pub justificacion: Justificacion }
pub enum Justificacion {
    Calculo { operacion: Operacion, operandos: Vec<Operando> },
    EquivalenciaTextual { extracto: String /* id */, fragmento_literal: String },
    NoRespaldada,
}
pub struct Operando { pub valor: String, pub extracto: String /* id o "pregunta" */ }
pub enum Operacion { Suma, Resta, Producto, Cociente, Porcentaje, Media, Redondeo { decimales: u8 } }
pub struct CitaLiteral { pub extracto: String, pub fragmento_literal: String }
```

**Adaptador `infrastructure/llm/claude_headless_verificador.rs`:**
- comando: `claude -p --model <haiku> --tools "" --no-session-persistence --strict-mcp-config --json-schema <esquema del Dictamen>`, con un system prompt corto y fijo;
- reutiliza `ejecutar_cli` y la lectura de `structured_output` de los turnos dirigidos (`claude_agent_runner.rs:478` y siguientes);
- el modelo es configurable y Haiku va por defecto.

**Composición:** en `bootstrap.rs`. `AgentOrchestrator` recibe `Arc<dyn VerificadorAfirmacionesPort>`; en los tests, un `VerificadorFalso`.

**Latencia, medida en el spike de la Fase 0** (`arnes_fase0/bateria.jsonl`, Haiku con `claude -p`):
- caso de control sin esquema: 8,7 s y 8,8 s de reloj;
- con `--json-schema`: entre 6,0 y 9,3 s en los casos simples, con valores atípicos de 16 a 22 s.

Ese spike tenía otro prompt y una sola máquina con carga, así que el tiempo **real** de la microverificación no está medido.

**Coste:** se paga solo cuando hay sospecha. En `main`, la regla léxica saltó en 17 de 122 consultas del set v2. Con R2 en todas las intenciones, la frecuencia puede subir. **Se estima sin Claude** repuntuando las respuestas guardadas con R2 nuevo (§5.6) antes de implementar el adaptador.

### 4.5 Fallo o tiempo agotado (D6)

- Límite: **30 s**, sin reintento de la microllamada.
- **Falla cerrada:** si la microllamada falla, se agota el tiempo o el JSON no valida, cada sospecha cuenta como no respaldada. Es el comportamiento de hoy: reintento y salida segura.
- Se emite `EtapaArnes::VerificacionNoDisponible` con el motivo, para la telemetría y la GUI.
- **Por qué falla cerrada:** fallar abierto permitiría que una cifra de memoria pasara justo cuando Haiku no responde, y eso rompería las trampas.

### 4.6 Cómo se impide que acepte una cifra de memoria

El LLM **propone** la justificación y **Rust la comprueba** (`arnes_turno/dictamen.rs`, puro). Solo pasa lo que Rust puede verificar:

1. **`Calculo`:**
   - cada operando debe estar anclado **en el extracto que cita** (o en la pregunta);
   - Rust recalcula la operación;
   - el resultado debe coincidir con la cifra del borrador, con una tolerancia de media unidad de su último decimal mostrado;
   - como máximo 6 operandos.

   Un operando que no está en su extracto, o un resultado que no cuadra, hacen que la justificación no sea válida.
2. **`EquivalenciaTextual`:** `fragmento_literal` debe aparecer **literalmente** (comparando esqueletos, como en `verificador_paginas`) en el extracto citado. Cubre las cifras escritas en palabras, los ordinales y los números romanos, en cualquier idioma, sin listas.

   **Límite reconocido:** que el fragmento signifique esa cifra lo afirma el LLM y Rust no lo comprueba. Para colar una cifra de memoria, tendría que señalar un fragmento real y leerlo mal.
3. **`NoRespaldada`,** o cualquier cifra sin dictamen: es una violación.
4. **`ConclusionSinConsulta`:** si `afirma_sobre_documentos` es verdadero, o no viene en el dictamen, se produce un reintento con «consulta las herramientas antes de afirmar o negar».
5. **`RolNoDeclarado`:** solo pasa con una `CitaLiteral` que exista en un extracto.
6. **Prompt fijo del verificador:** lo que no está en los extractos o en la pregunta es `NoRespaldada`, aunque sea conocimiento general. No tiene tools ni historial.
7. **El extractor de cifras y el anclaje siguen siendo deterministas.** El LLM solo puede **añadir** aceptaciones que Rust compruebe; nunca quita un rechazo sin evidencia comprobada.

**Riesgo residual:** una coincidencia aritmética casual, como una cifra de memoria que coincide con `a − b` de dos cifras recuperadas. La limitan dos condiciones: los operandos deben estar **en el extracto citado** y la operación la declara el modelo, no una búsqueda exhaustiva de Rust. Por eso **no** se buscan combinaciones aritméticas a ciegas: con unos 200 números en las salidas, casi cualquier cifra sería la diferencia de dos de ellos.

### 4.7 Mantener deterministas las 8 trampas y los 9 «no consta»

| Hoy | Después |
|---|---|
| Trampa respondida con el valor del documento: está anclada y pasa | Igual: está anclada y no hay microllamada |
| Cifra de memoria en una respuesta con intención tabular: `CifraSinAnclar`, reintento | Sin anclar → microllamada → sin justificación verificable → reintento. **Mismo resultado, en cualquier intención e idioma** |
| «No consta» sin cifras: pasa | Igual: sin sospechas no hay microllamada |
| «No consta» + valor de referencia: `CifraEnAbstencion` (solo si la locución está en la lista española) | Sin anclar → no verificable → reintento. **Más cobertura:** también con «not stated» |
| Cálculo correcto (V53): reintento, y a veces se pierde | `Calculo` comprobado por Rust: pasa sin intervención |
| Artículo citado con otra forma escrita (V22, supuesto) | `EquivalenciaTextual` con fragmento real: pasa. **Si el número de artículo sale de memoria y no está en los extractos, se sigue rechazando.** No se sabe cuál de los dos casos fue V22 (el informe solo dice «no figuraban literalmente») |

### 4.8 Archivos

- `src/application/ports/verificador_afirmaciones.rs` (nuevo)
- `src/application/use_cases/arnes_turno/dictamen.rs` (nuevo)
- `src/application/use_cases/arnes_turno/post_inference_guard.rs`
- `cifras.rs`
- `fechas.rs`
- `pre_inference_interceptor.rs` (`retener_texto`)
- `agent_orchestrator.rs` (cascada y evento)
- `src/infrastructure/llm/claude_headless_verificador.rs` (nuevo)
- `src/infrastructure/bootstrap.rs`
- `src/presentation/gui/intervenciones_arnes.rs` (etapa nueva)
- `tests/adversarial_arnes_tests.rs`
- `tests/integration_chat_arnes.rs`

### 4.9 Plan TDD

Todos los tests usan `VerificadorFalso`, que devuelve dictámenes fijados por cada test.

**Rojo, `dictamen.rs`:**
1. `Calculo` correcto: `Resta` de dos celdas de una tabla financiera EN da la diferencia mostrada → válido.
2. Operando inexistente en su extracto → inválido.
3. Resultado que no cuadra → inválido.
4. Redondeo a 1 decimal → válido.
5. `EquivalenciaTextual` con «doce» (ES), «twelve» (EN), «XII» y «十二» (ZH), cuyo fragmento existe → válido.
6. Fragmento inventado → inválido.
7. `NoRespaldada` → inválido.
8. Dictamen sin la cifra → inválido.

**Rojo, cascada en el orquestador:**
1. Trampa sintética: el documento dice 412 y el modelo da 400 (de memoria). El falso verificador devuelve `NoRespaldada` → reintento y, al segundo fallo, salida segura.
2. La misma trampa, con un falso verificador que **intenta** colar 400 como `Calculo` con operandos inexistentes → Rust lo rechaza.
3. «Not stated, but typically 20 MPa» (EN), sin lista léxica → reintento.
4. «No consta» sin cifras → 0 microllamadas.
5. Cálculo válido → se entrega sin `ArnesIntervino`.
6. El verificador devuelve un error o agota el tiempo → falla cerrada y etapa `VerificacionNoDisponible`.
7. 0 tools y la respuesta afirma hechos → reintento; saludo con `afirma_sobre_documentos: false` → se entrega.
8. Respuesta en alemán, japonés y griego con cifras ancladas → 0 microllamadas.

**Regresión:** la suite actual de `post_inference_guard` (`:515-1508`) se revisa test a test:
- los de anclaje y ejes deben seguir en verde;
- los de `CapaDosNoIntentada` y de locuciones se **reescriben** como casos de R2 y se documenta cada uno en el PR.

### 4.10 Riesgos de retroceso y medición

| Riesgo | Medición | Criterio |
|---|---|---|
| Se cuela una cifra de memoria | Tests de trampas sintéticas y `acceptance_8_problemas` | 100 % de los casos de trampa rechazados |
| Más intervenciones por anclar en todas las intenciones | Repuntuación de las respuestas guardadas con R2 nuevo: número de turnos con sospecha | Informado antes de implementar el adaptador. Si supera el 30 % de los turnos, se revisa la extracción |
| Latencia | Tiempo de la microllamada, 3 o más ejecuciones (el equipo tiene carga) | Informado, no bloquea |
| Retroceso en «no consta» | Las preguntas sin respuesta de los benchmarks propios | Ninguna cifra en una abstención |

---

## 5. Protocolo de validación rápida

### 5.1 Principio

Cada pieza se mide primero **sin inferencia de Claude** y en minutos. El set oculto (unas 2,5 h) solo se usa al final, y lo ejecuta la sesión evaluadora aislada. **Esta sesión no ha leído `docs/validacion/set_oculto*`.**

| Nivel | Coste de Claude | Qué mide | Piezas |
|---|---|---|---|
| N0 | 0 | `cargo test`, `clippy`, `fmt` | Todas |
| N1 | 0 | Extracción y compuerta en un corpus público | 1 |
| N2 | 1 ingesta por documento, una vez | Anclaje de páginas de tabla, repetido sin coste | 2 |
| N3 | 0 | Recuperación pura por MCP | 3a |
| N4 | 0 | Repuntuar respuestas guardadas (`--ask --json` guarda las tools) | 3b, 3c y 4 |
| N5 | Claude (Aguja, ~78 min) | Campaña con el modelo | 3c y 4 |
| N6 | Claude (~2,5 h) | Set oculto v2, **sesión aislada** | Todas, al final |

### 5.2 Corpus público para N1 (no está en el set oculto)

Entre 12 y 20 PDF públicos, descargados a `.cache/datasets/legibilidad/`, registrados en `benchmarks/manifiesto_datasets.json` con su sha256 (como `aguja_base`) y **sin versionar su texto**. Deben cubrir:
- artículos LaTeX con fuentes Type1/CFF, el caso que motivó la pieza;
- griego, cirílico, CJK, árabe y devanagari;
- un PDF con muchas fórmulas;
- unos estados financieros con muchas tablas;
- un PDF con iconos en la PUA;
- uno escaneado sin texto;
- MIL-STD-882E (`aguja_base`).

**Exclusión:** no conocemos los PDF del set oculto. Se pide a la sesión evaluadora una lista de **sha256** de su corpus (sin nombres). Cualquier PDF que coincida se sustituye.

**Herramienta:** `cargo run --example medir_extraccion -- <carpeta>`. Es sencilla, conforme a §4 de `CLAUDE.md`. Por cada PDF y página da el hash del texto, las métricas y el veredicto, en formato JSONL, en unos segundos. Antes y después se comparan los hashes y los veredictos.

### 5.3 N2: anclaje de páginas de tabla

- Se crea **una vez** un centro de medición con PDF públicos que tengan tablas: MIL-STD-882E (sus matrices de riesgo) y 2 o 3 del corpus de §5.2. Así se paga la destilación una sola vez.
- **Herramienta:** `cargo run --example medir_anclaje -- <centro>` lee 02 (texto y saltos) y 05 (celdas) del centro **en solo lectura**, ejecuta `anclar_tabla` y escribe la página de cada fila.
- **Oráculo:** la página de cada fila se verifica con un script en Python (`pypdf`) que busca la coincidencia única de los valores de la fila. Si es ambigua, el caso se excluye y se informa.
- **Métricas:** páginas falsas (criterio: 0) y cobertura (porcentaje de filas con página, informado).
- Iterar sobre el algoritmo **no** exige volver a ingerir.

### 5.4 N3: cuota multidocumento

- Centro de medición con varios documentos: el de §5.3, más `benchmarks/fixtures/ciclo_de_vida` y `rgb_industrial`.
- **Herramienta:** un script en Python que llama a `search_normative_context` por MCP (`benchmarks/lakehouse/mcp.py`, como `run_aguja._recuperacion_directa`), sin Claude.
- **Preguntas de un solo documento:** las del benchmark Aguja y las de RGB. Métrica: recall@8 del fragmento de referencia. Criterio: **no baja**.
- **Preguntas cruzadas sintéticas:** concatenan una pregunta de A y otra de B. Métrica: porcentaje de respuestas con fragmentos de los dos documentos en el top 8. Se espera que suba; se informa.

### 5.5 N4 y N5

- **N4:** `benchmarks/evaluacion/repuntuar.py` se amplía con un modo que vuelve a ejecutar sobre los JSON guardados:
  - el verificador de páginas (3c), contando las notas antes y después y revisando a mano las eliminadas;
  - el detector de llamadas en crudo (3b);
  - R2 de la Pieza 4, contando los turnos con sospecha.

  Necesita un binario auxiliar `--revisar-guardado` o un test `#[ignore]` que lea los JSON. No usa Claude.
- **N5:** Aguja 5 × 3, igual que la línea base de `benchmarks/informes/aguja_citas_pagina_20261110.md`.

### 5.6 Línea base y criterios de no retroceso (`CLAUDE.md` §6)

**Antes de cada pieza,** en `main` (`62f3010`):
- `cargo test` completo, contando aprobados e ignorados;
- `clippy --all-targets -D warnings` y `fmt --check`;
- los niveles N1 a N4 que correspondan, guardados en `benchmarks/informes/linea_base_generalidad_<pieza>.md`.

| Pieza | Criterio de no retroceso (cualquier empeoramiento bloquea el merge) |
|---|---|
| 1 | Todos los tests verdes. Hash de texto igual en toda página sin fuentes sin ToUnicode. 0 páginas dudosas en los PDF legibles. Aguja: `recall_aguja`, `pagina_exacta_estricta` y `seccion_real` iguales |
| 2 | 0 páginas falsas en N2. RGB y `acceptance_8_problemas`: la recuperación de tablas no baja. Un centro v6 se rechaza con mensaje |
| 3a | Recall@8 de un solo documento sin bajar. Centro de un documento: salida idéntica |
| 3b | 0 detecciones en las respuestas correctas guardadas |
| 3c | Las `Discrepante` verdaderas de los tests se mantienen. Las notas eliminadas en N4 no eran errores reales (revisión manual). Aguja: 0 notas nuevas |
| 4 | Trampas sintéticas: 100 % rechazadas. Benchmarks propios: 0 cifras inventadas (como en `rgb_arnes_corregido_20261109.md`). Abstenciones sin cifras. Ningún «no consta» se pierde |

Los tiempos son orientativos: el equipo del usuario tiene carga. Se dan con 3 o más ejecuciones y nunca se concluye con una sola.

### 5.7 Qué debería medir la sesión evaluadora aislada

**Al final, no por cada cambio:**
- **Subconjunto rápido** (se propone; la sesión evaluadora elige las preguntas sin decírnoslas): las preguntas de las 4 que fallan en `main`, por categoría (cálculo en tabla, dato repartido legal, PDF académico); las 8 trampas; las 9 sin respuesta; y las multidocumento. Unas 25 preguntas en 3 pasadas, que dan más señal que 61 en 2.
- **Ingesta:** el número de documentos `LECTURA_DUDOSA` en su corpus y si coinciden con los ilegibles reales.
- **Si el set v2 se usa para decidir un merge, deja de ser held-out.** Habrá que escribir el v3, como ya propone `situacion_rag_20261111.md` §7.

---

## 6. Independencia y orden

| Pieza | Archivos compartidos | Puede ir en paralelo con |
|---|---|---|
| 1 | `mapeo_destilacion.rs` (`mapear_con_estado`), `estructura_documental.rs`, regla 7 del prompt | 3a, 3c |
| 2 | `mapeo_destilacion.rs` (`vincular_tablas`, otras funciones), esquemas, **v7** | 3a, 3c. Con la 1 es posible, pero una sola de las dos lleva la subida de versión |
| 3a | `busqueda.rs` | Todas |
| 3b | `post_inference_guard.rs`, `agent_orchestrator.rs`, GUI | 1, 2, 3a, 3c. **No con la 4** |
| 3c | `verificador_paginas.rs` | Todas |
| 4 | `post_inference_guard.rs`, `agent_orchestrator.rs`, `cifras.rs`, `fechas.rs` | 1, 2, 3a, 3c. **Después de la 3b** |

**Orden recomendado:**
1. **3c + 3a + 1, en paralelo.** La 3c es pequeña y quita ruido. La 3a es independiente y se mide en N3. La 1 corrige 4 de los 8 fallos y fija el valor `LECTURA_DUDOSA`.
2. **2.** Sube a v7, incluye el valor de la Pieza 1 y se recrean los centros una sola vez.
3. **3b.** Crea `llamada_en_crudo.rs`, el evento `TextoEnVivoDescartado` y el invariante R1.
4. **4.** El mayor riesgo. Antes de escribir el adaptador, se repuntúa R2 en N4 para conocer la frecuencia de microllamadas.

---

## 7. Lo que este SSD no cubre (seguimiento)

- Las listas de `intent_detector.rs`. Dejan de bloquear, pero siguen eligiendo el primer paso.
- `PALABRAS_DE_PAGINA` (`verificador_paginas.rs:29`).
- `INICIOS_DE_NOTA` y `PALABRAS_DE_CAPITULO` (`pdf_estructura.rs:33` y `:48`): heurísticas de estructura en español e inglés, ajenas a estas 4 piezas.
- La página impresa (`/PageLabels`).
- Tablas por regiones geométricas (requiere pdfium).
