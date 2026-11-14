import unittest

from run_n3_separadas import doc_ids_literales, separar_por_entidad, union_de_resultados


class SepararPorEntidad(unittest.TestCase):
    def test_corta_tras_cada_interrogacion(self):
        self.assertEqual(
            separar_por_entidad("What is the mass of batch X? ¿Cuál es la presión de la línea L-104?"),
            ["What is the mass of batch X?", "¿Cuál es la presión de la línea L-104?"],
        )

    def test_una_instruccion_sin_interrogacion_va_con_la_pregunta_anterior(self):
        self.assertEqual(
            separar_por_entidad("Quelle est la valeur ? Otra pregunta? Indica la revisión."),
            ["Quelle est la valeur ?", "Otra pregunta? Indica la revisión."],
        )

    def test_limite_una_instruccion_seguida_de_otra_pregunta_no_se_separa(self):
        self.assertEqual(
            separar_por_entidad("Una pregunta? Indica la revisión. Otra pregunta?"),
            ["Una pregunta?", "Indica la revisión. Otra pregunta?"],
        )

    def test_sin_interrogacion_queda_entera(self):
        self.assertEqual(separar_por_entidad("presión de prueba"), ["presión de prueba"])


class DocIdsLiterales(unittest.TestCase):
    def test_solo_los_que_aparecen_tal_cual(self):
        self.assertEqual(doc_ids_literales("Según ptf-pur-05 y la L-104", ["PTF-PUR-05", "PTF-HID-10"]), ["PTF-PUR-05"])


class Union(unittest.TestCase):
    def test_sin_repetidos_y_en_orden(self):
        a = [{"id": "1"}, {"id": "2"}]
        b = [{"id": "2"}, {"id": "3"}]
        self.assertEqual([r["id"] for r in union_de_resultados([a, b])], ["1", "2", "3"])


if __name__ == "__main__":
    unittest.main()
