"""Fase 4: KPIs, detección de fallas, fiabilidad y planificador, con fallas inyectadas conocidas."""
from __future__ import annotations

import datetime as dt
import json
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


def _error(fn, *args, **kw):
    """Mensaje de la excepción ValueError que lanza la llamada, o None si no falla."""
    try:
        fn(*args, **kw)
    except ValueError as e:
        return str(e)
    return None


def _escribir(td, nombre, texto, cod="utf-8"):
    """Escribe un archivo de prueba y devuelve su ruta."""
    ruta = os.path.join(td, nombre)
    with open(ruta, "wb") as f:
        f.write(texto.encode(cod))
    return ruta


_SCADA_ES = (
    "Fecha;Hora;Inversor;Irradiancia (W/m2);Temp. ambiente (°C);Potencia AC (kW);Potencia DC (kW);En servicio\n"
    "05/03/2024;10:00;A;800,5;20,0;70,0;72,0;ON\n"
    "05/03/2024;11:00;A;900,0;21,5;80,0;82,0;ON\n"
    "05/03/2024;11:00;A;900,0;21,5;80,0;82,0;ON\n"
    "05/03/2024;12:00;A;950,0;22,0;0,0;0,0;OFF\n"
    "05/03/2024;10:00;B;800,5;20,0;60,0;62,0;sí\n"
    "05/03/2024;11:00;B;900,0;21,5;65,0;67,0;sí\n"
    "05/03/2024;12:00;B;950,0;22,0;66,0;68,0;no\n")


@prueba(4, "Importación de un SCADA español", "CSV de Excel: «;», coma decimal, dd/mm, fecha y hora, unidades, duplicados")
def importacion_espanola(m):
    """Un CSV en Windows-1252 con fecha y hora separadas, unidades en los títulos, ON/OFF y una fila repetida se importa sin error de lectura."""
    with tempfile.TemporaryDirectory() as td:
        d, q = mt.importar_scada(_escribir(td, "scada.csv", _SCADA_ES, "cp1252"))
    m.verdad("Reconoce las columnas con unidades",
             {"fecha", "inversor", "irradiancia_wm2", "temp_amb_c", "p_ac_kw", "p_dc_kw", "en_servicio"} <= set(d.columns))
    m.verdad("Une fecha y hora en una sola columna", "hora" not in d.columns and d["fecha"].iloc[0] == pd.Timestamp("2024-03-05 10:00"))
    m.verdad("«05/03» es el 5 de marzo (día primero)", (d["fecha"].dt.month == 3).all() and (d["fecha"].dt.day == 5).all())
    m.error("Fila repetida eliminada", q["duplicados"], 1, 0)
    m.error("Filas tras eliminar duplicados", q["filas"], 6, 0)
    m.verdad("El aviso cuenta los duplicados", any("1 filas repetidas" in a for a in q["avisos"]))
    m.error("Paso temporal", q["paso_min"], 60.0, 1e-9, "min")
    m.verdad("en_servicio ON/OFF y sí/no pasa a 0/1", d.sort_values(["inversor", "fecha"])["en_servicio"].tolist() == [1, 1, 0, 1, 1, 0])
    m.error("Coma decimal: irradiancia", d["irradiancia_wm2"].iloc[0], 800.5, 1e-9)
    tot, _ = mt.kpis(d, 100.0, gamma=0.0, eta_inv=1.0)
    # E_ac = 70 + 80 + 0 + 60 + 65 + 66 = 341 kWh; con los duplicados valdría 421 kWh
    m.error("Energía sin duplicados", tot["energia_kwh"], 341.0, 1e-9, "kWh")


@prueba(4, "Fecha y hora separadas", "Los mismos datos con fecha ISO y con hora separada dan la misma serie")
def fecha_y_hora(m):
    """Con fecha y hora en columnas distintas la energía no es cero ni hay división por cero; ISO y dd/mm coinciden."""
    t = pd.date_range("2026-01-05 08:00", periods=5, freq="h")
    base = pd.DataFrame({"inversor": "A", "irradiancia": [100.0, 300.0, 600.0, 800.0, 700.0], "Pac": [10.0, 28.0, 55.0, 70.0, 62.0], "T_amb": 15.0})
    junto = base.assign(Timestamp=t)
    aparte = base.assign(Fecha=t.strftime("%d/%m/%Y"), Hora=t.strftime("%H:%M"))
    a, b = mt.normalizar_columnas(junto), mt.normalizar_columnas(aparte)
    m.verdad("Mismas fechas con Timestamp que con Fecha + Hora", a["fecha"].tolist() == b["fecha"].tolist())
    tot, _ = mt.kpis(b, 100.0, gamma=0.0, eta_inv=1.0)
    m.error("Energía con fecha y hora separadas", tot["energia_kwh"], 225.0, 1e-9, "kWh")
    m.verdad("PR finito y positivo", math.isfinite(tot["PR"]) and tot["PR"] > 0)
    horas = aparte.assign(Hora=[8, 9, 10, 11, 12])
    c = mt.normalizar_columnas(horas)
    m.verdad("Hora como número entero", c["fecha"].iloc[-1] == pd.Timestamp("2026-01-05 12:00"))
    err = _error(mt.normalizar_columnas, base.assign(en_servicio=["ON", "OFF", "quizá", "ON", "ON"], Timestamp=t))
    m.verdad("en_servicio desconocido: error que nombra el valor", err is not None and "quizá" in err)


@prueba(4, "Cobertura temporal y paso inválido", "Huecos contados a mano")
def cobertura(m):
    """Tres lecturas horarias que faltan en una serie de 10 horas dan cobertura 7/10 y un hueco; sin paso positivo se rechaza."""
    t = pd.date_range("2026-01-01", periods=10, freq="h").delete([3, 4, 5])
    d = pd.DataFrame({"fecha": t, "inversor": "A", "irradiancia_wm2": 500.0, "p_ac_kw": 40.0})
    q = mt.resumen_calidad(d)
    m.error("Cobertura 7 de 10", q["cobertura"], 0.7, 1e-12)
    m.error("Lecturas esperadas", q["esperadas"], 10, 0)
    m.verdad("Un hueco de 3 lecturas", q["huecos_total"] == 1 and q["huecos"][0]["perdidas"] == 3)
    una = pd.DataFrame({"fecha": pd.to_datetime(["2026-01-01 10:00"]), "inversor": "A", "irradiancia_wm2": 1.0, "p_ac_kw": 1.0})
    m.verdad("Una sola lectura: paso inválido en los KPI", "paso temporal" in (_error(mt.kpis, una, 100.0) or ""))
    with tempfile.TemporaryDirectory() as td:
        ruta = _escribir(td, "una.csv", "fecha,irradiancia,pac\n2026-01-01 10:00,500,40\n")
        m.verdad("Importar una sola lectura: error explicado", "paso temporal" in (_error(mt.importar_scada, ruta) or ""))


@prueba(4, "Columnas requeridas con mensaje claro", "Falta la columna X (nombres aceptados: ...)")
def columnas_requeridas(m):
    """Sin una columna necesaria, el error nombra la columna y sus alias en lugar de un KeyError."""
    t = pd.date_range("2026-01-01", periods=48, freq="h")
    d = pd.DataFrame({"fecha": t, "inversor": "A", "irradiancia_wm2": 500.0, "p_ac_kw": 40.0})
    e1 = _error(mt.requerir_scada, d)
    m.verdad("Sin temperatura: pide temp_modulo_c o temp_amb_c", e1 is not None and "temp_modulo_c" in e1 and "temp_amb_c" in e1 and "nombres aceptados" in e1)
    e2 = _error(mt.requerir_scada, d.drop(columns=["p_ac_kw"]).assign(temp_amb_c=20.0))
    m.verdad("Sin p_ac_kw: «Falta la columna p_ac_kw (nombres aceptados: …»", e2 is not None and e2.startswith("Falta la columna p_ac_kw (nombres aceptados: ") and "potencia_ac" in e2)
    e3 = _error(mt.requerir_scada, d.assign(temp_igbt_c=50.0), aislamiento=True)
    m.verdad("Con temp_igbt_c y sin temp_amb_c lo pide", e3 is not None and "temp_amb_c" in e3)
    m.verdad("pr_diario sin temperatura: ValueError y no KeyError", "temp_modulo_c" in (_error(mt.pr_diario, d, 100.0) or ""))
    s = [1] * 5 + [0] * 3 + [1] * 5
    p = pd.DataFrame({"fecha": pd.date_range("2026-01-01", periods=13, freq="h"), "inversor": "A", "p_ac_kw": [10.0 if x else 0.0 for x in s]})
    tbf, ttr, _ = mt.tiempos_de_eventos(p)
    m.verdad("tiempos_de_eventos sin en_servicio usa p_ac_kw > 0", tbf.tolist() == [5.0] and ttr.tolist() == [3.0])
    m.verdad("Sin en_servicio ni p_ac_kw: error claro", "p_ac_kw" in (_error(mt.tiempos_de_eventos, p.drop(columns=["p_ac_kw"])) or ""))


@prueba(4, "Correctivas idempotentes", "No se duplica una CORR abierta del mismo equipo y trabajo")
def correctivas(m):
    """Crear las correctivas dos veces deja el mismo estado; tras cerrar una, vuelve a poder abrirse."""
    e = mt.estado_inicial("2026-01-01")
    ev = [{"trabajo": "String caído: S3", "equipo": "INV-2", "notas": "x"}, {"trabajo": "Sobretemperatura IGBT", "equipo": "INV-1"}]
    m.error("Primera vez: 2 órdenes", mt.ordenes_de_alarmas(e, ev, "2026-02-01"), 2, 0)
    m.error("Segunda vez: ninguna", mt.ordenes_de_alarmas(e, ev, "2026-02-02"), 0, 0)
    m.error("Dos correctivas en total", sum(o["plan"] == "CORR" for o in e["ordenes"]), 2, 0)
    ident = next(o["id"] for o in e["ordenes"] if o["plan"] == "CORR")
    mt.cerrar_orden(e, ident, "2026-02-03")
    m.error("Cerrada la primera, vuelve a crearse", mt.ordenes_de_alarmas(e, ev[:1], "2026-02-04"), 1, 0)
    o, creada = mt.crear_orden(e, "INV-1", "Sobretemperatura IGBT", "2026-03-01", "correctiva")
    m.verdad("Una correctiva manual igual a una abierta no se duplica", not creada)


@prueba(4, "Frecuencia no positiva y planes inválidos", "No hay bucle infinito ni se rompe la generación de los demás")
def frecuencia_cero(m):
    """Un plan con frecuencia 0 o fecha mala se omite con el nombre del plan; los demás se generan y la llamada termina."""
    import threading
    e = mt.estado_inicial("2026-01-01")
    e["planes"][0].update(frecuencia_dias=0, nombre="Limpieza rota")
    e["planes"][1].update(inicio="31/02/2026", nombre="Termografía rota")
    errores = []
    resultado = {}
    hilo = threading.Thread(target=lambda: resultado.update(n=mt.generar_ordenes(e, "2026-06-30", errores)), daemon=True)
    hilo.start()
    hilo.join(10.0)
    m.verdad("La generación termina (no hay bucle infinito)", not hilo.is_alive())
    m.verdad("Genera las órdenes de los planes válidos", resultado.get("n", 0) > 0 and any(o["plan"] == "P003" for o in e["ordenes"]))
    m.verdad("Dos errores con el nombre de cada plan", len(errores) == 2 and "Limpieza rota" in errores[0] and "Termografía rota" in errores[1])
    m.verdad("Sin lista de errores lanza el error del plan", "Limpieza rota" in (_error(mt.generar_ordenes, e, "2026-06-30") or ""))
    m.verdad("Fecha límite inválida", "AAAA-MM-DD" in (_error(mt.generar_ordenes, e, "mañana", []) or ""))
    m.verdad("validar_plan rechaza una frecuencia negativa", _error(mt.validar_plan, dict(e["planes"][2], frecuencia_dias=-5)) is not None)
    m.verdad("El ejemplo escalona el inicio de los planes", len({p["inicio"] for p in mt.estado_inicial("2026-01-01")["planes"]}) == len(mt.estado_inicial("2026-01-01")["planes"]))


@prueba(4, "Crear, editar, reprogramar, cerrar, reabrir y borrar órdenes", "Estados y historial coherentes")
def ciclo_de_ordenes(m):
    """Una orden manual recorre su ciclo de vida; cerrar o reabrir dos veces no duplica el historial."""
    e = mt.estado_inicial("2026-01-01")
    o, creada = mt.crear_orden(e, "MOD-A", "Cambio de módulo", "2026-02-10", "correctiva", "Ana", ["Aislar", "Cambiar"])
    ident = o["id"]
    m.verdad("Se crea pendiente con sus datos", creada and o["estado"] == "pendiente" and o["responsable"] == "Ana" and len(o["checklist"]) == 2)
    m.verdad("Crear dos veces lo mismo no duplica", mt.crear_orden(e, "MOD-A", "Cambio de módulo", "2026-02-10", "correctiva")[1] is False)
    m.verdad("Equipo inexistente: error", "No existe el equipo" in (_error(mt.crear_orden, e, "XXX", "t", "2026-02-10") or ""))
    m.verdad("Fecha inválida: error", "AAAA-MM-DD" in (_error(mt.crear_orden, e, "MOD-A", "t", "10/02/2026") or ""))
    mt.editar_orden(e, ident, trabajo="Cambio de dos módulos", equipo="MOD-B", fecha="2026-02-12", responsable="Luis")
    m.verdad("Editar cambia trabajo, equipo, fecha y responsable", (o["nombre"], o["equipo"], o["fecha"], o["responsable"]) == ("Cambio de dos módulos", "MOD-B", "2026-02-12", "Luis"))
    mt.reprogramar_orden(e, ident, "2026-02-20")
    m.verdad("Reprogramar", o["fecha"] == "2026-02-20")
    m.verdad("El orden de la lista sigue la fecha", [x["fecha"] for x in e["ordenes"]] == sorted(x["fecha"] for x in e["ordenes"]))
    mt.cerrar_orden(e, ident, "2026-02-21", "Hecho")
    mt.cerrar_orden(e, ident, "2026-02-22", "Otra vez")
    m.error("Cerrar dos veces: una sola entrada en el historial", sum(h["orden"] == ident for h in e["historial"]), 1, 0)
    m.verdad("Conserva la fecha de cierre elegida", o["cerrada"] == "2026-02-21" and e["historial"][-1]["fecha"] == "2026-02-21")
    m.verdad("Cerrada: no se cambia la fecha", "reábrela" in (_error(mt.reprogramar_orden, e, ident, "2026-03-01") or ""))
    mt.editar_orden(e, ident, notas="Con notas")
    m.verdad("Cerrada: sí se editan las notas, también en el historial", e["historial"][-1]["notas"] == "Con notas")
    mt.reabrir_orden(e, ident)
    mt.reabrir_orden(e, ident)
    m.verdad("Reabrir deja pendiente y quita el historial", o["estado"] == "pendiente" and o["cerrada"] == "" and not any(h["orden"] == ident for h in e["historial"]))
    m.verdad("Borrar quita la orden", mt.borrar_orden(e, ident) and not any(x["id"] == ident for x in e["ordenes"]))
    m.verdad("Borrar otra vez no falla", mt.borrar_orden(e, ident) is False)
    m.verdad("Cerrar una orden inexistente: error", "No existe la orden" in (_error(mt.cerrar_orden, e, "OT-99999") or ""))


@prueba(4, "Planes: borrar con sus pendientes, ids únicos y regenerar", "Cuentas de órdenes exactas")
def edicion_de_planes(m):
    """Borrar un plan con sus pendientes conserva las cerradas; los ids no se repiten; cambiar la frecuencia ofrece regenerar."""
    e = mt.estado_inicial("2026-01-01")
    mt.generar_ordenes(e, "2026-06-30")
    plan = e["planes"][0]
    pend = [o for o in e["ordenes"] if o["plan"] == plan["id"]]
    mt.cerrar_orden(e, pend[0]["id"], "2026-01-05")
    otras = len(e["ordenes"]) - len(pend)
    m.error("Borrar el plan y sus pendientes", mt.borrar_plan(e, plan["id"], True), len(pend) - 1, 0)
    m.error("Quedan las cerradas y las de otros planes", len(e["ordenes"]), otras + 1, 0)
    m.error("Borrar otra vez avisa", mt.borrar_plan(e, plan["id"], True), -1, 0)
    n = mt.crear_plan(e, "Plan nuevo", "Módulos", 45, "2026-01-10")
    m.verdad("El id nuevo no existe ya", n["id"] not in {p["id"] for p in e["planes"][:-1]})
    mt.borrar_plan(e, e["planes"][3]["id"])
    n2 = mt.crear_plan(e, "Otro", "Módulos", 45, "2026-01-10")
    m.verdad("Tras borrar uno intermedio los ids siguen siendo únicos", len({p["id"] for p in e["planes"]}) == len(e["planes"]) and n2["id"] != n["id"])
    m.verdad("Crear un plan con frecuencia 0 falla", "frecuencia" in (_error(mt.crear_plan, e, "Mal", "Módulos", 0, "2026-01-10") or ""))
    mt.generar_ordenes(e, "2026-06-30")
    p2 = n
    antes = sum(o["plan"] == p2["id"] and o["estado"] != "cerrada" for o in e["ordenes"])
    _, regenerar = mt.editar_plan(e, p2["id"], frecuencia_dias=p2["frecuencia_dias"] * 2)
    m.verdad("Cambiar la frecuencia ofrece regenerar", regenerar and antes > 0)
    b, nuevas = mt.regenerar_plan(e, p2["id"], "2026-06-30")
    m.error("Regenerar sustituye las pendientes", b, antes, 0)
    m.verdad("Las nuevas siguen la frecuencia nueva", nuevas < antes)
    _, regenerar = mt.editar_plan(e, p2["id"], nombre="Renombrado")
    m.verdad("Renombrar no pide regenerar y renombra las pendientes", not regenerar and all(o["nombre"] == "Renombrado" for o in e["ordenes"] if o["plan"] == p2["id"]))
    _, regenerar = mt.editar_plan(e, p2["id"], activo=False)
    m.verdad("Desactivar ofrece regenerar", regenerar)
    mt.regenerar_plan(e, p2["id"], "2026-06-30")
    m.verdad("Plan inactivo regenerado: sin pendientes", not any(o["plan"] == p2["id"] and o["estado"] != "cerrada" for o in e["ordenes"]))
    m.verdad("Editar con una fecha inválida no cambia el plan", _error(mt.editar_plan, e, p2["id"], inicio="2026-13-01") is not None and p2["inicio"] != "2026-13-01")


@prueba(4, "Catálogo: códigos únicos, renombrar y borrar equipos", "Las órdenes siguen al equipo")
def edicion_de_equipos(m):
    """Renombrar un código actualiza órdenes e historial; borrar un equipo quita sus pendientes y conserva las cerradas."""
    e = mt.estado_inicial("2026-01-01")
    mt.generar_ordenes(e, "2026-04-30")
    o = next(x for x in e["ordenes"] if x["equipo"] == "INV-1")
    mt.cerrar_orden(e, o["id"], "2026-01-20")
    mt.editar_equipo(e, "INV-1", codigo="INV-A")
    m.verdad("Las órdenes pasan al código nuevo", not any(x["equipo"] == "INV-1" for x in e["ordenes"]) and any(x["equipo"] == "INV-A" for x in e["ordenes"]))
    m.verdad("El historial también", e["historial"][0]["equipo"] == "INV-A")
    m.verdad("Código repetido: error", "Ya existe" in (_error(mt.editar_equipo, e, "INV-A", codigo="INV-2") or ""))
    m.verdad("Crear con código repetido: error", "Ya existe" in (_error(mt.crear_equipo, e, "INV-2") or ""))
    a, b = mt.crear_equipo(e), mt.crear_equipo(e)
    m.verdad("Los códigos automáticos no se repiten", a["codigo"] != b["codigo"])
    m.verdad("Fecha de instalación inválida: error", "AAAA-MM-DD" in (_error(mt.editar_equipo, e, "INV-A", instalado="ayer") or ""))
    pendientes = sum(x["equipo"] == "INV-A" and x["estado"] != "cerrada" for x in e["ordenes"])
    m.error("Borrar el equipo borra sus pendientes", mt.borrar_equipo(e, "INV-A", True), pendientes, 0)
    m.verdad("Su orden cerrada y el historial se conservan", sum(x["equipo"] == "INV-A" for x in e["ordenes"]) == 1 and len(e["historial"]) == 1)
    m.error("Borrar otra vez avisa", mt.borrar_equipo(e, "INV-A"), -1, 0)
    mod = sum(x["equipo"] == "MOD-A" and x["estado"] != "cerrada" for x in e["ordenes"])
    _, quitadas = mt.editar_equipo(e, "MOD-A", tipo="Trackers")
    m.verdad("Cambiar el tipo quita las pendientes de planes del tipo anterior", quitadas == mod and mod > 0)


@prueba(4, "Abrir un plan: formato y claves obligatorias", "Un archivo ajeno o roto no se acepta")
def validacion_de_plan(m):
    """Se rechazan formatos ajenos, claves ausentes e identificadores repetidos con el motivo; los plazos dudosos se abren con aviso."""
    base = mt.estado_inicial("2026-01-01")
    mt.generar_ordenes(base, "2026-03-01")
    with tempfile.TemporaryDirectory() as td:
        def abrir(contenido):
            return _error(mt.cargar_estado, _escribir(td, "p.json", contenido if isinstance(contenido, str) else json.dumps(contenido)))
        ok = os.path.join(td, "ok.json")
        mt.guardar_estado(base, ok)
        m.verdad("Un plan guardado se abre", _error(mt.cargar_estado, ok) is None)
        m.verdad("Formato ajeno", "Formato no reconocido" in (abrir(dict(base, formato="otro/9")) or ""))
        m.verdad("Sin formato", "Formato no reconocido" in (abrir({k: v for k, v in base.items() if k != "formato"}) or ""))
        m.verdad("Falta una clave obligatoria", "«ordenes»" in (abrir({k: v for k, v in base.items() if k != "ordenes"}) or ""))
        m.verdad("No es JSON", "no es un JSON válido" in (abrir("{roto") or ""))
        m.verdad("No es un objeto", "objeto JSON" in (abrir([1, 2]) or ""))
        m.verdad("No existe", "No se pudo leer" in (_error(mt.cargar_estado, os.path.join(td, "nada.json")) or ""))
        rep = json.loads(json.dumps(base))
        rep["ordenes"].append(dict(rep["ordenes"][0]))
        m.verdad("Orden repetida", "repetido" in (abrir(rep) or ""))
        mala = json.loads(json.dumps(base))
        mala["ordenes"][0]["fecha"] = "2026-99-99"
        m.verdad("Orden con fecha imposible", "AAAA-MM-DD" in (abrir(mala) or "") or "calendario" in (abrir(mala) or ""))
        dudoso = json.loads(json.dumps(base))
        dudoso["planes"][0]["frecuencia_dias"] = 0
        del dudoso["historial"]
        e, avisos = mt.cargar_estado_con_avisos(_escribir(td, "d.json", json.dumps(dudoso)))
        m.verdad("Plan con frecuencia 0: se abre con aviso que nombra el plan", len(avisos) == 1 and dudoso["planes"][0]["nombre"] in avisos[0])
        m.verdad("Completa lo opcional ausente", e["historial"] == [] and e["contador"] >= len(base["ordenes"]))
    m.verdad("El estado vacío es válido", mt.validar_estado(mt.estado_vacio())[1] == [])


@prueba(4, "Exportar AMEF e historial a CSV", "Comillas, «;», BOM UTF-8 y filtro")
def exportar_csv(m):
    """El CSV con comas, comillas y saltos de línea en el texto se lee de vuelta idéntico; respeta el filtro y avisa si no se puede escribir."""
    import csv
    e = mt.estado_inicial("2026-01-01")
    mt.crear_modo_falla(e, "Inversores", 'Fallo "raro", con coma; y punto y coma', "Causa\ncon salto", "Efecto", 3, 4, 5, "Control", "Acción: revisar")
    with tempfile.TemporaryDirectory() as td:
        ruta = os.path.join(td, "amef.csv")
        n = mt.exportar_amef(e, ruta, "Inversores")
        crudo = open(ruta, "rb").read()
        m.verdad("Empieza con BOM UTF-8", crudo.startswith(b"\xef\xbb\xbf"))
        filas = list(csv.reader(open(ruta, encoding="utf-8-sig", newline=""), delimiter=";"))
        m.error("Filas filtradas por equipo (+ cabecera)", len(filas), n + 1, 0)
        m.verdad("Solo inversores", {f[0] for f in filas[1:]} == {"Inversores"})
        raro = filas[-1]
        m.verdad("El texto con comas, comillas y salto de línea vuelve idéntico", raro[1] == 'Fallo "raro", con coma; y punto y coma' and raro[2] == "Causa\ncon salto")
        m.verdad("RPN calculado y acción incluida", raro[7] == "60" and raro[9] == "Acción: revisar")
        todos = mt.exportar_amef(e, os.path.join(td, "todos.csv"))
        m.verdad("Sin filtro escribe todas", todos == len(e["amef"]))
        mt.generar_ordenes(e, "2026-01-31")
        for o in e["ordenes"][:3]:
            mt.cerrar_orden(e, o["id"], "2026-02-01", "nota; con, signos")
        eq = e["ordenes"][0]["equipo"]
        h = mt.exportar_historial(e, os.path.join(td, "h.csv"), eq)
        m.verdad("Historial filtrado por equipo", h == sum(x["equipo"] == eq for x in e["historial"]) and h > 0)
        err = _error(mt.exportar_amef, e, os.path.join(td, "no_existe", "x.csv"))
        m.verdad("Si no se puede escribir, error con la ruta", err is not None and "No se pudo escribir" in err)


@prueba(4, "AMEF: crear, borrar y ordenar por RPN", "El orden solo cambia al pedirlo")
def edicion_amef(m):
    """Ordenar es una operación explícita y estable; los valores S, O y D fuera de 1 a 10 se rechazan."""
    e = mt.estado_inicial("2026-01-01")
    antes = [f["modo"] for f in e["amef"]]
    mt.amef_por_equipo(e)
    m.verdad("Consultar el AMEF no reordena el estado", antes == [f["modo"] for f in e["amef"]])
    mt.crear_modo_falla(e, "Módulos", "Muy grave", s=10, o=10, d=10)
    m.verdad("Crear añade al final", e["amef"][-1]["modo"] == "Muy grave")
    mt.ordenar_amef(e)
    m.verdad("Ordenar pone el mayor RPN primero", e["amef"][0]["modo"] == "Muy grave" and all(mt.rpn(a) >= mt.rpn(b) for a, b in zip(e["amef"], e["amef"][1:])))
    m.verdad("S fuera de rango: error", "1 a 10" in (_error(mt.crear_modo_falla, e, "Módulos", s=11) or ""))
    m.verdad("Borrar por posición", mt.borrar_modo_falla(e, 0) and e["amef"][0]["modo"] != "Muy grave" and mt.borrar_modo_falla(e, 999) is False)


@prueba(4, "Operaciones del planificador por nombre", "La interfaz solo llama a aplicar()")
def operaciones(m):
    """`aplicar` ejecuta cada operación y devuelve un mensaje; ante un error el estado queda intacto."""
    e = mt.estado_inicial("2026-01-01")
    mt.generar_ordenes(e, "2026-03-01")
    msg = mt.aplicar(e, "orden_crear", {"equipo": "INV-1", "trabajo": "Revisar ventilador", "fecha": "2026-02-02", "tipo": "inspección"}, "2026-01-15")
    m.verdad("Crear una orden por nombre", "creada" in msg and any(o["nombre"] == "Revisar ventilador" for o in e["ordenes"]))
    ident = next(o["id"] for o in e["ordenes"] if o["nombre"] == "Revisar ventilador")
    m.verdad("Cerrar sin fecha usa hoy", "cerrada" in mt.aplicar(e, "orden_cerrar", {"ident": ident}, "2026-01-15") and _orden_por_id(e, ident)["cerrada"] == "2026-01-15")
    m.verdad("Reabrir", "reabierta" in mt.aplicar(e, "orden_reabrir", {"ident": ident}))
    foto = json.dumps(e, sort_keys=True)
    m.verdad("Error: operación desconocida", "desconocida" in (_error(mt.aplicar, e, "volar", {}) or ""))
    m.verdad("Error: parámetros que sobran", "no válidos" in (_error(mt.aplicar, e, "orden_borrar", {"ident": ident, "otro": 1}) or ""))
    m.verdad("Error: fecha inválida, sin cambios", _error(mt.aplicar, e, "orden_editar", {"ident": ident, "fecha": "ayer", "trabajo": "X"}) is not None and json.dumps(e, sort_keys=True) == foto)
    m.verdad("Plan nuevo con inicio por defecto en hoy", "creado" in mt.aplicar(e, "plan_crear", {"nombre": "Nuevo", "tipo_equipo": "Módulos", "frecuencia_dias": 10}, "2026-01-15") and e["planes"][-1]["inicio"] == "2026-01-15")
    m.verdad("Borrar equipo inexistente es un aviso, no un error", "ya no existe" in mt.aplicar(e, "equipo_borrar", {"codigo": "NADA"}))
    m.verdad("tipo_de infiere correctiva/preventiva en órdenes antiguas", mt.tipo_de({"plan": "CORR"}) == "correctiva" and mt.tipo_de({"plan": "P001"}) == "preventiva")


def _orden_por_id(e, ident):
    """Orden del estado con ese identificador."""
    return next(o for o in e["ordenes"] if o["id"] == ident)
