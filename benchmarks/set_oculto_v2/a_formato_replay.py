"""Convierte respuestas crudas del set oculto al formato «benchmark» de la repetición del arnés.

Uso: python -I a_formato_replay.py <preguntas.json> <salida_dir> <crudas1.jsonl> [<crudas2.jsonl> ...]
        [--ids V56,V58] [--tipos trampa,sin_respuesta]

Escribe en <salida_dir> un `replay_<nombre>.jsonl` por fichero de entrada (una línea por respuesta,
con `pregunta`, `respuesta` y `herramientas` [{nombre, argumentos, resultado, es_error}]) y un
`replay_indice.json` que dice a qué id y fichero corresponde cada línea. Omite (y lo cuenta) las
filas con fallo de ejecución y las que el arnés ya bloqueó: su texto original no se guardó.
"""

import argparse
import json
from pathlib import Path

TEXTO_BLOQUEO = "No he podido validar la respuesta"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("preguntas")
    ap.add_argument("salida_dir")
    ap.add_argument("crudas", nargs="+")
    ap.add_argument("--ids", default="")
    ap.add_argument("--tipos", default="")
    a = ap.parse_args()
    preguntas = {p["id"]: p for p in json.loads(Path(a.preguntas).read_text(encoding="utf-8"))}
    ids = set(filter(None, a.ids.split(",")))
    tipos = set(filter(None, a.tipos.split(",")))
    elegidas = {i for i, p in preguntas.items() if i in ids or p["tipo"] in tipos}
    salida = Path(a.salida_dir)
    salida.mkdir(parents=True, exist_ok=True)
    indice, omitidas = {}, {"fallo": 0, "bloqueada": 0}
    for ruta in map(Path, a.crudas):
        destino = salida / f"replay_{ruta.stem}.jsonl"
        lineas = []
        for l in ruta.read_text(encoding="utf-8").splitlines():
            d = json.loads(l)
            if d["id"] not in elegidas:
                continue
            if "fallo" in d:
                omitidas["fallo"] += 1
                continue
            if any(i.get("etapa") == "salida_segura" for i in d.get("arnes", [])) or d.get("respuesta", "").startswith(TEXTO_BLOQUEO):
                omitidas["bloqueada"] += 1
                continue
            herramientas = [{k: h.get(k) for k in ("nombre", "argumentos", "resultado", "es_error")} for h in d.get("herramientas", [])]
            lineas.append(json.dumps({"pregunta": d["pregunta"], "respuesta": d["respuesta"], "herramientas": herramientas}, ensure_ascii=False))
            indice.setdefault(destino.name, []).append({"linea": len(lineas), "id": d["id"], "tipo": preguntas[d["id"]]["tipo"]})
        destino.write_text("\n".join(lineas) + ("\n" if lineas else ""), encoding="utf-8")
    (salida / "replay_indice.json").write_text(json.dumps({"omitidas": omitidas, "ficheros": indice}, ensure_ascii=False, indent=1), encoding="utf-8")
    print("omitidas:", omitidas, "| líneas:", {k: len(v) for k, v in indice.items()})


if __name__ == "__main__":
    main()
