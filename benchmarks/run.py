"""Punto de entrada del benchmark de caja negra: solo habla con la app por CLI y MCP.

Códigos de salida: 0 informe sin fallos, 2 uso inválido, 3 informe con fallos de ejecución.
"""

import argparse
import hashlib
import json
import random
import sys
import tempfile
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

RAIZ_BENCHMARKS = Path(__file__).resolve().parent
sys.path.insert(0, str(RAIZ_BENCHMARKS))

from evaluacion.informe import construir_seccion_pista, escribir_informe  # noqa: E402
from lakehouse.cli import (  # noqa: E402
    AislamientoInvalido,
    ClienteCli,
    FalloEjecucion,
    comando_de_binario,
    entorno_aislado,
    valor_admitido,
)
from lakehouse.mcp import ClienteMcp  # noqa: E402
from lakehouse.proceso import ejecutar_proceso  # noqa: E402
from pistas import ciclo_de_vida, humo, rgb_industrial  # noqa: E402

PISTAS = {pista.nombre: pista for pista in (humo.PISTA, ciclo_de_vida.PISTA, rgb_industrial.PISTA)}

BANDERAS_DEL_CONTRATO = ("--mcp-server", "--crear-centro", "--activar", "--ingest", "--centro", "--ask", "--modelo")
MARCA_CLI_SIMULADA = "BENCHMARK_CLI_SIMULADA"
TIMEOUT_AYUDA_S = 30
HERRAMIENTA_RECUPERACION = "search_normative_context"
HERRAMIENTA_CATALOGO = "inspect_lakehouse_catalog"
CODIGO_OK = 0
CODIGO_USO = 2
CODIGO_CON_FALLOS = 3


class UsoInvalido(Exception):
    pass


def _fallo_como_dict(fallo):
    return {"tipo": fallo.tipo, "mensaje": fallo.mensaje}


def _registro_base(consulta, repeticion):
    return {
        "repeticion": repeticion,
        "id": consulta.id,
        "categoria": consulta.categoria,
        "pregunta": consulta.pregunta,
        "estado": "fallo_ejecucion",
        "puntuacion": None,
        "clasificacion": None,
        "dudosa": False,
        "motivos": [],
        "rechazo_semantico": None,
        "fallo": None,
        "latencia_respuesta_ms": None,
        "latencia_recuperacion_ms": None,
        "fallo_recuperacion": None,
        "respuesta": None,
        "herramientas": [],
    }


def _registro_fallido(consulta, repeticion, fallo):
    return _registro_base(consulta, repeticion) | {"fallo": fallo}


def _medir_recuperacion(mcp, centro_id, consulta, registro):
    if isinstance(mcp, FalloEjecucion):
        registro["fallo_recuperacion"] = _fallo_como_dict(mcp)
        return
    try:
        resultado = mcp.llamar_herramienta(HERRAMIENTA_RECUPERACION, {"centro_id": centro_id, "consulta": consulta.pregunta})
        registro["latencia_recuperacion_ms"] = resultado.latencia_ms
    except FalloEjecucion as fallo:
        registro["fallo_recuperacion"] = _fallo_como_dict(fallo)


def _ejecutar_consulta(cliente, mcp, centro_id, consulta, repeticion, modelo):
    registro = _registro_base(consulta, repeticion)
    try:
        respuesta = cliente.preguntar(consulta.pregunta, centro_id, modelo)
    except FalloEjecucion as fallo:
        registro["fallo"] = _fallo_como_dict(fallo)
        return registro
    veredicto = consulta.puntuar(respuesta.texto)
    registro.update(
        estado="puntuada",
        puntuacion=veredicto.puntuacion,
        clasificacion=veredicto.clasificacion,
        dudosa=veredicto.dudosa,
        motivos=list(veredicto.motivos),
        rechazo_semantico=veredicto.rechazo_semantico,
        latencia_respuesta_ms=respuesta.latencia_ms,
        respuesta=respuesta.texto,
        herramientas=respuesta.herramientas,
    )
    _medir_recuperacion(mcp, centro_id, consulta, registro)
    return registro


def _resumen_de_ingesta(documento, repeticion, salida):
    return {
        "repeticion": repeticion,
        "documento": documento.nombre_archivo,
        "doc_id": salida.get("doc_id"),
        "estado_contenido": salida.get("estado_contenido"),
        "version": salida.get("version"),
        "pendiente_destilacion": bool(salida.get("pendiente_destilacion")),
        "avisos": salida.get("avisos", []),
        "latencia_ms": salida.get("latencia_ms"),
    }


def _bytes_de(documento):
    """Un PDF viaja tal cual; un Markdown, en UTF-8."""
    contenido = documento.contenido
    return contenido if isinstance(contenido, bytes) else contenido.encode("utf-8")


def _preparar_centro(cliente, pista, raiz, centro_id, repeticion):
    cliente.crear_centro(centro_id, raiz / "centros" / centro_id)
    cliente.activar(centro_id)
    documentos = raiz / "documentos" / centro_id
    documentos.mkdir(parents=True)
    ingestas = []
    for documento in pista.documentos:
        ruta = documentos / documento.nombre_archivo
        ruta.write_bytes(_bytes_de(documento))
        ingestas.append(_resumen_de_ingesta(documento, repeticion, cliente.ingestar(ruta, centro_id)))
    return ingestas


def _abrir_mcp(comando, raiz):
    """Si no arranca, la recuperación se da por no medida sin invalidar las respuestas."""
    mcp = ClienteMcp(comando, raiz)
    try:
        mcp.iniciar()
    except FalloEjecucion as fallo:
        mcp.cerrar()
        return fallo
    return mcp


def _foto_del_catalogo(mcp, centro_id, repeticion):
    """Lo que la app entendió de cada documento: revisión, vigencia y fecha. Solo diagnostica."""
    if isinstance(mcp, FalloEjecucion):
        return {"repeticion": repeticion, "fallo": _fallo_como_dict(mcp)}
    try:
        resultado = mcp.llamar_herramienta(HERRAMIENTA_CATALOGO, {"centro_id": centro_id})
    except FalloEjecucion as fallo:
        return {"repeticion": repeticion, "fallo": _fallo_como_dict(fallo)}
    return {"repeticion": repeticion, "contenido": resultado.contenido}


def _consultas_de_repeticion(pista, semilla, repeticion, max_por_categoria):
    consultas = list(pista.consultas)
    random.Random(f"{semilla}:{pista.nombre}:{repeticion}").shuffle(consultas)
    if max_por_categoria is None:
        return consultas
    vistas = Counter()
    elegidas = []
    for consulta in consultas:
        vistas[consulta.categoria] += 1
        if vistas[consulta.categoria] <= max_por_categoria:
            elegidas.append(consulta)
    return elegidas


def _ejecutar_repeticion(comando, pista, repeticion, modelo, semilla, max_por_categoria, al_registrar):
    with tempfile.TemporaryDirectory(prefix="benchmark_", ignore_cleanup_errors=True) as temporal:
        raiz = Path(temporal)
        cliente = ClienteCli(comando, raiz)
        centro_id = f"bench-{pista.nombre}-r{repeticion}"
        consultas = _consultas_de_repeticion(pista, semilla, repeticion, max_por_categoria)
        try:
            ingestas = _preparar_centro(cliente, pista, raiz, centro_id, repeticion)
        except FalloEjecucion as fallo:
            detalle = {"tipo": "preparacion", "mensaje": f"{fallo.tipo}: {fallo.mensaje}"}
            registros = [_registro_fallido(consulta, repeticion, detalle) for consulta in consultas]
            for registro in registros:
                al_registrar(registro)
            return registros, [], None
        mcp = _abrir_mcp(comando, raiz)
        try:
            catalogo = _foto_del_catalogo(mcp, centro_id, repeticion)
            registros = []
            for consulta in consultas:
                registro = _ejecutar_consulta(cliente, mcp, centro_id, consulta, repeticion, modelo)
                al_registrar(registro)
                registros.append(registro)
            return registros, ingestas, catalogo
        finally:
            if not isinstance(mcp, FalloEjecucion):
                mcp.cerrar()


def _sha256(ruta):
    resumen = hashlib.sha256()
    with open(ruta, "rb") as archivo:
        for bloque in iter(lambda: archivo.read(1024 * 1024), b""):
            resumen.update(bloque)
    return resumen.hexdigest()


def _sha256_de_documentos(pista):
    resumen = hashlib.sha256()
    for documento in pista.documentos:
        resumen.update(documento.nombre_archivo.encode("utf-8") + b"\0" + _bytes_de(documento) + b"\0")
    return resumen.hexdigest()


def _commit_de_la_app():
    """Commit del checkout; el binario probado se identifica por su hash."""
    comando = ["git", "-C", str(RAIZ_BENCHMARKS), "rev-parse", "HEAD"]
    try:
        resultado = ejecutar_proceso(comando, None, None, TIMEOUT_AYUDA_S)
    except OSError:
        return "desconocido"
    return resultado.stdout.strip() if resultado.codigo == 0 else "desconocido"


def _comprobar_contrato(comando, pistas):
    """`--help` es gratis: confirma el contrato antes de lanzar nada que cueste dinero o abra la GUI."""
    with tempfile.TemporaryDirectory(prefix="benchmark_", ignore_cleanup_errors=True) as temporal:
        try:
            ayuda = ejecutar_proceso([*comando, "--help"], entorno_aislado(temporal), temporal, TIMEOUT_AYUDA_S)
        except OSError as error:
            raise UsoInvalido(f"No se pudo lanzar el binario: {error}") from error
    if ayuda.expirado or ayuda.codigo != 0:
        raise UsoInvalido("El binario no respondió a --help")
    faltan = [bandera for bandera in BANDERAS_DEL_CONTRATO if bandera not in ayuda.stdout]
    if faltan:
        raise UsoInvalido(f"El binario no cumple el contrato, faltan: {', '.join(faltan)}")
    if MARCA_CLI_SIMULADA not in ayuda.stdout:
        for pista in pistas:
            if pista.solo_cli_simulada:
                raise UsoInvalido(f"La pista '{pista.nombre}' solo corre contra la CLI simulada")


def _analizar_argumentos(argv):
    analizador = argparse.ArgumentParser(description="Benchmark de caja negra de Lakehouse RAG")
    analizador.add_argument("--binario", required=True, type=Path)
    analizador.add_argument("--pista", required=True, action="append", choices=sorted(PISTAS))
    analizador.add_argument("--modelo", default="sonnet")
    analizador.add_argument("--repeticiones", type=int, default=3)
    analizador.add_argument("--semilla", type=int, default=42)
    analizador.add_argument("--max-consultas-por-categoria", type=int, help="Piloto para medir coste: las ingestas no se reducen")
    analizador.add_argument("--salida", type=Path, default=RAIZ_BENCHMARKS / "resultados")
    return analizador.parse_args(argv)


def _ejecutar(argumentos):
    if argumentos.repeticiones < 1:
        raise UsoInvalido("--repeticiones debe ser al menos 1")
    if argumentos.max_consultas_por_categoria is not None and argumentos.max_consultas_por_categoria < 1:
        raise UsoInvalido("--max-consultas-por-categoria debe ser al menos 1")
    if not argumentos.binario.is_file():
        raise UsoInvalido(f"No existe el binario: {argumentos.binario}")
    try:
        valor_admitido(argumentos.modelo)
    except FalloEjecucion as fallo:
        raise UsoInvalido(f"--modelo: {fallo.mensaje}") from fallo
    pistas = [PISTAS[nombre] for nombre in dict.fromkeys(argumentos.pista)]
    comando = comando_de_binario(argumentos.binario)
    _comprobar_contrato(comando, pistas)

    marca_temporal = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S_%f")
    argumentos.salida.mkdir(parents=True, exist_ok=True)
    app_commit = _commit_de_la_app()
    secciones = []
    # Cada respuesta se guarda al llegar: un corte no tira lo ya pagado
    with open(argumentos.salida / f"benchmark_crudas_{marca_temporal}.jsonl", "w", encoding="utf-8") as crudas:
        def al_registrar(registro):
            crudas.write(json.dumps(registro, ensure_ascii=False) + "\n")
            crudas.flush()

        for pista in pistas:
            registros, ingestas, catalogos = [], [], []
            for repeticion in range(1, argumentos.repeticiones + 1):
                de_repeticion, ingestas_de_repeticion, catalogo = _ejecutar_repeticion(
                    comando, pista, repeticion, argumentos.modelo, argumentos.semilla,
                    argumentos.max_consultas_por_categoria, al_registrar,
                )
                registros += de_repeticion
                ingestas += ingestas_de_repeticion
                if catalogo is not None:
                    catalogos.append(catalogo)
            secciones.append(
                construir_seccion_pista(
                    benchmark=pista.benchmark,
                    comparable_con_articulo=pista.comparable_con_articulo,
                    app_commit=app_commit,
                    modelo=argumentos.modelo,
                    repeticiones=argumentos.repeticiones,
                    registros=registros,
                    ingestas=ingestas,
                    catalogos=catalogos,
                )
            )
    informe = {
        "generado_en": datetime.now(timezone.utc).isoformat(),
        "semilla": argumentos.semilla,
        "binario": {"nombre": argumentos.binario.name, "sha256": _sha256(argumentos.binario)},
        "datasets_sha256": {pista.nombre: _sha256_de_documentos(pista) for pista in pistas},
        "pistas": secciones,
    }
    ruta_json, ruta_md = escribir_informe(argumentos.salida, informe, marca_temporal)
    print(f"Informe: {ruta_json}\nInforme: {ruta_md}")
    return CODIGO_CON_FALLOS if any(s["fallos_de_ejecucion"] for s in secciones) else CODIGO_OK


def principal(argv=None):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    argumentos = _analizar_argumentos(argv)
    try:
        return _ejecutar(argumentos)
    except (UsoInvalido, AislamientoInvalido) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return CODIGO_USO


if __name__ == "__main__":
    sys.exit(principal())
