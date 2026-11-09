"""Doble de `lakehouse_rag_cli.exe`: mismo contrato de argumentos, salida y MCP, sin Claude ni disco real.

`CLI_SIMULADA_MODO` activa cada avería. Es un script independiente: no importa nada de `benchmarks`.
"""

import json
import os
import subprocess
import sys
import time
from pathlib import Path

MODO = os.environ.get("CLI_SIMULADA_MODO", "normal")

AYUDA = """BENCHMARK_CLI_SIMULADA
--mcp-server --crear-centro <id> <ruta> --activar <id> --ingest <archivo> --centro <id>
--ask "<texto>" --centro <id> --modelo <alias> --json
"""

RESPUESTAS = {
    "L-7": "La presión máxima de la línea L-7 es **0,42** MPa (equivale a 4,2 bar).",
    "ZX-999": "No consta en la documentación ningún valor para la norma ZX-999.",
    "T-3": "El par de apriete del tramo T-3 es 85 N, solo con la junta seca.",
}


def imprimir_json(contenido):
    print(json.dumps(contenido, ensure_ascii=False))


def fallar(mensaje, codigo=1):
    imprimir_json({"ok": False, "error": mensaje})
    print(f"ERROR: {mensaje}", file=sys.stderr)
    sys.exit(codigo)


def valor_de(argumentos, bandera):
    posicion = argumentos.index(bandera) + 1
    if posicion >= len(argumentos) or argumentos[posicion].startswith("--"):
        fallar(f"{bandera} <valor>", 2)
    return argumentos[posicion]


def registro_de_centros():
    return Path(os.environ["LAKEHOUSE_RAG_CONFIG_DIR"]) / "centers_registry.json"


def centros_registrados():
    ruta = registro_de_centros()
    return json.loads(ruta.read_text(encoding="utf-8")) if ruta.exists() else {}


def guardar_centros(centros):
    registro_de_centros().write_text(json.dumps(centros), encoding="utf-8")


def centro_existente(centro_id):
    centros = centros_registrados()
    if centro_id not in centros:
        fallar(f"Centro no encontrado: {centro_id}")
    return centros


def crear_centro(argumentos):
    posicion = argumentos.index("--crear-centro")
    centro_id, ruta = argumentos[posicion + 1], argumentos[posicion + 2]
    centros = centros_registrados()
    centros[centro_id] = {"ruta": ruta, "activo": False, "documentos": 0}
    guardar_centros(centros)
    imprimir_json({"ok": True, "centro": {"id": centro_id, "ruta_disco": ruta, "activo": False}})


def activar(argumentos):
    centro_id = valor_de(argumentos, "--activar")
    centros = centro_existente(centro_id)
    centros[centro_id]["activo"] = True
    guardar_centros(centros)
    imprimir_json({"ok": True, "proyeccion": {"centro_id": centro_id, "vistas": []}})


def ingestar(argumentos):
    archivo = valor_de(argumentos, "--ingest")
    centro_id = valor_de(argumentos, "--centro")
    centros = centro_existente(centro_id)
    if MODO == "ingesta_falla":
        fallar("Destilación rechazada")
    if not Path(archivo).is_file():
        fallar(f"No existe {archivo}")
    centros[centro_id]["documentos"] += 1
    guardar_centros(centros)
    provisional = MODO == "ingesta_provisional"
    imprimir_json(
        {
            "ok": True,
            "doc_id": Path(archivo).stem,
            "centro_id": centro_id,
            "version": 1,
            "ya_ingerido": False,
            "estado_contenido": "PARCIAL_UNIVERSAL" if provisional else "COMPLETO",
            "pendiente_destilacion": provisional,
            "avisos": [],
            "sha256": "0" * 64,
            "totales": {"chunks": 1},
        }
    )


def colgarse_con_nieto():
    """El hijo espera a un nieto que hereda stdout, como la app con el proceso de Claude."""
    nieto = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
    nieto.wait()


def respuestas_inyectadas():
    ruta = os.environ.get("CLI_SIMULADA_RESPUESTAS")
    return json.loads(Path(ruta).read_text(encoding="utf-8")) if ruta else {}


def respuesta_a(texto):
    inyectadas = respuestas_inyectadas()
    if texto in inyectadas:
        return inyectadas[texto]
    for clave, respuesta in RESPUESTAS.items():
        if clave in texto:
            if MODO == "alucina" and clave == "ZX-999":
                return "No consta con certeza, pero el valor habitual es 7,5 bar."
            return respuesta
    return "No consta en la documentación."


def preguntar(argumentos):
    texto = valor_de(argumentos, "--ask")
    centro_id = valor_de(argumentos, "--centro")
    modelo = valor_de(argumentos, "--modelo") if "--modelo" in argumentos else "sonnet"
    centro_existente(centro_id)
    if MODO == "cuelga":
        time.sleep(60)
    if MODO == "nieto_colgado":
        colgarse_con_nieto()
    if MODO == "sesion_caducada":
        fallar("Claude Code no tiene una sesión válida: no has iniciado sesión o la sesión ha caducado.")
    if MODO == "json_roto":
        print("{no es json")
        return
    if MODO == "ok_falso_codigo_cero":
        imprimir_json({"ok": False, "error": "fallo silencioso"})
        return
    if MODO == "muere_con_ok":
        imprimir_json({"ok": True, "respuesta": "0,42 MPa", "herramientas": []})
        sys.exit(1)
    if MODO == "respuesta_vacia":
        imprimir_json({"ok": True, "centro_id": centro_id, "modelo": modelo, "respuesta": "  ", "herramientas": []})
        return
    if MODO == "login_en_respuesta":
        respuesta = "Not logged in · Please run /login"
    else:
        respuesta = respuesta_a(texto)
    imprimir_json(
        {
            "ok": True,
            "centro_id": centro_id,
            "modelo": modelo,
            "respuesta": respuesta,
            "herramientas": [{"nombre": "search_normative_context", "argumentos": {}, "es_error": False, "resultado": []}],
            "arnes": [{"etapa": "reintento", "motivo": "cifra sin anclar"}] if MODO == "arnes_reintento" else [],
        }
    )


def responder_mcp(respuesta):
    sys.stdout.write(json.dumps(respuesta) + "\n")
    sys.stdout.flush()


def resultado_herramienta(peticion):
    if MODO == "mcp_error_herramienta":
        return {"content": [{"type": "text", "text": "Error ejecutando herramienta: centro inactivo"}], "isError": True}
    return {"content": [{"type": "text", "text": json.dumps({"resultados": [{"texto": "fragmento"}]})}], "isError": False}


def servir_mcp():
    for linea in sys.stdin:
        if not linea.strip():
            continue
        peticion = json.loads(linea)
        if "id" not in peticion:
            continue
        metodo, identificador = peticion["method"], peticion["id"]
        if metodo == "initialize":
            resultado = {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "simulada"}}
        elif metodo == "tools/list":
            resultado = {"tools": [{"name": "search_normative_context", "inputSchema": {}}]}
        elif metodo == "tools/call":
            if MODO == "mcp_cuelga":
                time.sleep(60)
            if MODO == "mcp_muere":
                sys.exit(1)
            if MODO == "mcp_basura":
                print("log suelto en stdout")
                responder_mcp({"jsonrpc": "2.0", "id": identificador + 1000, "result": {}})
            if MODO == "mcp_protocolo":
                responder_mcp({"jsonrpc": "2.0", "id": identificador, "error": {"code": -32602, "message": "Invalid params"}})
                continue
            resultado = resultado_herramienta(peticion)
        else:
            responder_mcp({"jsonrpc": "2.0", "id": identificador, "error": {"code": -32601, "message": "Method not found"}})
            continue
        responder_mcp({"jsonrpc": "2.0", "id": identificador, "result": resultado})


def principal():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    argumentos = sys.argv[1:]
    if "--help" in argumentos or "-h" in argumentos:
        print(AYUDA)
        return
    if "LAKEHOUSE_RAG_CONFIG_DIR" not in os.environ:
        fallar("Sin aislamiento: LAKEHOUSE_RAG_CONFIG_DIR ausente", 99)
    if "--mcp-server" in argumentos or "-m" in argumentos:
        servir_mcp()
    elif "--crear-centro" in argumentos:
        crear_centro(argumentos)
    elif "--activar" in argumentos:
        activar(argumentos)
    elif "--ingest" in argumentos:
        ingestar(argumentos)
    elif "--ask" in argumentos:
        preguntar(argumentos)
    else:
        fallar("Sin subcomando: la app real abriría la GUI", 98)


if __name__ == "__main__":
    principal()
