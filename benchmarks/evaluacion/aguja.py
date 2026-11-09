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
    r"(?<!\w)(?:pages?|pp?\.|p[áa]g(?:ina)?s?\.?)(?:\s+(?:del\s+dato|exacta|f[íi]sica|of\s+the\s+value))?"
    r"[\s*:]*(\d+(?:\s*(?:[-–,]|\by\b|\band\b|\bto\b)\s*\d+)*)",
    re.IGNORECASE,
)
_RANGO = re.compile(r"(\d+)\s*[-–]\s*(\d+)|(\d+)")
# Bloque que añade la app tras la respuesta del modelo (`ssd_citas_pagina.md` §5.4)
ENCABEZADO_VERIFICACION = "**Verificación automática de páginas**"
ETAPA_VERIFICACION = "verificacion_pagina"
_MARCA_DE_PAGINA = re.compile(r"⟦p\. [1-9]\d{0,5}⟧ ?")


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


def paginas_estrictas_citadas(texto):
    """Páginas citadas solas: un rango o una lista no cuentan."""
    paginas = set()
    for cita in _CITA_DE_PAGINAS.finditer(texto):
        numeros = re.findall(r"\d+", cita.group(1))
        if len(numeros) == 1:
            paginas.add(int(numeros[0]))
    return paginas


def pagina_exacta_estricta(respuesta, esperada):
    """1.0 solo si la respuesta cita la página esperada sola: un rango o una lista no cuentan."""
    return 1.0 if esperada in paginas_estrictas_citadas(respuesta) else 0.0


def texto_del_modelo(respuesta):
    """La respuesta sin el bloque de verificación que añade la app: lo que escribió el modelo."""
    return respuesta.split(ENCABEZADO_VERIFICACION, 1)[0]


def valor_y_pagina(respuesta, aguja, otra, esperada, pagina_otra):
    """Métrica conjunta (a la manera de la SAA de CiteVQA), sobre el texto del modelo.

    Acierta si da el valor de esta aguja y cita sola su página. El valor o la página de la otra aguja
    solo se admiten en una línea que nombre el lote de la otra: es mencionarla como descartada, no
    confundirla. Cualquier otra página citada sola es un fallo.
    """
    texto = texto_del_modelo(respuesta)
    if recall_aguja(texto, aguja) != 1.0 or esperada not in paginas_estrictas_citadas(texto):
        return 0.0
    for linea in texto.splitlines():
        de_la_otra = otra.lote in linea
        if recall_aguja(linea, otra) == 1.0 and not de_la_otra:
            return 0.0
        for pagina in paginas_estrictas_citadas(linea):
            if pagina != esperada and not (pagina == pagina_otra and de_la_otra):
                return 0.0
    return 1.0


def fragmento_con_ambas(contenido, lote, otro_lote):
    """`True` si algún texto devuelto por las tools contiene los dos lotes: las agujas comparten chunk."""
    pendientes = [contenido]
    while pendientes:
        nodo = _como_estructura(pendientes.pop())
        if isinstance(nodo, str):
            if lote in nodo and otro_lote in nodo:
                return True
        elif isinstance(nodo, list):
            pendientes.extend(nodo)
        elif isinstance(nodo, dict):
            pendientes.extend(nodo.values())
    return False


def coste_de_marcas(salida):
    """Caracteres de las marcas `⟦p. N⟧` (con su espacio) y de toda la salida serializada."""
    serializada = json.dumps(salida, ensure_ascii=False)
    marcas = 0
    pendientes = [salida]
    while pendientes:
        nodo = _como_estructura(pendientes.pop())
        if isinstance(nodo, str):
            marcas += sum(len(m) for m in _MARCA_DE_PAGINA.findall(nodo))
        elif isinstance(nodo, list):
            pendientes.extend(nodo)
        elif isinstance(nodo, dict):
            pendientes.extend(nodo.values())
    return marcas, len(serializada)


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
    ("pagina_exacta_estricta_modelo", lambda c: c.get("pagina_exacta_estricta_modelo")),
    ("valor_y_pagina", lambda c: c.get("valor_y_pagina")),
    ("mismo_chunk", lambda c: c.get("mismo_chunk")),
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


def _bloqueantes(consulta):
    """Intervenciones que rechazan o dirigen; la verificación de páginas solo anota."""
    return [i for i in consulta.get("arnes") or [] if i.get("etapa") != ETAPA_VERIFICACION]


def resumen_arnes(casos):
    """Intervenciones del arnés. Un reintento con la aguja bien dada al final es un falso bloqueo probable:
    el dato es único e inequívoco, aunque el texto rechazado no se conserva. La verificación de páginas
    se cuenta aparte, porque no bloquea."""
    consultas = [c for caso in casos for c in caso["consultas"]]
    por_etapa = {}
    for consulta in consultas:
        for intervencion in consulta.get("arnes") or []:
            etapa = intervencion.get("etapa", "desconocida")
            por_etapa[etapa] = por_etapa.get(etapa, 0) + 1
    return {
        "por_etapa": por_etapa,
        "consultas_con_intervencion": sum(1 for c in consultas if _bloqueantes(c)),
        "consultas_con_verificacion_pagina": sum(
            1 for c in consultas if any(i.get("etapa") == ETAPA_VERIFICACION for i in c.get("arnes") or [])
        ),
        "falsos_bloqueos_probables": sum(
            1
            for c in consultas
            if any(i.get("etapa") == "reintento" for i in c.get("arnes") or []) and c.get("recall_aguja") == 1.0
        ),
    }
