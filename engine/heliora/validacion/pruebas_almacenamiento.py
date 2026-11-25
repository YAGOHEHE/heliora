"""Almacenamiento: batería, supercondensador y cargador DC-DC (encargo de modelos físicos, Fase 3, Anexo B)."""
from __future__ import annotations

import math

import numpy as np

from .banco import prueba
from ..sim.diagrama import bloque, diagrama, hilo, simular

SOC = np.linspace(0.0, 1.0, 11)
OCV = np.array([3.0, 3.45, 3.55, 3.62, 3.67, 3.72, 3.79, 3.87, 3.96, 4.07, 4.2])


def _ocv(s):
    return np.interp(s, SOC, OCV)


def _energia_ocv(s0, s1, n=200001):
    """∫ OCV(s) ds entre s0 y s1 (tabla lineal a tramos, rejilla fina)."""
    s = np.linspace(s0, s1, n)
    return float(np.trapezoid(_ocv(s), s))


def _bateria_con_corriente(I, t_fin, **bat):
    """Batería con una fuente de corriente: I > 0 la descarga. Mide v, i, SoC y ∫v·i."""
    return diagrama("bat", [
        bloque("b", "bateria", **bat), bloque("fi", "fuente_idc", I=I), bloque("g", "tierra"),
        bloque("pv", "producto"), bloque("ev", "integrador"), bloque("o", "osciloscopio", n=4),
    ], [hilo("b.p", "fi.p"), hilo("fi.n", "b.n"), hilo("b.n", "g.t"),
        hilo("b.v", "pv.u1"), hilo("b.i", "pv.u2"), hilo("pv.y", "ev.u"),
        hilo("b.v", "o.u1"), hilo("b.i", "o.u2"), hilo("b.SoC", "o.u3"), hilo("ev.y", "o.u4")],
        metodo="dopri", t_fin=t_fin, rtol=1e-9, atol=1e-9)


@prueba(2, "Almacenamiento: descarga de un paquete a corriente constante",
        "v = N_s·OCV(SoC) - i·R0 - i·R1·(1 - e^(-t/τ)); balance de energía (Anexo B.1)")
def descarga(m):
    """Paquete 2s3p de celdas de 1 Ah a 6 A desde SoC = 1 durante 1620 s: el SoC baja a 0.1."""
    Ns, Np, I, T = 2, 3, 6.0, 1620.0
    s = simular(_bateria_con_corriente(I, T, Q=1.0, SoC0=1.0, Ns=Ns, Np=Np))["senales"][0]
    t, v, soc, ev = s["t"], s["trazas"][0], s["trazas"][2], s["trazas"][3]
    Qp, R0p, R1p, C1p = 3600.0 * Np, 0.03 * Ns / Np, 0.02 * Ns / Np, 2000.0 * Np / Ns
    tau = R1p * C1p
    m.error("SoC final por recuento de carga", soc[-1], 1.0 - I * T / Qp, 1e-9)
    v_ref = Ns * _ocv(1.0 - I * t / Qp) - I * R0p - I * R1p * (1 - np.exp(-t / tau))
    m.cota("Tensión en bornes frente a la curva OCV(SoC) analítica", np.max(np.abs(v - v_ref)), 1e-6, "V")
    # Energía química = entregada + pérdidas en R0 y R1 + energía que queda en C1
    quimica = Qp * Ns * _energia_ocv(soc[-1], 1.0)
    v1 = lambda tt: I * R1p * (1 - np.exp(-tt / tau))
    tt = np.linspace(0.0, T, 200001)
    perdidas = I**2 * R0p * T + float(np.trapezoid(v1(tt) ** 2 / R1p, tt)) + 0.5 * C1p * v1(T) ** 2
    m.error("Balance: entregada + pérdidas = energía química", ev[-1] + perdidas, quimica, 0.01, relativo=True)
    m.cota("Balance: cierre del balance (relativo)", abs(ev[-1] + perdidas - quimica) / quimica, 1e-5)


@prueba(2, "Almacenamiento: carga con eficiencia culómbica", "dSoC/dt = η·|i|/(3600·Q)")
def carga(m):
    """2 A de carga durante 900 s en una celda de 1 Ah con η = 0.99 desde SoC = 0.2."""
    s = simular(_bateria_con_corriente(-2.0, 900.0, Q=1.0, SoC0=0.2, eta=0.99))["senales"][0]
    m.error("SoC final", s["trazas"][2][-1], 0.2 + 0.99 * 2.0 * 900.0 / 3600.0, 1e-9)
    t, v = s["t"], s["trazas"][0]
    v_ref = _ocv(0.2 + 0.99 * 2.0 * t / 3600.0) + 2.0 * 0.03 + 2.0 * 0.02 * (1 - np.exp(-t / 40.0))
    m.cota("Tensión de carga frente a OCV(SoC) + i·R0 + v1", np.max(np.abs(v - v_ref)), 1e-6, "V")


@prueba(2, "Almacenamiento: supercondensador", "v_C = v0 - I·t/C;  fuga: v_C = v0·e^(-t/(R_fuga·C))")
def supercondensador(m):
    """Descarga a 10 A de 100 F con 10 mΩ, y autodescarga en vacío por una fuga de 10 Ω."""
    d = diagrama("sc", [bloque("sc", "supercondensador", C=100.0, R_esr=0.01, R_fuga=1e12, v0=2.7),
                        bloque("fi", "fuente_idc", I=10.0), bloque("g", "tierra"), bloque("o", "osciloscopio", n=3)],
                 [hilo("sc.p", "fi.p"), hilo("fi.n", "sc.n"), hilo("sc.n", "g.t"),
                  hilo("sc.v", "o.u1"), hilo("sc.i", "o.u2"), hilo("sc.E", "o.u3")],
                 metodo="dopri", t_fin=10.0, rtol=1e-9, atol=1e-9)
    s = simular(d)["senales"][0]
    t = s["t"]
    m.cota("Descarga: v = v0 - I·t/C - I·R_esr", np.max(np.abs(s["trazas"][0] - (2.7 - 0.1 * t - 0.1))), 1e-8, "V")
    m.error("Corriente de descarga", s["trazas"][1][-1], 10.0, 1e-9, "A")
    m.error("Energía almacenada al final", s["trazas"][2][-1], 0.5 * 100.0 * 1.7**2, 1e-6, "J")
    d = diagrama("fuga", [bloque("sc", "supercondensador", C=100.0, R_esr=0.01, R_fuga=10.0, v0=2.7),
                          bloque("g", "tierra"), bloque("o", "osciloscopio", n=1)],
                 [hilo("sc.n", "g.t"), hilo("sc.v", "o.u1")], metodo="dopri", t_fin=500.0, rtol=1e-9, atol=1e-10)
    s = simular(d)["senales"][0]
    m.cota("Autodescarga exponencial con τ = R_fuga·C = 1000 s",
           np.max(np.abs(s["trazas"][0] - 2.7 * np.exp(-s["t"] / 1000.0))), 1e-7, "V")


def _cargador(modo, ref, t_fin, SoC0, **extra):
    """Bus de 48 V → cargador → batería 4s1p de 0.05 Ah (los SoC cambian deprisa)."""
    return diagrama("carg", [
        bloque("bus", "fuente_dc", V=48.0), bloque("g", "tierra"), bloque("r", "constante", c=ref),
        bloque("c", "cargador_dcdc", modo=modo, L=1e-3, R_L=0.0, eta=0.95, f_sw=20000.0, i_max=3.0, i_min=-3.0, **extra),
        bloque("b", "bateria", Q=0.05, Ns=4, SoC0=SoC0),
        bloque("ea", "integrador"), bloque("eb", "integrador"), bloque("o", "osciloscopio", n=5),
    ], [hilo("bus.p", "c.alto"), hilo("bus.n", "g.t"), hilo("c.com", "g.t"), hilo("c.bajo", "b.p"), hilo("b.n", "g.t"),
        hilo("r.y", "c.ref"), hilo("c.P_alto", "ea.u"), hilo("c.P_bajo", "eb.u"),
        hilo("c.i_L", "o.u1"), hilo("b.v", "o.u2"), hilo("b.SoC", "o.u3"), hilo("ea.y", "o.u4"), hilo("eb.y", "o.u5")],
        metodo="bdf", t_fin=t_fin, rtol=1e-7, atol=1e-9)


@prueba(2, "Almacenamiento: cargador bidireccional", "Carga a corriente constante: OCV(SoC) y P_bajo = η·P_alto (Anexo B.3)")
def cargador(m):
    """Carga a 3 A, descarga a 3 A y carga a tensión constante de una batería pequeña desde un bus de 48 V."""
    s = simular(_cargador("corriente", 3.0, 30.0, 0.1))["senales"][0]
    t, iL, v, soc, ea, eb = s["t"], *s["trazas"]
    m.error("La corriente sigue a la consigna", iL[-1], 3.0, 1e-6, "A")
    m.error("SoC final: 0.1 + η_c·I·t/(3600·Q)", soc[-1], 0.1 + 0.99 * 3.0 * 30.0 / 180.0, 1e-4)
    sel = t > 0.05
    v_ref = 4 * _ocv(0.1 + 0.99 * 3.0 * t[sel] / 180.0) + 3.0 * 0.12 + 3.0 * 0.08 * (1 - np.exp(-t[sel] / 40.0))
    m.cota("Tensión de la batería frente a su curva OCV(SoC)", np.max(np.abs(v[sel] - v_ref)), 2e-3, "V")
    m.error("Carga: energía en el lado bajo = η·energía en el lado alto", eb[-1], 0.95 * ea[-1], 0.01, relativo=True)
    s = simular(_cargador("corriente", -3.0, 5.0, 0.6))["senales"][0]
    ea, eb = s["trazas"][3][-1], s["trazas"][4][-1]
    m.error("Descarga: energía al bus = η·energía de la batería", ea, 0.95 * eb, 0.01, relativo=True)
    # Con Kp_v = 5 A/V el error inicial de 1.4 V pide 7 A: arranca limitado a i_max = 3 A
    s = simular(_cargador("tensión", 15.6, 40.0, 0.2, Kp_v=5.0, Ki_v=50.0))["senales"][0]
    m.error("Modo tensión: la batería llega a la consigna", s["trazas"][1][-1], 15.6, 2e-3, "V")
    i_ini = s["trazas"][0][np.argmax(s["t"] > 0.01)]
    m.verdad(f"Modo tensión: la corriente se limita a i_max al empezar ({i_ini:.4f} A)", abs(i_ini - 3.0) < 1e-3)
