"""Manifiesto de una medición del set oculto: qué se midió, con qué y dónde están los datos crudos.

Uso: python -I manifiesto.py --etiqueta <et> --commit <sha> [--datos <carpeta del set>] [--centro set-oculto]

Escribe <datos>/manifiesto_<et>.json con el commit y los sha256 del binario y de las DLL, la ruta
del centro aislado, el doc_id y el sha256 de cada PDF ingerido, los modelos resueltos, los
agregados por pasada (aciertos, procesos `claude -p`, tokens, coste, microllamadas a Haiku,
latencias y CPU) y la lista de ficheros crudos. No contiene texto de preguntas ni de documentos.
"""

import argparse
import hashlib
import json
import statistics
from pathlib import Path

AQUI = Path(__file__).resolve().parent
REPO = AQUI.parents[1]


def sha256(ruta):
    return hashlib.sha256(Path(ruta).read_bytes()).hexdigest()


def agregados(metricas, puntuacion):
    m = json.loads(Path(metricas).read_text(encoding="utf-8")) if Path(metricas).exists() else []
    p = json.loads(Path(puntuacion).read_text(encoding="utf-8")) if Path(puntuacion).exists() else []
    lat = [x["latencia_ms"] for x in m if x.get("latencia_ms")]
    sumar = lambda campo: round(sum(x.get(campo) or 0 for x in m), 6)  # noqa: E731
    haiku = [d for x in m for d in x.get("haiku_latencia_ms", []) if d]
    return {
        "preguntas": len(p),
        "aciertos": sum(r["veredicto"] in ("correcta", "correcta_menciona_memoria", "abstencion_correcta") for r in p),
        "procesos_claude": sum(x.get("procesos_claude", 0) for x in m),
        "procesos_por_pregunta_mediana": statistics.median([x.get("procesos_claude", 0) for x in m]) if m else None,
        "coste_usd": sumar("coste_usd"),
        "input_tokens": sumar("input_tokens"),
        "output_tokens": sumar("output_tokens"),
        "cache_creation_input_tokens": sumar("cache_creation_input_tokens"),
        "cache_read_input_tokens": sumar("cache_read_input_tokens"),
        "haiku_llamadas_completadas": sum(x.get("haiku_llamadas", 0) for x in m),
        "haiku_latencia_ms_mediana": statistics.median(haiku) if haiku else None,
        "reintentos": sum(x.get("reintentos", 0) for x in m),
        "bloqueos": sum(x.get("bloqueos", 0) for x in m),
        "latencia_ms_mediana": statistics.median(lat) if lat else None,
        "latencia_ms_total": sum(lat),
        "cpu_previa_mediana": statistics.median([x["cpu_previa"] for x in m if x.get("cpu_previa") is not None]) if m else None,
        "modelos_resueltos": sorted({mod for x in m for mod in x.get("modelos_resueltos", [])}),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--etiqueta", required=True)
    ap.add_argument("--commit", required=True)
    ap.add_argument("--datos", default=str(REPO / "docs" / "validacion" / "set_oculto_v2"))
    ap.add_argument("--centro", default="set-oculto")
    ap.add_argument("--pasadas", type=int, default=2)
    a = ap.parse_args()
    d, et = Path(a.datos), a.etiqueta
    bin_dir = d / f"bin_{et}"
    ingesta = [json.loads(l) for l in (d / f"ingesta_{et}.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()]
    manifiesto = {
        "etiqueta": et,
        "commit": a.commit,
        "binario": {f.name: sha256(f) for f in sorted(bin_dir.glob("*")) if f.suffix in (".exe", ".dll")},
        "centro": {"id": a.centro, "directorio_aislado": str(d / f"aislado_{et}"), "ruta": str(d / f"aislado_{et}" / "centros" / a.centro)},
        "documentos": [{"archivo": x.get("archivo"), "doc_id": x.get("doc_id"), "sha256": x.get("sha256"),
                        "estado_contenido": x.get("estado_contenido"), "fallo": x.get("fallo"), "segundos": x.get("segundos")} for x in ingesta],
        "pasadas": {f"p{k}": agregados(d / f"metricas_{et}_p{k}.json", d / f"puntuacion_{et}_p{k}.json") for k in range(1, a.pasadas + 1)},
        "ficheros_crudos": sorted(str(f.relative_to(d)) for f in d.glob(f"*{et}*") if f.is_file()),
        "trazas_claude": sorted(str(f.relative_to(d)) for f in d.glob(f"crudas_{et}_p*") if f.is_dir()),
        "repetir_con_otro_adaptador": (
            f"python -I benchmarks/set_oculto_v2/ejecutar.py preguntar --binario <nuevo>/lakehouse_rag_cli.exe "
            f"--aislado {d / f'aislado_{et}'} --set {d / 'preguntas.json'} --salida {d / 'crudas_<nueva>_p1.jsonl'} "
            f"--modelo sonnet [--envoltorio-claude {d / 'envoltorio_claude'}]"
        ),
    }
    salida = d / f"manifiesto_{et}.json"
    salida.write_text(json.dumps(manifiesto, ensure_ascii=False, indent=1), encoding="utf-8")
    print(salida)
    print(json.dumps(manifiesto["pasadas"], ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
