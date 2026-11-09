"""Pista Aguja sin Claude: PDF sintéticos creados en el test, nunca texto del documento base real."""

import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from pypdf import PdfReader, PdfWriter

RAIZ_BENCHMARKS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(RAIZ_BENCHMARKS))

from evaluacion.aguja import (  # noqa: E402
    fragmentos_con_aguja,
    pagina_exacta,
    paginas_citadas,
    recall_aguja,
    seccion_real,
)
from pistas.aguja import (  # noqa: E402
    PROFUNDIDADES,
    generar_aguja,
    indice_de_insercion,
    insertar_aguja,
    pagina_de_texto,
    preguntas_de,
)

RUN_AGUJA = RAIZ_BENCHMARKS / "run_aguja.py"
CLI_SIMULADA = Path(__file__).with_name("cli_simulada.py")


def pdf_sintetico(paginas=20, secciones=((1, "Alpha scope"), (6, "Bravo requirements"), (14, "Charlie tasks"))):
    """`paginas` páginas con texto propio y un marcador de primer nivel por sección (página con base 1)."""
    escritor = PdfWriter()
    for numero in range(1, paginas + 1):
        escritor.add_page(PdfReader(io.BytesIO(pagina_de_texto([f"Synthetic page {numero}", "Filler text."]))).pages[0])
    for pagina, titulo in secciones:
        escritor.add_outline_item(titulo, pagina - 1)
    salida = io.BytesIO()
    escritor.write(salida)
    return salida.getvalue()


def texto_de_pagina(pdf, pagina):
    return PdfReader(io.BytesIO(pdf)).pages[pagina - 1].extract_text()


class GenerarAguja(unittest.TestCase):
    def test_es_determinista_por_semilla_repeticion_y_profundidad(self):
        self.assertEqual(generar_aguja(42, 1, 0.5), generar_aguja(42, 1, 0.5))

    def test_cambia_lote_y_valor_por_profundidad_y_repeticion(self):
        agujas = [generar_aguja(42, r, p) for r in (1, 2, 3) for p in PROFUNDIDADES]
        self.assertEqual(len({a.lote for a in agujas}), len(agujas))
        self.assertEqual(len({a.valor for a in agujas}), len(agujas))

    def test_lote_sin_caracteres_ambiguos_y_valor_con_un_decimal(self):
        aguja = generar_aguja(7, 1, 0.1)
        self.assertRegex(aguja.lote, r"^AGJ-[A-HJ-NP-Z2-9]{4}$")
        self.assertRegex(aguja.valor, r"^[1-9]\d{2}\.\d$")
        self.assertIn(aguja.lote, aguja.frase)
        self.assertIn(f"{aguja.valor} {aguja.unidad}", aguja.frase)

    def test_evita_lote_y_valor_presentes_en_el_texto_base(self):
        ocupada = generar_aguja(42, 1, 0.5)
        texto_base = f"Mentions {ocupada.lote} and {ocupada.valor} already."
        libre = generar_aguja(42, 1, 0.5, texto_base=texto_base)
        self.assertNotIn(libre.lote, texto_base)
        self.assertNotIn(libre.valor, texto_base)

    def test_preguntas_lexica_con_lote_y_semantica_sin_lote(self):
        lexica, semantica = preguntas_de(generar_aguja(42, 1, 0.5))
        aguja = generar_aguja(42, 1, 0.5)
        self.assertIn(aguja.lote, lexica)
        self.assertNotIn(aguja.lote, semantica)
        self.assertNotIn(aguja.valor, lexica + semantica)


class IndiceDeInsercion(unittest.TestCase):
    def test_redondea_la_profundidad_sobre_las_paginas_originales(self):
        self.assertEqual(indice_de_insercion(106, 0.5), 53)
        self.assertEqual(indice_de_insercion(106, 0.1), 11)
        self.assertEqual(indice_de_insercion(106, 0.9), 95)

    def test_nunca_antes_de_la_primera_pagina_del_cuerpo_ni_al_final(self):
        self.assertEqual(indice_de_insercion(20, 0.1, primera_del_cuerpo=6), 5)
        self.assertEqual(indice_de_insercion(3, 0.99), 2)

    def test_rechaza_profundidad_fuera_de_rango(self):
        for profundidad in (0, 1, -0.2, 1.5):
            with self.assertRaises(ValueError):
                indice_de_insercion(100, profundidad)


class InsertarAguja(unittest.TestCase):
    def setUp(self):
        self.base = pdf_sintetico()
        self.aguja = generar_aguja(42, 1, 0.5)

    def test_la_pagina_esperada_contiene_la_aguja_y_el_resto_se_desplaza(self):
        documento = insertar_aguja(self.base, self.aguja, 0.5)
        self.assertEqual(documento.pagina_esperada, 11)
        self.assertEqual(len(PdfReader(io.BytesIO(documento.pdf)).pages), 21)
        self.assertIn(self.aguja.lote, texto_de_pagina(documento.pdf, 11))
        self.assertIn("Synthetic page 10", texto_de_pagina(documento.pdf, 10))
        self.assertIn("Synthetic page 11", texto_de_pagina(documento.pdf, 12))

    def test_la_pagina_insertada_lleva_titulo_cifra_y_aviso_de_cambio(self):
        texto = texto_de_pagina(insertar_aguja(self.base, self.aguja, 0.5).pdf, 11)
        self.assertIn(self.aguja.titulo, texto)
        self.assertIn(self.aguja.valor, texto)
        self.assertIn("not part of the original document", texto)

    def test_la_seccion_esperada_es_el_marcador_que_envuelve_la_insercion(self):
        self.assertEqual(insertar_aguja(self.base, self.aguja, 0.5).seccion_esperada, "Bravo requirements")
        self.assertEqual(insertar_aguja(self.base, self.aguja, 0.75).seccion_esperada, "Charlie tasks")

    def test_conserva_los_marcadores_sin_anadir_uno_para_la_aguja(self):
        lector = PdfReader(io.BytesIO(insertar_aguja(self.base, self.aguja, 0.75).pdf))
        marcadores = [(m.title, lector.get_destination_page_number(m) + 1) for m in lector.outline]
        self.assertEqual(marcadores, [("Alpha scope", 1), ("Bravo requirements", 6), ("Charlie tasks", 14)])

    def test_marcador_en_la_pagina_siguiente_no_envuelve_la_aguja(self):
        base = pdf_sintetico(secciones=((1, "Alpha scope"), (11, "Delta starts after")))
        documento = insertar_aguja(base, self.aguja, 0.5)
        self.assertEqual(documento.pagina_esperada, 11)
        self.assertEqual(documento.seccion_esperada, "Alpha scope")

    def test_sin_marcadores_la_seccion_esperada_queda_sin_definir(self):
        documento = insertar_aguja(pdf_sintetico(secciones=()), self.aguja, 0.5)
        self.assertIsNone(documento.seccion_esperada)

    def test_informa_la_profundidad_real(self):
        self.assertAlmostEqual(insertar_aguja(self.base, self.aguja, 0.25).profundidad_real, 5 / 20)

    def test_rechaza_un_pdf_cifrado(self):
        escritor = PdfWriter(clone_from=PdfReader(io.BytesIO(self.base)))
        escritor.encrypt("")
        cifrado = io.BytesIO()
        escritor.write(cifrado)
        with self.assertRaises(ValueError):
            insertar_aguja(cifrado.getvalue(), self.aguja, 0.5)


class Metricas(unittest.TestCase):
    def setUp(self):
        self.aguja = generar_aguja(42, 1, 0.5)

    def resultado_busqueda(self, inicio, fin, breadcrumb):
        return {
            "resultados": [
                {"tema": "t", "resultados": [
                    {"texto": "unrelated", "cita": {"pagina_inicio": 2, "pagina_fin": 2, "breadcrumb": "Doc > Alpha"}},
                    {"texto": f"... batch {self.aguja.lote} ...",
                     "cita": {"pagina_inicio": inicio, "pagina_fin": fin, "breadcrumb": breadcrumb}},
                ]},
            ]
        }

    def test_recall_acepta_coma_o_punto_y_exige_la_unidad(self):
        entero, decimal = self.aguja.valor.split(".")
        self.assertEqual(recall_aguja(f"It is {entero},{decimal} {self.aguja.unidad}.", self.aguja), 1.0)
        self.assertEqual(recall_aguja(f"It is {self.aguja.valor} {self.aguja.unidad}.", self.aguja), 1.0)
        self.assertEqual(recall_aguja("It is not stated in the document.", self.aguja), 0.0)

    def test_fragmentos_con_aguja_recorre_cualquier_anidamiento(self):
        contenido = [{"nombre": "search_normative_context", "resultado": self.resultado_busqueda(11, 11, "Doc > Bravo")}]
        fragmentos = fragmentos_con_aguja(contenido, self.aguja.lote)
        self.assertEqual(len(fragmentos), 1)
        self.assertEqual(fragmentos[0]["breadcrumb"], "Doc > Bravo")

    def test_fragmentos_aceptan_resultado_serializado_como_texto(self):
        contenido = [{"resultado": json.dumps(self.resultado_busqueda(11, 11, "Doc > Bravo"))}]
        self.assertEqual(len(fragmentos_con_aguja(contenido, self.aguja.lote)), 1)

    def test_pagina_exacta_si_algun_fragmento_cubre_la_pagina_esperada(self):
        fragmentos = fragmentos_con_aguja(self.resultado_busqueda(10, 11, "Doc > Bravo"), self.aguja.lote)
        self.assertEqual(pagina_exacta(fragmentos, 11), 1.0)
        self.assertEqual(pagina_exacta(fragmentos, 12), 0.0)
        self.assertIsNone(pagina_exacta([], 11))

    def test_seccion_real_compara_el_ultimo_tramo_del_breadcrumb(self):
        fragmentos = fragmentos_con_aguja(self.resultado_busqueda(11, 11, "Doc > TASK 201  PRELIMINARY hazard list"), self.aguja.lote)
        self.assertEqual(seccion_real(fragmentos, "TASK 201 PRELIMINARY HAZARD LIST"), 1.0)
        self.assertEqual(seccion_real(fragmentos, "TASK 202 PRELIMINARY HAZARD ANALYSIS"), 0.0)
        self.assertIsNone(seccion_real(fragmentos, None))
        self.assertIsNone(seccion_real([], "TASK 201"))

    def test_paginas_citadas_en_la_respuesta(self):
        texto = "See page 54 (pp. 53-55) and p. 7; también la página 12 y págs. 3 y 4."
        self.assertEqual(paginas_citadas(texto), {54, 53, 55, 7, 12, 3, 4})


class ClienteQueAnota:
    def __init__(self):
        self.ingeridos = []

    def crear_centro(self, centro_id, ruta):
        pass

    def activar(self, centro_id):
        pass

    def ingestar(self, ruta, centro_id):
        self.ingeridos.append(Path(ruta).read_bytes())
        return {"doc_id": "D", "estado_contenido": "COMPLETO"}


class DocumentosBinariosEnRun(unittest.TestCase):
    def test_preparar_centro_escribe_bytes_tal_cual_y_texto_en_utf8(self):
        import run
        from pistas.base import Documento, Pista

        pdf = pdf_sintetico(paginas=3)
        pista = Pista("p", "P", False, (Documento("a.pdf", pdf), Documento("b.md", "Presión ñ")), ())
        cliente = ClienteQueAnota()
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temporal:
            run._preparar_centro(cliente, pista, Path(temporal), "c", 1)
        self.assertEqual(cliente.ingeridos, [pdf, "Presión ñ".encode("utf-8")])

    def test_sha256_de_documentos_admite_bytes(self):
        import run
        from pistas.base import Documento, Pista

        una = Pista("p", "P", False, (Documento("a.pdf", b"%PDF-1"),), ())
        otra = Pista("p", "P", False, (Documento("a.pdf", b"%PDF-2"),), ())
        self.assertNotEqual(run._sha256_de_documentos(una), run._sha256_de_documentos(otra))


class RunAgujaExtremoAExtremo(unittest.TestCase):
    def test_piloto_con_cli_simulada_y_pdf_sintetico(self):
        with tempfile.TemporaryDirectory(ignore_cleanup_errors=True) as temporal:
            carpeta = Path(temporal)
            base = carpeta / "base.pdf"
            base.write_bytes(pdf_sintetico())
            aguja = generar_aguja(42, 1, 0.5)
            lexica, semantica = preguntas_de(aguja)
            respuestas = carpeta / "respuestas.json"
            respuestas.write_text(json.dumps({lexica: f"{aguja.valor} {aguja.unidad} (page 11)", semantica: "No consta."}), encoding="utf-8")
            salida = carpeta / "salida"
            resultado = subprocess.run(
                [sys.executable, str(RUN_AGUJA), "--binario", str(CLI_SIMULADA), "--base", str(base),
                 "--profundidad", "0.5", "--repeticiones", "1", "--semilla", "42", "--modelo", "haiku", "--salida", str(salida)],
                capture_output=True, text=True, encoding="utf-8", timeout=120, cwd=carpeta,
                env=dict(os.environ, CLI_SIMULADA_RESPUESTAS=str(respuestas)),
            )
            self.assertEqual(resultado.returncode, 0, resultado.stderr)
            informe = json.loads(next(salida.glob("aguja_report_*.json")).read_text(encoding="utf-8"))
            self.assertEqual(len(informe["casos"]), 1)
            caso = informe["casos"][0]
            self.assertEqual(caso["pagina_esperada"], 11)
            self.assertEqual(caso["seccion_esperada"], "Bravo requirements")
            self.assertEqual(caso["ingesta"]["estado_contenido"], "COMPLETO")
            por_tipo = {c["tipo"]: c for c in caso["consultas"]}
            self.assertEqual(por_tipo["lexica"]["recall_aguja"], 1.0)
            self.assertEqual(por_tipo["semantica"]["recall_aguja"], 0.0)
            self.assertIn(11, por_tipo["lexica"]["paginas_citadas_respuesta"])
            self.assertEqual(len(informe["base"]["sha256"]), 64)
            self.assertTrue(list(salida.glob("aguja_report_*.md")))


if __name__ == "__main__":
    unittest.main()
