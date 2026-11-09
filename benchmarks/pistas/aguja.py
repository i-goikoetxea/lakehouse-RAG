"""Pista Aguja (SSD §4.2): una página con un dato sintético inequívoco insertada a una profundidad dada.

La página lleva un título propio sin numerar, para no entrar en la heurística de numeración del
parser, y un aviso de que no forma parte del original, porque la licencia exige declarar el cambio.
No se añade marcador: la sección esperada es la que envuelve el punto de inserción en `/Outlines`.
"""

import hashlib
import io
import random
import textwrap
from dataclasses import dataclass

from pypdf import PdfReader, PdfWriter
from pypdf.generic import DecodedStreamObject, DictionaryObject, NameObject

PROFUNDIDADES = (0.10, 0.25, 0.50, 0.75, 0.90)
PREFIJO_LOTE = "AGJ-"
# Sin 0/O ni 1/I: el lote no debe admitir dos lecturas
ALFABETO_LOTE = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"
LONGITUD_LOTE = 4
# Un decimal y tres cifras enteras: 100.0 a 999.9, sin ambigüedad de miles
VALOR_MINIMO_DECIMAS = 1000
VALOR_MAXIMO_DECIMAS = 9999
MAX_INTENTOS = 1000
AVISO_DE_CAMBIO = "Inserted page (benchmark test); not part of the original document."

ANCHO_PAGINA_PT = 612
ALTO_PAGINA_PT = 792
MARGEN_PT = 72
CUERPO_PT = 11
INTERLINEA_PT = 15
CARACTERES_POR_LINEA = 90


@dataclass(frozen=True)
class Plantilla:
    clave: str
    sujeto: str
    unidad: str


PLANTILLAS = (
    Plantilla("presion", "maximum test pressure for the relief valves", "kPa"),
    Plantilla("holgura", "minimum guard clearance for the conveyor drives", "mm"),
    Plantilla("carga", "maximum lifting load for the hoist frames", "kg"),
    Plantilla("prueba", "minimum proof load for the anchor bolts", "kN"),
)


# Sin lote ni palabras del título de la página insertada («supplementary», «technical», «note»)
FORMULACIONES_SEMANTICAS = (
    "What value is required for the {sujeto}?",
    "Which limit applies to the {sujeto}?",
    "How much is specified as the {sujeto}?",
    "What figure must be used for the {sujeto}?",
)


@dataclass(frozen=True)
class Aguja:
    lote: str
    valor: str
    unidad: str
    plantilla: str
    sujeto: str
    formulacion_semantica: int = 0

    @property
    def titulo(self):
        return f"Supplementary technical note NT-{self.lote}"

    @property
    def frase(self):
        return f"The {self.sujeto} of batch {self.lote} shall be {self.valor} {self.unidad}."


@dataclass(frozen=True)
class DocumentoAguja:
    pdf: bytes
    pagina_esperada: int
    seccion_esperada: str | None
    profundidad_real: float
    paginas_originales: int


def _generador(semilla, repeticion, profundidad):
    clave = f"{semilla}|{repeticion}|{profundidad:.2f}".encode("utf-8")
    return random.Random(hashlib.sha256(clave).hexdigest())


def generar_aguja(semilla, repeticion, profundidad, texto_base=""):
    """Determinista por (semilla, repetición, profundidad); ni el lote ni el valor aparecen en la base."""
    generador = _generador(semilla, repeticion, profundidad)
    plantilla = generador.choice(PLANTILLAS)
    for _ in range(MAX_INTENTOS):
        lote = PREFIJO_LOTE + "".join(generador.choice(ALFABETO_LOTE) for _ in range(LONGITUD_LOTE))
        valor = f"{generador.randint(VALOR_MINIMO_DECIMAS, VALOR_MAXIMO_DECIMAS) / 10:.1f}"
        if lote not in texto_base and valor not in texto_base:
            # Se sortea al final: el lote y el valor no cambian respecto a la versión sin rotación
            formulacion = generador.randrange(len(FORMULACIONES_SEMANTICAS))
            return Aguja(lote, valor, plantilla.unidad, plantilla.clave, plantilla.sujeto, formulacion)
    raise ValueError("No se encontró una aguja ausente del texto base")


def preguntas_de(aguja):
    """Léxica, con el lote; semántica, sin él. Hay una sola aguja por centro, así que no es ambigua."""
    lexica = f"According to the documentation, what is the {aguja.sujeto} of batch {aguja.lote}?"
    semantica = FORMULACIONES_SEMANTICAS[aguja.formulacion_semantica].format(sujeto=aguja.sujeto)
    return lexica, semantica


def indice_de_insercion(paginas, profundidad, primera_del_cuerpo=1):
    """Páginas originales que preceden a la aguja; su página física (base 1) es este valor + 1."""
    if not 0 < profundidad < 1:
        raise ValueError(f"Profundidad fuera de (0, 1): {profundidad}")
    indice = max(round(profundidad * paginas), primera_del_cuerpo - 1, 1)
    return min(indice, paginas - 1)


def _literal_pdf(texto):
    codificado = texto.encode("cp1252", errors="replace")
    return b"(" + codificado.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)") + b")"


def _contenido_de_lineas(lineas):
    partes = [b"BT", f"/F1 {CUERPO_PT} Tf {INTERLINEA_PT} TL {MARGEN_PT} {ALTO_PAGINA_PT - MARGEN_PT} Td".encode("ascii")]
    for linea in lineas:
        for tramo in textwrap.wrap(linea, CARACTERES_POR_LINEA) or [""]:
            partes.append(_literal_pdf(tramo) + b" Tj T*")
        partes.append(b"T*")
    partes.append(b"ET")
    return b"\n".join(partes)


def pagina_de_texto(lineas):
    """PDF de una página con Helvetica estándar (WinAnsi): texto extraíble sin dependencias extra."""
    escritor = PdfWriter()
    pagina = escritor.add_blank_page(ANCHO_PAGINA_PT, ALTO_PAGINA_PT)
    fuente = DictionaryObject(
        {
            NameObject("/Type"): NameObject("/Font"),
            NameObject("/Subtype"): NameObject("/Type1"),
            NameObject("/BaseFont"): NameObject("/Helvetica"),
            NameObject("/Encoding"): NameObject("/WinAnsiEncoding"),
        }
    )
    pagina[NameObject("/Resources")] = DictionaryObject(
        {NameObject("/Font"): DictionaryObject({NameObject("/F1"): fuente})}
    )
    contenido = DecodedStreamObject()
    contenido.set_data(_contenido_de_lineas(lineas))
    pagina.replace_contents(contenido)
    salida = io.BytesIO()
    escritor.write(salida)
    return salida.getvalue()


def lineas_de_aguja(aguja):
    return [
        aguja.titulo,
        "This note records batch-specific limits agreed during the design review of the equipment "
        "received for acceptance inspection.",
        aguja.frase,
        "Inspectors shall record the batch identifier on the acceptance report.",
        AVISO_DE_CAMBIO,
    ]


def _marcadores_planos(lector, nodos, salida):
    for nodo in nodos:
        if isinstance(nodo, list):
            _marcadores_planos(lector, nodo, salida)
            continue
        try:
            indice = lector.get_destination_page_number(nodo)
        except Exception:  # noqa: BLE001 - un destino roto no envuelve nada
            continue
        if indice is not None and indice >= 0:
            salida.append((indice, len(salida), " ".join(str(nodo.title).split())))
    return salida


def seccion_envolvente(lector, indice_de_insercion_):
    """Título del marcador que abre más tarde sin pasar de la página previa a la aguja."""
    previos = [m for m in _marcadores_planos(lector, lector.outline, []) if m[0] <= indice_de_insercion_ - 1]
    return max(previos)[2] if previos else None


def insertar_aguja(pdf_base, aguja, profundidad, primera_del_cuerpo=1):
    lector = PdfReader(io.BytesIO(pdf_base))
    if lector.is_encrypted:
        raise ValueError("El PDF base declara /Encrypt: la app lo rechazaría")
    paginas = len(lector.pages)
    indice = indice_de_insercion(paginas, profundidad, primera_del_cuerpo)
    escritor = PdfWriter(clone_from=lector)
    escritor.insert_page(PdfReader(io.BytesIO(pagina_de_texto(lineas_de_aguja(aguja)))).pages[0], indice)
    salida = io.BytesIO()
    escritor.write(salida)
    return DocumentoAguja(
        pdf=salida.getvalue(),
        pagina_esperada=indice + 1,
        seccion_esperada=seccion_envolvente(lector, indice),
        profundidad_real=indice / paginas,
        paginas_originales=paginas,
    )


def texto_de_pdf(pdf):
    return "\n".join(pagina.extract_text() or "" for pagina in PdfReader(io.BytesIO(pdf)).pages)
