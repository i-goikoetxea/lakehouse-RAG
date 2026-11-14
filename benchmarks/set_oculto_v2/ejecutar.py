"""Ejecuta un set oculto contra la CLI real, en un centro aislado.

Uso:
  python -I ejecutar.py ingestar --binario <cli.exe> --aislado <dir> --corpus <dir_pdf> [--centro set-oculto]
  python -I ejecutar.py preguntar --binario <cli.exe> --aislado <dir> --set <preguntas.json>
                        --salida <crudas.jsonl> [--modelo sonnet] [--centro set-oculto] [--limite-min 120]

`preguntas.json` es una lista de objetos con al menos `id` y `pregunta`. Cada respuesta se
guarda al llegar, con `herramientas` (argumentos y salidas), `arnes`, latencia y la carga de
CPU del equipo antes de la consulta. Reanuda: salta los `id` que ya estén en la salida.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from lakehouse.cli import ClienteCli, FalloEjecucion, comando_de_binario  # noqa: E402


def carga_cpu():
    """Porcentaje de CPU del equipo (Windows); None si no se puede leer."""
    try:
        r = subprocess.run(
            ["powershell", "-NoProfile", "-Command", "(Get-CimInstance Win32_Processor | Measure-Object LoadPercentage -Average).Average"],
            capture_output=True, text=True, timeout=30,
        )
        return float(r.stdout.strip().replace(",", "."))
    except (OSError, ValueError, subprocess.SubprocessError):
        return None


def cliente(args):
    c = ClienteCli(comando_de_binario(args.binario), args.aislado, timeout_consulta_s=900, timeout_ingesta_s=3600)
    original = c._invocar

    def invocar_guardando_sobre(argumentos, timeout_s):
        # Conserva el sobre JSON completo de la CLI (modelo resuelto, uso, coste, pasos...) para auditar
        salida, latencia_ms = original(argumentos, timeout_s)
        c.ultimo_sobre = salida
        return salida, latencia_ms

    c._invocar = invocar_guardando_sobre
    c.ultimo_sobre = None
    return c


def ingestar(args):
    c = cliente(args)
    ruta_centro = Path(args.aislado) / "centros" / args.centro
    c.crear_centro(args.centro, ruta_centro)
    c.activar(args.centro)
    for pdf in sorted(Path(args.corpus).glob("*.pdf")):
        carga = carga_cpu()
        t0 = time.time()
        try:
            salida = c.ingestar(pdf, args.centro)
            estado = {k: v for k, v in salida.items() if k not in ("texto",)}
        except FalloEjecucion as e:
            estado = {"fallo": e.tipo, "detalle": str(e)[:500]}
        print(json.dumps({"archivo": pdf.name, "segundos": round(time.time() - t0, 1), "cpu_previa": carga, **estado}, ensure_ascii=False), flush=True)


def activar_registro_claude(args):
    """Antepone al PATH el envoltorio `claude.exe` (claude_shim/) que registra cada `claude -p`."""
    if not args.envoltorio_claude:
        return
    real = shutil.which("claude")
    if not real or Path(real).resolve().parent == Path(args.envoltorio_claude).resolve():
        raise SystemExit("No encuentro el claude.exe real en el PATH")
    os.environ["CLAUDE_REAL"] = real
    os.environ["PATH"] = str(Path(args.envoltorio_claude).resolve()) + os.pathsep + os.environ["PATH"]


def preguntar(args):
    activar_registro_claude(args)
    c = cliente(args)
    preguntas = json.loads(Path(args.set).read_text(encoding="utf-8"))
    salida = Path(args.salida)
    hechas = set()
    if salida.exists():
        hechas = {json.loads(l)["id"] for l in salida.read_text(encoding="utf-8").splitlines() if l.strip()}
    inicio = time.time()
    with salida.open("a", encoding="utf-8") as f:
        for p in preguntas:
            if p["id"] in hechas:
                continue
            if (time.time() - inicio) / 60 > args.limite_min:
                print(f"Límite de {args.limite_min} min alcanzado; parada.", flush=True)
                break
            if args.envoltorio_claude:
                registro = Path(args.salida).with_suffix("") / p["id"]
                registro.mkdir(parents=True, exist_ok=True)
                os.environ["LAKEHOUSE_SHIM_LOG"] = str(registro)
            fila = {"id": p["id"], "pregunta": p["pregunta"], "modelo_pedido": args.modelo, "cpu_previa": carga_cpu(), "inicio": time.strftime("%Y-%m-%dT%H:%M:%S")}
            try:
                r = c.preguntar(p["pregunta"], args.centro, args.modelo)
                fila |= {"respuesta": r.texto, "modelo": r.modelo, "herramientas": r.herramientas, "arnes": r.arnes, "latencia_ms": r.latencia_ms}
                sobre = c.ultimo_sobre or {}
                fila["sobre"] = {k: v for k, v in sobre.items() if k not in ("respuesta", "herramientas", "arnes")}
            except FalloEjecucion as e:
                fila |= {"fallo": e.tipo, "detalle": str(e)[:2000]}
            fila["cpu_posterior"] = carga_cpu()
            f.write(json.dumps(fila, ensure_ascii=False) + "\n")
            f.flush()
            print(p["id"], fila.get("fallo", "ok"), round(fila.get("latencia_ms", 0) / 1000, 1), "s", flush=True)


def main():
    sys.stdout.reconfigure(encoding="utf-8")  # la salida se redirige a .jsonl: UTF-8 siempre
    ap = argparse.ArgumentParser()
    ap.add_argument("accion", choices=["ingestar", "preguntar"])
    ap.add_argument("--binario", required=True)
    ap.add_argument("--aislado", required=True)
    ap.add_argument("--centro", default="set-oculto")
    ap.add_argument("--corpus")
    ap.add_argument("--set")
    ap.add_argument("--salida")
    ap.add_argument("--modelo", default="sonnet")
    ap.add_argument("--limite-min", type=float, default=120)
    ap.add_argument("--envoltorio-claude", help="carpeta con el claude.exe de claude_shim/: registra cada claude -p en <salida>/<id>/")
    args = ap.parse_args()
    ingestar(args) if args.accion == "ingestar" else preguntar(args)


if __name__ == "__main__":
    main()
