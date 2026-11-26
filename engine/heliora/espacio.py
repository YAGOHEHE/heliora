"""Espacio de trabajo y consola: variables, ejecución de código y gráficas."""
from __future__ import annotations

import ast
import contextlib
import io
import sys
import traceback

import numpy as np
import pandas as pd

from . import almacen

MAX_PUNTOS = 3000
MAX_SALIDA = 200_000
_AUSENTE = object()


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


def figura(titulo="", xlabel="", ylabel="", series=None, lineas_h=None, log_x=False, log_y=False, nota=""):
    """Construye una figura 2D con su título, etiquetas de ejes, series, líneas horizontales y ejes logarítmicos opcionales.

    `nota` se dibuja sobre el área de la gráfica: explica, por ejemplo, por qué faltan series.
    """
    r = {"clase": "2d", "titulo": titulo, "xlabel": xlabel, "ylabel": ylabel, "series": series or [],
         "lineas_h": lineas_h or [], "log_x": log_x, "log_y": log_y}
    if nota:
        r["nota"] = nota
    return r


def figura3d(titulo="", X=None, Y=None, Z=None, xlabel="x", ylabel="y", zlabel="z", tipo="superficie"):
    """Construye una figura 3D, de tipo `superficie` (malla submuestreada) o línea, con coordenadas en listas JSON."""
    X, Y, Z = np.asarray(X, dtype=float), np.asarray(Y, dtype=float), np.asarray(Z, dtype=float)
    if tipo == "superficie":
        if X.ndim == 1 and Y.ndim == 1:
            X, Y = np.meshgrid(X, Y)
        if not X.shape == Y.shape == Z.shape or Z.ndim != 2:
            raise ValueError(
                f"surf: X, Y y Z deben tener la misma forma 2D (X {X.shape}, Y {Y.shape}, Z {Z.shape}); "
                "con X e Y como vectores, Z debe ser de len(Y) × len(X)")
        paso = max(1, max(Z.shape) // 80)
        X, Y, Z = X[::paso, ::paso], Y[::paso, ::paso], Z[::paso, ::paso]
        return {"clase": "3d", "tipo": tipo, "titulo": titulo, "xlabel": xlabel, "ylabel": ylabel, "zlabel": zlabel,
                "x": [_lista(f) for f in X], "y": [_lista(f) for f in Y], "z": [_lista(f) for f in Z]}
    X, Y, Z = X.ravel(), Y.ravel(), Z.ravel()
    if not X.size == Y.size == Z.size:
        raise ValueError(f"plot3: x, y y z deben tener la misma longitud (x {X.size}, y {Y.size}, z {Z.size})")
    if X.size > MAX_PUNTOS:
        # Paso uniforme: los tres ejes comparten los mismos índices y la última muestra se conserva
        i = np.unique(np.append(np.arange(0, X.size, -(-X.size // MAX_PUNTOS)), X.size - 1))
        X, Y, Z = X[i], Y[i], Z[i]
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


class _Salida(io.TextIOBase):
    """Búfer de la salida de la consola que descarta lo que exceda de `MAX_SALIDA` caracteres."""

    def __init__(self):
        self.partes = []
        self.n = 0
        self.truncada = False

    def writable(self):
        """Indica que el búfer admite escritura."""
        return True

    def write(self, s):
        """Añade `s` al búfer hasta el límite y devuelve su longitud, como un flujo normal."""
        escrito = len(s)
        resto = MAX_SALIDA - self.n
        if escrito > resto:
            self.truncada = True
            s = s[:max(resto, 0)]
        self.partes.append(s)
        self.n += len(s)
        return escrito

    def getvalue(self):
        """Devuelve el texto escrito, con un aviso si se truncó."""
        texto = "".join(self.partes)
        return texto + f"\n[salida truncada: se muestran los primeros {MAX_SALIDA // 1000} KB]" if self.truncada else texto


class Espacio:
    """El espacio de trabajo de la sesión."""

    def __init__(self):
        self.g = Graficas()
        self.reiniciar()

    def reiniciar(self):
        """Recrea el espacio de nombres de la consola con solo las funciones y librerías base."""
        self.borradas = set()
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
        self.base_obj = dict(ns)

    def _simular(self, diagrama):
        from .sim.diagrama import simular
        return simular(diagrama, self.ns)

    def es_usuario(self, nombre, valor):
        """Indica si `nombre` es una variable del usuario: no es privada y no es el símbolo base original."""
        return not nombre.startswith("_") and self.base_obj.get(nombre, _AUSENTE) is not valor

    def nombre_variable(self, texto):
        """Convierte `texto` en un nombre de variable válido que no pise un símbolo base como `plot`."""
        return almacen.nombre_variable(texto, self.base)

    def cargar(self, ruta, nombre=None):
        """Carga una tabla (CSV, TSV, TXT o Parquet) como DataFrame; con `nombre` la deja también como variable.

        El nombre se sanea hasta ser un identificador válido que no coincida con un símbolo base.

        Raises:
            ErrorLectura: si el archivo no se puede leer como tabla.
        """
        from .lectura import leer_tabla
        d = leer_tabla(ruta)
        if nombre:
            self.ns[self.nombre_variable(nombre)] = d
        return d

    def borrar(self, nombre):
        """Elimina una variable y la anota para que el próximo guardado borre también su archivo."""
        self.ns.pop(nombre, None)
        self.borradas.add(nombre)

    def borrar_todo(self):
        """Elimina todas las variables del usuario y las anota como borradas."""
        self.borradas |= {k for k, v in self.ns.items() if self.es_usuario(k, v)}
        borradas = self.borradas
        self.reiniciar()
        self.borradas = borradas

    def guardar_en(self, carpeta):
        """Guarda las variables del usuario en una carpeta (ver `almacen.guardar`) y devuelve guardados, conservados y omitidos."""
        meta = {"salidas_herr": sorted(self.ns.get("_salidas_herr", ()))}
        r = almacen.guardar({k: v for k, v in self.ns.items() if self.es_usuario(k, v)}, carpeta, self.borradas, meta)
        self.borradas = set()
        return r

    def cargar_de(self, carpeta, confiar=False, solo=None):
        """Carga en el espacio las variables guardadas en una carpeta.

        Returns:
            Tupla (errores, pendientes): mensajes de lectura y objetos `.pkl` que esperan confirmación.
        """
        variables, errores, pendientes = almacen.cargar(carpeta, self.base, confiar, solo)
        self.ns.update(variables)
        salidas = almacen.leer_meta(carpeta).get("salidas_herr", [])
        self.ns.setdefault("_salidas_herr", set()).update(n for n in salidas if isinstance(n, str))
        if solo is None:
            self.borradas = set()
        return errores, pendientes

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

    def _ejecutar(self, codigo, interactivo=True, ruta=None):
        salida = _Salida()
        error = None
        resultado = None
        self.g.figuras = []
        # input() leería la entrada estándar, que es el canal del protocolo con la interfaz
        entrada, sys.stdin = sys.stdin, io.StringIO("")
        previos = {} if interactivo else {k: self.ns.get(k, _AUSENTE) for k in ("__name__", "__file__")}
        if not interactivo:
            self.ns["__name__"] = "__main__"
            if ruta:
                self.ns["__file__"] = str(ruta)
        try:
            with contextlib.redirect_stdout(salida), contextlib.redirect_stderr(salida):
                try:
                    arbol = ast.parse(codigo.lstrip("﻿"), "<consola>", "exec")
                except SyntaxError as e:
                    error = "".join(traceback.format_exception_only(type(e), e)).strip()
                else:
                    try:
                        cuerpo = arbol.body
                        ultimo = None
                        if interactivo and cuerpo and isinstance(cuerpo[-1], ast.Expr):
                            ultimo = ast.Expression(cuerpo.pop().value)
                        if cuerpo:
                            exec(compile(arbol, "<consola>", "exec"), self.ns)
                        if ultimo is not None:
                            resultado = eval(compile(ultimo, "<consola>", "eval"), self.ns)
                    except SystemExit:
                        error = "El código llamó a exit(), quit() o sys.exit(): la consola sigue abierta."
                    except Exception as e:
                        # Solo interesa la parte del código del usuario
                        tb = e.__traceback__.tb_next if e.__traceback__ else None
                        error = "".join(traceback.format_exception(type(e), e, tb)).replace("Traceback (most recent call last):\n", "").strip()
                        if isinstance(e, EOFError):
                            error += "\n(La consola no admite entrada interactiva: input() no está disponible.)"
                    except KeyboardInterrupt:
                        # Cancelado desde la interfaz: las variables ya asignadas se conservan
                        error = "Detenido por el usuario."
        finally:
            sys.stdin = entrada
            for k, v in previos.items():
                if v is _AUSENTE:
                    self.ns.pop(k, None)
                else:
                    self.ns[k] = v
        texto = salida.getvalue()
        repr_res = None
        if resultado is not None and not (isinstance(resultado, dict) and "clase" in resultado):
            try:
                repr_res = _formatear(resultado)[:MAX_SALIDA]
            except Exception as e:
                repr_res = f"<no se pudo mostrar el resultado: {type(e).__name__}: {e}>"
            self.ns["ans"] = resultado
        return {"salida": texto, "resultado": repr_res, "error": error, "figuras": self.g.figuras, "variables": self.variables()}

    def evaluar(self, codigo):
        """Ejecuta el código en modo interactivo y devuelve el valor de la última expresión."""
        return self._ejecutar(codigo, True)

    def ejecutar_script(self, codigo, ruta=None):
        """Ejecuta el código completo sin evaluar la última expresión, como `__main__` y con `__file__` igual a `ruta`."""
        return self._ejecutar(codigo, False, ruta)

    def variables(self):
        """Devuelve la descripción de las variables del usuario, ordenadas por nombre."""
        filas = []
        for k, v in list(self.ns.items()):
            if not self.es_usuario(k, v) or callable(v) or isinstance(v, type) or type(v).__name__ == "module":
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
        try:
            texto = str(v)[:5000]
        except Exception as e:
            texto = f"<no representable: {type(e).__name__}: {e}>"
        return {"columnas": ["valor"], "filas": [[texto]], "total": 1}


def _celda(x):
    if x is None:
        return ""
    if isinstance(x, (float, np.floating)):
        return "" if not np.isfinite(x) else f"{float(x):.6g}"
    if isinstance(x, (int, np.integer, bool, np.bool_)):
        return str(x)
    return str(x)


def describir_var(k, v):
    """Devuelve el nombre, tipo, forma y resumen de la variable `k` de valor `v`.

    Un objeto cuyo `__repr__` o `__len__` lanza una excepción se describe sin ellos, para que una
    sola variable no impida listar las demás.
    """
    try:
        return _describir(k, v)
    except Exception as e:
        return {"nombre": k, "tipo": type(v).__name__, "forma": "?", "resumen": f"<no representable: {type(e).__name__}>",
                "columnas": [], "numericas": []}


def _describir(k, v):
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
