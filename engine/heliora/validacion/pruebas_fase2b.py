"""Fase 2 (continuación): el resto de elementos, con su solución analítica, y la planta conmutada."""
from __future__ import annotations

import math

import numpy as np

from .banco import prueba
from ..lib import potencia as pot
from ..sim.diagrama import bloque, diagrama, hilo, simular


def _amplitud(t, y, f0):
    """Amplitud de la fundamental en los dos últimos periodos, ya sin el transitorio inicial."""
    T = 1.0 / f0
    sel = t >= t[-1] - 2 * T + 1e-12
    tt, yy = t[sel], y[sel]
    w = 2 * math.pi * f0
    a = 2 * np.trapezoid(yy * np.cos(w * tt), tt) / (tt[-1] - tt[0])
    b = 2 * np.trapezoid(yy * np.sin(w * tt), tt) / (tt[-1] - tt[0])
    return math.hypot(a, b)


@prueba(2, "RLC en paralelo", "v(t) = (I/(C·ωd))·e^(-αt)·sin(ωd·t)")
def rlc_paralelo(m):
    """Una fuente de corriente sobre un RLC paralelo, frente a la solución de segundo orden."""
    R, L, C, I = 100.0, 1e-3, 1e-6, 1.0
    d = diagrama("rlcp", [
        bloque("i", "fuente_idc", I=I), bloque("g", "tierra"), bloque("rlc", "rlc", R=R, L=L, C=C, modo="paralelo"),
        bloque("vm", "voltimetro"), bloque("o", "osciloscopio", n=1)],
        [hilo("i.p", "g.t"), hilo("i.n", "rlc.p"), hilo("rlc.n", "g.t"), hilo("rlc.p", "vm.p"), hilo("rlc.n", "vm.n"), hilo("vm.v", "o.u1")],
        metodo="rk4", t_fin=5e-4, paso=2e-7)
    s = simular(d)["senales"][0]
    al = 1 / (2 * R * C)
    wd = math.sqrt(1 / (L * C) - al**2)
    v = I / (C * wd) * np.exp(-al * s["t"]) * np.sin(wd * s["t"])
    m.cota("Error máximo de la tensión", np.max(np.abs(s["trazas"][0] - v)), 1e-3, "V")


@prueba(2, "Diodo: rectificador de media onda", "V_media = V_pico/π")
def diodo(m):
    """Fuente AC, diodo y resistencia: la tensión media en la carga es V_pico/π."""
    V = 230.0
    d = diagrama("rect", [
        bloque("v", "fuente_ac", V=V, f=50.0), bloque("g", "tierra"), bloque("d", "diodo", Ron=1e-3, Vf=0.0), bloque("r", "resistor", R=100.0),
        bloque("vm", "voltimetro"), bloque("o", "osciloscopio", n=1)],
        [hilo("v.p", "d.p"), hilo("d.n", "r.p"), hilo("r.n", "g.t"), hilo("v.n", "g.t"), hilo("r.p", "vm.p"), hilo("r.n", "vm.n"), hilo("vm.v", "o.u1")],
        metodo="rk4", t_fin=0.04, paso=1e-5)
    s = simular(d)["senales"][0]
    sel = s["t"] > 0.02
    m.error("Tensión media en la carga", np.mean(s["trazas"][0][sel]), V * math.sqrt(2) / math.pi, 0.01, "por unidad", relativo=True)
    # En bloqueo solo queda la fuga del modelo: Goff·V_pico·R = 1e-6·325·100 = 0.0325 V
    m.error("Tensión negativa = fuga del diodo en bloqueo", -np.min(s["trazas"][0]), 1e-6 * V * math.sqrt(2) * 100, 1e-3, "V")


def _dc_con_llave(llave, gate, sensor="am.i", tf=1e-3):
    return diagrama("llave", [
        bloque("v", "fuente_dc", V=10.0), bloque("g", "tierra"), llave, bloque("r", "resistor", R=10.0), bloque("am", "amperimetro"),
        bloque("gate", "constante", c=gate), bloque("o", "osciloscopio", n=1)], [], metodo="rk4", t_fin=tf, paso=1e-5)


@prueba(2, "MOSFET e IGBT en conducción", "Ley de Ohm con R_on y V_ce0; energía de conducción")
def llaves(m):
    """Con la compuerta fija se comparan corriente y pérdidas con el cálculo a mano."""
    def caso(tipo, puertos, g, **kw):
        d = diagrama("llave", [
            bloque("v", "fuente_dc", V=10.0), bloque("g", "tierra"), bloque("q", tipo, **kw), bloque("r", "resistor", R=10.0), bloque("am", "amperimetro"),
            bloque("gate", "constante", c=g), bloque("o", "osciloscopio", n=2 if tipo == "igbt" else 1)],
            [hilo("v.p", "am.p"), hilo("am.n", f"q.{puertos[0]}"), hilo(f"q.{puertos[1]}", "r.p"), hilo("r.n", "g.t"), hilo("v.n", "g.t"), hilo("gate.y", "q.g"), hilo("am.i", "o.u1")]
            + ([hilo("q.E_cond", "o.u2")] if tipo == "igbt" else []), metodo="rk4", t_fin=1e-3, paso=1e-5)
        return simular(d)["senales"][0]
    s = caso("mosfet", ("d", "s"), 1.0, Ron=0.02)
    m.error("MOSFET encendido: i = V/(R + R_on)", s["trazas"][0][-1], 10 / 10.02, 1e-6, "A")
    s = caso("mosfet", ("d", "s"), 0.0, Ron=0.02, Goff=1e-6)
    m.cota("MOSFET apagado: corriente de fuga", s["trazas"][0][-1], 2e-5, "A")
    s = caso("igbt", ("c", "e"), 1.0, Ron=0.05, Vce0=1.0)
    i = (10 - 1.0) / 10.05
    m.error("IGBT encendido: i = (V - V_ce0)/(R + R_on)", s["trazas"][0][-1], i, 1e-6, "A")
    m.error("IGBT: energía de conducción = (V_ce0 + R_on·i)·i·t", s["trazas"][1][-1], (1.0 + 0.05 * i) * i * 1e-3, 0.03, "por unidad", relativo=True)


@prueba(2, "Puente H conmutado y promediado", "v_ab = (d_a - d_b)·V_bus")
def puente_h(m):
    """Con órdenes fijas (conmutado) y con ciclos fijos (promediado), frente a la tensión esperada en la carga."""
    d = diagrama("h", [
        bloque("v", "fuente_dc", V=100.0), bloque("g", "tierra"), bloque("h", "puente_h_conm", Ron=0.1, Vce0=1.0), bloque("r", "resistor", R=10.0), bloque("am", "amperimetro"),
        bloque("sa", "constante", c=1.0), bloque("sb", "constante", c=0.0), bloque("o", "osciloscopio", n=1)],
        [hilo("v.p", "h.dc_p"), hilo("v.n", "h.dc_n"), hilo("v.n", "g.t"), hilo("sa.y", "h.s_a"), hilo("sb.y", "h.s_b"), hilo("h.a", "am.p"), hilo("am.n", "r.p"), hilo("r.n", "h.b"), hilo("am.i", "o.u1")],
        metodo="rk4", t_fin=1e-3, paso=1e-5)
    s = simular(d)["senales"][0]
    # La tolerancia admite la fuga de los dos IGBT abiertos (1e-6 S × 100 V)
    m.error("Conmutado: i = (V - 2·V_ce0)/(R + 2·R_on)", s["trazas"][0][-1], (100 - 2.0) / 10.2, 1e-5, "A")
    d = diagrama("hp", [
        bloque("v", "fuente_dc", V=100.0), bloque("g", "tierra"), bloque("h", "puente_h_prom"), bloque("r", "resistor", R=10.0), bloque("am", "amperimetro"), bloque("am2", "amperimetro"),
        bloque("da", "constante", c=0.75), bloque("db", "constante", c=0.25), bloque("o", "osciloscopio", n=2)],
        [hilo("v.p", "am2.p"), hilo("am2.n", "h.dc_p"), hilo("v.n", "h.dc_n"), hilo("v.n", "g.t"), hilo("da.y", "h.d_a"), hilo("db.y", "h.d_b"), hilo("h.a", "am.p"), hilo("am.n", "r.p"), hilo("r.n", "h.b"), hilo("am.i", "o.u1"), hilo("am2.i", "o.u2")],
        metodo="rk4", t_fin=1e-3, paso=1e-5)
    s = simular(d)["senales"][0]
    m.error("Promediado: i_carga = (d_a - d_b)·V/R", s["trazas"][0][-1], 5.0, 1e-6, "A")
    m.error("Promediado: balance de potencia, i_bus = (d_a - d_b)·i_carga", abs(s["trazas"][1][-1]), 2.5, 1e-6, "A")


@prueba(2, "Filtro LC y elementos trifásicos", "Fasores en régimen permanente")
def trifasicos(m):
    """Fuente trifásica con L, C, R y filtro LC, frente al cálculo con fasores."""
    Vp = 400 * math.sqrt(2 / 3)
    w = 2 * math.pi * 50
    # Filtro LC con carga en estrella
    L, Rs, C, R = 1e-3, 0.5, 20e-6, 10.0
    d = diagrama("lc", [
        bloque("v", "fuente_ac3", VLL=400.0, f=50.0), bloque("f", "filtro_lc", L=L, R=Rs, C=C), bloque("r", "resistor3", R=R), bloque("n", "fuente_ac3", VLL=0.0, f=50.0),
        bloque("o", "osciloscopio", n=1)],
        [hilo("v.a", "f.a"), hilo("f.b", "r.a"), hilo("r.b", "n.a"), hilo("f.v_c", "o.u1")], metodo="rk4", t_fin=0.1, paso=1e-5)
    s = simular(d)["senales"][0]
    Zp = 1 / (1 / R + 1j * w * C)
    m.error("Filtro LC: amplitud de v_c", _amplitud(s["t"], s["trazas"][0][:, 0], 50.0), Vp * abs(Zp / (Rs + 1j * w * L + Zp)), 0.005, "por unidad", relativo=True)
    # L trifásica con C trifásica
    L, Rs, C = 5e-3, 5.0, 100e-6
    d = diagrama("lc3", [
        bloque("v", "fuente_ac3", VLL=400.0, f=50.0), bloque("l", "inductor3", L=L, R=Rs), bloque("c", "capacitor3", C=C), bloque("m", "medidor3"), bloque("o", "osciloscopio", n=1)],
        [hilo("v.a", "l.a"), hilo("l.b", "m.a"), hilo("m.b", "c.a"), hilo("l.i_abc", "o.u1")], metodo="rk4", t_fin=0.1, paso=1e-5)
    s = simular(d)["senales"][0]
    Z = Rs + 1j * w * L + 1 / (1j * w * C)
    m.error("L y C trifásicas: amplitud de corriente", _amplitud(s["t"], s["trazas"][0][:, 0], 50.0), Vp / abs(Z), 0.005, "por unidad", relativo=True)
    # Transformador trifásico
    d = diagrama("tr", [
        bloque("v", "fuente_ac3", VLL=400.0, f=50.0), bloque("t", "transformador3", a=2.0, L=1e-4, R=0.005, Lm=10.0), bloque("r", "resistor3", R=10.0), bloque("n", "fuente_ac3", VLL=0.0, f=50.0), bloque("o", "osciloscopio", n=1)],
        [hilo("v.a", "t.p"), hilo("t.s", "r.a"), hilo("r.b", "n.a"), hilo("t.i2_abc", "o.u1")], metodo="rk4", t_fin=0.06, paso=5e-6)
    s = simular(d)["senales"][0]
    m.error("Transformador trifásico: corriente del lado 2", _amplitud(s["t"], s["trazas"][0][:, 0], 50.0), (Vp / 2) / abs(10.005 + 1j * w * 1e-4), 0.005, "por unidad", relativo=True)
    # Red con impedancia sobre una carga resistiva
    d = diagrama("red", [
        bloque("red", "red_infinita", VLL=400.0, R=0.1, L=1e-3), bloque("r", "resistor3", R=10.0), bloque("n", "fuente_ac3", VLL=0.0, f=50.0), bloque("o", "osciloscopio", n=1)],
        [hilo("red.a", "r.a"), hilo("r.b", "n.a"), hilo("red.i_abc", "o.u1")], metodo="rk4", t_fin=0.06, paso=5e-6)
    s = simular(d)["senales"][0]
    m.error("Red con impedancia: corriente", _amplitud(s["t"], s["trazas"][0][:, 0], 50.0), Vp / abs(10.1 + 1j * w * 1e-3), 0.005, "por unidad", relativo=True)


@prueba(2, "Medición de THD y SVPWM con sobremodulación", "Señal con armónicos conocidos; V1 = m·Vdc/2")
def medicion(m):
    """El bloque THD sobre una señal de 10 % de tercer armónico; SVPWM alcanza m = 1.1 sin saturar."""
    d = diagrama("thd", [
        bloque("a", "senoidal", A=100.0, f=50.0), bloque("b", "senoidal", A=10.0, f=150.0), bloque("s", "suma", signos="++"), bloque("t", "thd", f0=50.0, periodos=2),
        bloque("o", "osciloscopio", n=2)], [hilo("a.y", "s.u1"), hilo("b.y", "s.u2"), hilo("s.y", "t.u"), hilo("t.thd_pct", "o.u1"), hilo("t.fundamental", "o.u2")],
        metodo="rk4", t_fin=0.09, paso=1e-5)
    s = simular(d)["senales"][0]
    m.error("THD", s["trazas"][0][-1], 10.0, 0.01, "%")
    m.error("Fundamental", s["trazas"][1][-1], 100.0, 0.05)
    from .pruebas_fase2 import _inversor_ref
    d, _ = _inversor_ref("svpwm", 1.1, tf=0.04, dt=1e-6)
    s = simular(d)["senales"][0]
    sel = s["t"] >= 0.02 - 1e-9
    v = s["trazas"][0]
    _, v1 = pot.thd_total(v[sel, 0] - v[sel, 1], 1e6, 50.0)
    m.error("SVPWM: fundamental de línea con m = 1.1 (lineal)", v1, math.sqrt(3) * 1.1 * 350, 0.03, "por unidad", relativo=True)


@prueba(2, "Bloques de recurso solar", "Coherencia con pvlib")
def solar(m):
    """Sol en plano inclinado y temperatura de celda dentro del diagrama, frente a pvlib directo."""
    import pvlib
    d = diagrama("sol", [
        bloque("s", "sol_poa", lat=40.4, lon=-3.7, fecha="2026-06-21", hora0=12.0, escala=3600.0), bloque("ta", "constante", c=25.0), bloque("tc", "termico_modulo", viento=1.0),
        bloque("o", "osciloscopio", n=2)], [hilo("s.G", "tc.G"), hilo("ta.y", "tc.T_amb"), hilo("s.G", "o.u1"), hilo("tc.T_c", "o.u2")], metodo="rk4", t_fin=1.0, paso=0.25)
    s = simular(d)["senales"][0]
    G, T = s["trazas"][0][-1], s["trazas"][1][-1]
    m.verdad(f"Irradiancia al mediodía plausible ({G:.0f} W/m²)", 700 < G < 1150)
    par = pvlib.temperature.TEMPERATURE_MODEL_PARAMETERS["sapm"]["open_rack_glass_polymer"]
    m.error("Temperatura de celda frente a pvlib", T, float(pvlib.temperature.sapm_cell(G, 25.0, 1.0, **par)), 1e-9, "°C")


@prueba(2, "Ejemplo completo conmutado", "Balance de potencia, bus regulado y THD de corriente")
def planta_conmutada(m):
    """La planta con boost e inversor conmutados: arreglo → boost con MPPT → bus → inversor → LCL → red."""
    from .ejemplos import planta_conmutada as diag
    r = simular(diag())
    por = {s["etiqueta"]: s for s in r["senales"]}
    p = por["Potencias"]
    b = por["Bus y entrada"]
    # Balance de energía en los últimos 20 ms (la ventana de la medida de P):
    # P_arreglo = P_red + dE_bus/dt + dE_entrada/dt + pérdidas, con E = C·V²/2
    t0, t1 = 0.06, 0.08
    Ppv = float(np.mean(p["trazas"][0][p["t"] >= t0]))
    Pred = float(p["trazas"][1][-1])
    energia = lambda C, tr: 0.5 * C * (np.interp(t1, b["t"], tr) ** 2 - np.interp(t0, b["t"], tr) ** 2) / (t1 - t0)
    Palmacenada = energia(8e-3, b["trazas"][0]) + energia(2e-3, b["trazas"][1])
    perdidas = (Ppv - Pred - Palmacenada) / Ppv
    m.verdad(f"Balance de energía: pérdidas de 0 a 10 % ({100 * perdidas:.2f} %; red {Pred/1000:.1f} kW, arreglo {Ppv/1000:.1f} kW)", 0.0 < perdidas < 0.10)
    m.error("Bus DC regulado", float(np.mean(b["trazas"][0][b["t"] > 0.065])), 750.0, 0.02, "por unidad", relativo=True)
    i = por["Corriente de red"]
    t = i["t"]
    # Último periodo completo, remuestreado a paso uniforme
    tg = np.arange(0.06, 0.08, 4e-6)
    ia = np.interp(tg, t, i["trazas"][0][:, 0])
    h, a1 = pot.thd_total(ia, 1 / 4e-6, 50.0)
    m.verdad(f"THD de corriente de red por debajo de 8 % ({100 * h:.2f} %)", h < 0.08)
    m.verdad(f"Corriente fundamental coherente con la potencia ({a1:.1f} A)", abs(1.5 * 326.6 * a1 - Pred) < 0.1 * Pred)
