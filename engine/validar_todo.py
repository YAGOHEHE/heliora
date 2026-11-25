"""Ejecuta toda la validación desde la línea de órdenes y resume el resultado."""
import sys
import time

from heliora.validacion import banco

# El resumen usa ✔ y ✘; en Windows, la salida redirigida usaría cp1252 y no podría escribirlos.
sys.stdout.reconfigure(encoding="utf-8")
t = time.time()
r = banco.ejecutar(4)
errores = banco.imprimir(r)
total = sum(len(f["pruebas"]) for f in r)
bien = sum(1 for f in r for p in f["pruebas"] if p["ok"])
print(f"\n{bien}/{total} pruebas en {time.time() - t:.0f} s")
sys.exit(0 if bien == total and all(f["ok"] for f in r) else 1)
