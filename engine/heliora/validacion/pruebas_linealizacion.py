"""Linealización: el modelo lineal del diagrama frente a la solución analítica y frente a la
respuesta pequeña-señal que se obtiene simulando el diagrama no lineal."""
import copy
import math

import numpy as np

from .banco import prueba
from ..sim.diagrama import cargar_librerias, simular
from ..sim.linealizar import funcion_transferencia, linealizar


def _respuesta(lin, w):
    """G(jω) = C·(jωI - A)⁻¹·B + D, de la primera entrada a la primera salida."""
    A, B, C, D = (np.asarray(lin[k], dtype=float) for k in "ABCD")
    n = A.shape[0]
    return complex((C @ np.linalg.solve(1j * w * np.eye(n) - A, B) + D)[0, 0])


@prueba(2, "Linealización del lazo de control", "T(s) analítica y respuesta pequeña-señal simulada")
def lazo(m):
    """El ejemplo «lazo»: PI (Kp = 2, Ki = 1) sobre 1/(s+1)² con realimentación unitaria.

    Analítica: T(s) = (2s + 1)/(s³ + 2s² + 3s + 1). Pequeña señal: la referencia pasa a ser
    1 + 0.01·sin(ωt); en régimen, la salida oscila con ganancia |T(jω)| y desfase ∠T(jω), que se
    miden por mínimos cuadrados sobre los cuatro últimos periodos.
    """
    from .ejemplos import obtener
    cargar_librerias()
    d = obtener("lazo")
    lin = linealizar(d)
    num, den = funcion_transferencia(lin)
    m.error("Numerador: 2s + 1 (coef. de s)", num[0], 2.0, 1e-6)
    m.error("Numerador: término independiente", num[-1], 1.0, 1e-6)
    for i, ref in enumerate([1.0, 2.0, 3.0, 1.0]):
        m.error(f"Denominador: coeficiente de s^{3 - i}", den[i], ref, 1e-6)
    T = lambda w: (2j * w + 1) / ((1j * w) ** 3 + 2 * (1j * w) ** 2 + 3j * w + 1)
    for w in (0.1, 1.0, 10.0):
        m.error(f"|G(jω)| frente a la analítica, ω = {w:g} rad/s", abs(_respuesta(lin, w)), abs(T(w)), 1e-6, relativo=True)
    m.cota("Residuo de f en el punto de operación", lin["punto"]["residuo_f"], 1e-8)
    # Pequeña señal simulada sobre el diagrama no lineal
    w = 1.0
    d2 = copy.deepcopy(d)
    for b in d2["sistema"]["bloques"]:
        if b["id"] == "r":
            b["tipo"] = "senoidal"
            b["params"] = {"A": 0.01, "f": w / (2 * math.pi), "fase": 0.0, "c": 1.0}
    d2["solver"] = {"metodo": "dopri", "t_fin": 80.0, "rtol": 1e-8, "atol": 1e-10, "paso_max": 0.02}
    r = simular(d2)
    s = next(x for x in r["senales"] if x["bloque"] == "o")
    t, y = np.asarray(s["t"]), np.asarray(s["trazas"][1])
    sel = t >= t[-1] - 4 * 2 * math.pi / w
    M = np.column_stack([np.sin(w * t[sel]), np.cos(w * t[sel]), np.ones(sel.sum())])
    a, b, c = np.linalg.lstsq(M, y[sel], rcond=None)[0]
    ganancia = math.hypot(a, b) / 0.01
    fase = math.degrees(math.atan2(b, a))
    G = _respuesta(lin, w)
    m.error("Ganancia pequeña-señal simulada frente a la linealizada (ω = 1)", ganancia, abs(G), 1e-3, relativo=True)
    m.error("Desfase simulado frente al linealizado (ω = 1)", fase, math.degrees(np.angle(G)), 0.2, "°")
    m.error("Nivel medio de la salida frente al punto de operación", c, lin["punto"]["y"][0], 1e-3)
