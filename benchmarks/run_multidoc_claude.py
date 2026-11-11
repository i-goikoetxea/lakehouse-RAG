"""Regla multidocumento con Claude real: las mismas preguntas con dos binarios sobre el centro de N3.

    python run_multidoc_claude.py preguntar --antes <cli_main.exe> --despues <cli_rama.exe> --directorio <carpeta> --salida crudas.jsonl
    python run_multidoc_claude.py informe crudas.jsonl

Usa las 23 preguntas de un documento y las 26 cruzadas de `run_n3_cuota.py`. Para repartir la carga
del equipo, alterna el orden de los binarios en cada pregunta. Cada respuesta se guarda al llegar, y
volver a ejecutar salta las que ya están.

Puntuación (reglas de `evaluacion.puntuar`, sin Claude):
- fixtures: la consulta de su pista, con esperados y prohibidos;
- aguja: el valor de la aguja;
- cruzada: correcta solo si lo son las dos partes.
"""

import argparse
import json
import statistics
import sys
import threading
from pathlib import Path

import psutil

RAIZ_BENCHMARKS = Path(__file__).resolve().parent
sys.path.insert(0, str(RAIZ_BENCHMARKS))

import run  # noqa: E402
from evaluacion.puntuar import ValorEsperado, puntuar_valores  # noqa: E402
from lakehouse.cli import ClienteCli, FalloEjecucion, comando_de_binario  # noqa: E402
from pistas import ciclo_de_vida, rgb_industrial  # noqa: E402
from run_n3_cuota import ARCHIVO_PREPARACION, CENTRO, CONSULTAS_FIXTURES, _consultas  # noqa: E402

MODELO = "sonnet"
VARIANTES = ("antes", "despues")


def _consultas_de_pista():
    """Id de `run_n3_cuota` (`rgb-…`, `ciclo-…`) → consulta puntuable de su pista."""
    por_id = {f"rgb-{c.id}": c for c in rgb_industrial.PISTA.consultas}
    por_id |= {f"ciclo-{c.id}": c for c in ciclo_de_vida.PISTA.consultas}
    return {ident: por_id[ident] for ident, *_ in CONSULTAS_FIXTURES}


def puntuadores(preparacion):
    """Id de pregunta → función que puntúa una respuesta (1.0 o 0.0) y la describe."""
    pista = _consultas_de_pista()
    agujas = {f"aguja-{n}": a for n, a in enumerate(preparacion["agujas"])}

    def de_fixture(ident):
        return lambda texto: pista[ident].puntuar(texto).puntuacion

    def de_aguja(aguja):
        return lambda texto: puntuar_valores(texto, (ValorEsperado(aguja["valor"]),)).puntuacion

    individuales, cruzadas = _consultas(preparacion)
    resultado = {}
    for consulta in individuales:
        ident = consulta["id"]
        resultado[ident] = de_fixture(ident) if ident in pista else de_aguja(agujas[ident.rsplit("-", 1)[0]])
    for consulta in cruzadas:
        # cruzada-<tipo>-<n>-<id de fixture>
        _, _, n, fixture = consulta["id"].split("-", 3)
        partes = (de_aguja(agujas[f"aguja-{n}"]), de_fixture(fixture))
        resultado[consulta["id"]] = lambda texto, partes=partes: min(p(texto) for p in partes)
    return resultado


class MuestreoCpu:
    """CPU media del equipo mientras dura una pregunta: una muestra por segundo."""

    def __init__(self):
        self._muestras, self._parar = [], threading.Event()
        self._hilo = threading.Thread(target=self._muestrear, daemon=True)

    def _muestrear(self):
        psutil.cpu_percent(None)
        while not self._parar.wait(1.0):
            self._muestras.append(psutil.cpu_percent(None))

    def __enter__(self):
        self._hilo.start()
        return self

    def __exit__(self, *_):
        self._parar.set()
        self._hilo.join()

    @property
    def media(self):
        return statistics.fmean(self._muestras) if self._muestras else None


def _ya_hechas(salida):
    if not salida.exists():
        return set()
    return {(r["id"], r["variante"]) for r in map(json.loads, salida.read_text(encoding="utf-8").splitlines()) if r.get("fallo") is None}


def _preguntar(cliente, consulta, variante, puntuar):
    registro = {"id": consulta["id"], "variante": variante, "tipo": "cruzada" if consulta["id"].startswith("cruzada") else "individual", "fallo": None}
    with MuestreoCpu() as cpu:
        try:
            respuesta = cliente.preguntar(consulta["pregunta"], CENTRO, MODELO)
        except FalloEjecucion as fallo:
            return registro | {"fallo": run._fallo_como_dict(fallo), "cpu_media": cpu.media}
    herramientas = [h.get("nombre") for h in respuesta.herramientas if isinstance(h, dict)]
    return registro | {
        "correcta": puntuar(respuesta.texto),
        "respuesta": respuesta.texto,
        "herramientas": herramientas,
        "argumentos": [h.get("argumentos") for h in respuesta.herramientas if isinstance(h, dict)],
        "arnes": respuesta.arnes,
        "latencia_ms": respuesta.latencia_ms,
        "cpu_media": cpu.media,
    }


def preguntar(argumentos):
    directorio = argumentos.directorio.resolve()
    preparacion = json.loads((directorio / ARCHIVO_PREPARACION).read_text(encoding="utf-8"))
    individuales, cruzadas = _consultas(preparacion)
    puntuar = puntuadores(preparacion)
    clientes = {
        "antes": ClienteCli(comando_de_binario(argumentos.antes), directorio),
        "despues": ClienteCli(comando_de_binario(argumentos.despues), directorio),
    }
    hechas = _ya_hechas(argumentos.salida)
    with open(argumentos.salida, "a", encoding="utf-8") as crudas:
        for numero, consulta in enumerate(individuales + cruzadas):
            orden = VARIANTES if numero % 2 == 0 else tuple(reversed(VARIANTES))
            for variante in orden:
                if (consulta["id"], variante) in hechas:
                    continue
                registro = _preguntar(clientes[variante], consulta, variante, puntuar[consulta["id"]])
                crudas.write(json.dumps(registro, ensure_ascii=False) + "\n")
                crudas.flush()
                print(f"{consulta['id']} [{variante}] correcta={registro.get('correcta')} tools={len(registro.get('herramientas', []))} fallo={registro['fallo']}", flush=True)
    return run.CODIGO_OK


def _ultimas(registros):
    """El último registro sin fallo de cada (id, variante); si solo hay fallos, el último fallo."""
    por_clave = {}
    for r in registros:
        clave = (r["id"], r["variante"])
        if r.get("fallo") is None or clave not in por_clave or por_clave[clave].get("fallo") is not None:
            por_clave[clave] = r
    return por_clave


def resumen(registros):
    por_clave = _ultimas(registros)
    filas = {}
    for tipo in ("individual", "cruzada"):
        for variante in VARIANTES:
            grupo = [r for (_, v), r in por_clave.items() if v == variante and r["tipo"] == tipo]
            validas = [r for r in grupo if r.get("fallo") is None]
            filas[(tipo, variante)] = {
                "preguntas": len(grupo),
                "fallos_ejecucion": len(grupo) - len(validas),
                "correctas": sum(r["correcta"] == 1.0 for r in validas),
                "tools_media": statistics.fmean(len(r["herramientas"]) for r in validas) if validas else None,
                "busquedas_media": statistics.fmean(r["herramientas"].count("search_normative_context") for r in validas) if validas else None,
                "catalogo_media": statistics.fmean(r["herramientas"].count("inspect_lakehouse_catalog") for r in validas) if validas else None,
                "latencia_mediana_s": statistics.median(r["latencia_ms"] / 1000 for r in validas) if validas else None,
                "cpu_media": statistics.fmean(r["cpu_media"] for r in validas if r.get("cpu_media") is not None) if validas else None,
            }
    cambios = []
    ids = sorted({i for i, _ in por_clave})
    for ident in ids:
        antes, despues = por_clave.get((ident, "antes")), por_clave.get((ident, "despues"))
        if not antes or not despues or antes.get("fallo") or despues.get("fallo"):
            continue
        if antes["correcta"] != despues["correcta"]:
            cambios.append({"id": ident, "antes": antes["correcta"], "despues": despues["correcta"]})
    return filas, cambios


def informe(argumentos):
    registros = [json.loads(linea) for linea in argumentos.crudas.read_text(encoding="utf-8").splitlines() if linea.strip()]
    filas, cambios = resumen(registros)
    print("| Tipo | Variante | Preguntas | Fallos de ejecución | Correctas | Tools (media) | Búsquedas (media) | Catálogo (media) | Latencia mediana (s) | CPU media (%) |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    formato = lambda v: "—" if v is None else (f"{v:.2f}" if isinstance(v, float) else str(v))  # noqa: E731
    for (tipo, variante), f in filas.items():
        print(f"| {tipo} | {variante} | " + " | ".join(formato(f[k]) for k in ("preguntas", "fallos_ejecucion", "correctas", "tools_media", "busquedas_media", "catalogo_media", "latencia_mediana_s", "cpu_media")) + " |")
    print("Cambios de corrección (antes → después):")
    for c in cambios:
        print(f"- {c['id']}: {c['antes']} → {c['despues']}")
    return run.CODIGO_OK


def principal(argv=None):
    analizador = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    pasos = analizador.add_subparsers(dest="paso", required=True)
    paso = pasos.add_parser("preguntar")
    paso.add_argument("--antes", type=Path, required=True)
    paso.add_argument("--despues", type=Path, required=True)
    paso.add_argument("--directorio", type=Path, required=True)
    paso.add_argument("--salida", type=Path, required=True)
    paso = pasos.add_parser("informe")
    paso.add_argument("crudas", type=Path)
    argumentos = analizador.parse_args(argv)
    return {"preguntar": preguntar, "informe": informe}[argumentos.paso](argumentos)


if __name__ == "__main__":
    sys.exit(principal())
