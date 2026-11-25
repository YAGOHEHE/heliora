"""Diagramas de ejemplo que se ofrecen en la aplicación (se guardan como JSON)."""
from __future__ import annotations

import json
import os

from ..sim.diagrama import bloque, diagrama, hilo
from .pruebas_fase2 import ordenar_planta, planta_promediada


def rc_serie():
    """Devuelve el diagrama del circuito RC serie con escalón de tensión."""
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


def pid_discreto():
    """PID discreto a 10 kHz con medida ruidosa muestreada y consigna con limitador de tasa."""
    return diagrama("Control PID discreto a 10 kHz", [
        bloque("r", "escalon", 40, 80, "Consigna", t0=0.02, y0=0.0, y1=1.0),
        bloque("lim", "limitador_tasa_discreto", 200, 80, "Limitador de tasa", Ts=1e-4, R_alta=50.0, R_baja=-50.0),
        bloque("pid", "pid_discreto", 380, 90, "PID discreto", Ts=1e-4, Kp=2.0, Ki=40.0, Kd=0.02, N=500.0, c=0.0,
               u_min=-5.0, u_max=5.0),
        bloque("g", "funcion_transferencia", 560, 90, "Planta 1/(0.05 s + 1)", num="[1]", den="[0.05, 1]"),
        bloque("n", "ruido", 560, 220, "Ruido de medida", Ts=1e-4, PSD=1e-8, semilla=1),
        bloque("s", "suma", 720, 200, "Medida", signos="++"),
        bloque("mu", "muestreo", 880, 200, "Muestreo 10 kHz", Ts=1e-4),
        bloque("o", "osciloscopio", 880, 60, "Consigna, salida y mando", n=3, cada=10),
    ], [hilo("r.y", "lim.u"), hilo("lim.y", "pid.r"), hilo("mu.y", "pid.y"), hilo("pid.u", "g.u"),
        hilo("g.y", "s.u1"), hilo("n.y", "s.u2"), hilo("s.y", "mu.u"),
        hilo("lim.y", "o.u1"), hilo("g.y", "o.u2"), hilo("pid.u", "o.u3")], metodo="dopri", t_fin=0.3, rtol=1e-4, atol=1e-6)


def troceador_termico():
    """Troceador reductor con IGBT y diodo acoplados a un disipador: pérdidas → Tj → V_ce0, R_on."""
    return diagrama("Troceador con modelo electro-térmico", [
        bloque("v", "fuente_dc", 40, 80, "Bus de 300 V", V=300.0),
        bloque("q", "igbt", 220, 60, "IGBT"),
        bloque("dl", "diodo", 380, 200, "Diodo libre", Ron=2e-3, Vf=0.9),
        bloque("L", "inductor", 420, 60, "L 2 mH", L=2e-3, i0=150.0),
        bloque("R", "resistor", 600, 60, "Carga 1 Ω", R=1.0),
        bloque("g", "tierra", 380, 320, "Tierra"),
        bloque("d", "constante", 40, 220, "Ciclo de trabajo", c=0.5),
        bloque("pwm", "pwm", 200, 220, "PWM 5 kHz", fc=5000.0, modo="bajada"),
        bloque("rcq", "resistencia_termica", 220, 420, "R_th cápsula-disipador IGBT", R=0.02),
        bloque("rcd", "resistencia_termica", 380, 420, "R_th cápsula-disipador diodo", R=0.02),
        bloque("dis", "capacidad_termica", 560, 420, "Disipador", C=50.0, T0=40.0),
        bloque("rha", "resistencia_termica", 720, 420, "R_th disipador-ambiente", R=0.3),
        bloque("amb", "ambiente", 880, 420, "Ambiente 40 °C", T_amb=40.0),
        bloque("o", "osciloscopio", 880, 200, "Temperaturas de unión y disipador", n=3, cada=20),
    ], [hilo("v.p", "q.c"), hilo("q.e", "L.p"), hilo("L.n", "R.p"), hilo("R.n", "g.t"), hilo("v.n", "g.t"),
        hilo("dl.p", "g.t"), hilo("dl.n", "q.e"), hilo("d.y", "pwm.d"), hilo("pwm.g", "q.g"),
        hilo("q.th", "rcq.p"), hilo("dl.th", "rcd.p"), hilo("rcq.n", "dis.p"), hilo("rcd.n", "dis.p"),
        hilo("dis.p", "rha.p"), hilo("rha.n", "amb.p"),
        hilo("q.Tj", "o.u1"), hilo("dl.Tj", "o.u2"), hilo("dis.T", "o.u3")], metodo="rk4", t_fin=0.05, paso=2e-6)


def planta_con_bateria():
    """La planta FV promediada con una batería de 360 V / 50 Ah cargada desde el bus DC a 20 A."""
    d = planta_promediada()
    d["nombre"] = "Planta FV con batería en el bus DC"
    d["sistema"]["bloques"] += [
        bloque("carg", "cargador_dcdc", 640, 560, "Cargador bidireccional", modo="corriente", L=2e-3, R_L=0.02,
               eta=0.97, f_sw=20000.0, i_max=60.0, i_min=-60.0),
        bloque("iref", "constante", 480, 640, "Corriente de carga", c=20.0),
        bloque("bat", "bateria", 820, 600, "Batería 100s10p", Q=5.0, Ns=100, Np=10, SoC0=0.5),
        bloque("osc4", "osciloscopio", 1320, 480, "Batería: corriente, SoC y potencia", n=3),
    ]
    d["sistema"]["conexiones"] += [
        hilo("cdc.p", "carg.alto"), hilo("carg.com", "tierra.t"), hilo("carg.bajo", "bat.p"), hilo("bat.n", "tierra.t"),
        hilo("iref.y", "carg.ref"), hilo("carg.i_L", "osc4.u1"), hilo("bat.SoC", "osc4.u2"), hilo("carg.P_bajo", "osc4.u3"),
    ]
    return d


def arranque_induccion():
    """Arranque directo de un motor de inducción que mueve un ventilador a través de un reductor.

    Un engranaje ideal une rígidamente los dos ejes: solo puede haber una inercia. La del rotor
    (0.02 kg·m²) va referida al eje lento, J = 0.3 + n²·0.02 = 0.38 kg·m², y el motor lleva J = 0.
    """
    return diagrama("Arranque de un motor de inducción con ventilador", [
        bloque("red", "fuente_ac3", 40, 80, "Red 400 V, 50 Hz", VLL=400.0, f=50.0),
        bloque("med", "medidor3", 220, 80, "Medidor"),
        bloque("mq", "maquina_induccion", 400, 80, "Motor de inducción 4 polos", J=0.0),
        bloque("red_eng", "engranaje", 580, 80, "Reductor 2:1", n=2.0),
        bloque("jv", "inercia", 760, 60, "Inercia del ventilador y del rotor referida", J=0.38),
        bloque("ven", "par_carga", 760, 200, "Ventilador T = k·ω²", T0=0.5, k=0.006),
        bloque("sv", "sensor_velocidad", 760, 320, "Velocidad del ventilador"),
        bloque("o", "osciloscopio", 960, 80, "Velocidades y par", n=3),
        bloque("oi", "osciloscopio", 960, 260, "Corriente de línea", n=1, cada=2),
    ], [hilo("red.a", "med.a"), hilo("med.b", "mq.abc"), hilo("mq.eje", "red_eng.p1"), hilo("red_eng.p2", "jv.p"),
        hilo("jv.p", "ven.p"), hilo("jv.p", "sv.p"),
        hilo("mq.w", "o.u1"), hilo("sv.w", "o.u2"), hilo("mq.Te", "o.u3"), hilo("med.i_abc", "oi.u1")],
        metodo="dopri", t_fin=2.0, rtol=1e-5, atol=1e-7)


CATALOGO_EJEMPLOS = {
    "planta_promediada": ("Planta FV completa (promediada)", planta_promediada),
    "planta_conmutada": ("Planta FV completa (conmutada)", planta_conmutada),
    "sombreado": ("Sombreado parcial con MPPT", sombreado),
    "rc": ("Circuito RC: respuesta al escalón", rc_serie),
    "lazo": ("Lazo de control con subsistema", lazo_control),
    "pid_discreto": ("Control PID discreto a 10 kHz", pid_discreto),
    "troceador_termico": ("Troceador con modelo electro-térmico", troceador_termico),
    "planta_con_bateria": ("Planta FV con batería en el bus DC", planta_con_bateria),
    "arranque_induccion": ("Arranque de un motor de inducción con ventilador", arranque_induccion),
}


def obtener(clave):
    """Devuelve el diagrama del ejemplo `clave` del catálogo."""
    return CATALOGO_EJEMPLOS[clave][1]()


def listar():
    """Devuelve la lista de ejemplos del catálogo como dicts con `clave` y `nombre`."""
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
    """Escribe cada ejemplo del catálogo como `<clave>.json` canónico en `carpeta`, que crea si no existe."""
    os.makedirs(carpeta, exist_ok=True)
    for k in CATALOGO_EJEMPLOS:
        with open(os.path.join(carpeta, k + ".json"), "w", encoding="utf8", newline="\n") as f:
            f.write(json_canonico(obtener(k)))
