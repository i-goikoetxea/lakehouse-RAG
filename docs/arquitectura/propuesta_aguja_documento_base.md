# Propuesta: documento base y diseño de la pista Aguja

Estado: **propuesta pendiente de aprobación** (2026-11-09). No se ha descargado ni ingerido nada. Desarrolla el §4.2 de `ssd_benchmark_runner.md`.

## 1. Candidatos

Las licencias se han leído en la fuente oficial, en páginas HTML. Ningún PDF se ha abierto. Por eso el número de columnas, los marcadores, `/PageLabels` y `/Encrypt` quedan **sin verificar**: se comprueban tras la aprobación (§3).

| # | Documento | Editor | Págs. | Idioma | Licencia | Encaje con la app |
| :- | :--- | :--- | ---: | :- | :--- | :--- |
| A | MIL-STD-882E Change 1, *Standard Practice: System Safety* (27-09-2023) | DoD (ASSIST, DLA) | 106 (ficha oficial) | EN | Dist. A + 17 U.S.C. §105 | Alto: norma de seguridad de sistemas, matrices de riesgo y tareas numeradas |
| B | NIST SP 800-37 Rev. 2, *Risk Management Framework* (12-2018) | NIST | 183 (portadilla, vista en una copia de terceros) | EN | Declaración de NIST que permite obras derivadas | Medio: marco normativo de procesos, no industrial |
| C | Reglamento (UE) 2023/1230, relativo a las máquinas | UE (EUR-Lex, DO L 165, pp. 1-102) | 102 | ES (+23) | CC BY 4.0 / Decisión 2011/833/UE | Alto en dominio; bajo en estructura (artículos) |
| D | *Guía técnica para la evaluación y prevención de los riesgos relativos a la utilización de los equipos de trabajo* (RD 1215/1997, ed. 2021) | INSST | **desconocido**, quizá > 200 | ES | Aviso legal del INSST: permite modificar | Muy alto en dominio; estructura y longitud sin confirmar |
| E | NASA-STD-8739.4A Change 4, *Workmanship Standard for Crimping, Interconnecting Cables, Harnesses, and Wiring* | NASA (OSMA) | 113 (GlobalSpec, fuente secundaria) | EN | «Internet Public»; dominio público en EE. UU., sin declaración específica para normas | Alto: requisitos de taller numerados |

Descartados:
- **NIST SP 800-82 Rev. 3**: unas 316 páginas.
- **RD 513/2017**: 38 páginas en el BOE.
- **RITE**: 54 páginas en el BOE.
- **CTE DB SE-A**: su aviso legal es un PDF que no se abrió, así que la licencia no está verificada.
- **Reglamento (UE) 2024/3110**: publicado acto por acto, sin rango de páginas en la referencia.

### A. MIL-STD-882E Change 1
- **Ficha:** https://quicksearch.dla.mil/qsDocDetails.aspx?ident_number=36027. Activa, revisión prevista para 25-09-2028.
- **PDF:** `https://quicksearch.dla.mil/ImageRedirector.aspx?token=5784600.36027`, con 106 páginas y Dist. A. La Rev. E de 2012 tiene 104 páginas y el token `5715775.36027`.
- **Licencia:**
  - https://assist.dla.mil/Online/help/dist_stmt.cfm: «DISTRIBUTION STATEMENT A. Approved for public release.» Esos documentos «may be made available or sold to the public and foreign nationals».
  - Al ser obra del Gobierno federal, no tiene copyright en EE. UU. (17 U.S.C. §105).
  - Ninguna fuente prohíbe modificarla. Hay que confirmar la leyenda en la portada.
- **Ventajas:**
  - Numeración decimal (`4.3.1`) y tareas `101`-`400` con subapartados decimales, que es lo que mejor reconoce `pdf_estructura.rs`.
  - Tablas de severidad, probabilidad y matriz de riesgo.
  - Longitud justa, que acota el coste.
- **Riesgos:**
  - El PDF se sirve por un redirector con token, no por una URL de archivo estable. Se mitiga fijando sha256 y token.
  - La cabecera `MIL-STD-882E` se repite en cada página.
  - Es probable que las páginas preliminares lleven numeración romana, así que la página impresa y la física no coinciden.
  - Los apéndices A y B pueden reiniciar la numeración, y las tareas `101.2.1` mezclan esquemas.
  - Puede haber formularios o figuras sin texto.

### B. NIST SP 800-37 Rev. 2
- **Ficha:** https://csrc.nist.gov/pubs/sp/800/37/r2/final
- **PDF:** https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-37r2.pdf, con DOI 10.6028/NIST.SP.800-37r2.
- **Licencia:** https://www.nist.gov/open/copyright-fair-use-and-licensing-statements-srd-data-software-and-technical-series-publications
  - «Works authored by NIST employees are not subject to Copyright protection within the United States».
  - Concede «the non-exclusive, perpetual, paid-up, royalty-free, worldwide right to reprint works in all formats», incluidas las obras derivadas.
  - Pide añadir «Republished courtesy of the National Institute of Standards and Technology.»
  - Los derechos fuera de EE. UU. quedan reservados, pero la concesión los cubre.
- **Ventajas:**
  - Es la URL más estable (DOI) y la licencia más explícita sobre obras derivadas.
  - Los PDF de NIST suelen traer marcadores. Está sin verificar.
  - Tiene muchas tablas y apéndices A-H.
- **Riesgos:**
  - Tiene 183 páginas, casi el doble de texto que A, así que la destilación agotará casi con seguridad los 300 s (§4).
  - Las tareas `P-1`, `C-2`… no son numeración decimal.
  - Cabecera y pie largos y repetidos, con la URL del DOI.
  - Preliminares en romanos.
  - El dominio es la ciberseguridad, no la industria.

### C. Reglamento (UE) 2023/1230 (máquinas)
- **Ficha:** https://eur-lex.europa.eu/legal-content/ES/TXT/?uri=CELEX:32023R1230
- **PDF:** https://eur-lex.europa.eu/legal-content/ES/TXT/PDF/?uri=CELEX:32023R1230
- **ELI:** http://data.europa.eu/eli/reg/2023/1230/oj
- **Licencia:** aviso jurídico de EUR-Lex, https://eur-lex.europa.eu/content/legal-notice/legal-notice.html
  - «you can re-use the legal documents published in EUR-Lex for commercial or non-commercial purposes».
  - «you can re-use the content provided you acknowledge the source and indicate any changes you have made».
  - Por la Decisión 2011/833/UE, art. 6.2.b: «la obligación de no deformar el contenido o significado original de los documentos».
  - CC BY 4.0 permite «remezclar, transformar y construir a partir del material».
  - Solo el DO publicado es auténtico, así que la página insertada debe declararse ajena.
- **Ventajas:**
  - Está en español y en el dominio exacto: seguridad de máquinas y anexo III de requisitos esenciales.
  - Hay 24 versiones lingüísticas, lo que permite una variante en inglés con el mismo esqueleto.
- **Riesgos:**
  - El parser **no reconoce `Artículo N`** como encabezado: solo admite decimales, romanos, `Capítulo`/`Chapter` y `ANEXO`. Sin marcadores en el PDF, el cuerpo quedaría en muy pocas secciones y `seccion_real` mediría poco.
  - Los considerandos `(1)`…`(100+)` ocupan unas 20 páginas iniciales.
  - La cabecera `DO L 165/…`, con fecha, se repite en cada página.
  - Con 102 páginas está justo en el límite inferior.

### D. Guía técnica de equipos de trabajo (INSST)
- **Novedad:** https://www.insst.es/el-instituto-al-dia/actualizacion-guia-tecnica-equipos-de-trabajo
- **PDF:** https://www.insst.es/documents/94886/789467/Gu%C3%ADa+t%C3%A9cnica+para+la+evaluaci%C3%B3n+y+prevenci%C3%B3n+de+los+riesgos+relativos+a+la+utilizaci%C3%B3n+de+los+equipos+de+trabajo.pdf (11,5 MB)
- **Licencia:** https://insst.es/informacion-general/aviso-legal
  - Autoriza «la copia, difusión, modificación, adaptación, extracción, reordenación y combinación de la información».
  - «Está prohibido desnaturalizar el sentido de la información.»
  - Hay que citar «Origen de los datos: Instituto Nacional de Seguridad y Salud en el Trabajo».
- **Ventajas:** es el más parecido a los documentos reales del usuario.
- **Riesgos:**
  - **Se desconoce el número de páginas.** Una copia de terceros tiene un índice que llega a la p. 204, y la edición de 2021 podría superar 200.
  - Es probable que tenga recuadros, texto del RD intercalado con comentarios y figuras, lo que da columnas y orden de lectura dudoso.
  - Es el candidato con más riesgo de extracción.

### E. NASA-STD-8739.4A Change 4
- **Ficha:** https://standards.nasa.gov/standard/NASA/NASA-STD-87394
- **PDF:** https://standards.nasa.gov/sites/default/files/standards/NASA/A/4/nasa-std-87394a_w_change_4_0.pdf
- **Licencia:**
  - Marcado «Internet Public» («cleared for public accessibility on the internet»).
  - https://www.nasa.gov/nasa-brand-center/images-and-media/: el contenido de NASA «generally are not subject to copyright in the United States».
  - Esa declaración trata de medios, **no de normas**: es la licencia más débil del grupo.
- **Riesgos:**
  - Abundan las fotografías de engastes, con poco texto por página.
  - El número de páginas solo consta en una fuente secundaria.

## 2. Recomendación

**A, MIL-STD-882E Change 1**, con **B** como reserva.

- **Estructura:** de los candidatos con licencia firme, A tiene la que mejor reconoce el parser (decimal y tablas). Así, `seccion_real` mide la app y no un límite conocido del parser. Con C pasaría lo segundo, por los artículos.
- **Coste:** 106 páginas es el menor texto del grupo. Reduce el riesgo de agotar los 300 s de destilación y abarata cada repetición.
- **Licencia sin ambigüedad:** Dist. A, sin copyright en EE. UU. y nada que prohíba modificar.
- **Inconvenientes:**
  - Está en inglés, frente a la preferencia de la app por el español.
  - El redirector con token es menos estable que el DOI de B.

Si el español es prioritario, la alternativa es **D**, solo si tras descargarla tiene 200 páginas o menos y una sola columna. **C** solo vale si el PDF de EUR-Lex trae marcadores. Si no, conviene esperar a que el parser reconozca `Artículo`, que es otra tarea.

## 3. Verificación tras la aprobación, antes de fijar el sha256

Se descarga el documento en una carpeta de cuarentena vacía, fuera del repositorio, y un script aislado (`python -I`) comprueba:
1. El número de páginas, entre 100 y 200.
2. Que la media de caracteres por página es ≥ 50, el mismo umbral que `MEDIA_MINIMA_CARACTERES_POR_PAGINA`.
3. Que **no hay `/Encrypt`**: el parser rechaza cualquier PDF que lo declare, aunque la contraseña esté vacía.
4. Si hay `/Outlines` y `/PageLabels`.
5. La primera página del cuerpo, tras el índice.
6. La leyenda de licencia en la portada.

Solo entonces se fijan la URL, el sha256 y la fecha de acceso en `benchmarks/`. El PDF no se versiona.

## 4. Borrador de la pista (sin implementar)

### Aguja
- **Valor sintético inequívoco:** una cifra con unidad real y un código de lote que no existe en el documento base. En el idioma del documento: *«The tightening torque for anchor bolts of batch AGJ-7Q3K shall be 487.3 N·m.»*
  - El lote tiene 4 caracteres de un alfabeto sin ambiguos (sin `0/O/1/I`).
  - El valor va de 100,0 a 999,9, con un decimal.
  - Se comprueba que ni el lote ni el valor aparecen en el texto extraído del documento base.
- **Página insertada con título propio:**
  - Título sin numerar, por ejemplo *«Supplementary technical note NT-AGJ-7Q3K»*. Así no se cuela en la heurística de numeración ni rompe la secuencia decimal.
  - Dos o tres párrafos de relleno neutro con la cifra en medio, para que no sea la primera línea.
  - Pie: *«Página insertada para pruebas; no forma parte del documento original»*. Lo exigen las licencias (indicar cambios y no aparentar texto oficial).
  - No se añade marcador a `/Outlines`.
- **Sección real esperada:** la sección que contiene el punto de inserción.
  - Con `/Outlines`, se calcula con pypdf: es el último marcador cuya página es anterior o igual a la página previa a la aguja.
  - Sin marcadores, sale de una ingesta de control del documento base sin aguja. El árbol del parser es determinista, así que basta una vez por sha256.
  - Esto mide si la app cita la sección real y no un título inventado.
- **Página física esperada:**
  - Con `N` páginas originales y una profundidad `p`, se toma `k = max(primera_pagina_del_cuerpo, round(p·N))`. Se construye un `PdfWriter` con las páginas `[0, k)`, la aguja y `[k, N)`.
  - La página esperada es `k + 1`, con base 1, que es la que usa la app (`pagina_inicio`).
  - Tras escribir el PDF, se relee la página `k + 1` y se exige que contenga el lote.
  - Se registran la profundidad real `k/N` y la página impresa vecina, para detectar si Claude cita la numeración impresa en vez de la física.
  - `/PageLabels` no se actualiza, pero la app no lo usa.
- **Variación por profundidad (SSD §4.2):**
  - El lote y el valor salen de una semilla determinista `sha256(semilla_campaña | repetición | profundidad)`. Cada profundidad y cada repetición tienen aguja distinta, y la campaña es reproducible.
  - Las plantillas de frase (par de apriete, presión de prueba, temperatura máxima…) rotan con la misma semilla.
- **Preguntas por centro:**
  1. **Léxica**, con el lote.
  2. **Semántica**, sin el lote. No es ambigua porque hay una sola aguja por centro.
- **Centros:** uno por profundidad y repetición. Las cinco variantes comparten `doc_id`, y en un mismo centro se tratarían como revisiones y se derogarían entre sí.
- **Runner:** `_preparar_centro` hoy escribe con `write_text`. La aguja necesita documentos binarios (`write_bytes`). Es un cambio pequeño en `run.py`, a validar antes de implementarlo.
- **Métricas:**
  - `recall_aguja`: el valor normalizado está en la respuesta. Se aceptan `487.3` y `487,3`.
  - `pagina_exacta`: la página de la cita o de `search_normative_context` es `k + 1`.
  - `seccion_real`: el breadcrumb o el `nodo_id` coincide con la sección esperada.
  - Se informa además `estado_contenido` (definitivo o provisional), porque una destilación agotada no impide encontrar la aguja por los chunks.

### Coste estimado (sin medir; requiere un piloto)

Supuestos:
- Unos 3.000 caracteres por página, unos 4 caracteres por token y chunks de 512 tokens.
- Embeddings E5-large en CPU a ~0,8 s por texto: 13 textos tardaron ~10 s en la Fase 11.
- Respuesta de `--ask`: ~33 s de media (informe del 2026-11-09).
- La destilación envía el documento **entero** en una llamada, con un máximo de 300 s.

| | A (106 págs.) | B (183 págs.) |
| :--- | ---: | ---: |
| Texto por ingesta | ~320 k caracteres, ~80 k tokens | ~550 k caracteres, ~140 k tokens |
| Destilación | 2-5 min; riesgo real de agotar 300 s y quedar provisional | Casi seguro que agota 300 s |
| Embeddings (~160 / ~270 chunks + hitos) | ~2-3 min | ~4-5 min |
| Ingesta | ~5-8 min | ~9-11 min |
| **Por repetición** (5 ingestas + 10 preguntas) | **~30-45 min**, ~0,4 M tokens de entrada a Claude | **~50-60 min**, ~0,7 M tokens |
| 3 repeticiones | ~1,5-2,25 h | ~2,5-3 h |

Antes de la campaña, un piloto de **una sola profundidad (50 %)** mide el tiempo y si la destilación termina. Si agota el tiempo en todas las ingestas, se decide entre:
- aceptar el contenido provisional y medirlo así;
- subir el tiempo máximo;
- abordar la destilación por secciones (`decisiones_stair.md` §5, «Destilación»).

## 5. Pendiente de decisión del usuario
1. El documento base: A, B, C, D o E.
2. Su descarga para la verificación del §3.
3. Si el piloto de una profundidad se ejecuta antes de implementar la pista completa.
