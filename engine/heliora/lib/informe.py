"""Informe de operación y mantenimiento: indicadores, hallazgos, órdenes y calendario de 30 días.

Se escribe en HTML (con gráficas SVG dentro del propio archivo) y en PDF. El PDF lo genera un
escritor mínimo propio (PDF 1.4, fuentes estándar Helvetica con codificación WinAnsi, texto,
líneas y rectángulos), así que no hace falta ninguna librería externa.
"""
from __future__ import annotations

import datetime as dt
import html
import math
import zlib

from . import mantenimiento as mt

DIAS_CALENDARIO = 30
A4 = (595.28, 841.89)   # puntos
MARGEN = 42.0


# ================================================================= datos del informe
def reunir(estado, hoy=None, hallazgos=None, kpis=None, planta="Planta fotovoltaica"):
    """Junta lo que el informe muestra, como datos simples."""
    hoy_d = dt.date.fromisoformat(hoy) if hoy else dt.date.today()
    tab = mt.tablero(estado, hoy_d.isoformat())
    fin = hoy_d + dt.timedelta(days=DIAS_CALENDARIO - 1)
    pend = [o for o in estado["ordenes"] if o["estado"] != "cerrada"]
    vencidas = [o for o in pend if dt.date.fromisoformat(o["fecha"]) < hoy_d]
    proximas = [o for o in pend if hoy_d <= dt.date.fromisoformat(o["fecha"]) <= fin]
    por_dia = {}
    for o in proximas:
        por_dia.setdefault(o["fecha"], []).append(o)
    filas_h = []
    if hallazgos is not None:
        for _, f in hallazgos.iterrows():
            filas_h.append([str(f["tipo"]), str(f["equipo"]), str(f["valor"]), str(f["detalle"]), str(f["estado"])])
    filas_k = []
    if kpis is not None:
        for _, f in kpis.iterrows():
            filas_k.append([str(f["inversor"]), f"{100 * f['PR']:.1f} %", f"{100 * f['disponibilidad']:.1f} %",
                            f"{f['energia_kwh'] / 1000:.1f}", f"{f['energia_perdida_kwh'] / 1000:.1f}"])
    riesgos = sorted(mt.amef_por_equipo(estado), key=lambda f: -f["RPN"])[:5]
    return {
        "planta": planta, "hoy": hoy_d.isoformat(), "fin": fin.isoformat(), "tablero": tab,
        "vencidas": vencidas, "proximas": proximas, "por_dia": por_dia,
        "hallazgos": filas_h, "kpis": filas_k, "riesgos": riesgos,
        "generado": dt.datetime.now().strftime("%Y-%m-%d %H:%M"),
    }


# ================================================================= HTML
_CSS = """
body{font-family:-apple-system,Segoe UI,Helvetica,Arial,sans-serif;color:#1c2630;margin:32px auto;max-width:980px;line-height:1.45}
h1{font-size:24px;margin:0}h2{font-size:13px;letter-spacing:.06em;text-transform:uppercase;color:#4a5866;margin:28px 0 8px;border-bottom:1px solid #d5dbe1;padding-bottom:4px}
.sub{color:#5b6875;font-size:13px}.kpis{display:flex;gap:10px;flex-wrap:wrap}.kpi{border:1px solid #d5dbe1;border-radius:4px;padding:10px 14px;min-width:130px}
.kpi b{display:block;font:600 20px ui-monospace,Menlo,monospace}.kpi span{font-size:12px;color:#5b6875}
table{border-collapse:collapse;width:100%;font-size:13px}th{background:#eef1f4;text-align:left;font-size:11px;text-transform:uppercase;letter-spacing:.04em;padding:6px}
td{border-top:1px solid #e1e6ea;padding:5px 6px}td.n{font-family:ui-monospace,Menlo,monospace;text-align:right}
.estado::before{content:"";display:inline-block;width:7px;height:7px;margin-right:6px}.falla::before{background:#c0392b}.alerta::before{background:#c98a0b}.normal::before,.bueno::before{background:#2e8b57}
.cal{display:grid;grid-template-columns:repeat(7,1fr);gap:4px}.dia{border:1px solid #d5dbe1;border-radius:3px;min-height:58px;padding:4px;font-size:11px}
.dia .f{font-family:ui-monospace,Menlo,monospace;color:#5b6875}.dia .o{margin-top:2px}.vacio{border:none}
footer{margin-top:28px;font-size:11px;color:#6b7782}
"""


def _e(t):
    return html.escape(str(t))


def _tabla_html(cols, filas, numericas=(), estado=None):
    h = "<table><tr>" + "".join(f"<th>{_e(c)}</th>" for c in cols) + "</tr>"
    for f in filas:
        celdas = []
        for i, v in enumerate(f):
            if estado is not None and i == estado:
                celdas.append(f'<td><span class="estado {_e(v)}">{_e(v)}</span></td>')
            else:
                celdas.append(f'<td class="n">{_e(v)}</td>' if i in numericas else f"<td>{_e(v)}</td>")
        h += "<tr>" + "".join(celdas) + "</tr>"
    return h + "</table>"


def _barras_svg(etiquetas, valores, titulo, ancho=900, alto=170):
    """Gráfica de barras sencilla en SVG (órdenes por semana)."""
    n = max(len(valores), 1)
    vmax = max(max(valores, default=0), 1)
    paso = (ancho - 60) / n
    partes = [f'<svg viewBox="0 0 {ancho} {alto}" width="100%" role="img" aria-label="{_e(titulo)}">',
              f'<line x1="40" y1="{alto - 24}" x2="{ancho - 10}" y2="{alto - 24}" stroke="#9aa5ae"/>']
    for i, (et, v) in enumerate(zip(etiquetas, valores)):
        h = (alto - 50) * v / vmax
        x = 44 + i * paso
        partes.append(f'<rect x="{x:.1f}" y="{alto - 24 - h:.1f}" width="{paso * 0.7:.1f}" height="{h:.1f}" fill="#3f8fb0"/>')
        partes.append(f'<text x="{x + paso * 0.35:.1f}" y="{alto - 28 - h:.1f}" font-size="11" text-anchor="middle" font-family="monospace">{v}</text>')
        partes.append(f'<text x="{x + paso * 0.35:.1f}" y="{alto - 8}" font-size="10" text-anchor="middle" fill="#5b6875">{_e(et)}</text>')
    partes.append("</svg>")
    return "".join(partes)


def _semanas(d):
    hoy = dt.date.fromisoformat(d["hoy"])
    et, val = [], []
    for s in range(0, DIAS_CALENDARIO, 7):
        a, b = hoy + dt.timedelta(days=s), hoy + dt.timedelta(days=min(s + 6, DIAS_CALENDARIO - 1))
        et.append(f"{a:%d/%m}-{b:%d/%m}")
        val.append(sum(1 for o in d["proximas"] if a <= dt.date.fromisoformat(o["fecha"]) <= b))
    return et, val


def html_informe(d):
    t = d["tablero"]
    out = [f"<!doctype html><html lang='es'><head><meta charset='utf-8'><title>Informe O&amp;M · {_e(d['planta'])}</title><style>{_CSS}</style></head><body>"]
    out.append(f"<h1>Informe de operación y mantenimiento</h1><div class='sub'>{_e(d['planta'])} · {_e(d['hoy'])} a {_e(d['fin'])}</div>")
    out.append("<h2>Resumen del plan</h2><div class='kpis'>")
    for v, e in [(t["pendientes"], "órdenes pendientes"), (t["vencidas"], "vencidas"), (t["proximas_30"], "en los próximos 30 días"),
                 (t["cerradas"], "cerradas"), (f"{100 * t['cumplimiento']:.0f} %", "cumplimiento del plan")]:
        out.append(f"<div class='kpi'><b>{_e(v)}</b><span>{_e(e)}</span></div>")
    out.append("</div>")
    if d["kpis"]:
        out.append("<h2>Indicadores por inversor</h2>" + _tabla_html(["Inversor", "PR", "Disponibilidad", "Energía [MWh]", "Perdida [MWh]"], d["kpis"], numericas=(1, 2, 3, 4)))
    out.append("<h2>Hallazgos</h2>")
    if d["hallazgos"]:
        out.append(_tabla_html(["Tipo", "Equipo", "Valor", "Detalle", "Estado"], d["hallazgos"], estado=4))
    else:
        out.append("<p class='sub'>Sin análisis de fallas en esta sesión. Ejecuta «Detección de fallas» para incluirlos.</p>")
    out.append("<h2>Órdenes vencidas</h2>")
    out.append(_tabla_html(["Orden", "Trabajo", "Equipo", "Fecha"], [[o["id"], o["nombre"], o["equipo"], o["fecha"]] for o in d["vencidas"]], numericas=(3,))
               if d["vencidas"] else "<p class='sub'>No hay órdenes vencidas.</p>")
    out.append("<h2>Carga de trabajo por semana</h2>")
    et, val = _semanas(d)
    out.append(_barras_svg(et, val, "Órdenes por semana"))
    out.append("<h2>Calendario de los próximos 30 días</h2><div class='cal'>")
    hoy = dt.date.fromisoformat(d["hoy"])
    for _ in range(hoy.weekday()):
        out.append("<div class='dia vacio'></div>")
    for i in range(DIAS_CALENDARIO):
        dia = hoy + dt.timedelta(days=i)
        ords = d["por_dia"].get(dia.isoformat(), [])
        cuerpo = "".join(f"<div class='o'>{_e(o['equipo'])}: {_e(o['nombre'][:28])}</div>" for o in ords[:3])
        if len(ords) > 3:
            cuerpo += f"<div class='o'>+{len(ords) - 3} más</div>"
        out.append(f"<div class='dia'><div class='f'>{dia:%a %d/%m}</div>{cuerpo}</div>")
    out.append("</div>")
    out.append("<h2>Órdenes de los próximos 30 días</h2>")
    out.append(_tabla_html(["Fecha", "Orden", "Trabajo", "Equipo"], [[o["fecha"], o["id"], o["nombre"], o["equipo"]] for o in d["proximas"]], numericas=(0,)))
    if d["riesgos"]:
        out.append("<h2>Modos de falla de mayor riesgo (AMEF)</h2>")
        out.append(_tabla_html(["Equipo", "Modo", "Causa", "RPN"], [[r["tipo_equipo"], r["modo"], r["causa"], r["RPN"]] for r in d["riesgos"]], numericas=(3,)))
    out.append(f"<footer>Generado por Heliora el {_e(d['generado'])}.</footer></body></html>")
    return "".join(out)


# ================================================================= PDF mínimo
def _pdf_texto(t):
    """Texto para una cadena PDF en WinAnsi; los caracteres fuera de ella se aproximan."""
    sustituir = {"·": "\xb7", "—": "-", "–": "-", "→": "->", "≥": ">=", "≤": "<=", "°": "\xb0", "€": "\x80", "’": "'", "“": '"', "”": '"'}
    s = "".join(sustituir.get(c, c) for c in str(t))
    b = s.encode("cp1252", errors="replace")
    return b.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")


# Anchos de Helvetica (1/1000 de em) para medir el texto; el resto se aproxima a 556
_ANCHOS = {" ": 278, "i": 222, "l": 222, "j": 222, "t": 278, "f": 278, "r": 333, "I": 278, ".": 278, ",": 278, ":": 278, ";": 278,
           "m": 833, "w": 722, "M": 833, "W": 944, "(": 333, ")": 333, "-": 333, "/": 278, "%": 889, "1": 556}


def _ancho(t, tam, negrita=False):
    return sum(_ANCHOS.get(c, 611 if c.isupper() else 556) for c in str(t)) * tam / 1000.0 * (1.05 if negrita else 1.0)


def _recortar(t, tam, ancho_max):
    t = str(t)
    if _ancho(t, tam) <= ancho_max:
        return t
    while t and _ancho(t + "...", tam) > ancho_max:
        t = t[:-1]
    return t + "..."


class _PDF:
    """Páginas A4 con texto, líneas y rectángulos. Las coordenadas van desde arriba a la izquierda."""

    def __init__(self):
        self.paginas = []
        self.nueva()

    def nueva(self):
        self.ops = []
        self.paginas.append(self.ops)
        self.y = MARGEN

    def _y(self, y):
        return A4[1] - y

    def texto(self, x, y, t, tam=10, negrita=False, gris=0.1, mono=False):
        fuente = "F3" if mono else ("F2" if negrita else "F1")
        self.ops.append(b"BT /%s %.1f Tf %.3f g %.2f %.2f Td (" % (fuente.encode(), tam, gris, x, self._y(y)) + _pdf_texto(t) + b") Tj ET")

    def linea(self, x1, y1, x2, y2, gris=0.75, grosor=0.6):
        self.ops.append(b"%.3f G %.2f w %.2f %.2f m %.2f %.2f l S" % (gris, grosor, x1, self._y(y1), x2, self._y(y2)))

    def rect(self, x, y, w, h, rgb=(0.93, 0.95, 0.96), borde=None):
        r, g, b = rgb
        op = b"%.3f %.3f %.3f rg %.2f %.2f %.2f %.2f re f" % (r, g, b, x, self._y(y + h), w, h)
        self.ops.append(op)
        if borde is not None:
            self.ops.append(b"%.3f G 0.6 w %.2f %.2f %.2f %.2f re S" % (borde, x, self._y(y + h), w, h))

    def espacio(self, h):
        if self.y + h > A4[1] - MARGEN:
            self.nueva()
            return True
        return False

    def guardar(self, ruta):
        objs = []

        def obj(b):
            objs.append(b)
            return len(objs)

        f1 = obj(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
        f2 = obj(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>")
        f3 = obj(b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>")
        raiz_paginas = len(objs) + 1 + 2 * len(self.paginas)   # se reserva su número
        hijos = []
        for ops in self.paginas:
            datos = zlib.compress(b"\n".join(ops))
            c = obj(b"<< /Length %d /Filter /FlateDecode >>\nstream\n" % len(datos) + datos + b"\nendstream")
            p = obj(b"<< /Type /Page /Parent %d 0 R /MediaBox [0 0 %.2f %.2f] /Contents %d 0 R "
                    b"/Resources << /Font << /F1 %d 0 R /F2 %d 0 R /F3 %d 0 R >> >> >>" % (raiz_paginas, A4[0], A4[1], c, f1, f2, f3))
            hijos.append(p)
        rp = obj(b"<< /Type /Pages /Kids [" + b" ".join(b"%d 0 R" % h for h in hijos) + b"] /Count %d >>" % len(hijos))
        assert rp == raiz_paginas
        cat = obj(b"<< /Type /Catalog /Pages %d 0 R >>" % rp)
        info = obj(b"<< /Producer (Heliora) /Title (" + _pdf_texto("Informe de operación y mantenimiento") + b") >>")
        salida = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
        pos = []
        for i, b in enumerate(objs, 1):
            pos.append(len(salida))
            salida += b"%d 0 obj\n" % i + b + b"\nendobj\n"
        xref = len(salida)
        salida += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
        for p in pos:
            salida += b"%010d 00000 n \n" % p
        salida += b"trailer\n<< /Size %d /Root %d 0 R /Info %d 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, cat, info, xref)
        with open(ruta, "wb") as f:
            f.write(bytes(salida))


_COLOR_ESTADO = {"falla": (0.75, 0.22, 0.17), "alerta": (0.79, 0.54, 0.04), "normal": (0.18, 0.55, 0.34), "bueno": (0.18, 0.55, 0.34)}


def _pdf_titulo(p, t):
    p.espacio(40)
    p.y += 16
    p.texto(MARGEN, p.y, t.upper(), 9, True, 0.3)
    p.y += 5
    p.linea(MARGEN, p.y, A4[0] - MARGEN, p.y)
    p.y += 12


def _pdf_tabla(p, cols, filas, anchos, estado=None, mono=()):
    total = A4[0] - 2 * MARGEN
    xs = [MARGEN]
    for a in anchos[:-1]:
        xs.append(xs[-1] + a * total)

    def cabecera():
        p.rect(MARGEN, p.y - 10, total, 15)
        for x, c in zip(xs, cols):
            p.texto(x + 3, p.y, str(c).upper(), 7.5, True, 0.3)
        p.y += 15

    cabecera()
    for f in filas:
        if p.espacio(15):
            cabecera()
        for i, (x, v) in enumerate(zip(xs, f)):
            ancho = anchos[i] * total - 6
            if estado is not None and i == estado:
                p.rect(x + 3, p.y - 6, 5, 5, _COLOR_ESTADO.get(str(v), (0.5, 0.5, 0.5)))
                p.texto(x + 11, p.y, _recortar(v, 8.5, ancho - 8), 8.5)
            else:
                p.texto(x + 3, p.y, _recortar(v, 8.5, ancho), 8.5, mono=i in mono)
        p.y += 4
        p.linea(MARGEN, p.y, MARGEN + total, p.y, 0.88, 0.4)
        p.y += 11


def pdf_informe(d, ruta):
    p = _PDF()
    t = d["tablero"]
    p.texto(MARGEN, p.y + 10, "Informe de operación y mantenimiento", 18, True)
    p.y += 28
    p.texto(MARGEN, p.y, f"{d['planta']} · {d['hoy']} a {d['fin']}", 10, gris=0.35)
    p.y += 10
    _pdf_titulo(p, "Resumen del plan")
    cajas = [(t["pendientes"], "pendientes"), (t["vencidas"], "vencidas"), (t["proximas_30"], "próximos 30 días"),
             (t["cerradas"], "cerradas"), (f"{100 * t['cumplimiento']:.0f} %", "cumplimiento")]
    w = (A4[0] - 2 * MARGEN - 4 * 8) / 5
    for i, (v, e) in enumerate(cajas):
        x = MARGEN + i * (w + 8)
        p.rect(x, p.y - 4, w, 40, (1, 1, 1), borde=0.8)
        p.texto(x + 8, p.y + 14, v, 15, True, mono=True)
        p.texto(x + 8, p.y + 28, e, 8, gris=0.4)
    p.y += 46
    if d["kpis"]:
        _pdf_titulo(p, "Indicadores por inversor")
        _pdf_tabla(p, ["Inversor", "PR", "Disponibilidad", "Energía MWh", "Perdida MWh"], d["kpis"], [0.24, 0.17, 0.2, 0.2, 0.19], mono=(1, 2, 3, 4))
    _pdf_titulo(p, "Hallazgos")
    if d["hallazgos"]:
        _pdf_tabla(p, ["Tipo", "Equipo", "Valor", "Detalle", "Estado"], d["hallazgos"], [0.2, 0.12, 0.16, 0.38, 0.14], estado=4)
    else:
        p.texto(MARGEN, p.y, "Sin análisis de fallas en esta sesión.", 9, gris=0.4)
        p.y += 14
    _pdf_titulo(p, "Órdenes vencidas")
    if d["vencidas"]:
        _pdf_tabla(p, ["Orden", "Trabajo", "Equipo", "Fecha"], [[o["id"], o["nombre"], o["equipo"], o["fecha"]] for o in d["vencidas"]], [0.15, 0.5, 0.15, 0.2], mono=(0, 3))
    else:
        p.texto(MARGEN, p.y, "No hay órdenes vencidas.", 9, gris=0.4)
        p.y += 14
    # Carga por semana como barras
    _pdf_titulo(p, "Carga de trabajo por semana")
    et, val = _semanas(d)
    p.espacio(110)
    vmax = max(max(val, default=0), 1)
    paso = (A4[0] - 2 * MARGEN) / max(len(val), 1)
    base = p.y + 80
    p.linea(MARGEN, base, A4[0] - MARGEN, base, 0.6)
    for i, (e, v) in enumerate(zip(et, val)):
        h = 66 * v / vmax
        x = MARGEN + i * paso + paso * 0.15
        p.rect(x, base - h, paso * 0.7, h, (0.25, 0.56, 0.69))
        p.texto(x + paso * 0.3, base - h - 3, str(v), 8, mono=True)
        p.texto(x, base + 11, e, 7.5, gris=0.4)
    p.y = base + 22
    # Calendario de 30 días (entero en una página)
    hoy = dt.date.fromisoformat(d["hoy"])
    cw = (A4[0] - 2 * MARGEN) / 7
    ch = 46
    filas = math.ceil((hoy.weekday() + DIAS_CALENDARIO) / 7)
    p.espacio(filas * ch + 60)
    _pdf_titulo(p, "Calendario de los próximos 30 días")
    for j, nombre in enumerate(["lun", "mar", "mié", "jue", "vie", "sáb", "dom"]):
        p.texto(MARGEN + j * cw + 3, p.y, nombre, 7.5, True, 0.35)
    p.y += 5
    y0 = p.y
    for i in range(DIAS_CALENDARIO):
        k = hoy.weekday() + i
        x, y = MARGEN + (k % 7) * cw, y0 + (k // 7) * ch
        dia = hoy + dt.timedelta(days=i)
        p.rect(x + 1, y, cw - 2, ch - 2, (1, 1, 1), borde=0.82)
        p.texto(x + 4, y + 10, f"{dia:%d/%m}", 7.5, gris=0.35, mono=True)
        ords = d["por_dia"].get(dia.isoformat(), [])
        for n, o in enumerate(ords[:2]):
            p.texto(x + 4, y + 21 + 9 * n, _recortar(f"{o['equipo']} {o['nombre']}", 6.5, cw - 8), 6.5)
        if len(ords) > 2:
            p.texto(x + 4, y + 39, f"+{len(ords) - 2} más", 6.5, gris=0.4)
    p.y = y0 + filas * ch + 4
    _pdf_titulo(p, "Órdenes de los próximos 30 días")
    _pdf_tabla(p, ["Fecha", "Orden", "Trabajo", "Equipo"], [[o["fecha"], o["id"], o["nombre"], o["equipo"]] for o in d["proximas"]], [0.17, 0.15, 0.53, 0.15], mono=(0, 1))
    if d["riesgos"]:
        _pdf_titulo(p, "Modos de falla de mayor riesgo (AMEF)")
        _pdf_tabla(p, ["Equipo", "Modo", "Causa", "RPN"], [[r["tipo_equipo"], r["modo"], r["causa"], r["RPN"]] for r in d["riesgos"]], [0.18, 0.32, 0.38, 0.12], mono=(3,))
    p.espacio(20)
    p.y += 10
    p.texto(MARGEN, p.y, f"Generado por Heliora el {d['generado']}.", 7.5, gris=0.45)
    # Números de página
    for i, ops in enumerate(p.paginas):
        p.ops = ops
        p.texto(A4[0] - MARGEN - 40, A4[1] - 22, f"{i + 1} / {len(p.paginas)}", 7.5, gris=0.5, mono=True)
    p.guardar(ruta)
    return len(p.paginas)


def escribir(estado, carpeta, hoy=None, hallazgos=None, kpis=None, planta="Planta fotovoltaica"):
    """Escribe informe_om.html e informe_om.pdf en la carpeta. Devuelve sus rutas."""
    import os
    os.makedirs(carpeta, exist_ok=True)
    d = reunir(estado, hoy, hallazgos, kpis, planta)
    r_html = os.path.join(carpeta, "informe_om.html")
    r_pdf = os.path.join(carpeta, "informe_om.pdf")
    with open(r_html, "w", encoding="utf8") as f:
        f.write(html_informe(d))
    paginas = pdf_informe(d, r_pdf)
    return {"html": r_html, "pdf": r_pdf, "paginas": paginas, "hallazgos": len(d["hallazgos"]), "proximas": len(d["proximas"]), "vencidas": len(d["vencidas"])}
