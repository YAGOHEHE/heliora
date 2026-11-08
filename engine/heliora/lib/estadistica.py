"""Módulo 5: estadística. Funciones cortas sobre SciPy y statsmodels."""
from __future__ import annotations

import numpy as np
import pandas as pd
from scipy import optimize, stats


def descriptiva(x):
    """Media, mediana, desviación, cuartiles, asimetría y curtosis (exceso)."""
    x = np.asarray(x, dtype=float)
    x = x[np.isfinite(x)]
    q = np.percentile(x, [0, 25, 50, 75, 100])
    return {"n": int(x.size), "media": float(x.mean()), "mediana": float(q[2]), "desv": float(x.std(ddof=1)),
            "var": float(x.var(ddof=1)), "min": float(q[0]), "q1": float(q[1]), "q3": float(q[3]), "max": float(q[4]),
            "asimetria": float(stats.skew(x)), "curtosis": float(stats.kurtosis(x))}


DISTRIBUCIONES = {"normal": stats.norm, "lognormal": stats.lognorm, "exponencial": stats.expon, "gamma": stats.gamma,
                  "weibull": stats.weibull_min}


def ajustar(x, nombre="weibull"):
    """Ajuste por máxima verosimilitud, con log-verosimilitud, AIC y prueba de Kolmogorov-Smirnov."""
    x = np.asarray(x, dtype=float)
    d = DISTRIBUCIONES[nombre]
    par = d.fit(x, floc=0) if nombre in ("weibull", "lognormal", "gamma", "exponencial") else d.fit(x)
    ll = float(np.sum(d.logpdf(x, *par)))
    k = len(par) - (1 if nombre in ("weibull", "lognormal", "gamma", "exponencial") else 0)
    ks = stats.kstest(x, d.cdf, args=par)
    return {"distribucion": nombre, "parametros": [float(p) for p in par], "log_verosimilitud": ll, "aic": 2 * k - 2 * ll,
            "ks_estadistico": float(ks.statistic), "ks_p": float(ks.pvalue)}


def comparar_ajustes(x):
    """Ajusta todas las distribuciones y las ordena por AIC."""
    x = np.asarray(x, dtype=float)
    r = [ajustar(x, n) for n in DISTRIBUCIONES if not (n != "normal" and np.any(x <= 0))]
    return sorted(r, key=lambda a: a["aic"])


def weibull_fit(t, censurado=None, alfa=0.05):
    """Ajuste de Weibull por máxima verosimilitud, con censura por la derecha e intervalos.

    Log-verosimilitud:  L = Σ_fallos [ln(β/η) + (β-1)·ln(t/η)] - Σ_todos (t/η)^β
    Se resuelve por el método del perfil: 1/β = Σ_f t^β ln t / Σ t^β - media(ln t_f) y
    η^β = Σ t^β / r. Los intervalos salen de la información observada (delta en log-escala).
    """
    t = np.asarray(t, dtype=float)
    c = np.zeros_like(t, dtype=bool) if censurado is None else np.asarray(censurado, dtype=bool)
    if int((~c).sum()) < 2:
        raise ValueError("Se necesitan al menos dos fallos para ajustar Weibull")
    # Se trabaja en tiempo adimensional para evitar desbordes de t^β
    escala = float(t.max())
    t = t / escala
    f = t[~c]
    r = f.size
    logf = np.log(f)
    lt = np.log(t)

    def perfil(b):
        tb = t**b
        return np.sum(tb * lt) / np.sum(tb) - 1.0 / b - logf.mean()

    b = optimize.brentq(perfil, 1e-3, 200.0)
    eta = float((np.sum(t**b) / r) ** (1.0 / b))
    # Información observada numérica en (ln β, ln η)

    def nll(th):
        lb, le = th
        be, et = np.exp(lb), np.exp(le)
        return -(r * (lb - be * le) + (be - 1) * logf.sum() - np.sum((t / et) ** be))

    h = 1e-4
    x0 = np.array([np.log(b), np.log(eta)])
    H = np.zeros((2, 2))
    for i in range(2):
        for j in range(2):
            ei, ej = np.eye(2)[i] * h, np.eye(2)[j] * h
            H[i, j] = (nll(x0 + ei + ej) - nll(x0 + ei - ej) - nll(x0 - ei + ej) + nll(x0 - ei - ej)) / (4 * h * h)
    cov = np.linalg.inv(H)
    z = stats.norm.ppf(1 - alfa / 2)
    ib = np.exp(x0[0] + np.array([-1, 1]) * z * np.sqrt(cov[0, 0]))
    ie = np.exp(x0[1] + np.array([-1, 1]) * z * np.sqrt(cov[1, 1]))
    return {"beta": float(b), "eta": eta * escala, "beta_ic": ib.tolist(), "eta_ic": (ie * escala).tolist(),
            "fallos": int(r), "censurados": int(c.sum())}


def intervalo_media(x, confianza=0.95):
    """Intervalo de confianza t de Student para la media: x̄ ± t·s/√n"""
    x = np.asarray(x, dtype=float)
    m, s = x.mean(), x.std(ddof=1)
    h = stats.t.ppf(0.5 + confianza / 2, x.size - 1) * s / np.sqrt(x.size)
    return {"media": float(m), "inf": float(m - h), "sup": float(m + h), "confianza": confianza}


def prueba_t(a, b=None, mu0=0.0, varianzas_iguales=False):
    """Prueba t: una muestra frente a mu0, o dos muestras (Welch por defecto)."""
    a = np.asarray(a, dtype=float)
    if b is None:
        r = stats.ttest_1samp(a, mu0)
        tipo = f"una muestra, μ0 = {mu0:g}"
    else:
        r = stats.ttest_ind(a, np.asarray(b, dtype=float), equal_var=varianzas_iguales)
        tipo = "dos muestras" + (" (varianzas iguales)" if varianzas_iguales else " (Welch)")
    return {"tipo": tipo, "t": float(r.statistic), "p": float(r.pvalue), "gl": float(r.df)}


def prueba_normalidad(x):
    """Shapiro-Wilk (hasta 5000 muestras) y Jarque-Bera."""
    x = np.asarray(x, dtype=float)
    sw = stats.shapiro(x[:5000])
    jb = stats.jarque_bera(x)
    return {"shapiro_W": float(sw.statistic), "shapiro_p": float(sw.pvalue), "jb": float(jb.statistic), "jb_p": float(jb.pvalue)}


def chi2_varianza(x, sigma0):
    """Prueba de varianza: (n-1)·s²/σ0² sigue una χ² con n-1 grados de libertad."""
    x = np.asarray(x, dtype=float)
    chi = (x.size - 1) * x.var(ddof=1) / sigma0**2
    p = 2 * min(stats.chi2.cdf(chi, x.size - 1), stats.chi2.sf(chi, x.size - 1))
    return {"chi2": float(chi), "gl": int(x.size - 1), "p": float(p)}


def regresion_lineal(X, y, nombres=None):
    """Regresión lineal por mínimos cuadrados con statsmodels: coeficientes, errores, p, R²."""
    import statsmodels.api as sm
    X = np.asarray(X, dtype=float)
    if X.ndim == 1:
        X = X[:, None]
    A = sm.add_constant(X)
    r = sm.OLS(np.asarray(y, dtype=float), A).fit()
    nom = ["constante"] + (nombres or [f"x{i+1}" for i in range(X.shape[1])])
    return {"coeficientes": [{"termino": n, "valor": float(c), "error": float(e), "t": float(t), "p": float(p)}
                             for n, c, e, t, p in zip(nom, r.params, r.bse, r.tvalues, r.pvalues)],
            "r2": float(r.rsquared), "r2_ajustado": float(r.rsquared_adj), "rmse": float(np.sqrt(r.mse_resid)),
            "f": float(r.fvalue), "f_p": float(r.f_pvalue), "n": int(r.nobs), "ajuste": r.fittedvalues, "residuos": r.resid}


def regresion_no_lineal(f, x, y, p0):
    """Regresión no lineal por Levenberg-Marquardt: f(x, *p) con errores por la covarianza."""
    p, cov = optimize.curve_fit(f, np.asarray(x, dtype=float), np.asarray(y, dtype=float), p0=p0, maxfev=20000)
    e = np.asarray(y) - f(np.asarray(x), *p)
    return {"parametros": p.tolist(), "errores": np.sqrt(np.diag(cov)).tolist(), "rmse": float(np.sqrt(np.mean(e**2)))}


def anova_un_factor(*grupos):
    """ANOVA de un factor: F = (SC_entre/(k-1)) / (SC_dentro/(N-k))."""
    grupos = [np.asarray(g, dtype=float) for g in grupos]
    r = stats.f_oneway(*grupos)
    todo = np.concatenate(grupos)
    sce = sum(g.size * (g.mean() - todo.mean()) ** 2 for g in grupos)
    scd = sum(np.sum((g - g.mean()) ** 2) for g in grupos)
    return {"F": float(r.statistic), "p": float(r.pvalue), "sc_entre": float(sce), "sc_dentro": float(scd),
            "gl_entre": len(grupos) - 1, "gl_dentro": int(todo.size - len(grupos))}


def anova_dos_factores(df, y, a, b):
    """ANOVA de dos factores con interacción (statsmodels), suma de cuadrados tipo II."""
    import statsmodels.api as sm
    from statsmodels.formula.api import ols
    d = df.rename(columns={y: "y_", a: "a_", b: "b_"})
    m = ols("y_ ~ C(a_) + C(b_) + C(a_):C(b_)", data=d).fit()
    t = sm.stats.anova_lm(m, typ=2)
    t.index = [a, b, f"{a} × {b}", "Residuo"]
    return t.reset_index().rename(columns={"index": "fuente"})


def descomposicion(x, periodo, modelo="additive"):
    """Descomposición clásica en tendencia, estacionalidad y residuo (statsmodels)."""
    from statsmodels.tsa.seasonal import seasonal_decompose
    r = seasonal_decompose(np.asarray(x, dtype=float), period=int(periodo), model=modelo, extrapolate_trend="period")
    return {"tendencia": r.trend, "estacional": r.seasonal, "residuo": r.resid}


def arima(x, orden=(1, 1, 1), pasos=24):
    """Modelo ARIMA(p,d,q) con statsmodels: parámetros, AIC y pronóstico con intervalos del 95 %."""
    from statsmodels.tsa.arima.model import ARIMA
    m = ARIMA(np.asarray(x, dtype=float), order=tuple(int(k) for k in orden)).fit()
    f = m.get_forecast(int(pasos))
    ic = f.conf_int(alpha=0.05)
    return {"parametros": {str(k): float(v) for k, v in zip(m.param_names, m.params)}, "aic": float(m.aic), "bic": float(m.bic),
            "pronostico": np.asarray(f.predicted_mean), "inf": np.asarray(ic)[:, 0], "sup": np.asarray(ic)[:, 1]}


def _sigma_rango_movil(x):
    """Sigma por rango móvil: σ = media(|x[i] - x[i-1]|)/1.128"""
    return float(np.mean(np.abs(np.diff(x))) / 1.128)


def carta_shewhart(x, nsigma=3.0):
    """Carta de individuos: LC = x̄; LCS, LCI = x̄ ± n·σ con σ por rango móvil."""
    x = np.asarray(x, dtype=float)
    c, s = float(x.mean()), _sigma_rango_movil(x)
    lcs, lci = c + nsigma * s, c - nsigma * s
    fuera = np.where((x > lcs) | (x < lci))[0]
    return {"centro": c, "sigma": s, "lcs": lcs, "lci": lci, "fuera": fuera.tolist()}


def carta_ewma(x, lam=0.4, nsigma=3.0, objetivo=None):
    """Carta EWMA: z[i] = λ·x[i] + (1-λ)·z[i-1]

    Límites: μ ± n·σ·√(λ/(2-λ)·(1 - (1-λ)^(2i)))
    """
    x = np.asarray(x, dtype=float)
    mu = float(x.mean()) if objetivo is None else objetivo
    s = _sigma_rango_movil(x)
    z = np.empty_like(x)
    prev = mu
    for i, v in enumerate(x):
        prev = lam * v + (1 - lam) * prev
        z[i] = prev
    i = np.arange(1, x.size + 1)
    a = nsigma * s * np.sqrt(lam / (2 - lam) * (1 - (1 - lam) ** (2 * i)))
    fuera = np.where((z > mu + a) | (z < mu - a))[0]
    return {"z": z, "centro": mu, "lcs": mu + a, "lci": mu - a, "fuera": fuera.tolist(), "sigma": s}


def carta_cusum(x, k=0.5, h=5.0, objetivo=None):
    """CUSUM tabular: C+[i] = max(0, x[i] - μ - k·σ + C+[i-1]);  C-[i] = max(0, μ - k·σ - x[i] + C-[i-1])

    Alarma si C+ o C- superan h·σ.
    """
    x = np.asarray(x, dtype=float)
    mu = float(x.mean()) if objetivo is None else objetivo
    s = _sigma_rango_movil(x)
    cp, cn = np.zeros_like(x), np.zeros_like(x)
    a, b = 0.0, 0.0
    for i, v in enumerate(x):
        a = max(0.0, v - mu - k * s + a)
        b = max(0.0, mu - k * s - v + b)
        cp[i], cn[i] = a, b
    lim = h * s
    return {"c_mas": cp, "c_menos": cn, "limite": lim, "alarma": np.where((cp > lim) | (cn > lim))[0].tolist(), "sigma": s, "centro": mu}
