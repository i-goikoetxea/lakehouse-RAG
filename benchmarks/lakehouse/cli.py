"""Cliente de `lakehouse_rag_cli.exe`: una llamada por subcomando, siempre con `--json` y aislada."""

import json
import os
import sys
from dataclasses import dataclass
from pathlib import Path

from lakehouse.proceso import ejecutar_proceso

VARIABLE_AISLAMIENTO = "LAKEHOUSE_RAG_CONFIG_DIR"
LONGITUD_MAXIMA_MENSAJE = 2000
# La app los busca en cualquier posición: como valor cambiarían de modo el proceso entero
VALORES_RESERVADOS = {"-h", "-m"}
SENALES_SESION_CLAUDE = ("no tiene una sesión válida", "please run /login")


class FalloEjecucion(Exception):
    """Fallo de la ejecución, nunca una respuesta de la app. Se cuenta aparte."""

    def __init__(self, tipo, mensaje):
        self.tipo = tipo
        self.mensaje = mensaje[:LONGITUD_MAXIMA_MENSAJE]
        super().__init__(f"{tipo}: {self.mensaje}")


class AislamientoInvalido(ValueError):
    """La operación podría tocar los centros reales del usuario."""


@dataclass(frozen=True)
class RespuestaAsk:
    texto: str
    modelo: str | None
    herramientas: list
    latencia_ms: float


def comando_de_binario(ruta):
    """Un `.py` (la CLI simulada) necesita su intérprete; un binario se lanza directo."""
    ruta = str(Path(ruta).resolve())
    return [sys.executable, ruta] if ruta.lower().endswith(".py") else [ruta]


def _directorios_reales_de_la_app():
    bases = [os.environ.get("APPDATA"), Path.home() / ".config"]
    return [(Path(base) / "lakehouse_rag").resolve() for base in bases if base]


def validar_directorio_aislado(directorio):
    resuelto = Path(directorio).resolve()
    if not resuelto.is_dir():
        raise AislamientoInvalido(f"El directorio aislado no existe: {resuelto}")
    for real in _directorios_reales_de_la_app():
        if resuelto.is_relative_to(real):
            raise AislamientoInvalido(f"Directorio de configuración real de la app: {resuelto}")
    return resuelto


def entorno_aislado(directorio):
    return dict(os.environ, **{VARIABLE_AISLAMIENTO: str(directorio)})


def es_fallo_de_sesion(texto):
    minusculas = texto.lower()
    return any(senal in minusculas for senal in SENALES_SESION_CLAUDE)


def valor_admitido(valor):
    """Devuelve el valor si la CLI lo leerá como tal y no como una bandera."""
    es_invalido = (
        not valor.strip() or valor.startswith("--") or valor.strip() in VALORES_RESERVADOS or "\0" in valor
    )
    if es_invalido:
        raise FalloEjecucion("argumento_invalido", f"Valor no admitido por la CLI: {valor[:80]!r}")
    return valor


class ClienteCli:
    def __init__(self, comando_base, directorio_aislado, *, timeout_consulta_s=600, timeout_ingesta_s=1800, timeout_gestion_s=300):
        self._comando_base = list(comando_base)
        self._directorio = validar_directorio_aislado(directorio_aislado)
        self._timeout_consulta_s = timeout_consulta_s
        self._timeout_ingesta_s = timeout_ingesta_s
        self._timeout_gestion_s = timeout_gestion_s

    def crear_centro(self, centro_id, ruta):
        ruta_resuelta = Path(ruta).resolve()
        if not ruta_resuelta.is_relative_to(self._directorio):
            raise AislamientoInvalido(f"Centro fuera de la carpeta aislada: {ruta_resuelta}")
        ruta_resuelta.parent.mkdir(parents=True, exist_ok=True)
        return self._invocar(["--crear-centro", valor_admitido(centro_id), str(ruta_resuelta)], self._timeout_gestion_s)[0]

    def activar(self, centro_id):
        return self._invocar(["--activar", valor_admitido(centro_id)], self._timeout_gestion_s)[0]

    def ingestar(self, archivo, centro_id):
        argumentos = ["--ingest", str(Path(archivo).resolve()), "--centro", valor_admitido(centro_id)]
        salida, latencia_ms = self._invocar(argumentos, self._timeout_ingesta_s)
        return salida | {"latencia_ms": latencia_ms}

    def preguntar(self, texto, centro_id, modelo):
        argumentos = ["--ask", valor_admitido(texto), "--centro", valor_admitido(centro_id), "--modelo", valor_admitido(modelo)]
        salida, latencia_ms = self._invocar(argumentos, self._timeout_consulta_s)
        respuesta = salida.get("respuesta")
        if not isinstance(respuesta, str) or not respuesta.strip():
            raise FalloEjecucion("respuesta_vacia", "La app devolvió ok sin texto de respuesta")
        if es_fallo_de_sesion(respuesta):
            raise FalloEjecucion("sesion_claude", respuesta)
        herramientas = salida.get("herramientas")
        return RespuestaAsk(respuesta, salida.get("modelo"), herramientas if isinstance(herramientas, list) else [], latencia_ms)

    def _invocar(self, argumentos, timeout_s):
        comando = [*self._comando_base, *argumentos, "--json"]
        try:
            resultado = ejecutar_proceso(comando, entorno_aislado(self._directorio), self._directorio, timeout_s)
        except (OSError, ValueError) as error:
            raise FalloEjecucion("arranque", str(error)) from error
        if resultado.expirado:
            raise FalloEjecucion("timeout", f"Sin respuesta en {timeout_s} s: {argumentos[0]}")
        return _interpretar_salida(resultado), resultado.latencia_ms


def _interpretar_salida(resultado):
    salida = None
    try:
        salida = json.loads(resultado.stdout)
    except (ValueError, RecursionError):
        pass
    detalle = (salida.get("error") if isinstance(salida, dict) else None) or resultado.stderr or resultado.stdout
    if es_fallo_de_sesion(str(detalle)):
        raise FalloEjecucion("sesion_claude", str(detalle))
    if resultado.codigo != 0:
        raise FalloEjecucion("codigo_salida", f"Código {resultado.codigo}: {detalle}")
    if not isinstance(salida, dict):
        raise FalloEjecucion("salida_no_json", str(detalle))
    if salida.get("ok") is not True:
        raise FalloEjecucion("ok_falso", str(detalle))
    return salida
