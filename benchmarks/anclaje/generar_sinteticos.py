"""Casos sintéticos para la medición N2 del anclaje de tablas (ssd_generalidad_4_piezas.md §6.3).

Genera texto paginado (página, línea) con tablas de varios idiomas, dominios, tamaños y
estructuras, y la página válida de cada fila por construcción. Lo lee
`cargo run --example medir_anclaje -- sinteticos <casos.json> <salida.jsonl>`.

Sesgo: los casos los diseña la misma sesión que escribe el algoritmo. Un 100 % aquí significa
«funciona en los casos conocidos», no «funciona en general» (CLAUDE.md §1).

Uso: python benchmarks/anclaje/generar_sinteticos.py <casos.json> [semilla]
"""

import json
import random
import sys

# Vocabulario por idioma: (filas, columnas, palabras de prosa). Solo datos de prueba.
IDIOMAS = {
    "es": (["Acero S275", "Acero S355", "Aluminio 6061", "Cobre C110", "Latón CW617", "Titanio G2",
            "Inoxidable 304", "Inoxidable 316", "Fundición GJL", "Zinc Z1"],
           ["Espesor fino", "Espesor medio", "Espesor grueso", "Mínimo", "Máximo"],
           "el procedimiento exige revisar los valores de la tabla antes de cada turno de trabajo".split()),
    "en": (["North America", "Western Europe", "Asia Pacific", "Latin America", "Middle East",
            "Eastern Europe", "Oceania", "Southern Africa", "Nordics", "Caribbean"],
           ["Q1", "Q2", "Q3", "Q4", "FY"],
           "revenue grew in most regions while costs remained stable across the reporting period".split()),
    "de": (["Kinder bis 6", "Kinder bis 12", "Jugendliche", "Erwachsene", "Senioren",
            "Schwangere", "Stillende", "Leistungssportler", "Patienten", "Pflegebedürftige"],
           ["Morgens", "Mittags", "Abends", "Tagesdosis", "Höchstdosis"],
           "die dosierung richtet sich nach dem körpergewicht und der ärztlichen anweisung".split()),
    "el": (["Χάλυβας", "Αλουμίνιο", "Χαλκός", "Ορείχαλκος", "Τιτάνιο", "Ψευδάργυρος",
            "Νικέλιο", "Μόλυβδος", "Κασσίτερος", "Μαγνήσιο"],
           ["Ελάχιστη", "Μέγιστη", "Μέση", "Όριο", "Ανοχή"],
           "ο πίνακας δείχνει τις θερμοκρασίες λειτουργίας για κάθε υλικό της εγκατάστασης".split()),
    "ru": (["Сталь", "Алюминий", "Медь", "Латунь", "Титан", "Цинк", "Никель", "Свинец",
            "Олово", "Магний"],
           ["Минимум", "Максимум", "Среднее", "Допуск", "Предел"],
           "таблица содержит значения температуры для каждого материала установки".split()),
    "zh": (["钢材", "铝材", "铜材", "黄铜", "钛材", "锌材", "镍材", "铅材", "锡材", "镁材"],
           ["最低温度", "最高温度", "平均温度", "允许偏差", "极限值"],
           ["本表", "列出", "各种", "材料", "的", "工作", "温度", "范围"]),
    "ar": (["الصلب", "الألومنيوم", "النحاس", "النحاس الأصفر", "التيتانيوم", "الزنك", "النيكل",
            "الرصاص", "القصدير", "المغنيسيوم"],
           ["الحد الأدنى", "الحد الأقصى", "المتوسط", "التفاوت", "الحد"],
           "يوضح الجدول درجات حرارة التشغيل لكل مادة في المنشأة".split()),
    "hi": (["इस्पात", "एल्युमिनियम", "तांबा", "पीतल", "टाइटेनियम", "जस्ता", "निकल", "सीसा",
            "टिन", "मैग्नीशियम"],
           ["न्यूनतम", "अधिकतम", "औसत", "सहनशीलता", "सीमा"],
           "यह तालिका प्रत्येक सामग्री के लिए संचालन तापमान दिखाती है".split()),
}

SIN_ESPACIOS = {"zh"}


def unir(idioma, partes):
    return ("" if idioma in SIN_ESPACIOS else " ").join(partes)


def valor(rnd, decimal_coma):
    if rnd.random() < 0.5:
        texto = str(rnd.randint(1, 400))
    else:
        texto = f"{rnd.randint(0, 99)}.{rnd.randint(0, 9)}"
    return texto, (texto.replace(".", ",") if decimal_coma else texto)


def prosa(rnd, idioma, numeros):
    _, _, palabras = IDIOMAS[idioma]
    partes = rnd.sample(palabras, min(len(palabras), 6))
    if numeros:
        partes.insert(rnd.randint(0, len(partes)), rnd.choice(numeros))
    return unir(idioma, partes)


def tabla_base(rnd, idioma, n_filas, n_columnas, claves_numericas=False):
    filas_vocab, columnas_vocab, _ = IDIOMAS[idioma]
    columnas = columnas_vocab[:n_columnas]
    if claves_numericas:
        claves = [str(v) for v in sorted(rnd.sample(range(4, 60), n_filas))]
    else:
        claves = filas_vocab[:n_filas] if n_filas <= len(filas_vocab) else [
            f"{filas_vocab[i % len(filas_vocab)]} {i // len(filas_vocab) + 1}" for i in range(n_filas)]
    decimal_coma = idioma in {"es", "de", "el", "ru"} and rnd.random() < 0.7
    filas = []
    for clave in claves:
        pares = [valor(rnd, decimal_coma) for _ in columnas]
        filas.append({"clave": [clave], "valores": [p[0] for p in pares], "texto": [p[1] for p in pares]})
    return columnas, filas


def linea_fila(idioma, fila):
    """Las celdas de una fila van separadas por blancos en cualquier escritura"""
    return " ".join([fila["clave"][0]] + fila["texto"])


def caso(id_caso, tipo, idioma, lineas, columnas, filas, validas):
    return {
        "id": id_caso, "tipo": tipo, "idioma": idioma, "lineas": lineas, "cabeceras": columnas,
        "filas": [{"clave": f["clave"], "valores": f["valores"], "paginas_validas": v}
                  for f, v in zip(filas, validas)],
    }


def con_prosa(rnd, idioma, lineas, paginas, numeros):
    """Añade párrafos de relleno en las páginas indicadas, con cifras de la tabla"""
    for pagina in paginas:
        for _ in range(rnd.randint(2, 6)):
            lineas.append((pagina, prosa(rnd, idioma, numeros)))


def generar(rnd, n):
    casos = []
    idiomas = list(IDIOMAS)
    tipos = ["una_pagina", "partida", "intro_lejana", "numeros_repetidos", "anexo_duplicado",
             "transpuesta", "fila_inventada", "rangos", "grande", "prosa_con_cifras"]
    for i in range(n):
        tipo = tipos[i % len(tipos)]
        idioma = idiomas[(i // len(tipos)) % len(idiomas)]
        p0 = rnd.randint(20, 200)
        lineas = []
        if tipo == "una_pagina":
            columnas, filas = tabla_base(rnd, idioma, rnd.randint(3, 8), rnd.randint(2, 5))
            con_prosa(rnd, idioma, lineas, [p0 - 1], [])
            lineas.append((p0, unir(idioma, ["#"] + columnas)))
            lineas += [(p0, linea_fila(idioma, f)) for f in filas]
            con_prosa(rnd, idioma, lineas, [p0 + 1], [])
            validas = [[p0]] * len(filas)
        elif tipo in ("partida", "grande"):
            n_filas = rnd.randint(6, 10) if tipo == "partida" else rnd.randint(25, 40)
            columnas, filas = tabla_base(rnd, idioma, n_filas, rnd.randint(2, 5))
            n_paginas = 2 if tipo == "partida" else 3
            cortes = sorted(rnd.sample(range(1, n_filas), n_paginas - 1))
            validas = []
            inicio = 0
            for k, fin in enumerate(cortes + [n_filas]):
                pagina = p0 + k
                lineas.append((pagina, unir(idioma, ["#"] + columnas)))
                for f in filas[inicio:fin]:
                    lineas.append((pagina, linea_fila(idioma, f)))
                    validas.append([pagina])
                inicio = fin
        elif tipo == "intro_lejana":
            columnas, filas = tabla_base(rnd, idioma, rnd.randint(4, 8), rnd.randint(2, 4))
            f = rnd.choice(filas)
            lineas.append((p0 - rnd.randint(5, 15), unir(idioma, [prosa(rnd, idioma, []), f["clave"][0], f["texto"][0]])))
            lineas.append((p0, unir(idioma, ["#"] + columnas)))
            lineas += [(p0, linea_fila(idioma, f)) for f in filas]
            validas = [[p0]] * len(filas)
        elif tipo == "numeros_repetidos":
            columnas, filas = tabla_base(rnd, idioma, rnd.randint(6, 10), 2, claves_numericas=True)
            # los valores reutilizan las claves de otras filas: los números se repiten entre páginas
            claves = [f["clave"][0] for f in filas]
            for f in filas:
                f["valores"] = rnd.sample(claves, 2)
                f["texto"] = list(f["valores"])
            corte = len(filas) // 2
            validas = []
            for k, trozo in enumerate([filas[:corte], filas[corte:]]):
                lineas.append((p0 + k, unir(idioma, ["#"] + columnas)))
                for f in trozo:
                    lineas.append((p0 + k, linea_fila(idioma, f)))
                    validas.append([p0 + k])
        elif tipo == "anexo_duplicado":
            columnas, filas = tabla_base(rnd, idioma, rnd.randint(3, 6), rnd.randint(2, 4))
            p1 = p0 + rnd.randint(10, 40)
            for pagina in (p0, p1):
                lineas.append((pagina, unir(idioma, ["#"] + columnas)))
                lineas += [(pagina, linea_fila(idioma, f)) for f in filas]
            validas = [[p0, p1]] * len(filas)
        elif tipo == "transpuesta":
            # las «filas» destiladas aparecen como columnas del texto, en dos páginas
            columnas, filas = tabla_base(rnd, idioma, rnd.randint(3, 5), rnd.randint(4, 5))
            corte = len(columnas) // 2
            for k, rango in enumerate([range(0, corte), range(corte, len(columnas))]):
                pagina = p0 + k
                lineas.append((pagina, unir(idioma, ["#"] + [f["clave"][0] for f in filas])))
                for c in rango:
                    lineas.append((pagina, unir(idioma, [columnas[c]] + [f["texto"][c] for f in filas])))
            # cada fila destilada tiene celdas en las dos páginas: ninguna página única es válida
            validas = [[]] * len(filas)
        elif tipo == "fila_inventada":
            columnas, filas = tabla_base(rnd, idioma, rnd.randint(4, 8), rnd.randint(2, 4))
            lineas.append((p0, unir(idioma, ["#"] + columnas)))
            lineas += [(p0, linea_fila(idioma, f)) for f in filas[:-1]]
            con_prosa(rnd, idioma, lineas, [p0 + 1], filas[-1]["texto"])
            validas = [[p0]] * (len(filas) - 1) + [[]]
        elif tipo == "rangos":
            columnas, _ = tabla_base(rnd, idioma, 1, rnd.randint(2, 4))
            limites = sorted(rnd.sample(range(5, 300), 4))
            filas = []
            textos = [f"≤ {limites[0]}", f"{limites[0]} – {limites[1]}", f"{limites[1]} – {limites[2]}",
                      f"{limites[2]} – {limites[3]}", f"> {limites[3]}"]
            claves = [[str(limites[0])], [str(limites[0]), str(limites[1])], [str(limites[1]), str(limites[2])],
                      [str(limites[2]), str(limites[3])], [str(limites[3])]]
            lineas.append((p0, unir(idioma, ["#"] + columnas)))
            for clave, texto in zip(claves, textos):
                pares = [valor(rnd, False) for _ in columnas]
                filas.append({"clave": clave, "valores": [p[0] for p in pares], "texto": [p[1] for p in pares]})
                lineas.append((p0, " ".join([texto] + [p[1] for p in pares])))
            validas = [[p0]] * len(filas)
        else:  # prosa_con_cifras: tabla de 2 páginas rodeada de prosa con sus cifras
            columnas, filas = tabla_base(rnd, idioma, rnd.randint(6, 10), rnd.randint(2, 4))
            numeros = [t for f in filas for t in f["texto"]]
            con_prosa(rnd, idioma, lineas, [p0 - 1], numeros)
            corte = len(filas) // 2
            validas = []
            for k, trozo in enumerate([filas[:corte], filas[corte:]]):
                pagina = p0 + k
                con_prosa(rnd, idioma, lineas, [pagina], numeros)
                lineas.append((pagina, unir(idioma, ["#"] + columnas)))
                for f in trozo:
                    lineas.append((pagina, linea_fila(idioma, f)))
                    validas.append([pagina])
                con_prosa(rnd, idioma, lineas, [pagina], numeros)
            con_prosa(rnd, idioma, lineas, [p0 + 2], numeros)
        lineas.sort(key=lambda par: par[0])
        casos.append(caso(f"{tipo}-{idioma}-{i}", tipo, idioma, lineas, columnas, filas, validas))
    return casos


def main():
    destino = sys.argv[1]
    semilla = int(sys.argv[2]) if len(sys.argv) > 2 else 20261111
    casos = generar(random.Random(semilla), 400)
    with open(destino, "w", encoding="utf-8") as f:
        json.dump(casos, f, ensure_ascii=False)
    print(f"{len(casos)} casos, {sum(len(c['filas']) for c in casos)} filas -> {destino}")


if __name__ == "__main__":
    main()
