import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

RAIZ_BENCHMARKS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(RAIZ_BENCHMARKS))

from evaluacion.puntuar import ValorEsperado, _describir, extraer_cantidades, puntuar_valores  # noqa: E402
from pistas import ciclo_de_vida, rgb_industrial  # noqa: E402

RUN = RAIZ_BENCHMARKS / "run.py"
CLI_SIMULADA = Path(__file__).with_name("cli_simulada.py")


def lecturas_de(contenido):
    return {lectura for cantidad in extraer_cantidades(contenido) for lectura in cantidad.lecturas}


def contenido_de(pista, nombre_archivo):
    return next(d.contenido for d in pista.documentos if d.nombre_archivo == nombre_archivo)


def respuesta_ideal(consulta):
    if consulta.categoria == "rechazo":
        return "No consta en la documentación."
    return "Respuesta: " + "; ".join(_describir(valor) for valor in consulta.esperados)


class RevisionCitada(unittest.TestCase):
    def test_acepta_las_formas_habituales_de_citar(self):
        for texto in ("Rev. 01", "rev 1", "Rev.01", "Revisión 01", "revisión n.º 1", "**Rev. 01**", "(R01)"):
            self.assertEqual(puntuar_valores(texto, (ValorEsperado.revision(1),)).puntuacion, 1.0, texto)

    def test_no_confunde_otra_revision(self):
        for texto in ("Rev. 00", "Rev. 10", "Rev. 11", "previo", "PTF-1"):
            self.assertEqual(puntuar_valores(texto, (ValorEsperado.revision(1),)).puntuacion, 0.0, texto)

    def test_revision_cero(self):
        self.assertEqual(puntuar_valores("según la Rev. 00", (ValorEsperado.revision(0),)).puntuacion, 1.0)
        self.assertEqual(puntuar_valores("según la Rev. 01", (ValorEsperado.revision(0),)).puntuacion, 0.0)


class UnidadesDeTiempo(unittest.TestCase):
    def test_minutos_y_horas_se_convierten(self):
        for texto in ("27 min", "27 minutos", "0,45 h"):
            self.assertEqual(puntuar_valores(texto, (ValorEsperado("27", "min"),)).puntuacion, 1.0, texto)
        self.assertEqual(puntuar_valores("16 horas", (ValorEsperado("16", "h"),)).puntuacion, 1.0)

    def test_otra_magnitud_no_coincide(self):
        self.assertEqual(puntuar_valores("27 bar", (ValorEsperado("27", "min"),)).puntuacion, 0.0)


class FixturesRgbIndustrial(unittest.TestCase):
    def test_tiene_objetivo_y_cinco_distractores_de_la_misma_terminologia(self):
        distractores = [d for d in rgb_industrial.PISTA.documentos if d.nombre_archivo.startswith("distractor")]
        self.assertEqual(len(distractores), 5)
        for distractor in distractores:
            self.assertIn("Presión de prueba hidrostática", distractor.contenido)
            self.assertNotIn("L-104", distractor.contenido)

    def test_ruido_esperados_en_objetivo_y_prohibidos_en_distractores(self):
        objetivo = lecturas_de(contenido_de(rgb_industrial.PISTA, "ruido_objetivo.md"))
        otros = lecturas_de(
            "\n".join(d.contenido for d in rgb_industrial.PISTA.documentos if d.nombre_archivo.startswith("distractor"))
        )
        ruido = [c for c in rgb_industrial.PISTA.consultas if c.categoria == "ruido"]
        self.assertGreaterEqual(len(ruido), 5)
        for consulta in ruido:
            self.assertEqual(len(consulta.prohibidos), 5, consulta.id)
            for esperado in consulta.esperados:
                self.assertIn(esperado.cantidad.lecturas[0], objetivo, consulta.id)
                self.assertNotIn(esperado.cantidad.lecturas[0], otros, consulta.id)
            for prohibido in consulta.prohibidos:
                self.assertIn(prohibido.cantidad.lecturas[0], otros, consulta.id)

    def test_rechazo_pregunta_por_lo_que_no_existe(self):
        todo = "\n".join(d.contenido for d in rgb_industrial.PISTA.documentos)
        rechazos = [c for c in rgb_industrial.PISTA.consultas if c.categoria == "rechazo"]
        self.assertGreaterEqual(len(rechazos), 5)
        for codigo in ("ZX-9931", "L-999", "PTF-HID-77", "Brinell", "bombeo"):
            self.assertNotIn(codigo, todo)
            self.assertTrue(any(codigo in c.pregunta for c in rechazos), codigo)

    def test_integracion_dato_y_condicion_en_secciones_distintas(self):
        integracion = [c for c in rgb_industrial.PISTA.consultas if c.categoria == "integracion"]
        self.assertGreaterEqual(len(integracion), 4)
        secciones = contenido_de(rgb_industrial.PISTA, "integracion_tabular.md").split("## ")
        dato = next(s for s in secciones if s.startswith("2."))
        condicion = next(s for s in secciones if s.startswith("7."))
        for consulta in integracion:
            self.assertEqual(len(consulta.esperados), 2, consulta.id)
            valor, requisito = consulta.esperados
            self.assertIn(valor.cantidad.lecturas[0], lecturas_de(dato), consulta.id)
            if requisito.texto is not None:
                self.assertIn(requisito.texto, condicion.casefold(), consulta.id)
                self.assertNotIn(requisito.texto, dato.casefold(), consulta.id)
            else:
                self.assertIn(requisito.cantidad.lecturas[0], lecturas_de(condicion), consulta.id)
                self.assertNotIn(requisito.cantidad.lecturas[0], lecturas_de(dato), consulta.id)

    def test_tabular_cruza_la_celda_exacta(self):
        tabulares = [c for c in rgb_industrial.PISTA.consultas if c.categoria == "tabular"]
        self.assertGreaterEqual(len(tabulares), 2)
        for consulta in tabulares:
            self.assertTrue(consulta.prohibidos, consulta.id)


class FixturesCicloDeVida(unittest.TestCase):
    def test_rev00_se_ingiere_antes_que_rev01(self):
        nombres = [d.nombre_archivo for d in ciclo_de_vida.PISTA.documentos]
        self.assertEqual(nombres, ["purga_rev00.md", "purga_rev01.md"])

    def test_cada_revision_declara_codigo_revision_y_fecha(self):
        for documento, revision, fecha in zip(
            ciclo_de_vida.PISTA.documentos, ("Rev. 00", "Rev. 01"), ("2025-02-03", "2026-03-02")
        ):
            self.assertIn("PTF-PUR-05", documento.contenido)
            self.assertIn(revision, documento.contenido)
            self.assertIn(fecha, documento.contenido)

    def test_la_rev01_no_contiene_los_valores_de_la_rev00(self):
        rev00 = lecturas_de(ciclo_de_vida.PISTA.documentos[0].contenido.split("## 2.")[1].split("## 3.")[0])
        rev01 = lecturas_de(ciclo_de_vida.PISTA.documentos[1].contenido)
        self.assertEqual(rev00 & rev01 - {"2"}, set())

    def test_prueba_las_dos_formas_y_lo_vigente(self):
        por_categoria = {}
        for consulta in ciclo_de_vida.PISTA.consultas:
            por_categoria.setdefault(consulta.categoria, []).append(consulta)
        self.assertEqual(set(por_categoria), {"vigente", "historico_fecha", "historico_revision"})
        for consulta in por_categoria["vigente"]:
            self.assertEqual(_describir(consulta.esperados[-1]), "Rev. 01", consulta.id)
        for categoria in ("historico_fecha", "historico_revision"):
            for consulta in por_categoria[categoria]:
                self.assertEqual(_describir(consulta.esperados[-1]), "Rev. 00", consulta.id)
                self.assertTrue(consulta.prohibidos, consulta.id)

    def test_por_fecha_cita_una_fecha_y_por_revision_no(self):
        for consulta in ciclo_de_vida.PISTA.consultas:
            if consulta.categoria == "historico_fecha":
                self.assertIn("2025", consulta.pregunta, consulta.id)
                self.assertNotIn("Rev.", consulta.pregunta, consulta.id)
            if consulta.categoria == "historico_revision":
                self.assertIn("Rev. 00", consulta.pregunta, consulta.id)
                self.assertNotIn("2025", consulta.pregunta, consulta.id)


class Puntuacion(unittest.TestCase):
    def test_abstencion_con_cifras_ajenas_vale_cero_pero_se_revisa(self):
        consulta = next(c for c in rgb_industrial.PISTA.consultas if c.id == "rechazo-procedimiento")
        veredicto = consulta.puntuar("PTF-HID-77 no consta; el PTF-HID-10 fija 45 min.")
        self.assertEqual(veredicto.puntuacion, 0.0)
        self.assertEqual(veredicto.clasificacion, "cifra_no_presente")
        self.assertTrue(veredicto.dudosa)

    def test_cifra_ajena_sin_abstencion_no_es_dudosa(self):
        consulta = next(c for c in rgb_industrial.PISTA.consultas if c.id == "rechazo-linea")
        self.assertFalse(consulta.puntuar("La presión es 14,5 bar.").dudosa)

    def test_respuesta_ideal_acierta_en_todas_las_consultas(self):
        for pista in (ciclo_de_vida.PISTA, rgb_industrial.PISTA):
            for consulta in pista.consultas:
                self.assertEqual(consulta.puntuar(respuesta_ideal(consulta)).puntuacion, 1.0, consulta.id)

    def test_ids_unicos(self):
        for pista in (ciclo_de_vida.PISTA, rgb_industrial.PISTA):
            ids = [c.id for c in pista.consultas]
            self.assertEqual(len(ids), len(set(ids)))

    def test_valor_derogado_presentado_como_vigente_falla(self):
        consulta = next(c for c in ciclo_de_vida.PISTA.consultas if c.id == "vigente-presion")
        self.assertEqual(consulta.puntuar("La presión de purga es 6,5 bar (Rev. 00).").puntuacion, 0.0)

    def test_distractor_en_lugar_del_dato_falla(self):
        consulta = next(c for c in rgb_industrial.PISTA.consultas if c.id == "ruido-presion")
        self.assertEqual(consulta.puntuar("La presión de prueba es 14,5 bar.").clasificacion, "valor_prohibido")


class RunConPistas(unittest.TestCase):
    def setUp(self):
        carpeta = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.addCleanup(carpeta.cleanup)
        self.salida = Path(carpeta.name)

    def ejecutar(self, pista, respuestas, extra=()):
        ruta_respuestas = self.salida / "respuestas.json"
        ruta_respuestas.write_text(json.dumps(respuestas, ensure_ascii=False), encoding="utf-8")
        entorno = dict(os.environ, CLI_SIMULADA_RESPUESTAS=str(ruta_respuestas))
        resultado = subprocess.run(
            [sys.executable, str(RUN), "--binario", str(CLI_SIMULADA), "--pista", pista.nombre, "--modelo", "sonnet",
             "--repeticiones", "1", "--salida", str(self.salida), *extra],
            capture_output=True, text=True, encoding="utf-8", env=entorno, cwd=self.salida, timeout=300,
        )
        self.assertEqual(resultado.returncode, 0, resultado.stderr)
        informe = json.loads(next(self.salida.glob("benchmark_report_*.json")).read_text(encoding="utf-8"))
        return informe, informe["pistas"][0]

    def test_ciclo_de_vida_ideal_con_metricas_por_forma(self):
        respuestas = {c.pregunta: respuesta_ideal(c) for c in ciclo_de_vida.PISTA.consultas}
        _, seccion = self.ejecutar(ciclo_de_vida.PISTA, respuestas)
        self.assertEqual(seccion["precision_global"]["media"], 1.0)
        self.assertEqual(seccion["metricas"]["vigente"], 1.0)
        self.assertEqual(seccion["metricas"]["historico_por_fecha"], 1.0)
        self.assertEqual(seccion["metricas"]["historico_por_revision"], 1.0)
        self.assertIsNone(seccion["metricas"]["robustez_ruido"])
        self.assertEqual([i["documento"] for i in seccion["ingestas"]], ["purga_rev00.md", "purga_rev01.md"])

    def test_fallo_solo_por_numero_de_revision_se_ve_separado(self):
        respuestas = {c.pregunta: respuesta_ideal(c) for c in ciclo_de_vida.PISTA.consultas}
        for consulta in ciclo_de_vida.PISTA.consultas:
            if consulta.categoria == "historico_revision":
                respuestas[consulta.pregunta] = "El valor es 8,2 bar, 85 °C y 36 min, según la Rev. 01."
        _, seccion = self.ejecutar(ciclo_de_vida.PISTA, respuestas)
        self.assertEqual(seccion["metricas"]["historico_por_fecha"], 1.0)
        self.assertEqual(seccion["metricas"]["historico_por_revision"], 0.0)

    def test_rgb_industrial_ideal_cubre_todas_las_metricas(self):
        respuestas = {c.pregunta: respuesta_ideal(c) for c in rgb_industrial.PISTA.consultas}
        _, seccion = self.ejecutar(rgb_industrial.PISTA, respuestas)
        for metrica in ("robustez_ruido", "rechazo_estricto", "rechazo_semantico", "integracion", "exactitud_tabular"):
            self.assertEqual(seccion["metricas"][metrica], 1.0, metrica)
        self.assertEqual(len(seccion["ingestas"]), 7)

    def test_el_informe_registra_hash_de_fixtures_catalogo_y_latencia_de_ingesta(self):
        respuestas = {c.pregunta: respuesta_ideal(c) for c in ciclo_de_vida.PISTA.consultas}
        informe, seccion = self.ejecutar(ciclo_de_vida.PISTA, respuestas)
        self.assertEqual(len(informe["datasets_sha256"][ciclo_de_vida.PISTA.nombre]), 64)
        self.assertEqual(len(seccion["catalogos"]), 1)
        self.assertEqual(seccion["catalogos"][0]["repeticion"], 1)
        self.assertGreater(seccion["ingestas"][0]["latencia_ms"], 0)

    def test_piloto_limita_consultas_por_categoria_sin_reducir_ingestas(self):
        respuestas = {c.pregunta: respuesta_ideal(c) for c in rgb_industrial.PISTA.consultas}
        _, seccion = self.ejecutar(rgb_industrial.PISTA, respuestas, extra=("--max-consultas-por-categoria", "1"))
        self.assertEqual(seccion["total_consultas"], 4)
        self.assertEqual(len({r["categoria"] for r in seccion["registros"]}), 4)
        self.assertEqual(len(seccion["ingestas"]), 7)


if __name__ == "__main__":
    unittest.main()
