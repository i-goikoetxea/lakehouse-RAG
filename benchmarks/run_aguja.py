"""Pista Aguja (SSD §4.2): un centro por repetición y profundidad, con el PDF base y la aguja insertada.

Sin `--base` usa el documento de `manifiesto_datasets.json` y exige su sha256.
Códigos de salida: 0 sin fallos, 2 uso inválido, 3 con fallos de ejecución.
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
    fragmentos_con_aguja,
    pagina_exacta,
    paginas_citadas,
    recall_aguja,
    seccion_en_respuesta,
    seccion_real,
)
from lakehouse.cli import AislamientoInvalido, ClienteCli, FalloEjecucion, comando_de_binario, valor_admitido  # noqa: E402
from pistas.aguja import PROFUNDIDADES, generar_aguja, insertar_aguja, preguntas_de, texto_de_pdf  # noqa: E402
from pistas.base import Documento, Pista  # noqa: E402

MANIFIESTO = RAIZ_BENCHMARKS / "manifiesto_datasets.json"
CLAVE_DATASET = "aguja_base"
NOMBRE_DOCUMENTO = "documento_aguja.pdf"


def _base_del_manifiesto():
    dataset = json.loads(MANIFIESTO.read_text(encoding="utf-8"))["datasets"][CLAVE_DATASET]
    ruta = RAIZ_BENCHMARKS / dataset["ruta_local"]
    if not ruta.is_file():
        raise run.UsoInvalido(f"Falta el documento base: descárgalo a {ruta} (ver {MANIFIESTO.name})")
    if run._sha256(ruta) != dataset["sha256"]:
        raise run.UsoInvalido(f"El sha256 de {ruta.name} no coincide con el manifiesto")
    return ruta


def _resumen_de_fragmentos(fragmentos, esperada, seccion):
    return {
        "fragmentos": fragmentos,
        "pagina_exacta": pagina_exacta(fragmentos, esperada),
        "seccion_real": seccion_real(fragmentos, seccion),
    }


def _consulta(cliente, mcp, centro_id, modelo, tipo, pregunta, aguja, documento):
    registro = {"tipo": tipo, "pregunta": pregunta, "fallo": None}
    try:
        respuesta = cliente.preguntar(pregunta, centro_id, modelo)
    except FalloEjecucion as fallo:
        return registro | {"fallo": run._fallo_como_dict(fallo)}
    citadas = paginas_citadas(respuesta.texto)
    registro.update(
        respuesta=respuesta.texto,
        latencia_respuesta_ms=respuesta.latencia_ms,
        herramientas=[h.get("nombre") for h in respuesta.herramientas if isinstance(h, dict)],
        recall_aguja=recall_aguja(respuesta.texto, aguja),
        paginas_citadas_respuesta=sorted(citadas),
        pagina_exacta=float(documento.pagina_esperada in citadas),
        seccion_en_respuesta=seccion_en_respuesta(respuesta.texto, documento.seccion_esperada),
        tools_del_agente=_resumen_de_fragmentos(
            fragmentos_con_aguja(respuesta.herramientas, aguja.lote), documento.pagina_esperada, documento.seccion_esperada
        ),
    )
    registro["mcp_directo"] = _recuperacion_directa(mcp, centro_id, pregunta, aguja, documento)
    return registro


def _recuperacion_directa(mcp, centro_id, pregunta, aguja, documento):
    """`search_normative_context` con la pregunta tal cual: mide la recuperación sin el agente."""
    if isinstance(mcp, FalloEjecucion):
        return {"fallo": run._fallo_como_dict(mcp)}
    try:
        resultado = mcp.llamar_herramienta(run.HERRAMIENTA_RECUPERACION, {"centro_id": centro_id, "consulta": pregunta})
    except FalloEjecucion as fallo:
        return {"fallo": run._fallo_como_dict(fallo)}
    fragmentos = fragmentos_con_aguja(resultado.contenido, aguja.lote)
    return _resumen_de_fragmentos(fragmentos, documento.pagina_esperada, documento.seccion_esperada) | {
        "latencia_ms": resultado.latencia_ms
    }


def _ejecutar_caso(comando, pdf_base, texto_base, repeticion, profundidad, argumentos):
    aguja = generar_aguja(argumentos.semilla, repeticion, profundidad, texto_base)
    documento = insertar_aguja(pdf_base, aguja, profundidad, argumentos.primera_del_cuerpo)
    caso = {
        "repeticion": repeticion,
        "profundidad": profundidad,
        "profundidad_real": documento.profundidad_real,
        "pagina_esperada": documento.pagina_esperada,
        "seccion_esperada": documento.seccion_esperada,
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
                caso["consultas"].append(_consulta(cliente, mcp, centro_id, argumentos.modelo, tipo, pregunta, aguja, documento))
        finally:
            if not isinstance(mcp, FalloEjecucion):
                mcp.cerrar()
    return caso


def _celda(valor):
    if valor is None:
        return "n/d"
    return f"{valor:.2f}" if isinstance(valor, float) else str(valor)


def _markdown(informe):
    lineas = [
        "# Pista Aguja",
        "",
        f"Base `{informe['base']['nombre']}` (sha256 `{informe['base']['sha256'][:12]}…`), binario sha256 "
        f"`{informe['binario']['sha256'][:12]}…`, modelo `{informe['modelo']}`, semilla {informe['semilla']}.",
        "",
        "| Rep. | Prof. | Pág. esperada | Sección esperada | Ingesta (s) | Estado | Pregunta | recall | pág. en respuesta | pág. tool agente | sección tool agente | pág. MCP | sección MCP | Latencia (s) |",
        "| -: | -: | -: | :- | -: | :- | :- | -: | -: | -: | -: | -: | -: | -: |",
    ]
    for caso in informe["casos"]:
        ingesta = caso["ingesta"] or {}
        segundos = ingesta.get("latencia_ms")
        cabeza = (
            f"| {caso['repeticion']} | {caso['profundidad']:.2f} | {caso['pagina_esperada']} | {caso['seccion_esperada']} | "
            f"{_celda(segundos / 1000 if segundos else None)} | {ingesta.get('estado_contenido') or ingesta.get('fallo', {}).get('tipo')} |"
        )
        for consulta in caso["consultas"] or [{"tipo": "—"}]:
            agente = consulta.get("tools_del_agente", {})
            directo = consulta.get("mcp_directo", {})
            latencia = consulta.get("latencia_respuesta_ms")
            lineas.append(
                f"{cabeza} {consulta['tipo']} | {_celda(consulta.get('recall_aguja'))} | {_celda(consulta.get('pagina_exacta'))} | "
                f"{_celda(agente.get('pagina_exacta'))} | {_celda(agente.get('seccion_real'))} | "
                f"{_celda(directo.get('pagina_exacta'))} | {_celda(directo.get('seccion_real'))} | "
                f"{_celda(latencia / 1000 if latencia else None)} |"
            )
    return "\n".join(lineas) + "\n"


def _hay_fallos(casos):
    return any(
        (c["ingesta"] or {}).get("fallo") or any(q.get("fallo") for q in c["consultas"]) for c in casos
    )


def _analizar_argumentos(argv):
    analizador = argparse.ArgumentParser(description="Pista Aguja del benchmark de caja negra")
    analizador.add_argument("--binario", required=True, type=Path)
    analizador.add_argument("--base", type=Path, help="PDF base; por defecto, el del manifiesto con su sha256")
    analizador.add_argument("--profundidad", type=float, action="append", help=f"Por defecto {PROFUNDIDADES}")
    analizador.add_argument("--primera-del-cuerpo", type=int, default=1)
    analizador.add_argument("--modelo", default="sonnet")
    analizador.add_argument("--repeticiones", type=int, default=1)
    analizador.add_argument("--semilla", type=int, default=42)
    analizador.add_argument("--salida", type=Path, default=RAIZ_BENCHMARKS / "resultados")
    return analizador.parse_args(argv)


def _ejecutar(argumentos):
    if argumentos.repeticiones < 1:
        raise run.UsoInvalido("--repeticiones debe ser al menos 1")
    if not argumentos.binario.is_file():
        raise run.UsoInvalido(f"No existe el binario: {argumentos.binario}")
    try:
        valor_admitido(argumentos.modelo)
    except FalloEjecucion as fallo:
        raise run.UsoInvalido(f"--modelo: {fallo.mensaje}") from fallo
    ruta_base = argumentos.base or _base_del_manifiesto()
    profundidades = argumentos.profundidad or list(PROFUNDIDADES)
    comando = comando_de_binario(argumentos.binario)
    run._comprobar_contrato(comando, [])
    pdf_base = ruta_base.read_bytes()
    texto_base = texto_de_pdf(pdf_base)
    casos = [
        _ejecutar_caso(comando, pdf_base, texto_base, repeticion, profundidad, argumentos)
        for repeticion in range(1, argumentos.repeticiones + 1)
        for profundidad in profundidades
    ]
    informe = {
        "generado_en": datetime.now(timezone.utc).isoformat(),
        "semilla": argumentos.semilla,
        "modelo": argumentos.modelo,
        "app_commit": run._commit_de_la_app(),
        "binario": {"nombre": argumentos.binario.name, "sha256": run._sha256(argumentos.binario)},
        "base": {"nombre": ruta_base.name, "sha256": run._sha256(ruta_base)},
        "casos": casos,
    }
    marca = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S_%f")
    argumentos.salida.mkdir(parents=True, exist_ok=True)
    ruta_json = argumentos.salida / f"aguja_report_{marca}.json"
    ruta_json.write_text(json.dumps(informe, ensure_ascii=False, indent=1), encoding="utf-8")
    ruta_md = ruta_json.with_suffix(".md")
    ruta_md.write_text(_markdown(informe), encoding="utf-8")
    print(f"Informe: {ruta_json}\nInforme: {ruta_md}")
    return run.CODIGO_CON_FALLOS if _hay_fallos(casos) else run.CODIGO_OK


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
