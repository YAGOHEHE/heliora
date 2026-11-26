"""Máquinas eléctricas que acoplan el dominio eléctrico con el mecánico rotacional.

Cada máquina tiene un puerto mecánico «eje» (dominio mecánico) en el que inyecta su par
electromagnético Te frente a la bancada. Si J > 0 incluye la inercia del rotor (estado ω, ecuación
J·dω/dt = Te - B·ω - T_eje) y entonces el eje no admite una fuente de velocidad; con J = 0 la
velocidad es la del nodo del eje, que debe fijar la red mecánica externa (una inercia o una fuente de
velocidad). La fricción B va del eje a la bancada.

Ecuaciones (P. Krause, O. Wasynczuk y S. Sudhoff, *Analysis of Electric Machinery and Drive Systems*,
IEEE Press / Wiley; edición sin verificar, ver MECHANICS.md). Transformadas de `potencia.py` con
invariancia de amplitud: P = 1.5·(v_d·i_d + v_q·i_q). Las máquinas trifásicas son de tres hilos
(neutro aislado): la tensión de fase se toma frente al neutro virtual, v_x - (v_a + v_b + v_c)/3.
"""
from __future__ import annotations

from typing_extensions import override
import numpy as np

from ..sim.circuito import Elemento
from ..sim.nucleo import P, registrar
from .mecanica import DOMINIO
from .potencia import clarke, clarke_inv, park, park_inv


class _Maquina(Elemento):
    """Base: eje mecánico con inercia opcional del rotor, fricción y ángulo mecánico."""
    categoria = "Máquinas"
    dominio_puertos = {"eje": DOMINIO}
    n_electricos = 0

    @staticmethod
    def params_mecanicos(J, B):
        return [P("J", "Inercia del rotor (0 = sin inercia propia)", J, "kg·m²"),
                P("B", "Fricción viscosa", B, "N·m·s"), P("w0", "Velocidad inicial", 0.0, "rad/s"),
                P("th0", "Ángulo mecánico inicial", 0.0, "rad")]

    def preparar_eje(self):
        self.J, self.B = float(self.p["J"]), float(self.p["B"])
        if self.J < 0 or self.B < 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: J y B no pueden ser negativos")
        self.n_ramas = type(self).n_ramas + (1 if self.J > 0 else 0)

    def n_estados(self):
        return self.n_electricos + (2 if self.J > 0 else 1)

    def x0_eje(self):
        return ([float(self.p["w0"])] if self.J > 0 else []) + [float(self.p["th0"])]

    def estampar_eje(self, S):
        if self.J > 0:
            S.rama_tension(self.ramas[-1], self.n["eje"], -1)
        if self.B > 0:
            S.g(self.n["eje"], -1, self.B)

    def rhs_eje(self, b, x, Te):
        b[self.n["eje"]] += Te
        if self.J > 0:
            b[self.ramas[-1]] = x[self.n_electricos]

    def estados_en_rama(self):
        """Con inercia propia, la velocidad del rotor fija la última rama (la del eje)."""
        return {self.n_electricos: len(self.ramas) - 1} if self.J > 0 else {}

    def omega(self, z, x):
        return x[self.n_electricos] if self.J > 0 else z[self.n["eje"]]

    def theta(self, x):
        return x[-1]

    def derivada_eje(self, z, x):
        w = self.omega(z, x)
        return ([z[self.ramas[-1]] / self.J] if self.J > 0 else []) + [w]


@registrar
class MaquinaDC(_Maquina):
    """Máquina de corriente continua de excitación constante.

    v_a = R_a·i_a + L_a·di_a/dt + K_e·ω;   Te = K_t·i_a  (K_t = K_e en unidades SI)
    En régimen con par de carga T_L: i_a = (T_L + B·ω)/K_t,  ω = (v_a - R_a·i_a)/K_e.
    Puertos: p, n (inducido) y eje. Salidas: ω [rad/s], Te [N·m], i_a [A]. Valores **estimados**.
    """
    tipo = "maquina_dc"; nombre = "Máquina de continua"
    parametros = [P("Ra", "Resistencia del inducido", 1.0, "Ω"), P("La", "Inductancia del inducido", 0.01, "H"),
                  P("Ke", "Constante K_e = K_t", 0.5, "V·s/rad"), P("ia0", "Corriente inicial", 0.0, "A")] + \
        _Maquina.params_mecanicos(0.01, 1e-3)
    puertos_e = ["p", "n", "eje"]
    salidas = ["w", "Te", "ia"]
    n_electricos = 1

    @override
    def preparar(self):
        self.Ra, self.La, self.Ke = float(self.p["Ra"]), float(self.p["La"]), float(self.p["Ke"])
        if self.La <= 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: L_a debe ser positiva")
        self.preparar_eje()

    @override
    def x0(self):
        return [float(self.p["ia0"])] + self.x0_eje()

    @override
    def estampar_fijo(self, S):
        self.estampar_eje(S)

    @override
    def rhs(self, b, t, x, u):
        b[self.n["p"]] -= x[0]
        b[self.n["n"]] += x[0]
        self.rhs_eje(b, x, self.Ke * x[0])

    @override
    def derivada_e(self, z, t, x, u):
        v = z[self.n["p"]] - z[self.n["n"]]
        return [(v - self.Ra * x[0] - self.Ke * self.omega(z, x)) / self.La] + self.derivada_eje(z, x)

    @override
    def salida_e(self, z, t, x, u):
        return [self.omega(z, x), self.Ke * x[0], x[0]]


class _Trifasica(_Maquina):
    """Tensión de fase frente al neutro virtual en αβ, e inyección de corrientes αβ en los bornes."""
    puertos_e3 = ["abc"]

    def v_alfa_beta(self, z):
        a = self.n["abc"]
        v = np.array([z[a[0]], z[a[1]], z[a[2]]])
        return clarke(v - v.mean())

    def inyectar(self, b, alfa, beta):
        i = clarke_inv(alfa, beta)
        a = self.n["abc"]
        for f in range(3):
            b[a[f]] -= i[f]
        return i


@registrar
class PMSM(_Trifasica):
    """Máquina síncrona de imanes permanentes en ejes dq del rotor.

    v_d = R_s·i_d + L_d·di_d/dt - ω_e·L_q·i_q
    v_q = R_s·i_q + L_q·di_q/dt + ω_e·(L_d·i_d + λ_pm)
    Te = 1.5·p·(λ_pm·i_q + (L_d - L_q)·i_d·i_q),   ω_e = p·ω,   θ_e = p·θ
    Lee v_abc en los bornes, la pasa a dq con θ_e (Clarke y Park de `potencia.py`) e inyecta i_abc
    con las inversas, como fuentes de corriente trifásicas. Con L_d = L_q la constante de par es
    K_t = 1.5·p·λ_pm. Salidas: ω, Te, i_d, i_q, i_abc. Valores por defecto de un servo pequeño, **estimados**.
    """
    tipo = "pmsm"; nombre = "Máquina síncrona de imanes (PMSM)"
    parametros = [P("Rs", "Resistencia del estátor", 0.5, "Ω"), P("Ld", "Inductancia L_d", 4e-3, "H"),
                  P("Lq", "Inductancia L_q", 4e-3, "H"), P("lam", "Flujo de los imanes λ_pm", 0.1, "Wb"),
                  P("pp", "Pares de polos", 4)] + _Maquina.params_mecanicos(1e-3, 1e-4)
    puertos_e = ["eje"]
    salidas = ["w", "Te", "id", "iq", "i_abc"]
    n_electricos = 2

    @override
    def preparar(self):
        p = self.p
        self.Rs, self.Ld, self.Lq, self.lam = (float(p[k]) for k in ("Rs", "Ld", "Lq", "lam"))
        self.pp = float(p["pp"])
        if self.Ld <= 0 or self.Lq <= 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: L_d y L_q deben ser positivas")
        self.preparar_eje()

    @override
    def x0(self):
        return [0.0, 0.0] + self.x0_eje()

    def par(self, x):
        """Devuelve el par electromagnético (N·m) a partir de los estados eléctricos x."""
        return 1.5 * self.pp * (self.lam * x[1] + (self.Ld - self.Lq) * x[0] * x[1])

    @override
    def estampar_fijo(self, S):
        self.estampar_eje(S)

    @override
    def rhs(self, b, t, x, u):
        self.inyectar(b, *park_inv(x[0], x[1], self.pp * self.theta(x)))
        self.rhs_eje(b, x, self.par(x))

    @override
    def derivada_e(self, z, t, x, u):
        vd, vq = park(*self.v_alfa_beta(z), self.pp * self.theta(x))
        we = self.pp * self.omega(z, x)
        did = (vd - self.Rs * x[0] + we * self.Lq * x[1]) / self.Ld
        diq = (vq - self.Rs * x[1] - we * (self.Ld * x[0] + self.lam)) / self.Lq
        return [did, diq] + self.derivada_eje(z, x)

    @override
    def salida_e(self, z, t, x, u):
        i = clarke_inv(*park_inv(x[0], x[1], self.pp * self.theta(x)))
        return [self.omega(z, x), self.par(x), x[0], x[1], i]


@registrar
class MaquinaInduccion(_Trifasica):
    """Máquina de inducción de jaula, modelo de cuarto orden en ejes estacionarios αβ.

    ψ_s = L_s·i_s + L_m·i_r;  ψ_r = L_m·i_s + L_r·i_r;  L_s = L_ls + L_m,  L_r = L_lr + L_m
    dψ_s/dt = v_s - R_s·i_s;   dψ_r/dt = -R_r·i_r + j·ω_r·ψ_r   (rotor en cortocircuito, ω_r = p·ω)
    Te = 1.5·p·(ψ_sα·i_sβ - ψ_sβ·i_sα);   deslizamiento s = (ω_s - ω_r)/ω_s
    Estados: los cuatro flujos (y ω si J > 0) y θ. Salidas: ω, Te, i_abc. Valores **estimados**
    (motor de jaula de unos 4 kW, 400 V, 4 polos).
    """
    tipo = "maquina_induccion"; nombre = "Máquina de inducción"
    parametros = [P("Rs", "Resistencia del estátor", 1.5, "Ω"), P("Rr", "Resistencia del rotor (referida)", 1.2, "Ω"),
                  P("Lls", "Dispersión del estátor", 6e-3, "H"), P("Llr", "Dispersión del rotor", 6e-3, "H"),
                  P("Lm", "Inductancia magnetizante", 0.18, "H"), P("pp", "Pares de polos", 2)] + \
        _Maquina.params_mecanicos(0.02, 2e-3)
    puertos_e = ["eje"]
    salidas = ["w", "Te", "i_abc"]
    n_electricos = 4

    @override
    def preparar(self):
        p = self.p
        self.Rs, self.Rr, self.Lm = float(p["Rs"]), float(p["Rr"]), float(p["Lm"])
        self.Ls, self.Lr = float(p["Lls"]) + self.Lm, float(p["Llr"]) + self.Lm
        self.D = self.Ls * self.Lr - self.Lm ** 2
        if self.Lm <= 0 or self.D <= 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: inductancias no válidas")
        self.pp = float(p["pp"])
        self.preparar_eje()

    @override
    def x0(self):
        return [0.0] * 4 + self.x0_eje()

    def corrientes(self, x):
        """(i_sα, i_sβ, i_rα, i_rβ) a partir de los flujos."""
        sa, sb, ra, rb = x[0], x[1], x[2], x[3]
        return ((self.Lr * sa - self.Lm * ra) / self.D, (self.Lr * sb - self.Lm * rb) / self.D,
                (self.Ls * ra - self.Lm * sa) / self.D, (self.Ls * rb - self.Lm * sb) / self.D)

    def par(self, x):
        """Devuelve el par electromagnético (N·m) a partir de los flujos x."""
        isa, isb, _, _ = self.corrientes(x)
        return 1.5 * self.pp * (x[0] * isb - x[1] * isa)

    @override
    def estampar_fijo(self, S):
        self.estampar_eje(S)

    @override
    def rhs(self, b, t, x, u):
        isa, isb, _, _ = self.corrientes(x)
        self.inyectar(b, isa, isb)
        self.rhs_eje(b, x, self.par(x))

    @override
    def derivada_e(self, z, t, x, u):
        va, vb = self.v_alfa_beta(z)
        isa, isb, ira, irb = self.corrientes(x)
        wr = self.pp * self.omega(z, x)
        return [va - self.Rs * isa, vb - self.Rs * isb,
                -self.Rr * ira - wr * x[3], -self.Rr * irb + wr * x[2]] + self.derivada_eje(z, x)

    @override
    def salida_e(self, z, t, x, u):
        isa, isb, _, _ = self.corrientes(x)
        return [self.omega(z, x), self.par(x), clarke_inv(isa, isb)]
