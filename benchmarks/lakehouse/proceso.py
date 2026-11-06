"""Lanzamiento de procesos sin shell, con timeout que mata el árbol completo.

La salida va a archivos y no a tuberías: la app lanza a Claude como nieto, y un nieto vivo que
hereda una tubería dejaría colgado a `subprocess.run` en Windows incluso tras matar al hijo.
"""

import os
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass

ES_WINDOWS = sys.platform == "win32"
BANDERAS_DE_CREACION = subprocess.CREATE_NO_WINDOW if ES_WINDOWS else 0
ESPERA_TRAS_MATAR_S = 10


@dataclass(frozen=True)
class ResultadoProceso:
    codigo: int | None
    stdout: str
    stderr: str
    expirado: bool
    latencia_ms: float


def matar_arbol(proceso):
    if proceso.poll() is not None:
        return
    if ES_WINDOWS:
        subprocess.run(
            ["taskkill", "/T", "/F", "/PID", str(proceso.pid)],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=BANDERAS_DE_CREACION,
            check=False,
        )
    else:
        try:
            os.killpg(proceso.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    try:
        proceso.wait(ESPERA_TRAS_MATAR_S)
    except subprocess.TimeoutExpired:
        proceso.kill()


def _leer(archivo):
    archivo.seek(0)
    return archivo.read().decode("utf-8", errors="replace")


def ejecutar_proceso(comando, entorno, directorio, timeout_s):
    """Lanza `comando` (lista, sin shell). Propaga OSError y ValueError si no puede arrancar."""
    with tempfile.TemporaryFile() as salida, tempfile.TemporaryFile() as errores:
        inicio = time.perf_counter()
        proceso = subprocess.Popen(
            comando,
            stdin=subprocess.DEVNULL,
            stdout=salida,
            stderr=errores,
            env=entorno,
            cwd=directorio,
            creationflags=BANDERAS_DE_CREACION,
            start_new_session=not ES_WINDOWS,
        )
        expirado = False
        try:
            proceso.wait(timeout_s)
        except subprocess.TimeoutExpired:
            expirado = True
            matar_arbol(proceso)
        latencia_ms = (time.perf_counter() - inicio) * 1000
        return ResultadoProceso(proceso.returncode, _leer(salida), _leer(errores), expirado, latencia_ms)
