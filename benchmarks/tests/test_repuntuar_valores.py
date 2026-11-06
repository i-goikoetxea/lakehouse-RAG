import unittest

from evaluacion.puntuar import ValorEsperado
from evaluacion.repuntuar import RevisionInvalida, consultas_de_pistas, repuntuar_informe
from pistas.base import Consulta
from tests.test_informe import registro
from tests.test_rechazo_dual import informe_con, registro_de_rechazo, revision

PREGUNTA = "¿Qué temperatura fijaba la Rev. 00?"
HISTORICA = Consulta(
    "revision-temperatura",
    "historico_revision",
    PREGUNTA,
    esperados=(ValorEsperado("72", "°C"), ValorEsperado.revision(0)),
    prohibidos=(ValorEsperado("85", "°C"),),
)
POR_BENCHMARK = {"Prueba": {HISTORICA.id: HISTORICA}}
CORRECTA_CON_MENCION = "Valor: 72 °C (Rev. 00).\n- La Rev. 01 (vigente) fija 85 °C."


def registro_historico(respuesta, **extra):
    campos = {
        "id": "revision-temperatura",
        "pregunta": PREGUNTA,
        "respuesta": respuesta,
        "clasificacion": "valor_prohibido",
        "dudosa": True,
    }
    puntuacion = extra.pop("puntuacion", 0.0)
    return registro(1, "historico_revision", puntuacion, **(campos | extra))


class RepuntuacionDeValores(unittest.TestCase):
    def test_con_las_consultas_repuntua_los_valores(self):
        informe = informe_con([registro_historico(CORRECTA_CON_MENCION)])
        nuevo = repuntuar_informe(informe, [], POR_BENCHMARK)
        repuntuado = nuevo["pistas"][0]["registros"][0]
        self.assertEqual(repuntuado["puntuacion"], 1.0)
        self.assertEqual(repuntuado["clasificacion"], "acierto_con_mencion")
        self.assertFalse(repuntuado["dudosa"])
        self.assertEqual(nuevo["pistas"][0]["metricas"]["historico_por_revision"], 1.0)

    def test_sin_las_consultas_los_valores_no_cambian(self):
        original = registro_historico(CORRECTA_CON_MENCION)
        nuevo = repuntuar_informe(informe_con([original]), [])
        self.assertEqual(nuevo["pistas"][0]["registros"][0], original)

    def test_consulta_desconocida_se_rechaza(self):
        with self.assertRaises(RevisionInvalida):
            repuntuar_informe(informe_con([registro_historico(CORRECTA_CON_MENCION, id="otra")]), [], POR_BENCHMARK)

    def test_pregunta_cambiada_se_rechaza(self):
        cambiada = registro_historico(CORRECTA_CON_MENCION, pregunta="¿Otra pregunta?")
        with self.assertRaises(RevisionInvalida):
            repuntuar_informe(informe_con([cambiada]), [], POR_BENCHMARK)

    def test_los_fallos_de_ejecucion_no_se_puntuan(self):
        fallida = registro_historico(None, estado="fallo_ejecucion", puntuacion=None, fallo={"tipo": "t", "mensaje": "m"})
        nuevo = repuntuar_informe(informe_con([fallida]), [], POR_BENCHMARK)
        self.assertIsNone(nuevo["pistas"][0]["registros"][0]["puntuacion"])

    def test_las_consultas_de_las_pistas_reales_se_indexan_por_benchmark(self):
        consultas = consultas_de_pistas()
        self.assertIn("revision-temperatura", consultas["Ciclo-de-vida"])
        self.assertIn("tabular-esquina", consultas["RGB-Industrial"])


class RevisionYaResueltaPorReglas(unittest.TestCase):
    def test_la_revision_concordante_con_las_reglas_queda_trazada(self):
        nuevo = repuntuar_informe(informe_con([registro_de_rechazo(1, "No consta.")]), [revision(1, "No consta.")])
        repuntuado = nuevo["pistas"][0]["registros"][0]
        self.assertEqual(repuntuado["rechazo_semantico"], 1.0)
        self.assertEqual(repuntuado["rechazo_semantico_origen"], "reglas")
        self.assertTrue(repuntuado["revision_manual_concordante"])

    def test_la_revision_que_contradice_a_las_reglas_se_rechaza(self):
        with self.assertRaises(RevisionInvalida):
            repuntuar_informe(informe_con([registro_de_rechazo(1, "No consta.")]), [revision(1, "No consta.", nota=0.0)])


if __name__ == "__main__":
    unittest.main()
