"""Librería eléctrica: pasivos, fuentes, red, semiconductores, transformador y medidas.

Convención: la corriente de un elemento va de su puerto p a su puerto n.
Tensión: v = v_p - v_n. Todos los valores son reales instantáneos.
"""
from __future__ import annotations

import math

import numpy as np

from ..sim.circuito import Elemento
from ..sim.nucleo import P, registrar

# ---------------------------------------------------------------- referencia


@registrar
class Tierra(Elemento):
    """Tierra: fija el nodo de referencia,  v = 0."""
    tipo = "tierra"; nombre = "Tierra"; categoria = "Eléctrica · Pasivos"
    puertos_e = ["t"]
    es_tierra = True


# ---------------------------------------------------------------- pasivos


@registrar
class Resistor(Elemento):
    """Resistencia.   v = R·i"""
    tipo = "resistor"; nombre = "Resistencia R"; categoria = "Eléctrica · Pasivos"
    parametros = [P("R", "Resistencia", 10.0, "Ω")]
    puertos_e = ["p", "n"]

    def estampar_fijo(self, S):
        S.g(self.n["p"], self.n["n"], 1.0 / float(self.p["R"]))


@registrar
class Inductor(Elemento):
    """Inductancia.   L·di/dt = v_p - v_n,   i(0) = i0

    Es una fuente de corriente de valor igual a su estado.
    """
    tipo = "inductor"; nombre = "Inductancia L"; categoria = "Eléctrica · Pasivos"
    parametros = [P("L", "Inductancia", 1e-3, "H"), P("i0", "Corriente inicial", 0.0, "A")]
    puertos_e = ["p", "n"]

    def n_estados(self):
        return 1

    def x0(self):
        return [float(self.p["i0"])]

    def rhs(self, b, t, x, u):
        i = x[0]
        b[self.n["p"]] -= i
        b[self.n["n"]] += i

    def derivada_e(self, z, t, x, u):
        return [(z[self.n["p"]] - z[self.n["n"]]) / float(self.p["L"])]


@registrar
class Capacitor(Elemento):
    """Capacidad.   C·dv/dt = i,   v(0) = v0

    Es una fuente de tensión de valor igual a su estado.
    """
    tipo = "capacitor"; nombre = "Capacidad C"; categoria = "Eléctrica · Pasivos"
    parametros = [P("C", "Capacidad", 1e-6, "F"), P("v0", "Tensión inicial", 0.0, "V")]
    puertos_e = ["p", "n"]
    n_ramas = 1

    def n_estados(self):
        return 1

    def x0(self):
        return [float(self.p["v0"])]

    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], self.n["n"])

    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = x[0]

    def derivada_e(self, z, t, x, u):
        return [z[self.ramas[0]] / float(self.p["C"])]


@registrar
class RLC(Elemento):
    """RLC en serie o en paralelo.

    Serie:     v = R·i + L·di/dt + v_C,   C·dv_C/dt = i
    Paralelo:  i = v/R + i_L + C·dv/dt,   L·di_L/dt = v
    """
    tipo = "rlc"; nombre = "RLC"; categoria = "Eléctrica · Pasivos"
    parametros = [P("R", "Resistencia", 1.0, "Ω"), P("L", "Inductancia", 1e-3, "H"), P("C", "Capacidad", 1e-6, "F"),
                  P("modo", "Conexión", "serie", tipo="opcion", opciones=["serie", "paralelo"])]
    puertos_e = ["p", "n"]
    n_internos = 2
    n_ramas = 1

    def n_estados(self):
        return 2

    def x0(self):
        return [0.0, 0.0]  # i_L, v_C

    def estampar_fijo(self, S):
        p, n = self.n["p"], self.n["n"]
        if self.p["modo"] == "serie":
            a, c = self.internos
            S.g(p, a, 1.0 / float(self.p["R"]))
            S.rama_tension(self.ramas[0], c, n)
        else:
            S.g(p, n, 1.0 / float(self.p["R"]))
            S.rama_tension(self.ramas[0], p, n)
            S.g(self.internos[0], -1, 1.0)
            S.g(self.internos[1], -1, 1.0)

    def rhs(self, b, t, x, u):
        iL, vC = x
        p, n = self.n["p"], self.n["n"]
        if self.p["modo"] == "serie":
            a, c = self.internos
            b[a] -= iL
            b[c] += iL
        else:
            b[p] -= iL
            b[n] += iL
        b[self.ramas[0]] = vC

    def derivada_e(self, z, t, x, u):
        p, n = self.n["p"], self.n["n"]
        if self.p["modo"] == "serie":
            a, c = self.internos
            return [(z[a] - z[c]) / float(self.p["L"]), z[self.ramas[0]] / float(self.p["C"])]
        return [(z[p] - z[n]) / float(self.p["L"]), z[self.ramas[0]] / float(self.p["C"])]


# ---------------------------------------------------------------- trifásicos pasivos


@registrar
class Resistor3(Elemento):
    """Resistencia trifásica.   v_x = R·i_x   para cada fase x."""
    tipo = "resistor3"; nombre = "R trifásica"; categoria = "Eléctrica · Pasivos"
    parametros = [P("R", "Resistencia por fase", 0.1, "Ω")]
    puertos_e3 = ["a", "b"]

    def estampar_fijo(self, S):
        for f in range(3):
            S.g(self.n["a"][f], self.n["b"][f], 1.0 / float(self.p["R"]))


@registrar
class Inductor3(Elemento):
    """Bobina trifásica de 3 hilos (sin neutro).

    L·di_x/dt = (v1_x - v2_x) - (1/3)·Σ(v1 - v2),   i_a + i_b + i_c = 0

    Con 3 hilos solo hay 2 corrientes independientes, de modo que el sistema queda sin
    modos rígidos artificiales de secuencia cero. Estados: i_a, i_b.
    """
    tipo = "inductor3"; nombre = "L trifásica"; categoria = "Eléctrica · Pasivos"
    parametros = [P("L", "Inductancia por fase", 1e-3, "H"), P("R", "Resistencia serie", 0.0, "Ω")]
    puertos_e3 = ["a", "b"]
    salidas = ["i_abc"]

    def preparar(self):
        self.Lt, self.Rt = float(self.p["L"]), float(self.p["R"])

    def absorber(self, L, R):
        self.Lt += L
        self.Rt += R

    def i2_abc(self, x):
        return np.array([x[0], x[1], -x[0] - x[1]])

    def di2(self, z, x):
        d = self.derivada_e(z, 0.0, x, None)
        return np.array([d[0], d[1], -d[0] - d[1]])

    def n_estados(self):
        return 2

    def x0(self):
        return [0.0, 0.0]

    def rhs(self, b, t, x, u):
        na, nb = self.n["a"], self.n["b"]
        ia, ib = x
        ic = -ia - ib
        b[na[0]] -= ia; b[nb[0]] += ia
        b[na[1]] -= ib; b[nb[1]] += ib
        b[na[2]] -= ic; b[nb[2]] += ic

    def derivada_e(self, z, t, x, u):
        na, nb = self.n["a"], self.n["b"]
        va = z[na[0]] - z[nb[0]]
        vb = z[na[1]] - z[nb[1]]
        vc = z[na[2]] - z[nb[2]]
        m = (va + vb + vc) / 3.0
        return [(va - m - self.Rt * x[0]) / self.Lt, (vb - m - self.Rt * x[1]) / self.Lt]

    def salida_e(self, z, t, x, u):
        return [np.array([x[0], x[1], -x[0] - x[1]])]


@registrar
class Capacitor3(Elemento):
    """Banco de condensadores trifásico en estrella con neutro aislado.

    C·dv_x/dt = i_x,   v_a + v_b + v_c = 0
    Estados: v_a, v_b (referidas al neutro virtual). Con R serie de amortiguamiento opcional.
    """
    tipo = "capacitor3"; nombre = "C trifásica"; categoria = "Eléctrica · Pasivos"
    parametros = [P("C", "Capacidad por fase", 10e-6, "F"), P("Rd", "Amortiguamiento serie", 0.0, "Ω")]
    puertos_e3 = ["a"]
    n_internos = 1  # neutro
    n_ramas = 3

    def n_estados(self):
        return 2

    def x0(self):
        return [0.0, 0.0]

    def estampar_fijo(self, S):
        na = self.n["a"]
        N = self.internos[0]
        Rd = float(self.p["Rd"])
        for f in range(3):
            S.rama_tension(self.ramas[f], na[f], N)
            if Rd > 0:
                # v_x - v_N = v_C + Rd·i_x  (rama con resistencia)
                S.M[self.ramas[f], self.ramas[f]] -= Rd

    def rhs(self, b, t, x, u):
        va, vb = x
        b[self.ramas[0]] = va
        b[self.ramas[1]] = vb
        b[self.ramas[2]] = -va - vb

    def derivada_e(self, z, t, x, u):
        C = float(self.p["C"])
        return [z[self.ramas[0]] / C, z[self.ramas[1]] / C]


# ---------------------------------------------------------------- fuentes


@registrar
class FuenteDC(Elemento):
    """Fuente de tensión DC.   v_p - v_n = V"""
    tipo = "fuente_dc"; nombre = "Fuente DC"; categoria = "Eléctrica · Fuentes"
    parametros = [P("V", "Tensión", 100.0, "V")]
    puertos_e = ["p", "n"]
    n_ramas = 1

    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], self.n["n"])

    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = float(self.p["V"])


@registrar
class FuenteAC(Elemento):
    """Fuente AC monofásica.   v(t) = √2·V·sin(2π·f·t + φ)   (V eficaz)"""
    tipo = "fuente_ac"; nombre = "Fuente AC"; categoria = "Eléctrica · Fuentes"
    parametros = [P("V", "Tensión eficaz", 230.0, "V"), P("f", "Frecuencia", 50.0, "Hz"), P("fase", "Fase", 0.0, "°")]
    puertos_e = ["p", "n"]
    n_ramas = 1

    def preparar(self):
        self.A = math.sqrt(2) * float(self.p["V"])
        self.w = 2 * math.pi * float(self.p["f"])
        self.ph = math.radians(float(self.p["fase"]))

    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], self.n["n"])

    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = self.A * math.sin(self.w * t + self.ph)


@registrar
class FuenteAC3(Elemento):
    """Fuente AC trifásica en estrella con neutro a tierra.

    v_x(t) = √2·(V_LL/√3)·sin(2π·f·t + φ - k·120°),  k = 0, 1, 2
    """
    tipo = "fuente_ac3"; nombre = "Fuente AC trifásica"; categoria = "Eléctrica · Fuentes"
    parametros = [P("VLL", "Tensión de línea eficaz", 400.0, "V"), P("f", "Frecuencia", 50.0, "Hz"),
                  P("fase", "Fase", 0.0, "°")]
    puertos_e3 = ["a"]
    n_ramas = 3

    def preparar(self):
        self.A = math.sqrt(2.0 / 3.0) * float(self.p["VLL"])
        self.w = 2 * math.pi * float(self.p["f"])
        self.ph = math.radians(float(self.p["fase"]))

    def estampar_fijo(self, S):
        for f in range(3):
            S.rama_tension(self.ramas[f], self.n["a"][f], -1)

    def rhs(self, b, t, x, u):
        th = self.w * t + self.ph
        for f in range(3):
            b[self.ramas[f]] = self.A * math.sin(th - f * 2 * math.pi / 3)


@registrar
class FuenteIDC(Elemento):
    """Fuente de corriente DC.   i = I (de p a n por dentro de la fuente)."""
    tipo = "fuente_idc"; nombre = "Fuente de corriente"; categoria = "Eléctrica · Fuentes"
    parametros = [P("I", "Corriente", 1.0, "A")]
    puertos_e = ["p", "n"]

    def rhs(self, b, t, x, u):
        i = float(self.p["I"])
        b[self.n["p"]] -= i
        b[self.n["n"]] += i


@registrar
class FuenteVControlada(Elemento):
    """Fuente de tensión controlada por señal.   v_p - v_n = u"""
    tipo = "fuente_v_ctrl"; nombre = "Tensión controlada"; categoria = "Eléctrica · Fuentes"
    entradas = ["v"]
    puertos_e = ["p", "n"]
    n_ramas = 1

    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], self.n["n"])

    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = float(u[0])


@registrar
class FuenteIControlada(Elemento):
    """Fuente de corriente controlada por señal.   i = u"""
    tipo = "fuente_i_ctrl"; nombre = "Corriente controlada"; categoria = "Eléctrica · Fuentes"
    entradas = ["i"]
    puertos_e = ["p", "n"]

    def rhs(self, b, t, x, u):
        i = float(u[0])
        b[self.n["p"]] -= i
        b[self.n["n"]] += i


@registrar
class RedInfinita(Elemento):
    """Red como barra infinita con impedancia.

    e_x = √2·(V_LL/√3)·sin(ω·t + φ - k·120°)
    v_pcc,x = e_x + R·i_x + L·di_x/dt     (i_x va del punto de conexión hacia la red)

    Modelo de 3 hilos: i_a + i_b + i_c = 0. Si el puerto se conecta a un filtro L o LCL, sus
    inductancias quedan en serie y se suman en un solo estado (la corriente es la misma).
    Salidas: corriente que entra a la red i_abc [A] y tensión v_pcc [V].
    """
    tipo = "red_infinita"; nombre = "Red (barra infinita)"; categoria = "Eléctrica · Fuentes"
    parametros = [P("VLL", "Tensión de línea eficaz", 400.0, "V"), P("f", "Frecuencia", 50.0, "Hz"),
                  P("fase", "Fase", 0.0, "°"), P("R", "Resistencia de red", 0.01, "Ω"),
                  P("L", "Inductancia de red", 50e-6, "H")]
    puertos_e3 = ["a"]
    salidas = ["i_abc", "v_pcc"]

    def preparar(self):
        self.A = math.sqrt(2.0 / 3.0) * float(self.p["VLL"])
        self.w = 2 * math.pi * float(self.p["f"])
        self.ph = math.radians(float(self.p["fase"]))
        self.Rg, self.Lg = float(self.p["R"]), float(self.p["L"])
        self.vecino = None
        self.n_ramas = 0

    def antes_de_numerar(self, elementos, raiz_de):
        """Si solo la une a una bobina en serie, la bobina asume R y L de la red."""
        nodo = raiz_de(self.id, "a")
        vecinos = []
        for o in elementos:
            if o is self:
                continue
            for q in o.puertos(o.crudos)["e3"]:
                if raiz_de(o.id, q) == nodo:
                    vecinos.append((o, q))
        if len(vecinos) == 1 and vecinos[0][1] == "b" and hasattr(vecinos[0][0], "absorber"):
            self.vecino = vecinos[0][0]
            self.vecino.absorber(self.Lg, self.Rg)
            self.n_ramas = 3

    def _e(self, t):
        th = self.w * t + self.ph
        A = self.A
        return (A * math.sin(th), A * math.sin(th - 2 * math.pi / 3), A * math.sin(th + 2 * math.pi / 3))

    def n_estados(self):
        return 0 if self.vecino else 2

    def x0(self):
        return [] if self.vecino else [0.0, 0.0]

    def estampar_fijo(self, S):
        if self.vecino:
            for f in range(3):
                S.rama_tension(self.ramas[f], self.n["a"][f], -1)

    def rhs(self, b, t, x, u):
        if self.vecino:
            e = self._e(t)
            for f in range(3):
                b[self.ramas[f]] = e[f]
        else:
            ia, ib = x
            na = self.n["a"]
            b[na[0]] -= ia
            b[na[1]] -= ib
            b[na[2]] += ia + ib

    def derivada_e(self, z, t, x, u):
        e = self._e(t)
        na = self.n["a"]
        v = [z[na[0]], z[na[1]], z[na[2]]]
        m = (v[0] + v[1] + v[2]) / 3.0  # neutro virtual del punto de conexión
        return [((v[0] - m) - e[0] - self.Rg * x[0]) / self.Lg, ((v[1] - m) - e[1] - self.Rg * x[1]) / self.Lg]

    def salida_e(self, z, t, x, u):
        na = self.n["a"]
        if self.vecino:
            vec = self.vecino
            xv = self.red.x_actual[vec.xa:vec.xb]
            i = vec.i2_abc(xv)
            e = np.array(self._e(t))
            v = e + self.Rg * i + self.Lg * vec.di2(z, xv)
            return [i, v]
        v = np.array([z[na[0]], z[na[1]], z[na[2]]])
        return [np.array([x[0], x[1], -x[0] - x[1]]), v - v.mean()]


# ---------------------------------------------------------------- semiconductores


class _Interruptor(Elemento):
    """Base de los interruptores con resistencia de conducción."""
    con_estado_discreto = True


@registrar
class Diodo(_Interruptor):
    """Diodo ideal con resistencia de conducción.

    Conduce:     v = Vf + Ron·i   (mientras i > 0)
    Bloquea:     i = Goff·v       (mientras v < Vf)
    """
    tipo = "diodo"; nombre = "Diodo"; categoria = "Eléctrica · Semiconductores"
    parametros = [P("Ron", "Resistencia de conducción", 1e-3, "Ω"), P("Vf", "Tensión directa", 0.8, "V"),
                  P("Goff", "Conductancia en bloqueo", 1e-6, "S")]
    puertos_e = ["p", "n"]
    salidas = ["i"]

    def preparar(self):
        self.on = False

    def reiniciar_e(self):
        self.on = False

    def clave(self):
        return self.on

    def estampar_estado(self, S):
        S.g(self.n["p"], self.n["n"], 1.0 / float(self.p["Ron"]) if self.on else float(self.p["Goff"]))

    def rhs(self, b, t, x, u):
        if self.on:
            i0 = float(self.p["Vf"]) / float(self.p["Ron"])
            b[self.n["p"]] += i0
            b[self.n["n"]] -= i0

    def corriente(self, z):
        v = z[self.n["p"]] - z[self.n["n"]]
        return (v - float(self.p["Vf"])) / float(self.p["Ron"]) if self.on else v * float(self.p["Goff"])

    def actualizar(self, z, t, x, u):
        v = z[self.n["p"]] - z[self.n["n"]]
        if self.on and self.corriente(z) < 0.0:
            self.on = False
            return True
        if not self.on and v > float(self.p["Vf"]):
            self.on = True
            return True
        return False

    def cruce_e(self, z, t, x, u):
        # Conduciendo cambia al anularse la corriente; bloqueando, al alcanzar V_f
        if self.on:
            return [self.corriente(z)]
        return [z[self.n["p"]] - z[self.n["n"]] - float(self.p["Vf"])]

    def salida_e(self, z, t, x, u):
        return [self.corriente(z)]


@registrar
class InterruptorIdeal(_Interruptor):
    """Interruptor ideal bidireccional.   g >= 0.5: v = Ron·i;   g < 0.5: i = Goff·v"""
    tipo = "interruptor"; nombre = "Interruptor"; categoria = "Eléctrica · Semiconductores"
    parametros = [P("Ron", "Resistencia de conducción", 1e-3, "Ω"), P("Goff", "Conductancia abierto", 1e-6, "S")]
    puertos_e = ["p", "n"]
    entradas = ["g"]
    entradas_compuerta = ("g",)

    def preparar(self):
        self.on = False

    def reiniciar_e(self):
        self.on = False

    def clave(self):
        return self.on

    def estampar_estado(self, S):
        S.g(self.n["p"], self.n["n"], 1.0 / float(self.p["Ron"]) if self.on else float(self.p["Goff"]))

    def actualizar(self, z, t, x, u):
        nuevo = float(u[0]) >= 0.5
        if nuevo != self.on:
            self.on = nuevo
            return True
        return False


class DispositivoIGBT:
    """Un IGBT con su diodo antiparalelo, con el modelo de pérdidas.

    Estado 0 bloqueo: i = Goff·v
    Estado 1 IGBT:    v = Vce0 + Ron·i   (i > 0)
    Estado 2 diodo:   v = -Vf + Rd·i     (i < 0)

    Pérdida de conducción: p = v·i
    Pérdida de conmutación por evento: E = E_ref·(|i|/I_ref)·(|V_bus|/V_ref)
    Los estados 1 y 2 se describen con R y una tensión interna.
    """
    __slots__ = ("c", "e", "st", "Ron", "Vce0", "Rd", "Vf", "Goff", "g")

    def __init__(self, c, e, Ron, Vce0, Rd, Vf, Goff=1e-6):
        self.c, self.e = c, e
        self.Ron, self.Vce0, self.Rd, self.Vf, self.Goff = Ron, Vce0, Rd, Vf, Goff
        self.st = 0
        self.g = False

    def conductancia(self):
        return (self.Goff, 1.0 / self.Ron, 1.0 / self.Rd)[self.st]

    def inyeccion(self, b):
        if self.st == 1:
            i0 = self.Vce0 / self.Ron
        elif self.st == 2:
            i0 = -self.Vf / self.Rd
        else:
            return
        b[self.c] += i0
        b[self.e] -= i0

    def corriente(self, z):
        v = z[self.c] - z[self.e]
        if self.st == 1:
            return (v - self.Vce0) / self.Ron
        if self.st == 2:
            return (v + self.Vf) / self.Rd
        return v * self.Goff

    def revisar(self, z):
        v = z[self.c] - z[self.e]
        i = self.corriente(z)
        st = self.st
        if st == 1:
            if not self.g or i < 0.0:
                self.st = 0
        elif st == 2:
            if i > 0.0:
                self.st = 0
        else:
            if self.g and v > self.Vce0 + 1e-9:
                self.st = 1
            elif v < -self.Vf - 1e-9:
                self.st = 2
        return self.st != st


class _ParametrosIGBT:
    # Por defecto, un módulo de 1200 V y 600 A a 125 °C. Son datos de ejemplo: pon los de tu equipo.
    # V_ce0 y R_on salen de ajustar una recta a su curva V-I. E_rr y los valores del diodo son
    # estimados (ver MECHANICS.md).
    PARAMS = [
        P("Ron", "Resistencia del IGBT", 0.002, "Ω"), P("Vce0", "Tensión umbral del IGBT", 0.8, "V"),
        P("Rd", "Resistencia del diodo", 0.002, "Ω"), P("Vf", "Tensión del diodo", 0.9, "V"),
        P("Eon", "Energía de encendido", 139.96e-3, "J"), P("Eoff", "Energía de apagado", 174.78e-3, "J"),
        P("Err", "Energía de recuperación", 42e-3, "J"), P("Vref", "Tensión de referencia", 600.0, "V"),
        P("Iref", "Corriente de referencia", 1191.06, "A"),
    ]


@registrar
class IGBT(_Interruptor, _ParametrosIGBT):
    """IGBT con diodo antiparalelo.

    Modelo de conducción de `DispositivoIGBT`. Salidas: corriente i y energías
    acumuladas E_cond y E_sw [J]; la pérdida media es ΔE/Δt.
    """
    tipo = "igbt"; nombre = "IGBT + diodo"; categoria = "Eléctrica · Semiconductores"
    parametros = _ParametrosIGBT.PARAMS
    puertos_e = ["c", "e"]
    entradas = ["g"]
    salidas = ["i", "E_cond", "E_sw"]
    entradas_compuerta = ("g",)

    def preparar(self):
        self.d = None
        self.Ec = self.Es = 0.0
        self.t_ant = 0.0

    def reiniciar_e(self):
        p = self.p
        self.d = DispositivoIGBT(self.n["c"], self.n["e"], float(p["Ron"]), float(p["Vce0"]), float(p["Rd"]), float(p["Vf"]))
        self.Ec = self.Es = 0.0
        self.t_ant = 0.0

    def _dev(self):
        if self.d is None:
            self.reiniciar_e()
        return self.d

    def clave(self):
        return self._dev().st

    def estampar_estado(self, S):
        d = self._dev()
        S.g(d.c, d.e, d.conductancia())

    def rhs(self, b, t, x, u):
        self._dev().inyeccion(b)

    def actualizar(self, z, t, x, u):
        d = self._dev()
        d.g = float(u[0]) >= 0.5
        i0 = d.corriente(z)
        v0 = abs(z[d.c] - z[d.e])
        st0 = d.st
        c = d.revisar(z)
        if c:
            p = self.p
            if st0 == 1 and d.st == 0:
                self.Es += float(p["Eoff"]) * abs(i0) / float(p["Iref"]) * v0 / float(p["Vref"])
            elif st0 == 2 and d.st == 0:
                self.Es += float(p["Err"]) * abs(i0) / float(p["Iref"]) * v0 / float(p["Vref"])
        return c

    def tras_paso(self, z, t, x, u):
        d = self._dev()
        dt = t - self.t_ant
        self.t_ant = t
        if d.st:
            self.Ec += (z[d.c] - z[d.e]) * d.corriente(z) * dt

    def salida_e(self, z, t, x, u):
        return [self._dev().corriente(z), self.Ec, self.Es]


@registrar
class MOSFET(_Interruptor):
    """MOSFET con diodo interno.

    g >= 0.5:  v = Ron·i (bidireccional)
    g < 0.5:   conduce el diodo interno si v < -Vf:  v = -Vf + Rd·i;  si no, i = Goff·v
    """
    tipo = "mosfet"; nombre = "MOSFET"; categoria = "Eléctrica · Semiconductores"
    parametros = [P("Ron", "Resistencia de conducción", 0.02, "Ω"), P("Rd", "Resistencia del diodo", 0.02, "Ω"),
                  P("Vf", "Tensión del diodo", 0.9, "V"), P("Goff", "Conductancia en bloqueo", 1e-6, "S")]
    puertos_e = ["d", "s"]
    entradas = ["g"]
    salidas = ["i"]
    entradas_compuerta = ("g",)

    def preparar(self):
        self.st = 0  # 0 bloqueo, 1 canal, 2 diodo

    def reiniciar_e(self):
        self.st = 0

    def clave(self):
        return self.st

    def estampar_estado(self, S):
        g = (float(self.p["Goff"]), 1.0 / float(self.p["Ron"]), 1.0 / float(self.p["Rd"]))[self.st]
        S.g(self.n["d"], self.n["s"], g)

    def rhs(self, b, t, x, u):
        if self.st == 2:
            i0 = -float(self.p["Vf"]) / float(self.p["Rd"])
            b[self.n["d"]] += i0
            b[self.n["s"]] -= i0

    def corriente(self, z):
        v = z[self.n["d"]] - z[self.n["s"]]
        if self.st == 1:
            return v / float(self.p["Ron"])
        if self.st == 2:
            return (v + float(self.p["Vf"])) / float(self.p["Rd"])
        return v * float(self.p["Goff"])

    def actualizar(self, z, t, x, u):
        g = float(u[0]) >= 0.5
        v = z[self.n["d"]] - z[self.n["s"]]
        st = self.st
        if g:
            self.st = 1
        elif st == 1:
            self.st = 0
        elif st == 2:
            if self.corriente(z) > 0.0:
                self.st = 0
        elif v < -float(self.p["Vf"]) - 1e-9:
            self.st = 2
        return self.st != st

    def salida_e(self, z, t, x, u):
        return [self.corriente(z)]


# ---------------------------------------------------------------- transformador


@registrar
class Transformador(Elemento):
    """Transformador monofásico con relación a = N1/N2.

    Ideal:  v1 = a·v2,   i2 = a·i1  (el lado 2 entrega corriente)
    Más magnetización Lm en el lado 1 y dispersión R2 + L2 referida al lado 2:
        Lm·di_m/dt = v1;   L2·di_2/dt = v_x - v_2 - R2·i_2
    """
    tipo = "transformador"; nombre = "Transformador"; categoria = "Eléctrica · Pasivos"
    parametros = [P("a", "Relación N1/N2", 2.0), P("Lm", "Inductancia magnetizante", 1.0, "H"),
                  P("R2", "Resistencia serie (lado 2)", 0.01, "Ω"), P("L2", "Dispersión (lado 2)", 1e-4, "H")]
    puertos_e = ["p1", "n1", "p2", "n2"]
    n_internos = 1  # nodo ideal secundario
    n_ramas = 1

    def n_estados(self):
        return 2

    def x0(self):
        return [0.0, 0.0]  # i_m, i_2

    def estampar_fijo(self, S):
        k = self.ramas[0]
        a = float(self.p["a"])
        p1, n1 = self.n["p1"], self.n["n1"]
        x, n2 = self.internos[0], self.n["n2"]
        M = S.M
        # v1 - a·vx = 0;  la corriente primaria i1 y la secundaria a·i1 (entra al nodo x)
        M[k, p1] += 1.0; M[k, n1] -= 1.0; M[k, x] -= a; M[k, n2] += a
        M[p1, k] += 1.0; M[n1, k] -= 1.0
        M[x, k] -= a; M[n2, k] += a

    def rhs(self, b, t, x, u):
        im, i2 = x
        p1, n1 = self.n["p1"], self.n["n1"]
        b[p1] -= im
        b[n1] += im
        xx = self.internos[0]
        b[xx] -= i2
        b[self.n["p2"]] += i2

    def derivada_e(self, z, t, x, u):
        v1 = z[self.n["p1"]] - z[self.n["n1"]]
        return [v1 / float(self.p["Lm"]),
                (z[self.internos[0]] - z[self.n["p2"]] - float(self.p["R2"]) * x[1]) / float(self.p["L2"])]


# ---------------------------------------------------------------- medidas


@registrar
class Voltimetro(Elemento):
    """Voltímetro.   y = v_p - v_n"""
    tipo = "voltimetro"; nombre = "Voltímetro"; categoria = "Eléctrica · Medición"
    puertos_e = ["p", "n"]
    salidas = ["v"]

    def salida_e(self, z, t, x, u):
        return [z[self.n["p"]] - z[self.n["n"]]]


@registrar
class Amperimetro(Elemento):
    """Amperímetro (rama de tensión cero).   y = i   (de p a n)"""
    tipo = "amperimetro"; nombre = "Amperímetro"; categoria = "Eléctrica · Medición"
    puertos_e = ["p", "n"]
    salidas = ["i"]
    n_ramas = 1

    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], self.n["n"])

    def salida_e(self, z, t, x, u):
        return [z[self.ramas[0]]]


@registrar
class Medidor3(Elemento):
    """Medidor trifásico en serie.

    Salidas: v_abc (fase a neutro virtual, v_x - promedio) e i_abc (de a hacia b).
    """
    tipo = "medidor3"; nombre = "Medidor trifásico"; categoria = "Eléctrica · Medición"
    puertos_e3 = ["a", "b"]
    salidas = ["v_abc", "i_abc"]
    n_ramas = 3

    def estampar_fijo(self, S):
        for f in range(3):
            S.rama_tension(self.ramas[f], self.n["a"][f], self.n["b"][f])

    def salida_e(self, z, t, x, u):
        na = self.n["a"]
        v = np.array([z[na[0]], z[na[1]], z[na[2]]])
        r = self.ramas
        return [v - v.mean(), np.array([z[r[0]], z[r[1]], z[r[2]]])]
