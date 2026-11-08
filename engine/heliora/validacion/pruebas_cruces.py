"""Pruebas de los cruces por cero, los flancos exactos de PWM y las unidades en los parámetros."""
import math

import numpy as np

from .banco import prueba
from ..lib import electrica, potencia as pot  # noqa: F401
from ..sim.diagrama import bloque, diagrama, hilo, simular
from ..sim.nucleo import ErrorUnidad, evaluar_numero
from ..sim import lazos, solvers


@prueba(1, "Localización de un cruce por cero", "Raíz conocida de una función suave")
def localiza(m):
    """Illinois sobre g(t) = cos(t): la raíz es π/2, hasta el ancho final 128·eps(t)."""
    evals = [0]

    def g(t):
        evals[0] += 1
        return [math.cos(t)]

    te, _ = solvers.localizar_cruce(g, 1.0, g(1.0), 2.0, g(2.0))
    m.error("Instante del cruce de cos(t)", te, math.pi / 2, 1e-12)
    m.verdad(f"Converge sin recorrer el intervalo ({evals[0]} evaluaciones)", evals[0] < 20)
    # Función muy asimétrica: la secante sola se atascaría en un extremo
    te, _ = solvers.localizar_cruce(lambda t: [t**9 - 0.5], 0.0, [-0.5], 1.0, [0.5])
    m.error("Instante del cruce de t⁹ - 0.5", te, 0.5 ** (1 / 9), 1e-12)


@prueba(1, "Switch con paso variable", "Integral exacta por tramos")
def switch(m):
    """∫ de un Switch que pasa de 2 a 5 cuando t cruza 1/π: el error de la esquina no depende de la tolerancia."""
    d = diagrama("sw", [bloque("r", "reloj"), bloque("a", "constante", c=2.0), bloque("b", "constante", c=5.0),
                        bloque("s", "switch", umbral=1 / math.pi), bloque("i", "integrador"),
                        bloque("o", "osciloscopio", n=1)],
                 [hilo("b.y", "s.u1"), hilo("r.t", "s.control"), hilo("a.y", "s.u3"), hilo("s.y", "i.u"), hilo("i.y", "o.u1")],
                 metodo="dopri", t_fin=1.0, rtol=1e-3, atol=1e-6)
    r = simular(d)
    y = r["senales"][0]["trazas"][0][-1]
    exacto = 2.0 / math.pi + 5.0 * (1.0 - 1.0 / math.pi)
    m.error("Integral del Switch", y, exacto, 1e-10)
    m.verdad("Se contó un cruce", r["estadisticas"]["cruces"] == 1)
    m.verdad(f"Pasos ({r['estadisticas']['pasos']}) y rechazos ({r['estadisticas']['rechazos']}): sin rechazos",
             r["estadisticas"]["rechazos"] == 0)


@prueba(1, "Saturación con paso variable", "Integral exacta por tramos")
def saturacion(m):
    """∫ min(t, 0.5) dt en [0, 1] = 0.125 + 0.25."""
    d = diagrama("sat", [bloque("r", "reloj"), bloque("s", "saturacion", inf=-1.0, sup=0.5), bloque("i", "integrador"),
                         bloque("o", "osciloscopio", n=1)],
                 [hilo("r.t", "s.u"), hilo("s.y", "i.u"), hilo("i.y", "o.u1")],
                 metodo="dopri", t_fin=1.0, rtol=1e-3, atol=1e-6)
    r = simular(d)
    m.error("Integral de la saturación", r["senales"][0]["trazas"][0][-1], 0.375, 1e-10)
    m.verdad("Se contó un cruce", r["estadisticas"]["cruces"] == 1)


@prueba(2, "Rectificador con cruces por cero", "Conducción exacta del diodo ideal")
def rectificador(m):
    """Media onda con carga R: el diodo abre y cierra cuando la señal cruza; la media es V/π."""
    V, f = 10.0, 50.0
    d = diagrama("rect", [bloque("v", "fuente_ac", V=V, f=f), bloque("d", "diodo", Ron=1e-6, Vf=0.0),
                          bloque("r", "resistor", R=10.0), bloque("gnd", "tierra"),
                          bloque("s", "amperimetro"), bloque("i", "integrador"), bloque("o", "osciloscopio", n=1)],
                 [hilo("v.p", "d.p"), hilo("d.n", "s.p"), hilo("s.n", "r.p"), hilo("r.n", "gnd.t"), hilo("v.n", "gnd.t"),
                  hilo("s.i", "i.u"), hilo("i.y", "o.u1")],
                 metodo="dopri", t_fin=0.1, rtol=1e-4, atol=1e-7)
    r = simular(d)
    Q = r["senales"][0]["trazas"][0][-1]
    m.error("Corriente media frente a √2·V/(π·R)", Q / 0.1, math.sqrt(2) * V / (math.pi * 10.0), 3e-3, relativo=True)
    m.verdad("Dos cruces por periodo, 5 periodos", 8 <= r["estadisticas"]["cruces"] <= 11)


@prueba(2, "Flancos exactos de PWM", "Ciclo de trabajo independiente del paso")
def flancos(m):
    """∫g dt en N periodos = N·T·D exacto, con el solver cayendo en cada flanco."""
    N, fc, D = 20, 1000.0, 0.3
    for modo in ("centrado", "bajada", "subida"):
        d = diagrama("pwm", [bloque("c", "constante", c=D), bloque("p", "pwm", fc=fc, modo=modo), bloque("i", "integrador"),
                             bloque("o", "osciloscopio", n=1)],
                     [hilo("c.y", "p.d"), hilo("p.g", "i.u"), hilo("i.y", "o.u1")],
                     metodo="dopri", t_fin=N / fc, rtol=1e-3, atol=1e-6)
        r = simular(d)
        m.error(f"∫g dt, pulso {modo}", r["senales"][0]["trazas"][0][-1], N * D / fc, 1e-9)
    # Flancos de la PWM centrada en el primer periodo
    d = diagrama("pwm2", [bloque("c", "constante", c=D), bloque("p", "pwm", fc=fc), bloque("o", "osciloscopio", n=1)],
                 [hilo("c.y", "p.d"), hilo("p.g", "o.u1")], metodo="dopri", t_fin=1.0 / fc)
    s = simular(d)["senales"][0]
    t, g = s["t"], s["trazas"][0]
    subida = t[np.argmax(g > 0.5)]
    bajada = t[len(g) - 1 - np.argmax(g[::-1] > 0.5)]
    m.error("Sube en T·(1-D)/2", subida, 0.5 * (1 - D) / fc, 1e-12)
    m.error("Baja en T·(1+D)/2", bajada, 0.5 * (1 + D) / fc, 1e-12)
    # Recorte a [1/res, 1 - 1/res]: d = 0 aún da un pulso mínimo
    d = diagrama("pwm3", [bloque("c", "constante", c=0.0), bloque("p", "pwm", fc=fc, res=100.0), bloque("i", "integrador"),
                          bloque("o", "osciloscopio", n=1)],
                 [hilo("c.y", "p.d"), hilo("p.g", "i.u"), hilo("i.y", "o.u1")], metodo="dopri", t_fin=N / fc)
    m.error("d = 0 con res = 100: pulso de T/100", simular(d)["senales"][0]["trazas"][0][-1], N * 0.01 / fc, 1e-9)


@prueba(2, "Boost conmutado con paso variable", "V_out = V_in/(1-D) en régimen")
def boost_variable(m):
    """El mismo boost, con flancos exactos: con Dormand-Prince da la misma tensión que RK4, en menos pasos."""
    Vin, D, R, L, C = 48.0, 0.5, 10.0, 1e-3, 470e-6

    def modelo(**solver):
        return diagrama("b", [bloque("v", "fuente_dc", V=Vin), bloque("c", "constante", c=D), bloque("p", "pwm", fc=10000.0),
                              bloque("b", "boost_conm", L=L, R=1e-6, Ron=1e-4, Rd=1e-4, Vf=0.0, i0=Vin / (R * (1 - D) ** 2)),
                              bloque("cap", "capacitor", C=C, v0=Vin / (1 - D)), bloque("r", "resistor", R=R),
                              bloque("gnd", "tierra"), bloque("s", "voltimetro"), bloque("o", "osciloscopio", n=1)],
                        [hilo("v.p", "b.in"), hilo("v.n", "gnd.t"), hilo("b.com", "gnd.t"), hilo("b.out", "cap.p"), hilo("cap.n", "gnd.t"),
                         hilo("b.out", "r.p"), hilo("r.n", "gnd.t"), hilo("b.out", "s.p"), hilo("s.n", "gnd.t"),
                         hilo("c.y", "p.d"), hilo("p.g", "b.g"), hilo("s.v", "o.u1")], **solver)

    a = simular(modelo(metodo="dopri", t_fin=0.02, rtol=1e-3, atol=1e-5))
    b = simular(modelo(metodo="rk4", t_fin=0.02, paso=1e-6))
    va = a["senales"][0]["trazas"][0][-1]
    vb = b["senales"][0]["trazas"][0][-1]
    m.error("V_out con Dormand-Prince frente a Vin/(1-D)", va, Vin / (1 - D), 1e-3, relativo=True)
    m.verdad(f"Más cerca de lo exacto que RK4 con la compuerta muestreada ({va:.3f} frente a {vb:.3f} V)",
             abs(va - Vin / (1 - D)) < abs(vb - Vin / (1 - D)))
    m.verdad(f"Menos pasos que RK4 ({a['estadisticas']['pasos']} frente a {b['estadisticas']['pasos']})",
             a["estadisticas"]["pasos"] < b["estadisticas"]["pasos"])
    m.verdad("Dice qué estado limita el paso", bool(a["estadisticas"].get("estado_limitante")))


@prueba(1, "Región de confianza para lazos", "Sistemas con solución conocida")
def region(m):
    """Lineal: un solo paso de Newton. No lineal, con mal punto de partida: la región de confianza converge."""
    A = np.array([[3.0, 1.0], [1.0, 2.0]])
    b = np.array([9.0, 8.0])
    n = [0]

    def lineal(z):
        n[0] += 1
        return A @ z - b

    z, _, ev = lazos.resolver(lineal, np.zeros(2))
    m.error("Sistema lineal, x", z[0], 2.0, 1e-9)
    m.error("Sistema lineal, y", z[1], 3.0, 1e-9)
    m.verdad(f"Lineal: pocas evaluaciones ({ev})", ev <= 8)
    # atan(z) desde lejos: Newton puro diverge
    z, _, _ = lazos.resolver(lambda v: np.arctan(v - 1.0), np.array([20.0]))
    m.error("atan(z - 1) = 0 desde z = 20", z[0], 1.0, 1e-8)
    try:
        lazos.resolver(lambda v: v * v + 1.0, np.array([0.5]))
        m.verdad("z² + 1 = 0 se rechaza", False)
    except lazos.SinSolucion:
        m.verdad("z² + 1 = 0 se rechaza", True)


@prueba(1, "Reglas de paso de Dormand-Prince", "Escalones del paso tras un rechazo")
def reglas(m):
    """Tras un rechazo el paso no crece; si falta poco para el final, el paso se estira hasta él."""
    pasos = []
    def registrar(t, x):
        pasos.append(t)
        return False
    x = np.array([1.0])
    x, n, r = solvers._bucle_dopri(lambda t, x: -x, 0.0, 1.05, x, 1e-3, 1e-6, 1.0, 0.0, 1.0, registrar, None, None)
    m.verdad("Termina exactamente en t_f", pasos[-1] == 1.05)
    m.error("Solución de x' = -x", x[0], math.exp(-1.05), 1e-3)
    # Con un primer paso enorme hay rechazos, y el error queda dentro de la tolerancia
    m.verdad(f"Con h inicial grande hay rechazos ({r})", r >= 1)


@prueba(1, "Unidades en los parámetros", "Prefijos y comprobación contra la unidad del parámetro")
def unidades(m):
    m.error("10 mH", evaluar_numero("10 mH", None, "H"), 0.01, 1e-15)
    m.error("1 kΩ", evaluar_numero("1 kΩ", None, "Ω"), 1000.0, 1e-9)
    m.error("50 µF", evaluar_numero("50 µF", None, "F"), 50e-6, 1e-15)
    m.error("230 V", evaluar_numero("230 V", None, "V"), 230.0, 1e-12)
    m.error("2*pi*50 sigue siendo una expresión", evaluar_numero("2*pi*50", None, "rad/s"), 2 * math.pi * 50, 1e-12)
    m.error("Una variable sigue funcionando", evaluar_numero("Lf*2", {"Lf": 1.5}, "H"), 3.0, 1e-12)
    try:
        evaluar_numero("10 mF", None, "H")
        m.verdad("10 mF en una inductancia se rechaza", False)
    except ErrorUnidad as e:
        m.verdad("10 mF en una inductancia se rechaza con la unidad esperada", "H" in str(e))
    try:
        bloque_mal = diagrama("mal", [bloque("r", "resistor", R="10 mH"), bloque("g", "tierra")],
                              [hilo("r.p", "g.t"), hilo("r.n", "g.t")], metodo="rk4", t_fin=1e-3, paso=1e-4)
        simular(bloque_mal)
        m.verdad("Un bloque con unidad equivocada no simula", False)
    except ValueError as e:
        m.verdad("Un bloque con unidad equivocada avisa con su nombre", "Ω" in str(e))


@prueba(2, "Herramientas de control", "Márgenes, escalón y PI con solución analítica")
def control(m):
    """G = 1/(s+1)³: MG = 8 en ω = √3. Segundo orden: sobreimpulso exp(-πζ/√(1-ζ²)). PI que cancela el polo: MF = 90°."""
    from ..lib import control as ct
    r = ct.margenes([1.0], [1.0, 3.0, 3.0, 1.0])
    m.error("Margen de ganancia de 1/(s+1)³", r["mg"], 8.0, 1e-9)
    m.error("Frecuencia de cruce de fase", r["w_mg"], math.sqrt(3.0), 1e-9)
    r = ct.margenes([2.0], [1.0, 3.0, 3.0, 1.0])
    m.error("Margen de fase de 2/(s+1)³", r["mf"], 180 - 3 * math.degrees(math.atan(math.sqrt(2 ** (2 / 3) - 1))), 1e-8, "°")
    z, wn = 0.3, 2.0
    _, _, e = ct.escalon([wn**2], [1.0, 2 * z * wn, wn**2])
    m.error("Sobreimpulso con ζ = 0.3", e["sobreimpulso"], math.exp(-math.pi * z / math.sqrt(1 - z * z)), 1e-6)
    m.error("Tiempo de pico π/ω_d", e["t_pico"], math.pi / (wn * math.sqrt(1 - z * z)), 2e-3, "s")
    _, _, e = ct.escalon([1.0], [0.5, 1.0])
    m.error("Subida 10-90 % de primer orden = τ·ln 9", e["t_subida"], 0.5 * math.log(9), 1e-6, "s")
    m.error("Establecimiento 2 % = -τ·ln 0.02", e["t_establecimiento"], -0.5 * math.log(0.02), 1e-6, "s")
    r = ct.ajustar_pi([1.0], [0.01, 1.0], 90.0, 1000.0)
    m.error("PI con MF = 90°: Ti = τ (cancela el polo)", r["Ti"], 0.01, 1e-12, "s")
    r = ct.ajustar_pi([1.0], [1.0, 3.0, 3.0, 1.0], 60.0)
    m.error("PI automático: margen de fase comprobado", ct.margenes(*r["L"])["mf"], 60.0, 1e-6, "°")
    _, R = ct.lugar_raices([1.0], [1.0, 2.0, 0.0], [0.0, 5.0])
    m.error("Lugar de 1/(s(s+2)) con K = 5: s = -1 ± 2j", float(np.max(R[-1].imag)), 2.0, 1e-12)
