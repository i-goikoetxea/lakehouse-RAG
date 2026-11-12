import unittest

from run_multidoc_claude import auditoria, conserva_idioma, palabras, resumen


def _r(ident, variante, correcta, herramientas=(), argumentos=None, fallo=None, tipo="individual", pasada=1, pregunta="¿Cuál es la presión de prueba?"):
    herramientas = list(herramientas)
    return {
        "id": ident,
        "variante": variante,
        "pasada": pasada,
        "tipo": tipo,
        "pregunta": pregunta,
        "fallo": fallo,
        "correcta": correcta,
        "herramientas": herramientas,
        "argumentos": argumentos if argumentos is not None else [{} for _ in herramientas],
        "latencia_ms": 1000.0,
        "cpu_media": 50.0,
    }


class Resumen(unittest.TestCase):
    def test_cuenta_correctas_por_pasada_y_detecta_cambios(self):
        registros = [
            _r("a", "antes", 1.0, ["search_normative_context"]),
            _r("a", "despues", 0.0, ["search_normative_context", "search_normative_context"]),
            _r("a", "antes", 1.0, pasada=2),
            _r("a", "despues", 1.0, pasada=2),
        ]
        filas, cambios, _ = resumen(registros)
        self.assertEqual(filas[("individual", "antes")]["correctas_por_pasada"], [1, 1])
        self.assertEqual(filas[("individual", "despues")]["correctas_por_pasada"], [0, 1])
        self.assertEqual(cambios, [{"id": "a", "antes": "2/2", "despues": "1/2"}])

    def test_un_reintento_sin_fallo_sustituye_al_fallo(self):
        registros = [_r("a", "antes", None, fallo={"tipo": "timeout"}), _r("a", "antes", 1.0)]
        filas, _, _ = resumen(registros)
        self.assertEqual(filas[("individual", "antes")]["fallos_ejecucion"], 0)
        self.assertEqual(filas[("individual", "antes")]["correctas_por_pasada"], [1])


class Auditoria(unittest.TestCase):
    def test_palabras_sin_cifras_ni_cortas(self):
        self.assertEqual(palabras("The L-104 line at 12 bar shall hold"), {"line", "shall", "hold"})

    def test_conservar_exige_dos_palabras_literales(self):
        self.assertTrue(conserva_idioma("maximum test pressure relief valves", "What is the maximum test pressure for the relief valves?"))
        self.assertFalse(conserva_idioma("presión máxima de prueba válvulas de alivio", "What is the maximum test pressure for the relief valves?"))

    def test_cruzada_audita_cada_parte_con_su_primera_busqueda(self):
        pregunta = "What is the maximum test pressure for the relief valves? ¿Cuál es la presión de prueba de la línea L-104?"
        registro = _r(
            "c",
            "despues",
            1.0,
            ["search_normative_context", "search_normative_context"],
            [{"consulta": "presión máxima válvulas de alivio"}, {"consulta": "presión de prueba línea L-104"}],
            tipo="cruzada",
            pregunta=pregunta,
        )
        self.assertEqual(auditoria(registro), [False, True])

    def test_una_parte_resuelta_por_tabla_no_cuenta(self):
        registro = _r(
            "t",
            "antes",
            1.0,
            ["lookup_double_entry_table"],
            [{"tabla_id": "T-4B"}],
            pregunta="Según la Tabla 4-B, ¿qué temperatura mínima lleva el acero P-91?",
        )
        self.assertEqual(auditoria(registro), [None])

    def test_sin_busquedas_ni_tabla_la_parte_no_se_conserva(self):
        self.assertEqual(auditoria(_r("s", "antes", 0.0)), [False])


if __name__ == "__main__":
    unittest.main()
