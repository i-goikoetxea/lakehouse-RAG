import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

RAIZ_BENCHMARKS = Path(__file__).resolve().parent.parent
RUN = RAIZ_BENCHMARKS / "run.py"
CLI_SIMULADA = Path(__file__).with_name("cli_simulada.py")


class RunExtremoAExtremo(unittest.TestCase):
    def setUp(self):
        carpeta = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.addCleanup(carpeta.cleanup)
        self.salida = Path(carpeta.name)

    def ejecutar(self, modo="normal", binario=CLI_SIMULADA, extra=(), entorno_extra=None):
        entorno = dict(os.environ, CLI_SIMULADA_MODO=modo, **(entorno_extra or {}))
        return subprocess.run(
            [sys.executable, str(RUN), "--binario", str(binario), "--pista", "humo", "--modelo", "haiku",
             "--repeticiones", "2", "--semilla", "42", "--salida", str(self.salida), *extra],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=entorno,
            cwd=self.salida,
            timeout=120,
        )

    def informe(self):
        return json.loads(next(self.salida.glob("benchmark_report_*.json")).read_text(encoding="utf-8"))

    def test_camino_feliz_genera_informe_completo(self):
        resultado = self.ejecutar()
        self.assertEqual(resultado.returncode, 0, resultado.stderr)
        informe = self.informe()
        self.assertEqual(informe["semilla"], 42)
        self.assertEqual(len(informe["binario"]["sha256"]), 64)
        self.assertEqual(informe["binario"]["nombre"], "cli_simulada.py")
        self.assertNotIn(str(CLI_SIMULADA.parent), json.dumps(informe))
        pista = informe["pistas"][0]
        self.assertFalse(pista["comparable_con_articulo"])
        self.assertEqual(pista["modelo"], "haiku")
        self.assertEqual(pista["repeticiones"], 2)
        self.assertEqual(pista["total_consultas"], 6)
        self.assertEqual(pista["fallos_de_ejecucion"], 0)
        self.assertEqual(pista["precision_global"], {"media": 1.0, "min": 1.0, "max": 1.0})
        self.assertEqual(pista["metricas"]["robustez_ruido"], 1.0)
        self.assertEqual(pista["metricas"]["rechazo_estricto"], 1.0)
        self.assertIsNotNone(pista["metricas"]["latencia_recuperacion_ms_media"])
        crudas = pista["registros"]
        self.assertTrue(all(r["respuesta"] and r["herramientas"] for r in crudas))
        self.assertTrue(list(self.salida.glob("benchmark_report_*.md")))
        lineas = next(self.salida.glob("benchmark_crudas_*.jsonl")).read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(lineas), 6)

    def test_rechazo_con_cifra_inventada_baja_la_metrica(self):
        self.assertEqual(self.ejecutar("alucina").returncode, 0)
        pista = self.informe()["pistas"][0]
        self.assertEqual(pista["metricas"]["rechazo_estricto"], 0.0)
        self.assertEqual(pista["fallos_de_ejecucion"], 0)

    def test_sesion_caducada_son_fallos_no_abstenciones(self):
        resultado = self.ejecutar("sesion_caducada")
        self.assertEqual(resultado.returncode, 3)
        pista = self.informe()["pistas"][0]
        self.assertEqual(pista["fallos_de_ejecucion"], 6)
        self.assertEqual(pista["fallos_por_tipo"], {"sesion_claude": 6})
        self.assertIsNone(pista["metricas"]["rechazo_estricto"])
        self.assertIsNone(pista["precision_global"]["media"])

    def test_ingesta_fallida_marca_las_consultas_sin_preguntar(self):
        resultado = self.ejecutar("ingesta_falla")
        self.assertEqual(resultado.returncode, 3)
        pista = self.informe()["pistas"][0]
        self.assertEqual(pista["fallos_por_tipo"], {"preparacion": 6})
        self.assertTrue(all(r["respuesta"] is None for r in pista["registros"]))

    def test_ingesta_provisional_queda_senalada(self):
        self.assertEqual(self.ejecutar("ingesta_provisional").returncode, 0)
        self.assertEqual(self.informe()["pistas"][0]["ingestas_no_definitivas"], 2)

    def test_fallo_de_mcp_no_invalida_la_respuesta(self):
        self.assertEqual(self.ejecutar("mcp_error_herramienta").returncode, 0)
        pista = self.informe()["pistas"][0]
        self.assertEqual(pista["fallos_de_ejecucion"], 0)
        self.assertEqual(pista["fallos_de_recuperacion"], 6)
        self.assertIsNone(pista["metricas"]["latencia_recuperacion_ms_media"])

    def test_centros_reales_quedan_intactos_y_se_borra_el_temporal(self):
        real = self.salida / "config_real"
        real.mkdir()
        (real / "centers_registry.json").write_text("{}", encoding="utf-8")
        temporal = self.salida / "tmp"
        temporal.mkdir()
        self.ejecutar(entorno_extra={"LAKEHOUSE_RAG_CONFIG_DIR": str(real), "TEMP": str(temporal), "TMP": str(temporal), "TMPDIR": str(temporal)})
        self.assertEqual((real / "centers_registry.json").read_text(encoding="utf-8"), "{}")
        self.assertEqual(list(temporal.glob("benchmark_*")), [])

    def test_misma_semilla_mismo_orden_y_distintas_repeticiones(self):
        self.ejecutar()
        ids = [r["id"] for r in self.informe()["pistas"][0]["registros"]]
        for ruta in self.salida.glob("benchmark_*"):
            ruta.unlink()
        self.ejecutar()
        self.assertEqual(ids, [r["id"] for r in self.informe()["pistas"][0]["registros"]])

    def test_binario_que_no_cumple_el_contrato_se_rechaza_antes_de_ejecutar(self):
        impostor = self.salida / "impostor.py"
        impostor.write_text("print('otra cosa')", encoding="utf-8")
        resultado = self.ejecutar(binario=impostor)
        self.assertEqual(resultado.returncode, 2)
        self.assertIn("--crear-centro", resultado.stderr)
        self.assertEqual(list(self.salida.glob("benchmark_report_*")), [])

    def test_humo_se_niega_a_correr_sobre_un_binario_que_gastaria_claude(self):
        real_simulado = self.salida / "real.py"
        real_simulado.write_text(
            "print('--mcp-server --crear-centro --activar --ingest --centro --ask --modelo')\n"
            "raise SystemExit(0)\n",
            encoding="utf-8",
        )
        resultado = self.ejecutar(binario=real_simulado)
        self.assertEqual(resultado.returncode, 2)
        self.assertIn("simulada", resultado.stderr)

    def test_binario_inexistente_y_repeticiones_invalidas(self):
        self.assertEqual(self.ejecutar(binario=self.salida / "falta.exe").returncode, 2)
        self.assertEqual(self.ejecutar(extra=("--repeticiones", "0")).returncode, 2)

    def test_modelo_que_la_app_leeria_como_bandera_se_rechaza_antes_de_gastar(self):
        resultado = self.ejecutar(extra=("--modelo=-m",))
        self.assertEqual(resultado.returncode, 2)
        self.assertEqual(list(self.salida.glob("benchmark_*")), [])

    def test_temporal_dentro_del_directorio_real_se_rechaza_sin_traza(self):
        real = self.salida / "lakehouse_rag"
        real.mkdir()
        resultado = self.ejecutar(entorno_extra={"APPDATA": str(self.salida), "TEMP": str(real), "TMP": str(real), "TMPDIR": str(real)})
        self.assertEqual(resultado.returncode, 2)
        self.assertNotIn("Traceback", resultado.stderr)

    def test_pista_no_implementada_no_se_acepta(self):
        resultado = subprocess.run(
            [sys.executable, str(RUN), "--binario", str(CLI_SIMULADA), "--pista", "rgb-publico"],
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        self.assertEqual(resultado.returncode, 2)


if __name__ == "__main__":
    unittest.main()
