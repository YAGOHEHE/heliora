"""Módulo 6: aprendizaje automático. Funciones cortas sobre scikit-learn."""
from __future__ import annotations

from dataclasses import dataclass, field

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
VALIDACIONES = ["aleatoria (k pliegues)", "temporal", "hold-out"]


@dataclass
class ModeloEntrenado:
    """Modelo listo para aplicar: preprocesado y estimador, columnas de entrada y tipo de tarea.

    Es serializable con joblib: guarda todo lo necesario para predecir sobre otra tabla.
    """
    pipeline: object
    columnas: list
    tarea: str
    objetivo: str
    modelo: str
    clases: list = field(default_factory=list)
    metricas: dict = field(default_factory=dict)

    @property
    def estimador(self):
        """Último paso del pipeline (el modelo propiamente dicho)."""
        return self.pipeline[-1]

    def predecir(self, df):
        """Predice sobre una tabla con las mismas columnas de entrenamiento.

        Args:
            df: tabla con, al menos, las columnas de entrada del entrenamiento.

        Returns:
            Tupla (predicciones, filas con algún valor imputado).

        Raises:
            ValueError: si faltan columnas o alguna no es numérica.
        """
        d = df.copy()
        d.columns = [str(c) for c in d.columns]
        faltan = [c for c in self.columnas if c not in d.columns]
        if faltan:
            raise ValueError(f"A la tabla le faltan {len(faltan)} columna(s) con las que se entrenó el modelo: {', '.join(faltan)}. "
                             f"Columnas de la tabla: {', '.join(d.columns)}.")
        X = d[self.columnas].copy()
        for c in self.columnas:
            n = pd.to_numeric(X[c], errors="coerce")
            if n.isna().sum() > X[c].isna().sum():
                raise ValueError(f"La columna «{c}» no es numérica y el modelo se entrenó con valores numéricos.")
            X[c] = n
        A = X.to_numpy(dtype=float)
        A[~np.isfinite(A)] = np.nan
        return self.pipeline.predict(A), int(np.isnan(A).any(axis=1).sum())


def _con_preproceso(estimador):
    from sklearn.impute import SimpleImputer
    from sklearn.pipeline import make_pipeline
    from sklearn.preprocessing import StandardScaler
    return make_pipeline(SimpleImputer(strategy="median"), StandardScaler(), estimador)


def crear_regresor(nombre, semilla=0, hiper=None):
    """Pipeline (imputación, escalado, regresor); `hiper` admite n_estimators, max_depth (0 = por defecto) y alpha."""
    from sklearn.ensemble import GradientBoostingRegressor, RandomForestRegressor
    from sklearn.linear_model import LinearRegression, Ridge
    h = hiper or {}
    n, prof = int(h.get("n_estimators", 100)), int(h.get("max_depth", 0)) or None
    return _con_preproceso({"lineal": lambda: LinearRegression(), "ridge": lambda: Ridge(alpha=float(h.get("alpha", 1.0))),
                            "bosque": lambda: RandomForestRegressor(n_estimators=n, max_depth=prof, random_state=semilla),
                            "gradiente": lambda: GradientBoostingRegressor(n_estimators=n, max_depth=prof or 3, random_state=semilla)}[nombre]())


def crear_clasificador(nombre, semilla=0, hiper=None):
    """Pipeline (imputación, escalado, clasificador); `hiper` admite n_estimators, max_depth (0 = por defecto) y C."""
    from sklearn.ensemble import GradientBoostingClassifier, RandomForestClassifier
    from sklearn.linear_model import LogisticRegression
    from sklearn.svm import SVC
    h = hiper or {}
    n, prof, C = int(h.get("n_estimators", 100)), int(h.get("max_depth", 0)) or None, float(h.get("C", 1.0))
    return _con_preproceso({"logistica": lambda: LogisticRegression(C=C, max_iter=1000),
                            "bosque": lambda: RandomForestClassifier(n_estimators=n, max_depth=prof, random_state=semilla),
                            "gradiente": lambda: GradientBoostingClassifier(n_estimators=n, max_depth=prof or 3, random_state=semilla),
                            "svm": lambda: SVC(C=C, probability=True, random_state=semilla)}[nombre]())


def _metricas_reg(y, p):
    from sklearn.metrics import mean_absolute_error, r2_score
    y, p = np.asarray(y, dtype=float), np.asarray(p, dtype=float)
    return {"r2": float(r2_score(y, p)), "rmse": float(np.sqrt(np.mean((y - p) ** 2))), "mae": float(mean_absolute_error(y, p))}


def _metricas_clas(y, p):
    from sklearn.metrics import accuracy_score, f1_score
    return {"exactitud": float(accuracy_score(y, p)), "f1": float(f1_score(y, p, average="macro"))}


def particiones(n, validacion, pliegues=5, semilla=0, pct=20.0, y=None, orden=None):
    """Pares (entrenamiento, validación) de índices según el tipo de validación.

    Args:
        n: número de filas.
        validacion: una de VALIDACIONES.
        pliegues: pliegues de la validación aleatoria o divisiones de la temporal.
        semilla: semilla del barajado.
        pct: porcentaje de validación del hold-out.
        y: clases, para estratificar (opcional).
        orden: clave de orden de las filas para la validación temporal (por defecto, el orden de la tabla).

    Raises:
        ValueError: si el tipo es desconocido o no hay filas suficientes.
    """
    from sklearn.model_selection import KFold, ShuffleSplit, StratifiedKFold, StratifiedShuffleSplit, TimeSeriesSplit
    if validacion == "temporal":
        perm = np.argsort(np.asarray(orden), kind="stable") if orden is not None else np.arange(n)
        return [(perm[a], perm[b]) for a, b in TimeSeriesSplit(int(pliegues)).split(perm)]
    if validacion == "hold-out":
        sep = (StratifiedShuffleSplit if y is not None else ShuffleSplit)(n_splits=1, test_size=pct / 100, random_state=semilla)
    elif validacion.startswith("aleatoria"):
        sep = (StratifiedKFold if y is not None else KFold)(int(pliegues), shuffle=True, random_state=semilla)
    else:
        raise ValueError(f"Tipo de validación desconocido: {validacion}")
    return list(sep.split(np.zeros(n), y))


def _entrenar(tarea, X, y, nombre, validacion, pliegues, semilla, pct, orden, hiper):
    n = len(y)
    crear = crear_regresor if tarea == "regresion" else crear_clasificador
    metricas = _metricas_reg if tarea == "regresion" else _metricas_clas
    partes = particiones(n, validacion, pliegues, semilla, pct, None if tarea == "regresion" else y, orden)
    pred = np.full(n, np.nan) if tarea == "regresion" else np.full(n, y[0])
    valida = np.zeros(n, dtype=bool)
    en_entrenamiento = []
    for tr, te in partes:
        m = crear(nombre, semilla, hiper).fit(X[tr], y[tr])
        pred[te], valida[te] = m.predict(X[te]), True
        en_entrenamiento.append(metricas(y[tr], m.predict(X[tr])))
    entrenamiento = {k: float(np.mean([f[k] for f in en_entrenamiento])) for k in en_entrenamiento[0]}
    final = crear(nombre, semilla, hiper).fit(X, y)
    r = {"modelo": nombre, "validacion": validacion, "pliegues": len(partes), "semilla": int(semilla), "n": n, "n_validado": int(valida.sum()),
         "entrenamiento": entrenamiento, "validada": valida, **metricas(y[valida], pred[valida])}
    return final, r, pred


def entrenar_regresion(X, y, nombre="bosque", pliegues=5, semilla=0, validacion="aleatoria (k pliegues)", pct=20.0, orden=None, hiper=None):
    """Entrena un regresor y lo valida fuera de muestra (aleatoria, temporal o hold-out).

    Returns:
        Tupla (pipeline entrenado con todos los datos, métricas, predicciones de validación con NaN en las filas no validadas).
        Las métricas de validación están al nivel superior y las de entrenamiento en `entrenamiento`.
    """
    return _entrenar("regresion", np.asarray(X, dtype=float), np.asarray(y, dtype=float), nombre, validacion, pliegues, semilla, pct, orden, hiper)


def entrenar_clasificacion(X, y, nombre="bosque", pliegues=5, semilla=0, validacion="aleatoria (k pliegues)", pct=20.0, orden=None, hiper=None):
    """Clasificación con validación fuera de muestra: exactitud, F1 y matriz de confusión de las filas validadas."""
    from sklearn.metrics import confusion_matrix
    y = np.asarray(y)
    m, r, p = _entrenar("clasificacion", np.asarray(X, dtype=float), y, nombre, validacion, pliegues, semilla, pct, orden, hiper)
    clases = sorted(pd.unique(pd.Series(y)).tolist(), key=str)
    v = r["validada"]
    r.update(clases=[str(c) for c in clases], confusion=confusion_matrix(y[v], p[v], labels=clases).tolist())
    return m, r, p


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
    """Carga con joblib un modelo guardado; los archivos de origen desconocido pueden ejecutar código."""
    import joblib
    return joblib.load(ruta)
