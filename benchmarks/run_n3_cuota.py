"""N3 (SSD de generalidad §6.4): recuperación pura por MCP, sin inferencia de Claude, para la cuota por documento.

Tres pasos, siempre sobre una carpeta aislada (`LAKEHOUSE_RAG_CONFIG_DIR`):

    python run_n3_cuota.py preparar --binario <cli.exe> --directorio <carpeta>
    python run_n3_cuota.py medir --binario <cli.exe> --directorio <carpeta> --salida antes.json
    python run_n3_cuota.py comparar antes.json despues.json

`preparar` crea un centro con varios documentos (el PDF de `aguja_base` con 5 agujas insertadas y los
fixtures de `ciclo_de_vida` y `rgb_industrial`). La ingesta destila con el modelo por defecto de la app.
`medir` solo llama a `search_normative_context`, así que no gasta Claude y se puede repetir con
binarios distintos sobre el mismo centro.

Métricas:
- recall@k de un solo documento: el top-k contiene algún fragmento del documento objetivo con cada
  valor esperado (literal del documento). También se mide con `doc_id`, donde la salida debe ser idéntica;
- cruzadas (A + B): porcentaje de consultas con los dos documentos en el top-k.
"""

import argparse
import json
import re
import sys
from pathlib import Path

RAIZ_BENCHMARKS = Path(__file__).resolve().parent
sys.path.insert(0, str(RAIZ_BENCHMARKS))

import run  # noqa: E402
import run_aguja  # noqa: E402
from lakehouse.cli import ClienteCli, comando_de_binario  # noqa: E402
from lakehouse.mcp import ClienteMcp  # noqa: E402
from pistas.aguja import generar_aguja, insertar_aguja, preguntas_de, texto_de_pdf  # noqa: E402
from pistas.base import cargar_documentos  # noqa: E402

CENTRO = "n3-cuota"
SEMILLA = 42
NOMBRE_PDF = "documento_aguja.pdf"
ARCHIVO_PREPARACION = "n3_preparacion.json"
TOP_K = 8

# Consultas de un solo documento: (id, pregunta, archivo objetivo, valores literales del documento)
CONSULTAS_FIXTURES = (
    ("rgb-ruido-presion", "¿Cuál es la presión de prueba hidrostática de la línea de condensado L-104?", "ruido_objetivo.md", ("12,5",)),
    ("rgb-ruido-tiempo", "¿Cuánto tiempo se mantiene la presión en la prueba hidrostática de la línea L-104?", "ruido_objetivo.md", ("45",)),
    ("rgb-ruido-temperatura", "¿Cuál es la temperatura máxima del agua de prueba de la línea L-104?", "ruido_objetivo.md", ("38",)),
    ("rgb-ruido-espesor", "¿Qué espesor mínimo debe tener la junta de brida en la prueba de la línea L-104?", "ruido_objetivo.md", ("3,2",)),
    ("rgb-ruido-perdida", "¿Qué pérdida de presión se admite al final de la prueba hidrostática de la línea L-104?", "ruido_objetivo.md", ("1,5",)),
    ("rgb-integracion-par", "¿Qué par de apriete llevan las bridas de la línea L-510 y con qué condición es válido?", "integracion_tabular.md", ("185", "LB-2")),
    ("rgb-integracion-precalentamiento", "¿Qué temperatura de precalentamiento lleva el acero P-22 y a partir de qué espesor se exige?", "integracion_tabular.md", ("150", "25")),
    ("rgb-integracion-curado", "¿Cuánto dura el curado del recubrimiento RX-4 y con qué humedad relativa es válido?", "integracion_tabular.md", ("16", "70")),
    ("rgb-integracion-inspeccion", "¿Cada cuánto se inspeccionan las válvulas V-88 y a partir de qué presión de servicio se aplica ese intervalo?", "integracion_tabular.md", ("6", "40")),
    ("rgb-tabular-centro", "Según la Tabla 4-B, ¿qué temperatura mínima de precalentamiento lleva el acero P-91 con espesor de 30 mm y CE de 0,42 %?", "integracion_tabular.md", ("125",)),
    ("rgb-tabular-esquina", "Según la Tabla 4-B, ¿qué temperatura mínima de precalentamiento lleva el acero P-91 con espesor de 55 mm y CE de 0,50 %?", "integracion_tabular.md", ("175",)),
    ("ciclo-vigente-presion", "¿Cuál es actualmente la presión de purga del procedimiento PTF-PUR-05? Indica la revisión.", "purga_rev01.md", ("8,2",)),
    ("ciclo-vigente-temperatura", "¿Qué temperatura del agua de purga está vigente hoy según el procedimiento PTF-PUR-05? Indica la revisión.", "purga_rev01.md", ("85",)),
)


def contiene_valor(texto, valor):
    """Coincidencia del literal sin pegarse a otros dígitos: `6` no casa con `16` ni con `6,5`."""
    patron = r"(?<![\d,.])" + re.escape(valor) + r"(?![\d]|[,.]\d)"
    return re.search(patron, texto) is not None


def resultados_planos(contenido):
    """Los resultados de todos los grupos, por score descendente (como los ordena `fusionar`)."""
    planos = [
        {"id": r["id"], "doc_id": r["cita"]["doc_id"], "score": r["score"], "texto": r["texto"]}
        for grupo in contenido.get("grupos", [])
        for r in grupo.get("resultados", [])
    ]
    return sorted(planos, key=lambda r: (-r["score"], r["id"]))


def rango_de_referencia(planos, doc_id, valores):
    """Posición (1-based) en la que ya están todos los valores en fragmentos del documento; `None` si faltan."""
    pendientes = set(valores)
    for posicion, resultado in enumerate(planos, start=1):
        if resultado["doc_id"] == doc_id:
            pendientes -= {v for v in pendientes if contiene_valor(resultado["texto"], v)}
        if not pendientes:
            return posicion
    return None


def _consultas(preparacion):
    """Consultas de un documento (fixtures y agujas) y cruzadas (aguja léxica + pregunta de fixture)."""
    doc_de = preparacion["doc_id_por_archivo"]
    individuales = [
        {"id": ident, "pregunta": pregunta, "doc_id": doc_de[archivo], "valores": list(valores)}
        for ident, pregunta, archivo, valores in CONSULTAS_FIXTURES
    ]
    for n, aguja in enumerate(preparacion["agujas"]):
        for tipo, pregunta in zip(("lexica", "semantica"), aguja["preguntas"]):
            individuales.append(
                {"id": f"aguja-{n}-{tipo}", "pregunta": pregunta, "doc_id": doc_de[NOMBRE_PDF], "valores": [aguja["lote"]] if tipo == "lexica" else [aguja["valor"]]}
            )
    fixtures = individuales[: len(CONSULTAS_FIXTURES)]
    # Léxica (con el lote) y semántica (sin él): la segunda deja la aguja sin ancla léxica
    cruzadas = [
        {
            "id": f"cruzada-{tipo}-{n}-{fixture['id']}",
            "pregunta": f"{aguja['preguntas'][indice]} {fixture['pregunta']}",
            "documentos": [doc_de[NOMBRE_PDF], fixture["doc_id"]],
            "referencias": [
                [doc_de[NOMBRE_PDF], [aguja["lote"] if indice == 0 else aguja["valor"]]],
                [fixture["doc_id"], fixture["valores"]],
            ],
        }
        for indice, tipo in enumerate(("lexica", "semantica"))
        for n, aguja in enumerate(preparacion["agujas"])
        for fixture in fixtures[n :: len(preparacion["agujas"])]
    ]
    return individuales, cruzadas


def preparar(argumentos):
    directorio = argumentos.directorio.resolve()
    directorio.mkdir(parents=True, exist_ok=True)
    comando = comando_de_binario(argumentos.binario)
    ruta_base, profundidades, _ = run_aguja._configuracion(argparse.Namespace(base=None, profundidad=None, sin_seccion=None))
    pdf = ruta_base.read_bytes()
    texto_base = texto_de_pdf(pdf)
    agujas = []
    for profundidad in profundidades:
        aguja = generar_aguja(SEMILLA, 1, profundidad, texto_base)
        pdf = insertar_aguja(pdf, aguja, profundidad).pdf
        agujas.append({"lote": aguja.lote, "valor": aguja.valor, "profundidad": profundidad, "preguntas": list(preguntas_de(aguja))})
    documentos = directorio / "documentos"
    documentos.mkdir(exist_ok=True)
    (documentos / NOMBRE_PDF).write_bytes(pdf)
    rutas = [documentos / NOMBRE_PDF]
    fixtures = cargar_documentos("ciclo_de_vida", ("purga_rev00.md", "purga_rev01.md")) + cargar_documentos(
        "rgb_industrial",
        ("ruido_objetivo.md", "distractor_1.md", "distractor_2.md", "distractor_3.md", "distractor_4.md", "distractor_5.md", "integracion_tabular.md"),
    )
    for documento in fixtures:
        ruta = documentos / documento.nombre_archivo
        ruta.write_text(documento.contenido, encoding="utf-8")
        rutas.append(ruta)
    cliente = ClienteCli(comando, directorio)
    cliente.crear_centro(CENTRO, directorio / "centros" / CENTRO)
    cliente.activar(CENTRO)
    doc_id_por_archivo, ingestas = {}, []
    for ruta in rutas:
        salida = cliente.ingestar(ruta, CENTRO)
        doc_id_por_archivo[ruta.name] = salida.get("doc_id")
        ingestas.append({"archivo": ruta.name, "doc_id": salida.get("doc_id"), "latencia_ms": salida.get("latencia_ms"), "avisos": salida.get("avisos", [])})
        print(f"Ingerido {ruta.name}: {salida.get('doc_id')} ({salida.get('latencia_ms', 0) / 1000:.0f} s)")
    preparacion = {"centro": CENTRO, "semilla": SEMILLA, "agujas": agujas, "doc_id_por_archivo": doc_id_por_archivo, "ingestas": ingestas}
    (directorio / ARCHIVO_PREPARACION).write_text(json.dumps(preparacion, ensure_ascii=False, indent=1), encoding="utf-8")
    return run.CODIGO_OK


def _buscar(mcp, argumentos_busqueda):
    contenido = mcp.llamar_herramienta(run.HERRAMIENTA_RECUPERACION, argumentos_busqueda).contenido
    return [{k: r[k] for k in ("id", "doc_id", "score")} | {"texto": r["texto"]} for r in resultados_planos(contenido)]


def medir(argumentos):
    directorio = argumentos.directorio.resolve()
    preparacion = json.loads((directorio / ARCHIVO_PREPARACION).read_text(encoding="utf-8"))
    individuales, cruzadas = _consultas(preparacion)
    mcp = ClienteMcp(comando_de_binario(argumentos.binario), directorio)
    mcp.iniciar()
    try:
        for consulta in individuales:
            base = {"centro_id": CENTRO, "consulta": consulta["pregunta"], "top_k": TOP_K}
            consulta["resultados"] = _buscar(mcp, base)
            consulta["resultados_con_doc_id"] = _buscar(mcp, base | {"doc_id": consulta["doc_id"]})
        for consulta in cruzadas:
            consulta["resultados"] = _buscar(mcp, {"centro_id": CENTRO, "consulta": consulta["pregunta"], "top_k": TOP_K})
    finally:
        mcp.cerrar()
    medicion = {"binario_sha256": run._sha256(argumentos.binario), "individuales": individuales, "cruzadas": cruzadas}
    argumentos.salida.write_text(json.dumps(medicion, ensure_ascii=False, indent=1), encoding="utf-8")
    print(resumen(medicion))
    return run.CODIGO_OK


def _identidad(resultados):
    return [r["id"] for r in resultados]


def _ambos_documentos(consulta):
    return all(any(r["doc_id"] == doc for r in consulta["resultados"]) for doc in consulta["documentos"])


def _ambas_referencias(consulta):
    return all(rango_de_referencia(consulta["resultados"], doc, valores) is not None for doc, valores in consulta["referencias"])


def resumen(medicion):
    individuales = medicion["individuales"]
    aciertos = sum(rango_de_referencia(c["resultados"], c["doc_id"], c["valores"]) is not None for c in individuales)
    con_doc = sum(rango_de_referencia(c["resultados_con_doc_id"], c["doc_id"], c["valores"]) is not None for c in individuales)
    ambos = sum(_ambos_documentos(c) for c in medicion["cruzadas"])
    referencias = sum(_ambas_referencias(c) for c in medicion["cruzadas"])
    return (
        f"recall@{TOP_K} un documento: {aciertos}/{len(individuales)} · con doc_id: {con_doc}/{len(individuales)} · "
        f"cruzadas con los dos documentos: {ambos}/{len(medicion['cruzadas'])} · "
        f"con las dos referencias: {referencias}/{len(medicion['cruzadas'])}"
    )


def comparacion(antes, despues):
    """Filas por consulta y los retrocesos (recall perdido o salida con doc_id distinta)."""
    filas, retrocesos = [], []
    for a, d in zip(antes["individuales"], despues["individuales"], strict=True):
        rango_a = rango_de_referencia(a["resultados"], a["doc_id"], a["valores"])
        rango_d = rango_de_referencia(d["resultados"], d["doc_id"], d["valores"])
        otros_d = sum(r["doc_id"] != d["doc_id"] for r in d["resultados"])
        otros_a = sum(r["doc_id"] != a["doc_id"] for r in a["resultados"])
        identica_doc = _identidad(a["resultados_con_doc_id"]) == _identidad(d["resultados_con_doc_id"])
        filas.append({"id": a["id"], "rango_antes": rango_a, "rango_despues": rango_d, "otros_docs_antes": otros_a, "otros_docs_despues": otros_d, "identica_con_doc_id": identica_doc})
        if (rango_a is not None and rango_d is None) or not identica_doc:
            retrocesos.append(a["id"])
    cruzadas = [
        {
            "id": a["id"],
            "ambos_antes": _ambos_documentos(a),
            "ambos_despues": _ambos_documentos(d),
            "referencias_antes": _ambas_referencias(a),
            "referencias_despues": _ambas_referencias(d),
        }
        for a, d in zip(antes["cruzadas"], despues["cruzadas"], strict=True)
    ]
    return {"individuales": filas, "cruzadas": cruzadas, "retrocesos": retrocesos}


def comparar(argumentos):
    antes = json.loads(argumentos.antes.read_text(encoding="utf-8"))
    despues = json.loads(argumentos.despues.read_text(encoding="utf-8"))
    print("Antes:   " + resumen(antes))
    print("Después: " + resumen(despues))
    resultado = comparacion(antes, despues)
    print("| Consulta | Rango antes | Rango después | Otros docs antes | Otros docs después | Igual con doc_id |")
    print("|---|---|---|---|---|---|")
    for f in resultado["individuales"]:
        print(f"| {f['id']} | {f['rango_antes']} | {f['rango_despues']} | {f['otros_docs_antes']} | {f['otros_docs_despues']} | {'sí' if f['identica_con_doc_id'] else 'NO'} |")
    print("| Cruzada | Dos docs antes | Dos docs después | Dos referencias antes | Dos referencias después |")
    print("|---|---|---|---|---|")
    si = {True: "sí", False: "no"}
    for c in resultado["cruzadas"]:
        print(f"| {c['id']} | {si[c['ambos_antes']]} | {si[c['ambos_despues']]} | {si[c['referencias_antes']]} | {si[c['referencias_despues']]} |")
    print(f"Retrocesos: {resultado['retrocesos'] or 'ninguno'}")
    return run.CODIGO_CON_FALLOS if resultado["retrocesos"] else run.CODIGO_OK


def principal(argv=None):
    analizador = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    pasos = analizador.add_subparsers(dest="paso", required=True)
    for nombre in ("preparar", "medir"):
        paso = pasos.add_parser(nombre)
        paso.add_argument("--binario", type=Path, required=True)
        paso.add_argument("--directorio", type=Path, required=True)
        if nombre == "medir":
            paso.add_argument("--salida", type=Path, required=True)
    paso = pasos.add_parser("comparar")
    paso.add_argument("antes", type=Path)
    paso.add_argument("despues", type=Path)
    argumentos = analizador.parse_args(argv)
    return {"preparar": preparar, "medir": medir, "comparar": comparar}[argumentos.paso](argumentos)


if __name__ == "__main__":
    sys.exit(principal())
