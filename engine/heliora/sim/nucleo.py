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
import operator
import re
import types
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
    tipo: str = "num"  # num | texto | opcion | ruta | codigo | columna | variable
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


_BINARIOS = {ast.Add: operator.add, ast.Sub: operator.sub, ast.Mult: operator.mul, ast.Div: operator.truediv,
             ast.FloorDiv: operator.floordiv, ast.Mod: operator.mod, ast.Pow: operator.pow, ast.MatMult: operator.matmul}
_UNARIOS = {ast.UAdd: operator.pos, ast.USub: operator.neg, ast.Not: operator.not_, ast.Invert: operator.invert}
_COMPARADORES = {ast.Eq: operator.eq, ast.NotEq: operator.ne, ast.Lt: operator.lt, ast.LtE: operator.le,
                 ast.Gt: operator.gt, ast.GtE: operator.ge, ast.In: lambda a, b: a in b, ast.NotIn: lambda a, b: a not in b}

# Lista blanca: una expresión de parámetro es matemática sobre datos. Solo se alcanzan estas
# funciones de NumPy y estos atributos de matrices y tablas; cualquier otro acceso (ficheros,
# módulos, métodos que reciben el nombre de otro método como texto) queda fuera por construcción.
_NUMPY_PERMITIDO = frozenset({
    "pi", "e", "inf", "nan", "sqrt", "cbrt", "exp", "expm1", "log", "log10", "log2", "log1p", "sin", "cos", "tan",
    "arcsin", "arccos", "arctan", "arctan2", "sinh", "cosh", "tanh", "arcsinh", "arccosh", "arctanh", "hypot",
    "deg2rad", "rad2deg", "radians", "degrees", "abs", "absolute", "sign", "floor", "ceil", "round", "trunc",
    "power", "maximum", "minimum", "clip", "sum", "prod", "mean", "median", "std", "var", "max", "min", "cumsum",
    "cumprod", "diff", "interp", "array", "asarray", "linspace", "logspace", "arange", "zeros", "ones", "full",
    "eye", "diag", "dot", "outer", "cross", "concatenate", "stack", "hstack", "vstack", "reshape", "transpose",
    "real", "imag", "conj", "angle", "float64", "int64", "complex128", "isfinite", "where"})
_ATRIBUTOS_DATOS = frozenset({
    "T", "real", "imag", "shape", "size", "ndim", "sum", "prod", "mean", "std", "var", "min", "max", "cumsum",
    "flatten", "ravel", "reshape", "transpose", "conj", "item", "values", "to_numpy", "iloc", "loc", "columns"})
_EXPONENTE_MAX = 10_000
_ELEMENTOS_MAX = 1_000_000


def _es_dato(v):
    """Valor de datos que una expresión puede leer: número, matriz, lista o tabla, nunca código."""
    if isinstance(v, (bool, int, float, complex, np.number, np.ndarray, list, tuple)):
        return True
    return type(v).__module__.partition(".")[0] == "pandas" and type(v).__name__ in ("DataFrame", "Series")


def _atributo_seguro(objeto, nombre):
    """Lee `objeto.nombre` si está en la lista blanca; en otro caso lanza `ValueError`."""
    if objeto is np:
        if nombre not in _NUMPY_PERMITIDO:
            raise ValueError(f"«np.{nombre}» no está permitido en una expresión")
        return getattr(np, nombre)
    es_indexador = type(objeto).__name__ in ("_iLocIndexer", "_LocIndexer")
    if es_indexador or not _es_dato(objeto) or nombre not in _ATRIBUTOS_DATOS:
        raise ValueError(f"el atributo «{nombre}» no está permitido en una expresión")
    return getattr(objeto, nombre)


class _Evaluador:
    """Evalúa una expresión de Python recorriendo su AST con una lista blanca de nodos.

    Admite literales, nombres de datos del espacio, operadores, comparaciones, subíndices, listas,
    tuplas, las funciones de `_ESPACIO_MAT`, las de NumPy de `_NUMPY_PERMITIDO` y los atributos de
    `_ATRIBUTOS_DATOS`. Todo lo demás (lambdas, comprensiones, cadenas formateadas, módulos,
    funciones del espacio de trabajo, nombres con `_`) se rechaza, de modo que un diagrama ajeno
    no pueda ejecutar código ni tocar archivos al simular.
    """

    def __init__(self, espacio):
        self.ns = espacio

    def __call__(self, nodo):
        metodo = getattr(self, "n_" + type(nodo).__name__, None)
        if metodo is None:
            raise ValueError(f"la expresión usa una construcción no permitida ({type(nodo).__name__})")
        return metodo(nodo)

    def n_Expression(self, n):
        return self(n.body)

    def n_Constant(self, n):
        if isinstance(n.value, bytes):
            raise ValueError("los literales de bytes no están permitidos")
        return n.value

    def n_Name(self, n):
        if n.id.startswith("_") or n.id not in self.ns:
            raise NameError(f"la variable «{n.id}» no está definida")
        v = self.ns[n.id]
        if not (_es_dato(v) or n.id in _ESPACIO_MAT and v is _ESPACIO_MAT[n.id]):
            raise ValueError(f"«{n.id}» no es un dato y no se puede usar en una expresión")
        return v

    def n_List(self, n):
        return [self(e) for e in n.elts]

    def n_Tuple(self, n):
        return tuple(self(e) for e in n.elts)

    def n_UnaryOp(self, n):
        return _UNARIOS[type(n.op)](self(n.operand))

    def n_BinOp(self, n):
        a, b = self(n.left), self(n.right)
        if isinstance(n.op, ast.Pow) and isinstance(a, int) and isinstance(b, int) and abs(b) > _EXPONENTE_MAX:
            raise ValueError("el exponente entero es demasiado grande")
        return _BINARIOS[type(n.op)](a, b)

    def n_BoolOp(self, n):
        r = None
        for e in n.values:
            r = self(e)
            if bool(r) == isinstance(n.op, ast.Or):
                return r
        return r

    def n_IfExp(self, n):
        return self(n.body) if self(n.test) else self(n.orelse)

    def n_Compare(self, n):
        izq = self(n.left)
        r = True
        for op, e in zip(n.ops, n.comparators):
            der = self(e)
            r = _COMPARADORES[type(op)](izq, der)
            if not r:
                return r
            izq = der
        return r

    def n_Attribute(self, n):
        return _atributo_seguro(self(n.value), n.attr)

    def n_ListComp(self, n):
        # Tablas como [[f(a, b) for b in ejes] for a in ejes]: cada bucle recorre datos y liga un
        # nombre simple en un ámbito propio, sin tocar el espacio de nombres de fuera
        salida = []

        def recorrer(i, ambito):
            if i == len(n.generators):
                if len(salida) >= _ELEMENTOS_MAX:
                    raise ValueError("la comprensión genera demasiados elementos")
                salida.append(_Evaluador(ambito)(n.elt))
                return
            g = n.generators[i]
            if not isinstance(g.target, ast.Name) or g.target.id.startswith("_") or g.is_async:
                raise ValueError("en una comprensión, la variable del bucle debe ser un nombre simple")
            datos = _Evaluador(ambito)(g.iter)
            if not isinstance(datos, (list, tuple, np.ndarray)):
                raise ValueError("una comprensión solo puede recorrer listas, tuplas o matrices")
            for x in datos:
                hijo = dict(ambito)
                hijo[g.target.id] = x
                if all(_Evaluador(hijo)(c) for c in g.ifs):
                    recorrer(i + 1, hijo)

        recorrer(0, self.ns)
        return salida

    def n_Subscript(self, n):
        return self(n.value)[self._indice(n.slice)]

    def _indice(self, n):
        if isinstance(n, ast.Slice):
            return slice(*(None if x is None else self(x) for x in (n.lower, n.upper, n.step)))
        if isinstance(n, ast.Tuple):
            return tuple(self._indice(e) for e in n.elts)
        return self(n)

    def n_Call(self, n):
        f = n.func
        if isinstance(f, ast.Name):
            if f.id not in _ESPACIO_MAT or self.ns.get(f.id) is not _ESPACIO_MAT[f.id]:
                raise ValueError(f"la función «{f.id}» no está permitida en una expresión")
            fn = _ESPACIO_MAT[f.id]
        elif isinstance(f, ast.Attribute):
            fn = self(f)
        else:
            raise ValueError("solo se pueden llamar funciones por su nombre o como atributo")
        if any(isinstance(a, ast.Starred) for a in n.args) or any(k.arg is None for k in n.keywords):
            raise ValueError("no se admiten argumentos con * ni **")
        return fn(*[self(a) for a in n.args], **{k.arg: self(k.value) for k in n.keywords})


def evaluar_expresion(texto, espacio):
    """Evalúa una expresión de Python con la lista blanca de nodos de `_Evaluador`.

    Args:
        texto: código de la expresión.
        espacio: nombres disponibles (variables y funciones).

    Returns:
        El valor de la expresión.

    Raises:
        ValueError: si la expresión usa algo que la lista blanca no permite.
        SyntaxError: si el texto no es Python válido.
    """
    return _Evaluador(espacio)(ast.parse(texto, mode="eval"))


_COMA_DECIMAL = re.compile(r"^[-+]?\d+,\d+(?:[eE][-+]?\d+)?$")


def espacio_de_expresion(espacio):
    """Nombres que ve una expresión: las funciones matemáticas y solo los datos del espacio de trabajo."""
    ns = {k: v for k, v in (espacio or {}).items() if not str(k).startswith("_") and _es_dato(v)}
    ns.update(_ESPACIO_MAT)
    return ns


def evaluar_numero(valor, espacio=None, unidad="", vacio=None):
    """Convierte un parámetro a número o arreglo.

    Acepta números, listas y expresiones: `2*pi*50`, `[1, 2, 3]`, `Vdc/2`
    (las variables se buscan en el espacio de trabajo). También un número con unidad y prefijo,
    como `10 mH` o `1 kΩ`, que se comprueba contra la unidad del parámetro: `10 mF` en una
    inductancia da un error en lugar de un modelo equivocado. La coma decimal («0,5») se acepta.
    Las expresiones se evalúan con una lista blanca de nodos (ver `_Evaluador`).

    Args:
        valor: número, lista o texto.
        espacio: variables del espacio de trabajo.
        unidad: unidad del parámetro, para validar el sufijo de un número con unidad.
        vacio: valor que se devuelve si el texto está vacío; con None, un texto vacío es un error.

    Raises:
        ValueError: si el valor falta, no se puede evaluar o usa algo no permitido.
    """
    if isinstance(valor, bool):
        return float(valor)
    if isinstance(valor, (int, float, np.ndarray)):
        return valor
    if isinstance(valor, (list, tuple)):
        return np.asarray(valor, dtype=float)
    texto = str(valor).strip()
    if not texto:
        if vacio is None:
            raise ValueError("falta valor")
        return vacio
    if _COMA_DECIMAL.match(texto):
        texto = texto.replace(",", ".")
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
    r = evaluar_expresion(texto, espacio_de_expresion(espacio))
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
        self.avisos_preparacion = []
        self.p = {}
        for d in self.definicion_parametros(self.crudos):
            v = self.crudos.get(d.nombre, d.defecto)
            if d.tipo == "num":
                try:
                    v = evaluar_numero(v, self.espacio, d.unidad)
                except Exception as e:
                    detalle = f"falta valor en {d.etiqueta}" if str(e) == "falta valor" else f"no se pudo evaluar {d.etiqueta} = {v!r} ({e})"
                    raise ValueError(f"Bloque «{self.etiqueta}»: {detalle}") from e
            self.p[d.nombre] = v
        try:
            self.preparar()
        except ValueError as e:
            if str(e).startswith("Bloque «"):
                raise
            raise ValueError(f"Bloque «{self.etiqueta}»: {e}") from e

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
