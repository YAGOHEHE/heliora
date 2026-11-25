"""Dominio térmico y acoplamiento electro-térmico (encargo de modelos físicos, Fase 2, Anexo A.7)."""
from __future__ import annotations

import math

import numpy as np
from scipy.optimize import brentq

from .banco import prueba
from .pruebas_fase2 import _inversor_ref
from ..sim.diagrama import bloque, diagrama, hilo, simular


@prueba(2, "Térmica: régimen permanente de una cadena de resistencias", "Tj = T_amb + P·ΣR_th")
def permanente(m):
    """100 W por R_jc = 0.1 K/W y R_ha = 0.2 K/W hasta un ambiente a 40 °C: Tj = 70 °C."""
    d = diagrama("perm", [
        bloque("P", "constante", c=100.0), bloque("q", "fuente_calor"),
        bloque("cj", "capacidad_termica", C=10.0, T0=40.0),
        bloque("r1", "resistencia_termica", R=0.1), bloque("r2", "resistencia_termica", R=0.2),
        bloque("amb", "ambiente", T_amb=40.0), bloque("tj", "termometro"), bloque("o", "osciloscopio", n=2),
    ], [hilo("P.y", "q.Q"), hilo("q.p", "cj.p"), hilo("cj.p", "r1.p"), hilo("r1.n", "r2.p"), hilo("r2.n", "amb.p"),
        hilo("cj.p", "tj.p"), hilo("tj.T", "o.u1"), hilo("amb.Q", "o.u2")],
        metodo="dopri", t_fin=80.0, rtol=1e-8, atol=1e-10)
    s = simular(d)["senales"][0]
    m.error("Temperatura de unión en régimen (t = 27·τ)", s["trazas"][0][-1], 40.0 + 100.0 * 0.3, 1e-5, "°C")
    m.error("Calor que llega al ambiente", s["trazas"][1][-1], 100.0, 1e-4, "W")


@prueba(2, "Térmica: transitorio de un polo", "T(t) = T_amb + P·R·(1 - e^(-t/(R·C)))")
def un_polo(m):
    """50 W en una capacidad de 2 J/K unida al ambiente (25 °C) por 0.5 K/W: τ = 1 s."""
    d = diagrama("polo", [
        bloque("P", "constante", c=50.0), bloque("q", "fuente_calor"), bloque("c", "capacidad_termica", C=2.0, T0=25.0),
        bloque("r", "resistencia_termica", R=0.5), bloque("amb", "ambiente", T_amb=25.0), bloque("o", "osciloscopio", n=1),
    ], [hilo("P.y", "q.Q"), hilo("q.p", "c.p"), hilo("c.p", "r.p"), hilo("r.n", "amb.p"), hilo("c.T", "o.u1")],
        metodo="dopri", t_fin=5.0, rtol=1e-8, atol=1e-10)
    s = simular(d)["senales"][0]
    t, T = s["t"], s["trazas"][0]
    m.cota("Error máximo frente a la exponencial", np.max(np.abs(T - (25.0 + 25.0 * (1 - np.exp(-t))))), 1e-6, "K")
    # Constante de tiempo despejada de la muestra más cercana a t = 1 s: τ = -t / ln(1 - ΔT/(P·R))
    k = int(np.argmin(np.abs(t - 1.0)))
    tau = -t[k] / math.log(1.0 - (T[k] - 25.0) / 25.0)
    m.error("Constante de tiempo medida", tau, 1.0, 1e-5, "s")


@prueba(2, "Térmica: impedancia Foster y red Cauer", "Z_th(t) = Σ R_i·(1 - e^(-t/τ_i)); Cauer: R_th = ΣR_i")
def foster(m):
    """Con 100 W en a y b a 0 °C, T_a/P reproduce la Z_th publicada; la Cauer llega a ΣR en régimen."""
    R, tau = [0.01, 0.02, 0.015], [1e-3, 1e-2, 1e-1]
    d = diagrama("foster", [
        bloque("P", "constante", c=100.0), bloque("q", "fuente_calor"),
        bloque("f", "red_termica_rc", modo="foster", R=R, tau=tau),
        bloque("c", "red_termica_rc", modo="cauer", R=R, C=[0.05, 0.2, 2.0], T0=0.0),
        bloque("q2", "fuente_calor"), bloque("amb", "ambiente", T_amb=0.0),
        bloque("ta", "termometro"), bloque("tc", "termometro"), bloque("o", "osciloscopio", n=2),
    ], [hilo("P.y", "q.Q"), hilo("q.p", "f.a"), hilo("f.b", "amb.p"), hilo("f.a", "ta.p"),
        hilo("P.y", "q2.Q"), hilo("q2.p", "c.a"), hilo("c.b", "amb.p"), hilo("c.a", "tc.p"),
        hilo("ta.T", "o.u1"), hilo("tc.T", "o.u2")], metodo="dopri", t_fin=1.5, rtol=1e-9, atol=1e-11)
    s = simular(d)["senales"][0]
    t = s["t"]
    zth = sum(r * (1 - np.exp(-t / ta)) for r, ta in zip(R, tau))
    m.cota("Foster: |T_a/P - Z_th(t)| máximo", np.max(np.abs(s["trazas"][0] / 100.0 - zth)), 1e-7, "K/W")
    m.error("Cauer: T_a/P en régimen = ΣR", s["trazas"][1][-1] / 100.0, sum(R), 1e-6, "K/W")


def _chopper_termico(t_fin):
    """IGBT siempre encendido con 100 V y 0.5 Ω; cápsula → 0.02 K/W → disipador (5 J/K, 0.2 K/W) → 40 °C.

    La red Foster de la unión es de dos pares (ΣR = 0.045 K/W, como la de defecto): con las τ de hasta
    0.5 s de la de defecto, sus capacidades en serie con el disipador dan un modo de unos 12 s.
    """
    return diagrama("et", [
        bloque("v", "fuente_dc", V=100.0), bloque("rl", "resistor", R=0.5), bloque("g", "tierra"),
        bloque("on", "constante", c=1.0), bloque("q", "igbt", Rth_q="[0.02, 0.025]", tau_q="[1e-3, 1e-2]", tau_e=1e-3),
        bloque("rch", "resistencia_termica", R=0.02), bloque("dis", "capacidad_termica", C=5.0, T0=40.0),
        bloque("rha", "resistencia_termica", R=0.2), bloque("amb", "ambiente", T_amb=40.0),
        bloque("vce", "voltimetro"), bloque("o", "osciloscopio", n=3),
    ], [hilo("v.p", "rl.p"), hilo("rl.n", "q.c"), hilo("q.e", "g.t"), hilo("v.n", "g.t"), hilo("on.y", "q.g"),
        hilo("q.th", "rch.p"), hilo("rch.n", "dis.p"), hilo("dis.p", "rha.p"), hilo("rha.n", "amb.p"),
        hilo("q.c", "vce.p"), hilo("q.e", "vce.n"),
        hilo("q.Tj", "o.u1"), hilo("vce.v", "o.u2"), hilo("q.i", "o.u3")],
        metodo="dopri", t_fin=t_fin, rtol=1e-6, atol=1e-8)


@prueba(2, "Térmica: lazo electro-térmico de un IGBT", "Punto fijo Tj = T_amb + P(Tj)·ΣR_th (Anexo A.4)")
def electrotermico(m):
    """Las pérdidas calientan la unión, Tj cambia V_ce0 y R_on, y el lazo converge al punto fijo analítico."""
    s = simular(_chopper_termico(12.0))["senales"][0]
    t, Tj, vce = s["t"], s["trazas"][0], s["trazas"][1]
    # Punto fijo con los valores por defecto del bloque (T_ref = 125 °C, a_V = -2 mV/K, a_R = +0.6 %/K)
    Rth = 0.045 + 0.02 + 0.2

    def residuo(T):
        dT = T - 125.0
        V0, Ron = 0.8 - 2e-3 * dT, 0.002 * (1 + 6e-3 * dT)
        i = (100.0 - V0) / (0.5 + Ron)
        return T - (40.0 + (V0 + Ron * i) * i * Rth)
    Tref = brentq(residuo, 40.0, 300.0)
    m.error("Tj en régimen frente al punto fijo analítico", Tj[-1], Tref, 0.05, "°C")
    # En t = 0 el IGBT aún no conduce (la compuerta se lee al final del primer paso)
    v_frio = vce[np.argmax(t > 1e-3)]
    m.verdad(f"V_ce sube al calentarse ({v_frio:.4f} → {vce[-1]:.4f} V)", vce[-1] > v_frio)
    m.cota("Converge: |Tj(12 s) - Tj(11 s)|", Tj[-1] - np.interp(11.0, t, Tj), 0.02, "K")


@prueba(2, "Térmica: pérdidas del inversor como calor", "Calor al ambiente en un periodo = Δ(E_cond + E_sw)")
def inversor_termico(m):
    """Cada IGBT y diodo calienta su propia unión; en régimen periódico el calor que sale es la pérdida.

    En una red Foster el flujo en la cápsula es igual al de la unión en cada instante (los pulsos de
    conmutación llegan sin filtrar), así que el calor se integra con el solver y no por trapecios.
    La salida E_cond es una suma de Riemann por la derecha de v·i; el modelo térmico integra v·i con
    el solver: con 100 pasos por periodo de conmutación difieren en torno al 0.7 %.
    """
    d, _ = _inversor_ref(tf=0.04, dt=2e-6)
    inv = next(b for b in d["sistema"]["bloques"] if b["id"] == "inv")
    inv["params"].update(Rth_q="[0.03, 0.015]", tau_q="[1e-4, 5e-4]", Rth_d="[0.06, 0.03]", tau_d="[1e-4, 5e-4]", tau_e=1e-5)
    d["sistema"]["bloques"] += [bloque("rch", "resistencia_termica", R=0.02), bloque("amb", "ambiente", T_amb=40.0),
                                bloque("iq", "integrador"), bloque("o2", "osciloscopio", n=3)]
    d["sistema"]["conexiones"] += [hilo("inv.th", "rch.p"), hilo("rch.n", "amb.p"), hilo("amb.Q", "iq.u"),
                                   hilo("iq.y", "o2.u1"), hilo("inv.Tj_q", "o2.u2"), hilo("inv.Tj_d", "o2.u3")]
    r = simular(d)
    s = next(x for x in r["senales"] if x["bloque"] == "o2")
    e = next(x for x in r["senales"] if x["bloque"] == "o")
    calor = s["trazas"][0][-1] - np.interp(0.02, s["t"], s["trazas"][0])
    E = e["trazas"][2] + e["trazas"][3]
    perdida = E[-1] - np.interp(0.02, e["t"], E)
    m.error("Calor al ambiente en el último periodo frente a la pérdida", calor, perdida, 0.01, "por unidad", relativo=True)
    Tq, Td = s["trazas"][1], s["trazas"][2]
    m.verdad(f"Seis uniones de IGBT y seis de diodo por encima del ambiente (Tj_q máx {Tq[-1].max():.1f} °C)",
             Tq.shape[1] == 6 and Td.shape[1] == 6 and Tq[-1].min() > 40.0 and Td[-1].min() > 40.0)


@prueba(2, "Térmica: los dominios no se mezclan", "Un puerto térmico no se une a uno eléctrico")
def dominios(m):
    """Unir la cápsula de un IGBT a un nodo eléctrico es un error con un mensaje claro."""
    d = diagrama("mal", [bloque("v", "fuente_dc", V=10.0), bloque("q", "igbt"), bloque("g", "tierra")],
                 [hilo("v.p", "q.c"), hilo("q.e", "g.t"), hilo("v.n", "g.t"), hilo("q.th", "g.t")], metodo="rk4", paso=1e-3)
    try:
        simular(d)
        m.verdad("Se rechaza unir el puerto térmico a la tierra eléctrica", False)
    except ValueError as e:
        texto = str(e)
        m.verdad(f"Se rechaza unir el puerto térmico a la tierra eléctrica ({texto})",
                 texto.startswith("No se puede unir") and "térmico" in texto and "eléctrico" in texto)
