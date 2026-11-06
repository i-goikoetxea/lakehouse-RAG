"""Ciclo de vida: se ingiere la Rev. 00 y después la Rev. 01. Inspirada en CRUD, no comparable con el artículo.

Lo histórico se pregunta de dos formas, por fecha y por número de revisión, y cada una tiene su métrica.
"""

from evaluacion.puntuar import ValorEsperado
from pistas.base import Consulta, Pista, cargar_documentos

REV_00 = ValorEsperado.revision(0)
REV_01 = ValorEsperado.revision(1)

PRESION_00 = ValorEsperado("6,5", "bar")
PRESION_01 = ValorEsperado("8,2", "bar")
TEMPERATURA_00 = ValorEsperado("72", "°C")
TEMPERATURA_01 = ValorEsperado("85", "°C")
DURACION_00 = ValorEsperado("27", "min")
DURACION_01 = ValorEsperado("36", "min")


def _vigente(identificador, pregunta, actual, derogado):
    return Consulta(identificador, "vigente", pregunta, esperados=(actual, REV_01), prohibidos=(derogado,))


def _historico(identificador, categoria, pregunta, antiguo, posterior):
    return Consulta(identificador, categoria, pregunta, esperados=(antiguo, REV_00), prohibidos=(posterior,))


PISTA = Pista(
    nombre="ciclo-de-vida",
    benchmark="Ciclo-de-vida",
    comparable_con_articulo=False,
    documentos=cargar_documentos("ciclo_de_vida", ("purga_rev00.md", "purga_rev01.md")),
    consultas=(
        _vigente(
            "vigente-presion",
            "¿Cuál es actualmente la presión de purga del procedimiento PTF-PUR-05? Indica la revisión.",
            PRESION_01,
            PRESION_00,
        ),
        _vigente(
            "vigente-temperatura",
            "¿Qué temperatura del agua de purga está vigente hoy según el procedimiento PTF-PUR-05? Indica la revisión.",
            TEMPERATURA_01,
            TEMPERATURA_00,
        ),
        _vigente(
            "vigente-duracion-por-fecha",
            "¿Qué duración de la purga exigía el procedimiento PTF-PUR-05 el 14 de abril de 2026? Indica la revisión.",
            DURACION_01,
            DURACION_00,
        ),
        _historico(
            "fecha-presion",
            "historico_fecha",
            "¿Qué presión de purga exigía el procedimiento PTF-PUR-05 el 16 de junio de 2025? Indica la revisión.",
            PRESION_00,
            PRESION_01,
        ),
        _historico(
            "fecha-temperatura",
            "historico_fecha",
            "¿Cuál era la temperatura del agua de purga del procedimiento PTF-PUR-05 el 11 de septiembre de 2025? Indica la revisión.",
            TEMPERATURA_00,
            TEMPERATURA_01,
        ),
        _historico(
            "fecha-duracion",
            "historico_fecha",
            "¿Qué duración de la purga estaba en vigor en el procedimiento PTF-PUR-05 el 18 de noviembre de 2025? Indica la revisión.",
            DURACION_00,
            DURACION_01,
        ),
        _historico(
            "revision-presion",
            "historico_revision",
            "¿Qué presión de purga fijaba la Rev. 00 del procedimiento PTF-PUR-05?",
            PRESION_00,
            PRESION_01,
        ),
        _historico(
            "revision-temperatura",
            "historico_revision",
            "Según la Rev. 00 del procedimiento PTF-PUR-05, ¿cuál es la temperatura del agua de purga?",
            TEMPERATURA_00,
            TEMPERATURA_01,
        ),
        _historico(
            "revision-duracion",
            "historico_revision",
            "En la Rev. 00 del procedimiento PTF-PUR-05, ¿cuánto duraba la purga?",
            DURACION_00,
            DURACION_01,
        ),
    ),
)
