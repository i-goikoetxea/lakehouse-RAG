"""Agrega las puntuaciones de varias pasadas de un set oculto. Solo cifras, ids y categorías.

Uso: python -I resumen.py <preguntas.json> <puntuacion_p1.json> [<puntuacion_p2.json> ...]
                          --salida <resumen.json> [--base <resumen_base.json>]

No imprime texto de preguntas ni de documentos: lo que sale puede ir a un informe versionado.
Con `--base` compara pregunta a pregunta con otra medición (p. ej. `main`) y lista lo que
empeora: cualquier empeoramiento es un retroceso que bloquea el merge (CLAUDE.md §6).
"""

import argparse
import json
import re
import statistics
from collections import Counter, defaultdict
from pathlib import Path

ACIERTO = {"correcta", "correcta_menciona_memoria", "abstencion_correcta"}
# Orden de gravedad para comparar con la base (mayor = peor)
GRAVEDAD = {
    "correcta": 0, "abstencion_correcta": 0, "correcta_menciona_memoria": 0,
    "dudosa": 1, "abstencion_con_dato_externo": 1, "incompleta": 2,
    "abstencion_indebida": 3, "no_se_abstiene": 3, "incorrecta": 4, "respuesta_de_memoria": 4,
    "fallo_ejecucion": 5,
}
GRAVEDAD_PAGINA = {"correcta": 0, "no_aplica": 0, "parcial": 1, "sin_pagina": 2, "incorrecta": 3}  # una página falsa engaña más que ninguna


def etapa(r):
    """Dónde se rompió una respuesta fallida: ¿llegó el extracto esperado al modelo?"""
    if r["veredicto"] in ACIERTO:
        return None
    if r.get("causa"):
        return r["causa"]
    if any(i.get("etapa") == "salida_segura" for i in r.get("arnes", [])):
        return "arnes_bloqueo"
    if r.get("extracto_llego_al_modelo"):
        return "respuesta"
    return "busqueda_o_anterior" if r.get("paginas_esperadas") else "sin_extracto"


def resumir_pasada(preguntas, res):
    por_tipo = defaultdict(Counter)
    for r in res:
        por_tipo[preguntas[r["id"]]["tipo"]][r["veredicto"]] += 1
    lat = [r["latencia_s"] for r in res if r.get("latencia_s")]
    cpu = [r["cpu_previa"] for r in res if r.get("cpu_previa") is not None]
    arnes = Counter()
    for r in res:
        for i in r.get("arnes", []):
            # Agrupa por clase de motivo: sin valores citados ni números de página
            motivo = re.sub(r"«[^»]*»", "«…»", str(i.get("motivo", "?")))
            motivo = re.sub(r"p\. \d+( y en p\. \d+)*", "p. N", motivo)
            motivo = re.split(r"[;.] ", motivo)[0]
            arnes[f'{i.get("etapa", "?")}: {motivo[:80]}'] += 1
    fallos = [
        {"id": r["id"], "tipo": preguntas[r["id"]]["tipo"], "veredicto": r["veredicto"], "etapa": etapa(r)}
        for r in res if r["veredicto"] not in ACIERTO
    ]
    con_pagina = [r for r in res if r.get("pagina") not in (None, "no_aplica") and r["veredicto"] in ACIERTO]
    return {
        "n": len(res),
        "aciertos": sum(r["veredicto"] in ACIERTO for r in res),
        "veredictos": dict(Counter(r["veredicto"] for r in res)),
        "por_tipo": {t: dict(c) for t, c in sorted(por_tipo.items())},
        "fallos": fallos,
        "pagina_en_aciertos": dict(Counter(r["pagina"] for r in con_pagina)),
        "pagina_por_id": {r["id"]: r.get("pagina") for r in res},
        "veredicto_por_id": {r["id"]: r["veredicto"] for r in res},
        "trampas": {r["id"]: r["veredicto"] for r in res if preguntas[r["id"]]["tipo"] == "trampa"},
        "consultas_con_arnes": sum(bool(r.get("arnes")) for r in res),
        "intervenciones_arnes": dict(arnes.most_common()),
        "latencia_s": {"mediana": statistics.median(lat), "max": max(lat), "min": min(lat), "total_min": round(sum(lat) / 60, 1)} if lat else {},
        "cpu_previa": {"mediana": statistics.median(cpu), "max": max(cpu)} if cpu else {},
        "llamadas_herramienta_mediana": statistics.median([r.get("tool_calls", 0) for r in res]) if res else 0,
    }


def variacion(pasadas):
    ids = set(pasadas[0]["veredicto_por_id"])
    cambian = sorted(i for i in ids if len({p["veredicto_por_id"].get(i) for p in pasadas}) > 1)
    return {i: [p["veredicto_por_id"].get(i) for p in pasadas] for i in cambian}


def comparar(actual, base):
    """Pregunta a pregunta, peor veredicto de todas las pasadas frente al de la base."""
    def peor(resumen, campo, gravedad):
        res = {}
        for p in resumen["pasadas"]:
            for i, v in p[campo].items():
                if i not in res or gravedad.get(v, 9) > gravedad.get(res[i], 9):
                    res[i] = v
        return res
    out = {"empeoran": [], "mejoran": [], "pagina_empeora": [], "pagina_mejora": []}
    for campo, gravedad, emp, mej in (("veredicto_por_id", GRAVEDAD, "empeoran", "mejoran"), ("pagina_por_id", GRAVEDAD_PAGINA, "pagina_empeora", "pagina_mejora")):
        a, b = peor(actual, campo, gravedad), peor(base, campo, gravedad)
        for i in sorted(set(a) & set(b)):
            ga, gb = gravedad.get(a[i], 9), gravedad.get(b[i], 9)
            if ga > gb:
                out[emp].append({"id": i, "base": b[i], "ahora": a[i]})
            elif ga < gb:
                out[mej].append({"id": i, "base": b[i], "ahora": a[i]})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("preguntas")
    ap.add_argument("puntuaciones", nargs="+")
    ap.add_argument("--salida", required=True)
    ap.add_argument("--base")
    args = ap.parse_args()
    preguntas = {p["id"]: p for p in json.loads(Path(args.preguntas).read_text(encoding="utf-8"))}
    pasadas = [resumir_pasada(preguntas, json.loads(Path(f).read_text(encoding="utf-8"))) for f in args.puntuaciones]
    resumen = {
        "preguntas_por_tipo": dict(Counter(p["tipo"] for p in preguntas.values())),
        "pasadas": pasadas,
        "variacion_entre_pasadas": variacion(pasadas) if len(pasadas) > 1 else {},
    }
    if args.base:
        resumen["comparacion_con_base"] = comparar(resumen, json.loads(Path(args.base).read_text(encoding="utf-8")))
    Path(args.salida).write_text(json.dumps(resumen, ensure_ascii=False, indent=1), encoding="utf-8")
    for k, p in enumerate(pasadas, 1):
        print(f"Pasada {k}: {p['aciertos']}/{p['n']} aciertos; veredictos {p['veredictos']}")
        print("  fallos:", [(f["id"], f["tipo"], f["veredicto"], f["etapa"]) for f in p["fallos"]])
        print("  página en aciertos:", p["pagina_en_aciertos"], "| trampas:", p["trampas"])
        print("  arnés:", p["consultas_con_arnes"], "consultas;", p["intervenciones_arnes"])
        print("  latencia:", p["latencia_s"], "| cpu previa:", p["cpu_previa"])
    print("Variación entre pasadas:", resumen["variacion_entre_pasadas"])
    if args.base:
        c = resumen["comparacion_con_base"]
        print("RETROCESOS frente a la base:", c["empeoran"] or "ninguno", "| página:", c["pagina_empeora"] or "ninguno")
        print("Mejoras:", c["mejoran"], "| página:", c["pagina_mejora"])
        return 1 if c["empeoran"] or c["pagina_empeora"] else 0
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
