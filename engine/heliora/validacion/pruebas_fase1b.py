"""Fase 1 (continuación): fuentes desde datos, función Python, señales y exportación."""
from __future__ import annotations

import math
import os
import tempfile

import numpy as np
import pandas as pd

from .banco import prueba
from ..sim.diagrama import bloque, diagrama, hilo, simular


@prueba(1, "Fuentes desde datos y bloque de código", "Valores conocidos")
def fuentes_datos(m):
    """Desde CSV, desde el espacio de trabajo, Función Python, combinar/separar y exportar."""
    with tempfile.TemporaryDirectory() as td:
        ruta = os.path.join(td, "datos.csv")
        pd.DataFrame({"t": [0.0, 1.0, 2.0], "v": [0.0, 10.0, 0.0]}).to_csv(ruta, index=False)
        esp = {"serie": np.array([[0.0, 5.0], [2.0, 7.0]]), "ganancia_k": 3.0}
        d = diagrama("datos", [
            bloque("csv", "desde_csv", ruta=ruta), bloque("ws", "desde_espacio", variable="serie"),
            bloque("fn", "funcion_python", n=2, codigo="y = u[0] + 2*u[1] + t"),
            bloque("mux", "mux", n=2), bloque("dem", "demux", n=2), bloque("g", "ganancia", k="ganancia_k*2"),
            bloque("sen", "senoidal", A=2.0, f=0.25, fase=90.0, c=1.0), bloque("pul", "pulso", A=1.0, T=1.0, d=0.5),
            bloque("exp", "a_espacio", variable="salida_prueba"), bloque("o", "osciloscopio", n=6),
        ], [hilo("csv.y", "fn.u1"), hilo("ws.y", "fn.u2"), hilo("csv.y", "mux.u1"), hilo("ws.y", "mux.u2"), hilo("mux.y", "dem.u"), hilo("dem.y2", "g.u"),
            hilo("csv.y", "o.u1"), hilo("ws.y", "o.u2"), hilo("fn.y", "o.u3"), hilo("g.y", "o.u4"), hilo("sen.y", "o.u5"), hilo("pul.y", "o.u6"), hilo("fn.y", "exp.u")],
            metodo="rk4", t_fin=1.5, paso=0.01)
        r = simular(d, esp)
    s = next(x for x in r["senales"] if not x["exportado"])
    t, y = s["t"], s["trazas"]
    m.error("Desde CSV: interpolación en t=1.5", y[0][-1], 5.0, 1e-9)
    m.error("Desde espacio de trabajo en t=1.5", y[1][-1], 6.5, 1e-9)
    m.error("Función Python: u1 + 2·u2 + t", y[2][-1], 5.0 + 13.0 + 1.5, 1e-9)
    m.error("Parámetro con expresión y variable: 6·u", y[3][-1], 6.0 * 6.5, 1e-9)
    m.error("Senoidal con fase y desplazamiento", y[4][-1], 2 * math.sin(2 * math.pi * 0.25 * 1.5 + math.pi / 2) + 1, 1e-9)
    alto = np.mean(y[5][(t > 0.2) & (t < 1.2)])
    m.error("Tren de pulsos: valor medio", alto, 0.5, 0.03)
    v = r["exportados"]["salida_prueba"]
    m.verdad("Exportación al espacio de trabajo", v["t"].size == t.size and abs(v["y"][-1] - y[2][-1]) < 1e-12)


@prueba(1, "Paso variable con discontinuidades", "El solver cae en el escalón")
def eventos(m):
    """Dormand-Prince debe caer justo en el instante del escalón, sin error por la discontinuidad."""
    d = diagrama("ev", [
        bloque("e", "escalon", t0=0.37, y0=0.0, y1=1.0), bloque("i", "integrador"), bloque("o", "osciloscopio", n=1)],
        [hilo("e.y", "i.u"), hilo("i.y", "o.u1")], metodo="dopri", t_fin=1.0, rtol=1e-6, atol=1e-8)
    r = simular(d)
    s = r["senales"][0]
    m.error("Integral tras el escalón en t=1", s["trazas"][0][-1], 0.63, 1e-9)
    m.cota("Error mínimo de t respecto a t0", float(np.min(np.abs(s["t"] - 0.37))), 1e-12, "s")
    m.cota("Pasos rechazados", r["estadisticas"]["rechazos"], 2)
