"""Dominio térmico por analogía con el eléctrico, resuelto con la misma red MNA.

    flujo de calor Q [W]           ↔  corriente
    temperatura de nodo T [°C]     ↔  tensión de nodo
    resistencia térmica R_th [K/W] ↔  resistencia
    capacidad térmica C_th [J/K]   ↔  capacidad (su estado es la temperatura)
    ambiente / fuente de T         ↔  fuente de tensión a la referencia

Las temperaturas son absolutas en °C: la referencia de la red (0 V) equivale a 0 °C, de modo que el
ambiente es una fuente de temperatura y no se mezcla con la tierra eléctrica. Los puertos térmicos
solo se pueden unir entre sí (la red rechaza unirlos con un puerto eléctrico).

Redes de impedancia térmica (Infineon, AN2008-03 *Thermal equivalent circuit models*; Semikron,
*Application Manual Power Semiconductors*; edición y apartado sin verificar, ver MECHANICS.md):
    Foster:  Z_th(t) = Σ R_i·(1 - e^(-t/τ_i)),  τ_i = R_i·C_i   (pares R‖C en serie; nodos sin sentido físico)
    Cauer:   escalera R_i en serie con C_i a la referencia      (nodos con temperatura física)

Pérdidas de los semiconductores como fuente de calor (Mohan, Undeland y Robbins, *Power Electronics:
Converters, Applications and Design*, Wiley; edición sin verificar): cada IGBT y cada diodo tienen su
propia unión, con su red Foster unión-cápsula; la cápsula es el puerto térmico del bloque.
"""
from __future__ import annotations

from typing_extensions import override
import numpy as np

from ..sim.circuito import Elemento
from ..sim.nucleo import P, registrar

DOMINIO = "térmico"


class _ElementoTermico(Elemento):
    """Base de los elementos térmicos."""
    categoria = "Térmica"


def _termico(cls):
    """Marca como térmicos todos los puertos de la clase y la registra."""
    cls.dominio_puertos = {q: DOMINIO for q in cls.puertos_e}
    return registrar(cls)


@_termico
class ResistenciaTermica(_ElementoTermico):
    """Resistencia térmica.   Q = (T_p - T_n) / R_th."""
    tipo = "resistencia_termica"; nombre = "Resistencia térmica"
    parametros = [P("R", "Resistencia térmica", 0.1, "K/W")]
    puertos_e = ["p", "n"]
    salidas = ["Q"]

    @override
    def preparar(self):
        if not float(self.p["R"]) > 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: la resistencia térmica debe ser positiva")

    @override
    def estampar_fijo(self, S):
        S.g(self.n["p"], self.n["n"], 1.0 / float(self.p["R"]))

    @override
    def salida_e(self, z, t, x, u):
        return [(z[self.n["p"]] - z[self.n["n"]]) / float(self.p["R"])]


@_termico
class CapacidadTermica(_ElementoTermico):
    """Capacidad térmica a la referencia.   C_th·dT/dt = Q,   T(0) = T0.

    Es una fuente de temperatura de valor igual a su estado (como `capacitor`). Salida: T [°C].
    """
    tipo = "capacidad_termica"; nombre = "Capacidad térmica"
    parametros = [P("C", "Capacidad térmica", 100.0, "J/K"), P("T0", "Temperatura inicial", 25.0, "°C")]
    puertos_e = ["p"]
    salidas = ["T"]
    n_ramas = 1

    @override
    def preparar(self):
        if not float(self.p["C"]) > 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: la capacidad térmica debe ser positiva")

    @override
    def n_estados(self):
        return 1

    @override
    def x0(self):
        return [float(self.p["T0"])]

    @override
    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], -1)

    @override
    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = x[0]

    @override
    def derivada_e(self, z, t, x, u):
        return [z[self.ramas[0]] / float(self.p["C"])]

    @override
    def salida_e(self, z, t, x, u):
        return [x[0]]


@_termico
class FuenteCalor(_ElementoTermico):
    """Fuente de calor controlada.   Q = u   [W], inyectado en el nodo."""
    tipo = "fuente_calor"; nombre = "Fuente de calor"
    entradas = ["Q"]
    puertos_e = ["p"]

    @override
    def rhs(self, b, t, x, u):
        b[self.n["p"]] += float(u[0])


class _FijaTemperatura(_ElementoTermico):
    """Impone la temperatura del nodo frente a la referencia; su salida es el calor que absorbe."""
    puertos_e = ["p"]
    salidas = ["Q"]
    n_ramas = 1

    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], -1)

    def salida_e(self, z, t, x, u):
        return [z[self.ramas[0]]]


@_termico
class Ambiente(_FijaTemperatura):
    """Ambiente: temperatura fija T_amb. Salida Q: calor que entra al ambiente [W]."""
    tipo = "ambiente"; nombre = "Ambiente"
    parametros = [P("T_amb", "Temperatura ambiente", 40.0, "°C")]

    @override
    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = float(self.p["T_amb"])


@_termico
class FuenteTemperatura(_FijaTemperatura):
    """Fuente de temperatura controlada.   T_p = u   [°C]. Salida Q: calor que absorbe [W]."""
    tipo = "fuente_temperatura"; nombre = "Fuente de temperatura"
    entradas = ["T"]

    @override
    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = float(u[0])


@_termico
class Termometro(_ElementoTermico):
    """Termómetro.   y = T_p   [°C]."""
    tipo = "termometro"; nombre = "Termómetro"
    puertos_e = ["p"]
    salidas = ["T"]

    @override
    def salida_e(self, z, t, x, u):
        return [z[self.n["p"]]]


@_termico
class MedidorCalor(_ElementoTermico):
    """Medidor de flujo de calor en serie.   y = Q de p a n   [W]."""
    tipo = "medidor_calor"; nombre = "Medidor de flujo de calor"
    puertos_e = ["p", "n"]
    salidas = ["Q"]
    n_ramas = 1

    @override
    def estampar_fijo(self, S):
        S.rama_tension(self.ramas[0], self.n["p"], self.n["n"])

    @override
    def salida_e(self, z, t, x, u):
        return [z[self.ramas[0]]]


def _vector(valor, nombre, etiqueta):
    v = np.ravel(np.asarray(valor, dtype=float))
    if v.size == 0 or np.any(v <= 0):
        raise ValueError(f"Bloque «{etiqueta}»: {nombre} debe ser una lista de valores positivos")
    return v


@_termico
class RedTermicaRC(_ElementoTermico):
    """Impedancia térmica entre a y b, en forma Foster o Cauer.

    Foster: pares R_i‖C_i en serie de a a b, con C_i = τ_i/R_i. Con a calentado por Q y b a temperatura
    fija, T_a - T_b = Q·Σ R_i·(1 - e^(-t/τ_i)) (la Z_th(t) de las hojas de datos). Sus nodos internos
    no tienen significado físico y no debe ponerse en serie con otra red sin convertirla.
    Cauer: escalera a –R_1– · –R_2– ... –R_n– b, con C_i del nodo de entrada de R_i a la referencia
    (temperatura inicial T0). Sus nodos sí son temperaturas físicas.
    Estados: n (Foster: caídas ΔT_i; Cauer: temperaturas de los nodos).
    """
    tipo = "red_termica_rc"; nombre = "Red térmica Foster / Cauer"
    parametros = [P("modo", "Forma", "foster", tipo="opcion", opciones=["foster", "cauer"]),
                  P("R", "Resistencias R_i", "[0.01, 0.02, 0.015]", "K/W"),
                  P("tau", "Constantes τ_i (Foster)", "[1e-3, 1e-2, 1e-1]", "s"),
                  P("C", "Capacidades C_i (Cauer)", "[0.1, 0.5, 6.7]", "J/K"),
                  P("T0", "Temperatura inicial (Cauer)", 25.0, "°C")]
    puertos_e = ["a", "b"]

    @override
    def preparar(self):
        self.foster = self.p["modo"] == "foster"
        self.R = _vector(self.p["R"], "R", self.etiqueta)
        otro = _vector(self.p["tau"] if self.foster else self.p["C"], "τ" if self.foster else "C", self.etiqueta)
        if otro.size != self.R.size:
            raise ValueError(f"Bloque «{self.etiqueta}»: R y {'τ' if self.foster else 'C'} deben tener el mismo largo")
        self.C = otro / self.R if self.foster else otro
        self.n_internos = self.R.size - 1
        self.n_ramas = self.R.size

    @override
    def n_estados(self):
        return self.R.size

    @override
    def x0(self):
        return [0.0] * self.R.size if self.foster else [float(self.p["T0"])] * self.R.size

    def _cadena(self):
        return [self.n["a"]] + list(self.internos) + [self.n["b"]]

    @override
    def estampar_fijo(self, S):
        c = self._cadena()
        for i, R in enumerate(self.R):
            S.g(c[i], c[i + 1], 1.0 / R)
            S.rama_tension(self.ramas[i], c[i], c[i + 1] if self.foster else -1)

    @override
    def rhs(self, b, t, x, u):
        for i in range(self.R.size):
            b[self.ramas[i]] = x[i]

    @override
    def derivada_e(self, z, t, x, u):
        return [z[self.ramas[i]] / self.C[i] for i in range(self.R.size)]


# ================================================================= Uniones de semiconductores


def parametros_union(especies):
    """Parámetros térmicos de un bloque con uniones de IGBT ("q") y/o diodo ("d").

    Valores por defecto: módulo de 1200 V / 600 A tipo FZ600R12KE3. Son **estimados** (Anexo A.5 del
    encargo, ver MECHANICS.md): sustitúyelos por los de la hoja de datos del módulo.
    """
    p = []
    if "q" in especies:
        p += [P("Rth_q", "R_th unión-cápsula del IGBT (Foster)", "[0.0045, 0.0135, 0.018, 0.009]", "K/W"),
              P("tau_q", "τ Foster del IGBT", "[0.5e-3, 5e-3, 50e-3, 500e-3]", "s")]
    if "d" in especies:
        p += [P("Rth_d", "R_th unión-cápsula del diodo (Foster)", "[0.009, 0.027, 0.036, 0.018]", "K/W"),
              P("tau_d", "τ Foster del diodo", "[0.5e-3, 5e-3, 50e-3, 500e-3]", "s")]
    p += [P("T_ref", "Temperatura de los datos eléctricos", 125.0, "°C"),
          P("a_V", "Coeficiente de V_ce0 y V_f", -2e-3, "V/K"),
          P("a_R", "Coeficiente de R_on y R_d", 6e-3, "1/K", ayuda="relativo: 0.006 = +0.6 %/K"),
          P("Tj_max", "Temperatura de unión máxima (aviso)", 150.0, "°C"),
          P("tau_e", "Retardo de entrega del calor", 1e-4, "s",
            ayuda="las pérdidas llegan a la unión con esta constante de tiempo; la energía se conserva"),
          P("dT", "Salto de Tj que reajusta R_on y V", 0.1, "K")]
    if "q" in especies:
        p += [P("a_E", "Coeficiente de las energías de conmutación", 3e-3, "1/K", ayuda="relativo: 0.003 = +0.3 %/K"),
              P("k_i", "Exponente de corriente de las energías", 1.0)]
    return p


class UnionesTermicas:
    """Mezcla para elementos con uniones de semiconductor y un puerto térmico opcional «th».

    Cada unión j tiene su red Foster unión → cápsula (internos y ramas propios) y recibe como calor
    su energía de pérdidas:
        dE_cond_j/dt = p_cond_j(t)                     (conducción v·i, integrada por el solver)
        E_j = E_cond_j + E_conm_j                      (E_conm: suma de las energías de conmutación)
        P_j = (E_j - E_entregada_j) / τ_e,      dE_entregada_j/dt = P_j
    Toda la energía llega a la unión (con un retardo τ_e mucho menor que las τ térmicas) y la fuente
    de calor depende solo de estados, no de la solución de la red en el mismo instante (no hay lazo
    algebraico). La cápsula es el puerto «th». Si «th» no se conecta, el elemento no crea nodos ni
    estados: se comporta como sin modelo térmico, a la temperatura de los datos T_ref.

    El elemento define `potencia_conduccion(z)`: pérdida de conducción instantánea de cada unión.
    `E_union` acumula solo las energías de conmutación.
    """

    puertos_opcionales = ("th",)
    dominio_puertos = {"th": DOMINIO}

    def preparar_uniones(self, especies):
        """Lee los parámetros térmicos; `especies` dice el tipo ("q" o "d") de cada unión."""
        p = self.p
        self.especies = list(especies)
        self.foster = {}
        for k in set(especies):
            R = _vector(p[f"Rth_{k}"], f"R_th del {'IGBT' if k == 'q' else 'diodo'}", self.etiqueta)
            tau = _vector(p[f"tau_{k}"], f"τ del {'IGBT' if k == 'q' else 'diodo'}", self.etiqueta)
            if R.size != tau.size:
                raise ValueError(f"Bloque «{self.etiqueta}»: R_th y τ deben tener el mismo largo")
            self.foster[k] = (R, tau / R)
        self.T_ref, self.a_V, self.a_R = float(p["T_ref"]), float(p["a_V"]), float(p["a_R"])
        self.a_E = float(p.get("a_E", 0.0))
        self.k_i = float(p.get("k_i", 1.0))
        self.tau_e, self.dT, self.Tj_max = float(p["tau_e"]), float(p["dT"]), float(p["Tj_max"])
        if not self.tau_e > 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: τ_e debe ser positivo")
        self.termico = False
        self._base = (type(self).n_internos, type(self).n_ramas)
        self.reiniciar_uniones()

    def reiniciar_uniones(self):
        """Pone a cero las energías acumuladas y la temperatura de unión usada y de pico."""
        self.E_union = np.zeros(len(self.especies))
        self.Tj_usada = None
        self.Tj_pico = -np.inf
        self.avisado = False

    def antes_de_numerar(self, elementos, raiz_de):
        """Determina si el puerto térmico está conectado y ajusta el número de nodos y ramas internos."""
        self.termico = bool(self.conectados.get("th"))
        m = sum(self.foster[k][0].size for k in self.especies) if self.termico else 0
        self.n_internos = self._base[0] + m
        self.n_ramas = self._base[1] + m
        self._disp = None

    def n_estados(self):
        """Devuelve el número de estados de las redes Foster y de los acumuladores de energía."""
        if not self.termico:
            return 0
        return sum(self.foster[k][0].size + 2 for k in self.especies)

    def x0(self):
        """Devuelve el estado inicial, con todos los estados a cero."""
        return [0.0] * self.n_estados()

    def _disposicion(self):
        """Por unión: (nodos de la cadena unión → cápsula, ramas, primer estado, R, C).

        Estados de cada unión: las m caídas ΔT_i de la red Foster, E_cond y E_entregada.
        """
        if self._disp is None:
            i0 = self._base[0]
            r0 = self._base[1]
            s0 = 0
            self._disp = []
            for k in self.especies:
                R, C = self.foster[k]
                m = R.size
                nodos = list(self.internos[i0:i0 + m]) + [self.n["th"]]
                self._disp.append((nodos, list(self.ramas[r0:r0 + m]), s0, R, C))
                i0, r0, s0 = i0 + m, r0 + m, s0 + m + 2
        return self._disp

    def estampar_uniones(self, S):
        """Estampa en la matriz las conductancias de las redes Foster y sus ramas de tensión."""
        if not self.termico:
            return
        for nodos, ramas, _, R, _ in self._disposicion():
            for i in range(R.size):
                S.g(nodos[i], nodos[i + 1], 1.0 / R[i])
                S.rama_tension(ramas[i], nodos[i], nodos[i + 1])

    def _potencias(self, x):
        return [(x[s + R.size] + self.E_union[j] - x[s + R.size + 1]) / self.tau_e
                for j, (_, _, s, R, _) in enumerate(self._disposicion())]

    def rhs_uniones(self, b, x):
        """Añade al término independiente las caídas de tensión de las redes Foster y la potencia de cada unión."""
        if not self.termico:
            return
        for (nodos, ramas, s, R, _), Pj in zip(self._disposicion(), self._potencias(x)):
            for i in range(R.size):
                b[ramas[i]] = x[s + i]
            b[nodos[0]] += Pj

    def derivada_uniones(self, z, x):
        """Devuelve las derivadas de los estados: caídas Foster, energía de conducción y energía entregada."""
        if not self.termico:
            return []
        d = []
        pc = self.potencia_conduccion(z)
        for (_, ramas, s, R, C), Pj, pcj in zip(self._disposicion(), self._potencias(x), pc):
            d.extend(z[ramas[i]] / C[i] for i in range(R.size))
            d.append(pcj)
            d.append(Pj)
        return d

    def temperaturas(self, z):
        """Temperatura de cada unión [°C]; T_ref si el puerto térmico no está conectado."""
        if not self.termico:
            return np.full(len(self.especies), self.T_ref)
        return np.array([z[nodos[0]] for nodos, *_ in self._disposicion()])

    def factor_energia(self, j, i_rel):
        """Escala de una energía de conmutación: (|i|/I_ref)^k_i·(1 + a_E·(Tj - T_ref))."""
        f = i_rel ** self.k_i
        if self.termico and self.Tj_usada is not None:
            f *= 1.0 + self.a_E * (self.Tj_usada[j] - self.T_ref)
        return f

    def revisar_temperaturas(self, z, aplicar):
        """Si alguna unión se movió más de dT, reajusta los parámetros eléctricos con `aplicar(Tj)`.

        Devuelve True si cambió la estampa (la red refactoriza).
        """
        if not self.termico:
            return False
        Tj = self.temperaturas(z)
        pico = float(np.max(Tj))
        if pico > self.Tj_pico:
            self.Tj_pico = pico
            if pico > self.Tj_max and not self.avisado:
                self.avisado = True
                self.red.avisos.append(f"«{self.etiqueta}»: la temperatura de unión llegó a {pico:.1f} °C, "
                                       f"por encima de Tj_max = {self.Tj_max:g} °C")
        if self.Tj_usada is not None and np.max(np.abs(Tj - self.Tj_usada)) < self.dT:
            return False
        self.Tj_usada = Tj
        aplicar(Tj)
        return True

    def con_temperatura(self, ref_V, ref_R, Tj):
        """(V(Tj), R(Tj)) a partir de los valores a T_ref:  V + a_V·ΔT,  R·(1 + a_R·ΔT)."""
        dT = Tj - self.T_ref
        return ref_V + self.a_V * dT, ref_R * (1.0 + self.a_R * dT)
