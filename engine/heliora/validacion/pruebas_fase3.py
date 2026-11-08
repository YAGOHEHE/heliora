"""Fase 3: estadística y aprendizaje automático con datos de parámetros conocidos."""
from __future__ import annotations

import os
import tempfile

import numpy as np
import pandas as pd
from scipy import stats

from .banco import prueba
from ..lib import aprendizaje as ml
from ..lib import estadistica as es


@prueba(3, "Estadística descriptiva", "Valores exactos de 1..9")
def descriptiva(m):
    """Media = 5, mediana = 5, varianza = 7.5."""
    r = es.descriptiva(np.arange(1, 10))
    m.error("Media", r["media"], 5.0, 1e-12)
    m.error("Varianza", r["var"], 7.5, 1e-12)
    m.error("Mediana", r["mediana"], 5.0, 1e-12)
    m.error("Asimetría", r["asimetria"], 0.0, 1e-12)


@prueba(3, "Ajuste de Weibull", "Muestra simulada con β = 2.5, η = 1000")
def weibull(m):
    """Con censura por la derecha, recupera los parámetros dentro de su intervalo de confianza."""
    rng = np.random.default_rng(42)
    t = 1000 * rng.weibull(2.5, 2000)
    lim = 1400.0
    c = t > lim
    t2 = np.minimum(t, lim)
    r = es.weibull_fit(t2, c)
    m.error("β con censura", r["beta"], 2.5, 0.03, "por unidad", relativo=True)
    m.error("η con censura", r["eta"], 1000.0, 0.03, "por unidad", relativo=True)
    m.verdad("β real dentro del intervalo del 95 %", r["beta_ic"][0] <= 2.5 <= r["beta_ic"][1])
    m.verdad("η real dentro del intervalo del 95 %", r["eta_ic"][0] <= 1000.0 <= r["eta_ic"][1])
    a = es.ajustar(t, "weibull")
    m.error("β por máxima verosimilitud (SciPy)", a["parametros"][0], 2.5, 0.03, "por unidad", relativo=True)
    mejor = es.comparar_ajustes(t)[0]["distribucion"]
    m.verdad(f"El AIC elige Weibull ({mejor})", mejor == "weibull")


@prueba(3, "Pruebas de hipótesis e intervalos", "Datos con media conocida y fórmula de t")
def hipotesis(m):
    """La prueba t coincide con su fórmula; el intervalo contiene la media real."""
    rng = np.random.default_rng(1)
    x = rng.normal(10, 2, 400)
    r = es.prueba_t(x, mu0=10.0)
    t_f = (x.mean() - 10.0) / (x.std(ddof=1) / np.sqrt(400))
    m.error("t frente a la fórmula", r["t"], t_f, 1e-10)
    ic = es.intervalo_media(x)
    m.verdad("El intervalo del 95 % contiene μ", ic["inf"] <= 10.0 <= ic["sup"])
    y = rng.normal(13, 2, 400)
    m.verdad("Detecta medias distintas (p < 0.001)", es.prueba_t(x, y)["p"] < 1e-3)
    m.verdad("Normalidad aceptada (p > 0.01)", es.prueba_normalidad(x)["shapiro_p"] > 0.01)


@prueba(3, "Regresión lineal y no lineal", "Coeficientes conocidos")
def regresion(m):
    """y = 2 + 3x con ruido; y = 5·exp(-0.7·x) sin ruido."""
    rng = np.random.default_rng(3)
    x = np.linspace(0, 10, 200)
    r = es.regresion_lineal(x, 2 + 3 * x + rng.normal(0, 0.1, 200))
    m.error("Pendiente", r["coeficientes"][1]["valor"], 3.0, 0.01)
    m.error("Ordenada", r["coeficientes"][0]["valor"], 2.0, 0.05)
    m.verdad("R² > 0.999", r["r2"] > 0.999)
    f = lambda x, a, b: a * np.exp(-b * x)
    n = es.regresion_no_lineal(f, x, f(x, 5.0, 0.7), [1.0, 1.0])
    m.error("Parámetro a", n["parametros"][0], 5.0, 1e-5)
    m.error("Parámetro b", n["parametros"][1], 0.7, 1e-5)


@prueba(3, "ANOVA", "F exacta en un caso a mano")
def anova(m):
    """Grupos {1,2,3}, {2,3,4}, {5,6,7}: F = 13."""
    r = es.anova_un_factor([1, 2, 3], [2, 3, 4], [5, 6, 7])
    m.error("F", r["F"], 13.0, 1e-9)
    d = pd.DataFrame({"y": [1, 2, 3, 4, 2, 3, 4, 5, 5, 6, 7, 8], "a": list("xxxxyyyyzzzz"), "b": list("ppqqppqqppqq")})
    t = es.anova_dos_factores(d, "y", "a", "b")
    m.verdad("ANOVA de dos factores produce la tabla", set(t["fuente"]) >= {"a", "b", "Residuo"})


@prueba(3, "Series temporales", "Componentes conocidas y AR(1) con φ = 0.7")
def series(m):
    """Descompone tendencia + estacionalidad y recupera φ con ARIMA."""
    n = 240
    t = np.arange(n)
    est = 5 * np.sin(2 * np.pi * t / 24)
    x = 0.1 * t + est
    r = es.descomposicion(x, 24)
    m.cota("Amplitud estacional recuperada", np.max(np.abs(r["estacional"][24:48] - est[24:48])), 0.2)
    rng = np.random.default_rng(5)
    e = rng.normal(0, 1, 3000)
    a = np.zeros(3000)
    for i in range(1, 3000):
        a[i] = 0.7 * a[i - 1] + e[i]
    ar = es.arima(a, (1, 0, 0), 12)
    m.error("φ del AR(1)", ar["parametros"]["ar.L1"], 0.7, 0.03)
    m.verdad("Pronóstico con intervalos", ar["pronostico"].size == 12 and np.all(ar["sup"] > ar["inf"]))


@prueba(3, "Control estadístico de procesos", "Cambio de media conocido en la muestra 150")
def spc(m):
    """Shewhart, EWMA y CUSUM deben señalar un cambio de 1.5σ y no alarmar antes."""
    rng = np.random.default_rng(9)
    x0 = rng.normal(100, 1, 150)
    x = np.concatenate([x0, rng.normal(101.5, 1, 100)])
    sh = es.carta_shewhart(x0)
    m.error("Shewhart: σ por rango móvil", sh["sigma"], 1.0, 0.15)
    c = es.carta_cusum(x, k=0.5, h=5.0, objetivo=100.0)
    primera = c["alarma"][0] if c["alarma"] else -1
    m.verdad(f"CUSUM detecta tras el cambio (muestra {primera})", 150 <= primera < 175)
    e = es.carta_ewma(x, lam=0.2, objetivo=100.0)
    primera_e = e["fuera"][0] if e["fuera"] else -1
    m.verdad(f"EWMA detecta tras el cambio (muestra {primera_e})", 150 <= primera_e < 190)
    m.error("EWMA: límite asintótico", e["lcs"][-1] - 100.0, 3 * e["sigma"] * np.sqrt(0.2 / 1.8), 1e-9)


@prueba(3, "Preprocesamiento", "Conteos exactos")
def preproc(m):
    """Limpieza, imputación, escalado y variables temporales."""
    d = pd.DataFrame({"a": [1.0, 2.0, np.nan, 4.0, 1000.0, 2.0], "b": [1, 1, 1, 1, 1, 1]})
    l, inf = ml.limpiar(d, {"a": (0, 100)})
    m.verdad("Marca fuera de rango", inf["fuera_de_rango"] == 1)
    i = ml.imputar(l, "mediana")
    m.verdad("Sin faltantes tras imputar", int(i["a"].isna().sum()) == 0)
    Z, _ = ml.escalar(np.arange(10.0)[:, None])
    m.error("Estandarizado: media 0", Z.mean(), 0.0, 1e-12)
    m.error("Estandarizado: desviación 1", Z.std(), 1.0, 1e-12)
    t = ml.variables_temporales(pd.DataFrame({"t": pd.date_range("2026-01-01 06:00", periods=2, freq="h")}), "t")
    m.error("Hora codificada: sin(2π·6/24)", t["hora_sin"].iloc[0], 1.0, 1e-12)


@prueba(3, "Regresión y clasificación con validación cruzada", "Relación conocida y clases separables")
def supervisado(m):
    """R² alto en una relación suave; exactitud alta en clases separables."""
    rng = np.random.default_rng(11)
    X = rng.uniform(0, 1, (500, 3))
    y = 3 * X[:, 0] + np.sin(4 * X[:, 1]) + rng.normal(0, 0.03, 500)
    _, r, _ = ml.entrenar_regresion(X, y, "bosque", 3)
    m.verdad(f"Bosque: R² validado > 0.95 ({r['r2']:.3f})", r["r2"] > 0.95)
    c = np.array([0] * 150 + [1] * 150)
    Xc = np.vstack([rng.normal(0, 0.5, (150, 2)), rng.normal(3, 0.5, (150, 2))])
    mod, s, _ = ml.entrenar_clasificacion(Xc, c, "logistica", 3)
    m.verdad(f"Logística: exactitud > 0.99 ({s['exactitud']:.3f})", s["exactitud"] > 0.99)
    with tempfile.TemporaryDirectory() as td:
        ruta = os.path.join(td, "modelo.joblib")
        ml.guardar_modelo(mod, ruta, {"modelo": "logistica"})
        rec = ml.cargar_modelo(ruta)
        m.verdad("Modelo guardado y recuperado predice igual", np.array_equal(rec.predict(Xc), mod.predict(Xc)))


@prueba(3, "Clustering y PCA", "Grupos conocidos y varianza conocida")
def no_supervisado(m):
    """K-medias separa tres grupos; PCA concentra la varianza en una dirección."""
    rng = np.random.default_rng(13)
    X = np.vstack([rng.normal(c, 0.2, (100, 2)) for c in ([0, 0], [4, 4], [0, 4])])
    etiquetas, r = ml.agrupar(X, 3)
    puros = all(len(set(etiquetas[i:i + 100])) == 1 for i in (0, 100, 200))
    m.verdad(f"K-medias recupera los tres grupos (silueta {r['silueta']:.2f})", puros)
    t = rng.normal(0, 1, 400)
    Y = np.c_[t, 2 * t + rng.normal(0, 0.01, 400)]
    _, p = ml.pca(Y, 2)
    m.verdad(f"PCA: primera componente > 99 % ({p['varianza_explicada'][0]:.4f})", p["varianza_explicada"][0] > 0.99)


@prueba(3, "Detección de anomalías", "Puntos anómalos insertados")
def anomalias(m):
    """Isolation Forest y residuos contra modelo encuentran las anomalías insertadas."""
    rng = np.random.default_rng(17)
    X = rng.normal(0, 1, (1000, 3))
    X[:10] += 8
    a, s, _ = ml.bosque_aislamiento(X, 0.01)
    m.verdad(f"Isolation Forest: recall ≥ 90 % ({a[:10].sum()}/10)", a[:10].sum() >= 9)
    esperado = np.sin(np.linspace(0, 20, 1000)) * 100 + 500
    medido = esperado + rng.normal(0, 1, 1000)
    medido[500:520] -= 30
    r = ml.residuos_modelo(medido, esperado)
    m.verdad("Residuos: detecta el tramo de 20 puntos", np.all(r["anomalia"][500:520]))
    m.verdad(f"Residuos: pocas falsas alarmas ({int(r['anomalia'][:500].sum())})", r["anomalia"][:500].mean() < 0.02)
