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
import csv
import datetime as dt
import json
import os
import re
import unicodedata

import numpy as np
import pandas as pd

from ..lectura import a_fechas, leer_tabla

ALIAS = {
    "fecha": ["fecha", "timestamp", "datetime", "fecha_hora", "date", "time", "tiempo", "hora"],
    "inversor": ["inversor", "inverter", "equipo", "id_inversor"],
    "irradiancia_wm2": ["irradiancia_wm2", "irradiancia", "irradiancia_poa", "poa", "ghi", "g_poa", "g"],
    "temp_amb_c": ["temp_amb_c", "t_amb", "tamb", "temp_amb", "temp_ambiente", "temperatura_ambiente"],
    "temp_modulo_c": ["temp_modulo_c", "t_modulo", "tmod", "temp_modulo", "temperatura_modulo"],
    "p_dc_kw": ["p_dc_kw", "pdc", "p_dc", "potencia_dc"],
    "p_ac_kw": ["p_ac_kw", "pac", "p_ac", "potencia_ac", "potencia"],
    "v_dc_v": ["v_dc_v", "vdc", "v_dc", "tension_dc"],
    "i_dc_a": ["i_dc_a", "idc", "i_dc", "corriente_dc"],
    "v_ac_v": ["v_ac_v", "vac", "v_ac", "tension_ac"],
    "i_ac_a": ["i_ac_a", "iac", "i_ac", "corriente_ac"],
    "temp_igbt_c": ["temp_igbt_c", "t_igbt", "tigbt", "temp_igbt", "temperatura_igbt"],
    "alarma": ["alarma", "alarm", "alarmas", "codigo_alarma"],
    "en_servicio": ["en_servicio", "disponible", "online", "servicio", "estado_inversor"],
}
# Columna de hora aceptada cuando la fecha viene en otra columna
ALIAS_HORA = ["hora", "time", "hour", "hora_local"]
# Nombres de columna que contienen solo la fecha y, por tanto, admiten una columna de hora aparte
_SOLO_FECHA = {"fecha", "date"}
_NO_NUMERICAS = {"fecha", "inversor", "alarma", "en_servicio"}
_SI = {"on", "true", "t", "si", "s", "yes", "y", "1", "ok", "online", "run", "running", "servicio", "en servicio"}
_NO = {"off", "false", "f", "no", "n", "0", "offline", "stop", "stopped", "fault", "fallo", "parado", "fuera de servicio"}


def _sin_acentos(texto):
    """Texto sin marcas diacríticas («sí» pasa a «si»)."""
    return "".join(ch for ch in unicodedata.normalize("NFKD", texto) if not unicodedata.combining(ch))


def _clave(nombre):
    """Normaliza un nombre de columna: sin unidades entre paréntesis, sin acentos, en minúsculas y con «_»."""
    t = re.sub(r"[\(\[][^\)\]]*[\)\]]", " ", str(nombre).strip().lower())
    t = _sin_acentos(t)
    return re.sub(r"[^a-z0-9]+", "_", t).strip("_")


def _columna(claves, alias):
    """Primera columna original cuyo nombre normalizado coincide con algún alias, o None."""
    for a in alias:
        if _clave(a) in claves:
            return claves[_clave(a)]
    return None


def nombres_aceptados(columna):
    """Nombres con los que se reconoce una columna normal, para los mensajes de error."""
    return ", ".join(ALIAS.get(columna, [columna]))


def requerir_columnas(d, columnas, uno_de=()):
    """Comprueba que la tabla tiene las columnas normales necesarias.

    Args:
        d: tabla SCADA ya normalizada.
        columnas: nombres que deben estar todos.
        uno_de: tuplas de nombres de las que debe estar al menos uno.

    Raises:
        ValueError: «Falta la columna X (nombres aceptados: ...)» con la primera que falte.
    """
    for c in columnas:
        if c not in d.columns:
            raise ValueError(f"Falta la columna {c} (nombres aceptados: {nombres_aceptados(c)})")
    for grupo in uno_de:
        if not any(c in d.columns for c in grupo):
            opciones = " o ".join(f"{c} (nombres aceptados: {nombres_aceptados(c)})" for c in grupo)
            raise ValueError(f"Falta una de estas columnas: {opciones}")


def requerir_scada(d, aislamiento=False):
    """Comprueba las columnas que necesitan los KPI, la detección de fallas y la fiabilidad.

    Args:
        d: tabla SCADA normalizada.
        aislamiento: si es True y hay temperatura de IGBT, exige también la temperatura ambiente.

    Raises:
        ValueError: con la columna que falta y los nombres con que se acepta.
    """
    requerir_columnas(d, ["fecha", "inversor", "irradiancia_wm2", "p_ac_kw"], uno_de=[("temp_modulo_c", "temp_amb_c")])
    if aislamiento and "temp_igbt_c" in d.columns:
        requerir_columnas(d, ["temp_amb_c"])


def _a_servicio(serie, p_ac=None):
    """Convierte la columna en_servicio (ON/OFF, sí/no, true/false, 0/1) en enteros 0/1.

    Los valores vacíos se completan con `p_ac > 0` cuando hay potencia y con 0 si no.

    Returns:
        Tupla (serie de 0/1, número de valores vacíos completados).

    Raises:
        ValueError: si hay textos que no corresponden a un estado conocido.
    """
    relleno = (p_ac > 0).astype(int) if p_ac is not None else pd.Series(0, index=serie.index)
    if pd.api.types.is_bool_dtype(serie):
        return serie.astype(int), 0
    if pd.api.types.is_numeric_dtype(serie):
        vacias = serie.isna()
        return (serie.fillna(0) > 0).astype(int).where(~vacias, relleno), int(vacias.sum())
    texto = serie.astype(str).str.strip().str.lower().map(_sin_acentos).where(serie.notna(), "")
    valores = texto.map(lambda t: 1 if t in _SI else 0 if t in _NO else (-2 if t == "" else -1))
    raros = sorted(set(serie.astype(str).str.strip()[valores == -1]))
    if raros:
        raise ValueError(f"La columna en_servicio tiene valores que no se reconocen: {', '.join(raros[:5])} "
                         "(se admiten ON/OFF, true/false, sí/no y 1/0)")
    vacias = valores == -2
    return valores.where(~vacias, relleno).astype(int), int(vacias.sum())


def _fechas_del_archivo(d, claves):
    """Columna de fechas del archivo, uniendo fecha y hora cuando vienen en columnas separadas.

    Returns:
        Tupla (columna de fecha, fechas interpretadas, columna de hora usada o None).
    """
    col = _columna(claves, ALIAS["fecha"])
    if col is None:
        raise ValueError("El archivo no tiene una columna de fecha reconocible "
                         f"(nombres aceptados: {nombres_aceptados('fecha')})")
    hora = _columna({k: v for k, v in claves.items() if v != col}, ALIAS_HORA)
    if hora is not None and _clave(col) in _SOLO_FECHA:
        f = d[col]
        f = f.dt.strftime("%Y-%m-%d") if pd.api.types.is_datetime64_any_dtype(f) else f.astype(str).str.strip()
        h = d[hora]
        if pd.api.types.is_numeric_dtype(h):
            h = h.astype(int).map(lambda x: f"{x:02d}:00")
        return col, a_fechas(f + " " + h.astype(str).str.strip()), hora
    return col, a_fechas(d[col]), None


def _normalizar(df):
    """Normaliza la tabla y devuelve (tabla, {"duplicados": n, "avisos": [...]})."""
    claves = {}
    for c in df.columns:
        claves.setdefault(_clave(c), c)
    col_fecha, fechas, col_hora = _fechas_del_archivo(df, claves)
    libres = {k: v for k, v in claves.items() if v not in (col_fecha, col_hora)}
    nombres = {}
    for destino, alias in ALIAS.items():
        orig = None if destino == "fecha" else _columna(libres, alias)
        if orig is not None:
            nombres[orig] = destino
    d = df.drop(columns=[col_hora] if col_hora is not None else []).rename(columns=nombres).copy()
    d = d.rename(columns={col_fecha: "fecha"})
    d["fecha"] = fechas.to_numpy()
    avisos = []
    if "inversor" not in d:
        d["inversor"] = "INV-1"
    d["inversor"] = d["inversor"].astype(str).str.strip()
    for c in d.columns:
        if (c in ALIAS or str(c).startswith("i_s")) and c not in _NO_NUMERICAS and d[c].dtype == object:
            antes = int(d[c].isna().sum())
            num = pd.to_numeric(d[c].astype(str).str.replace(",", ".", regex=False), errors="coerce")
            nuevas = int(num.isna().sum()) - antes
            if nuevas > 0:
                avisos.append(f"{nuevas} valores de «{c}» no son números y se tomaron como faltantes.")
            d[c] = num
    if "en_servicio" in d:
        d["en_servicio"], vacios = _a_servicio(d["en_servicio"], d["p_ac_kw"] if "p_ac_kw" in d else None)
        if vacios:
            avisos.append(f"{vacios} valores vacíos de en_servicio se completaron con p_ac_kw > 0.")
    n0 = len(d)
    d = d.drop_duplicates(["fecha", "inversor"], keep="first")
    duplicados = n0 - len(d)
    if duplicados:
        avisos.append(f"{duplicados} filas repetidas (misma fecha e inversor) se eliminaron; se conservó la primera.")
    return d.sort_values(["fecha", "inversor"]).reset_index(drop=True), {"duplicados": duplicados, "avisos": avisos}


def normalizar_columnas(df):
    """Renombra columnas a sus nombres normales, une fecha y hora, convierte tipos y elimina duplicados.

    Args:
        df: tabla leída del archivo, con nombres de columna libres (admite unidades y acentos).

    Returns:
        Tabla ordenada por fecha e inversor, sin repetidos (fecha, inversor).

    Raises:
        ValueError: si no hay columna de fecha, una fecha no se interpreta o en_servicio tiene valores desconocidos.
    """
    return _normalizar(df)[0]


def importar_scada(ruta):
    """Lee un CSV, TSV o Parquet de SCADA y devuelve los datos normalizados y un resumen de calidad.

    Args:
        ruta: archivo con las lecturas (admite CSV de Excel en español).

    Returns:
        Tupla (tabla, resumen). El resumen es el de `resumen_calidad` más `duplicados` y `avisos`.

    Raises:
        ValueError: si el archivo no se lee, falta la fecha o el paso temporal no es positivo.
    """
    d, info = _normalizar(leer_tabla(ruta))
    q = resumen_calidad(d)
    if len(q["pasos_min"]) < q["inversores"]:
        raise ValueError("No se puede determinar el paso temporal: hace falta más de una lectura por inversor con fechas distintas")
    q.update(info)
    return d, q


def resumen_calidad(d):
    """Resume filas, inversores, fechas, paso de muestreo, faltantes, columnas y cobertura temporal.

    Cada inversor se mide con su propio paso mediano: la cobertura compara sus lecturas con las
    esperadas entre su primera y su última fecha, y un salto de más de 1.5 pasos es un hueco.

    Returns:
        Diccionario con `paso_min` (el paso común si todos coinciden; si no, el mayor), `pasos_min`
        (por inversor), `paso_regular` (False si en algún inversor menos del 90 % de los saltos son
        su paso), `cobertura` (0 a 1), `esperadas`, `huecos_total` y `huecos` (hasta 20, con
        `inversor`, `desde`, `hasta` y `perdidas`).
    """
    faltan = {c: int(d[c].isna().sum()) for c in d.columns if d[c].isna().any()}
    esperadas, huecos, pasos, regular = 0, [], {}, True
    for nombre, g in d.groupby("inversor"):
        f = g["fecha"].sort_values().reset_index(drop=True)
        saltos = f.diff().dropna()
        paso = saltos.median() if len(saltos) else pd.NaT
        if pd.isna(paso) or paso.total_seconds() <= 0:
            continue
        pasos[str(nombre)] = float(paso.total_seconds() / 60.0)
        regular &= bool((abs(saltos - paso) <= paso * 0.1).mean() >= 0.9)
        esperadas += int(round((f.iloc[-1] - f.iloc[0]) / paso)) + 1
        for i in np.flatnonzero(f.diff() > paso * 1.5):
            huecos.append({"inversor": str(nombre), "desde": str(f.iloc[i - 1]), "hasta": str(f.iloc[i]),
                           "perdidas": int(round((f.iloc[i] - f.iloc[i - 1]) / paso)) - 1})
    return {"filas": int(len(d)), "inversores": int(d["inversor"].nunique()), "desde": str(d["fecha"].min()),
            "hasta": str(d["fecha"].max()), "paso_min": max(pasos.values()) if pasos else 0.0, "pasos_min": pasos,
            "paso_regular": regular, "faltantes": faltan, "columnas": [str(c) for c in d.columns],
            "esperadas": esperadas, "cobertura": float(min(len(d) / esperadas, 1.0)) if esperadas else 0.0,
            "huecos_total": len(huecos), "huecos": huecos[:20]}


def horas_por_lectura(d):
    """Horas que representa cada lectura: el paso mediano de su propio inversor.

    Un inversor que registra cada 20 minutos aporta 1/3 h por lectura aunque otro registre cada 10.

    Returns:
        Serie alineada con `d`.

    Raises:
        ValueError: si algún inversor no tiene un paso positivo (fechas repetidas o una sola lectura).
    """
    paso = d.sort_values("fecha").groupby("inversor")["fecha"].diff().groupby(d["inversor"]).median()
    malos = [str(n) for n, p in paso.items() if pd.isna(p) or p.total_seconds() <= 0]
    if malos:
        raise ValueError("No se puede determinar el paso temporal de " + ", ".join(malos)
                         + " (fechas repetidas o una sola lectura)")
    return d["inversor"].map(paso.dt.total_seconds() / 3600.0).astype(float)


def generar_scada(inicio="2024-01-01", dias=730, paso_min=30, inversores=2, p_nom_kw=500.0, semilla=7, lat=40.4, lon=-3.7,
                  degradacion_anual=0.007, suciedad_dia=0.002, fallas=True):
    """Datos SCADA de muestra con fallas conocidas (para aprender y validar el análisis).

    Inyecta: degradación anual, suciedad en diente de sierra con lluvias, un string caído,
    episodios de sobretemperatura de IGBT y paradas de inversor.
    Devuelve (datos, verdad), donde verdad describe lo inyectado: solo los eventos que caen dentro
    de los días generados (con muestras cortas no hay string caído ni sobretemperatura).
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
        if fallas and iv == 1 and dias > 200:
            factor_strings[(dia >= 200) & (dia < 215), 2] = 0.0
            verdad["string_caido"] = {"inversor": nombre, "string": 3, "dia_ini": 200, "dia_fin": min(215, dias),
                                      "desde": str(t[0].normalize() + pd.Timedelta(days=200)),
                                      "hasta": str(t[0].normalize() + pd.Timedelta(days=min(215, dias)))}
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
        if fallas and iv == 0 and dias > 400:
            sel = (dia >= 400) & (dia < 412)
            T_igbt = np.where(sel, T_igbt + 28.0, T_igbt)
            verdad["igbt"] = {"inversor": nombre, "dia_ini": 400, "dia_fin": min(412, dias), "exceso_c": 28.0,
                              "desde": str(t[0].normalize() + pd.Timedelta(days=400)),
                              "hasta": str(t[0].normalize() + pd.Timedelta(days=min(412, dias)))}
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
    """KPIs globales de la planta y por inversor. p_nom_kw es la potencia por inversor.

    La energía integra cada lectura con el paso de su inversor. La energía de los huecos no se
    estima: solo cuenta lo medido, y `cobertura` dice qué fracción del periodo representa.
    Sin potencia DC, `eta_inversor` es NaN.

    Returns:
        (totales, tabla por inversor). Los totales añaden `cobertura`, `paso_regular`, `pasos_min`
        y `hay_dc`.
    """
    requerir_columnas(d, ["fecha", "inversor", "irradiancia_wm2", "p_ac_kw"])
    hl = horas_por_lectura(d)
    q = resumen_calidad(d)
    hay_dc = "p_dc_kw" in d.columns and d["p_dc_kw"].notna().any()
    filas = []
    for nombre, g in d.groupby("inversor"):
        h = hl.loc[g.index]
        H = (g["irradiancia_wm2"] * h).sum() / 1000.0  # kWh/m²
        E_ac = (g["p_ac_kw"] * h).sum()
        E_dc = (g["p_dc_kw"] * h).sum() if hay_dc else float("nan")
        q_inv = resumen_calidad(g)
        sol = g["irradiancia_wm2"] > g_min
        serv = (g["en_servicio"] > 0) if "en_servicio" in g else (g["p_ac_kw"] > 0)
        esp = potencia_esperada(g, p_nom_kw, gamma, eta_inv)
        perdida = np.maximum(esp - g["p_ac_kw"], 0.0)
        filas.append({"inversor": nombre, "PR": E_ac / (p_nom_kw * H) if H > 0 else 0.0,
                      "disponibilidad": float(serv[sol].mean()) if sol.any() else 0.0,
                      "energia_kwh": float(E_ac), "energia_perdida_kwh": float((perdida * h).sum()),
                      "perdida_parada_kwh": float((perdida * h)[~serv].sum()),
                      "eta_inversor": float(E_ac / E_dc) if hay_dc and E_dc else float("nan"),
                      "insolacion_kwh_m2": float(H), "paso_min": q["pasos_min"].get(str(nombre), float("nan")),
                      "cobertura": q_inv["cobertura"], "energia_dc_kwh": float(E_dc)})
    t = pd.DataFrame(filas)
    H_tot = t["insolacion_kwh_m2"].sum()
    E_dc_tot = t["energia_dc_kwh"].sum() if hay_dc else float("nan")
    tot = {"PR": float((t["PR"] * t["insolacion_kwh_m2"]).sum() / H_tot) if H_tot > 0 else float("nan"),
           "disponibilidad": float(t["disponibilidad"].mean()), "energia_kwh": float(t["energia_kwh"].sum()),
           "energia_perdida_kwh": float(t["energia_perdida_kwh"].sum()),
           "eta_inversor": float(t["energia_kwh"].sum() / E_dc_tot) if hay_dc and E_dc_tot else float("nan"),
           "cobertura": q["cobertura"], "paso_regular": q["paso_regular"], "pasos_min": q["pasos_min"], "hay_dc": bool(hay_dc)}
    return tot, t


def pr_diario(d, p_nom_kw, gamma=-0.0037, g_min=300.0, eta_inv=1.0):
    """Índice de rendimiento diario corregido por temperatura, con G > g_min y en servicio."""
    requerir_scada(d)
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
    """Sobretemperatura de IGBT, por límite absoluto y frente a un modelo térmico.

    Modelo: T_esp = T_amb + c0 + c1·P_ac  (ajuste robusto). Exceso diario: mediana(T - T_esp).
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
    """Tiempos entre fallos y de reparación a partir de la columna en_servicio (o de p_ac_kw > 0 si falta).

    Raises:
        ValueError: si no hay ni en_servicio ni p_ac_kw, o el paso temporal no es válido.
    """
    requerir_columnas(d, ["fecha", "inversor"])
    if "en_servicio" not in d.columns:
        requerir_columnas(d, ["p_ac_kw"])
    hl = horas_por_lectura(d)
    tbf, ttr, ev = [], [], []
    for nombre, g in d.groupby("inversor"):
        h = float(hl.loc[g.index].iloc[0])
        s = (g["en_servicio"].to_numpy() > 0) if "en_servicio" in g else (g["p_ac_kw"].to_numpy() > 0)
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


FORMATO = "heliora-mantenimiento/1"
TIPOS_ORDEN = ["correctiva", "preventiva", "predictiva", "inspección"]
ESTADOS_ORDEN = ("pendiente", "cerrada")
COLUMNAS_AMEF = ["Equipo", "Modo de falla", "Causa", "Efecto", "S", "O", "D", "RPN", "Control", "Acción"]
COLUMNAS_HISTORIAL = ["Fecha", "Orden", "Equipo", "Trabajo", "Lista completa", "Notas"]


def fecha_valida(texto, campo="fecha"):
    """Interpreta una fecha AAAA-MM-DD estricta.

    Args:
        texto: fecha escrita.
        campo: nombre de la fecha para el mensaje de error.

    Returns:
        `datetime.date` de la fecha.

    Raises:
        ValueError: si el formato no es AAAA-MM-DD o el día no existe.
    """
    t = str(texto).strip()
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", t):
        raise ValueError(f"La {campo} «{texto}» no tiene el formato AAAA-MM-DD")
    try:
        return dt.date.fromisoformat(t)
    except ValueError:
        raise ValueError(f"La {campo} «{texto}» no existe en el calendario") from None


def _hoy(hoy):
    """Fecha de referencia como `date`; la del sistema si no se indica."""
    return fecha_valida(hoy, "fecha de hoy") if hoy else dt.date.today()


def validar_plan(p):
    """Comprueba que un plan se puede usar para generar órdenes.

    Args:
        p: diccionario del plan.

    Raises:
        ValueError: con el nombre del plan y el motivo (clave ausente, frecuencia no positiva, fecha inválida).
    """
    nombre = p.get("nombre") or p.get("id") or "sin nombre"
    try:
        for k in ("id", "nombre", "tipo_equipo", "frecuencia_dias", "inicio"):
            if k not in p:
                raise ValueError(f"falta la clave «{k}»")
        f = p["frecuencia_dias"]
        if isinstance(f, bool) or not isinstance(f, (int, float)) or f != int(f) or f < 1:
            raise ValueError("la frecuencia debe ser un número entero de días mayor que 0")
        fecha_valida(p["inicio"], "fecha de inicio")
        if not isinstance(p.get("checklist", []), list):
            raise ValueError("la lista de comprobación debe ser una lista")
    except ValueError as e:
        raise ValueError(f"Plan «{nombre}»: {e}") from None


def validar_estado(e):
    """Comprueba la estructura de un plan de mantenimiento cargado de un archivo y completa lo opcional.

    Los problemas de valores de un plan o de un equipo (frecuencia, fechas) no impiden abrirlo, para
    que se puedan corregir; se devuelven como avisos. La estructura sí es obligatoria.

    Args:
        e: contenido del archivo ya leído como JSON.

    Returns:
        Tupla (estado completado, lista de avisos).

    Raises:
        ValueError: si el formato no es heliora-mantenimiento/1, falta una clave obligatoria, hay
            identificadores repetidos o una orden tiene una fecha o un estado inválidos.
    """
    if not isinstance(e, dict):
        raise ValueError("El archivo no es un plan de mantenimiento (se esperaba un objeto JSON)")
    if e.get("formato") != FORMATO:
        raise ValueError(f"Formato no reconocido: «{e.get('formato')}» (se espera {FORMATO})")
    for k in ("equipos", "planes", "ordenes"):
        if not isinstance(e.get(k), list):
            raise ValueError(f"Falta la clave obligatoria «{k}» (una lista) en el plan")
    e.setdefault("historial", [])
    e.setdefault("amef", [])
    for k in ("historial", "amef"):
        if not isinstance(e[k], list):
            raise ValueError(f"La clave «{k}» debe ser una lista")
    avisos = []
    codigos = set()
    for q in e["equipos"]:
        if not isinstance(q, dict) or not str(q.get("codigo", "")).strip() or "tipo" not in q:
            raise ValueError("Cada equipo necesita «codigo» y «tipo»")
        if q["codigo"] in codigos:
            raise ValueError(f"Código de equipo repetido: {q['codigo']}")
        codigos.add(q["codigo"])
        for k in ("descripcion", "ubicacion"):
            q.setdefault(k, "")
        q.setdefault("instalado", "")
        if q["instalado"]:
            try:
                fecha_valida(q["instalado"], "fecha de instalación")
            except ValueError as err:
                avisos.append(f"Equipo «{q['codigo']}»: {err}")
    ids = set()
    for p in e["planes"]:
        if not isinstance(p, dict) or "id" not in p:
            raise ValueError("Cada plan necesita «id»")
        if p["id"] in ids:
            raise ValueError(f"Identificador de plan repetido: {p['id']}")
        ids.add(p["id"])
        p.setdefault("checklist", [])
        p.setdefault("activo", True)
        try:
            validar_plan(p)
        except ValueError as err:
            avisos.append(str(err))
    ords = set()
    for o in e["ordenes"]:
        if not isinstance(o, dict) or any(k not in o for k in ("id", "plan", "nombre", "equipo", "fecha", "estado")):
            raise ValueError("Cada orden necesita «id», «plan», «nombre», «equipo», «fecha» y «estado»")
        if o["id"] in ords:
            raise ValueError(f"Identificador de orden repetido: {o['id']}")
        ords.add(o["id"])
        if o["estado"] not in ESTADOS_ORDEN:
            raise ValueError(f"La orden {o['id']} tiene un estado desconocido: «{o['estado']}»")
        fecha_valida(o["fecha"], f"fecha de la orden {o['id']}")
        o.setdefault("responsable", "")
        o.setdefault("checklist", [])
        o.setdefault("notas", "")
        o.setdefault("cerrada", "")
    for f in e["amef"]:
        if not isinstance(f, dict) or any(k not in f for k in ("tipo_equipo", "modo", "S", "O", "D")):
            raise ValueError("Cada modo de falla del AMEF necesita «tipo_equipo», «modo», «S», «O» y «D»")
        for k in ("S", "O", "D"):
            if isinstance(f[k], bool) or not isinstance(f[k], int) or not 1 <= f[k] <= 10:
                raise ValueError(f"El modo de falla «{f['modo']}» tiene {k} fuera de 1 a 10")
        for k in ("causa", "efecto", "control", "accion"):
            f.setdefault(k, "")
    numeros = [int(m.group(1)) for o in e["ordenes"] if (m := re.fullmatch(r"OT-(\d+)", str(o["id"])))]
    c = e.get("contador")
    e["contador"] = max(c if isinstance(c, int) and not isinstance(c, bool) else 0, *numeros, 0)
    return e, avisos


def estado_vacio():
    """Plan de mantenimiento sin equipos, planes, órdenes ni AMEF."""
    return {"formato": FORMATO, "equipos": [], "planes": [], "ordenes": [], "historial": [], "amef": [], "contador": 0}


def estado_inicial(hoy=None):
    """Plan de mantenimiento de ejemplo con catálogo, planes y AMEF.

    El inicio de cada plan se escalona 3 días respecto al anterior para que las órdenes no caigan el mismo día.
    """
    hoy = _hoy(hoy)
    equipos = []
    ejemplos = [("MOD-A", "Módulos", "Campo A (12 x 20 CS1H-330MS)"), ("MOD-B", "Módulos", "Campo B (12 x 20 CS1H-330MS)"),
                ("INV-1", "Inversores", "Inversor central 500 kW"), ("INV-2", "Inversores", "Inversor central 500 kW"),
                ("TRK-01", "Trackers", "Fila de trackers 01"), ("TR-01", "Transformadores", "Transformador 1 MVA 0.4/20 kV"),
                ("CAB-01", "Cableado", "Cajas de string, campo A")]
    for cod, tipo, desc in ejemplos:
        equipos.append({"codigo": cod, "tipo": tipo, "descripcion": desc, "ubicacion": "Planta", "instalado": "2024-01-15"})
    planes = []
    for tipo, lista in PLANES_BASE.items():
        for nombre, frec, chk in lista:
            planes.append({"id": f"P{len(planes) + 1:03d}", "nombre": nombre, "tipo_equipo": tipo, "frecuencia_dias": frec, "checklist": chk,
                           "inicio": (hoy + dt.timedelta(days=3 * len(planes))).isoformat(), "activo": True})
    amef = []
    for tipo, lista in AMEF_BASE.items():
        for modo, causa, efecto, s, o, d_, ctrl in lista:
            amef.append({"tipo_equipo": tipo, "modo": modo, "causa": causa, "efecto": efecto, "S": s, "O": o, "D": d_, "control": ctrl, "accion": ""})
    return {"formato": FORMATO, "equipos": equipos, "planes": planes, "ordenes": [], "historial": [], "amef": amef, "contador": 0}


def rpn(fila):
    """Número de prioridad de riesgo: RPN = S·O·D (severidad, ocurrencia, detección; 1 a 10)."""
    return int(fila["S"]) * int(fila["O"]) * int(fila["D"])


def amef_por_equipo(estado, tipo=None):
    """Devuelve los modos de fallo del AMEF con su RPN, filtrados por tipo de equipo y ordenados de mayor a menor RPN."""
    filas = [dict(f, RPN=rpn(f)) for f in estado["amef"] if tipo in (None, "", f["tipo_equipo"])]
    return sorted(filas, key=lambda f: -f["RPN"])


def _id_orden(estado):
    """Siguiente identificador de orden que no existe todavía."""
    existentes = {o["id"] for o in estado["ordenes"]}
    while True:
        estado["contador"] += 1
        ident = f"OT-{estado['contador']:05d}"
        if ident not in existentes:
            return ident


def _nueva_orden(estado, plan, nombre, equipo, fecha, checklist, tipo, notas="", responsable=""):
    """Añade una orden pendiente al estado y la devuelve."""
    o = {"id": _id_orden(estado), "plan": plan, "nombre": nombre, "equipo": equipo, "fecha": fecha, "estado": "pendiente", "tipo": tipo,
         "responsable": responsable, "checklist": [{"texto": t, "hecho": False} for t in checklist], "notas": notas, "cerrada": ""}
    estado["ordenes"].append(o)
    return o


def tipo_de(orden):
    """Tipo de una orden: el guardado o, en planes antiguos, correctiva si viene de una falla y preventiva si no."""
    return orden.get("tipo") or ("correctiva" if orden.get("plan") == "CORR" else "preventiva")


def generar_ordenes(estado, hasta, errores=None, solo_plan=None):
    """Crea las órdenes de trabajo que tocan hasta la fecha, según la frecuencia de cada plan.

    Cada combinación plan-equipo avanza desde su última fecha (real o programada) sumando
    frecuencia_dias. No duplica órdenes ya creadas.

    Args:
        estado: plan de mantenimiento, que se modifica.
        hasta: fecha límite AAAA-MM-DD.
        errores: lista donde se anotan los planes inválidos, que se omiten. Si es None, el primero lanza la excepción.
        solo_plan: identificador de un plan; si se da, solo se generan sus órdenes.

    Returns:
        Número de órdenes nuevas.

    Raises:
        ValueError: si `hasta` no es una fecha válida o, sin lista de errores, un plan no es válido.
    """
    hasta = fecha_valida(hasta, "fecha límite")
    nuevas = 0
    ya = {(o["plan"], o["equipo"], o["fecha"]) for o in estado["ordenes"]}
    for p in estado["planes"]:
        if not p.get("activo", True) or solo_plan not in (None, p.get("id")):
            continue
        try:
            validar_plan(p)
        except ValueError as e:
            if errores is None:
                raise
            errores.append(str(e))
            continue
        paso = dt.timedelta(days=max(1, int(p["frecuencia_dias"])))
        for e in estado["equipos"]:
            if e["tipo"] != p["tipo_equipo"]:
                continue
            fechas = [dt.date.fromisoformat(o["fecha"]) for o in estado["ordenes"] if o["plan"] == p["id"] and o["equipo"] == e["codigo"]]
            fecha = fecha_valida(p["inicio"], "fecha de inicio") if not fechas else max(fechas) + paso
            while fecha <= hasta:
                clave = (p["id"], e["codigo"], fecha.isoformat())
                if clave not in ya:
                    _nueva_orden(estado, p["id"], p["nombre"], e["codigo"], fecha.isoformat(), p.get("checklist", []), "preventiva")
                    ya.add(clave)
                    nuevas += 1
                fecha += paso
    estado["ordenes"].sort(key=lambda o: (o["fecha"], o["id"]))
    return nuevas


def _orden(estado, ident):
    """Orden con ese identificador."""
    for o in estado["ordenes"]:
        if o["id"] == ident:
            return o
    raise ValueError(f"No existe la orden {ident}")


def cerrar_orden(estado, ident, fecha=None, notas=""):
    """Cierra una orden y la pasa al historial. Cerrar una orden ya cerrada no cambia nada.

    Args:
        estado: plan de mantenimiento.
        ident: identificador de la orden.
        fecha: fecha de cierre AAAA-MM-DD; hoy si no se indica.
        notas: notas del cierre; sustituyen a las de la orden si no están vacías.

    Returns:
        La orden.

    Raises:
        ValueError: si la orden no existe o la fecha no es válida.
    """
    o = _orden(estado, ident)
    cierre = fecha_valida(fecha, "fecha de cierre").isoformat() if fecha else dt.date.today().isoformat()
    if o["estado"] == "cerrada":
        return o
    o["estado"] = "cerrada"
    o["cerrada"] = cierre
    if notas:
        o["notas"] = notas
    estado["historial"].append({"orden": o["id"], "equipo": o["equipo"], "trabajo": o["nombre"], "fecha": o["cerrada"], "notas": o["notas"],
                                "completo": all(c["hecho"] for c in o["checklist"])})
    return o


def reabrir_orden(estado, ident):
    """Devuelve una orden cerrada a pendiente y quita su entrada del historial. Si ya está pendiente no hace nada.

    Raises:
        ValueError: si la orden no existe.
    """
    o = _orden(estado, ident)
    if o["estado"] != "cerrada":
        return o
    o["estado"] = "pendiente"
    o["cerrada"] = ""
    estado["historial"] = [h for h in estado["historial"] if h["orden"] != ident]
    return o


def _equipo(estado, codigo):
    """Equipo del catálogo con ese código."""
    for e in estado["equipos"]:
        if e["codigo"] == codigo:
            return e
    raise ValueError(f"No existe el equipo «{codigo}» en el catálogo")


def crear_orden(estado, equipo, trabajo, fecha, tipo="correctiva", responsable="", checklist=None, notas=""):
    """Crea una orden manual. Si ya hay una pendiente igual (equipo, trabajo, fecha y tipo) devuelve esa.

    Args:
        estado: plan de mantenimiento.
        equipo: código de un equipo del catálogo.
        trabajo: descripción del trabajo.
        fecha: fecha programada AAAA-MM-DD.
        tipo: uno de `TIPOS_ORDEN`.
        responsable: persona o empresa a cargo.
        checklist: pasos de la lista de comprobación (por defecto diagnosticar, ejecutar y verificar).
        notas: notas libres.

    Returns:
        Tupla (orden, creada).

    Raises:
        ValueError: si el equipo no existe, el trabajo está vacío, la fecha o el tipo no son válidos.
    """
    _equipo(estado, equipo)
    trabajo = str(trabajo).strip()
    if not trabajo:
        raise ValueError("El trabajo de la orden no puede estar vacío")
    fecha = fecha_valida(fecha, "fecha de la orden").isoformat()
    if tipo not in TIPOS_ORDEN:
        raise ValueError(f"Tipo de orden desconocido «{tipo}» (se admite: {', '.join(TIPOS_ORDEN)})")
    for o in estado["ordenes"]:
        if o["estado"] == "cerrada" or (o["equipo"], o["nombre"]) != (equipo, trabajo):
            continue
        # Una correctiva abierta no se repite aunque cambie la fecha; las demás se distinguen por fecha y tipo
        if (tipo == "correctiva" and o["plan"] == "CORR") or (o["fecha"], tipo_de(o)) == (fecha, tipo):
            return o, False
    pasos = ["Diagnosticar", "Ejecutar", "Verificar"] if checklist is None else list(checklist)
    o = _nueva_orden(estado, "CORR" if tipo == "correctiva" else "MAN", trabajo, equipo, fecha, pasos, tipo, notas, responsable)
    estado["ordenes"].sort(key=lambda x: (x["fecha"], x["id"]))
    return o, True


def editar_orden(estado, ident, **campos):
    """Cambia campos de una orden: `trabajo`, `equipo`, `fecha`, `tipo`, `responsable`, `notas`.

    Una orden cerrada solo admite `responsable` y `notas`; para el resto hay que reabrirla.

    Returns:
        La orden modificada.

    Raises:
        ValueError: si la orden no existe, un valor no es válido o se intenta cambiar una orden cerrada.
    """
    o = _orden(estado, ident)
    permitidos = {"trabajo", "equipo", "fecha", "tipo", "responsable", "notas"}
    if set(campos) - permitidos:
        raise ValueError(f"Campos no editables: {', '.join(sorted(set(campos) - permitidos))}")
    if o["estado"] == "cerrada" and set(campos) - {"responsable", "notas"}:
        raise ValueError(f"La orden {ident} está cerrada: reábrela para cambiar su fecha, trabajo, equipo o tipo")
    cambios = dict(campos)
    if "trabajo" in cambios:
        cambios["trabajo"] = str(cambios["trabajo"]).strip()
        if not cambios["trabajo"]:
            raise ValueError("El trabajo de la orden no puede estar vacío")
    if "equipo" in cambios:
        _equipo(estado, cambios["equipo"])
    if "fecha" in cambios:
        cambios["fecha"] = fecha_valida(cambios["fecha"], "fecha de la orden").isoformat()
    if "tipo" in cambios and cambios["tipo"] not in TIPOS_ORDEN:
        raise ValueError(f"Tipo de orden desconocido «{cambios['tipo']}»")
    if "trabajo" in cambios:
        o["nombre"] = cambios.pop("trabajo")
    o.update(cambios)
    if o["estado"] == "cerrada":
        for h in estado["historial"]:
            if h["orden"] == ident:
                h["notas"] = o["notas"]
    estado["ordenes"].sort(key=lambda x: (x["fecha"], x["id"]))
    return o


def reprogramar_orden(estado, ident, fecha):
    """Cambia la fecha de una orden pendiente."""
    return editar_orden(estado, ident, fecha=fecha)


def borrar_orden(estado, ident):
    """Borra una orden y su entrada del historial.

    Returns:
        True si existía; False si ya no estaba (borrar dos veces no falla).
    """
    n = len(estado["ordenes"])
    estado["ordenes"] = [o for o in estado["ordenes"] if o["id"] != ident]
    estado["historial"] = [h for h in estado["historial"] if h["orden"] != ident]
    return len(estado["ordenes"]) < n


def _borrar_pendientes(estado, criterio):
    """Quita las órdenes no cerradas que cumplen el criterio y devuelve cuántas."""
    antes = len(estado["ordenes"])
    estado["ordenes"] = [o for o in estado["ordenes"] if o["estado"] == "cerrada" or not criterio(o)]
    return antes - len(estado["ordenes"])


def _id_plan(estado):
    """Siguiente identificador de plan P### que no existe."""
    usados = {p["id"] for p in estado["planes"]}
    n = 1 + max([int(m.group(1)) for i in usados if (m := re.fullmatch(r"P(\d+)", str(i)))], default=0)
    while f"P{n:03d}" in usados:
        n += 1
    return f"P{n:03d}"


def crear_plan(estado, nombre, tipo_equipo, frecuencia_dias, inicio, checklist=None, activo=True):
    """Añade un plan preventivo con identificador único.

    Raises:
        ValueError: si el plan no es válido (nombre vacío, frecuencia no positiva, fecha inválida).
    """
    p = {"id": _id_plan(estado), "nombre": str(nombre).strip(), "tipo_equipo": tipo_equipo, "frecuencia_dias": frecuencia_dias,
         "checklist": list(checklist) if checklist is not None else ["Paso 1"], "inicio": str(inicio).strip(), "activo": bool(activo)}
    if not p["nombre"]:
        raise ValueError("El nombre del plan no puede estar vacío")
    validar_plan(p)
    estado["planes"].append(p)
    return p


def _plan(estado, ident):
    """Plan con ese identificador."""
    for p in estado["planes"]:
        if p["id"] == ident:
            return p
    raise ValueError(f"No existe el plan {ident}")


def editar_plan(estado, ident, **campos):
    """Cambia campos de un plan: `nombre`, `tipo_equipo`, `frecuencia_dias`, `inicio`, `checklist`, `activo`.

    El nuevo nombre se copia a las órdenes pendientes del plan.

    Returns:
        Tupla (plan, regenerar). `regenerar` es True si cambió algo que afecta a las fechas de las
        órdenes pendientes (frecuencia, inicio, tipo de equipo o activación) y el plan tiene alguna.

    Raises:
        ValueError: si el plan no existe o el resultado no es un plan válido.
    """
    p = _plan(estado, ident)
    permitidos = {"nombre", "tipo_equipo", "frecuencia_dias", "inicio", "checklist", "activo"}
    if set(campos) - permitidos:
        raise ValueError(f"Campos no editables: {', '.join(sorted(set(campos) - permitidos))}")
    nuevo = dict(p, **campos)
    nuevo["nombre"] = str(nuevo["nombre"]).strip()
    if not nuevo["nombre"]:
        raise ValueError("El nombre del plan no puede estar vacío")
    validar_plan(nuevo)
    afecta = any(p.get(k) != nuevo[k] for k in ("frecuencia_dias", "inicio", "tipo_equipo", "activo"))
    p.update(nuevo)
    if "nombre" in campos:
        for o in estado["ordenes"]:
            if o["plan"] == ident and o["estado"] != "cerrada":
                o["nombre"] = p["nombre"]
    pendientes = any(o["plan"] == ident and o["estado"] != "cerrada" for o in estado["ordenes"])
    return p, bool(afecta and pendientes)


def borrar_plan(estado, ident, borrar_pendientes=False):
    """Borra un plan y, si se pide, sus órdenes pendientes. Las cerradas se conservan en el historial.

    Returns:
        Número de órdenes pendientes borradas; -1 si el plan ya no existía.
    """
    n = len(estado["planes"])
    estado["planes"] = [p for p in estado["planes"] if p["id"] != ident]
    if len(estado["planes"]) == n:
        return -1
    return _borrar_pendientes(estado, lambda o: o["plan"] == ident) if borrar_pendientes else 0


def regenerar_plan(estado, ident, hasta):
    """Rehace las órdenes pendientes de un plan: borra las pendientes y genera de nuevo hasta la fecha.

    Returns:
        Tupla (borradas, nuevas).

    Raises:
        ValueError: si el plan no existe o no es válido, o la fecha no es válida.
    """
    p = _plan(estado, ident)
    fecha_valida(hasta, "fecha límite")
    if p.get("activo", True):
        validar_plan(p)
    borradas = _borrar_pendientes(estado, lambda o: o["plan"] == ident)
    return borradas, generar_ordenes(estado, hasta, solo_plan=ident)


def _codigo_libre(estado):
    """Código EQ-### sin usar."""
    usados = {e["codigo"] for e in estado["equipos"]}
    n = len(usados) + 1
    while f"EQ-{n:03d}" in usados:
        n += 1
    return f"EQ-{n:03d}"


def crear_equipo(estado, codigo=None, tipo="Módulos", descripcion="", ubicacion="", instalado=None):
    """Añade un equipo al catálogo; sin código se asigna uno libre.

    Raises:
        ValueError: si el código ya existe, está vacío o la fecha de instalación no es válida.
    """
    codigo = _codigo_libre(estado) if codigo is None else str(codigo).strip()
    if not codigo:
        raise ValueError("El código del equipo no puede estar vacío")
    if any(e["codigo"] == codigo for e in estado["equipos"]):
        raise ValueError(f"Ya existe un equipo con el código «{codigo}»")
    instalado = dt.date.today().isoformat() if instalado is None else fecha_valida(instalado, "fecha de instalación").isoformat()
    e = {"codigo": codigo, "tipo": tipo, "descripcion": descripcion, "ubicacion": ubicacion, "instalado": instalado}
    estado["equipos"].append(e)
    return e


def editar_equipo(estado, actual, **campos):
    """Cambia campos de un equipo: `codigo`, `tipo`, `descripcion`, `ubicacion`, `instalado`.

    Renombrar el código actualiza sus órdenes y su historial. Cambiar el tipo borra las órdenes
    pendientes que venían de planes de otro tipo.

    Returns:
        Tupla (equipo, órdenes pendientes borradas por el cambio de tipo).

    Raises:
        ValueError: si el equipo no existe, el nuevo código está repetido o vacío, o la fecha no es válida.
    """
    e = _equipo(estado, actual)
    permitidos = {"codigo", "tipo", "descripcion", "ubicacion", "instalado"}
    if set(campos) - permitidos:
        raise ValueError(f"Campos no editables: {', '.join(sorted(set(campos) - permitidos))}")
    nuevo = dict(campos)
    if "codigo" in nuevo:
        nuevo["codigo"] = str(nuevo["codigo"]).strip()
        if not nuevo["codigo"]:
            raise ValueError("El código del equipo no puede estar vacío")
        if nuevo["codigo"] != actual and any(x["codigo"] == nuevo["codigo"] for x in estado["equipos"]):
            raise ValueError(f"Ya existe un equipo con el código «{nuevo['codigo']}»")
    if "instalado" in nuevo and nuevo["instalado"]:
        nuevo["instalado"] = fecha_valida(nuevo["instalado"], "fecha de instalación").isoformat()
    borradas = 0
    if "tipo" in nuevo and nuevo["tipo"] != e["tipo"]:
        tipos = {p["id"]: p["tipo_equipo"] for p in estado["planes"]}
        borradas = _borrar_pendientes(estado, lambda o: o["equipo"] == actual and o["plan"] in tipos and tipos[o["plan"]] != nuevo["tipo"])
    e.update(nuevo)
    if e["codigo"] != actual:
        for o in estado["ordenes"]:
            if o["equipo"] == actual:
                o["equipo"] = e["codigo"]
        for h in estado["historial"]:
            if h["equipo"] == actual:
                h["equipo"] = e["codigo"]
    return e, borradas


def borrar_equipo(estado, codigo, borrar_ordenes=True):
    """Borra un equipo del catálogo y, si se pide, sus órdenes pendientes. Las cerradas se conservan.

    Returns:
        Número de órdenes pendientes borradas; -1 si el equipo ya no existía.
    """
    n = len(estado["equipos"])
    estado["equipos"] = [e for e in estado["equipos"] if e["codigo"] != codigo]
    if len(estado["equipos"]) == n:
        return -1
    return _borrar_pendientes(estado, lambda o: o["equipo"] == codigo) if borrar_ordenes else 0


def crear_modo_falla(estado, tipo_equipo, modo="Modo de falla", causa="", efecto="", s=5, o=5, d=5, control="", accion=""):
    """Añade un modo de falla al AMEF.

    Raises:
        ValueError: si S, O o D no son enteros de 1 a 10.
    """
    for k, v in (("S", s), ("O", o), ("D", d)):
        if isinstance(v, bool) or not isinstance(v, int) or not 1 <= v <= 10:
            raise ValueError(f"{k} debe ser un entero de 1 a 10")
    f = {"tipo_equipo": tipo_equipo, "modo": modo, "causa": causa, "efecto": efecto, "S": s, "O": o, "D": d, "control": control, "accion": accion}
    estado["amef"].append(f)
    return f


def borrar_modo_falla(estado, indice):
    """Borra el modo de falla en esa posición del AMEF; False si no existe."""
    if not 0 <= indice < len(estado["amef"]):
        return False
    del estado["amef"][indice]
    return True


def ordenar_amef(estado):
    """Ordena el AMEF de mayor a menor RPN, conservando el orden relativo de los empatados."""
    estado["amef"].sort(key=lambda f: -rpn(f))


def tablero(estado, hoy=None):
    """Resumen: pendientes, vencidas, próximas 30 días y cumplimiento del plan."""
    hoy = _hoy(hoy)
    pend = [o for o in estado["ordenes"] if o["estado"] != "cerrada"]
    venc = [o for o in pend if dt.date.fromisoformat(o["fecha"]) < hoy]
    prox = [o for o in pend if hoy <= dt.date.fromisoformat(o["fecha"]) <= hoy + dt.timedelta(days=30)]
    cerr = [o for o in estado["ordenes"] if o["estado"] == "cerrada"]
    cumpl = len(cerr) / max(len(cerr) + len(venc), 1)
    return {"pendientes": len(pend), "vencidas": len(venc), "proximas_30": len(prox), "cerradas": len(cerr), "cumplimiento": cumpl}


def ordenes_de_alarmas(estado, eventos, hoy=None):
    """Crea órdenes correctivas a partir de fallas detectadas por el análisis.

    No duplica una correctiva abierta con el mismo equipo y trabajo: ejecutarlo dos veces con los
    mismos hallazgos deja el mismo estado.

    Returns:
        Número de órdenes nuevas.
    """
    hoy = _hoy(hoy).isoformat()
    n = 0
    for ev in eventos:
        if any(o["plan"] == "CORR" and o["estado"] != "cerrada" and (o["equipo"], o["nombre"]) == (ev["equipo"], ev["trabajo"]) for o in estado["ordenes"]):
            continue
        _nueva_orden(estado, "CORR", ev["trabajo"], ev["equipo"], hoy, ev.get("checklist", ["Diagnosticar", "Reparar", "Verificar"]), "correctiva", ev.get("notas", ""))
        n += 1
    return n


def _escribir_csv(ruta, columnas, filas):
    """Escribe un CSV legible por Excel en español: separador «;», comillas donde hacen falta y UTF-8 con BOM.

    Raises:
        ValueError: si no se puede escribir el archivo.
    """
    tmp = f"{ruta}.tmp"
    try:
        with open(tmp, "w", encoding="utf-8-sig", newline="") as f:
            w = csv.writer(f, delimiter=";", lineterminator="\r\n")
            w.writerow(columnas)
            w.writerows(filas)
        os.replace(tmp, ruta)
    except OSError as e:
        raise ValueError(f"No se pudo escribir «{ruta}»: {e.strerror or e}") from None
    return len(filas)


def exportar_amef(estado, ruta, tipo=None):
    """Exporta el AMEF (en el orden mostrado y filtrado por tipo de equipo) a CSV.

    Returns:
        Número de filas escritas.

    Raises:
        ValueError: si no se puede escribir el archivo.
    """
    filas = [[f["tipo_equipo"], f["modo"], f.get("causa", ""), f.get("efecto", ""), f["S"], f["O"], f["D"], rpn(f), f.get("control", ""), f.get("accion", "")]
             for f in estado["amef"] if tipo in (None, "", f["tipo_equipo"])]
    return _escribir_csv(ruta, COLUMNAS_AMEF, filas)


def exportar_historial(estado, ruta, equipo=None):
    """Exporta el historial de trabajos (más recientes primero, filtrado por equipo) a CSV.

    Returns:
        Número de filas escritas.

    Raises:
        ValueError: si no se puede escribir el archivo.
    """
    filas = [[h["fecha"], h["orden"], h["equipo"], h["trabajo"], "sí" if h.get("completo") else "no", h.get("notas", "")]
             for h in reversed(estado["historial"]) if equipo in (None, "", h["equipo"])]
    return _escribir_csv(ruta, COLUMNAS_HISTORIAL, filas)


def _op_orden_crear(estado, hoy=None, **a):
    o, creada = crear_orden(estado, **a)
    return f"Orden {o['id']} creada." if creada else f"Ya existía la orden {o['id']} con los mismos datos; no se duplicó."


def _op_orden_editar(estado, hoy=None, ident="", **campos):
    editar_orden(estado, ident, **campos)
    return f"Orden {ident} actualizada."


def _op_orden_cerrar(estado, hoy=None, ident="", fecha=None, notas=""):
    o = _orden(estado, ident)
    ya = o["estado"] == "cerrada"
    cerrar_orden(estado, ident, fecha or _hoy(hoy).isoformat(), notas)
    return f"La orden {ident} ya estaba cerrada." if ya else f"Orden {ident} cerrada."


def _op_orden_reabrir(estado, hoy=None, ident=""):
    ya = _orden(estado, ident)["estado"] != "cerrada"
    reabrir_orden(estado, ident)
    return f"La orden {ident} ya estaba abierta." if ya else f"Orden {ident} reabierta."


def _op_orden_borrar(estado, hoy=None, ident=""):
    return f"Orden {ident} borrada." if borrar_orden(estado, ident) else f"La orden {ident} ya no existe."


def _op_plan_crear(estado, hoy=None, **a):
    a.setdefault("inicio", _hoy(hoy).isoformat())
    return f"Plan {crear_plan(estado, **a)['id']} creado."


def _op_plan_editar(estado, hoy=None, ident="", **campos):
    _, regenerar = editar_plan(estado, ident, **campos)
    msg = f"Plan {ident} actualizado." + (" Sus órdenes pendientes siguen con las fechas anteriores: puedes regenerarlas." if regenerar else "")
    return {"mensaje": msg, "regenerar": ident if regenerar else None}


def _op_plan_borrar(estado, hoy=None, ident="", borrar_pendientes=False):
    n = borrar_plan(estado, ident, borrar_pendientes)
    if n < 0:
        return f"El plan {ident} ya no existe."
    return f"Plan {ident} borrado" + (f" junto con {n} órdenes pendientes." if borrar_pendientes else ".")


def _op_plan_regenerar(estado, hoy=None, ident="", hasta=""):
    b, n = regenerar_plan(estado, ident, hasta)
    return f"Plan {ident}: {b} órdenes pendientes sustituidas por {n} nuevas."


def _op_equipo_crear(estado, hoy=None, **a):
    a.setdefault("instalado", _hoy(hoy).isoformat())
    return f"Equipo {crear_equipo(estado, **a)['codigo']} añadido."


def _op_equipo_editar(estado, hoy=None, actual="", **campos):
    e, n = editar_equipo(estado, actual, **campos)
    return f"Equipo {e['codigo']} actualizado." + (f" Se borraron {n} órdenes pendientes de planes de otro tipo." if n else "")


def _op_equipo_borrar(estado, hoy=None, codigo="", borrar_ordenes=True):
    n = borrar_equipo(estado, codigo, borrar_ordenes)
    if n < 0:
        return f"El equipo {codigo} ya no existe."
    return f"Equipo {codigo} borrado" + (f" junto con {n} órdenes pendientes." if borrar_ordenes else ".")


def _op_amef_crear(estado, hoy=None, **a):
    crear_modo_falla(estado, **a)
    return "Modo de falla añadido."


def _op_amef_borrar(estado, hoy=None, indice=-1):
    return "Modo de falla borrado." if borrar_modo_falla(estado, indice) else "El modo de falla ya no existe."


def _op_amef_ordenar(estado, hoy=None):
    ordenar_amef(estado)
    return "AMEF ordenado por RPN."


def _op_exportar(estado, hoy=None, que="", ruta="", filtro=None):
    if que == "amef":
        return f"AMEF exportado: {exportar_amef(estado, ruta, filtro)} filas en {ruta}"
    if que == "historial":
        return f"Historial exportado: {exportar_historial(estado, ruta, filtro)} filas en {ruta}"
    raise ValueError(f"No se puede exportar «{que}»")


OPERACIONES = {
    "orden_crear": _op_orden_crear, "orden_editar": _op_orden_editar, "orden_cerrar": _op_orden_cerrar, "orden_reabrir": _op_orden_reabrir,
    "orden_borrar": _op_orden_borrar, "plan_crear": _op_plan_crear, "plan_editar": _op_plan_editar, "plan_borrar": _op_plan_borrar,
    "plan_regenerar": _op_plan_regenerar, "equipo_crear": _op_equipo_crear, "equipo_editar": _op_equipo_editar, "equipo_borrar": _op_equipo_borrar,
    "amef_crear": _op_amef_crear, "amef_borrar": _op_amef_borrar, "amef_ordenar": _op_amef_ordenar, "exportar": _op_exportar,
}


def aplicar(estado, operacion, args=None, hoy=None):
    """Ejecuta una operación del planificador sobre el estado, que se modifica en sitio.

    Args:
        estado: plan de mantenimiento.
        operacion: clave de `OPERACIONES`.
        args: parámetros de la operación.
        hoy: fecha de referencia AAAA-MM-DD para los valores por defecto.

    Returns:
        Diccionario con `mensaje` para la persona y, si la operación lo indica, `regenerar` con el
        plan cuyas órdenes pendientes conviene rehacer.

    Raises:
        ValueError: si la operación no existe o sus datos no son válidos; el estado no se modifica en ese caso.
    """
    f = OPERACIONES.get(operacion)
    if f is None:
        raise ValueError(f"Operación desconocida: {operacion}")
    try:
        r = f(estado, hoy=hoy, **(args or {}))
    except TypeError as e:
        raise ValueError(f"Parámetros no válidos para «{operacion}»: {e}") from None
    return r if isinstance(r, dict) else {"mensaje": r}


def guardar_estado(estado, ruta):
    """Guarda el estado en JSON de forma atómica, mediante un archivo temporal."""
    tmp = str(ruta) + ".tmp"
    # Fin de línea LF en todos los sistemas: el archivo se versiona con Git
    with open(tmp, "w", encoding="utf8", newline="\n") as f:
        json.dump(estado, f, ensure_ascii=False, indent=1)
    os.replace(tmp, ruta)


def cargar_estado(ruta):
    """Carga y valida un plan de mantenimiento desde un archivo JSON.

    Raises:
        ValueError: si el archivo no existe, no es JSON o no es un plan válido (ver `validar_estado`).
    """
    return cargar_estado_con_avisos(ruta)[0]


def cargar_estado_con_avisos(ruta):
    """Como `cargar_estado`, y además devuelve los avisos de valores dudosos.

    Returns:
        Tupla (estado, avisos).
    """
    try:
        with open(ruta, encoding="utf-8-sig") as f:
            e = json.load(f)
    except OSError as err:
        raise ValueError(f"No se pudo leer «{ruta}»: {err.strerror or err}") from None
    except json.JSONDecodeError as err:
        raise ValueError(f"«{ruta}» no es un JSON válido: {err}") from None
    return validar_estado(e)


def copiar(estado):
    """Devuelve una copia profunda del estado."""
    return copy.deepcopy(estado)
