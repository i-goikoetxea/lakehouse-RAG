"""Definición declarativa de una pista: documentos que ingerir y consultas que puntuar."""

from dataclasses import dataclass
from pathlib import Path

from evaluacion.puntuar import puntuar_rechazo, puntuar_valores

CARPETA_FIXTURES = Path(__file__).resolve().parent.parent / "fixtures"


@dataclass(frozen=True)
class Documento:
    nombre_archivo: str
    contenido: str


def cargar_documentos(carpeta, nombres):
    """Fixtures estáticos, en el orden de ingesta que se indica."""
    return tuple(
        Documento(nombre, (CARPETA_FIXTURES / carpeta / nombre).read_text(encoding="utf-8")) for nombre in nombres
    )


@dataclass(frozen=True)
class Consulta:
    id: str
    categoria: str
    pregunta: str
    esperados: tuple = ()
    prohibidos: tuple = ()

    def puntuar(self, respuesta):
        if self.categoria == "rechazo":
            return puntuar_rechazo(self.pregunta, respuesta)
        return puntuar_valores(respuesta, self.esperados, self.prohibidos, pide_vigente=self.categoria == "vigente")


@dataclass(frozen=True)
class Pista:
    nombre: str
    benchmark: str
    comparable_con_articulo: bool
    documentos: tuple
    consultas: tuple
    # Una pista de humo no mide nada: se niega a gastar Claude sobre un binario real
    solo_cli_simulada: bool = False
