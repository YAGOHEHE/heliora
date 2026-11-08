"""Diagramas de ejemplo que se ofrecen en la aplicación (se guardan como JSON)."""
from __future__ import annotations

import json
import os

from ..sim.diagrama import bloque, diagrama, hilo
from .pruebas_fase2 import ordenar_planta, planta_promediada


def rc_serie():
    return diagrama("Circuito RC: respuesta al escalón", [
        bloque("v", "fuente_dc", 80, 80, "Fuente 10 V", V=10.0),
        bloque("r", "resistor", 280, 60, "R 1 kΩ", R=1000.0),
        bloque("c", "capacitor", 480, 120, "C 1 µF", C=1e-6),
        bloque("t", "tierra", 280, 240, "Tierra"),
        bloque("vm", "voltimetro", 660, 60, "v_C"),
        bloque("o", "osciloscopio", 860, 80, "Tensión del condensador", n=1),
    ], [hilo("v.p", "r.p"), hilo("r.n", "c.p"), hilo("c.n", "t.t"), hilo("v.n", "t.t"), hilo("c.p", "vm.p"), hilo("c.n", "vm.n"),
        hilo("vm.v", "o.u1")], metodo="dopri", t_fin=0.005, rtol=1e-4, atol=1e-6)


def lazo_control():
    """Un lazo con subsistema: planta de segundo orden con PI, con saturación y escalón."""
    planta = {"bloques": [
        bloque("in", "entrada", 40, 80, "u", numero=1),
        bloque("g", "funcion_transferencia", 200, 60, "Planta 1/(s²+2s+1)", num="[1]", den="[1, 2, 1]"),
        bloque("out", "salida", 420, 80, "y", numero=1),
    ], "conexiones": [hilo("in.y", "g.u"), hilo("g.y", "out.u")]}
    return diagrama("Lazo de control con subsistema", [
        bloque("r", "escalon", 40, 100, "Referencia", t0=1.0, y0=0.0, y1=1.0),
        bloque("s", "suma", 220, 100, "Error", signos="+-"),
        bloque("pi", "pi", 380, 90, "PI", Kp=2.0, Ki=1.0, y_min=-5, y_max=5),
        bloque("planta", "subsistema", 580, 90, "Planta", sistema=planta),
        bloque("o", "osciloscopio", 800, 90, "Referencia y salida", n=2),
        bloque("exp", "a_espacio", 800, 220, "Exportar", variable="salida_lazo"),
    ], [hilo("r.y", "s.u1"), hilo("s.y", "pi.e"), hilo("pi.y", "planta.in1"), hilo("planta.out1", "s.u2"),
        hilo("r.y", "o.u1"), hilo("planta.out1", "o.u2"), hilo("planta.out1", "exp.u")], metodo="dopri", t_fin=12.0, rtol=1e-4, atol=1e-6)


def _comun_arreglo(bloques, con):
    bloques += [
        bloque("G", "escalon", 40, 40, "Irradiancia", t0=0.05, y0=1000.0, y1=800.0),
        bloque("T", "constante", 40, 130, "Temperatura de celda", c=40.0),
        bloque("pv", "arreglo_fv", 200, 70, "Arreglo FV", modulo="CS1H-330MS (330 W, 66 celdas)", Ns=12, Np=20),
        bloque("cpv", "capacitor", 340, 170, "C entrada", C=2e-3, v0=420.0),
        bloque("tierra", "tierra", 480, 240, "Tierra"),
    ]
    con += [hilo("G.y", "pv.G"), hilo("T.y", "pv.T"), hilo("pv.p", "cpv.p"), hilo("pv.n", "tierra.t"), hilo("cpv.n", "tierra.t")]


def planta_conmutada():
    """El mismo ejemplo con el boost y el inversor conmutados (simulación corta)."""
    b, c = [], []
    _comun_arreglo(b, c)
    b += [
        bloque("mppt", "mppt", 360, 300, "MPPT", metodo="Perturbar y observar", Ts=5e-3, dd=0.002, d0=0.44),
        bloque("pwm", "pwm", 480, 300, "PWM boost", fc=20000.0),
        bloque("boost", "boost_conm", 480, 70, "Boost conmutado", L=1e-3, R=0.01, Ron=0.01, Rd=0.01, Vf=0.8, i0=170.0),
        bloque("cdc", "capacitor", 640, 170, "Bus DC", C=8e-3, v0=750.0),
        bloque("vb", "voltimetro", 620, 290, "V bus"),
        bloque("inv", "inversor_conm", 780, 70, "Inversor conmutado"),
        bloque("lcl", "filtro_lcl", 960, 70, "Filtro LCL", L1=0.5e-3, R1=0.01, L2=0.1e-3, R2=0.005, C=50e-6, Rd=0.5),
        bloque("red", "red_infinita", 1140, 70, "Red", VLL=400.0, R=0.01, L=50e-6),
        bloque("pll", "srf_pll", 1140, 240, "PLL", f0=50.0),
        bloque("bus", "control_bus", 620, 390, "Control bus DC", Vref=750.0, Kp=2.0, Ki=25.0, Imax=250.0, Imin=-30.0, I0=115.0),
        bloque("iq", "constante", 620, 480, "i_q*", c=0.0),
        bloque("ctrl", "control_dq", 840, 400, "Control dq", Kp=1.9, Ki=100.0, L=0.65e-3),
        bloque("mod", "svpwm", 840, 260, "SVPWM", fc=5000.0),
        bloque("pq", "potencia_pq", 1140, 400, "P, Q, FP", f0=50.0),
        bloque("osc1", "osciloscopio", 1320, 60, "Potencias", n=3, cada=5),
        bloque("osc2", "osciloscopio", 1320, 200, "Bus y entrada", n=2, cada=5),
        bloque("osc3", "osciloscopio", 1320, 340, "Corriente de red", n=1, cada=2),
    ]
    c += [
        hilo("cpv.p", "boost.in"), hilo("boost.com", "tierra.t"), hilo("boost.out", "cdc.p"), hilo("cdc.n", "tierra.t"),
        hilo("boost.out", "inv.dc_p"), hilo("cdc.n", "inv.dc_n"), hilo("inv.ac", "lcl.a"), hilo("lcl.b", "red.a"),
        hilo("cdc.p", "vb.p"), hilo("cdc.n", "vb.n"), hilo("pv.v", "mppt.v"), hilo("boost.i_L", "mppt.i"),
        hilo("mppt.d", "pwm.d"), hilo("pwm.g", "boost.g"),
        hilo("red.v_pcc", "pll.v_abc"), hilo("vb.v", "bus.vbus"), hilo("bus.id_ref", "ctrl.id_ref"), hilo("iq.y", "ctrl.iq_ref"),
        hilo("lcl.i2_abc", "ctrl.i_abc"), hilo("red.v_pcc", "ctrl.v_abc"), hilo("pll.theta", "ctrl.theta"), hilo("vb.v", "ctrl.vbus"),
        hilo("ctrl.m_abc", "mod.m"), hilo("mod.s", "inv.s"), hilo("red.v_pcc", "pq.v_abc"), hilo("red.i_abc", "pq.i_abc"),
        hilo("pv.pot", "osc1.u1"), hilo("pq.P", "osc1.u2"), hilo("pq.Q", "osc1.u3"),
        hilo("vb.v", "osc2.u1"), hilo("pv.v", "osc2.u2"), hilo("lcl.i2_abc", "osc3.u1"),
    ]
    return ordenar_planta(diagrama("Planta FV completa (conmutada)", b, c, metodo="rk4", t_fin=0.08, paso=2e-6))


def sombreado():
    """Arreglo con sombra parcial y MPPT: la potencia cae a un máximo local."""
    return diagrama("Sombreado parcial con MPPT", [
        bloque("G", "constante", 40, 40, "Irradiancia", c=1000.0),
        bloque("T", "constante", 40, 130, "Temperatura de celda", c=35.0),
        bloque("pv", "arreglo_fv_sombra", 200, 70, "Arreglo con sombra", Ns=12, Np=20, sombra="[1]*24 + [0.4]*12", n_sombra=8),
        bloque("cpv", "capacitor", 380, 170, "C entrada", C=2e-3, v0=420.0),
        bloque("boost", "boost_prom", 500, 70, "Boost", L=1e-3, R=0.01, i0=150.0),
        bloque("bus", "fuente_dc", 680, 90, "Bus de 750 V", V=750.0),
        bloque("tierra", "tierra", 500, 240, "Tierra"),
        bloque("mppt", "mppt", 380, 300, "MPPT", Ts=5e-3, dd=0.002, d0=0.45),
        bloque("o", "osciloscopio", 700, 300, "Potencia y tensión", n=2, cada=5),
    ], [hilo("G.y", "pv.G"), hilo("T.y", "pv.T"), hilo("pv.p", "cpv.p"), hilo("pv.n", "tierra.t"), hilo("cpv.n", "tierra.t"),
        hilo("cpv.p", "boost.in"), hilo("boost.out", "bus.p"), hilo("bus.n", "tierra.t"), hilo("boost.com", "tierra.t"),
        hilo("pv.v", "mppt.v"), hilo("boost.i_L", "mppt.i"), hilo("mppt.d", "boost.d"), hilo("pv.pot", "o.u1"), hilo("pv.v", "o.u2")],
        metodo="rk4", t_fin=0.6, paso=2e-5)


CATALOGO_EJEMPLOS = {
    "planta_promediada": ("Planta FV completa (promediada)", planta_promediada),
    "planta_conmutada": ("Planta FV completa (conmutada)", planta_conmutada),
    "sombreado": ("Sombreado parcial con MPPT", sombreado),
    "rc": ("Circuito RC: respuesta al escalón", rc_serie),
    "lazo": ("Lazo de control con subsistema", lazo_control),
}


def obtener(clave):
    return CATALOGO_EJEMPLOS[clave][1]()


def listar():
    return [{"clave": k, "nombre": v[0]} for k, v in CATALOGO_EJEMPLOS.items()]


def _ordenar(v):
    """Bloques por id y conexiones por extremos en todos los niveles (forma canónica)."""
    if isinstance(v, dict):
        if isinstance(v.get("bloques"), list):
            v["bloques"].sort(key=lambda b: b["id"])
        if isinstance(v.get("conexiones"), list):
            v["conexiones"].sort(key=lambda c: (c["de"][0], c["de"][1], c["a"][0], c["a"][1]))
        for x in v.values():
            _ordenar(x)
    elif isinstance(v, list):
        for x in v:
            _ordenar(x)
    return v


def json_canonico(diagrama):
    """El mismo formato con el que guarda la aplicación: claves ordenadas y sangría de 2 espacios."""
    return json.dumps(_ordenar(json.loads(json.dumps(diagrama))), ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def escribir(carpeta):
    os.makedirs(carpeta, exist_ok=True)
    for k in CATALOGO_EJEMPLOS:
        with open(os.path.join(carpeta, k + ".json"), "w", encoding="utf8", newline="\n") as f:
            f.write(json_canonico(obtener(k)))
