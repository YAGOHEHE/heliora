"""Lectura y validación de las entradas de las herramientas de análisis.

Toda comprobación de un parámetro (número, unidad, rango, nombre de salida, modelo escrito
como expresión) vive aquí, de modo que la herramienta recibe valores ya válidos y los mensajes
de error nombran siempre la herramienta y el parámetro.
"""
from __future__ import annotations

import ast
import keyword
import math
import re

import numpy as np

from .sim.nucleo import _CON_UNIDAD, _PREFIJOS


class ErrorHerramienta(ValueError):
    """Entrada no válida de una herramienta: el mensaje ya nombra la herramienta y el parámetro."""


_UNIDAD_ETIQUETA = re.compile(r"\[([A-Za-zµμΩ]+)\]\s*$")
_BASES = {"H", "F", "W", "V", "A", "Ω", "Hz", "s", "VA", "var", "Wh", "J"}


def unidad_de_etiqueta(etiqueta):
    """Devuelve la unidad escrita entre corchetes al final de la etiqueta («Potencia [kW]» da «kW»), o ''."""
    m = _UNIDAD_ETIQUETA.search(etiqueta)
    return m.group(1) if m else ""


def nombre_de_etiqueta(etiqueta):
    """Quita la unidad entre corchetes de la etiqueta para usarla en los mensajes."""
    return _UNIDAD_ETIQUETA.sub("", etiqueta).strip()


def _partir(u):
    if u in _BASES:
        return 1.0, u
    if len(u) > 1 and u[0] in _PREFIJOS and u[1:] in _BASES:
        return _PREFIJOS[u[0]], u[1:]
    return None


def con_unidad(texto, unidad):
    """Convierte un texto como «0.5 mH» o «500 W» a la unidad del campo.

    Args:
        texto: contenido del campo.
        unidad: unidad del campo (por ejemplo «kW»).

    Returns:
        El número expresado en `unidad`, o None si el texto no tiene la forma número + unidad.

    Raises:
        ErrorHerramienta: si la unidad escrita no corresponde a la magnitud del campo.
    """
    m = _CON_UNIDAD.match(texto.strip())
    if m is None or not unidad:
        return None
    numero, sufijo = float(m.group(1)), m.group(2)
    if sufijo == unidad:
        return numero
    a, b = _partir(sufijo), _partir(unidad)
    if a is None or b is None or a[1] != b[1]:
        raise ErrorHerramienta(f"la unidad «{sufijo}» no corresponde a este campo, que está en {unidad}")
    return numero * a[0] / b[0]


def _condicion(lim):
    a, b = lim.get("min"), lim.get("max")
    if a is not None and b is not None:
        ex_a, ex_b = lim.get("abierto_min", False), lim.get("abierto_max", False)
        return f"un valor entre {a:g} y {b:g}" + (" (sin incluir " + ("ambos extremos" if ex_a and ex_b else "el mínimo" if ex_a else "el máximo") + ")" if ex_a or ex_b else "")
    if a is not None:
        return f"mayor que {a:g}" if lim.get("abierto_min") else f"mayor o igual que {a:g}"
    return f"menor que {b:g}" if lim.get("abierto_max") else f"menor o igual que {b:g}"


def comprobar_rango(valor, lim, titulo, etiqueta):
    """Comprueba un número contra los límites declarados {min, max, abierto_min, abierto_max}.

    Raises:
        ErrorHerramienta: con el nombre de la herramienta, el parámetro y el valor recibido.
    """
    if not lim:
        return
    a, b = lim.get("min"), lim.get("max")
    mal = (a is not None and (valor < a or (lim.get("abierto_min") and valor <= a))) or (b is not None and (valor > b or (lim.get("abierto_max") and valor >= b)))
    if mal:
        raise ErrorHerramienta(f"«{titulo}»: «{nombre_de_etiqueta(etiqueta)}» debe ser {_condicion(lim)} (se recibió {valor:g}).")


def leer_numero(valor, p, titulo, evaluar):
    """Convierte el texto del formulario en número (o entero) y comprueba sus límites.

    Args:
        valor: texto del campo, con unidad opcional o expresión sobre las variables.
        p: descripción del parámetro (etiqueta, tipo, limites).
        titulo: título de la herramienta, para los mensajes.
        evaluar: función que evalúa una expresión numérica en el espacio de trabajo.

    Returns:
        float o int según el tipo del parámetro.

    Raises:
        ErrorHerramienta: si el texto no es un número, no es entero cuando debe serlo o sale de rango.
    """
    etiqueta = nombre_de_etiqueta(p["etiqueta"])
    try:
        x = con_unidad(str(valor), unidad_de_etiqueta(p["etiqueta"])) if isinstance(valor, str) else None
        if x is None:
            x = evaluar(valor)
        if isinstance(x, np.ndarray) or not isinstance(x, (int, float, np.number)):
            raise TypeError("se esperaba un solo número")
        x = float(x)
    except ErrorHerramienta as e:
        raise ErrorHerramienta(f"«{titulo}», «{etiqueta}»: {e}") from None
    except Exception as e:
        raise ErrorHerramienta(f"«{titulo}»: «{etiqueta}» no es un número válido (escribió «{valor}»): {e}") from None
    if not math.isfinite(x):
        raise ErrorHerramienta(f"«{titulo}»: «{etiqueta}» debe ser un número finito (se recibió {x}).")
    if p["tipo"] == "entero":
        if x != int(x):
            raise ErrorHerramienta(f"«{titulo}»: «{etiqueta}» debe ser un número entero (se recibió {x:g}).")
        x = int(x)
    comprobar_rango(x, p.get("limites"), titulo, p["etiqueta"])
    return x


def comprobar_nombre_salida(nombre, titulo, etiqueta, ns, base, usados, propias):
    """Valida el nombre de una variable de salida.

    Args:
        nombre: nombre escrito (vacío significa «no guardar»).
        titulo: título de la herramienta.
        etiqueta: etiqueta del parámetro.
        ns: espacio de trabajo.
        base: nombres reservados del espacio (np, pd, funciones de gráfica...).
        usados: nombres de las tablas que la herramienta lee y de otras salidas ya aceptadas.
        propias: nombres de variables creadas antes por las herramientas, que pueden reescribirse.

    Raises:
        ErrorHerramienta: si no es un identificador, está reservado, coincide con una entrada o
            pisaría una variable del usuario.
    """
    nombre = nombre.strip()
    etiqueta = nombre_de_etiqueta(etiqueta)
    if not nombre.isidentifier() or keyword.iskeyword(nombre):
        raise ErrorHerramienta(f"«{titulo}»: «{etiqueta}» ({nombre!r}) no es un nombre de variable válido: usa letras, cifras y guion bajo, sin empezar por cifra.")
    if nombre in base:
        raise ErrorHerramienta(f"«{titulo}»: «{nombre}» es un nombre reservado del espacio de trabajo; elige otro en «{etiqueta}».")
    if nombre in usados:
        raise ErrorHerramienta(f"«{titulo}»: «{etiqueta}» ({nombre}) pisaría la tabla de entrada u otra salida de la herramienta; elige otro nombre.")
    if nombre in ns and nombre not in propias:
        raise ErrorHerramienta(f"«{titulo}»: la variable «{nombre}» ya existe en el espacio de trabajo y no la creó esta herramienta; elige otro nombre en «{etiqueta}» o bórrala antes.")


# Funciones de NumPy permitidas en los modelos escritos como expresión.
_FUNCIONES = {"exp", "log", "log10", "log2", "sqrt", "cbrt", "square", "abs", "sign", "sin", "cos", "tan", "arcsin", "arccos", "arctan", "arctan2",
              "sinh", "cosh", "tanh", "power", "minimum", "maximum"}
_CONSTANTES = {"pi": math.pi, "e": math.e}
_OPERADORES = (ast.Add, ast.Sub, ast.Mult, ast.Div, ast.Pow, ast.Mod, ast.FloorDiv)


class _Enteros(ast.NodeTransformer):
    """Convierte las constantes enteras en reales para que `9**9**9` desborde en lugar de colgar el motor."""

    def visit_Constant(self, nodo):
        return ast.copy_location(ast.Constant(float(nodo.value)), nodo) if isinstance(nodo.value, int) and not isinstance(nodo.value, bool) else nodo


def _revisar(nodo, nombres):
    if isinstance(nodo, ast.Expression):
        return _revisar(nodo.body, nombres)
    if isinstance(nodo, ast.Constant):
        if isinstance(nodo.value, (int, float)) and not isinstance(nodo.value, bool):
            return
        raise ErrorHerramienta(f"la constante {nodo.value!r} no es un número")
    if isinstance(nodo, ast.Name):
        if nodo.id not in nombres and nodo.id not in _CONSTANTES and nodo.id not in _FUNCIONES:
            raise ErrorHerramienta(f"el nombre «{nodo.id}» no está permitido: usa x, {', '.join(sorted(nombres - {'x'}))}, pi, e y funciones como exp, log, sin")
        return
    if isinstance(nodo, ast.BinOp) and isinstance(nodo.op, _OPERADORES):
        _revisar(nodo.left, nombres)
        _revisar(nodo.right, nombres)
        return
    if isinstance(nodo, ast.UnaryOp) and isinstance(nodo.op, (ast.USub, ast.UAdd)):
        _revisar(nodo.operand, nombres)
        return
    if isinstance(nodo, ast.Call) and not nodo.keywords:
        f = nodo.func
        if isinstance(f, ast.Attribute) and isinstance(f.value, ast.Name) and f.value.id == "np" and f.attr in _FUNCIONES and not f.attr.startswith("_"):
            pass
        elif isinstance(f, ast.Name) and f.id in _FUNCIONES:
            pass
        else:
            raise ErrorHerramienta("solo se permiten funciones de NumPy de la lista: " + ", ".join(sorted(_FUNCIONES)))
        for a in nodo.args:
            _revisar(a, nombres)
        return
    raise ErrorHerramienta(f"la expresión usa una construcción no permitida ({type(nodo).__name__}); solo se admiten números, nombres, operadores aritméticos y funciones de NumPy")


def compilar_modelo(expresion, letras):
    """Compila una expresión f(x, a, b, ...) restringida a una lista blanca de construcciones.

    Args:
        expresion: texto como «a*np.exp(-b*x)+c».
        letras: nombres de los parámetros a ajustar.

    Returns:
        Función f(x, *parametros).

    Raises:
        ErrorHerramienta: si la expresión no es válida o usa algo fuera de la lista blanca.
    """
    if len(expresion) > 300:
        raise ErrorHerramienta("la expresión es demasiado larga (máximo 300 caracteres)")
    try:
        arbol = ast.parse(expresion.strip(), mode="eval")
    except SyntaxError as e:
        raise ErrorHerramienta(f"la expresión no es válida: {e.msg}") from None
    _revisar(arbol, set(letras) | {"x"})
    arbol = _Enteros().visit(arbol)
    argumentos = ast.arguments(posonlyargs=[], args=[ast.arg("x")] + [ast.arg(a) for a in letras], kwonlyargs=[], kw_defaults=[], defaults=[])
    codigo = compile(ast.fix_missing_locations(ast.Expression(ast.Lambda(argumentos, arbol.body))), "<modelo>", "eval")
    entorno = {"__builtins__": {}, "np": np, **_CONSTANTES, **{k: getattr(np, k) for k in _FUNCIONES}}
    return eval(codigo, entorno)  # el árbol ya pasó la lista blanca
