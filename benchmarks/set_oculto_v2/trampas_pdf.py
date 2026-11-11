"""Inserta «trampas de conocimiento previo» en copias de PDF, editando los literales de texto.

Uso: python -I trampas_pdf.py <trampas.json> <dir_originales> <dir_salida>

`trampas.json` es una lista de objetos:
  {"id", "documento" (nombre del PDF), "pagina" (1-based), "contexto", "original", "modificado"}
`contexto` es un fragmento de la página que contiene `original`; se busca sin espacios para
tolerar el interletrado. Solo se sustituye la aparición de `original` dentro de `contexto`.
Los caracteres no ASCII valen como comodín, porque la codificación de 8 bits depende de la
fuente (WinAnsi, MacRoman...). `modificado` debe ser ASCII.

Se editan los operadores Tj/TJ/'/" de los flujos de contenido de la página y de sus Form
XObjects. Fuentes simples de 1 byte y fuentes Type0 de 2 bytes (Identity-H) con mapa
ToUnicode; en estas, cada carácter de `modificado` debe existir ya en el subconjunto de la fuente. Así el texto nuevo queda en el mismo punto del orden de
lectura. No usa redacciones ni capas superpuestas, que dejarían el texto original extraíble.
El script no contiene texto de ningún documento: todo llega por el JSON (que no se versiona).
"""

import json
import re
import sys
from pathlib import Path

import pymupdf

DELIM = b"()<>[]{}/%"
WS = b" \t\r\n\f\x00"
COMODIN = "?"
ESCAPES = {0x6E: b"\n", 0x72: b"\r", 0x74: b"\t", 0x62: b"\b", 0x66: b"\f"}


def _literal(s, i):
    """Lee un literal `( ... )` desde `i`; devuelve (fin, bytes)."""
    n, j, prof, out = len(s), i + 1, 1, bytearray()
    while j < n:
        b = s[j]
        if b == 0x5C:
            j += 1
            e = s[j]
            if e in b"01234567":
                k = j
                while k < n and k < j + 3 and s[k] in b"01234567":
                    k += 1
                out.append(int(s[j:k], 8) & 0xFF)
                j = k
                continue
            if e not in b"\r\n":
                out += ESCAPES.get(e, bytes([e]))
            j += 1
            continue
        if b == 0x28:
            prof += 1
        elif b == 0x29:
            prof -= 1
            if prof == 0:
                break
        out.append(b)
        j += 1
    return j + 1, bytes(out)


def lexar(s):
    """Tokens (tipo, inicio, fin, valor). tipo: str | hex | otro."""
    i, n = 0, len(s)
    while i < n:
        c = s[i]
        if c in WS:
            i += 1
        elif c == 0x25:  # comentario
            while i < n and s[i] not in b"\r\n":
                i += 1
        elif c == 0x28:
            fin, valor = _literal(s, i)
            yield ("str", i, fin, valor)
            i = fin
        elif c == 0x3C and i + 1 < n and s[i + 1] != 0x3C:
            j = s.index(b">", i)
            hexa = re.sub(rb"\s", b"", s[i + 1 : j]).decode()
            yield ("hex", i, j + 1, bytes.fromhex(hexa + "0" * (len(hexa) % 2)))
            i = j + 1
        elif c in b"<>" and i + 1 < n and s[i + 1] == c:
            yield ("otro", i, i + 2, s[i : i + 2])
            i += 2
        elif c in b"[]{}":
            yield ("otro", i, i + 1, s[i : i + 1])
            i += 1
        else:
            j = i + 1
            while j < n and s[j] not in WS and s[j] not in DELIM:
                j += 1
            yield ("otro", i, j, s[i:j])
            i = j


def escapar(b):
    return b"(" + b.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)") + b")"


def literales_de_texto(s):
    """Literales que se muestran con Tj, TJ, ' o ", en orden, con la fuente activa (`/F0`)."""
    pendientes, res, previo, fuente = [], [], None, None
    for tipo, a, b, v in lexar(s):
        if tipo in ("str", "hex"):
            pendientes.append((a, b, v, fuente))
        elif v in (b"Tj", b"TJ", b"'", b'"'):
            res.extend(pendientes)
            pendientes = []
        elif v == b"Tf" and previo and previo.startswith(b"/"):
            fuente = previo[1:].decode("latin-1")
        elif v not in (b"[", b"]") and not re.fullmatch(rb"[-+.0-9]+", v):
            pendientes = []
        if tipo == "otro" and not re.fullmatch(rb"[-+.0-9]+", v):
            previo = v
    return res


def _cmap_tounicode(texto):
    """Código (int) -> carácter, de un CMap ToUnicode (bfchar y bfrange)."""
    mapa = {}
    def uni(h):
        return bytes.fromhex(h).decode("utf-16-be", errors="replace")
    for bloque in re.findall(r"beginbfchar(.*?)endbfchar", texto, re.S):
        for src, dst in re.findall(r"<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>", bloque):
            mapa[int(src, 16)] = uni(dst)
    for bloque in re.findall(r"beginbfrange(.*?)endbfrange", texto, re.S):
        for lo, hi, dst in re.findall(r"<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>\s*(<[0-9A-Fa-f]+>|\[[^\]]*\])", bloque):
            lo, hi = int(lo, 16), int(hi, 16)
            if dst.startswith("["):
                for k, h in enumerate(re.findall(r"<([0-9A-Fa-f]+)>", dst)):
                    mapa[lo + k] = uni(h)
            else:
                base = int(dst[1:-1], 16)
                for k in range(hi - lo + 1):
                    mapa[lo + k] = chr(base + k)
    return mapa


def codecs_de_pagina(doc, page):
    """Nombre de fuente -> mapa ToUnicode, solo para fuentes Type0 Identity-H (2 bytes)."""
    res = {}
    for xref, _, tipo, _, nombre, codificacion, *_ in page.get_fonts(full=True):
        if tipo != "Type0" or "Identity" not in codificacion:
            continue
        clase, valor = doc.xref_get_key(xref, "ToUnicode")
        if clase == "xref":
            cm = doc.xref_stream(int(valor.split()[0])).decode("latin-1")
            res[nombre] = _cmap_tounicode(cm)
    return res


def _normalizar(texto):
    sin_espacios = re.sub(r"\s", "", texto)
    return "".join(c if ord(c) < 0x80 else COMODIN for c in sin_espacios)


def _unidades(valor, mapa):
    """(offset, código, carácter) de un literal; 2 bytes por código si hay mapa Type0."""
    if mapa is None:
        return [(k, b, chr(b) if b < 0x80 else COMODIN) for k, b in enumerate(valor)]
    res = []
    for k in range(0, len(valor) - 1, 2):
        codigo = int.from_bytes(valor[k : k + 2], "big")
        ch = mapa.get(codigo, COMODIN)
        res.append((k, codigo, ch if len(ch) == 1 and ord(ch) < 0x80 else COMODIN))
    return res


def _codificar(texto, mapa):
    if mapa is None:
        return texto.encode("ascii")
    inverso = {}
    for codigo, ch in sorted(mapa.items()):
        inverso.setdefault(ch, codigo)
    faltan = [c for c in texto if c not in inverso]
    if faltan:
        raise KeyError("glifo ausente en la fuente: " + "".join(sorted(set(faltan))))
    return b"".join(inverso[c].to_bytes(2, "big") for c in texto)


def sustituir(s, contexto, original, nuevo, codecs=None):
    codecs = codecs or {}
    lits = literales_de_texto(s)
    chars = []  # (indice_literal, offset, caracter), sin espacios
    unidades = []
    for li, (_, _, v, fuente) in enumerate(lits):
        u = _unidades(v, codecs.get(fuente))
        unidades.append(u)
        for k, _, ch in u:
            if not ch.isspace():
                chars.append((li, k, ch))
    plano = "".join(c for _, _, c in chars)
    ctx, org = _normalizar(contexto), _normalizar(original)
    pos = plano.find(ctx)
    if pos < 0:
        return None, "contexto no encontrado"
    if plano.find(ctx, pos + 1) >= 0:
        return None, "contexto ambiguo"
    rel = ctx.find(org)
    if rel < 0:
        return None, "original fuera del contexto"
    tramo = chars[pos + rel : pos + rel + len(org)]
    primero, ultimo = tramo[0][:2], tramo[-1][:2]
    borrar = {t[:2] for t in tramo[1:]}
    afectados = sorted({t[0] for t in tramo})
    nuevos = {}
    for li in afectados:
        mapa = codecs.get(lits[li][3])
        try:
            reemplazo = _codificar(nuevo, mapa)
        except KeyError as e:
            return None, str(e)
        v, out = lits[li][2], bytearray()
        for k, codigo, ch in unidades[li]:
            ancho = 1 if mapa is None else 2
            if (li, k) == primero:
                out += reemplazo
            elif (li, k) in borrar:
                continue
            elif primero < (li, k) < ultimo and ch.isspace():
                continue  # espacios internos de `original`
            else:
                out += v[k : k + ancho]
        nuevos[li] = (bytes(out), mapa is not None)
    partes, cursor = [], 0
    for li in afectados:
        a, b, _, _ = lits[li]
        cuerpo, es_hex = nuevos[li]
        partes += [s[cursor:a], b"<" + cuerpo.hex().upper().encode() + b">" if es_hex else escapar(cuerpo)]
        cursor = b
    partes.append(s[cursor:])
    return b"".join(partes), "ok"


def flujos_de_pagina(doc, page):
    """Flujos de contenido de la página y de sus Form XObjects (recursivo)."""
    vistos, cola = [], list(page.get_contents()) + [x[0] for x in page.get_xobjects()]
    while cola:
        x = cola.pop(0)
        if x in vistos or not doc.xref_is_stream(x):
            continue
        vistos.append(x)
        tipo, valor = doc.xref_get_key(x, "Resources/XObject")
        if tipo == "dict":
            cola += [int(ref) for ref in re.findall(r"(\d+) 0 R", valor)]
    return vistos


def main():
    trampas = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    origen, destino = Path(sys.argv[2]), Path(sys.argv[3])
    destino.mkdir(parents=True, exist_ok=True)
    por_doc = {}
    for t in trampas:
        por_doc.setdefault(t["documento"], []).append(t)
    informe = []
    for pdf in sorted(origen.glob("*.pdf")):
        doc = pymupdf.open(pdf)
        for t in por_doc.get(pdf.name, []):
            page = doc[t["pagina"] - 1]
            estado = "contexto no encontrado"
            codecs = codecs_de_pagina(doc, page)
            for x in flujos_de_pagina(doc, page):
                nuevo_s, estado = sustituir(doc.xref_stream(x), t["contexto"], t["original"], t["modificado"], codecs)
                if nuevo_s is not None:
                    doc.update_stream(x, nuevo_s)
                    break
            informe.append({"id": t["id"], "estado": estado})
        doc.save(destino / pdf.name, garbage=0, deflate=True)
        doc.close()
    print(json.dumps(informe, ensure_ascii=False))
    return 0 if all(r["estado"] == "ok" for r in informe) else 1


if __name__ == "__main__":
    sys.exit(main())
