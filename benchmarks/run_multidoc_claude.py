"""Regla multidocumento con Claude real: las mismas preguntas con dos binarios sobre el centro de N3.

    python run_multidoc_claude.py preguntar --antes <cli_main.exe> --despues <cli_rama.exe> --directorio <carpeta> --salida crudas.jsonl [--pasadas 2]
    python run_multidoc_claude.py informe crudas.jsonl

Usa las 23 preguntas de un documento y las 26 cruzadas de `run_n3_cuota.py`. Para repartir la carga
del equipo, alterna el orden de los binarios en cada pregunta y en cada pasada. Cada respuesta se
guarda al llegar, y volver a ejecutar salta las que ya están.

Puntuación (reglas de `evaluacion.puntuar`, sin Claude):
- fixtures: la consulta de su pista, con esperados y prohibidos;
- aguja: el valor de la aguja;
- cruzada: correcta solo si lo son las dos partes.

Auditoría de trazas, sin listas de idioma: la primera búsqueda que trata una parte de la pregunta
(la pregunta entera o, en una cruzada, cada trozo separado por `?`) «conserva el idioma» si repite al
menos 2 palabras literales de esa parte, de 4 o más letras y sin cifras. Una consulta traducida no
comparte ninguna. El idioma de cada parte se conoce por construcción: la aguja está en inglés y los
fixtures en español.
"""

import argparse
import json
import re
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
from run_n3_separadas import separar_por_entidad  # noqa: E402

MODELO = "sonnet"
VARIANTES = ("antes", "despues")
BUSQUEDA = "search_normative_context"
CATALOGO = "inspect_lakehouse_catalog"
PALABRAS_COMPARTIDAS_MINIMAS = 2
LETRAS_MINIMAS_PALABRA = 4
PATRON_PALABRA = re.compile(r"[^\W\d_]+")


def _consultas_de_pista():
    """Id de `run_n3_cuota` (`rgb-…`, `ciclo-…`) → consulta puntuable de su pista."""
    por_id = {f"rgb-{c.id}": c for c in rgb_industrial.PISTA.consultas}
    por_id |= {f"ciclo-{c.id}": c for c in ciclo_de_vida.PISTA.consultas}
    return {ident: por_id[ident] for ident, *_ in CONSULTAS_FIXTURES}


def puntuadores(preparacion):
    """Id de pregunta → función que puntúa una respuesta (1.0 o 0.0)."""
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


def palabras(texto):
    """Palabras de 4 o más letras (sin cifras), en minúsculas."""
    return {p.casefold() for p in PATRON_PALABRA.findall(texto) if len(p) >= LETRAS_MINIMAS_PALABRA}


def conserva_idioma(consulta, origen):
    return len(palabras(consulta) & palabras(origen)) >= PALABRAS_COMPARTIDAS_MINIMAS


def consultas_de_busqueda(registro):
    return [
        (argumentos or {}).get("consulta", "")
        for nombre, argumentos in zip(registro["herramientas"], registro["argumentos"], strict=True)
        if nombre == BUSQUEDA
    ]


def idiomas_de_partes(registro):
    """Idioma de origen de cada parte, por construcción del set."""
    if registro["tipo"] == "cruzada":
        return ["en", "es"]
    return ["en" if registro["id"].startswith("aguja") else "es"]


def auditoria(registro):
    """Por parte: si la búsqueda que la trata conserva su idioma.

    Con una sola parte, la que la trata es la primera búsqueda (la consulta inicial). En una cruzada,
    es la que más palabras comparte con esa parte (a igualdad, la primera): una consulta traducida de
    una parte puede compartir palabras con la otra. Sin búsquedas, la parte no conserva su idioma.
    """
    consultas = consultas_de_busqueda(registro)
    partes = separar_por_entidad(registro["pregunta"])
    if not consultas:
        return [False] * len(partes)
    if len(partes) == 1:
        return [conserva_idioma(consultas[0], partes[0])]
    return [
        conserva_idioma(max(consultas, key=lambda c, p=parte: len(palabras(c) & palabras(p))), parte)
        for parte in partes
    ]


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


def _leer(ruta):
    if not ruta.exists():
        return []
    return [json.loads(linea) for linea in ruta.read_text(encoding="utf-8").splitlines() if linea.strip()]


def _ya_hechas(salida):
    return {(r["id"], r["variante"], r.get("pasada", 1)) for r in _leer(salida) if r.get("fallo") is None}


def _preguntar(cliente, consulta, variante, puntuar, pasada):
    tipo = "cruzada" if consulta["id"].startswith("cruzada") else "individual"
    registro = {"id": consulta["id"], "variante": variante, "pasada": pasada, "tipo": tipo, "pregunta": consulta["pregunta"], "fallo": None}
    with MuestreoCpu() as cpu:
        try:
            respuesta = cliente.preguntar(consulta["pregunta"], CENTRO, MODELO)
        except FalloEjecucion as fallo:
            return registro | {"fallo": run._fallo_como_dict(fallo), "cpu_media": cpu.media}
    llamadas = [h for h in respuesta.herramientas if isinstance(h, dict)]
    return registro | {
        "correcta": puntuar(respuesta.texto),
        "respuesta": respuesta.texto,
        "herramientas": [h.get("nombre") for h in llamadas],
        "argumentos": [h.get("argumentos") for h in llamadas],
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
        for pasada in range(1, argumentos.pasadas + 1):
            for numero, consulta in enumerate(individuales + cruzadas):
                orden = VARIANTES if (numero + pasada) % 2 == 0 else tuple(reversed(VARIANTES))
                for variante in orden:
                    if (consulta["id"], variante, pasada) in hechas:
                        continue
                    registro = _preguntar(clientes[variante], consulta, variante, puntuar[consulta["id"]], pasada)
                    crudas.write(json.dumps(registro, ensure_ascii=False) + "\n")
                    crudas.flush()
                    print(
                        f"p{pasada} {consulta['id']} [{variante}] correcta={registro.get('correcta')} "
                        f"tools={len(registro.get('herramientas', []))} fallo={registro['fallo']}",
                        flush=True,
                    )
    return run.CODIGO_OK


def _ultimas(registros):
    """El último registro sin fallo de cada (id, variante, pasada); si solo hay fallos, el último."""
    por_clave = {}
    for r in registros:
        clave = (r["id"], r["variante"], r.get("pasada", 1))
        if r.get("fallo") is None or clave not in por_clave or por_clave[clave].get("fallo") is not None:
            por_clave[clave] = r
    return por_clave


def _media(valores):
    valores = [v for v in valores if v is not None]
    return statistics.fmean(valores) if valores else None


def resumen(registros):
    """Filas por (tipo, variante), cambios por pregunta y auditoría de idioma por (variante, idioma)."""
    por_clave = _ultimas(registros)
    pasadas = sorted({p for _, _, p in por_clave})
    filas = {}
    for tipo in ("individual", "cruzada"):
        for variante in VARIANTES:
            grupo = [r for (_, v, _), r in por_clave.items() if v == variante and r["tipo"] == tipo]
            validas = [r for r in grupo if r.get("fallo") is None]
            por_pasada = [
                sum(r["correcta"] == 1.0 for r in validas if r.get("pasada", 1) == p) for p in pasadas
            ]
            filas[(tipo, variante)] = {
                "preguntas_por_pasada": len(grupo) // max(len(pasadas), 1),
                "fallos_ejecucion": len(grupo) - len(validas),
                "correctas_por_pasada": por_pasada,
                "tools_media": _media(len(r["herramientas"]) for r in validas),
                "busquedas_media": _media(r["herramientas"].count(BUSQUEDA) for r in validas),
                "catalogo_media": _media(r["herramientas"].count(CATALOGO) for r in validas),
                "latencia_mediana_s": statistics.median(r["latencia_ms"] / 1000 for r in validas) if validas else None,
                "cpu_media": _media(r.get("cpu_media") for r in validas),
            }
    aciertos = {}
    for (ident, variante, _), r in por_clave.items():
        if r.get("fallo") is None:
            aciertos.setdefault(ident, {v: [] for v in VARIANTES})[variante].append(r["correcta"] == 1.0)
    cambios = [
        {"id": ident, "antes": f"{sum(a['antes'])}/{len(a['antes'])}", "despues": f"{sum(a['despues'])}/{len(a['despues'])}"}
        for ident, a in sorted(aciertos.items())
        if sum(a["antes"]) != sum(a["despues"])
    ]
    idioma = {}
    for r in por_clave.values():
        if r.get("fallo") is not None or "pregunta" not in r:
            continue
        for lengua, conserva in zip(idiomas_de_partes(r), auditoria(r), strict=False):
            idioma.setdefault((r["variante"], lengua), []).append(conserva)
    return filas, cambios, {clave: (sum(v), len(v)) for clave, v in sorted(idioma.items())}


def informe(argumentos):
    filas, cambios, idioma = resumen(_leer(argumentos.crudas))

    def formato(valor):
        if valor is None:
            return "—"
        return f"{valor:.2f}" if isinstance(valor, float) else str(valor)

    claves = ("preguntas_por_pasada", "fallos_ejecucion", "correctas_por_pasada", "tools_media", "busquedas_media", "catalogo_media", "latencia_mediana_s", "cpu_media")
    print("| Tipo | Variante | Preguntas por pasada | Fallos de ejecución | Correctas por pasada | Tools (media) | Búsquedas (media) | Catálogo (media) | Latencia mediana (s) | CPU media (%) |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    for (tipo, variante), fila in filas.items():
        print(f"| {tipo} | {variante} | " + " | ".join(formato(fila[k]) for k in claves) + " |")
    print("\nAuditoría: primera búsqueda que conserva el idioma de la parte")
    for (variante, lengua), (si, total) in idioma.items():
        print(f"- {variante} · partes en {lengua}: {si}/{total}")
    print("\nCambios de aciertos por pregunta (antes → después, sobre las pasadas):")
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
    paso.add_argument("--pasadas", type=int, default=1)
    paso = pasos.add_parser("informe")
    paso.add_argument("crudas", type=Path)
    argumentos = analizador.parse_args(argv)
    return {"preguntar": preguntar, "informe": informe}[argumentos.paso](argumentos)


if __name__ == "__main__":
    sys.exit(principal())
