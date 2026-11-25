"""Base de los bloques: parámetros, registro y puertos.

Para crear un bloque nuevo basta con una clase corta:

    @registrar
    class Doble(Bloque):
        '''Duplica la entrada.  y = 2·u'''
        tipo = "doble"; nombre = "Doble"; categoria = "Matemática"
        entradas = ["u"]; salidas = ["y"]
        def salida(self, t, x, u):
            return [2.0 * u[0]]

Métodos que puede definir:
    n_estados()              número de estados continuos
    x0()                     valor inicial de los estados
    salida(t, x, u)          lista de salidas
    derivada(t, x, u)        lista de dx/dt
    paso_mayor(t, x, u, y)   actualización discreta al final de cada paso aceptado;
                             devuelve True si hubo un salto en la salida
    proximo_evento(t)        siguiente instante en el que el solver debe caer

Los bloques de tiempo discreto heredan de `BloqueDiscreto`: declaran un periodo de muestreo `Ts`
y un desfase `offset`, y en cada instante k·Ts + offset el modelo llama a `tic(t, u)` (fija la
salida retenida) y después a `actualizar_tic(t, u)` (avanza el estado). Entre instantes la salida
es el valor retenido.
"""
from __future__ import annotations

from typing_extensions import override
import ast
import math
import re
from dataclasses import dataclass, field

import numpy as np

REGISTRO: dict[str, type] = {}


@dataclass
class P:
    """Descripción de un parámetro de bloque (lo que la interfaz muestra)."""
    nombre: str
    etiqueta: str
    defecto: object = 0.0
    unidad: str = ""
    tipo: str = "num"  # num | texto | opcion | ruta | codigo
    opciones: list = field(default_factory=list)
    ayuda: str = ""


def registrar(cls):
    """Registra la clase de bloque en `REGISTRO` bajo su atributo `tipo` y la devuelve."""
    REGISTRO[cls.tipo] = cls
    return cls


_ESPACIO_MAT = {k: getattr(math, k) for k in ("pi", "e", "sqrt", "sin", "cos", "tan", "exp", "log", "log10", "tanh", "atan2")}
_ESPACIO_MAT.update(np=np, inf=math.inf)


_PREFIJOS = {"f": 1e-15, "p": 1e-12, "n": 1e-9, "u": 1e-6, "µ": 1e-6, "μ": 1e-6, "m": 1e-3, "k": 1e3, "M": 1e6, "G": 1e9}
_CON_UNIDAD = re.compile(r"^([-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)\s*([A-Za-zµμΩ°/%²]+)$")


class ErrorUnidad(ValueError):
    """La unidad escrita no corresponde a la del parámetro."""


def _con_unidad(texto, unidad):
    """Interpreta `10 mH`, `1 kΩ`, `50 µF`. Devuelve None si el texto no tiene esa forma."""
    m = _CON_UNIDAD.match(texto)
    if m is None:
        return None
    numero, sufijo = float(m.group(1)), m.group(2)
    if unidad and sufijo == unidad:
        return numero
    if unidad and sufijo.endswith(unidad) and sufijo[:-len(unidad)] in _PREFIJOS:
        return numero * _PREFIJOS[sufijo[:-len(unidad)]]
    if not unidad and sufijo in _PREFIJOS:
        return numero * _PREFIJOS[sufijo]
    raise ErrorUnidad(f"la unidad «{sufijo}» no es válida aquí" + (f": se esperaba {unidad}" if unidad else ""))


def evaluar_numero(valor, espacio=None, unidad=""):
    """Convierte un parámetro a número o arreglo.

    Acepta números, listas y expresiones: `2*pi*50`, `[1, 2, 3]`, `Vdc/2`
    (las variables se buscan en el espacio de trabajo). También un número con unidad y prefijo,
    como `10 mH` o `1 kΩ`, que se comprueba contra la unidad del parámetro: `10 mF` en una
    inductancia da un error en lugar de un modelo equivocado.
    """
    if isinstance(valor, bool):
        return float(valor)
    if isinstance(valor, (int, float, np.ndarray)):
        return valor
    if isinstance(valor, (list, tuple)):
        return np.asarray(valor, dtype=float)
    texto = str(valor).strip()
    if not texto:
        return 0.0
    r = _con_unidad(texto, unidad)
    if r is not None:
        return r
    try:
        v = ast.literal_eval(texto)
        if isinstance(v, (int, float)):
            return float(v)
        if isinstance(v, (list, tuple)):
            return np.asarray(v, dtype=float)
    except (ValueError, SyntaxError):
        pass
    ns = dict(_ESPACIO_MAT)
    if espacio:
        ns.update(espacio)
    r = eval(texto, {"__builtins__": {}}, ns)  # expresión escrita por el propio usuario
    if isinstance(r, (list, tuple)):
        return np.asarray(r, dtype=float)
    return r


class Bloque:
    """Clase base de todo bloque."""

    tipo = ""
    nombre = ""
    categoria = "General"
    parametros: list = []
    entradas: list = []
    salidas: list = []
    puertos_e: list = []     # puertos eléctricos monofásicos
    puertos_e3: list = []    # puertos eléctricos trifásicos (una línea = 3 fases)
    directo = True           # ¿la salida depende de la entrada en el mismo instante?
    sumidero = False         # guarda señales (osciloscopio, exportar)
    color = ""
    puertos_dinamicos = None

    def __init__(self, params=None, ident="", etiqueta="", espacio=None):
        self.id = ident
        self.etiqueta = etiqueta or self.nombre
        self.espacio = espacio or {}
        self.crudos = dict(params or {})
        self.p = {}
        for d in self.definicion_parametros(self.crudos):
            v = self.crudos.get(d.nombre, d.defecto)
            if d.tipo == "num":
                try:
                    v = evaluar_numero(v, self.espacio, d.unidad)
                except Exception as e:
                    raise ValueError(
                        f"Bloque «{self.etiqueta}»: no se pudo evaluar {d.etiqueta} = {v!r} ({e})"
                    ) from e
            self.p[d.nombre] = v
        self.preparar()

    @classmethod
    def definicion_parametros(cls, crudos=None):
        """Devuelve la lista de parámetros `P` del bloque.

        Args:
            crudos: valores sin evaluar de los parámetros, tal como llegan del diagrama. Los
                bloques cuya lista de parámetros depende de otro parámetro lo redefinen y lo leen.

        Returns:
            Lista de `P`; por defecto, el atributo de clase `parametros`.
        """
        return cls.parametros

    @classmethod
    def puertos(cls, params=None):
        """Puertos según los parámetros. Los bloques con puertos variables lo redefinen."""
        return {"entradas": list(cls.entradas), "salidas": list(cls.salidas),
                "e": list(cls.puertos_e), "e3": list(cls.puertos_e3)}

    def preparar(self):
        """Deriva atributos de cálculo a partir de `self.p`; el constructor lo llama una vez, al final."""

    def n_estados(self):
        """Devuelve el número de estados continuos del bloque."""
        return 0

    def x0(self):
        """Devuelve la lista con el valor inicial de cada estado, de longitud `n_estados()`."""
        return []

    def salida(self, t, x, u):
        """Calcula las salidas del bloque.

        Args:
            t: tiempo de simulación, en s.
            x: estados del bloque, de longitud `n_estados()`; `None` en los bloques sumidero.
            u: valores de las entradas, en el orden de `entradas`; `None` si `directo` es False.

        Returns:
            Lista con un valor por salida, en el orden de `salidas`.
        """
        return []

    def derivada(self, t, x, u):
        """Calcula la derivada temporal de los estados.

        Args:
            t: tiempo de simulación, en s.
            x: estados del bloque, de longitud `n_estados()`.
            u: valores de las entradas, en el orden de `entradas`.

        Returns:
            Lista de dx/dt, de longitud `n_estados()`.
        """
        return []

    def paso_mayor(self, t, x, u, y):
        """Actualiza el estado discreto del bloque; el modelo lo llama al final de cada paso aceptado.

        Args:
            t: tiempo al final del paso, en s.
            x: estados del bloque; `None` en los sumideros y en la segunda llamada tras un cambio.
            u: valores de las entradas, en el orden de `entradas`.
            y: salidas vigentes del bloque; `None` en las llamadas con `x` igual a `None`.

        Returns:
            True si hubo un salto en la salida, de modo que el modelo recalcula las señales.
        """
        return False

    def proximo_evento(self, t):
        """Devuelve el siguiente instante, en s, en el que el solver debe caer; `math.inf` si no hay.

        Args:
            t: tiempo actual de simulación, en s.
        """
        return math.inf

    def reiniciar(self):
        """Devuelve el estado interno del bloque a su valor inicial; el modelo lo llama al iniciar."""

    # Cruces por cero (solo con solvers de paso variable). Un bloque con discontinuidad define
    # cruce(t, x, u) -> lista de señales, y mantiene su modo hasta paso_mayor si usar_zc es True.
    usar_zc = False
    cruce = None
    de_evento = False   # su paso_mayor solo cambia de modo y se puede repetir sin efectos

    def tiene_cruce(self):
        """Indica si el bloque define señales de cruce por cero."""
        return self.cruce is not None

    # Tiempo discreto: Ts > 0 marca un bloque muestreado (ver BloqueDiscreto).
    Ts = 0.0
    offset = 0.0

    def tic(self, t, u):
        """Fija la salida retenida en un instante de muestreo; devuelve True si cambió."""
        return False

    def actualizar_tic(self, t, u):
        """Avanza el estado discreto tras fijar las salidas de todos los bloques del instante."""

    @classmethod
    def describir(cls):
        """Ficha para la paleta y la documentación."""
        import inspect
        src = inspect.getsourcefile(cls)
        try:
            linea = inspect.getsourcelines(cls)[1]
        except OSError:
            linea = 1
        return {
            "tipo": cls.tipo, "nombre": cls.nombre, "categoria": cls.categoria,
            "doc": inspect.getdoc(cls) or "",
            "params": [
                {"nombre": d.nombre, "etiqueta": d.etiqueta, "defecto": d.defecto, "unidad": d.unidad,
                 "tipo": d.tipo, "opciones": d.opciones, "ayuda": d.ayuda}
                for d in cls.definicion_parametros({})
            ],
            "puertos": cls.puertos({}),
            "puertos_dinamicos": cls.puertos_dinamicos,
            "archivo": src, "linea": linea, "color": cls.color,
        }


# Margen relativo (en periodos de muestreo) con el que un instante se da por alcanzado: cubre el
# redondeo de t + (t_evento - t) del solver y la suma acumulada de los pasos fijos.
TOL_TIC = 1e-9


class BloqueDiscreto(Bloque):
    """Base de los bloques muestreados con periodo `Ts` y desfase `offset`.

    Los instantes de muestreo son t_k = offset + k·Ts. El modelo los publica en `proximo_evento`,
    de modo que los solvers de paso variable caen exactamente en ellos; con paso fijo se atienden
    en el primer paso igual o posterior. Las subclases guardan la salida retenida en `self.y`.
    """

    PARAMS_TS = [P("Ts", "Periodo de muestreo", 1e-3, "s"), P("offset", "Desfase", 0.0, "s")]

    @override
    def preparar(self):
        self.Ts = float(self.p["Ts"])
        self.offset = float(self.p["offset"])
        if not self.Ts > 0.0:
            raise ValueError(f"Bloque «{self.etiqueta}»: el periodo de muestreo debe ser positivo")
        self.preparar_discreto()
        self.reiniciar()

    def preparar_discreto(self):
        """Lee los parámetros propios del bloque (se llama una vez, antes de reiniciar)."""

    @override
    def reiniciar(self):
        self._n = -1
        self.tics_saltados = 0
        self.reiniciar_discreto()

    def reiniciar_discreto(self):
        """Devuelve la salida retenida y el estado a sus valores iniciales."""

    def toca_tic(self, t):
        """True si en t se alcanza un instante de muestreo aún no atendido (y lo da por atendido)."""
        if t + TOL_TIC * self.Ts < self.offset + (self._n + 1) * self.Ts:
            return False
        n = math.floor((t - self.offset) / self.Ts + TOL_TIC)
        self.tics_saltados += max(0, n - self._n - 1)
        self._n = n
        return True

    @override
    def proximo_evento(self, t):
        return self.offset + (self._n + 1) * self.Ts

    @override
    def salida(self, t, x, u):
        return [self.y]

    def _retener(self, valor):
        """Fija la salida retenida; True si cambió."""
        previo = self.y
        self.y = valor
        return not np.array_equal(np.asarray(previo), np.asarray(valor))
