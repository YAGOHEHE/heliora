"""Fase 1: entorno de cálculo y solvers contra soluciones analíticas."""
from __future__ import annotations

import numpy as np

from .banco import prueba
from ..sim.diagrama import ErrorDiagrama, bloque, diagrama, hilo, simular
from ..sim.solvers import integrar


@prueba(1, "Orden de convergencia de Euler y RK4", "dx/dt = -x  →  x(t) = e^(-t)")
def orden_fijo(m):
    """Al dividir el paso por 2, el error debe caer 2 veces (Euler) y 16 veces (RK4)."""
    def err(metodo, h):
        t, x = integrar(lambda t, x: -x, (0, 5), [1.0], metodo=metodo, paso=h)
        return abs(x[-1, 0] - np.exp(-5.0))

    e1, e2 = err("euler", 0.02), err("euler", 0.01)
    m.error("Euler: orden observado", np.log2(e1 / e2), 1.0, 0.05)
    # Término principal del error de Euler: h·t/2·e^(-t) = 1.684e-4
    m.error("Euler: error en t=5 frente al teórico h·t/2·e^-t", e2, 0.01 * 5 / 2 * np.exp(-5.0), 2e-6)
    r1, r2 = err("rk4", 0.2), err("rk4", 0.1)
    m.error("RK4: orden observado", np.log2(r1 / r2), 4.0, 0.15)
    m.cota("RK4: error en t=5 (h=0.1)", r2, 1e-6)


@prueba(1, "Dormand-Prince con tolerancias", "Oscilador x'' + x = 0  →  x(t) = cos(t)")
def dopri(m):
    """El error global debe seguir a la tolerancia pedida."""
    f = lambda t, x: np.array([x[1], -x[0]])
    for rtol, cota in ((1e-3, 5e-2), (1e-6, 1e-4), (1e-9, 3e-7)):
        t, x = integrar(f, (0, 20), [1.0, 0.0], metodo="dopri", rtol=rtol, atol=rtol * 1e-3)
        e = np.max(np.abs(x[:, 0] - np.cos(t)))
        m.cota(f"Error máximo con rtol={rtol:g} ({t.size} pasos)", e, cota)


@prueba(1, "BDF en sistema rígido", "x' = -1e5·(x - cos t) - sin t  →  x(t) = cos t")
def bdf(m):
    """BDF debe resolver el rígido con muchos menos pasos que un método explícito."""
    f = lambda t, x: -1e5 * (x - np.cos(t)) - np.sin(t)
    t, x = integrar(f, (0, 2), [1.0], metodo="bdf", rtol=1e-6, atol=1e-8)
    e = np.max(np.abs(x[:, 0] - np.cos(t)))
    m.cota("BDF: error máximo", e, 1e-3)
    m.verdad(f"BDF: usa pocos pasos ({t.size})", t.size < 1000)
    t2, x2 = integrar(f, (0, 2e-3), [1.0], metodo="dopri", rtol=1e-6, atol=1e-8)
    ritmo_dp = t2.size / 2e-3
    ritmo_bdf = t.size / 2.0
    m.verdad(f"Dormand-Prince necesita >100x más pasos por segundo ({ritmo_dp:.0f} vs {ritmo_bdf:.0f})",
             ritmo_dp > 100 * ritmo_bdf)


def _rc_diagrama(metodo, **kw):
    # dy/dt = (u - y)/τ con función de transferencia 1/(τ s + 1)
    return diagrama("rc", [
        bloque("e", "escalon", t0=0.0, y0=0.0, y1=1.0),
        bloque("g", "funcion_transferencia", num=[1.0], den=[1e-3, 1.0]),
        bloque("o", "osciloscopio", n=1),
    ], [hilo("e.y", "g.u"), hilo("g.y", "o.u1")], metodo=metodo, t_fin=5e-3, **kw)


@prueba(1, "Bloques: respuesta al escalón de primer orden", "y(t) = 1 - e^(-t/τ)")
def escalon(m):
    """El mismo diagrama con los cuatro solvers debe seguir la solución exacta."""
    casos = [("euler", dict(paso=1e-6), 3e-3), ("rk4", dict(paso=1e-5), 1e-5),
             ("dopri", dict(rtol=1e-6, atol=1e-8), 1e-4), ("bdf", dict(rtol=1e-6, atol=1e-8), 2e-3)]
    for metodo, kw, tol in casos:
        r = simular(_rc_diagrama(metodo, **kw))
        s = r["senales"][0]
        # El escalón en t=0 actúa tras el paso inicial, en la primera muestra
        e = np.max(np.abs(s["trazas"][0] - (1 - np.exp(-s["t"] / 1e-3))))
        m.cota(f"{metodo}: error máximo ({r['estadisticas']['pasos']} pasos)", e, tol)


@prueba(1, "Bloques genéricos", "Valores conocidos de cada bloque")
def genericos(m):
    """Integrador de rampa, senoidal, saturación, switch, lookup, retardo, derivador y espacio de estados."""
    d = diagrama("g", [
        bloque("r", "rampa", m=2.0),
        bloque("i", "integrador"),
        bloque("sat", "saturacion", inf=-1, sup=1),
        bloque("tab", "lookup", xs="[0, 1, 2]", ys="[0, 10, 40]"),
        bloque("ret", "retardo", tau=0.5),
        bloque("der", "derivador"),
        bloque("ss", "espacio_estados", A="[[0, 1], [-1, 0]]", B="[[0], [1]]", C="[[1, 0]]", D="[[0]]"),
        bloque("c", "constante", c=1.0),
        bloque("suma", "suma", signos="+-"),
        bloque("gan", "ganancia", k=3.0),
        bloque("sw", "switch", umbral=1.0),
        bloque("o", "osciloscopio", n=8),
    ], [
        hilo("r.y", "i.u"), hilo("r.y", "sat.u"), hilo("r.y", "tab.u"), hilo("r.y", "ret.u"), hilo("r.y", "der.u"),
        hilo("c.y", "ss.u"), hilo("r.y", "suma.u1"), hilo("c.y", "suma.u2"), hilo("r.y", "gan.u"),
        hilo("c.y", "sw.u1"), hilo("r.y", "sw.control"), hilo("gan.y", "sw.u3"),
        hilo("i.y", "o.u1"), hilo("sat.y", "o.u2"), hilo("tab.y", "o.u3"), hilo("ret.y", "o.u4"), hilo("der.y", "o.u5"),
        hilo("ss.y", "o.u6"), hilo("suma.y", "o.u7"), hilo("sw.y", "o.u8"),
    ], metodo="rk4", t_fin=2.0, paso=1e-3)
    r = simular(d)
    s = r["senales"][0]
    t, y = s["t"], s["trazas"]
    m.error("Integrador: ∫2t dt en t=2", y[0][-1], 4.0, 1e-9)
    m.error("Saturación en t=2", y[1][-1], 1.0, 1e-12)
    m.error("Lookup (entrada 1.5 → 25)", np.interp(0.75, t, y[2]), 25.0, 1e-6)
    m.error("Retardo τ=0.5: u(1.5)=3", np.interp(2.0, t, y[3]), 3.0, 1e-6)
    m.error("Derivador de rampa", y[4][-1], 2.0, 1e-6)
    m.error("Espacio de estados: 1-cos(2)", y[5][-1], 1 - np.cos(2.0), 1e-9)
    m.error("Suma: 2t-1 en t=2", y[6][-1], 3.0, 1e-9)
    m.error("Switch en t=2 (control=4≥1 → u1=1)", y[7][-1], 1.0, 1e-12)


@prueba(1, "Subsistemas anidados", "Resultado igual al diagrama plano")
def subsistemas(m):
    """Dos niveles de subsistema deben dar la misma señal que los bloques sueltos."""
    interior = {"bloques": [bloque("in", "entrada", numero=1), bloque("g", "ganancia", k=2.0),
                            bloque("out", "salida", numero=1)],
                "conexiones": [hilo("in.y", "g.u"), hilo("g.y", "out.u")]}
    medio = {"bloques": [bloque("in", "entrada", numero=1), bloque("s", "subsistema", sistema=interior),
                         bloque("i", "integrador"), bloque("out", "salida", numero=1)],
             "conexiones": [hilo("in.y", "s.in1"), hilo("s.out1", "i.u"), hilo("i.y", "out.u")]}
    d = diagrama("sub", [bloque("c", "constante", c=1.5), bloque("n", "subsistema", sistema=medio),
                         bloque("o", "osciloscopio", n=1)],
                 [hilo("c.y", "n.in1"), hilo("n.out1", "o.u1")], metodo="rk4", t_fin=1.0, paso=0.01)
    r = simular(d)
    m.error("Salida del subsistema (2·1.5·t en t=1)", r["senales"][0]["trazas"][0][-1], 3.0, 1e-10)


@prueba(1, "Lazos algebraicos", "Se resuelven; si no hay solución, un mensaje claro")
def lazo(m):
    """y = 0.5·(1 - y) tiene solución exacta 1/3. y = 1 + y no tiene solución y debe avisar."""
    d = diagrama("lazo", [bloque("c", "constante", c=1.0), bloque("s", "suma", signos="+-"),
                          bloque("g", "ganancia", k=0.5), bloque("o", "osciloscopio", n=1)],
                 [hilo("c.y", "s.u1"), hilo("g.y", "s.u2"), hilo("s.y", "g.u"), hilo("g.y", "o.u1")],
                 metodo="rk4", t_fin=0.2, paso=0.1)
    r = simular(d)
    m.error("y = 0.5·(1 - y) resuelto", r["senales"][0]["trazas"][0][-1], 1.0 / 3.0, 1e-9)
    m.verdad("Avisa de que hay un lazo resuelto", any("Lazo algebraico" in a for a in r["avisos"]))
    # No lineal: y = cos(y), punto fijo de Dottie 0.739085133215...
    d = diagrama("lazo3", [bloque("f", "funcion_python", codigo="y = math.cos(u[0])", n=1), bloque("o", "osciloscopio", n=1)],
                 [hilo("f.y", "f.u1"), hilo("f.y", "o.u1")], metodo="rk4", t_fin=0.2, paso=0.1)
    r = simular(d)
    m.error("y = cos(y) resuelto", r["senales"][0]["trazas"][0][-1], 0.7390851332151607, 1e-9)
    d = diagrama("lazo_sin", [bloque("c", "constante", c=1.0), bloque("s", "suma", signos="++")],
                 [hilo("c.y", "s.u1"), hilo("s.y", "s.u2")], metodo="rk4", t_fin=1, paso=0.1)
    try:
        simular(d)
        m.verdad("y = 1 + y se rechaza", False)
    except ErrorDiagrama as e:
        m.verdad("y = 1 + y se rechaza con mensaje claro", "lazo algebraico" in str(e))
    # Con memoria, el mismo lazo ya funciona
    d = diagrama("lazo2", [bloque("c", "constante", c=1.0), bloque("s", "suma", signos="+-"),
                           bloque("mem", "memoria"), bloque("o", "osciloscopio", n=1)],
                 [hilo("c.y", "s.u1"), hilo("mem.y", "s.u2"), hilo("s.y", "mem.u"), hilo("s.y", "o.u1")],
                 metodo="rk4", t_fin=0.5, paso=0.1)
    r = simular(d)
    m.verdad("Con Memoria el lazo se ejecuta", r["senales"][0]["t"].size > 0)


@prueba(1, "Cálculo matricial, FFT, interpolación y optimización", "Resultados exactos conocidos")
def calculo(m):
    """Las operaciones del entorno de cálculo frente a resultados exactos."""
    from scipy import interpolate, linalg, optimize

    rng = np.random.default_rng(0)
    A = rng.normal(size=(50, 50)) + 50 * np.eye(50)
    b = rng.normal(size=50)
    x = np.linalg.solve(A, b)
    m.cota("Residuo de A·x = b", np.max(np.abs(A @ x - b)), 1e-12)
    w = np.linalg.eigvals(np.array([[2.0, 0.0], [0.0, 3.0]]))
    m.error("Autovalor mayor", np.max(w.real), 3.0, 1e-12)
    P, L, U = linalg.lu(A)
    m.cota("Descomposición LU: |PLU - A|", np.max(np.abs(P @ L @ U - A)), 1e-12)
    n, fs = 1024, 1024.0
    t = np.arange(n) / fs
    s = 3.0 * np.sin(2 * np.pi * 50 * t)
    F = np.fft.rfft(s) * 2 / n
    m.error("FFT: amplitud a 50 Hz", abs(F[50]), 3.0, 1e-10)
    xs = np.linspace(0, np.pi, 40)
    cs = interpolate.CubicSpline(xs, np.sin(xs))
    m.cota("Interpolación cúbica de sin(x)", np.max(np.abs(cs(np.linspace(0, np.pi, 500)) - np.sin(np.linspace(0, np.pi, 500)))), 2e-5)
    o = optimize.minimize(optimize.rosen, [-1.2, 1.0], method="BFGS")
    m.cota("Optimización: mínimo de Rosenbrock", np.max(np.abs(o.x - 1.0)), 1e-4)
