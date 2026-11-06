"""Un valor prohibido solo es fallo si se da como respuesta; mencionarlo atribuido a otra cosa no lo es.

Textos sintéticos con la forma de las respuestas reales, sin texto de documentos reales.
"""

import unittest

from evaluacion.puntuar import ValorEsperado, puntuar_valores

REV_00 = ValorEsperado.revision(0)
REV_01 = ValorEsperado.revision(1)
HISTORICO = (ValorEsperado("72", "°C"), REV_00)
POSTERIOR = (ValorEsperado("85", "°C"),)
VIGENTE = (ValorEsperado("8,2", "bar"), REV_01)
DEROGADO = (ValorEsperado("6,5", "bar"),)
CELDA = (ValorEsperado("175", "°C"),)
CELDA_INVERTIDA = (ValorEsperado("110", "°C"),)
RUIDO = (ValorEsperado("38", "°C"),)
DISTRACTORES = (ValorEsperado("42", "°C"), ValorEsperado("35", "°C"))


def historico(texto):
    return puntuar_valores(texto, HISTORICO, POSTERIOR)


def vigente(texto):
    return puntuar_valores(texto, VIGENTE, DEROGADO, pide_vigente=True)


class MencionAtribuidaEsAcierto(unittest.TestCase):
    def assertAcierto(self, veredicto, texto):
        self.assertEqual(veredicto.puntuacion, 1.0, texto)
        self.assertEqual(veredicto.clasificacion, "acierto_con_mencion", texto)
        self.assertFalse(veredicto.dudosa, texto)
        self.assertTrue(any("Mención atribuida" in motivo for motivo in veredicto.motivos), texto)

    def test_valor_de_otra_revision_en_su_propia_linea(self):
        texto = "**Valor: 72 °C** (Rev. 00).\n\n- La Rev. 01 (vigente) fija 85 °C."
        self.assertAcierto(historico(texto), texto)

    def test_la_frase_sin_cita_hereda_la_revision_de_la_frase_anterior(self):
        texto = "La Rev. 00 fijaba 72 °C.\n- La Rev. 01 la sustituye. En esa revisión, fija 85 °C."
        self.assertAcierto(historico(texto), texto)

    def test_la_revision_negada_o_sustituida_no_cuenta_como_cita(self):
        for linea in (
            "Esa cifra de 85 °C es la que rige ahora, no la de la Rev. 00.",
            "- **Rev. 01:** sustituye a la Rev. 00. Fija 85 °C.",
            "Para operar hoy rige la Rev. 01 (85 °C), no el de la Rev. 00.",
        ):
            texto = f"La Rev. 00 fijaba 72 °C.\nLa Rev. 01 entró en vigor después. {linea}"
            self.assertAcierto(historico(texto), texto)

    def test_tabla_con_revisiones_en_las_columnas(self):
        texto = (
            "En la Rev. 00 era 72 °C.\n\n"
            "| Parámetro | Rev. 00 (derogada) | Rev. 01 (vigente) |\n"
            "|---|---|---|\n"
            "| Temperatura | 72 °C | 85 °C |"
        )
        self.assertAcierto(historico(texto), texto)

    def test_tabla_con_revisiones_en_las_filas(self):
        texto = "| Revisión | Estado | Temperatura |\n|---|---|---|\n| Rev. 00 | No vigente | **72 °C** |\n| Rev. 01 | Vigente | 85 °C |"
        self.assertAcierto(historico(texto), texto)

    def test_orden_de_ejes_descartado(self):
        for linea in (
            "Mi primera consulta tenía los ejes invertidos y dio 110 °C. Ese resultado no es válido y lo descarto.",
            "**Consulta descartada:** con el orden inverso devolvió 110 °C.",
            "Con X e Y al revés, la tabla da 110 °C, que no corresponde a su caso.",
        ):
            texto = f"**Temperatura mínima: 175 °C**\n\n{linea}"
            self.assertAcierto(puntuar_valores(texto, CELDA, CELDA_INVERTIDA), texto)

    def test_valores_de_otras_entidades_que_no_aplican(self):
        texto = "La temperatura es 38 °C.\n\nOtras líneas dan valores distintos (L-2: 42 °C, L-3: 35 °C). No aplican a L-1."
        self.assertAcierto(puntuar_valores(texto, RUIDO, DISTRACTORES), texto)

    def test_vigente_con_el_derogado_como_historia(self):
        for linea in ("La Rev. 00, ya derogada, fijaba 6,5 bar.", "La Rev. 00 (no vigente) fijaba 6,5 bar."):
            texto = f"La presión vigente es 8,2 bar, según la Rev. 01.\n{linea}"
            self.assertAcierto(vigente(texto), texto)

    def test_la_lectura_ambigua_del_esperado_sigue_marcando_duda(self):
        veredicto = puntuar_valores("Son 1.234 kg.\nLa Rev. 01 fija 9 kg.", (ValorEsperado("1234", "kg"), REV_00), (ValorEsperado("9", "kg"),))
        self.assertEqual(veredicto.puntuacion, 0.0)
        veredicto = puntuar_valores(
            "Son 1.234 kg (Rev. 00).\nLa Rev. 01 fija 9 kg.", (ValorEsperado("1234", "kg"), REV_00), (ValorEsperado("9", "kg"),)
        )
        self.assertEqual(veredicto.puntuacion, 1.0)
        self.assertTrue(veredicto.dudosa)


class ValorProhibidoDadoComoRespuestaEsFallo(unittest.TestCase):
    def assertFallo(self, veredicto, texto, clasificacion="valor_prohibido"):
        self.assertEqual(veredicto.puntuacion, 0.0, texto)
        self.assertEqual(veredicto.clasificacion, clasificacion, texto)

    def test_el_prohibido_como_unica_respuesta(self):
        texto = "La temperatura de la Rev. 00 es 85 °C."
        self.assertFallo(historico(texto), texto)

    def test_mencion_sin_ninguna_atribucion(self):
        texto = "Es 72 °C (Rev. 00).\nTambién figura 85 °C."
        veredicto = historico(texto)
        self.assertFallo(veredicto, texto)
        self.assertTrue(veredicto.dudosa)

    def test_atribuido_a_la_revision_pedida(self):
        texto = "Valor: 72 °C.\nLa Rev. 00 fijaba 85 °C."
        self.assertFallo(historico(texto), texto)

    def test_revisiones_cruzadas_sin_atribucion_clara(self):
        texto = "Valor: 72 °C.\nLa Rev. 00 fijaba 85 °C, igual que la Rev. 01."
        veredicto = historico(texto)
        self.assertFallo(veredicto, texto, "mencion_dudosa")
        self.assertTrue(veredicto.dudosa)

    def test_negacion_simple_no_es_atribucion(self):
        texto = "Es 72 °C, no 85 °C (Rev. 00)."
        self.assertFallo(historico(texto), texto)

    def test_el_derogado_presentado_como_vigente_no_es_acierto(self):
        for linea in ("La presión vigente es 6,5 bar, según la Rev. 00.", "Hoy rige 6,5 bar (Rev. 00)."):
            texto = f"Valor: 8,2 bar (Rev. 01).\n{linea}"
            veredicto = vigente(texto)
            self.assertFallo(veredicto, texto, "mencion_dudosa")
            self.assertTrue(veredicto.dudosa)

    def test_el_vigente_sin_pedir_vigencia_no_es_presentacion(self):
        texto = "Valor: 72 °C (Rev. 00).\nLa Rev. 01, vigente hoy, fija 85 °C."
        self.assertEqual(historico(texto).puntuacion, 1.0)

    def test_marca_de_respuesta_con_atribucion_queda_dudosa(self):
        for linea in ("La respuesta es 85 °C, de la Rev. 01.", "El valor correcto es 110 °C, aunque lo descarto."):
            texto = f"Valor: 72 °C (Rev. 00). Valor: 175 °C.\n{linea}"
            veredicto = puntuar_valores(texto, HISTORICO + CELDA, POSTERIOR + CELDA_INVERTIDA)
            self.assertFallo(veredicto, texto, "mencion_dudosa")
            self.assertTrue(veredicto.dudosa)

    def test_esperado_y_prohibido_en_la_misma_frase_quedan_dudosos(self):
        texto = "Según la Rev. 00 eran 72 °C y en la Rev. 01 son 85 °C."
        veredicto = historico(texto)
        self.assertFallo(veredicto, texto, "mencion_dudosa")
        self.assertTrue(veredicto.dudosa)

    def test_esperado_solo_dentro_de_una_linea_de_descarte(self):
        texto = "La Rev. 01 fija 85 °C.\nLos 72 °C de la Rev. 00 los descarto."
        veredicto = historico(texto)
        self.assertFallo(veredicto, texto, "mencion_dudosa")
        self.assertTrue(veredicto.dudosa)

    def test_descarte_de_otra_cosa_en_la_frase_del_prohibido(self):
        texto = "Temperatura: 175 °C.\nDescarto la Tabla 4-A. El valor es 110 °C."
        veredicto = puntuar_valores(texto, CELDA, CELDA_INVERTIDA)
        self.assertFallo(veredicto, texto, "mencion_dudosa")

    def test_esperado_ausente_con_prohibido_atribuido(self):
        texto = "La Rev. 01 fija 85 °C. Para la Rev. 00 no tengo el dato."
        veredicto = historico(texto)
        self.assertEqual(veredicto.puntuacion, 0.0)
        self.assertNotEqual(veredicto.clasificacion, "acierto_con_mencion")

    def test_tablas_mal_formadas_no_rompen_ni_atribuyen(self):
        for texto in (
            "Valor: 72 °C (Rev. 00).\n| Rev. 01 |\n|---|\n|\n| 85 °C | | | |",
            "Valor: 72 °C (Rev. 00).\n|---|\n| 85 °C |",
            "Valor: 72 °C (Rev. 00).\n| a | b |\n|---|---|",
        ):
            self.assertIn(historico(texto).clasificacion, ("valor_prohibido", "acierto"), texto)

    def test_respuesta_enorme_se_puntua_sin_agotar_recursos(self):
        texto = "Valor: 72 °C (Rev. 00).\n" + "La Rev. 01 fija 85 °C. Esa cifra rige hoy.\n" * 20000
        self.assertEqual(historico(texto).puntuacion, 1.0)

    def test_revision_con_miles_de_digitos_no_rompe(self):
        texto = "Valor: 72 °C (Rev. 00).\nLa Rev. " + "9" * 10000 + " fija 85 °C."
        self.assertEqual(historico(texto).clasificacion, "valor_prohibido")

    def test_el_codigo_de_centro_no_es_cita_de_revision(self):
        texto = "Valor: 72 °C (Rev. 00).\nEl centro `bench-r1` da 85 °C."
        self.assertFallo(historico(texto), texto)
        self.assertEqual(puntuar_valores("Centro bench-r1.", (REV_01,)).puntuacion, 0.0)


if __name__ == "__main__":
    unittest.main()
