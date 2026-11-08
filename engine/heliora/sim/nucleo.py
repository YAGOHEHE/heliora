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
"""
from __future__ import annotations

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
        return cls.parametros

    @classmethod
    def puertos(cls, params=None):
        """Puertos según los parámetros. Los bloques con puertos variables lo redefinen."""
        return {"entradas": list(cls.entradas), "salidas": list(cls.salidas),
                "e": list(cls.puertos_e), "e3": list(cls.puertos_e3)}

    def preparar(self):
        pass

    def n_estados(self):
        return 0

    def x0(self):
        return []

    def salida(self, t, x, u):
        return []

    def derivada(self, t, x, u):
        return []

    def paso_mayor(self, t, x, u, y):
        return False

    def proximo_evento(self, t):
        return math.inf

    def reiniciar(self):
        pass

    # Cruces por cero (solo con solvers de paso variable). Un bloque con discontinuidad define
    # cruce(t, x, u) -> lista de señales, y mantiene su modo hasta paso_mayor si usar_zc es True.
    usar_zc = False
    cruce = None
    de_evento = False   # su paso_mayor solo cambia de modo y se puede repetir sin efectos

    def tiene_cruce(self):
        return self.cruce is not None

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
