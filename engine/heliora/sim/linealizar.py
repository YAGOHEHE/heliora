"""Linealización de un diagrama en un punto de operación.

Alrededor de un punto (t*, x*, u*), el diagrama no lineal dx/dt = f(t, x, u), y = g(t, x, u)
se aproxima por

    d(Δx)/dt = A·Δx + B·Δu        Δy = C·Δx + D·Δu

con A = ∂f/∂x, B = ∂f/∂u, C = ∂g/∂x y D = ∂g/∂u, calculadas por diferencias centrales:

    ∂f/∂x_j ≈ (f(x + δ_j·e_j) - f(x - δ_j·e_j)) / (2·δ_j)

con el paso del Jacobiano aproximado del original: δ_j = √eps·máx(|x_j|, 1), redondeado para que
x_j ± δ_j sea exacto; si se anula, el factor sube ×100 hasta 0.1 (ver `paso_perturbacion`).

Entradas: salidas de bloques que se fijan desde fuera (por ejemplo la referencia de un lazo).
Salidas: cualquier señal del diagrama. Ambas se escriben «bloque» o «bloque.puerto».

Punto de operación:
  - dado por la persona: {"t": ..., "x": [...]} (lo que falte se completa);
  - si no, se simula el diagrama hasta su tiempo final (asentar) y se refina con el Newton de
    región de confianza de `lazos.resolver` sobre f(t*, x, u*) = 0. Si Newton no converge o se
    aleja, se usa el estado simulado tal cual (y se dice).
"""
from __future__ import annotations

import numpy as np

from .diagrama import ErrorDiagrama, Modelo, _num
from .lazos import SinSolucion, resolver
from .nucleo import evaluar_numero
from .solvers import funcion_estados, integrar_modelo

# Perturbación de las derivadas numéricas, como el Jacobiano aproximado del original
# (libmwslexec_solver.dylib, ApproximateJacobian, constructor 0xf98):
FAC_INICIAL = 1.4901161193847656e-08   # √eps (campo +0x28)
FAC_MAX = 0.1                          # facmax (campo +0x38)
FACTOR_AUMENTO = 100.0                 # 0x4059000000000000 en perturbation, 0xaf664
ESCALA_MINIMA = 1.0                    # propia: el original toma un umbral que fija quien lo llama
# Tamaño máximo del Jacobiano por perturbación: el original, por encima de 0x1999999 entradas,
# cambia de método para no agotar la memoria (sl::jacobian::modelNeedsUseFullPerturbation, 0x764fe8).
# Heliora solo tiene perturbación completa, así que por encima de ese tamaño se niega con un aviso.
MAX_ENTRADAS_JACOBIANO = 26843545
FUENTES = ("escalon", "constante", "senoidal", "rampa", "entrada", "tren_pulsos")


def _entradas_por_defecto(diagrama):
    """La primera fuente de señal del nivel superior que alimenta a algún bloque."""
    s = diagrama.get("sistema", {})
    usadas = {c["de"][0] for c in s.get("conexiones", [])}
    for b in s.get("bloques", []):
        if b["tipo"] in FUENTES and b["id"] in usadas:
            return [b["id"]]
    raise ErrorDiagrama("No hay ninguna fuente de señal (escalón, constante...) que usar como entrada. Indica una.")


def _salidas_por_defecto(diagrama, entradas):
    """Las señales que llegan al primer osciloscopio, sin contar las entradas."""
    s = diagrama.get("sistema", {})
    for b in s.get("bloques", []):
        if b["tipo"] == "osciloscopio":
            llegan = sorted((c for c in s.get("conexiones", []) if c["a"][0] == b["id"]), key=lambda c: c["a"][1])
            sal = [f"{c['de'][0]}.{c['de'][1]}" for c in llegan if c["de"][0] not in entradas]
            if sal:
                return sal
    raise ErrorDiagrama("No hay ninguna salida marcada. Conecta un osciloscopio a la señal que quieres estudiar o indícala.")


def paso_perturbacion(v, fac=FAC_INICIAL, escala=ESCALA_MINIMA):
    """Paso δ de la perturbación de una variable, con la regla del original.

    δ = fac·máx(|v|, escala), y se toma δ = (v + δ) - v para que v ± δ sea exacto en coma
    flotante. Si así sale 0, fac se multiplica por 100 hasta FAC_MAX (0.1) y se repite.
    (ApproximateJacobian, 0xf98; NonLinearSparseApproximateJacobian::perturbation, 0xaf664)
    """
    peso = max(abs(v), escala)
    while True:
        d = (v + fac * peso) - v
        if d != 0.0 or fac >= FAC_MAX:
            return d if d != 0.0 else fac * peso
        fac = min(fac * FACTOR_AUMENTO, FAC_MAX)


def _jacobiano(fun, v, n_sal):
    """Diferencias centrales: (f(v + δ·e_j) - f(v - δ·e_j)) / (2·δ), columna a columna."""
    m = np.empty((n_sal, v.size))
    for j in range(v.size):
        d = paso_perturbacion(float(v[j]))
        a, b = v.copy(), v.copy()
        a[j] += d
        b[j] -= d
        m[:, j] = (fun(a) - fun(b)) / (2.0 * d)
    return m


def linealizar(diagrama, espacio=None, entradas=None, salidas=None, punto=None, cancelar=None):
    """Modelo lineal (A, B, C, D) de un diagrama continuo en un punto de operación.

    Args:
        diagrama: diagrama JSON.
        espacio: variables del espacio de trabajo para evaluar los parámetros.
        entradas: señales que se perturban («bloque» o «bloque.puerto»); por defecto, la primera fuente.
        salidas: señales observadas; por defecto, las del primer osciloscopio.
        punto: punto de operación {"t": ..., "x": [...]}; si falta, se simula y se refina con Newton.
        cancelar: función que devuelve True para detener la simulación del punto.

    Returns:
        Diccionario con A, B, C, D, los nombres de estados, entradas y salidas, el punto usado y el
        modelo compilado (para la procedencia).

    Raises:
        ErrorDiagrama: si el diagrama tiene bloques de tiempo discreto, si faltan entradas o
            salidas, o si el modelo sería demasiado grande.
    """
    entradas = list(entradas or _entradas_por_defecto(diagrama))
    salidas = list(salidas or _salidas_por_defecto(diagrama, entradas))
    m = Modelo(diagrama, espacio)
    # A, B, C y D solo cubren el estado continuo: el estado de un bloque muestreado quedaría fuera y
    # el modelo describiría otro sistema. Hasta linealizar su mapa de actualización, se rechaza.
    if m.discretos:
        bloques = ", ".join(f"«{m.lista[k].etiqueta}» (Ts = {m.lista[k].Ts:g} s)" for k in m.discretos[:6])
        raise ErrorDiagrama(
            f"No se puede linealizar un diagrama con tiempo discreto: {bloques}. Su estado muestreado no entra "
            "en el modelo continuo (A, B, C, D), que describiría otro sistema. Sustituye esos bloques por sus "
            "equivalentes continuos para linealizar.")
    cfg = dict(diagrama.get("solver", {}))
    punto = dict(punto or {})
    # 1. Punto de operación
    metodo_punto = "dado"
    if "x" in punto:
        t = float(evaluar_numero(punto.get("t", cfg.get("t_fin", 0.0))))
        f, _ = funcion_estados(m, t)
        x = np.asarray(punto["x"], dtype=float)
        if x.size != m.x_ini.size:
            raise ErrorDiagrama(f"El punto tiene {x.size} estados y el diagrama {m.x_ini.size}")
        m.paso_mayor(t, x)
    else:
        t = float(evaluar_numero(punto.get("t", cfg.get("t_fin", 1.0))))
        metodo = cfg.get("metodo", "dopri")
        info = integrar_modelo(
            m, metodo=metodo, t0=float(evaluar_numero(cfg.get("t_ini", 0.0), vacio=0.0)), tf=t, paso=_num(cfg.get("paso")),
            rtol=float(evaluar_numero(cfg.get("rtol", 1e-3), vacio=1e-3)), atol=float(evaluar_numero(cfg.get("atol", 1e-6), vacio=1e-6)),
            paso_max=_num(cfg.get("paso_max")), cancelar=cancelar)
        x = np.asarray(info["x_final"], dtype=float)
        metodo_punto = "simulado hasta t = %g s" % t
    f = m.f
    # Los conmutadores evalúan su modo directamente (sin el retén del paso variable)
    for b in m.lista:
        b.usar_zc = False
    dirs_u = [m.direccion_senal(e) for e in entradas]
    dirs_y = [m.direccion_senal(s) for s in salidas]
    for d in dirs_u:
        if d[0] == m.red_idx:
            raise ErrorDiagrama("Una entrada de linealización debe ser una señal, no una salida eléctrica")
    m.salidas(t, x)
    u0 = np.array([float(np.ravel(m.Y[k][j])[0]) for k, j in dirs_u])

    def fijar(u):
        m.forzadas = {}
        for (k, j), v in zip(dirs_u, u):
            m.forzadas.setdefault(k, {})[j] = float(v)

    def fx(xv, u=u0):
        fijar(u)
        return np.asarray(f(t, xv), dtype=float)

    def gx(xv, u=u0):
        fijar(u)
        m.salidas(t, xv)
        return np.array([float(np.ravel(m.Y[k][j])[0]) for k, j in dirs_y])

    # Refinado con Newton (solo si se simuló): f(t*, x, u*) = 0
    if "x" not in punto and x.size:
        try:
            xr, _, _ = resolver(lambda v: fx(v), x)
            if np.all(np.isfinite(xr)) and np.linalg.norm(xr - x) <= 1e-2 * (1.0 + np.linalg.norm(x)):
                x = xr
                metodo_punto += ", refinado con Newton (f = 0)"
        except (SinSolucion, np.linalg.LinAlgError):
            metodo_punto += " (Newton no convergió: estado simulado)"
    # 2. Jacobianos
    n, p_, q = x.size, len(u0), len(dirs_y)
    if (n + q) * (n + p_) > MAX_ENTRADAS_JACOBIANO:
        raise ErrorDiagrama(
            f"El modelo lineal tendría {(n + q) * (n + p_)} coeficientes (más de {MAX_ENTRADAS_JACOBIANO}). "
            "Linealiza un subsistema más pequeño.")
    A = _jacobiano(lambda v: fx(v), x, n) if n else np.zeros((0, 0))
    B = _jacobiano(lambda v: fx(x, v), u0, n) if n else np.zeros((0, p_))
    C = _jacobiano(lambda v: gx(v), x, q) if n else np.zeros((q, 0))
    D = _jacobiano(lambda v: gx(x, v), u0, q)
    y0 = gx(x)
    residuo = float(np.max(np.abs(fx(x)))) if n else 0.0
    m.forzadas = {}
    estados = [f"{m.nombre_estado(i)} [{i}]" for i in range(n)]
    return {"A": A, "B": B, "C": C, "D": D, "estados": estados, "entradas": entradas, "salidas": salidas,
            "punto": {"t": t, "x": x, "u": u0, "y": y0, "metodo": metodo_punto, "residuo_f": residuo}, "modelo": m}


def funcion_transferencia(lin, entrada=0, salida=0):
    """G(s) de una entrada a una salida: (num, den) en potencias decrecientes de s."""
    from scipy import signal
    A, B, C, D = (np.asarray(lin[k], dtype=float) for k in "ABCD")
    if A.size == 0:
        return np.array([D[salida, entrada]]), np.array([1.0])
    num, den = signal.ss2tf(A, B[:, [entrada]], C[[salida], :], D[[salida], :][:, [entrada]])
    num = np.trim_zeros(np.where(np.abs(num[0]) < 1e-12 * max(1.0, np.max(np.abs(num[0]))), 0.0, num[0]), "f")
    return (num if num.size else np.array([0.0])), np.asarray(den, dtype=float)
