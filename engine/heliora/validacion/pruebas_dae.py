"""Solver DAE: almacenes unidos rígidamente, valores iniciales incompatibles y acuerdo con el motor de siempre."""
from __future__ import annotations

import copy

import numpy as np

from .banco import prueba
from ..sim.diagrama import bloque, diagrama, hilo, simular


def _falla(d, config=None):
    """Mensaje de error de la simulación, o None si terminó."""
    try:
        simular(copy.deepcopy(d), config=config)
    except ValueError as e:
        return str(e)
    return None


def _rc_paralelo(v0a=0.0, v0b=0.0, metodo="dae"):
    """Fuente de 10 V con R = 1 kΩ cargando dos condensadores en paralelo (1 µF y 3 µF)."""
    return diagrama("rc paralelo", [
        bloque("v", "fuente_dc", V=10.0), bloque("r", "resistor", R=1000.0),
        bloque("c1", "capacitor", C=1e-6, v0=v0a), bloque("c2", "capacitor", C=3e-6, v0=v0b),
        bloque("g", "tierra"), bloque("vm", "voltimetro"), bloque("o", "osciloscopio", n=1),
    ], [hilo("v.p", "r.p"), hilo("r.n", "c1.p"), hilo("r.n", "c2.p"), hilo("c1.n", "g.t"), hilo("c2.n", "g.t"),
        hilo("v.n", "g.t"), hilo("r.n", "vm.p"), hilo("vm.n", "g.t"), hilo("vm.v", "o.u1")],
        metodo=metodo, t_fin=0.02, rtol=1e-8, atol=1e-11)


@prueba(1, "DAE: condensadores en paralelo", "v(t) = V·(1 - e^(-t/(R·(C1+C2))))")
def dae_condensadores_paralelo(m):
    """El motor de siempre no admite dos condensadores en paralelo; el DAE los resuelve con su suma.

    El DAE es de orden 2 y controla el error de cada paso: el error global es unas decenas de veces rtol.
    """
    error = _falla(_rc_paralelo(metodo="dopri"))
    m.verdad("Motor ODE: aviso de condensadores en paralelo", error is not None and "condensadores" in error)
    s = simular(_rc_paralelo())["senales"][0]
    t, v = s["t"], s["trazas"][0]
    m.cota("DAE: carga con τ = R·(C1+C2) = 4 ms", np.max(np.abs(v - 10.0 * (1 - np.exp(-t / 4e-3)))), 1e-4, "V")


@prueba(1, "DAE: valores iniciales incompatibles", "Conservación de la carga: v = (C1·v1 + C2·v2)/(C1 + C2)")
def dae_proyeccion_carga(m):
    """Dos condensadores en paralelo con 10 V y 0 V: al unirse comparten la carga y se avisa."""
    d = _rc_paralelo(v0a=10.0, v0b=0.0)
    d["sistema"]["bloques"][0]["params"]["V"] = 2.5
    r = simular(d)
    v = r["senales"][0]["trazas"][0]
    m.error("v(0) conserva la carga", float(v[0]), 2.5, 1e-9, "V")
    m.cota("Con V = 2.5 V queda en reposo", float(np.max(np.abs(v - 2.5))), 1e-6, "V")
    m.verdad("Aviso de valores incompatibles", any("incompatibles" in a for a in r["avisos"]))


def _ejes(n_engranaje=None, w1=0.0, w2=0.0, metodo="dae"):
    """Par de 2 N·m sobre J1 = 0.1; J2 = 0.4 unida directamente o por un engranaje de relación n."""
    bloques = [bloque("T", "constante", c=2.0), bloque("fp", "fuente_par"),
               bloque("J1", "inercia", J=0.1, w0=w1), bloque("J2", "inercia", J=0.4, w0=w2),
               bloque("o", "osciloscopio", n=2)]
    con = [hilo("T.y", "fp.T"), hilo("fp.p", "J1.p"), hilo("J1.w", "o.u1"), hilo("J2.w", "o.u2")]
    if n_engranaje is None:
        con.append(hilo("J1.p", "J2.p"))
    else:
        bloques.append(bloque("g", "engranaje", n=n_engranaje))
        con += [hilo("J1.p", "g.p1"), hilo("g.p2", "J2.p")]
    return diagrama("ejes", bloques, con, metodo=metodo, t_fin=1.0, rtol=1e-8, atol=1e-11)


@prueba(1, "DAE: inercias unidas rígidamente", "ω1' = T/(J1 + J2/n²);  ω2 = ω1/n")
def dae_inercias(m):
    """Dos inercias en el mismo eje o por un engranaje: el DAE no pide sumarlas a mano."""
    error = _falla(_ejes(metodo="dopri"))
    m.verdad("Motor ODE: aviso de inercias en paralelo", error is not None and "inercias" in error)
    s = simular(_ejes())["senales"][0]
    t = s["t"]
    m.cota("Mismo eje: ω = T·t/(J1+J2)", np.max(np.abs(s["trazas"][0] - 2.0 * t / 0.5)), 1e-5, "rad/s")
    s = simular(_ejes(n_engranaje=2.0))["senales"][0]
    t = s["t"]
    j_eq = 0.1 + 0.4 / 4.0
    m.cota("Engranaje 2:1: ω1 = T·t/(J1 + J2/n²)", np.max(np.abs(s["trazas"][0] - 2.0 * t / j_eq)), 1e-5, "rad/s")
    m.cota("Engranaje 2:1: ω2 = ω1/n", np.max(np.abs(s["trazas"][1] - s["trazas"][0] / 2.0)), 1e-9, "rad/s")


@prueba(1, "DAE: momento angular con engranaje", "ω2(0) = (J1·n·ω1 + J2·ω2)/(J1·n² + J2)")
def dae_proyeccion_momento(m):
    """Velocidades iniciales incompatibles con el engranaje: se igualan conservando el momento referido."""
    r = simular(_ejes(n_engranaje=2.0, w1=10.0, w2=0.0))
    s = r["senales"][0]
    w2 = (0.1 * 2.0 * 10.0 + 0.4 * 0.0) / (0.1 * 4.0 + 0.4)
    m.error("ω2(0) conserva el momento", float(s["trazas"][1][0]), w2, 1e-9, "rad/s")
    m.error("ω1(0) = n·ω2(0)", float(s["trazas"][0][0]), 2.0 * w2, 1e-9, "rad/s")
    m.verdad("Aviso de valores incompatibles", any("incompatibles" in a for a in r["avisos"]))


def _acuerdo(d, tf):
    """Máxima diferencia relativa entre el DAE y Dormand-Prince ajustado, fuera de los saltos."""
    d = copy.deepcopy(d)
    d["solver"]["t_fin"] = tf
    a = simular(copy.deepcopy(d), config={"metodo": "dopri", "rtol": 1e-8, "atol": 1e-10})
    b = simular(copy.deepcopy(d), config={"metodo": "dae", "rtol": 1e-6, "atol": 1e-9})
    peor = 0.0
    for sa, sb in zip(a["senales"], b["senales"]):
        for ya, yb in zip(sa["trazas"], sb["trazas"]):
            ya = np.asarray(ya, dtype=float)
            if ya.ndim != 1 or ya.size == 0:
                continue
            ta = np.asarray(sa["t"], dtype=float)
            yi = np.interp(ta, sb["t"], np.asarray(yb, dtype=float))
            rango = max(float(np.ptp(ya)), float(np.max(np.abs(ya))), 1e-12)
            peor = max(peor, float(np.max(np.abs(yi - ya))) / rango)
    return peor


@prueba(1, "DAE: acuerdo con Dormand-Prince", "Mismos ejemplos, dos formulaciones: diferencia relativa < 1e-3")
def dae_acuerdo(m):
    """El RC y el arranque del motor de inducción dan lo mismo con la red reducida a EDO y con el DAE."""
    from . import ejemplos
    m.cota("RC: respuesta al escalón", _acuerdo(ejemplos.rc_serie(), 0.005), 1e-3)
    m.cota("Motor de inducción: primeros 0.3 s del arranque", _acuerdo(ejemplos.arranque_induccion(), 0.3), 1e-3)
