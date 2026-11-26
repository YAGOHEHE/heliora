"""Solver de ecuaciones diferenciales-algebraicas (DAE) para el diagrama completo.

Incógnitas:  y = [x; z]
    x  estados del diagrama (bloques de señal y estados libres de los elementos)
    z  tensiones de nodo y corrientes de rama de la red (modo DAE de `circuito.Red`)

Residuo:  F(t, y, ẏ) = 0
    F_x = ẋ - f(t, x, z)                         (f: derivadas del modelo con z dado)
    F_z = M(z)·z - b(t, x, u(z))                  (ecuaciones de la red)
          d(fila_k·z)/dt - derivada del almacén   (ramas de condensadores, inercias, capacidades térmicas)

Integración: fórmulas de diferencias hacia atrás (BDF) de paso variable.
    orden 1   ẏ₁ = (y₁ - y₀)/h
    orden 2   ẏ₁ = [(1+2ω)/(1+ω)·y₁ - (1+ω)·y₀ + ω²/(1+ω)·y₋₁]/h,   ω = h/h_anterior
Cada paso se resuelve con Newton: J = ∂F/∂y + α·∂F/∂ẏ, con α el coeficiente de y₁ en ẏ₁.
∂F/∂ẏ es constante (identidad en x, filas de almacén en z); ∂F/∂y se calcula por diferencias
finitas y se reutiliza mientras Newton converge, una por cada combinación de interruptores.

Error local (Milne): diferencia entre el predictor (extrapolación) y el corrector, escalada por las
constantes de error de cada fórmula. Solo cuenta en las incógnitas diferenciales (x y las de z que
aparecen derivadas): las algebraicas, incluidas las de índice 2 (par de un engranaje entre dos
inercias), quedan fuera del control de error, como en los simuladores de circuitos.

Tras un evento (interruptor, muestreo, cruce por cero) el paso vuelve a orden 1: la historia de un
lado del salto no vale para el otro.
"""
from __future__ import annotations

import numpy as np
import scipy.linalg

from .solvers import Cancelado, ErrorSolver, _bucle_dopri, _Eventos, _cruza, localizar_cruce

SEGURIDAD = 0.8
CRECIMIENTO_MAX = 2.0
CRECIMIENTO_MIN = 1.2
REDUCCION_MIN = 0.2
MAX_NEWTON = 5
TOL_NEWTON = 0.33          # la corrección de Newton debe ser menor que 1/3 de la tolerancia
MAX_FALLOS_NEWTON = 12


class SistemaDAE:
    """Residuo, Jacobiano y estado coherente de un `Modelo` compilado en modo DAE."""

    def __init__(self, modelo):
        self.m = modelo
        self.red = modelo.red if (modelo.red is not None and modelo.red.dae) else None
        self.nx = int(modelo.x_ini.size)
        self.nz = self.red.tam if self.red else 0
        self.n = self.nx + self.nz
        self.Jp = np.zeros((self.n, self.n))
        self.Jp[:self.nx, :self.nx] = np.eye(self.nx)
        if self.red:
            self.Jp[self.nx:, self.nx:] = self.red.masa()
            self.dif = np.concatenate([np.ones(self.nx, dtype=bool), self.red.diferenciales()])
        else:
            self.dif = np.ones(self.nx, dtype=bool)
        self.evaluaciones = 0
        self.jacobianos = 0
        self._cache_jac = {}
        self._patron = None
        self._grupos = []

    # ------------------------------------------------------------ residuo
    def fijar(self, y):
        """Propone y a la red (z) antes de evaluar el modelo."""
        if self.red:
            self.red.fijar_z(y[self.nx:])

    def residuo(self, t, y, yp):
        """F(t, y, ẏ) para el diagrama completo."""
        self.evaluaciones += 1
        x = y[:self.nx]
        self.fijar(y)
        F = np.empty(self.n)
        F[:self.nx] = yp[:self.nx] - self.m.f(t, x)
        if self.red:
            r = self.m.red_idx
            a, c = self.m.tramos[r]
            F[self.nx:] = self.red.residuo(t, x[a:c], self.m._u(r), yp[self.nx:])
        return F

    def jacobiano_y(self, t, y, yp, F0, escala, denso=False):
        """∂F/∂y por diferencias finitas hacia delante.

        La primera vez (o con `denso`) se perturba cada columna por separado y se guarda qué filas
        dependen de cada incógnita. Después, las columnas que no comparten filas se perturban a la vez
        (agrupación de Curtis, Powell y Reid): la red es dispersa y bastan unos pocos grupos.
        """
        self.jacobianos += 1
        raiz = np.sqrt(np.finfo(float).eps)
        paso = raiz * np.maximum(np.abs(y), escala)
        J = np.zeros((self.n, self.n))
        if denso or self._patron is None:
            for j in range(self.n):
                yj = y.copy()
                yj[j] += paso[j]
                J[:, j] = (self.residuo(t, yj, yp) - F0) / paso[j]
            nuevo = (J != 0.0) | np.eye(self.n, dtype=bool)
            self._patron = nuevo if self._patron is None else (self._patron | nuevo)
            self._grupos = _colorear(self._patron)
        else:
            for g in self._grupos:
                yj = y.copy()
                yj[g] += paso[g]
                dF = self.residuo(t, yj, yp) - F0
                for j in g:
                    filas = self._patron[:, j]
                    J[filas, j] = dF[filas] / paso[j]
        self.fijar(y)
        return J

    def clave(self):
        """Combinación de interruptores: cada una tiene su Jacobiano guardado.

        Los cambios pequeños de valores (resistencias que dependen de la temperatura, `tras_paso`) no
        cambian la clave: los absorben las correcciones de Broyden y, si no basta, el recálculo.
        """
        if not self.red:
            return 0
        return tuple(e.clave() for e in self.red.conmutados)

    # ------------------------------------------------------------ estado coherente
    def y_actual(self, x):
        """Junta x con la z que la red tenga ahora (tras iniciar o tras un evento)."""
        if not self.red:
            return np.array(x, dtype=float)
        return np.concatenate([x, self.red.z_ext[:-1]])

    def coherente(self, t, x, vueltas=4):
        """Repite la resolución coherente de la red con las entradas ya actualizadas por el diagrama.

        La primera resolución usa las salidas de los bloques del instante anterior; unas pocas vueltas
        bastan para que sensores y controladores conectados a la red queden de acuerdo con ella.
        """
        if not self.red:
            return
        r = self.m.red_idx
        a, c = self.m.tramos[r]
        previo = None
        for _ in range(vueltas):
            self.m.salidas(t, x)
            z = self.red.consistente(t, x[a:c], self.m._u(r)).copy()
            if previo is not None and np.allclose(z, previo, rtol=1e-10, atol=1e-12):
                break
            previo = z


def _norma(v, w):
    return float(np.max(np.abs(v) / w)) if v.size else 0.0


def _colorear(patron):
    """Grupos de columnas sin filas en común (coloreado voraz), para perturbarlas a la vez."""
    grupos, filas_de = [], []
    for j in range(patron.shape[1]):
        col = patron[:, j]
        for g, ocupadas in zip(grupos, filas_de):
            if not np.any(ocupadas & col):
                g.append(j)
                ocupadas |= col
                break
        else:
            grupos.append([j])
            filas_de.append(col.copy())
    return [np.array(g, dtype=int) for g in grupos]


def _factorizar(A):
    """Factorización LU de la matriz de Newton, o None si es singular o no finita."""
    try:
        lu = scipy.linalg.lu_factor(A, check_finite=False)
    except (ValueError, np.linalg.LinAlgError):
        return None
    d = np.abs(np.diag(lu[0]))
    if not np.all(np.isfinite(lu[0])) or d.size and np.min(d) == 0:
        return None
    return lu


def bucle_dae(modelo, t, tf, rtol, atol, hmax, hmin, h_ini, cancelar=None, informe=None, stats=None):
    """Integra el modelo en modo DAE de t a tf; devuelve (x final, pasos, rechazos)."""
    S = SistemaDAE(modelo)
    nx = S.nx
    x0 = modelo.iniciar(t, preparar=S.coherente)
    y = S.y_actual(x0)
    mayor = modelo.paso_mayor
    limite = modelo.proximo_evento
    cruces = modelo.cruces if modelo.hay_cruces else None
    if S.n == 0:
        # Sin estados ni red no hay ecuaciones implícitas: el diagrama de señal avanza como siempre y sus
        # osciloscopios registran cada paso, no solo el instante inicial.
        return _bucle_dopri(modelo.f, t, tf, x0, rtol, atol, hmax, hmin, h_ini, mayor, limite, cancelar,
                            informe, cruces, stats)
    # Perturbación de las diferencias finitas: √eps·max(|y|, 1). Con un suelo menor, una corriente pequeña
    # (mA) cambia una ecuación de nodo con términos de 1e5 menos que su redondeo y su entrada se pierde.
    escala = np.ones(S.n)
    eventos = _Eventos()
    t0 = t
    pasos = rechazos = 0
    rechazos_newton = 0
    hist = [(t, y.copy())]     # (t, y) aceptados desde el último evento, el actual al final
    yp = np.zeros(S.n)
    h = h_ini or min(hmax, (tf - t) * 1e-4)
    fallos = 0

    def g(s, ys):
        S.fijar(ys)
        return cruces(s, ys[:nx])

    def derivada_inicial(t, y, previa):
        """Derivada para el predictor del primer paso tras un evento.

        ẋ = f(t, x) exacta. En la red se conserva la derivada de antes del evento: medido en los
        ejemplos conmutados, rechaza menos pasos que la derivada de norma mínima de las ramas de almacén.
        """
        F = S.residuo(t, y, np.zeros(S.n))
        d = previa.copy()
        d[:nx] = -F[:nx]
        return d

    g_ant = g(t, y) if cruces else None
    yp = derivada_inicial(t, y, yp)
    while t < tf - 1e-14 * max(abs(tf), 1.0):
        if cancelar is not None and cancelar():
            raise Cancelado()
        hminimo = max(hmin, 16 * np.spacing(abs(t) + 1e-300))
        tope = min(limite(t), tf)
        if tope <= t:
            tope = tf
        h = min(h, hmax)
        hp = tope - t if 1.1 * h >= tope - t else h
        orden = 2 if len(hist) >= 3 else 1
        # Coeficientes: ẏ₁ = α·y₁ + β
        if orden == 1:
            alfa = 1.0 / hp
            beta = -y / hp
            if len(hist) >= 2:
                (ta, ya) = hist[-2]
                pred = y + (y - ya) * (hp / (t - ta))
                ha = t - ta
                k_err = hp / (2.0 * hp + ha)
            else:
                pred = y + hp * yp
                k_err = 0.5
        else:
            (ta, ya), (tb, yb) = hist[-2], hist[-3]
            ha = t - ta
            w = hp / ha
            alfa = (1.0 + 2.0 * w) / ((1.0 + w) * hp)
            beta = (-(1.0 + w) * y + (w * w / (1.0 + w)) * ya) / hp
            # Predictor: parábola por los tres últimos puntos
            t1 = t + hp
            l0 = (t1 - ta) * (t1 - tb) / ((t - ta) * (t - tb))
            l1 = (t1 - t) * (t1 - tb) / ((ta - t) * (ta - tb))
            l2 = (t1 - t) * (t1 - ta) / ((tb - t) * (tb - ta))
            pred = l0 * y + l1 * ya + l2 * yb
            c_cor = 2.0 / 9.0 * hp ** 3
            c_pre = hp * (hp + ha) * (hp + (t - tb)) / 6.0
            k_err = c_cor / (c_cor + c_pre)
        t1 = tf if abs(t + hp - tf) <= 1e-14 * max(abs(tf), 1.0) else t + hp
        w_tol = atol + rtol * np.maximum(np.abs(y), np.abs(pred))
        # Newton
        y1 = pred.copy()
        clave = S.clave()
        convergio = False
        # Intentos: Jacobiano guardado; recalculado por grupos de columnas; recalculado columna a columna
        # (por si el patrón de dispersión guardado no tiene una dependencia que acaba de aparecer)
        fresco = None
        for intento in range(3):
            Jy = S._cache_jac.get(clave) if intento == 0 else None
            if Jy is None:
                if fresco == "denso":
                    break
                fresco = "grupos" if (fresco is None and S._patron is not None) else "denso"
                F0 = S.residuo(t1, y1, alfa * y1 + beta)
                Jy = S.jacobiano_y(t1, y1, alfa * y1 + beta, F0, escala, denso=fresco == "denso")
                S._cache_jac[clave] = Jy
            lu = _factorizar(Jy + alfa * S.Jp)
            if lu is not None:
                previo = None
                F = S.residuo(t1, y1, alfa * y1 + beta)
                for _ in range(MAX_NEWTON):
                    if not np.all(np.isfinite(F)):
                        break
                    dy = scipy.linalg.lu_solve(lu, -F, check_finite=False)
                    y1 = y1 + dy
                    nd = _norma(dy, w_tol)
                    if not np.isfinite(nd):
                        break
                    Fn = S.residuo(t1, y1, alfa * y1 + beta)
                    # Broyden: corrige ∂F/∂y con lo observado en esta iteración. Sigue a los Jacobianos que
                    # cambian deprisa (par electromagnético de una máquina en alterna) sin recalcularlos.
                    if np.all(np.isfinite(Fn)):
                        dd = float(dy @ dy)
                        if dd > 0:
                            Jy += np.outer(Fn - F - alfa * (S.Jp @ dy) - Jy @ dy, dy) / dd
                            lu = _factorizar(Jy + alfa * S.Jp) or lu
                    F = Fn
                    # Un incremento pequeño no basta: el punto debe cumplir unas ecuaciones finitas
                    if not np.all(np.isfinite(Fn)):
                        break
                    if nd <= 1e-2 * TOL_NEWTON:
                        convergio = True
                        break
                    if previo is not None:
                        ritmo = nd / previo
                        if ritmo >= 0.9:
                            break
                        # Error que queda tras esta corrección, si la convergencia es lineal con ese ritmo
                        if nd * ritmo / (1.0 - ritmo) <= TOL_NEWTON:
                            convergio = True
                            break
                    previo = nd
                if convergio:
                    break
            S._cache_jac.pop(clave, None)
            y1 = pred.copy()
        if not convergio:
            rechazos += 1
            rechazos_newton += 1
            fallos += 1
            if fallos > MAX_FALLOS_NEWTON or hp <= hminimo:
                raise ErrorSolver(f"El solver DAE no converge en t={t:.6g} con paso {hp:.3g}: revisa los valores "
                                  "iniciales o si la red tiene una incógnita sin ecuación.")
            h = max(hminimo, hp / 4.0)
            hist = hist[-1:]
            continue
        fallos = 0
        S.fijar(y1)
        err = _norma(k_err * (y1 - pred)[S.dif], w_tol[S.dif]) if np.any(S.dif) else 0.0
        if not np.isfinite(err):
            err = 1e12
        if err > 1.0:
            if hp <= hminimo:
                raise ErrorSolver(f"No se alcanza la tolerancia en t={t:.6g} con paso {hp:.3g}")
            rechazos += 1
            h = max(hminimo, hp * max(REDUCCION_MIN, SEGURIDAD * err ** (-1.0 / (orden + 1))))
            continue
        # Paso aceptado
        yp_n = alfa * y1 + beta
        reinicio = False
        if cruces:
            g_n = g(t1, y1)
            if any(_cruza(a, b) for a, b in zip(g_ant, g_n)):
                ti, yi = t, y.copy()

                def interp(s, ti=ti, yi=yi, y1=y1, t1=t1):
                    return yi + (y1 - yi) * ((s - ti) / (t1 - ti))

                te, _ = localizar_cruce(lambda s: g(s, interp(s)), t, g_ant, t1, g_n)
                t1, y1 = te, interp(te)
                S.fijar(y1)
                eventos.anotar(te)
                reinicio = True
        pasos += 1
        fac = CRECIMIENTO_MAX if err == 0 else min(CRECIMIENTO_MAX, max(REDUCCION_MIN, SEGURIDAD * err ** (-1.0 / (orden + 1))))
        # Histéresis: el paso solo crece si puede crecer bastante (evita rechazos por subir y bajar)
        if 1.0 <= fac < CRECIMIENTO_MIN:
            fac = 1.0
        h = max(hminimo, hp * fac)
        t, y = t1, y1
        yp = yp_n
        cambio = mayor(t, y[:nx])
        # paso_mayor puede haber cambiado los interruptores y, con ellos, la parte algebraica de z
        y = S.y_actual(y[:nx])
        if cruces:
            g_ant = g(t, y)
        if cambio or reinicio:
            hist = [(t, y.copy())]
            yp = derivada_inicial(t, y, yp)
        else:
            hist.append((t, y.copy()))
            hist = hist[-3:]
        if informe is not None and (pasos & 63) == 0:
            informe((t - t0) / (tf - t0))
    if stats is not None:
        stats["cruces"] = eventos.total
        stats["jacobianos"] = S.jacobianos
        stats["rechazos_newton"] = rechazos_newton
        stats["evaluaciones_dae"] = S.evaluaciones
        if S.red is not None:
            stats["avisos_dae"] = sorted(S.red.avisos_dae)
    return y[:nx], pasos, rechazos
