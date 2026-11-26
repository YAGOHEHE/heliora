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

Modo DAE (`Red(..., dae=True)`, solver «dae»): los almacenes (condensadores, inercias, capacidades
térmicas) dejan de ser fuentes. Cada uno declara en `estados_en_rama()` qué estado fija qué rama de
tensión; esa rama pasa a ser la ecuación diferencial

    d(fila_k · z)/dt = derivada del estado,

con fila_k la fila de la rama en la matriz fija (la tensión que antes imponía). El estado deja de
existir como tal: se reconstruye de z. Así dos almacenes en paralelo comparten la misma tensión y la
red ya no es singular. El solver de `dae.py` resuelve z junto con los estados del diagrama.
"""
from __future__ import annotations

from typing_extensions import override
import numpy as np

from .nucleo import Bloque

GMIN = 1e-12  # conductancia mínima de cada nodo a tierra (evita nodos flotantes)

# Iteración de los elementos no lineales: converge cuando el cambio entre iterados baja de
# TOL_NO_LINEAL. Si se agotan las iteraciones, el último iterado solo se acepta si su residuo
# escalado (|M(z)·z - b(z)| frente a la magnitud de los términos de cada ecuación) es menor que
# TOL_RESIDUO; si no, la evaluación falla con un error.
MAX_ITER_NO_LINEAL = 30
TOL_NO_LINEAL = 1e-8
TOL_RESIDUO = 1e-9
# Intentos de encontrar un estado coherente de los interruptores en un mismo instante
MAX_ITER_CONMUTADORES = 12

# Al reducir la red a dx/dt = f(t, x), cada condensador, inercia o capacidad térmica es una fuente de
# tensión: si dos quedan en paralelo (directamente o por un transformador o engranaje ideal) su estado
# está duplicado y la matriz es singular.
MENSAJE_SINGULAR = (
    "La red no tiene solución única: hay fuentes de tensión, condensadores, inercias, capacidades térmicas o "
    "fuentes de velocidad en paralelo, directamente o a través de un transformador o un engranaje ideal. "
    "Deja solo una (suma las inercias, refiriéndolas con n² si hay engranaje) o separa las dos con una "
    "resistencia, una fricción o un resorte.")


def _residuo_relativo(A, z, c):
    """Máximo de |A·z - c| relativo a la magnitud de los términos de cada ecuación."""
    if not A.size:
        return 0.0
    r = np.abs(A @ z - c)
    return float(np.max(r / np.maximum(np.abs(A) @ np.abs(z) + np.abs(c), 1e-300)))


def _resolver_o_aproximar(A, c):
    """Solución de A·z = c; si A es singular (almacenes unidos rígidamente), la de mínimos cuadrados."""
    if not A.size:
        return np.zeros(0)
    try:
        z = np.linalg.solve(A, c)
        if np.all(np.isfinite(z)) and _residuo_relativo(A, z, c) <= 1e-9:
            return z
    except np.linalg.LinAlgError:
        pass
    return np.linalg.lstsq(A, c, rcond=None)[0]


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
    # Dominio físico de cada puerto que no sea eléctrico ("termico", "mecanico"). La misma MNA
    # resuelve todos los dominios por analogía, pero un nodo no puede mezclar dos de ellos.
    dominio_puertos = {}
    # Puertos que pueden quedar sin conectar: entonces no crean nodo y `self.conectados[q]` es False
    puertos_opcionales = ()

    def estampar_fijo(self, S):
        """Escribe en la matriz M las estampas que no cambian durante la simulación.

        Args:
            S: `Estampa` sobre M; los nodos del elemento están en `self.n`.
        """

    def estampar_estado(self, S):
        """Escribe en la matriz M las estampas que dependen del estado discreto (interruptores).

        Args:
            S: `Estampa` sobre M; se llama solo si `con_estado_discreto` es True.
        """

    def clave(self):
        """Devuelve el valor hashable que identifica el estado discreto actual de la matriz M.

        La red guarda una factorización por cada combinación de claves de sus elementos con
        `con_estado_discreto`.
        """
        return 0

    def estampar_din(self, M, b, z, t, x, u):
        """Añade a M y b las estampas que dependen de la solución z (elementos no lineales o dinámicos).

        Args:
            M: matriz de la red, copia sobre la que se escribe.
            b: término independiente, copia sobre la que se escribe.
            z: solución vigente, con tensiones de nodo y corrientes de rama.
            t: tiempo de simulación, en s.
            x: estados del elemento.
            u: entradas del elemento.
        """

    def rhs(self, b, t, x, u):
        """Acumula en el término independiente b la contribución del elemento.

        Args:
            b: término independiente de la red, indexado como z.
            t: tiempo de simulación, en s.
            x: estados del elemento.
            u: entradas del elemento.
        """

    def derivada_e(self, z, t, x, u):
        """Calcula la derivada temporal de los estados del elemento.

        Args:
            z: solución de la red, con tensiones de nodo y corrientes de rama.
            t: tiempo de simulación, en s.
            x: estados del elemento.
            u: entradas del elemento.

        Returns:
            Lista de dx/dt, de longitud `n_estados()`.
        """
        return []

    def salida_e(self, z, t, x, u):
        """Calcula las salidas del elemento a partir de la solución de la red.

        Args:
            z: solución de la red, con tensiones de nodo y corrientes de rama.
            t: tiempo de simulación, en s.
            x: estados del elemento.
            u: entradas del elemento.

        Returns:
            Lista con un valor por salida, en el orden de `salidas`.
        """
        return []

    def actualizar(self, z, t, x, u):
        """Revisa los interruptores; devuelve True si cambió algo."""
        return False

    cruce_e = None   # señales de cruce por cero del elemento: cruce_e(z, t, x, u) -> lista

    def tras_paso(self, z, t, x, u):
        """Al terminar un paso aceptado (pérdidas, muestreo).

        Devuelve True si el elemento cambió algún valor de `estampar_estado` (por ejemplo, una
        resistencia que depende de la temperatura): la red vuelve entonces a factorizar sus matrices.
        Los valores de `estampar_fijo` no pueden cambiar durante la simulación.
        """

    def reiniciar_e(self):
        """Restablece el estado del elemento ligado a la red; la red lo llama tras `reiniciar`."""

    def estados_en_rama(self):
        """Estados que fijan el valor de una rama de tensión, para el modo DAE.

        Returns:
            Diccionario {índice local del estado: índice local de la rama}. Cada rama debe recibir
            en `rhs` exactamente ese estado (b[rama] = x[estado]). Vacío si el elemento no almacena.
        """
        return {}


class Red(Bloque):
    """Reúne los elementos en un solo sistema de ecuaciones."""

    tipo = "_red"
    nombre = "Red eléctrica"
    directo = True

    def __init__(self, elementos, conexiones, instancias, dae=False):
        self.etiqueta = "Red eléctrica"
        self.id = "_red"
        self.crudos = {}
        self.p = {}
        self.espacio = {}
        self.elementos = elementos
        self.dae = dae
        self._numerar(conexiones, instancias)
        self._preparar_matrices()
        if dae:
            self._preparar_dae()
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

        # Cuántos puertos de elementos llegan a cada nodo, y de qué dominio son
        cuenta, dominio = {}, {}
        for e in self.elementos:
            pu = e.puertos(e.crudos)
            for q in pu["e"] + pu["e3"]:
                d = e.dominio_puertos.get(q, "eléctrico")
                for k in llaves(e.id, q):
                    r = raiz(k)
                    cuenta[r] = cuenta.get(r, 0) + 1
                    previo = dominio.setdefault(r, (d, e.etiqueta))
                    if previo[0] != d:
                        raise ValueError(f"No se puede unir un puerto {previo[0]} ({previo[1]}) con uno {d} ({e.etiqueta})")
        for e in self.elementos:
            pu = e.puertos(e.crudos)
            e.conectados = {q: cuenta.get(raiz(llaves(e.id, q)[0]), 0) > 1 for q in pu["e"] + pu["e3"]}

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
                # Un puerto opcional suelto no crea nodo: la red queda igual que sin ese puerto
                e.n[q] = -1 if (q in e.puertos_opcionales and not e.conectados[q]) else nodo((e.id, q, None))
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
        """Devuelve el índice de la salida `puerto` del elemento `bid` en `salidas_lista`."""
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
        self.version = 0             # cambia cuando `tras_paso` cambia los valores de las matrices
        self._usar_clave()

    def _revisar_nodos(self, M):
        """Añade 10 Ω a tierra y un aviso en los nodos sin tensión definida.

        Un nodo al que solo llegan corrientes impuestas (bobinas en serie) no tiene tensión definida.
        """
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
            try:
                # En modo DAE la red no se invierte: la resuelve el solver (y, para ser coherente
                # tras un cambio, mínimos cuadrados en `consistente`)
                inv = None if (self.din or self.dae) else np.linalg.inv(M[:-1, :-1])
            except np.linalg.LinAlgError:
                raise ValueError(MENSAJE_SINGULAR) from None
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
            self._comprobar_finita(z, t)
            self._clave_resolver = clave
            return z
        for _ in range(MAX_ITER_NO_LINEAL if self.hay_no_lineal else 1):
            M, bb = self._estampar_din(z, b, t, x, u)
            try:
                nuevo = np.linalg.solve(M[:-1, :-1], bb[:-1])
            except np.linalg.LinAlgError:
                raise ValueError(MENSAJE_SINGULAR) from None
            error = np.max(np.abs(nuevo - z[:-1])) if self.hay_no_lineal else 0.0
            z[:-1] = nuevo
            self._comprobar_finita(z, t)
            if error < TOL_NO_LINEAL:
                break
        else:
            # Se agotaron las iteraciones: el último iterado solo vale si cumple las ecuaciones
            residuo = self._residuo_escalado(z, b, t, x, u)
            if residuo > TOL_RESIDUO:
                raise ValueError(
                    f"La red no lineal no convergió en t = {t:.6g} s: tras {MAX_ITER_NO_LINEAL} iteraciones el último "
                    f"cambio es {error:.3g} y el residuo escalado de sus ecuaciones {residuo:.3g} "
                    f"(tolerancia {TOL_RESIDUO:g}). Elementos no lineales: "
                    + ", ".join(e.etiqueta for e in self.din if e.no_lineal)[:300])
        self._clave_resolver = clave
        return z

    def _estampar_din(self, z, b, t, x, u):
        """Matriz y término independiente con las estampas que dependen de la solución z."""
        M = self.M.copy()
        bb = b.copy()
        for e in self.din:
            e.estampar_din(M, bb, z, t, x[e.xa:e.xb], u[e.ua:e.ub])
        return M, bb

    def _residuo_escalado(self, z, b, t, x, u):
        """Máximo de |M(z)·z - b(z)| relativo a la magnitud de los términos de cada ecuación."""
        M, bb = self._estampar_din(z, b, t, x, u)
        A, zz, c = M[:-1, :-1], z[:-1], bb[:-1]
        r = np.abs(A @ zz - c)
        escala = np.abs(A) @ np.abs(zz) + np.abs(c)
        return float(np.max(r / np.maximum(escala, 1e-300))) if r.size else 0.0

    @staticmethod
    def _comprobar_finita(z, t):
        if not np.all(np.isfinite(z)):
            raise ValueError(f"La solución de la red dejó de ser finita en t = {t:.6g} s "
                             "(una entrada o un parámetro no finito, o una red mal condicionada).")

    # ---------------------------------------------------------------- modo DAE
    def _preparar_dae(self):
        """Localiza los almacenes y separa sus estados de los que siguen siendo diferenciales."""
        almacen = []   # (índice en x completo, rama k, elemento, estado local)
        for e in self.elementos:
            for i, r in e.estados_en_rama().items():
                almacen.append((e.xa + i, e.ramas[r], e, i))
        self.almacen = almacen
        de_almacen = {j for j, _, _, _ in almacen}
        self.idx_libres = np.array([j for j in range(self.nx) if j not in de_almacen], dtype=int)
        self.idx_alm = np.array([j for j, _, _, _ in almacen], dtype=int)
        self.ramas_alm = np.array([k for _, k, _, _ in almacen], dtype=int)
        # La fila de cada rama de almacén es la tensión que fijaba: con ella se reconstruye el estado
        self.filas_alm = self.M_fijo[self.ramas_alm, :].copy() if almacen else np.zeros((0, self.tam + 1))
        self.nx_dae = int(self.idx_libres.size)
        self.z_ext = np.zeros(self.tam + 1)
        self.x_alm_ini = np.zeros(len(almacen))
        self.pendiente = True
        self.avisos_dae = set()

    def _x_completo(self, xr, z=None):
        """Estados de todos los elementos: los libres de `xr` y los de almacén reconstruidos de z."""
        xf = np.empty(self.nx)
        xf[self.idx_libres] = xr
        if self.almacen:
            xf[self.idx_alm] = self.filas_alm @ (self.z_ext if z is None else z)
        return xf

    def fijar_z(self, z):
        """Fija la solución de la red que proponen el solver o la interpolación (sin la tierra)."""
        self.z_ext[:-1] = z
        self.z_ext[-1] = 0.0

    def masa(self):
        """Matriz E de ∂R/∂ż para las incógnitas de la red (sin la tierra): filas de las ramas de almacén."""
        E = np.zeros((self.tam, self.tam))
        for f, k in enumerate(self.ramas_alm):
            E[k, :] = self.filas_alm[f, :-1]
        return E

    def diferenciales(self):
        """Máscara de las incógnitas de la red que aparecen derivadas (columnas no nulas de E)."""
        return np.any(self.masa() != 0.0, axis=0)

    def residuo(self, t, xr, u, zp):
        """Residuo de la red para el solver DAE: M(z)·z - b(z) y, en las ramas de almacén, d(fila·z)/dt - derivada.

        Args:
            t: tiempo, en s.
            xr: estados libres de la red (sin los de almacén).
            u: entradas de la red.
            zp: derivada temporal de z (sin la tierra).

        Returns:
            Vector de residuos, uno por incógnita de la red.
        """
        z = self.z_ext
        xf = self._x_completo(xr)
        b = np.zeros(self.tam + 1)
        for e in self.elementos:
            e.rhs(b, t, xf[e.xa:e.xb], u[e.ua:e.ub])
        if self.din:
            M, b = self._estampar_din(z, b, t, xf, u)
        else:
            M = self.M
        R = M[:-1, :] @ z - b[:-1]
        if self.almacen:
            d = {}
            for f, (_, k, e, i) in enumerate(self.almacen):
                if e not in d:
                    d[e] = e.derivada_e(z, t, xf[e.xa:e.xb], u[e.ua:e.ub])
                R[k] = float(self.filas_alm[f, :-1] @ zp) - float(d[e][i])
        return R

    def _pesos_almacen(self, t, xf, u):
        """Capacidad de cada almacén (C, J o capacidad térmica): la inversa de ∂(derivada)/∂(corriente de su rama)."""
        z = self.z_ext.copy()
        pesos = np.ones(len(self.almacen))
        for f, (_, k, e, i) in enumerate(self.almacen):
            d0 = float(e.derivada_e(z, t, xf[e.xa:e.xb], u[e.ua:e.ub])[i])
            z[k] += 1.0
            d1 = float(e.derivada_e(z, t, xf[e.xa:e.xb], u[e.ua:e.ub])[i])
            z[k] -= 1.0
            if d1 != d0 and np.isfinite(d1 - d0):
                pesos[f] = abs(1.0 / (d1 - d0))
        return pesos

    def _proyectar(self, A, c, pesos):
        """Cumple exactamente las ecuaciones que no son de almacén y, de las de almacén, lo más posible.

        Minimiza Σ C_i·(valor_i - fijado_i)² con el resto de la red como restricción: dos condensadores
        en paralelo quedan con la tensión que conserva la carga; dos inercias por un engranaje, con la
        velocidad que conserva el momento angular referido al eje.
        """
        n = A.shape[0]
        alm = np.zeros(n, dtype=bool)
        alm[self.ramas_alm] = True
        S, v = A[alm], c[alm]
        Aa, ca = A[~alm], c[~alm]
        W = np.diag(pesos)
        K = np.block([[S.T @ W @ S, Aa.T], [Aa, np.zeros((Aa.shape[0], Aa.shape[0]))]])
        rhs = np.concatenate([S.T @ W @ v, ca])
        return np.linalg.lstsq(K, rhs, rcond=None)[0][:n]

    def consistente(self, t, xr, u, inicial=False):
        """Resuelve la red con cada almacén fijado a su valor actual (o al inicial) y la deja en z_ext.

        Es la red de siempre (almacenes como fuentes). Si dos almacenes unidos rígidamente tienen
        valores incompatibles, se proyectan conservando la carga, el momento o el calor (`_proyectar`)
        y se avisa.
        """
        xf = self._x_completo(xr)
        if inicial and self.almacen:
            xf[self.idx_alm] = self.x_alm_ini
        z = self.z_ext
        b = np.zeros(self.tam + 1)
        for e in self.elementos:
            e.rhs(b, t, xf[e.xa:e.xb], u[e.ua:e.ub])
        proyectado = False
        for _ in range(MAX_ITER_NO_LINEAL if self.hay_no_lineal else 1):
            M, bb = self._estampar_din(z, b, t, xf, u) if self.din else (self.M, b)
            A, c = M[:-1, :-1], bb[:-1]
            nuevo = _resolver_o_aproximar(A, c)
            if A.size and self.almacen and _residuo_relativo(A, nuevo, c) > 1e-9:
                if not hasattr(self, "_pesos"):
                    self._pesos = self._pesos_almacen(t, xf, u)
                nuevo = self._proyectar(A, c, self._pesos)
                proyectado = True
            cambio = np.max(np.abs(nuevo - z[:-1])) if nuevo.size else 0.0
            z[:-1] = nuevo
            self._comprobar_finita(z, t)
            if cambio < TOL_NO_LINEAL * max(1.0, float(np.max(np.abs(nuevo))) if nuevo.size else 1.0):
                break
        if proyectado and _residuo_relativo(A, z[:-1], c) > 1e-6:
            self.avisos_dae.add(
                "Hay almacenes unidos rígidamente (condensadores en paralelo, inercias por un engranaje) con "
                "valores iniciales incompatibles: se igualaron conservando la carga, el momento angular o el calor.")
        self.pendiente = False
        return z

    # ---------------------------------------------------------------- interfaz de bloque
    @override
    def n_estados(self):
        return self.nx_dae if self.dae else self.nx

    @override
    def x0(self):
        v = []
        for e in self.elementos:
            v.extend(e.x0())
        if self.dae:
            v = np.asarray(v, dtype=float)
            self.x_alm_ini = v[self.idx_alm].copy() if self.almacen else np.zeros(0)
            return list(v[self.idx_libres])
        return v

    @override
    def reiniciar(self):
        for e in self.elementos:
            e.reiniciar()
            e.reiniciar_e()
        self.z[:] = 0.0
        if self.dae:
            self.z_ext[:] = 0.0
            self.pendiente = True
        self._usar_clave()

    def _solucion(self, t, x, u):
        """(z, estados completos de los elementos): resuelta por la red o, en modo DAE, la del solver."""
        if not self.dae:
            return self._resolver(t, x, u), x
        if self.pendiente:
            self.consistente(t, x, u, inicial=True)
        return self.z_ext, self._x_completo(x)

    def calcular_salidas(self, t, x, u, Y):
        """Resuelve la red y escribe en Y[e.ya:e.yb] las salidas de cada elemento."""
        # Estados completos: algunos elementos leen los de un vecino (red infinita con su filtro)
        self.x_actual = x
        z, xf = self._solucion(t, x, u)
        self.x_actual = xf
        for e in self.con_salida:
            Y[e.ya:e.yb] = e.salida_e(z, t, xf[e.xa:e.xb], u[e.ua:e.ub])

    @override
    def derivada(self, t, x, u):
        z, xf = self._solucion(t, x, u)
        dx = np.empty(self.nx)
        for e in self.elementos:
            if e.xb > e.xa:
                dx[e.xa:e.xb] = e.derivada_e(z, t, xf[e.xa:e.xb], u[e.ua:e.ub])
        return dx[self.idx_libres] if self.dae else dx

    def paso_mayor(self, t, x, u, y):
        """Lee las compuertas y busca el estado coherente de los interruptores."""
        if self.dae:
            return self._paso_mayor_dae(t, x, u)
        cambio = False
        if self.conmutados:
            for _ in range(MAX_ITER_CONMUTADORES):
                z = self._resolver(t, x, u)
                cambiados = [e for e in self.conmutados if e.actualizar(z, t, x[e.xa:e.xb], u[e.ua:e.ub])]
                if not cambiados:
                    break
                cambio = True
                self._usar_clave()
            else:
                raise ValueError(
                    f"Los interruptores no encuentran un estado coherente en t = {t:.6g} s: tras "
                    f"{MAX_ITER_CONMUTADORES} intentos siguen cambiando "
                    + ", ".join(e.etiqueta for e in cambiados)[:300]
                    + ". Revisa si dos interruptores se contradicen (por ejemplo, un diodo en antiparalelo con una fuente).")
        z = self._resolver(t, x, u)
        estampa = False
        for e in self.elementos:
            if e.tras_paso(z, t, x[e.xa:e.xb], u[e.ua:e.ub]):
                estampa = True
        if estampa:
            # Cambió algún valor de `estampar_estado`: las factorizaciones guardadas ya no valen
            self._cache = {}
            self.version += 1
            self._usar_clave()
            cambio = True
        return cambio

    def _paso_mayor_dae(self, t, xr, u):
        """Atiende interruptores y `tras_paso` en modo DAE.

        Tras cada cambio la red se vuelve a hacer coherente con los almacenes en su valor actual: los
        almacenes no saltan; las corrientes y tensiones libres sí.
        """
        cambio = False
        if self.pendiente:
            self.consistente(t, xr, u, inicial=True)
        if self.conmutados:
            for _ in range(MAX_ITER_CONMUTADORES):
                xf = self._x_completo(xr)
                cambiados = [e for e in self.conmutados
                             if e.actualizar(self.z_ext, t, xf[e.xa:e.xb], u[e.ua:e.ub])]
                if not cambiados:
                    break
                cambio = True
                self._usar_clave()
                self.consistente(t, xr, u)
            else:
                raise ValueError(
                    f"Los interruptores no encuentran un estado coherente en t = {t:.6g} s: tras "
                    f"{MAX_ITER_CONMUTADORES} intentos siguen cambiando "
                    + ", ".join(e.etiqueta for e in cambiados)[:300]
                    + ". Revisa si dos interruptores se contradicen (por ejemplo, un diodo en antiparalelo con una fuente).")
        xf = self._x_completo(xr)
        estampa = False
        for e in self.elementos:
            if e.tras_paso(self.z_ext, t, xf[e.xa:e.xb], u[e.ua:e.ub]):
                estampa = True
        if estampa:
            self._cache = {}
            self.version += 1
            self._usar_clave()
            self.consistente(t, xr, u)
            cambio = True
        return cambio

    @override
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
        x = np.ones(self.n_estados()) * 0.7
        if self.dae:
            # En modo DAE z es una incógnita del solver: solo cuenta lo que la salida lee de u directamente
            self.pendiente = False
            self.z_ext[:-1] = 0.3
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
        if self.dae:
            self.z_ext[:] = 0.0
            self.pendiente = True
        self.hay_directas = bool(self.salidas_directas)

    def entradas_directas(self):
        """Devuelve los índices de las entradas que afectan a alguna salida en el mismo instante."""
        return self._entradas_directas

    @override
    def tiene_cruce(self):
        return any(e.cruce_e is not None for e in self.elementos)

    @override
    def cruce(self, t, x, u):
        z, xf = self._solucion(t, x, u)
        g = []
        for e in self.elementos:
            if e.cruce_e is not None:
                g.extend(e.cruce_e(z, t, xf[e.xa:e.xb], u[e.ua:e.ub]))
        return g
