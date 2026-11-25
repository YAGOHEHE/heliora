"""Herramientas de análisis: validación de entradas, modelos aplicables, validación temporal y búsqueda CEC."""
from __future__ import annotations

import os
import tempfile

import numpy as np
import pandas as pd

from .banco import prueba
from .. import herramientas
from ..entradas import ErrorHerramienta, compilar_modelo
from ..espacio import Espacio
from ..lib import aprendizaje as ml
from ..lib import fotovoltaica as fv


def _espacio(**variables):
    e = Espacio()
    e.ns.update(variables)
    return e


def _error(ident, valores, espacio):
    """Mensaje de error de la herramienta, o None si se ejecutó."""
    try:
        herramientas.ejecutar(ident, valores, espacio)
    except ErrorHerramienta as e:
        return str(e)
    return None


def _datos(n=120, semilla=0):
    rng = np.random.default_rng(semilla)
    d = pd.DataFrame({"a": rng.normal(size=n), "b": rng.normal(size=n), "t": np.arange(n, dtype=float)})
    d["y"] = 2 * d["a"] - d["b"] + 0.01 * d["t"] + rng.normal(0, 0.1, n)
    d["clase"] = np.where(d["a"] + d["b"] > 0, "p", "n")
    d["texto"] = ["x"] * n
    return d


def _contiene(msg, *trozos):
    return msg is not None and all(t in msg for t in trozos)


@prueba(3, "Validación de las entradas de estadística y ML", "Casos que antes daban un error críptico o ninguno")
def entradas_ml(m):
    """Cada entrada inválida da un mensaje en español que nombra la herramienta y el parámetro."""
    d = _datos()
    e = _espacio(d=d)
    base = dict(tabla="d", y="y", x="a, b", modelo="lineal")
    m.verdad("Pliegues < 2", _contiene(_error("ml_regresion", {**base, "pliegues": 1}, e), "Regresión", "Pliegues", "mayor o igual que 2"))
    m.verdad("Pliegues > filas", _contiene(_error("ml_regresion", {**base, "pliegues": 500}, e), "Pliegues", "filas usadas"))
    m.verdad("k de clustering < 2", _contiene(_error("ml_clustering", dict(tabla="d", x="a, b", k=1), e), "Clustering", "Número de grupos"))
    m.verdad("k de clustering >= n", _contiene(_error("ml_clustering", dict(tabla="d", x="a, b", k=500), e), "Número de grupos", "filas usadas"))
    m.verdad("Contaminación fuera de (0, 0.5]", _contiene(_error("ml_anomalias", dict(tabla="d", x="a, b", contaminacion=0.9), e), "Contaminación", "0.5"))
    m.verdad("Confianza fuera de (0, 1)", _contiene(_error("es_hipotesis", dict(tabla="d", columna="a", conf=1.5), e), "Confianza", "entre 0 y 1"))
    m.verdad("λ de EWMA fuera de (0, 1]", _contiene(_error("es_spc", dict(tabla="d", columna="a", lam=0), e), "λ", "entre 0 y 1"))
    m.verdad("Periodo estacional inválido", _contiene(_error("es_serie", dict(tabla="d", columna="a", periodo=1), e), "Periodo estacional"))
    m.verdad("Periodo mayor que la serie", _contiene(_error("es_serie", dict(tabla="d", columna="a", periodo=100), e), "2 periodos"))
    m.verdad("Pasos de pronóstico inválidos", _contiene(_error("es_serie", dict(tabla="d", columna="a", periodo=4, pasos=0), e), "Pasos de pronóstico"))
    m.verdad("Columna de texto donde se espera un número", _contiene(_error("es_descriptiva", dict(tabla="d", columna="texto"), e), "«texto» no es numérica"))
    m.verdad("Columna inexistente, con las disponibles", _contiene(_error("es_descriptiva", dict(tabla="d", columna="zzz"), e), "«zzz»", "Columnas disponibles", "clase"))
    m.verdad("Objetivo entre las X", _contiene(_error("ml_regresion", {**base, "x": "a, y"}, e), "fuga de datos"))
    m.verdad("Tabla inexistente", _contiene(_error("es_descriptiva", dict(tabla="nada", columna="a"), e), "No existe", "Tablas disponibles"))
    m.verdad("Número ilegible", _contiene(_error("ml_clustering", dict(tabla="d", x="a, b", k="tres"), e), "Número de grupos", "no es un número"))
    m.verdad("Entero con decimales", _contiene(_error("ml_clustering", dict(tabla="d", x="a, b", k=2.5), e), "entero"))


@prueba(3, "n usado / n total al descartar filas con NaN", "20 filas con NaN de 120")
def n_usado(m):
    """El informe de regresión declara cuántas filas usó y cuántas descartó."""
    d = _datos()
    d.loc[:19, "a"] = np.nan
    e = _espacio(d=d)
    inf = herramientas.ejecutar("ml_regresion", dict(tabla="d", y="y", x="a, b", modelo="lineal", pliegues=3), e)
    kv = [s for s in inf["secciones"] if s["tipo"] == "kv" and s["titulo"].startswith("Validación")][0]
    m.verdad("Aparece «100 / 120»", any(f[0] == "n usado / n total" and f[1] == "100 / 120" for f in kv["items"]))
    m.verdad("Hay un aviso de filas descartadas", any(s["tipo"] == "aviso" and "100 de 120" in s["texto"] for s in inf["secciones"]))


@prueba(3, "Aplicar modelo: ida y vuelta", "Las predicciones del modelo guardado igualan a las del entrenado")
def predecir_ida_y_vuelta(m):
    """Entrena, aplica desde el espacio, guarda, recarga y vuelve a aplicar; faltan columnas = error claro."""
    d = _datos()
    e = _espacio(d=d)
    herramientas.ejecutar("ml_regresion", dict(tabla="d", y="y", x="a, b, t", modelo="ridge", pliegues=3), e)
    mod = e.ns["modelo"]
    m.verdad("El modelo guarda columnas y tarea", mod.columnas == ["a", "b", "t"] and mod.tarea == "regresion")
    herramientas.ejecutar("ml_predecir", dict(origen="espacio de trabajo", variable="modelo", tabla="d", salida="p1"), e)
    esperado = mod.pipeline.predict(d[["a", "b", "t"]].to_numpy())
    m.cota("Predicción desde el espacio", np.max(np.abs(e.ns["p1"]["prediccion"].to_numpy() - esperado)), 1e-12)
    with tempfile.TemporaryDirectory() as td:
        ruta = os.path.join(td, "m.joblib")
        herramientas.ejecutar("ml_guardar", dict(accion="guardar", variable="modelo", ruta=ruta), e)
        e2 = _espacio(d=d)
        herramientas.ejecutar("ml_predecir", dict(origen="archivo", ruta=ruta, tabla="d", salida="p2"), e2)
        m.cota("Predicción desde el archivo", np.max(np.abs(e2.ns["p2"]["prediccion"].to_numpy() - esperado)), 1e-12)
    msg = _error("ml_predecir", dict(origen="espacio de trabajo", variable="modelo", tabla="d2", salida="p3"), _espacio(d2=d[["a", "b"]], modelo=mod))
    m.verdad("Faltan columnas: error con su nombre", _contiene(msg, "falta", "«t»") or _contiene(msg, "faltan", "t"))
    m.verdad("Un estimador suelto no se acepta", _contiene(_error("ml_predecir", dict(origen="espacio de trabajo", variable="s", tabla="d", salida="p4"), _espacio(d=d, s=mod.estimador)), "no es un modelo entrenado"))


@prueba(3, "Validación temporal sin barajar", "Toda fila de validación es posterior a las de entrenamiento")
def validacion_temporal(m):
    """TimeSeriesSplit respeta el orden de la tabla o el de la columna indicada; el hold-out y los pliegues no lo hacen."""
    n = 100
    for nombre, orden in (("orden de la tabla", None), ("columna de orden invertida", np.arange(n)[::-1])):
        partes = ml.particiones(n, "temporal", 4, orden=orden)
        clave = np.arange(n) if orden is None else orden
        m.verdad(f"Temporal ({nombre}): validación posterior al entrenamiento", all(clave[tr].max() < clave[te].min() for tr, te in partes))
    m.verdad("Temporal: la primera porción nunca se valida", all(0 not in te for _, te in ml.particiones(n, "temporal", 4)))
    aleatoria = ml.particiones(n, "aleatoria (k pliegues)", 4)
    m.verdad("Aleatoria: baraja", not all(np.max(tr) < np.min(te) for tr, te in aleatoria))
    d = _datos(200)
    e = _espacio(d=d)
    herramientas.ejecutar("ml_regresion", dict(tabla="d", y="y", x="a, b", modelo="lineal", validacion="temporal", pliegues=4, guardar="mod_t", salida="val_t"), e)
    v = e.ns["val_t"]
    m.verdad("La herramienta valida solo la parte final", not v["validada"].iloc[:40].any() and v["validada"].iloc[-1])
    d["cn"] = (d["a"] > 0).astype(int)
    herramientas.ejecutar("ml_clasificacion", dict(tabla="d", y="cn", x="b, t", modelo="logistica", validacion="hold-out", pct=25), e)
    cl = e.ns["validacion_clas"]
    m.verdad("Clasificación con clases enteras y hold-out del 25 %", int(cl["validada"].sum()) == 50 and cl["prediccion"].notna().sum() == 50)
    m.verdad("Métricas de entrenamiento y validación por separado", "entrenamiento" in e.ns["mod_t"].metricas and "r2" in e.ns["mod_t"].metricas)


@prueba(3, "Modelo no lineal restringido a una lista blanca", "Sin eval abierto: atributos, importaciones y llamadas ajenas se rechazan")
def lista_blanca(m):
    """Se aceptan expresiones con NumPy y se rechazan construcciones peligrosas."""
    f = compilar_modelo("a*np.exp(-b*x)+c", "abc")
    m.error("Evalúa la expresión permitida", f(np.array([0.0]), 2.0, 0.0, 1.0)[0], 3.0, 1e-12)
    for malo in ("__import__('os').getcwd()", "x.__class__", "np.__dict__", "open('f')", "(lambda: 1)()", "np.load('a')", "[1][0]", "a if x else b"):
        try:
            compilar_modelo(malo, "abc")
            ok = False
        except ErrorHerramienta:
            ok = True
        m.verdad(f"Rechaza «{malo}»", ok)
    rng = np.random.default_rng(1)
    x = np.linspace(0, 4, 80)
    d = pd.DataFrame({"x": x, "y": 3 * np.exp(-0.7 * x) + 0.5 + rng.normal(0, 0.01, 80)})
    e = _espacio(d=d)
    herramientas.ejecutar("es_no_lineal", dict(tabla="d", x="x", y="y", expresion="a*np.exp(-b*x)+c", p0="1, 1, 0"), e)
    m.verdad("Se crea la variable de ajuste", "ajuste_no_lineal" in e.ns)
    m.verdad("Una expresión peligrosa llega como error claro", _contiene(_error("es_no_lineal", dict(tabla="d", x="x", y="y", expresion="__import__('os')", p0="1"), e), "solo se permiten funciones"))


@prueba(3, "Nombres de las variables de salida", "No se pisan np, pd, la tabla de entrada ni variables ajenas")
def nombres_salida(m):
    """Se rechazan nombres reservados, inválidos o que pisan datos; reejecutar sobre la propia salida sí vale."""
    d = _datos()
    e = _espacio(d=d, mia=3)
    base = dict(tabla="d", x="a, b", k=2)
    m.verdad("Nombre reservado", _contiene(_error("ml_clustering", {**base, "salida": "np"}, e), "reservado"))
    m.verdad("Tabla de entrada", _contiene(_error("ml_clustering", {**base, "salida": "d"}, e), "pisaría"))
    m.verdad("Variable ajena", _contiene(_error("ml_clustering", {**base, "salida": "mia"}, e), "ya existe"))
    m.verdad("Identificador inválido", _contiene(_error("ml_clustering", {**base, "salida": "2x y"}, e), "no es un nombre de variable válido"))
    m.verdad("Palabra clave", _contiene(_error("ml_clustering", {**base, "salida": "for"}, e), "no es un nombre de variable válido"))
    m.verdad("Primera ejecución", _error("ml_clustering", {**base, "salida": "g"}, e) is None)
    m.verdad("Reejecución sobre la propia salida", _error("ml_clustering", {**base, "salida": "g"}, e) is None)
    m.verdad("La etiqueta se alinea con la tabla", len(e.ns["g"]) == len(d) and set(e.ns["g"]["grupo"]) == {1, 2})


@prueba(2, "Validación de las entradas de potencia y control", "Valores físicamente imposibles")
def entradas_potencia(m):
    """Factor de potencia, potencia, bus, modulación, LCL, unidades y funciones de transferencia."""
    e = _espacio()
    m.verdad("fp fuera de (0, 1]", _contiene(_error("pot_perdidas", dict(fp=1.5), e), "Pérdidas", "Factor de potencia"))
    m.verdad("P <= 0", _contiene(_error("pot_perdidas", dict(P_kw=0), e), "Potencia de salida"))
    m.verdad("Vdc <= 0", _contiene(_error("pot_perdidas", dict(Vdc=-5), e), "Tensión del bus"))
    m.verdad("m > 1", _contiene(_error("pot_perdidas", dict(Vdc=400), e), "sobremodulación"))
    m.verdad("L1 <= 0", _contiene(_error("pot_lcl", dict(L1=0), e), "filtro LCL", "L lado inversor"))
    m.verdad("C <= 0", _contiene(_error("pot_lcl", dict(C=-1), e), "C"))
    inf = herramientas.ejecutar("pot_lcl", dict(Rd=0), e)
    m.verdad("Rd = 0 se explica como sin amortiguar", any(s["tipo"] == "aviso" and "sin amortiguar" in s["texto"] for s in inf["secciones"]))
    a = herramientas.ejecutar("pot_lcl", dict(L1="0.5 mH"), _espacio())
    b = herramientas.ejecutar("pot_lcl", dict(L1=0.5), _espacio())
    m.verdad("«0.5 mH» equivale a 0.5 en un campo en mH", a["secciones"][0] == b["secciones"][0])
    c = herramientas.ejecutar("pot_lcl", dict(L1="0.0005 H"), _espacio())
    m.verdad("«0.0005 H» se convierte a mH", c["secciones"][0] == b["secciones"][0])
    m.verdad("Unidad de otra magnitud", _contiene(_error("pot_lcl", dict(L1="5 kW"), e), "unidad", "mH"))
    m.verdad("Numerador de mayor grado", _contiene(_error("ctl_bode", dict(num="1, 2, 3, 4", den="1, 1"), e), "numerador", "grado"))
    m.verdad("K <= 0", _contiene(_error("ctl_bode", dict(k=0), e), "Ganancia del lazo"))
    m.verdad("K máx <= 0", _contiene(_error("ctl_lugar", dict(k_max=-1), e), "Ganancia máxima"))
    inf = herramientas.ejecutar("pot_perdidas", {}, _espacio())
    m.verdad("El informe de pérdidas dice qué variable creó", any(s["tipo"] == "aviso" and "eficiencia_inversor" in s["texto"] for s in inf["secciones"]))


@prueba(2, "Validación de las entradas fotovoltaicas", "Rangos del sombreado, hoja de datos, fecha y módulo desconocido")
def entradas_fv(m):
    """Sub-módulos, strings, V_mp ≥ V_oc, fecha inválida y módulo fuera de catálogo."""
    e = _espacio()
    m.verdad("Sub-módulos sombreados > total", _contiene(_error("fv_sombra", dict(Ns=2, sub=7), e), "Sub-módulos sombreados", "6"))
    m.verdad("Strings con sombra > Np", _contiene(_error("fv_sombra", dict(Np=2, n_somb=3), e), "Strings con sombra"))
    m.verdad("Ns < 1", _contiene(_error("fv_sombra", dict(Ns=0), e), "Módulos en serie"))
    m.verdad("Módulo desconocido, con opciones", _contiene(_error("fv_curvas", dict(modelo="no existe"), e), "desconocido", "CS6X-300M"))
    try:
        fv.modulo_de_catalogo("no existe")
        ok = False
    except ValueError:
        ok = True
    m.verdad("modulo_de_catalogo ya no devuelve el primero en silencio", ok)
    m.verdad("V_mp >= V_oc", _contiene(_error("fv_hoja", dict(modelo="Datos a mano", Vmp=46), e), "V_mp"))
    m.verdad("I_mp >= I_sc", _contiene(_error("fv_hoja", dict(modelo="Datos a mano", Imp=9.5), e), "I_mp"))
    m.verdad("Fecha inválida", _contiene(_error("fv_sol", dict(fecha="31/02/2026 ya"), e), "fecha", "AAAA-MM-DD"))
    m.verdad("Latitud fuera de rango", _contiene(_error("fv_sol", dict(lat=120), e), "Latitud"))
    inf = herramientas.ejecutar("fv_hoja", dict(modelo="Datos a mano", Voc=45, Vmp=38, Isc=9, Imp=8.5, Ns=72), _espacio())
    m.verdad("fv_hoja con datos a mano crea la curva y los parámetros", any(s["tipo"] == "aviso" and "iv_stc" in s["texto"] and "modulo_parametros" in s["texto"] for s in inf["secciones"]))


@prueba(2, "Búsqueda de módulos en la base CEC", "Palabras sueltas, tope de 50 y extracción de parámetros")
def busqueda_cec(m):
    """La búsqueda exige todas las palabras, limita a 50 y los módulos hallados se pueden usar en las herramientas."""
    r = fv.buscar_cec("canadian CS6X")
    m.verdad(f"Encuentra módulos Canadian Solar CS6X ({len(r)})", len(r) > 0 and all("cs6x" in n.lower() and "canadian" in n.lower() for n in r))
    m.verdad("Tope de 50 resultados", len(fv.buscar_cec("a")) == 50)
    m.verdad("Texto vacío = sin resultados", fv.buscar_cec("   ") == [])
    m.verdad("Sin coincidencias", fv.buscar_cec("zzzzqqq") == [])
    mod = fv.resolver_modulo(r[0])
    m.cota("Residuo de la extracción del módulo CEC", mod.residuo, 1e-6)
    e = _espacio()
    herramientas.ejecutar("fv_hoja", dict(modelo=r[0]), e)
    m.error("Pmp de la hoja CEC en la curva extraída", e.ns["iv_stc"]["P"].max(), mod.Pmp, 0.02, relativo=True)
    m.verdad("Módulo CEC inexistente", _contiene(_error("fv_curvas", dict(modelo="ZZZ_no_existe"), e), "desconocido"))
