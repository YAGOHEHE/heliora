"""Lectura de tablas: CSV regionales, codificaciones y fechas día/mes."""
from __future__ import annotations

import os
import tempfile

from .banco import prueba
from ..lectura import ErrorLectura, a_fechas, leer_tabla


def _escribir(td, nombre, texto, cod="utf-8"):
    """Escribe un archivo de prueba y devuelve su ruta."""
    ruta = os.path.join(td, nombre)
    with open(ruta, "wb") as f:
        f.write(texto.encode(cod))
    return ruta


@prueba(1, "Lectura de CSV regionales", "Valores escritos en el archivo")
def csv_regionales(m):
    """CSV estándar, CSV de Excel en español (; y coma decimal, Windows-1252) y TSV dan la misma tabla."""
    with tempfile.TemporaryDirectory() as td:
        std = leer_tabla(_escribir(td, "std.csv", "t,pot\n0,1.5\n1,2.25\n"))
        eu = leer_tabla(_escribir(td, "eu.csv", "t;pot;Temp °C\n0;1,5;20,1\n1;2,25;21,0\n", "cp1252"))
        tsv = leer_tabla(_escribir(td, "d.tsv", "t\tpot\n0\t1.5\n1\t2.25\n"))
        m.verdad("CSV estándar: 2 columnas", list(std.columns) == ["t", "pot"])
        m.verdad("CSV español: 3 columnas con «Temp °C»", list(eu.columns) == ["t", "pot", "Temp °C"])
        m.error("CSV español: coma decimal", eu["pot"].iloc[1], 2.25, 0.0)
        m.error("TSV: valor", tsv["pot"].iloc[0], 1.5, 0.0)
        try:
            leer_tabla(_escribir(td, "falso.xlsx", "PK\x03\x04"))
            excel = False
        except ErrorLectura:
            excel = True
        m.verdad("Excel sin openpyxl o archivo comprimido: error explicado", excel)


@prueba(1, "Fechas día/mes y ISO", "Fechas escritas en el archivo")
def fechas(m):
    """«05/03/2024» es el 5 de marzo; un 13 en el primer campo fija el día primero; ISO no cambia."""
    import pandas as pd
    ambiguas = a_fechas(pd.Series(["05/03/2024 10:00", "06/03/2024 10:00"]))
    m.verdad("Ambigua: día primero (5 de marzo)", ambiguas.iloc[0].month == 3 and ambiguas.iloc[0].day == 5)
    usa = a_fechas(pd.Series(["03/05/2024", "03/13/2024"]))
    m.verdad("Segundo campo > 12: mes primero", usa.iloc[1].month == 3 and usa.iloc[1].day == 13)
    iso = a_fechas(pd.Series(["2024-03-05 10:00", "2024-03-06 10:00"]))
    m.verdad("ISO: 5 de marzo", iso.iloc[0].day == 5 and iso.iloc[0].month == 3)
    try:
        a_fechas(pd.Series(["2024-03-05", "mañana"]))
        mala = False
    except ErrorLectura:
        mala = True
    m.verdad("Fecha inválida: error con la fila", mala)
