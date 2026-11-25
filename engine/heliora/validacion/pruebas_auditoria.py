"""Seguridad, persistencia, importación y errores de bloque (correcciones de la auditoría)."""
from __future__ import annotations

import importlib
import os
import tempfile
from unittest import mock

import numpy as np
import pandas as pd

from .banco import prueba
from ..espacio import Espacio, figura3d
from ..sim import diagrama as dg
from ..sim.diagrama import bloque, diagrama, hilo, simular
from ..sim.nucleo import REGISTRO, evaluar_numero


def _error(fn, *args, **kw):
    """Devuelve la excepción que lanza `fn(*args)` o None si no lanza ninguna."""
    try:
        fn(*args, **kw)
    except Exception as e:
        return e
    return None


def _escribir(td, nombre, texto, cod="utf-8"):
    """Escribe un archivo de prueba y devuelve su ruta."""
    ruta = os.path.join(td, nombre)
    with open(ruta, "wb") as f:
        f.write(texto.encode(cod))
    return ruta


@prueba(1, "Expresiones numéricas sin escape de intérprete", "Lista blanca de nodos del AST")
def expresiones_seguras(m):
    """Las expresiones habituales se evalúan; las que escapan del intérprete o ejecutan código se rechazan."""
    ns = {"Vdc": 600.0, "v": np.array([1.0, 2.0, 3.0])}
    m.error("2*pi*50", evaluar_numero("2*pi*50"), 314.1592653589793, 1e-12)
    m.error("Vdc/2 con variable del espacio", evaluar_numero("Vdc/2", ns), 300.0, 0.0)
    m.error("np.sqrt(np.pi)", evaluar_numero("np.sqrt(np.pi)"), 1.7724538509055159, 1e-12)
    m.error("subíndice v[1]", evaluar_numero("v[1]", ns), 2.0, 0.0)
    m.error("método v.sum()", evaluar_numero("v.sum()", ns), 6.0, 0.0)
    m.verdad("lista [1, 2]", list(evaluar_numero("[1, 2]")) == [1.0, 2.0])
    m.verdad("lista de expresiones [1, 2*pi]", abs(evaluar_numero("[1, 2*pi]")[1] - 2 * np.pi) < 1e-12)
    m.error("coma decimal «0,5»", evaluar_numero("0,5"), 0.5, 0.0)
    m.error("condicional 1 if Vdc > 1 else 0", evaluar_numero("1 if Vdc > 1 else 0", ns), 1.0, 0.0)
    for texto in ("().__class__.__base__.__subclasses__()", "(lambda: 1)()", "[x.__class__ for x in (1, 2)]", "[x for x in v.__class__.__mro__]", "open('x')",
                  "np.load('x.npy')", "v.__class__", "__import__('os')", "9**9**9", "np.lib.npyio.os"):
        e = _error(evaluar_numero, texto, ns)
        m.verdad(f"rechaza {texto}", isinstance(e, (ValueError, AttributeError)), str(e)[:70])
    m.verdad("comprensión sobre datos [2*x for x in (1, 2)]", list(evaluar_numero("[2*x for x in (1, 2)]")) == [2.0, 4.0])
    m.verdad("campo vacío es un error «falta valor»", "falta valor" in str(_error(evaluar_numero, "  ")))
    d = diagrama("x", [bloque("g", "ganancia", nombre="G", k="().__class__.__base__.__subclasses__()"),
                       bloque("c", "constante", c=1.0), bloque("o", "osciloscopio")],
                 [hilo("c.y", "g.u"), hilo("g.y", "o.u1")], t_fin=0.1, metodo="rk4", paso=0.01)
    e = _error(simular, d)
    m.verdad("un diagrama con una expresión maliciosa no se simula", isinstance(e, ValueError) and "Bloque «G»" in str(e), str(e)[:90])


@prueba(1, "Espacio de trabajo: guardar y cargar sin pérdidas", "Ida y vuelta de cada tipo de variable")
def espacio_ida_y_vuelta(m):
    """Variables con nombres que solo difieren en mayúsculas, complejos, 3D y tablas vuelven idénticas; lo omitido se declara."""
    e = Espacio()
    e.evaluar("A = np.array([[1., 2.], [3., 4.]]); a = np.arange(3); c = np.array([1 + 2j, 3 - 1j])")
    e.evaluar("T = np.arange(8).reshape(2, 2, 2); s = 'hola'; d = {'k': [1, 2]}; tup = (1, 2); df = pd.DataFrame({'p': [1., 2.]})")
    e.evaluar("stats = 5; f = lambda x: x")
    with tempfile.TemporaryDirectory() as td:
        r = e.guardar_en(td)
        m.verdad("el nombre de archivo de «A» y «a» es distinto sin distinguir mayúsculas",
                 len({n.lower() for n in os.listdir(td)}) == len(os.listdir(td)))
        m.verdad("lo no serializable se declara omitido", [o["nombre"] for o in r["omitidos"]] == ["f"], str(r["omitidos"]))
        m.verdad("una variable con nombre de símbolo base se guarda", "stats" in r["guardados"])
        e2 = Espacio()
        errores, pendientes = e2.cargar_de(td)
        m.verdad("sin errores de lectura", errores == [], str(errores))
        m.verdad("la tupla (joblib) espera confirmación", pendientes == ["tup"], str(pendientes))
        e2.cargar_de(td, confiar=True, solo=pendientes)
        m.verdad("A y a distintas", np.array_equal(e2.ns["A"], e.ns["A"]) and np.array_equal(e2.ns["a"], e.ns["a"]))
        m.verdad("complejos intactos", np.array_equal(e2.ns["c"], e.ns["c"]))
        m.verdad("matriz 3D intacta", np.array_equal(e2.ns["T"], e.ns["T"]) and e2.ns["T"].ndim == 3)
        m.verdad("los ndarray siguen siendo ndarray", isinstance(e2.ns["a"], np.ndarray))
        m.verdad("tabla", e2.ns["df"].equals(e.ns["df"]))
        m.verdad("texto, diccionario y tupla", e2.ns["s"] == "hola" and e2.ns["d"] == {"k": [1, 2]} and e2.ns["tup"] == (1, 2))
        m.verdad("«stats» recuperada como valor del usuario", e2.ns["stats"] == 5)
    with tempfile.TemporaryDirectory() as td:
        _escribir(td, "viejo.csv", "t;pot\n0;1,5\n1;2,5\n", "cp1252")
        pd.DataFrame({"x": [1, 2]}).to_parquet(os.path.join(td, "tabla.parquet"))
        e3 = Espacio()
        m.verdad("carpeta antigua sin índice", e3.cargar_de(td) == ([], []) and set(e3.ns) >= {"viejo", "tabla"})
        m.error("coma decimal en carpeta antigua", e3.ns["viejo"]["pot"].iloc[0], 1.5, 0.0)


@prueba(1, "Gráficas 3D con formas incoherentes", "ValueError claro en el motor; línea 3D reducida")
def graficas_3d(m):
    """`surf` con formas distintas falla con un mensaje claro y `plot3` reduce los puntos como la gráfica 2D."""
    X, Y = np.meshgrid(np.linspace(0, 1, 5), np.linspace(0, 1, 4))
    e = _error(figura3d, "t", X, Y, np.zeros((6, 6)))
    m.verdad("Z con otra forma", isinstance(e, ValueError) and "misma forma" in str(e), str(e)[:90])
    e = _error(figura3d, "t", np.arange(5.0), np.arange(4.0), np.zeros((5, 4)))
    m.verdad("vectores X e Y con Z traspuesta", isinstance(e, ValueError))
    ok = figura3d("t", np.arange(5.0), np.arange(4.0), np.zeros((4, 5)))
    m.verdad("vectores X e Y con Z de len(Y) × len(X)", len(ok["z"]) == 4 and len(ok["z"][0]) == 5)
    n = 100000
    t = np.linspace(0, 1, n)
    f = figura3d("l", t, t, t, tipo="linea3d")
    m.verdad("plot3 con 100000 puntos se reduce a 3000 o menos", len(f["x"]) <= 3000 and len(f["x"]) == len(f["y"]) == len(f["z"]), str(len(f["x"])))
    m.verdad("plot3 conserva el último punto", f["x"][-1] == 1.0)
    e = _error(figura3d, "l", t, t[:10], t, tipo="linea3d")
    m.verdad("plot3 con longitudes distintas", isinstance(e, ValueError))


@prueba(1, "Recarga de librerías atómica", "Un error al importar deja intacto el catálogo anterior")
def recarga_atomica(m):
    """Si una librería falla al importar, el registro de bloques queda como estaba; un .py con sintaxis errónea no se guarda."""
    from ..servidor import Motor
    from ..sim import nucleo
    dg.cargar_librerias()
    motor = Motor()
    antes = dict(nucleo.REGISTRO)
    real = importlib.reload
    llamadas = []

    def reload_roto(modulo):
        llamadas.append(modulo.__name__)
        if len(llamadas) == 6:
            raise SyntaxError("error simulado en la librería")
        return real(modulo)

    with mock.patch("importlib.reload", reload_roto):
        e = _error(motor.recargar_librerias)
    m.verdad("la recarga informa el error", isinstance(e, SyntaxError))
    m.verdad("el registro conserva todos los bloques", len(nucleo.REGISTRO) == len(antes) and nucleo.REGISTRO.keys() == antes.keys(),
             f"{len(nucleo.REGISTRO)} de {len(antes)}")
    m.verdad("las clases siguen siendo las originales", all(nucleo.REGISTRO[k] is antes[k] for k in antes))
    d = diagrama("x", [bloque("c", "constante", c=2.0), bloque("g", "ganancia", k=3.0), bloque("o", "osciloscopio")],
                 [hilo("c.y", "g.u"), hilo("g.y", "o.u1")], t_fin=0.1, metodo="rk4", paso=0.05)
    m.error("tras el fallo se puede simular", simular(d)["senales"][0]["trazas"][0][-1], 6.0, 1e-12)
    with tempfile.TemporaryDirectory() as td:
        ruta = os.path.join(td, "bloque.py")
        _escribir(td, "bloque.py", "x = 1\n")
        e = _error(motor.escribir_archivo, ruta, "def f(:\n    pass\n")
        m.verdad("un .py con error de sintaxis se rechaza", isinstance(e, ValueError) and "sintaxis" in str(e), str(e))
        m.verdad("y el archivo anterior no se toca", open(ruta, encoding="utf8").read() == "x = 1\n")
        m.verdad("uno válido se escribe", motor.escribir_archivo(ruta, "x = 2\n") is True and open(ruta, encoding="utf8").read() == "x = 2\n")


@prueba(1, "Consola: entrada, salida y errores", "input(), exit() y errores de sintaxis no rompen el motor")
def consola_robusta(m):
    """`input()` y `exit()` no cuelgan ni matan el motor; los errores de sintaxis no traen frames internos; la salida se trunca."""
    e = Espacio()
    r = e.evaluar("input()")
    m.verdad("input() devuelve un error en lugar de colgarse", r["error"] and "EOFError" in r["error"], str(r["error"])[:80])
    r = e.evaluar("exit()")
    m.verdad("exit() se captura con un mensaje claro", r["error"] and "exit()" in r["error"], str(r["error"])[:80])
    r = e.evaluar("import sys; sys.exit(3)")
    m.verdad("sys.exit() también", r["error"] and "sigue abierta" in r["error"])
    r = e.evaluar("x = (1,")
    m.verdad("error de sintaxis sin frames internos", r["error"] and "Traceback" not in r["error"] and "servidor" not in r["error"]
             and "espacio.py" not in r["error"] and "<consola>" in r["error"], str(r["error"])[:120])
    r = e.evaluar("\ufeffy = 4")
    m.verdad("el BOM inicial se descarta", r["error"] is None and e.ns["y"] == 4, str(r["error"]))
    r = e.ejecutar_script("nombre = __name__\narchivo = __file__", "C:/scripts/a.py")
    m.verdad("un script corre como __main__ con su __file__", e.ns["nombre"] == "__main__" and e.ns["archivo"] == "C:/scripts/a.py", str(r["error"]))
    m.verdad("y después el espacio recupera su nombre", e.ns["__name__"] == "__consola__" and "__file__" not in e.ns)
    r = e.evaluar("class R:\n    def __repr__(self):\n        raise RuntimeError('no')\n    def __len__(self):\n        raise RuntimeError('no')\nr = R()\nz = 1")
    m.verdad("un objeto con __repr__ roto no impide listar las variables", r["error"] is None and {"r", "z"} <= {v["nombre"] for v in r["variables"]})
    r = e.evaluar("print('a' * 500000)")
    m.verdad("la salida se trunca con aviso", len(r["salida"]) < 210000 and "truncada" in r["salida"], str(len(r["salida"])))


@prueba(1, "Importación de datos con el lector común", "Nombres válidos, separadores y fechas")
def importacion(m):
    """`cargar` lee CSV regionales y sanea el nombre; los bloques de tabla validan columnas, tiempo y fechas."""
    dg.cargar_librerias()
    e = Espacio()
    with tempfile.TemporaryDirectory() as td:
        ruta = _escribir(td, "d.csv", "t;pot\n0;1,5\n1;2,5\n", "cp1252")
        e.cargar(ruta, "plot")
        m.verdad("«plot» no pisa la función base", callable(e.ns["plot"]) and "plot_1" in e.ns)
        e.cargar(ruta, "2024 datos")
        m.verdad("nombre con espacios y dígito inicial", "v_2024_datos" in e.ns)
        m.error("coma decimal leída", e.ns["plot_1"]["pot"].iloc[0], 1.5, 0.0)
        csv = _escribir(td, "f.csv", "fecha;valor\n01/02/2024 00:00;1,5\n01/02/2024 00:10;2,5\n02/02/2024 00:00;3,5\n", "cp1252")
        b = REGISTRO["desde_csv"]({"ruta": csv}, "c", "CSV", {})
        m.error("fechas a segundos desde la primera fila", b.tt[1], 600.0, 0.0)
        m.error("interpolación en t = 300 s", b.salida(300.0, None, None)[0], 2.0, 1e-12)
        malo = _escribir(td, "m.csv", "t,y\n0,1\n2,5\n1,3\n")
        e1 = _error(REGISTRO["desde_csv"], {"ruta": malo}, "c", "Mi CSV", {})
        m.verdad("tiempo no creciente", isinstance(e1, ValueError) and "creciente" in str(e1) and "Mi CSV" in str(e1), str(e1))
        e2 = _error(REGISTRO["desde_csv"], {"ruta": malo, "col_y": "zz"}, "c", "Mi CSV", {})
        m.verdad("columna inexistente con las disponibles", isinstance(e2, ValueError) and "t, y" in str(e2), str(e2))
        nan = _escribir(td, "n.csv", "t,y\n0,1\n1,\n2,3\n")
        b = REGISTRO["desde_csv"]({"ruta": nan}, "c", "NaN", {})
        m.verdad("avisa de las filas con NaN y las descarta", len(b.tt) == 2 and any("descartaron 1" in a for a in b.avisos_preparacion), str(b.avisos_preparacion))
    df = pd.DataFrame({"hora": [0.0, 1.0, 2.0], "a": [1.0, 2.0, 3.0], "b": [10.0, 20.0, 30.0]})
    b = REGISTRO["desde_espacio"]({"variable": "df", "col_y": "b"}, "e", "Esp", {"df": df})
    m.error("columna de valor elegida", b.salida(1.5, None, None)[0], 25.0, 1e-12)
    b = REGISTRO["desde_espacio"]({"variable": "mat"}, "e", "Esp", {"mat": np.array([[0.0, 5.0], [2.0, 7.0]])})
    m.error("matriz con las columnas 0 y 1 por defecto", b.salida(1.0, None, None)[0], 6.0, 1e-12)
    e3 = _error(REGISTRO["desde_espacio"], {"variable": "nada"}, "e", "Esp", {"df": df})
    m.verdad("variable inexistente: lo dice y lista las tablas", isinstance(e3, ValueError) and "«nada»" in str(e3) and "df" in str(e3), str(e3))
    m.verdad("las columnas son del tipo que la interfaz convierte en desplegable",
             [p.tipo for p in REGISTRO["desde_espacio"].parametros] == ["variable", "columna", "columna"])


@prueba(1, "Errores de bloque con su nombre y validaciones", "Mensajes «Bloque «nombre»: …»")
def errores_con_nombre(m):
    """Los errores de parámetros y de preparación nombran el bloque; campos vacíos y límites incoherentes se rechazan."""
    dg.cargar_librerias()

    def crear(tipo, **p):
        return _error(REGISTRO[tipo], p, "id", "Mi bloque", {})

    for descr, tipo, p, frase in (
        ("campo vacío", "ganancia", {"k": ""}, "falta valor"),
        ("ganancia no finita", "ganancia", {"k": "inf"}, "finita"),
        ("saturación con inf > sup", "saturacion", {"inf": 2, "sup": 1}, "mayor"),
        ("integrador con límites invertidos", "integrador", {"lim_inf": 1, "lim_sup": -1}, "mayor"),
        ("retardo con τ ≤ 0", "retardo", {"tau": 0}, "positivo"),
        ("mux con n no entero", "mux", {"n": 2.5}, "entero"),
        ("mux con n = 0", "mux", {"n": 0}, "entero"),
        ("función de transferencia con denominador nulo", "funcion_transferencia", {"den": "[0, 0]"}, "nulo"),
        ("osciloscopio con n = 0", "osciloscopio", {"n": 0}, "entero"),
        ("expresión con variable inexistente", "ganancia", {"k": "inexistente*2"}, "inexistente"),
    ):
        e = crear(tipo, **p)
        m.verdad(descr, isinstance(e, ValueError) and "Bloque «Mi bloque»" in str(e) and frase in str(e), str(e)[:100])
    b = REGISTRO["ganancia"]({"k": "0,5"}, "g", "G", {})
    m.error("coma decimal en un parámetro de bloque", b.p["k"], 0.5, 0.0)
    m.verdad("un bloque válido se crea", crear("saturacion", inf=-1, sup=1) is None and crear("mux", n=3) is None)
