"""Espacio de trabajo y consola: variables, ejecución de código y gráficas."""
from __future__ import annotations

import ast
import contextlib
import io
import traceback

import numpy as np
import pandas as pd

MAX_PUNTOS = 3000


def reducir(x, y, maximo=MAX_PUNTOS):
    """Reduce una serie conservando mínimos y máximos por cubeta, para dibujarla rápido."""
    x = np.asarray(x, dtype=float).ravel()
    y = np.asarray(y, dtype=float).ravel()
    n = min(x.size, y.size)
    x, y = x[:n], y[:n]
    if n <= maximo:
        return x, y
    cubetas = maximo // 2
    borde = np.linspace(0, n, cubetas + 1).astype(int)
    xs, ys = [], []
    for a, b in zip(borde[:-1], borde[1:]):
        if b <= a:
            continue
        seg = y[a:b]
        if np.all(np.isnan(seg)):
            continue
        i, j = int(np.nanargmin(seg)), int(np.nanargmax(seg))
        for k in sorted((i, j)):
            xs.append(x[a + k])
            ys.append(seg[k])
    return np.asarray(xs), np.asarray(ys)


def _lista(a):
    return [None if not np.isfinite(v) else float(v) for v in np.asarray(a, dtype=float).ravel()]


def serie(x, y, nombre="", tipo="linea", color=None):
    """Construye una serie para una gráfica 2D, reducida a un máximo de puntos, con `x` e `y` en listas JSON."""
    x, y = reducir(x, y)
    d = {"x": _lista(x), "y": _lista(y), "nombre": nombre, "tipo": tipo}
    if color:
        d["color"] = color
    return d


def figura(titulo="", xlabel="", ylabel="", series=None, lineas_h=None, log_x=False, log_y=False):
    """Construye una figura 2D con su título, etiquetas de ejes, series, líneas horizontales y ejes logarítmicos opcionales."""
    return {"clase": "2d", "titulo": titulo, "xlabel": xlabel, "ylabel": ylabel, "series": series or [],
            "lineas_h": lineas_h or [], "log_x": log_x, "log_y": log_y}


def figura3d(titulo="", X=None, Y=None, Z=None, xlabel="x", ylabel="y", zlabel="z", tipo="superficie"):
    """Construye una figura 3D, de tipo `superficie` (malla submuestreada) o línea, con coordenadas en listas JSON."""
    X, Y, Z = np.asarray(X, dtype=float), np.asarray(Y, dtype=float), np.asarray(Z, dtype=float)
    if tipo == "superficie":
        if X.ndim == 1:
            X, Y = np.meshgrid(X, Y)
        paso = max(1, max(Z.shape) // 80)
        X, Y, Z = X[::paso, ::paso], Y[::paso, ::paso], Z[::paso, ::paso]
        return {"clase": "3d", "tipo": tipo, "titulo": titulo, "xlabel": xlabel, "ylabel": ylabel, "zlabel": zlabel,
                "x": [_lista(f) for f in X], "y": [_lista(f) for f in Y], "z": [_lista(f) for f in Z]}
    return {"clase": "3d", "tipo": tipo, "titulo": titulo, "xlabel": xlabel, "ylabel": ylabel, "zlabel": zlabel,
            "x": _lista(X), "y": _lista(Y), "z": _lista(Z)}


class Graficas:
    """Funciones de gráfica de la consola. Cada llamada añade o crea una figura."""

    def __init__(self):
        self.figuras = []
        self.actual = None

    def _fig(self, nueva=True):
        if nueva or self.actual is None or self.actual["clase"] != "2d":
            self.actual = figura()
            self.figuras.append(self.actual)
        return self.actual

    def plot(self, x, y=None, nombre="", titulo=None, xlabel=None, ylabel=None, nueva=None, tipo="linea"):
        """Dibuja `y` (o `y` frente a `x`); con `nueva=False` lo hace sobre la figura anterior."""
        if y is None:
            y = np.asarray(x, dtype=float)
            if y.ndim == 2:
                f = self._fig(nueva is not False)
                for i in range(y.shape[1]):
                    f["series"].append(serie(np.arange(y.shape[0]), y[:, i], f"col {i + 1}", tipo))
                return f
            x = np.arange(y.size)
        f = self._fig(nueva is not False if nueva is not None else True)
        f["series"].append(serie(x, y, nombre, tipo))
        if titulo:
            f["titulo"] = titulo
        if xlabel:
            f["xlabel"] = xlabel
        if ylabel:
            f["ylabel"] = ylabel
        return f

    def scatter(self, x, y, nombre="", **kw):
        """Dibuja `x` frente a `y` como puntos."""
        return self.plot(x, y, nombre, tipo="puntos", **kw)

    def bar(self, x, y, nombre="", **kw):
        """Dibuja `x` frente a `y` como barras."""
        return self.plot(x, y, nombre, tipo="barras", **kw)

    def hist(self, x, cubetas=30, nombre="", **kw):
        """Dibuja el histograma de los valores finitos de `x` con `cubetas` cubetas."""
        x = np.asarray(x, dtype=float)
        h, e = np.histogram(x[np.isfinite(x)], bins=int(cubetas))
        return self.plot(0.5 * (e[:-1] + e[1:]), h, nombre, tipo="barras", **kw)

    def surf(self, X, Y, Z, titulo=""):
        """Añade una figura de superficie 3D con la malla `X`, `Y`, `Z` y la devuelve."""
        f = figura3d(titulo, X, Y, Z)
        self.figuras.append(f)
        self.actual = f
        return f

    def plot3(self, x, y, z, titulo=""):
        """Añade una figura de línea 3D con las coordenadas `x`, `y`, `z` y la devuelve."""
        f = figura3d(titulo, x, y, z, tipo="linea3d")
        self.figuras.append(f)
        self.actual = f
        return f

    def titulo(self, t):
        """Fija el título de la figura actual, si existe."""
        if self.actual:
            self.actual["titulo"] = t

    def xlabel(self, t):
        """Fija la etiqueta del eje x de la figura actual, si existe."""
        if self.actual:
            self.actual["xlabel"] = t

    def ylabel(self, t):
        """Fija la etiqueta del eje y de la figura actual, si existe."""
        if self.actual:
            self.actual["ylabel"] = t


class _Silencio:
    """El resultado de plot() no debe escribirse en la consola."""


class Espacio:
    """El espacio de trabajo de la sesión."""

    def __init__(self):
        self.g = Graficas()
        self.reiniciar()

    def reiniciar(self):
        """Recrea el espacio de nombres de la consola con solo las funciones y librerías base."""
        import scipy
        from scipy import integrate, interpolate, linalg, optimize, signal, stats

        from .sim import solvers
        ns = {"__name__": "__consola__", "np": np, "pd": pd, "scipy": scipy, "linalg": linalg, "integrate": integrate,
              "interpolate": interpolate, "optimize": optimize, "signal": signal, "stats": stats,
              "plot": self.g.plot, "scatter": self.g.scatter, "bar": self.g.bar, "hist": self.g.hist, "surf": self.g.surf,
              "plot3": self.g.plot3, "titulo": self.g.titulo, "xlabel": self.g.xlabel, "ylabel": self.g.ylabel,
              "integrar": solvers.integrar, "cargar": self.cargar, "guardar": self.guardar, "simular": self._simular}
        self.ns = ns
        self.base = set(ns.keys())

    def _simular(self, diagrama):
        from .sim.diagrama import simular
        return simular(diagrama, self.ns)

    def cargar(self, ruta, nombre=None):
        """Carga un CSV o Parquet como DataFrame; con `nombre` lo deja también como variable."""
        d = pd.read_parquet(ruta) if str(ruta).lower().endswith(".parquet") else pd.read_csv(ruta)
        if nombre:
            self.ns[nombre] = d
        return d

    def guardar(self, dato, ruta):
        """Guarda un DataFrame o un arreglo en CSV o Parquet, según la extensión."""
        d = dato if isinstance(dato, pd.DataFrame) else pd.DataFrame(np.atleast_1d(dato))
        d.columns = [str(c) for c in d.columns]
        if str(ruta).lower().endswith(".parquet"):
            d.to_parquet(ruta)
        else:
            # LF en todos los sistemas, como pide el .gitattributes del proyecto
            d.to_csv(ruta, index=False, lineterminator="\n")
        return ruta

    def _ejecutar(self, codigo, interactivo=True):
        salida = io.StringIO()
        error = None
        resultado = None
        self.g.figuras = []
        with contextlib.redirect_stdout(salida), contextlib.redirect_stderr(salida):
            try:
                arbol = ast.parse(codigo, mode="exec")
                cuerpo = arbol.body
                ultimo = None
                if interactivo and cuerpo and isinstance(cuerpo[-1], ast.Expr):
                    ultimo = ast.Expression(cuerpo.pop().value)
                if cuerpo:
                    exec(compile(arbol, "<consola>", "exec"), self.ns)
                if ultimo is not None:
                    resultado = eval(compile(ultimo, "<consola>", "eval"), self.ns)
            except Exception as e:
                # Solo interesa la parte del código del usuario
                tb = e.__traceback__.tb_next if e.__traceback__ else None
                error = "".join(traceback.format_exception(type(e), e, tb)).replace("Traceback (most recent call last):\n", "").strip()
            except KeyboardInterrupt:
                # Cancelado desde la interfaz: las variables ya asignadas se conservan
                error = "Detenido por el usuario."
        texto = salida.getvalue()
        repr_res = None
        if resultado is not None and not (isinstance(resultado, dict) and "clase" in resultado):
            repr_res = _formatear(resultado)
            self.ns["ans"] = resultado
        return {"salida": texto, "resultado": repr_res, "error": error, "figuras": self.g.figuras, "variables": self.variables()}

    def evaluar(self, codigo):
        """Ejecuta el código en modo interactivo y devuelve el valor de la última expresión."""
        return self._ejecutar(codigo, True)

    def ejecutar_script(self, codigo):
        """Ejecuta el código completo sin evaluar la última expresión."""
        return self._ejecutar(codigo, False)

    def variables(self):
        """Devuelve la descripción de las variables del usuario, ordenadas por nombre."""
        filas = []
        for k, v in list(self.ns.items()):
            if k in self.base or k.startswith("_") or callable(v) or isinstance(v, type) or type(v).__name__ == "module":
                continue
            filas.append(describir_var(k, v))
        return sorted(filas, key=lambda f: f["nombre"].lower())

    def ver(self, nombre, limite=500):
        """Devuelve las columnas y filas (hasta `limite`) de la variable `nombre` para mostrarla en tabla."""
        v = self.ns[nombre]
        if isinstance(v, pd.Series):
            v = v.to_frame(v.name or "valor")
        if isinstance(v, pd.DataFrame):
            d = v.head(limite)
            return {"columnas": [str(c) for c in d.columns], "filas": [[_celda(x) for x in f] for f in d.itertuples(index=False)],
                    "total": len(v)}
        if isinstance(v, np.ndarray):
            a = np.atleast_2d(v) if v.ndim <= 2 else v.reshape(v.shape[0], -1)
            if v.ndim == 1:
                a = v[:, None]
            a = a[:limite, :50]
            return {"columnas": [str(i + 1) for i in range(a.shape[1])], "filas": [[_celda(x) for x in f] for f in a], "total": int(v.shape[0])}
        return {"columnas": ["valor"], "filas": [[str(v)[:5000]]], "total": 1}


def _celda(x):
    if x is None:
        return ""
    if isinstance(x, (float, np.floating)):
        return "" if not np.isfinite(x) else f"{float(x):.6g}"
    if isinstance(x, (int, np.integer, bool, np.bool_)):
        return str(x)
    return str(x)


def describir_var(k, v):
    """Devuelve el nombre, tipo, forma y resumen de la variable `k` de valor `v`."""
    if isinstance(v, pd.DataFrame):
        return {"nombre": k, "tipo": "tabla", "forma": f"{v.shape[0]} × {v.shape[1]}", "resumen": ", ".join(str(c) for c in v.columns[:6]),
                "columnas": [str(c) for c in v.columns], "numericas": [str(c) for c in v.select_dtypes("number").columns]}
    if isinstance(v, pd.Series):
        return {"nombre": k, "tipo": "serie", "forma": f"{v.shape[0]}", "resumen": f"{v.dtype}", "columnas": [str(v.name or k)], "numericas": []}
    if isinstance(v, np.ndarray):
        res = ""
        if v.size and np.issubdtype(v.dtype, np.number):
            res = f"[{np.nanmin(v.real):.4g}, {np.nanmax(v.real):.4g}]"
        return {"nombre": k, "tipo": f"matriz {v.dtype}", "forma": " × ".join(str(s) for s in v.shape) or "1", "resumen": res, "columnas": [], "numericas": []}
    if isinstance(v, (int, float, complex, np.number)) and not isinstance(v, bool):
        return {"nombre": k, "tipo": type(v).__name__, "forma": "1", "resumen": f"{v:.6g}" if not isinstance(v, complex) else str(v), "columnas": [], "numericas": []}
    return {"nombre": k, "tipo": type(v).__name__, "forma": str(len(v)) if hasattr(v, "__len__") else "1", "resumen": str(v)[:60], "columnas": [], "numericas": []}


def _formatear(v):
    if isinstance(v, np.ndarray):
        with np.printoptions(precision=5, suppress=False, linewidth=110, threshold=200):
            return repr(v)
    if isinstance(v, (pd.DataFrame, pd.Series)):
        with pd.option_context("display.max_rows", 20, "display.max_columns", 12, "display.width", 110):
            return str(v)
    return repr(v)
