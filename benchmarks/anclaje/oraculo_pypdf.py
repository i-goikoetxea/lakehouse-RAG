"""Oráculo pypdf para la medición N2 en PDF reales (ssd_generalidad_4_piezas.md §6.3).

Para cada fila de cada tabla destilada busca en TODO el documento (texto de pypdf, sin ventana)
las líneas que contienen la clave de la fila y al menos 2 valores distintos de la fila en la misma
línea o a ±2 líneas de la misma página (1 valor si la fila solo tiene uno). Si todas esas
líneas caen en una única página, esa es la página del oráculo; si no aparece o aparece en varias
páginas, la fila se excluye por ambigua y se informa.

El oráculo usa otro extractor (pypdf) y no limita la búsqueda a un bloque, pero comparte la idea
de «clave + valores cerca»: no es independiente del algoritmo y no mide filas que solo un humano
sabría situar.

Uso: python -I benchmarks/anclaje/oraculo_pypdf.py <salida_pdf.jsonl> <carpeta_pdf> <puntuable.jsonl>
"""

import json
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

from pypdf import PdfReader

RADIO = 2


def sin_espacios(c):
    o = ord(c)
    return 0x3040 <= o <= 0x30FF or 0x3400 <= o <= 0x4DBF or 0x4E00 <= o <= 0x9FFF or 0xF900 <= o <= 0xFAFF


def plegar(texto):
    salida = []
    for c in texto.upper():
        base = unicodedata.normalize("NFD", c)
        salida.append(base[0] if "LATIN" in unicodedata.name(base[0], "") else c)
    return "".join(salida)


def tokens(texto):
    resultado, actual = [], ""
    caracteres = plegar(texto)
    for i, c in enumerate(caracteres):
        if sin_espacios(c):
            if actual:
                resultado.append(actual)
            actual = ""
            resultado.append(c)
        elif c.isalnum():
            if actual and actual[-1] != "." and actual[-1].isdigit() != c.isdigit():
                resultado.append(actual)
                actual = ""
            actual += c
        elif c in ".," and actual[-1:].isdigit() and caracteres[i + 1:i + 2].isdigit():
            actual += "."
        else:
            if actual:
                resultado.append(actual)
            actual = ""
    if actual:
        resultado.append(actual)
    return resultado


def contiene(linea, aguja):
    n = len(aguja)
    return n > 0 and any(linea[i:i + n] == aguja for i in range(len(linea) - n + 1))


def numero(v):
    return None if v is None else (str(int(v)) if float(v).is_integer() else repr(float(v)))


def piezas(eje):
    categoria, minimo, maximo = eje
    if categoria and categoria.strip():
        return [categoria]
    return [p for p in (numero(minimo), numero(maximo)) if p is not None]


def lineas_pypdf(ruta):
    lineas = []
    for indice, pagina in enumerate(PdfReader(str(ruta)).pages, start=1):
        for linea in (pagina.extract_text() or "").splitlines():
            lineas.append((indice, tokens(linea)))
    return lineas


def pagina_oraculo(lineas, clave, valores):
    if not clave or any(not p for p in clave):
        return None, "clave_vacia"
    minimo = min(2, len(valores))
    if minimo == 0:
        return None, "sin_valores"
    paginas = set()
    for i, (pagina, linea) in enumerate(lineas):
        if not all(contiene(linea, p) for p in clave):
            continue
        cerca = [l for p, l in lineas[max(0, i - RADIO): i + RADIO + 1] if p == pagina]
        if sum(any(contiene(l, v) for l in cerca) for v in valores) >= minimo:
            paginas.add(pagina)
    if len(paginas) == 1:
        return paginas.pop(), None
    return None, "no_aparece" if not paginas else "varias_paginas"


def main():
    salida, carpeta, destino = sys.argv[1], Path(sys.argv[2]), sys.argv[3]
    celdas = [json.loads(l) for l in open(salida, encoding="utf-8") if l.strip()]
    filas = defaultdict(list)
    for c in celdas:
        filas[(c["archivo"], c["tabla"], json.dumps(c["fila"]))].append(c)
    cache, excluidas, puntuables = {}, defaultdict(int), []
    for (archivo, tabla, fila), grupo in filas.items():
        if archivo not in cache:
            cache[archivo] = lineas_pypdf(carpeta / archivo)
        clave = [tokens(p) for p in piezas(json.loads(fila))]
        valores = []
        for c in grupo:
            v = tokens(c["valor"])
            if v and v not in clave and v not in valores:
                valores.append(v)
        pagina, motivo = pagina_oraculo(cache[archivo], clave, valores)
        if motivo:
            excluidas[motivo] += len(grupo)
            continue
        for c in grupo:
            puntuables.append({"caso": f"{archivo}|{tabla}", "fila": fila, "validas": [pagina],
                               "calculada": c["pagina"], "bloque": c["bloque"], "archivo": archivo})
    with open(destino, "w", encoding="utf-8") as f:
        for r in puntuables:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"celdas: {len(celdas)}  puntuables: {len(puntuables)}  excluidas: {dict(excluidas)}")
    por_archivo = defaultdict(lambda: [0, 0])
    for c in celdas:
        por_archivo[c["archivo"]][0] += 1
        por_archivo[c["archivo"]][1] += c["pagina"] is not None
    for archivo, (total, con) in sorted(por_archivo.items()):
        print(f"  {archivo}: {total} celdas, {con} con página")


if __name__ == "__main__":
    main()
