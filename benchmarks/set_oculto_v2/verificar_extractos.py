"""Comprueba que cada extracto literal del set está en la página indicada del PDF.

Uso: python -I verificar_extractos.py <preguntas.json> <dir_corpus> [--originales <dir>]

Cada pregunta puede llevar `extractos`: [{"documento", "pagina" (1-based, física), "texto"}].
Las trampas llevan además `extracto_original`, que debe estar en el PDF original (`--originales`,
mismos nombres de archivo) y NO en el modificado.

La comparación ignora los espacios (pypdf parte palabras con espacios espurios) y los caracteres
de uso privado (símbolos como ≥ que algunas fuentes mapean ahí). Nada más se normaliza: si el
extracto no aparece así, la pregunta se corrige o se descarta.
Salida: una línea por extracto y código 1 si alguno falla.
"""

import argparse
import json
import re
import sys
from pathlib import Path

from pypdf import PdfReader


def normalizar(texto):
    sin_privados = "".join(c for c in texto if not 0xE000 <= ord(c) <= 0xF8FF)
    return re.sub(r"\s+", "", sin_privados)


class Paginas:
    def __init__(self, directorio):
        self.directorio = Path(directorio)
        self._cache = {}

    def texto(self, documento, pagina):
        if documento not in self._cache:
            lector = PdfReader(str(self.directorio / documento))
            self._cache[documento] = [normalizar(p.extract_text() or "") for p in lector.pages]
        paginas = self._cache[documento]
        return paginas[pagina - 1] if 0 < pagina <= len(paginas) else ""

    def paginas_con(self, documento, extracto):
        self.texto(documento, 1)
        return [i + 1 for i, t in enumerate(self._cache[documento]) if normalizar(extracto) in t]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("preguntas")
    ap.add_argument("corpus")
    ap.add_argument("--originales")
    args = ap.parse_args()
    preguntas = json.loads(Path(args.preguntas).read_text(encoding="utf-8"))
    corpus = Paginas(args.corpus)
    originales = Paginas(args.originales) if args.originales else None
    fallos = 0
    for p in preguntas:
        for e in p.get("extractos", []):
            ok = normalizar(e["texto"]) in corpus.texto(e["documento"], e["pagina"])
            otras = [] if ok else corpus.paginas_con(e["documento"], e["texto"])
            fallos += not ok
            print(p["id"], "OK " if ok else "FALLA", e["documento"], e["pagina"], f"(está en {otras})" if otras else "")
        if p.get("extracto_original") and originales:
            e = p["extractos"][0]
            en_original = normalizar(p["extracto_original"]) in originales.texto(e["documento"], e["pagina"])
            sigue = normalizar(p["extracto_original"]) in corpus.texto(e["documento"], e["pagina"])
            ok = en_original and not sigue
            fallos += not ok
            print(p["id"], "OK " if ok else "FALLA", "trampa: original presente en el PDF real =", en_original, "; retirado del modificado =", not sigue)
    print("fallos:", fallos)
    return 1 if fallos else 0


if __name__ == "__main__":
    sys.exit(main())
