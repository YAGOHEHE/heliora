"""Herramientas de análisis que la interfaz muestra como formularios.

Cada herramienta es una función decorada con @herramienta: sus parámetros (lo que el
formulario pregunta) y un informe de vuelta con texto, tablas y gráficas. Para añadir una,
basta con escribir aquí otra función.
"""
from __future__ import annotations

import datetime as dt
import math

import numpy as np
import pandas as pd

from .espacio import figura, figura3d, serie
from .lib import aprendizaje as ml
from .lib import estadistica as es
from .lib import fotovoltaica as fv
from .lib import mantenimiento as mt
from .lib import potencia as pot

HERRAMIENTAS = {}


def herramienta(modulo, ident, titulo, params=None, grupo=""):
    """Registra una herramienta. params: (nombre, etiqueta, tipo, defecto[, opciones])."""
    def deco(fn):
        HERRAMIENTAS[ident] = {"id": ident, "modulo": modulo, "titulo": titulo, "grupo": grupo, "doc": (fn.__doc__ or "").strip(),
                               "params": [{"nombre": p[0], "etiqueta": p[1], "tipo": p[2], "defecto": p[3], "opciones": list(p[4]) if len(p) > 4 else []}
                                          for p in (params or [])], "fn": fn}
        return fn
    return deco


def listar():
    """Fichas de las herramientas registradas, sin la función que las ejecuta."""
    return [{k: v for k, v in h.items() if k != "fn"} for h in HERRAMIENTAS.values()]


def ejecutar(ident, valores, espacio):
    """Ejecuta la herramienta `ident` con los valores dados, evaluando los numéricos en el espacio de trabajo."""
    h =HERRAMIENTAS[ident]
    v = {p["nombre"]: valores.get(p["nombre"], p["defecto"]) for p in h["params"]}
    for p in h["params"]:
        if p["tipo"] == "num":
            v[p["nombre"]] = float(espacio_eval(v[p["nombre"]], espacio))
        elif p["tipo"] == "entero":
            v[p["nombre"]] = int(float(espacio_eval(v[p["nombre"]], espacio)))
    return h["fn"](Ctx(espacio), **v)


def espacio_eval(x, espacio):
    """Evalúa una expresión numérica con las variables del espacio de trabajo."""
    from .sim.nucleo import evaluar_numero
    return evaluar_numero(x, espacio.ns)


class Ctx:
    """Lo que recibe cada herramienta: acceso al espacio de trabajo y al informe."""

    def __init__(self, espacio):
        self.espacio = espacio
        self.ns = espacio.ns
        self.sec = []

    def df(self, nombre):
        """Tabla del espacio de trabajo con ese nombre (convierte series y matrices en tabla)."""
        if not nombre:
            raise ValueError("Elige una tabla de datos. Puedes crearlas desde la consola o con «Importar datos».")
        d = self.ns.get(nombre)
        if isinstance(d, pd.Series):
            d = d.to_frame(d.name or "valor")
        if isinstance(d, np.ndarray):
            d = pd.DataFrame(d if d.ndim == 2 else d[:, None])
            d.columns = [str(c) for c in d.columns]
        if not isinstance(d, pd.DataFrame):
            raise ValueError(f"La variable «{nombre}» no es una tabla")
        return d

    def col(self, nombre, columna):
        """Columna numérica de una tabla como vector, sin los valores no numéricos."""
        d =self.df(nombre)
        if columna not in d.columns:
            raise ValueError(f"La tabla «{nombre}» no tiene la columna «{columna}»")
        return pd.to_numeric(d[columna], errors="coerce").dropna().to_numpy(dtype=float)

    def texto(self, t):
        """Añade un párrafo al informe."""
        self.sec.append({"tipo": "texto", "texto": t})

    def aviso(self, nivel, t):
        """Añade un aviso con su nivel (bueno, alerta o falla)."""
        self.sec.append({"tipo": "aviso", "nivel": nivel, "texto": t})

    def kv(self, titulo, items):
        """Añade una lista de pares nombre-valor, con unidad opcional como tercer elemento."""
        self.sec.append({"tipo": "kv", "titulo": titulo, "items": [[str(a), _fmt(b), str(c) if len(_x) > 2 else ""] for _x in items for a, b, c in [(*_x, "")[:3]]]})

    def tabla(self, titulo, columnas, filas):
        """Añade una tabla con sus columnas y filas."""
        self.sec.append({"tipo": "tabla", "titulo": titulo, "columnas": [str(c) for c in columnas], "filas": [[_fmt(x) for x in f] for f in filas]})

    def grafica(self, fig):
        """Añade una gráfica."""
        self.sec.append({"tipo": "grafica", "figura": fig})

    def informe(self, titulo):
        """Devuelve el informe con las secciones acumuladas y las variables del espacio de trabajo."""
        return {"titulo": titulo, "secciones": self.sec, "variables": self.espacio.variables()}


def _fmt(v):
    if isinstance(v, str):
        return v
    if isinstance(v, (bool, np.bool_)):
        return "sí" if v else "no"
    if isinstance(v, (int, np.integer)):
        return str(int(v))
    if isinstance(v, (float, np.floating)):
        v = float(v)
        if not math.isfinite(v):
            return "—"
        if v != 0 and (abs(v) >= 1e6 or abs(v) < 1e-3):
            return f"{v:.4e}"
        return f"{v:.5g}"
    return str(v)


def _pct(v):
    return f"{100 * float(v):.2f} %"


# ================================================================= Módulo 3: potencia


@herramienta("potencia", "pot_perdidas", "Pérdidas y eficiencia del inversor", [
    ("P_kw", "Potencia de salida [kW]", "num", 100.0), ("V_ll", "Tensión de línea [V]", "num", 400.0), ("fp", "Factor de potencia", "num", 1.0),
    ("Vdc", "Tensión del bus [V]", "num", 750.0), ("fsw", "Frecuencia de conmutación [Hz]", "num", 5000.0),
    ("Ron", "R del IGBT [Ω]", "num", 0.002), ("Vce0", "Tensión umbral IGBT [V]", "num", 0.8), ("Rd", "R del diodo [Ω]", "num", 0.002), ("Vf", "Tensión del diodo [V]", "num", 0.9),
    ("Eon", "E_on [mJ]", "num", 139.96), ("Eoff", "E_off [mJ]", "num", 174.78), ("Err", "E_rr [mJ]", "num", 42.0), ("Vref", "V de referencia [V]", "num", 600.0), ("Iref", "I de referencia [A]", "num", 1191.06)
], "Convertidores")
def pot_perdidas(c, P_kw, V_ll, fp, Vdc, fsw, Ron, Vce0, Rd, Vf, Eon, Eoff, Err, Vref, Iref):
    """Pérdidas de conducción y conmutación con las fórmulas de valor medio para SPWM."""
    p = dict(Ron=Ron, Vce0=Vce0, Rd=Rd, Vf=Vf, Eon=Eon * 1e-3, Eoff=Eoff * 1e-3, Err=Err * 1e-3, Vref=Vref, Iref=Iref)
    S = P_kw * 1e3 / fp
    Vpk = V_ll * math.sqrt(2 / 3)
    Ipk = 2 * S / (3 * Vpk)
    m = 2 * Vpk / Vdc
    r = pot.perdidas_analiticas(Ipk, m, fp, Vdc, fsw, p)
    eta = P_kw * 1e3 / (P_kw * 1e3 + r["total"])
    c.kv("Punto de operación", [("Corriente de pico", Ipk, "A"), ("Índice de modulación", m), ("Pérdidas de conducción", r["cond"], "W"),
                               ("Pérdidas de conmutación", r["sw"], "W"), ("Pérdidas totales", r["total"], "W"), ("Eficiencia", eta * 100, "%")])
    xs = np.linspace(0.05, 1.1, 60)
    etas = []
    for x in xs:
        Ii = Ipk * x
        etas.append(100 * P_kw * 1e3 * x / (P_kw * 1e3 * x + pot.perdidas_analiticas(Ii, m, fp, Vdc, fsw, p)["total"]))
    c.grafica(figura("Eficiencia frente a la carga", "Carga [por unidad]", "Eficiencia [%]", [serie(xs, etas, "η")]))
    return c.informe("Pérdidas del inversor")


@herramienta("potencia", "pot_lcl", "Diseño del filtro LCL", [
    ("L1", "L lado inversor [mH]", "num", 0.5), ("L2", "L lado red [mH]", "num", 0.15), ("C", "C [µF]", "num", 50.0), ("Rd", "R de amortiguamiento [Ω]", "num", 0.5),
    ("fsw", "Frecuencia de conmutación [Hz]", "num", 5000.0), ("f0", "Frecuencia de red [Hz]", "num", 50.0)
], "Filtros")
def pot_lcl(c, L1, L2, C, Rd, fsw, f0):
    """Respuesta en frecuencia i2/v1 de un LCL: G(s) = (1 + s·Rd·C)/(s³·L1·L2·C + s²·(L1+L2)·Rd·C + s·(L1+L2))."""
    a, b, cc = L1 * 1e-3, L2 * 1e-3, C * 1e-6
    fr = math.sqrt((a + b) / (a * b * cc)) / (2 * math.pi)
    f = np.logspace(1, 5, 500)
    s = 2j * np.pi * f
    G = (1 + s * Rd * cc) / (s**3 * a * b * cc + s**2 * (a + b) * Rd * cc + s * (a + b))
    ate = abs((1 + 2j * np.pi * fsw * Rd * cc) / ((2j * np.pi * fsw) ** 3 * a * b * cc + (2j * np.pi * fsw) ** 2 * (a + b) * Rd * cc + 2j * np.pi * fsw * (a + b)))
    c.kv("Diseño", [("Frecuencia de resonancia", fr, "Hz"), ("Razón f_res/f_sw", fr / fsw), ("Razón f_res/f_red", fr / f0),
                   ("Admitancia a f_sw", ate, "A/V"), ("Amortiguamiento con Rd (Q ≈)", (1 / Rd) * math.sqrt(a * b / ((a + b) * cc)))])
    if fr > fsw / 2:
        c.aviso("alerta", "La resonancia debería estar por debajo de f_sw/2.")
    if fr < 10 * f0:
        c.aviso("alerta", "La resonancia debería estar por encima de 10·f_red.")
    fig = figura("Respuesta en frecuencia i2/v1", "Frecuencia [Hz]", "|G| [dB]", [serie(f, 20 * np.log10(np.abs(G)), "|G|")], log_x=True)
    c.grafica(fig)
    return c.informe("Filtro LCL")


# ================================================================= Módulo 4: fotovoltaica


OPC_MOD = list(fv.CATALOGO.keys())


def _modulo(modelo, Voc, Isc, Vmp, Imp, Ns, alfa, beta):
    if modelo != "Datos a mano":
        return fv.modulo_de_catalogo(modelo)
    return fv.ModuloFV(nombre="A mano", Voc=Voc, Isc=Isc, Vmp=Vmp, Imp=Imp, Ns=int(Ns), alfa_sc=alfa * Isc / 100, beta_oc=beta * Voc / 100).extraer()


@herramienta("fv", "fv_hoja", "Parámetros desde la hoja de datos", [
    ("modelo", "Módulo", "opcion", OPC_MOD[0], OPC_MOD + ["Datos a mano"]), ("Voc", "V_oc [V]", "num", 45.0), ("Isc", "I_sc [A]", "num", 9.0), ("Vmp", "V_mp [V]", "num", 37.0),
    ("Imp", "I_mp [A]", "num", 8.5), ("Ns", "Celdas en serie", "num", 72), ("alfa", "Coef. I_sc [%/°C]", "num", 0.05), ("beta", "Coef. V_oc [%/°C]", "num", -0.29)
], "Módulo")
def fv_hoja(c, modelo, Voc, Isc, Vmp, Imp, Ns, alfa, beta):
    """Extrae los cinco parámetros del modelo de un diodo y compara la curva con la hoja de datos."""
    m = _modulo(modelo, Voc, Isc, Vmp, Imp, Ns, alfa, beta)
    V, I, P, pm = fv.curva_iv(m, 1000.0, 25.0, 1500)
    c.kv("Parámetros extraídos (STC)", [("a = n·N_s·k·T/q", m.a, "V"), ("Factor de idealidad n", m.a / (m.Ns * fv.K_EV * fv.T_REF)),
                                        ("I_L", m.IL, "A"), ("I_0", m.I0, "A"), ("R_s", m.Rs, "Ω"), ("R_sh", m.Rsh, "Ω"), ("Residuo de extracción", m.residuo)])
    c.tabla("Hoja de datos frente al modelo", ["Punto", "Hoja", "Modelo", "Error %"], [
        ["I_sc [A]", m.Isc, float(I[0]), 100 * abs(float(I[0]) - m.Isc) / m.Isc],
        ["V_oc [V]", m.Voc, float(V[-1]), 100 * abs(float(V[-1]) - m.Voc) / m.Voc],
        ["V_mp [V]", m.Vmp, pm["Vmp"], 100 * abs(pm["Vmp"] - m.Vmp) / m.Vmp],
        ["I_mp [A]", m.Imp, pm["Imp"], 100 * abs(pm["Imp"] - m.Imp) / m.Imp],
        ["P_mp [W]", m.Pmp, pm["Pmp"], 100 * abs(pm["Pmp"] - m.Pmp) / m.Pmp]])
    c.grafica(figura("Curva I-V a STC", "Tensión [V]", "Corriente [A]", [serie(V, I, "Modelo"), serie([0, m.Vmp, m.Voc], [m.Isc, m.Imp, 0], "Hoja de datos", "puntos")]))
    return c.informe(m.nombre)


@herramienta("fv", "fv_curvas", "Curvas según irradiancia y temperatura", [
    ("modelo", "Módulo", "opcion", OPC_MOD[0], OPC_MOD), ("temp", "T de celda para irradiancias [°C]", "num", 25.0), ("g", "G para temperaturas [W/m²]", "num", 1000.0)
], "Módulo")
def fv_curvas(c, modelo, temp, g):
    """Familias de curvas I-V y P-V con el modelo de De Soto, y superficie P(G, T)."""
    m = fv.modulo_de_catalogo(modelo)
    s1, s2 = [], []
    filas = []
    for G in (200, 400, 600, 800, 1000):
        V, I, P, pm = fv.curva_iv(m, G, temp)
        s1.append(serie(V, I, f"{G} W/m²"))
        s2.append(serie(V, P, f"{G} W/m²"))
        filas.append([G, temp, pm["Pmp"], pm["Vmp"], pm["Imp"]])
    for T in (0, 25, 50, 75):
        V, I, P, pm = fv.curva_iv(m, g, T)
        s2.append(serie(V, P, f"{T} °C"))
        filas.append([g, T, pm["Pmp"], pm["Vmp"], pm["Imp"]])
    c.grafica(figura("Curvas I-V", "Tensión [V]", "Corriente [A]", s1))
    c.grafica(figura("Curvas P-V", "Tensión [V]", "Potencia [W]", s2))
    Gs = np.linspace(100, 1200, 30)
    Ts = np.linspace(-5, 75, 30)
    Z = np.array([[fv.curva_iv(m, a, b, 150)[3]["Pmp"] for a in Gs] for b in Ts])
    c.grafica(figura3d("Potencia máxima P(G, T)", Gs, Ts, Z, "G [W/m²]", "T [°C]", "P [W]"))
    c.tabla("Máxima potencia", ["G [W/m²]", "T [°C]", "P_mp [W]", "V_mp [V]", "I_mp [A]"], filas)
    return c.informe("Curvas de " + modelo)


@herramienta("fv", "fv_sombra", "Sombreado parcial con bypass", [
    ("modelo", "Módulo", "opcion", OPC_MOD[0], OPC_MOD), ("Ns", "Módulos en serie", "entero", 12), ("Np", "Strings en paralelo", "entero", 4),
    ("sub", "Sub-módulos sombreados", "entero", 8), ("factor", "Irradiancia en sombra [por unidad]", "num", 0.3), ("n_somb", "Strings con sombra", "entero", 2),
    ("G", "Irradiancia [W/m²]", "num", 1000.0), ("T", "T de celda [°C]", "num", 25.0)
], "Arreglo")
def fv_sombra(c, modelo, Ns, Np, sub, factor, n_somb, G, T):
    """Curva del arreglo con y sin sombra; muestra los máximos locales que confunden al MPPT."""
    m = fv.modulo_de_catalogo(modelo)
    tot = Ns * 3
    limpio = [1.0] * tot
    somb = [1.0] * (tot - sub) + [factor] * sub
    V0, I0 = fv.curva_arreglo(m, [limpio] * Np, T, G)
    V1, I1 = fv.curva_arreglo(m, [somb] * n_somb + [limpio] * (Np - n_somb), T, G)
    P0, P1 = V0 * I0, V1 * I1
    k = np.where((P1[1:-1] > P1[:-2]) & (P1[1:-1] >= P1[2:]) & (P1[1:-1] > 0.05 * P1.max()))[0] + 1
    c.kv("Resultado", [("P máxima sin sombra", P0.max(), "W"), ("P máxima con sombra", P1.max(), "W"), ("Pérdida", 100 * (1 - P1.max() / P0.max()), "%"), ("Máximos locales", int(k.size))])
    c.tabla("Máximos locales", ["V [V]", "P [W]"], [[V1[i], P1[i]] for i in k])
    c.grafica(figura("Curva P-V", "Tensión [V]", "Potencia [W]", [serie(V0, P0, "Sin sombra"), serie(V1, P1, "Con sombra"), serie(V1[k], P1[k], "Máximos", "puntos")]))
    c.grafica(figura("Curva I-V", "Tensión [V]", "Corriente [A]", [serie(V0, I0, "Sin sombra"), serie(V1, I1, "Con sombra")]))
    return c.informe("Sombreado parcial")


@herramienta("fv", "fv_sol", "Posición solar, irradiancia y temperatura", [
    ("lat", "Latitud [°]", "num", 40.4), ("lon", "Longitud [°]", "num", -3.7), ("fecha", "Fecha", "texto", "2026-06-21"), ("incl", "Inclinación [°]", "num", 30.0), ("acim", "Acimut [°]", "num", 180.0),
    ("t_amb", "T ambiente [°C]", "num", 25.0), ("viento", "Viento [m/s]", "num", 2.0)
], "Recurso solar")
def fv_sol(c, lat, lon, fecha, incl, acim, t_amb, viento):
    """Posición solar (SPA), irradiancia en plano inclinado (Hay-Davies, cielo despejado) y temperatura de celda (SAPM), con pvlib."""
    t = pd.date_range(fecha, periods=24 * 12 + 1, freq="5min", tz="UTC")
    poa, sol, cs = fv.irradiancia_poa(lat, lon, t, incl, acim)
    h = np.arange(t.size) * 5 / 60
    Tc = fv.temperatura_modulo(poa["poa_global"].fillna(0), t_amb, viento)
    H = poa["poa_global"].fillna(0).sum() * (5 / 60) / 1000
    c.kv("Resumen del día", [("Irradiación en plano", H, "kWh/m²"), ("Irradiación horizontal", cs["ghi"].sum() * (5 / 60) / 1000, "kWh/m²"), ("Elevación máxima", sol["apparent_elevation"].max(), "°"), ("T de celda máxima", Tc.max(), "°C")])
    c.grafica(figura("Irradiancia (hora UTC)", "Hora", "W/m²", [serie(h, poa["poa_global"].fillna(0), "En plano"), serie(h, cs["ghi"], "Horizontal"), serie(h, cs["dni"], "Directa normal")]))
    c.grafica(figura("Posición solar", "Hora", "Grados", [serie(h, sol["apparent_elevation"], "Elevación"), serie(h, sol["azimuth"], "Acimut")]))
    c.grafica(figura("Temperatura de celda", "Hora", "°C", [serie(h, Tc, "T de celda")]))
    return c.informe(f"Recurso solar en {lat}°, {lon}°")


# ================================================================= Módulo 5: estadística


@herramienta("estadistica", "es_muestra", "Datos de ejemplo", [("nombre", "Nombre de la variable", "texto", "datos")], "Datos")
def es_muestra(c, nombre):
    """Crea una tabla de ejemplo con vida útil, potencias y grupos, para probar las herramientas."""
    rng = np.random.default_rng(0)
    n = 500
    grupos = rng.choice(["Campo A", "Campo B", "Campo C"], n)
    irr = rng.uniform(100, 1000, n)
    p = 0.48 * irr + 8 + rng.normal(0, 8, n) - (grupos == "Campo C") * 15
    d = pd.DataFrame({"vida_horas": 12000 * rng.weibull(1.7, n), "irradiancia": irr, "potencia_kw": p, "campo": grupos,
                      "temperatura": 25 + 0.03 * irr + rng.normal(0, 2, n)})
    c.ns[nombre] = d
    c.aviso("bueno", f"Se creó la tabla «{nombre}» con {n} filas en el espacio de trabajo.")
    return c.informe("Datos de ejemplo")


@herramienta("estadistica", "es_descriptiva", "Estadística descriptiva", [("tabla", "Tabla", "tabla", ""), ("columna", "Columna", "columna", "")], "Descripción")
def es_descriptiva(c, tabla, columna):
    """Media, mediana, dispersión, cuartiles, asimetría y curtosis, con histograma."""
    x = c.col(tabla, columna)
    r = es.descriptiva(x)
    c.kv(columna, [(k, v) for k, v in r.items()])
    h, e = np.histogram(x, 40)
    c.grafica(figura("Histograma", columna, "Frecuencia", [serie(0.5 * (e[:-1] + e[1:]), h, "n", "barras")]))
    return c.informe("Estadística descriptiva")


@herramienta("estadistica", "es_ajuste", "Ajuste de distribuciones", [("tabla", "Tabla", "tabla", ""), ("columna", "Columna", "columna", "")], "Distribuciones")
def es_ajuste(c, tabla, columna):
    """Ajusta normal, lognormal, exponencial, gamma y Weibull por máxima verosimilitud y las ordena por AIC."""
    x = c.col(tabla, columna)
    r = es.comparar_ajustes(x)
    c.tabla("Comparación (menor AIC es mejor)", ["Distribución", "Parámetros", "AIC", "KS", "p (KS)"],
            [[a["distribucion"], ", ".join(f"{p:.4g}" for p in a["parametros"]), a["aic"], a["ks_estadistico"], a["ks_p"]] for a in r])
    xs = np.linspace(x.min(), x.max(), 300)
    h, e = np.histogram(x, 40, density=True)
    ser = [serie(0.5 * (e[:-1] + e[1:]), h, "Datos", "barras")]
    for a in r[:3]:
        ser.append(serie(xs, es.DISTRIBUCIONES[a["distribucion"]].pdf(xs, *a["parametros"]), a["distribucion"]))
    c.grafica(figura("Densidad", columna, "Densidad", ser))
    return c.informe(f"Ajuste de {columna}")


@herramienta("estadistica", "es_weibull", "Análisis Weibull", [("tabla", "Tabla", "tabla", ""), ("columna", "Tiempos", "columna", ""), ("censura", "Columna de censura (opcional)", "columna_opc", "")], "Distribuciones")
def es_weibull(c, tabla, columna, censura):
    """Ajuste con censura e intervalos del 95 %, gráfica de probabilidad y función de supervivencia."""
    d = c.df(tabla)
    t = pd.to_numeric(d[columna], errors="coerce")
    v = t.notna() & (t > 0)
    cen = d.loc[v, censura].astype(bool).to_numpy() if censura else None
    x = t[v].to_numpy(dtype=float)
    r = es.weibull_fit(x, cen)
    c.kv("Weibull", [("β (forma)", r["beta"]), ("β: intervalo 95 %", f"{r['beta_ic'][0]:.3f} – {r['beta_ic'][1]:.3f}"), ("η (escala)", r["eta"]), ("η: intervalo 95 %", f"{r['eta_ic'][0]:.1f} – {r['eta_ic'][1]:.1f}"),
                     ("Fallos", r["fallos"]), ("Censurados", r["censurados"]), ("Vida media", mt.vida_media_weibull(r["beta"], r["eta"]))])
    if r["beta"] < 1:
        c.aviso("alerta", "β < 1: fallos tempranos, conviene revisar instalación y control de calidad.")
    elif r["beta"] > 1.5:
        c.aviso("bueno", "β > 1.5: fallos por desgaste; el mantenimiento preventivo es eficaz.")
    f = np.sort(x)
    pp = (np.arange(1, f.size + 1) - 0.3) / (f.size + 0.4)
    ser = [serie(np.log(f), np.log(-np.log(1 - pp)), "Datos", "puntos"), serie(np.log(f), r["beta"] * (np.log(f) - np.log(r["eta"])), "Ajuste")]
    c.grafica(figura("Papel Weibull", "ln t", "ln(-ln(1-F))", ser))
    ts = np.linspace(0, f.max(), 200)
    c.grafica(figura("Supervivencia R(t)", "t", "R(t)", [serie(ts, np.exp(-(ts / r["eta"]) ** r["beta"]), "R(t)")]))
    return c.informe("Análisis Weibull")


@herramienta("estadistica", "es_hipotesis", "Pruebas de hipótesis e intervalos", [
    ("tabla", "Tabla", "tabla", ""), ("columna", "Columna", "columna", ""), ("mu0", "Media de referencia", "num", 0.0), ("columna2", "Segunda columna (opcional)", "columna_opc", ""), ("conf", "Confianza", "num", 0.95)
], "Inferencia")
def es_hipotesis(c, tabla, columna, mu0, columna2, conf):
    """Prueba t (una o dos muestras), normalidad e intervalo de confianza de la media."""
    x = c.col(tabla, columna)
    ic = es.intervalo_media(x, conf)
    c.kv("Intervalo de confianza", [("Media", ic["media"]), (f"Límite inferior ({conf:.0%})", ic["inf"]), (f"Límite superior ({conf:.0%})", ic["sup"])])
    r = es.prueba_t(x, c.col(tabla, columna2) if columna2 else None, mu0)
    c.kv("Prueba t: " + r["tipo"], [("t", r["t"]), ("Grados de libertad", r["gl"]), ("p", r["p"]), ("¿Rechaza a α = 0.05?", r["p"] < 0.05)])
    n = es.prueba_normalidad(x)
    c.kv("Normalidad", [("Shapiro-Wilk W", n["shapiro_W"]), ("p", n["shapiro_p"]), ("Jarque-Bera", n["jb"]), ("p ", n["jb_p"])])
    return c.informe("Pruebas de hipótesis")


@herramienta("estadistica", "es_regresion", "Regresión lineal", [("tabla", "Tabla", "tabla", ""), ("y", "Variable Y", "columna", ""), ("x", "Variables X (separadas por coma)", "texto", "")], "Regresión")
def es_regresion(c, tabla, y, x):
    """Mínimos cuadrados ordinarios con errores, valores p, R² y residuos."""
    d = c.df(tabla)
    nombres = [a.strip() for a in x.split(",") if a.strip()]
    if not nombres:
        raise ValueError("Escribe al menos una columna X")
    sub = d[nombres + [y]].apply(pd.to_numeric, errors="coerce").dropna()
    r = es.regresion_lineal(sub[nombres].to_numpy(), sub[y].to_numpy(), nombres)
    c.kv("Ajuste", [("R²", r["r2"]), ("R² ajustado", r["r2_ajustado"]), ("RMSE", r["rmse"]), ("F", r["f"]), ("p (F)", r["f_p"]), ("n", r["n"])])
    c.tabla("Coeficientes", ["Término", "Valor", "Error", "t", "p"], [[a["termino"], a["valor"], a["error"], a["t"], a["p"]] for a in r["coeficientes"]])
    c.grafica(figura("Valores reales y ajustados", "Ajustado", "Real", [serie(r["ajuste"], sub[y], "Datos", "puntos"), serie([sub[y].min(), sub[y].max()], [sub[y].min(), sub[y].max()], "y = x")]))
    c.grafica(figura("Residuos", "Ajustado", "Residuo", [serie(r["ajuste"], r["residuos"], "Residuos", "puntos")]))
    return c.informe(f"Regresión de {y}")


@herramienta("estadistica", "es_no_lineal", "Regresión no lineal", [
    ("tabla", "Tabla", "tabla", ""), ("x", "Variable X", "columna", ""), ("y", "Variable Y", "columna", ""), ("expresion", "Modelo f(x, a, b, c)", "texto", "a*np.exp(-b*x)+c"), ("p0", "Valores iniciales", "texto", "1, 0.01, 0")
], "Regresión")
def es_no_lineal(c, tabla, x, y, expresion, p0):
    """Ajusta por Levenberg-Marquardt un modelo escrito como expresión en Python."""
    d = c.df(tabla)[[x, y]].apply(pd.to_numeric, errors="coerce").dropna().sort_values(x)
    ini = [float(a) for a in p0.split(",")]
    letras = "abcdefgh"[: len(ini)]
    f = eval(f"lambda x, {', '.join(letras)}: {expresion}", {"np": np, "math": math})
    r = es.regresion_no_lineal(f, d[x].to_numpy(), d[y].to_numpy(), ini)
    c.tabla("Parámetros", ["Parámetro", "Valor", "Error"], [[l, v, e] for l, v, e in zip(letras, r["parametros"], r["errores"])])
    c.kv("Calidad", [("RMSE", r["rmse"])])
    c.grafica(figura("Ajuste", x, y, [serie(d[x], d[y], "Datos", "puntos"), serie(d[x], f(d[x].to_numpy(), *r["parametros"]), "Modelo")]))
    return c.informe("Regresión no lineal")


@herramienta("estadistica", "es_anova", "ANOVA", [("tabla", "Tabla", "tabla", ""), ("y", "Variable", "columna", ""), ("a", "Factor A", "columna", ""), ("b", "Factor B (opcional)", "columna_opc", "")], "Regresión")
def es_anova(c, tabla, y, a, b):
    """ANOVA de uno o dos factores."""
    d = c.df(tabla)
    if b:
        t = es.anova_dos_factores(d[[y, a, b]].dropna(), y, a, b)
        c.tabla("ANOVA de dos factores", ["Fuente", "Suma de cuadrados", "gl", "F", "p"], t.values.tolist())
    else:
        gr = [pd.to_numeric(g[y], errors="coerce").dropna().to_numpy() for _, g in d.groupby(a)]
        r = es.anova_un_factor(*gr)
        c.kv("ANOVA de un factor", [("F", r["F"]), ("p", r["p"]), ("SC entre", r["sc_entre"]), ("gl entre", r["gl_entre"]), ("SC dentro", r["sc_dentro"]), ("gl dentro", r["gl_dentro"])])
        med = d.groupby(a)[y].agg(["count", "mean", "std"])
        c.tabla("Por grupo", [a, "n", "Media", "Desviación"], [[i, *f] for i, f in zip(med.index, med.values.tolist())])
        c.aviso("bueno" if r["p"] >= 0.05 else "alerta", "Las medias difieren (p < 0.05)." if r["p"] < 0.05 else "Sin diferencia significativa entre grupos.")
    return c.informe("ANOVA")


@herramienta("estadistica", "es_serie", "Descomposición y ARIMA", [
    ("tabla", "Tabla", "tabla", ""), ("columna", "Columna", "columna", ""), ("periodo", "Periodo estacional", "entero", 24), ("p", "p", "entero", 1), ("d", "d", "entero", 1), ("q", "q", "entero", 1), ("pasos", "Pasos de pronóstico", "entero", 48)
], "Series temporales")
def es_serie(c, tabla, columna, periodo, p, d, q, pasos):
    """Descomposición en tendencia, estacionalidad y residuo; ARIMA(p,d,q) con pronóstico e intervalos del 95 %."""
    x = c.col(tabla, columna)
    x = x[-5000:]
    r = es.descomposicion(x, periodo)
    k = np.arange(x.size)
    c.grafica(figura("Descomposición", "Muestra", columna, [serie(k, x, "Serie"), serie(k, r["tendencia"], "Tendencia"), serie(k, r["estacional"], "Estacional"), serie(k, r["residuo"], "Residuo")]))
    a = es.arima(x, (p, d, q), pasos)
    c.kv(f"ARIMA({p},{d},{q})", [(n, v) for n, v in a["parametros"].items()] + [("AIC", a["aic"]), ("BIC", a["bic"])])
    fut = np.arange(x.size, x.size + pasos)
    ult = max(0, x.size - 5 * pasos)
    c.grafica(figura("Pronóstico", "Muestra", columna, [serie(k[ult:], x[ult:], "Serie"), serie(fut, a["pronostico"], "Pronóstico"), serie(fut, a["inf"], "Inf 95 %"), serie(fut, a["sup"], "Sup 95 %")]))
    return c.informe("Series temporales")


@herramienta("estadistica", "es_spc", "Control estadístico de procesos", [
    ("tabla", "Tabla", "tabla", ""), ("columna", "Columna", "columna", ""), ("nsigma", "Límites n·σ", "num", 3.0), ("lam", "λ de EWMA", "num", 0.4), ("k", "k de CUSUM", "num", 0.5), ("h", "h de CUSUM", "num", 5.0)
], "Control de procesos")
def es_spc(c, tabla, columna, nsigma, lam, k, h):
    """Cartas de Shewhart (individuos), EWMA y CUSUM con los puntos fuera de control."""
    x = c.col(tabla, columna)[-5000:]
    i = np.arange(x.size)
    s = es.carta_shewhart(x, nsigma)
    c.grafica(figura("Carta de Shewhart", "Muestra", columna, [serie(i, x, columna), serie(np.array(s["fuera"]), x[s["fuera"]], "Fuera de control", "puntos", "#e5484d")], [s["centro"], s["lcs"], s["lci"]]))
    e = es.carta_ewma(x, lam, nsigma)
    c.grafica(figura("Carta EWMA", "Muestra", "z", [serie(i, e["z"], "EWMA"), serie(i, e["lcs"], "LCS"), serie(i, e["lci"], "LCI")], [e["centro"]]))
    u = es.carta_cusum(x, k, h)
    c.grafica(figura("Carta CUSUM", "Muestra", "Suma acumulada", [serie(i, u["c_mas"], "C+"), serie(i, u["c_menos"], "C-")], [u["limite"]]))
    c.kv("Resumen", [("Centro", s["centro"]), ("σ (rango móvil)", s["sigma"]), ("LCS", s["lcs"]), ("LCI", s["lci"]), ("Fuera en Shewhart", len(s["fuera"])), ("Fuera en EWMA", len(e["fuera"])), ("Alarmas en CUSUM", len(u["alarma"]))])
    return c.informe("Cartas de control")


# ================================================================= Módulo 6: aprendizaje automático


def _xy(c, tabla, x, y=None):
    d = c.df(tabla)
    nombres = [a.strip() for a in x.split(",") if a.strip()] or [n for n in d.select_dtypes("number").columns if n != y]
    cols = nombres + ([y] if y else [])
    sub = d[cols].dropna()
    return sub, nombres


@herramienta("ml", "ml_preparar", "Preparar datos", [
    ("tabla", "Tabla", "tabla", ""), ("salida", "Nombre de la tabla limpia", "texto", "limpios"), ("imputar", "Imputación", "opcion", "mediana", ["mediana", "media", "ultimo", "interpolar"]),
    ("columna_fecha", "Columna de fecha (opcional)", "columna_opc", ""), ("escalado", "Escalado", "opcion", "ninguno", ["ninguno", "estandar", "minmax", "robusto"])
], "Preprocesamiento")
def ml_preparar(c, tabla, salida, imputar, columna_fecha, escalado):
    """Quita duplicados e infinitos, imputa faltantes, añade variables temporales y escala."""
    d, inf = ml.limpiar(c.df(tabla))
    d = ml.imputar(d, imputar)
    if columna_fecha:
        d = ml.variables_temporales(d, columna_fecha)
    if escalado != "ninguno":
        num = d.select_dtypes("number").columns
        Z, _ = ml.escalar(d[num].to_numpy(), escalado)
        d[num] = Z
    c.ns[salida] = d
    c.kv("Resultado", [("Filas quitadas", inf["filas_quitadas"]), ("Faltantes rellenados", inf["nulos"]), ("Filas finales", len(d)), ("Columnas finales", d.shape[1])])
    c.aviso("bueno", f"Tabla «{salida}» creada en el espacio de trabajo.")
    return c.informe("Preparar datos")


@herramienta("ml", "ml_regresion", "Regresión", [
    ("tabla", "Tabla", "tabla", ""), ("y", "Objetivo", "columna", ""), ("x", "Variables (coma; vacío = todas)", "texto", ""), ("modelo", "Modelo", "opcion", "bosque", ml.MODELOS_REG), ("pliegues", "Pliegues de validación", "entero", 5), ("guardar", "Variable donde guardar", "texto", "modelo")
], "Supervisado")
def ml_regresion(c, tabla, y, x, modelo, pliegues, guardar):
    """Entrena con validación cruzada: R², RMSE y MAE fuera de muestra."""
    sub, nom = _xy(c, tabla, x, y)
    m, r, p = ml.entrenar_regresion(sub[nom].to_numpy(dtype=float), sub[y].to_numpy(dtype=float), modelo, pliegues)
    c.kv("Validación cruzada", [("Modelo", modelo), ("R²", r["r2"]), ("RMSE", r["rmse"]), ("MAE", r["mae"]), ("Pliegues", r["pliegues"])])
    if hasattr(m, "feature_importances_"):
        c.tabla("Importancia", ["Variable", "Importancia"], sorted(([n, float(v)] for n, v in zip(nom, m.feature_importances_)), key=lambda a: -a[1]))
    c.grafica(figura("Predicho frente a real", "Real", "Predicho", [serie(sub[y], p, "Validación", "puntos"), serie([sub[y].min(), sub[y].max()], [sub[y].min(), sub[y].max()], "y = x")]))
    if guardar:
        c.ns[guardar] = m
        c.aviso("bueno", f"Modelo guardado en la variable «{guardar}». Con «Guardar modelo» se lleva a un archivo.")
    return c.informe("Regresión")


@herramienta("ml", "ml_clasificacion", "Clasificación", [
    ("tabla", "Tabla", "tabla", ""), ("y", "Clase", "columna", ""), ("x", "Variables (coma; vacío = todas)", "texto", ""), ("modelo", "Modelo", "opcion", "bosque", ml.MODELOS_CLAS), ("pliegues", "Pliegues de validación", "entero", 5), ("guardar", "Variable donde guardar", "texto", "clasificador")
], "Supervisado")
def ml_clasificacion(c, tabla, y, x, modelo, pliegues, guardar):
    """Entrena con validación cruzada estratificada: exactitud, F1 y matriz de confusión."""
    sub, nom = _xy(c, tabla, x, y)
    m, r, _ = ml.entrenar_clasificacion(sub[nom].to_numpy(dtype=float), sub[y].to_numpy(), modelo, pliegues)
    c.kv("Validación cruzada", [("Modelo", modelo), ("Exactitud", r["exactitud"]), ("F1 macro", r["f1"]), ("Pliegues", r["pliegues"])])
    c.tabla("Matriz de confusión (filas = real)", ["Clase"] + r["clases"], [[a, *f] for a, f in zip(r["clases"], r["confusion"])])
    if guardar:
        c.ns[guardar] = m
    return c.informe("Clasificación")


@herramienta("ml", "ml_clustering", "Clustering", [("tabla", "Tabla", "tabla", ""), ("x", "Variables (coma; vacío = todas)", "texto", ""), ("k", "Número de grupos", "entero", 3)], "No supervisado")
def ml_clustering(c, tabla, x, k):
    """K-medias sobre datos estandarizados, con coeficiente de silueta."""
    sub, nom = _xy(c, tabla, x)
    et, r = ml.agrupar(sub[nom].to_numpy(dtype=float), k)
    c.kv("Resultado", [("Grupos", r["k"]), ("Silueta", r["silueta"]), ("Inercia", r["inercia"])])
    Z, inf = ml.pca(sub[nom].to_numpy(dtype=float), 2)
    c.grafica(figura("Grupos en las dos componentes principales", "PC1", "PC2", [serie(Z[et == g, 0], Z[et == g, 1], f"Grupo {g + 1}", "puntos") for g in range(k)]))
    c.tabla("Tamaño de cada grupo", ["Grupo", "n"], [[g + 1, int((et == g).sum())] for g in range(k)])
    return c.informe("Clustering")


@herramienta("ml", "ml_pca", "PCA", [("tabla", "Tabla", "tabla", ""), ("x", "Variables (coma; vacío = todas)", "texto", ""), ("n", "Componentes", "entero", 3)], "No supervisado")
def ml_pca(c, tabla, x, n):
    """Componentes principales de datos estandarizados y varianza explicada."""
    sub, nom = _xy(c, tabla, x)
    n = min(n, len(nom))
    Z, r = ml.pca(sub[nom].to_numpy(dtype=float), n)
    v = np.array(r["varianza_explicada"])
    c.tabla("Varianza explicada", ["Componente", "Fracción", "Acumulada"], [[f"PC{i + 1}", v[i], v[: i + 1].sum()] for i in range(n)])
    c.tabla("Cargas", ["Variable"] + [f"PC{i + 1}" for i in range(n)], [[nom[j]] + [r["componentes"][i][j] for i in range(n)] for j in range(len(nom))])
    if n >= 2:
        c.grafica(figura("Proyección", "PC1", "PC2", [serie(Z[:, 0], Z[:, 1], "Datos", "puntos")]))
    return c.informe("PCA")


@herramienta("ml", "ml_anomalias", "Detección de anomalías", [
    ("tabla", "Tabla", "tabla", ""), ("x", "Variables (coma; vacío = todas)", "texto", ""), ("metodo", "Método", "opcion", "Isolation Forest", ["Isolation Forest", "Residuos contra modelo"]),
    ("contaminacion", "Contaminación esperada", "num", 0.01), ("medido", "Columna medida (residuos)", "columna_opc", ""), ("esperado", "Columna esperada (residuos)", "columna_opc", ""), ("nsigma", "Umbral n·σ", "num", 3.0)
], "Anomalías")
def ml_anomalias(c, tabla, x, metodo, contaminacion, medido, esperado, nsigma):
    """Isolation Forest (100 árboles, 256 puntos) o residuos robustos contra un modelo físico."""
    if metodo == "Isolation Forest":
        sub, nom = _xy(c, tabla, x)
        a, s, _ = ml.bosque_aislamiento(sub[nom].to_numpy(dtype=float), contaminacion)
        c.kv("Resultado", [("Anomalías", int(a.sum())), ("Fracción", float(a.mean())), ("Umbral de puntuación", float(s[a].min()) if a.any() else float("nan"))])
        i = np.arange(s.size)
        c.grafica(figura("Puntuación de anomalía", "Muestra", "s(x)", [serie(i, s, "s(x)"), serie(i[a], s[a], "Anomalías", "puntos", "#e5484d")]))
    else:
        d = c.df(tabla)[[medido, esperado]].apply(pd.to_numeric, errors="coerce").dropna()
        r = ml.residuos_modelo(d[medido], d[esperado], nsigma)
        i = np.arange(len(d))
        an = r["anomalia"]
        c.kv("Resultado", [("Anomalías", int(an.sum())), ("Centro", r["centro"]), ("σ robusta", r["sigma"])])
        c.grafica(figura("Residuos", "Muestra", "medido - esperado", [serie(i, r["residuo"], "Residuo"), serie(i[an], r["residuo"][an], "Anomalías", "puntos", "#e5484d")], [r["centro"] + nsigma * r["sigma"], r["centro"] - nsigma * r["sigma"]]))
    return c.informe("Anomalías")


@herramienta("ml", "ml_guardar", "Guardar o cargar modelo", [
    ("accion", "Acción", "opcion", "guardar", ["guardar", "cargar"]), ("variable", "Variable del modelo", "texto", "modelo"), ("ruta", "Archivo .joblib", "ruta", "modelo.joblib")
], "Modelos")
def ml_guardar(c, accion, variable, ruta):
    """Guarda un modelo entrenado en un archivo o lo recupera al espacio de trabajo."""
    if accion == "guardar":
        ml.guardar_modelo(c.ns[variable], ruta, {"tipo": type(c.ns[variable]).__name__, "fecha": dt.datetime.now().isoformat()})
        c.aviso("bueno", f"Modelo guardado en {ruta}.")
    else:
        c.ns[variable] = ml.cargar_modelo(ruta)
        c.aviso("bueno", f"Modelo cargado en la variable «{variable}».")
    return c.informe("Modelos")


# ================================================================= Módulo 7: mantenimiento


@herramienta("mantenimiento", "mt_muestra", "Generar datos SCADA de muestra", [
    ("nombre", "Nombre de la tabla", "texto", "scada"), ("dias", "Días", "entero", 730), ("inversores", "Inversores", "entero", 2), ("p_nom", "Potencia por inversor [kW]", "num", 500.0),
    ("lat", "Latitud [°]", "num", 40.4), ("lon", "Longitud [°]", "num", -3.7)
], "Datos SCADA")
def mt_muestra(c, nombre, dias, inversores, p_nom, lat, lon):
    """Crea datos sintéticos con fallas conocidas para aprender el flujo de análisis."""
    d, v = mt.generar_scada(dias=dias, inversores=inversores, p_nom_kw=p_nom, lat=lat, lon=lon)
    c.ns[nombre] = d
    q = mt.resumen_calidad(d)
    c.kv("Datos generados", [("Filas", q["filas"]), ("Inversores", q["inversores"]), ("Desde", q["desde"]), ("Hasta", q["hasta"]), ("Paso", q["paso_min"], "min")])
    c.texto("Fallas inyectadas, para comprobar el análisis: degradación de 0.7 %/año, suciedad de 0.2 %/día con lluvias, "
            "el string 3 de INV-2 caído del día 200 al 215, sobretemperatura de IGBT en INV-1 del día 400 al 412, y paradas al azar.")
    return c.informe("Datos SCADA de muestra")


@herramienta("mantenimiento", "mt_importar", "Importar SCADA desde CSV o Parquet", [("ruta", "Archivo", "ruta", ""), ("nombre", "Nombre de la tabla", "texto", "scada")], "Datos SCADA")
def mt_importar(c, ruta, nombre):
    """Lee el archivo, reconoce alias de columnas (potencia, tensiones, corrientes, temperaturas, irradiancia, alarmas) y revisa la calidad."""
    d, q = mt.importar_scada(ruta)
    c.ns[nombre] = d
    c.kv("Calidad de los datos", [("Filas", q["filas"]), ("Inversores", q["inversores"]), ("Desde", q["desde"]), ("Hasta", q["hasta"]), ("Paso", q["paso_min"], "min"),
                                  ("Cobertura temporal", _pct(q["cobertura"])), ("Filas repetidas eliminadas", q["duplicados"])])
    c.texto("Columnas: " + ", ".join(q["columnas"]))
    for a in q["avisos"]:
        c.aviso("alerta", a)
    if q["huecos_total"]:
        c.aviso("alerta", f"Hay {q['huecos_total']} huecos en la serie temporal ({q['filas']} lecturas de {q['esperadas']} esperadas).")
        c.tabla("Huecos en los datos", ["Inversor", "Desde", "Hasta", "Lecturas perdidas"], [[h["inversor"], h["desde"], h["hasta"], h["perdidas"]] for h in q["huecos"]])
    if q["faltantes"]:
        c.tabla("Valores faltantes", ["Columna", "Faltantes"], [[a, b] for a, b in q["faltantes"].items()])
    return c.informe("Importación SCADA")


def _p_inv(d, p_nom):
    return float(p_nom)


@herramienta("mantenimiento", "mt_kpi", "KPIs de la planta", [("tabla", "Tabla SCADA", "tabla", "scada"), ("p_nom", "Potencia nominal por inversor [kW]", "num", 500.0), ("gamma", "Coef. de potencia [1/°C]", "num", -0.0037)], "KPIs")
def mt_kpi(c, tabla, p_nom, gamma):
    """Performance Ratio, disponibilidad, energía perdida y rendimiento del inversor."""
    d = c.df(tabla)
    mt.requerir_scada(d)
    tot, t = mt.kpis(d, p_nom, gamma)
    c.kv("Planta", [("Performance Ratio", _pct(tot["PR"])), ("Disponibilidad", _pct(tot["disponibilidad"])), ("Energía producida", tot["energia_kwh"] / 1000, "MWh"),
                   ("Energía perdida", tot["energia_perdida_kwh"] / 1000, "MWh"), ("Rendimiento del inversor", _pct(tot["eta_inversor"]))])
    c.tabla("Por inversor", ["Inversor", "PR", "Disponibilidad", "Energía [MWh]", "Perdida [MWh]", "Por paradas [MWh]", "η inversor"],
            [[f["inversor"], _pct(f["PR"]), _pct(f["disponibilidad"]), f["energia_kwh"] / 1000, f["energia_perdida_kwh"] / 1000, f["perdida_parada_kwh"] / 1000, _pct(f["eta_inversor"])] for _, f in t.iterrows()])
    ser = []
    for nombre, g in d.groupby("inversor"):
        r = mt.pr_diario(g, p_nom, gamma)
        ser.append(serie(np.asarray((r.index - r.index[0]).days, dtype=float), r.to_numpy(), str(nombre)))
    c.grafica(figura("Razón diaria medido/esperado", "Día", "Por unidad", ser))
    d2 = d.assign(_mes=d["fecha"].dt.to_period("M").astype(str)).groupby("_mes")
    h = mt._horas_paso(d)
    ni = d["inversor"].nunique()
    E = d2["p_ac_kw"].sum() * h / 1000
    H = d2["irradiancia_wm2"].sum() * h / 1000 / ni
    pr = E * 1000 / (p_nom * ni * H)
    x = np.arange(len(E))
    c.grafica(figura("Energía mensual", "Mes", "MWh", [serie(x, E.to_numpy(), "Energía", "barras")]))
    c.grafica(figura("PR mensual", "Mes", "PR", [serie(x, pr.to_numpy(), "PR")]))
    c.ns["kpi_inversores"] = t
    return c.informe("KPIs de la planta")


@herramienta("mantenimiento", "mt_fallas", "Detección de fallas", [
    ("tabla", "Tabla SCADA", "tabla", "scada"), ("p_nom", "Potencia nominal por inversor [kW]", "num", 500.0), ("gamma", "Coef. de potencia [1/°C]", "num", -0.0037), ("t_lim", "Límite de T de IGBT [°C]", "num", 85.0)
], "Fallas")
def mt_fallas(c, tabla, p_nom, gamma, t_lim):
    """Busca degradación de módulos, suciedad, strings caídos, sobretemperatura de IGBT y desviación del modelo físico."""
    d = c.df(tabla)
    mt.requerir_scada(d, aislamiento=True)
    filas = []
    ser_env = []
    for nombre, g in d.groupby("inversor"):
        try:
            dg = mt.estimar_degradacion(g, p_nom, gamma)
            filas.append(["Degradación", nombre, f"{100 * dg['tasa_anual']:.2f} %/año", f"método {dg['metodo']}", "alerta" if dg["tasa_anual"] > 0.01 else "normal"])
            ser_env.append(serie(np.asarray((dg["envolvente"].index - dg["envolvente"].index[0]).days, dtype=float), dg["envolvente"].to_numpy(), f"Envolvente {nombre}"))
        except ValueError as e:
            c.aviso("alerta", str(e))
        try:
            su = mt.estimar_suciedad(g, p_nom, gamma)
            filas.append(["Suciedad", nombre, f"{100 * su['tasa_dia']:.3f} %/día", f"pérdida media {100 * su['perdida_media']:.2f} %, {len(su['lavados'])} lavados", "alerta" if su["perdida_media"] > 0.03 else "normal"])
        except Exception as e:  # datos insuficientes
            c.aviso("alerta", f"Suciedad de {nombre}: {e}")
    if ser_env:
        c.grafica(figura("Envolvente del índice de rendimiento", "Día", "Por unidad", ser_env))
    st = mt.detectar_string_caido(d)
    for e in st:
        filas.append(["String caído", e["inversor"], e["string"], f"{e['desde']} a {e['hasta']} ({e['dias']} días), relación {e['relacion']:.2f}", "falla"])
    ig = mt.detectar_sobretemperatura(d, t_lim)
    for e in ig["eventos"]:
        filas.append(["Sobretemperatura IGBT", e["inversor"], f"+{e['exceso_c']:.1f} °C", f"{e['desde']} a {e['hasta']} ({e['dias']} días), máx. {e['t_max_c']:.0f} °C", "falla"])
    for nombre, g in d.groupby("inversor"):
        try:
            dv = mt.desviacion_modelo(g, p_nom, gamma)
        except Exception:
            continue
        al = dv["cusum"]["alarma"]
        filas.append(["Desviación del modelo", nombre, f"mediana {dv['razon'].median():.3f}", f"{len(al)} días con alarma CUSUM", "alerta" if al else "normal"])
    c.tabla("Hallazgos", ["Tipo", "Equipo", "Valor", "Detalle", "Estado"], filas)
    c.ns["hallazgos"] = pd.DataFrame(filas, columns=["tipo", "equipo", "valor", "detalle", "estado"])
    n = sum(1 for f in filas if f[4] == "falla")
    c.aviso("falla" if n else "bueno", f"{n} fallas activas que requieren una orden de trabajo." if n else "No hay fallas activas.")
    return c.informe("Detección de fallas")


@herramienta("mantenimiento", "mt_fiabilidad", "Fiabilidad y vida útil", [
    ("tabla", "Tabla SCADA", "tabla", "scada"), ("t_actual", "Horas de servicio actuales", "num", 500.0), ("umbral", "Límite de degradación (PR relativo)", "num", 0.8), ("p_nom", "Potencia nominal por inversor [kW]", "num", 500.0)
], "Fiabilidad")
def mt_fiabilidad(c, tabla, t_actual, umbral, p_nom):
    """MTBF, MTTR, disponibilidad intrínseca, Weibull de los tiempos entre fallos y vida remanente."""
    d = c.df(tabla)
    mt.requerir_scada(d)
    tbf, ttr, ev = mt.tiempos_de_eventos(d)
    f = mt.fiabilidad(tbf, ttr)
    c.kv("Mantenibilidad", [("MTBF", f["mtbf_h"], "h"), ("MTTR", f["mttr_h"], "h"), ("Disponibilidad intrínseca", _pct(f["disponibilidad"])), ("Fallos", f["fallos"])])
    if tbf.size >= 3:
        w = es.weibull_fit(tbf)
        rul = float(mt.vida_remanente_weibull(w["beta"], w["eta"], t_actual))
        rul10 = float(mt.vida_remanente_weibull(w["beta"], w["eta"], t_actual, 0.9))
        c.kv("Weibull de tiempos entre fallos", [("β", w["beta"]), ("η", w["eta"], "h"), ("Vida media", mt.vida_media_weibull(w["beta"], w["eta"]), "h"),
                                                 ("Vida remanente mediana", rul, "h"), ("Vida remanente con 90 % de fiabilidad", rul10, "h")])
        ts = np.linspace(0, tbf.max() * 1.2, 200)
        c.grafica(figura("Fiabilidad R(t)", "Horas", "R(t)", [serie(ts, np.exp(-(ts / w["eta"]) ** w["beta"]), "R(t)")]))
        h, e = np.histogram(tbf, 20)
        c.grafica(figura("Tiempos entre fallos", "Horas", "Frecuencia", [serie(0.5 * (e[:-1] + e[1:]), h, "n", "barras")]))
    filas = []
    for nombre, g in d.groupby("inversor"):
        try:
            dg = mt.estimar_degradacion(g, p_nom)
        except ValueError as e:
            c.aviso("alerta", f"Degradación de {nombre}: {e}")
            continue
        anos = np.log(umbral) / np.log(1 - max(dg["tasa_anual"], 1e-6))
        filas.append([nombre, f"{100 * dg['tasa_anual']:.2f} %/año", f"{anos:.1f} años"])
    if filas:
        c.tabla(f"Vida de los módulos hasta {umbral:.0%} de su potencia", ["Inversor", "Degradación", "Vida estimada"], filas)
    return c.informe("Fiabilidad")


# ================================================================= Control lineal
MODELO_LIN = "modelo linealizado (variable lin)"
_PLANTA = [("modelo", "Planta", "opcion", "coeficientes", ["coeficientes", MODELO_LIN]),
           ("num", "Numerador de G(s) (coeficientes)", "texto", "1"), ("den", "Denominador de G(s) (coeficientes)", "texto", "1, 3, 3, 1")]


def _planta(c, modelo, num, den):
    """G(s) desde coeficientes o desde el último modelo linealizado del diagrama (variable lin)."""
    from .lib import control as ct
    if modelo == MODELO_LIN:
        lin = c.ns.get("lin")
        if not isinstance(lin, dict) or "num" not in lin:
            raise ValueError("Aún no hay modelo linealizado: usa «Linealizar aquí» en el editor de bloques.")
        return ct.coeficientes(list(lin["num"])), ct.coeficientes(list(lin["den"]))
    return ct.coeficientes(num), ct.coeficientes(den)


def _polinomio(c):
    n = len(c) - 1
    partes = []
    for i, a in enumerate(c):
        p = n - i
        if a == 0:
            continue
        t = f"{a:.4g}" + ("" if p == 0 else ("·s" if p == 1 else f"·s^{p}"))
        partes.append(t)
    return " + ".join(partes).replace("+ -", "- ") or "0"


def _num_inf(v, unidad=""):
    return "∞" if not math.isfinite(v) else (f"{v:.4g} {unidad}".strip())


@herramienta("potencia", "ctl_bode", "Bode y márgenes de estabilidad", _PLANTA + [("k", "Ganancia del lazo K", "num", 1.0)], "Control")
def ctl_bode(c, modelo, num, den, k):
    """Diagrama de Bode de L(s) = K·G(s), con los márgenes de ganancia y de fase y sus frecuencias."""
    from .lib import control as ct
    n, d = _planta(c, modelo, num, den)
    n = k * n
    w, mag, fase = ct.bode(n, d)
    m = ct.margenes(n, d)
    c.texto(f"L(s) = ({_polinomio(n)}) / ({_polinomio(d)})")
    c.kv("Márgenes", [("Margen de ganancia", _num_inf(m["mg"]), f"{_num_inf(m['mg_db'], 'dB')}"),
                      ("Frecuencia de cruce de fase", _num_inf(m["w_mg"], "rad/s")),
                      ("Margen de fase", _num_inf(m["mf"], "°")),
                      ("Frecuencia de cruce de ganancia", _num_inf(m["w_mf"], "rad/s"))])
    T_num, T_den = ct.lazo_cerrado(n, d)
    est = ct.estable(T_den)
    c.aviso("bueno" if est else "falla", "El lazo cerrado con realimentación unitaria es estable." if est else "El lazo cerrado es INESTABLE.")
    c.grafica(figura("Magnitud", "ω [rad/s]", "|L| [dB]", [serie(w, mag, "|L(jω)|")], lineas_h=[0.0], log_x=True))
    c.grafica(figura("Fase", "ω [rad/s]", "∠L [°]", [serie(w, fase, "∠L(jω)")], lineas_h=[-180.0], log_x=True))
    c.ns["bode"] = pd.DataFrame({"w": w, "mag_db": mag, "fase_deg": fase})
    return c.informe("Bode y márgenes")


@herramienta("potencia", "ctl_escalon", "Respuesta al escalón y sus métricas", _PLANTA + [("lazo", "Sistema", "opcion", "abierto", ["abierto", "cerrado (realimentación unitaria)"])], "Control")
def ctl_escalon(c, modelo, num, den, lazo):
    """Sobreimpulso, tiempo de subida (10-90 %), de establecimiento (2 %), de pico y valor final."""
    from .lib import control as ct
    n, d = _planta(c, modelo, num, den)
    if lazo.startswith("cerrado"):
        n, d = ct.lazo_cerrado(n, d)
    t, y, m = ct.escalon(n, d)
    c.kv("Métricas", [("Valor final", m["final"]), ("Sobreimpulso", _pct(m["sobreimpulso"]) if math.isfinite(m["sobreimpulso"]) else "—"),
                      ("Tiempo de subida (10-90 %)", m["t_subida"], "s"), ("Tiempo de establecimiento (2 %)", _num_inf(m["t_establecimiento"]), "s"),
                      ("Tiempo de pico", m["t_pico"], "s"), ("Pico", m["pico"])])
    c.kv("Polos", [(f"p{i + 1}", f"{p.real:.5g}{p.imag:+.5g}j") for i, p in enumerate(np.roots(d))])
    c.grafica(figura("Respuesta al escalón", "t [s]", "y", [serie(t, y, "y(t)")], lineas_h=[m["final"]]))
    c.ns["escalon"] = pd.DataFrame({"t": t, "y": y})
    return c.informe("Respuesta al escalón")


@herramienta("potencia", "ctl_lugar", "Lugar de las raíces", _PLANTA + [("k_max", "Ganancia máxima", "num", 100.0)], "Control")
def ctl_lugar(c, modelo, num, den, k_max):
    """Polos de 1 + K·G(s) = 0 al variar K de 0 a K máx. La ganancia crítica es la que cruza el eje imaginario."""
    from .lib import control as ct
    n, d = _planta(c, modelo, num, den)
    ks = np.concatenate([[0.0], np.logspace(math.log10(k_max) - 5, math.log10(k_max), 800)])
    ks, ramas = ct.lugar_raices(n, d, ks)
    ser = [serie(ramas[:, j].real, ramas[:, j].imag, f"rama {j + 1}") for j in range(ramas.shape[1])]
    ser.append(serie(np.real(np.roots(d)), np.imag(np.roots(d)), "polos (K = 0)", "puntos"))
    if len(n) > 1:
        ser.append(serie(np.real(np.roots(n)), np.imag(np.roots(n)), "ceros", "puntos"))
    c.grafica(figura("Lugar de las raíces", "Re(s)", "Im(s)", ser))
    m = ct.margenes(n, d)
    if math.isfinite(m["mg"]):
        c.kv("Límite de estabilidad", [("Ganancia crítica", m["mg"]), ("Frecuencia de oscilación", m["w_mg"], "rad/s")])
    else:
        c.texto("El lazo cerrado no cruza el eje imaginario para ninguna ganancia positiva.")
    return c.informe("Lugar de las raíces")


@herramienta("potencia", "ctl_pi", "Ajuste automático de un PI", _PLANTA + [("mf", "Margen de fase deseado [°]", "num", 60.0),
                                                                          ("wc", "Frecuencia de cruce [rad/s] (0 = automática)", "num", 0.0)], "Control")
def ctl_pi(c, modelo, num, den, mf, wc):
    """C(s) = Kp·(1 + 1/(Ti·s)) que da el margen de fase pedido a la frecuencia de cruce, y su comprobación."""
    from .lib import control as ct
    n, d = _planta(c, modelo, num, den)
    r = ct.ajustar_pi(n, d, mf, wc if wc > 0 else None)
    Ln, Ld = r["L"]
    m = ct.margenes(Ln, Ld)
    c.kv("Controlador", [("Kp", r["Kp"]), ("Ti", r["Ti"], "s"), ("Ki = Kp/Ti", r["Ki"], "1/s"), ("Frecuencia de cruce", r["wc"], "rad/s")])
    c.kv("Comprobación", [("Margen de fase obtenido", _num_inf(m["mf"], "°")), ("Margen de ganancia", _num_inf(m["mg"]), _num_inf(m["mg_db"], "dB"))])
    Tn, Td = ct.lazo_cerrado(Ln, Ld)
    try:
        t, y, e = ct.escalon(Tn, Td)
        c.kv("Lazo cerrado", [("Sobreimpulso", _pct(e["sobreimpulso"])), ("Tiempo de establecimiento (2 %)", _num_inf(e["t_establecimiento"]), "s")])
        c.grafica(figura("Escalón en lazo cerrado", "t [s]", "y", [serie(t, y, "y(t)")], lineas_h=[1.0]))
    except ct.ErrorControl as ex:
        c.aviso("falla", str(ex))
    w, mag, fase = ct.bode(Ln, Ld)
    c.grafica(figura("Bode del lazo con el PI", "ω [rad/s]", "|L| [dB]", [serie(w, mag, "|L(jω)|")], lineas_h=[0.0], log_x=True))
    c.ns["pi_kp"], c.ns["pi_ti"] = r["Kp"], r["Ti"]
    return c.informe("Ajuste de un PI")


HERRAMIENTAS_MODULOS = {
    "potencia": "Potencia", "fv": "Fotovoltaica", "estadistica": "Estadística", "ml": "Aprendizaje automático", "mantenimiento": "Mantenimiento",
}
