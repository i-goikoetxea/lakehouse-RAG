import unittest

from run_n3_cuota import agujas_de_sujeto_unico, comparacion, contiene_valor, rango_de_referencia, resultados_planos


def _r(ident, doc, score, texto=""):
    return {"id": ident, "doc_id": doc, "score": score, "texto": texto}


class AgujasDeSujetoUnico(unittest.TestCase):
    def test_cinco_agujas_con_cinco_sujetos_distintos(self):
        agujas = agujas_de_sujeto_unico(42, [0.10, 0.28, 0.50, 0.72, 0.90])
        self.assertEqual(len({a.sujeto for a in agujas}), 5)
        self.assertEqual(len({a.lote for a in agujas}), 5)

    def test_mas_agujas_que_sujetos_es_un_error(self):
        with self.assertRaises(ValueError):
            agujas_de_sujeto_unico(42, [0.1, 0.2, 0.3, 0.4, 0.5, 0.6])


class ContieneValor(unittest.TestCase):
    def test_no_casa_con_otros_digitos(self):
        self.assertTrue(contiene_valor("cada 6 meses", "6"))
        self.assertFalse(contiene_valor("durante 16 h", "6"))
        self.assertFalse(contiene_valor("a 6,5 bar", "6"))
        self.assertTrue(contiene_valor("hasta 6.", "6"))
        self.assertTrue(contiene_valor("valor 12,5 bar", "12,5"))


class RangoDeReferencia(unittest.TestCase):
    def test_exige_todos_los_valores_en_el_documento_objetivo(self):
        planos = [_r("a", "OTRO", 0.9, "185 Nm"), _r("b", "DOC", 0.8, "185 Nm"), _r("c", "DOC", 0.7, "condición LB-2")]
        self.assertEqual(rango_de_referencia(planos, "DOC", ["185", "LB-2"]), 3)
        self.assertIsNone(rango_de_referencia(planos[:2], "DOC", ["185", "LB-2"]))


class ResultadosPlanos(unittest.TestCase):
    def test_aplana_los_grupos_por_score(self):
        contenido = {
            "grupos": [
                {"resultados": [{"id": "x", "score": 0.1, "texto": "", "cita": {"doc_id": "A"}}]},
                {"resultados": [{"id": "y", "score": 0.3, "texto": "", "cita": {"doc_id": "B"}}]},
            ]
        }
        self.assertEqual([r["id"] for r in resultados_planos(contenido)], ["y", "x"])


class Comparacion(unittest.TestCase):
    def _medicion(self, resultados, con_doc):
        individual = {"id": "q", "doc_id": "DOC", "valores": ["7"], "resultados": resultados, "resultados_con_doc_id": con_doc}
        cruzada = {"id": "c", "documentos": ["DOC", "OTRO"], "referencias": [["DOC", ["7"]]], "resultados": resultados}
        return {"individuales": [individual], "cruzadas": [cruzada]}

    def test_recall_perdido_es_retroceso(self):
        antes = self._medicion([_r("a", "DOC", 0.9, "7")], [_r("a", "DOC", 0.9, "7")])
        despues = self._medicion([_r("o", "OTRO", 0.9)], [_r("a", "DOC", 0.9, "7")])
        self.assertEqual(comparacion(antes, despues)["retrocesos"], ["q"])

    def test_salida_con_doc_id_distinta_es_retroceso(self):
        antes = self._medicion([_r("a", "DOC", 0.9, "7")], [_r("a", "DOC", 0.9, "7")])
        despues = self._medicion([_r("a", "DOC", 0.9, "7")], [_r("b", "DOC", 0.9, "7")])
        self.assertEqual(comparacion(antes, despues)["retrocesos"], ["q"])

    def test_sin_cambios_no_hay_retrocesos(self):
        antes = self._medicion([_r("a", "DOC", 0.9, "7")], [_r("a", "DOC", 0.9, "7")])
        self.assertEqual(comparacion(antes, antes)["retrocesos"], [])


if __name__ == "__main__":
    unittest.main()
