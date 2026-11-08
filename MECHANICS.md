# Hoja de valores: lo investigado en el programa instalado

Se estudió la instalación R2026b (`/Applications/MATLAB_R2026b.app`, 9.5 GB) sin ejecutarla. Es una aplicación
Qt + CEF + Java con un motor nativo y unas 300 cajas de herramientas en archivos `.m`, `.p` y `.slx`.

De aquí salieron valores y criterios. El código de Heliora está escrito desde cero: no contiene código, textos,
iconos ni formatos de archivo del original. Los mismos valores están en `mechanics.json`.

## Solvers

| Valor | Original | En Heliora | Fuente |
|---|---|---|---|
| Solvers registrados | ode1, ode14x, ode15s, ode113, ode2, ode23, ode23s, ode23t, ode23tb, ode3, ode4, ode45, ode5, ode8 y dos discretos | Euler, RK4, Dormand-Prince y BDF (lo pedido) | `libmwsl_solver_obsolete.dylib`, función `slvrSolverPkg::registerAllSolvers`, 0x6720 |
| Fórmula de ode4 | x + h/6·(k1 + 2k2 + 2k3 + k4), con etapas en t, t+h/2, t+h/2, t+h | igual | `slvrODE4::takeStep`, 0x38620 |
| Tolerancia relativa por defecto | 1e-3 | 1e-3 | `toolbox/matlab/funfun/private/odearguments.m`, línea 125 |
| Tolerancia absoluta por defecto | 1e-6 | 1e-6 | `odearguments.m`, línea 136 |
| Tolerancia relativa mínima | 100·eps | 100·eps | `odearguments.m`, línea 131 |
| Paso máximo por defecto | 1/10 del intervalo | 1/10 del intervalo | `odearguments.m`, línea 166 |
| Exponente del control de paso | 1/5 | 1/5 | `funfun/ode45.m`, línea 139 |
| Factor de seguridad | 0.8 | 0.8 | `ode45.m`, líneas 193 y 316 |
| Reducción mínima tras rechazar un paso | 0.1 | 0.1 | `ode45.m`, línea 316 |
| Crecimiento máximo del paso | 5 | 5 | `ode45.m`, línea 451 |
| Paso siguiente | h / (1.25·(err/rtol)^(1/5)) | igual | `ode45.m`, línea 447 |
| Tabla de Dormand-Prince | la publicada en 1980 | igual (dato público) | `ode45.m`, líneas 140 a 184 |
| Orden máximo del solver rígido | 5 | 5 (por `scipy.integrate.BDF`) | `funfun/ode15s.m`, línea 180 |

Una diferencia: el original usa NDF en `ode15s`. Heliora usa BDF, como pediste, y SciPy también aplica NDF internamente.

## Lo leído en el código compilado

El programa principal (`bin/maca64/MATLAB`, 8 funciones, sin cadenas de solvers ni de bloques) es solo un lanzador:
`_main` (0x100001280) ignora la señal SIGPIPE (`signal(13, 1)`) y llama a `MathWorks::matlab::getCmdLineArgs` y
`MathWorks::matlab::main`; `sub_100001338` solo libera la lista de argumentos. La lógica está en las
librerías, así que se abrieron las del motor de simulación. Ghidra no estaba disponible: todo se leyó con el
descompilador propio de Craft Anything (pseudocódigo por función). Los valores y reglas se pasaron a la hoja y el
código de Heliora se escribió de nuevo a partir de ella; no se copió código descompilado.

### Paso de Dormand-Prince

| Regla | Original | En Heliora | Fuente |
|---|---|---|---|
| Paso inicial | h = 0.8·rtol^(1/5) / máx(\|f₀\| / máx(\|x\|, atol/rtol)), limitado al paso máximo | igual | `libmwslexec_solver.dylib`, `ODE45::computeInitialStepSize` 0x83a70 (constantes 0.8 y 0.2) |
| Paso siguiente (versión actual) | h / máx(1.25·(err/rtol)^(1/5), 0.2), solo sin rechazos y con error finito | igual | `ODE45::computeNextStepSize` 0x839dc |
| Error del paso | máx. de \|h·Σ Eᵢkᵢ\| / máx(\|x\|, \|x_nuevo\|, atol/rtol), con E = 71/57600, … | igual | `libmwsl_solver_obsolete.dylib`, `slvrODE45::computeError` 0x39ec4 |
| Paso siguiente | h / máx(1.25·(err/rtol)^(1/5), 0.2), solo si el intento no tuvo rechazos | igual | `slvrODE45::computeNextH` 0x3a034 |
| Primer rechazo | h·máx(0.1, 0.8·(rtol/err)^(1/5)) | igual | `ode45.m`, línea 314 |
| Rechazos siguientes | h/2 | igual (antes reducía siempre con la fórmula) | `ode45.m`, línea 316 |
| Estirar el último paso | si 1.1·h llega al tope, el paso va hasta él | igual (nuevo) | `ode45.m`, línea 225 |
| Estado que limita el paso | se guarda el índice del estado con mayor error | igual: se cuenta y la barra de resultados dice qué bloque frena al solver | `libmwslexec_solver.dylib`, `ODE45::computeError` 0x8385c |
| Interpolación dentro del paso | polinomio de grado 4 con la tabla BI | igual (dato público de Dormand-Prince) | `ODE45Interpolant::interpolateState` 0x84098; `ntrp45.m`, línea 22 |

### Cruces por cero (nuevo en Heliora)

| Regla | Original | En Heliora | Fuente |
|---|---|---|---|
| Detección | cambio de signo de la señal entre el inicio y el final de un paso aceptado (con clases negativo, cero y positivo) | igual; una señal que llega a cero exacto da un paso diminuto para saber hacia dónde sale | `slvrZcClassic::detect` 0x4fa88; `zc::DefaultDetector::detect` 0xc0b88 |
| Localización | secante con corrección de Illinois: el extremo que se conserva dos veces seguidas reduce su señal a la mitad | igual | `slvrZcClassic::bracketEvent` 0x4fefc; `zc::DefaultDetector::updateBracket` 0xc185c |
| Señal casi plana | si \|g_d − g_i\| ≤ 2⁻⁴⁵ se usa el punto medio | igual | `slvrZcDetector` (constructor) 0x514c4 |
| Ancho final | 128·eps(t) | igual | 0x514c4 |
| Iteraciones máximas | 100 000 | igual | `zc::DefaultDetector` (constructor) 0xbf970 |
| Oscilación sin avanzar | error tras 1000 eventos a menos de 1280·eps(t) | igual | 0xbf970 |
| Refinado automático | apagado por defecto (factor 1) | no se implementa | 0xbf970 |

Bloques con cruce: Switch, Saturación, Diodo y Boost conmutado. Con paso variable mantienen su modo durante el paso
y cambian justo después del cruce; con paso fijo funcionan como antes. Límite conocido, igual que en el original
sin refinado: un pulso que empieza y termina dentro de un mismo paso no se ve; se evita con un paso máximo menor.

### Lazos algebraicos (antes se rechazaban; ahora se resuelven)

| Regla | Original | En Heliora | Fuente |
|---|---|---|---|
| Algoritmo por defecto | región de confianza | igual | `simulink.slx`, `configSet0.json`; `ne::TrustRegion::solve` 0xb0354 |
| Jacobiano | numérico por perturbación, con factorización QR | igual | 0xb0354 (llamadas a dgeqrf y dormqr) |
| Atajo lineal | comprueba si el lazo es lineal y entonces basta un paso de Newton | igual | 0xb0354 («check for linearity») |
| Evaluaciones máximas | 200 por incógnita | igual | `ne::NonLinearSolver` (constructor) 0xad7e0 |
| Radio inicial | 100·\|z\| (100 si z = 0), con tope opcional de 10·MaxXNorm | igual, sin tope | `ne::TrustRegion::solve` 0xb0354, constante 0x4059000000000000 en 0xb04b4 |
| Razón ρ | reducción real / prevista, con la prevista = 1 − (\|f + Jp\|/\|f\|)² | igual (sin normalizar, mismo cociente) | 0xb0354, bloque 0xb1450-0xb1474 |
| Reducir | ρ < 0.1: Δ = Δ/2 y se reinicia la cuenta de aciertos | igual | 0xb147c (x27 = 0x3FB999999999999A) y 0xb14fc |
| Ampliar | ρ ≥ 0.5 **o dos pasos aceptados seguidos**: Δ = máx(Δ, 2·\|p\|) | igual (nuevo: la cuenta de aciertos) | 0xb1494-0xb14bc |
| Predicción exacta | \|ρ − 1\| ≤ 0.1: Δ = 2·\|p\| | igual (nuevo) | 0xb14c0-0xb1508 |
| Aceptar | ρ ≥ 1e-4 | igual | 0xb1510-0xb1528 (0x3F1A36E2EB1C432D) |
| Tolerancia del residuo | máx(RelTol·\|F(z₀)\|, 10⁻⁶) | **distinto**: 10⁻¹⁰·(1 + máx\|z\|). Un residuo de 10⁻⁶ deja errores de ese orden en las señales del lazo; las pruebas piden 10⁻⁹ | 0xb0478-0xb04a0 (0x3EB0C6F7A0B5ED8D) |
| Poco avance | cuenta los pasos con reducción real < 10⁻³ | no se implementa: el límite de 200 evaluaciones por incógnita cumple esa función | 0xb1680-0xb16a4 (0x3F50624DD2F1A9FC) |
| Corte del lazo | elección interna | componentes fuertemente conexas (Tarjan) y corte en el bloque del que más dependen | propio |

Si el lazo no tiene solución (por ejemplo y = 1 + y), se dice con los nombres de los bloques; si la tiene, se resuelve
en cada paso, se avisa en el panel Diagnóstico y el lienzo marca sus bloques y cables.

### PWM con flancos exactos

| Regla | Original | En Heliora | Fuente |
|---|---|---|---|
| Tiempo de muestreo | variable: el bloque dice cuándo es su próximo flanco | igual con Dormand-Prince y BDF | `sFuncPwm.mexmaca64`, `sub_38cc` 0x38cc |
| Ciclo de trabajo | recortado a [1/res, 1 − 1/res]: siempre hay un pulso mínimo | igual; res = 10 000 es **estimado** | `sub_3980` 0x3980 |
| Centrado | sube en base + T(1−D)/2, baja en base + T(1+D)/2 | igual (por defecto) | `sub_3980`, modo 4; `sub_3b38` 0x3b38 |
| Flanco de bajada | sube en base, baja en base + T·D | igual | `sub_3980`, modo 1 |
| Flanco de subida | sube en base + T(1−D), baja al final del periodo | igual | `sub_3980`, modo 2 |
| Modo alterno (3) | alterna los dos anteriores | no se implementa | `sub_3980`, modo 3 |

Resultado: el boost conmutado con Dormand-Prince da 96,01 V frente a 96 V teóricos en 600 pasos; RK4 con la
compuerta muestreada cada 1 µs da 97,06 V en 20 000 pasos. Con paso fijo la compuerta se sigue comparando con la
portadora en cada muestra; la portadora tiene ahora su máximo al empezar el periodo para que el pulso quede centrado.

### Linealización (Jacobianos numéricos)

El `linmod` del original está cifrado (`toolbox/simulink/core/general/linmod.p`), pero la perturbación numérica
está compilada. El programa principal no la contiene (es el lanzador); está en `libmwsimulink.dylib` y en
`libmwslexec_solver.dylib`.

| Regla | Original | En Heliora | Fuente |
|---|---|---|---|
| Perturbación por defecto de cada puerto | «auto» | igual: la elige el motor, no la persona | `libmwsimulink.dylib`, `GetDefaultPerturbationForJacobian` 0xb80c4c; `gp_PerturbationForJacobian` 0xb85730 |
| Diferencias | centrales: f(x + δ) y f(x − δ) | igual | `libmwslexec_solver.dylib`, `NonLinearSparseApproximateJacobian::multiVariablesCentralDiffFuncEvaluate` 0xaf6d4 |
| Factor inicial | √eps = 1.4901161193847656e-08 | igual | `ApproximateJacobian` (constructor) 0xf98: `fsqrt` de eps (0x3CB0000000000000), campo +0x28 |
| Paso | δ = fac·máx(\|x\|, umbral), redondeado: δ = (x + δ) − x | igual, con umbral 1 | `NonLinearSparseApproximateJacobian::numjac` 0xaec08 (bloque 0xaee7c-0xaeedc) |
| Si el paso se anula | fac ← mín(100·fac, 0.1) y se repite | igual | `perturbation` 0xaf664 (0x4059000000000000) y constructor 0xf98 (0x3FB999999999999A en +0x38) |
| Umbral de escala | lo fija quien llama; si no, el menor \|x\| no nulo o √eps | **distinto**: 1. Con √eps, un estado en 0 tendría δ ≈ 2·10⁻¹⁶ y el redondeo dominaría la derivada | 0xaecd0-0xaed18 |
| Otras cotas del Jacobiano aproximado | eps^(7/8), eps^(3/4), eps^(1/4), facmin = eps^0.78 | no se usan: sirven al ajuste adaptativo de fac entre pasos del solver | constructor 0xf98 (0.875, 0.75, 0.25 y 0x3FE8F5C28F5C28F6) |
| Perturbación completa | se perturba el modelo entero (no bloque a bloque) con solvers locales, ciertos bloques, o si el Jacobiano bloque a bloque superaría unos 26,8 millones de entradas (0x1999999) | siempre perturbación completa (Heliora no tiene Jacobianos analíticos por bloque); el mismo número es el límite de coeficientes del modelo lineal: por encima, se pide linealizar un subsistema más pequeño | `sl::jacobian::modelNeedsUseFullPerturbation` 0x764fe8 |

Con estas reglas, el lazo de ejemplo da T(s) = (2s + 1)/(s³ + 2s² + 3s + 1) con errores de 10⁻⁹ o menos en
los coeficientes, y la ganancia y el desfase coinciden con la simulación pequeña-señal (prueba «Linealización
del lazo de control»).

### Solver rígido

`slvrODE15S::computeNextH` (0x17fe8) elige el paso con factores 1.2 (orden k), 1.3 (k−1) y 1.4 (k+1) y reduce como
mucho a 0.1. Heliora no reinventa BDF: usa `scipy.integrate.BDF`, cuyos factores son distintos (reducción mínima 0.2,
crecimiento máximo 10). Se deja anotado como diferencia.

### Unidades en los parámetros

Cada parámetro declara su unidad. Ahora se puede escribir `10 mH`, `1 kΩ` o `50 µF`; una unidad que no corresponde
(`10 mF` en una inductancia) da un error con el nombre del bloque en lugar de un modelo equivocado. Es una decisión
propia, no del original.

## Solar

| Valor | Original | En Heliora | Fuente |
|---|---|---|---|
| Módulo CS1H-330MS | 330 W, Voc 44.2 V, Isc 9.68 A, Vm 37 V, Im 8.92 A, 66 celdas | los mismos datos, como referencia | `physmod/elec/parts/m/resources/generated/Parts.sqlite3` |
| Módulo AS-6M30-HC | 320 W, Voc 40.2 V, Isc 10.14 A, Vm 33.4 V, Im 9.59 A, 60 celdas | los mismos datos | `Parts.sqlite3` |
| Módulo CS6X-300M | 300 W, Voc 45 V, Isc 8.74 A, Vm 36.5 V, Im 8.22 A, 72 celdas | no viene del programa: base CEC incluida en pvlib | `pvlib.pvsystem.retrieve_sam("CECMod")` |
| Banda prohibida del original | 1.12 eV | 1.121 eV, el valor de De Soto y de pvlib | `Parts.sqlite3`, parámetro `EG` |
| Coeficiente de Isc | 5·10⁻⁴ 1/K (aprox.) | 5·10⁻⁴ 1/K | `Parts.sqlite3`, parámetro `TIPH1` |
| Coeficiente de Voc en esos dos módulos | no consta en la base como dato de hoja | -0.29 %/K, **estimado** | se buscó en la base; se tomó un valor típico |

El original usa una celda de 8 parámetros con dos diodos. Heliora usa el modelo de un diodo, como pediste.
Las ecuaciones exactas del bloque original están cifradas (`solar_cell.sscp`, `PVArray.p`).

## Semiconductores

Por defecto, un módulo IGBT de 1200 V y 600 A (FZ600R12KE3 en `Parts.sqlite3`). Son datos de ejemplo: pon los de tu inversor.

| Valor | En Heliora | Fuente |
|---|---|---|
| E_on, E_off | 139.96 mJ, 174.78 mJ a 1191 A y 600 V | `Eon_scalar`, `Eoff_scalar`, `Iout_scalar`, `Voff` |
| V_ce0, R_on | 0.8 V, 0.002 Ω | **derivados**: recta ajustada a `Von_data` a 125 °C |
| V_f, R_d, E_rr del diodo | 0.9 V, 0.002 Ω, 42 mJ | **estimados**: la base no trae tabla útil del diodo de este módulo |

## Estadística y mantenimiento

| Valor | Original | En Heliora | Fuente |
|---|---|---|---|
| λ de la carta EWMA | 0.4 | 0.4 | `toolbox/stats/stats/controlchart.m`, línea 93 |
| Límites de control | 3·σ | 3·σ | `controlchart.m`, línea 94 |
| Sigma sin subgrupos | rango móvil | rango móvil | `controlchart.m`, línea 129 |
| Nivel de los intervalos de Weibull y de la prueba t | α = 0.05 | α = 0.05 | `stats/wblfit.m`, línea 25; `stats/ttest.m`, línea 24 |
| Isolation Forest | 100 árboles, 256 puntos | 100 árboles, 256 puntos | `stats/anomaly/IsolationForest.m`, líneas 254 y 255 |
| Armónicos del THD | 6, con ventana de Kaiser (β = 38) | **distinto**: 50 armónicos, ventana entera de periodos | `signal/signal/thd.m`, líneas 62 y 75 |
| Vida remanente por degradación | S(t) = φ + θ·exp(βt + ε - σ²/2), bayesiano | **más simple**: ajuste lineal o exponencial hasta un umbral | `predmaint/.../ExponentialDegradationRULEstimator.m`, línea 4 |

El THD usa 50 armónicos porque una fuente conmutada tiene contenido alto: con 6 no se vería la distorsión del PWM.

## Decisiones propias, no tomadas del original

| Valor | En Heliora | Por qué |
|---|---|---|
| Conductancia mínima de cada nodo | 1e-12 S | Evita nodos flotantes; su efecto medido es menor que el error del solver |
| Conducción discontinua del boost | la corriente residual cae con τ = 10 µs | **Estimado**: evita un sistema rígido al abrirse el diodo |
| Paso del modelo conmutado | 2 µs (100 muestras por periodo a 5 kHz) | **Estimado**: suficiente en las pruebas; la resolución de PWM es fc·h |
| Paso del modelo promediado | 50 µs | Margen frente a la resonancia del LCL, unos 2.5 kHz |
| Coeficiente de potencia en los KPI | -0.37 %/°C | **Estimado**, editable en cada herramienta |

| Detener la consola | interrupción a los 0,3 s; si el motor no responde en 3 s, se cierra y se abre otro | Un bucle infinito ya no congela el motor |
| Reinicio automático | hasta 3 veces por minuto, con espera creciente; se repiten las peticiones que no cambian nada | Si el motor se cae, la app sigue |
| Linealización: punto de operación | simular hasta el tiempo final y refinar con el Newton de región de confianza (f = 0) | Reutiliza el solver y el resolvedor de lazos que ya existen |
| Diagramas en JSON canónico | claves ordenadas, bloques por id, conexiones por extremos, sangría de 2 espacios | Un cambio de parámetro da una línea de diff en Git |
| Identidad visual | «Instrumento» (oscuro) y «Laboratorio» (claro), IBM Plex Sans y Mono (licencia OFL) | Diseño propio entregado por la persona |

## Lo que no se pudo saber

- Cómo se ve y se comporta la ventana original: no se ejecutó el programa, y la identidad de Heliora es propia a propósito.
- Las ecuaciones internas de los bloques cifrados de potencia y fotovoltaica. Las de Heliora vienen de la literatura
  publicada y están en la documentación de cada bloque.
