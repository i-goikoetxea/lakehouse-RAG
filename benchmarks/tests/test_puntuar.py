import unittest

from evaluacion.puntuar import (
    ValorEsperado,
    extraer_cantidades,
    puntuar_rechazo,
    puntuar_valores,
)


def lecturas(texto):
    return [cantidad.lecturas for cantidad in extraer_cantidades(texto)]


class ExtraccionNumerica(unittest.TestCase):
    def test_coma_y_punto_decimal_son_el_mismo_valor(self):
        self.assertEqual(lecturas("0,42"), lecturas("0.42"))
        self.assertEqual(lecturas("0,42"), [("0.42",)])

    def test_ceros_no_significativos_no_distinguen(self):
        self.assertEqual(lecturas("0,420"), [("0.42",)])
        self.assertEqual(lecturas("Rev. 01"), [("1",)])

    def test_miles_con_separador_decimal_explicito(self):
        self.assertEqual(lecturas("1.234,5"), [("1234.5",)])
        self.assertEqual(lecturas("1,234.5"), [("1234.5",)])
        self.assertEqual(lecturas("1.234.567"), [("1234567",)])

    def test_tres_decimales_tras_cero_no_son_ambiguos(self):
        self.assertEqual(lecturas("0,425"), [("0.425",)])

    def test_un_solo_separador_con_tres_digitos_conserva_ambas_lecturas(self):
        cantidad = extraer_cantidades("1.234")[0]
        self.assertTrue(cantidad.ambigua)
        self.assertEqual(set(cantidad.lecturas), {"1.234", "1234"})

    def test_espacio_fino_es_separador_de_miles(self):
        self.assertEqual(lecturas("1 234,5"), [("1234.5",)])

    def test_signo_solo_cuando_no_es_guion_de_codigo_ni_rango(self):
        self.assertEqual(lecturas("-5"), [("-5",)])
        self.assertEqual(lecturas("−5"), [("-5",)])
        self.assertEqual(lecturas("ISO-9001"), [("9001",)])
        self.assertEqual(lecturas("10-20"), [("10",), ("20",)])

    def test_notacion_cientifica(self):
        self.assertEqual(lecturas("1,5e3"), [("1500",)])
        self.assertEqual(lecturas("2E-2"), [("0.02",)])
        self.assertEqual(lecturas("2e-" + "0" * 20 + "2"), [("0.02",)])

    def test_cifras_dentro_de_codigos_cuentan(self):
        self.assertEqual(lecturas("perno M12"), [("12",)])

    def test_versiones_y_listas_se_separan_en_componentes(self):
        self.assertEqual(lecturas("1.2.3"), [("1",), ("2",), ("3",)])

    def test_superindice_de_unidad_no_es_cifra(self):
        self.assertEqual(lecturas("N/mm²"), [])

    def test_digitos_unicode_no_esquivan_la_extraccion(self):
        self.assertEqual(lecturas("٣"), [("3",)])
        self.assertEqual(lecturas("４２"), [("42",)])

    def test_valores_extremos_no_agotan_recursos(self):
        self.assertEqual(len(extraer_cantidades("1e999999999")), 1)
        digitos = "9" * 100000
        self.assertEqual(lecturas(digitos), [(digitos,)])


class Rechazo(unittest.TestCase):
    PREGUNTA = "¿Qué presión fija la norma ZX-999 a 0,42 m?"

    def test_abstencion_sin_cifras_vale_uno(self):
        veredicto = puntuar_rechazo(self.PREGUNTA, "No consta en la documentación.")
        self.assertEqual(veredicto.puntuacion, 1.0)
        self.assertEqual(veredicto.clasificacion, "acierto")

    def test_cifra_ajena_vale_cero_aunque_haya_abstencion(self):
        veredicto = puntuar_rechazo(self.PREGUNTA, "No consta, pero suele ser 7,5 bar.")
        self.assertEqual(veredicto.puntuacion, 0.0)
        self.assertEqual(veredicto.clasificacion, "cifra_no_presente")
        self.assertIn("7.5", " ".join(veredicto.motivos))

    def test_repetir_cifras_de_la_pregunta_con_otro_formato_es_valido(self):
        veredicto = puntuar_rechazo(self.PREGUNTA, "No consta para ZX-999 ni para 0.42 m.")
        self.assertEqual(veredicto.puntuacion, 1.0)

    def test_sin_abstencion_vale_cero(self):
        veredicto = puntuar_rechazo(self.PREGUNTA, "La norma es muy exigente.")
        self.assertEqual(veredicto.puntuacion, 0.0)
        self.assertEqual(veredicto.clasificacion, "sin_abstencion")

    def test_respuesta_vacia_no_es_abstencion(self):
        self.assertEqual(puntuar_rechazo(self.PREGUNTA, "   ").puntuacion, 0.0)

    def test_tildes_y_mayusculas_no_impiden_la_abstencion(self):
        self.assertEqual(puntuar_rechazo("¿X?", "NO DISPONGO DE INFORMACIÓN.").puntuacion, 1.0)
        self.assertEqual(puntuar_rechazo("¿X?", "No se ha encontrado información").puntuacion, 1.0)

    def test_apostrofo_tipografico_no_impide_la_abstencion(self):
        self.assertEqual(puntuar_rechazo("Who?", "I don’t know.").puntuacion, 1.0)

    def test_locucion_en_ingles_de_rgb(self):
        texto = "I can not answer the question because of the insufficient information in documents."
        self.assertEqual(puntuar_rechazo("Who?", texto).puntuacion, 1.0)

    def test_la_locucion_no_coincide_dentro_de_otra_palabra(self):
        self.assertEqual(puntuar_rechazo("¿X?", "Es no constante.").puntuacion, 0.0)

    def test_cifra_en_letra_queda_dudosa_para_revision_manual(self):
        veredicto = puntuar_rechazo("¿X?", "No consta, aunque suelen ser cuarenta bar.")
        self.assertEqual(veredicto.puntuacion, 1.0)
        self.assertTrue(veredicto.dudosa)

    def test_lectura_ambigua_queda_dudosa_si_coincide_con_la_pregunta(self):
        veredicto = puntuar_rechazo("¿Vale 1234?", "No consta que valga 1.234.")
        self.assertEqual(veredicto.puntuacion, 1.0)
        self.assertTrue(veredicto.dudosa)


class Valores(unittest.TestCase):
    def test_valor_con_formato_distinto_acierta(self):
        veredicto = puntuar_valores("Es 0.42 MPa.", [ValorEsperado("0,42", "MPa")])
        self.assertEqual(veredicto.puntuacion, 1.0)

    def test_no_coincide_por_subcadena(self):
        self.assertEqual(puntuar_valores("Es 142 y 4,2", [ValorEsperado("42")]).puntuacion, 0.0)

    def test_negrita_y_tablas_entre_valor_y_unidad(self):
        for texto in ("**0,42** MPa", "| 0,42 | MPa |", "0,42MPa", "0,42 (MPa)"):
            self.assertEqual(puntuar_valores(texto, [ValorEsperado("0,42", "MPa")]).puntuacion, 1.0, texto)

    def test_conversion_de_unidades_de_la_misma_magnitud(self):
        esperado = [ValorEsperado("0,42", "MPa")]
        for texto in ("420 kPa", "4,2 bar", "0,42 N/mm²", "0,42 N/mm2"):
            self.assertEqual(puntuar_valores(texto, esperado).puntuacion, 1.0, texto)

    def test_unidad_incorrecta_o_ausente_no_acierta(self):
        esperado = [ValorEsperado("0,42", "MPa")]
        for texto in ("0,42 mm", "0,42", "0,42 mPa"):
            self.assertEqual(puntuar_valores(texto, esperado).puntuacion, 0.0, texto)

    def test_integracion_exige_todos_los_esperados(self):
        esperados = [ValorEsperado("85", "N"), ValorEsperado(texto="junta seca")]
        self.assertEqual(puntuar_valores("Son 85 N con la Junta  Seca.", esperados).puntuacion, 1.0)
        parcial = puntuar_valores("Son 85 N.", esperados)
        self.assertEqual(parcial.puntuacion, 0.0)
        self.assertEqual(parcial.clasificacion, "valor_ausente")

    def test_valor_prohibido_anula_el_acierto_y_queda_dudoso(self):
        veredicto = puntuar_valores("Es 0,42, no 0,55.", [ValorEsperado("0,42")], [ValorEsperado("0.55")])
        self.assertEqual(veredicto.puntuacion, 0.0)
        self.assertEqual(veredicto.clasificacion, "valor_prohibido")
        self.assertTrue(veredicto.dudosa)

    def test_abstencion_ante_dato_existente_se_clasifica_aparte(self):
        veredicto = puntuar_valores("No consta.", [ValorEsperado("0,42")])
        self.assertEqual(veredicto.puntuacion, 0.0)
        self.assertEqual(veredicto.clasificacion, "abstencion")

    def test_acierto_por_lectura_ambigua_queda_dudoso(self):
        veredicto = puntuar_valores("Son 1.234 kg.", [ValorEsperado("1234", "kg")])
        self.assertEqual(veredicto.puntuacion, 1.0)
        self.assertTrue(veredicto.dudosa)

    def test_esperado_mal_escrito_se_rechaza_al_definirlo(self):
        for mal_formado in ("1.234", "abc", "1 y 2"):
            with self.assertRaises(ValueError, msg=mal_formado):
                ValorEsperado(mal_formado)
        with self.assertRaises(ValueError):
            ValorEsperado()

    def test_unidad_con_valor_gigante_no_rompe_la_conversion(self):
        veredicto = puntuar_valores("9" * 100000 + " kPa y 1e999999999 kPa", [ValorEsperado("1", "MPa")])
        self.assertEqual(veredicto.puntuacion, 0.0)


if __name__ == "__main__":
    unittest.main()
