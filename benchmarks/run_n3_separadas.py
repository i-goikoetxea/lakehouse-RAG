"""N3 bis: consulta única frente a consultas separadas por entidad en las cruzadas (A + B), sin Claude.

Usa el centro de `run_n3_cuota.py preparar` y el binario que se indique:

    python run_n3_separadas.py --binario <cli.exe> --directorio <carpeta> --salida separadas.json

Procedimiento genérico de separación (igual para todas las cruzadas y sin palabras de ningún idioma):
la consulta se corta tras cada signo de cierre de interrogación (`?`). Un trozo sin `?` (una
instrucción como «Indica la revisión.») se une al trozo anterior. Cada trozo es una subconsulta.
Límite: una instrucción seguida de otra pregunta en el mismo trozo no se separa de ella.

Métricas por cruzada:
- consulta única con `top_k = 8`, más el puesto en el top 20 (el máximo de la tool) del primer
  fragmento de cada documento y de cada referencia, en `HYBRID`, `EXACT_BM25` y `SEMANTIC_ONLY`;
- separadas con `top_k = 8` cada una (hasta 16 fragmentos) y con `top_k = 4` cada una (8, el mismo
  presupuesto que la única);
- `doc_id` del catálogo que aparecen literalmente en la consulta.
"""

import argparse
import json
import re
import sys
from pathlib import Path

RAIZ_BENCHMARKS = Path(__file__).resolve().parent
sys.path.insert(0, str(RAIZ_BENCHMARKS))

import run  # noqa: E402
from lakehouse.cli import comando_de_binario  # noqa: E402
from lakehouse.mcp import ClienteMcp  # noqa: E402
from run_n3_cuota import ARCHIVO_PREPARACION, CENTRO, _consultas, rango_de_referencia, resultados_planos  # noqa: E402

TOP_MAXIMO = 20
MODOS = ("HYBRID", "EXACT_BM25", "SEMANTIC_ONLY")


def separar_por_entidad(consulta):
    """Corta tras cada `?`; lo que no termina en `?` se une al trozo anterior."""
    trozos = [t.strip() for t in re.split(r"(?<=\?)", consulta) if t.strip()]
    subconsultas = []
    for trozo in trozos:
        if subconsultas and not trozo.endswith("?"):
            subconsultas[-1] = f"{subconsultas[-1]} {trozo}"
        else:
            subconsultas.append(trozo)
    return subconsultas


def doc_ids_literales(consulta, doc_ids):
    """`doc_id` del catálogo presentes tal cual en la consulta, sin distinguir mayúsculas."""
    plegada = consulta.casefold()
    return sorted(d for d in set(doc_ids) if d and d.casefold() in plegada)


def puesto_de_documento(planos, doc_id):
    return next((i for i, r in enumerate(planos, start=1) if r["doc_id"] == doc_id), None)


def union_de_resultados(listas):
    """Fragmentos de varias búsquedas sin repetir, en el orden en que aparecen."""
    vistos, union = set(), []
    for lista in listas:
        for r in lista:
            if r["id"] not in vistos:
                vistos.add(r["id"])
                union.append(r)
    return union


def cobertura(resultados, cruzada):
    return {
        "dos_documentos": all(puesto_de_documento(resultados, d) is not None for d in cruzada["documentos"]),
        "dos_referencias": all(rango_de_referencia(resultados, d, v) is not None for d, v in cruzada["referencias"]),
    }


def _buscar(mcp, consulta, top_k, modo="HYBRID"):
    argumentos = {"centro_id": CENTRO, "consulta": consulta, "top_k": top_k, "modo": modo}
    return resultados_planos(mcp.llamar_herramienta(run.HERRAMIENTA_RECUPERACION, argumentos).contenido)


def medir_cruzada(mcp, cruzada, doc_ids):
    unica = _buscar(mcp, cruzada["pregunta"], 8)
    puestos = {}
    for modo in MODOS:
        top = _buscar(mcp, cruzada["pregunta"], TOP_MAXIMO, modo)
        puestos[modo] = {
            doc: {"documento": puesto_de_documento(top, doc), "referencia": rango_de_referencia(top, doc, valores)}
            for doc, valores in cruzada["referencias"]
        }
    subconsultas = separar_por_entidad(cruzada["pregunta"])
    separadas_8 = union_de_resultados([_buscar(mcp, s, 8) for s in subconsultas])
    separadas_4 = union_de_resultados([_buscar(mcp, s, 4) for s in subconsultas])
    return {
        "id": cruzada["id"],
        "subconsultas": subconsultas,
        "doc_ids_literales": doc_ids_literales(cruzada["pregunta"], doc_ids),
        "unica": cobertura(unica, cruzada),
        "puestos_top20": puestos,
        "separadas_8": cobertura(separadas_8, cruzada) | {"fragmentos": len(separadas_8)},
        "separadas_4": cobertura(separadas_4, cruzada) | {"fragmentos": len(separadas_4)},
    }


def principal(argv=None):
    analizador = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    analizador.add_argument("--binario", type=Path, required=True)
    analizador.add_argument("--directorio", type=Path, required=True)
    analizador.add_argument("--salida", type=Path, required=True)
    argumentos = analizador.parse_args(argv)
    directorio = argumentos.directorio.resolve()
    preparacion = json.loads((directorio / ARCHIVO_PREPARACION).read_text(encoding="utf-8"))
    _, cruzadas = _consultas(preparacion)
    doc_ids = list(preparacion["doc_id_por_archivo"].values())
    mcp = ClienteMcp(comando_de_binario(argumentos.binario), directorio)
    mcp.iniciar()
    try:
        filas = [medir_cruzada(mcp, c, doc_ids) for c in cruzadas]
    finally:
        mcp.cerrar()
    argumentos.salida.write_text(json.dumps(filas, ensure_ascii=False, indent=1), encoding="utf-8")
    for clave in ("unica", "separadas_4", "separadas_8"):
        docs = sum(f[clave]["dos_documentos"] for f in filas)
        refs = sum(f[clave]["dos_referencias"] for f in filas)
        print(f"{clave}: dos documentos {docs}/{len(filas)} · dos referencias {refs}/{len(filas)}")
    con_dos = sum(len(f["doc_ids_literales"]) >= 2 for f in filas)
    con_uno = sum(len(f["doc_ids_literales"]) == 1 for f in filas)
    print(f"doc_id literales: 2 o más en {con_dos}/{len(filas)} · exactamente 1 en {con_uno}/{len(filas)}")
    return run.CODIGO_OK


if __name__ == "__main__":
    sys.exit(principal())
