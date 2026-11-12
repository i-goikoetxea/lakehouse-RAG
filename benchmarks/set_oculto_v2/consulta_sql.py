"""Consulta SQL de solo lectura sobre un centro aislado, vía `--mcp-server` (no gasta Claude).

Uso: python -I consulta_sql.py --binario <cli.exe> --aislado <dir> [--centro set-oculto] "<SQL>" ["<SQL>" ...]
      python -I consulta_sql.py --binario <cli.exe> --aislado <dir> --herramientas

Sirve para auditar lo que la ingesta dejó en el centro (estados, páginas, legibilidad) sin
preguntar al modelo. Imprime el JSON devuelto por `execute_athena_sql`.
"""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from lakehouse.cli import comando_de_binario  # noqa: E402
from lakehouse.mcp import ClienteMcp  # noqa: E402


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--binario", required=True)
    ap.add_argument("--aislado", required=True)
    ap.add_argument("--centro", default="set-oculto")
    ap.add_argument("--herramientas", action="store_true")
    ap.add_argument("--esquema", help="nombre de herramienta cuyo esquema de argumentos imprimir")
    ap.add_argument("sql", nargs="*")
    a = ap.parse_args()
    cliente = ClienteMcp(comando_de_binario(a.binario), a.aislado, timeout_llamada_s=300)
    cliente.iniciar()
    try:
        if a.herramientas:
            print(cliente.listar_herramientas())
        if a.esquema:
            res = cliente._llamar("tools/list", {}, 120)
            print(json.dumps([t for t in res.get("tools", []) if t.get("name") == a.esquema], ensure_ascii=False, indent=1))
        for sql in a.sql:
            r = cliente.llamar_herramienta("execute_athena_sql", {"centro_id": a.centro, "sql_query": sql})
            print(json.dumps(r.contenido, ensure_ascii=False, indent=1)[:20000])
    finally:
        cliente.cerrar()


if __name__ == "__main__":
    main()
