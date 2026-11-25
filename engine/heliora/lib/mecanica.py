"""Dominio mecánico rotacional por analogía con el eléctrico, resuelto con la misma red MNA.

    par T [N·m]                    ↔  corriente
    velocidad angular ω [rad/s]    ↔  tensión de nodo (la referencia, ω = 0, es la bancada)
    inercia J [kg·m²]              ↔  capacidad a la referencia (estado ω; además dθ/dt = ω)
    fricción viscosa B [N·m·s]     ↔  conductancia
    rigidez torsional K [N·m/rad]  ↔  inductancia (estado: par del resorte)
    engranaje n                    ↔  transformador ideal:  ω2 = ω1/n,  T2 = n·T1

Los puertos mecánicos solo se unen entre sí. Un puerto «n» que se deja suelto queda en la bancada.
Referencia de la analogía: K. Ogata, *System Dynamics*, Prentice Hall (cita no verificada, ver
MECHANICS.md).
"""
from __future__ import annotations

from typing_extensions import override
from ..sim.circuito import Elemento
from ..sim.nucleo import P, registrar

DOMINIO = "mecánico"


class _ElementoMecanico(Elemento):
    """Base de los elementos mecánicos rotacionales."""
    categoria = "Mecánica"


def _mecanico(cls):
    """Marca como mecánicos todos los puertos de la clase; «n» se puede dejar suelto (bancada)."""
    cls.dominio_puertos = {q: DOMINIO for q in cls.puertos_e}
    cls.puertos_opcionales = tuple(q for q in cls.puertos_e if q == "n")
    return registrar(cls)


@_mecanico
class Inercia(_ElementoMecanico):
    """Inercia.   J·dω/dt = T (par neto que recibe),   dθ/dt = ω.

    Es una fuente de velocidad de valor igual a su estado, respecto a la bancada. Salidas: ω [rad/s] y θ [rad].
    """
    tipo = "inercia"; nombre = "Inercia"
    parametros = [P("J", "Inercia", 0.01, "kg·m²"), P("w0", "Velocidad inicial", 0.0, "rad/s"),
                  P("th0", "Ángulo inicial", 0.0, "rad")]
    puertos_e = ["p"]
    salidas = ["w", "theta"]
    n_ramas = 1

    @override
    def preparar(self):
        if not float(self.p["J"]) > 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: la inercia debe ser positiva")

    @override
    def n_estados(self):
        return 2

    @override
    def x0(self):
        return [float(self.p["w0"]), float(self.p["th0"])]

    @override
    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], -1)

    @override
    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = x[0]

    @override
    def derivada_e(self, z, t, x, u):
        return [z[self.ramas[0]] / float(self.p["J"]), x[0]]

    @override
    def salida_e(self, z, t, x, u):
        return [x[0], x[1]]


@_mecanico
class Friccion(_ElementoMecanico):
    """Fricción viscosa.   T = B·(ω_p - ω_n)   (sin «n», contra la bancada)."""
    tipo = "friccion"; nombre = "Fricción viscosa"
    parametros = [P("B", "Coeficiente de fricción", 1e-3, "N·m·s")]
    puertos_e = ["p", "n"]
    salidas = ["T"]

    @override
    def preparar(self):
        if not float(self.p["B"]) > 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: B debe ser positivo")

    @override
    def estampar_fijo(self, S):
        S.g(self.n["p"], self.n["n"], float(self.p["B"]))

    @override
    def salida_e(self, z, t, x, u):
        return [float(self.p["B"]) * (z[self.n["p"]] - z[self.n["n"]])]


@_mecanico
class ResorteTorsional(_ElementoMecanico):
    """Resorte torsional.   dT/dt = K·(ω_p - ω_n),   T(0) = T0   (sin «n», contra la bancada).

    Es una fuente de par de valor igual a su estado. Salida: par T [N·m].
    """
    tipo = "resorte_torsional"; nombre = "Resorte torsional"
    parametros = [P("K", "Rigidez", 100.0, "N·m/rad"), P("T0", "Par inicial", 0.0, "N·m")]
    puertos_e = ["p", "n"]
    salidas = ["T"]

    @override
    def n_estados(self):
        return 1

    @override
    def x0(self):
        return [float(self.p["T0"])]

    @override
    def rhs(self, b, t, x, u):
        b[self.n["p"]] -= x[0]
        b[self.n["n"]] += x[0]

    @override
    def derivada_e(self, z, t, x, u):
        return [float(self.p["K"]) * (z[self.n["p"]] - z[self.n["n"]])]

    @override
    def salida_e(self, z, t, x, u):
        return [x[0]]


@_mecanico
class Engranaje(_ElementoMecanico):
    """Engranaje ideal de relación n.   ω_2 = ω_1/n,   T_2 = n·T_1   (los dos ejes respecto a la bancada).

    Sin pérdidas: ω_1·T_1 = ω_2·T_2. Salidas: par en el eje de entrada T1 y en el de salida T2.
    """
    tipo = "engranaje"; nombre = "Engranaje"
    parametros = [P("n", "Relación ω1/ω2", 5.0)]
    puertos_e = ["p1", "p2"]
    salidas = ["T1", "T2"]
    n_ramas = 1

    @override
    def preparar(self):
        if float(self.p["n"]) == 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: la relación no puede ser 0")

    @override
    def estampar_fijo(self, S):
        k, a = self.ramas[0], float(self.p["n"])
        p1, p2 = self.n["p1"], self.n["p2"]
        M = S.M
        # ω1 - n·ω2 = 0; el par T1 entra por el eje 1 y n·T1 sale por el eje 2
        M[k, p1] += 1.0
        M[k, p2] -= a
        M[p1, k] += 1.0
        M[p2, k] -= a

    @override
    def salida_e(self, z, t, x, u):
        T1 = z[self.ramas[0]]
        return [T1, float(self.p["n"]) * T1]


@_mecanico
class ParCarga(_ElementoMecanico):
    """Par de carga.   T = T0 + k·ω·|ω| + u   (lo absorbe el eje; sin «n», contra la bancada).

    T0 es un par constante, k·ω·|ω| un par cuadrático con la velocidad (ventilador, bomba centrífuga)
    y u la entrada de señal. Salida: par T [N·m].
    """
    tipo = "par_carga"; nombre = "Par de carga"
    parametros = [P("T0", "Par constante", 1.0, "N·m"), P("k", "Coeficiente cuadrático", 0.0, "N·m·s²")]
    puertos_e = ["p", "n"]
    entradas = ["u"]
    salidas = ["T"]

    @override
    def preparar(self):
        self.k = float(self.p["k"])
        self.no_lineal = self.k != 0.0

    def _par(self, z, u):
        w = z[self.n["p"]] - z[self.n["n"]]
        return float(self.p["T0"]) + self.k * w * abs(w) + float(u[0])

    @override
    def rhs(self, b, t, x, u):
        if not self.no_lineal:
            T = float(self.p["T0"]) + float(u[0])
            b[self.n["p"]] -= T
            b[self.n["n"]] += T

    @override
    def estampar_din(self, M, b, z, t, x, u):
        T = self._par(z, u)
        b[self.n["p"]] -= T
        b[self.n["n"]] += T

    @override
    def salida_e(self, z, t, x, u):
        return [self._par(z, u)]


@_mecanico
class FuentePar(_ElementoMecanico):
    """Fuente de par controlada.   T = u   [N·m], aplicado al eje p (sin «n», contra la bancada)."""
    tipo = "fuente_par"; nombre = "Fuente de par"
    entradas = ["T"]
    puertos_e = ["p", "n"]

    @override
    def rhs(self, b, t, x, u):
        T = float(u[0])
        b[self.n["p"]] += T
        b[self.n["n"]] -= T


@_mecanico
class FuenteVelocidad(_ElementoMecanico):
    """Fuente de velocidad controlada.   ω_p = u   [rad/s] respecto a la bancada. Salida: par que entrega T."""
    tipo = "fuente_velocidad"; nombre = "Fuente de velocidad"
    entradas = ["w"]
    puertos_e = ["p"]
    salidas = ["T"]
    n_ramas = 1

    @override
    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], -1)

    @override
    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = float(u[0])

    @override
    def salida_e(self, z, t, x, u):
        return [-z[self.ramas[0]]]


@_mecanico
class SensorVelocidad(_ElementoMecanico):
    """Sensor de velocidad.   y = ω_p   [rad/s]."""
    tipo = "sensor_velocidad"; nombre = "Sensor de velocidad"
    puertos_e = ["p"]
    salidas = ["w"]

    @override
    def salida_e(self, z, t, x, u):
        return [z[self.n["p"]]]


@_mecanico
class SensorPar(_ElementoMecanico):
    """Sensor de par en serie.   y = par transmitido de p a n   [N·m]."""
    tipo = "sensor_par"; nombre = "Sensor de par"
    puertos_e = ["p", "n"]
    salidas = ["T"]
    n_ramas = 1

    @override
    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], self.n["n"])

    @override
    def salida_e(self, z, t, x, u):
        return [z[self.ramas[0]]]
