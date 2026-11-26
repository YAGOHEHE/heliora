"""Almacenamiento: batería de circuito equivalente, supercondensador y cargador DC-DC bidireccional.

Convención de los acumuladores: i > 0 en descarga (sale por el borne p).

Batería (modelo de Thevenin con una o dos ramas RC; G. L. Plett, *Battery Management Systems, Vol. 1:
Battery Modeling*, Artech House; edición sin verificar, ver MECHANICS.md):
    v = N_s·OCV(SoC) - i·R0 - v1 - v2
    dv_k/dt = i/C_k - v_k/(R_k·C_k)
    dSoC/dt = -η·i / (3600·Q_Ah)            (η solo en carga, i < 0)
Escalado de celda a paquete con N_s en serie y N_p en paralelo:
    R0, R_k → R·N_s/N_p;   C_k → C·N_p/N_s;   Q_Ah → Q·N_p;   límites de tensión ·N_s y de corriente ·N_p.
En la red es una fuente de tensión de valor N_s·OCV(SoC) - v1 - v2 con R0 en serie: el SoC y las v_k son
estados, así que no hace falta iterar.
"""
from __future__ import annotations

from typing_extensions import override
import math

import numpy as np

from ..sim.circuito import Elemento
from ..sim.nucleo import P, registrar

CATEGORIA = "Almacenamiento"


class _Avisos:
    """Avisa una sola vez de cada límite superado."""

    def preparar_avisos(self):
        self.avisados = set()

    def avisar(self, clave, texto):
        if clave not in self.avisados:
            self.avisados.add(clave)
            self.red.avisos.append(f"«{self.etiqueta}»: {texto}")


@registrar
class Bateria(_Avisos, Elemento):
    """Batería (modelo de circuito equivalente con OCV(SoC), R0 y una o dos ramas RC).

    v = N_s·OCV(SoC) - i·R0 - v1 - v2;   dv_k/dt = i/C_k - v_k/(R_k·C_k);   dSoC/dt = -η·i/(3600·Q)
    con i > 0 en descarga y η (eficiencia culómbica) aplicada solo en carga. OCV(SoC) por celda es
    una tabla con interpolación lineal. R2 = 0 deja una sola rama RC. Salidas: tensión en bornes v,
    corriente i (descarga positiva) y SoC. Superar v_min, v_max, i_max o salir de 0 <= SoC <= 1 da un
    aviso; el modelo no corta la corriente. Valores por defecto de una celda Li-ion NMC, **estimados**.
    """
    tipo = "bateria"; nombre = "Batería"; categoria = CATEGORIA
    parametros = [
        P("Q", "Capacidad por celda", 5.0, "Ah"),
        P("soc_tabla", "SoC de la tabla", "[0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1]"),
        P("ocv_tabla", "OCV por celda", "[3.0, 3.45, 3.55, 3.62, 3.67, 3.72, 3.79, 3.87, 3.96, 4.07, 4.2]", "V"),
        P("R0", "Resistencia interna R0", 0.03, "Ω"),
        P("R1", "R1", 0.02, "Ω"), P("C1", "C1", 2000.0, "F"),
        P("R2", "R2 (0 = una sola rama)", 0.0, "Ω"), P("C2", "C2", 20000.0, "F"),
        P("SoC0", "SoC inicial", 0.5), P("eta", "Eficiencia culómbica", 0.99),
        P("v_min", "Tensión mínima por celda", 2.5, "V"), P("v_max", "Tensión máxima por celda", 4.2, "V"),
        P("i_max", "Corriente máxima por celda", 50.0, "A"),
        P("Ns", "Celdas en serie", 1), P("Np", "Ramas en paralelo", 1),
    ]
    puertos_e = ["p", "n"]
    salidas = ["v", "i", "SoC"]
    n_internos = 1
    n_ramas = 1

    @override
    def preparar(self):
        p = self.p
        self.soc = np.ravel(np.asarray(p["soc_tabla"], dtype=float))
        self.ocv = np.ravel(np.asarray(p["ocv_tabla"], dtype=float))
        if self.soc.size != self.ocv.size or self.soc.size < 2 or np.any(np.diff(self.soc) <= 0):
            raise ValueError(f"Bloque «{self.etiqueta}»: la tabla OCV(SoC) necesita SoC crecientes y el mismo largo")
        Ns, Np = float(p["Ns"]), float(p["Np"])
        if Ns < 1 or Np < 1 or float(p["Q"]) <= 0 or float(p["R0"]) <= 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: Ns, Np >= 1 y Q, R0 > 0")
        self.Ns = Ns
        self.Q = float(p["Q"]) * Np * 3600.0          # carga del paquete [C]
        self.R0 = float(p["R0"]) * Ns / Np
        self.ramas_rc = []
        for k in ("1", "2"):
            R, C = float(p["R" + k]), float(p["C" + k])
            if R > 0:
                if C <= 0:
                    raise ValueError(f"Bloque «{self.etiqueta}»: C{k} debe ser positiva")
                self.ramas_rc.append((R * Ns / Np, C * Np / Ns))
        self.eta = float(p["eta"])
        self.v_min, self.v_max = float(p["v_min"]) * Ns, float(p["v_max"]) * Ns
        self.i_max = float(p["i_max"]) * Np
        self.preparar_avisos()

    @override
    def reiniciar_e(self):
        self.preparar_avisos()

    def ocv_paquete(self, soc):
        """Tensión en vacío del paquete: N_s·OCV(SoC), con la tabla saturada en sus extremos."""
        return self.Ns * float(np.interp(soc, self.soc, self.ocv))

    @override
    def n_estados(self):
        return 3

    @override
    def x0(self):
        return [float(self.p["SoC0"]), 0.0, 0.0]

    @override
    def estampar_fijo(self, S):
        m = self.internos[0]
        S.g(m, self.n["p"], 1.0 / self.R0)
        S.rama_tension(self.ramas[0], m, self.n["n"])

    @override
    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = self.ocv_paquete(x[0]) - x[1] - x[2]

    def _corriente(self, z):
        # La rama va del nodo interno al borne n: la descarga la recorre al revés
        return -z[self.ramas[0]]

    @override
    def derivada_e(self, z, t, x, u):
        i = self._corriente(z)
        d = [-(self.eta if i < 0 else 1.0) * i / self.Q]
        for k in range(2):
            if k < len(self.ramas_rc):
                R, C = self.ramas_rc[k]
                d.append(i / C - x[1 + k] / (R * C))
            else:
                d.append(0.0)
        return d

    @override
    def tras_paso(self, z, t, x, u):
        v = z[self.n["p"]] - z[self.n["n"]]
        i = self._corriente(z)
        if v < self.v_min:
            self.avisar("v_min", f"la tensión bajó a {v:.4g} V, por debajo de v_min = {self.v_min:g} V (t = {t:.4g} s)")
        if v > self.v_max:
            self.avisar("v_max", f"la tensión subió a {v:.4g} V, por encima de v_max = {self.v_max:g} V (t = {t:.4g} s)")
        if abs(i) > self.i_max:
            self.avisar("i_max", f"la corriente llegó a {i:.4g} A, por encima de i_max = {self.i_max:g} A (t = {t:.4g} s)")
        if not 0.0 <= x[0] <= 1.0:
            self.avisar("soc", f"el SoC salió de [0, 1] ({x[0]:.4g} en t = {t:.4g} s)")
        return False

    @override
    def salida_e(self, z, t, x, u):
        return [z[self.n["p"]] - z[self.n["n"]], self._corriente(z), x[0]]


@registrar
class Supercondensador(Elemento):
    """Supercondensador con resistencia serie y fuga.

    C·dv_C/dt = -i - v_C/R_fuga;   v = v_C - i·R_esr      (i > 0 en descarga)
    Estado: v_C (la carga es q = C·v_C). Salidas: tensión en bornes v, corriente i y energía
    almacenada E = C·v_C²/2 [J]. Valores por defecto **estimados** (celda de 100 F).
    """
    tipo = "supercondensador"; nombre = "Supercondensador"; categoria = CATEGORIA
    parametros = [P("C", "Capacidad", 100.0, "F"), P("R_esr", "Resistencia serie", 0.01, "Ω"),
                  P("R_fuga", "Resistencia de fuga", 1e4, "Ω"), P("v0", "Tensión inicial", 0.0, "V")]
    puertos_e = ["p", "n"]
    salidas = ["v", "i", "E"]
    n_internos = 1
    n_ramas = 1

    @override
    def preparar(self):
        for k in ("C", "R_esr", "R_fuga"):
            if not float(self.p[k]) > 0:
                raise ValueError(f"Bloque «{self.etiqueta}»: {k} debe ser positiva")

    @override
    def n_estados(self):
        return 1

    @override
    def x0(self):
        return [float(self.p["v0"])]

    @override
    def estados_en_rama(self):
        """La tensión del condensador ideal fija su rama."""
        return {0: 0}

    @override
    def estampar_fijo(self, S):
        m = self.internos[0]
        S.g(m, self.n["p"], 1.0 / float(self.p["R_esr"]))
        S.g(m, self.n["n"], 1.0 / float(self.p["R_fuga"]))
        S.rama_tension(self.ramas[0], m, self.n["n"])

    @override
    def rhs(self, b, t, x, u):
        b[self.ramas[0]] = x[0]

    @override
    def derivada_e(self, z, t, x, u):
        # La corriente de la rama es la que carga el condensador ideal (la fuga ya está descontada)
        return [z[self.ramas[0]] / float(self.p["C"])]

    @override
    def salida_e(self, z, t, x, u):
        m = self.internos[0]
        i = (z[m] - z[self.n["p"]]) * (1.0 / float(self.p["R_esr"]))
        return [z[self.n["p"]] - z[self.n["n"]], i, 0.5 * float(self.p["C"]) * x[0] ** 2]


@registrar
class CargadorDCDC(Elemento):
    """Cargador DC-DC bidireccional (medio puente reductor-elevador), modelo promediado.

    Lado alto (bus) y lado bajo (acumulador) con borne común. Con d en [0, 1]:
        L·di_L/dt = d·v_alto - v_bajo - R_L·i_L          (i_L > 0: carga el lado bajo)
        i_alto = d·i_L / η   si la potencia va del alto al bajo,   d·i_L·η   si va al revés
    Control de corriente (PI con prealimentación de v_bajo, ancho de banda ω_c = 2π·f_sw/10):
        d = sat((v_bajo + Kp·(i_ref - i_L) + I) / v_alto, 0, 1),   Kp = L·ω_c,   dI/dt = (Kp·ω_c/5)·(i_ref - i_L)
    Modo tensión: un PI externo da i_ref = sat(Kp_v·(v_ref - v_bajo) + I_v, i_min, i_max), con
    anti-windup por retrocálculo dI_v/dt = Ki_v·e_v + (Ki_v/Kp_v)·(i_ref - i_ref sin limitar).
    La entrada «ref» es i_ref [A] en modo corriente o v_ref [V] en modo tensión; i_ref se limita a
    [i_min, i_max]. Con C > 0 hay un condensador en el lado bajo (con una batería en bornes, R0·C es
    un modo muy rápido: por eso C = 0 lo omite). Salidas: i_L, d, potencia en el lado alto y en el bajo.
    Patrón de `boost_prom` y `puente_h_prom`; ajuste del PI por cancelación del polo de la planta.
    """
    tipo = "cargador_dcdc"; nombre = "Cargador DC-DC bidireccional"; categoria = CATEGORIA
    parametros = [
        P("modo", "Control", "corriente", tipo="opcion", opciones=["corriente", "tensión"]),
        P("L", "Inductancia", 1e-3, "H"), P("R_L", "Resistencia de la bobina", 0.01, "Ω"),
        P("C", "Condensador del lado bajo (0 = sin él)", 0.0, "F"),
        P("f_sw", "Frecuencia de conmutación", 20000.0, "Hz"), P("eta", "Rendimiento", 0.97),
        P("i_max", "Corriente máxima de carga", 50.0, "A"), P("i_min", "Corriente máxima de descarga (negativa)", -50.0, "A"),
        P("Kp_v", "Kp del lazo de tensión", 1.0, "A/V"), P("Ki_v", "Ki del lazo de tensión", 10.0),
        P("i0", "Corriente inicial", 0.0, "A"),
    ]
    puertos_e = ["alto", "bajo", "com"]
    entradas = ["ref"]
    salidas = ["i_L", "d", "P_alto", "P_bajo"]
    no_lineal = True

    @override
    def preparar(self):
        p = self.p
        self.L, self.RL, self.eta = float(p["L"]), float(p["R_L"]), float(p["eta"])
        if self.L <= 0 or not 0 < self.eta <= 1:
            raise ValueError(f"Bloque «{self.etiqueta}»: L > 0 y 0 < η <= 1")
        wc = 2 * math.pi * float(p["f_sw"]) / 10.0
        self.Kp, self.Ki = self.L * wc, self.L * wc * wc / 5.0
        self.imax, self.imin = float(p["i_max"]), float(p["i_min"])
        self.tension = p["modo"] == "tensión"
        self.Cb = float(p["C"])
        self.n_ramas = 1 if self.Cb > 0 else 0

    @override
    def n_estados(self):
        return 4   # i_L, integral del lazo de corriente, integral del lazo de tensión, v_C

    @override
    def x0(self):
        return [float(self.p["i0"]), 0.0, 0.0, 0.0]

    @override
    def estampar_fijo(self, S):
        if self.Cb > 0:
            S.rama_tension(self.ramas[0], self.n["bajo"], self.n["com"])

    def _v(self, z):
        c = z[self.n["com"]]
        return z[self.n["alto"]] - c, z[self.n["bajo"]] - c

    def _consigna(self, x, u, vb):
        """(i_ref limitada, error del lazo de tensión, i_ref sin limitar)."""
        if self.tension:
            ev = float(u[0]) - vb
            libre = float(self.p["Kp_v"]) * ev + x[2]
        else:
            ev, libre = 0.0, float(u[0])
        return min(self.imax, max(self.imin, libre)), ev, libre

    def _ley(self, z, x, u):
        va, vb = self._v(z)
        iref, ev, libre = self._consigna(x, u, vb)
        libre_d = (vb + self.RL * x[0] + self.Kp * (iref - x[0]) + x[1]) / va if va > 1e-9 else 0.0
        return va, vb, iref, ev, libre, libre_d, min(1.0, max(0.0, libre_d))

    def _i_alto(self, d, va, iL):
        ps = d * va * iL
        return d * iL / self.eta if ps >= 0 else d * iL * self.eta

    @override
    def rhs(self, b, t, x, u):
        b[self.n["bajo"]] += x[0]
        b[self.n["com"]] -= x[0]
        if self.Cb > 0:
            b[self.ramas[0]] = x[3]

    @override
    def estampar_din(self, M, b, z, t, x, u):
        va, vb, *_, d = self._ley(z, x, u)
        ia = self._i_alto(d, va, x[0])
        b[self.n["alto"]] -= ia
        b[self.n["com"]] += ia

    @override
    def derivada_e(self, z, t, x, u):
        va, vb, iref, ev, libre, libre_d, d = self._ley(z, x, u)
        di = (d * va - vb - self.RL * x[0]) / self.L
        # Anti-windup: los integrales se detienen si su acción ya está saturada en ese sentido
        e_i = iref - x[0]
        dI = 0.0 if (libre_d >= 1.0 and e_i > 0) or (libre_d <= 0.0 and e_i < 0) else self.Ki * e_i
        # En el lazo de tensión, retrocálculo con Tt = Kp_v/Ki_v: continuo, sin conmutar en el límite
        Kpv, Kiv = float(self.p["Kp_v"]), float(self.p["Ki_v"])
        dIv = Kiv * ev + (Kiv / Kpv) * (iref - libre) if self.tension else 0.0
        dvc = z[self.ramas[0]] / self.Cb if self.Cb > 0 else 0.0
        return [di, dI, dIv, dvc]

    @override
    def salida_e(self, z, t, x, u):
        va, vb, *_, d = self._ley(z, x, u)
        return [x[0], d, va * self._i_alto(d, va, x[0]), vb * x[0]]
