"""Mide una rama con el set oculto v2 en un solo comando: compila, ingiere, pregunta, puntúa y compara.

Uso (desde el worktree que guarda el set, que está en docs/validacion/set_oculto_v2/, ignorado):
  python -I benchmarks/set_oculto_v2/medir.py --etiqueta arreglos --compilar <worktree_de_la_rama>
  python -I benchmarks/set_oculto_v2/medir.py --etiqueta main --binario <lakehouse_rag_cli.exe>

Opciones: --pasadas 2 --modelo sonnet --base main (resumen con el que comparar; «ninguna» para no
comparar) --datos <carpeta del set>. Reanuda: no reingiere si la ingesta ya terminó y salta las
preguntas que ya tengan respuesta. Las respuestas crudas se guardan para repuntuar sin gastar Claude:
  python -I benchmarks/set_oculto_v2/medir.py --etiqueta arreglos --solo-puntuar
Código de salida 1 si hay retrocesos frente a la base (CLAUDE.md §6).
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

AQUI = Path(__file__).resolve().parent
REPO = AQUI.parents[1]
PY = [sys.executable, "-I"]


def correr(args, **kw):
    print("»", " ".join(str(a) for a in args), flush=True)
    return subprocess.run([str(a) for a in args], **kw)


def compilar(ruta):
    r = correr(["cargo", "build", "--release", "--bin", "lakehouse_rag_cli"], cwd=ruta)
    if r.returncode:
        sys.exit("La compilación falló")
    destino = Path(os.environ.get("CARGO_TARGET_DIR") or Path(ruta) / "target")
    return destino / "release" / "lakehouse_rag_cli.exe"


def sha256(ruta):
    return hashlib.sha256(Path(ruta).read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--etiqueta", required=True)
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--binario")
    g.add_argument("--compilar", help="worktree o repo de la rama a medir")
    ap.add_argument("--solo-puntuar", action="store_true")
    ap.add_argument("--pasadas", type=int, default=2)
    ap.add_argument("--modelo", default="sonnet")
    ap.add_argument("--base", default="main")
    ap.add_argument("--datos", default=str(REPO / "docs" / "validacion" / "set_oculto_v2"))
    a = ap.parse_args()
    datos = Path(a.datos)
    preguntas = datos / "preguntas.json"
    et = a.etiqueta

    if not a.solo_puntuar:
        if not (a.binario or a.compilar):
            sys.exit("Indica --binario o --compilar")
        origen = compilar(a.compilar) if a.compilar else Path(a.binario)
        bin_dir = datos / f"bin_{et}"
        bin_dir.mkdir(exist_ok=True)
        binario = bin_dir / "lakehouse_rag_cli.exe"
        if origen.resolve() != binario.resolve() and (not binario.exists() or sha256(binario) != sha256(origen)):
            shutil.copy2(origen, binario)
        for dll in origen.parent.glob("*.dll"):  # bibliotecas nativas que el binario carga (p. ej. pdfium)
            if dll.resolve() != (bin_dir / dll.name).resolve():
                shutil.copy2(dll, bin_dir / dll.name)
        (bin_dir / "sha256.txt").write_text(sha256(binario), encoding="utf-8")
        aislado = datos / f"aislado_{et}"
        aislado.mkdir(exist_ok=True)
        ingesta = datos / f"ingesta_{et}.jsonl"
        n_pdf = len(list((datos / "corpus").glob("*.pdf")))
        hechas = len(ingesta.read_text(encoding="utf-8").splitlines()) if ingesta.exists() else 0
        if hechas < n_pdf:
            if hechas:
                sys.exit(f"Ingesta a medias en {aislado}: bórrala junto con {ingesta} y repite")
            with ingesta.open("w", encoding="utf-8") as f:
                correr([*PY, AQUI / "ejecutar.py", "ingestar", "--binario", binario, "--aislado", aislado, "--corpus", datos / "corpus"], stdout=f, check=True)
        for k in range(1, a.pasadas + 1):
            correr([*PY, AQUI / "ejecutar.py", "preguntar", "--binario", binario, "--aislado", aislado, "--set", preguntas,
                    "--salida", datos / f"crudas_{et}_p{k}.jsonl", "--modelo", a.modelo, "--limite-min", "600"], check=True)

    puntuaciones = []
    revision = datos / f"revision_{et}.json"
    for k in range(1, a.pasadas + 1):
        salida = datos / f"puntuacion_{et}_p{k}.json"
        extra = ["--revision", datos / f"revision_{et}_p{k}.json"] if (datos / f"revision_{et}_p{k}.json").exists() else (["--revision", revision] if revision.exists() else [])
        correr([*PY, AQUI / "puntuar.py", preguntas, datos / f"crudas_{et}_p{k}.jsonl", salida, *extra], check=True, stdout=subprocess.DEVNULL)
        puntuaciones.append(salida)
    base = [] if a.base in ("ninguna", et) else ["--base", datos / f"resumen_{a.base}.json"]
    r = correr([*PY, AQUI / "resumen.py", preguntas, *puntuaciones, "--salida", datos / f"resumen_{et}.json", *base])
    return r.returncode


if __name__ == "__main__":
    sys.exit(main())
