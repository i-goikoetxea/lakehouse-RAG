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
    r"(?<!\w)(?:pages?|pp?\.|p[áa]g(?:ina)?s?\.?)[\s*:]*(\d+(?:\s*(?:[-–,]|\by\b|\band\b|\bto\b)\s*\d+)*)",
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


def pagina_exacta_estricta(respuesta, esperada):
    """1.0 solo si la respuesta cita la página esperada sola: un rango o una lista no cuentan."""
    for cita in _CITA_DE_PAGINAS.finditer(respuesta):
        numeros = re.findall(r"\d+", cita.group(1))
        if len(numeros) == 1 and int(numeros[0]) == esperada:
            return 1.0
    return 0.0


def _de_tools(clave, origen):
    return lambda consulta: (consulta.get(origen) or {}).get(clave)


def _recall_de_tipo(tipo):
    return lambda consulta: consulta.get("recall_aguja") if consulta.get("tipo") == tipo else None


def _segundos(milisegundos):
    return None if milisegundos is None else milisegundos / 1000


METRICAS_DE_CONSULTA = (
    ("recall_aguja", lambda c: c.get("recall_aguja")),
    ("recall_lexica", _recall_de_tipo("lexica")),
    ("recall_semantica", _recall_de_tipo("semantica")),
    ("pagina_exacta", lambda c: c.get("pagina_exacta")),
    ("pagina_exacta_estricta", lambda c: c.get("pagina_exacta_estricta")),
    ("pagina_tool_agente", _de_tools("pagina_exacta", "tools_del_agente")),
    ("seccion_real", _de_tools("seccion_real", "tools_del_agente")),
    ("pagina_mcp", _de_tools("pagina_exacta", "mcp_directo")),
    ("seccion_mcp", _de_tools("seccion_real", "mcp_directo")),
    ("latencia_respuesta_s", lambda c: _segundos(c.get("latencia_respuesta_ms"))),
)


def _estadisticos(profundidad, metrica, valores):
    valores = [v for v in valores if v is not None]
    if not valores:
        return {"profundidad": profundidad, "metrica": metrica, "n": 0, "media": None, "min": None, "max": None}
    return {
        "profundidad": profundidad,
        "metrica": metrica,
        "n": len(valores),
        "media": sum(valores) / len(valores),
        "min": min(valores),
        "max": max(valores),
    }


def agregar_por_profundidad(casos):
    """Media, mínimo y máximo de cada métrica por profundidad; los nulos no cuentan como cero."""
    filas = []
    for profundidad in sorted({caso["profundidad"] for caso in casos}):
        propios = [caso for caso in casos if caso["profundidad"] == profundidad]
        consultas = [c for caso in propios for c in caso["consultas"] if not c.get("fallo")]
        for metrica, extraer in METRICAS_DE_CONSULTA:
            filas.append(_estadisticos(profundidad, metrica, [extraer(c) for c in consultas]))
        ingestas = [_segundos((caso.get("ingesta") or {}).get("latencia_ms")) for caso in propios]
        filas.append(_estadisticos(profundidad, "ingesta_s", ingestas))
    return filas


def resumen_arnes(casos):
    """Intervenciones del arnés. Un reintento con la aguja bien dada al final es un falso bloqueo probable:
    el dato es único e inequívoco, aunque el texto rechazado no se conserva."""
    consultas = [c for caso in casos for c in caso["consultas"]]
    por_etapa = {}
    for consulta in consultas:
        for intervencion in consulta.get("arnes") or []:
            etapa = intervencion.get("etapa", "desconocida")
            por_etapa[etapa] = por_etapa.get(etapa, 0) + 1
    return {
        "por_etapa": por_etapa,
        "consultas_con_intervencion": sum(1 for c in consultas if c.get("arnes")),
        "falsos_bloqueos_probables": sum(
            1
            for c in consultas
            if any(i.get("etapa") == "reintento" for i in c.get("arnes") or []) and c.get("recall_aguja") == 1.0
        ),
    }
