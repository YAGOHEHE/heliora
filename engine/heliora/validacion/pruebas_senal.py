"""Bloques de señal y control y tiempo discreto (encargo de modelos físicos, Fase 1)."""
from __future__ import annotations

import math

import numpy as np

from .banco import prueba
from ..sim.diagrama import bloque, cargar_librerias, diagrama, hilo, simular
from ..sim.nucleo import REGISTRO


def _b(tipo, **params):
    """Instancia suelta de un bloque, para probar su salida sin diagrama."""
    cargar_librerias()
    return REGISTRO[tipo](params, tipo)


def _y(b, *u):
    return b.salida(0.0, np.zeros(0), list(u))[0]


@prueba(1, "Señal: bloques sin estado", "Valores exactos calculados a mano")
def sin_estado(m):
    """Matemática, comparador, lógica, zona muerta, mínimo/máximo, tablas 2-D y n-D y producto matricial."""
    m.error("sin(π/6)", _y(_b("matematica", funcion="sin"), math.pi / 6), 0.5, 1e-15)
    m.error("atan2(1, -1)", _y(_b("matematica", funcion="atan2"), 1.0, -1.0), 3 * math.pi / 4, 1e-15)
    m.error("pow(2, 10)", _y(_b("matematica", funcion="pow"), 2.0, 10.0), 1024.0, 0.0)
    m.error("mod(-7, 3) = 2 (signo del divisor)", _y(_b("matematica", funcion="mod"), -7.0, 3.0), 2.0, 0.0)
    m.error("rem(-7, 3) = -1 (signo del dividendo)", _y(_b("matematica", funcion="rem"), -7.0, 3.0), -1.0, 0.0)
    m.error("grados(π)", _y(_b("matematica", funcion="grados"), math.pi), 180.0, 1e-12)
    m.error("reciproco(4)", _y(_b("matematica", funcion="reciproco"), 4.0), 0.25, 0.0)
    m.error("comparador 2 >= 2", _y(_b("comparador", op=">="), 2.0, 2.0), 1.0, 0.0)
    m.error("comparador 2 < 2", _y(_b("comparador", op="<"), 2.0, 2.0), 0.0, 0.0)
    m.error("comparador 1 != 2", _y(_b("comparador", op="!="), 1.0, 2.0), 1.0, 0.0)
    m.error("AND(1, 1, 0)", _y(_b("logica", op="AND", n=3), 1.0, 1.0, 0.0), 0.0, 0.0)
    m.error("XOR(1, 1, 1)", _y(_b("logica", op="XOR", n=3), 1.0, 1.0, 1.0), 1.0, 0.0)
    m.error("NOR(0, 0)", _y(_b("logica", op="NOR", n=2), 0.0, 0.2), 1.0, 0.0)
    m.error("NOT(0.7)", _y(_b("logica", op="NOT", n=1), 0.7), 0.0, 0.0)
    z = _b("zona_muerta", lo=-0.5, hi=0.5)
    m.error("zona muerta: u = 0.3 → 0", _y(z, 0.3), 0.0, 0.0)
    m.error("zona muerta: u = 2 → 1.5", _y(z, 2.0), 1.5, 0.0)
    m.error("zona muerta: u = -2 → -1.5", _y(z, -2.0), -1.5, 0.0)
    m.error("máximo de (3, -1, 7)", _y(_b("min_max", modo="max", n=3), 3.0, -1.0, 7.0), 7.0, 0.0)
    m.error("mínimo de (3, -1, 7)", _y(_b("min_max", modo="min", n=3), 3.0, -1.0, 7.0), -1.0, 0.0)
    # Tabla bilineal de f(x1, x2) = 2·x1 + 3·x2 + x1·x2: la interpolación bilineal la reproduce exacta
    x1, x2 = [0.0, 1.0, 3.0], [0.0, 2.0]
    tabla = [[2 * a + 3 * c + a * c for c in x2] for a in x1]
    t2 = _b("lookup_2d", x1=x1, x2=x2, tabla=tabla)
    m.error("lookup 2-D en (2, 0.5)", _y(t2, 2.0, 0.5), 2 * 2 + 3 * 0.5 + 2 * 0.5, 1e-12)
    m.error("lookup 2-D saturado en (5, 3) = f(3, 2)", _y(t2, 5.0, 3.0), 2 * 3 + 3 * 2 + 3 * 2, 1e-12)
    t2e = _b("lookup_2d", x1=x1, x2=x2, tabla=tabla, fuera="extrapolar")
    m.error("lookup 2-D extrapolado en (4, 1) (lineal por tramos)", _y(t2e, 4.0, 1.0), 2 * 4 + 3 * 1 + 4 * 1, 1e-12)
    # Trilineal de f = x + 10·y + 100·z: exacta por ser lineal en cada eje
    tn = _b("lookup_nd", n=3, puntos="[[0, 1], [0, 2, 4], [0, 1]]",
            tabla="[[[a + 10*b + 100*c for c in (0, 1)] for b in (0, 2, 4)] for a in (0, 1)]")
    m.error("lookup n-D en (0.25, 3, 0.5)", _y(tn, 0.25, 3.0, 0.5), 0.25 + 30.0 + 50.0, 1e-12)
    pm = _b("producto_matricial", A="[[1, 2], [3, 4]]")
    y = _y(pm, np.array([1.0, -1.0]))
    m.cota("A·u con A = [[1, 2], [3, 4]], u = [1, -1] → [-1, -1]", np.max(np.abs(y - np.array([-1.0, -1.0]))), 0.0)
    pm2 = _b("producto_matricial", modo="U1·U2")
    y2 = _y(pm2, np.array([[1.0, 2.0]]), np.array([[3.0], [4.0]]))
    m.error("U1·U2 = [1, 2]·[3, 4]ᵀ = 11", y2, 11.0, 0.0)
    try:
        _y(pm, np.array([1.0, 2.0, 3.0]))
        m.verdad("Formas incompatibles: error con el nombre del bloque", False)
    except ValueError as e:
        m.verdad("Formas incompatibles: error con el nombre del bloque", "formas incompatibles" in str(e))


def _planta_pid(controlador, t_fin=0.3, **kw):
    """Planta 1/(0.05·s + 1) en lazo cerrado con un escalón de consigna en t = 0.01."""
    return diagrama("pid", [
        bloque("r", "escalon", t0=0.01, y0=0.0, y1=1.0),
        controlador,
        bloque("g", "funcion_transferencia", num=[1.0], den=[0.05, 1.0]),
        bloque("o", "osciloscopio", n=2),
    ], [hilo("r.y", "c.r"), hilo("g.y", "c.y"), hilo("c.u", "g.u"), hilo("g.y", "o.u1"), hilo("c.u", "o.u2")],
        metodo="rk4", t_fin=t_fin, paso=1e-5, **kw)


@prueba(1, "Señal: PID discreto a 10 kHz frente al continuo",
        "Åström y Hägglund, Advanced PID Control (2006), §3.4; criterio del encargo: ±1 %")
def pid(m):
    """El PID discreto a 10 kHz sigue al continuo con paso fino; el anti-windup limita el sobreimpulso."""
    g = dict(Kp=2.0, Ki=40.0, Kd=0.02, N=500.0, c=0.0)
    rc = simular(_planta_pid(bloque("c", "pid", **g)))["senales"][0]
    rd = simular(_planta_pid(bloque("c", "pid_discreto", Ts=1e-4, **g)))["senales"][0]
    yc = np.interp(rd["t"], rc["t"], rc["trazas"][0])
    m.cota("Máxima diferencia de la salida (en por unidad de la consigna)", np.max(np.abs(rd["trazas"][0] - yc)), 0.01)
    # El modo lento del lazo (cero del PI en Ki/Kp = 20 rad/s) aún no se ha extinguido del todo en 0.3 s
    m.error("Acción integral: y(0.3 s) ≈ 1", rd["trazas"][0][-1], 1.0, 5e-3)
    # Saturación fuerte: sin anti-windup el integral se carga y la salida se pasa mucho de la consigna
    sat = dict(Kp=0.5, Ki=200.0, u_min=0.0, u_max=1.2)
    con = simular(_planta_pid(bloque("c", "pid", **sat), t_fin=0.6))["senales"][0]["trazas"][0]
    sin = simular(_planta_pid(bloque("c", "pid", Tt=1e9, **sat), t_fin=0.6))["senales"][0]["trazas"][0]
    so_con, so_sin = con.max() - 1.0, sin.max() - 1.0
    m.verdad(f"Anti-windup: sobreimpulso {100 * so_con:.1f} % frente a {100 * so_sin:.1f} % sin él",
             so_con < 0.5 * so_sin)
    m.cota("Anti-windup: sobreimpulso con retrocálculo", so_con, 0.08)


@prueba(1, "Señal: cruces por cero del relé y la zona muerta", "Instantes analíticos de cruce")
def cruces(m):
    """El solver de paso variable cae en el instante exacto del cambio."""
    d = diagrama("rele", [
        bloque("s", "senoidal", A=1.0, f=1.0), bloque("re", "rele", punto_subida=0.5, punto_bajada=-0.5),
        bloque("rp", "rampa", m=1.0, y0=-1.0), bloque("z", "zona_muerta", lo=-0.5, hi=0.5),
        bloque("o", "osciloscopio", n=2),
    ], [hilo("s.y", "re.u"), hilo("rp.y", "z.u"), hilo("re.y", "o.u1"), hilo("z.y", "o.u2")],
        metodo="dopri", t_fin=1.0, rtol=1e-6, atol=1e-9)
    r = simular(d)
    s = r["senales"][0]
    t, y = s["t"], s["trazas"][0]
    t_on = t[np.argmax(y > 0.5)]
    sube = math.asin(0.5) / (2 * math.pi)
    m.error("Relé: se enciende cuando sin(2πt) = 0.5", t_on, sube, 1e-9, "s")
    apaga = t[np.argmax((t > t_on) & (y < 0.5))]
    m.error("Relé: se apaga cuando sin(2πt) = -0.5", apaga, 0.5 + sube, 1e-9, "s")
    # Zona muerta con u = t - 1: deja la zona en u = 0.5 (t = 1.5 queda fuera); entra en t = 0.5
    m.cota("Zona muerta: el solver cae en t = 0.5 (u = lo)", np.min(np.abs(t - 0.5)), 1e-9)
    m.error("Zona muerta: y(t) = 0 dentro de la zona", np.max(np.abs(s["trazas"][1][(t > 0.5) & (t < 1.0)])), 0.0, 0.0)
    m.error("Zona muerta: y = u - lo antes de la zona (t = 0.25)",
            np.interp(0.25, t[t < 0.5], s["trazas"][1][t < 0.5]), -0.75 + 0.5, 1e-9)


@prueba(1, "Discreto: muestreo, retardo unitario, ZOH e integrador", "Secuencias exactas en cada instante")
def discreto(m):
    """Con u = t y Ts = 0.1: muestreo da k·Ts, z⁻¹ da (k-1)·Ts, el integrador adelante de 1 da k·Ts."""
    Ts = 0.1
    d = diagrama("disc", [
        bloque("t", "reloj"), bloque("uno", "constante", c=1.0),
        bloque("mu", "muestreo", Ts=Ts), bloque("z", "retardo_unitario", Ts=Ts, valor_inicial=-1.0),
        bloque("zoh", "retenedor_zoh", Ts=0.25), bloque("i", "integrador_discreto", Ts=Ts),
        bloque("tz", "funcion_transferencia_z", Ts=Ts, num="[1]", den="[1, -0.5]"),
        bloque("o", "osciloscopio", n=5),
    ], [hilo("t.t", "mu.u"), hilo("mu.y", "z.u"), hilo("t.t", "zoh.u"), hilo("uno.y", "i.u"), hilo("uno.y", "tz.u"),
        hilo("mu.y", "o.u1"), hilo("z.y", "o.u2"), hilo("zoh.y", "o.u3"), hilo("i.y", "o.u4"), hilo("tz.y", "o.u5")],
        metodo="dopri", t_fin=1.0)
    s = simular(d)["senales"][0]
    t, y = s["t"], s["trazas"]
    # Valor retenido en mitad de cada intervalo (k + 0.5)·Ts
    k = np.arange(10)
    medio = lambda tr, tt: np.array([tr[np.searchsorted(t, v, side="right") - 1] for v in tt])
    tm = (k + 0.5) * Ts
    m.cota("Muestreo: y = k·Ts entre instantes", np.max(np.abs(medio(y[0], tm) - k * Ts)), 1e-12)
    previo = np.where(k == 0, -1.0, (k - 1) * Ts)
    m.cota("Retardo unitario: y[k] = u[k-1] (y[0] = valor inicial)", np.max(np.abs(medio(y[1], tm) - previo)), 1e-12)
    m.cota("ZOH a 0.25 s: mantiene u(t_k) entre instantes", np.max(np.abs(medio(y[2], tm) - np.floor(tm / 0.25) * 0.25)), 1e-12)
    m.cota("Integrador adelante de u = 1: y[k] = Ts·k", np.max(np.abs(medio(y[3], tm) - Ts * k)), 1e-12)
    # 1/(z - 0.5) con u = 1: y[0] = 0, y[k] = 2·(1 - 0.5^k)
    m.cota("1/(z - 0.5) con escalón: y[k] = 2·(1 - 0.5^k)", np.max(np.abs(medio(y[4], tm) - 2 * (1 - 0.5**k))), 1e-12)


@prueba(1, "Discreto: el solver cae en los instantes de muestreo", "t_k = offset + k·Ts")
def instantes(m):
    """Con paso variable, cada instante k·Ts + offset aparece entre los pasos aceptados."""
    Ts, off = 0.0123, 0.004
    d = diagrama("tics", [bloque("s", "senoidal", f=3.0), bloque("mu", "muestreo", Ts=Ts, offset=off),
                          bloque("o", "osciloscopio", n=1)],
                 [hilo("s.y", "mu.u"), hilo("mu.y", "o.u1")], metodo="dopri", t_fin=0.5)
    t = simular(d)["senales"][0]["t"]
    tk = off + Ts * np.arange(int((0.5 - off) / Ts) + 1)
    err = max(np.min(np.abs(t - v)) for v in tk)
    m.cota(f"Error máximo de los {tk.size} instantes de muestreo", err, 1e-13, "s")
    d["solver"] = {"metodo": "rk4", "paso": 1e-3, "t_fin": 0.1}
    r = simular(d)
    m.verdad("Con paso fijo que no divide a Ts se avisa", any("múltiplos del paso" in a for a in r["avisos"]))


@prueba(1, "Señal: limitador de tasa continuo y discreto", "Pendiente máxima R exacta")
def limitador(m):
    """Una rampa de pendiente 10 limitada a R = 2 sube con pendiente 2; el discreto, |Δy| = R·Ts."""
    d = diagrama("tasa", [bloque("rp", "rampa", m=10.0), bloque("l", "limitador_tasa", R_alta=2.0, R_baja=-2.0),
                          bloque("ld", "limitador_tasa_discreto", Ts=0.01, R_alta=2.0, R_baja=-2.0),
                          bloque("o", "osciloscopio", n=2)],
                 [hilo("rp.y", "l.u"), hilo("rp.y", "ld.u"), hilo("l.y", "o.u1"), hilo("ld.y", "o.u2")],
                 metodo="dopri", t_fin=1.0, rtol=1e-8, atol=1e-10)
    s = simular(d)["senales"][0]
    t, y = s["t"], s["trazas"]
    pend = (np.interp(1.0, t, y[0]) - np.interp(0.5, t, y[0])) / 0.5
    m.error("Continuo: pendiente de la salida", pend, 2.0, 1e-9)
    saltos = np.diff(np.unique(np.round(y[1], 12)))
    m.error("Discreto: salto por instante R·Ts", np.max(saltos), 2.0 * 0.01, 1e-12)
    m.error("Discreto: y(1) = R·t", y[1][-1], 2.0, 1e-9)


@prueba(1, "Señal: ruido blanco de banda limitada", "Varianza PSD/Tc; misma semilla, misma secuencia")
def ruido(m):
    """Con Tc = 1 ms y PSD = 0.04, la varianza de las muestras es 40; la semilla la hace reproducible."""
    def correr(semilla, tipo):
        d = diagrama("ruido", [bloque("n", "ruido", Ts=1e-3, PSD=0.04, semilla=semilla, tipo_ruido=tipo),
                               bloque("mu", "muestreo", Ts=1e-3, offset=5e-4), bloque("o", "osciloscopio", n=1)],
                     [hilo("n.y", "mu.u"), hilo("mu.y", "o.u1")], metodo="rk4", paso=5e-4, t_fin=10.0)
        return simular(d)["senales"][0]["trazas"][0][1::2]
    a, b = correr(3, "gaussiano"), correr(3, "gaussiano")
    m.verdad("Misma semilla: secuencia idéntica", np.array_equal(a, b))
    m.error("Gaussiano: varianza PSD/Tc = 40", np.var(a), 40.0, 0.05, relativo=True)
    u = correr(5, "uniforme")
    m.error("Uniforme: varianza PSD/Tc = 40", np.var(u), 40.0, 0.05, relativo=True)
    m.cota("Uniforme: |y| <= √(3·PSD/Tc)", max(0.0, np.max(np.abs(u)) - math.sqrt(3 * 40.0)), 1e-12)
