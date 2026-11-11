"""Comparación N1 de la extracción de PDF, antes y después (ssd_generalidad_4_piezas.md §1.7 y §6.2).

Lee dos carpetas de JSON escritos por `cargo run --example medir_extraccion` y la carpeta de los
PDF, y mide por documento, sin Claude:

- secciones perdidas (títulos de antes que no están después);
- página de 20 frases por PDF: se eligen en el texto de `pypdf` (oráculo independiente), con una
  sola aparición en todo el documento, y se busca en qué página las pone la app antes y después;
- cobertura de palabras por página: proporción de las palabras de `pypdf` que aparecen en la
  página de la app (señala páginas cuyo texto empeora);
- controles y `U+FFFD` por página;
- clases de la compuerta (después).

Uso: python -I comparar.py <carpeta_pdf> <antes> <despues> [--json salida.json]

La salida resume conteos y páginas, sin texto de los documentos.
"""

from __future__ import annotations

import argparse
import json
import random
import re
import sys
import unicodedata
from pathlib import Path

FRASES_POR_PDF = 20
PALABRAS_POR_FRASE = 6
SEMILLA = 20261111
CAIDA_TOLERADA = 0.02


def esqueleto(texto: str) -> str:
    """Minúsculas, NFKC, sin guiones de corte y con los blancos colapsados."""
    texto = unicodedata.normalize("NFKC", texto).lower()
    texto = re.sub(r"-\s*\n\s*", "", texto)
    return " ".join(texto.split())


def palabras(texto: str) -> list[str]:
    return re.findall(r"\w+", unicodedata.normalize("NFKC", texto).lower())


def paginas_de_la_app(documento: dict | None) -> dict[int, str]:
    """Texto de cada página reconstruido con las secciones y sus saltos de página (bytes UTF-8)."""
    paginas: dict[int, list[str]] = {}
    if not documento:
        return {}
    for seccion in documento.get("secciones", []):
        crudo = seccion.get("contenido", "").encode("utf-8")
        actual = seccion.get("pagina_inicio")
        inicio = 0
        for salto in seccion.get("saltos_pagina", []):
            if actual is not None:
                paginas.setdefault(actual, []).append(crudo[inicio : salto["desplazamiento"]].decode("utf-8", "replace"))
            inicio, actual = salto["desplazamiento"], salto["pagina"]
        if actual is not None:
            paginas.setdefault(actual, []).append(crudo[inicio:].decode("utf-8", "replace"))
        titulo = seccion.get("titulo", "")
        if seccion.get("pagina_inicio") is not None and titulo:
            paginas.setdefault(seccion["pagina_inicio"], []).append(titulo)
    return {p: "\n".join(trozos) for p, trozos in paginas.items()}


def paginas_de_pypdf(ruta: Path) -> dict[int, str]:
    import pypdf

    lector = pypdf.PdfReader(str(ruta))
    resultado = {}
    for indice, pagina in enumerate(lector.pages, start=1):
        try:
            resultado[indice] = pagina.extract_text() or ""
        except Exception:  # noqa: BLE001 - el oráculo puede fallar en una página
            resultado[indice] = ""
    return resultado


def elegir_frases(oraculo: dict[int, str]) -> list[tuple[str, int]]:
    """Frases de 6 palabras de una misma línea de pypdf que aparecen una sola vez en el documento."""
    esqueletos = {p: esqueleto(t) for p, t in oraculo.items()}
    todo = "\n".join(esqueletos.values())
    candidatas = []
    for pagina, texto in oraculo.items():
        for linea in texto.splitlines():
            trozos = linea.split()
            if len(trozos) < PALABRAS_POR_FRASE:
                continue
            frase = esqueleto(" ".join(trozos[:PALABRAS_POR_FRASE]))
            if len(frase) >= 20 and todo.count(frase) == 1:
                candidatas.append((frase, pagina))
    random.Random(SEMILLA).shuffle(candidatas)
    return candidatas[:FRASES_POR_PDF]


def pagina_de_frase(frase: str, paginas: dict[int, str]) -> int | None:
    encontradas = [p for p, t in paginas.items() if frase in esqueleto(t)]
    return encontradas[0] if len(encontradas) == 1 else None


def cobertura(oraculo: str, app: str) -> float | None:
    esperadas = palabras(oraculo)
    if len(esperadas) < 20:
        return None
    presentes = set(palabras(app))
    return sum(1 for p in esperadas if p in presentes) / len(esperadas)


def anomalos(texto: str) -> int:
    return sum(
        1
        for c in texto
        if c == "�" or (unicodedata.category(c) == "Cc" and c not in "\n\r\t")
    )


def comparar_pdf(ruta: Path, antes: dict, despues: dict) -> dict:
    oraculo = paginas_de_pypdf(ruta)
    pag_antes = paginas_de_la_app(antes.get("documento"))
    pag_despues = paginas_de_la_app(despues.get("documento"))
    doc_antes = antes.get("documento") or {}
    doc_despues = despues.get("documento") or {}

    titulos_antes = [s["titulo"] for s in doc_antes.get("secciones", [])]
    titulos_despues = [s["titulo"] for s in doc_despues.get("secciones", [])]
    perdidas = [t for t in titulos_antes if t not in titulos_despues]

    frases = elegir_frases(oraculo)
    resultado_frases = {"total": len(frases), "igual_pagina_antes_despues": 0, "despues_en_pagina_pypdf": 0,
                        "antes_en_pagina_pypdf": 0, "no_localizadas_despues": 0, "cambian": []}
    for frase, pagina in frases:
        p_antes = pagina_de_frase(frase, pag_antes)
        p_despues = pagina_de_frase(frase, pag_despues)
        resultado_frases["antes_en_pagina_pypdf"] += p_antes == pagina
        resultado_frases["despues_en_pagina_pypdf"] += p_despues == pagina
        resultado_frases["no_localizadas_despues"] += p_despues is None
        if p_antes == p_despues:
            resultado_frases["igual_pagina_antes_despues"] += 1
        else:
            resultado_frases["cambian"].append({"pypdf": pagina, "antes": p_antes, "despues": p_despues})

    paginas = []
    for pagina in sorted(oraculo):
        c_antes = cobertura(oraculo[pagina], pag_antes.get(pagina, ""))
        c_despues = cobertura(oraculo[pagina], pag_despues.get(pagina, ""))
        paginas.append({
            "pagina": pagina,
            "cobertura_antes": c_antes,
            "cobertura_despues": c_despues,
            "anomalos_antes": anomalos(pag_antes.get(pagina, "")),
            "anomalos_despues": anomalos(pag_despues.get(pagina, "")),
        })
    empeoran = [
        p for p in paginas
        if p["cobertura_antes"] is not None and p["cobertura_despues"] is not None
        and p["cobertura_despues"] < p["cobertura_antes"] - CAIDA_TOLERADA
    ]
    mejoran = [
        p for p in paginas
        if p["cobertura_antes"] is not None and p["cobertura_despues"] is not None
        and p["cobertura_despues"] > p["cobertura_antes"] + CAIDA_TOLERADA
    ]

    def media(clave: str) -> float | None:
        valores = [p[clave] for p in paginas if p[clave] is not None]
        return round(sum(valores) / len(valores), 4) if valores else None

    return {
        "archivo": ruta.name,
        "paginas": len(oraculo),
        "error_antes": antes.get("error"),
        "error_despues": despues.get("error"),
        "segundos_antes": round(antes.get("segundos", 0), 2),
        "segundos_despues": round(despues.get("segundos", 0), 2),
        "requiere_ocr": [doc_antes.get("requiere_ocr"), doc_despues.get("requiere_ocr")],
        "secciones": [len(titulos_antes), len(titulos_despues)],
        "secciones_perdidas": perdidas,
        "frases": resultado_frases,
        "cobertura_media": [media("cobertura_antes"), media("cobertura_despues")],
        "anomalos": [sum(p["anomalos_antes"] for p in paginas), sum(p["anomalos_despues"] for p in paginas)],
        "paginas_que_empeoran": [
            {k: (round(v, 3) if isinstance(v, float) else v) for k, v in p.items()} for p in empeoran
        ],
        "paginas_que_mejoran": len(mejoran),
        "paginas_degradadas": doc_despues.get("paginas_degradadas", []),
        "paginas_sin_texto": doc_despues.get("paginas_sin_texto", []),
    }


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    argumentos = argparse.ArgumentParser()
    argumentos.add_argument("pdf", type=Path)
    argumentos.add_argument("antes", type=Path)
    argumentos.add_argument("despues", type=Path)
    argumentos.add_argument("--json", type=Path)
    opciones = argumentos.parse_args()

    informes = []
    for ruta in sorted(opciones.pdf.glob("*.pdf")):
        nombre = f"{ruta.name}.json"
        antes = json.loads((opciones.antes / nombre).read_text(encoding="utf-8"))
        despues = json.loads((opciones.despues / nombre).read_text(encoding="utf-8"))
        informe = comparar_pdf(ruta, antes, despues)
        informes.append(informe)
        f = informe["frases"]
        print(
            f"{informe['archivo']}: secciones {informe['secciones'][0]}→{informe['secciones'][1]}"
            f" (perdidas {len(informe['secciones_perdidas'])}); frases igual {f['igual_pagina_antes_despues']}/{f['total']}"
            f" (pypdf antes {f['antes_en_pagina_pypdf']}, después {f['despues_en_pagina_pypdf']});"
            f" cobertura {informe['cobertura_media'][0]}→{informe['cobertura_media'][1]};"
            f" empeoran {len(informe['paginas_que_empeoran'])}, mejoran {informe['paginas_que_mejoran']};"
            f" anómalos {informe['anomalos'][0]}→{informe['anomalos'][1]};"
            f" degradadas {informe['paginas_degradadas']}; sin texto {informe['paginas_sin_texto']}"
        )
    if opciones.json:
        opciones.json.write_text(json.dumps(informes, ensure_ascii=False, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
