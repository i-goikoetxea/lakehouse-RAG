"""Métricas de la pista Aguja: `recall_aguja`, `pagina_exacta` y `seccion_real` (SSD §4.2).

La página y la sección se leen de los fragmentos que devuelven las tools y que contienen el lote,
recorriendo cualquier anidamiento: así no dependen de la forma exacta de cada tool.
"""

import json
import re

from evaluacion.puntuar import ValorEsperado, puntuar_valores

CLAVES_BREADCRUMB = ("breadcrumb", "breadcrumb_jerarquia")
SEPARADOR_BREADCRUMB = ">"
MAX_PAGINAS_EN_RANGO = 50
_CITA_DE_PAGINAS = re.compile(
    r"(?<!\w)(?:pages?|pp?\.|p[áa]g(?:ina)?s?\.?)\s*(\d+(?:\s*(?:[-–,]|\by\b|\band\b|\bto\b)\s*\d+)*)",
    re.IGNORECASE,
)
_RANGO = re.compile(r"(\d+)\s*[-–]\s*(\d+)|(\d+)")


def recall_aguja(respuesta, aguja):
    """1.0 si la respuesta da el valor con su unidad; acepta coma o punto decimal y unidades convertibles."""
    return puntuar_valores(respuesta, (ValorEsperado(aguja.valor, aguja.unidad),)).puntuacion


def _como_estructura(valor):
    if isinstance(valor, str) and valor.lstrip()[:1] in ("{", "["):
        try:
            return json.loads(valor)
        except (ValueError, RecursionError):
            return valor
    return valor


def _entero(valor):
    return valor if isinstance(valor, int) and not isinstance(valor, bool) else None


def _fragmento(nodo):
    cita = nodo.get("cita") if isinstance(nodo.get("cita"), dict) else {}
    fuentes = (nodo, cita)
    inicio = next((_entero(f.get("pagina_inicio")) for f in fuentes if _entero(f.get("pagina_inicio")) is not None), None)
    fin = next((_entero(f.get("pagina_fin")) for f in fuentes if _entero(f.get("pagina_fin")) is not None), inicio)
    breadcrumb = next((f[c] for f in fuentes for c in CLAVES_BREADCRUMB if isinstance(f.get(c), str)), None)
    if inicio is None and breadcrumb is None:
        return None
    return {"pagina_inicio": inicio, "pagina_fin": fin, "breadcrumb": breadcrumb, "nodo_id": cita.get("nodo_id", nodo.get("nodo_id"))}


def fragmentos_con_aguja(contenido, lote):
    """Fragmentos con página o breadcrumb cuyo texto directo contiene el lote."""
    encontrados = []
    pendientes = [contenido]
    while pendientes:
        nodo = _como_estructura(pendientes.pop())
        if isinstance(nodo, list):
            pendientes.extend(reversed(nodo))
        elif isinstance(nodo, dict):
            if any(isinstance(v, str) and lote in v for v in nodo.values()):
                fragmento = _fragmento(nodo)
                if fragmento is not None:
                    encontrados.append(fragmento)
            pendientes.extend(reversed(list(nodo.values())))
    return encontrados


def pagina_exacta(fragmentos, esperada):
    """1.0 si algún fragmento con la aguja cubre la página física esperada; `None` sin fragmentos con página."""
    con_pagina = [f for f in fragmentos if f["pagina_inicio"] is not None]
    if not con_pagina:
        return None
    return 1.0 if any(f["pagina_inicio"] <= esperada <= f["pagina_fin"] for f in con_pagina) else 0.0


def _normalizar(texto):
    return " ".join(texto.casefold().split())


def seccion_real(fragmentos, seccion):
    """1.0 si algún tramo del breadcrumb de un fragmento con la aguja es la sección esperada."""
    con_breadcrumb = [f for f in fragmentos if f["breadcrumb"]]
    if seccion is None or not con_breadcrumb:
        return None
    esperada = _normalizar(seccion)
    tramos = {_normalizar(t) for f in con_breadcrumb for t in f["breadcrumb"].split(SEPARADOR_BREADCRUMB)}
    return 1.0 if esperada in tramos else 0.0


def seccion_en_respuesta(respuesta, seccion):
    return None if seccion is None else float(_normalizar(seccion) in _normalizar(respuesta))


def paginas_citadas(texto):
    """Páginas que cita la respuesta: `page 54`, `pp. 53-55`, `p. 7`, `página 12`, `págs. 3 y 4`."""
    paginas = set()
    for cita in _CITA_DE_PAGINAS.finditer(texto):
        for rango in _RANGO.finditer(cita.group(1)):
            if rango.group(3):
                paginas.add(int(rango.group(3)))
                continue
            inicio, fin = int(rango.group(1)), int(rango.group(2))
            if 0 <= fin - inicio <= MAX_PAGINAS_EN_RANGO:
                paginas.update(range(inicio, fin + 1))
            else:
                paginas.update((inicio, fin))
    return paginas
