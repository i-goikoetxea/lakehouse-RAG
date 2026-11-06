"""Agregación por pista e informe `benchmark_report_<TIMESTAMP>` en JSON y Markdown.

Lo que no se pudo medir es `None` (`n/d`), nunca 0: un fallo de ejecución no es una mala respuesta.
"""

import json
from collections import Counter
from pathlib import Path

METRICA_POR_CATEGORIA = {
    "ruido": "robustez_ruido",
    "rechazo": "rechazo_estricto",
    "integracion": "integracion",
    "tabular": "exactitud_tabular",
    "vigente": "vigente",
    "historico_fecha": "historico_por_fecha",
    "historico_revision": "historico_por_revision",
}


def _media(valores):
    return sum(valores) / len(valores) if valores else None


def _precision_global(puntuadas, repeticiones):
    precisiones = []
    for repeticion in range(1, repeticiones + 1):
        de_repeticion = [r["puntuacion"] for r in puntuadas if r["repeticion"] == repeticion]
        if de_repeticion:
            precisiones.append(_media(de_repeticion))
    if not precisiones:
        return {"media": None, "min": None, "max": None}
    return {"media": _media(precisiones), "min": min(precisiones), "max": max(precisiones)}


def _metricas_de_rechazo_semantico(puntuadas):
    """Con pendientes no hay nota: promediar solo lo resuelto la falsearía sin que se viera."""
    notas = [r.get("rechazo_semantico") for r in puntuadas if r["categoria"] == "rechazo"]
    pendientes = notas.count(None)
    return {
        "rechazo_semantico": None if pendientes else _media(notas),
        "rechazo_semantico_pendientes": pendientes,
    }


def _metricas(puntuadas):
    metricas = {
        nombre: _media([r["puntuacion"] for r in puntuadas if r["categoria"] == categoria])
        for categoria, nombre in METRICA_POR_CATEGORIA.items()
    }
    metricas.update(_metricas_de_rechazo_semantico(puntuadas))
    for clave in ("latencia_respuesta_ms", "latencia_recuperacion_ms"):
        latencias = [r[clave] for r in puntuadas if r[clave] is not None]
        metricas[f"{clave}_media"] = _media(latencias)
    return metricas


def construir_seccion_pista(*, benchmark, comparable_con_articulo, app_commit, modelo, repeticiones, registros, ingestas, catalogos=()):
    puntuadas = [r for r in registros if r["estado"] == "puntuada"]
    fallidas = [r for r in registros if r["estado"] == "fallo_ejecucion"]
    return {
        "benchmark": benchmark,
        "comparable_con_articulo": comparable_con_articulo,
        "app_commit": app_commit,
        "modelo": modelo,
        "repeticiones": repeticiones,
        "total_consultas": len(registros),
        "consultas_puntuadas": len(puntuadas),
        "fallos_de_ejecucion": len(fallidas),
        "fallos_por_tipo": dict(Counter(r["fallo"]["tipo"] for r in fallidas)),
        "fallos_de_recuperacion": sum(1 for r in registros if r["fallo_recuperacion"]),
        "ingestas_no_definitivas": sum(1 for i in ingestas if i["pendiente_destilacion"]),
        "consultas_dudosas": [r["id"] for r in puntuadas if r["dudosa"]],
        "precision_global": _precision_global(puntuadas, repeticiones),
        "metricas": _metricas(puntuadas),
        "ingestas": ingestas,
        "catalogos": list(catalogos),
        "registros": registros,
    }


def _celda(valor):
    if valor is None:
        return "n/d"
    if isinstance(valor, float):
        valor = f"{valor:.3f}"
    return " ".join(str(valor).split()).replace("|", "\\|")


def _rechazo_semantico_de(registro):
    if registro["categoria"] != "rechazo" or registro["estado"] != "puntuada":
        return "-"
    nota = registro.get("rechazo_semantico")
    return "pendiente" if nota is None else nota


def _lineas_de_pista(pista):
    precision = pista["precision_global"]
    comparable = "Comparable con el artículo" if pista["comparable_con_articulo"] else "No comparable con el artículo"
    lineas = [
        f"## {_celda(pista['benchmark'])}",
        "",
        f"{comparable}. Modelo `{pista['modelo']}`, {pista['repeticiones']} repeticiones.",
        "",
        f"- Consultas: {pista['total_consultas']}, puntuadas: {pista['consultas_puntuadas']}",
        f"- Fallos de ejecución: {pista['fallos_de_ejecucion']} {pista['fallos_por_tipo'] or ''}".rstrip(),
        f"- Fallos de recuperación por MCP: {pista['fallos_de_recuperacion']}",
        f"- Ingestas no definitivas: {pista['ingestas_no_definitivas']}",
        f"- Precisión global: media {_celda(precision['media'])}, mín. {_celda(precision['min'])}, máx. {_celda(precision['max'])}",
        "",
    ]
    if pista["fallos_de_ejecucion"]:
        lineas += ["**Atención:** la precisión solo cubre las consultas puntuadas; los fallos no entran en ella.", ""]
    pendientes = pista["metricas"]["rechazo_semantico_pendientes"]
    if pendientes:
        lineas += [
            f"**Atención:** {pendientes} respuestas de rechazo requieren revisión manual: se abstienen y citan cifras, "
            "y las reglas no pueden saber a quién las atribuyen. Hasta entonces `rechazo_semantico` queda sin nota.",
            "",
        ]
    lineas += ["| Métrica | Valor |", "| :--- | :--- |"]
    lineas += [f"| {nombre} | {_celda(valor)} |" for nombre, valor in pista["metricas"].items()]
    lineas += ["", "| Rep. | Consulta | Categoría | Estado | Puntuación | Clasificación | Dudosa | Rechazo semántico |", "| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |"]
    for registro in pista["registros"]:
        clasificacion = registro["clasificacion"] or (registro["fallo"] or {}).get("tipo")
        dudosa = "sí" if registro["dudosa"] else "no"
        celdas = (registro["repeticion"], registro["id"], registro["categoria"], registro["estado"], registro["puntuacion"], clasificacion, dudosa, _rechazo_semantico_de(registro))
        lineas.append("| " + " | ".join(_celda(celda) for celda in celdas) + " |")
    lineas.append("")
    return lineas


def renderizar_markdown(informe):
    lineas = [
        "# Informe de benchmark",
        "",
        f"Generado: {informe['generado_en']}. Semilla: {informe['semilla']}. Binario sha256: `{informe['binario']['sha256']}`.",
        "",
        "Las pistas no se mezclan entre sí. Cada respuesta cruda está en el JSON.",
        "",
    ]
    for pista in informe["pistas"]:
        lineas += _lineas_de_pista(pista)
    return "\n".join(lineas)


def escribir_informe(directorio, informe, marca_temporal):
    directorio = Path(directorio)
    directorio.mkdir(parents=True, exist_ok=True)
    ruta_json = directorio / f"benchmark_report_{marca_temporal}.json"
    ruta_md = directorio / f"benchmark_report_{marca_temporal}.md"
    ruta_json.write_text(json.dumps(informe, ensure_ascii=False, indent=2), encoding="utf-8")
    ruta_md.write_text(renderizar_markdown(informe), encoding="utf-8")
    return ruta_json, ruta_md
