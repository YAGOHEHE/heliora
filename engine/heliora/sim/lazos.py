"""Lazos algebraicos: se detectan, se cortan y se resuelven con una región de confianza.

Un lazo algebraico es un ciclo de dependencias directas: a = f(b), b = g(a). Se elige una
variable de corte z (las salidas de un bloque del ciclo) y se busca el valor que cierra el lazo:

    F(z) = y(z) - z = 0

donde y(z) es lo que devuelve ese bloque al evaluar el ciclo suponiendo z.

Método `resolver`:
  1. Jacobiano numérico por perturbación:  J[:, j] ≈ (F(z + δ·e_j) - F(z))/δ
  2. Paso de Newton por QR:  J·p = -F.  Se evalúa F en z + p; si el lazo es lineal, ahí termina.
  3. Si no: región de confianza de radio Δ con paso dogleg entre Newton y descenso.
       Δ inicial = 100·||z|| (o 100 si z = 0)
       razón ρ = reducción real / reducción prevista
       ρ < 0.1: Δ se reduce a la mitad
       ρ >= 0.5, o dos pasos aceptados seguidos: Δ = máx(Δ, 2·||p||)
       |ρ - 1| <= 0.1 (el modelo acertó): Δ = 2·||p||
       el paso se acepta si ρ >= 1e-4
  (reglas leídas en ne::TrustRegion::solve, 0xb0354, bloque 0xb1440-0xb1528)
  Como mucho, 200·n evaluaciones de F.
"""
from __future__ import annotations

import numpy as np

FACTOR_RADIO = 100.0
RHO_ACEPTAR = 1e-4
RHO_REDUCIR = 0.1
RHO_AMPLIAR = 0.5
EXITOS_PARA_AMPLIAR = 2          # pasos aceptados seguidos que también amplían la región
TOL_PREDICCION_EXACTA = 0.1      # |ρ - 1| <= 0.1: el radio pasa a ser 2·|p|
FACTOR_AMPLIAR = 2.0
MAX_EVAL_POR_VARIABLE = 200
TOL_DEFECTO = 1e-10


class SinSolucion(ValueError):
    """El lazo algebraico no tiene solución alcanzable."""


def _jacobiano(F, z, f0):
    n = z.size
    J = np.empty((n, n))
    for j in range(n):
        d = 1.5e-8 * max(abs(z[j]), 1.0)
        w = z.copy()
        w[j] += d
        J[:, j] = (F(w) - f0) / d
    return J


def _newton_qr(J, f):
    """Resuelve J·p = -f por QR; None si J es singular."""
    Q, R = np.linalg.qr(J)
    d = np.abs(np.diag(R))
    if d.size == 0 or d.min() <= 1e-13 * max(d.max(), 1.0):
        return None
    return np.linalg.solve(R, -Q.T @ f)


def _dogleg(J, f, p_n, radio):
    """Paso dentro de la región: Newton si cabe; si no, mezcla de descenso y Newton."""
    if p_n is not None and np.linalg.norm(p_n) <= radio:
        return p_n
    g = J.T @ f
    jg = J @ g
    den = float(jg @ jg)
    if den <= 0.0:
        return -radio * g / max(np.linalg.norm(g), 1e-300)
    p_c = -float(g @ g) / den * g
    nc = np.linalg.norm(p_c)
    if nc >= radio or p_n is None:
        return p_c * min(1.0, radio / max(nc, 1e-300))
    d = p_n - p_c
    a, b, c = float(d @ d), 2.0 * float(p_c @ d), float(p_c @ p_c) - radio**2
    s = (-b + np.sqrt(max(b * b - 4 * a * c, 0.0))) / (2 * a)
    return p_c + s * d


def resolver(F, z0, tol=TOL_DEFECTO, J_previo=None):
    """Resuelve F(z) = 0. Devuelve (z, J, evaluaciones). J_previo acelera lazos lineales."""
    z = np.array(z0, dtype=float)
    n = z.size
    f = F(z)
    evals = 1
    limite = MAX_EVAL_POR_VARIABLE * max(n, 1)
    escala = lambda v: tol * (1.0 + np.max(np.abs(v)))
    if np.max(np.abs(f)) <= escala(z):
        return z, J_previo, evals
    # Con el jacobiano del paso anterior, una o dos iteraciones suelen bastar
    if J_previo is not None:
        p = _newton_qr(J_previo, f)
        if p is not None:
            for _ in range(3):
                zt = z + p
                ft = F(zt)
                evals += 1
                if not np.all(np.isfinite(ft)) or np.linalg.norm(ft) >= np.linalg.norm(f):
                    break
                z, f = zt, ft
                if np.max(np.abs(f)) <= escala(z):
                    return z, J_previo, evals
                p = _newton_qr(J_previo, f)
    J = _jacobiano(F, z, f)
    evals += n
    radio = FACTOR_RADIO * np.linalg.norm(z) or FACTOR_RADIO
    exitos = 0
    while evals < limite:
        p_n = _newton_qr(J, f)
        p = _dogleg(J, f, p_n, radio)
        zt = z + p
        ft = F(zt)
        evals += 1
        previsto = np.linalg.norm(f) ** 2 - np.linalg.norm(f + J @ p) ** 2
        real = np.linalg.norm(f) ** 2 - np.linalg.norm(ft) ** 2 if np.all(np.isfinite(ft)) else -np.inf
        rho = real / previsto if previsto > 0 else -np.inf
        np_ = np.linalg.norm(p)
        if rho < RHO_REDUCIR:
            exitos = 0
            radio *= 0.5
        else:
            exitos += 1
            # Se amplía si el modelo predijo bien o tras dos pasos buenos seguidos
            if rho >= RHO_AMPLIAR or exitos >= EXITOS_PARA_AMPLIAR:
                radio = max(radio, FACTOR_AMPLIAR * np_)
            # Predicción casi exacta: el radio pasa a ser justo 2·|p|
            if abs(rho - 1.0) <= TOL_PREDICCION_EXACTA:
                radio = FACTOR_AMPLIAR * np_
        if rho >= RHO_ACEPTAR:
            z, f = zt, ft
            if np.max(np.abs(f)) <= escala(z):
                return z, J, evals
            J = _jacobiano(F, z, f)
            evals += n
        elif radio <= 1e-14 * (1.0 + np.linalg.norm(z)):
            break
    raise SinSolucion("no converge")


def componentes_fuertes(grafo):
    """Componentes fuertemente conexas (Tarjan, iterativo). grafo[k] = nodos de los que k depende."""
    n = len(grafo)
    indice = [-1] * n
    bajo = [0] * n
    en_pila = [False] * n
    pila, salida = [], []
    contador = 0
    for raiz in range(n):
        if indice[raiz] != -1:
            continue
        trabajo = [(raiz, iter(sorted(grafo[raiz])))]
        indice[raiz] = bajo[raiz] = contador
        contador += 1
        pila.append(raiz)
        en_pila[raiz] = True
        while trabajo:
            v, it = trabajo[-1]
            avanzo = False
            for w in it:
                if indice[w] == -1:
                    indice[w] = bajo[w] = contador
                    contador += 1
                    pila.append(w)
                    en_pila[w] = True
                    trabajo.append((w, iter(sorted(grafo[w]))))
                    avanzo = True
                    break
                if en_pila[w]:
                    bajo[v] = min(bajo[v], indice[w])
            if avanzo:
                continue
            trabajo.pop()
            if trabajo:
                bajo[trabajo[-1][0]] = min(bajo[trabajo[-1][0]], bajo[v])
            if bajo[v] == indice[v]:
                comp = []
                while True:
                    w = pila.pop()
                    en_pila[w] = False
                    comp.append(w)
                    if w == v:
                        break
                salida.append(sorted(comp))
    return salida  # en orden: una componente va después de aquellas de las que depende
