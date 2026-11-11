"""Puntúa la salida de `medir_anclaje` (modo sintético o PDF con oráculo) en tres clases.

- correcta: la página calculada está entre las válidas;
- none: no hay página (aceptable; se informa como cobertura perdida);
- falsa: hay página y no es válida. **Criterio de la Pieza 2: 0.**

Uso: python benchmarks/anclaje/puntuar.py <salida.jsonl> [casos.json]
Con `casos.json` (modo sintético) desglosa por tipo e idioma.
"""

import json
import sys
from collections import Counter, defaultdict


def clase(registro):
    calculada = registro.get("calculada")
    if calculada is None:
        return "none"
    return "correcta" if calculada in registro["validas"] else "falsa"


def main():
    registros = [json.loads(l) for l in open(sys.argv[1], encoding="utf-8") if l.strip()]
    tipos = {}
    if len(sys.argv) > 2:
        for c in json.load(open(sys.argv[2], encoding="utf-8")):
            tipos[c["id"]] = (c["tipo"], c["idioma"])
    total = Counter(clase(r) for r in registros)
    print(f"Filas: {len(registros)}  correctas: {total['correcta']}  none: {total['none']}  falsas: {total['falsa']}")
    for indice, etiqueta in ((0, "tipo"), (1, "idioma")):
        if not tipos:
            break
        grupos = defaultdict(Counter)
        for r in registros:
            grupos[tipos[r["caso"]][indice]][clase(r)] += 1
        print(f"\nPor {etiqueta}:")
        print(f"| {etiqueta} | filas | correctas | none | falsas |")
        print("|---|---:|---:|---:|---:|")
        for clave in sorted(grupos):
            g = grupos[clave]
            print(f"| {clave} | {sum(g.values())} | {g['correcta']} | {g['none']} | {g['falsa']} |")
    falsas = [r for r in registros if clase(r) == "falsa"]
    if falsas:
        print("\nPáginas falsas:")
        for r in falsas[:30]:
            print(json.dumps(r, ensure_ascii=False))


if __name__ == "__main__":
    main()
