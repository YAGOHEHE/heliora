"""Lectura de tablas desde archivo, común a la consola, las herramientas, el SCADA y los bloques.

Admite los CSV que produce Excel con configuración regional española (separador «;», coma decimal,
codificación Windows-1252) además de los CSV estándar, TSV y Parquet. Las fechas en formato
día/mes/año se interpretan con el día primero cuando el archivo no deja lugar a duda.
"""
from __future__ import annotations

import csv
import io
import os
import re

import pandas as pd

SEPARADORES = ",;\t|"
CODIFICACIONES = ("utf-8-sig", "cp1252")
_NUMERO_COMA = re.compile(r"^-?\d+,\d+$")


class ErrorLectura(ValueError):
    """El archivo no se puede leer como tabla; el mensaje explica por qué."""


def _decodificar(datos: bytes) -> str:
    """Decodifica el texto con la primera codificación que lo admita."""
    for cod in CODIFICACIONES:
        try:
            return datos.decode(cod)
        except UnicodeDecodeError:
            continue
    return datos.decode("latin-1")


def _separador(muestra: str) -> str:
    """Separador más probable a partir de las primeras líneas."""
    try:
        return csv.Sniffer().sniff(muestra, delimiters=SEPARADORES).delimiter
    except csv.Error:
        lineas = [ln for ln in muestra.splitlines() if ln.strip()][:20]
        cuentas = {s: min((ln.count(s) for ln in lineas), default=0) for s in SEPARADORES}
        mejor = max(cuentas, key=cuentas.get)
        return mejor if cuentas[mejor] > 0 else ","


def _decimal(texto: str, sep: str) -> str:
    """Coma decimal si los campos numéricos la usan y el separador no es la coma."""
    if sep == ",":
        return "."
    campos = [c.strip().strip('"') for ln in texto.splitlines()[1:200] for c in ln.split(sep)]
    con_coma = sum(1 for c in campos if _NUMERO_COMA.match(c))
    con_punto = sum(1 for c in campos if re.match(r"^-?\d+\.\d+$", c))
    return "," if con_coma > con_punto else "."


def leer_tabla(ruta, sep=None, decimal=None, encoding=None) -> pd.DataFrame:
    """Lee una tabla CSV, TSV, TXT o Parquet detectando separador, decimal y codificación.

    Args:
        ruta: ruta del archivo.
        sep: separador de campos; None lo detecta.
        decimal: separador decimal; None lo detecta.
        encoding: codificación del texto; None prueba UTF-8 y después Windows-1252.

    Returns:
        DataFrame con los nombres de columna sin espacios sobrantes.

    Raises:
        ErrorLectura: si el archivo no existe, está vacío, es un formato no admitido o no tiene filas.
    """
    ruta = os.fspath(ruta)
    if not os.path.isfile(ruta):
        raise ErrorLectura(f"No existe el archivo «{ruta}».")
    ext = os.path.splitext(ruta)[1].lower()
    if ext == ".parquet":
        return pd.read_parquet(ruta)
    if ext in (".xlsx", ".xlsm", ".xls"):
        try:
            import openpyxl  # noqa: F401
        except ImportError:
            raise ErrorLectura("Los archivos de Excel necesitan la librería openpyxl, que no está instalada. "
                               "Guarda la hoja como CSV desde Excel e impórtala.") from None
        return pd.read_excel(ruta)
    with open(ruta, "rb") as f:
        datos = f.read()
    if datos[:2] == b"PK":
        raise ErrorLectura(f"«{os.path.basename(ruta)}» parece un archivo comprimido o de Excel, no un CSV.")
    texto = _decodificar(datos) if encoding is None else datos.decode(encoding)
    if not texto.strip():
        raise ErrorLectura(f"«{os.path.basename(ruta)}» está vacío.")
    sep = sep or _separador("\n".join(texto.splitlines()[:50]))
    decimal = decimal or _decimal(texto, sep)
    try:
        d = pd.read_csv(io.StringIO(texto), sep=sep, decimal=decimal, skipinitialspace=True)
    except (pd.errors.ParserError, ValueError) as e:
        raise ErrorLectura(f"No se pudo leer «{os.path.basename(ruta)}» como tabla (separador «{sep}»): {e}") from None
    d.columns = [str(c).strip() for c in d.columns]
    if d.empty:
        raise ErrorLectura(f"«{os.path.basename(ruta)}» no tiene filas de datos.")
    return d


def a_fechas(serie: pd.Series, dayfirst=None) -> pd.Series:
    """Convierte una columna de texto en fechas, con el día primero salvo que el archivo indique otra cosa.

    Las fechas ISO (AAAA-MM-DD) se leen siempre igual. En formato con barras o guiones se decide
    el orden con todo el archivo: si algún primer campo pasa de 12, el día va primero; si algún
    segundo campo pasa de 12, el mes va primero; si todo es ambiguo, se toma el día primero
    (convención española) salvo que `dayfirst` diga lo contrario.

    Args:
        serie: columna con las fechas.
        dayfirst: fuerza el orden día/mes (True) o mes/día (False); None lo decide.

    Returns:
        Serie de fechas (`datetime64`).

    Raises:
        ErrorLectura: si alguna fila no se puede interpretar como fecha.
    """
    if pd.api.types.is_datetime64_any_dtype(serie):
        return serie
    texto = serie.astype(str).str.strip()
    try:
        return pd.to_datetime(texto, format="ISO8601")
    except (ValueError, TypeError):
        pass
    if dayfirst is None:
        partes = texto.str.extract(r"^(\d{1,2})[/.-](\d{1,2})[/.-]\d{2,4}")
        a = pd.to_numeric(partes[0], errors="coerce")
        b = pd.to_numeric(partes[1], errors="coerce")
        dayfirst = not ((b > 12).any() and not (a > 12).any())
    f = pd.to_datetime(texto, dayfirst=dayfirst, errors="coerce", format="mixed")
    malas = int(f.isna().sum() - serie.isna().sum())
    if malas > 0:
        ejemplo = texto[f.isna() & serie.notna()].iloc[0]
        raise ErrorLectura(f"{malas} filas no tienen una fecha válida (por ejemplo «{ejemplo}»).")
    return f
