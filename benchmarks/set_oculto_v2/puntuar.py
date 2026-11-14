"""Puntúa de forma determinista las respuestas crudas de un set oculto.

Uso: python -I puntuar.py <preguntas.json> <crudas.jsonl> <salida.json> [--revision <revision.json>]

Reglas (todas sobre el texto del modelo, sin el bloque «Verificación automática de páginas»):
- `claves`: lista de grupos; cada grupo es una lista de regex alternativas. Un grupo se cumple si
  alguna alternativa aparece. Todos cumplidos = contenido completo; alguno = incompleta.
- `prohibidos`: regex de valores erróneos (de memoria, de otra revisión, de otra fila). Si aparecen
  junto a todas las claves, la respuesta queda «dudosa» para revisión manual (puede ser una
  mención como descartado).
- `memoria` (trampas): si aparece y faltan las claves, es «respuesta_de_memoria»; si aparece
  junto a las claves, «correcta_menciona_memoria» (revisar: suele ser una aclaración legítima).
- `abstencion_esperada`: abstenerse es correcto; no hacerlo es «no_se_abstiene».
- Página: se extraen las páginas citadas y se comparan con las de los extractos. Cada extracto
  cuenta como citado si se cita su página o una de `paginas_aceptadas` (otra ubicación válida
  del mismo dato, comprobada a mano). Todas citadas = correcta; alguna = parcial.
- `abstencion_esperada` + `memoria`: si aparece un valor de memoria sin abstenerse, es
  «respuesta_de_memoria»; si se abstiene pero lo añade, «abstencion_con_dato_externo».
- Etapa (solo fallos): si algún extracto aparece en la salida de las herramientas de esa
  consulta, el dato llegó al modelo (etapa «respuesta»); si no, «búsqueda o anterior», que se
  afina después con otras pruebas.
`--revision` es un JSON {id: {"veredicto": ..., "nota": ...}} que sustituye al automático.
"""

import argparse
import json
import re
import sys
import unicodedata
from pathlib import Path

ENCABEZADO = "Verificación automática de páginas"
ABSTENCION = re.compile(
    r"no (consta|aparece|figura|indica|encuentro|he encontrado|se (ha )?encontr|se especifica|se indica|se establece|"
    r"contiene|establece|fija|recoge|incluye|hay (informaci|datos|ning|valor)|dispongo|puedo (confirmar|responder))|"
    r"not (found|specified|stated|provided|contain|include|set out|give)|does not (specify|state|contain|set|give|provide|include)|"
    r"no (information|numerical|specific|such)|isn.t (specified|stated)|no se puede (responder|determinar)",
    re.I,
)
PAGINAS = re.compile(r"(?:⟦\s*p\.\s*|\bp[áa]g(?:ina)?s?\.?[:*\s]*|\bpages?[:*\s]*|\bpp?\.\s*)(\d{1,3})(?:\s*[-–]\s*(\d{1,3}))?", re.I)


def normalizar(texto):
    t = unicodedata.normalize("NFKC", texto).replace(" ", " ")
    return re.sub(r"[ \t]+", " ", t)


def sin_espacios(texto):
    return re.sub(r"\s+", "", "".join(c for c in texto if not 0xE000 <= ord(c) <= 0xF8FF))


def texto_modelo(respuesta):
    i = respuesta.find(ENCABEZADO)
    return respuesta[:i] if i >= 0 else respuesta


def _paginas_de_tablas(texto):
    """Páginas en la columna «Página»/«Page» de tablas markdown (| … | 232 | … |)."""
    res, columna = set(), None
    for linea in texto.splitlines():
        if not linea.strip().startswith("|"):
            columna = None
            continue
        celdas = [c.strip() for c in linea.strip().strip("|").split("|")]
        if columna is None:
            idx = [i for i, c in enumerate(celdas) if re.fullmatch(r"p[áa]g(ina)?s?\.?|pages?", c, re.I)]
            columna = idx[0] if idx else -1
        elif 0 <= columna < len(celdas):
            res.update(int(n) for n in re.findall(r"\b(\d{1,3})\b", celdas[columna]))
    return res


def paginas_citadas(texto):
    res = set(_paginas_de_tablas(texto))
    for m in PAGINAS.finditer(texto):
        a = int(m.group(1))
        b = int(m.group(2)) if m.group(2) else a
        res.update(range(a, b + 1) if 0 <= b - a <= 10 else [a])
    return res


def grupos_cumplidos(claves, texto):
    return [any(re.search(alt, texto, re.I) for alt in grupo) for grupo in claves]


def cadenas(valor):
    """Todas las cadenas de un resultado de herramienta (dict/list anidados)."""
    if isinstance(valor, str):
        yield valor
    elif isinstance(valor, dict):
        for v in valor.values():
            yield from cadenas(v)
    elif isinstance(valor, list):
        for v in valor:
            yield from cadenas(v)


MARCA_PAGINA = re.compile(r"⟦\s*p\.\s*\d+\s*⟧")


def extracto_en_herramientas(pregunta, fila):
    textos = (c for h in fila.get("herramientas", []) for c in cadenas(h.get("resultado")))
    salidas = sin_espacios(MARCA_PAGINA.sub("", " ".join(textos)))
    return [e["pagina"] for e in pregunta.get("extractos", []) if sin_espacios(e["texto"])[:120] in salidas]


def puntuar(pregunta, fila):
    if "fallo" in fila:
        return {"veredicto": "fallo_ejecucion", "detalle": fila["fallo"]}
    texto = normalizar(texto_modelo(fila["respuesta"]))
    cumplidos = grupos_cumplidos(pregunta.get("claves", []), texto)
    prohibido = [p for p in pregunta.get("prohibidos", []) if re.search(p, texto, re.I)]
    memoria = [p for p in pregunta.get("memoria", []) if re.search(p, texto, re.I)]
    abstiene = bool(ABSTENCION.search(texto))
    if pregunta.get("abstencion_esperada"):
        if memoria:
            veredicto = "abstencion_con_dato_externo" if abstiene else "respuesta_de_memoria"
        else:
            veredicto = "abstencion_correcta" if abstiene else "no_se_abstiene"
    elif cumplidos and all(cumplidos):
        veredicto = "dudosa" if prohibido else ("correcta_menciona_memoria" if memoria else "correcta")
    elif memoria:
        veredicto = "respuesta_de_memoria"
    elif abstiene and not any(cumplidos):
        veredicto = "abstencion_indebida"
    elif any(cumplidos):
        veredicto = "incompleta"
    else:
        veredicto = "incorrecta"
    esperadas = {e["pagina"] for e in pregunta.get("extractos", [])}
    aceptadas = set(pregunta.get("paginas_aceptadas", []))
    citadas = paginas_citadas(texto)
    cubiertos = [e["pagina"] in citadas or bool(aceptadas & citadas) for e in pregunta.get("extractos", [])]
    if not esperadas:
        pagina = "no_aplica"
    elif not citadas:
        pagina = "sin_pagina"
    elif all(cubiertos):
        pagina = "correcta"
    elif any(cubiertos):
        pagina = "parcial"
    else:
        pagina = "incorrecta"
    return {
        "veredicto": veredicto,
        "claves": cumplidos,
        "prohibidos": prohibido,
        "memoria": memoria,
        "abstiene": abstiene,
        "pagina": pagina,
        "paginas_citadas": sorted(citadas)[:20],
        "paginas_esperadas": sorted(esperadas),
        "extracto_llego_al_modelo": extracto_en_herramientas(pregunta, fila),
        "herramientas": [h.get("nombre") for h in fila.get("herramientas", [])],
        "arnes": fila.get("arnes", []),
        "tool_calls": len(fila.get("herramientas", [])),
        "latencia_s": round(fila.get("latencia_ms", 0) / 1000, 1),
        "cpu_previa": fila.get("cpu_previa"),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("preguntas")
    ap.add_argument("crudas")
    ap.add_argument("salida")
    ap.add_argument("--revision")
    args = ap.parse_args()
    preguntas = {p["id"]: p for p in json.loads(Path(args.preguntas).read_text(encoding="utf-8"))}
    filas = [json.loads(l) for l in Path(args.crudas).read_text(encoding="utf-8").splitlines() if l.strip()]
    revision = json.loads(Path(args.revision).read_text(encoding="utf-8")) if args.revision else {}
    res = []
    for fila in filas:
        p = preguntas[fila["id"]]
        r = {"id": fila["id"], "tipo": p["tipo"], **puntuar(p, fila)}
        if fila["id"] in revision:
            r["veredicto_automatico"] = r["veredicto"]
            r.update(revision[fila["id"]])
        res.append(r)
    Path(args.salida).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
    cuenta = {}
    for r in res:
        cuenta[r["veredicto"]] = cuenta.get(r["veredicto"], 0) + 1
    print(json.dumps(cuenta, ensure_ascii=False))
    for r in res:
        print(r["id"], r["tipo"], r["veredicto"], "pág:", r.get("pagina"), r.get("paginas_citadas"), "llegó:", r.get("extracto_llego_al_modelo"), "arnés:", len(r.get("arnes", [])))
    return 0


if __name__ == "__main__":
    sys.exit(main())
