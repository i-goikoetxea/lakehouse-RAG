"""Puntuación determinista de respuestas, sin LLM juez.

Un fallo de ejecución nunca llega aquí: el runner lo registra aparte, sin puntuación.
"""

import re
import unicodedata
from dataclasses import dataclass, field
from fractions import Fraction

EXPONENTE_MAXIMO = 400

_ESPACIOS_DE_MILES = "   "
_NUMERO = re.compile(
    rf"(?P<signo>[-−](?=\d))?"
    rf"(?P<cuerpo>\d+(?:[.,{_ESPACIOS_DE_MILES}]\d+)*)"
    rf"(?P<exponente>[eE][+-]?\d+)?"
)
_UNIDAD_TRAS_NUMERO = re.compile(r"[\s*_`(|]{0,6}(%|°\s?C|[A-Za-zµμ]+(?:/[A-Za-z]+[²³23]?)?[²³23]?)")

_FACTORES_DE_UNIDAD = {
    "Pa": ("presion", Fraction(1)),
    "kPa": ("presion", Fraction(10**3)),
    "MPa": ("presion", Fraction(10**6)),
    "Mpa": ("presion", Fraction(10**6)),
    "N/mm2": ("presion", Fraction(10**6)),
    "bar": ("presion", Fraction(10**5)),
    "mm": ("longitud", Fraction(1, 1000)),
    "cm": ("longitud", Fraction(1, 100)),
    "m": ("longitud", Fraction(1)),
    "km": ("longitud", Fraction(1000)),
    "g": ("masa", Fraction(1, 1000)),
    "kg": ("masa", Fraction(1)),
    "t": ("masa", Fraction(1000)),
    "N": ("fuerza", Fraction(1)),
    "kN": ("fuerza", Fraction(1000)),
    "s": ("tiempo", Fraction(1)),
    "min": ("tiempo", Fraction(60)),
    "minuto": ("tiempo", Fraction(60)),
    "minutos": ("tiempo", Fraction(60)),
    "h": ("tiempo", Fraction(3600)),
    "hora": ("tiempo", Fraction(3600)),
    "horas": ("tiempo", Fraction(3600)),
}

_LOCUCIONES_DE_ABSTENCION = (
    "no consta",
    "no constan",
    "no se encuentra",
    "no se ha encontrado",
    "no he encontrado",
    "no encuentro",
    "no se dispone",
    "no dispongo",
    "no tengo informacion",
    "no tengo datos",
    "no hay informacion",
    "no hay datos",
    "no hay ningun",
    "no existe",
    "no aparece",
    "no figura",
    "no se menciona",
    "no se especifica",
    "no se indica",
    "no se define",
    "no puedo responder",
    "no puedo determinar",
    "sin informacion",
    "informacion insuficiente",
    "can not answer",
    "cannot answer",
    "insufficient information",
    "not mentioned",
    "no information",
    "i don't know",
    "i do not know",
)
_ABSTENCION = re.compile(r"\b(?:" + "|".join(re.escape(l) for l in _LOCUCIONES_DE_ABSTENCION) + r")\b")

# Solo en `rechazo_estricto`: dígitos que nombran algo (un código, una revisión, una capa) o cuentan una ausencia
_CERO_RESULTADOS = re.compile(
    r"(?<![\d.,])\b0\s+(?:resultados?|coincidencias?|filas?|fragmentos?|documentos?|referencias?)\b", re.IGNORECASE
)
_CITA_DE_REVISION_LITERAL = re.compile(r"\brev(?:isi[oó]n)?\.?\s*(?:n\.?\s*[ºo°]\.?\s*)?\d+\b", re.IGNORECASE)
_CITA_DE_CAPA = re.compile(r"\bCapas?\s+\d(?:\s*(?:,|y)\s*\d)*\b")
_PALABRA_CON_DIGITOS = re.compile(r"(?<![\w@#-])[^\W\d_][\w@#-]*")
_UNIDADES_DE_VALOR = set(_FACTORES_DE_UNIDAD) | {"%", "°C"}

# Una mención de un valor prohibido se atribuye a otra cosa si su línea lo descarta o su frase cita otra revisión
_MARCAS_DE_DESCARTE = re.compile(
    r"\b(?:descart\w*|invert\w*|invirt\w*|invers[oa]s?|al reves"
    r"|no (?:es|son) (?:valid[oa]s?|aplicables?)|no aplican?|no corresponden?)\b"
)
_MARCAS_DE_RESPUESTA = re.compile(
    r"\b(?:respuesta|valor (?:pedido|solicitado|correcto|buscado|consultado)|el valor es)\b|\bvalor\s*:"
)
_MARCAS_DE_VIGENCIA = re.compile(r"\b(?:vigentes?|en vigor|rigen?|actual|actualmente|hoy)\b")
_VIGENCIA_NEGADA = re.compile(r"\b(?:ya )?no (?:esta |estan |es |son )?(?:vigentes?|en vigor)\b|\bno rigen?\b")
_REVISION = r"(?:rev(?:ision)?\.?\s*(?:n\.?o?\.?\s*)?|(?<!-)r)"
# Un número de revisión de más de 6 cifras no es una cita: así nunca se convierte un número gigante a entero
_CITA_DE_REVISION = re.compile(rf"(?<!\w){_REVISION}(\d{{1,6}})(?!\w)")
_CITA_SECUNDARIA = re.compile(
    r"\b(?:no (?:el|la|los|las) de la|sustituye a la|reemplaza a la)\s+rev(?:ision)?\.?\s*(?:n\.?o?\.?\s*)?\d+(?!\w)"
)
_SEPARADOR_DE_FRASES = re.compile(r"(?<=[.!?])\s+(?=[*_«\"'(¿¡]*[A-ZÁÉÍÓÚÑ])")
_SEPARADOR_DE_TABLA = re.compile(r"^\s*\|?\s*:?-{2,}:?\s*(?:\|\s*:?-{2,}:?\s*)*\|?\s*$")

_CIFRAS_EN_LETRA = re.compile(
    r"\b(?:cero|dos|tres|cuatro|cinco|seis|siete|ocho|nueve|diez|once|doce|trece|catorce|quince|"
    r"dieciseis|diecisiete|dieciocho|diecinueve|veinte|treinta|cuarenta|cincuenta|sesenta|setenta|"
    r"ochenta|noventa|cien|ciento|mil|millon|millones|"
    r"zero|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|twenty|thirty|forty|fifty|"
    r"sixty|seventy|eighty|ninety|hundred|thousand|million)\b"
)


@dataclass(frozen=True)
class Cantidad:
    """Una cifra del texto. Varias lecturas solo si su separador es ambiguo (`1.234`)."""

    lecturas: tuple
    unidad: str | None = None

    @property
    def ambigua(self):
        return len(self.lecturas) > 1


@dataclass(frozen=True)
class Veredicto:
    puntuacion: float
    clasificacion: str
    dudosa: bool = False
    motivos: tuple = field(default_factory=tuple)
    # Solo en rechazo. None: las reglas no pueden saber a quién se atribuye una cifra, lo decide una persona
    rechazo_semantico: float | None = None


@dataclass(frozen=True, init=False)
class ValorEsperado:
    """Un valor numérico, con unidad opcional, una expresión de texto o una cita de revisión."""

    valor: str | None
    unidad: str | None
    texto: str | None
    cantidad: Cantidad | None
    numero_revision: int | None

    def __init__(self, valor=None, unidad=None, texto=None, numero_revision=None):
        if sum(campo is not None for campo in (valor, texto, numero_revision)) != 1:
            raise ValueError("ValorEsperado necesita un valor numérico, un texto o una revisión")
        cantidad = None if valor is None else _cantidad_inequivoca(valor)
        object.__setattr__(self, "valor", valor)
        object.__setattr__(self, "unidad", _normalizar_unidad(unidad) if unidad else None)
        object.__setattr__(self, "texto", None if texto is None else _normalizar_texto(texto))
        object.__setattr__(self, "cantidad", cantidad)
        object.__setattr__(self, "numero_revision", numero_revision)

    @classmethod
    def revision(cls, numero):
        """Cita de una revisión: `Rev. 01`, `rev 1`, `revisión n.º 1` o `R01`."""
        return cls(numero_revision=numero)


def _cantidad_inequivoca(valor):
    cantidades = extraer_cantidades(valor)
    if len(cantidades) != 1 or cantidades[0].ambigua:
        raise ValueError(f"Valor esperado ambiguo o mal formado: {valor!r}")
    return cantidades[0]


def _normalizar_texto(texto):
    sin_tildes = "".join(
        caracter
        for caracter in unicodedata.normalize("NFKD", texto.replace("’", "'"))
        if not unicodedata.combining(caracter)
    )
    return " ".join(sin_tildes.casefold().split())


def _normalizar_unidad(unidad):
    return (
        unidad.replace("²", "2")
        .replace("³", "3")
        .replace("μ", "µ")
        .replace(" ", "")
    )


def _digitos_ascii(digitos):
    return "".join(str(unicodedata.digit(caracter)) for caracter in digitos)


def _canonizar(signo, entero, fraccion, exponente):
    """Forma única de un número, por cadenas: sin límite de precisión ni de memoria."""
    if abs(exponente) > EXPONENTE_MAXIMO:
        return f"{signo}{entero}.{fraccion}e{exponente}"
    digitos = entero + fraccion
    posicion_coma = len(entero) + exponente
    if posicion_coma <= 0:
        digitos = "0" * (1 - posicion_coma) + digitos
        posicion_coma = 1
    elif posicion_coma > len(digitos):
        digitos += "0" * (posicion_coma - len(digitos))
    parte_entera = digitos[:posicion_coma].lstrip("0") or "0"
    parte_decimal = digitos[posicion_coma:].rstrip("0")
    canonico = f"{parte_entera}.{parte_decimal}" if parte_decimal else parte_entera
    return "0" if canonico == "0" else signo + canonico


def _lecturas_de_cuerpo(cuerpo):
    """Parte entera y decimal de cada cifra del cuerpo; más de una pareja si es ambiguo."""
    if any(espacio in cuerpo for espacio in _ESPACIOS_DE_MILES):
        return _lecturas_con_espacios(cuerpo)
    separadores = re.findall(r"[.,]", cuerpo)
    grupos = re.split(r"[.,]", cuerpo)
    if not separadores:
        return [[(cuerpo, "")]]
    if len(separadores) == 1:
        return [_lecturas_de_un_separador(grupos[0], grupos[1])]
    return _lecturas_de_varios_separadores(separadores, grupos)


def _lecturas_con_espacios(cuerpo):
    piezas = re.split(f"[{_ESPACIOS_DE_MILES}]", cuerpo)
    if all(len(_prefijo_entero(pieza)) == 3 for pieza in piezas[1:]):
        return _lecturas_de_cuerpo("".join(piezas))
    return [lectura for pieza in piezas for lectura in _lecturas_de_cuerpo(pieza)]


def _prefijo_entero(pieza):
    return re.split(r"[.,]", pieza)[0]


def _empieza_como_miles(grupo):
    return 1 <= len(grupo) <= 3 and unicodedata.digit(grupo[0]) != 0


def _lecturas_de_un_separador(izquierda, derecha):
    decimal = (izquierda, derecha)
    if len(derecha) == 3 and _empieza_como_miles(izquierda):
        return [decimal, (izquierda + derecha, "")]
    return [decimal]


def _lecturas_de_varios_separadores(separadores, grupos):
    como_componentes = [[(grupo, "")] for grupo in grupos]
    if not _empieza_como_miles(grupos[0]):
        return como_componentes
    if len(set(separadores)) == 1:
        if all(len(grupo) == 3 for grupo in grupos[1:]):
            return [[("".join(grupos), "")]]
        return como_componentes
    miles_coherentes = len(set(separadores[:-1])) == 1 and separadores[-1] != separadores[0]
    if miles_coherentes and all(len(grupo) == 3 for grupo in grupos[1:-1]):
        return [[("".join(grupos[:-1]), grupos[-1])]]
    return como_componentes


def _signo_de(texto, coincidencia):
    if not coincidencia.group("signo"):
        return ""
    inicio = coincidencia.start()
    anterior = texto[inicio - 1] if inicio > 0 else " "
    return "" if anterior.isalnum() or anterior == ")" else "-"


def _exponente_de(coincidencia):
    bruto = coincidencia.group("exponente")
    if not bruto:
        return 0
    digitos = _digitos_ascii(bruto[1:].lstrip("+-")).lstrip("0")[:12] or "0"
    return -int(digitos) if "-" in bruto else int(digitos)


def _unidad_tras(texto, posicion):
    coincidencia = _UNIDAD_TRAS_NUMERO.match(texto, posicion)
    return _normalizar_unidad(coincidencia.group(1)) if coincidencia else None


def extraer_cantidades(texto):
    cantidades = []
    for coincidencia in _NUMERO.finditer(texto):
        signo = _signo_de(texto, coincidencia)
        exponente = _exponente_de(coincidencia)
        componentes = _lecturas_de_cuerpo(coincidencia.group("cuerpo"))
        for indice, lecturas in enumerate(componentes):
            canonicas = tuple(
                dict.fromkeys(
                    _canonizar(signo, _digitos_ascii(entero), _digitos_ascii(fraccion), exponente)
                    for entero, fraccion in lecturas
                )
            )
            es_ultima = indice == len(componentes) - 1
            unidad = _unidad_tras(texto, coincidencia.end()) if es_ultima else None
            cantidades.append(Cantidad(canonicas, unidad))
    return cantidades


def _valor_en_base(lectura, unidad):
    """Valor exacto en la unidad base, o None si no es convertible con seguridad."""
    if unidad not in _FACTORES_DE_UNIDAD or "e" in lectura or len(lectura) > EXPONENTE_MAXIMO:
        return None
    return Fraction(lectura) * _FACTORES_DE_UNIDAD[unidad][1]


def _unidades_convertibles(una, otra):
    return (
        una in _FACTORES_DE_UNIDAD
        and otra in _FACTORES_DE_UNIDAD
        and _FACTORES_DE_UNIDAD[una][0] == _FACTORES_DE_UNIDAD[otra][0]
    )


def _lectura_coincide(lectura, unidad_encontrada, esperado):
    referencia = esperado.cantidad.lecturas[0]
    if esperado.unidad is None or unidad_encontrada == esperado.unidad:
        return lectura == referencia and (esperado.unidad is None or unidad_encontrada is not None)
    if unidad_encontrada is None or not _unidades_convertibles(unidad_encontrada, esperado.unidad):
        return False
    base_encontrada = _valor_en_base(lectura, unidad_encontrada)
    base_esperada = _valor_en_base(referencia, esperado.unidad)
    return base_encontrada is not None and base_encontrada == base_esperada


def _buscar_valor(esperado, cantidades, texto_normalizado):
    """Devuelve (encontrado, solo_por_lectura_ambigua)."""
    if esperado.numero_revision is not None:
        patron = rf"(?<!\w){_REVISION}0*{esperado.numero_revision}(?!\w)"
        return re.search(patron, texto_normalizado) is not None, False
    if esperado.texto is not None:
        patron = r"(?<!\w)" + re.escape(esperado.texto) + r"(?!\w)"
        return re.search(patron, texto_normalizado) is not None, False
    coincidentes = [
        cantidad
        for cantidad in cantidades
        if any(_lectura_coincide(lectura, cantidad.unidad, esperado) for lectura in cantidad.lecturas)
    ]
    return bool(coincidentes), bool(coincidentes) and all(c.ambigua for c in coincidentes)


def _describir(valor):
    if valor.numero_revision is not None:
        return f"Rev. {valor.numero_revision:02d}"
    if valor.texto is not None:
        return valor.texto
    return f"{valor.valor} {valor.unidad}" if valor.unidad else valor.valor


def es_abstencion(texto):
    return _ABSTENCION.search(_normalizar_texto(texto)) is not None


def _es_codigo(palabra):
    return any(c.isdigit() for c in palabra) and any(separador in palabra for separador in "-_@#")


def _sin_citas_de_capa(texto):
    def sustituir(coincidencia):
        return coincidencia.group() if _unidad_tras(texto, coincidencia.end()) in _UNIDADES_DE_VALOR else " "

    return _CITA_DE_CAPA.sub(sustituir, texto)


def _sin_identificadores(texto):
    """Borra lo que nombra en vez de medir: códigos con separador, revisiones, capas y «0 resultados»."""
    texto = _CERO_RESULTADOS.sub(" ", texto)
    texto = _CITA_DE_REVISION_LITERAL.sub(" ", texto)
    texto = _sin_citas_de_capa(texto)
    return _PALABRA_CON_DIGITOS.sub(lambda c: " " if _es_codigo(c.group()) else c.group(), texto)


def puntuar_rechazo(pregunta, respuesta):
    """Nota estricta del SSD: 1.0 con abstención explícita y sin cifras nuevas; 0.0 con cualquier cifra ajena.

    Nota semántica: 1.0 si se abstiene sin cifras nuevas, 0.0 si no se abstiene y None si se abstiene con cifras.
    """
    cifras_de_pregunta = {
        lectura for cantidad in extraer_cantidades(pregunta) for lectura in cantidad.lecturas
    }
    cantidades = extraer_cantidades(_sin_identificadores(respuesta))
    ajenas = [c for c in cantidades if not cifras_de_pregunta.intersection(c.lecturas)]
    if ajenas:
        descripcion = ", ".join(c.lecturas[0] for c in ajenas[:10])
        se_abstiene = es_abstencion(respuesta)
        return Veredicto(
            0.0,
            "cifra_no_presente",
            dudosa=se_abstiene,
            motivos=(f"Cifras ajenas a la pregunta: {descripcion}",),
            rechazo_semantico=None if se_abstiene else 0.0,
        )
    if not es_abstencion(respuesta):
        return Veredicto(0.0, "sin_abstencion", motivos=("Sin locución de abstención",), rechazo_semantico=0.0)
    motivos = []
    semantico = 1.0
    if any(c.ambigua for c in cantidades):
        motivos.append("Cifra con lectura ambigua")
    palabras_de_pregunta = set(_CIFRAS_EN_LETRA.findall(_normalizar_texto(pregunta)))
    if set(_CIFRAS_EN_LETRA.findall(_normalizar_texto(respuesta))) - palabras_de_pregunta:
        motivos.append("Posible cifra escrita en letra")
        semantico = None
    return Veredicto(1.0, "acierto", dudosa=bool(motivos), motivos=tuple(motivos), rechazo_semantico=semantico)


@dataclass(frozen=True)
class _Frase:
    """Unidad mínima de atribución: una frase de una línea o una celda de tabla con sus cabeceras."""

    original: str
    normalizada: str
    cantidades: tuple
    revisiones: frozenset
    linea_descarta: bool

    def contiene(self, valor):
        return _buscar_valor(valor, self.cantidades, self.normalizada)[0]


def _normalizar_para_marcas(texto):
    return _normalizar_texto(texto).replace("*", "")


def _revisiones_citadas(normalizada):
    principales = _CITA_SECUNDARIA.sub(" ", normalizada)
    return frozenset(int(cita.group(1)) for cita in _CITA_DE_REVISION.finditer(principales))


def _frase(original, revisiones_heredadas=frozenset(), linea_descarta=None):
    normalizada = _normalizar_para_marcas(original)
    return _Frase(
        original,
        normalizada,
        tuple(extraer_cantidades(original)),
        _revisiones_citadas(normalizada) or revisiones_heredadas,
        bool(_MARCAS_DE_DESCARTE.search(normalizada)) if linea_descarta is None else linea_descarta,
    )


def _frases_de_linea(linea):
    """Una frase sin cita de revisión hereda la de la frase anterior de la misma línea («En esa revisión…»)."""
    descarta = bool(_MARCAS_DE_DESCARTE.search(_normalizar_para_marcas(linea)))
    frases, heredadas = [], frozenset()
    for trozo in _SEPARADOR_DE_FRASES.split(linea):
        frase = _frase(trozo, heredadas, descarta)
        heredadas = frase.revisiones
        frases.append(frase)
    return frases


def _celdas(linea):
    return [celda.strip() for celda in linea.strip().strip("|").split("|")]


def _frases_de_fila(cabecera, fila):
    """Cada celda lleva la cabecera de su columna y la primera celda de su fila, que suelen decir de qué es."""
    frases = [_frase(fila[0])]
    for indice, celda in enumerate(fila[1:], start=1):
        titulo = cabecera[indice] if indice < len(cabecera) else ""
        frases.append(_frase(f"{titulo} {fila[0]} {celda}"))
    return frases


def _es_inicio_de_tabla(lineas, indice):
    return (
        lineas[indice].lstrip().startswith("|")
        and indice + 1 < len(lineas)
        and _SEPARADOR_DE_TABLA.match(lineas[indice + 1]) is not None
    )


def _frases_de_respuesta(respuesta):
    lineas = respuesta.splitlines()
    frases, indice = [], 0
    while indice < len(lineas):
        if not _es_inicio_de_tabla(lineas, indice):
            frases += _frases_de_linea(lineas[indice])
            indice += 1
            continue
        cabecera = _celdas(lineas[indice])
        indice += 2
        while indice < len(lineas) and lineas[indice].lstrip().startswith("|"):
            frases += _frases_de_fila(cabecera, _celdas(lineas[indice]))
            indice += 1
    return frases


def _presenta_como_respuesta(frase, pide_vigente):
    if _MARCAS_DE_RESPUESTA.search(frase.normalizada):
        return True
    return pide_vigente and _MARCAS_DE_VIGENCIA.search(_VIGENCIA_NEGADA.sub(" ", frase.normalizada)) is not None


def _estado_de_mencion(frase, esperados, pide_vigente):
    """`atribuida`, `ambigua` o `no_atribuida`. Ante cualquier choque de señales, `ambigua`."""
    revisiones_esperadas = frozenset(v.numero_revision for v in esperados if v.numero_revision is not None)
    otras_revisiones = frase.revisiones - revisiones_esperadas if revisiones_esperadas else frozenset()
    cruzada = bool(otras_revisiones) and bool(frase.revisiones & revisiones_esperadas)
    atribuida = frase.linea_descarta or bool(otras_revisiones)
    comparte = any(frase.contiene(valor) for valor in esperados if valor.numero_revision is None)
    if cruzada or (atribuida and (comparte or _presenta_como_respuesta(frase, pide_vigente))):
        return "ambigua"
    return "atribuida" if atribuida else "no_atribuida"


_GRAVEDAD_DE_MENCION = {"atribuida": 0, "ambigua": 1, "no_atribuida": 2}


def _estado_de_prohibido(valor, frases, esperados, pide_vigente):
    """El peor estado entre sus menciones. Si no cabe en ninguna frase (unidad en otra celda), `no_atribuida`."""
    estados = [_estado_de_mencion(frase, esperados, pide_vigente) for frase in frases if frase.contiene(valor)]
    return max(estados, key=_GRAVEDAD_DE_MENCION.get, default="no_atribuida")


def _aparece_como_respuesta(valor, frases, prohibidos):
    return any(
        frase.contiene(valor) and not frase.linea_descarta and not any(frase.contiene(p) for p in prohibidos)
        for frase in frases
    )


def _veredicto_con_menciones(respuesta, esperados, prohibidos, ausentes, pide_vigente):
    frases = _frases_de_respuesta(respuesta)
    estados = {valor: _estado_de_prohibido(valor, frases, esperados, pide_vigente) for valor in prohibidos}
    no_atribuidos = [valor for valor, estado in estados.items() if estado == "no_atribuida"]
    if no_atribuidos:
        motivos = tuple(f"Valor prohibido: {_describir(valor)}" for valor in no_atribuidos)
        return Veredicto(0.0, "valor_prohibido", dudosa=not ausentes, motivos=motivos)
    if ausentes:
        return ()
    ambiguos = [f"Mención ambigua: {_describir(v)}" for v, estado in estados.items() if estado == "ambigua"]
    ambiguos += [
        f"Esperado solo junto a una mención o un descarte: {_describir(valor)}"
        for valor in esperados
        if not _aparece_como_respuesta(valor, frases, prohibidos)
    ]
    if ambiguos:
        return Veredicto(0.0, "mencion_dudosa", dudosa=True, motivos=tuple(ambiguos))
    return tuple(f"Mención atribuida: {_describir(valor)}" for valor in prohibidos)


def puntuar_valores(respuesta, esperados, prohibidos=(), pide_vigente=False):
    """1.0 si aparecen todos los esperados y ningún prohibido se da como respuesta.

    Un prohibido mencionado como descartado o de otra revisión no es fallo; las reglas están en el SSD §5.1.
    `pide_vigente` hace que «vigente», «rige» u «hoy» junto al prohibido cuenten como presentarlo por respuesta.
    """
    cantidades = extraer_cantidades(respuesta)
    texto = _normalizar_texto(respuesta)
    busquedas = [(valor, *_buscar_valor(valor, cantidades, texto)) for valor in esperados]
    ausentes = [valor for valor, encontrado, _ in busquedas if not encontrado]
    presentes_prohibidos = [valor for valor in prohibidos if _buscar_valor(valor, cantidades, texto)[0]]
    menciones = ()
    if presentes_prohibidos:
        resultado = _veredicto_con_menciones(respuesta, esperados, presentes_prohibidos, ausentes, pide_vigente)
        if isinstance(resultado, Veredicto):
            return resultado
        menciones = resultado
    if ausentes:
        clasificacion = "abstencion" if es_abstencion(respuesta) else "valor_ausente"
        motivos = tuple(f"Falta: {_describir(valor)}" for valor in ausentes)
        return Veredicto(0.0, clasificacion, motivos=motivos)
    ambiguos = tuple(f"Lectura ambigua: {_describir(v)}" for v, _, ambiguo in busquedas if ambiguo)
    clasificacion = "acierto_con_mencion" if menciones else "acierto"
    return Veredicto(1.0, clasificacion, dudosa=bool(ambiguos), motivos=ambiguos + menciones)
