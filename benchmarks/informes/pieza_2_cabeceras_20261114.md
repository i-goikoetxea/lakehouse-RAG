# Pieza 2, ajuste: cabeceras multinivel y potencias en el anclaje de tablas (14-11-2026)

Rama `claude/pieza-2-cabeceras-96501f`, sobre `main` (`ad1fbb3`). Contrato: `docs/arquitectura/ssd_generalidad_4_piezas.md` §2.3.2, con las reglas aprobadas por el mentor. **No se ha leído `docs/validacion/set_oculto*`. No se ha tocado ningún centro ni se ha destilado nada: N2 reutiliza los paquetes Sonnet cacheados de la sesión anterior.**

**Alcance del resultado: «funciona en los casos conocidos», no «funciona en general».** Los sintéticos los diseñó la misma sesión que escribió las reglas, y solo hay 3 PDF públicos, todos en inglés.

## 1. Errores y retrocesos (primero)

**Medido:**
- **Páginas falsas: 0.**
  - Sintéticos: 0 en 9 semillas, 38 213 filas. Las 6 semillas antiguas dan unas 21 000 filas de los tipos previos, y se suman 6 210 filas de tipos nuevos.
  - PDF públicos: 0. Hay 126 celdas con página: las 95 previas, más 31 de la Tabla 2 del paper de Transformer.
- **Retrocesos: 0.**
  - Ninguna fila sintética que tenía página la cambia ni la pierde (comparación fila a fila en las 9 semillas).
  - Ninguna celda de PDF que tenía página cambia: 95 de 95 iguales.
  - Ninguna tabla que tenía página baja de cobertura.
- **Error de diseño encontrado durante el desarrollo: la trampa de prosa se activó una vez, con la regla del mentor tal como estaba escrita.**
  - Fue en la semilla de depuración 20261111, caso griego: 8 filas recibieron página.
  - No eran páginas falsas, porque las filas sí estaban en esa página. Pero la frase de prosa activó `C > 0` con las palabras de una sola columna.
  - Causa: la cabecera «ο Μέγιστη» se reduce a {Μέγιστη} al ignorar el artículo de 1 letra. Ese conjunto está dentro de la columna de la frase, «λειτουργίας Μέγιστη», así que las dos contaban como «2 columnas distintas».
  - Corrección general, que solo endurece la regla: la unión de los tokens de las cabeceras presentes no puede caber en los de una sola cabecera. Tiene su propio test rojo, con otros datos (castellano, «e Máximo» dentro de «Carga Máximo»).
  - Después, 0 de 2 157 filas trampa con página en las 9 semillas.
- **Un test adversarial mío estaba mal escrito:** sus cabeceras «y» y «o» aparecían contiguas, y la regla antigua ya las contaba. Lo corregí para que solo la composición pudiera activarlas. El código no cambió.
- **Incidencia de entorno:** el disco D: se llenó (quedaban 634 MB) durante la primera compilación de la línea base. Borré solo el `target/` parcial de este worktree y compilé en C:. No toqué los `target/` de otras sesiones. D: sigue casi lleno, unos 5 GB libres, y puede afectar a otras sesiones.

**Limitaciones (cobertura, no errores):**
- **MIL-STD-882E sigue en 0 de 44.** Son matrices de riesgo con palabras repetidas, y este ajuste no las toca.
- **Tabla 1 del paper: 6 de 12 sin página, igual que antes.** No está diagnosticado.
- **Variedad corta en PDF reales:** 3 documentos, todos en inglés. Solo uno, la Tabla 2, tiene cabecera multinivel. La regla multinivel está **probada en un único PDF real**.
- **El oráculo no es independiente:** comparte la idea de «clave + valores cerca» y no conoce la variante de potencias. En la Tabla 2 da 27 celdas puntuables. Las 31 se comprobaron a mano contra el texto de pdfium: todas sus filas están en la p. 8.
- **CJK:** con un token por carácter, una frase trampa puede contener por azar los caracteres de dos columnas. En estas semillas no pasó. No hay un tipo sintético que lo fuerce.
- **Los sintéticos no tienen potencias.** La regla de potencias solo está probada con tests unitarios y con la Tabla 2.
- No se ha medido con Claude: ni `acceptance_8_problemas` ni RGB.

## 2. Antes y después

«Antes» es `main` (`ad1fbb3`), medido en esta sesión con el mismo binario de ejemplo. Las 6 semillas antiguas reproducen exactamente los números del informe del 11-11.

### PDF públicos (N2, sin Claude, paquetes Sonnet cacheados)

| Tabla | Celdas | Antes: correctas / None / falsas | Después: correctas / None / falsas |
|---|---:|---|---|
| Paper de Transformer, Tabla 1 | 12 | 6 / 6 / 0 | 6 / 6 / **0** |
| **Paper de Transformer, Tabla 2** | 31 | **0 / 31 / 0** | **31 / 0 / 0** (oráculo: 27 de 27) |
| Paper de Transformer, Tabla 4 | 12 | 12 / 0 / 0 | 12 / 0 / **0** |
| Fed H.4.1 | 77 | 77 / 0 / 0 | 77 / 0 / **0** |
| MIL-STD-882E (2 tablas) | 44 | 0 / 44 / 0 | 0 / 44 / **0** |
| **Total** | 176 | 95 / 81 / 0 | **126 / 50 / 0** |

La Tabla 2 pasa de 0 de 26 a **27 de 27 puntuables por el oráculo** (el informe del 11-11 contaba 26 puntuables; con el oráculo de hoy son 27). En total, las 31 celdas tienen la p. 8, que es correcta. Las celdas «correctas» de la Fed se comprobaron a mano en el informe anterior y no cambian.

Sin la regla de potencias, 2 de las 10 filas de la Tabla 2 se quedarían sin página: son las que solo tienen un valor además de la potencia. Así se ve en el diagnóstico, no en una ejecución aparte.

### Sintéticos (N2, sin Claude)

Hay tres tipos nuevos, en un flujo aleatorio aparte. Así, los 400 casos antiguos de cada semilla son idénticos, y está comprobado:
- `multinivel_2`: 2 niveles en 2 líneas;
- `multinivel_3`: 3 niveles;
- `trampa_prosa`: sin cabecera física y con una frase de prosa justo antes de las filas, que contiene, desordenadas, las palabras de una sola columna. Debe quedar en `None`.

Las unidades van en una línea aparte en el 60 % de los casos y se añaden a la cabecera destilada la mitad de las veces. Las tablas ocupan 1 o 2 páginas, con la cabecera repetida. Se cubren las mismas 8 escrituras.

Semillas:
- **Depuración:** 20261111.
- **Validación, no miradas al depurar:** 1 a 5, que la sesión anterior usó para validar, y 6, 7 y 8, nuevas.

| Semilla | Filas | Antes: correctas / None / falsas | Después: correctas / None / falsas |
|---|---:|---|---|
| 20261111 (depuración) | 4299 | 3177 / 1122 / 0 | 3675 / 624 / **0** |
| 1 | 4211 | 3154 / 1057 / 0 | 3621 / 590 / **0** |
| 2 | 4232 | 3100 / 1132 / 0 | 3596 / 636 / **0** |
| 3 | 4219 | 3106 / 1113 / 0 | 3590 / 629 / **0** |
| 4 | 4213 | 3104 / 1109 / 0 | 3581 / 632 / **0** |
| 5 | 4297 | 3230 / 1067 / 0 | 3695 / 602 / **0** |
| 6 (nueva) | 4202 | 3122 / 1080 / 0 | 3610 / 592 / **0** |
| 7 (nueva) | 4203 | 3122 / 1081 / 0 | 3565 / 638 / **0** |
| 8 (nueva) | 4337 | 3210 / 1127 / 0 | 3702 / 635 / **0** |
| **Total** | 38 213 | 28 325 / 9 888 / 0 | **32 635 / 5 578 / 0** |

Por tipo, en las 9 semillas:
- `multinivel_2` y `multinivel_3`: de 0 % a 100 % (4 310 filas);
- `trampa_prosa`: 0 con página antes y después (2 157 filas en `None`);
- los 10 tipos antiguos: idénticos fila a fila.

**El 100 % de los multinivel está saturado.** Solo dice que el generador produce lo que la regla espera. Para que la prueba fuera útil, harían falta casos que la regla pueda fallar:
- una cabecera multinivel a más de 4 líneas, por notas o títulos largos;
- dos tablas apiladas con la cabecera de una pegada a las filas de la otra;
- una tabla multinivel duplicada en un anexo;
- potencias en los sintéticos;
- una trampa CJK con caracteres compartidos.

Lo más útil sería medir con más PDF públicos que tengan cabeceras multinivel, en otros idiomas y dominios. Requiere una destilación Sonnet por documento en un centro aislado. No la he lanzado: queda propuesta.

## 3. Qué cambia

`domain/services/anclaje_tabla.rs`, puro y sin listas de idioma:
- **Cabecera compuesta presente.** Todos sus tokens no triviales están, en cualquier orden, en las 4 líneas de la ventana anteriores a su primera fila anclada.
  - Los no triviales son los de 2 o más caracteres y los de 1 que no son una letra minúscula. Los conectores de 1 letra minúscula se ignoran.
  - Solo cuentan si en esas líneas están las de al menos 2 cabeceras distintas, y si la unión de sus tokens no cabe en una sola cabecera.
  - Suma a `C` junto con la regla contigua de siempre.
- **Potencias, solo de la celda al texto.** Si el valor contiene `\d+\^\d+`, también se busca `variante_sin_circunflejo` (`10^19` → `1019`). El texto no se transforma.
- `tokens` no cambia. `trocear` es su parte sin plegar mayúsculas, que se usa para saber qué letras son minúsculas.

Cambios de prueba y documentación:
- `benchmarks/anclaje/generar_sinteticos.py`: tipos `multinivel_2`, `multinivel_3` y `trampa_prosa`, en un flujo aparte.
- `docs/arquitectura/ssd_generalidad_4_piezas.md` §2.3.2.

## 4. Verificación

- `cargo test` completo:
  - línea base `ad1fbb3`: 1023 pasan, 0 fallan, 35 ignorados;
  - final: **1036 pasan, 0 fallan, 35 ignorados**.
- Tests nuevos:
  - 11 unitarios en `anclaje_tabla`: 2 y 3 niveles, tabla partida, conectores, mayúscula exigida, ventana de 4 líneas, 2 trampas de prosa, potencia, año del texto y la variante;
  - 2 adversariales: cabeceras solo de conectores o símbolos, y 2 000 cabeceras compuestas en 1 000 páginas en menos de 3 s.
- **Rojo verificado:**
  - 6 tests de comportamiento nuevo fallaron con el stub;
  - el del refinamiento falló con la regla sin endurecer;
  - los 4 de guarda pasaban ya antes, a propósito: protegen lo que no debe activarse.
- `cargo clippy --all-targets -- -D warnings` y `cargo fmt --check`: OK.
- Tiempos: no se midieron como resultado. El equipo estaba cargado por compilaciones.

## 5. Para integrar

- Sin push ni merge. Pendiente de la aprobación del usuario.
- Las páginas se guardan en la ingesta. Después de integrar, hay que **reingerir los centros una sola vez**. Lo coordina la sesión principal.
- No cambia el esquema: sigue en v7.
