"""Regresiones de la ronda de correcciones de rigor: ningún resultado inválido se presenta como válido."""
from __future__ import annotations

import json
import math

import numpy as np

from .banco import prueba
from ..sim.circuito import Elemento
from ..sim.diagrama import ErrorDiagrama, bloque, diagrama, hilo, simular
from ..sim.nucleo import P, registrar
from ..sim.solvers import ErrorSolver, integrar


def _registrar_elementos_de_prueba():
    """Registra dos elementos ocultos (tipo con «_») que solo existen para estas pruebas."""

    @registrar
    class _Cubica(Elemento):
        """Nodo a tierra con f(v) = v³ + a·v + c = 0, estampado como compañero de Newton."""
        tipo = "_prueba_cubica"; nombre = "Cúbica de prueba"
        parametros = [P("a", "a", -2.0), P("c", "c", 2.0)]
        puertos_e = ["p"]
        salidas = ["v"]
        no_lineal = True

        def estampar_din(self, M, b, z, t, x, u):
            v, a, c = z[self.n["p"]], float(self.p["a"]), float(self.p["c"])
            G = 3 * v * v + a
            M[self.n["p"], self.n["p"]] += G
            b[self.n["p"]] += G * v - (v ** 3 + a * v + c)

        def salida_e(self, z, t, x, u):
            return [z[self.n["p"]]]

    @registrar
    class _Vaiven(Elemento):
        """Interruptor que cambia de estado en cada intento: nunca alcanza un estado coherente."""
        tipo = "_prueba_vaiven"; nombre = "Vaivén de prueba"
        puertos_e = ["p"]
        con_estado_discreto = True

        def preparar(self):
            self.on = False

        def clave(self):
            return self.on

        def estampar_estado(self, S):
            S.g(self.n["p"], -1, 1.0 if self.on else 2.0)

        def actualizar(self, z, t, x, u):
            self.on = not self.on
            return True


def _cubica(a, c):
    _registrar_elementos_de_prueba()
    return diagrama("cub", [bloque("q", "_prueba_cubica", a=a, c=c), bloque("o", "osciloscopio", n=1)],
                    [hilo("q.v", "o.u1")], metodo="rk4", paso=0.1, t_fin=0.2)


@prueba(1, "Correcciones G1: la red no lineal distingue convergencia de agotamiento",
        "v³ - 2v + 2 = 0 desde v = 0: Newton oscila 0 ↔ 1 (residuo 2); v³ + v - 2 = 0 converge a 1")
def red_no_lineal(m):
    """Una red que no converge lanza un error con instante y residuo; una que converge sigue igual."""
    try:
        simular(_cubica(-2.0, 2.0))
        m.verdad("v³ - 2v + 2 = 0 sin convergencia: error explícito", False)
    except ValueError as e:
        texto = str(e)
        m.verdad(f"v³ - 2v + 2 = 0 sin convergencia: error explícito ({texto[:90]}...)",
                 "no convergió" in texto and "residuo" in texto and "t = " in texto)
    s = simular(_cubica(1.0, -2.0))["senales"][0]
    m.error("v³ + v - 2 = 0 converge a la raíz v = 1", s["trazas"][0][-1], 1.0, 1e-9, "V")
    d = diagrama("vai", [bloque("s", "_prueba_vaiven")], [], metodo="rk4", paso=0.1, t_fin=0.2)
    try:
        simular(d)
        m.verdad("Interruptores sin estado coherente: error explícito", False)
    except ValueError as e:
        m.verdad("Interruptores sin estado coherente: error explícito", "estado coherente" in str(e))


@prueba(1, "Correcciones G3: el paso fijo valida su entrada y no devuelve valores no finitos",
        "Euler con derivada infinita → error en t = 1; paso negativo → error; dx/dt = 1 → x(1) = 1")
def paso_fijo(m):
    """El estado se comprueba después de cada paso, incluido el último; el paso debe ser positivo."""
    try:
        integrar(lambda t, x: np.array([np.inf]), (0, 1), [0.0], metodo="euler", paso=1.0)
        m.verdad("Derivada infinita: error con el instante", False)
    except ErrorSolver as e:
        m.verdad(f"Derivada infinita: error con el instante ({e})", "t=1" in str(e))
    for paso in (-0.1, 0.0, float("nan")):
        try:
            integrar(lambda t, x: np.ones(1), (0, 1), [0.0], metodo="euler", paso=paso)
            m.verdad(f"Paso {paso}: rechazado", False)
        except ValueError as e:
            m.verdad(f"Paso {paso}: rechazado ({e})", "paso" in str(e))
    try:
        integrar(lambda t, x: np.ones(1), (1, 0), [0.0], metodo="rk4", paso=0.1)
        m.verdad("Intervalo invertido: rechazado", False)
    except ValueError:
        m.verdad("Intervalo invertido: rechazado", True)
    t, x = integrar(lambda t, x: np.ones(1), (0, 1), [0.0], metodo="euler", paso=0.1)
    m.error("dx/dt = 1 con paso 0.1: x(1) = 1", x[-1, 0], 1.0, 1e-12)
    m.error("dx/dt = 1 con paso 0.1: último instante t = 1", t[-1], 1.0, 0.0, "s")
    d = diagrama("neg", [bloque("c", "constante"), bloque("i", "integrador"), bloque("o", "osciloscopio", n=1)],
                 [hilo("c.y", "i.u"), hilo("i.y", "o.u1")], metodo="rk4", paso=-0.1, t_fin=1.0)
    try:
        simular(d)
        m.verdad("Diagrama con paso negativo: rechazado", False)
    except ValueError as e:
        m.verdad("Diagrama con paso negativo: rechazado", "paso" in str(e))


@prueba(1, "Correcciones G2: la linealización rechaza el tiempo discreto",
        "constante → retardo_unitario (Ts = 0.1) → osciloscopio: error que nombra Ts y el bloque")
def linealizar_discreto(m):
    """Un diagrama con estado muestreado no se linealiza como si fuera continuo."""
    from ..sim.linealizar import linealizar
    d = diagrama("z", [bloque("c", "constante", c=1.0), bloque("z", "retardo_unitario", nombre="Retardo z", Ts=0.1),
                       bloque("o", "osciloscopio", n=1)],
                 [hilo("c.y", "z.u"), hilo("z.y", "o.u1")], metodo="dopri", t_fin=1.0)
    try:
        linealizar(d)
        m.verdad("Linealizar un diagrama discreto: error explícito", False)
    except ErrorDiagrama as e:
        m.verdad(f"Linealizar un diagrama discreto: error explícito ({str(e)[:80]}...)",
                 "Ts = 0.1" in str(e) and "Retardo z" in str(e))
    d = diagrama("c", [bloque("e", "escalon", t0=0.0), bloque("g", "funcion_transferencia", num=[2.0], den=[1.0, 1.0]),
                       bloque("o", "osciloscopio", n=1)],
                 [hilo("e.y", "g.u"), hilo("g.y", "o.u1")], metodo="dopri", t_fin=1.0)
    lin = linealizar(d)
    m.error("Un diagrama continuo se sigue linealizando: A = -1", lin["A"][0, 0], -1.0, 1e-6)


@prueba(1, "Correcciones I3: resultados con NaN o infinitos se serializan", "NaN, ±inf → null, con sus rutas anotadas")
def serializacion(m):
    """Una matriz NumPy con valores no finitos llega como null y se distingue de un valor ausente."""
    from ..servidor import _conv, respuesta_ok
    res = {"A": np.array([[1.0, np.nan], [np.inf, 2.0]]), "k": np.float64(-np.inf), "n": np.int64(3), "ausente": None}
    r = respuesta_ok(7, res)
    try:
        texto = json.dumps(r, ensure_ascii=False, allow_nan=False, default=_conv)
        m.verdad("Se serializa sin ValueError con allow_nan=False", True)
    except ValueError:
        m.verdad("Se serializa sin ValueError con allow_nan=False", False)
        return
    v = json.loads(texto)
    m.verdad("Matriz con null donde había NaN e infinito", v["resultado"]["A"] == [[1.0, None], [None, 2.0]])
    m.verdad("Escalares NumPy convertidos (−inf → null, int64 → 3)", v["resultado"]["k"] is None and v["resultado"]["n"] == 3)
    m.verdad(f"Rutas de los no finitos anotadas ({v.get('no_finitos')})", v.get("no_finitos") == ["A[0][1]", "A[1][0]", "k"])


def _con_ganancia(valor="ganancia"):
    return diagrama("g", [bloque("c", "constante", c=valor), bloque("o", "osciloscopio", n=1)],
                    [hilo("c.y", "o.u1")], metodo="rk4", paso=0.1, t_fin=0.2)


@prueba(1, "Correcciones I4: la procedencia distingue el valor de las variables",
        "Mismo diagrama, variable distinta → huellas resueltas distintas")
def procedencia(m):
    """El sha256 del diagrama no cambia; el de los parámetros resueltos sí; el del código no."""
    from ..servidor import Motor
    mo = Motor()
    mo.espacio.ns["ganancia"] = 2.0
    p1 = mo.simular(_con_ganancia())["procedencia"]
    mo.espacio.ns["ganancia"] = 3.0
    p2 = mo.simular(_con_ganancia())["procedencia"]
    m.verdad("Mismo diagrama: misma huella de entrada", p1["sha256"] == p2["sha256"])
    m.verdad("Variable distinta: huella de parámetros resueltos distinta", p1["sha256_resuelto"] != p2["sha256_resuelto"])
    m.verdad("Mismo código: misma huella de código", p1["sha256_codigo"] == p2["sha256_codigo"])
    m.verdad(f"Lista los archivos de código ({len(p1['archivos_codigo'])})",
             "sim/bloques_genericos.py" in p1["archivos_codigo"] and "sim/circuito.py" in p1["archivos_codigo"])


@prueba(1, "Correcciones I5: estadísticas temporales del barrido", "y = t en [0, 1]: media en el tiempo = 0.5, RMS = 1/√3")
def estadisticas(m):
    """Con paso variable la media muestral depende de los pasos; la temporal no."""
    from ..servidor import _barrido_corrida
    # El polo rápido de 1/(0.01·s + 1) obliga a pasos cortos al principio: las muestras se agolpan cerca de t = 0
    d = diagrama("r", [bloque("r", "rampa", m=1.0), bloque("e", "escalon", t0=0.0),
                       bloque("g", "funcion_transferencia", num=[1.0], den=[0.01, 1.0]), bloque("o", "osciloscopio", n=2)],
                 [hilo("r.y", "o.u1"), hilo("e.y", "g.u"), hilo("g.y", "o.u2")], metodo="dopri", t_fin=1.0)
    s = _barrido_corrida(d, {}, {}, ["Rampa"])["senales"]
    s = next(v for k, v in s.items() if "Rampa" in k)
    m.error("Media en el tiempo", s["media_t"], 0.5, 1e-12)
    m.error("RMS en el tiempo (trapecios sobre y = t: √(1/3 + Σh²/6))", s["rms_t"], math.sqrt(1 / 3), 2e-3)
    m.verdad(f"La media muestral se conserva aparte ({s['media']:.3f})", "media" in s and abs(s["media"] - 0.5) > 0.05)


@prueba(1, "Correcciones I6: una simulación reenviada no usa un espacio perdido",
        "Tras un reinicio, c = «ganancia» se rechaza; un diagrama autosuficiente se repite")
def reenvio(m):
    """`variables_del_espacio` detecta las variables usadas; `simular(_reenviado=True)` las exige."""
    from ..servidor import Motor, variables_del_espacio
    m.verdad("Detecta «ganancia»", variables_del_espacio(_con_ganancia()) == ["ganancia"])
    m.verdad("«2*pi» no usa el espacio", variables_del_espacio(_con_ganancia("2*pi")) == [])
    tn = diagrama("t", [bloque("t", "lookup_nd", n=2, puntos="[[0, 1], [0, 1]]",
                                tabla="[[a + b for b in (0, 1)] for a in (0, 1)]")], [])
    m.verdad("Las variables de una comprensión no cuentan", variables_del_espacio(tn) == [])
    mo = Motor()
    try:
        mo.simular(_con_ganancia(), _reenviado=True)
        m.verdad("Reenviada y dependiente del espacio: rechazada", False)
    except ValueError as e:
        m.verdad("Reenviada y dependiente del espacio: rechazada", "ganancia" in str(e) and "reinició" in str(e))
    r = mo.simular(_con_ganancia("1.5"), _reenviado=True)
    m.verdad("Reenviada y autosuficiente: se simula", bool(r["figuras"]))
