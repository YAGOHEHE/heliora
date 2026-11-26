"""KPIs de planta: energía con el paso de cada inversor, cobertura, resultados parciales y muestra honesta."""
from __future__ import annotations

import pandas as pd

from .banco import prueba
from .. import herramientas
from ..espacio import Espacio
from ..lib import mantenimiento as mt


def _dos_pasos():
    """Dos inversores a 100 kW durante 2 h: A registra cada 10 min y B cada 20 (200 kWh cada uno)."""
    t0 = pd.Timestamp("2024-06-01 10:00")
    a = pd.DataFrame({"fecha": pd.date_range(t0, periods=12, freq="10min"), "inversor": "A"})
    b = pd.DataFrame({"fecha": pd.date_range(t0, periods=6, freq="20min"), "inversor": "B"})
    d = pd.concat([a, b], ignore_index=True)
    d["irradiancia_wm2"], d["p_ac_kw"], d["temp_modulo_c"] = 800.0, 100.0, 25.0
    return d


def _informe(ident, valores, **variables):
    e = Espacio()
    e.ns.update(variables)
    return herramientas.ejecutar(ident, valores, e)


def _kv(inf, titulo, etiqueta):
    for s in inf["secciones"]:
        if s["tipo"] == "kv" and s["titulo"] == titulo:
            for it in s["items"]:
                if it[0] == etiqueta:
                    return it
    return None


@prueba(4, "Energía con el paso de cada inversor", "Un inversor cada 10 min y otro cada 20: 400 kWh, no 300")
def energia_por_inversor(m):
    """Cada lectura se integra con el paso de su propio inversor."""
    tot, t = mt.kpis(_dos_pasos(), 100.0, gamma=0.0, eta_inv=1.0)
    m.error("Energía total", tot["energia_kwh"], 400.0, 1e-9, "kWh")
    m.error("Energía de B (cada 20 min)", float(t.loc[t["inversor"] == "B", "energia_kwh"].iloc[0]), 200.0, 1e-9, "kWh")
    m.verdad("Pasos por inversor 10 y 20 min", tot["pasos_min"] == {"A": 10.0, "B": 20.0})
    m.error("Cobertura sin huecos", tot["cobertura"], 1.0, 1e-12)


@prueba(4, "Cobertura con huecos", "La energía de los huecos no se inventa y el informe da la cobertura")
def cobertura_con_huecos(m):
    """Un hueco de 4 lecturas en A baja la cobertura y la energía solo cuenta lo medido."""
    d = _dos_pasos()
    d = d.drop(index=d.index[(d["inversor"] == "A")][4:8])
    tot, t = mt.kpis(d, 100.0, gamma=0.0, eta_inv=1.0)
    m.error("Energía medida", tot["energia_kwh"], 400.0 - 4 * 100.0 / 6, 1e-9, "kWh")
    m.error("Cobertura", tot["cobertura"], 14 / 18, 1e-12)
    inf = _informe("mt_kpi", {"tabla": "scada", "p_nom": 100.0, "gamma": 0.0}, scada=d)
    m.verdad("Aviso de cobertura en el informe", any(s["tipo"] == "aviso" and "no se estima" in s["texto"] for s in inf["secciones"]))
    m.verdad("Bloque «Cobertura de los datos»", _kv(inf, "Cobertura de los datos", "Lecturas en el periodo") is not None)


@prueba(4, "KPIs parciales sin potencia DC", "Rendimiento no disponible y gráfica diaria explicada")
def kpis_parciales(m):
    """Sin p_dc_kw los demás KPIs siguen; sin días válidos, la gráfica lo explica en lugar de fallar."""
    inf = _informe("mt_kpi", {"tabla": "scada", "p_nom": 100.0, "gamma": 0.0}, scada=_dos_pasos())
    eta = _kv(inf, "Planta", "Rendimiento del inversor")
    m.verdad("Rendimiento: «No disponible · falta potencia DC»", eta is not None and eta[1] == "No disponible" and eta[2] == "falta potencia DC")
    m.verdad("Energía producida presente", _kv(inf, "Planta", "Energía producida") is not None)
    nublado = _dos_pasos().assign(irradiancia_wm2=200.0)
    inf = _informe("mt_kpi", {"tabla": "scada", "p_nom": 100.0, "gamma": 0.0}, scada=nublado)
    m.verdad("Con 200 W/m² siguen los KPIs", _kv(inf, "Planta", "Energía producida") is not None)
    fig = next(s["figura"] for s in inf["secciones"] if s["tipo"] == "grafica" and s["figura"]["titulo"].startswith("Razón diaria"))
    m.verdad("La gráfica diaria explica la ausencia", "Sin días válidos para A, B" in fig.get("nota", "") and not fig["series"])


@prueba(4, "Muestra SCADA honesta", "Siete días describen solo los eventos de esos siete días")
def muestra_honesta(m):
    """Con 7 días no hay string caído (día 200) ni sobretemperatura (día 400); con 730 sí, con fechas."""
    _, v = mt.generar_scada(dias=7)
    m.verdad("7 días: sin string caído ni IGBT", v["string_caido"] is None and v["igbt"] is None)
    inf = _informe("mt_muestra", {"nombre": "scada", "dias": 7})
    texto = " ".join(s.get("texto", "") for s in inf["secciones"])
    m.verdad("El informe no menciona los días 200 ni 400", "200" not in texto and "400" not in texto)
    d, v = mt.generar_scada(dias=730)
    m.verdad("730 días: string caído con fechas", v["string_caido"] is not None and v["string_caido"]["desde"].startswith("2024-07-19"))
    fin = d["fecha"].max()
    m.verdad("Todas las paradas dentro de los datos", all(pd.Timestamp(p["inicio"]) <= fin for p in v["paradas"]))
