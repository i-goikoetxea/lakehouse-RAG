"""Cliente MCP por stdio de `--mcp-server`: JSON-RPC 2.0, un mensaje por línea, sin shell."""

import json
import queue
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass

from lakehouse.cli import FalloEjecucion, entorno_aislado, validar_directorio_aislado
from lakehouse.proceso import BANDERAS_DE_CREACION, ES_WINDOWS, matar_arbol

VERSION_PROTOCOLO = "2024-11-05"
LINEAS_DE_RUIDO_MAXIMAS = 1000
ESPERA_DE_CIERRE_S = 5


@dataclass(frozen=True)
class ResultadoHerramienta:
    contenido: object
    latencia_ms: float


def _bombear_lineas(salida, cola):
    for linea in salida:
        cola.put(linea)
    cola.put(None)


def _interpretar_contenido(resultado):
    """El servidor envuelve la salida en texto; se devuelve como JSON si lo es."""
    bloques = resultado.get("content") if isinstance(resultado, dict) else None
    if not isinstance(bloques, list) or not bloques or not isinstance(bloques[0], dict):
        raise FalloEjecucion("mcp_error", "Resultado de tools/call sin content")
    texto = str(bloques[0].get("text", ""))
    if resultado.get("isError") is not False:
        raise FalloEjecucion("herramienta_error", texto)
    try:
        return json.loads(texto)
    except (ValueError, RecursionError):
        return texto


class ClienteMcp:
    def __init__(self, comando_base, directorio_aislado, *, timeout_inicio_s=300, timeout_llamada_s=120):
        self._comando = [*comando_base, "--mcp-server"]
        self._directorio = validar_directorio_aislado(directorio_aislado)
        self._timeout_inicio_s = timeout_inicio_s
        self._timeout_llamada_s = timeout_llamada_s
        self._proceso = None
        self._errores = None
        self._lineas = queue.Queue()
        self._siguiente_id = 0

    def iniciar(self):
        self._errores = tempfile.TemporaryFile()
        try:
            self._proceso = subprocess.Popen(
                self._comando,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=self._errores,
                env=entorno_aislado(self._directorio),
                cwd=self._directorio,
                creationflags=BANDERAS_DE_CREACION,
                start_new_session=not ES_WINDOWS,
            )
        except (OSError, ValueError) as error:
            self.cerrar()
            raise FalloEjecucion("arranque", str(error)) from error
        threading.Thread(target=_bombear_lineas, args=(self._proceso.stdout, self._lineas), daemon=True).start()
        parametros = {
            "protocolVersion": VERSION_PROTOCOLO,
            "capabilities": {},
            "clientInfo": {"name": "benchmark-runner", "version": "1"},
        }
        self._llamar("initialize", parametros, self._timeout_inicio_s)
        self._enviar({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})

    def listar_herramientas(self):
        resultado = self._llamar("tools/list", {}, self._timeout_llamada_s)
        return [herramienta.get("name") for herramienta in resultado.get("tools", [])]

    def llamar_herramienta(self, nombre, argumentos):
        inicio = time.perf_counter()
        resultado = self._llamar("tools/call", {"name": nombre, "arguments": argumentos}, self._timeout_llamada_s)
        latencia_ms = (time.perf_counter() - inicio) * 1000
        return ResultadoHerramienta(_interpretar_contenido(resultado), latencia_ms)

    def cerrar(self):
        proceso, self._proceso = self._proceso, None
        if proceso is not None:
            self._cerrar_proceso(proceso)
        errores, self._errores = self._errores, None
        if errores is not None:
            errores.close()

    def _cerrar_proceso(self, proceso):
        try:
            proceso.stdin.close()
            proceso.wait(ESPERA_DE_CIERRE_S)
        except (OSError, subprocess.TimeoutExpired):
            pass
        matar_arbol(proceso)

    def _enviar(self, mensaje):
        if self._proceso is None:
            raise FalloEjecucion("mcp_cerrado", "Servidor MCP no iniciado")
        try:
            self._proceso.stdin.write(json.dumps(mensaje).encode("utf-8") + b"\n")
            self._proceso.stdin.flush()
        except (OSError, ValueError) as error:
            raise FalloEjecucion("mcp_cerrado", str(error)) from error

    def _llamar(self, metodo, parametros, timeout_s):
        self._siguiente_id += 1
        identificador = self._siguiente_id
        self._enviar({"jsonrpc": "2.0", "id": identificador, "method": metodo, "params": parametros})
        try:
            respuesta = self._esperar_respuesta(identificador, metodo, timeout_s)
        except FalloEjecucion as fallo:
            # Un servidor colgado no vuelve a responder: las siguientes llamadas fallan al instante
            if fallo.tipo == "timeout":
                self.cerrar()
            raise
        if respuesta.get("error"):
            raise FalloEjecucion("mcp_error", str(respuesta["error"]))
        return respuesta.get("result")

    def _esperar_respuesta(self, identificador, metodo, timeout_s):
        limite = time.monotonic() + timeout_s
        lineas_de_ruido = 0
        while lineas_de_ruido <= LINEAS_DE_RUIDO_MAXIMAS:
            linea = self._siguiente_linea(limite, metodo, timeout_s)
            mensaje = self._mensaje_de(linea)
            if isinstance(mensaje, dict) and mensaje.get("id") == identificador:
                return mensaje
            if isinstance(mensaje, dict) and mensaje.get("id", 0) is None and mensaje.get("error"):
                raise FalloEjecucion("mcp_error", str(mensaje["error"]))
            lineas_de_ruido += 1
        raise FalloEjecucion("mcp_error", f"Demasiado ruido en stdout esperando {metodo}")

    def _siguiente_linea(self, limite, metodo, timeout_s):
        try:
            linea = self._lineas.get(timeout=max(0.0, limite - time.monotonic()))
        except queue.Empty as error:
            raise FalloEjecucion("timeout", f"Sin respuesta MCP a {metodo} en {timeout_s} s") from error
        if linea is None:
            raise FalloEjecucion("mcp_cerrado", f"El servidor terminó esperando {metodo}: {self._cola_de_errores()}")
        return linea

    @staticmethod
    def _mensaje_de(linea):
        try:
            return json.loads(linea)
        except (ValueError, RecursionError):
            return None

    def _cola_de_errores(self):
        if self._errores is None:
            return ""
        self._errores.seek(0, 2)
        self._errores.seek(max(0, self._errores.tell() - 1000))
        return self._errores.read().decode("utf-8", errors="replace")
