"""Banco de pruebas por fases.

Cada prueba compara el motor contra una referencia independiente (solución analítica o
hoja de datos) y registra el error numérico. Las fases se ejecutan en orden: si una fase
falla, las siguientes quedan bloqueadas y no se ejecutan.
"""
from __future__ import annotations

import math
import time
import traceback

PRUEBAS = []

FASES = {
    1: "Fase 1: cálculo y solvers",
    2: "Fase 2: potencia y fotovoltaica",
    3: "Fase 3: estadística y ML",
    4: "Fase 4: mantenimiento",
}


def prueba(fase, nombre, referencia=""):
    """Decorador: registra una función que recibe `m` (medidor) y hace comprobaciones."""
    def deco(fn):
        PRUEBAS.append({"fase": fase, "nombre": nombre, "referencia": referencia, "fn": fn,
                        "doc": (fn.__doc__ or "").strip()})
        return fn
    return deco


class Medidor:
    def __init__(self):
        self.medidas = []

    def error(self, nombre, valor, ref, tol, unidad="", relativo=False):
        """Comprueba |valor - ref| <= tol (o el error relativo si relativo=True)."""
        valor, ref = float(valor), float(ref)
        err = abs(valor - ref)
        if relativo:
            err = err / abs(ref) if ref != 0 else err
        ok = math.isfinite(err) and err <= tol
        self.medidas.append({"nombre": nombre, "valor": valor, "referencia": ref, "error": err,
                             "tolerancia": tol, "unidad": unidad, "relativo": relativo, "ok": ok})
        return ok

    def cota(self, nombre, valor, tol, unidad=""):
        """Comprueba que una magnitud de error directa sea <= tol."""
        valor = float(valor)
        ok = math.isfinite(valor) and abs(valor) <= tol
        self.medidas.append({"nombre": nombre, "valor": valor, "referencia": 0.0, "error": abs(valor),
                             "tolerancia": tol, "unidad": unidad, "relativo": False, "ok": ok})
        return ok

    def verdad(self, nombre, cond, detalle=""):
        self.medidas.append({"nombre": nombre, "valor": 1.0 if cond else 0.0, "referencia": 1.0,
                             "error": 0.0 if cond else 1.0, "tolerancia": 0.0, "unidad": detalle,
                             "relativo": False, "ok": bool(cond)})
        return bool(cond)


def _cargar():
    from . import pruebas  # noqa: F401 (registra las pruebas)


def _ficha(p):
    return {"fase": p["fase"], "titulo_fase": FASES.get(p["fase"], f"Fase {p['fase']}"), "nombre": p["nombre"],
            "referencia": p["referencia"], "doc": p["doc"]}


def listar():
    """Todas las pruebas registradas, en el orden en que se ejecutan (por fase).

    Es la misma lista que recorre `ejecutar`: len(listar()) es el número real de pruebas.
    Cada prueba dice su fase y el título de la fase; `fases()` las da agrupadas.
    """
    _cargar()
    return [_ficha(p) for f in sorted(FASES) for p in PRUEBAS if p["fase"] == f]


def fases():
    """Cada fase con sus pruebas y su número."""
    lista = listar()
    return [{"fase": f, "titulo": FASES[f], "total": sum(1 for p in lista if p["fase"] == f),
             "pruebas": [p for p in lista if p["fase"] == f]} for f in sorted(FASES)]


def huella_pruebas():
    """SHA-256 del código de las pruebas: identifica qué banco produjo un resultado."""
    import hashlib
    import os
    h = hashlib.sha256()
    carpeta = os.path.dirname(os.path.abspath(__file__))
    for nombre in sorted(os.listdir(carpeta)):
        if nombre.endswith(".py"):
            with open(os.path.join(carpeta, nombre), "rb") as f:
                h.update(nombre.encode() + b"\0" + f.read())
    return h.hexdigest()


def ejecutar(hasta_fase=4, cancelar=None, informe=None):
    """Ejecuta las fases en orden; una fase con fallos bloquea las siguientes."""
    _cargar()
    salida = []
    bloqueada = None
    total = sum(1 for p in PRUEBAS if p["fase"] <= hasta_fase)
    hechas = 0
    for fase in sorted(FASES):
        if fase > hasta_fase:
            break
        grupo = [p for p in PRUEBAS if p["fase"] == fase]
        res_fase = {"fase": fase, "titulo": FASES[fase], "total": len(grupo), "pruebas": [], "bloqueada": bloqueada is not None}
        if bloqueada is not None:
            res_fase["motivo"] = f"Bloqueada: la fase {bloqueada} tiene pruebas sin pasar"
            res_fase["ok"] = False
            # Se listan igual, sin ejecutar, para que el recuento coincida con el catálogo
            res_fase["pruebas"] = [{"nombre": p["nombre"], "referencia": p["referencia"], "doc": p["doc"], "ok": False,
                                    "ejecutada": False, "medidas": [], "excepcion": None, "segundos": 0.0} for p in grupo]
            salida.append(res_fase)
            continue
        ok_fase = True
        for p in grupo:
            if cancelar and cancelar():
                raise KeyboardInterrupt
            m = Medidor()
            t0 = time.perf_counter()
            error = None
            try:
                p["fn"](m)
            except Exception:
                error = traceback.format_exc(limit=4)
            ok = error is None and bool(m.medidas) and all(x["ok"] for x in m.medidas)
            ok_fase &= ok
            res_fase["pruebas"].append({
                "nombre": p["nombre"], "referencia": p["referencia"], "doc": p["doc"], "ok": ok,
                "medidas": m.medidas, "excepcion": error, "segundos": time.perf_counter() - t0})
            hechas += 1
            if informe:
                informe(hechas / max(total, 1), p["nombre"])
        res_fase["ok"] = ok_fase
        if not ok_fase:
            bloqueada = fase
        salida.append(res_fase)
    return salida


def imprimir(resultado):
    """Resumen de texto, para ejecutar desde la línea de órdenes."""
    errores = 0
    for f in resultado:
        print(f"\n== {f['titulo']}: {'OK' if f['ok'] else ('BLOQUEADA' if f['bloqueada'] else 'FALLA')}")
        for p in f["pruebas"]:
            print(f" {'✔' if p['ok'] else '✘'} {p['nombre']} ({p['segundos']:.2f} s)")
            for m in p["medidas"]:
                marca = " " if m["ok"] else "!"
                ud = m["unidad"]
                tipo = "rel" if m["relativo"] else "abs"
                print(f"   {marca} {m['nombre']}: valor={m['valor']:.6g} ref={m['referencia']:.6g} "
                      f"err({tipo})={m['error']:.3e} tol={m['tolerancia']:.1e} {ud}")
            if p["excepcion"]:
                print(p["excepcion"])
                errores += 1
    return errores
