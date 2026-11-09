"""Pista Aguja (SSD §4.2): un centro por repetición y profundidad, con el PDF base y la aguja insertada.

Sin `--base` usa el documento de `manifiesto_datasets.json`: exige su sha256 y toma de él las
profundidades y las que no tienen sección fiable.
Códigos de salida: 0 sin fallos, 2 uso inválido, 3 con fallos de ejecución o campaña detenida.
"""

import argparse
import json
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

RAIZ_BENCHMARKS = Path(__file__).resolve().parent
sys.path.insert(0, str(RAIZ_BENCHMARKS))

import run  # noqa: E402
from evaluacion.aguja import (  # noqa: E402
    agregar_por_profundidad,
    fragmentos_con_aguja,
    pagina_exacta,
    pagina_exacta_estricta,
    paginas_citadas,
    recall_aguja,
    resumen_arnes,
    seccion_en_respuesta,
    seccion_real,
)
from lakehouse.cli import AislamientoInvalido, ClienteCli, FalloEjecucion, comando_de_binario, valor_admitido  # noqa: E402
from pistas.aguja import PROFUNDIDADES, generar_aguja, insertar_aguja, preguntas_de, texto_de_pdf  # noqa: E402
from pistas.base import Documento, Pista  # noqa: E402

MANIFIESTO = RAIZ_BENCHMARKS / "manifiesto_datasets.json"
CLAVE_DATASET = "aguja_base"
NOMBRE_DOCUMENTO = "documento_aguja.pdf"
DECIMALES_PROFUNDIDAD = 4


def _dataset_del_manifiesto():
    return json.loads(MANIFIESTO.read_text(encoding="utf-8"))["datasets"][CLAVE_DATASET]


def _base_del_manifiesto(dataset):
    ruta = RAIZ_BENCHMARKS / dataset["ruta_local"]
    if not ruta.is_file():
        raise run.UsoInvalido(f"Falta el documento base: descárgalo a {ruta} (ver {MANIFIESTO.name})")
    if run._sha256(ruta) != dataset["sha256"]:
        raise run.UsoInvalido(f"El sha256 de {ruta.name} no coincide con el manifiesto")
    return ruta


def _clave(profundidad):
    return round(profundidad, DECIMALES_PROFUNDIDAD)


def _resumen_de_fragmentos(fragmentos, esperada, seccion):
    return {
        "fragmentos": fragmentos,
        "pagina_exacta": pagina_exacta(fragmentos, esperada),
        "seccion_real": seccion_real(fragmentos, seccion),
    }


def _consulta(cliente, mcp, centro_id, modelo, tipo, pregunta, aguja, esperado):
    registro = {"tipo": tipo, "pregunta": pregunta, "fallo": None, "arnes": []}
    try:
        respuesta = cliente.preguntar(pregunta, centro_id, modelo)
    except FalloEjecucion as fallo:
        return registro | {"fallo": run._fallo_como_dict(fallo)}
    pagina, seccion = esperado
    citadas = paginas_citadas(respuesta.texto)
    registro.update(
        respuesta=respuesta.texto,
        latencia_respuesta_ms=respuesta.latencia_ms,
        herramientas=[h.get("nombre") for h in respuesta.herramientas if isinstance(h, dict)],
        arnes=respuesta.arnes,
        recall_aguja=recall_aguja(respuesta.texto, aguja),
        paginas_citadas_respuesta=sorted(citadas),
        pagina_exacta=float(pagina in citadas),
        pagina_exacta_estricta=pagina_exacta_estricta(respuesta.texto, pagina),
        seccion_en_respuesta=seccion_en_respuesta(respuesta.texto, seccion),
        tools_del_agente=_resumen_de_fragmentos(fragmentos_con_aguja(respuesta.herramientas, aguja.lote), pagina, seccion),
    )
    registro["mcp_directo"] = _recuperacion_directa(mcp, centro_id, pregunta, aguja, esperado)
    return registro


def _recuperacion_directa(mcp, centro_id, pregunta, aguja, esperado):
    """`search_normative_context` con la pregunta tal cual: mide la recuperación sin el agente."""
    if isinstance(mcp, FalloEjecucion):
        return {"fallo": run._fallo_como_dict(mcp)}
    try:
        resultado = mcp.llamar_herramienta(run.HERRAMIENTA_RECUPERACION, {"centro_id": centro_id, "consulta": pregunta})
    except FalloEjecucion as fallo:
        return {"fallo": run._fallo_como_dict(fallo)}
    fragmentos = fragmentos_con_aguja(resultado.contenido, aguja.lote)
    return _resumen_de_fragmentos(fragmentos, *esperado) | {"latencia_ms": resultado.latencia_ms}


def _ejecutar_caso(comando, pdf_base, texto_base, repeticion, profundidad, argumentos, sin_seccion):
    aguja = generar_aguja(argumentos.semilla, repeticion, profundidad, texto_base)
    documento = insertar_aguja(pdf_base, aguja, profundidad, argumentos.primera_del_cuerpo)
    excluida = _clave(profundidad) in sin_seccion
    seccion = None if excluida else documento.seccion_esperada
    caso = {
        "repeticion": repeticion,
        "profundidad": profundidad,
        "profundidad_real": documento.profundidad_real,
        "pagina_esperada": documento.pagina_esperada,
        "seccion_esperada": seccion,
        "seccion_marcador": documento.seccion_esperada,
        "seccion_excluida": excluida,
        "aguja": {"lote": aguja.lote, "valor": aguja.valor, "unidad": aguja.unidad, "frase": aguja.frase},
        "ingesta": None,
        "consultas": [],
    }
    with tempfile.TemporaryDirectory(prefix="benchmark_", ignore_cleanup_errors=True) as temporal:
        raiz = Path(temporal)
        cliente = ClienteCli(comando, raiz)
        centro_id = f"bench-aguja-r{repeticion}-p{round(profundidad * 100):02d}"
        pista = Pista("aguja", "Aguja", False, (Documento(NOMBRE_DOCUMENTO, documento.pdf),), ())
        inicio = time.perf_counter()
        try:
            caso["ingesta"] = run._preparar_centro(cliente, pista, raiz, centro_id, repeticion)[0]
        except FalloEjecucion as fallo:
            caso["ingesta"] = {"fallo": run._fallo_como_dict(fallo), "latencia_ms": (time.perf_counter() - inicio) * 1000}
            return caso
        mcp = run._abrir_mcp(comando, raiz)
        try:
            for tipo, pregunta in zip(("lexica", "semantica"), preguntas_de(aguja)):
                esperado = (documento.pagina_esperada, seccion)
                caso["consultas"].append(_consulta(cliente, mcp, centro_id, argumentos.modelo, tipo, pregunta, aguja, esperado))
        finally:
            if not isinstance(mcp, FalloEjecucion):
                mcp.cerrar()
    return caso


def _motivo_de_parada(caso, inicio, argumentos):
    if argumentos.parar_si_provisional and (caso["ingesta"] or {}).get("pendiente_destilacion"):
        return f"Ingesta provisional en la repetición {caso['repeticion']}, profundidad {caso['profundidad']}"
    minutos = (time.perf_counter() - inicio) / 60
    if argumentos.tiempo_maximo_min is not None and minutos > argumentos.tiempo_maximo_min:
        return f"Se superó el tiempo máximo: {minutos:.1f} min de {argumentos.tiempo_maximo_min} min"
    return None


def _celda(valor):
    if valor is None:
        return "n/d"
    return f"{valor:.2f}" if isinstance(valor, float) else str(valor)


def _tabla_de_casos(casos):
    lineas = [
        "| Rep. | Prof. | Pág. | Sección esperada | Ingesta (s) | Estado | Pregunta | recall | pág. | pág. estricta | pág. tool | sección tool | pág. MCP | sección MCP | Arnés | Latencia (s) |",
        "| -: | -: | -: | :- | -: | :- | :- | -: | -: | -: | -: | -: | -: | -: | :- | -: |",
    ]
    for caso in casos:
        ingesta = caso["ingesta"] or {}
        segundos = ingesta.get("latencia_ms")
        seccion = "excluida" if caso["seccion_excluida"] else caso["seccion_esperada"]
        cabeza = (
            f"| {caso['repeticion']} | {caso['profundidad']:.2f} | {caso['pagina_esperada']} | {seccion} | "
            f"{_celda(segundos / 1000 if segundos else None)} | {ingesta.get('estado_contenido') or ingesta.get('fallo', {}).get('tipo')} |"
        )
        for consulta in caso["consultas"] or [{"tipo": "—"}]:
            agente = consulta.get("tools_del_agente", {})
            directo = consulta.get("mcp_directo", {})
            latencia = consulta.get("latencia_respuesta_ms")
            arnes = ", ".join(i.get("etapa", "?") for i in consulta.get("arnes") or []) or "—"
            lineas.append(
                f"{cabeza} {consulta['tipo']} | {_celda(consulta.get('recall_aguja'))} | {_celda(consulta.get('pagina_exacta'))} | "
                f"{_celda(consulta.get('pagina_exacta_estricta'))} | {_celda(agente.get('pagina_exacta'))} | "
                f"{_celda(agente.get('seccion_real'))} | {_celda(directo.get('pagina_exacta'))} | "
                f"{_celda(directo.get('seccion_real'))} | {arnes} | {_celda(latencia / 1000 if latencia else None)} |"
            )
    return lineas


def _tabla_de_agregados(agregados):
    lineas = ["| Prof. | Métrica | n | Media | Mín. | Máx. |", "| -: | :- | -: | -: | -: | -: |"]
    for fila in agregados:
        lineas.append(
            f"| {fila['profundidad']:.2f} | {fila['metrica']} | {fila['n']} | {_celda(fila['media'])} | "
            f"{_celda(fila['min'])} | {_celda(fila['max'])} |"
        )
    return lineas


def _markdown(informe):
    arnes = informe["arnes"]
    lineas = [
        "# Pista Aguja",
        "",
        f"Base `{informe['base']['nombre']}` (sha256 `{informe['base']['sha256'][:12]}…`), binario sha256 "
        f"`{informe['binario']['sha256'][:12]}…`, modelo `{informe['modelo']}`, semilla {informe['semilla']}, "
        f"{informe['duracion_min']:.1f} min.",
        "",
        f"Detenida: {informe['detenida'] or 'no'}.",
        "",
        "## Agregados por profundidad",
        "",
        *_tabla_de_agregados(informe["agregados"]),
        "",
        "## Arnés",
        "",
        f"Intervenciones por etapa: {arnes['por_etapa'] or 'ninguna'}. Consultas con intervención: "
        f"{arnes['consultas_con_intervencion']}. Falsos bloqueos probables: {arnes['falsos_bloqueos_probables']}.",
        "",
        "## Casos",
        "",
        *_tabla_de_casos(informe["casos"]),
    ]
    return "\n".join(lineas) + "\n"


def _hay_fallos(casos):
    return any((c["ingesta"] or {}).get("fallo") or any(q.get("fallo") for q in c["consultas"]) for c in casos)


def _analizar_argumentos(argv):
    analizador = argparse.ArgumentParser(description="Pista Aguja del benchmark de caja negra")
    analizador.add_argument("--binario", required=True, type=Path)
    analizador.add_argument("--base", type=Path, help="PDF base; por defecto, el del manifiesto con su sha256")
    analizador.add_argument("--profundidad", type=float, action="append", help="Por defecto, las del manifiesto")
    analizador.add_argument("--sin-seccion", type=float, action="append", help="Profundidad sin sección fiable")
    analizador.add_argument("--primera-del-cuerpo", type=int, default=1)
    analizador.add_argument("--modelo", default="sonnet")
    analizador.add_argument("--repeticiones", type=int, default=1)
    analizador.add_argument("--semilla", type=int, default=42)
    analizador.add_argument("--parar-si-provisional", action="store_true")
    analizador.add_argument("--tiempo-maximo-min", type=float)
    analizador.add_argument("--salida", type=Path, default=RAIZ_BENCHMARKS / "resultados")
    return analizador.parse_args(argv)


def _configuracion(argumentos):
    """Base, profundidades y profundidades sin sección fiable: de los argumentos o del manifiesto."""
    if argumentos.base is not None:
        profundidades = argumentos.profundidad or list(PROFUNDIDADES)
        return argumentos.base, profundidades, {_clave(p) for p in argumentos.sin_seccion or ()}
    dataset = _dataset_del_manifiesto()
    profundidades = argumentos.profundidad or dataset.get("profundidades") or list(PROFUNDIDADES)
    sin_seccion = argumentos.sin_seccion or dataset.get("profundidades_sin_seccion_fiable", [])
    return _base_del_manifiesto(dataset), profundidades, {_clave(p) for p in sin_seccion}


def _ejecutar(argumentos):
    if argumentos.repeticiones < 1:
        raise run.UsoInvalido("--repeticiones debe ser al menos 1")
    if not argumentos.binario.is_file():
        raise run.UsoInvalido(f"No existe el binario: {argumentos.binario}")
    try:
        valor_admitido(argumentos.modelo)
    except FalloEjecucion as fallo:
        raise run.UsoInvalido(f"--modelo: {fallo.mensaje}") from fallo
    ruta_base, profundidades, sin_seccion = _configuracion(argumentos)
    comando = comando_de_binario(argumentos.binario)
    run._comprobar_contrato(comando, [])
    pdf_base = ruta_base.read_bytes()
    texto_base = texto_de_pdf(pdf_base)
    marca = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S_%f")
    argumentos.salida.mkdir(parents=True, exist_ok=True)
    inicio = time.perf_counter()
    casos, detenida = [], None
    # Cada caso se guarda al terminar: un corte no tira lo ya pagado
    with open(argumentos.salida / f"aguja_crudas_{marca}.jsonl", "w", encoding="utf-8") as crudas:
        for repeticion in range(1, argumentos.repeticiones + 1):
            for profundidad in profundidades:
                caso = _ejecutar_caso(comando, pdf_base, texto_base, repeticion, profundidad, argumentos, sin_seccion)
                casos.append(caso)
                crudas.write(json.dumps(caso, ensure_ascii=False) + "\n")
                crudas.flush()
                detenida = _motivo_de_parada(caso, inicio, argumentos)
                if detenida:
                    break
            if detenida:
                break
    informe = {
        "generado_en": datetime.now(timezone.utc).isoformat(),
        "duracion_min": (time.perf_counter() - inicio) / 60,
        "semilla": argumentos.semilla,
        "modelo": argumentos.modelo,
        "app_commit": run._commit_de_la_app(),
        "binario": {"nombre": argumentos.binario.name, "sha256": run._sha256(argumentos.binario)},
        "base": {"nombre": ruta_base.name, "sha256": run._sha256(ruta_base)},
        "profundidades": profundidades,
        "profundidades_sin_seccion_fiable": sorted(sin_seccion),
        "detenida": detenida,
        "agregados": agregar_por_profundidad(casos),
        "arnes": resumen_arnes(casos),
        "casos": casos,
    }
    ruta_json = argumentos.salida / f"aguja_report_{marca}.json"
    ruta_json.write_text(json.dumps(informe, ensure_ascii=False, indent=1), encoding="utf-8")
    ruta_json.with_suffix(".md").write_text(_markdown(informe), encoding="utf-8")
    print(f"Informe: {ruta_json}\nInforme: {ruta_json.with_suffix('.md')}")
    return run.CODIGO_CON_FALLOS if detenida or _hay_fallos(casos) else run.CODIGO_OK


def principal(argv=None):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    argumentos = _analizar_argumentos(argv)
    try:
        return _ejecutar(argumentos)
    except (run.UsoInvalido, AislamientoInvalido, ValueError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return run.CODIGO_USO


if __name__ == "__main__":
    sys.exit(principal())
