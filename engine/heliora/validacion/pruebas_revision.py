"""Regresiones de la revisión independiente: evaluador seguro, guardado sin pérdidas y etiquetas de clasificación."""
from __future__ import annotations

import os
import tempfile

import numpy as np
import pandas as pd

from .banco import prueba
from .. import almacen
from ..sim.nucleo import evaluar_numero


def _falla(fn):
    """True si la llamada lanza una excepción."""
    try:
        fn()
    except Exception:
        return True
    return False


@prueba(1, "Expresiones: solo matemática sobre datos", "Lista blanca del evaluador")
def expresiones_seguras(m):
    """Ningún parámetro alcanza ficheros, módulos ni funciones del espacio; los usos matemáticos siguen igual."""
    with tempfile.TemporaryDirectory() as td:
        ruta = os.path.join(td, "x.bin").replace("\\", "/")
        ns = {"np": np, "pd": pd, "__builtins__": __builtins__, "f": print, "Vdc": 750.0, "v": np.array([1.0, 2.0, 3.0]),
              "d": pd.DataFrame({"a": [1.0, 2.0]})}
        m.verdad("np.lib.format.open_memmap rechazado y sin archivo", _falla(lambda: evaluar_numero(
            f"np.lib.format.open_memmap('{ruta}', mode='w+', dtype='u1', shape=(1,))", ns)) and not os.path.exists(ruta))
        m.verdad("v.tofile rechazado", _falla(lambda: evaluar_numero(f"v.tofile('{ruta}')", ns)) and not os.path.exists(ruta))
        for t in ("d.agg('sum')", "pd.DataFrame", "f(1)", "__builtins__", "(1).__class__", "np.load", "[f][0](1)"):
            m.verdad(f"rechazado: {t}", _falla(lambda t=t: evaluar_numero(t, ns)))
        m.error("2*pi*50", evaluar_numero("2*pi*50", ns), 100 * np.pi, 1e-12)
        m.error("Vdc/2", evaluar_numero("Vdc/2", ns), 375.0, 0.0)
        m.error("np.sqrt(v).sum()", evaluar_numero("np.sqrt(v).sum()", ns), 1 + np.sqrt(2) + np.sqrt(3), 1e-12)
        m.error("d.values.sum()", evaluar_numero("d.values.sum()", ns), 3.0, 0.0)
    from ..lib.senal import _evaluar_texto
    m.verdad("Tabla N-D: el texto tampoco usa eval", _falla(lambda: _evaluar_texto("().__class__.__base__.__subclasses__()", {})))
    m.verdad("Tabla N-D: listas de puntos", _evaluar_texto("[[0, 1], [0, 1, 2]]", {}) == [[0, 1], [0, 1, 2]])


@prueba(1, "Guardar no borra lo que no está en memoria", "Archivos del índice")
def guardado_sin_perdidas(m):
    """Tras un reinicio del motor (espacio vacío) guardar conserva los archivos; solo se borra lo borrado a propósito."""
    with tempfile.TemporaryDirectory() as td:
        almacen.guardar({"A": np.eye(2), "a": np.array([7.0]), "modelo": {1, 2}}, td)
        r = almacen.guardar({}, td)
        v, _, pend = almacen.cargar(td)
        m.verdad("Espacio vacío: A y a se conservan", sorted(r["conservados"]) == ["A", "a", "modelo"] and "A" in v and "a" in v)
        m.verdad("A y a no colisionan", np.array_equal(v["A"], np.eye(2)) and np.array_equal(v["a"], [7.0]))
        m.verdad("El objeto .pkl no se carga sin confirmación", pend == ["modelo"] and "modelo" not in v)
        v2, _, _ = almacen.cargar(td, confiar=True, solo=["modelo"])
        m.verdad("Con confirmación se carga solo lo pedido", v2 == {"modelo": {1, 2}})
        almacen.guardar({}, td, borradas={"a"})
        v3, _, _ = almacen.cargar(td)
        m.verdad("Lo borrado a propósito desaparece", "a" not in v3 and "A" in v3)


@prueba(3, "Clasificación con etiquetas de texto de distinta longitud", "Etiquetas escritas en los datos")
def etiquetas_de_texto(m):
    """Las predicciones conservan la etiqueta completa aunque la primera fila tenga una más corta."""
    from ..lib.aprendizaje import entrenar_clasificacion
    rng = np.random.default_rng(0)
    x = rng.normal(size=(120, 2))
    y = np.where(x[:, 0] > 0, "falla", "ok")
    y[0] = "ok"
    _, r, pred = entrenar_clasificacion(x, y, nombre="logistica", pliegues=4)
    validas = set(pred[r["validada"]].tolist())
    m.verdad("Predicciones con etiquetas completas", validas <= {"ok", "falla"} and "falla" in validas)
    m.cota("Exactitud de validación ≥ 0,9 en datos separables", max(0.0, 0.9 - r["exactitud"]), 0.0)


@prueba(4, "Herramientas de mantenimiento: nombres de salida", "Capa de validación")
def salidas_mantenimiento(m):
    """Generar SCADA con un nombre reservado falla con mensaje; con uno válido crea la tabla y la anota."""
    from .. import herramientas
    from ..espacio import Espacio
    esp = Espacio()
    m.verdad("Nombre reservado «pd» rechazado", _falla(lambda: herramientas.ejecutar("mt_muestra", {"nombre": "pd", "dias": 5}, esp)))
    herramientas.ejecutar("mt_muestra", {"nombre": "scada", "dias": 5}, esp)
    m.verdad("Tabla creada y registrada como salida", isinstance(esp.ns.get("scada"), pd.DataFrame) and "scada" in esp.ns["_salidas_herr"])
    m.verdad("Repetir con el mismo nombre la sustituye", herramientas.ejecutar("mt_muestra", {"nombre": "scada", "dias": 6}, esp) is not None)
    m.verdad("Días = 0 rechazado", _falla(lambda: herramientas.ejecutar("mt_muestra", {"nombre": "otra", "dias": 0}, esp)))


@prueba(1, "Código de bloque que falla al cargarse no queda escrito", "Contenido del archivo")
def recarga_fallida_repone(m):
    """Si la recarga de librerías falla tras escribir un módulo del motor, se repone el texto anterior."""
    from ..servidor import Motor
    import heliora
    motor = Motor()
    ruta = os.path.join(os.path.dirname(heliora.__file__), "_prueba_recarga.py")
    with open(ruta, "w", encoding="utf8", newline="\n") as f:
        f.write("x = 1\n")
    try:
        def recargar():
            raise NameError("nombre_inexistente")
        motor.recargar_librerias = recargar
        fallo = _falla(lambda: motor.escribir_archivo(ruta, "x = nombre_inexistente\n"))
        with open(ruta, encoding="utf8") as f:
            texto = f.read()
        m.verdad("La escritura se rechaza", fallo)
        m.verdad("El archivo conserva el texto anterior", texto == "x = 1\n")
    finally:
        os.remove(ruta)
