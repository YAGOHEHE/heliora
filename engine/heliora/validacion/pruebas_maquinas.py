"""Dominio mecánico y máquinas eléctricas (encargo de modelos físicos, Fase 4, Anexo C.5)."""
from __future__ import annotations

import math

import numpy as np
from scipy.optimize import brentq

from .banco import prueba
from ..sim.diagrama import bloque, diagrama, hilo, simular


@prueba(2, "Mecánica: inercia, fricción, engranaje, resorte y par cuadrático",
        "ω(t) = (T/B)·(1 - e^(-(B/J)·t)); ω2 = ω1/n; ω = cos(√(K/J)·t); ω = √(T/k)")
def mecanica(m):
    """Casos analíticos del dominio rotacional."""
    d = diagrama("mec", [bloque("T", "constante", c=2.0), bloque("fp", "fuente_par"), bloque("J", "inercia", J=0.5),
                         bloque("B", "friccion", B=0.1), bloque("o", "osciloscopio", n=2)],
                 [hilo("T.y", "fp.T"), hilo("fp.p", "J.p"), hilo("J.p", "B.p"), hilo("J.w", "o.u1"), hilo("J.theta", "o.u2")],
                 metodo="dopri", t_fin=20.0, rtol=1e-9, atol=1e-10)
    s = simular(d)["senales"][0]
    t = s["t"]
    m.cota("Inercia con fricción: ω(t)", np.max(np.abs(s["trazas"][0] - 20.0 * (1 - np.exp(-0.2 * t)))), 1e-7, "rad/s")
    m.cota("Ángulo θ(t) = ∫ω dt", np.max(np.abs(s["trazas"][1] - 20.0 * (t - (1 - np.exp(-0.2 * t)) / 0.2))), 1e-6, "rad")
    # Engranaje n = 2 (reductor): la fricción del eje 2 se ve en el eje 1 dividida por n², B/n² = 0.025;
    # ω1(t) = 40·(1 - e^(-t/4)) con J = 0.1
    d = diagrama("eng", [bloque("T", "constante", c=1.0), bloque("fp", "fuente_par"), bloque("J", "inercia", J=0.1),
                         bloque("g", "engranaje", n=2.0), bloque("B", "friccion", B=0.1),
                         bloque("s2", "sensor_velocidad"), bloque("o", "osciloscopio", n=3)],
                 [hilo("T.y", "fp.T"), hilo("fp.p", "J.p"), hilo("J.p", "g.p1"), hilo("g.p2", "B.p"), hilo("g.p2", "s2.p"),
                  hilo("J.w", "o.u1"), hilo("s2.w", "o.u2"), hilo("g.T2", "o.u3")], metodo="dopri", t_fin=6.0, rtol=1e-9, atol=1e-10)
    s = simular(d)["senales"][0]
    t = s["t"]
    w1 = 40.0 * (1 - np.exp(-t / 4.0))
    m.cota("Engranaje: ω1(t) con la fricción reflejada B/n²", np.max(np.abs(s["trazas"][0] - w1)), 1e-6, "rad/s")
    m.cota("Engranaje: ω2 = ω1/n", np.max(np.abs(s["trazas"][1] - s["trazas"][0] / 2.0)), 1e-9, "rad/s")
    m.error("Engranaje: T2 = n·T1 = B·ω2", s["trazas"][2][-1], 0.1 * w1[-1] / 2.0, 1e-6, "N·m")
    d = diagrama("res", [bloque("J", "inercia", J=0.01, w0=1.0), bloque("K", "resorte_torsional", K=100.0),
                         bloque("o", "osciloscopio", n=1)],
                 [hilo("J.p", "K.p"), hilo("J.w", "o.u1")], metodo="dopri", t_fin=0.2, rtol=1e-10, atol=1e-12)
    s = simular(d)["senales"][0]
    m.cota("Resorte: ω(t) = cos(√(K/J)·t)", np.max(np.abs(s["trazas"][0] - np.cos(100.0 * s["t"]))), 1e-6, "rad/s")
    d = diagrama("ven", [bloque("T", "constante", c=4.0), bloque("fp", "fuente_par"), bloque("J", "inercia", J=0.05),
                         bloque("v", "par_carga", T0=0.0, k=0.01), bloque("o", "osciloscopio", n=1)],
                 [hilo("T.y", "fp.T"), hilo("fp.p", "J.p"), hilo("J.p", "v.p"), hilo("J.w", "o.u1")],
                 metodo="dopri", t_fin=10.0, rtol=1e-9, atol=1e-10)
    s = simular(d)["senales"][0]
    # J·dω/dt = T - k·ω²  →  ω(t) = ω∞·tanh(t·√(T·k)/J),  ω∞ = √(T/k)
    m.cota("Par cuadrático: ω(t) = √(T/k)·tanh(√(T·k)·t/J)",
           np.max(np.abs(s["trazas"][0] - 20.0 * np.tanh(math.sqrt(0.04) * s["t"] / 0.05))), 1e-6, "rad/s")
    # Dos inercias unidas por un engranaje ideal duplican un estado: se explica en lugar de fallar
    d = diagrama("dos", [bloque("J1", "inercia"), bloque("g", "engranaje", n=2.0), bloque("J2", "inercia")],
                 [hilo("J1.p", "g.p1"), hilo("g.p2", "J2.p")], metodo="dopri", t_fin=1.0)
    try:
        simular(d)
        m.verdad("Dos inercias unidas rígidamente: mensaje claro", False)
    except ValueError as e:
        m.verdad("Dos inercias unidas rígidamente: mensaje claro", "inercias" in str(e) and "engranaje" in str(e))


@prueba(2, "Máquinas: continua en régimen permanente", "i_a = (T_L + B·ω)/K_t;  ω = (v_a - R_a·i_a)/K_e (Anexo C.4)")
def maquina_dc(m):
    """100 V en el inducido y 2 N·m de carga: con R_a = 1 Ω, K = 0.5 V·s/rad y B = 1e-3 N·m·s."""
    d = diagrama("dc", [bloque("v", "fuente_dc", V=100.0), bloque("g", "tierra"),
                        bloque("mq", "maquina_dc", Ra=1.0, La=0.01, Ke=0.5, J=0.01, B=1e-3),
                        bloque("c", "par_carga", T0=2.0), bloque("o", "osciloscopio", n=3)],
                 [hilo("v.p", "mq.p"), hilo("mq.n", "g.t"), hilo("v.n", "g.t"), hilo("mq.eje", "c.p"),
                  hilo("mq.w", "o.u1"), hilo("mq.ia", "o.u2"), hilo("mq.Te", "o.u3")], metodo="dopri", t_fin=2.0, rtol=1e-9, atol=1e-10)
    s = simular(d)["senales"][0]
    ia = (2.0 + 1e-3 * 100.0 / 0.5) / (0.5 + 1e-3 * 1.0 / 0.5)
    m.error("Corriente de inducido", s["trazas"][1][-1], ia, 1e-6, "A")
    m.error("Velocidad", s["trazas"][0][-1], (100.0 - ia) / 0.5, 1e-5, "rad/s")
    m.error("Par = K_t·i_a", s["trazas"][2][-1], 0.5 * ia, 1e-6, "N·m")


def _pmsm_red(w0, T0, t_fin=6.0):
    """PMSM de 4 pares de polos con λ = 0.1 Wb en una red de 10 Hz con la tensión de su fuerza electromotriz."""
    f = 10.0
    VLL = 0.1 * 2 * math.pi * f * math.sqrt(1.5)
    return diagrama("pmsm", [bloque("red", "fuente_ac3", VLL=VLL, f=f), bloque("mq", "pmsm", w0=w0),
                             bloque("c", "par_carga", T0=T0), bloque("o", "osciloscopio", n=3)],
                    [hilo("red.a", "mq.abc"), hilo("mq.eje", "c.p"), hilo("mq.w", "o.u1"), hilo("mq.Te", "o.u2"),
                     hilo("mq.iq", "o.u3")], metodo="dopri", t_fin=t_fin, rtol=1e-7, atol=1e-9)


@prueba(2, "Máquinas: PMSM en sincronismo, constante de par y cortocircuito",
        "ω → ω_s/p;  K_t = 1.5·p·λ_pm;  par de cortocircuito de las ecuaciones dq en régimen (Anexo C.2)")
def pmsm(m):
    """Arranca al 90 % de la velocidad síncrona y se sincroniza; con carga, Te/i_q = 1.5·p·λ; en cortocircuito frena."""
    ws = 2 * math.pi * 10.0 / 4
    s = simular(_pmsm_red(0.9 * ws, 0.0))["senales"][0]
    m.error("En vacío alcanza la velocidad síncrona ω_s/p", s["trazas"][0][-1], ws, 1e-6, "rad/s")
    s = simular(_pmsm_red(ws, 0.3))["senales"][0]
    m.error("Con carga sigue en sincronismo", s["trazas"][0][-1], ws, 1e-5, "rad/s")
    m.error("Constante de par Te/i_q = 1.5·p·λ_pm", s["trazas"][1][-1] / s["trazas"][2][-1], 1.5 * 4 * 0.1, 1e-6, "N·m/A")
    m.error("Par = carga + fricción", s["trazas"][1][-1], 0.3 + 1e-4 * ws, 1e-5, "N·m")
    # Cortocircuito a 50 rad/s impuestos (J = 0): régimen de las ecuaciones dq con v_d = v_q = 0
    Rs, Ld, Lq, lam, pp, w = 0.5, 4e-3, 6e-3, 0.1, 4, 50.0
    d = diagrama("cc", [bloque("cc", "fuente_ac3", VLL=0.0), bloque("mq", "pmsm", Ld=Ld, Lq=Lq, J=0.0),
                        bloque("w", "constante", c=w), bloque("fv", "fuente_velocidad"), bloque("o", "osciloscopio", n=1)],
                 [hilo("cc.a", "mq.abc"), hilo("mq.eje", "fv.p"), hilo("w.y", "fv.w"), hilo("mq.Te", "o.u1")],
                 metodo="dopri", t_fin=0.5, rtol=1e-9, atol=1e-10)
    s = simular(d)["senales"][0]
    we = pp * w
    i_d, i_q = np.linalg.solve([[-Rs, we * Lq], [-we * Ld, -Rs]], [0.0, we * lam])
    Te = 1.5 * pp * (lam * i_q + (Ld - Lq) * i_d * i_q)
    m.error("Par de frenado en cortocircuito (L_d ≠ L_q)", s["trazas"][0][-1], Te, 1e-6, "N·m")


# Motor de inducción por defecto en una red de 400 V y 50 Hz
_IM = dict(Rs=1.5, Rr=1.2, Lls=6e-3, Llr=6e-3, Lm=0.18, pp=2)


def _par_circuito(s, V=400.0 / math.sqrt(3), f=50.0, Rs=1.5, Rr=1.2, Lls=6e-3, Llr=6e-3, Lm=0.18, pp=2):
    """Par del circuito equivalente por fase en régimen: Te = 3·p/ω_s·|I_r|²·R_r/s."""
    ws = 2 * math.pi * f
    Zm = 1j * ws * Lm
    Zs = Rs + 1j * ws * Lls
    Vth, Zth = V * Zm / (Zs + Zm), Zs * Zm / (Zs + Zm)
    Ir = Vth / (Zth + Rr / s + 1j * ws * Llr)
    return 3 * pp / ws * abs(Ir) ** 2 * Rr / s, Vth, Zth


def _induccion_a_velocidad(s, t_fin=2.0):
    ws = 2 * math.pi * 50.0
    d = diagrama("im", [bloque("red", "fuente_ac3", VLL=400.0, f=50.0), bloque("mq", "maquina_induccion", J=0.0, **_IM),
                        bloque("w", "constante", c=(1 - s) * ws / 2), bloque("fv", "fuente_velocidad"),
                        bloque("o", "osciloscopio", n=1)],
                 [hilo("red.a", "mq.abc"), hilo("mq.eje", "fv.p"), hilo("w.y", "fv.w"), hilo("mq.Te", "o.u1")],
                 metodo="dopri", t_fin=t_fin, rtol=1e-8, atol=1e-9)
    sig = simular(d)["senales"][0]
    return float(np.mean(sig["trazas"][0][sig["t"] > t_fin - 0.02]))


@prueba(2, "Máquinas: inducción, par de descolgamiento y arranque con carga",
        "Circuito equivalente exacto; T_max y s_k de Kloss; deslizamiento estable (Anexo C.3)")
def induccion(m):
    """El par en régimen a velocidad impuesta sigue al circuito equivalente; con carga se estabiliza en su deslizamiento."""
    ws = 2 * math.pi * 50.0
    _, Vth, Zth = _par_circuito(0.05)
    X = Zth.imag + ws * 6e-3
    sk = 1.2 / math.hypot(Zth.real, X)
    Tmax = 3 * 2 / ws * abs(Vth) ** 2 / (2 * (Zth.real + math.hypot(Zth.real, X)))
    m.error("Par a s = 0.04 frente al circuito equivalente", _induccion_a_velocidad(0.04), _par_circuito(0.04)[0], 0.005,
            relativo=True)
    m.error(f"Par de descolgamiento en s_k = {sk:.4f} (fórmula de Kloss)", _induccion_a_velocidad(sk), Tmax, 0.005,
            relativo=True)
    # Arranque directo desde parado con 20 N·m de carga
    TL, B = 20.0, 2e-3
    d = diagrama("arr", [bloque("red", "fuente_ac3", VLL=400.0, f=50.0), bloque("mq", "maquina_induccion", J=0.02, B=B, **_IM),
                         bloque("c", "par_carga", T0=TL), bloque("o", "osciloscopio", n=2)],
                 [hilo("red.a", "mq.abc"), hilo("mq.eje", "c.p"), hilo("mq.w", "o.u1"), hilo("mq.Te", "o.u2")],
                 metodo="dopri", t_fin=3.0, rtol=1e-7, atol=1e-9)
    sig = simular(d)["senales"][0]
    t, w = sig["t"], sig["trazas"][0]
    s_med = 1 - 2 * w[-1] / ws
    s_ref = brentq(lambda s: _par_circuito(s)[0] - TL - B * (1 - s) * ws / 2, 1e-6, sk)
    m.error("Deslizamiento estable frente al de régimen del circuito equivalente", s_med, s_ref, 0.005, relativo=True)
    m.cota("Velocidad estabilizada: |ω(3 s) - ω(2.5 s)|", w[-1] - np.interp(2.5, t, w), 1e-3, "rad/s")
