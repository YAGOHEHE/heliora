"""Librería fotovoltaica: módulo de un diodo, strings, sombreado, MPPT y recurso solar.

Modelo de un diodo:
    I = I_L - I_0·(exp((V + I·R_s)/a) - 1) - (V + I·R_s)/R_sh,    a = n·N_s·k·T/q

Dependencia con irradiancia G y temperatura T (De Soto):
    I_L = (G/G_ref)·(I_L,ref + α_sc·(T - T_ref))
    a   = a_ref·T/T_ref
    I_0 = I_0,ref·(T/T_ref)³·exp(E_g,ref/(k·T_ref) - E_g/(k·T))
    E_g = E_g,ref·(1 - 0.0002677·(T - T_ref)),   E_g,ref = 1.121 eV
    R_sh = R_sh,ref·G_ref/G;     R_s constante
"""
from __future__ import annotations

import bisect
import math
import re
from dataclasses import dataclass, field

import numpy as np
from scipy.optimize import least_squares
from scipy.special import wrightomega

from ..sim.circuito import Elemento
from ..sim.nucleo import Bloque, P, registrar

K_EV = 8.617333262e-5   # eV/K
T_REF = 298.15          # K
G_REF = 1000.0          # W/m²
EG_REF = 1.121          # eV
DEG = 0.0002677         # 1/K


@dataclass
class ModuloFV:
    """Datos de hoja de un módulo y sus cinco parámetros (a, I_L, I_0, R_s, R_sh) a STC."""
    nombre: str = ""
    Voc: float = 44.2
    Isc: float = 9.68
    Vmp: float = 37.0
    Imp: float = 8.92
    Ns: int = 66
    alfa_sc: float = 0.0005 * 9.68   # A/K
    beta_oc: float = -0.0029 * 44.2  # V/K
    a: float = 0.0
    IL: float = 0.0
    I0: float = 0.0
    Rs: float = 0.0
    Rsh: float = 0.0
    residuo: float = field(default=0.0)

    def extraer(self):
        """Calcula los cinco parámetros a partir de la hoja de datos (ver extraer_parametros)."""
        if self.a == 0.0:
            extraer_parametros(self)
        return self

    @property
    def Pmp(self):
        """Potencia en el punto de máxima potencia [W]."""
        return self.Vmp * self.Imp


def extraer_parametros(m: ModuloFV):
    """Extrae (a, I_L, I_0, R_s, R_sh) resolviendo cinco condiciones a STC (ver la lista).

    1. Cortocircuito:   I(0) = I_sc
    2. Circuito abierto: I(V_oc) = 0
    3. Máxima potencia: I(V_mp) = I_mp
    4. Máxima potencia: dP/dV = 0 en V_mp
    5. Coeficiente de V_oc: I(V_oc + β·ΔT; T_ref + ΔT) = 0

    Se resuelve con mínimos cuadrados en variables logarítmicas, que aseguran valores
    positivos. Da el residuo relativo final, que debe ser muy pequeño.
    """
    Voc, Isc, Vmp, Imp = m.Voc, m.Isc, m.Vmp, m.Imp
    vt = K_EV * T_REF * m.Ns
    dT = 10.0
    TT = T_REF + dT
    Eg = EG_REF * (1 - DEG * dT)
    VocT = Voc + m.beta_oc * dT

    def f(p):
        """Residuos relativos de las cinco condiciones para los parámetros en logaritmo."""
        a, IL, I0, Rs, Rsh = np.exp(p)
        e_sc = np.exp(np.minimum(Isc * Rs / a, 500.0))
        e_oc = np.exp(np.minimum(Voc / a, 500.0))
        x = Vmp + Imp * Rs
        e_mp = np.exp(np.minimum(x / a, 500.0))
        r1 = IL - I0 * (e_sc - 1) - Isc * Rs / Rsh - Isc
        r2 = IL - I0 * (e_oc - 1) - Voc / Rsh
        r3 = IL - I0 * (e_mp - 1) - x / Rsh - Imp
        gd = I0 / a * e_mp + 1 / Rsh
        r4 = Imp / Vmp - gd / (1 + Rs * gd)
        aT = a * TT / T_REF
        I0T = I0 * (TT / T_REF) ** 3 * np.exp(EG_REF / (K_EV * T_REF) - Eg / (K_EV * TT))
        ILT = IL + m.alfa_sc * dT
        r5 = ILT - I0T * (np.exp(np.minimum(VocT / aT, 500.0)) - 1) - VocT / Rsh
        return np.array([r1 / Isc, r2 / Isc, r3 / Isc, r4 * Vmp / Imp, r5 / Isc])

    mejor = None
    for n_ideal in (1.1, 1.3, 1.0, 1.5):
        a0 = n_ideal * vt
        IL0 = Isc
        I00 = max(Isc * math.exp(-Voc / a0), 1e-15)
        for Rs0 in (0.2, 0.4, 0.1):
            p0 = np.log([a0, IL0, I00, Rs0, 400.0])
            s = least_squares(f, p0, xtol=1e-13, ftol=1e-13, gtol=1e-13, max_nfev=2000)
            if mejor is None or s.cost < mejor.cost:
                mejor = s
        if mejor.cost < 1e-16:
            break
    m.a, m.IL, m.I0, m.Rs, m.Rsh = np.exp(mejor.x).tolist()
    m.residuo = float(np.max(np.abs(mejor.fun)))
    return m


def parametros_operacion(m: ModuloFV, G, T_c):
    """Parámetros en operación (De Soto) para G [W/m²] y T_c [°C]."""
    m.extraer()
    T = np.asarray(T_c, dtype=float) + 273.15
    G = np.maximum(np.asarray(G, dtype=float), 1e-6)
    IL = G / G_REF * (m.IL + m.alfa_sc * (T - T_REF))
    a = m.a * T / T_REF
    Eg = EG_REF * (1 - DEG * (T - T_REF))
    I0 = m.I0 * (T / T_REF) ** 3 * np.exp(EG_REF / (K_EV * T_REF) - Eg / (K_EV * T))
    Rsh = m.Rsh * G_REF / G
    return IL, I0, m.Rs, Rsh, a


def v_de_i(I, IL, I0, Rs, Rsh, a):
    """Solución explícita V(I) con la función omega de Wright.

    V = R_sh·(I_L + I_0 - I) - I·R_s - a·ω(ln(I_0·R_sh/a) + R_sh·(I_L + I_0 - I)/a)
    """
    I = np.asarray(I, dtype=float)
    y = Rsh * (IL + I0 - I)
    w = wrightomega(np.log(I0 * Rsh / a) + y / a).real
    return y - I * Rs - a * w


def i_de_v(V, IL, I0, Rs, Rsh, a):
    """Solución explícita I(V) con la función omega de Wright.

    I = (R_sh·(I_L + I_0) - V)/(R_s + R_sh) - (a/R_s)·ω(ln(θ) + R_sh·(R_s·(I_L + I_0) + V)/(a·(R_s + R_sh)))
    θ = R_s·I_0·R_sh/(a·(R_s + R_sh))
    """
    V = np.asarray(V, dtype=float)
    s = Rs + Rsh
    th = Rs * I0 * Rsh / (a * s)
    arg = np.log(th) + Rsh * (Rs * (IL + I0) + V) / (a * s)
    return (Rsh * (IL + I0) - V) / s - (a / Rs) * wrightomega(arg).real


def curva_iv(m: ModuloFV, G=1000.0, T_c=25.0, n=300):
    """Curva I-V y P-V de un módulo. Devuelve V, I, P y el punto de máxima potencia."""
    par = parametros_operacion(m, G, T_c)
    voc = float(v_de_i(0.0, *par))
    V = np.linspace(0.0, voc, n)
    I = np.maximum(i_de_v(V, *par), 0.0)
    return V, I, V * I, mpp(V, I)


def mpp(V, I):
    """Máxima potencia de una curva con refinamiento parabólico."""
    P = np.asarray(V) * np.asarray(I)
    k = int(np.argmax(P))
    if 0 < k < len(P) - 1:
        a = np.polyfit(V[k - 1:k + 2], P[k - 1:k + 2], 2)
        if a[0] < 0:
            vm = -a[1] / (2 * a[0])
            return {"Vmp": float(vm), "Pmp": float(np.polyval(a, vm)), "Imp": float(np.polyval(a, vm) / vm)}
    return {"Vmp": float(V[k]), "Pmp": float(P[k]), "Imp": float(I[k])}


def curva_string(m: ModuloFV, factores, T_c=25.0, G=1000.0, bypass_por_modulo=3, Vd=0.7, n=500):
    """Curva V(I) de un string con diodos de bypass y sombreado parcial.

    Cada sub-módulo tiene N_s/bypass celdas y su propio factor de irradiancia.
    Sub-módulo con bypass: V_k(I) = max(V_sub(I), -V_d). String: V(I) = Σ V_k(I).
    `factores` tiene un valor por sub-módulo (1 = sin sombra, 0.3 = 70 % de sombra).
    Devuelve V e I crecientes en V, desde cortocircuito hasta circuito abierto.
    """
    factores = np.asarray(factores, dtype=float)
    Imax = max(float(G / G_REF) * (m.Isc + m.alfa_sc * (T_c - 25.0)) * float(np.max(factores)) * 1.02, 1e-3)
    I = np.linspace(0.0, Imax, n)
    Vt = np.zeros_like(I)
    frac = 1.0 / bypass_por_modulo
    grupos, cuenta = np.unique(np.round(factores, 6), return_counts=True)
    for f, c in zip(grupos, cuenta):
        par = parametros_operacion(m, G * max(f, 1e-4), T_c)
        Vs = v_de_i(I, *par) * frac
        Vt += c * np.maximum(Vs, -Vd)
    orden = np.argsort(Vt)
    return Vt[orden], I[orden]


def curva_arreglo(m: ModuloFV, strings, T_c=25.0, G=1000.0, bypass_por_modulo=3, Vd=0.7):
    """Arreglo de strings en paralelo: I(V) = Σ I_s(V). `strings` es una lista de listas de factores."""
    curvas = [curva_string(m, s, T_c, G, bypass_por_modulo, Vd) for s in strings]
    vmax = max(v[-1] for v, _ in curvas)
    V = np.linspace(0.0, vmax, 800)
    I = np.zeros_like(V)
    for v, i in curvas:
        I += np.maximum(np.interp(V, v, i, right=0.0), 0.0)
    return V, I


# Módulos comerciales de referencia. Datos de hoja a STC (1000 W/m², 25 °C).
CATALOGO = {
    "CS1H-330MS (330 W, 66 celdas)": dict(Voc=44.2, Isc=9.68, Vmp=37.0, Imp=8.92, Ns=66,
                                         alfa_sc=0.0005 * 9.68, beta_oc=-0.0029 * 44.2),
    "AS-6M30-HC (320 W, 60 celdas)": dict(Voc=40.2, Isc=10.14, Vmp=33.4, Imp=9.59, Ns=60,
                                         alfa_sc=0.0005 * 10.14, beta_oc=-0.0029 * 40.2),
    "CS6X-300M (300 W, 72 celdas)": dict(Voc=45.0, Isc=8.74, Vmp=36.5, Imp=8.22, Ns=72,
                                         alfa_sc=0.004326, beta_oc=-0.15372),
}
_cache_mod = {}
_cec = []


def modulo_de_catalogo(nombre):
    """Módulo del catálogo propio, con sus parámetros ya extraídos.

    Raises:
        ValueError: si el nombre no está en el catálogo (se listan las opciones).
    """
    if nombre not in CATALOGO:
        raise ValueError(f"Módulo desconocido «{nombre}». Opciones del catálogo: {', '.join(CATALOGO)}. Para otros módulos usa la búsqueda en la base CEC.")
    if nombre not in _cache_mod:
        _cache_mod[nombre] = ModuloFV(nombre=nombre, **CATALOGO[nombre]).extraer()
    return _cache_mod[nombre]


def _base_cec():
    if not _cec:
        import pvlib
        _cec.append(pvlib.pvsystem.retrieve_sam("CECMod"))
    return _cec[0]


def buscar_cec(texto, limite=50):
    """Busca módulos en la base CEC de pvlib: cada palabra del texto debe aparecer en el nombre.

    pvlib guarda los nombres con «_» en lugar de guiones, puntos y espacios («CS6K-300MS» es
    «CS6K_300MS»), así que el texto y los nombres se comparan sin distinguir mayúsculas y con esos
    signos unificados.
    """
    palabras = [_clave_cec(p) for p in texto.split()]
    palabras = [p for p in palabras if p.strip("_")]
    if not palabras:
        return []
    return [c for c in dict.fromkeys(_base_cec().columns) if all(p in _clave_cec(c) for p in palabras)][:int(limite)]


def _clave_cec(texto):
    """Forma de comparación de un nombre CEC: minúsculas y cualquier signo que no sea letra o cifra como «_»."""
    return re.sub(r"[^0-9a-z]", "_", str(texto).lower())


def modulo_cec(nombre):
    """Crea un ModuloFV con los datos de hoja de la base CEC de pvlib (sin extraer los parámetros)."""
    base = _base_cec()
    if nombre not in base.columns:
        raise ValueError(f"El módulo «{nombre}» no está en la base CEC; búscalo con el selector.")
    d = base[nombre]
    if getattr(d, "ndim", 1) == 2:
        d = d.iloc[:, 0]
    return ModuloFV(nombre=nombre, Voc=float(d["V_oc_ref"]), Isc=float(d["I_sc_ref"]), Vmp=float(d["V_mp_ref"]),
                    Imp=float(d["I_mp_ref"]), Ns=int(d["N_s"]), alfa_sc=float(d["alpha_sc"]), beta_oc=float(d["beta_oc"]))


def resolver_modulo(nombre):
    """Módulo (con parámetros extraídos) del catálogo propio o de la base CEC.

    Raises:
        ValueError: si el nombre no existe en ninguna de las dos fuentes.
    """
    if nombre in CATALOGO:
        return modulo_de_catalogo(nombre)
    if nombre not in _cache_mod:
        if nombre not in _base_cec().columns:
            raise ValueError(f"Módulo desconocido «{nombre}». Opciones del catálogo: {', '.join(CATALOGO)}; o búscalo en la base CEC.")
        _cache_mod[nombre] = modulo_cec(nombre).extraer()
    return _cache_mod[nombre]


# ================================================================= Recurso solar (pvlib)


def posicion_solar(lat, lon, tiempos, altitud=0.0):
    """Posición solar con el algoritmo SPA de pvlib. Devuelve elevación, cenit y acimut."""
    import pvlib
    return pvlib.solarposition.get_solarposition(tiempos, lat, lon, altitude=altitud)


def irradiancia_poa(lat, lon, tiempos, inclinacion, acimut, altitud=0.0, modelo="haydavies"):
    """Irradiancia en plano inclinado con cielo despejado (Ineichen) y transposición pvlib."""
    import pvlib
    loc = pvlib.location.Location(lat, lon, altitude=altitud)
    cs = loc.get_clearsky(tiempos, model="ineichen")
    sol = loc.get_solarposition(tiempos)
    dni_extra = pvlib.irradiance.get_extra_radiation(tiempos)
    return pvlib.irradiance.get_total_irradiance(
        inclinacion, acimut, sol["apparent_zenith"], sol["azimuth"], cs["dni"], cs["ghi"], cs["dhi"],
        dni_extra=dni_extra, model=modelo), sol, cs


def temperatura_modulo(poa, t_amb, viento=1.0, montaje="open_rack_glass_polymer"):
    """Temperatura de celda con el modelo térmico SAPM (pvlib).

    T_m = G·exp(a + b·v) + T_a;   T_c = T_m + (G/1000)·ΔT
    """
    import pvlib
    par = pvlib.temperature.TEMPERATURE_MODEL_PARAMETERS["sapm"][montaje]
    return pvlib.temperature.sapm_cell(poa, t_amb, viento, **par)


# ================================================================= Bloques


def _opciones():
    return list(CATALOGO.keys())


class _ArregloBase(Elemento):
    """Arreglo como fuente de corriente no lineal I(V, G, T), linealizada con Newton."""
    no_lineal = True
    categoria = "Fotovoltaica"
    color = "fv"
    puertos_e = ["p", "n"]
    entradas = ["G", "T"]
    salidas = ["i", "v", "pot"]

    def estampar_din(self, M, b, z, t, x, u):
        p, n = self.n["p"], self.n["n"]
        v = z[p] - z[n]
        I, g = self.corriente(v, float(u[0]), float(u[1]))
        # Lineal alrededor de v: I(v') ≈ I + g·(v' - v). Sale de p por dentro del arreglo.
        M[p, p] -= g; M[n, n] -= g; M[p, n] += g; M[n, p] += g
        i0 = I - g * v
        b[p] += i0
        b[n] -= i0

    def salida_e(self, z, t, x, u):
        v = z[self.n["p"]] - z[self.n["n"]]
        I, _ = self.corriente(v, float(u[0]), float(u[1]))
        return [I, v, v * I]


@registrar
class ArregloFV(_ArregloBase):
    """Arreglo FV uniforme de N_s módulos en serie y N_p strings en paralelo.

    Modelo de un diodo con parámetros extraídos de la hoja de datos (ver `ModuloFV`).
    I_arreglo(V) = N_p·I_mód(V/N_s).  Entradas: G [W/m²] y T de celda [°C].
    Salidas: corriente, tensión y potencia del arreglo.
    """
    tipo = "arreglo_fv"; nombre = "Arreglo FV"
    parametros = [P("modulo", "Módulo", _opciones()[0], tipo="opcion", opciones=_opciones()),
                  P("Ns", "Módulos en serie", 12), P("Np", "Strings en paralelo", 20)]

    def preparar(self):
        """Prepara el estado interno a partir de los parámetros del elemento."""
        self.m = modulo_de_catalogo(self.p["modulo"])
        self.ns, self.np = float(self.p["Ns"]), float(self.p["Np"])
        self._clave = None
        self.I = self.m.Imp
        self.par = None

    def corriente(self, V, G, T):
        """Corriente del elemento para la tensión, la irradiancia y la temperatura dadas."""
        if self._clave != (G, T):
            IL, I0, Rs, Rsh, a = parametros_operacion(self.m, G, T)
            self.par = (float(IL), float(I0), float(Rs), float(Rsh), float(a))
            self._clave = (G, T)
        IL, I0, Rs, Rsh, a = self.par
        v = V / self.ns
        I = self.I
        # Newton sobre la ecuación implícita; parte de la última solución
        for _ in range(40):
            xx = v + I * Rs
            e = I0 * math.exp(min(xx / a, 600.0))
            F = IL - e + I0 - xx / Rsh - I
            dF = -e * Rs / a - Rs / Rsh - 1.0
            d = F / dF
            I -= d
            if abs(d) < 1e-11:
                break
        self.I = I
        xx = v + I * Rs
        gd = I0 * math.exp(min(xx / a, 600.0)) / a + 1.0 / Rsh
        dIdV = -gd / (1.0 + Rs * gd)
        return self.np * I, dIdV * self.np / self.ns


@registrar
class ArregloFVSombreado(_ArregloBase):
    """Arreglo FV con sombreado parcial y diodos de bypass.

    `sombra` da un factor por sub-módulo de un string (el resto de strings según `n_sombra`).
    Sub-módulo: V_k(I) = max(V_sub(I), -V_d);  string: V(I) = Σ V_k(I);  arreglo: I(V) = Σ I_s(V).
    Una curva se tabula por cada G, T y se interpola en cada evaluación.
    """
    tipo = "arreglo_fv_sombra"; nombre = "Arreglo FV con sombra"
    parametros = [P("modulo", "Módulo", _opciones()[0], tipo="opcion", opciones=_opciones()),
                  P("Ns", "Módulos en serie", 12), P("Np", "Strings en paralelo", 20),
                  P("sombra", "Factor por sub-módulo", "[1]*24 + [0.4]*12"),
                  P("n_sombra", "Strings con sombra", 5), P("bypass", "Bypass por módulo", 3), P("Vd", "Tensión de bypass", 0.7, "V")]

    def preparar(self):
        """Prepara el estado interno a partir de los parámetros del elemento."""
        self.m = modulo_de_catalogo(self.p["modulo"])
        self._clave = None
        self.tabla = None

    def _tabular(self, G, T):
        ns = int(self.p["Ns"])
        np_ = int(self.p["Np"])
        byp = int(self.p["bypass"])
        total = ns * byp
        f = np.ones(total)
        s = np.ravel(self.p["sombra"]).astype(float)
        f[: min(total, s.size)] = s[: total]
        n_s = int(self.p["n_sombra"])
        strings = [f.tolist()] * n_s + [[1.0] * total] * (np_ - n_s)
        V, I = curva_arreglo(self.m, strings, T, G, byp, float(self.p["Vd"]))
        self.tabla = (V.tolist(), I.tolist())
        self._clave = (G, T)

    def corriente(self, V, G, T):
        """Corriente del elemento para la tensión, la irradiancia y la temperatura dadas."""
        if self._clave is None or abs(G - self._clave[0]) > 0.5 or abs(T - self._clave[1]) > 0.2:
            self._tabular(G, T)
        vv, ii = self.tabla
        if V <= vv[0]:
            return ii[0], 0.0
        if V >= vv[-1]:
            return 0.0, (ii[-1] - ii[-2]) / (vv[-1] - vv[-2])
        k = bisect.bisect_right(vv, V)
        s = (ii[k] - ii[k - 1]) / (vv[k] - vv[k - 1])
        return ii[k - 1] + s * (V - vv[k - 1]), s


@registrar
class MPPT(Bloque):
    """Seguimiento del punto de máxima potencia. Se ejecuta cada T_s y actúa sobre d de un boost.

    Perturbar y observar:  si ΔP·ΔV > 0 subir V (d -= Δd);  si no, bajar V (d += Δd)
    Conductancia incremental: dI/dV + I/V = 0 en el MPP
        dI/dV > -I/V  →  a la izquierda del MPP: subir V (d -= Δd)
        dI/dV < -I/V  →  a la derecha del MPP: bajar V (d += Δd)
    En un boost, subir d baja la tensión de entrada.
    """
    tipo = "mppt"; nombre = "MPPT"; categoria = "Fotovoltaica"; color = "fv"
    parametros = [P("metodo", "Método", "Perturbar y observar", tipo="opcion",
                    opciones=["Perturbar y observar", "Conductancia incremental"]),
                  P("Ts", "Periodo", 5e-3, "s"), P("dd", "Paso de ciclo Δd", 0.002), P("d0", "Ciclo inicial", 0.4),
                  P("d_min", "Ciclo mínimo", 0.05), P("d_max", "Ciclo máximo", 0.9)]
    entradas = ["v", "i"]; salidas = ["d"]; directo = False

    def preparar(self):
        """Prepara el estado interno a partir de los parámetros del elemento."""
        self.d = float(self.p["d0"])
        self.t_prox = float(self.p["Ts"])
        self.v_a = self.i_a = self.p_a = None

    def reiniciar(self):
        """Restablece el estado interno del bloque."""
        self.preparar()

    def salida(self, t, x, u):
        """Calcula la salida del bloque en el instante dado."""
        return [self.d]

    def proximo_evento(self, t):
        """Instante del próximo evento del bloque."""
        return self.t_prox

    def paso_mayor(self, t, x, u, y):
        """Paso máximo admisible desde el instante actual."""
        if t < self.t_prox - 1e-12:
            return False
        self.t_prox += float(self.p["Ts"])
        v, i = float(u[0]), float(u[1])
        pot = v * i
        if self.v_a is None:
            self.v_a, self.i_a, self.p_a = v, i, pot
            self.d += float(self.p["dd"])
            return True
        dV, dI, dP = v - self.v_a, i - self.i_a, pot - self.p_a
        dd = float(self.p["dd"])
        if self.p["metodo"].startswith("Perturbar"):
            subir_v = (dP > 0) == (dV > 0)
            nuevo = self.d + (-dd if subir_v else dd)
        else:
            if abs(dV) < 1e-9:
                nuevo = self.d if abs(dI) < 1e-9 else self.d + (-dd if dI > 0 else dd)
            else:
                g = dI / dV + i / max(v, 1e-6)
                nuevo = self.d if abs(g) < 1e-6 else self.d + (-dd if g > 0 else dd)
        self.d = min(float(self.p["d_max"]), max(float(self.p["d_min"]), nuevo))
        self.v_a, self.i_a, self.p_a = v, i, pot
        return True


@registrar
class TermicoModulo(Bloque):
    """Modelo térmico del módulo (SAPM, pvlib): T_c = G·exp(a + b·v) + T_a + (G/1000)·ΔT."""
    tipo = "termico_modulo"; nombre = "Temperatura de celda"; categoria = "Fotovoltaica"; color = "fv"
    parametros = [P("viento", "Viento", 1.0, "m/s"),
                  P("montaje", "Montaje", "open_rack_glass_polymer", tipo="opcion",
                    opciones=["open_rack_glass_polymer", "open_rack_glass_glass", "close_mount_glass_glass",
                              "insulated_back_glass_polymer"])]
    entradas = ["G", "T_amb"]; salidas = ["T_c"]

    def preparar(self):
        """Prepara el estado interno a partir de los parámetros del elemento."""
        import pvlib
        par = pvlib.temperature.TEMPERATURE_MODEL_PARAMETERS["sapm"][self.p["montaje"]]
        self.a, self.b, self.dT = par["a"], par["b"], par["deltaT"]

    def salida(self, t, x, u):
        """Calcula la salida del bloque en el instante dado."""
        G, Ta = float(u[0]), float(u[1])
        return [G * math.exp(self.a + self.b * float(self.p["viento"])) + Ta + G / 1000.0 * self.dT]


@registrar
class SolPOA(Bloque):
    """Irradiancia en plano inclinado a lo largo de un día (cielo despejado, pvlib).

    Se calcula una vez al iniciar con posición solar SPA, Ineichen y transposición Hay-Davies.
    `escala` indica cuántos segundos reales son 1 s de simulación.
    """
    tipo = "sol_poa"; nombre = "Sol en plano inclinado"; categoria = "Fotovoltaica"; color = "fuente"
    parametros = [P("lat", "Latitud", 40.4, "°"), P("lon", "Longitud", -3.7, "°"), P("incl", "Inclinación", 30.0, "°"),
                  P("acim", "Acimut", 180.0, "°"), P("fecha", "Fecha", "2026-06-21", tipo="texto"),
                  P("hora0", "Hora inicial", 8.0, "h"), P("escala", "Escala de tiempo", 3600.0)]
    salidas = ["G"]; directo = False

    def preparar(self):
        """Prepara el estado interno a partir de los parámetros del elemento."""
        import pandas as pd
        t = pd.date_range(self.p["fecha"], periods=24 * 12 + 1, freq="5min", tz="UTC")
        poa, _, _ = irradiancia_poa(float(self.p["lat"]), float(self.p["lon"]), t, float(self.p["incl"]), float(self.p["acim"]))
        self.h = (np.arange(t.size) * 5.0 / 60.0).tolist()
        self.g = poa["poa_global"].fillna(0.0).clip(lower=0.0).tolist()

    def salida(self, t, x, u):
        """Calcula la salida del bloque en el instante dado."""
        h = float(self.p["hora0"]) + t * float(self.p["escala"]) / 3600.0
        return [float(np.interp(h, self.h, self.g))]
