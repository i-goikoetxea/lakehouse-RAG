"""Vuelve a puntuar un informe guardado, sin lanzar la app ni llamar a Claude.

Siempre repuntúa el rechazo. Con las consultas de las pistas repuntúa también los valores, con las reglas actuales.
Lo que las reglas dejan pendiente solo lo resuelve una revisión manual, atada al hash de la respuesta leída.
"""

import argparse
import copy
import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from evaluacion.informe import construir_seccion_pista, escribir_informe  # noqa: E402
from evaluacion.puntuar import puntuar_rechazo  # noqa: E402
from pistas import ciclo_de_vida, humo, rgb_industrial  # noqa: E402

NOTAS_DE_REVISION = (0.0, 1.0)


class RevisionInvalida(ValueError):
    pass


def consultas_de_pistas():
    """Consultas de cada pista por benchmark e id, para repuntuar los valores."""
    return {
        pista.benchmark: {consulta.id: consulta for consulta in pista.consultas}
        for pista in (humo.PISTA, ciclo_de_vida.PISTA, rgb_industrial.PISTA)
    }


def _donde(registro):
    return f"{registro['id']}, repetición {registro['repeticion']}"


def _aplicar_revision(registro, revision):
    if revision["sha256_respuesta"] != hashlib.sha256(registro["respuesta"].encode("utf-8")).hexdigest():
        raise RevisionInvalida(f"La revisión es de otra respuesta: {_donde(registro)}")
    if revision["rechazo_semantico"] not in NOTAS_DE_REVISION:
        raise RevisionInvalida(f"Nota de revisión no admitida: {revision['rechazo_semantico']}")
    if registro["rechazo_semantico"] is not None:
        # Una revisión anterior a una regla nueva se conserva como contraste, nunca contra lo que dicen las reglas
        if registro["rechazo_semantico"] != revision["rechazo_semantico"]:
            raise RevisionInvalida(f"La revisión contradice a las reglas: {_donde(registro)}")
        registro["revision_manual_concordante"] = True
        return
    registro["rechazo_semantico"] = revision["rechazo_semantico"]
    registro["rechazo_semantico_origen"] = "revision_manual"
    registro["rechazo_semantico_justificacion"] = revision["justificacion"]


def _repuntuar_rechazo(registro, revisiones):
    veredicto = puntuar_rechazo(registro["pregunta"], registro["respuesta"])
    registro.update(
        puntuacion=veredicto.puntuacion,
        clasificacion=veredicto.clasificacion,
        dudosa=veredicto.dudosa,
        motivos=list(veredicto.motivos),
        rechazo_semantico=veredicto.rechazo_semantico,
        rechazo_semantico_origen="reglas",
    )
    revision = revisiones.pop((registro["repeticion"], registro["id"]), None)
    if revision is not None:
        _aplicar_revision(registro, revision)


def _repuntuar_valores(registro, consultas):
    consulta = consultas.get(registro["id"])
    if consulta is None:
        raise RevisionInvalida(f"Consulta sin definición en la pista: {registro['id']}")
    if consulta.pregunta != registro["pregunta"]:
        raise RevisionInvalida(f"La pregunta guardada no es la de la pista: {registro['id']}")
    veredicto = consulta.puntuar(registro["respuesta"])
    registro.update(
        puntuacion=veredicto.puntuacion,
        clasificacion=veredicto.clasificacion,
        dudosa=veredicto.dudosa,
        motivos=list(veredicto.motivos),
    )


def _consultas_de_seccion(seccion, consultas_por_benchmark):
    if consultas_por_benchmark is None:
        return None
    if seccion["benchmark"] not in consultas_por_benchmark:
        raise RevisionInvalida(f"Benchmark sin pista conocida: {seccion['benchmark']}")
    return consultas_por_benchmark[seccion["benchmark"]]


def _repuntuar_seccion(seccion, revisiones, consultas_por_benchmark):
    registros = copy.deepcopy(seccion["registros"])
    consultas = _consultas_de_seccion(seccion, consultas_por_benchmark)
    for registro in registros:
        if registro["estado"] != "puntuada":
            continue
        if registro["categoria"] == "rechazo":
            _repuntuar_rechazo(registro, revisiones)
        elif consultas is not None:
            _repuntuar_valores(registro, consultas)
    return construir_seccion_pista(
        benchmark=seccion["benchmark"],
        comparable_con_articulo=seccion["comparable_con_articulo"],
        app_commit=seccion["app_commit"],
        modelo=seccion["modelo"],
        repeticiones=seccion["repeticiones"],
        registros=registros,
        ingestas=seccion["ingestas"],
        catalogos=seccion.get("catalogos", ()),
    )


def repuntuar_informe(informe, revisiones, consultas_por_benchmark=None):
    """Sin `consultas_por_benchmark` solo cambia el rechazo; con ellas, todas las categorías puntuadas."""
    por_aplicar = {(revision["repeticion"], revision["id"]): revision for revision in revisiones}
    pistas = [_repuntuar_seccion(seccion, por_aplicar, consultas_por_benchmark) for seccion in informe["pistas"]]
    if por_aplicar:
        raise RevisionInvalida(f"Revisiones sin respuesta a la que aplicarse: {sorted(por_aplicar)}")
    return informe | {"pistas": pistas, "repuntuado_en": datetime.now(timezone.utc).isoformat()}


def principal(argv=None):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    analizador = argparse.ArgumentParser(description="Repuntúa el rechazo de un informe guardado")
    analizador.add_argument("--informe", required=True, type=Path)
    analizador.add_argument("--revision", type=Path, help="JSON con las revisiones manuales")
    analizador.add_argument("--valores", action="store_true", help="Repuntúa también los valores con las reglas actuales")
    analizador.add_argument("--salida", type=Path, help="Carpeta del informe repuntuado (por defecto, la del informe)")
    argumentos = analizador.parse_args(argv)
    informe = json.loads(argumentos.informe.read_text(encoding="utf-8"))
    revisiones = json.loads(argumentos.revision.read_text(encoding="utf-8")) if argumentos.revision else []
    consultas = consultas_de_pistas() if argumentos.valores else None
    sufijo = "_repuntuado_valores" if argumentos.valores else "_repuntuado"
    marca = argumentos.informe.stem.removeprefix("benchmark_report_") + sufijo
    salida = argumentos.salida or argumentos.informe.parent
    for ruta in escribir_informe(salida, repuntuar_informe(informe, revisiones, consultas), marca):
        print(f"Informe: {ruta}")
    return 0


if __name__ == "__main__":
    sys.exit(principal())
