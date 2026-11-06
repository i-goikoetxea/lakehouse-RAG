import json
import tempfile
import unittest
from pathlib import Path

from evaluacion.informe import construir_seccion_pista, escribir_informe, renderizar_markdown


def registro(repeticion, categoria, puntuacion, **extra):
    base = {
        "repeticion": repeticion,
        "id": f"{categoria}-{repeticion}",
        "categoria": categoria,
        "pregunta": "¿X?",
        "estado": "puntuada",
        "puntuacion": puntuacion,
        "clasificacion": "acierto" if puntuacion else "valor_ausente",
        "dudosa": False,
        "motivos": [],
        "fallo": None,
        "latencia_respuesta_ms": 100,
        "latencia_recuperacion_ms": 10,
        "fallo_recuperacion": None,
        "respuesta": "texto",
        "herramientas": [],
    }
    base.update(extra)
    return base


def fallo(repeticion, categoria, tipo="timeout"):
    return registro(
        repeticion,
        categoria,
        None,
        estado="fallo_ejecucion",
        clasificacion=None,
        fallo={"tipo": tipo, "mensaje": "m"},
        latencia_respuesta_ms=None,
        latencia_recuperacion_ms=None,
        respuesta=None,
    )


def seccion(registros, repeticiones=2):
    return construir_seccion_pista(
        benchmark="Prueba",
        comparable_con_articulo=False,
        app_commit="abc",
        modelo="sonnet",
        repeticiones=repeticiones,
        registros=registros,
        ingestas=[],
    )


class Agregacion(unittest.TestCase):
    def test_esquema_del_ssd(self):
        resultado = seccion([registro(1, "ruido", 1.0)], 1)
        for clave in (
            "benchmark",
            "comparable_con_articulo",
            "app_commit",
            "modelo",
            "repeticiones",
            "total_consultas",
            "fallos_de_ejecucion",
            "precision_global",
            "metricas",
        ):
            self.assertIn(clave, resultado)
        for clave in (
            "robustez_ruido",
            "rechazo_estricto",
            "exactitud_tabular",
            "latencia_respuesta_ms_media",
            "latencia_recuperacion_ms_media",
        ):
            self.assertIn(clave, resultado["metricas"])

    def test_media_y_extremos_entre_repeticiones(self):
        resultado = seccion(
            [
                registro(1, "ruido", 1.0),
                registro(1, "rechazo", 1.0),
                registro(2, "ruido", 0.0),
                registro(2, "rechazo", 1.0),
            ]
        )
        self.assertEqual(resultado["precision_global"], {"media": 0.75, "min": 0.5, "max": 1.0})
        self.assertEqual(resultado["metricas"]["robustez_ruido"], 0.5)
        self.assertEqual(resultado["metricas"]["rechazo_estricto"], 1.0)

    def test_fallo_no_cuenta_como_acierto_ni_como_cero_y_se_informa_aparte(self):
        resultado = seccion([registro(1, "rechazo", 1.0), fallo(1, "rechazo"), fallo(2, "ruido", "sesion_claude")])
        self.assertEqual(resultado["total_consultas"], 3)
        self.assertEqual(resultado["consultas_puntuadas"], 1)
        self.assertEqual(resultado["fallos_de_ejecucion"], 2)
        self.assertEqual(resultado["fallos_por_tipo"], {"timeout": 1, "sesion_claude": 1})
        self.assertEqual(resultado["metricas"]["rechazo_estricto"], 1.0)
        self.assertIsNone(resultado["metricas"]["robustez_ruido"])
        self.assertEqual(resultado["precision_global"]["media"], 1.0)

    def test_todo_fallido_da_nulos_nunca_cero(self):
        resultado = seccion([fallo(1, "ruido")], 1)
        self.assertEqual(resultado["precision_global"], {"media": None, "min": None, "max": None})
        self.assertIsNone(resultado["metricas"]["latencia_respuesta_ms_media"])

    def test_latencias_y_fallos_de_recuperacion(self):
        resultado = seccion(
            [
                registro(1, "ruido", 1.0, latencia_respuesta_ms=100, latencia_recuperacion_ms=10),
                registro(1, "ruido", 1.0, latencia_respuesta_ms=300, latencia_recuperacion_ms=None,
                         fallo_recuperacion={"tipo": "timeout", "mensaje": "m"}),
            ],
            1,
        )
        self.assertEqual(resultado["metricas"]["latencia_respuesta_ms_media"], 200)
        self.assertEqual(resultado["metricas"]["latencia_recuperacion_ms_media"], 10)
        self.assertEqual(resultado["fallos_de_recuperacion"], 1)
        self.assertEqual(resultado["fallos_de_ejecucion"], 0)

    def test_dudosas_e_ingestas_no_definitivas_quedan_a_la_vista(self):
        resultado = construir_seccion_pista(
            benchmark="Prueba",
            comparable_con_articulo=False,
            app_commit="abc",
            modelo="sonnet",
            repeticiones=1,
            registros=[registro(1, "ruido", 1.0, dudosa=True)],
            ingestas=[{"estado_contenido": "PARCIAL_UNIVERSAL", "pendiente_destilacion": True}],
        )
        self.assertEqual(resultado["consultas_dudosas"], ["ruido-1"])
        self.assertEqual(resultado["ingestas_no_definitivas"], 1)


class Escritura(unittest.TestCase):
    def test_json_y_markdown_con_nombre_del_ssd(self):
        informe = {"generado_en": "t", "semilla": 42, "binario": {"sha256": "h"}, "pistas": [seccion([fallo(1, "ruido")], 1)]}
        with tempfile.TemporaryDirectory() as carpeta:
            ruta_json, ruta_md = escribir_informe(Path(carpeta) / "nueva", informe, "20261102T000000")
            self.assertEqual(ruta_json.name, "benchmark_report_20261102T000000.json")
            self.assertEqual(ruta_md.name, "benchmark_report_20261102T000000.md")
            self.assertEqual(json.loads(ruta_json.read_text(encoding="utf-8")), informe)
            self.assertIn("Fallos de ejecución: 1", ruta_md.read_text(encoding="utf-8"))

    def test_markdown_no_rompe_la_tabla_ni_inventa_ceros(self):
        pregunta = "a|b\nc"
        informe = {
            "generado_en": "t",
            "semilla": 1,
            "binario": {"sha256": "h"},
            "pistas": [seccion([fallo(1, "ruido") | {"id": pregunta}], 1)],
        }
        texto = renderizar_markdown(informe)
        self.assertIn("a\\|b c", texto)
        self.assertIn("n/d", texto)
        self.assertIn("No comparable", texto)


if __name__ == "__main__":
    unittest.main()
