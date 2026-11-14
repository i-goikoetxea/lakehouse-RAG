"""Métricas por pregunta a partir del registro del envoltorio `claude_shim` y de las respuestas crudas.

Uso: python -I metricas_claude.py <crudas.jsonl> [--registro <dir>] --salida <metricas.json>

`<dir>` (por defecto, <crudas> sin extensión) tiene una carpeta por id de pregunta con un fichero
por proceso `claude -p`: una cabecera JSON (args, duración, código) y su salida stream-json.
Por pregunta: latencia total, pasos (procesos), procesos por alias de modelo, identificadores de
modelo resueltos, tokens y coste declarados por `claude -p`, microllamadas a Haiku y su latencia,
herramientas, reintentos e intervenciones del arnés, y CPU antes y después.
No guarda texto de preguntas ni de respuestas: solo cifras e identificadores.
"""

import argparse
import json
import statistics
import sys
from collections import Counter
from pathlib import Path

CAMPOS_USO = ("input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens")


def leer_proceso(fichero):
    lineas = fichero.read_text(encoding="utf-8", errors="replace").splitlines()
    cabecera = json.loads(lineas[0]) if lineas else {}
    args = cabecera.get("args", [])
    alias = args[args.index("--model") + 1] if "--model" in args else None
    modelos, resultado = set(), None
    for linea in lineas[1:]:
        try:
            evento = json.loads(linea)
        except ValueError:
            continue
        if not isinstance(evento, dict):
            continue
        if evento.get("type") == "system" and evento.get("model"):
            modelos.add(evento["model"])
        mensaje = evento.get("message") if isinstance(evento.get("message"), dict) else {}
        if mensaje.get("model"):
            modelos.add(mensaje["model"])
        if evento.get("type") == "result":
            resultado = evento
    uso = (resultado or {}).get("usage") or {}
    modelos_uso = (resultado or {}).get("modelUsage") or {}
    modelos.update(modelos_uso)
    return {
        "alias": alias,
        "modelos": sorted(modelos),
        "duracion_ms": cabecera.get("duracion_ms"),
        "codigo": cabecera.get("codigo"),
        "coste_usd": (resultado or {}).get("total_cost_usd"),
        "turnos": (resultado or {}).get("num_turns"),
        **{c: uso.get(c) for c in CAMPOS_USO},
    }


def sumar(valores):
    v = [x for x in valores if isinstance(x, (int, float))]
    return round(sum(v), 6) if v else None


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("crudas")
    ap.add_argument("--registro")
    ap.add_argument("--salida", required=True)
    a = ap.parse_args()
    registro = Path(a.registro) if a.registro else Path(a.crudas).with_suffix("")
    filas = [json.loads(l) for l in Path(a.crudas).read_text(encoding="utf-8").splitlines() if l.strip()]
    res = []
    for f in filas:
        procesos = [leer_proceso(p) for p in sorted((registro / f["id"]).glob("*.json"))] if (registro / f["id"]).is_dir() else []
        haiku = [p for p in procesos if (p["alias"] or "").lower().startswith("haiku") or any("haiku" in m for m in p["modelos"])]
        res.append({
            "id": f["id"],
            "fallo": f.get("fallo"),
            "latencia_ms": f.get("latencia_ms"),
            "cpu_previa": f.get("cpu_previa"),
            "cpu_posterior": f.get("cpu_posterior"),
            "herramientas": [h.get("nombre") for h in f.get("herramientas", [])],
            "n_herramientas": len(f.get("herramientas", [])),
            "arnes": [{"etapa": i.get("etapa")} for i in f.get("arnes", [])],
            "reintentos": sum(1 for i in f.get("arnes", []) if i.get("etapa") == "reintento"),
            "bloqueos": sum(1 for i in f.get("arnes", []) if i.get("etapa") == "salida_segura"),
            "procesos_claude": len(procesos),
            "procesos_por_alias": dict(Counter(p["alias"] for p in procesos)),
            "modelos_resueltos": sorted({m for p in procesos for m in p["modelos"]}),
            "coste_usd": sumar(p["coste_usd"] for p in procesos),
            **{c: sumar(p[c] for p in procesos) for c in CAMPOS_USO},
            "haiku_llamadas": len(haiku),
            "haiku_latencia_ms": [p["duracion_ms"] for p in haiku],
            "procesos": procesos,
        })
    Path(a.salida).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
    con = [r for r in res if r["procesos_claude"]]
    print(f"{len(res)} preguntas; {len(con)} con registro de claude -p")
    if con:
        print("procesos por pregunta (mediana):", statistics.median(r["procesos_claude"] for r in con))
        print("modelos resueltos:", sorted({m for r in con for m in r["modelos_resueltos"]}))
        print("alias:", dict(sum((Counter(r["procesos_por_alias"]) for r in con), Counter())))
        print("coste total USD:", sumar(r["coste_usd"] for r in con))
        print("tokens:", {c: sumar(r[c] for r in con) for c in CAMPOS_USO})
        h = [x for r in con for x in r["haiku_latencia_ms"] if x]
        print("microllamadas Haiku:", sum(r["haiku_llamadas"] for r in con), "latencia mediana ms:", statistics.median(h) if h else None)


if __name__ == "__main__":
    main()
