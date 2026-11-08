"""Red eléctrica por análisis nodal modificado (MNA).

Todos los elementos eléctricos de un diagrama forman una sola red. En cada evaluación:

    M(estado_discreto) · z = b(t, x, u)

donde z = [tensiones de nodo, corrientes de rama]. Las incógnitas dinámicas no entran en z:

    condensador  fija su tensión (rama de tensión con valor x)     dv/dt = i_C / C
    bobina       inyecta su corriente (fuente de corriente x)      di/dt = v_L / L

Así la red se reduce a dx/dt = f(t, x, u) y la integra cualquier solver de `solvers.py`.
Los interruptores cambian M; cada combinación de estados se factoriza una vez y se guarda.

Índices: el último elemento de z es siempre 0 y representa la tierra, de modo que un
elemento puede usar el nodo -1 sin casos especiales.
"""
from __future__ import annotations

import numpy as np

from .nucleo import Bloque

GMIN = 1e-12  # conductancia mínima de cada nodo a tierra (evita nodos flotantes)


class Estampa:
    """Ayudante para escribir en la matriz M."""

    def __init__(self, M):
        self.M = M

    def g(self, a, b, valor):
        """Conductancia entre los nodos a y b."""
        M = self.M
        M[a, a] += valor
        M[b, b] += valor
        M[a, b] -= valor
        M[b, a] -= valor

    def rama_tension(self, k, p, n):
        """Rama k que impone v_p - v_n = b[k]; su corriente va de p a n por dentro."""
        M = self.M
        M[p, k] += 1.0
        M[n, k] -= 1.0
        M[k, p] += 1.0
        M[k, n] -= 1.0


class Elemento(Bloque):
    """Base de un elemento eléctrico. Sus nodos llegan en self.n[puerto]."""

    categoria = "Eléctrica"
    color = "electrica"
    n_ramas = 0
    n_internos = 0
    no_lineal = False
    dinamico = False            # su matriz depende de una entrada (requiere resolver cada vez)
    con_estado_discreto = False # sus interruptores cambian la matriz
    entradas_compuerta = ()     # entradas que solo se leen al final de cada paso
    entradas_vector = {}        # entradas con tres componentes

    def estampar_fijo(self, S):
        pass

    def estampar_estado(self, S):
        pass

    def clave(self):
        return 0

    def estampar_din(self, M, b, z, t, x, u):
        pass

    def rhs(self, b, t, x, u):
        pass

    def derivada_e(self, z, t, x, u):
        return []

    def salida_e(self, z, t, x, u):
        return []

    def actualizar(self, z, t, x, u):
        """Revisa los interruptores; devuelve True si cambió algo."""
        return False

    cruce_e = None   # señales de cruce por cero del elemento: cruce_e(z, t, x, u) -> lista

    def tras_paso(self, z, t, x, u):
        """Al terminar un paso aceptado (pérdidas, muestreo)."""

    def reiniciar_e(self):
        pass


class Red(Bloque):
    """Reúne los elementos en un solo sistema de ecuaciones."""

    tipo = "_red"
    nombre = "Red eléctrica"
    directo = True

    def __init__(self, elementos, conexiones, instancias):
        self.etiqueta = "Red eléctrica"
        self.id = "_red"
        self.crudos = {}
        self.p = {}
        self.espacio = {}
        self.elementos = elementos
        self._numerar(conexiones, instancias)
        self._preparar_matrices()
        self._analizar_directas()

    # ---------------------------------------------------------------- numeración
    def _numerar(self, conexiones, instancias):
        padre = {}

        def raiz(a):
            padre.setdefault(a, a)
            while padre[a] != a:
                padre[a] = padre[padre[a]]
                a = padre[a]
            return a

        def llaves(bid, puerto):
            pu = instancias[bid].puertos(instancias[bid].crudos)
            if puerto in pu["e3"]:
                return [(bid, puerto, f) for f in range(3)]
            return [(bid, puerto, None)]

        for (a, c) in conexiones:
            la, lc = llaves(*a), llaves(*c)
            if len(la) != len(lc):
                raise ValueError(
                    f"No se puede unir un puerto monofásico con uno trifásico "
                    f"({instancias[a[0]].etiqueta} – {instancias[c[0]].etiqueta})")
            for p, q in zip(la, lc):
                padre[raiz(p)] = raiz(q)

        def raiz_de(bid, puerto):
            return raiz(llaves(bid, puerto)[0])

        for e in self.elementos:
            e.red = self
            if hasattr(e, "antes_de_numerar"):
                e.antes_de_numerar(self.elementos, raiz_de)

        # La tierra fija el nodo de referencia
        tierras = set()
        for e in self.elementos:
            if getattr(e, "es_tierra", False):
                tierras.add(raiz((e.id, "t", None)))

        numeros = {}

        def nodo(k):
            r = raiz(k)
            if r in tierras:
                return -1
            if r not in numeros:
                numeros[r] = len(numeros)
            return numeros[r]

        for e in self.elementos:
            pu = e.puertos(e.crudos)
            e.n = {}
            for q in pu["e"]:
                e.n[q] = nodo((e.id, q, None))
            for q in pu["e3"]:
                e.n[q] = tuple(nodo((e.id, q, f)) for f in range(3))
        nn = len(numeros)
        for e in self.elementos:
            e.internos = list(range(nn, nn + e.n_internos))
            nn += e.n_internos
        self.n_nodos = nn
        k = nn
        for e in self.elementos:
            e.ramas = list(range(k, k + e.n_ramas))
            k += e.n_ramas
        self.tam = k

        self.entradas_lista = []
        self.salidas_lista = []
        self._salida_idx = {}
        xpos = upos = 0
        for e in self.elementos:
            pu = e.puertos(e.crudos)
            e.xa, xpos = xpos, xpos + e.n_estados()
            e.xb = xpos
            e.ua = upos
            for q in pu["entradas"]:
                self.entradas_lista.append((e, q))
                upos += 1
            e.ub = upos
            e.ya = len(self.salidas_lista)
            for q in pu["salidas"]:
                self._salida_idx[(e.id, q)] = len(self.salidas_lista)
                self.salidas_lista.append((e, q))
            e.yb = len(self.salidas_lista)
        self.nx = xpos

    def indice_salida(self, bid, puerto):
        i = self._salida_idx.get((bid, puerto))
        if i is None:
            raise ValueError(f"El elemento {bid} no tiene la salida «{puerto}»")
        return i

    def _preparar_matrices(self):
        n = self.tam + 1  # el último es la tierra
        M = np.zeros((n, n))
        S = Estampa(M)
        for i in range(self.n_nodos):
            M[i, i] += GMIN
        for e in self.elementos:
            e.estampar_fijo(S)
        self.avisos = []
        self._revisar_nodos(M)
        self.M_fijo = M
        self._cache = {}
        self.con_rhs = self.elementos
        self.conmutados = [e for e in self.elementos if e.con_estado_discreto]
        self.din = [e for e in self.elementos if e.dinamico or e.no_lineal]
        self.hay_no_lineal = any(e.no_lineal for e in self.elementos)
        self.con_salida = [e for e in self.elementos if e.yb > e.ya]
        self.z = np.zeros(n)
        self.b = np.zeros(n)
        self._clave_resolver = None  # memo de una entrada: (t, x, u) del último solve
        self._usar_clave()

    def _revisar_nodos(self, M):
        """Un nodo al que solo llegan corrientes impuestas (bobinas en serie) no tiene tensión
        definida. Se le añade una resistencia a tierra alta y se avisa."""
        prueba = M.copy()
        S = Estampa(prueba)
        for e in self.elementos:
            if e.con_estado_discreto:
                e.clave()
                e.estampar_estado(S)
        exentos = set()
        for e in self.elementos:
            if e.dinamico or e.no_lineal:
                for v in e.n.values():
                    exentos.update(v if isinstance(v, tuple) else (v,))
        for i in range(self.n_nodos):
            if i in exentos:
                continue
            fila = prueba[i, :-1].copy()
            fila[i] = 0.0
            if abs(prueba[i, i] - GMIN) < 1e-14 and not np.any(fila):
                M[i, i] += 0.1
                self.avisos.append(
                    "Un nodo solo une bobinas o fuentes de corriente en serie: se añadió 10 Ω a tierra "
                    "para definir su tensión (requiere pasos pequeños). Conecta un condensador o una resistencia.")

    def _usar_clave(self):
        clave = tuple(e.clave() for e in self.conmutados)
        d = self._cache.get(clave)
        if d is None:
            M = self.M_fijo.copy()
            S = Estampa(M)
            for e in self.conmutados:
                e.estampar_estado(S)
            inv = None if self.din else np.linalg.inv(M[:-1, :-1])
            d = (M, inv)
            self._cache[clave] = d
        self.M, self.Minv = d
        self._clave_resolver = None  # cambió la matriz: la caché del solve ya no vale

    def _resolver(self, t, x, u):
        # f(t,x) resuelve la red dos veces con los mismos (t, x, u): una en calcular_salidas
        # y otra en derivada. Una caché de una entrada evita repetir el solve (y, si es no
        # lineal, toda su iteración). Se invalida al cambiar el estado de los interruptores
        # (_usar_clave) o al reiniciar, que son las únicas vías por las que cambia self.M.
        # La clave serializa cada entrada por separado: u puede ser irregular (escalares y
        # vectores trifásicos mezclados), así que no se fuerza a un arreglo homogéneo.
        clave = (t, np.asarray(x, dtype=float).tobytes(),
                 tuple(np.asarray(v, dtype=float).tobytes() for v in u))
        if clave == self._clave_resolver:
            return self.z
        b = self.b
        b[:] = 0.0
        for e in self.con_rhs:
            e.rhs(b, t, x[e.xa:e.xb], u[e.ua:e.ub])
        z = self.z
        if not self.din:
            z[:-1] = self.Minv.dot(b[:-1])
            self._clave_resolver = clave
            return z
        for _ in range(30 if self.hay_no_lineal else 1):
            M = self.M.copy()
            bb = b.copy()
            for e in self.din:
                e.estampar_din(M, bb, z, t, x[e.xa:e.xb], u[e.ua:e.ub])
            nuevo = np.linalg.solve(M[:-1, :-1], bb[:-1])
            error = np.max(np.abs(nuevo - z[:-1])) if self.hay_no_lineal else 0.0
            z[:-1] = nuevo
            if error < 1e-8:
                break
        self._clave_resolver = clave
        return z

    # ---------------------------------------------------------------- interfaz de bloque
    def n_estados(self):
        return self.nx

    def x0(self):
        v = []
        for e in self.elementos:
            v.extend(e.x0())
        return v

    def reiniciar(self):
        for e in self.elementos:
            e.reiniciar()
            e.reiniciar_e()
        self.z[:] = 0.0
        self._usar_clave()

    def calcular_salidas(self, t, x, u, Y):
        self.x_actual = x
        z = self._resolver(t, x, u)
        for e in self.con_salida:
            Y[e.ya:e.yb] = e.salida_e(z, t, x[e.xa:e.xb], u[e.ua:e.ub])

    def derivada(self, t, x, u):
        z = self._resolver(t, x, u)
        dx = np.empty(self.nx)
        for e in self.elementos:
            if e.xb > e.xa:
                dx[e.xa:e.xb] = e.derivada_e(z, t, x[e.xa:e.xb], u[e.ua:e.ub])
        return dx

    def paso_mayor(self, t, x, u, y):
        """Lee las compuertas y busca el estado coherente de los interruptores."""
        cambio = False
        if self.conmutados:
            for _ in range(12):
                z = self._resolver(t, x, u)
                hubo = False
                for e in self.conmutados:
                    if e.actualizar(z, t, x[e.xa:e.xb], u[e.ua:e.ub]):
                        hubo = True
                if not hubo:
                    break
                cambio = True
                self._usar_clave()
        z = self._resolver(t, x, u)
        for e in self.elementos:
            e.tras_paso(z, t, x[e.xa:e.xb], u[e.ua:e.ub])
        return cambio

    def proximo_evento(self, t):
        m = np.inf
        for e in self.elementos:
            v = e.proximo_evento(t)
            if v < m:
                m = v
        return m

    # ---------------------------------------------------------------- dependencias directas
    def _analizar_directas(self):
        """¿Qué salidas (sensores) dependen de qué entradas en el mismo instante?

        Se prueba numéricamente: se cambia cada entrada y se mira si cambia cada sensor.
        Las compuertas no cuentan porque solo actúan al final de cada paso. Esto evita
        falsos lazos algebraicos: una corriente de bobina medida es un estado, no depende
        de la tensión aplicada en el mismo instante.
        """
        def base(e, q):
            return np.full(3, 0.5) if q in e.entradas_vector else 0.5

        u0 = [base(e, q) for e, q in self.entradas_lista]
        x = np.ones(self.nx) * 0.7
        self.salidas_directas = set()
        self._entradas_directas = []
        if not self.salidas_lista or not self.entradas_lista:
            self.hay_directas = False
            return
        Y0 = [0.0] * len(self.salidas_lista)
        self.calcular_salidas(0.0, x, u0, Y0)
        for i, (e, q) in enumerate(self.entradas_lista):
            if q in e.entradas_compuerta:
                continue
            u = list(u0)
            u[i] = u0[i] + 0.13
            Y = [0.0] * len(self.salidas_lista)
            self.calcular_salidas(0.0, x, u, Y)
            afecta = False
            for j in range(len(Y)):
                if np.max(np.abs(np.asarray(Y[j], dtype=float) - np.asarray(Y0[j], dtype=float))) > 1e-9:
                    self.salidas_directas.add(j)
                    afecta = True
            if afecta:
                self._entradas_directas.append(i)
        self.z[:] = 0.0
        self.hay_directas = bool(self.salidas_directas)

    def entradas_directas(self):
        return self._entradas_directas

    def tiene_cruce(self):
        return any(e.cruce_e is not None for e in self.elementos)

    def cruce(self, t, x, u):
        z = self._resolver(t, x, u)
        g = []
        for e in self.elementos:
            if e.cruce_e is not None:
                g.extend(e.cruce_e(z, t, x[e.xa:e.xb], u[e.ua:e.ub]))
        return g
