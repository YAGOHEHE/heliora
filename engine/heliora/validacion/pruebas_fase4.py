"""Fase 4: KPIs, detección de fallas, fiabilidad y planificador, con fallas inyectadas conocidas."""
from __future__ import annotations

import datetime as dt
import math
import os
import tempfile

import numpy as np
import pandas as pd

from .banco import prueba
from ..lib import mantenimiento as mt

_DATOS = {}


def _scada():
    """Dos años de datos de muestra, generados una sola vez."""
    if "d" not in _DATOS:
        _DATOS["d"], _DATOS["v"] = mt.generar_scada()
    return _DATOS["d"], _DATOS["v"]


@prueba(4, "Importación de SCADA", "Archivo con columnas con alias")
def importacion(m):
    """Un CSV con nombres alternativos de columna se normaliza y se resume correctamente."""
    with tempfile.TemporaryDirectory() as td:
        ruta = os.path.join(td, "scada.csv")
        pd.DataFrame({"Timestamp": pd.date_range("2026-01-01", periods=4, freq="h"), "Inverter": "A", "POA": [0, 100, 500, 800],
                      "Pac": [0, 9, 45, 70], "Pdc": [0, 10, 47, 72], "T_amb": [10, 11, 12, 13], "Alarma": ["", "", "W1", ""]}).to_csv(ruta, index=False)
        d, q = mt.importar_scada(ruta)
        m.verdad("Reconoce fecha, inversor e irradiancia", {"fecha", "inversor", "irradiancia_wm2", "p_ac_kw", "p_dc_kw"} <= set(d.columns))
        m.error("Número de filas", q["filas"], 4, 0)
        m.error("Paso temporal", q["paso_min"], 60.0, 1e-9, "min")
        d.to_parquet(os.path.join(td, "s.parquet"))
        d2, _ = mt.importar_scada(os.path.join(td, "s.parquet"))
        m.verdad("También lee Parquet", len(d2) == 4)


@prueba(4, "KPIs a mano", "Datos de 3 horas calculados a mano")
def kpis_mano(m):
    """PR = E/(Pn·H/1), disponibilidad, energía perdida y rendimiento del inversor con valores exactos."""
    t = pd.date_range("2026-01-01 10:00", periods=4, freq="h")
    d = pd.DataFrame({"fecha": t, "inversor": "A", "irradiancia_wm2": [1000.0, 1000.0, 800.0, 0.0], "p_ac_kw": [80.0, 0.0, 60.0, 0.0],
                      "p_dc_kw": [82.0, 0.0, 61.0, 0.0], "temp_modulo_c": [25.0, 25.0, 25.0, 25.0], "en_servicio": [1, 0, 1, 1]})
    tot, _ = mt.kpis(d, 100.0, gamma=0.0, eta_inv=1.0)
    m.error("PR = 140 kWh/(100 kW·2.8 h)", tot["PR"], 140.0 / 280.0, 1e-9)
    m.error("Disponibilidad = 2 de 3 horas con sol", tot["disponibilidad"], 2 / 3, 1e-9)
    m.error("Energía perdida = 20 + 100 + 20 kWh", tot["energia_perdida_kwh"], 140.0, 1e-9, "kWh")
    m.error("Rendimiento del inversor = 140/143", tot["eta_inversor"], 140.0 / 143.0, 1e-9)


@prueba(4, "KPIs de dos años de datos", "Rangos físicos")
def kpis_largos(m):
    """El PR anual y el rendimiento deben quedar en los valores que corresponden a las pérdidas inyectadas."""
    d, v = _scada()
    tot, t = mt.kpis(d, v["p_nom_inversor_kw"])
    m.verdad(f"PR plausible ({tot['PR']:.3f})", 0.70 < tot["PR"] < 0.95)
    m.verdad(f"Disponibilidad < 100 % por las paradas ({tot['disponibilidad']:.4f})", 0.9 < tot["disponibilidad"] < 1.0)
    m.verdad(f"Rendimiento del inversor ({tot['eta_inversor']:.4f})", 0.93 < tot["eta_inversor"] < 0.99)
    m.verdad("Hay energía perdida por paradas", t["perdida_parada_kwh"].sum() > 0)


@prueba(4, "Degradación de módulos", "Tasa inyectada de 0.7 %/año")
def degradacion(m):
    """Se recupera la tasa de degradación con la suciedad y las paradas presentes."""
    d, v = _scada()
    g = d[d["inversor"] == "INV-1"]
    r = mt.estimar_degradacion(g, v["p_nom_inversor_kw"], v["gamma"])
    m.error("Tasa anual de degradación", r["tasa_anual"], v["degradacion_anual"], 0.0025, "1/año")


@prueba(4, "Suciedad", "Tasa inyectada de 0.2 %/día con lluvias")
def suciedad(m):
    """Detecta la caída diaria y los lavados por lluvia."""
    d, v = _scada()
    g = d[d["inversor"] == "INV-1"]
    r = mt.estimar_suciedad(g, v["p_nom_inversor_kw"], v["gamma"])
    m.error("Tasa de suciedad", r["tasa_dia"], v["suciedad_dia"], 0.0008, "1/día")
    m.verdad(f"Detecta lavados ({len(r['lavados'])})", len(r["lavados"]) >= 10)
    m.verdad(f"Pérdida media coherente ({100 * r['perdida_media']:.2f} %)", 0.005 < r["perdida_media"] < 0.06)


@prueba(4, "String caído", "String 3 de INV-2, días 200 a 215")
def string_caido(m):
    """Debe encontrar el string caído con sus fechas y ningún falso positivo."""
    d, v = _scada()
    ev = mt.detectar_string_caido(d)
    m.verdad(f"Un solo evento ({len(ev)})", len(ev) == 1)
    e = ev[0]
    m.verdad("Inversor y string correctos", e["inversor"] == "INV-2" and e["string"] == "i_s3")
    inicio = pd.Timestamp("2024-01-01") + pd.Timedelta(days=200)
    fin = pd.Timestamp("2024-01-01") + pd.Timedelta(days=214)
    m.cota("Error en la fecha de inicio", (pd.Timestamp(e["desde"]) - inicio).days, 4, "días")
    m.cota("Error en la fecha final", (pd.Timestamp(e["hasta"]) - fin).days, 4, "días")


@prueba(4, "Sobretemperatura de IGBT", "28 °C de exceso en INV-1, días 400 a 412")
def igbt(m):
    """Debe aislar el episodio de sobretemperatura y estimar el exceso."""
    d, v = _scada()
    r = mt.detectar_sobretemperatura(d)
    ev = [e for e in r["eventos"] if e["dias"] >= 3]
    m.verdad(f"Detecta el episodio ({len(ev)})", len(ev) == 1 and ev[0]["inversor"] == "INV-1")
    m.error("Exceso de temperatura", ev[0]["exceso_c"], v["igbt"]["exceso_c"], 3.0, "°C")
    m.error("Pendiente térmica c1", r["modelo"]["INV-1"]["c1"], 0.09, 0.005, "°C/kW")


@prueba(4, "Desviación frente al modelo físico", "Medido entre el modelo; CUSUM")
def desviacion(m):
    """Con degradación y suciedad, la razón baja y el CUSUM debe alarmar tras el inicio."""
    d, v = _scada()
    r = mt.desviacion_modelo(d[d["inversor"] == "INV-1"], v["p_nom_inversor_kw"], v["gamma"])
    c = r["cusum"]
    m.verdad("El CUSUM alarma", len(c["alarma"]) > 0)
    m.verdad("Razón en un rango físico", 0.7 < float(r["razon"].median()) < 1.05)


@prueba(4, "MTBF, MTTR y Weibull", "Tiempos construidos a mano y muestra simulada")
def fiabilidad(m):
    """Con una secuencia de estados conocida, MTBF y MTTR deben ser exactos."""
    s = [1] * 10 + [0] * 2 + [1] * 20 + [0] * 4 + [1] * 30
    d = pd.DataFrame({"fecha": pd.date_range("2026-01-01", periods=len(s), freq="h"), "inversor": "A", "en_servicio": s})
    tbf, ttr, ev = mt.tiempos_de_eventos(d)
    f = mt.fiabilidad(tbf, ttr)
    m.error("MTBF", f["mtbf_h"], 15.0, 1e-9, "h")
    m.error("MTTR", f["mttr_h"], 3.0, 1e-9, "h")
    m.error("Disponibilidad", f["disponibilidad"], 15 / 18, 1e-9)
    b, e = 1.8, 700.0
    m.error("Vida media = η·Γ(1+1/β)", mt.vida_media_weibull(b, e), e * math.gamma(1 + 1 / b), 1e-9, "h")
    # Vida remanente: comprobación por la fórmula de supervivencia condicional
    t = 500.0
    x = float(mt.vida_remanente_weibull(b, e, t, 0.5))
    R = math.exp(-(((t + x) / e) ** b - (t / e) ** b))
    m.error("Vida remanente: R(t+x|t) = 0.5", R, 0.5, 1e-9)
    d2, v = _scada()
    tbf2, ttr2, _ = mt.tiempos_de_eventos(d2)
    from ..lib.estadistica import weibull_fit
    w = weibull_fit(tbf2)
    m.error("β de los tiempos entre fallos (inyectado 1.8)", w["beta"], 1.8, 0.8)
    r = mt.rul_degradacion(np.arange(10.0), 100 - 2 * np.arange(10.0), 80.0)
    m.error("RUL lineal hasta 80 (y = 100 - 2t)", r["rul"], 1.0, 1e-9)


@prueba(4, "Planificador", "Conteo exacto de órdenes generadas")
def planificador(m):
    """Las órdenes siguen la frecuencia de cada plan; cerrar pasa al historial; no hay duplicados."""
    e = mt.estado_inicial("2026-01-01")
    # 2 equipos de módulos × limpieza cada 30 días hasta el 31-mar (1 ene, 31 ene, 2 mar) = 3 por equipo
    n = mt.generar_ordenes(e, "2026-03-31")
    limp = [o for o in e["ordenes"] if o["nombre"] == "Limpieza de módulos"]
    m.error("Órdenes de limpieza", len(limp), 6, 0)
    m.error("Fechas: la última es 2-mar", int(dt.date.fromisoformat(limp[-1]["fecha"]).strftime("%m%d")), 302, 0)
    n2 = mt.generar_ordenes(e, "2026-03-31")
    m.error("Generar de nuevo no duplica", n2, 0, 0)
    t = mt.tablero(e, "2026-02-15")
    m.verdad(f"Tablero con vencidas ({t['vencidas']})", t["vencidas"] > 0)
    o = limp[0]
    for c in o["checklist"]:
        c["hecho"] = True
    mt.cerrar_orden(e, o["id"], "2026-01-02", "Hecho")
    m.verdad("Pasa al historial", e["historial"][-1]["orden"] == o["id"] and e["historial"][-1]["completo"])
    with tempfile.TemporaryDirectory() as td:
        ruta = os.path.join(td, "mant.json")
        mt.guardar_estado(e, ruta)
        m.verdad("Se guarda y recupera en JSON", mt.cargar_estado(ruta)["ordenes"][0]["id"] == e["ordenes"][0]["id"])


@prueba(4, "AMEF", "RPN = S·O·D")
def amef(m):
    """La plantilla cubre los cinco tipos de equipo y el RPN se ordena de mayor a menor."""
    e = mt.estado_inicial("2026-01-01")
    tipos = {f["tipo_equipo"] for f in e["amef"]}
    m.verdad("Cubre los cinco tipos de equipo", tipos == set(mt.TIPOS_EQUIPO))
    f = {"S": 8, "O": 4, "D": 5}
    m.error("RPN", mt.rpn(f), 160, 0)
    filas = mt.amef_por_equipo(e, "Inversores")
    m.verdad("Ordenado por RPN", all(a["RPN"] >= b["RPN"] for a, b in zip(filas, filas[1:])))
