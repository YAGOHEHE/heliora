"""Bloques de señal y control: matemática, lógica, tablas n-D, PID, discontinuidades y tiempo discreto.

Los bloques continuos y algebraicos siguen el patrón de `sim/bloques_genericos.py`. Los bloques de
tiempo discreto heredan de `BloqueDiscreto` (`sim/nucleo.py`): su salida es un valor retenido que
solo cambia en los instantes t_k = offset + k·Ts, en los que el solver cae exactamente.

Referencias (edición y apartado sin verificar contra el texto; ver MECHANICS.md):
    PID con filtro de derivada y anti-windup por retrocálculo: K. J. Åström y T. Hägglund,
    *Advanced PID Control*, ISA, 2006.
    Discretización (Euler hacia delante para el integral, hacia atrás para la derivada): K. J. Åström
    y B. Wittenmark, *Computer-Controlled Systems*, Prentice Hall, 1997.
"""
from __future__ import annotations

from typing_extensions import override
import math

import numpy as np

from ..sim.nucleo import Bloque, BloqueDiscreto, P, _ESPACIO_MAT, registrar

INF = math.inf

# ================================================================= Algebraicos


def _escalar_o_vector(v):
    """Devuelve un float si el arreglo tiene un solo elemento; si no, el arreglo."""
    a = np.asarray(v, dtype=float)
    return float(a.ravel()[0]) if a.size == 1 else a


_UNARIAS = {
    "sin": np.sin, "cos": np.cos, "tan": np.tan, "asin": np.arcsin, "acos": np.arccos, "atan": np.arctan,
    "exp": np.exp, "log": np.log, "log10": np.log10, "sqrt": np.sqrt, "sign": np.sign,
    "cuadrado": np.square, "reciproco": np.reciprocal, "grados": np.degrees, "radianes": np.radians,
}
_BINARIAS = {"atan2": np.arctan2, "pow": np.power, "mod": np.mod, "rem": np.fmod}


@registrar
class Matematica(Bloque):
    """Función matemática.   y = f(u1)  o  y = f(u1, u2).

    Unarias: sin, cos, tan, asin, acos, atan, exp, log (natural), log10, sqrt, sign, cuadrado (u²),
    reciproco (1/u), grados (rad → °), radianes (° → rad). Binarias: atan2(u1, u2), pow (u1^u2),
    mod (resto con el signo del divisor) y rem (resto con el signo del dividendo). Ángulos en
    radianes. Las funciones unarias solo leen u1.
    """
    tipo = "matematica"; nombre = "Función matemática"; categoria = "Matemática"
    parametros = [P("funcion", "Función", "sin", tipo="opcion", opciones=list(_UNARIAS) + list(_BINARIAS))]
    entradas = ["u1", "u2"]; salidas = ["y"]

    @override
    def preparar(self):
        f = str(self.p["funcion"])
        if f not in _UNARIAS and f not in _BINARIAS:
            raise ValueError(f"Bloque «{self.etiqueta}»: función desconocida «{f}»")
        self.binaria = f in _BINARIAS
        self.fn = _BINARIAS.get(f) or _UNARIAS[f]

    @override
    def salida(self, t, x, u):
        if self.binaria:
            return [_escalar_o_vector(self.fn(np.asarray(u[0], dtype=float), np.asarray(u[1], dtype=float)))]
        return [_escalar_o_vector(self.fn(np.asarray(u[0], dtype=float)))]


_OPERADORES = {"<": np.less, "<=": np.less_equal, ">": np.greater, ">=": np.greater_equal,
               "==": np.equal, "!=": np.not_equal}


@registrar
class Comparador(Bloque):
    """Comparador.   y = 1 si (u1 op u2);  y = 0 en otro caso.   op ∈ {<, <=, >, >=, ==, !=}.

    Con paso variable y una desigualdad, mantiene el resultado durante el paso y el solver localiza
    el instante en que u1 - u2 cambia de signo.
    """
    tipo = "comparador"; nombre = "Comparador"; categoria = "Matemática"
    parametros = [P("op", "Operación", ">=", tipo="opcion", opciones=list(_OPERADORES))]
    entradas = ["u1", "u2"]; salidas = ["y"]
    de_evento = True

    @override
    def preparar(self):
        if self.p["op"] not in _OPERADORES:
            raise ValueError(f"Bloque «{self.etiqueta}»: operación desconocida «{self.p['op']}»")
        self.fn = _OPERADORES[self.p["op"]]
        self.desigualdad = self.p["op"] not in ("==", "!=")
        self.res = None

    @override
    def reiniciar(self):
        self.res = None

    def _evaluar(self, u):
        return _escalar_o_vector(self.fn(np.asarray(u[0], dtype=float), np.asarray(u[1], dtype=float)).astype(float))

    @override
    def salida(self, t, x, u):
        if self.usar_zc and self.desigualdad and self.res is not None and np.isscalar(self.res):
            return [self.res]
        return [self._evaluar(u)]

    @override
    def cruce(self, t, x, u):
        if not self.desigualdad or not np.isscalar(u[0]) or not np.isscalar(u[1]):
            return []
        return [float(u[0]) - float(u[1])]

    @override
    def paso_mayor(self, t, x, u, y):
        if not self.usar_zc or not self.desigualdad:
            return False
        r = self._evaluar(u)
        c = self.res is not None and not np.array_equal(r, self.res)
        self.res = r
        return c


_LOGICAS = ("AND", "OR", "NOT", "XOR", "NAND", "NOR")


@registrar
class Logica(Bloque):
    """Operación lógica.   y ∈ {0, 1}.

    Cada entrada es verdadera si u > 0.5. AND, OR, XOR (número impar de verdaderas), NAND y NOR se
    aplican a todas las entradas; NOT solo a u1.
    """
    tipo = "logica"; nombre = "Operación lógica"; categoria = "Matemática"
    parametros = [P("op", "Operación", "AND", tipo="opcion", opciones=list(_LOGICAS)), P("n", "Entradas", 2)]
    salidas = ["y"]
    puertos_dinamicos = {"param": "n", "modo": "valor", "prefijo": "u"}

    @classmethod
    @override
    def puertos(cls, params=None):
        n = int(float((params or {}).get("n", 2)))
        return {"entradas": [f"u{i+1}" for i in range(n)], "salidas": ["y"], "e": [], "e3": []}

    @override
    def preparar(self):
        if self.p["op"] not in _LOGICAS:
            raise ValueError(f"Bloque «{self.etiqueta}»: operación desconocida «{self.p['op']}»")

    @override
    def salida(self, t, x, u):
        v = [np.asarray(a, dtype=float) > 0.5 for a in u]
        op = self.p["op"]
        if op == "NOT":
            r = ~v[0]
        elif op in ("AND", "NAND"):
            r = np.logical_and.reduce(v)
        elif op in ("OR", "NOR"):
            r = np.logical_or.reduce(v)
        else:
            r = np.logical_xor.reduce(v)
        if op in ("NAND", "NOR"):
            r = ~r
        return [_escalar_o_vector(np.asarray(r, dtype=float))]


@registrar
class MinMax(Bloque):
    """Mínimo o máximo.   y = min(u_i)  o  y = max(u_i).

    Con paso variable y entradas escalares, mantiene la entrada elegida durante el paso y el solver
    localiza el instante en que otra entrada pasa a ser la menor (o la mayor).
    """
    tipo = "min_max"; nombre = "Mínimo / máximo"; categoria = "Matemática"
    parametros = [P("modo", "Modo", "max", tipo="opcion", opciones=["min", "max"]), P("n", "Entradas", 2)]
    salidas = ["y"]
    puertos_dinamicos = {"param": "n", "modo": "valor", "prefijo": "u"}
    de_evento = True

    @classmethod
    @override
    def puertos(cls, params=None):
        n = int(float((params or {}).get("n", 2)))
        return {"entradas": [f"u{i+1}" for i in range(n)], "salidas": ["y"], "e": [], "e3": []}

    @override
    def preparar(self):
        self.signo = 1.0 if self.p["modo"] == "max" else -1.0
        self.sel = None

    @override
    def reiniciar(self):
        self.sel = None

    def _escalares(self, u):
        return all(np.ndim(a) == 0 for a in u)

    def _elegir(self, u):
        return int(np.argmax([self.signo * float(a) for a in u]))

    @override
    def salida(self, t, x, u):
        if not self._escalares(u):
            f = np.maximum if self.signo > 0 else np.minimum
            return [_escalar_o_vector(f.reduce([np.asarray(a, dtype=float) for a in u]))]
        i = self.sel if (self.usar_zc and self.sel is not None and self.sel < len(u)) else self._elegir(u)
        return [float(u[i])]

    @override
    def cruce(self, t, x, u):
        if not self._escalares(u) or len(u) < 2:
            return []
        i = self.sel if self.sel is not None and self.sel < len(u) else self._elegir(u)
        # Señal positiva mientras la elegida sigue siendo la mejor frente a cada una de las otras
        return [self.signo * (float(u[i]) - float(u[j])) for j in range(len(u)) if j != i]

    @override
    def paso_mayor(self, t, x, u, y):
        if not self.usar_zc or not self._escalares(u):
            return False
        s = self._elegir(u)
        c = self.sel is not None and s != self.sel
        self.sel = s
        return c


@registrar
class ProductoMatricial(Bloque):
    """Producto matricial.   y = A·u1  (A constante)   o   y = U1·U2  (dos entradas).

    Las formas deben ser compatibles; si no lo son, el error dice el nombre del bloque y las formas.
    """
    tipo = "producto_matricial"; nombre = "Producto matricial"; categoria = "Matemática"
    parametros = [P("modo", "Modo", "A·u", tipo="opcion", opciones=["A·u", "U1·U2"]),
                  P("A", "Matriz A", "[[1, 0], [0, 1]]")]
    entradas = ["u1", "u2"]; salidas = ["y"]

    @override
    def preparar(self):
        self.A = np.atleast_2d(np.asarray(self.p["A"], dtype=float))

    @override
    def salida(self, t, x, u):
        if self.p["modo"] == "U1·U2":
            a, b = np.asarray(u[0], dtype=float), np.asarray(u[1], dtype=float)
        else:
            a, b = self.A, np.asarray(u[0], dtype=float)
        try:
            r = a @ b if a.ndim and b.ndim else a * b
        except ValueError:
            raise ValueError(f"Bloque «{self.etiqueta}»: formas incompatibles {a.shape} y {b.shape}") from None
        return [_escalar_o_vector(r)]


def _evaluar_texto(valor, espacio):
    """Evalúa una expresión que puede dar listas de distinto largo (puntos de ruptura por eje)."""
    if not isinstance(valor, str):
        return valor
    ns = dict(_ESPACIO_MAT)
    ns.update(espacio or {})
    return eval(valor, {"__builtins__": {}}, ns)  # expresión escrita por el propio usuario


class _TablaN(Bloque):
    """Interpolación lineal multidimensional sobre una rejilla regular."""

    def _montar(self, puntos, tabla):
        from scipy.interpolate import RegularGridInterpolator
        self.ejes = [np.ravel(np.asarray(p, dtype=float)) for p in puntos]
        tabla = np.asarray(tabla, dtype=float)
        forma = tuple(e.size for e in self.ejes)
        if tabla.shape != forma:
            raise ValueError(f"Bloque «{self.etiqueta}»: la tabla tiene forma {tabla.shape} y los puntos piden {forma}")
        for e in self.ejes:
            if e.size < 2 or np.any(np.diff(e) <= 0):
                raise ValueError(f"Bloque «{self.etiqueta}»: cada eje necesita al menos 2 puntos crecientes")
        self.saturar = self.p["fuera"] == "saturar"
        self.interp = RegularGridInterpolator(self.ejes, tabla, method="linear", bounds_error=False, fill_value=None)

    def salida(self, t, x, u):
        q = np.array([float(a) for a in u])
        if self.saturar:
            q = np.array([min(e[-1], max(e[0], v)) for e, v in zip(self.ejes, q)])
        return [float(self.interp(q[None, :])[0])]


@registrar
class TablaBusqueda2D(_TablaN):
    """Lookup table 2-D.   y = interpolación bilineal de la tabla T[i, j] en (u1, u2).

    T[i, j] es el valor en (x1[i], x2[j]). Fuera de la rejilla satura o extrapola linealmente.
    """
    tipo = "lookup_2d"; nombre = "Lookup table 2-D"; categoria = "Discontinuidades"
    parametros = [P("x1", "Puntos del eje 1", "[0, 1]"), P("x2", "Puntos del eje 2", "[0, 1]"),
                  P("tabla", "Tabla", "[[0, 1], [1, 2]]"),
                  P("fuera", "Fuera de rango", "saturar", tipo="opcion", opciones=["saturar", "extrapolar"])]
    entradas = ["u1", "u2"]; salidas = ["y"]

    @override
    def preparar(self):
        self._montar([self.p["x1"], self.p["x2"]], self.p["tabla"])


@registrar
class TablaBusquedaND(_TablaN):
    """Lookup table n-D.   y = interpolación multilineal de la tabla en (u1, ..., un).

    `puntos` es una lista con los puntos de ruptura de cada eje (pueden tener distinto largo) y
    `tabla` un arreglo de n dimensiones con una dimensión por eje.
    """
    tipo = "lookup_nd"; nombre = "Lookup table n-D"; categoria = "Discontinuidades"
    parametros = [P("n", "Entradas", 3), P("puntos", "Puntos por eje", "[[0, 1], [0, 1], [0, 1]]", tipo="texto"),
                  P("tabla", "Tabla", "np.arange(8).reshape(2, 2, 2)", tipo="texto"),
                  P("fuera", "Fuera de rango", "saturar", tipo="opcion", opciones=["saturar", "extrapolar"])]
    salidas = ["y"]
    puertos_dinamicos = {"param": "n", "modo": "valor", "prefijo": "u"}
    # Parámetros de texto que se evalúan con el espacio de trabajo
    parametros_evaluados = ("puntos", "tabla")

    @classmethod
    @override
    def puertos(cls, params=None):
        n = int(float((params or {}).get("n", 3)))
        return {"entradas": [f"u{i+1}" for i in range(n)], "salidas": ["y"], "e": [], "e3": []}

    @override
    def preparar(self):
        try:
            puntos = _evaluar_texto(self.p["puntos"], self.espacio)
            tabla = _evaluar_texto(self.p["tabla"], self.espacio)
        except Exception as e:
            raise ValueError(f"Bloque «{self.etiqueta}»: no se pudieron evaluar los puntos o la tabla ({e})") from e
        if len(puntos) != int(float(self.p["n"])):
            raise ValueError(f"Bloque «{self.etiqueta}»: hay {len(puntos)} ejes y {int(float(self.p['n']))} entradas")
        self._montar(puntos, tabla)


# ================================================================= Discontinuidades


@registrar
class ZonaMuerta(Bloque):
    """Zona muerta.   y = 0 si lo <= u <= hi;   y = u - hi si u > hi;   y = u - lo si u < lo.

    Con paso variable mantiene su zona durante el paso y el solver localiza los cruces de u por lo y hi.
    """
    tipo = "zona_muerta"; nombre = "Zona muerta"; categoria = "Discontinuidades"
    parametros = [P("lo", "Inicio de la zona", -0.5), P("hi", "Fin de la zona", 0.5)]
    entradas = ["u"]; salidas = ["y"]
    de_evento = True

    @override
    def preparar(self):
        if float(self.p["lo"]) > float(self.p["hi"]):
            raise ValueError(f"Bloque «{self.etiqueta}»: el inicio de la zona debe ser menor que el fin")
        self.zona = None

    @override
    def reiniciar(self):
        self.zona = None

    def _zona(self, v):
        return -1 if v < self.p["lo"] else (1 if v > self.p["hi"] else 0)

    def _aplicar(self, v, z):
        return v - self.p["hi"] if z > 0 else (v - self.p["lo"] if z < 0 else 0.0)

    @override
    def salida(self, t, x, u):
        a = u[0]
        if isinstance(a, np.ndarray):
            lo, hi = self.p["lo"], self.p["hi"]
            return [np.where(a > hi, a - hi, np.where(a < lo, a - lo, 0.0))]
        z = self.zona if (self.usar_zc and self.zona is not None) else self._zona(a)
        return [self._aplicar(float(a), z)]

    @override
    def cruce(self, t, x, u):
        if isinstance(u[0], np.ndarray):
            return []
        return [float(u[0]) - float(self.p["lo"]), float(u[0]) - float(self.p["hi"])]

    @override
    def paso_mayor(self, t, x, u, y):
        if not self.usar_zc or isinstance(u[0], np.ndarray):
            return False
        z = self._zona(float(u[0]))
        c = self.zona is not None and z != self.zona
        self.zona = z
        return c


@registrar
class Rele(Bloque):
    """Relé con histéresis (disparador de Schmitt).

    Apagado pasa a encendido cuando u >= punto_subida; encendido pasa a apagado cuando
    u <= punto_bajada (punto_subida > punto_bajada). Salida: salida_on o salida_off. El modo es un
    estado discreto; con paso variable el solver localiza el cruce con el umbral del modo activo.
    """
    tipo = "rele"; nombre = "Relé"; categoria = "Discontinuidades"
    parametros = [P("punto_subida", "Umbral de encendido", 0.5), P("punto_bajada", "Umbral de apagado", -0.5),
                  P("salida_on", "Salida encendido", 1.0), P("salida_off", "Salida apagado", 0.0),
                  P("inicial", "Estado inicial", "apagado", tipo="opcion", opciones=["apagado", "encendido"])]
    entradas = ["u"]; salidas = ["y"]
    de_evento = True

    @override
    def preparar(self):
        if not float(self.p["punto_subida"]) > float(self.p["punto_bajada"]):
            raise ValueError(f"Bloque «{self.etiqueta}»: el umbral de encendido debe superar al de apagado")
        self.on = None

    @override
    def reiniciar(self):
        self.on = None

    def _decidir(self, v, on):
        if on is None:
            on = self.p["inicial"] == "encendido"
        if not on and v >= self.p["punto_subida"]:
            return True
        if on and v <= self.p["punto_bajada"]:
            return False
        return on

    @override
    def salida(self, t, x, u):
        on = self.on if self.on is not None else self._decidir(float(u[0]), None)
        return [self.p["salida_on"] if on else self.p["salida_off"]]

    @override
    def cruce(self, t, x, u):
        on = self.on if self.on is not None else self._decidir(float(u[0]), None)
        return [float(u[0]) - float(self.p["punto_bajada" if on else "punto_subida"])]

    @override
    def paso_mayor(self, t, x, u, y):
        nuevo = self._decidir(float(u[0]), self.on)
        c = self.on is not None and nuevo != self.on
        self.on = nuevo
        return c


@registrar
class LimitadorTasa(Bloque):
    """Limitador de tasa (versión continua).   dy/dt = max(R_baja, min(R_alta, k·(u - y))).

    Con k grande la salida sigue a la entrada salvo cuando su pendiente supera los límites, y
    entonces sube (o baja) exactamente con pendiente R_alta (o R_baja). Es una aproximación continua
    con una constante de tiempo 1/k; la versión exacta por muestra es `limitador_tasa_discreto`.
    """
    tipo = "limitador_tasa"; nombre = "Limitador de tasa"; categoria = "Discontinuidades"
    parametros = [P("R_alta", "Pendiente máxima de subida", 1.0, ayuda="unidades de la señal por segundo, >= 0"),
                  P("R_baja", "Pendiente máxima de bajada", -1.0, ayuda="unidades de la señal por segundo, <= 0"),
                  P("k", "Ganancia de seguimiento", 1e4, "1/s"), P("y0", "Salida inicial", 0.0)]
    entradas = ["u"]; salidas = ["y"]; directo = False

    @override
    def preparar(self):
        self.ra, self.rb, self.k = float(self.p["R_alta"]), float(self.p["R_baja"]), float(self.p["k"])
        if self.ra < 0 or self.rb > 0 or self.k <= 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: hace falta R_alta >= 0, R_baja <= 0 y k > 0")

    @override
    def n_estados(self):
        return 1

    @override
    def x0(self):
        return [float(self.p["y0"])]

    @override
    def salida(self, t, x, u):
        return [float(x[0])]

    @override
    def derivada(self, t, x, u):
        return [max(self.rb, min(self.ra, self.k * (float(u[0]) - x[0])))]


# ================================================================= PID


class _GananciasPID:
    """Parámetros y ecuaciones comunes del PID continuo y del discreto."""

    PARAMS = [
        P("Kp", "Kp", 1.0), P("Ki", "Ki", 0.0, "1/s"), P("Kd", "Kd", 0.0, "s"),
        P("N", "Filtro de derivada N", 10.0, "1/s", ayuda="polo del filtro: Kd·N·s/(s + N)"),
        P("Tt", "Tiempo de seguimiento", 0.0, "s", ayuda="anti-windup; 0 = automático √(Kd/Ki), o Kp/Ki sin derivada"),
        P("u_min", "Salida mínima", "-inf"), P("u_max", "Salida máxima", "inf"),
        P("b", "Peso de consigna en P", 1.0), P("c", "Peso de consigna en D", 1.0,
                                                 ayuda="0 = derivada sobre la medida (sin golpe de derivada)"),
        P("I0", "Integral inicial", 0.0),
    ]

    def _leer(self):
        p = self.p
        self.Kp, self.Ki, self.Kd, self.N = (float(p[k]) for k in ("Kp", "Ki", "Kd", "N"))
        self.umin, self.umax = float(p["u_min"]), float(p["u_max"])
        self.bw, self.cw = float(p["b"]), float(p["c"])
        if self.Kd and self.N <= 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: con derivada, N debe ser positivo")
        Tt = float(p["Tt"])
        if Tt <= 0:
            # Åström y Hägglund proponen Tt = √(Ti·Td), que en ganancias es √(Kd/Ki)
            if self.Kd > 0 and self.Ki > 0:
                Tt = math.sqrt(self.Kd / self.Ki)
            elif self.Ki > 0 and self.Kp > 0:
                Tt = self.Kp / self.Ki
            else:
                Tt = 1.0
        self.Tt = Tt
        self.aw = math.isfinite(self.umin) or math.isfinite(self.umax)

    def _saturar(self, v):
        return min(self.umax, max(self.umin, v))


@registrar
class PID(Bloque, _GananciasPID):
    """PID paralelo con filtro de derivada y anti-windup por retrocálculo.

    e = r - y;   P = Kp·(b·r - y);   e_d = c·r - y
    dd/dt = N·(e_d - d);   D = Kd·N·(e_d - d)          (derivada filtrada Kd·N·s/(s + N))
    u = P + I + D;   salida = sat(u, u_min, u_max)
    dI/dt = Ki·e + (sat(u) - u)/Tt                      (el retrocálculo frena el integral al saturar)

    Sin límites el anti-windup no actúa. Estados: I, d. Generaliza el patrón de `pi` (potencia).
    Fuente: Åström y Hägglund, *Advanced PID Control*, ISA, 2006.
    """
    tipo = "pid"; nombre = "PID"; categoria = "Continuo"
    parametros = _GananciasPID.PARAMS
    entradas = ["r", "y"]; salidas = ["u"]

    @override
    def preparar(self):
        self._leer()

    @override
    def n_estados(self):
        return 2

    @override
    def x0(self):
        return [float(self.p["I0"]), 0.0]

    def _ley(self, x, u):
        r, y = float(u[0]), float(u[1])
        ed = self.cw * r - y
        v = self.Kp * (self.bw * r - y) + x[0] + self.Kd * self.N * (ed - x[1])
        return r - y, ed, v, self._saturar(v)

    @override
    def salida(self, t, x, u):
        return [self._ley(x, u)[3]]

    @override
    def derivada(self, t, x, u):
        e, ed, v, vs = self._ley(x, u)
        dI = self.Ki * e + ((vs - v) / self.Tt if self.aw else 0.0)
        return [dI, self.N * (ed - x[1]) if self.Kd else 0.0]


# ================================================================= Tiempo discreto


class _Retencion(BloqueDiscreto):
    """Muestrea la entrada en cada instante y la retiene hasta el siguiente (ZOH)."""
    categoria = "Discreto"
    parametros = BloqueDiscreto.PARAMS_TS + [P("y0", "Salida antes del primer muestreo", 0.0)]
    entradas = ["u"]; salidas = ["y"]

    def reiniciar_discreto(self):
        self.y = self.p["y0"]

    def tic(self, t, u):
        return self._retener(u[0].copy() if isinstance(u[0], np.ndarray) else float(u[0]))


@registrar
class Muestreo(_Retencion):
    """Muestreo (continuo → discreto).   y = u(t_k)   para t_k <= t < t_k + Ts,   t_k = offset + k·Ts."""
    tipo = "muestreo"; nombre = "Muestreo"


@registrar
class RetenedorZOH(_Retencion):
    """Retenedor de orden cero (discreto → continuo).   y(t) = u(t_k)   para t_k <= t < t_k + Ts.

    En este simulador todas las señales son funciones del tiempo continuo, de modo que retener una
    señal discreta coincide con muestrearla a su mismo periodo.
    """
    tipo = "retenedor_zoh"; nombre = "Retenedor ZOH"


@registrar
class TransicionTasa(_Retencion):
    """Transición de tasa.   y = u(t_k) a la tasa de salida Ts, retenido entre instantes.

    Pasa una señal entre dos tasas (o entre continuo y discreto) leyendo su valor ya fijado en el
    instante: el bloque se atiende después de los que lo alimentan, así nunca lee un valor a medio
    actualizar. Fuera de tiempo real equivale a un remuestreo ZOH.
    """
    tipo = "transicion_tasa"; nombre = "Transición de tasa"


@registrar
class RetardoUnitario(BloqueDiscreto):
    """Retardo unitario z⁻¹.   y[k] = u[k-1],   y[0] = valor_inicial."""
    tipo = "retardo_unitario"; nombre = "Retardo unitario (z⁻¹)"; categoria = "Discreto"
    parametros = BloqueDiscreto.PARAMS_TS + [P("valor_inicial", "Valor inicial", 0.0)]
    entradas = ["u"]; salidas = ["y"]; directo = False

    @override
    def reiniciar_discreto(self):
        self.estado = self.p["valor_inicial"]
        self.y = self.estado

    @override
    def tic(self, t, u):
        return self._retener(self.estado)

    @override
    def actualizar_tic(self, t, u):
        self.estado = u[0].copy() if isinstance(u[0], np.ndarray) else float(u[0])


@registrar
class IntegradorDiscreto(BloqueDiscreto):
    """Integrador discreto con límites opcionales (anti-windup por recorte del estado).

    adelante:     y[k] = x[k],   x[k+1] = x[k] + Ts·u[k]
    atrás:        y[k] = x[k] = x[k-1] + Ts·u[k]
    trapezoidal:  y[k] = x[k] = x[k-1] + Ts/2·(u[k] + u[k-1])
    En el primer instante la salida es x0. El estado se recorta a [lim_inf, lim_sup].
    """
    tipo = "integrador_discreto"; nombre = "Integrador discreto"; categoria = "Discreto"
    parametros = BloqueDiscreto.PARAMS_TS + [
        P("metodo", "Método", "adelante", tipo="opcion", opciones=["adelante", "atrás", "trapezoidal"]),
        P("x0", "Valor inicial", 0.0), P("lim_inf", "Límite inferior", "-inf"), P("lim_sup", "Límite superior", "inf")]
    entradas = ["u"]; salidas = ["y"]

    @override
    def preparar_discreto(self):
        self.metodo = self.p["metodo"]
        if self.metodo not in ("adelante", "atrás", "trapezoidal"):
            raise ValueError(f"Bloque «{self.etiqueta}»: método desconocido «{self.metodo}»")
        self.directo = self.metodo != "adelante"
        self.li, self.ls = float(self.p["lim_inf"]), float(self.p["lim_sup"])

    def _recortar(self, v):
        return min(self.ls, max(self.li, v))

    @override
    def reiniciar_discreto(self):
        self.s = self._recortar(float(self.p["x0"]))
        self.u_ant = 0.0
        self.primero = True
        self.y = self.s

    @override
    def tic(self, t, u):
        v = float(u[0])
        if self.metodo == "adelante" or self.primero:
            nuevo = self.s
        elif self.metodo == "atrás":
            nuevo = self._recortar(self.s + self.Ts * v)
        else:
            nuevo = self._recortar(self.s + 0.5 * self.Ts * (v + self.u_ant))
        return self._retener(nuevo)

    @override
    def actualizar_tic(self, t, u):
        v = float(u[0])
        self.s = self._recortar(self.s + self.Ts * v) if self.metodo == "adelante" else self.y
        self.u_ant = v
        self.primero = False


@registrar
class FuncionTransferenciaZ(BloqueDiscreto):
    """Función de transferencia discreta.   Y(z)/U(z) = (b0·z^m + ... + bm) / (a0·z^n + ... + an).

    Se convierte a espacio de estados discreto (x[k+1] = A·x[k] + B·u[k], y[k] = C·x[k] + D·u[k])
    en forma canónica controlable, como `funcion_transferencia`. Debe ser propia (m <= n).
    """
    tipo = "funcion_transferencia_z"; nombre = "Función de transferencia (z)"; categoria = "Discreto"
    parametros = BloqueDiscreto.PARAMS_TS + [P("num", "Numerador", "[1]"), P("den", "Denominador", "[1, -0.5]")]
    entradas = ["u"]; salidas = ["y"]

    @override
    def preparar_discreto(self):
        from scipy.signal import tf2ss
        num = np.atleast_1d(self.p["num"]).astype(float)
        den = np.atleast_1d(self.p["den"]).astype(float)
        if len(num) > len(den):
            raise ValueError(f"Bloque «{self.etiqueta}»: función impropia (numerador mayor que denominador)")
        A, B, C, D = tf2ss(num, den)
        self.A, self.B, self.C, self.D = A, B.ravel(), C.ravel(), float(np.ravel(D)[0])
        self.directo = self.D != 0.0

    @override
    def reiniciar_discreto(self):
        self.s = np.zeros(self.A.shape[0])
        self.y = 0.0

    @override
    def tic(self, t, u):
        y = float(self.C @ self.s) if self.s.size else 0.0
        return self._retener(y + self.D * float(u[0]))

    @override
    def actualizar_tic(self, t, u):
        if self.s.size:
            self.s = self.A @ self.s + self.B * float(u[0])


@registrar
class PIDDiscreto(BloqueDiscreto, _GananciasPID):
    """PID discreto: las ecuaciones de `pid` muestreadas con periodo Ts.

    e = r - y;   P = Kp·(b·r - y);   e_d = c·r - y
    d[k] = (d[k-1] + Ts·N·e_d[k]) / (1 + Ts·N)          (filtro de derivada, Euler hacia atrás)
    D = Kd·N·(e_d[k] - d[k]);   u = P + I[k] + D;   salida = sat(u)
    I[k+1] = I[k] + Ts·(Ki·e[k] + (sat(u) - u)/Tt)      (integral, Euler hacia delante)

    Fuentes: Åström y Hägglund, *Advanced PID Control*, ISA, 2006; Åström y Wittenmark,
    *Computer-Controlled Systems*, Prentice Hall, 1997.
    """
    tipo = "pid_discreto"; nombre = "PID discreto"; categoria = "Discreto"
    parametros = BloqueDiscreto.PARAMS_TS + _GananciasPID.PARAMS
    entradas = ["r", "y"]; salidas = ["u"]

    @override
    def preparar_discreto(self):
        self._leer()

    @override
    def reiniciar_discreto(self):
        self.I = float(self.p["I0"])
        self.d = 0.0
        self.y = 0.0
        self._pend = (0.0, 0.0, 0.0)

    @override
    def tic(self, t, u):
        r, y = float(u[0]), float(u[1])
        ed = self.cw * r - y
        d = (self.d + self.Ts * self.N * ed) / (1.0 + self.Ts * self.N) if self.Kd else 0.0
        v = self.Kp * (self.bw * r - y) + self.I + self.Kd * self.N * (ed - d)
        vs = self._saturar(v)
        self._pend = (r - y, d, vs - v)
        return self._retener(vs)

    @override
    def actualizar_tic(self, t, u):
        e, d, exceso = self._pend
        self.I += self.Ts * (self.Ki * e + (exceso / self.Tt if self.aw else 0.0))
        self.d = d


@registrar
class LimitadorTasaDiscreto(BloqueDiscreto):
    """Limitador de tasa exacto por muestra.   y[k] = y[k-1] + min(R_alta·Ts, max(R_baja·Ts, u[k] - y[k-1])).

    Cumple |y[k] - y[k-1]| <= R·Ts en cada instante. y[-1] = y0.
    """
    tipo = "limitador_tasa_discreto"; nombre = "Limitador de tasa discreto"; categoria = "Discreto"
    parametros = BloqueDiscreto.PARAMS_TS + [
        P("R_alta", "Pendiente máxima de subida", 1.0, ayuda="unidades de la señal por segundo, >= 0"),
        P("R_baja", "Pendiente máxima de bajada", -1.0, ayuda="unidades de la señal por segundo, <= 0"),
        P("y0", "Salida inicial", 0.0)]
    entradas = ["u"]; salidas = ["y"]

    @override
    def preparar_discreto(self):
        self.ra, self.rb = float(self.p["R_alta"]), float(self.p["R_baja"])
        if self.ra < 0 or self.rb > 0:
            raise ValueError(f"Bloque «{self.etiqueta}»: hace falta R_alta >= 0 y R_baja <= 0")

    @override
    def reiniciar_discreto(self):
        self.y = float(self.p["y0"])
        self.previo = self.y

    @override
    def tic(self, t, u):
        paso = min(self.ra * self.Ts, max(self.rb * self.Ts, float(u[0]) - self.previo))
        return self._retener(self.previo + paso)

    @override
    def actualizar_tic(self, t, u):
        self.previo = self.y


@registrar
class Ruido(BloqueDiscreto):
    """Ruido blanco de banda limitada.   y = √(PSD/Tc)·w[k],  retenido durante Tc.

    w[k] es gaussiana de media 0 y varianza 1, o uniforme en [-√3, √3] (misma varianza), generada
    con la semilla dada: la misma semilla da la misma secuencia. Con Tc mucho menor que las
    constantes de tiempo del sistema aproxima ruido blanco de densidad espectral PSD.
    """
    tipo = "ruido"; nombre = "Ruido blanco"; categoria = "Fuentes"; color = "fuente"
    parametros = [P("Ts", "Tiempo de correlación Tc", 1e-2, "s"), P("offset", "Desfase", 0.0, "s"),
                  P("PSD", "Densidad espectral de potencia", 0.1, ayuda="unidades de la señal al cuadrado por Hz"),
                  P("semilla", "Semilla", 0),
                  P("tipo_ruido", "Distribución", "gaussiano", tipo="opcion", opciones=["gaussiano", "uniforme"])]
    salidas = ["y"]; directo = False

    @override
    def preparar_discreto(self):
        self.escala = math.sqrt(float(self.p["PSD"]) / self.Ts)

    @override
    def reiniciar_discreto(self):
        self.rng = np.random.default_rng(int(float(self.p["semilla"])))
        self.y = 0.0

    @override
    def tic(self, t, u):
        if self.p["tipo_ruido"] == "uniforme":
            w = self.rng.uniform(-math.sqrt(3.0), math.sqrt(3.0))
        else:
            w = self.rng.standard_normal()
        return self._retener(self.escala * float(w))
