"""Imprime el esquema (rutas y ejemplo de valor) de un sobre JSON de `--ask --json`, sin texto largo."""
import json
import sys

sys.stdout.reconfigure(encoding="utf-8")


def esquema(valor, ruta=""):
    if isinstance(valor, dict):
        for k, v in valor.items():
            yield from esquema(v, f"{ruta}.{k}")
    elif isinstance(valor, list):
        yield f"{ruta} [lista de {len(valor)}]"
        if valor:
            yield from esquema(valor[0], f"{ruta}[0]")
    else:
        yield f"{ruta} = {str(valor)[:80]!r}"


datos = json.load(open(sys.argv[1], encoding="utf-8"))
omitir = set(sys.argv[2].split(",")) if len(sys.argv) > 2 else {"respuesta"}
for linea in esquema({k: v for k, v in datos.items() if k not in omitir}):
    if ".resultado." not in linea:
        print(linea)
