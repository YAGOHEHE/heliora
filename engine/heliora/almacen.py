"""Persistencia del espacio de trabajo: un índice JSON y un archivo por variable.

Cada variable se guarda en un archivo cuyo nombre no depende de las mayúsculas (en Windows `A` y `a`
son el mismo archivo): `indice.json` asocia el nombre exacto de la variable con su archivo.
Las tablas van en Parquet, las matrices de cualquier dimensión y tipo en `.npy`, los valores
JSON simples en `.json` y el resto de lo serializable con joblib. Lo que no se puede guardar se
devuelve en una lista de omitidos, nunca se descarta en silencio.
"""
from __future__ import annotations

import hashlib
import json
import keyword
import os
import re
import types

import numpy as np
import pandas as pd

INDICE = "indice.json"
FORMATO = "heliora-espacio/1"


def nombre_variable(texto, reservados=()):
    """Convierte un texto en un identificador de Python válido que no pise un nombre reservado.

    Args:
        texto: nombre propuesto, p. ej. el nombre de un archivo.
        reservados: nombres que no se pueden usar (símbolos base de la consola).

    Returns:
        El mismo texto si ya es válido; si no, una versión saneada, con sufijo `_1` si coincide con una
        palabra clave o un nombre reservado.
    """
    n = re.sub(r"\W", "_", str(texto).strip()) or "datos"
    if n[0].isdigit():
        n = "v_" + n
    if keyword.iskeyword(n) or n in reservados:
        n += "_1"
    return n


def es_nombre_valido(nombre, reservados=()):
    """Indica si `nombre` es un identificador público de Python que no es palabra clave ni nombre reservado."""
    return nombre.isidentifier() and not nombre.startswith("_") and not keyword.iskeyword(nombre) and nombre not in reservados


def _archivo_seguro(nombre):
    """Nombre de archivo sin extensión, único para cada nombre de variable aunque difiera solo en mayúsculas."""
    base = re.sub(r"[^A-Za-z0-9_]", "_", nombre)[:40].lower()
    return f"{base}-{hashlib.sha1(nombre.encode('utf8')).hexdigest()[:8]}"


def _es_json_simple(v):
    """True si `v` sobrevive intacto a un viaje por JSON (escalares, listas y diccionarios simples)."""
    if not isinstance(v, (str, int, float, bool, list, dict, type(None))):
        return False
    try:
        r = json.loads(json.dumps(v))
    except (TypeError, ValueError):
        return False
    return r == v and type(r) is type(v)


def _escribir(v, ruta):
    """Escribe `v` en `ruta` más la extensión que le corresponde y devuelve esa extensión."""
    if isinstance(v, pd.DataFrame):
        d = v.copy()
        d.columns = [str(c) for c in d.columns]
        try:
            d.to_parquet(ruta + ".parquet")
            return ".parquet"
        except Exception:
            _quitar(ruta + ".parquet")
    elif isinstance(v, np.ndarray):
        try:
            with open(ruta + ".npy", "wb") as f:
                np.save(f, v, allow_pickle=False)
            return ".npy"
        except ValueError:
            _quitar(ruta + ".npy")
    elif _es_json_simple(v):
        with open(ruta + ".json", "w", encoding="utf8", newline="\n") as f:
            json.dump(v, f, ensure_ascii=False)
        return ".json"
    try:
        import joblib
        joblib.dump(v, ruta + ".pkl")
    except BaseException:
        _quitar(ruta + ".pkl")
        raise
    return ".pkl"


def _quitar(ruta):
    """Borra el archivo si existe."""
    try:
        os.remove(ruta)
    except FileNotFoundError:
        pass


def _leer_indice(carpeta):
    """Devuelve el diccionario nombre → archivo del índice de la carpeta, o None si no hay índice."""
    ruta = os.path.join(carpeta, INDICE)
    if not os.path.isfile(ruta):
        return None
    with open(ruta, encoding="utf8") as f:
        datos = json.load(f)
    if datos.get("formato") != FORMATO or not isinstance(datos.get("variables"), dict):
        raise ValueError(f"«{INDICE}» no tiene el formato {FORMATO}")
    return datos["variables"]


def guardar(variables, carpeta):
    """Guarda las variables en la carpeta y escribe el índice.

    Args:
        variables: diccionario nombre → valor de las variables del usuario.
        carpeta: carpeta de destino; se crea si no existe.

    Returns:
        {"guardados": nombres de las variables guardadas, "omitidos": lista de {"nombre", "motivo"}
        con las que no se pudieron guardar}.
    """
    os.makedirs(carpeta, exist_ok=True)
    try:
        previo = _leer_indice(carpeta) or {}
    except (ValueError, OSError):
        previo = {}
    indice, omitidos = {}, []
    for nombre, v in variables.items():
        if isinstance(v, types.ModuleType):
            continue
        base = os.path.join(carpeta, _archivo_seguro(nombre))
        try:
            indice[nombre] = os.path.basename(base) + _escribir(v, base)
        except Exception as e:
            omitidos.append({"nombre": nombre, "motivo": f"{type(v).__name__}: {str(e)[:120]}"})
    tmp = os.path.join(carpeta, INDICE + ".tmp~")
    with open(tmp, "w", encoding="utf8", newline="\n") as f:
        json.dump({"formato": FORMATO, "variables": indice}, f, ensure_ascii=False, indent=1, sort_keys=True)
    os.replace(tmp, os.path.join(carpeta, INDICE))
    for archivo in set(previo.values()) - set(indice.values()):
        _quitar(os.path.join(carpeta, os.path.basename(archivo)))
    return {"guardados": sorted(indice), "omitidos": omitidos}


def _cargar_archivo(ruta):
    """Lee el archivo de una variable según su extensión."""
    ext = os.path.splitext(ruta)[1].lower()
    if ext == ".parquet":
        return pd.read_parquet(ruta)
    if ext == ".npy":
        return np.load(ruta, allow_pickle=False)
    if ext == ".json":
        with open(ruta, encoding="utf8") as f:
            return json.load(f)
    if ext == ".pkl":
        import joblib
        return joblib.load(ruta)
    raise ValueError(f"extensión «{ext}» desconocida")


def cargar(carpeta, reservados=()):
    """Lee las variables guardadas en una carpeta.

    Con `indice.json` carga lo que el índice enumera (un nombre puede coincidir con un símbolo base
    si el usuario lo había redefinido). Sin él, lee como tablas los archivos `.csv` y
    `.parquet` (carpetas de versiones anteriores) con el nombre del archivo como variable.

    Args:
        carpeta: carpeta del espacio de trabajo guardado.
        reservados: nombres que no pueden usar las tablas leídas de una carpeta antigua.

    Returns:
        Tupla (variables, errores): diccionario nombre → valor y lista de mensajes de lo que no se pudo leer.
    """
    from .lectura import leer_tabla
    variables, errores = {}, []
    try:
        indice = _leer_indice(carpeta)
    except (ValueError, OSError) as e:
        return variables, [f"{INDICE}: {e}"]
    if indice is not None:
        for nombre, archivo in indice.items():
            if not es_nombre_valido(nombre):
                errores.append(f"{nombre}: nombre de variable no válido")
                continue
            try:
                variables[nombre] = _cargar_archivo(os.path.join(carpeta, os.path.basename(archivo)))
            except Exception as e:
                errores.append(f"{nombre}: {str(e)[:120]}")
        return variables, errores
    for a in sorted(os.listdir(carpeta)):
        n, ext = os.path.splitext(a)
        if ext.lower() in (".csv", ".parquet"):
            try:
                variables[nombre_variable(n, reservados)] = leer_tabla(os.path.join(carpeta, a))
            except Exception as e:
                errores.append(f"{a}: {str(e)[:120]}")
    return variables, errores
