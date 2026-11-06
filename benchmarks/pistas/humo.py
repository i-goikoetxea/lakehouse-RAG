"""Pista mínima e inventada que ejercita el runner de punta a punta contra la CLI simulada."""

from evaluacion.puntuar import ValorEsperado
from pistas.base import Consulta, Documento, Pista

PISTA = Pista(
    nombre="humo",
    benchmark="Humo-CLI-Simulada",
    comparable_con_articulo=False,
    solo_cli_simulada=True,
    documentos=(
        Documento(
            "procedimiento_ficticio.md",
            "# Procedimiento ficticio\n\n"
            "La presión máxima de la línea L-7 es 0,42 MPa.\n\n"
            "El par de apriete del tramo T-3 es 85 N.\n\n"
            "Ese par solo se aplica con la junta seca.\n",
        ),
    ),
    consultas=(
        Consulta(
            "ruido-presion",
            "ruido",
            "¿Cuál es la presión máxima de la línea L-7?",
            esperados=(ValorEsperado("0,42", "MPa"),),
            prohibidos=(ValorEsperado("0,55", "MPa"),),
        ),
        Consulta("rechazo-norma", "rechazo", "¿Qué presión fija la norma ZX-999?"),
        Consulta(
            "integracion-par",
            "integracion",
            "¿Qué par de apriete lleva el tramo T-3 y con qué condición?",
            esperados=(ValorEsperado("85", "N"), ValorEsperado(texto="junta seca")),
        ),
    ),
)
