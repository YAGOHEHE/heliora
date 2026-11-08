"""Servidor del motor: atiende a la interfaz por entrada y salida estándar, con JSON por líneas.

Petición:  {"id": 1, "metodo": "evaluar", "params": {...}}
Respuesta: {"id": 1, "ok": true, "resultado": ...}   o   {"id": 1, "ok": false, "error": "..."}
Avance:    {"id": 1, "avance": 0.4, "mensaje": "..."}
"""
from __future__ import annotations

import json
import os
import queue
import sys
import threading
import traceback
import warnings

SALIDA = None
CANDADO = threading.Lock()
CANCELAR = threading.Event()


def enviar(obj):
    linea = json.dumps(obj, ensure_ascii=False, allow_nan=False, default=_conv)
    with CANDADO:
        SALIDA.write(linea + "\n")
        SALIDA.flush()


def _conv(o):
    import numpy as np
    if isinstance(o, (np.integer,)):
        return int(o)
    if isinstance(o, (np.floating,)):
        v = float(o)
        return v if np.isfinite(v) else None
    if isinstance(o, np.ndarray):
        return o.tolist()
    if isinstance(o, (np.bool_,)):
        return bool(o)
    return str(o)


def _limpiar(o):
    """Convierte NaN e infinitos en null (JSON no los admite)."""
    import math
    if isinstance(o, float):
        return o if math.isfinite(o) else None
    if isinstance(o, dict):
        return {k: _limpiar(v) for k, v in o.items()}
    if isinstance(o, (list, tuple)):
        return [_limpiar(v) for v in o]
    return o


def huella(obj):
    """SHA-256 de un objeto JSON en forma canónica (claves ordenadas, sin espacios)."""
    import hashlib
    texto = json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False, default=_conv)
    return hashlib.sha256(texto.encode("utf8")).hexdigest()


def procedencia(entrada=None, sha=None):
    """Lo necesario para reproducir un resultado: huella de la entrada y versiones del motor y librerías."""
    import platform
    from importlib import metadata
    vers = {}
    for nombre, paquete in (("numpy", "numpy"), ("scipy", "scipy"), ("pandas", "pandas"), ("sklearn", "scikit-learn"),
                            ("statsmodels", "statsmodels"), ("pvlib", "pvlib")):
        try:
            vers[nombre] = metadata.version(paquete)
        except metadata.PackageNotFoundError:
            vers[nombre] = None
    import datetime
    return {"sha256": sha if sha is not None else huella(entrada), "version": _version(), "python": platform.python_version(),
            "librerias": vers, "cuando": datetime.datetime.now().isoformat(timespec="seconds")}


class Motor:
    def __init__(self):
        from .espacio import Espacio
        self.espacio = Espacio()
        self.raiz = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    # ---------------------------------------------------------------- consola
    def evaluar(self, codigo):
        return self.espacio.evaluar(codigo)

    def ejecutar_script(self, codigo):
        return self.espacio.ejecutar_script(codigo)

    def variables(self):
        return self.espacio.variables()

    def ver_variable(self, nombre):
        return self.espacio.ver(nombre)

    def borrar_variable(self, nombre):
        self.espacio.ns.pop(nombre, None)
        return self.espacio.variables()

    def limpiar_espacio(self):
        self.espacio.reiniciar()
        return []

    def guardar_variable(self, nombre, ruta):
        self.espacio.guardar(self.espacio.ns[nombre], ruta)
        return ruta

    def importar_datos(self, ruta, nombre):
        self.espacio.cargar(ruta, nombre)
        return self.espacio.variables()

    def guardar_espacio(self, carpeta):
        """Guarda en una carpeta las tablas (Parquet) y matrices (CSV) del espacio de trabajo."""
        import numpy as np
        import pandas as pd
        os.makedirs(carpeta, exist_ok=True)
        hechos = []
        for k, v in self.espacio.ns.items():
            if k in self.espacio.base or k.startswith("_"):
                continue
            if isinstance(v, pd.DataFrame):
                d = v.copy()
                d.columns = [str(c) for c in d.columns]
                d.to_parquet(os.path.join(carpeta, k + ".parquet"))
                hechos.append(k + ".parquet")
            elif isinstance(v, np.ndarray) and np.issubdtype(v.dtype, np.number) and v.ndim <= 2:
                pd.DataFrame(v).to_csv(os.path.join(carpeta, k + ".csv"), index=False)
                hechos.append(k + ".csv")
        return hechos

    def cargar_espacio(self, carpeta):
        """Carga todas las tablas de una carpeta de datos del proyecto."""
        for a in sorted(os.listdir(carpeta)):
            r = os.path.join(carpeta, a)
            n, ext = os.path.splitext(a)
            if ext.lower() in (".csv", ".parquet"):
                self.espacio.cargar(r, n)
        return self.espacio.variables()

    # ---------------------------------------------------------------- bloques
    def catalogo(self):
        from .sim import diagrama
        from .sim.nucleo import REGISTRO
        diagrama.cargar_librerias()
        return [c.describir() for k, c in REGISTRO.items() if not k.startswith("_")]

    def ejemplos(self):
        from .validacion import ejemplos
        return ejemplos.listar()

    def ejemplo(self, clave):
        from .validacion import ejemplos
        return ejemplos.obtener(clave)

    def puertos(self, tipo, params):
        from .sim import diagrama
        from .sim.nucleo import REGISTRO
        diagrama.cargar_librerias()
        return REGISTRO[tipo].puertos(params)

    def simular(self, diagrama, _informe=None):
        from .espacio import reducir
        from .sim.diagrama import simular
        CANCELAR.clear()
        r = simular(diagrama, self.espacio.ns, cancelar=CANCELAR.is_set, informe=_informe)
        figuras = []
        for s in r["senales"]:
            t = s["t"]
            trazas = []
            for j, tr in enumerate(s["trazas"]):
                base = s["nombres"][j] if j < len(s["nombres"]) else f"entrada {j + 1}"
                if tr.ndim == 1:
                    x, y = reducir(t, tr)
                    trazas.append({"nombre": base, "x": x.tolist(), "y": y.tolist()})
                else:
                    for k in range(tr.shape[1]):
                        x, y = reducir(t, tr[:, k])
                        trazas.append({"nombre": f"{base} ({'abc'[k] if tr.shape[1] == 3 else k + 1})", "x": x.tolist(), "y": y.tolist()})
            figuras.append({"bloque": s["bloque"], "etiqueta": s["etiqueta"], "trazas": trazas})
        import numpy as np
        for nombre, v in r["exportados"].items():
            self.espacio.ns[nombre] = np.column_stack([v["t"], v["y"]])
        # Último resultado completo, para analizarlo en la consola
        self.espacio.ns["ultima_simulacion"] = {s["etiqueta"]: {"t": s["t"], "y": s["trazas"]} for s in r["senales"]}
        return {"figuras": figuras, "estadisticas": r["estadisticas"], "avisos": r["avisos"], "lazos": r.get("lazos", []),
                "procedencia": procedencia(diagrama), "variables": self.espacio.variables()}

    def barrido(self, diagrama, parametros, metricas=None, n_jobs=-1, max_combinaciones=2000, _informe=None):
        """Barrido paramétrico en paralelo: simula el diagrama para cada combinación de valores.

        `parametros` asigna a cada destino una lista de valores; se recorre el producto
        cartesiano. Un destino es «solver.<clave>» (p. ej. «solver.paso») o «<id_bloque>.<parám>»
        (p. ej. «boost.D»). Por cada combinación se devuelven las estadísticas y una reducción
        (final, media, máx, mín, rms) de cada señal de osciloscopio; `metricas`, si se da, filtra
        las señales cuyo nombre contenga alguna de esas subcadenas. El cálculo es por procesos
        (joblib), así que escala con los núcleos. Un error en una combinación no detiene el resto.
        """
        import itertools
        import time
        from joblib import Parallel, delayed
        if not parametros:
            raise ValueError("Da al menos un parámetro con una lista de valores para barrer.")
        destinos = list(parametros)
        listas = [list(parametros[k]) for k in destinos]
        combos = [dict(zip(destinos, vals)) for vals in itertools.product(*listas)]
        if len(combos) > int(max_combinaciones):
            raise ValueError(f"El barrido tiene {len(combos)} combinaciones (> {max_combinaciones}). "
                             f"Reduce los valores o sube max_combinaciones.")
        esp = _espacio_picklable(self.espacio.ns, getattr(self.espacio, "base", ()))
        t0 = time.perf_counter()
        resultados = Parallel(n_jobs=n_jobs)(
            delayed(_barrido_corrida)(diagrama, ov, esp, metricas) for ov in combos)
        if _informe:
            _informe(1.0, "barrido terminado")
        n_ok = sum(1 for r in resultados if r.get("error") is None)
        return {
            "combinaciones": len(combos), "exitosas": n_ok, "fallidas": len(combos) - n_ok,
            "segundos": time.perf_counter() - t0, "n_jobs": n_jobs, "destinos": destinos,
            "resultados": resultados,
        }

    def linealizar(self, diagrama, entradas=None, salidas=None, punto=None):
        """Modelo lineal (A, B, C, D) del diagrama en un punto de operación, con su Bode y sus polos.

        También deja en la consola la variable `lin`, que usan las herramientas de control.
        """
        import numpy as np

        from . import herramientas
        from .sim.linealizar import funcion_transferencia, linealizar
        CANCELAR.clear()
        r = linealizar(diagrama, self.espacio.ns, entradas, salidas, punto, cancelar=CANCELAR.is_set)
        num, den = funcion_transferencia(r)
        polos = np.roots(den) if len(den) > 1 else np.array([])
        ceros = np.roots(num) if len(num) > 1 else np.array([])
        self.espacio.ns["lin"] = {"A": r["A"], "B": r["B"], "C": r["C"], "D": r["D"], "num": num, "den": den,
                                  "estados": r["estados"], "entradas": r["entradas"], "salidas": r["salidas"], "punto": r["punto"]}
        # Informe: punto, función de transferencia, Bode y respuesta al escalón con las herramientas de control
        # Primero G(s) y su Bode (lo que se viene a ver), después el punto, los márgenes y el resto
        secciones = [{"tipo": "texto", "texto": f"G(s) = ({herramientas._polinomio(num)}) / ({herramientas._polinomio(den)})"}]
        modelo = herramientas.MODELO_LIN
        resto = []
        for ident in ("ctl_bode", "ctl_escalon"):
            try:
                inf = herramientas.ejecutar(ident, {"modelo": modelo}, self.espacio)
                graficas = [s for s in inf["secciones"] if s["tipo"] == "grafica"]
                otras = [s for s in inf["secciones"] if s["tipo"] in ("kv", "aviso")]
                if ident == "ctl_bode":
                    secciones.extend(graficas)
                    resto.extend(otras)
                else:
                    resto.extend(otras + graficas)
            except Exception as e:  # p. ej. un modelo inestable no tiene respuesta al escalón
                resto.append({"tipo": "aviso", "nivel": "alerta", "texto": str(e)})
        p = r["punto"]
        secciones.append({"tipo": "kv", "titulo": "Punto de operación", "items": [
            ["Instante", herramientas._fmt(p["t"]), "s"], ["Cómo se obtuvo", p["metodo"], ""],
            ["Entrada", ", ".join(r["entradas"]) + f" = {herramientas._fmt(p['u'][0])}", ""],
            ["Salida", ", ".join(r["salidas"]) + f" = {herramientas._fmt(p['y'][0])}" if len(p["y"]) else "—", ""],
            ["Residuo |f(x*, u*)|", herramientas._fmt(p["residuo_f"]), ""], ["Estados", str(len(r["estados"])), ""]]})
        secciones.append({"tipo": "tabla", "titulo": "Polos y ceros", "columnas": ["Tipo", "Real", "Imaginaria"],
                          "filas": [["polo", herramientas._fmt(z.real), herramientas._fmt(z.imag)] for z in polos] +
                                   [["cero", herramientas._fmt(z.real), herramientas._fmt(z.imag)] for z in ceros]})
        secciones.extend(resto)
        if polos.size or ceros.size:
            from .espacio import figura, serie
            ser = [serie(polos.real, polos.imag, "polos", "puntos")]
            if ceros.size:
                ser.append(serie(ceros.real, ceros.imag, "ceros", "puntos"))
            secciones.append({"tipo": "grafica", "figura": figura("Polos y ceros del modelo linealizado", "Re(s)", "Im(s)", ser)})
        return {"A": r["A"], "B": r["B"], "C": r["C"], "D": r["D"], "estados": r["estados"], "entradas": r["entradas"],
                "salidas": r["salidas"], "punto": r["punto"], "num": num, "den": den,
                "informe": {"titulo": "Modelo linealizado", "secciones": secciones},
                "procedencia": procedencia(diagrama), "variables": self.espacio.variables()}

    def cancelar(self):
        CANCELAR.set()
        return True

    # ---------------------------------------------------------------- código de los bloques
    def leer_archivo(self, ruta):
        with open(ruta, encoding="utf8") as f:
            return f.read()

    def escribir_archivo(self, ruta, texto):
        with open(ruta, "w", encoding="utf8") as f:
            f.write(texto)
        return True

    def recargar_librerias(self):
        """Vuelve a leer el código de los bloques tras editarlo."""
        import importlib

        from .lib import electrica, fotovoltaica, potencia
        from .sim import bloques_genericos, circuito, nucleo
        from .sim import diagrama as dg
        nucleo.REGISTRO.clear()
        for m in (nucleo, circuito, bloques_genericos, electrica, potencia, fotovoltaica, dg):
            importlib.reload(m)
        return self.catalogo()

    # ---------------------------------------------------------------- herramientas
    def herramientas(self):
        from . import herramientas
        return herramientas.listar()

    def ejecutar_herramienta(self, ident, valores):
        from . import herramientas
        return herramientas.ejecutar(ident, valores, self.espacio)

    # ---------------------------------------------------------------- validación
    def pruebas(self):
        """Catálogo del banco: cada fase con sus pruebas (el mismo conjunto que ejecuta validar)."""
        from .validacion import banco
        f = banco.fases()
        return {"total": sum(x["total"] for x in f), "fases": f}

    def validar(self, hasta_fase=4, _informe=None):
        from .validacion import banco
        CANCELAR.clear()
        import datetime
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            fases = banco.ejecutar(int(hasta_fase), CANCELAR.is_set, lambda f, n: _informe(f, n) if _informe else None)
        r = {"cuando": datetime.datetime.now().strftime("%Y-%m-%d %H:%M"), "fases": _limpiar(fases),
             "procedencia": procedencia(sha=banco.huella_pruebas())}
        # Se guarda para mostrarlo sin repetir las pruebas la próxima vez
        try:
            with open(os.path.join(self.raiz, "ultima_validacion.json"), "w", encoding="utf8") as f:
                json.dump(r, f, ensure_ascii=False, indent=1)
        except OSError:
            pass
        return r

    # ---------------------------------------------------------------- mantenimiento
    def plan_inicial(self, hoy, hasta):
        from .lib import mantenimiento as mt
        e = mt.estado_inicial(hoy)
        mt.generar_ordenes(e, hasta)
        return {"estado": e, "tablero": mt.tablero(e, hoy)}

    def plan_generar(self, estado, hasta, hoy=None):
        from .lib import mantenimiento as mt
        n = mt.generar_ordenes(estado, hasta)
        return {"estado": estado, "nuevas": n, "tablero": mt.tablero(estado, hoy)}

    def plan_tablero(self, estado, hoy=None):
        from .lib import mantenimiento as mt
        return mt.tablero(estado, hoy)

    def plan_cerrar(self, estado, ident, fecha=None, notas="", hoy=None):
        from .lib import mantenimiento as mt
        mt.cerrar_orden(estado, ident, fecha, notas)
        return {"estado": estado, "tablero": mt.tablero(estado, hoy)}

    def plan_correctivas(self, estado, hoy=None):
        """Crea órdenes correctivas a partir de la tabla de hallazgos."""
        from .lib import mantenimiento as mt
        h = self.espacio.ns.get("hallazgos")
        if h is None:
            raise ValueError("Primero ejecuta «Detección de fallas».")
        ev = [{"trabajo": f"{f['tipo']}: {f['valor']}", "equipo": str(f["equipo"]), "notas": str(f["detalle"])}
              for _, f in h.iterrows() if f["estado"] == "falla"]
        n = mt.ordenes_de_alarmas(estado, ev, hoy)
        return {"estado": estado, "nuevas": n, "tablero": mt.tablero(estado, hoy)}

    def plan_guardar(self, estado, ruta):
        from .lib import mantenimiento as mt
        mt.guardar_estado(estado, ruta)
        return ruta

    def plan_cargar(self, ruta, hoy=None):
        from .lib import mantenimiento as mt
        e = mt.cargar_estado(ruta)
        return {"estado": e, "tablero": mt.tablero(e, hoy)}

    def informe_om(self, estado, carpeta, hoy=None, planta="Planta fotovoltaica"):
        """Informe de O&M en HTML y PDF con los hallazgos y KPIs de la sesión, si los hay."""
        from .lib import informe
        return informe.escribir(estado, carpeta, hoy, self.espacio.ns.get("hallazgos"), self.espacio.ns.get("kpi_inversores"), planta)

    def amef_rpn(self, estado, tipo=None):
        from .lib import mantenimiento as mt
        return mt.amef_por_equipo(estado, tipo)


# Petición en curso: la cancelación necesita saber cuál es y en qué hilo corre
EN_CURSO = {"id": None, "hilo": None}
CANDADO_CURSO = threading.Lock()
ESPERA_INTERRUPCION = 0.3     # s: margen para una cancelación cooperativa antes de interrumpir
SOLO_LECTURA = {"variables", "ver_variable"}


def _interrumpir(ident):
    """Lanza KeyboardInterrupt en el hilo de trabajo si sigue con la misma petición.

    Corta un bucle de Python (`while True: pass`). Una llamada bloqueada en C (time.sleep,
    una factorización enorme) no lo recibe hasta que vuelve; para eso la interfaz cierra el
    proceso y lo vuelve a abrir si no hay respuesta en unos segundos.
    """
    import ctypes
    with CANDADO_CURSO:
        if EN_CURSO["id"] != ident or EN_CURSO["hilo"] is None:
            return
        ctypes.pythonapi.PyThreadState_SetAsyncExc(ctypes.c_ulong(EN_CURSO["hilo"]), ctypes.py_object(KeyboardInterrupt))


def _cancelar():
    CANCELAR.set()
    with CANDADO_CURSO:
        ident = EN_CURSO["id"]
    if ident is not None:
        t = threading.Timer(ESPERA_INTERRUPCION, _interrumpir, args=(ident,))
        t.daemon = True
        t.start()


def _trabajador(motor, cola):
    """Atiende las peticiones de una en una; la cancelación se atiende aparte."""
    while True:
        try:
            _atender(motor, cola)
            return
        except KeyboardInterrupt:
            # Una interrupción que llegó justo al terminar una petición: se sigue atendiendo
            continue


def _atender(motor, cola):
    while True:
        with CANDADO_CURSO:
            EN_CURSO["id"] = None
        pet = cola.get()
        if pet is None:
            return
        ident = pet.get("id")
        with CANDADO_CURSO:
            EN_CURSO["id"] = ident
            EN_CURSO["hilo"] = threading.get_ident()
        try:
            fn = getattr(motor, pet["metodo"])
            params = dict(pet.get("params") or {})
            ultimo = [0.0]

            def informe(fr, mensaje=""):
                # Sin saturar la interfaz: como mucho, un aviso por cada 1 %
                if fr - ultimo[0] >= 0.01 or mensaje:
                    ultimo[0] = fr
                    enviar({"id": ident, "avance": float(fr), "mensaje": mensaje})

            if "_informe" in fn.__code__.co_varnames:
                params["_informe"] = informe
            res = fn(**params)
            with CANDADO_CURSO:
                EN_CURSO["id"] = None
            enviar({"id": ident, "ok": True, "resultado": _limpiar(res)})
        except BaseException as e:  # el error se muestra al usuario, el motor sigue vivo
            with CANDADO_CURSO:
                EN_CURSO["id"] = None
            detalle = traceback.format_exc(limit=6)
            nombre = type(e).__name__
            if nombre == "Cancelado" or isinstance(e, KeyboardInterrupt):
                enviar({"id": ident, "ok": False, "error": "Detenido por el usuario", "cancelado": True})
            else:
                print(detalle, file=sys.stderr, flush=True)
                enviar({"id": ident, "ok": False, "error": str(e) or nombre})


def principal():
    global SALIDA
    SALIDA = sys.stdout
    # Lo que un modelo imprima no debe mezclarse con el protocolo
    sys.stdout = sys.stderr
    warnings.filterwarnings("ignore", category=FutureWarning)
    enviar({"evento": "iniciando"})
    motor = Motor()
    cola = queue.Queue()
    hilo = threading.Thread(target=_trabajador, args=(motor, cola), daemon=True)
    hilo.start()
    enviar({"evento": "listo", "version": _version()})
    for linea in sys.stdin:
        linea = linea.strip()
        if not linea:
            continue
        try:
            pet = json.loads(linea)
        except ValueError:
            continue
        metodo = pet.get("metodo")
        if metodo == "cancelar":
            _cancelar()
            enviar({"id": pet.get("id"), "ok": True, "resultado": True})
        elif metodo in SOLO_LECTURA and EN_CURSO["id"] is not None:
            # Mientras el motor trabaja, mirar variables no espera en la cola
            try:
                fn = getattr(motor, metodo)
                enviar({"id": pet.get("id"), "ok": True, "resultado": _limpiar(fn(**dict(pet.get("params") or {})))})
            except Exception as e:
                enviar({"id": pet.get("id"), "ok": False, "error": f"No se pudo leer mientras el motor trabaja: {e}"})
        else:
            cola.put(pet)


def _version():
    from . import __version__
    return __version__


# ------------------------------------------------------------ barrido paramétrico (joblib)

def _fijar_parametro(d, destino, valor):
    """Fija «solver.<clave>» o «<id_bloque>.<parámetro>» en el diagrama (que ya es una copia)."""
    cabeza, _, resto = destino.partition(".")
    if not resto:
        raise ValueError(f"Destino «{destino}» mal escrito: usa «solver.clave» o «bloque.parámetro».")
    if cabeza == "solver":
        d.setdefault("solver", {})[resto] = valor
        return

    def buscar(sistema):
        for b in sistema.get("bloques", []):
            if b.get("id") == cabeza:
                b.setdefault("params", {})[resto] = valor
                return True
            if b.get("tipo") == "subsistema" and b.get("sistema") and buscar(b["sistema"]):
                return True
        return False

    if not buscar(d.get("sistema", d)):
        raise ValueError(f"No encontré el destino «{destino}» (no hay un bloque «{cabeza}»).")


def _espacio_picklable(ns, base=()):
    """Subconjunto del espacio de trabajo que se puede enviar a los procesos de trabajo."""
    import numpy as np
    base = set(base or ())
    ok = (int, float, complex, bool, str, list, tuple, np.ndarray)
    return {k: v for k, v in ns.items()
            if k not in base and not str(k).startswith("_") and isinstance(v, ok)}


def _barrido_corrida(diagrama, overrides, espacio, metricas):
    """Una simulación del barrido (en un proceso aparte). Devuelve solo escalares picklables."""
    import copy
    import numpy as np
    from .sim.diagrama import simular
    try:
        d = copy.deepcopy(diagrama)
        for destino, valor in overrides.items():
            _fijar_parametro(d, destino, valor)
        r = simular(d, dict(espacio))
        est = r["estadisticas"]
        senales = {}
        for s in r["senales"]:
            nombres = s.get("nombres", [])
            for j, tr in enumerate(s["trazas"]):
                tr = np.asarray(tr, dtype=float)
                if tr.ndim != 1 or tr.size == 0:
                    continue
                etq = nombres[j] if j < len(nombres) else f"{j + 1}"
                nombre = f"{s['etiqueta']}:{etq}"
                if metricas and not any(str(m).lower() in nombre.lower() for m in metricas):
                    continue
                senales[nombre] = {
                    "final": float(tr[-1]), "media": float(tr.mean()),
                    "max": float(tr.max()), "min": float(tr.min()),
                    "rms": float(np.sqrt(np.mean(tr * tr))),
                }
        return {
            "parametros": overrides,
            "estadisticas": {k: est.get(k) for k in ("pasos", "evaluaciones", "rechazos", "segundos", "lazos")},
            "avisos": r.get("avisos", []), "senales": senales, "error": None,
        }
    except Exception as e:  # un fallo en una combinación no debe tumbar el barrido
        return {"parametros": overrides, "estadisticas": {}, "senales": {}, "avisos": [], "error": str(e)}


if __name__ == "__main__":
    principal()
