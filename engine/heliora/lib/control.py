"""Análisis de control lineal: Bode, márgenes, respuesta al escalón, lugar de las raíces y ajuste de un PI.

Una planta es una función de transferencia G(s) = num(s)/den(s), con los coeficientes en
potencias decrecientes de s:  [1, 3, 3, 1]  es  s³ + 3s² + 3s + 1.

Márgenes (de la ganancia de lazo L(jω)):
    cruce de ganancia ω_g:  |L(jω_g)| = 1          margen de fase  MF = 180° + ∠L(jω_g)
    cruce de fase ω_f:      ∠L(jω_f) = -180°        margen de ganancia  MG = 1/|L(jω_f)|
Cada cruce se busca en una rejilla logarítmica y se afina con Brent sobre la función exacta.

Respuesta al escalón (lazo estable, valor final y∞ = G(0)):
    sobreimpulso  = (y_max - y∞)/|y∞|
    t_subida      = de 10 % a 90 % de y∞
    t_establec.   = último instante con |y - y∞| > 2 %·|y∞|
    t_pico        = instante de y_max

PI  C(s) = Kp·(1 + 1/(Ti·s)), ajustado en frecuencia para un margen de fase MF a una frecuencia
de cruce ω_c:
    ∠C(jω_c) = atan(ω_c·Ti) - 90°  y se pide  ∠P(jω_c) + ∠C(jω_c) = MF - 180°
    ⇒  Ti = tan(MF - 90° - ∠P(jω_c)) / ω_c,   Kp = 1 / (|P(jω_c)|·√(1 + 1/(ω_c·Ti)²))
"""
from __future__ import annotations

import math

import numpy as np
from scipy import optimize, signal

PUNTOS_REJILLA = 4000
TOL_ESTABLECIMIENTO = 0.02


class ErrorControl(ValueError):
    """La planta o los objetivos no permiten el cálculo pedido."""


def coeficientes(v):
    """Lista de coeficientes desde texto ('1, 3, 3, 1' o '[1 3 3 1]'), lista o número."""
    if isinstance(v, str):
        t = v.replace("[", " ").replace("]", " ").replace(";", " ").replace(",", " ")
        v = [float(eval(x, {"__builtins__": {}}, {"pi": math.pi, "sqrt": math.sqrt})) for x in t.split()]
    a = np.atleast_1d(np.asarray(v, dtype=float))
    a = np.trim_zeros(a, "f")
    if a.size == 0:
        raise ErrorControl("Un polinomio no puede ser cero")
    return a


def respuesta_frecuencia(num, den, w):
    s = 1j * np.asarray(w, dtype=float)
    return np.polyval(num, s) / np.polyval(den, s)


def rejilla(num, den, w_min=None, w_max=None, n=PUNTOS_REJILLA):
    """Rejilla de frecuencias que cubre dos décadas a cada lado de los polos y ceros."""
    raices = np.concatenate([np.roots(num), np.roots(den)])
    mags = np.abs(raices[np.abs(raices) > 1e-12])
    lo = mags.min() if mags.size else 1.0
    hi = mags.max() if mags.size else 1.0
    w_min = w_min or lo / 100.0
    w_max = w_max or hi * 100.0
    return np.logspace(math.log10(w_min), math.log10(w_max), n)


def bode(num, den, w=None):
    w = rejilla(num, den) if w is None else np.asarray(w, dtype=float)
    H = respuesta_frecuencia(num, den, w)
    mag_db = 20.0 * np.log10(np.maximum(np.abs(H), 1e-300))
    fase = np.degrees(np.unwrap(np.angle(H)))
    # La fase se lleva a la rama que empieza cerca de la de baja frecuencia en (-360, 0]
    if fase[0] > 0:
        fase = fase - 360.0 * math.ceil(fase[0] / 360.0)
    elif fase[0] <= -360.0:
        fase = fase + 360.0 * math.floor(-fase[0] / 360.0)
    return w, mag_db, fase


def margenes(num, den):
    """Márgenes de ganancia y de fase con sus frecuencias. inf si no hay cruce."""
    w, mag, fase = bode(num, den)
    ab = lambda x: abs(complex(respuesta_frecuencia(num, den, [x])[0]))

    def fase_continua(x):
        # fase en x siguiendo la rama de la rejilla
        i = int(np.clip(np.searchsorted(w, x), 1, len(w) - 1))
        f = math.degrees(np.angle(respuesta_frecuencia(num, den, [x])[0]))
        ref = fase[i]
        return f + 360.0 * round((ref - f) / 360.0)

    res = {"mg": math.inf, "w_mg": math.nan, "mf": math.inf, "w_mf": math.nan, "cruces_g": [], "cruces_f": []}
    # Cruce de ganancia
    for i in np.nonzero(np.diff(np.sign(mag)) != 0)[0]:
        x = optimize.brentq(lambda q: 20 * math.log10(ab(q)), w[i], w[i + 1], xtol=1e-14, rtol=1e-14)
        mf = 180.0 + fase_continua(x)
        res["cruces_g"].append((x, mf))
    # Cruce de fase por -180 + k·360
    for i in range(len(w) - 1):
        for k in range(-3, 2):
            o = -180.0 + 360.0 * k
            a, b = fase[i] - o, fase[i + 1] - o
            if a == 0 or a * b < 0:
                x = optimize.brentq(lambda q: fase_continua(q) - o, w[i], w[i + 1], xtol=1e-14, rtol=1e-14)
                res["cruces_f"].append((x, 1.0 / ab(x)))
    if res["cruces_g"]:
        x, mf = min(res["cruces_g"], key=lambda c: abs(c[1]))
        res["mf"], res["w_mf"] = mf, x
    if res["cruces_f"]:
        x, mg = min(res["cruces_f"], key=lambda c: abs(math.log(c[1])))
        res["mg"], res["w_mg"] = mg, x
    res["mg_db"] = 20 * math.log10(res["mg"]) if math.isfinite(res["mg"]) else math.inf
    return res


def lazo_cerrado(num, den):
    """T = L/(1 + L) con realimentación unitaria."""
    return num, np.polyadd(den, num)


def estable(den):
    return bool(np.all(np.real(np.roots(den)) < 0))


def escalon(num, den, t_fin=None, n=20000):
    """Respuesta al escalón unitario y sus métricas."""
    polos = np.roots(den)
    if not estable(den):
        raise ErrorControl("El sistema no es estable: la respuesta al escalón crece sin límite. Polos: " +
                           ", ".join(f"{p:.4g}" for p in polos))
    if t_fin is None:
        lento = np.min(np.abs(np.real(polos))) if polos.size else 1.0
        t_fin = 10.0 / max(lento, 1e-9)
    t = np.linspace(0.0, t_fin, n)
    t, y = signal.step(signal.lti(num, den), T=t)
    yf = float(np.polyval(num, 0.0) / np.polyval(den, 0.0))
    return t, y, metricas_escalon(t, y, yf)


def metricas_escalon(t, y, yf):
    t, y = np.asarray(t), np.asarray(y)
    m = {"final": yf}
    if abs(yf) < 1e-15:
        m.update(sobreimpulso=math.nan, t_subida=math.nan, t_establecimiento=math.nan, t_pico=math.nan, pico=float(np.max(y)))
        return m
    s = np.sign(yf)
    ip = int(np.argmax(s * y))
    m["pico"] = float(y[ip])
    m["t_pico"] = float(t[ip])
    m["sobreimpulso"] = max(0.0, (s * y[ip] - abs(yf)) / abs(yf))

    def cruza(nivel):
        i = int(np.argmax(s * y >= nivel * abs(yf)))
        if i == 0:
            return float(t[0])
        return float(t[i - 1] + (t[i] - t[i - 1]) * (nivel * abs(yf) - s * y[i - 1]) / (s * y[i] - s * y[i - 1]))

    m["t_subida"] = cruza(0.9) - cruza(0.1)
    fuera = np.nonzero(np.abs(y - yf) > TOL_ESTABLECIMIENTO * abs(yf))[0]
    if fuera.size == 0:
        m["t_establecimiento"] = 0.0
    elif fuera[-1] + 1 >= len(t):
        m["t_establecimiento"] = math.inf
    else:
        i = fuera[-1]
        # Interpolación del instante en que entra en la banda
        e0, e1 = abs(y[i] - yf), abs(y[i + 1] - yf)
        b = TOL_ESTABLECIMIENTO * abs(yf)
        m["t_establecimiento"] = float(t[i] + (t[i + 1] - t[i]) * (e0 - b) / max(e0 - e1, 1e-300))
    return m


def lugar_raices(num, den, ganancias=None):
    """Polos de 1 + k·G(s) para cada k, ordenados para que cada rama sea continua."""
    if ganancias is None:
        ganancias = np.concatenate([[0.0], np.logspace(-3, 3, 600)])
    n = len(den) - 1
    ramas = np.empty((len(ganancias), n), dtype=complex)
    previo = None
    for i, k in enumerate(ganancias):
        r = np.roots(np.polyadd(den, k * np.asarray(num)))
        if r.size < n:
            r = np.concatenate([r, np.full(n - r.size, np.nan + 0j)])
        if previo is not None:
            # Se empareja cada raíz con la rama más cercana
            orden, libres = [], list(range(n))
            for p in previo:
                j = min(libres, key=lambda q: abs(r[q] - p) if np.isfinite(r[q]) else math.inf)
                orden.append(j)
                libres.remove(j)
            r = r[orden]
        ramas[i] = r
        previo = r
    return np.asarray(ganancias), ramas


def ajustar_pi(num, den, mf=60.0, wc=None):
    """Kp y Ti para un margen de fase mf [°] a la frecuencia de cruce wc [rad/s].

    Sin wc, se usa la mayor frecuencia en la que el integrador quita como mucho 30° (la fase
    de la planta es al menos mf - 150°), o 10 veces el polo más rápido si la planta nunca cae tanto.
    """
    if not 0.0 < mf < 180.0:
        raise ErrorControl("El margen de fase debe estar entre 0° y 180°")
    w, _, fase = bode(num, den)
    if wc is None or wc <= 0:
        validos = np.nonzero(fase >= mf - 150.0)[0]
        if validos.size == 0:
            raise ErrorControl(f"La planta ya tiene menos de {mf - 150:.0f}° de fase a baja frecuencia: no se llega a {mf:.0f}° de margen con un PI.")
        if validos[-1] == len(w) - 1:
            polos = np.abs(np.roots(den))
            wc = 10.0 * (polos.max() if polos.size and polos.max() > 0 else 1.0)
        else:
            wc = float(w[validos[-1]])
    P = complex(respuesta_frecuencia(num, den, [wc])[0])
    fase_p = math.degrees(math.atan2(P.imag, P.real))
    # Rama continua de la fase de la planta en wc
    ref = float(np.interp(math.log10(wc), np.log10(w), fase))
    fase_p += 360.0 * round((ref - fase_p) / 360.0)
    adelanto = mf - 90.0 - fase_p      # = atan(wc·Ti) en grados
    if not 0.0 < adelanto < 90.0:
        raise ErrorControl(f"Con ω_c = {wc:.4g} rad/s un PI no da {mf:.0f}° de margen (haría falta {adelanto:.1f}° de adelanto). Prueba otra ω_c.")
    Ti = math.tan(math.radians(adelanto)) / wc
    Kp = 1.0 / (abs(P) * math.sqrt(1.0 + 1.0 / (wc * Ti) ** 2))
    numc = np.array([Kp * Ti, Kp])
    denc = np.array([Ti, 0.0])
    L_num, L_den = np.polymul(numc, num), np.polymul(denc, den)
    return {"Kp": Kp, "Ti": Ti, "Ki": Kp / Ti, "wc": wc, "L": (L_num, L_den)}
