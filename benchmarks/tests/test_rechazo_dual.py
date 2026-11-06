import hashlib
import unittest

from evaluacion.informe import construir_seccion_pista, renderizar_markdown
from evaluacion.puntuar import puntuar_rechazo
from evaluacion.repuntuar import RevisionInvalida, repuntuar_informe
from tests.test_informe import registro

PREGUNTA = "¿Qué presión fija la norma ZX-999?"
CON_CIFRAS = "ZX-999 no consta. La línea L-204 tiene 14,5 bar."


def registro_de_rechazo(repeticion, respuesta, identificador="rechazo-norma"):
    return registro(
        repeticion, "rechazo", 0.5, id=identificador, pregunta=PREGUNTA, respuesta=respuesta, clasificacion="antigua"
    )


def seccion(registros):
    return construir_seccion_pista(
        benchmark="Prueba",
        comparable_con_articulo=False,
        app_commit="abc",
        modelo="sonnet",
        repeticiones=1,
        registros=registros,
        ingestas=[],
    )


def informe_con(registros):
    return {"generado_en": "t", "semilla": 42, "binario": {"nombre": "b", "sha256": "0"}, "pistas": [seccion(registros)]}


def revision(repeticion, respuesta, nota=1.0, identificador="rechazo-norma"):
    return {
        "repeticion": repeticion,
        "id": identificador,
        "sha256_respuesta": hashlib.sha256(respuesta.encode("utf-8")).hexdigest(),
        "rechazo_semantico": nota,
        "justificacion": "Cifras atribuidas a otras líneas.",
    }


class RechazoSemantico(unittest.TestCase):
    def semantico(self, respuesta):
        return puntuar_rechazo(PREGUNTA, respuesta).rechazo_semantico

    def test_abstencion_sin_cifras_vale_uno(self):
        self.assertEqual(self.semantico("No consta en la documentación."), 1.0)

    def test_cifra_de_la_pregunta_no_impide_el_acierto(self):
        self.assertEqual(puntuar_rechazo("¿Y la 999?", "La 999 no consta.").rechazo_semantico, 1.0)

    def test_sin_abstencion_vale_cero(self):
        self.assertEqual(self.semantico("La presión es 14,5 bar."), 0.0)
        self.assertEqual(self.semantico("La norma es muy exigente."), 0.0)

    def test_abstencion_con_cifras_queda_pendiente_sin_adivinar(self):
        for respuesta in (CON_CIFRAS, "No consta, pero suele ser 7,5 bar."):
            self.assertIsNone(self.semantico(respuesta), respuesta)
            self.assertTrue(puntuar_rechazo(PREGUNTA, respuesta).dudosa)

    def test_cifra_en_letra_queda_pendiente(self):
        self.assertIsNone(self.semantico("No consta, aunque suelen ser cuarenta bar."))

    def test_la_nota_estricta_no_cambia(self):
        self.assertEqual(puntuar_rechazo(PREGUNTA, CON_CIFRAS).puntuacion, 0.0)
        self.assertEqual(puntuar_rechazo(PREGUNTA, "No consta.").puntuacion, 1.0)


class MetricasDeRechazo(unittest.TestCase):
    def test_con_todo_resuelto_da_las_dos_notas(self):
        metricas = seccion(
            [
                registro(1, "rechazo", 0.0, rechazo_semantico=1.0),
                registro(1, "rechazo", 1.0, rechazo_semantico=1.0),
            ]
        )["metricas"]
        self.assertEqual(metricas["rechazo_estricto"], 0.5)
        self.assertEqual(metricas["rechazo_semantico"], 1.0)
        self.assertEqual(metricas["rechazo_semantico_pendientes"], 0)
        self.assertNotIn("rechazo_negativo", metricas)

    def test_con_pendientes_la_nota_semantica_no_se_da(self):
        metricas = seccion(
            [
                registro(1, "rechazo", 0.0, rechazo_semantico=None),
                registro(1, "rechazo", 1.0, rechazo_semantico=1.0),
            ]
        )["metricas"]
        self.assertIsNone(metricas["rechazo_semantico"])
        self.assertEqual(metricas["rechazo_semantico_pendientes"], 1)

    def test_sin_consultas_de_rechazo_no_hay_nota_ni_pendientes(self):
        metricas = seccion([registro(1, "ruido", 1.0)])["metricas"]
        self.assertIsNone(metricas["rechazo_semantico"])
        self.assertEqual(metricas["rechazo_semantico_pendientes"], 0)

    def test_un_fallo_de_ejecucion_no_es_pendiente(self):
        fallida = registro(1, "rechazo", None, estado="fallo_ejecucion", fallo={"tipo": "t", "mensaje": "m"})
        self.assertEqual(seccion([fallida])["metricas"]["rechazo_semantico_pendientes"], 0)

    def test_el_markdown_avisa_de_las_pendientes(self):
        informe = informe_con([registro(1, "rechazo", 0.0, rechazo_semantico=None)])
        texto = renderizar_markdown(informe | {"datasets_sha256": {}})
        self.assertIn("rechazo_estricto", texto)
        self.assertIn("rechazo_semantico", texto)
        self.assertIn("revisión manual", texto)


class Repuntuacion(unittest.TestCase):
    def test_recalcula_sin_revision_y_deja_pendientes(self):
        nuevo = repuntuar_informe(informe_con([registro_de_rechazo(1, CON_CIFRAS)]), [])
        pista = nuevo["pistas"][0]
        self.assertEqual(pista["registros"][0]["puntuacion"], 0.0)
        self.assertIsNone(pista["registros"][0]["rechazo_semantico"])
        self.assertEqual(pista["metricas"]["rechazo_semantico_pendientes"], 1)

    def test_la_revision_manual_resuelve_y_queda_trazada(self):
        nuevo = repuntuar_informe(informe_con([registro_de_rechazo(1, CON_CIFRAS)]), [revision(1, CON_CIFRAS)])
        pista = nuevo["pistas"][0]
        self.assertEqual(pista["metricas"]["rechazo_semantico"], 1.0)
        self.assertEqual(pista["metricas"]["rechazo_estricto"], 0.0)
        self.assertEqual(pista["registros"][0]["rechazo_semantico_origen"], "revision_manual")
        self.assertEqual(pista["registros"][0]["rechazo_semantico_justificacion"], "Cifras atribuidas a otras líneas.")

    def test_revision_de_otra_respuesta_se_rechaza(self):
        with self.assertRaises(RevisionInvalida):
            repuntuar_informe(informe_con([registro_de_rechazo(1, CON_CIFRAS)]), [revision(1, "otro texto")])

    def test_revision_que_contradice_lo_resuelto_por_reglas_se_rechaza(self):
        with self.assertRaises(RevisionInvalida):
            repuntuar_informe(informe_con([registro_de_rechazo(1, "No consta.")]), [revision(1, "No consta.", nota=0.0)])

    def test_revision_huerfana_se_rechaza(self):
        with self.assertRaises(RevisionInvalida):
            repuntuar_informe(informe_con([registro_de_rechazo(1, CON_CIFRAS)]), [revision(2, CON_CIFRAS)])

    def test_nota_de_revision_fuera_de_rango_se_rechaza(self):
        with self.assertRaises(RevisionInvalida):
            repuntuar_informe(informe_con([registro_de_rechazo(1, CON_CIFRAS)]), [revision(1, CON_CIFRAS, nota=0.5)])

    def test_otras_categorias_y_fallos_no_cambian(self):
        fallida = registro(1, "rechazo", None, estado="fallo_ejecucion", fallo={"tipo": "t", "mensaje": "m"}, respuesta=None)
        ruido = registro(1, "ruido", 1.0)
        pista = repuntuar_informe(informe_con([fallida, ruido]), [])["pistas"][0]
        self.assertIsNone(pista["registros"][0]["puntuacion"])
        self.assertEqual(pista["registros"][1], ruido)
        self.assertEqual(pista["fallos_de_ejecucion"], 1)

    def test_no_altera_el_informe_original(self):
        original = informe_con([registro_de_rechazo(1, CON_CIFRAS)])
        repuntuar_informe(original, [])
        self.assertEqual(original["pistas"][0]["registros"][0]["puntuacion"], 0.5)

    def test_conserva_datos_de_seccion_e_informe(self):
        nuevo = repuntuar_informe(informe_con([registro_de_rechazo(1, CON_CIFRAS)]), [])
        self.assertEqual(nuevo["binario"], {"nombre": "b", "sha256": "0"})
        self.assertEqual(nuevo["pistas"][0]["modelo"], "sonnet")
        self.assertTrue(nuevo["repuntuado_en"])


if __name__ == "__main__":
    unittest.main()
