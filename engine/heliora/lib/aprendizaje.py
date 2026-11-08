"""Módulo 6: aprendizaje automático. Funciones cortas sobre scikit-learn."""
from __future__ import annotations

import numpy as np
import pandas as pd


def limpiar(df, reglas=None):
    """Quita duplicados e infinitos y aplica rangos válidos {columna: (mín, máx)} (fuera = NaN)."""
    d = df.drop_duplicates().replace([np.inf, -np.inf], np.nan).copy()
    fuera = 0
    for c, (a, b) in (reglas or {}).items():
        if c in d:
            mal = (d[c] < a) | (d[c] > b)
            fuera += int(mal.sum())
            d.loc[mal, c] = np.nan
    return d, {"filas_quitadas": int(len(df) - len(d)), "fuera_de_rango": fuera, "nulos": int(d.isna().sum().sum())}


def imputar(df, metodo="mediana"):
    """Rellena valores faltantes: media, mediana, último valor o interpolación temporal."""
    d = df.copy()
    num = d.select_dtypes("number").columns
    if metodo == "media":
        d[num] = d[num].fillna(d[num].mean())
    elif metodo == "mediana":
        d[num] = d[num].fillna(d[num].median())
    elif metodo == "ultimo":
        d[num] = d[num].ffill().bfill()
    elif metodo == "interpolar":
        d[num] = d[num].interpolate(limit_direction="both")
    else:
        raise ValueError(f"Método desconocido: {metodo}")
    return d


def escalar(X, metodo="estandar"):
    """Estandar: (x-μ)/σ.  Minmax: (x-min)/(max-min).  Robusto: (x-mediana)/RIC."""
    from sklearn.preprocessing import MinMaxScaler, RobustScaler, StandardScaler
    e = {"estandar": StandardScaler, "minmax": MinMaxScaler, "robusto": RobustScaler}[metodo]()
    return e.fit_transform(X), e


def variables_temporales(df, columna=None):
    """Añade hora, día del año y codificación cíclica: sin/cos(2π·hora/24), sin/cos(2π·día/365)."""
    d = df.copy()
    t = pd.to_datetime(d[columna]) if columna else pd.to_datetime(d.index)
    h = t.dt.hour + t.dt.minute / 60 if hasattr(t, "dt") else t.hour + t.minute / 60
    dia = t.dt.dayofyear if hasattr(t, "dt") else t.dayofyear
    d["hora"] = np.asarray(h)
    d["dia_anio"] = np.asarray(dia)
    d["hora_sin"], d["hora_cos"] = np.sin(2 * np.pi * d["hora"] / 24), np.cos(2 * np.pi * d["hora"] / 24)
    d["dia_sin"], d["dia_cos"] = np.sin(2 * np.pi * d["dia_anio"] / 365.25), np.cos(2 * np.pi * d["dia_anio"] / 365.25)
    return d


MODELOS_REG = ["lineal", "ridge", "bosque", "gradiente"]
MODELOS_CLAS = ["logistica", "bosque", "gradiente", "svm"]


def crear_regresor(nombre, semilla=0):
    from sklearn.ensemble import GradientBoostingRegressor, RandomForestRegressor
    from sklearn.linear_model import LinearRegression, Ridge
    return {"lineal": LinearRegression(), "ridge": Ridge(),
            "bosque": RandomForestRegressor(n_estimators=100, random_state=semilla),
            "gradiente": GradientBoostingRegressor(random_state=semilla)}[nombre]


def crear_clasificador(nombre, semilla=0):
    from sklearn.ensemble import GradientBoostingClassifier, RandomForestClassifier
    from sklearn.linear_model import LogisticRegression
    from sklearn.pipeline import make_pipeline
    from sklearn.preprocessing import StandardScaler
    from sklearn.svm import SVC
    return {"logistica": make_pipeline(StandardScaler(), LogisticRegression(max_iter=1000)),
            "bosque": RandomForestClassifier(n_estimators=100, random_state=semilla),
            "gradiente": GradientBoostingClassifier(random_state=semilla),
            "svm": make_pipeline(StandardScaler(), SVC(probability=True, random_state=semilla))}[nombre]


def _metricas_reg(y, p):
    from sklearn.metrics import mean_absolute_error, r2_score
    return {"r2": float(r2_score(y, p)), "rmse": float(np.sqrt(np.mean((np.asarray(y) - p) ** 2))), "mae": float(mean_absolute_error(y, p))}


def entrenar_regresion(X, y, nombre="bosque", pliegues=5, semilla=0):
    """Validación cruzada por pliegues y modelo final con todos los datos."""
    from sklearn.model_selection import KFold, cross_val_predict
    m = crear_regresor(nombre, semilla)
    p = cross_val_predict(m, X, y, cv=KFold(int(pliegues), shuffle=True, random_state=semilla))
    m.fit(X, y)
    return m, {"modelo": nombre, "pliegues": int(pliegues), **_metricas_reg(y, p)}, p


def entrenar_clasificacion(X, y, nombre="bosque", pliegues=5, semilla=0):
    """Clasificación con validación cruzada: exactitud, F1 y matriz de confusión."""
    from sklearn.metrics import accuracy_score, confusion_matrix, f1_score
    from sklearn.model_selection import StratifiedKFold, cross_val_predict
    m = crear_clasificador(nombre, semilla)
    p = cross_val_predict(m, X, y, cv=StratifiedKFold(int(pliegues), shuffle=True, random_state=semilla))
    m.fit(X, y)
    clases = sorted(pd.unique(pd.Series(y)).tolist(), key=str)
    return m, {"modelo": nombre, "pliegues": int(pliegues), "exactitud": float(accuracy_score(y, p)),
               "f1": float(f1_score(y, p, average="macro")), "clases": [str(c) for c in clases],
               "confusion": confusion_matrix(y, p, labels=clases).tolist()}, p


def agrupar(X, k=3, semilla=0):
    """K-medias sobre datos estandarizados, con el coeficiente de silueta."""
    from sklearn.cluster import KMeans
    from sklearn.metrics import silhouette_score
    Z, _ = escalar(X)
    m = KMeans(int(k), n_init=10, random_state=semilla).fit(Z)
    return m.labels_, {"k": int(k), "inercia": float(m.inertia_), "silueta": float(silhouette_score(Z, m.labels_))}


def pca(X, componentes=2):
    """Análisis de componentes principales sobre datos estandarizados."""
    from sklearn.decomposition import PCA
    Z, _ = escalar(X)
    p = PCA(int(componentes)).fit(Z)
    return p.transform(Z), {"varianza_explicada": p.explained_variance_ratio_.tolist(), "componentes": p.components_.tolist()}


def bosque_aislamiento(X, contaminacion=0.01, arboles=100, submuestras=256, semilla=0):
    """Isolation Forest: puntúa cada punto por lo fácil que es aislarlo.

    s(x) = 2^(-E[h(x)]/c(n)),   c(n) = 2·H(n-1) - 2·(n-1)/n
    100 árboles con 256 puntos cada uno por defecto.
    """
    from sklearn.ensemble import IsolationForest
    m = IsolationForest(n_estimators=int(arboles), max_samples=min(int(submuestras), len(X)),
                        contamination=contaminacion, random_state=semilla).fit(X)
    return m.predict(X) == -1, -m.score_samples(X), m


def residuos_modelo(medido, esperado, nsigma=3.0, ventana=None):
    """Anomalías por residuo contra modelo físico: r = medido - esperado, alarma si |r - r̄| > n·σ."""
    r = np.asarray(medido, dtype=float) - np.asarray(esperado, dtype=float)
    r_s = pd.Series(r)
    if ventana:
        r_s = r_s.rolling(int(ventana), min_periods=1).mean()
    c, s = float(np.nanmedian(r_s)), float(1.4826 * np.nanmedian(np.abs(r_s - np.nanmedian(r_s))))
    return {"residuo": r_s.to_numpy(), "centro": c, "sigma": s, "anomalia": (np.abs(r_s - c) > nsigma * max(s, 1e-12)).to_numpy()}


def guardar_modelo(modelo, ruta, metadatos=None):
    """Guarda un modelo entrenado con joblib; la ficha JSON queda al lado."""
    import json

    import joblib
    joblib.dump(modelo, ruta)
    with open(str(ruta) + ".json", "w", encoding="utf8") as f:
        json.dump(metadatos or {}, f, ensure_ascii=False, indent=1)
    return ruta


def cargar_modelo(ruta):
    import joblib
    return joblib.load(ruta)
