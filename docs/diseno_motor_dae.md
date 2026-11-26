# Diseño: motor acausal multidominio con DAE (Fase 5 del encargo)

Estado (2026-11-26): **construido como solver opcional «dae»** en la rama `motor-dae`. El motor de
siempre sigue siendo el de por defecto. La sección 0 describe lo construido y en qué se aparta de la
propuesta; las secciones 1 a 6 son la propuesta original, que se conserva como contexto.

## 0. Lo construido

| Parte | Cómo quedó | Archivo |
|---|---|---|
| Formulación | `F(t, y, ẏ) = 0` con `y = [x; z]`: estados del diagrama y tensiones/corrientes de la red, resueltos juntos | `sim/dae.py` |
| Almacenes | Cada elemento declara en `estados_en_rama()` qué estado fija qué rama de tensión. En modo DAE esa rama pasa a ser `d(fila·z)/dt = derivada del estado`; el estado se reconstruye de z. No hubo que reescribir ningún elemento (adaptador genérico) | `sim/circuito.py` y 10 clases de `lib/` |
| Solver | BDF de paso variable, orden 1 y 2 (A-estable, el de los simuladores de circuitos), en lugar de orden variable 1-5 | `sim/dae.py` |
| Newton | Jacobiano ∂F/∂y por diferencias finitas, agrupando columnas sin filas comunes (Curtis-Powell-Reid), uno por combinación de interruptores, corregido con Broyden en cada iteración; ∂F/∂ẏ es constante y exacta | `sim/dae.py` |
| Error | Estimación de Milne (predictor frente a corrector) solo sobre las incógnitas diferenciales: las algebraicas, también las de índice 2 (par del engranaje entre dos inercias), quedan fuera | `sim/dae.py` |
| Condiciones iniciales y conmutaciones | La red se resuelve con los almacenes fijados. Si dos almacenes unidos rígidamente tienen valores incompatibles, se proyectan minimizando Σ C_i·(valor - fijado)² con el resto de la red como restricción: conserva la carga, el momento angular referido al eje o el calor, y se avisa | `Red.consistente`, `Red._proyectar` |
| Reducción de índice (Pantelides) | No hizo falta: BDF resuelve el caso de índice 2 del engranaje (comprobado contra la solución analítica) | — |
| Interfaz | Opción «DAE (BDF implícito)» en el panel del solver | `src/bloques.rs` |

Verificado (banco `pruebas_dae.py`): dos condensadores en paralelo y dos inercias en el mismo eje o
por un engranaje, que el motor de siempre rechaza, contra su solución analítica; proyección que
conserva la carga y el momento angular; acuerdo con Dormand-Prince en el RC y en el arranque del motor
de inducción (diferencia relativa < 1e-3).

Medido en los ejemplos con la tolerancia por defecto (rtol 1e-3): el DAE es más rápido que
Dormand-Prince en las plantas FV promediada y con batería (rígidas) y más lento en el arranque del motor
de inducción, la planta conmutada y el troceador (entre 3 y 12 veces), por su orden 2 y por el
Jacobiano de la máquina, que gira con la red de 50 Hz.

Pendiente:
- La fuente de calor de las uniones sigue usando el estado de energía con retardo τ_e: leer v·i en el
  mismo instante exige reescribir esas estampas, no solo el solver.
- La linealización sigue sobre el motor de siempre; con DAE serían pares de matrices (E, A).
- Orden variable mayor que 2, para acercarse a Dormand-Prince en los modelos suaves.

## Propuesta original (2026-11-19)

Este documento decidía si Heliora seguía como simulador eléctrico (con dominios análogos) sobre la red
actual o pasaba a un motor de redes físicas acausales con ecuaciones diferenciales-algebraicas (DAE). Las
estimaciones de esfuerzo eran **estimadas**, no medidas.

## 1. Situación actual (verificada en el código)

- **Red MNA densa reducida a EDO** (`engine/heliora/sim/circuito.py`). Cada evaluación resuelve
  `M(estado discreto)·z = b(t, x, u)`. Condensadores, inercias y capacidades térmicas son fuentes de
  tensión con valor igual a su estado; bobinas y resortes, fuentes de corriente. Así la red queda en
  `dx/dt = f(t, x, u)` y la integran los solvers explícitos (Euler, RK4, Dormand-Prince) o `scipy` BDF.
- **Matriz densa e inversa guardada** por combinación de interruptores (`_usar_clave`): `np.linalg.inv`
  sobre la matriz completa. Con el inversor térmico (12 uniones) la matriz pasa de unos 30 a unos 130
  nodos y ramas y cada combinación nueva cuesta una inversión O(n³).
- **No linealidades por punto fijo** (`Red._resolver`): hasta 30 iteraciones con parada en 1e-8 y
  **sin error si no converge**: el último iterado se usa en silencio. Lo usan el arreglo FV, el par de
  carga cuadrático y el cargador DC-DC.
- **Lazos algebraicos de señal** por región de confianza (`sim/lazos.py`), aparte de la red.
- Los dominios térmico y mecánico de las Fases 2 y 4 comparten esa misma red por analogía, con
  comprobación de que un nodo no mezcla dominios.

## 2. Lo que las Fases 1-4 han mostrado

| Síntoma | Dónde | Causa de fondo |
|---|---|---|
| Dos inercias unidas por un engranaje ideal (o dos condensadores en paralelo) dan una matriz singular | Fase 4, ejemplo de arranque | Cada inercia fija su nodo; unidas rígidamente, el estado está duplicado. Es una DAE de índice 2 que la reducción a EDO no admite. Hoy se avisa y se pide sumar las inercias a mano |
| La fuente de calor de una unión no puede depender de v·i en el mismo instante | Fase 2 | `rhs` no puede leer la solución `z` (sería un lazo algebraico dentro de la red). Se resolvió con un estado de energía y un retardo τ_e |
| Parámetros que dependen de la temperatura obligan a refactorizar | Fase 2 | Matriz densa factorizada por combinación de interruptores; cada cambio vacía la caché. Se cuantizó en saltos de 0.1 K |
| Pasos rechazados en masa si un bloque cambia su entrada a la red al final de cada paso | Fase 2 (corregido) | La entrada discreta no se avisa al solver (el k1 reutilizado queda viejo) |
| Modos rápidos (R0·C de una batería con condensador, ω_c del cargador) obligan a pasos pequeños con solvers explícitos | Fase 3 | La red es rígida; BDF de `scipy` ayuda, pero no admite matriz de masa ni eventos internos |
| Punto fijo que puede no converger sin avisar | Fases 1-4 | Falta Newton con Jacobiano y un criterio de fallo |

## 3. Propuesta

### 3.1 Formulación

Sistema semiexplícito, con matriz de masa posiblemente singular:

    E · dy/dt = F(t, y, u),     y = [x; z]

`x` son los estados diferenciales (los de hoy) y `z` las incógnitas algebraicas de la red (tensiones,
temperaturas y velocidades de nodo; corrientes, flujos de calor y pares de rama). Las filas algebraicas
de `E` son cero. Cada elemento estampa **residuos** y su **Jacobiano** en lugar de `rhs` y `derivada_e`:

    estampar_residuo(R, J, t, y, dy, u)   # suma su contribución a R = F - E·dy y a ∂R/∂y, ∂R/∂dy

Un condensador pasa a ser `C·dv/dt - i = 0` con `v = v_p - v_n` (variable de nodo, no fuente). Dos
condensadores en paralelo ya no duplican estado: comparten la misma variable de nodo.

### 3.2 Condiciones iniciales consistentes

Antes de integrar: fijar `x(0)`, resolver las filas algebraicas `F_alg(0, x0, z) = 0` por Newton y, si
el usuario fija valores incompatibles (dos inercias rígidas con velocidades distintas), proyectar y avisar.
Para índice 1 basta Newton sobre `z`; para índice 2 (inercias por engranaje) hace falta una reducción de
índice. Propuesta: **no** implementar Pantelides al principio; detectar la singularidad estructural
(rango de `∂F_alg/∂z` con `E`) y avisar con el bloque responsable, como hoy pero con nombre.

### 3.3 Solver implícito

| Opción | A favor | En contra |
|---|---|---|
| `scipy.integrate.BDF` / `Radau` | ya es dependencia | **no admite matriz de masa** ni residuos implícitos: no sirve para la DAE sin reformular |
| IDA (SUNDIALS) vía `scikits.odes` | DAE de índice 1 probado, BDF de orden variable, eventos | dependencia nueva con compilado nativo (Windows y macOS); el encargo pide justificarla |
| BDF propio de orden 1-2 (o TR-BDF2, como los simuladores de circuitos) con Newton y LU dispersa | control total de eventos, conmutación e interruptores; sin dependencias nuevas | hay que escribir y validar control de paso, orden y convergencia de Newton |

**Recomendación:** BDF propio de orden variable 1-2 con Newton modificado (Jacobiano reutilizado
mientras converge) y factorización dispersa `scipy.sparse.linalg.splu`. Mantener RK4 / Dormand-Prince
para diagramas sin red. Evaluar IDA como opción si el BDF propio no alcanza la precisión de las pruebas.

### 3.4 Matrices dispersas y Newton

- Ensamblar `J = ∂R/∂y + (α/h)·∂R/∂dy` en formato COO → CSC; factorizar con `splu` y reutilizar el patrón
  simbólico entre pasos (el patrón solo cambia con la topología, no con los interruptores, si estos se
  estampan como conductancias variables).
- Sustituir el punto fijo de `Red._resolver` por Newton con búsqueda lineal y **error explícito** si no
  converge en N iteraciones (hoy falla en silencio). Este cambio es útil también sin DAE (ver §4, paso 1).
- Interruptores: conductancia por estado como hoy; un cambio de estado solo cambia valores, no el patrón.

## 4. Migración sin romper las Fases 1-4

1. **Newton y dispersión en la red actual** (sin DAE). `Red._resolver` con Newton para los elementos no
   lineales y `splu` en lugar de `inv`. Mantiene la interfaz `Elemento`. Criterio: las pruebas actuales
   pasan y los ejemplos dan el mismo resultado dentro de la tolerancia del solver (ya no bit a bit).
2. **Interfaz de residuos con adaptador.** Nueva interfaz `estampar_residuo`. Un adaptador genérico
   traduce los elementos actuales (`estampar_fijo`, `estampar_estado`, `rhs`, `derivada_e`) a residuos:
   sus estados siguen siendo diferenciales y sus ramas, algebraicas. Ningún bloque de las Fases 1-4 se
   reescribe para empezar.
3. **Motor DAE opcional** (`solver: "dae"`) junto al actual. Se reescriben primero los elementos que se
   benefician: capacidad, inercia, capacidad térmica, engranaje y transformador (dejan de ser fuentes) y
   las uniones térmicas (la fuente de calor puede leer v·i sin retardo τ_e).
4. **Comparación cruzada.** Cada ejemplo de `engine/ejemplos/` se simula con los dos motores; el banco
   compara las señales con tolerancia. El motor actual sigue por defecto hasta que el DAE las pase todas.
5. **Cambio de motor por defecto** solo tras revisión humana de esa comparación.

## 5. Esfuerzo y riesgos (estimados)

| Paso | Esfuerzo estimado | Riesgo principal |
|---|---|---|
| 1. Newton + `splu` en la red actual | 1-2 semanas | cambios numéricos pequeños en los ejemplos conmutados (orden de eventos) |
| 2. Interfaz de residuos y adaptador | 2-3 semanas | el adaptador debe reproducir la semántica de `paso_mayor`, `tras_paso` y las compuertas |
| 3. BDF propio con eventos y conmutación | 4-6 semanas | control de paso y de orden en presencia de miles de conmutaciones (PWM a 5-20 kHz) |
| 4. Comparación cruzada y validación | 2 semanas | discrepancias que haya que atribuir a uno u otro motor |
| Reducción de índice (Pantelides) | 3-4 semanas, opcional | complejidad; se puede posponer con un aviso claro |

Riesgos transversales: rendimiento en Python puro (el ensamblado por elemento en cada iteración de Newton
cuesta; puede requerir vectorizar las estampas), y la linealización actual (`sim/linealizar.py`), que
habría que rehacer sobre `E·dy/dt = F` (pares de matrices `(E, A)` en lugar de `A`).

## 6. Decisión que se pide

- **Quedarse** con la red actual más el paso 1 (Newton y dispersión): Heliora sigue como simulador
  eléctrico con dominios análogos, con las limitaciones del §2 documentadas. Esfuerzo bajo.
- **Ir a DAE** (pasos 1 a 5): Heliora pasa a motor acausal multidominio. Esfuerzo estimado de 2 a 3
  meses de una persona, con el motor actual como red de seguridad mientras dure la migración.

Recomendación: aprobar el **paso 1** ya (corrige el fallo silencioso del punto fijo y prepara la
dispersión) y decidir los pasos 2-5 tras ver cuántos casos reales tropiezan con las limitaciones del §2.
