"""RGB industrial: adapta la metodología de RGB con fixtures propios. Nunca se mezcla con la pista pública."""

from evaluacion.puntuar import ValorEsperado
from pistas.base import Consulta, Pista, cargar_documentos


def _ruido(identificador, pregunta, esperado, valores_de_distractores, unidad):
    prohibidos = tuple(ValorEsperado(valor, unidad) for valor in valores_de_distractores)
    return Consulta(identificador, "ruido", pregunta, esperados=(ValorEsperado(esperado, unidad),), prohibidos=prohibidos)


def _integracion(identificador, pregunta, valor, requisito):
    return Consulta(identificador, "integracion", pregunta, esperados=(valor, requisito))


def _tabular(identificador, pregunta, celda, vecinas):
    prohibidos = tuple(ValorEsperado(valor, "°C") for valor in vecinas)
    return Consulta(identificador, "tabular", pregunta, esperados=(ValorEsperado(celda, "°C"),), prohibidos=prohibidos)


PISTA = Pista(
    nombre="rgb-industrial",
    benchmark="RGB-Industrial",
    comparable_con_articulo=False,
    documentos=cargar_documentos(
        "rgb_industrial",
        (
            "ruido_objetivo.md",
            "distractor_1.md",
            "distractor_2.md",
            "distractor_3.md",
            "distractor_4.md",
            "distractor_5.md",
            "integracion_tabular.md",
        ),
    ),
    consultas=(
        _ruido(
            "ruido-presion",
            "¿Cuál es la presión de prueba hidrostática de la línea de condensado L-104?",
            "12,5",
            ("14,5", "16", "10,5", "18", "9,5"),
            "bar",
        ),
        _ruido(
            "ruido-tiempo",
            "¿Cuánto tiempo se mantiene la presión en la prueba hidrostática de la línea L-104?",
            "45",
            ("30", "60", "90", "25", "120"),
            "min",
        ),
        _ruido(
            "ruido-temperatura",
            "¿Cuál es la temperatura máxima del agua de prueba de la línea L-104?",
            "38",
            ("42", "35", "40", "33", "44"),
            "°C",
        ),
        _ruido(
            "ruido-espesor",
            "¿Qué espesor mínimo debe tener la junta de brida en la prueba de la línea L-104?",
            "3,2",
            ("2,5", "4,0", "3,6", "5,0", "2,8"),
            "mm",
        ),
        _ruido(
            "ruido-perdida",
            "¿Qué pérdida de presión se admite al final de la prueba hidrostática de la línea L-104?",
            "1,5",
            ("2,0", "0,5", "1,0", "2,5", "3,0"),
            "%",
        ),
        Consulta("rechazo-norma", "rechazo", "¿Qué presión de prueba exige la norma ZX-9931 para las líneas de condensado?"),
        Consulta("rechazo-linea", "rechazo", "¿Cuál es la presión de prueba hidrostática de la línea de condensado L-999?"),
        Consulta("rechazo-procedimiento", "rechazo", "¿Qué tiempo de mantenimiento fija el procedimiento PTF-HID-77?"),
        Consulta("rechazo-parametro-dureza", "rechazo", "¿Qué dureza Brinell mínima se exige a la junta de brida de la línea L-104?"),
        Consulta("rechazo-parametro-caudal", "rechazo", "¿Cuál es el caudal máximo de bombeo durante la prueba de la línea L-104?"),
        _integracion(
            "integracion-par",
            "¿Qué par de apriete llevan las bridas de la línea L-510 y con qué condición es válido?",
            ValorEsperado("185"),
            ValorEsperado(texto="LB-2"),
        ),
        _integracion(
            "integracion-precalentamiento",
            "¿Qué temperatura de precalentamiento lleva el acero P-22 y a partir de qué espesor se exige?",
            ValorEsperado("150", "°C"),
            ValorEsperado("25", "mm"),
        ),
        _integracion(
            "integracion-curado",
            "¿Cuánto dura el curado del recubrimiento RX-4 y con qué humedad relativa es válido?",
            ValorEsperado("16", "h"),
            ValorEsperado("70", "%"),
        ),
        _integracion(
            "integracion-inspeccion",
            "¿Cada cuánto se inspeccionan las válvulas V-88 y a partir de qué presión de servicio se aplica ese intervalo?",
            ValorEsperado("6"),
            ValorEsperado("40", "bar"),
        ),
        _tabular(
            "tabular-centro",
            "Según la Tabla 4-B, ¿qué temperatura mínima de precalentamiento lleva el acero P-91 con espesor de 30 mm y CE de 0,42 %?",
            "125",
            ("105", "140", "95", "155"),
        ),
        _tabular(
            "tabular-esquina",
            "Según la Tabla 4-B, ¿qué temperatura mínima de precalentamiento lleva el acero P-91 con espesor de 55 mm y CE de 0,50 %?",
            "175",
            ("155", "140", "130", "110"),
        ),
    ),
)
