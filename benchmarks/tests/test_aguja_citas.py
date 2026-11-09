"""Pista Aguja tras las citas de página exacta (`docs/arquitectura/ssd_citas_pagina.md` §7).

Sin Claude: PDF sintéticos creados en el test, nunca texto del documento base real.
"""

import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from pypdf import PdfReader

RAIZ_BENCHMARKS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(RAIZ_BENCHMARKS))

from evaluacion.aguja import (  # noqa: E402
    ENCABEZADO_VERIFICACION,
    agregar_por_profundidad,
    coste_de_marcas,
    fragmento_con_ambas,
    paginas_estrictas_citadas,
    resumen_arnes,
    texto_del_modelo,
    valor_y_pagina,
)
from pistas.aguja import generar_aguja, generar_par_de_agujas, insertar_agujas, preguntas_de  # noqa: E402
from tests.test_aguja import CLI_SIMULADA, RUN_AGUJA, pdf_sintetico  # noqa: E402


def texto_de_pagina(pdf, pagina):
    return PdfReader(io.BytesIO(pdf)).pages[pagina - 1].extract_text()


class TextoDelModelo(unittest.TestCase):
    def test_separa_el_bloque_de_verificacion_de_la_app(self):
        respuesta = f"El valor es 512.5 kPa (p. 52).\n\n{ENCABEZADO_VERIFICACION}\n- «x»: está en p. 53."
        self.assertEqual(texto_del_modelo(respuesta).strip(), "El valor es 512.5 kPa (p. 52).")
        self.assertEqual(texto_del_modelo("Sin bloque (p. 7)."), "Sin bloque (p. 7).")

    def test_paginas_estrictas_solo_cuentan_citas_de_una_pagina(self):
        self.assertEqual(paginas_estrictas_citadas("p. 54, pp. 52-54 y página 12"), {54, 12})
        self.assertEqual(paginas_estrictas_citadas("Págs. 3 y 4"), set())


class ValorYPagina(unittest.TestCase):
    def setUp(self):
        self.una, self.otra = generar_par_de_agujas(42, 1, 0.5)

    def puntuar(self, respuesta):
        return valor_y_pagina(respuesta, self.una, self.otra, 11, 12)

    def test_acierta_solo_con_su_valor_y_su_pagina_sola(self):
        self.assertEqual(self.puntuar(f"{self.una.valor} {self.una.unidad} (p. 11)."), 1.0)
        self.assertEqual(self.puntuar(f"{self.una.valor} {self.una.unidad} (p. 12)."), 0.0)
        self.assertEqual(self.puntuar(f"{self.una.valor} {self.una.unidad} (pp. 11-12)."), 0.0)
        self.assertEqual(self.puntuar(f"{self.una.valor} {self.una.unidad} (p. 11 y también p. 30)."), 0.0)
        mezcla = f"{self.una.valor} {self.una.unidad} (p. 11); {self.otra.valor} {self.otra.unidad} (p. 12)."
        self.assertEqual(self.puntuar(mezcla), 0.0)
        self.assertEqual(self.puntuar(f"{self.otra.valor} {self.otra.unidad} (p. 11)."), 0.0)

    def test_admite_la_otra_aguja_si_se_atribuye_a_su_lote_en_la_misma_linea(self):
        descartada = (
            f"**Valor:** {self.una.valor} {self.una.unidad}\n- **Página del dato:** 11\n"
            f"- En la página 12 figura otra nota con {self.otra.valor} {self.otra.unidad}, del lote {self.otra.lote}."
        )
        self.assertEqual(self.puntuar(descartada), 1.0)
        sin_atribuir = (
            f"**Valor:** {self.una.valor} {self.una.unidad}\n- Página del dato: 11\n- Véase también la página 12."
        )
        self.assertEqual(self.puntuar(sin_atribuir), 0.0)

    def test_ignora_el_bloque_de_verificacion(self):
        respuesta = f"{self.una.valor} {self.una.unidad} (p. 12).\n\n{ENCABEZADO_VERIFICACION}\n- está en p. 11."
        self.assertEqual(self.puntuar(respuesta), 0.0)

    def test_pagina_del_dato_cuenta_como_cita_de_pagina(self):
        from evaluacion.aguja import pagina_exacta_estricta, paginas_citadas

        for texto in ("- **Página del dato:** 55, nota técnica", "Página del dato: 55.", "página física 55", "Page of the value: 55"):
            self.assertEqual(paginas_citadas(texto), {55}, texto)
            self.assertEqual(pagina_exacta_estricta(texto, 55), 1.0, texto)
        self.assertEqual(paginas_citadas("la página del manual de 2024"), set())


class ParDeAgujas(unittest.TestCase):
    def test_casi_iguales_deterministas_y_distintas(self):
        una, otra = generar_par_de_agujas(42, 1, 0.5)
        self.assertEqual((una, otra), generar_par_de_agujas(42, 1, 0.5))
        self.assertEqual((una.sujeto, una.unidad), (otra.sujeto, otra.unidad))
        self.assertNotEqual(una.lote, otra.lote)
        self.assertNotEqual(una.valor, otra.valor)
        self.assertNotEqual(generar_par_de_agujas(42, 2, 0.5), (una, otra))

    def test_ninguna_aparece_en_el_texto_base(self):
        una = generar_aguja(42, 1, 0.5)
        base = f"{una.lote} {una.valor}"
        par = generar_par_de_agujas(42, 1, 0.5, base)
        for aguja in par:
            self.assertNotIn(aguja.lote, base)
            self.assertNotIn(aguja.valor, base)

    def test_inserta_dos_paginas_consecutivas(self):
        base = pdf_sintetico()
        par = generar_par_de_agujas(42, 1, 0.5)
        documento = insertar_agujas(base, par, 0.5)
        self.assertEqual(documento.paginas_esperadas, (11, 12))
        self.assertEqual(len(PdfReader(io.BytesIO(documento.pdf)).pages), 22)
        for aguja, pagina in zip(par, documento.paginas_esperadas):
            self.assertIn(aguja.lote, texto_de_pagina(documento.pdf, pagina))
        self.assertEqual(documento.seccion_esperada, "Bravo requirements")


class FragmentosYCoste(unittest.TestCase):
    def test_fragmento_con_ambas_agujas(self):
        contenido = [{"resultado": json.dumps({"r": [{"texto": "⟦p. 11⟧ AGJ-AAAA\n⟦p. 12⟧ AGJ-BBBB", "cita": {}}]})}]
        self.assertTrue(fragmento_con_ambas(contenido, "AGJ-AAAA", "AGJ-BBBB"))
        self.assertFalse(fragmento_con_ambas([{"texto": "AGJ-AAAA"}, {"texto": "AGJ-BBBB"}], "AGJ-AAAA", "AGJ-BBBB"))

    def test_coste_de_marcas_cuenta_caracteres_de_marca_y_totales(self):
        salida = {"grupos": [{"resultados": [{"texto": "⟦p. 11⟧ uno ⟦p. 12⟧ dos"}]}]}
        marcas, total = coste_de_marcas(salida)
        self.assertEqual(marcas, 2 * len("⟦p. 11⟧ "))
        self.assertEqual(total, len(json.dumps(salida, ensure_ascii=False)))


class AgregadosNuevos(unittest.TestCase):
    def test_metricas_nuevas_se_agregan_y_la_verificacion_se_cuenta_aparte(self):
        casos = [{"profundidad": 0.5, "ingesta": {"latencia_ms": 1000}, "consultas": [
            {"tipo": "aguja_1", "recall_aguja": 1.0, "fallo": None, "pagina_exacta_estricta_modelo": 1.0,
             "valor_y_pagina": 1.0, "mismo_chunk": 1.0, "arnes": [{"etapa": "verificacion_pagina", "motivo": "m"}]},
        ]}]
        filas = {(f["profundidad"], f["metrica"]): f for f in agregar_por_profundidad(casos)}
        for metrica in ("pagina_exacta_estricta_modelo", "valor_y_pagina", "mismo_chunk"):
            self.assertEqual(filas[(0.5, metrica)]["media"], 1.0, metrica)
        resumen = resumen_arnes(casos)
        self.assertEqual(resumen["consultas_con_verificacion_pagina"], 1)
        self.assertEqual(resumen["consultas_con_intervencion"], 0)


class RunDosAgujas(unittest.TestCase):
    def test_variante_con_cli_simulada(self):
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temporal:
            carpeta = Path(temporal)
            base = carpeta / "base.pdf"
            base.write_bytes(pdf_sintetico())
            una, otra = generar_par_de_agujas(42, 1, 0.5)
            respuestas = carpeta / "respuestas.json"
            respuestas.write_text(json.dumps({
                preguntas_de(una)[0]: f"{una.valor} {una.unidad} (p. 11)",
                preguntas_de(otra)[0]: f"{otra.valor} {otra.unidad} (p. 11)",
            }), encoding="utf-8")
            salida = carpeta / "salida"
            resultado = subprocess.run(
                [sys.executable, str(RUN_AGUJA), "--binario", str(CLI_SIMULADA), "--base", str(base), "--dos-agujas",
                 "--profundidad", "0.5", "--repeticiones", "1", "--semilla", "42", "--modelo", "haiku", "--salida", str(salida)],
                capture_output=True, text=True, encoding="utf-8", timeout=120, cwd=carpeta,
                env=dict(os.environ, CLI_SIMULADA_RESPUESTAS=str(respuestas)),
            )
            self.assertEqual(resultado.returncode, 0, resultado.stderr)
            informe = json.loads(next(salida.glob("aguja_report_*.json")).read_text(encoding="utf-8"))
            self.assertEqual(informe["variante"], "dos_agujas")
            caso = informe["casos"][0]
            self.assertEqual(caso["paginas_esperadas"], [11, 12])
            por_tipo = {c["tipo"]: c for c in caso["consultas"]}
            self.assertEqual(set(por_tipo), {"aguja_1", "aguja_2"})
            self.assertEqual(por_tipo["aguja_1"]["valor_y_pagina"], 1.0)
            self.assertEqual(caso["consultas"][0]["pagina_otra_aguja"], 12)
            self.assertEqual(por_tipo["aguja_2"]["valor_y_pagina"], 0.0)
            self.assertEqual(por_tipo["aguja_2"]["recall_aguja"], 1.0)
            self.assertIn("mismo_chunk", por_tipo["aguja_1"])


if __name__ == "__main__":
    unittest.main()
