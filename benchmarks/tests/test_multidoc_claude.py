import unittest

from run_multidoc_claude import resumen


def _r(ident, variante, correcta, herramientas=(), fallo=None, tipo="individual"):
    return {
        "id": ident,
        "variante": variante,
        "tipo": tipo,
        "fallo": fallo,
        "correcta": correcta,
        "herramientas": list(herramientas),
        "latencia_ms": 1000.0,
        "cpu_media": 50.0,
    }


class Resumen(unittest.TestCase):
    def test_cuenta_correctas_y_detecta_cambios(self):
        registros = [
            _r("a", "antes", 1.0, ["search_normative_context"]),
            _r("a", "despues", 0.0, ["search_normative_context", "search_normative_context"]),
            _r("b", "antes", 1.0),
            _r("b", "despues", 1.0),
        ]
        filas, cambios = resumen(registros)
        self.assertEqual(filas[("individual", "antes")]["correctas"], 2)
        self.assertEqual(filas[("individual", "despues")]["busquedas_media"], 1.0)
        self.assertEqual(cambios, [{"id": "a", "antes": 1.0, "despues": 0.0}])

    def test_un_reintento_sin_fallo_sustituye_al_fallo(self):
        registros = [_r("a", "antes", None, fallo={"tipo": "timeout"}), _r("a", "antes", 1.0)]
        filas, _ = resumen(registros)
        self.assertEqual(filas[("individual", "antes")]["fallos_ejecucion"], 0)
        self.assertEqual(filas[("individual", "antes")]["correctas"], 1)


if __name__ == "__main__":
    unittest.main()
