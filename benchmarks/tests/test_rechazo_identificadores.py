"""`rechazo_estricto` no cuenta los dígitos de identificadores ni el recuento «0 resultados».

Cualquier otro número sigue contando: con unidad o sin ella, en tablas, fechas, apartados o páginas.
"""

import unittest

from evaluacion.puntuar import puntuar_rechazo

PREGUNTA = "¿Qué presión de prueba exige la norma ZX-9931?"


def estricta(respuesta):
    return puntuar_rechazo(PREGUNTA, respuesta).puntuacion


class IdentificadoresNoSonCifras(unittest.TestCase):
    def test_codigos_con_separador_no_cuentan(self):
        for codigo in (
            "`bench-rgb-industrial-r1`",
            "EXACT_BM25",
            "PTF-HID-10",
            "L-104",
            "`PTF-HID-10#sec-2`",
            "`PTF-HID-10@v2-CHK-0002`",
        ):
            texto = f"No consta. Busqué en {codigo} sin éxito."
            self.assertEqual(estricta(texto), 1.0, codigo)

    def test_citas_de_revision_y_de_capa_no_cuentan(self):
        for cita in ("la Rev. 02", "la revisión n.º 3", "la Capa 0", "las Capas 0 y 2"):
            texto = f"No consta en {cita}."
            self.assertEqual(estricta(texto), 1.0, cita)

    def test_cero_resultados_no_cuenta(self):
        for recuento in ("devolvió 0 resultados", "dio 0 coincidencias", "0 filas"):
            self.assertEqual(estricta(f"No consta: la búsqueda {recuento}."), 1.0, recuento)

    def test_la_respuesta_real_tipica_sin_valores_es_acierto(self):
        texto = (
            "**No consta** en el centro `bench-rgb-industrial-r2` ninguna norma ZX-9931.\n\n"
            "- La búsqueda literal EXACT_BM25 en la Capa 0 devolvió 0 resultados.\n"
            "- El catálogo no incluye ese código."
        )
        veredicto = puntuar_rechazo(PREGUNTA, texto)
        self.assertEqual(veredicto.puntuacion, 1.0)
        self.assertEqual(veredicto.rechazo_semantico, 1.0)


class LosValoresSiguenCastigando(unittest.TestCase):
    def test_valores_con_unidad_cuentan_aunque_vayan_junto_a_codigos(self):
        for texto in (
            "No consta. La L-204 tiene 14,5 bar.",
            "No consta; PTF-HID-10 fija 45 min.",
            "No consta.\n\n| Parámetro | Valor |\n|---|---|\n| Presión | 12,5 bar |",
        ):
            self.assertEqual(estricta(texto), 0.0, texto)

    def test_numeros_sueltos_fechas_apartados_y_paginas_cuentan(self):
        for texto in (
            "No consta, pero suele ser 7.",
            "No consta. Vigente desde 2025-03-14.",
            "No consta en los apartados 1 y 2.",
            "No consta en la página 12.",
            "No consta; nivel de prelación 2.",
            "No consta: la búsqueda devolvió 3 resultados.",
        ):
            self.assertEqual(estricta(texto), 0.0, texto)

    def test_codigo_sin_separador_sigue_contando(self):
        self.assertEqual(estricta("No consta para el perno M12."), 0.0)

    def test_numero_negativo_o_tras_guion_suelto_cuenta(self):
        self.assertEqual(estricta("No consta; los documentos -30 y -40 tampoco."), 0.0)

    def test_valor_dentro_de_comillas_invertidas_sin_codigo_cuenta(self):
        self.assertEqual(estricta("No consta. Otra norma da `7,5 bar`."), 0.0)

    def test_cero_con_otra_unidad_cuenta(self):
        self.assertEqual(estricta("No consta, aunque la holgura es 0 mm."), 0.0)

    def test_capa_con_unidad_es_un_valor(self):
        self.assertEqual(estricta("No consta; la Capa 2 mm de otra norma no aplica."), 0.0)

    def test_revision_pegada_a_un_valor_no_lo_esconde(self):
        self.assertEqual(estricta("No consta; la Rev. 02 fija 16 bar."), 0.0)


if __name__ == "__main__":
    unittest.main()
