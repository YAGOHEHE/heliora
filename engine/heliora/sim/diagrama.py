"""Del diagrama JSON al modelo que integran los solvers.

Pasos de compilación:
1. Aplanar los subsistemas, sustituyendo puertos por sus conexiones reales.
2. Construir la red eléctrica: unir nodos conectados y crear un único bloque de red.
3. Ordenar los bloques: una salida se calcula antes que quien la usa directamente.
   Si hay un ciclo de dependencias directas, es un lazo algebraico y se avisa.
"""
from __future__ import annotations

import math
import time

import numpy as np

from .nucleo import REGISTRO, evaluar_numero
from . import bloques_genericos  # noqa: F401 (registra los bloques)
from .bloques_genericos import AEspacio, Osciloscopio
from .lazos import SinSolucion, componentes_fuertes, resolver as resolver_no_lineal
from .solvers import integrar_modelo

FORMATO = "heliora-diagrama/1"


class ErrorDiagrama(ValueError):
    pass


def cargar_librerias():
    """Importa las librerías para que registren sus bloques."""
    import importlib
    for nombre in ("electrica", "potencia", "fotovoltaica"):
        try:
            importlib.import_module(f"..lib.{nombre}", __package__)
        except ModuleNotFoundError as e:
            if nombre not in str(e):
                raise


def _aplanar(sistema, prefijo, bloques, conexiones, alias):
    """Copia bloques y conexiones a listas planas y anota los alias de los subsistemas."""
    for b in sistema.get("bloques", []):
        bid = prefijo + b["id"]
        if b["tipo"] == "subsistema":
            interior = b.get("sistema") or {"bloques": [], "conexiones": []}
            _aplanar(interior, bid + "/", bloques, conexiones, alias)
            for ib in interior.get("bloques", []):
                num = int(float((ib.get("params") or {}).get("numero", 1)))
                iid = f"{bid}/{ib['id']}"
                if ib["tipo"] == "entrada":
                    alias[(bid, f"in{num}")] = (iid, "y")
                elif ib["tipo"] == "salida":
                    alias[(bid, f"out{num}")] = (iid, "u")
                elif ib["tipo"] in ("puerto_e", "puerto_e3"):
                    alias[(bid, f"e{num}")] = (iid, "e")
        else:
            d = dict(b)
            d["id"] = bid
            bloques.append(d)
    for c in sistema.get("conexiones", []):
        conexiones.append(((prefijo + c["de"][0], c["de"][1]), (prefijo + c["a"][0], c["a"][1])))


def _vectorizar(valores):
    """Convierte una lista de escalares y arreglos en un vector, y anota sus formas."""
    formas = [np.shape(v) for v in valores]
    if not valores:
        return np.zeros(0), formas
    return np.concatenate([np.ravel(np.asarray(v, dtype=float)) for v in valores]), formas


def _desaplanar(z, formas):
    salida, i = [], 0
    for f in formas:
        n = int(np.prod(f)) if f else 1
        salida.append(float(z[i]) if not f else z[i:i + n].reshape(f))
        i += n
    return salida


class _Lazo:
    """Un lazo algebraico: se corta en uno o varios bloques y se resuelve en cada evaluación."""

    def __init__(self, modelo, comp, depende):
        self.m = modelo
        self.comp = comp
        conj = set(comp)
        # Un bloque que se alimenta a sí mismo se corta en él mismo
        cortes, orden = (list(comp) if len(comp) == 1 else []), []
        pendientes = set(comp)
        while pendientes:
            libres = sorted(k for k in pendientes if not ((depende[k] & pendientes) - set(cortes)))
            if libres:
                orden.extend(libres)
                pendientes -= set(libres)
                continue
            # Se corta el bloque del que dependen más, evitando la red si se puede
            salen = lambda k: sum(k in depende[j] for j in pendientes)
            candidatos = [k for k in pendientes if k not in cortes]
            candidatos.sort(key=lambda k: (modelo.lista[k] is modelo.red, -salen(k), k))
            cortes.append(candidatos[0])
        self.cortes = cortes
        self.orden = orden
        self.conj = conj
        self.J = None
        self.formas = None
        self.evals = 0
        self.nombres = [modelo.lista[k].etiqueta for k in comp]

    def _F(self, z, t, x):
        m = self.m
        Y = m.Y
        for k, v in zip(self.cortes, self._trocear(z)):
            Y[k] = v
        nuevos = {}
        for k in self.orden:
            y = m.evaluar_bloque(k, t, x)
            if k in self.cortes:
                nuevos[k] = y
            else:
                Y[k] = y
        vec, _ = _vectorizar([e for k in self.cortes for e in nuevos[k]])
        return vec - z

    def _trocear(self, z):
        planos = _desaplanar(z, self.formas)
        salida, i = [], 0
        for k in self.cortes:
            n = len(self.m.Y[k])
            salida.append(planos[i:i + n])
            i += n
        return salida

    def resolver(self, t, x):
        m = self.m
        Y = m.Y
        # Se evalúa una vez para conocer la forma de las señales cortadas
        previos = [e for k in self.cortes for e in Y[k]]
        z0, formas = _vectorizar(previos)
        for k in self.orden:
            y = m.evaluar_bloque(k, t, x)
            if k not in self.cortes:
                Y[k] = y
            else:
                Y[k] = list(y) if isinstance(y, list) else y
        actuales, formas2 = _vectorizar([e for k in self.cortes for e in Y[k]])
        if formas2 != formas:
            z0 = actuales
            self.J = None
        self.formas = formas2
        try:
            z, self.J, n = resolver_no_lineal(lambda w: self._F(w, t, x), z0, J_previo=self.J)
        except (SinSolucion, np.linalg.LinAlgError):
            raise ErrorDiagrama(
                "No se pudo resolver el lazo algebraico entre: " + ", ".join(self.nombres[:8]) +
                f" (t = {t:.6g}). Añade una Memoria o un Integrador en el lazo.") from None
        self.evals += n
        self._F(z, t, x)
        for k, v in zip(self.cortes, self._trocear(z)):
            Y[k] = v


class Modelo:
    """Modelo compilado: evalúa señales y derivadas y mantiene el estado discreto."""

    def __init__(self, diagrama, espacio=None):
        cargar_librerias()
        self.espacio = espacio if espacio is not None else {}
        self.evaluaciones = 0
        self.t_actual = 0.0
        self.registrar_saltos = False
        self._compilar(diagrama)

    # ------------------------------------------------------------ compilación
    def _compilar(self, d):
        from . import circuito

        planos, cons, alias = [], [], {}
        _aplanar(d.get("sistema", d), "", planos, cons, alias)

        self.instancias = {}
        for b in planos:
            cls = REGISTRO.get(b["tipo"])
            if cls is None:
                raise ErrorDiagrama(f"Tipo de bloque desconocido: {b['tipo']}")
            self.instancias[b["id"]] = cls(b.get("params"), b["id"], b.get("nombre", ""), self.espacio)

        # Separar conexiones de señal y eléctricas
        fuente_de = {}
        grupos_e = []
        for a, c in cons:
            a = alias.get(a, a)
            c = alias.get(c, c)
            if a[0] not in self.instancias or c[0] not in self.instancias:
                continue
            ia, ic = self.instancias[a[0]], self.instancias[c[0]]
            pua, puc = ia.puertos(ia.crudos), ic.puertos(ic.crudos)
            if a[1] in pua["e"] + pua["e3"] or c[1] in puc["e"] + puc["e3"]:
                grupos_e.append((a, c))
                continue
            # Se admite tirar del hilo al revés (de entrada a salida)
            if a[1] in pua["entradas"] and c[1] in puc["salidas"] and ic.tipo != "entrada":
                a, c = c, a
            if c in fuente_de:
                raise ErrorDiagrama(f"La entrada «{c[1]}» de «{self.instancias[c[0]].etiqueta}» tiene dos fuentes")
            fuente_de[c] = a

        def resolver(fuente):
            """Atraviesa los puertos Entrada/Salida hasta el bloque que produce la señal."""
            vistos = set()
            while fuente is not None:
                if fuente in vistos:
                    raise ErrorDiagrama("Lazo entre puertos de subsistema")
                vistos.add(fuente)
                if self.instancias[fuente[0]].tipo in ("entrada", "salida"):
                    fuente = fuente_de.get(fuente)
                else:
                    return fuente
            return None

        paso = {"entrada", "salida", "puerto_e", "puerto_e3", "subsistema"}
        elementos = [i for i in self.instancias.values() if isinstance(i, circuito.Elemento)]
        normales = [i for i in self.instancias.values()
                    if i.tipo not in paso and not isinstance(i, circuito.Elemento)]
        self.red = circuito.Red(elementos, grupos_e, self.instancias) if elementos else None
        self.lista = list(normales)
        indice = {b.id: k for k, b in enumerate(self.lista)}
        self.red_idx = -1
        if self.red:
            self.red_idx = len(self.lista)
            self.lista.append(self.red)

        def direccion(fuente):
            f = resolver(fuente) if fuente else None
            if f is None:
                return None
            inst = self.instancias[f[0]]
            if isinstance(inst, circuito.Elemento):
                return (self.red_idx, self.red.indice_salida(f[0], f[1]))
            pu = inst.puertos(inst.crudos)["salidas"]
            if f[1] not in pu:
                raise ErrorDiagrama(f"«{inst.etiqueta}» no tiene la salida «{f[1]}»")
            return (indice[f[0]], pu.index(f[1]))

        # Se guardan para localizar señales por nombre (linealización)
        self._direccion, self._fuente_de, self._alias = direccion, fuente_de, alias
        self.forzadas = {}
        self.fuentes = []
        for b in normales:
            ent = b.puertos(b.crudos)["entradas"]
            self.fuentes.append([direccion(fuente_de.get((b.id, p))) for p in ent])
        if self.red:
            self.fuentes.append([direccion(fuente_de.get((inst.id, p)))
                                 for inst, p in self.red.entradas_lista])

        # Estados
        self.tramos = []
        pos = 0
        x0 = []
        for b in self.lista:
            n = b.n_estados()
            self.tramos.append((pos, pos + n))
            pos += n
            x0.extend(b.x0())
        self.x_ini = np.array(x0, dtype=float)
        self.Y = []
        for b in self.lista:
            n = len(self.red.salidas_lista) if b is self.red else len(b.puertos(b.crudos)["salidas"])
            self.Y.append([0.0] * n)
        self._ordenar()

    def _ordenar(self):
        """Orden topológico por dependencias directas. Un ciclo es un lazo algebraico."""
        n = len(self.lista)
        depende = [set() for _ in range(n)]
        for k, b in enumerate(self.lista):
            if b is self.red:
                for i in b.entradas_directas():
                    f = self.fuentes[k][i]
                    if f is not None:
                        depende[k].add(f[0])
            elif b.directo:
                for f in self.fuentes[k]:
                    if f is None:
                        continue
                    # Una salida de la red que solo depende de estados no crea dependencia
                    if f[0] == self.red_idx and f[1] not in self.red.salidas_directas:
                        continue
                    depende[k].add(f[0])
        propio = {k for k in range(n) if k in depende[k]}
        for k in range(n):
            depende[k].discard(k)
        # Cada componente fuerte con más de un bloque (o con autodependencia) es un lazo algebraico
        orden = []
        self.lazos = []
        for comp in componentes_fuertes(depende):
            if len(comp) == 1 and comp[0] not in propio:
                orden.append(comp[0])
            else:
                lazo = _Lazo(self, comp, depende)
                self.lazos.append(lazo)
                orden.append(lazo)
        self.orden = orden
        self.con_estado = [k for k, (a, b) in enumerate(self.tramos) if b > a]
        self.cruzables = [k for k, b in enumerate(self.lista) if b.tiene_cruce()]
        self.hay_cruces = bool(self.cruzables)

    def nombre_estado(self, i):
        """Bloque al que pertenece el estado i (para decir cuál limita el paso)."""
        for k, (a, c) in enumerate(self.tramos):
            if a <= i < c:
                b = self.lista[k]
                if b is self.red:
                    for e in b.elementos:
                        if e.xa <= i - a < e.xb:
                            return e.etiqueta
                return b.etiqueta
        return "?"

    def cruces(self, t, x):
        """Señales de cruce por cero de todos los bloques que las declaran."""
        self.salidas(t, x)
        g = []
        for k in self.cruzables:
            a, c = self.tramos[k]
            g.extend(self.lista[k].cruce(t, x[a:c], self._u(k)))
        return g

    def evaluar_bloque(self, k, t, x):
        """Salidas de un bloque, sin modificar la tabla de señales."""
        b = self.lista[k]
        a, c = self.tramos[k]
        if b is self.red:
            y = list(self.Y[k])
            b.calcular_salidas(t, x[a:c], self._u(k), y)
            return y
        return b.salida(t, x[a:c], self._u(k) if b.directo else None)

    # ------------------------------------------------------------ evaluación
    def _u(self, k):
        Y = self.Y
        return [0.0 if f is None else Y[f[0]][f[1]] for f in self.fuentes[k]]

    def salidas(self, t, x):
        Y, lista, tramos = self.Y, self.lista, self.tramos
        r = self.red_idx
        if r >= 0:
            # Primera pasada de la red: sus salidas que solo dependen de estados
            a, c = tramos[r]
            lista[r].calcular_salidas(t, x[a:c], self._u(r), Y[r])
        for k in self.orden:
            if k.__class__ is _Lazo:
                k.resolver(t, x)
                continue
            b = lista[k]
            a, c = tramos[k]
            if k == r:
                if b.hay_directas:
                    b.calcular_salidas(t, x[a:c], self._u(k), Y[k])
            elif b.directo:
                Y[k] = b.salida(t, x[a:c], self._u(k))
            else:
                Y[k] = b.salida(t, x[a:c], None)
            if self.forzadas and k in self.forzadas:
                # Entrada de linealización: la salida de este bloque se fija desde fuera
                Y[k] = list(Y[k])
                for j, v in self.forzadas[k].items():
                    Y[k][j] = v

    def direccion_senal(self, texto):
        """(bloque, salida) de una señal escrita como «bloque» o «bloque.puerto» (con subsistemas)."""
        bid, _, puerto = str(texto).partition(".")
        if bid not in self.instancias and not any(a[0] == bid for a in self._alias):
            raise ErrorDiagrama(f"No hay ningún bloque «{bid}»")
        if not puerto:
            inst = self.instancias.get(bid)
            if inst is not None:
                sal = inst.puertos(inst.crudos)["salidas"]
                puerto = sal[0] if sal else "y"
            else:
                puerto = "out1"
        ref = (bid, puerto)
        vistos = set()
        while ref in self._alias and ref not in vistos:
            vistos.add(ref)
            ref = self._alias[ref]
        inst = self.instancias.get(ref[0])
        if inst is not None and inst.tipo == "salida":
            ref = self._fuente_de.get(ref)
        d = self._direccion(ref) if ref else None
        if d is None:
            raise ErrorDiagrama(f"La señal «{texto}» no está conectada a nada")
        return d

    def f(self, t, x):
        self.evaluaciones += 1
        self.salidas(t, x)
        dx = np.empty_like(x)
        for k in self.con_estado:
            a, c = self.tramos[k]
            dx[a:c] = self.lista[k].derivada(t, x[a:c], self._u(k))
        return dx

    def iniciar(self, t0=0.0):
        for b in self.lista:
            b.reiniciar()
        x = self.x_ini.copy()
        self.t_actual = t0
        self.evaluaciones = 0
        self.salidas(t0, x)
        self.paso_mayor(t0, x)
        return x

    def paso_mayor(self, t, x):
        """Actualización discreta al final de un paso aceptado. True si hubo discontinuidad."""
        self.t_actual = t
        self.salidas(t, x)
        # Con paso variable, un salto se registra con sus dos valores para que se vea vertical
        previas = [(k, self._u(k)) for k, b in enumerate(self.lista) if b.sumidero] if self.registrar_saltos else ()
        cambio = False
        # Moduladores, switches y saturaciones cambian antes que el resto: una compuerta que
        # sube en este instante debe verla la red en este mismo instante, no en el paso siguiente.
        if self._de_evento(t, x):
            cambio = True
            self.salidas(t, x)
        for k, b in enumerate(self.lista):
            if b.sumidero or b.de_evento:
                continue
            a, c = self.tramos[k]
            if b.paso_mayor(t, x[a:c], self._u(k), self.Y[k]):
                cambio = True
        if cambio:
            for k, u in previas:
                self.lista[k].paso_mayor(t, None, u, None)
            self.salidas(t, x)
            # Lo que acaba de cambiar puede haber llevado una señal al otro lado de un umbral
            if self._de_evento(t, x):
                self.salidas(t, x)
        for k, b in enumerate(self.lista):
            if b.sumidero:
                b.paso_mayor(t, None, self._u(k), None)
        return cambio

    def _de_evento(self, t, x):
        cambio = False
        for k, b in enumerate(self.lista):
            if b.de_evento:
                a, c = self.tramos[k]
                if b.paso_mayor(t, x[a:c], self._u(k), self.Y[k]):
                    cambio = True
        return cambio

    def nombres_entradas(self, b):
        """Nombre legible de la señal que llega a cada entrada de un bloque."""
        k = self.lista.index(b)
        nombres = []
        for i, f in enumerate(self.fuentes[k]):
            if f is None:
                nombres.append(f"entrada {i + 1}")
            elif f[0] == self.red_idx:
                e, q = self.red.salidas_lista[f[1]]
                nombres.append(f"{e.etiqueta}: {q}")
            else:
                o = self.lista[f[0]]
                sal = o.puertos(o.crudos)["salidas"]
                nombres.append(o.etiqueta if len(sal) == 1 else f"{o.etiqueta}: {sal[f[1]]}")
        return nombres

    def proximo_evento(self, t):
        m = math.inf
        for b in self.lista:
            e = b.proximo_evento(t)
            if e < m:
                m = e
        return m


def _num(v):
    if v in (None, "", 0, 0.0):
        return None
    return float(evaluar_numero(v))


def simular(diagrama, espacio=None, cancelar=None, config=None, informe=None):
    """Compila y simula un diagrama. Devuelve las señales de cada osciloscopio."""
    t_ini = time.perf_counter()
    cfg = dict(diagrama.get("solver", {}))
    cfg.update(config or {})
    m = Modelo(diagrama, espacio)
    metodo = cfg.get("metodo", "dopri")
    variable = metodo in ("dopri", "bdf")
    m.registrar_saltos = variable
    # Con paso variable, los bloques con discontinuidad mantienen su modo durante el paso y el
    # solver localiza el instante del cambio. Con paso fijo no hay localización.
    for b in m.lista:
        if b is m.red:
            for e in b.elementos:
                e.usar_zc = variable
        else:
            b.usar_zc = variable
    info = integrar_modelo(
        m, metodo=metodo, t0=float(evaluar_numero(cfg.get("t_ini", 0.0))),
        tf=float(evaluar_numero(cfg.get("t_fin", 1.0))), paso=_num(cfg.get("paso")),
        rtol=float(evaluar_numero(cfg.get("rtol", 1e-3))), atol=float(evaluar_numero(cfg.get("atol", 1e-6))),
        paso_max=_num(cfg.get("paso_max")), paso_ini=_num(cfg.get("paso_ini")),
        cancelar=cancelar, informe=informe)
    res = {"senales": [], "estadisticas": {
        "pasos": info["pasos"], "rechazos": info["rechazos"], "evaluaciones": info["evaluaciones"],
        "estados": int(m.x_ini.size), "bloques": len(m.instancias), "metodo": metodo,
        "cruces": info["cruces"], "lazos": len(m.lazos), "segundos": time.perf_counter() - t_ini}}
    # El estado que más veces fijó el error del paso indica qué parte del modelo frena al solver
    lim = info.get("limitante")
    if lim is not None and lim.size and lim.sum() > 0:
        i = int(np.argmax(lim))
        res["estadisticas"]["estado_limitante"] = m.nombre_estado(i)
        res["estadisticas"]["limitante_pct"] = 100.0 * float(lim[i]) / float(lim.sum())
    exportados = {}
    for b in m.lista:
        if isinstance(b, (Osciloscopio, AEspacio)):
            t = np.asarray(b.ts)
            n = len(b.puertos(b.crudos)["entradas"])
            trazas = []
            for j in range(n):
                col = [v[j] for v in b.vs]
                trazas.append(np.asarray(col, dtype=float) if col else np.zeros(0))
            if isinstance(b, AEspacio):
                exportados[str(b.p["variable"])] = {"t": t, "y": trazas[0]}
            res["senales"].append({"bloque": b.id, "etiqueta": b.etiqueta, "t": t, "trazas": trazas,
                                   "exportado": isinstance(b, AEspacio), "nombres": m.nombres_entradas(b)})
    res["avisos"] = list(m.red.avisos) if m.red else []
    # Bloques de cada lazo (sin la red, que reúne a todos los elementos eléctricos)
    res["lazos"] = [[m.lista[k].id for k in lazo.comp if m.lista[k] is not m.red] for lazo in m.lazos]
    for lazo in m.lazos:
        cortados = ", ".join(m.lista[k].etiqueta for k in lazo.cortes)
        res["avisos"].append(
            f"Lazo algebraico entre {', '.join(lazo.nombres[:6])}: resuelto por región de confianza, "
            f"cortando en {cortados} ({lazo.evals} evaluaciones). Una Memoria en el lazo acelera la simulación.")
    res["exportados"] = exportados
    res["modelo"] = m
    return res


def bloque(ident, tipo, x=0, y=0, nombre=None, sistema=None, **params):
    """Ayuda para escribir diagramas en Python (usada por ejemplos y pruebas)."""
    d = {"id": ident, "tipo": tipo, "x": x, "y": y, "nombre": nombre or "", "params": params}
    if sistema is not None:
        d["sistema"] = sistema
    return d


def hilo(de, a):
    """Conexión: hilo("r.p", "c.p") une el puerto p del bloque r con el puerto p del bloque c."""
    b1, p1 = de.rsplit(".", 1)
    b2, p2 = a.rsplit(".", 1)
    return {"de": [b1, p1], "a": [b2, p2]}


def diagrama(nombre, bloques, conexiones, **solver):
    return {"formato": FORMATO, "nombre": nombre, "solver": solver,
            "sistema": {"bloques": bloques, "conexiones": conexiones}}
