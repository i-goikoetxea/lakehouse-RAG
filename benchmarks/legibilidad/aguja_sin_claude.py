"""Aguja con recuperación pura, sin Claude (ssd_generalidad_4_piezas.md §1.7 y la Pieza 1).

Reutiliza `run_aguja.py` (mismas agujas, profundidades, centros aislados e ingesta por CLI), pero:
- lanza la app sin `claude` en el PATH, así que la destilación falla y el documento queda
  provisional con su texto indexado (02 y 07 salen del parser, no de Claude);
- no pregunta al agente: cada consulta llama solo a `search_normative_context` por MCP.

Métricas: `recall_mcp` (algún fragmento devuelto contiene la aguja), `pagina_mcp` y `seccion_mcp`.

Uso: python -I aguja_sin_claude.py --binario <lakehouse_rag_cli.exe> --repeticiones 3 [--salida <dir>]
"""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(RAIZ))

import run_aguja  # noqa: E402


def _consulta_sin_agente(cliente, mcp, centro_id, modelo, tipo, pregunta, aguja, esperado, otra=None, pagina_otra=None):
    directo = run_aguja._recuperacion_directa(mcp, centro_id, pregunta, aguja, esperado)
    recall = None if "fallo" in directo else float(bool(directo.get("fragmentos")))
    return {"tipo": tipo, "pregunta": pregunta, "fallo": None, "arnes": [], "recall_aguja": recall, "mcp_directo": directo}


def _path_sin_claude() -> str:
    """PATH sin las carpetas que contienen el ejecutable de Claude Code."""
    carpetas = os.environ.get("PATH", "").split(os.pathsep)
    return os.pathsep.join(
        c for c in carpetas if not any((Path(c) / n).exists() for n in ("claude", "claude.exe", "claude.cmd"))
    )


def main() -> int:
    os.environ["PATH"] = _path_sin_claude()
    if shutil.which("claude"):
        print("claude sigue en el PATH: no se mide", file=sys.stderr)
        return 2
    run_aguja._consulta = _consulta_sin_agente
    return run_aguja.main()


if __name__ == "__main__":
    sys.exit(main())
