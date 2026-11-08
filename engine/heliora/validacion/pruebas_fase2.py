"""Fase 2: elementos eléctricos, convertidores, control y fotovoltaica."""
from __future__ import annotations

import math

import numpy as np

from .banco import prueba
from ..lib import electrica, fotovoltaica as fv, potencia as pot  # noqa: F401
from ..sim.diagrama import bloque, diagrama, hilo, simular

SOLVERS = [("euler", dict(paso=1e-6)), ("rk4", dict(paso=1e-5)),
           ("dopri", dict(rtol=1e-6, atol=1e-8)), ("bdf", dict(rtol=1e-6, atol=1e-8))]


def _circuito_rc(metodo, **kw):
    return diagrama("RC", [
        bloque("v", "fuente_dc", V=10.0), bloque("tierra", "tierra"), bloque("r", "resistor", R=1000.0),
        bloque("c", "capacitor", C=1e-6), bloque("vm", "voltimetro"), bloque("o", "osciloscopio", n=1),
    ], [hilo("v.p", "r.p"), hilo("r.n", "c.p"), hilo("c.n", "tierra.t"), hilo("v.n", "tierra.t"),
        hilo("c.p", "vm.p"), hilo("c.n", "vm.n"), hilo("vm.v", "o.u1")], metodo=metodo, t_fin=5e-3, **kw)


def _circuito_rl(metodo, **kw):
    return diagrama("RL", [
        bloque("v", "fuente_dc", V=10.0), bloque("tierra", "tierra"), bloque("r", "resistor", R=10.0),
        bloque("l", "inductor", L=10e-3), bloque("am", "amperimetro"), bloque("o", "osciloscopio", n=1),
    ], [hilo("v.p", "am.p"), hilo("am.n", "r.p"), hilo("r.n", "l.p"), hilo("l.n", "tierra.t"), hilo("v.n", "tierra.t"),
        hilo("am.i", "o.u1")], metodo=metodo, t_fin=5e-3, **kw)


@prueba(2, "Escalón RC con los cuatro solvers", "v_C(t) = V·(1 - e^(-t/RC))")
def rc(m):
    """El circuito se simula con elementos físicos R y C; el error se compara con la solución exacta."""
    for metodo, kw in SOLVERS:
        s = simular(_circuito_rc(metodo, **kw))["senales"][0]
        e = np.max(np.abs(s["trazas"][0] - 10 * (1 - np.exp(-s["t"] / 1e-3))))
        m.cota(f"{metodo}: error máximo de v_C", e, {"euler": 3e-3, "rk4": 1e-7, "dopri": 1e-4, "bdf": 3e-3}[metodo], "V")


@prueba(2, "Escalón RL con los cuatro solvers", "i_L(t) = (V/R)·(1 - e^(-t·R/L))")
def rl(m):
    """Corriente medida con un amperímetro en serie frente a la solución exacta."""
    for metodo, kw in SOLVERS:
        s = simular(_circuito_rl(metodo, **kw))["senales"][0]
        e = np.max(np.abs(s["trazas"][0] - (1 - np.exp(-s["t"] / 1e-3))))
        m.cota(f"{metodo}: error máximo de i_L", e, {"euler": 3e-4, "rk4": 1e-8, "dopri": 1e-5, "bdf": 3e-4}[metodo], "A")


@prueba(2, "RLC serie subamortiguado", "Solución analítica de segundo orden")
def rlc(m):
    """v_C(t) = V·(1 - e^(-αt)·(cos(ωd·t) + (α/ωd)·sin(ωd·t)))"""
    R, L, C, V = 10.0, 1e-3, 1e-6, 5.0
    d = diagrama("RLC", [
        bloque("v", "fuente_dc", V=V), bloque("tierra", "tierra"), bloque("rlc", "rlc", R=R, L=L, C=C, modo="serie"),
        bloque("am", "amperimetro"), bloque("o", "osciloscopio", n=1),
    ], [hilo("v.p", "am.p"), hilo("am.n", "rlc.p"), hilo("rlc.n", "tierra.t"), hilo("v.n", "tierra.t"), hilo("am.i", "o.u1")],
        metodo="rk4", t_fin=1e-3, paso=2e-7)
    s = simular(d)["senales"][0]
    al = R / (2 * L)
    wd = math.sqrt(1 / (L * C) - al**2)
    i = V / (wd * L) * np.exp(-al * s["t"]) * np.sin(wd * s["t"])
    m.cota("Error máximo de la corriente", np.max(np.abs(s["trazas"][0] - i)), 1e-6, "A")


@prueba(2, "Fuentes AC, transformador y RMS", "V_rms y relación de transformación")
def ac(m):
    """Una fuente de 230 V con transformador 2:1 y carga R debe dar 115 V eficaces."""
    d = diagrama("AC", [
        bloque("v", "fuente_ac", V=230.0, f=50.0), bloque("g", "tierra"),
        bloque("t", "transformador", a=2.0, Lm=100.0, R2=1e-3, L2=1e-3), bloque("r", "resistor", R=100.0),
        bloque("vm", "voltimetro"), bloque("rms", "rms", f0=50.0), bloque("o", "osciloscopio", n=1),
    ], [hilo("v.p", "t.p1"), hilo("v.n", "g.t"), hilo("t.n1", "g.t"), hilo("t.p2", "r.p"), hilo("r.n", "g.t"), hilo("t.n2", "g.t"),
        hilo("t.p2", "vm.p"), hilo("t.n2", "vm.n"), hilo("vm.v", "rms.u"), hilo("rms.y", "o.u1")],
        metodo="rk4", t_fin=0.06, paso=1e-5)
    s = simular(d)["senales"][0]
    m.error("V eficaz en el secundario", s["trazas"][0][-1], 115.0, 2e-3, "por unidad", relativo=True)


@prueba(2, "Boost: promediado y conmutado", "V_sal = V_ent/(1 - D)")
def boost(m):
    """Un elevador en lazo abierto en régimen frente a la relación ideal, con pérdidas pequeñas."""
    Vin, D, R = 100.0, 0.5, 20.0
    def caso(tipo, entrada, extra):
        return diagrama("b", [
            bloque("v", "fuente_dc", V=Vin), bloque("g", "tierra"),
            bloque("b", tipo, L=1e-3, R=0.0 if tipo == "boost_prom" else 0.0, **extra),
            bloque("c", "capacitor", C=100e-6, v0=Vin / (1 - D)), bloque("r", "resistor", R=R),
            bloque("vm", "voltimetro"), entrada, bloque("o", "osciloscopio", n=1),
        ], [hilo("v.p", "b.in"), hilo("v.n", "g.t"), hilo("b.out", "c.p"), hilo("b.out", "r.p"), hilo("c.n", "g.t"), hilo("r.n", "g.t"),
            hilo("b.com", "g.t"), hilo("b.out", "vm.p"), hilo("c.n", "vm.n"), hilo("vm.v", "o.u1")] +
            ([hilo("d.y", "b.d")] if tipo == "boost_prom" else [hilo("d.y", "pw.d"), hilo("pw.g", "b.g")]),
            metodo="rk4", t_fin=0.03, paso=5e-6 if tipo == "boost_prom" else 1e-6)
    s = simular(caso("boost_prom", bloque("d", "constante", c=D), {}))["senales"][0]
    m.error("Promediado: V_sal en régimen", s["trazas"][0][-1], Vin / (1 - D), 1e-2, "por unidad", relativo=True)
    d2 = caso("boost_conm", bloque("d", "constante", c=D), dict(Ron=1e-4, Rd=1e-4, Vf=0.0))
    d2["sistema"]["bloques"].append(bloque("pw", "pwm", fc=20000.0))
    s = simular(d2)["senales"][0]
    v_media = np.mean(s["trazas"][0][s["t"] > 0.02])
    m.error("Conmutado: V_sal media en régimen", v_media, Vin / (1 - D), 1e-2, "por unidad", relativo=True)


@prueba(2, "Transformadas y SVPWM", "Identidades analíticas")
def transformadas(m):
    """Clarke-Park de un sistema equilibrado; SVPWM min-max frente a los tiempos del vector espacial."""
    th = 1.1
    abc = 100 * np.array([math.cos(th), math.cos(th - 2 * math.pi / 3), math.cos(th + 2 * math.pi / 3)])
    al, be = pot.clarke(abc)
    d, q = pot.park(al, be, th)
    m.error("v_d = amplitud", d, 100.0, 1e-10)
    m.cota("v_q = 0", q, 1e-10)
    m.cota("Clarke inversa", np.max(np.abs(pot.clarke_inv(al, be) - abc)), 1e-10)
    peor = 0.0
    for ang in np.linspace(0, 2 * math.pi, 73)[:-1]:
        v = 300.0 * np.array([math.cos(ang), math.cos(ang - 2 * math.pi / 3), math.cos(ang + 2 * math.pi / 3)])
        a, b = pot.clarke(v)
        por_espacio = pot.ciclos_svpwm(a, b, 700.0)
        mx = v / 350.0
        mm = mx - 0.5 * (mx.max() + mx.min())
        peor = max(peor, float(np.max(np.abs(por_espacio - 0.5 * (1 + mm)))))
    m.cota("SVPWM: min-max frente al vector espacial (ciclo de trabajo)", peor, 1e-9)


def _inversor_ref(modulacion="spwm", Mi=0.8, tf=0.04, dt=1e-6):
    p = dict(Ron=0.002, Vce0=0.8, Rd=0.002, Vf=0.9, Eon=139.96e-3, Eoff=174.78e-3, Err=42e-3, Vref=600.0, Iref=1191.06)
    return diagrama("inv", [
        bloque("dc", "fuente_dc", V=700.0), bloque("g", "tierra"),
        bloque("inv", "inversor_conm", **p),
        bloque("mod", "moduladora3", M=Mi, f=50.0), bloque("pwm", modulacion, fc=5000.0),
        bloque("med", "medidor3"),
        bloque("rl", "filtro_l", L=5e-3, R=1e-4),
        bloque("carga", "resistor3", R=7.0), bloque("n", "fuente_ac3", VLL=0.0, f=50.0),
        bloque("o", "osciloscopio", n=4),
    ], [hilo("dc.p", "inv.dc_p"), hilo("dc.n", "inv.dc_n"), hilo("dc.n", "g.t"),
        hilo("mod.m", "pwm.m"), hilo("pwm.s", "inv.s"),
        hilo("inv.ac", "med.a"), hilo("med.b", "rl.a"), hilo("rl.b", "carga.a"), hilo("carga.b", "n.a"),
        hilo("med.v_abc", "o.u1"), hilo("med.i_abc", "o.u2"), hilo("inv.E_cond", "o.u3"), hilo("inv.E_sw", "o.u4")],
        metodo="rk4", t_fin=tf, paso=dt), p


@prueba(2, "Inversor de referencia: THD y eficiencia", "V1 = m·Vdc/2; THD = √(8/(√3·π·m) - 1); pérdidas de valor medio")
def inversor(m):
    """Inversor de 700 V, 5 kHz y m = 0.8 con carga RL. Compara fundamental, THD de tensión de línea y eficiencia."""
    d, p = _inversor_ref()
    r = simular(d)
    s = r["senales"][0]
    t = s["t"]
    fs = 1.0 / 1e-6
    v = s["trazas"][0]
    i = s["trazas"][1]
    ec, es = s["trazas"][2], s["trazas"][3]
    sel = t >= 0.02 - 1e-9
    vab = v[sel, 0] - v[sel, 1]
    Mi = 0.8
    # Fundamental de línea: √3·m·Vdc/2
    thd_med, v1 = pot.thd_total(vab, fs, 50.0)
    m.error("Fundamental de v_ab (incluye la caída en los dispositivos)", v1, math.sqrt(3) * Mi * 700 / 2, 0.015, "por unidad", relativo=True)
    m.error("THD de tensión de línea", thd_med, pot.thd_analitica_linea(Mi), 0.02, "por unidad", relativo=True)
    # Corriente de pico y factor de potencia de la carga (R = 7 Ω, L = 5 mH)
    Z = complex(7.0 + 1e-4, 2 * math.pi * 50 * 5e-3)
    Ipk_a = Mi * 350 / abs(Z)
    i_a = i[sel, 0]
    F = np.fft.rfft(i_a) * 2 / i_a.size
    k = int(round(i_a.size * 50.0 / fs))
    m.error("Corriente de pico", abs(F[k]), Ipk_a, 0.015, "por unidad", relativo=True)
    cosfi = Z.real / abs(Z)
    an = pot.perdidas_analiticas(Ipk_a, Mi, cosfi, 700.0, 5000.0, p)
    T = t[-1] - t[sel][0]
    Pc = (ec[-1] - ec[sel][0]) / T
    Ps = (es[-1] - es[sel][0]) / T
    m.error("Pérdidas de conducción", Pc, an["cond"], 0.05, "por unidad", relativo=True)
    m.error("Pérdidas de conmutación", Ps, an["sw"], 0.10, "por unidad", relativo=True)
    Psal = 1.5 * Ipk_a * (Mi * 350) * cosfi
    eta_an = Psal / (Psal + an["total"])
    eta_sim = Psal / (Psal + Pc + Ps)
    m.error("Eficiencia", eta_sim, eta_an, 0.01)


@prueba(2, "SRF-PLL y control dq sobre una red", "θ sincronizado; i_d sigue su referencia; P = 1.5·v_d·i_d")
def control(m):
    """Inversor promediado con control de corriente y PLL conectado a la red por un filtro L."""
    p = dict(Ron=0.0, Vce0=0.0, Rd=0.0, Vf=0.0, Eon=0.0, Eoff=0.0, Err=0.0)
    d = diagrama("dq", [
        bloque("dc", "fuente_dc", V=750.0), bloque("g", "tierra"), bloque("inv", "inversor_prom", Ron=1e-6, Rd=1e-6, **{k: v for k, v in p.items() if k not in ("Ron", "Rd")}),
        bloque("fil", "filtro_l", L=1e-3, R=0.01), bloque("red", "red_infinita", VLL=400.0, L=50e-6, R=1e-3),
        bloque("pll", "srf_pll", f0=50.0),
        bloque("ctrl", "control_dq", Kp=3.0, Ki=300.0, L=1.05e-3),
        bloque("id", "escalon", t0=0.1, y0=0.0, y1=100.0), bloque("iq", "constante", c=0.0),
        bloque("vb", "voltimetro"), bloque("mod", "svpwm", fc=5000.0), bloque("pq", "potencia_pq", f0=50.0),
        bloque("o", "osciloscopio", n=6),
    ], [hilo("dc.p", "inv.dc_p"), hilo("dc.n", "inv.dc_n"), hilo("dc.n", "g.t"), hilo("inv.ac", "fil.a"), hilo("fil.b", "red.a"),
        hilo("red.v_pcc", "pll.v_abc"), hilo("id.y", "ctrl.id_ref"), hilo("iq.y", "ctrl.iq_ref"), hilo("fil.i_abc", "ctrl.i_abc"),
        hilo("red.v_pcc", "ctrl.v_abc"), hilo("pll.theta", "ctrl.theta"), hilo("dc.p", "vb.p"), hilo("dc.n", "vb.n"), hilo("vb.v", "ctrl.vbus"),
        hilo("ctrl.m_abc", "mod.m"), hilo("mod.d", "inv.d"), hilo("red.v_pcc", "pq.v_abc"), hilo("red.i_abc", "pq.i_abc"),
        hilo("ctrl.i_d", "o.u1"), hilo("ctrl.i_q", "o.u2"), hilo("pll.omega", "o.u3"), hilo("pq.P", "o.u4"), hilo("pq.Q", "o.u5"), hilo("pq.FP", "o.u6")],
        metodo="rk4", t_fin=0.3, paso=2e-5)
    s = simular(d)["senales"][0]
    y = s["trazas"]
    Vp = 400 * math.sqrt(2 / 3)
    m.error("PLL: ω en régimen", y[2][-1], 2 * math.pi * 50, 0.05, "rad/s")
    m.error("Control: i_d en régimen", y[0][-1], 100.0, 0.5, "A")
    m.cota("Control: i_q en régimen", y[1][-1], 0.5, "A")
    m.error("Potencia activa", y[3][-1], 1.5 * Vp * 100, 0.01, "por unidad", relativo=True)
    m.cota("Potencia reactiva / activa", abs(y[4][-1]) / y[3][-1], 0.03)
    m.error("Factor de potencia", y[5][-1], 1.0, 0.002)


@prueba(2, "PI con anti-windup", "Saturación sin acumulación de integral")
def pi_aw(m):
    """Con saturación, el retrocálculo debe limitar la integral: al invertir el error, sale rápido."""
    d = diagrama("pi", [
        bloque("e", "escalon", t0=1.0, y0=1.0, y1=-1.0), bloque("pi", "pi", Kp=0.0, Ki=1.0, y_min=-0.5, y_max=0.5, Kaw=100.0),
        bloque("o", "osciloscopio", n=1)], [hilo("e.y", "pi.e"), hilo("pi.y", "o.u1")], metodo="rk4", t_fin=2.0, paso=1e-3)
    s = simular(d)["senales"][0]
    t, y = s["t"], s["trazas"][0]
    m.error("Saturado a 0.5 antes de t=1", y[t < 1.0][-1], 0.5, 1e-9)
    # Tras invertir el error, sale de saturación en 0.01 (error integral retrocalculado) y alcanza -0.5 en t = 2
    m.error("Llega a -0.5 tras invertir", np.interp(2.0, t, y), -0.5, 0.01)
    cruce = t[(t > 1.0) & (y <= 0.0)][0]
    m.error("Sin windup: cruza cero a los 0.5 s de invertir", cruce, 1.5, 0.02, "s")


@prueba(2, "Módulo comercial frente a su hoja de datos", "CS1H-330MS, AS-6M30-HC y CS6X-300M a STC")
def hoja_datos(m):
    """Se extraen los cinco parámetros y se compara la curva I-V con los puntos de la hoja."""
    for nombre, datos in fv.CATALOGO.items():
        mod = fv.ModuloFV(nombre=nombre, **datos).extraer()
        V, I, P, pm = fv.curva_iv(mod, 1000.0, 25.0, 2000)
        corto = nombre.split(" ")[0]
        m.cota(f"{corto}: residuo de extracción", mod.residuo, 1e-6)
        m.error(f"{corto}: I_sc", I[0], datos["Isc"], 1e-3, "por unidad", relativo=True)
        m.error(f"{corto}: V_oc", V[-1], datos["Voc"], 1e-3, "por unidad", relativo=True)
        m.error(f"{corto}: V_mp", pm["Vmp"], datos["Vmp"], 2e-3, "por unidad", relativo=True)
        m.error(f"{corto}: I_mp", pm["Imp"], datos["Imp"], 2e-3, "por unidad", relativo=True)
        m.error(f"{corto}: P_mp", pm["Pmp"], datos["Vmp"] * datos["Imp"], 1e-3, "por unidad", relativo=True)


@prueba(2, "Modelo de un diodo frente a pvlib", "pvlib.pvsystem.singlediode y calcparams_desoto")
def contra_pvlib(m):
    """El mismo módulo con G y T variadas: el motor debe coincidir con la implementación de pvlib."""
    import pvlib
    mod = fv.modulo_de_catalogo("CS6X-300M (300 W, 72 celdas)")
    for G, T in ((1000, 25), (800, 45), (400, 60), (200, 15)):
        IL, I0, Rs, Rsh, a = pvlib.pvsystem.calcparams_desoto(
            G, T, mod.alfa_sc, mod.a, mod.IL, mod.I0, mod.Rsh, mod.Rs, EgRef=fv.EG_REF, dEgdT=-fv.DEG)
        ref = pvlib.pvsystem.singlediode(IL, I0, Rs, Rsh, a)
        V, I, P, pm = fv.curva_iv(mod, G, T, 1500)
        m.error(f"G={G}, T={T}: P_mp", pm["Pmp"], float(ref["p_mp"]), 1e-3, "por unidad", relativo=True)
        m.error(f"G={G}, T={T}: V_oc", V[-1], float(ref["v_oc"]), 1e-3, "por unidad", relativo=True)


@prueba(2, "Sombreado parcial y bypass", "V_oc total y dos máximos locales")
def sombra(m):
    """Con un sub-módulo sombreado, el diodo de bypass limita la caída y aparecen varios máximos."""
    mod = fv.modulo_de_catalogo("CS1H-330MS (330 W, 66 celdas)")
    limpio = [1.0] * 36
    V, I = fv.curva_string(mod, limpio, 25.0)
    m.error("V_oc del string sin sombra", V[-1], 12 * mod.Voc, 0.002, "por unidad", relativo=True)
    m.error("I_sc del string sin sombra", np.interp(0.0, V, I), mod.Isc, 0.01, "por unidad", relativo=True)
    somb = [1.0] * 24 + [0.3] * 12
    V2, I2 = fv.curva_string(mod, somb, 25.0)
    Vs = np.linspace(0.0, V2[-1], 1500)
    P2 = Vs * np.maximum(np.interp(Vs, V2, I2), 0)
    picos = int(np.sum((P2[1:-1] > P2[:-2]) & (P2[1:-1] > P2[2:]) & (P2[1:-1] > 0.15 * P2.max())))
    m.verdad(f"Curva P-V con dos o más máximos ({picos})", picos >= 2)
    P_sin = float(np.max(np.linspace(0, V[-1], 1500) * np.maximum(np.interp(np.linspace(0, V[-1], 1500), V, I), 0)))
    m.verdad("La sombra reduce la potencia", P2.max() < P_sin)
    # La corriente de bypass limita la tensión negativa a -Vd por sub-módulo
    m.verdad("Con bypass la tensión mínima es finita", V2[0] > -40 * 0.7)


@prueba(2, "Arreglo en el circuito y MPPT", "Punto de trabajo y máxima potencia")
def arreglo_mppt(m):
    """El arreglo con carga R cae en la curva I-V; los dos MPPT llegan al MPP en un boost."""
    mod = fv.modulo_de_catalogo("CS1H-330MS (330 W, 66 celdas)")
    d = diagrama("pvr", [
        bloque("G", "constante", c=1000.0), bloque("T", "constante", c=25.0), bloque("t", "tierra"),
        bloque("pv", "arreglo_fv", modulo="CS1H-330MS (330 W, 66 celdas)", Ns=2, Np=2),
        bloque("c", "capacitor", C=100e-6, v0=70.0), bloque("r", "resistor", R=4.0),
        bloque("o", "osciloscopio", n=2),
    ], [hilo("G.y", "pv.G"), hilo("T.y", "pv.T"), hilo("pv.p", "c.p"), hilo("pv.p", "r.p"), hilo("pv.n", "t.t"), hilo("c.n", "t.t"), hilo("r.n", "t.t"),
        hilo("pv.v", "o.u1"), hilo("pv.i", "o.u2")], metodo="rk4", t_fin=0.02, paso=5e-6)
    s = simular(d)["senales"][0]
    v, i = s["trazas"][0][-1], s["trazas"][1][-1]
    m.error("Ley de Ohm en el punto de trabajo: V/I", v / i, 4.0, 1e-3, "Ω")
    par = fv.parametros_operacion(mod, 1000.0, 25.0)
    m.error("Corriente frente a la curva I(V)", i, 2 * float(fv.i_de_v(v / 2, *par)), 1e-4, "A")

    _, _, _, pm = fv.curva_iv(mod, 1000.0, 25.0, 1500)
    Pmax = 12 * 20 * pm["Pmp"]
    for metodo in ("Perturbar y observar", "Conductancia incremental"):
        d = diagrama("mp", [
            bloque("G", "constante", c=1000.0), bloque("T", "constante", c=25.0), bloque("t", "tierra"),
            bloque("pv", "arreglo_fv", modulo="CS1H-330MS (330 W, 66 celdas)", Ns=12, Np=20),
            bloque("c", "capacitor", C=2e-3, v0=420.0), bloque("b", "boost_prom", L=1e-3, R=0.01, i0=170.0),
            bloque("bus", "fuente_dc", V=750.0), bloque("mppt", "mppt", metodo=metodo, Ts=5e-3, dd=0.002, d0=0.45),
            bloque("o", "osciloscopio", n=1, cada=20),
        ], [hilo("G.y", "pv.G"), hilo("T.y", "pv.T"), hilo("pv.p", "c.p"), hilo("pv.n", "t.t"), hilo("c.n", "t.t"), hilo("c.p", "b.in"),
            hilo("b.out", "bus.p"), hilo("bus.n", "t.t"), hilo("b.com", "t.t"), hilo("pv.v", "mppt.v"), hilo("b.i_L", "mppt.i"),
            hilo("mppt.d", "b.d"), hilo("pv.pot", "o.u1")], metodo="rk4", t_fin=0.5, paso=2e-5)
        s = simular(d)["senales"][0]
        P_media = float(np.mean(s["trazas"][0][s["t"] > 0.35]))
        m.error(f"{metodo}: potencia frente al MPP", P_media, Pmax, 0.01, "por unidad", relativo=True)


POSICIONES_PLANTA = {
    "G": (0, 0), "T": (0, 100), "pv": (196, 20), "cpv": (392, 150), "mppt": (392, 300), "boost": (588, 20),
    "tierra": (588, 170), "pwm": (588, 300), "cdc": (784, 150), "vb": (784, 270), "bus": (784, 380), "iq": (784, 480),
    "inv": (980, 20), "mod": (980, 190), "ctrl": (980, 340), "lcl": (1176, 20), "red": (1372, 20), "pll": (1372, 190),
    "pq": (1372, 350), "osc1": (1568, 0), "osc2": (1568, 160), "osc3": (1568, 320),
}


def ordenar_planta(d):
    """Coloca los bloques de la planta: potencia arriba, control abajo, de izquierda a derecha."""
    for b in d["sistema"]["bloques"]:
        if b["id"] in POSICIONES_PLANTA:
            b["x"], b["y"] = POSICIONES_PLANTA[b["id"]]
    return d


def planta_promediada(t_fin=1.0, dt=5e-5):
    """Ejemplo completo: arreglo FV → boost con MPPT → bus DC → inversor → LCL → red."""
    bloques = [
        bloque("G", "escalon", 40, 40, "Irradiancia", t0=0.5, y0=1000.0, y1=700.0),
        bloque("T", "constante", 40, 130, "Temperatura de celda", c=40.0),
        bloque("pv", "arreglo_fv", 200, 70, "Arreglo FV", modulo="CS1H-330MS (330 W, 66 celdas)", Ns=12, Np=20),
        bloque("cpv", "capacitor", 340, 170, "C entrada", C=2e-3, v0=420.0),
        bloque("mppt", "mppt", 360, 300, "MPPT", metodo="Perturbar y observar", Ts=5e-3, dd=0.002, d0=0.44),
        bloque("boost", "boost_prom", 480, 70, "Boost", L=1e-3, R=0.01, i0=170.0),
        bloque("cdc", "capacitor", 640, 170, "Bus DC", C=8e-3, v0=750.0),
        bloque("vb", "voltimetro", 620, 290, "V bus"),
        bloque("inv", "inversor_prom", 780, 70, "Inversor", fsw=5000.0),
        bloque("lcl", "filtro_lcl", 960, 70, "Filtro LCL", L1=0.5e-3, R1=0.01, L2=0.1e-3, R2=0.005, C=50e-6, Rd=0.5),
        bloque("red", "red_infinita", 1140, 70, "Red", VLL=400.0, R=0.01, L=50e-6),
        bloque("tierra", "tierra", 480, 230, "Tierra"),
        bloque("pll", "srf_pll", 1140, 240, "PLL", f0=50.0),
        bloque("bus", "control_bus", 620, 390, "Control bus DC", Vref=750.0, Kp=2.0, Ki=25.0, Imax=250.0, Imin=-30.0, I0=115.0),
        bloque("iq", "constante", 620, 480, "i_q*", c=0.0),
        bloque("ctrl", "control_dq", 840, 400, "Control dq", Kp=1.9, Ki=100.0, L=0.65e-3),
        bloque("mod", "svpwm", 840, 260, "SVPWM", fc=5000.0),
        bloque("pq", "potencia_pq", 1140, 400, "P, Q, FP", f0=50.0),
        bloque("osc1", "osciloscopio", 1320, 60, "Potencias", n=3),
        bloque("osc2", "osciloscopio", 1320, 200, "Bus y entrada", n=3),
        bloque("osc3", "osciloscopio", 1320, 340, "Corrientes", n=3, cada=2),
    ]
    con = [
        hilo("G.y", "pv.G"), hilo("T.y", "pv.T"), hilo("pv.p", "cpv.p"), hilo("pv.n", "tierra.t"), hilo("cpv.n", "tierra.t"),
        hilo("cpv.p", "boost.in"), hilo("boost.com", "tierra.t"), hilo("boost.out", "cdc.p"), hilo("cdc.n", "tierra.t"),
        hilo("boost.out", "inv.dc_p"), hilo("cdc.n", "inv.dc_n"), hilo("inv.ac", "lcl.a"), hilo("lcl.b", "red.a"),
        hilo("cdc.p", "vb.p"), hilo("cdc.n", "vb.n"), hilo("pv.v", "mppt.v"), hilo("boost.i_L", "mppt.i"), hilo("mppt.d", "boost.d"),
        hilo("red.v_pcc", "pll.v_abc"), hilo("vb.v", "bus.vbus"), hilo("bus.id_ref", "ctrl.id_ref"), hilo("iq.y", "ctrl.iq_ref"),
        hilo("lcl.i2_abc", "ctrl.i_abc"), hilo("red.v_pcc", "ctrl.v_abc"), hilo("pll.theta", "ctrl.theta"), hilo("vb.v", "ctrl.vbus"),
        hilo("ctrl.m_abc", "mod.m"), hilo("mod.d", "inv.d"), hilo("red.v_pcc", "pq.v_abc"), hilo("red.i_abc", "pq.i_abc"),
        hilo("pv.pot", "osc1.u1"), hilo("pq.P", "osc1.u2"), hilo("pq.Q", "osc1.u3"),
        hilo("vb.v", "osc2.u1"), hilo("pv.v", "osc2.u2"), hilo("pq.FP", "osc2.u3"),
        hilo("ctrl.i_d", "osc3.u1"), hilo("ctrl.i_q", "osc3.u2"), hilo("lcl.i2_abc", "osc3.u3"),
    ]
    return ordenar_planta(diagrama("Planta FV completa (promediada)", bloques, con, metodo="rk4", t_fin=t_fin, paso=dt, rtol=1e-3, atol=1e-6))


@prueba(2, "Ejemplo completo promediado", "Balance de energía y regulación")
def ejemplo_completo(m):
    """El ejemplo completo debe seguir el MPP, regular el bus e inyectar potencia a la red a FP ≈ 1."""
    mod = fv.modulo_de_catalogo("CS1H-330MS (330 W, 66 celdas)")
    _, _, _, pm = fv.curva_iv(mod, 1000.0, 40.0, 1500)
    _, _, _, pm2 = fv.curva_iv(mod, 700.0, 40.0, 1500)
    d = planta_promediada()
    r = simular(d)
    pot_s = next(s for s in r["senales"] if s["etiqueta"] == "Potencias")
    bus_s = next(s for s in r["senales"] if s["etiqueta"] == "Bus y entrada")
    t = pot_s["t"]
    a = (t > 0.35) & (t < 0.5)
    b = t > 0.85
    Ppv1 = np.mean(pot_s["trazas"][0][a])
    Ppv2 = np.mean(pot_s["trazas"][0][b])
    m.error("P arreglo a 1000 W/m² frente al MPP", Ppv1, 240 * pm["Pmp"], 0.01, "por unidad", relativo=True)
    m.error("P arreglo a 700 W/m² frente al MPP", Ppv2, 240 * pm2["Pmp"], 0.015, "por unidad", relativo=True)
    Pred = np.mean(pot_s["trazas"][1][a])
    # El balance debe cerrar: la potencia inyectada es la del arreglo menos las pérdidas (<5 %)
    m.verdad(f"Potencia en la red entre 90 % y 100 % de la del arreglo ({Pred/1000:.1f} kW)", 0.9 * Ppv1 < Pred < Ppv1 * 1.001)
    m.error("Bus DC regulado", np.mean(bus_s["trazas"][0][a]), 750.0, 0.01, "por unidad", relativo=True)
    m.error("Factor de potencia", np.abs(np.mean(bus_s["trazas"][2][a])), 1.0, 0.01)
