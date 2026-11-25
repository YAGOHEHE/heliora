"""Bloques genéricos: fuentes, matemática, continuo, discontinuidades, tablas y sumideros."""
from __future__ import annotations

import bisect
import math

import numpy as np

from .nucleo import Bloque, P, registrar, evaluar_numero

def _entero_positivo(valor, que):
    """Comprueba que `valor` sea un entero mayor o igual que 1; si no, lanza `ValueError` nombrando `que`."""
    v = float(valor)
    if v != int(v) or v < 1:
        raise ValueError(f"{que} debe ser un entero mayor o igual que 1 (se dio {valor:g})")


# ---------------------------------------------------------------- Fuentes


@registrar
class Constante(Bloque):
    """Constante.   y(t) = c."""
    tipo = "constante"; nombre = "Constante"; categoria = "Fuentes"; color = "fuente"
    parametros = [P("c", "Valor", 1.0)]
    salidas = ["y"]; directo = False

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [self.p["c"]]


@registrar
class Escalon(Bloque):
    """Escalón.   y(t) = y0 si t < t0;  y1 si t >= t0.

    El salto se aplica al final de un paso aceptado y el solver cae justo en t0,
    así la discontinuidad no degrada el error de integración.
    """
    tipo = "escalon"; nombre = "Escalón"; categoria = "Fuentes"; color = "fuente"
    parametros = [P("t0", "Instante", 1.0, "s"), P("y0", "Valor inicial", 0.0), P("y1", "Valor final", 1.0)]
    salidas = ["y"]; directo = False

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.valor = self.p["y0"]

    def reiniciar(self):
        """Devuelve el estado interno del bloque a su valor inicial."""
        self.valor = self.p["y0"]

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [self.valor]

    def paso_mayor(self, t, x, u, y):
        """Actualiza el estado discreto del bloque al final de un paso aceptado."""
        nuevo = self.p["y1"] if t >= self.p["t0"] - 1e-12 else self.p["y0"]
        if nuevo != self.valor:
            self.valor = nuevo
            return True
        return False

    def proximo_evento(self, t):
        """Devuelve el siguiente instante, en s, en el que el solver debe caer."""
        return self.p["t0"] if t < self.p["t0"] - 1e-12 else math.inf


@registrar
class Rampa(Bloque):
    """Rampa.   y(t) = y0 + m·max(0, t - t0)."""
    tipo = "rampa"; nombre = "Rampa"; categoria = "Fuentes"; color = "fuente"
    parametros = [P("m", "Pendiente", 1.0), P("t0", "Inicio", 0.0, "s"), P("y0", "Valor inicial", 0.0)]
    salidas = ["y"]; directo = False

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [self.p["y0"] + self.p["m"] * max(0.0, t - self.p["t0"])]


@registrar
class Senoidal(Bloque):
    """Senoidal.   y(t) = A·sin(2π·f·t + φ) + c   (φ en grados)."""
    tipo = "senoidal"; nombre = "Senoidal"; categoria = "Fuentes"; color = "fuente"
    parametros = [P("A", "Amplitud", 1.0), P("f", "Frecuencia", 1.0, "Hz"),
                  P("fase", "Fase", 0.0, "°"), P("c", "Desplazamiento", 0.0)]
    salidas = ["y"]; directo = False

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.w = 2 * math.pi * self.p["f"]
        self.ph = math.radians(self.p["fase"])

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [self.p["A"] * math.sin(self.w * t + self.ph) + self.p["c"]]


@registrar
class Pulso(Bloque):
    """Tren de pulsos.   y = A durante una fracción d de cada periodo T; 0 el resto."""
    tipo = "pulso"; nombre = "Tren de pulsos"; categoria = "Fuentes"; color = "fuente"
    parametros = [P("A", "Amplitud", 1.0), P("T", "Periodo", 1.0, "s"), P("d", "Fracción encendido", 0.5)]
    salidas = ["y"]; directo = False

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.valor = self.p["A"]

    def reiniciar(self):
        """Devuelve el estado interno del bloque a su valor inicial."""
        self.valor = self.p["A"]

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [self.valor]

    def paso_mayor(self, t, x, u, y):
        """Actualiza el estado discreto del bloque al final de un paso aceptado."""
        T = self.p["T"]
        fase = (t / T) % 1.0
        nuevo = self.p["A"] if fase < self.p["d"] - 1e-12 else 0.0
        c = nuevo != self.valor
        self.valor = nuevo
        return c

    def proximo_evento(self, t):
        """Devuelve el siguiente instante, en s, en el que el solver debe caer."""
        T, d = self.p["T"], self.p["d"]
        n = math.floor(t / T + 1e-12)
        a = (n + d) * T
        return a if a > t + 1e-12 else (n + 1) * T


@registrar
class Reloj(Bloque):
    """Reloj.   y(t) = t."""
    tipo = "reloj"; nombre = "Reloj"; categoria = "Fuentes"; color = "fuente"
    salidas = ["t"]; directo = False

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [t]


def _serie_de_tabla(d, col_t, col_y):
    """Extrae de una tabla el tiempo (en s) y el valor, validados, para interpolar.

    Args:
        d: DataFrame de origen.
        col_t: nombre de la columna de tiempo; vacío para la primera.
        col_y: nombre de la columna de valor; vacío para la segunda.

    Returns:
        Tupla (tiempo, valor, avisos) con arreglos de float sin NaN, el tiempo estrictamente creciente
        y los avisos de las filas descartadas. Las fechas se convierten a segundos desde la primera fila.

    Raises:
        ValueError: si falta una columna (con la lista de las disponibles) o el tiempo no es válido.
    """
    import pandas as pd
    from ..lectura import a_fechas
    nombres = {str(c): c for c in d.columns}
    if len(nombres) < 2:
        raise ValueError("la tabla necesita al menos dos columnas (tiempo y valor)")

    def columna(nombre, defecto):
        if not str(nombre).strip():
            return list(d.columns)[defecto]
        if str(nombre).strip() not in nombres:
            raise ValueError(f"no existe la columna «{nombre}»; disponibles: {', '.join(nombres)}")
        return nombres[str(nombre).strip()]

    ct, cy = columna(col_t, 0), columna(col_y, 1)
    bruto = d[ct]
    if pd.api.types.is_numeric_dtype(bruto):
        t = bruto.astype(float)
    else:
        f = a_fechas(bruto)
        t = (f - f.iloc[0]).dt.total_seconds()
    y = pd.to_numeric(d[cy], errors="coerce").astype(float)
    t, y = t.to_numpy(), y.to_numpy()
    validas = np.isfinite(t) & np.isfinite(y)
    avisos = [f"se descartaron {int((~validas).sum())} filas con valores no numéricos o NaN"] if not validas.all() else []
    t, y = t[validas], y[validas]
    if t.size == 0:
        raise ValueError("la tabla no tiene filas con tiempo y valor numéricos")
    malo = np.flatnonzero(np.diff(t) <= 0)
    if malo.size:
        raise ValueError(f"el tiempo debe ser estrictamente creciente (falla entre las filas válidas {int(malo[0]) + 1} y {int(malo[0]) + 2})")
    return t, y, avisos


@registrar
class DesdeCSV(Bloque):
    """Desde CSV.   y(t) = interpolación lineal de la columna elegida en el tiempo t.

    La primera columna del archivo (o la indicada) es el tiempo en segundos; si son fechas, se
    convierten a segundos desde la primera fila. El tiempo debe ser estrictamente creciente.
    Fuera del rango se mantiene el primer o el último valor. Lee CSV (con separador y coma decimal
    detectados), TSV y Parquet.
    """
    tipo = "desde_csv"; nombre = "Desde CSV"; categoria = "Fuentes"; color = "fuente"
    parametros = [P("ruta", "Archivo", "", tipo="ruta"), P("col_t", "Columna de tiempo", "", tipo="columna"),
                  P("col_y", "Columna de valor", "", tipo="columna")]
    salidas = ["y"]; directo = False

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        from ..lectura import leer_tabla
        ruta = self.p["ruta"]
        if not ruta:
            raise ValueError("elige un archivo CSV")
        self.tt, self.yy, self.avisos_preparacion = _serie_de_tabla(leer_tabla(ruta), self.p["col_t"], self.p["col_y"])

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [float(np.interp(t, self.tt, self.yy))]


@registrar
class DesdeEspacio(Bloque):
    """Desde el espacio de trabajo.   y(t) = interpolación de dos columnas de una variable.

    La variable es una tabla (columnas por nombre) o una matriz (columnas por posición, desde 0).
    Sin columnas indicadas se usan la primera (tiempo) y la segunda (valor).
    """
    tipo = "desde_espacio"; nombre = "Desde espacio de trabajo"; categoria = "Fuentes"; color = "fuente"
    parametros = [P("variable", "Variable", "datos", tipo="variable"), P("col_t", "Columna de tiempo", "", tipo="columna"),
                  P("col_y", "Columna de valor", "", tipo="columna")]
    salidas = ["y"]; directo = False

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        import pandas as pd
        nombre = str(self.p["variable"]).strip()
        if nombre not in self.espacio:
            tablas = sorted(k for k, v in self.espacio.items() if isinstance(v, (pd.DataFrame, np.ndarray)) and not str(k).startswith("_"))
            raise ValueError(f"la variable «{nombre}» no existe en el espacio de trabajo"
                             + (f"; tablas y matrices disponibles: {', '.join(tablas)}" if tablas else ""))
        v = self.espacio[nombre]
        if isinstance(v, pd.DataFrame):
            d = v
        elif isinstance(v, np.ndarray) and v.ndim == 2:
            d = pd.DataFrame(v)
        else:
            raise ValueError(f"la variable «{nombre}» debe ser una tabla o una matriz de dos columnas o más")
        self.tt, self.yy, self.avisos_preparacion = _serie_de_tabla(d, self.p["col_t"], self.p["col_y"])

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [float(np.interp(t, self.tt, self.yy))]


# ---------------------------------------------------------------- Matemática


@registrar
class Ganancia(Bloque):
    """Ganancia.   y = k·u."""
    tipo = "ganancia"; nombre = "Ganancia"; categoria = "Matemática"
    parametros = [P("k", "Ganancia", 1.0)]
    entradas = ["u"]; salidas = ["y"]

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        if not np.all(np.isfinite(self.p["k"])):
            raise ValueError("la ganancia debe ser finita")

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [self.p["k"] * u[0]]


@registrar
class Suma(Bloque):
    """Suma.   y = Σ s_i·u_i   donde s_i es el signo i-ésimo del parámetro (p. ej. "+-")."""
    tipo = "suma"; nombre = "Suma"; categoria = "Matemática"
    parametros = [P("signos", "Signos", "+-", tipo="texto")]
    salidas = ["y"]
    puertos_dinamicos = {"param": "signos", "modo": "largo", "prefijo": "u"}

    @classmethod
    def puertos(cls, params=None):
        """Devuelve los puertos del bloque según sus parámetros."""
        s = str((params or {}).get("signos", "+-"))
        return {"entradas": [f"u{i+1}" for i in range(len(s))], "salidas": ["y"], "e": [], "e3": []}

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.s = [1.0 if c == "+" else -1.0 for c in str(self.p["signos"])]

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        y = 0.0
        for si, ui in zip(self.s, u):
            y = y + si * ui
        return [y]


@registrar
class Producto(Bloque):
    """Producto.   y = u1·u2."""
    tipo = "producto"; nombre = "Producto"; categoria = "Matemática"
    entradas = ["u1", "u2"]; salidas = ["y"]

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [u[0] * u[1]]


@registrar
class Absoluto(Bloque):
    """Valor absoluto.   y = |u|."""
    tipo = "absoluto"; nombre = "Valor absoluto"; categoria = "Matemática"
    entradas = ["u"]; salidas = ["y"]

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [abs(u[0])]


@registrar
class FuncionPython(Bloque):
    """Función en Python.   y = f(t, u).

    Código editable por el usuario. `u` es la lista de entradas, `t` el tiempo y `np`
    NumPy. El código debe terminar asignando `y`.
    """
    tipo = "funcion_python"; nombre = "Función Python"; categoria = "Matemática"
    parametros = [P("codigo", "Código", "y = u[0]**2", tipo="codigo"), P("n", "Entradas", 1)]
    salidas = ["y"]
    puertos_dinamicos = {"param": "n", "modo": "valor", "prefijo": "u"}

    @classmethod
    def puertos(cls, params=None):
        """Devuelve los puertos del bloque según sus parámetros."""
        n = int(float((params or {}).get("n", 1)))
        return {"entradas": [f"u{i+1}" for i in range(n)], "salidas": ["y"], "e": [], "e3": []}

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.cod = compile(str(self.p["codigo"]), f"<{self.etiqueta}>", "exec")

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        ns = {"t": t, "u": u, "np": np, "math": math, "y": 0.0}
        exec(self.cod, ns)
        return [ns["y"]]


@registrar
class Mux(Bloque):
    """Combinar señales.   y = [u1, u2, ...]."""
    tipo = "mux"; nombre = "Combinar (mux)"; categoria = "Señales"
    parametros = [P("n", "Entradas", 3)]
    salidas = ["y"]
    puertos_dinamicos = {"param": "n", "modo": "valor", "prefijo": "u"}

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        _entero_positivo(self.p["n"], "el número de entradas")

    @classmethod
    def puertos(cls, params=None):
        """Devuelve los puertos del bloque según sus parámetros."""
        n = int(float((params or {}).get("n", 3)))
        return {"entradas": [f"u{i+1}" for i in range(n)], "salidas": ["y"], "e": [], "e3": []}

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [np.hstack([np.ravel(a) for a in u])]


@registrar
class Demux(Bloque):
    """Separar señales.   y_i = u[i]."""
    tipo = "demux"; nombre = "Separar (demux)"; categoria = "Señales"
    parametros = [P("n", "Salidas", 3)]
    entradas = ["u"]
    puertos_dinamicos = {"param": "n", "modo": "valor_salidas", "prefijo": "y"}

    @classmethod
    def puertos(cls, params=None):
        """Devuelve los puertos del bloque según sus parámetros."""
        n = int(float((params or {}).get("n", 3)))
        return {"entradas": ["u"], "salidas": [f"y{i+1}" for i in range(n)], "e": [], "e3": []}

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        n = int(self.p["n"])
        v = np.ravel(u[0])
        return [float(v[i]) if i < v.size else 0.0 for i in range(n)]


# ---------------------------------------------------------------- Continuo


@registrar
class Integrador(Bloque):
    """Integrador.   dx/dt = u,  y = x,  x(0) = x0,  con límites opcionales lim_inf <= x <= lim_sup."""
    tipo = "integrador"; nombre = "Integrador"; categoria = "Continuo"
    parametros = [P("x0", "Valor inicial", 0.0), P("lim_inf", "Límite inferior", "-inf"),
                  P("lim_sup", "Límite superior", "inf")]
    entradas = ["u"]; salidas = ["y"]; directo = False

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.li = float(self.p["lim_inf"])
        self.ls = float(self.p["lim_sup"])
        if self.li > self.ls:
            raise ValueError(f"el límite inferior ({self.li:g}) es mayor que el superior ({self.ls:g})")

    def n_estados(self):
        """Devuelve el número de estados continuos del bloque."""
        return 1

    def x0(self):
        """Devuelve el valor inicial de los estados."""
        return [float(self.p["x0"])]

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [min(self.ls, max(self.li, float(x[0])))]

    def derivada(self, t, x, u):
        """Calcula la derivada temporal de los estados."""
        v = float(u[0])
        if (x[0] >= self.ls and v > 0) or (x[0] <= self.li and v < 0):
            return [0.0]
        return [v]


@registrar
class Derivador(Bloque):
    """Derivador numérico.   y(t) ≈ (u(t) - u(t_ant)) / (t - t_ant), donde t_ant es el último paso aceptado."""
    tipo = "derivador"; nombre = "Derivador"; categoria = "Continuo"
    entradas = ["u"]; salidas = ["y"]

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.ta = None
        self.ua = 0.0
        self.ya = 0.0

    def reiniciar(self):
        """Devuelve el estado interno del bloque a su valor inicial."""
        self.preparar()

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        if self.ta is None or t <= self.ta:
            return [self.ya]
        return [(u[0] - self.ua) / (t - self.ta)]

    def paso_mayor(self, t, x, u, y):
        """Actualiza el estado discreto del bloque al final de un paso aceptado."""
        if self.ta is not None and t > self.ta:
            self.ya = (u[0] - self.ua) / (t - self.ta)
        self.ta = t
        self.ua = u[0]
        return False


@registrar
class FuncionTransferencia(Bloque):
    """Función de transferencia.   Y(s)/U(s) = (b0·s^m + ... + bm) / (a0·s^n + ... + an).

    Se convierte a espacio de estados en forma canónica controlable (dx/dt = A·x + B·u,
    y = C·x + D·u). Debe ser propia (m <= n).
    """
    tipo = "funcion_transferencia"; nombre = "Función de transferencia"; categoria = "Continuo"
    parametros = [P("num", "Numerador", "[1]"), P("den", "Denominador", "[1, 1]")]
    entradas = ["u"]; salidas = ["y"]

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        from scipy.signal import tf2ss
        num = np.atleast_1d(self.p["num"]).astype(float)
        den = np.atleast_1d(self.p["den"]).astype(float)
        if not np.any(den):
            raise ValueError("el denominador no puede ser nulo")
        if len(num) > len(den):
            raise ValueError(f"Bloque «{self.etiqueta}»: función impropia (numerador mayor que denominador)")
        A, B, C, D = tf2ss(num, den)
        self.A, self.B, self.C, self.D = A, B.ravel(), C.ravel(), float(np.ravel(D)[0])
        self.directo = self.D != 0.0

    def n_estados(self):
        """Devuelve el número de estados continuos del bloque."""
        return self.A.shape[0]

    def x0(self):
        """Devuelve el valor inicial de los estados."""
        return [0.0] * self.A.shape[0]

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        y = float(self.C @ x) if x.size else 0.0
        if self.directo:
            y += self.D * float(u[0])
        return [y]

    def derivada(self, t, x, u):
        """Calcula la derivada temporal de los estados."""
        return self.A @ x + self.B * float(u[0])


@registrar
class EspacioEstados(Bloque):
    """Espacio de estados.   dx/dt = A·x + B·u,   y = C·x + D·u."""
    tipo = "espacio_estados"; nombre = "Espacio de estados"; categoria = "Continuo"
    parametros = [P("A", "Matriz A", "[[-1]]"), P("B", "Matriz B", "[[1]]"), P("C", "Matriz C", "[[1]]"),
                  P("D", "Matriz D", "[[0]]"), P("x0", "Estado inicial", "0")]
    entradas = ["u"]; salidas = ["y"]

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.A = np.atleast_2d(self.p["A"]).astype(float)
        self.B = np.atleast_2d(self.p["B"]).astype(float)
        self.C = np.atleast_2d(self.p["C"]).astype(float)
        self.D = np.atleast_2d(self.p["D"]).astype(float)
        self.directo = bool(np.any(self.D != 0))

    def n_estados(self):
        """Devuelve el número de estados continuos del bloque."""
        return self.A.shape[0]

    def x0(self):
        """Devuelve el valor inicial de los estados."""
        return np.broadcast_to(np.ravel(self.p["x0"]).astype(float), (self.A.shape[0],)).tolist()

    def _vec(self, u):
        return np.ravel(u[0]).astype(float)

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        y = self.C @ x
        if self.directo:
            y = y + self.D @ self._vec(u)
        return [float(y[0]) if y.size == 1 else y]

    def derivada(self, t, x, u):
        """Calcula la derivada temporal de los estados."""
        return self.A @ x + self.B @ self._vec(u)


# ---------------------------------------------------------------- Discontinuidades y tablas


@registrar
class Saturacion(Bloque):
    """Saturación.   y = min(sup, max(inf, u)).

    Con paso variable mantiene su zona (inferior, lineal o superior) durante el paso y el solver
    localiza el instante en que u cruza un límite.
    """
    tipo = "saturacion"; nombre = "Saturación"; categoria = "Discontinuidades"
    parametros = [P("inf", "Límite inferior", -1.0), P("sup", "Límite superior", 1.0)]
    entradas = ["u"]; salidas = ["y"]
    de_evento = True

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        if self.p["inf"] > self.p["sup"]:
            raise ValueError(f"el límite inferior ({self.p['inf']:g}) es mayor que el superior ({self.p['sup']:g})")
        self.zona = None

    def reiniciar(self):
        """Devuelve el estado interno del bloque a su valor inicial."""
        self.zona = None

    def _zona(self, v):
        return -1 if v < self.p["inf"] else (1 if v > self.p["sup"] else 0)

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        a = u[0]
        if isinstance(a, np.ndarray):
            return [np.clip(a, self.p["inf"], self.p["sup"])]
        z = self.zona if (self.usar_zc and self.zona is not None) else self._zona(a)
        return [self.p["inf"] if z < 0 else (self.p["sup"] if z > 0 else a)]

    def cruce(self, t, x, u):
        """Devuelve las señales de cruce por cero del bloque."""
        a = u[0]
        if isinstance(a, np.ndarray):
            return []
        return [float(a) - float(self.p["inf"]), float(a) - float(self.p["sup"])]

    def paso_mayor(self, t, x, u, y):
        """Actualiza el estado discreto del bloque al final de un paso aceptado."""
        if not self.usar_zc or isinstance(u[0], np.ndarray):
            return False
        z = self._zona(float(u[0]))
        c = self.zona is not None and z != self.zona
        self.zona = z
        return c


@registrar
class Interruptor(Bloque):
    """Switch.   y = u1 si control >= umbral;  y = u3 en otro caso.

    Con paso variable mantiene su posición durante el paso y el solver localiza el cruce
    de control - umbral = 0, de modo que la discontinuidad no degrada el error.
    """
    tipo = "switch"; nombre = "Switch"; categoria = "Discontinuidades"
    parametros = [P("umbral", "Umbral", 0.5)]
    entradas = ["u1", "control", "u3"]; salidas = ["y"]
    de_evento = True

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.arriba = None

    def reiniciar(self):
        """Devuelve el estado interno del bloque a su valor inicial."""
        self.arriba = None

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        a = self.arriba if (self.usar_zc and self.arriba is not None) else u[1] >= self.p["umbral"]
        return [u[0] if a else u[2]]

    def cruce(self, t, x, u):
        """Devuelve las señales de cruce por cero del bloque."""
        return [float(u[1]) - float(self.p["umbral"])]

    def paso_mayor(self, t, x, u, y):
        """Actualiza el estado discreto del bloque al final de un paso aceptado."""
        if not self.usar_zc:
            return False
        a = bool(u[1] >= self.p["umbral"])
        c = self.arriba is not None and a != self.arriba
        self.arriba = a
        return c


@registrar
class Retardo(Bloque):
    """Retardo de transporte.   y(t) = u(t - τ)   (interpolación lineal del historial)."""
    tipo = "retardo"; nombre = "Retardo"; categoria = "Discontinuidades"
    parametros = [P("tau", "Retardo τ", 0.1, "s"), P("y0", "Salida inicial", 0.0)]
    entradas = ["u"]; salidas = ["y"]; directo = False

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        if not self.p["tau"] > 0.0:
            raise ValueError("el retardo τ debe ser positivo")
        self.tt = []
        self.uu = []

    def reiniciar(self):
        """Devuelve el estado interno del bloque a su valor inicial."""
        self.tt = []
        self.uu = []

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        td = t - self.p["tau"]
        if not self.tt or td < self.tt[0]:
            return [self.p["y0"]]
        if td >= self.tt[-1]:
            return [self.uu[-1]]
        i = bisect.bisect_right(self.tt, td)
        t0, t1 = self.tt[i - 1], self.tt[i]
        a = (td - t0) / (t1 - t0) if t1 > t0 else 0.0
        return [self.uu[i - 1] + a * (self.uu[i] - self.uu[i - 1])]

    def paso_mayor(self, t, x, u, y):
        """Actualiza el estado discreto del bloque al final de un paso aceptado."""
        if not self.tt or t > self.tt[-1]:
            self.tt.append(t)
            self.uu.append(float(u[0]))
        return False


@registrar
class Memoria(Bloque):
    """Memoria de un paso.   y(t) = u en el último paso aceptado (rompe lazos algebraicos)."""
    tipo = "memoria"; nombre = "Memoria"; categoria = "Discontinuidades"
    parametros = [P("y0", "Salida inicial", 0.0)]
    entradas = ["u"]; salidas = ["y"]; directo = False

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.v = self.p["y0"]

    def reiniciar(self):
        """Devuelve el estado interno del bloque a su valor inicial."""
        self.preparar()

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        return [self.v]

    def paso_mayor(self, t, x, u, y):
        """Actualiza el estado discreto del bloque al final de un paso aceptado."""
        self.v = u[0]
        return False


@registrar
class TablaBusqueda(Bloque):
    """Lookup table 1-D.   y = interpolación lineal de (xs, ys) en u; extrapola o satura fuera del rango."""
    tipo = "lookup"; nombre = "Lookup table"; categoria = "Discontinuidades"
    parametros = [P("xs", "Puntos x", "[-5, 0, 5]"), P("ys", "Valores y", "[-1, 0, 1]"),
                  P("fuera", "Fuera de rango", "saturar", tipo="opcion", opciones=["saturar", "extrapolar"])]
    entradas = ["u"]; salidas = ["y"]

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        self.xs = np.ravel(self.p["xs"]).astype(float).tolist()
        self.ys = np.ravel(self.p["ys"]).astype(float).tolist()
        if len(self.xs) != len(self.ys) or len(self.xs) < 2:
            raise ValueError(f"Bloque «{self.etiqueta}»: x e y deben tener igual largo (al menos 2)")

    def salida(self, t, x, u):
        """Calcula las salidas del bloque."""
        v = float(u[0])
        xs, ys = self.xs, self.ys
        if self.p["fuera"] == "saturar":
            if v <= xs[0]:
                return [ys[0]]
            if v >= xs[-1]:
                return [ys[-1]]
        i = min(max(bisect.bisect_right(xs, v), 1), len(xs) - 1)
        a = (v - xs[i - 1]) / (xs[i] - xs[i - 1])
        return [ys[i - 1] + a * (ys[i] - ys[i - 1])]


# ---------------------------------------------------------------- Sumideros


class _Registrador(Bloque):
    directo = False
    sumidero = True

    def preparar(self):
        self.ts = []
        self.vs = []
        self._n = 0

    def reiniciar(self):
        self.preparar()

    def paso_mayor(self, t, x, u, y):
        self._n += 1
        if (self._n - 1) % max(1, int(self.p.get("cada", 1))) == 0:
            self.ts.append(t)
            self.vs.append([a.copy() if isinstance(a, np.ndarray) else float(a) for a in u])
        return False


@registrar
class Osciloscopio(_Registrador):
    """Osciloscopio: registra y muestra sus entradas respecto al tiempo."""
    tipo = "osciloscopio"; nombre = "Osciloscopio"; categoria = "Sumideros"; color = "sumidero"
    parametros = [P("n", "Entradas", 1), P("cada", "Guardar 1 de cada", 1)]
    puertos_dinamicos = {"param": "n", "modo": "valor", "prefijo": "u"}

    @classmethod
    def puertos(cls, params=None):
        """Devuelve los puertos del bloque según sus parámetros."""
        n = int(float((params or {}).get("n", 1)))
        return {"entradas": [f"u{i+1}" for i in range(n)], "salidas": [], "e": [], "e3": []}

    def preparar(self):
        """Deriva de los parámetros los atributos de cálculo del bloque."""
        _entero_positivo(self.p["n"], "el número de entradas")
        super().preparar()


@registrar
class AEspacio(_Registrador):
    """Exportar al espacio de trabajo: crea una variable con el tiempo y la señal al terminar."""
    tipo = "a_espacio"; nombre = "Exportar a espacio"; categoria = "Sumideros"; color = "sumidero"
    parametros = [P("variable", "Variable", "salida", tipo="texto"), P("cada", "Guardar 1 de cada", 1)]
    entradas = ["u"]


# ---------------------------------------------------------------- Subsistemas


@registrar
class Entrada(Bloque):
    """Puerto de entrada de un subsistema."""
    tipo = "entrada"; nombre = "Entrada"; categoria = "Subsistemas"; color = "puerto"
    parametros = [P("numero", "Número", 1)]
    salidas = ["y"]


@registrar
class Salida(Bloque):
    """Puerto de salida de un subsistema."""
    tipo = "salida"; nombre = "Salida"; categoria = "Subsistemas"; color = "puerto"
    parametros = [P("numero", "Número", 1)]
    entradas = ["u"]


@registrar
class PuertoE(Bloque):
    """Puerto eléctrico de un subsistema (monofásico)."""
    tipo = "puerto_e"; nombre = "Puerto eléctrico"; categoria = "Subsistemas"; color = "puerto"
    parametros = [P("numero", "Número", 1)]
    puertos_e = ["e"]


@registrar
class PuertoE3(Bloque):
    """Puerto eléctrico de un subsistema (trifásico)."""
    tipo = "puerto_e3"; nombre = "Puerto trifásico"; categoria = "Subsistemas"; color = "puerto"
    parametros = [P("numero", "Número", 1)]
    puertos_e3 = ["e"]


@registrar
class Subsistema(Bloque):
    """Subsistema: un diagrama dentro de un bloque; se puede anidar sin límite.

    Dentro se usan los bloques Entrada, Salida y Puerto eléctrico. Al simular se aplana.
    """
    tipo = "subsistema"; nombre = "Subsistema"; categoria = "Subsistemas"
