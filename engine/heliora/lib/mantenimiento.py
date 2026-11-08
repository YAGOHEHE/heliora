"""Módulo 7: mantenimiento preventivo y predictivo de una planta fotovoltaica.

Datos SCADA por marca de tiempo e inversor. Columnas que se reconocen (admite alias):
    fecha, inversor, irradiancia_wm2, temp_amb_c, temp_modulo_c, p_dc_kw, p_ac_kw, v_dc_v, i_dc_a,
    v_ac_v, i_ac_a, temp_igbt_c, alarma, en_servicio, i_s1, i_s2, ... (corriente por string)

KPI (IEC 61724):
    PR = E_ac / (P_nom · H_poa / G_stc)         H_poa = ∫G dt
    Disponibilidad = horas en servicio con G > G_min / horas con G > G_min
    Energía perdida = ∫max(0, P_esperada - P_ac) dt
    Rendimiento del inversor η = E_ac / E_dc

Modelo físico esperado (PVWatts):
    P_dc,esp = P_nom·(G/1000)·(1 + γ·(T_c - 25))
"""
from __future__ import annotations

import copy
import datetime as dt
import json
import os

import numpy as np
import pandas as pd

ALIAS = {
    "fecha": ["fecha", "timestamp", "datetime", "time", "hora", "date", "tiempo"],
    "inversor": ["inversor", "inverter", "equipo", "id_inversor"],
    "irradiancia_wm2": ["irradiancia_wm2", "irradiancia", "poa", "ghi", "g_poa", "g"],
    "temp_amb_c": ["temp_amb_c", "t_amb", "tamb", "temperatura_ambiente"],
    "temp_modulo_c": ["temp_modulo_c", "t_modulo", "tmod", "temperatura_modulo"],
    "p_dc_kw": ["p_dc_kw", "pdc", "p_dc", "potencia_dc"],
    "p_ac_kw": ["p_ac_kw", "pac", "p_ac", "potencia_ac", "potencia"],
    "v_dc_v": ["v_dc_v", "vdc", "v_dc", "tension_dc"],
    "i_dc_a": ["i_dc_a", "idc", "i_dc", "corriente_dc"],
    "v_ac_v": ["v_ac_v", "vac", "v_ac", "tension_ac"],
    "i_ac_a": ["i_ac_a", "iac", "i_ac", "corriente_ac"],
    "temp_igbt_c": ["temp_igbt_c", "t_igbt", "tigbt", "temperatura_igbt"],
    "alarma": ["alarma", "alarm", "alarmas", "codigo_alarma"],
    "en_servicio": ["en_servicio", "disponible", "online"],
}


def normalizar_columnas(df):
    """Renombra columnas a sus nombres normales y convierte la fecha."""
    nombres = {}
    bajas = {str(c).strip().lower(): c for c in df.columns}
    for destino, alias in ALIAS.items():
        for a in alias:
            if a in bajas:
                nombres[bajas[a]] = destino
                break
    d = df.rename(columns=nombres).copy()
    if "fecha" not in d:
        raise ValueError("El archivo no tiene una columna de fecha reconocible")
    d["fecha"] = pd.to_datetime(d["fecha"])
    if "inversor" not in d:
        d["inversor"] = "INV-1"
    return d.sort_values(["fecha", "inversor"]).reset_index(drop=True)


def importar_scada(ruta):
    """Lee CSV o Parquet y devuelve los datos normalizados y un resumen de calidad."""
    d = pd.read_parquet(ruta) if str(ruta).lower().endswith(".parquet") else pd.read_csv(ruta)
    d = normalizar_columnas(d)
    return d, resumen_calidad(d)


def resumen_calidad(d):
    paso = d.groupby("inversor")["fecha"].diff().median()
    faltan = {c: int(d[c].isna().sum()) for c in d.columns if d[c].isna().any()}
    return {"filas": int(len(d)), "inversores": int(d["inversor"].nunique()), "desde": str(d["fecha"].min()),
            "hasta": str(d["fecha"].max()), "paso_min": float(paso.total_seconds() / 60.0) if pd.notna(paso) else 0.0,
            "faltantes": faltan, "columnas": [str(c) for c in d.columns]}


def _horas_paso(d):
    return d.groupby("inversor")["fecha"].diff().median().total_seconds() / 3600.0


def generar_scada(inicio="2024-01-01", dias=730, paso_min=30, inversores=2, p_nom_kw=500.0, semilla=7, lat=40.4, lon=-3.7,
                  degradacion_anual=0.007, suciedad_dia=0.002, fallas=True):
    """Datos SCADA de muestra con fallas conocidas (para aprender y validar el análisis).

    Inyecta: degradación anual, suciedad en diente de sierra con lluvias, un string caído,
    episodios de sobretemperatura de IGBT y paradas de inversor.
    Devuelve (datos, verdad), donde verdad describe lo inyectado.
    """
    from .fotovoltaica import irradiancia_poa, temperatura_modulo

    rng = np.random.default_rng(semilla)
    t = pd.date_range(inicio, periods=int(dias * 24 * 60 / paso_min), freq=f"{paso_min}min")
    poa, _, _ = irradiancia_poa(lat, lon, t.tz_localize("UTC"), 30.0, 180.0)
    g_cs = poa["poa_global"].fillna(0.0).clip(lower=0.0).to_numpy()
    dia = np.asarray((t - t[0]).days)
    # Nubes: un factor por día con variación por intervalo
    nube_dia = np.clip(rng.beta(5.0, 1.3, dias + 1), 0.15, 1.0)
    G = g_cs * nube_dia[dia] * np.clip(1 + rng.normal(0, 0.03, t.size), 0.5, 1.15)
    doy = np.asarray(t.dayofyear)
    hora = np.asarray(t.hour + t.minute / 60)
    Ta = 15 + 9 * np.sin(2 * np.pi * (doy - 110) / 365.25) + 5 * np.sin(2 * np.pi * (hora - 9) / 24) + rng.normal(0, 0.5, t.size)
    Tc = np.asarray(temperatura_modulo(G, Ta, 2.0))
    # Suciedad: cae a diario y se limpia con lluvia
    lluvia = rng.uniform(size=dias + 1) < 1 / 28.0
    s = np.ones(dias + 1)
    for k in range(1, dias + 1):
        s[k] = 1.0 if lluvia[k] else max(0.8, s[k - 1] - suciedad_dia)
    gamma = -0.0037
    filas = []
    verdad = {"degradacion_anual": degradacion_anual, "suciedad_dia": suciedad_dia, "gamma": gamma, "p_nom_kw": p_nom_kw * inversores,
              "p_nom_inversor_kw": p_nom_kw, "paradas": [], "string_caido": None, "igbt": None}
    h = paso_min / 60.0
    for iv in range(inversores):
        nombre = f"INV-{iv + 1}"
        deg = (1 - degradacion_anual) ** (dia / 365.25)
        factor_strings = np.ones((t.size, 4))
        if fallas and iv == 1:
            factor_strings[(dia >= 200) & (dia < 215), 2] = 0.0
            verdad["string_caido"] = {"inversor": nombre, "string": 3, "dia_ini": 200, "dia_fin": 215}
        P_dc = p_nom_kw * (G / 1000.0) * (1 + gamma * (Tc - 25.0)) * s[dia] * deg * factor_strings.mean(axis=1)
        servicio = np.ones(t.size, dtype=bool)
        if fallas:
            # Paradas con tiempos entre fallos de Weibull
            k = 0
            while True:
                k += int(rng.weibull(1.8) * 700.0 / h)
                if k >= t.size:
                    break
                dur = max(1, int(rng.lognormal(np.log(10.0), 0.6) / h))
                servicio[k:k + dur] = False
                verdad["paradas"].append({"inversor": nombre, "inicio": str(t[k]), "horas": dur * h})
                k += dur
        P_dc = np.where(servicio, P_dc, 0.0)
        x = np.clip(P_dc / p_nom_kw, 0.0, 1.5)
        eta = np.where(x > 0.005, 0.985 - 0.012 / (x + 0.04) * 0.15, 0.0)
        P_ac = np.minimum(P_dc * eta, p_nom_kw)
        V_dc = np.where(P_dc > 0, 780 * (1 - 0.003 * (Tc - 25)) * (1 + 0.02 * np.log(np.maximum(G, 10) / 1000)), 0.0)
        I_dc = np.where(V_dc > 0, 1000 * P_dc / np.maximum(V_dc, 1), 0.0)
        V_ac = np.where(servicio, 400 + rng.normal(0, 1.0, t.size), 0.0)
        I_ac = np.where(V_ac > 0, 1000 * P_ac / (np.sqrt(3) * np.maximum(V_ac, 1)), 0.0)
        T_igbt = Ta + 8.0 + 0.09 * P_ac + rng.normal(0, 0.8, t.size)
        if fallas and iv == 0:
            sel = (dia >= 400) & (dia < 412)
            T_igbt = np.where(sel, T_igbt + 28.0, T_igbt)
            verdad["igbt"] = {"inversor": nombre, "dia_ini": 400, "dia_fin": 412, "exceso_c": 28.0}
        alarma = np.where(~servicio, "E101-parada", np.where(T_igbt > 95, "W210-temp-igbt", ""))
        d = {"fecha": t, "inversor": nombre, "irradiancia_wm2": G, "temp_amb_c": Ta, "temp_modulo_c": Tc, "p_dc_kw": P_dc,
             "p_ac_kw": P_ac, "v_dc_v": V_dc, "i_dc_a": I_dc, "v_ac_v": V_ac, "i_ac_a": I_ac, "temp_igbt_c": T_igbt,
             "alarma": alarma, "en_servicio": servicio.astype(int)}
        cada = I_dc / 4.0
        for j in range(4):
            d[f"i_s{j + 1}"] = np.where(servicio, cada * 4 * factor_strings[:, j] / np.maximum(factor_strings.sum(axis=1), 1) * rng.normal(1, 0.01, t.size), 0.0)
        filas.append(pd.DataFrame(d))
    return pd.concat(filas, ignore_index=True), verdad


def potencia_esperada(d, p_nom_kw, gamma=-0.0037, eta_inv=0.975):
    """P_ac,esp = P_nom·(G/1000)·(1 + γ·(T_c - 25))·η, limitada a P_nom."""
    tc = d["temp_modulo_c"] if "temp_modulo_c" in d else d.get("temp_amb_c", 25.0) + d["irradiancia_wm2"] * 0.03
    return np.minimum(p_nom_kw * d["irradiancia_wm2"] / 1000.0 * (1 + gamma * (tc - 25.0)) * eta_inv, p_nom_kw)


def kpis(d, p_nom_kw, gamma=-0.0037, g_min=50.0, eta_inv=0.975):
    """KPIs globales de la planta y por inversor. p_nom_kw es la potencia por inversor."""
    h = _horas_paso(d)
    filas = []
    for nombre, g in d.groupby("inversor"):
        H = g["irradiancia_wm2"].sum() * h / 1000.0  # kWh/m²
        E_ac = g["p_ac_kw"].sum() * h
        E_dc = g["p_dc_kw"].sum() * h if "p_dc_kw" in g else float("nan")
        sol = g["irradiancia_wm2"] > g_min
        serv = (g["en_servicio"] > 0) if "en_servicio" in g else (g["p_ac_kw"] > 0)
        esp = potencia_esperada(g, p_nom_kw, gamma, eta_inv)
        perdida = np.maximum(esp - g["p_ac_kw"], 0.0)
        filas.append({"inversor": nombre, "PR": E_ac / (p_nom_kw * H) if H > 0 else 0.0,
                      "disponibilidad": float(serv[sol].mean()) if sol.any() else 0.0,
                      "energia_kwh": float(E_ac), "energia_perdida_kwh": float(perdida.sum() * h),
                      "perdida_parada_kwh": float(perdida[~serv].sum() * h),
                      "eta_inversor": float(E_ac / E_dc) if E_dc else float("nan"),
                      "insolacion_kwh_m2": float(H)})
    t = pd.DataFrame(filas)
    tot = {"PR": float((t["PR"] * t["insolacion_kwh_m2"]).sum() / t["insolacion_kwh_m2"].sum()),
           "disponibilidad": float(t["disponibilidad"].mean()), "energia_kwh": float(t["energia_kwh"].sum()),
           "energia_perdida_kwh": float(t["energia_perdida_kwh"].sum()), "eta_inversor": float(t["energia_kwh"].sum() / d["p_dc_kw"].sum() / h) if "p_dc_kw" in d else float("nan")}
    return tot, t


def pr_diario(d, p_nom_kw, gamma=-0.0037, g_min=300.0, eta_inv=1.0):
    """Índice de rendimiento diario corregido por temperatura, con G > g_min y en servicio."""
    tc = d["temp_modulo_c"] if "temp_modulo_c" in d else d["temp_amb_c"]
    sel = d["irradiancia_wm2"] > g_min
    if "en_servicio" in d:
        sel &= d["en_servicio"] > 0
    g = d[sel].copy()
    esp = p_nom_kw * g["irradiancia_wm2"] / 1000.0 * (1 + gamma * (tc[sel] - 25.0)) * eta_inv
    g["_esp"] = esp
    g["_dia"] = g["fecha"].dt.floor("D")
    a = g.groupby("_dia").agg(p=("p_ac_kw", "sum"), e=("_esp", "sum"), n=("p_ac_kw", "size"))
    a = a[a["n"] >= 4]
    return a["p"] / a["e"]


def estimar_degradacion(d, p_nom_kw, gamma=-0.0037):
    """Tasa anual de degradación.

    La envolvente (percentil 90 en ventanas de 30 días) quita el efecto de la suciedad.
    Con más de un año de datos se usa el método interanual, que no tiene sesgo estacional:
        tasa = 1 - mediana(envolvente(t + 1 año)/envolvente(t))
    Con menos de un año se usa la pendiente de Theil-Sen (menos fiable).
    """
    from scipy.stats import theilslopes
    pr = pr_diario(d, p_nom_kw, gamma)
    if len(pr) < 60:
        raise ValueError("Se necesitan al menos 60 días con sol para estimar la degradación")
    dias_idx = pd.date_range(pr.index[0], pr.index[-1], freq="D")
    env = pr.reindex(dias_idx).rolling(30, center=True, min_periods=8).quantile(0.9)
    razon = (env.shift(-365) / env).dropna()
    if len(razon) >= 60:
        tasa = float(1 - razon.median())
        ic = [float(1 - razon.quantile(0.75)), float(1 - razon.quantile(0.25))]
        metodo = "interanual"
    else:
        e2 = env.dropna()
        x = (e2.index - e2.index[0]).days.to_numpy(dtype=float)
        pend, inter, lo, hi = theilslopes(e2.to_numpy(), x, 0.9)
        tasa, ic = float(-pend / inter * 365.25), [float(-hi / inter * 365.25), float(-lo / inter * 365.25)]
        metodo = "Theil-Sen"
    return {"tasa_anual": tasa, "tasa_ic": ic, "metodo": metodo, "pr_inicial": float(env.dropna().iloc[0]),
            "envolvente": env.dropna(), "pr_diario": pr}


def estimar_suciedad(d, p_nom_kw, gamma=-0.0037, salto=0.012):
    """Suciedad: razón r = PR/envolvente móvil; lavados = saltos de r > `salto`.

    Tasa = mediana de la pendiente de r entre lavados.
    Pérdida media = 1 - mediana(r).
    """
    pr = pr_diario(d, p_nom_kw, gamma)
    env = pr.rolling(45, center=True, min_periods=10).quantile(0.97)
    r = (pr / env).dropna()
    r_s = r.rolling(3, center=True, min_periods=1).median()
    salto_d = r_s.diff()
    lavados = list(r_s.index[salto_d > salto])
    pendientes = []
    puntos = [r_s.index[0]] + lavados + [r_s.index[-1]]
    for a, b in zip(puntos[:-1], puntos[1:]):
        tramo = r_s[(r_s.index > a) & (r_s.index < b)]
        if len(tramo) >= 8:
            x = (tramo.index - tramo.index[0]).days.to_numpy(dtype=float)
            pendientes.append(np.polyfit(x, tramo.to_numpy(), 1)[0])
    tasa = float(-np.median(pendientes)) if pendientes else 0.0
    return {"tasa_dia": tasa, "perdida_media": float(1 - r.median()), "lavados": [str(x.date()) for x in lavados], "razon": r}


def detectar_string_caido(d, g_min=300.0, umbral=0.5):
    """String caído: i_s/mediana(i de sus compañeros) < umbral, en días con G > g_min."""
    columnas = [c for c in d.columns if str(c).startswith("i_s")]
    if len(columnas) < 2:
        return []
    eventos = []
    for nombre, g in d[d["irradiancia_wm2"] > g_min].groupby("inversor"):
        mat = g[columnas].to_numpy(dtype=float)
        med = np.nanmedian(mat, axis=1, keepdims=True)
        with np.errstate(divide="ignore", invalid="ignore"):
            rel = mat / np.where(med > 0.5, med, np.nan)
        dias = g["fecha"].dt.floor("D")
        for j, col in enumerate(columnas):
            s = pd.Series(rel[:, j], index=dias).groupby(level=0).median().dropna()
            bajo = s < umbral
            if not bajo.any():
                continue
            grupos = (bajo != bajo.shift()).cumsum()
            for _, tramo in s[bajo].groupby(grupos[bajo]):
                eventos.append({"inversor": nombre, "string": col, "desde": str(tramo.index[0].date()),
                                "hasta": str(tramo.index[-1].date()), "dias": int(len(tramo)), "relacion": float(tramo.mean())})
    return eventos


def detectar_sobretemperatura(d, limite=85.0, exceso=12.0):
    """Sobretemperatura de IGBT, por límite absoluto y frente a un modelo térmico:

    T_esp = T_amb + c0 + c1·P_ac  (ajuste robusto). Exceso diario: mediana(T - T_esp).
    """
    if "temp_igbt_c" not in d:
        return {"eventos": [], "modelo": None}
    eventos = []
    modelos = {}
    for nombre, g in d[d["p_ac_kw"] > 1.0].groupby("inversor"):
        y = (g["temp_igbt_c"] - g["temp_amb_c"]).to_numpy()
        x = g["p_ac_kw"].to_numpy()
        c1, c0 = np.polyfit(x, y, 1)
        for _ in range(3):
            r = y - (c0 + c1 * x)
            s = 1.4826 * np.median(np.abs(r - np.median(r)))
            ok = np.abs(r - np.median(r)) < 3 * s
            c1, c0 = np.polyfit(x[ok], y[ok], 1)
        modelos[nombre] = {"c0": float(c0), "c1": float(c1)}
        res = pd.Series(y - (c0 + c1 * x), index=g["fecha"].dt.floor("D")).groupby(level=0).median()
        tmax = g.groupby(g["fecha"].dt.floor("D"))["temp_igbt_c"].max()
        mal = (res > exceso) | (tmax > limite)
        grupos = (mal != mal.shift()).cumsum()
        for _, tr in res[mal].groupby(grupos[mal]):
            eventos.append({"inversor": nombre, "desde": str(tr.index[0].date()), "hasta": str(tr.index[-1].date()),
                            "dias": int(len(tr)), "exceso_c": float(tr.mean()), "t_max_c": float(tmax[tr.index].max())})
    return {"eventos": eventos, "modelo": modelos}


def desviacion_modelo(d, p_nom_kw, gamma=-0.0037, k=0.5, h=5.0):
    """Desviación frente al modelo físico: CUSUM unilateral de la razón diaria medido/esperado.

    z[i] = (r[i] - r_ref)/σ, con r_ref y σ robustos del primer tercio de los datos
    C[i] = max(0, -z[i] - k + C[i-1]);  alarma si C > h
    Solo avisa de caídas sostenidas por debajo del comportamiento inicial.
    """
    pr = pr_diario(d, p_nom_kw, gamma)
    v = pr.to_numpy()
    base = v[: max(30, len(v) // 3)]
    ref = float(np.median(base))
    sigma = max(float(1.4826 * np.median(np.abs(base - ref))), 1e-6)
    c = np.zeros_like(v)
    a = 0.0
    for i, x in enumerate(v):
        a = max(0.0, -(x - ref) / sigma - k + a)
        # Tras una alarma se reinicia, como al atender el aviso
        c[i] = a
        if a > 3 * h:
            a = 0.0
    alarma = np.where(c > h)[0].tolist()
    return {"razon": pr, "cusum": {"c_menos": c, "limite": h, "alarma": alarma, "centro": ref, "sigma": sigma}}


def tiempos_de_eventos(d):
    """Tiempos entre fallos y de reparación a partir de la columna en_servicio."""
    h = _horas_paso(d)
    tbf, ttr, ev = [], [], []
    for nombre, g in d.groupby("inversor"):
        s = (g["en_servicio"].to_numpy() > 0)
        f = g["fecha"].to_numpy()
        cambios = np.flatnonzero(np.diff(s.astype(int)))
        ini = 0
        for c in cambios:
            dur = (c + 1 - ini) * h
            if s[c]:
                tbf.append(dur)
            else:
                ttr.append(dur)
                ev.append({"inversor": nombre, "fin": str(f[c]), "horas": dur})
            ini = c + 1
    return np.asarray(tbf, dtype=float), np.asarray(ttr, dtype=float), ev


def fiabilidad(tbf, ttr):
    """MTBF = horas en servicio/fallos;  MTTR = horas parado/reparaciones;  A = MTBF/(MTBF+MTTR)."""
    tbf, ttr = np.asarray(tbf, dtype=float), np.asarray(ttr, dtype=float)
    mtbf = float(tbf.mean()) if tbf.size else float("nan")
    mttr = float(ttr.mean()) if ttr.size else float("nan")
    return {"mtbf_h": mtbf, "mttr_h": mttr, "disponibilidad": mtbf / (mtbf + mttr) if tbf.size and ttr.size else float("nan"),
            "fallos": int(tbf.size)}


def vida_remanente_weibull(beta, eta, t, nivel=0.5):
    """Vida remanente condicionada a haber sobrevivido hasta t.

    R(t + x | t) = exp(-(((t+x)/η)^β - (t/η)^β))
    Con R = nivel:  x = η·((t/η)^β - ln(nivel))^(1/β) - t.  Con nivel = 0.5: la mediana.
    """
    return eta * ((t / eta) ** beta - np.log(nivel)) ** (1.0 / beta) - t


def vida_media_weibull(beta, eta):
    """Vida media: MTTF = η·Γ(1 + 1/β)."""
    from scipy.special import gamma
    return float(eta * gamma(1 + 1 / beta))


def rul_degradacion(t, y, umbral, modelo="lineal"):
    """Vida remanente por tendencia de una señal de degradación.

    Lineal: y = a + b·t.   Exponencial: ln(y - φ) = ln(θ) + β·t (con φ = 0).
    RUL = tiempo hasta y(t) = umbral, medido desde el último dato.
    """
    t, y = np.asarray(t, dtype=float), np.asarray(y, dtype=float)
    if modelo == "lineal":
        b, a = np.polyfit(t, y, 1)
        cruce = (umbral - a) / b if b != 0 else np.inf
        pred = a + b * t
    else:
        b, a = np.polyfit(t, np.log(np.maximum(y, 1e-12)), 1)
        cruce = (np.log(umbral) - a) / b if b != 0 else np.inf
        pred = np.exp(a + b * t)
    rul = cruce - t[-1]
    return {"rul": float(rul) if rul > 0 else 0.0, "cruce": float(cruce), "pendiente": float(b), "pred": pred}


# ================================================================= Planificador


TIPOS_EQUIPO = ["Módulos", "Inversores", "Trackers", "Transformadores", "Cableado"]

PLANES_BASE = {
    "Módulos": [
        ("Limpieza de módulos", 30, ["Revisar nivel de suciedad", "Lavar con agua desmineralizada", "Medir el PR antes y después"]),
        ("Termografía de módulos", 180, ["Vuelo o recorrido con cámara a > 600 W/m²", "Registrar puntos calientes > 10 °C", "Abrir orden correctiva si hay diodos abiertos"]),
        ("Curva I-V de strings de muestra", 365, ["Medir curva I-V en un 5 % de strings", "Corregir a STC", "Comparar con el modelo físico"]),
    ],
    "Inversores": [
        ("Inspección de inversor", 90, ["Revisar alarmas y registro", "Limpiar filtros y ventiladores", "Comprobar temperatura de IGBT"]),
        ("Mantenimiento anual de inversor", 365, ["Reapretar conexiones de potencia", "Medir aislamiento DC", "Revisar condensadores del bus DC", "Actualizar firmware"]),
    ],
    "Trackers": [
        ("Revisión de trackers", 180, ["Comprobar actuadores y motores", "Verificar el ángulo frente al controlador", "Lubricar rodamientos"]),
    ],
    "Transformadores": [
        ("Inspección de transformador", 90, ["Nivel y temperatura del aceite", "Fugas y estado de silicagel", "Ruido y vibración"]),
        ("Análisis de aceite", 365, ["Tomar muestra", "Gases disueltos y rigidez dieléctrica", "Registrar tendencia"]),
    ],
    "Cableado": [
        ("Revisión de cableado", 365, ["Termografía de cajas de string y conectores", "Resistencia de aislamiento", "Reapretar bornes y revisar fusibles"]),
    ],
}

AMEF_BASE = {
    "Módulos": [
        ("Punto caliente", "Celda agrietada o sombra", "Pérdida de potencia, riesgo de incendio", 8, 4, 5, "Termografía semestral"),
        ("Degradación inducida (PID)", "Potencial alto a tierra", "Pérdida de potencia gradual", 7, 3, 6, "Curva I-V anual; puesta a tierra"),
        ("Suciedad", "Polvo y falta de lluvia", "Baja de producción", 4, 8, 2, "Limpieza por PR"),
        ("Diodo de bypass abierto", "Sobrecorriente, rayo", "Punto caliente y pérdida de sub-módulo", 7, 3, 6, "Termografía semestral"),
    ],
    "Inversores": [
        ("Sobretemperatura de IGBT", "Filtros sucios, ventilador averiado", "Limitación o parada", 7, 5, 3, "Inspección trimestral; vigilancia de temperatura"),
        ("Fallo de condensador de bus DC", "Envejecimiento térmico", "Parada del inversor", 8, 4, 5, "Medir capacidad cada año"),
        ("Fallo de IGBT", "Ciclado térmico", "Parada del inversor", 9, 3, 5, "Vigilar temperatura y energía de conmutación"),
        ("Fallo de aislamiento DC", "Cable dañado, humedad", "Parada y riesgo eléctrico", 9, 3, 4, "Medida anual"),
    ],
    "Trackers": [
        ("Actuador bloqueado", "Desgaste, polvo", "Pérdida de producción", 6, 4, 3, "Revisión semestral"),
        ("Ángulo incorrecto", "Sensor o controlador", "Pérdida de producción", 5, 4, 4, "Comparar con el modelo solar"),
    ],
    "Transformadores": [
        ("Degradación del aceite", "Humedad y temperatura", "Falla dieléctrica", 9, 3, 5, "Análisis de gases anual"),
        ("Sobrecarga térmica", "Refrigeración insuficiente", "Envejecimiento acelerado", 8, 3, 3, "Vigilar temperatura"),
    ],
    "Cableado": [
        ("Conector MC4 caliente", "Mal crimpado", "Pérdida de string, incendio", 8, 4, 5, "Termografía anual"),
        ("Fallo de aislamiento", "Roedores, UV", "Fallo a tierra", 8, 3, 4, "Aislamiento anual"),
    ],
}


def estado_inicial(hoy=None):
    """Plan de mantenimiento de ejemplo con catálogo, planes y AMEF."""
    hoy = hoy or dt.date.today().isoformat()
    equipos = []
    ejemplos = [("MOD-A", "Módulos", "Campo A (12 x 20 CS1H-330MS)"), ("MOD-B", "Módulos", "Campo B (12 x 20 CS1H-330MS)"),
                ("INV-1", "Inversores", "Inversor central 500 kW"), ("INV-2", "Inversores", "Inversor central 500 kW"),
                ("TRK-01", "Trackers", "Fila de trackers 01"), ("TR-01", "Transformadores", "Transformador 1 MVA 0.4/20 kV"),
                ("CAB-01", "Cableado", "Cajas de string, campo A")]
    for i, (cod, tipo, desc) in enumerate(ejemplos):
        equipos.append({"codigo": cod, "tipo": tipo, "descripcion": desc, "ubicacion": "Planta", "instalado": "2024-01-15"})
    planes = []
    for tipo, lista in PLANES_BASE.items():
        for nombre, frec, chk in lista:
            planes.append({"id": f"P{len(planes) + 1:03d}", "nombre": nombre, "tipo_equipo": tipo, "frecuencia_dias": frec, "checklist": chk,
                           "inicio": hoy, "activo": True})
    amef = []
    for tipo, lista in AMEF_BASE.items():
        for modo, causa, efecto, s, o, d_, ctrl in lista:
            amef.append({"tipo_equipo": tipo, "modo": modo, "causa": causa, "efecto": efecto, "S": s, "O": o, "D": d_, "control": ctrl, "accion": ""})
    return {"formato": "heliora-mantenimiento/1", "equipos": equipos, "planes": planes, "ordenes": [], "historial": [], "amef": amef, "contador": 0}


def rpn(fila):
    """Número de prioridad de riesgo: RPN = S·O·D (severidad, ocurrencia, detección; 1 a 10)."""
    return int(fila["S"]) * int(fila["O"]) * int(fila["D"])


def amef_por_equipo(estado, tipo=None):
    filas = [dict(f, RPN=rpn(f)) for f in estado["amef"] if tipo in (None, "", f["tipo_equipo"])]
    return sorted(filas, key=lambda f: -f["RPN"])


def generar_ordenes(estado, hasta):
    """Crea las órdenes de trabajo que tocan hasta la fecha, según la frecuencia de cada plan.

    Cada combinación plan-equipo avanza desde su última fecha (real o programada) sumando
    frecuencia_dias. No duplica órdenes ya creadas. Devuelve el número de órdenes nuevas.
    """
    hasta = dt.date.fromisoformat(str(hasta)[:10])
    nuevas = 0
    ya = {(o["plan"], o["equipo"], o["fecha"]) for o in estado["ordenes"]}
    for p in estado["planes"]:
        if not p.get("activo", True):
            continue
        for e in estado["equipos"]:
            if e["tipo"] != p["tipo_equipo"]:
                continue
            ultima = None
            for o in estado["ordenes"]:
                if o["plan"] == p["id"] and o["equipo"] == e["codigo"]:
                    f = dt.date.fromisoformat(o["fecha"])
                    ultima = f if ultima is None or f > ultima else ultima
            fecha = dt.date.fromisoformat(p["inicio"][:10]) if ultima is None else ultima + dt.timedelta(days=int(p["frecuencia_dias"]))
            while fecha <= hasta:
                clave = (p["id"], e["codigo"], fecha.isoformat())
                if clave not in ya:
                    estado["contador"] += 1
                    estado["ordenes"].append({
                        "id": f"OT-{estado['contador']:05d}", "plan": p["id"], "nombre": p["nombre"], "equipo": e["codigo"],
                        "fecha": fecha.isoformat(), "estado": "pendiente", "responsable": "",
                        "checklist": [{"texto": c, "hecho": False} for c in p["checklist"]], "notas": "", "cerrada": ""})
                    ya.add(clave)
                    nuevas += 1
                fecha += dt.timedelta(days=int(p["frecuencia_dias"]))
    estado["ordenes"].sort(key=lambda o: (o["fecha"], o["id"]))
    return nuevas


def cerrar_orden(estado, ident, fecha=None, notas=""):
    """Cierra una orden y la pasa al historial."""
    for o in estado["ordenes"]:
        if o["id"] == ident:
            o["estado"] = "cerrada"
            o["cerrada"] = fecha or dt.date.today().isoformat()
            if notas:
                o["notas"] = notas
            estado["historial"].append({"orden": o["id"], "equipo": o["equipo"], "trabajo": o["nombre"], "fecha": o["cerrada"], "notas": o["notas"],
                                        "completo": all(c["hecho"] for c in o["checklist"])})
            return o
    raise KeyError(ident)


def tablero(estado, hoy=None):
    """Resumen: pendientes, vencidas, próximas 30 días y cumplimiento del plan."""
    hoy = dt.date.fromisoformat(hoy) if hoy else dt.date.today()
    pend = [o for o in estado["ordenes"] if o["estado"] != "cerrada"]
    venc = [o for o in pend if dt.date.fromisoformat(o["fecha"]) < hoy]
    prox = [o for o in pend if hoy <= dt.date.fromisoformat(o["fecha"]) <= hoy + dt.timedelta(days=30)]
    cerr = [o for o in estado["ordenes"] if o["estado"] == "cerrada"]
    cumpl = len(cerr) / max(len(cerr) + len(venc), 1)
    return {"pendientes": len(pend), "vencidas": len(venc), "proximas_30": len(prox), "cerradas": len(cerr), "cumplimiento": cumpl}


def ordenes_de_alarmas(estado, eventos, hoy=None):
    """Crea órdenes correctivas a partir de fallas detectadas por el análisis."""
    hoy = hoy or dt.date.today().isoformat()
    n = 0
    for ev in eventos:
        estado["contador"] += 1
        estado["ordenes"].append({"id": f"OT-{estado['contador']:05d}", "plan": "CORR", "nombre": ev["trabajo"], "equipo": ev["equipo"], "fecha": hoy,
                                  "estado": "pendiente", "responsable": "", "checklist": [{"texto": t, "hecho": False} for t in ev.get("checklist", ["Diagnosticar", "Reparar", "Verificar"])],
                                  "notas": ev.get("notas", ""), "cerrada": ""})
        n += 1
    return n


def guardar_estado(estado, ruta):
    tmp = str(ruta) + ".tmp"
    with open(tmp, "w", encoding="utf8") as f:
        json.dump(estado, f, ensure_ascii=False, indent=1)
    os.replace(tmp, ruta)


def cargar_estado(ruta):
    with open(ruta, encoding="utf8") as f:
        return json.load(f)


def copiar(estado):
    return copy.deepcopy(estado)
