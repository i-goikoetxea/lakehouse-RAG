import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from lakehouse.cli import AislamientoInvalido, ClienteCli, FalloEjecucion, comando_de_binario
from lakehouse.mcp import ClienteMcp

CLI_SIMULADA = Path(__file__).with_name("cli_simulada.py")


def en_modo(modo):
    return mock.patch.dict(os.environ, {"CLI_SIMULADA_MODO": modo})


class BaseCliente(unittest.TestCase):
    def setUp(self):
        carpeta = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.addCleanup(carpeta.cleanup)
        self.raiz = Path(carpeta.name)
        self.comando = comando_de_binario(CLI_SIMULADA)
        self.cliente = ClienteCli(self.comando, self.raiz)

    def centro_activo(self):
        self.cliente.crear_centro("bench-a", self.raiz / "centros" / "bench-a")
        self.cliente.activar("bench-a")


class ClienteCliContrato(BaseCliente):
    def test_flujo_completo_y_utf8(self):
        self.centro_activo()
        documento = self.raiz / "ñandú con espacios.md"
        documento.write_text("# doc", encoding="utf-8")
        ingesta = self.cliente.ingestar(documento, "bench-a")
        self.assertEqual(ingesta["estado_contenido"], "COMPLETO")
        respuesta = self.cliente.preguntar("¿Cuál es la presión de L-7?", "bench-a", "haiku")
        self.assertIn("presión", respuesta.texto)
        self.assertEqual(respuesta.modelo, "haiku")
        self.assertEqual(respuesta.herramientas[0]["nombre"], "search_normative_context")
        self.assertGreaterEqual(respuesta.latencia_ms, 0)

    def test_la_telemetria_del_arnes_llega_con_la_respuesta(self):
        self.centro_activo()
        sin_intervencion = self.cliente.preguntar("¿Cuál es la presión de L-7?", "bench-a", "haiku")
        self.assertEqual(sin_intervencion.arnes, [])
        with en_modo("arnes_reintento"):
            respuesta = self.cliente.preguntar("¿Cuál es la presión de L-7?", "bench-a", "haiku")
        self.assertEqual(respuesta.arnes, [{"etapa": "reintento", "motivo": "cifra sin anclar"}])

    def test_comillas_y_metacaracteres_no_pasan_por_shell(self):
        self.centro_activo()
        centinela = self.raiz / "inyectado"
        pregunta = f'L-7 " & echo x > "{centinela}" ; $(whoami) | %PATH%'
        self.assertIn("presión", self.cliente.preguntar(pregunta, "bench-a", "haiku").texto)
        self.assertFalse(centinela.exists())

    def test_valores_que_la_app_leeria_como_bandera_se_rechazan_sin_lanzar(self):
        self.centro_activo()
        for valor in ("--help", "--json", "--ingest", "--centro", "-h", "-m", "", "   ", "con\0nulo"):
            with self.assertRaises(FalloEjecucion) as contexto:
                self.cliente.preguntar(valor, "bench-a", "haiku")
            self.assertEqual(contexto.exception.tipo, "argumento_invalido", repr(valor))

    def test_averias_se_traducen_a_un_tipo_de_fallo(self):
        self.centro_activo()
        casos = {
            "sesion_caducada": "sesion_claude",
            "login_en_respuesta": "sesion_claude",
            "json_roto": "salida_no_json",
            "ok_falso_codigo_cero": "ok_falso",
            "muere_con_ok": "codigo_salida",
            "respuesta_vacia": "respuesta_vacia",
        }
        for modo, tipo in casos.items():
            with self.subTest(modo=modo), en_modo(modo):
                with self.assertRaises(FalloEjecucion) as contexto:
                    self.cliente.preguntar("L-7", "bench-a", "haiku")
                self.assertEqual(contexto.exception.tipo, tipo)

    def test_binario_inexistente_es_fallo_tipado(self):
        cliente = ClienteCli([str(self.raiz / "no_existe.exe")], self.raiz)
        with self.assertRaises(FalloEjecucion) as contexto:
            cliente.preguntar("x", "bench-a", "haiku")
        self.assertEqual(contexto.exception.tipo, "arranque")

    def test_timeout_devuelve_el_control_a_tiempo(self):
        self.centro_activo()
        cliente = ClienteCli(self.comando, self.raiz, timeout_consulta_s=1)
        for modo in ("cuelga", "nieto_colgado"):
            with self.subTest(modo=modo), en_modo(modo):
                inicio = time.monotonic()
                with self.assertRaises(FalloEjecucion) as contexto:
                    cliente.preguntar("L-7", "bench-a", "haiku")
                self.assertEqual(contexto.exception.tipo, "timeout")
                self.assertLess(time.monotonic() - inicio, 15)

    def test_el_mensaje_de_error_queda_acotado(self):
        fallo = FalloEjecucion("codigo_salida", "x" * 100000)
        self.assertLess(len(fallo.mensaje), 2100)


class Aislamiento(BaseCliente):
    def test_la_variable_del_padre_no_se_filtra(self):
        with mock.patch.dict(os.environ, {"LAKEHOUSE_RAG_CONFIG_DIR": str(self.raiz / "real")}):
            self.cliente.crear_centro("bench-a", self.raiz / "centros" / "bench-a")
        self.assertTrue((self.raiz / "centers_registry.json").exists())
        self.assertFalse((self.raiz / "real").exists())

    def test_rechaza_el_directorio_real_de_la_app(self):
        with mock.patch.dict(os.environ, {"APPDATA": str(self.raiz)}):
            for directorio in (self.raiz / "lakehouse_rag", self.raiz / "lakehouse_rag" / "sub"):
                directorio.mkdir(parents=True, exist_ok=True)
                with self.assertRaises(AislamientoInvalido):
                    ClienteCli(self.comando, directorio)

    def test_rechaza_directorio_inexistente(self):
        with self.assertRaises(AislamientoInvalido):
            ClienteCli(self.comando, self.raiz / "falta")

    def test_un_centro_fuera_de_la_carpeta_aislada_se_rechaza(self):
        with self.assertRaises(AislamientoInvalido):
            self.cliente.crear_centro("bench-a", self.raiz.parent / "fuera")
        with self.assertRaises(AislamientoInvalido):
            self.cliente.crear_centro("bench-a", self.raiz / ".." / "fuera")


class ComandoDeBinario(unittest.TestCase):
    def test_script_python_usa_el_interprete_y_binario_va_directo(self):
        self.assertEqual(comando_de_binario(Path("x.py").resolve())[0], sys.executable)
        self.assertEqual(comando_de_binario(Path("x.exe").resolve()), [str(Path("x.exe").resolve())])


class ClienteMcpContrato(BaseCliente):
    def abrir(self, **opciones):
        cliente = ClienteMcp(self.comando, self.raiz, **opciones)
        self.addCleanup(cliente.cerrar)
        return cliente

    def test_handshake_listado_y_llamada(self):
        cliente = self.abrir()
        cliente.iniciar()
        self.assertIn("search_normative_context", cliente.listar_herramientas())
        resultado = cliente.llamar_herramienta("search_normative_context", {"centro_id": "a", "consulta": "x"})
        self.assertEqual(resultado.contenido["resultados"][0]["texto"], "fragmento")
        self.assertGreaterEqual(resultado.latencia_ms, 0)

    def test_ignora_ruido_de_stdout_y_respuestas_de_otro_id(self):
        with en_modo("mcp_basura"):
            cliente = self.abrir()
            cliente.iniciar()
            resultado = cliente.llamar_herramienta("search_normative_context", {})
        self.assertIn("resultados", resultado.contenido)

    def test_averias_se_traducen_a_un_tipo_de_fallo(self):
        casos = {
            "mcp_error_herramienta": "herramienta_error",
            "mcp_protocolo": "mcp_error",
            "mcp_muere": "mcp_cerrado",
            "mcp_cuelga": "timeout",
        }
        for modo, tipo in casos.items():
            with self.subTest(modo=modo), en_modo(modo):
                cliente = self.abrir(timeout_llamada_s=1)
                cliente.iniciar()
                with self.assertRaises(FalloEjecucion) as contexto:
                    cliente.llamar_herramienta("search_normative_context", {})
                self.assertEqual(contexto.exception.tipo, tipo)
                cliente.cerrar()

    def test_tras_un_timeout_las_siguientes_llamadas_no_vuelven_a_esperar(self):
        with en_modo("mcp_cuelga"):
            cliente = self.abrir(timeout_llamada_s=1)
            cliente.iniciar()
            with self.assertRaises(FalloEjecucion):
                cliente.llamar_herramienta("search_normative_context", {})
            inicio = time.monotonic()
            with self.assertRaises(FalloEjecucion) as contexto:
                cliente.llamar_herramienta("search_normative_context", {})
        self.assertEqual(contexto.exception.tipo, "mcp_cerrado")
        self.assertLess(time.monotonic() - inicio, 0.5)

    def test_binario_inexistente_es_fallo_tipado(self):
        cliente = ClienteMcp([str(self.raiz / "no_existe.exe")], self.raiz)
        with self.assertRaises(FalloEjecucion) as contexto:
            cliente.iniciar()
        self.assertEqual(contexto.exception.tipo, "arranque")

    def test_cerrar_dos_veces_es_seguro(self):
        cliente = self.abrir()
        cliente.iniciar()
        cliente.cerrar()
        cliente.cerrar()


if __name__ == "__main__":
    unittest.main()
