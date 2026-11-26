"""Electrónica de potencia: convertidores, filtros, modulación, control y medida.

Transformadas con invariancia de amplitud:
    α = (2/3)(a - b/2 - c/2);   β = (b - c)/√3
    d =  α·cos θ + β·sin θ;     q = -α·sin θ + β·cos θ
Con v_a = V·cos θ se obtiene v_d = V, v_q = 0. Potencias: P = 1.5·(v_d·i_d + v_q·i_q).
"""
from __future__ import annotations

from typing_extensions import override
import math
from collections import deque

import numpy as np

from ..sim.circuito import Elemento
from ..sim.nucleo import Bloque, P, registrar
from .electrica import DispositivoIGBT, _ParametrosIGBT, aplicar_temperatura_igbt
from .termica import UnionesTermicas, parametros_union

RAIZ3 = math.sqrt(3.0)
DOS_TERCIOS_PI = 2.0 * math.pi / 3.0

# ================================================================= Transformadas (funciones)


def clarke(abc):
    """Transforma las magnitudes trifásicas abc a las componentes (α, β) de Clarke."""
    a, b, c = abc
    return (2.0 / 3.0) * (a - 0.5 * b - 0.5 * c), (b - c) / RAIZ3


def clarke_inv(alfa, beta):
    """Transforma las componentes (α, β) de Clarke a magnitudes trifásicas abc."""
    return np.array([alfa, -0.5 * alfa + 0.5 * RAIZ3 * beta, -0.5 * alfa - 0.5 * RAIZ3 * beta])


def park(alfa, beta, th):
    """Transforma (α, β) al marco giratorio (d, q) para el ángulo th (rad)."""
    c, s = math.cos(th), math.sin(th)
    return alfa * c + beta * s, -alfa * s + beta * c


def park_inv(d, q, th):
    """Transforma (d, q) al marco estacionario (α, β) para el ángulo th (rad)."""
    c, s = math.cos(th), math.sin(th)
    return d * c - q * s, d * s + q * c


def tiempos_svpwm(alfa, beta, vdc):
    """Sector y tiempos relativos del vector espacial.

    t1 = (√3·|v|/Vdc)·sin(60° - γ);   t2 = (√3·|v|/Vdc)·sin(γ);   t0 = 1 - t1 - t2
    donde γ es el ángulo dentro del sector. Se devuelve (sector 1..6, t1, t2, t0).
    """
    ang = math.atan2(beta, alfa) % (2 * math.pi)
    sector = int(ang / (math.pi / 3.0)) % 6
    gamma = ang - sector * math.pi / 3.0
    mod = math.hypot(alfa, beta) * RAIZ3 / vdc
    t1 = mod * math.sin(math.pi / 3.0 - gamma)
    t2 = mod * math.sin(gamma)
    return sector + 1, t1, t2, 1.0 - t1 - t2


def ciclos_svpwm(alfa, beta, vdc):
    """Ciclo de trabajo de cada rama con siete segmentos centrados (t0 repartido por igual)."""
    sector, t1, t2, t0 = tiempos_svpwm(alfa, beta, vdc)
    vecinos = {1: ((1, 0, 0), (1, 1, 0)), 2: ((1, 1, 0), (0, 1, 0)), 3: ((0, 1, 0), (0, 1, 1)),
               4: ((0, 1, 1), (0, 0, 1)), 5: ((0, 0, 1), (1, 0, 1)), 6: ((1, 0, 1), (1, 0, 0))}[sector]
    return np.array([0.5 * t0 + t1 * vecinos[0][k] + t2 * vecinos[1][k] for k in range(3)])


def portadora_triangular(t, fc):
    """Portadora triangular entre -1 y +1, con periodo 1/fc y máximo en t = 0.

    Así, comparar m >= c da un pulso centrado en la mitad de cada periodo.
    """
    fase = (t * fc) % 1.0
    return 1.0 - 4.0 * fase if fase < 0.5 else 4.0 * fase - 3.0


class _Rama:
    """Una salida de modulador con flancos exactos: se calcula el instante, no se compara.

    Con T = 1/fc, base = n·T y D recortado a [1/res, 1 - 1/res]:
        centrado  sube en base + T·(1-D)/2,  baja en base + T·(1+D)/2
        bajada    sube en base,              baja en base + T·D
        subida    sube en base + T·(1-D),    baja en base + T
    D se lee de nuevo en cada flanco, de modo que un periodo tiene como mucho un pulso.
    """

    def __init__(self, modo, T, res):
        self.modo, self.T, self.res = modo, T, res
        self.n = None
        self.fase = 2             # 0: espera subir; 1: espera bajar; 2: espera el nuevo periodo
        self.g = 0.0
        self.siguiente = -math.inf

    def _subida(self, base, D):
        if self.modo == "centrado":
            return base + 0.5 * self.T * (1.0 - D)
        return base if self.modo == "bajada" else base + self.T * (1.0 - D)

    def _bajada(self, base, D):
        if self.modo == "centrado":
            return base + 0.5 * self.T * (1.0 + D)
        return base + self.T * D if self.modo == "bajada" else base + self.T

    def avanzar(self, t, d):
        """Aplica los flancos que ya han llegado. True si cambió la salida."""
        T = self.T
        if self.n is None:
            self.n = math.floor(t / T) - 1
        tol = 8.0 * np.spacing(abs(t) + T)
        D = min(1.0 - 1.0 / self.res, max(1.0 / self.res, d))
        previo = self.g
        while self.siguiente <= t + tol:
            if self.fase == 2:
                self.n += 1
                self.fase = 0
                self.siguiente = self._subida(self.n * T, D)
            elif self.fase == 0:
                self.g, self.fase = 1.0, 1
                self.siguiente = self._bajada(self.n * T, D)
            else:
                self.g, self.fase = 0.0, 2
                self.siguiente = (self.n + 1) * T
        return self.g != previo


def thd_de(senal, fs, f0, armonicos=50):
    """THD = √(Σ_{k=2..N} V_k²) / V_1, por FFT de una ventana entera de periodos.

    Devuelve (thd en por unidad, amplitud de la fundamental).
    """
    senal = np.asarray(senal, dtype=float)
    por_periodo = fs / f0
    periodos = int(senal.size / por_periodo)
    if periodos < 1:
        raise ValueError("La ventana debe contener al menos un periodo")
    n = int(round(periodos * por_periodo))
    F = np.fft.rfft(senal[-n:]) * 2.0 / n
    k1 = periodos
    v1 = abs(F[k1])
    idx = [k1 * k for k in range(2, armonicos + 1) if k1 * k < F.size]
    return float(np.sqrt(np.sum(np.abs(F[idx]) ** 2)) / v1), float(v1)


def thd_total(senal, fs, f0):
    """THD con todo el contenido no fundamental: √(V_eficaz² - V1_eficaz²)/V1_eficaz."""
    senal = np.asarray(senal, dtype=float)
    por_periodo = fs / f0
    periodos = int(senal.size / por_periodo)
    n = int(round(periodos * por_periodo))
    x = senal[-n:]
    x = x - x.mean()
    F = np.fft.rfft(x) * 2.0 / n
    v1 = abs(F[periodos])
    return float(np.sqrt(max(np.mean(x**2) / (v1**2 / 2.0) - 1.0, 0.0))), float(v1)


def thd_analitica_linea(m):
    """THD de la tensión de línea con SPWM: √(8/(√3·π·m) - 1) (tensión en vacío)."""
    return math.sqrt(8.0 / (RAIZ3 * math.pi * m) - 1.0)


def perdidas_analiticas(I, m, cos_fi, vdc, fsw, p):
    """Pérdidas de un inversor de dos niveles con SPWM (fórmulas de valor medio).

    Por IGBT:   P_c = Vce0·I·(1/(2π) + m·cosφ/8) + Ron·I²·(1/8 + m·cosφ/(3π))
    Por diodo:  P_c = Vf·I·(1/(2π) - m·cosφ/8) + Rd·I²·(1/8 - m·cosφ/(3π))
    Conmutación por IGBT: (Eon + Eoff)·fsw·(1/π)·(I/Iref)·(Vdc/Vref)
    Conmutación por diodo: Err·fsw·(1/π)·(I/Iref)·(Vdc/Vref)
    I es la corriente de pico. Devuelve las pérdidas totales de los seis pares.
    """
    pc_t = p["Vce0"] * I * (1 / (2 * math.pi) + m * cos_fi / 8) + p["Ron"] * I**2 * (1 / 8 + m * cos_fi / (3 * math.pi))
    pc_d = p["Vf"] * I * (1 / (2 * math.pi) - m * cos_fi / 8) + p["Rd"] * I**2 * (1 / 8 - m * cos_fi / (3 * math.pi))
    esc = fsw * (1 / math.pi) * (I / p["Iref"]) * (vdc / p["Vref"])
    ps_t = (p["Eon"] + p["Eoff"]) * esc
    ps_d = p["Err"] * esc
    return {"cond": 6 * (pc_t + pc_d), "sw": 6 * (ps_t + ps_d), "total": 6 * (pc_t + pc_d + ps_t + ps_d)}


# ================================================================= Convertidores: elevador


@registrar
class BoostPromediado(Elemento):
    """Elevador DC-DC promediado.

    L·di/dt = v_in - (1 - d)·v_out - R·i
    i_entrada = i,   i_salida = (1 - d)·i       d en [0, 1]
    """
    tipo = "boost_prom"; nombre = "Boost promediado"; categoria = "Potencia · Convertidores"
    parametros = [P("L", "Inductancia", 1e-3, "H"), P("R", "Resistencia serie", 0.01, "Ω"), P("i0", "Corriente inicial", 0.0, "A")]
    puertos_e = ["in", "out", "com"]
    entradas = ["d"]
    salidas = ["i_L"]

    @override
    def n_estados(self):
        return 1

    @override
    def x0(self):
        return [float(self.p["i0"])]

    @override
    def rhs(self, b, t, x, u):
        i = x[0]
        d = min(1.0, max(0.0, float(u[0])))
        b[self.n["in"]] -= i
        b[self.n["com"]] += d * i
        b[self.n["out"]] += (1.0 - d) * i

    @override
    def derivada_e(self, z, t, x, u):
        d = min(1.0, max(0.0, float(u[0])))
        c = z[self.n["com"]]
        return [((z[self.n["in"]] - c) - (1.0 - d) * (z[self.n["out"]] - c) - float(self.p["R"]) * x[0]) / float(self.p["L"])]

    @override
    def salida_e(self, z, t, x, u):
        return [x[0]]


@registrar
class BoostConmutado(Elemento):
    """Elevador DC-DC conmutado con transistor y diodo.

    L·di/dt = v_in - v_m - R·i
    g = 1: transistor conduce, v_m = Ron·i
    g = 0: el diodo conduce si i > 0, v_m = v_out + Vf + Rd·i
    Si la corriente se anula, el diodo se abre (conducción discontinua) y la corriente
    residual se extingue con constante de tiempo TAU_DCM = 10 µs: di/dt = -i/TAU_DCM.
    """
    TAU_DCM = 1e-5
    tipo = "boost_conm"; nombre = "Boost conmutado"; categoria = "Potencia · Convertidores"
    parametros = [P("L", "Inductancia", 1e-3, "H"), P("R", "Resistencia serie", 0.01, "Ω"),
                  P("Ron", "Resistencia del transistor", 0.01, "Ω"), P("Rd", "Resistencia del diodo", 0.01, "Ω"),
                  P("Vf", "Tensión del diodo", 0.8, "V"), P("i0", "Corriente inicial", 0.0, "A")]
    puertos_e = ["in", "out", "com"]
    entradas = ["g"]
    salidas = ["i_L"]
    entradas_compuerta = ("g",)
    con_estado_discreto = True
    n_internos = 1

    @override
    def preparar(self):
        self.t_on = False
        self.d_on = False

    @override
    def reiniciar_e(self):
        self.preparar()

    @override
    def n_estados(self):
        return 1

    @override
    def x0(self):
        return [float(self.p["i0"])]

    @override
    def clave(self):
        return (self.t_on, self.d_on)

    @override
    def estampar_estado(self, S):
        m = self.internos[0]
        S.g(m, self.n["com"], 1.0 / float(self.p["Ron"]) if self.t_on else 1e-6)
        S.g(m, self.n["out"], 1.0 / float(self.p["Rd"]) if self.d_on else 1e-6)

    @override
    def rhs(self, b, t, x, u):
        i = x[0]
        m = self.internos[0]
        b[self.n["in"]] -= i
        b[m] += i
        if self.d_on:
            i0 = float(self.p["Vf"]) / float(self.p["Rd"])
            b[m] += i0
            b[self.n["out"]] -= i0

    @override
    def derivada_e(self, z, t, x, u):
        if not self.t_on and not self.d_on:
            # Conducción discontinua: ambos abiertos, la corriente residual se extingue
            return [-x[0] / self.TAU_DCM]
        return [(z[self.n["in"]] - z[self.internos[0]] - float(self.p["R"]) * x[0]) / float(self.p["L"])]

    @override
    def actualizar(self, z, t, x, u):
        g = float(u[0]) >= 0.5
        cambio = g != self.t_on
        self.t_on = g
        m, o = self.internos[0], self.n["out"]
        v = z[m] - z[o]
        vf = float(self.p["Vf"])
        if self.d_on:
            if (v - vf) / float(self.p["Rd"]) < 0.0:
                self.d_on = False
                cambio = True
        elif not self.t_on and (x[0] > 1e-6 and v > vf or z[self.n["in"]] - z[o] > vf + 1e-9):
            self.d_on = True
            cambio = True
        return cambio

    @override
    def cruce_e(self, z, t, x, u):
        # El diodo se abre cuando la corriente de la bobina se anula
        return [float(x[0])] if self.d_on else [1.0]

    @override
    def salida_e(self, z, t, x, u):
        return [x[0]]


# ================================================================= Puente H e inversor


class _Ramas(Elemento, _ParametrosIGBT):
    """Base de los convertidores conmutados: pares de IGBT por rama.

    Pérdidas de conducción: ∫v·i dt en cada dispositivo. Conmutación, por evento:
    E = E_ref·(|i|/I_ref)·(|V_bus|/V_ref).  Salidas de energía acumulada [J].
    """
    con_estado_discreto = True
    entradas_compuerta = ("s",)

    def _crear(self, nodos_rama, dcp, dcn):
        p = self.p
        par = (float(p["Ron"]), float(p["Vce0"]), float(p["Rd"]), float(p["Vf"]))
        self.dev = []
        for a in nodos_rama:
            self.dev.append(DispositivoIGBT(dcp, a, *par))
            self.dev.append(DispositivoIGBT(a, dcn, *par))
        self.dcp, self.dcn = dcp, dcn
        self.st_ant = [0] * len(self.dev)
        self.i_ant = [0.0] * len(self.dev)
        self.Ec = self.Es = 0.0
        self.t_ant = 0.0
        self.s_ant = [None] * len(nodos_rama)
        self.t_cambio = [-1.0] * len(nodos_rama)

    def clave(self):
        return tuple(d.st for d in self.dev)

    def estampar_estado(self, S):
        for d in self.dev:
            S.g(d.c, d.e, d.conductancia())

    def rhs(self, b, t, x, u):
        for d in self.dev:
            d.inyeccion(b)

    def _ordenes(self, u):
        return np.ravel(u[0])

    def actualizar(self, z, t, x, u):
        s = self._ordenes(u)
        td = float(self.p.get("t_muerto", 0.0))
        for k in range(len(self.dev) // 2):
            v = float(s[k]) >= 0.5
            if self.s_ant[k] is None:
                self.s_ant[k] = v
            elif v != self.s_ant[k]:
                self.s_ant[k] = v
                self.t_cambio[k] = t
            listo = (t - self.t_cambio[k]) >= td - 1e-15
            self.dev[2 * k].g = v and listo
            self.dev[2 * k + 1].g = (not v) and listo
        c = False
        for d in self.dev:
            if d.revisar(z):
                c = True
        return c

    def tras_paso(self, z, t, x, u):
        dt = t - self.t_ant
        self.t_ant = t
        p = self.p
        vbus = abs(z[self.dcp] - z[self.dcn])
        escala_v = vbus / float(p["Vref"])
        Iref = float(p["Iref"])
        if getattr(self, "termico", False):
            self._perdidas_por_union(z, dt, escala_v, Iref)
            return
        for k, d in enumerate(self.dev):
            i = d.corriente(z)
            if d.st:
                self.Ec += (z[d.c] - z[d.e]) * i * dt
            st0 = self.st_ant[k]
            if st0 != d.st:
                if d.st == 1:
                    self.Es += float(p["Eon"]) * abs(i) / Iref * escala_v
                elif st0 == 1:
                    self.Es += float(p["Eoff"]) * abs(self.i_ant[k]) / Iref * escala_v
                elif st0 == 2:
                    self.Es += float(p["Err"]) * abs(self.i_ant[k]) / Iref * escala_v
                self.st_ant[k] = d.st
            self.i_ant[k] = i

    def _perdidas_por_union(self, z, dt, escala_v, Iref):
        """Acumula las mismas pérdidas y apunta además las de conmutación en su unión.

        La unión 2k corresponde al IGBT del dispositivo k y la 2k+1 a su diodo. El solver
        integra la conducción (`potencia_conduccion`).
        """
        p = self.p
        for k, d in enumerate(self.dev):
            i = d.corriente(z)
            if d.st:
                self.Ec += (z[d.c] - z[d.e]) * i * dt
            st0 = self.st_ant[k]
            if st0 != d.st:
                if d.st == 1:
                    nombre, j, ic = "Eon", 2 * k, i
                elif st0 == 1:
                    nombre, j, ic = "Eoff", 2 * k, self.i_ant[k]
                elif st0 == 2:
                    nombre, j, ic = "Err", 2 * k + 1, self.i_ant[k]
                else:
                    nombre = None
                if nombre:
                    e = float(p[nombre]) * self.factor_energia(j, abs(ic) / Iref) * escala_v
                    self.Es += e
                    self.E_union[j] += e
                self.st_ant[k] = d.st
            self.i_ant[k] = i


@registrar
class InversorConmutado(UnionesTermicas, _Ramas):
    """Inversor trifásico de dos niveles, conmutado.

    Cada rama: dos IGBT con diodo antiparalelo. Entrada s = [s_a, s_b, s_c] (1 arriba, 0
    abajo), con tiempo muerto opcional. Salidas de energía de conducción y conmutación [J] y
    temperaturas de unión Tj_q (6 IGBT) y Tj_d (6 diodos) [°C], en el orden a+, a-, b+, b-, c+, c-.

    Puerto térmico opcional «th» (cápsula común del módulo): si se conecta, cada IGBT y cada diodo
    tienen su propia unión con su red Foster, reciben sus propias pérdidas (conducción v·i;
    E_on y E_off en el IGBT, E_rr en el diodo) y sus parámetros siguen a su temperatura, como en
    el bloque `igbt`. Sin puerto térmico se comporta como antes, a T_ref.
    """
    tipo = "inversor_conm"; nombre = "Inversor conmutado"; categoria = "Potencia · Convertidores"
    parametros = _ParametrosIGBT.PARAMS + [P("t_muerto", "Tiempo muerto", 0.0, "s")] + parametros_union("qd")
    puertos_e = ["dc_p", "dc_n", "th"]
    puertos_e3 = ["ac"]
    entradas = ["s"]
    entradas_vector = {"s": 3}
    salidas = ["E_cond", "E_sw", "Tj_q", "Tj_d"]

    @override
    def preparar(self):
        self.dev = None
        self.preparar_uniones(["q", "d"] * 6)

    @override
    def reiniciar_e(self):
        self._crear(self.n["ac"], self.n["dc_p"], self.n["dc_n"])
        self.reiniciar_uniones()

    @override
    def clave(self):
        if self.dev is None:
            self.reiniciar_e()
        return _Ramas.clave(self)

    @override
    def estampar_fijo(self, S):
        self.estampar_uniones(S)

    @override
    def rhs(self, b, t, x, u):
        _Ramas.rhs(self, b, t, x, u)
        self.rhs_uniones(b, x)

    @override
    def derivada_e(self, z, t, x, u):
        return self.derivada_uniones(z, x)

    def potencia_conduccion(self, z):
        """Devuelve la lista plana de pérdidas de conducción (W), IGBT y diodo de cada dispositivo."""
        p = []
        for d in self.dev:
            v = (z[d.c] - z[d.e]) * d.corriente(z) if d.st else 0.0
            p += (v, 0.0) if d.st == 1 else (0.0, v)
        return p

    @override
    def tras_paso(self, z, t, x, u):
        _Ramas.tras_paso(self, z, t, x, u)

        def aplicar(Tj):
            for k, d in enumerate(self.dev):
                aplicar_temperatura_igbt(self, d, Tj[2 * k], Tj[2 * k + 1])
        return self.revisar_temperaturas(z, aplicar)

    @override
    def salida_e(self, z, t, x, u):
        Tj = self.temperaturas(z)
        return [self.Ec, self.Es, Tj[0::2].copy(), Tj[1::2].copy()]


@registrar
class PuenteHConmutado(_Ramas):
    """Puente H conmutado (dos ramas con IGBT y diodo).   v_ab = (s_a - s_b)·V_bus."""
    tipo = "puente_h_conm"; nombre = "Puente H conmutado"; categoria = "Potencia · Convertidores"
    parametros = _ParametrosIGBT.PARAMS + [P("t_muerto", "Tiempo muerto", 0.0, "s")]
    puertos_e = ["dc_p", "dc_n", "a", "b"]
    entradas = ["s_a", "s_b"]
    entradas_compuerta = ("s_a", "s_b")
    salidas = ["E_cond", "E_sw"]

    @override
    def preparar(self):
        self.dev = None

    @override
    def reiniciar_e(self):
        self._crear((self.n["a"], self.n["b"]), self.n["dc_p"], self.n["dc_n"])

    @override
    def clave(self):
        if self.dev is None:
            self.reiniciar_e()
        return _Ramas.clave(self)

    def _ordenes(self, u):
        return [u[0], u[1]]

    @override
    def salida_e(self, z, t, x, u):
        return [self.Ec, self.Es]


@registrar
class PuenteHPromediado(Elemento):
    """Puente H promediado.   v_a = d_a·V_bus,  v_b = d_b·V_bus;  i_bus = d_a·i_a + d_b·i_b."""
    tipo = "puente_h_prom"; nombre = "Puente H promediado"; categoria = "Potencia · Convertidores"
    puertos_e = ["dc_p", "dc_n", "a", "b"]
    entradas = ["d_a", "d_b"]
    dinamico = True
    n_ramas = 2

    @override
    def estampar_din(self, M, b, z, t, x, u):
        dp, dn = self.n["dc_p"], self.n["dc_n"]
        for k, puerto in enumerate(("a", "b")):
            r = self.ramas[k]
            d = min(1.0, max(0.0, float(u[k])))
            a = self.n[puerto]
            M[r, a] += 1.0; M[r, dp] -= d; M[r, dn] -= 1.0 - d
            M[a, r] += 1.0; M[dp, r] -= d; M[dn, r] -= 1.0 - d


@registrar
class InversorPromediado(Elemento, _ParametrosIGBT):
    """Inversor trifásico de dos niveles, promediado.

    v_x = v_dc- + d_x·V_bus    (d_x: ciclo de trabajo de la rama x)
    i_bus = Σ d_x·i_x + P_pérdidas/V_bus

    Pérdidas instantáneas por rama (corriente i, ciclo d):
      conducción, con el dispositivo superior o inferior según el signo de la corriente:
          p_c = d·(V0_a·|i| + R_a·i²) + (1-d)·(V0_b·|i| + R_b·i²)
      conmutación: p_s = fsw·(Eon + Eoff + Err)·(|i|/Iref)·(V_bus/Vref)
    Salidas: pérdidas de conducción y de conmutación [W].
    """
    tipo = "inversor_prom"; nombre = "Inversor promediado"; categoria = "Potencia · Convertidores"
    parametros = _ParametrosIGBT.PARAMS + [P("fsw", "Frecuencia de conmutación", 5000.0, "Hz")]
    puertos_e = ["dc_p", "dc_n"]
    puertos_e3 = ["ac"]
    entradas = ["d"]
    entradas_vector = {"d": 3}
    salidas = ["P_cond", "P_sw"]
    dinamico = True
    n_ramas = 3

    @override
    def estampar_din(self, M, b, z, t, x, u):
        dp, dn = self.n["dc_p"], self.n["dc_n"]
        d = np.clip(np.ravel(u[0]), 0.0, 1.0) if not np.isscalar(u[0]) else np.zeros(3) + 0.5
        for f in range(3):
            r = self.ramas[f]
            a = self.n["ac"][f]
            dx = float(d[f])
            M[r, a] += 1.0; M[r, dp] -= dx; M[r, dn] -= 1.0 - dx
            M[a, r] += 1.0; M[dp, r] -= dx; M[dn, r] -= 1.0 - dx
        pc, ps = self._perdidas(z, d)
        vb = z[dp] - z[dn]
        if vb > 1.0:
            ip = (pc + ps) / vb
            b[dp] -= ip
            b[dn] += ip

    def _perdidas(self, z, d):
        p = self.p
        Ron, V0, Rd, Vf = float(p["Ron"]), float(p["Vce0"]), float(p["Rd"]), float(p["Vf"])
        esc = float(p["fsw"]) * (float(p["Eon"]) + float(p["Eoff"]) + float(p["Err"])) / float(p["Iref"]) \
            * abs(z[self.n["dc_p"]] - z[self.n["dc_n"]]) / float(p["Vref"])
        pc = ps = 0.0
        for f in range(3):
            # i_salida > 0: conducen el IGBT superior (d) y el diodo inferior (1-d)
            i = -z[self.ramas[f]]
            ai = abs(i)
            dx = float(d[f])
            if i >= 0:
                pc += dx * (V0 * ai + Ron * i * i) + (1 - dx) * (Vf * ai + Rd * i * i)
            else:
                pc += dx * (Vf * ai + Rd * i * i) + (1 - dx) * (V0 * ai + Ron * i * i)
            ps += esc * ai
        return pc, ps

    @override
    def salida_e(self, z, t, x, u):
        d = np.clip(np.ravel(u[0]), 0.0, 1.0) if not np.isscalar(u[0]) else np.zeros(3) + 0.5
        return list(self._perdidas(z, d))


# ================================================================= Filtros y transformador trifásico


class _Trifasico3Hilos(Elemento):
    """Cálculo común: tensiones de fase quitando el neutro virtual (3 hilos)."""

    @staticmethod
    def fases(z, nodos):
        v = np.array([z[nodos[0]], z[nodos[1]], z[nodos[2]]])
        return v - v.mean()


@registrar
class FiltroL(_Trifasico3Hilos):
    """Filtro L trifásico.   L·di/dt = v_a - v_b - R·i   (3 hilos; estados i_a, i_b)."""
    tipo = "filtro_l"; nombre = "Filtro L"; categoria = "Potencia · Filtros"
    parametros = [P("L", "Inductancia", 1e-3, "H"), P("R", "Resistencia serie", 0.01, "Ω")]
    puertos_e3 = ["a", "b"]
    salidas = ["i_abc"]

    @override
    def preparar(self):
        self.Lt, self.Rt = float(self.p["L"]), float(self.p["R"])

    def absorber(self, L, R):
        """Suma una inductancia L (H) y una resistencia R (Ω) en serie al filtro."""
        self.Lt += L
        self.Rt += R

    @override
    def n_estados(self):
        return 2

    @override
    def x0(self):
        return [0.0, 0.0]

    @override
    def rhs(self, b, t, x, u):
        na, nb = self.n["a"], self.n["b"]
        i = (x[0], x[1], -x[0] - x[1])
        for f in range(3):
            b[na[f]] -= i[f]
            b[nb[f]] += i[f]

    def i2_abc(self, x):
        """Devuelve las tres corrientes de fase [i_a, i_b, i_c] (A) a partir de los estados."""
        return np.array([x[0], x[1], -x[0] - x[1]])

    def di2(self, z, x):
        """Devuelve las derivadas de las tres corrientes de fase (A/s)."""
        v = self.fases(z, self.n["a"]) - self.fases(z, self.n["b"])
        d = (v[:2] - self.Rt * x[:2]) / self.Lt
        return np.array([d[0], d[1], -d[0] - d[1]])

    @override
    def derivada_e(self, z, t, x, u):
        return self.di2(z, x)[:2]

    @override
    def salida_e(self, z, t, x, u):
        return [self.i2_abc(x)]


@registrar
class FiltroLC(_Trifasico3Hilos):
    """Filtro LC trifásico con condensador en estrella (neutro aislado).

    L·di/dt = v_a - v_c - R·i;     C·dv_c/dt = i - i_b
    La tensión del puerto b la fija el condensador. Salidas: i_abc y v_c.
    """
    tipo = "filtro_lc"; nombre = "Filtro LC"; categoria = "Potencia · Filtros"
    parametros = [P("L", "Inductancia", 1e-3, "H"), P("R", "Resistencia serie", 0.01, "Ω"), P("C", "Capacidad", 20e-6, "F")]
    puertos_e3 = ["a", "b"]
    salidas = ["i_abc", "v_c"]
    n_ramas = 3
    n_internos = 1

    @override
    def n_estados(self):
        return 4

    @override
    def x0(self):
        return [0.0] * 4

    @override
    def estados_en_rama(self):
        """Las tensiones v_a y v_b del condensador fijan sus ramas; la tercera es la restricción."""
        return {2: 0, 3: 1}

    @override
    def estampar_fijo(self, S):
        N = self.internos[0]
        for f in range(3):
            S.rama_tension(self.ramas[f], self.n["b"][f], N)

    @override
    def rhs(self, b, t, x, u):
        i = (x[0], x[1], -x[0] - x[1])
        v = (x[2], x[3], -x[2] - x[3])
        for f in range(3):
            b[self.n["a"][f]] -= i[f]
            b[self.n["b"][f]] += i[f]
            b[self.ramas[f]] = v[f]

    @override
    def derivada_e(self, z, t, x, u):
        va = self.fases(z, self.n["a"])
        L, R, C = float(self.p["L"]), float(self.p["R"]), float(self.p["C"])
        return [(va[0] - x[2] - R * x[0]) / L, (va[1] - x[3] - R * x[1]) / L,
                z[self.ramas[0]] / C, z[self.ramas[1]] / C]

    @override
    def salida_e(self, z, t, x, u):
        return [np.array([x[0], x[1], -x[0] - x[1]]), np.array([x[2], x[3], -x[2] - x[3]])]


@registrar
class FiltroLCL(_Trifasico3Hilos):
    """Filtro LCL trifásico con amortiguamiento pasivo (3 hilos; 6 estados).

    L1·di1/dt = v_a - v_n - R1·i1
    L2·di2/dt = v_n - v_b - R2·i2
    C·dv_c/dt = i1 - i2;     v_n = v_c + Rd·(i1 - i2)

    Resonancia: f_res = (1/2π)·√((L1 + L2)/(L1·L2·C)).
    Si se conecta directamente a la Red (barra infinita), las inductancias de red y de L2
    quedan en serie: se suman en un solo estado, como en el circuito real.
    Salidas: i1_abc, i2_abc, v_c.
    """
    tipo = "filtro_lcl"; nombre = "Filtro LCL"; categoria = "Potencia · Filtros"
    parametros = [P("L1", "L lado inversor", 0.5e-3, "H"), P("R1", "R lado inversor", 0.01, "Ω"),
                  P("L2", "L lado red", 0.1e-3, "H"), P("R2", "R lado red", 0.005, "Ω"),
                  P("C", "Capacidad", 50e-6, "F"), P("Rd", "Resistencia de amortiguamiento", 0.5, "Ω")]
    puertos_e3 = ["a", "b"]
    salidas = ["i1_abc", "i2_abc", "v_c"]

    @override
    def preparar(self):
        p = self.p
        self.L1, self.R1 = float(p["L1"]), float(p["R1"])
        self.L2, self.R2 = float(p["L2"]), float(p["R2"])
        self.C, self.Rd = float(p["C"]), float(p["Rd"])
        self.L2o, self.R2o = self.L2, self.R2

    def absorber(self, L, R):
        """Suma una inductancia L (H) y una resistencia R (Ω) en serie al lado de red."""
        self.L2 = self.L2o + L
        self.R2 = self.R2o + R

    @override
    def n_estados(self):
        return 6

    @override
    def x0(self):
        return [0.0] * 6

    @override
    def rhs(self, b, t, x, u):
        i1 = (x[0], x[1], -x[0] - x[1])
        i2 = (x[2], x[3], -x[2] - x[3])
        for f in range(3):
            b[self.n["a"][f]] -= i1[f]
            b[self.n["b"][f]] += i2[f]

    def i2_abc(self, x):
        """Devuelve las tres corrientes de fase del lado de red (A) a partir de los estados."""
        return np.array([x[2], x[3], -x[2] - x[3]])

    def di2(self, z, x):
        """Devuelve las derivadas de las tres corrientes de fase del lado de red (A/s)."""
        vb = self.fases(z, self.n["b"])
        vn = x[4:6] + self.Rd * (x[0:2] - x[2:4])
        d = (vn - vb[:2] - self.R2 * x[2:4]) / self.L2
        return np.array([d[0], d[1], -d[0] - d[1]])

    @override
    def derivada_e(self, z, t, x, u):
        va = self.fases(z, self.n["a"])
        vb = self.fases(z, self.n["b"])
        i1, i2, vc = x[0:2], x[2:4], x[4:6]
        vn = vc + self.Rd * (i1 - i2)
        return np.concatenate(((va[:2] - vn - self.R1 * i1) / self.L1,
                               (vn - vb[:2] - self.R2 * i2) / self.L2,
                               (i1 - i2) / self.C))

    @override
    def salida_e(self, z, t, x, u):
        return [np.array([x[0], x[1], -x[0] - x[1]]), np.array([x[2], x[3], -x[2] - x[3]]),
                np.array([x[4], x[5], -x[4] - x[5]])]


@registrar
class Transformador3(_Trifasico3Hilos):
    """Transformador trifásico (circuito equivalente por fase, 3 hilos). a = V1/V2 de fase.

    L·di_2/dt = v_1/a - v_2 - R·i_2;     i_1 = i_2/a + i_m;     Lm·di_m/dt = v_1
    (R y L referidas al lado 2.) Estados: i_2 e i_m de dos fases.
    """
    tipo = "transformador3"; nombre = "Transformador trifásico"; categoria = "Potencia · Filtros"
    parametros = [P("a", "Relación V1/V2", 2.0), P("L", "Dispersión (lado 2)", 1e-4, "H"),
                  P("R", "Resistencia (lado 2)", 0.005, "Ω"), P("Lm", "Inductancia magnetizante", 1.0, "H")]
    puertos_e3 = ["p", "s"]
    salidas = ["i2_abc"]

    @override
    def n_estados(self):
        return 4

    @override
    def x0(self):
        return [0.0] * 4

    @override
    def rhs(self, b, t, x, u):
        a = float(self.p["a"])
        i2 = (x[0], x[1], -x[0] - x[1])
        im = (x[2], x[3], -x[2] - x[3])
        for f in range(3):
            b[self.n["p"][f]] -= i2[f] / a + im[f]
            b[self.n["s"][f]] += i2[f]

    @override
    def derivada_e(self, z, t, x, u):
        p = self.p
        a, L, R, Lm = float(p["a"]), float(p["L"]), float(p["R"]), float(p["Lm"])
        v1, v2 = self.fases(z, self.n["p"]), self.fases(z, self.n["s"])
        return [(v1[0] / a - v2[0] - R * x[0]) / L, (v1[1] / a - v2[1] - R * x[1]) / L, v1[0] / Lm, v1[1] / Lm]

    @override
    def salida_e(self, z, t, x, u):
        return [np.array([x[0], x[1], -x[0] - x[1]])]


# ================================================================= Modulación


class _Modulador(Bloque):
    """Base de los moduladores con generación de flancos.

    Con paso variable el modulador calcula sus flancos y el solver cae exactamente en ellos,
    de modo que el ciclo de trabajo no depende del paso. Con paso fijo compara en cada muestra.
    """
    ramas_n = 1
    de_evento = True

    def preparar(self):
        self.ramas = None

    def reiniciar(self):
        self.ramas = None

    def _T(self):
        return 1.0 / float(self.p["fc"])

    def _modo(self):
        return "centrado"

    def _ciclos(self, u):
        raise NotImplementedError

    def proximo_evento(self, t):
        if self.ramas is not None:
            return min(r.siguiente for r in self.ramas)
        return math.inf

    def paso_mayor(self, t, x, u, y):
        if not self.usar_zc:
            return False
        d = self._ciclos(u)
        if self.ramas is None:
            self.ramas = [_Rama(self._modo(), self._T(), float(self.p["res"])) for _ in d]
        cambio = False
        for r, v in zip(self.ramas, d):
            cambio = r.avanzar(t, float(v)) or cambio
        return cambio

    def _latidos(self):
        return np.array([r.g for r in self.ramas]) if self.ramas is not None else np.zeros(self.ramas_n)


_P_RES = P("res", "Resolución del ciclo de trabajo", 1e4, "",
           ayuda="El ciclo se recorta a [1/res, 1 - 1/res]: siempre hay un pulso mínimo.")


@registrar
class PWM(_Modulador):
    """PWM.   Con T = 1/fc y D = d recortado a [1/res, 1 - 1/res], en cada periodo:.

        centrado  g = 1 entre T·(1-D)/2 y T·(1+D)/2
        bajada    g = 1 entre 0 y T·D
        subida    g = 1 entre T·(1-D) y T

    Con Dormand-Prince o BDF el solver cae en cada flanco y el ciclo de trabajo es exacto para
    cualquier tolerancia. Con Euler o RK4 la compuerta se lee en cada muestra: el ciclo tiene una
    resolución de fc·h; usa un paso que dé un número par de muestras por periodo, 50 o más.
    """
    tipo = "pwm"; nombre = "PWM"; categoria = "Potencia · Modulación"
    parametros = [P("fc", "Frecuencia de portadora", 10000.0, "Hz"),
                  P("modo", "Alineación del pulso", "centrado", "", "opcion", ["centrado", "bajada", "subida"]),
                  _P_RES]
    entradas = ["d"]; salidas = ["g"]

    def _modo(self):
        return str(self.p.get("modo", "centrado"))

    def _ciclos(self, u):
        return [float(u[0])]

    @override
    def salida(self, t, x, u):
        if self.usar_zc:
            return [float(self._latidos()[0])]
        d, fc = float(u[0]), float(self.p["fc"])
        modo = self._modo()
        if modo == "centrado":
            return [1.0 if 2.0 * d - 1.0 >= portadora_triangular(t, fc) else 0.0]
        fase = (t * fc) % 1.0
        return [1.0 if (fase < d if modo == "bajada" else fase >= 1.0 - d) else 0.0]


class _Modulador3(_Modulador):
    ramas_n = 3

    def _moduladora(self, u):
        return np.ravel(u[0]) if not np.isscalar(u[0]) else np.zeros(3)

    def _ciclos(self, u):
        return 0.5 * (1.0 + np.clip(self._moduladora(u), -1, 1))

    def salida(self, t, x, u):
        m = self._moduladora(u)
        d = 0.5 * (1.0 + np.clip(m, -1, 1))
        if self.usar_zc:
            return [self._latidos(), d]
        return [(m >= portadora_triangular(t, float(self.p["fc"]))).astype(float), d]


@registrar
class SPWM(_Modulador3):
    """SPWM trifásica.   s_x = 1 si m_x >= c(t)   (m_x: moduladora en por unidad de Vdc/2).

    c(t) es triangular y su máximo cae al inicio de cada periodo, así que el pulso queda centrado.
    Con paso variable se usan los flancos exactos de la PWM centrada con d_x = (1 + m_x)/2.
    """
    tipo = "spwm"; nombre = "SPWM"; categoria = "Potencia · Modulación"
    parametros = [P("fc", "Frecuencia de portadora", 5000.0, "Hz"), _P_RES]
    entradas = ["m"]; salidas = ["s", "d"]


@registrar
class SVPWM(_Modulador3):
    """SVPWM trifásica (equivalente a inyectar secuencia cero min-max).

    m'_x = m_x + v0,   v0 = -(max(m) + min(m))/2;   s_x = 1 si m'_x >= c(t)
    Es igual a los siete segmentos centrados del vector espacial, y permite modular hasta
    |v| = Vdc/√3 (15.5 % más que SPWM). Salidas: órdenes s y ciclos de trabajo d.
    """
    tipo = "svpwm"; nombre = "SVPWM"; categoria = "Potencia · Modulación"
    parametros = [P("fc", "Frecuencia de portadora", 5000.0, "Hz"), _P_RES]
    entradas = ["m"]; salidas = ["s", "d"]

    def _moduladora(self, u):
        m = np.ravel(u[0]) if not np.isscalar(u[0]) else np.zeros(3)
        return m - 0.5 * (m.max() + m.min())


@registrar
class FuenteModuladora(Bloque):
    """Moduladora trifásica de referencia.   m_x = M·sin(2π·f·t + φ - k·120°)."""
    tipo = "moduladora3"; nombre = "Moduladora trifásica"; categoria = "Potencia · Modulación"; color = "fuente"
    parametros = [P("M", "Índice de modulación", 0.8), P("f", "Frecuencia", 50.0, "Hz"), P("fase", "Fase", 0.0, "°")]
    salidas = ["m"]; directo = False

    @override
    def salida(self, t, x, u):
        th = 2 * math.pi * float(self.p["f"]) * t + math.radians(float(self.p["fase"]))
        M = float(self.p["M"])
        return [np.array([M * math.sin(th), M * math.sin(th - DOS_TERCIOS_PI), M * math.sin(th + DOS_TERCIOS_PI)])]


# ================================================================= Control


@registrar
class Clarke(Bloque):
    """Transformada de Clarke.   α = (2/3)(a - b/2 - c/2);   β = (b - c)/√3."""
    tipo = "clarke"; nombre = "Clarke abc→αβ"; categoria = "Potencia · Control"
    entradas = ["abc"]; salidas = ["alfa", "beta"]

    @override
    def salida(self, t, x, u):
        return list(clarke(np.ravel(u[0]))) if not np.isscalar(u[0]) else [0.0, 0.0]


@registrar
class ClarkeInv(Bloque):
    """Clarke inversa.   a = α;  b = -α/2 + (√3/2)·β;  c = -α/2 - (√3/2)·β."""
    tipo = "clarke_inv"; nombre = "Clarke αβ→abc"; categoria = "Potencia · Control"
    entradas = ["alfa", "beta"]; salidas = ["abc"]

    @override
    def salida(self, t, x, u):
        return [clarke_inv(float(u[0]), float(u[1]))]


@registrar
class Park(Bloque):
    """Transformada de Park.   d = α·cos θ + β·sin θ;   q = -α·sin θ + β·cos θ."""
    tipo = "park"; nombre = "Park αβ→dq"; categoria = "Potencia · Control"
    entradas = ["alfa", "beta", "theta"]; salidas = ["d", "q"]

    @override
    def salida(self, t, x, u):
        return list(park(float(u[0]), float(u[1]), float(u[2])))


@registrar
class ParkInv(Bloque):
    """Park inversa.   α = d·cos θ - q·sin θ;   β = d·sin θ + q·cos θ."""
    tipo = "park_inv"; nombre = "Park dq→αβ"; categoria = "Potencia · Control"
    entradas = ["d", "q", "theta"]; salidas = ["alfa", "beta"]

    @override
    def salida(self, t, x, u):
        return list(park_inv(float(u[0]), float(u[1]), float(u[2])))


@registrar
class SRFPLL(Bloque):
    """PLL en marco de referencia síncrono.

    v_d, v_q = Park(Clarke(v_abc), θ)
    dθ/dt = ω0 + Kp·(v_q/V_n) + I;     dI/dt = Ki·(v_q/V_n)

    En régimen v_q = 0, v_d = amplitud y θ sigue a la tensión (v_a = V·cos θ).
    Salidas: θ [rad], ω [rad/s], v_d, v_q.
    """
    tipo = "srf_pll"; nombre = "SRF-PLL"; categoria = "Potencia · Control"
    parametros = [P("f0", "Frecuencia nominal", 50.0, "Hz"), P("Vn", "Tensión de fase de pico", 326.6, "V"),
                  P("Kp", "Ganancia proporcional", 264.0), P("Ki", "Ganancia integral", 35500.0),
                  P("th0", "Ángulo inicial", "-pi/2", "rad")]
    entradas = ["v_abc"]; salidas = ["theta", "omega", "v_d", "v_q"]
    directo = False

    @override
    def preparar(self):
        self.w0 = 2 * math.pi * float(self.p["f0"])
        self.Vn = float(self.p["Vn"])

    @override
    def n_estados(self):
        return 2

    @override
    def x0(self):
        return [float(self.p["th0"]), 0.0]

    def _vdq(self, x, u):
        v = np.ravel(u[0]) if u is not None and not np.isscalar(u[0]) else np.zeros(3)
        al, be = clarke(v)
        return park(al, be, x[0])

    @override
    def salida(self, t, x, u):
        # La salida θ es un estado: no depende directamente de la entrada
        return [float(x[0]), self.w0 + float(x[1]), 0.0, 0.0]

    @override
    def derivada(self, t, x, u):
        _, vq = self._vdq(x, u)
        e = vq / self.Vn
        return [self.w0 + float(self.p["Kp"]) * e + x[1], float(self.p["Ki"]) * e]


@registrar
class PIAntiWindup(Bloque):
    """Controlador PI con anti-windup por retrocálculo.

    v = Kp·e + I;   y = sat(v, y_min, y_max);   dI/dt = Ki·e + Kaw·(y - v)
    """
    tipo = "pi"; nombre = "PI con anti-windup"; categoria = "Potencia · Control"
    parametros = [P("Kp", "Kp", 1.0), P("Ki", "Ki", 10.0), P("y_min", "Salida mínima", "-inf"),
                  P("y_max", "Salida máxima", "inf"), P("Kaw", "Ganancia anti-windup", 10.0), P("I0", "Integral inicial", 0.0)]
    entradas = ["e"]; salidas = ["y"]

    @override
    def n_estados(self):
        return 1

    @override
    def x0(self):
        return [float(self.p["I0"])]

    @override
    def salida(self, t, x, u):
        v = float(self.p["Kp"]) * float(u[0]) + x[0]
        return [min(float(self.p["y_max"]), max(float(self.p["y_min"]), v))]

    @override
    def derivada(self, t, x, u):
        e = float(u[0])
        v = float(self.p["Kp"]) * e + x[0]
        y = min(float(self.p["y_max"]), max(float(self.p["y_min"]), v))
        return [float(self.p["Ki"]) * e + float(self.p["Kaw"]) * (y - v)]


@registrar
class ControlCorrienteDQ(Bloque):
    """Control de corriente en ejes dq con desacoplo y prealimentación de tensión.

    i_d, i_q = Park(Clarke(i_abc), θ);   v_d, v_q = Park(Clarke(v_abc), θ)
    v*_d = v_d - ω·L·i_q + PI_d(i*_d - i_d)
    v*_q = v_q + ω·L·i_d + PI_q(i*_q - i_q)
    m_abc = Clarke⁻¹(Park⁻¹(v*_d, v*_q, θ)) / (V_bus/2)

    Con anti-windup por retrocálculo: |v*| limitado a V_bus/√3 (límite de SVPWM).
    """
    tipo = "control_dq"; nombre = "Control de corriente dq"; categoria = "Potencia · Control"
    parametros = [P("Kp", "Kp", 1.9), P("Ki", "Ki", 100.0), P("L", "Inductancia total", 0.65e-3, "H"),
                  P("Kaw", "Ganancia anti-windup", 100.0), P("f0", "Frecuencia", 50.0, "Hz")]
    entradas = ["id_ref", "iq_ref", "i_abc", "v_abc", "theta", "vbus"]
    salidas = ["m_abc", "i_d", "i_q"]

    @override
    def preparar(self):
        self.w = 2 * math.pi * float(self.p["f0"])

    @override
    def n_estados(self):
        return 2

    @override
    def x0(self):
        return [0.0, 0.0]

    def _calcular(self, x, u):
        Kp = float(self.p["Kp"])
        th = float(u[4])
        i = np.ravel(u[2]) if not np.isscalar(u[2]) else np.zeros(3)
        v = np.ravel(u[3]) if not np.isscalar(u[3]) else np.zeros(3)
        id_, iq = park(*clarke(i), th)
        vd, vq = park(*clarke(v), th)
        ed, eq = float(u[0]) - id_, float(u[1]) - iq
        wL = self.w * float(self.p["L"])
        vd_s = vd - wL * iq + Kp * ed + x[0]
        vq_s = vq + wL * id_ + Kp * eq + x[1]
        vb = max(float(u[5]), 1.0)
        lim = vb / RAIZ3
        mod = math.hypot(vd_s, vq_s)
        esc = lim / mod if mod > lim else 1.0
        return id_, iq, ed, eq, vd_s, vq_s, vd_s * esc, vq_s * esc, vb

    @override
    def salida(self, t, x, u):
        id_, iq, _, _, _, _, vd, vq, vb = self._calcular(x, u)
        al, be = park_inv(vd, vq, float(u[4]))
        return [clarke_inv(al, be) / (0.5 * vb), id_, iq]

    @override
    def derivada(self, t, x, u):
        _, _, ed, eq, vd_s, vq_s, vd, vq, _ = self._calcular(x, u)
        Ki, Kaw = float(self.p["Ki"]), float(self.p["Kaw"])
        return [Ki * ed + Kaw * (vd - vd_s), Ki * eq + Kaw * (vq - vq_s)]


@registrar
class ControlBusDC(Bloque):
    """Control de tensión del bus DC.

    i*_d = Kp·(V_bus - V_ref) + I,  limitada a [I_min, I_max];
    dI/dt = Ki·(V_bus - V_ref) + Kaw·(y - v)

    Si la tensión sube sobre la referencia, se inyecta más corriente activa a la red.
    """
    tipo = "control_bus"; nombre = "Control del bus DC"; categoria = "Potencia · Control"
    parametros = [P("Vref", "Tensión de referencia", 750.0, "V"), P("Kp", "Kp", 1.0), P("Ki", "Ki", 25.0),
                  P("Imax", "Corriente máxima", 300.0, "A"), P("Imin", "Corriente mínima", -30.0, "A"),
                  P("Kaw", "Ganancia anti-windup", 5.0), P("I0", "Integral inicial", 0.0)]
    entradas = ["vbus"]; salidas = ["id_ref"]

    @override
    def n_estados(self):
        return 1

    @override
    def x0(self):
        return [float(self.p["I0"])]

    @override
    def salida(self, t, x, u):
        v = float(self.p["Kp"]) * (float(u[0]) - float(self.p["Vref"])) + x[0]
        return [min(float(self.p["Imax"]), max(float(self.p["Imin"]), v))]

    @override
    def derivada(self, t, x, u):
        e = float(u[0]) - float(self.p["Vref"])
        v = float(self.p["Kp"]) * e + x[0]
        y = min(float(self.p["Imax"]), max(float(self.p["Imin"]), v))
        return [float(self.p["Ki"]) * e + float(self.p["Kaw"]) * (y - v)]


# ================================================================= Medición


class _Ventana(Bloque):
    """Promedio móvil exacto sobre una ventana de tiempo (regla del trapecio)."""
    directo = False

    def _reset(self):
        self.h = deque()
        self.acum = 0.0
        self.y_ant = None
        self.t_ant = None

    def _agregar(self, t, y, T):
        if self.t_ant is None:
            self.t_ant, self.y_ant = t, y
            return
        dt = t - self.t_ant
        area = 0.5 * (y + self.y_ant) * dt
        self.h.append((t, area))
        self.acum += area
        self.t_ant, self.y_ant = t, y
        while self.h and self.h[0][0] <= t - T:
            self.acum -= self.h.popleft()[1]


@registrar
class RMS(_Ventana):
    """Valor eficaz.   y = √((1/T)·∫ u² dt)   sobre una ventana móvil de T = 1/f0."""
    tipo = "rms"; nombre = "RMS"; categoria = "Potencia · Medición"
    parametros = [P("f0", "Frecuencia fundamental", 50.0, "Hz")]
    entradas = ["u"]; salidas = ["y"]

    @override
    def preparar(self):
        self._reset()
        self.v = 0.0

    @override
    def reiniciar(self):
        self.preparar()

    @override
    def salida(self, t, x, u):
        return [self.v]

    @override
    def paso_mayor(self, t, x, u, y):
        T = 1.0 / float(self.p["f0"])
        self._agregar(t, float(u[0]) ** 2, T)
        self.v = math.sqrt(max(self.acum, 0.0) / T)
        return False


@registrar
class PotenciaPQ(Bloque):
    """Potencia activa, reactiva y factor de potencia trifásicos (ventana de 1/f0).

    p = 1.5·(v_α·i_α + v_β·i_β);   q = 1.5·(v_β·i_α - v_α·i_β)
    P, Q = promedio móvil de p, q;   FP = P/√(P² + Q²)
    """
    tipo = "potencia_pq"; nombre = "Potencia P, Q y FP"; categoria = "Potencia · Medición"
    parametros = [P("f0", "Frecuencia fundamental", 50.0, "Hz")]
    entradas = ["v_abc", "i_abc"]; salidas = ["P", "Q", "FP"]
    directo = False

    @override
    def preparar(self):
        self.vent = (_Ventana.__new__(_Ventana), _Ventana.__new__(_Ventana))
        for v in self.vent:
            v._reset()
        self.val = [0.0, 0.0, 0.0]

    @override
    def reiniciar(self):
        self.preparar()

    @override
    def salida(self, t, x, u):
        return self.val

    @override
    def paso_mayor(self, t, x, u, y):
        if np.isscalar(u[0]) or np.isscalar(u[1]):
            return False
        va, vb = clarke(np.ravel(u[0]))
        ia, ib = clarke(np.ravel(u[1]))
        T = 1.0 / float(self.p["f0"])
        self.vent[0]._agregar(t, 1.5 * (va * ia + vb * ib), T)
        self.vent[1]._agregar(t, 1.5 * (vb * ia - va * ib), T)
        P_, Q_ = self.vent[0].acum / T, self.vent[1].acum / T
        s = math.hypot(P_, Q_)
        self.val = [P_, Q_, P_ / s if s > 0 else 0.0]
        return False


@registrar
class THD(Bloque):
    """Distorsión armónica total.   THD = √(Σ_{k=2..N} V_k²)/V_1   por FFT cada ventana.

    Se recalcula al completar `periodos` periodos de la fundamental. Salidas: THD en % y
    amplitud de la fundamental.
    """
    tipo = "thd"; nombre = "THD"; categoria = "Potencia · Medición"
    parametros = [P("f0", "Frecuencia fundamental", 50.0, "Hz"), P("periodos", "Periodos por ventana", 2),
                  P("armonicos", "Armónicos considerados", 50), P("muestras", "Muestras por periodo", 2048)]
    entradas = ["u"]; salidas = ["thd_pct", "fundamental"]
    directo = False

    @override
    def preparar(self):
        self.tt = []
        self.uu = []
        self.val = [0.0, 0.0]
        self.t_fin = int(self.p["periodos"]) / float(self.p["f0"])

    @override
    def reiniciar(self):
        self.preparar()

    @override
    def salida(self, t, x, u):
        return self.val

    @override
    def paso_mayor(self, t, x, u, y):
        self.tt.append(t)
        self.uu.append(float(u[0]))
        if t >= self.t_fin:
            T = int(self.p["periodos"]) / float(self.p["f0"])
            ini = self.t_fin - T
            n = int(self.p["muestras"]) * int(self.p["periodos"])
            tg = ini + np.arange(n) * T / n
            s = np.interp(tg, self.tt, self.uu)
            h, v1 = thd_de(s, n / T, float(self.p["f0"]), int(self.p["armonicos"]))
            self.val = [100.0 * h, v1]
            self.t_fin += T
            self.tt, self.uu = self.tt[-2:], self.uu[-2:]
        return False

