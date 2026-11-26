"""Solvers de ecuaciones diferenciales dx/dt = f(t, x).

Paso fijo:
    euler   x[n+1] = x[n] + h·f(t[n], x[n])                                  (orden 1)
    rk4     x[n+1] = x[n] + h/6·(k1 + 2·k2 + 2·k3 + k4)                      (orden 4)

Paso variable:
    dopri   Dormand-Prince 5(4), con error local e = h·Σ Ei·ki.
            err = ||e / max(|x|, |x_nuevo|, atol/rtol)||inf;  se rechaza si err > rtol.
            Primer rechazo: h' = h·max(0.1, 0.8·(rtol/err)^(1/5)).  Siguientes: h' = h/2.
            Paso aceptado sin rechazos: h' = h / max(1.25·(err/rtol)^(1/5), 0.2).
            Tras un rechazo, el paso no crece en ese intento.
            Si faltan menos de 1.1·h para el siguiente tope, el paso se estira hasta él.
    bdf     Fórmulas de diferencias hacia atrás de orden 1 a 5 (rígidos), por medio
            de scipy.integrate.BDF, que resuelve cada paso con Newton.

Por defecto: rtol = 1e-3, atol = 1e-6 y paso máximo = 1/10 del intervalo.

Cruces por cero (solo con paso variable): si un bloque declara una señal g(t) y esta cambia de
signo en un paso, se localiza el instante con `localizar_cruce` y el paso termina justo después.

Dos formas de uso:
    integrar(f, (t0, tf), x0, metodo=...)  para una función suelta (consola).
    integrar_modelo(modelo, ...)           para un diagrama de bloques.
"""
from __future__ import annotations

import math

import numpy as np

RTOL_DEFECTO = 1e-3
ATOL_DEFECTO = 1e-6
FRACCION_PASO_MAX = 0.1
SEGURIDAD = 0.8
REDUCCION_MIN = 0.1
CRECIMIENTO_MAX = 5.0
POTENCIA = 1.0 / 5.0
ESTIRAR = 1.1

# Cruces por cero
TOL_SENAL = 2.842170943040401e-14   # 2^-45: diferencia de señal por debajo de la cual se usa el punto medio
FACTOR_TOL_TIEMPO = 128           # ancho final del intervalo: 128·eps(t)
MAX_ITER_ACOTACION = 100000
MAX_EVENTOS_CONSECUTIVOS = 1000
TOL_EVENTOS_CONSECUTIVOS = 1280   # dos eventos son consecutivos si distan menos de 1280·eps(t)

# Tabla de Butcher de Dormand-Prince (publicación original de 1980).
_C = (1 / 5, 3 / 10, 4 / 5, 8 / 9, 1.0)
_A = (
    (1 / 5,),
    (3 / 40, 9 / 40),
    (44 / 45, -56 / 15, 32 / 9),
    (19372 / 6561, -25360 / 2187, 64448 / 6561, -212 / 729),
    (9017 / 3168, -355 / 33, 46732 / 5247, 49 / 176, -5103 / 18656),
    (35 / 384, 0.0, 500 / 1113, 125 / 192, -2187 / 6784, 11 / 84),
)
_E = (71 / 57600, 0.0, -71 / 16695, 71 / 1920, -17253 / 339200, 22 / 525, -1 / 40)

# Interpolante continuo de cuarto orden: x(t + s·h) = x + h·Σ k_i·(BI[i][0]·s + ... + BI[i][3]·s⁴)
_BI = (
    (1.0, -183 / 64, 37 / 12, -145 / 128),
    (0.0, 0.0, 0.0, 0.0),
    (0.0, 1500 / 371, -1000 / 159, 1000 / 371),
    (0.0, -125 / 32, 125 / 12, -375 / 64),
    (0.0, 9477 / 3392, -729 / 106, 25515 / 6784),
    (0.0, -11 / 7, 11 / 3, -55 / 28),
    (0.0, 3 / 2, -4.0, 5 / 2),
)


class Cancelado(Exception):
    """El usuario detuvo la simulación."""


class ErrorSolver(RuntimeError):
    """El solver no pudo seguir (tolerancia imposible o resultado no finito)."""


def paso_euler(f, t, x, h):
    """Avanza un paso h de Euler explícito y devuelve x(t + h)."""
    return x + h * f(t, x)


def paso_rk4(f, t, x, h):
    """Avanza un paso h de Runge-Kutta clásico de orden 4 y devuelve x(t + h)."""
    k1 = f(t, x)
    k2 = f(t + 0.5 * h, x + 0.5 * h * k1)
    k3 = f(t + 0.5 * h, x + 0.5 * h * k2)
    k4 = f(t + h, x + h * k3)
    return x + (h / 6.0) * (k1 + 2.0 * k2 + 2.0 * k3 + k4)


def paso_dopri(f, t, x, h, k1):
    """Un intento de Dormand-Prince. Devuelve (x_nuevo, lista de k, error_local)."""
    k = [k1]
    for ci, ai in zip(_C, _A[:5]):
        xs = x + h * sum(a * kj for a, kj in zip(ai, k))
        k.append(f(t + ci * h, xs))
    xn = x + h * sum(a * kj for a, kj in zip(_A[5], k))
    k.append(f(t + h, xn))
    e = sum(ei * kj for ei, kj in zip(_E, k) if ei != 0.0)
    return xn, k, h * e


def interpolar_dopri(t, x, h, k, ti):
    """Estado en ti, dentro de un paso aceptado de Dormand-Prince."""
    s = (ti - t) / h
    pot = (s, s * s, s * s * s, s * s * s * s)
    suma = 0.0
    for ki, bi in zip(k, _BI):
        c = bi[0] * pot[0] + bi[1] * pot[1] + bi[2] * pot[2] + bi[3] * pot[3]
        if c != 0.0:
            suma = suma + c * ki
    return x + h * suma


def _tol_tiempo(t):
    """128·eps(t): ancho con el que se da por localizado un cruce."""
    return max(FACTOR_TOL_TIEMPO * np.spacing(abs(t)), 2.0**-97)


def _cruza(a, b):
    return (a > 0.0 and b <= 0.0) or (a < 0.0 and b >= 0.0)


def localizar_cruce(g, t_i, g_i, t_d, g_d):
    """Acota el primer cambio de signo de g(t) en [t_i, t_d]; devuelve (t_d final, g en t_d).

    Secante con la corrección de Illinois, señal por señal. Para cada señal que cruza:
        candidato = (t_i·g_d - t_d·g_i)/(g_d - g_i)
    recortado a [t_i + tol/2, t_d - tol/2]; si |g_d - g_i| <= 2^-45, se usa el punto medio.
    Se evalúa el menor de los candidatos. Si un extremo se conserva dos veces seguidas, sus
    valores de señal se reducen a la mitad, lo que evita la convergencia lenta de la secante.
    Termina cuando t_d - t_i <= 128·eps(t_d). El estado que se usa es el de t_d: el evento ya ocurrió.
    """
    g_i, g_d = np.array(g_i, dtype=float), np.array(g_d, dtype=float)
    orig_i, orig_d = g_i.copy(), g_d.copy()
    lado = 0
    for _ in range(MAX_ITER_ACOTACION):
        ancho = t_d - t_i
        tol = _tol_tiempo(t_d)
        if ancho <= tol:
            return t_d, orig_d
        centro = 0.5 * (t_i + t_d)
        menor, mayor = t_i + 0.5 * tol, t_d - 0.5 * tol
        tc = t_d
        for j in range(g_i.size):
            if not _cruza(orig_i[j], orig_d[j]):
                continue
            dif = g_d[j] - g_i[j]
            cand = centro if abs(dif) <= TOL_SENAL else (t_i * g_d[j] - t_d * g_i[j]) / dif
            tc = min(tc, min(mayor, max(menor, cand)))
        if not tc < t_d:
            tc = centro
        gc = np.array(g(tc), dtype=float)
        if any(_cruza(a, b) for a, b in zip(orig_i, gc)):
            # El cruce está a la izquierda: se conserva el extremo izquierdo
            t_d, g_d, orig_d = tc, gc.copy(), gc
            if lado == -1:
                g_i = 0.5 * g_i
            lado = -1
        else:
            t_i, g_i, orig_i = tc, gc.copy(), gc
            if lado == 1:
                g_d = 0.5 * g_d
            lado = 1
    raise ErrorSolver(f"No se pudo acotar un cruce por cero cerca de t={t_d:.6g}")


class _Eventos:
    """Cuenta eventos y detiene la simulación si se repiten sin avanzar en el tiempo (chattering)."""

    def __init__(self):
        self.total = 0
        self.seguidos = 0
        self.ultimo = -math.inf

    def anotar(self, t):
        self.total += 1
        if t - self.ultimo <= TOL_EVENTOS_CONSECUTIVOS * np.spacing(abs(t)):
            self.seguidos += 1
            if self.seguidos >= MAX_EVENTOS_CONSECUTIVOS:
                raise ErrorSolver(
                    f"{MAX_EVENTOS_CONSECUTIVOS} cruces por cero seguidos cerca de t={t:.6g}: "
                    "el modelo oscila sin avanzar. Revisa las discontinuidades.")
        else:
            self.seguidos = 0
        self.ultimo = t


def _paso_inicial(f0, x, rtol, umbral, hmax, intervalo):
    """Calcula el primer paso a partir de la pendiente inicial.

    h = 0.8·rtol^(1/5) / ||f0/max(|x|, umbral)||inf
    """
    h = min(hmax, intervalo)
    if f0.size == 0:
        return h
    rh = np.max(np.abs(f0) / np.maximum(np.abs(x), umbral)) / (SEGURIDAD * rtol**POTENCIA)
    if h * rh > 1.0:
        h = 1.0 / rh
    return h


def _bucle_dopri(f, t, tf, x, rtol, atol, hmax, hmin, h_ini, mayor, limite, cancelar, informe=None,
                 cruces=None, stats=None):
    rtol = max(rtol, 100 * np.finfo(float).eps)
    umbral = atol / rtol
    k1 = f(t, x)
    h = h_ini if h_ini else _paso_inicial(k1, x, rtol, umbral, hmax, tf - t)
    pasos = rechazos = 0
    t0 = t
    limitante = np.zeros(x.size, dtype=int)
    eventos = _Eventos()
    g_ant = cruces(t, x) if cruces else None
    ceros = None
    while t < tf - 1e-14 * max(abs(tf), 1.0):
        if cancelar is not None and cancelar():
            raise Cancelado()
        hminimo = max(hmin, 16 * np.spacing(abs(t) + 1e-300))
        h = min(hmax, max(hminimo, h))
        tope = min(limite(t) if limite else tf, tf)
        if tope <= t:
            tope = tf
        hp = h
        # Una señal que acaba de llegar a cero exacto (una fuente senoidal en t = 0) no tiene
        # signo todavía: se da un paso diminuto para saber hacia dónde sale.
        if cruces:
            ceros_ahora = [a == 0.0 for a in g_ant]
            if ceros is None or len(ceros) != len(ceros_ahora):
                ceros = [False] * len(ceros_ahora)
            if any(c and not p for c, p in zip(ceros_ahora, ceros)):
                hp = min(hp, max(hminimo, 1e-6 * hmax))
            ceros = ceros_ahora
        if ESTIRAR * hp >= tope - t:
            hp = tope - t
        recortado = hp < h
        sin_rechazo = True
        while True:
            xn, k, e = paso_dopri(f, t, x, hp, k1)
            if xn.size:
                rel = np.abs(e) / np.maximum(np.maximum(np.abs(x), np.abs(xn)), umbral)
                i_err = int(np.argmax(rel))
                err = float(rel[i_err])
            else:
                err, i_err = 0.0, -1
            if not np.isfinite(err):
                err = 1e12
            if err <= rtol:
                break
            rechazos += 1
            recortado = False
            if hp <= hminimo:
                raise ErrorSolver(f"No se alcanza la tolerancia en t={t:.6g} con paso {hp:.3g}")
            if sin_rechazo:
                sin_rechazo = False
                hp = max(hminimo, hp * max(REDUCCION_MIN, SEGURIDAD * (rtol / err) ** POTENCIA))
            else:
                hp = max(hminimo, 0.5 * hp)
        if i_err >= 0:
            limitante[i_err] += 1
        tn = tf if abs(t + hp - tf) <= 1e-14 * max(abs(tf), 1.0) else t + hp
        reinicio = False
        # Cruces por cero dentro del paso aceptado
        if cruces:
            g_n = cruces(tn, xn)
            if any(_cruza(a, b) for a, b in zip(g_ant, g_n)):
                ti, xi, ki = t, x, k
                te, _ = localizar_cruce(lambda s: cruces(s, interpolar_dopri(ti, xi, hp, ki, s)), t, g_ant, tn, g_n)
                xn = interpolar_dopri(ti, xi, hp, ki, te)
                tn = te
                eventos.anotar(te)
                reinicio = True
        pasos += 1
        if sin_rechazo:
            # El paso solo crece si no hubo rechazos en este intento
            temp = 1.25 * (err / rtol) ** POTENCIA if err > 0 else 0.0
            hn = hp / max(temp, 1.0 / CRECIMIENTO_MAX)
            h = max(h, hn) if recortado else hn
        else:
            h = hp
        t, x = tn, xn
        cambio = mayor(t, x) if mayor else False
        if cruces:
            g_ant = cruces(t, x)
        k1 = f(t, x) if (cambio or reinicio) else k[-1]
        if informe is not None and (pasos & 63) == 0:
            informe((t - t0) / (tf - t0))
    if stats is not None:
        stats["limitante"] = limitante
        stats["cruces"] = eventos.total
    return x, pasos, rechazos


def _validar(t0, tf, paso=None, rtol=None, atol=None, paso_max=None, fijo=False):
    """Comprueba el intervalo, el paso y las tolerancias antes de integrar.

    Raises:
        ValueError: si el intervalo no es finito o está invertido, si un solver de paso fijo no
            tiene un paso finito y positivo, o si alguna tolerancia o el paso máximo no son válidos.
    """
    if not (math.isfinite(t0) and math.isfinite(tf)) or tf < t0:
        raise ValueError(f"Intervalo de integración no válido: [{t0:g}, {tf:g}]")
    if fijo and not (paso is not None and math.isfinite(paso) and paso > 0):
        raise ValueError(f"Los solvers de paso fijo necesitan un paso finito y positivo (se dio {paso!r})")
    if rtol is not None and not (math.isfinite(rtol) and rtol > 0):
        raise ValueError(f"La tolerancia relativa debe ser positiva (se dio {rtol!r})")
    if atol is not None and not (math.isfinite(atol) and atol >= 0):
        raise ValueError(f"La tolerancia absoluta no puede ser negativa (se dio {atol!r})")
    if paso_max is not None and not paso_max > 0:
        raise ValueError(f"El paso máximo debe ser positivo (se dio {paso_max!r})")


def _bucle_fijo(paso, f, t, tf, x, h, mayor, cancelar, informe=None):
    if tf <= t:
        return x, 0, 0
    n = max(1, int(np.ceil((tf - t) / h - 1e-9)))
    t0 = t
    for i in range(n):
        if (i & 255) == 0:
            if cancelar is not None and cancelar():
                raise Cancelado()
            if informe is not None:
                informe(i / n)
        hp = min(h, tf - t)
        x = paso(f, t, x, hp)
        t = t0 + (i + 1) * h if i + 1 < n else tf
        # Se comprueba antes de registrar el estado: un valor no finito nunca llega a los resultados
        if not np.all(np.isfinite(x)):
            raise ErrorSolver(f"La solución dejó de ser finita en t={t:.6g}. Reduce el paso.")
        if mayor:
            mayor(t, x)
    return x, n, 0


def _bucle_bdf(f, t, tf, x, rtol, atol, hmax, h_ini, mayor, limite, cancelar, informe=None, cruces=None, stats=None):
    from scipy.integrate import BDF

    pasos = 0
    t0 = t
    eventos = _Eventos()
    g_ant = cruces(t, x) if cruces else None
    while t < tf - 1e-14 * max(abs(tf), 1.0):
        tope = min(limite(t) if limite else tf, tf)
        if tope <= t:
            tope = tf
        s = BDF(f, t, x, tope, max_step=hmax, rtol=rtol, atol=atol,
                first_step=None if not h_ini else min(h_ini, tope - t))
        reinicio = False
        while s.status == "running":
            if cancelar is not None and cancelar():
                raise Cancelado()
            msg = s.step()
            if s.status == "failed":
                raise ErrorSolver(f"BDF falló en t={s.t:.6g}: {msg}")
            pasos += 1
            tn, xn = s.t, s.y.copy()
            if informe is not None and (pasos & 63) == 0:
                informe((tn - t0) / (tf - t0))
            if cruces:
                g_n = cruces(tn, xn)
                if any(_cruza(a, b) for a, b in zip(g_ant, g_n)):
                    ip = s.dense_output()
                    te, _ = localizar_cruce(lambda q: cruces(q, ip(q)), s.t_old, g_ant, tn, g_n)
                    tn, xn = te, ip(te)
                    eventos.anotar(te)
                    reinicio = True
            t, x = tn, xn
            cambio = mayor(t, x) if mayor else False
            if cruces:
                g_ant = cruces(t, x)
            if cambio or reinicio:
                reinicio = True
                break
        if not reinicio:
            t, x = s.t, s.y.copy()
    if stats is not None:
        stats["cruces"] = eventos.total
    return x, pasos, 0


def funcion_estados(modelo, t0=0.0, iniciar=True):
    """La función de estados que integra `integrar_modelo`, sin integrar: (f, x0).

    f(t, x) devuelve dx/dt del diagrama con el estado discreto que el modelo tenga en ese
    momento (memorias, modos de los conmutadores). Con iniciar=True se parte del estado
    inicial del diagrama; con False, del estado en que lo dejó una simulación.
    """
    x0 = modelo.iniciar(t0) if iniciar else None
    return modelo.f, x0


def integrar_modelo(modelo, metodo="dopri", t0=0.0, tf=1.0, paso=None, rtol=RTOL_DEFECTO,
                    atol=ATOL_DEFECTO, paso_max=None, paso_min=0.0, paso_ini=None,
                    cancelar=None, informe=None):
    """Integra un modelo de bloques; el modelo guarda sus señales en sus sumideros."""
    fijo = metodo in ("euler", "rk4")
    _validar(t0, tf, paso, None if fijo else rtol, None if fijo else atol, paso_max, fijo)
    span = tf - t0
    hmax = paso_max if paso_max else max(FRACCION_PASO_MAX * span, 16 * np.spacing(abs(tf)))
    if metodo == "dae":
        from .dae import bucle_dae
        stats = {}
        x, n, r = bucle_dae(modelo, t0, tf, rtol, atol, hmax, paso_min, paso_ini, cancelar, informe, stats)
        return {"pasos": n, "rechazos": r, "evaluaciones": modelo.evaluaciones, "x_final": x, "limitante": None,
                "cruces": stats.get("cruces", 0), "jacobianos": stats.get("jacobianos", 0),
                "avisos_dae": stats.get("avisos_dae", [])}
    x = modelo.iniciar(t0)
    f = modelo.f
    mayor = modelo.paso_mayor
    limite = modelo.proximo_evento
    cruces = modelo.cruces if modelo.hay_cruces else None
    stats = {}
    if fijo:
        p = paso_euler if metodo == "euler" else paso_rk4
        x, n, r = _bucle_fijo(p, f, t0, tf, x, paso, mayor, cancelar, informe)
    elif metodo == "dopri":
        x, n, r = _bucle_dopri(f, t0, tf, x, rtol, atol, hmax, paso_min, paso_ini, mayor, limite, cancelar, informe, cruces, stats)
    elif metodo == "bdf":
        x, n, r = _bucle_bdf(f, t0, tf, x, rtol, atol, hmax, paso_ini, mayor, limite, cancelar, informe, cruces, stats)
    else:
        raise ValueError(f"Solver desconocido: {metodo}")
    return {"pasos": n, "rechazos": r, "evaluaciones": modelo.evaluaciones, "x_final": x,
            "limitante": stats.get("limitante"), "cruces": stats.get("cruces", 0)}


def integrar(f, intervalo, x0, metodo="dopri", paso=None, rtol=RTOL_DEFECTO, atol=ATOL_DEFECTO,
             paso_max=None):
    """Integra f(t, x) y devuelve (t, x) con una fila por paso.

    Ejemplo:
        t, x = integrar(lambda t, x: -x, (0, 5), [1.0], metodo="rk4", paso=0.01)
    """
    t0, tf = float(intervalo[0]), float(intervalo[1])
    fijo = metodo in ("euler", "rk4")
    _validar(t0, tf, paso, None if fijo else rtol, None if fijo else atol, paso_max, fijo)
    x0 = np.atleast_1d(np.asarray(x0, dtype=float))
    ts, xs = [t0], [x0.copy()]

    def registrar(t, x):
        ts.append(t)
        xs.append(x.copy())
        return False

    g = lambda t, x: np.atleast_1d(np.asarray(f(t, x), dtype=float))
    hmax = paso_max if paso_max else FRACCION_PASO_MAX * (tf - t0)
    if metodo in ("euler", "rk4"):
        p = paso_euler if metodo == "euler" else paso_rk4
        _bucle_fijo(p, g, t0, tf, x0, paso, registrar, None)
    elif metodo == "dopri":
        _bucle_dopri(g, t0, tf, x0, rtol, atol, hmax, 0.0, None, registrar, None, None)
    elif metodo == "bdf":
        _bucle_bdf(g, t0, tf, x0, rtol, atol, hmax, None, registrar, None, None)
    else:
        raise ValueError(f"Solver desconocido: {metodo}")
    return np.array(ts), np.array(xs)
