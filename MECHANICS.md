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

## Modelos físicos: bloques de señal y tiempo discreto (Fase 1 del encargo)

Ecuaciones escritas desde la literatura citada en cada bloque (`engine/heliora/lib/senal.py`); no proceden del
programa instalado. Los valores marcados **estimado** son elecciones de ingeniería, no medidas.

**Citas no verificadas.** Las referencias bibliográficas de esta sección y de las siguientes (libros y notas de
aplicación) se dan por autor, título y editorial; la edición y el apartado exactos no se han comprobado contra el
texto. Las ecuaciones sí están comprobadas: cada una tiene una prueba analítica en `engine/heliora/validacion/`.

| Valor | En Heliora | Fuente o motivo |
|---|---|---|
| PID: forma | paralela, filtro de derivada Kd·N·s/(s + N), pesos de consigna b y c, anti-windup por retrocálculo | Åström y Hägglund, *Advanced PID Control*, ISA, 2006 (no verificada) |
| PID: N por defecto | 10 rad/s | valor pedido en el encargo (Anexo D.2) |
| PID: Tt automático | √(Kd/Ki) = √(Ti·Td); sin derivada, Kp/Ki = Ti | regla de Åström y Hägglund (no verificada). **Distinto del encargo**, que proponía Kd/Ki: esa magnitud tiene unidades de s², no es un tiempo |
| PID discreto | integral por Euler hacia delante, filtro de derivada por Euler hacia atrás | Åström y Wittenmark, *Computer-Controlled Systems*, 1997 (no verificada). Medido: a 10 kHz difiere del continuo un 0.40 % de la consigna (criterio: 1 %) |
| Limitador de tasa continuo: k | 1e4 1/s | **Estimado**: constante de tiempo de seguimiento de 0.1 ms; editable |
| Tolerancia de un instante de muestreo | 1e-9·Ts | **Estimado**: cubre el redondeo de t + (t_k − t) y de la suma de pasos fijos |
| Paso fijo que no divide a Ts | el muestreo se aplica en el primer paso posterior y se avisa | encargo, Anexo D.3 |
| Ruido de banda limitada | muestra retenida durante Tc con desviación √(PSD/Tc); uniforme en ±√3·σ | forma clásica del ruido blanco de banda limitada; varianza medida dentro del 1 % |
| `combinar_bus` / `seleccionar_bus` | **no construidos** | `mux` y `demux` ya cubren el caso posicional; un bus con nombres exigiría un tipo de señal no numérico que el motor (lazos, osciloscopio, linealización) no maneja. El encargo permite no construirlos en ese caso |
| `saturacion_tasa` | no existe aparte: es `limitador_tasa` | el encargo los unifica (Anexo D.2) |

## Modelos físicos: dominio térmico y acoplamiento electro-térmico (Fase 2 del encargo)

`engine/heliora/lib/termica.py`. Analogía térmica-eléctrica resuelta en la misma MNA; temperaturas absolutas en
°C (la referencia de la red es 0 °C). Valores por defecto del módulo de ejemplo de 1200 V / 600 A (tipo
FZ600R12KE3), **todos estimados** salvo que se diga otra cosa: sustitúyelos por los de la hoja de datos.

| Valor | En Heliora | Fuente o motivo |
|---|---|---|
| R_thJC del IGBT | 0.045 K/W, repartida en 4 pares Foster: 0.0045 / 0.0135 / 0.018 / 0.009 | **estimado** (encargo, Anexo A.5); el reparto 10/30/40/20 % es **estimado** |
| R_thJC del diodo | 0.090 K/W, mismo reparto: 0.009 / 0.027 / 0.036 / 0.018 | **estimado** (Anexo A.5) |
| τ Foster unión-cápsula | 0.5 / 5 / 50 / 500 ms | **estimado** (Anexo A.5) |
| R_thCH (cápsula-disipador) | 0.02 K/W en los ejemplos | **estimado** (Anexo A.5) |
| T_amb | 40 °C | por defecto editable (Anexo A.5) |
| T_ref de los datos eléctricos | 125 °C | coherente con V_ce0 = 0.8 V ajustada a 125 °C (sección Semiconductores) |
| a_V (V_ce0 y V_f) | −2 mV/K | **estimado** (Anexo A.5) |
| a_R (R_on y R_d) | +0.6 %/K | **estimado** (Anexo A.5) |
| a_E (energías de conmutación) | +0.3 %/K | **estimado** (Anexo A.5) |
| k_i (exponente de corriente de las energías) | 1 (lineal, como el modelo previo) | Anexo A.3 |
| Tj_max (aviso) | 150 °C | límite de hoja (Anexo A.5) |
| Retardo de entrega del calor τ_e | 0.1 ms | **estimado**: 5 veces menor que la τ Foster más corta; la energía se conserva entera |
| Salto de Tj que reajusta R_on y V | 0.1 K | **estimado**: el error de pérdidas que introduce es del orden de 0.01 W en el ejemplo |
| Unión por dispositivo | cada IGBT y cada diodo tienen su red Foster; la cápsula es el puerto «th» del bloque | Anexo A.3; en el inversor, 12 uniones con cápsula común |
| Foster unión-cápsula en serie con red externa | se acepta, como recomienda el encargo | **Limitación conocida**: el régimen es exacto, pero el flujo en la cápsula iguala al de la unión en cada instante y las capacidades Foster en serie con el disipador crean modos lentos (unos 12 s con los valores por defecto) |
| Fuentes | analogía y redes Foster/Cauer: Infineon AN2008-03 y Semikron, *Application Manual Power Semiconductors*; pérdidas: Mohan, Undeland y Robbins, *Power Electronics*, Wiley | citas no verificadas (edición y apartado) |

## Modelos físicos: almacenamiento (Fase 3 del encargo)

`engine/heliora/lib/almacenamiento.py`. Convención: i > 0 en descarga. Valores por celda Li-ion NMC, **todos
estimados** (orden de magnitud): sustitúyelos por una caracterización real.

| Valor | En Heliora | Fuente o motivo |
|---|---|---|
| Modelo de batería | Thevenin: N_s·OCV(SoC) − i·R0 − v1 − v2, ramas RC, recuento de carga | Plett, *Battery Management Systems, Vol. 1*, Artech House (cita no verificada) |
| Capacidad por celda | 5 Ah | **estimado** (el encargo no la fija) |
| OCV(SoC) | 3.00 / 3.45 / 3.55 / 3.62 / 3.67 / 3.72 / 3.79 / 3.87 / 3.96 / 4.07 / 4.20 V en SoC 0 … 1 | **estimado**: forma típica NMC, monótona de 3.0 a 4.2 V (Anexo B.1) |
| R0 | 30 mΩ | **estimado** (Anexo B.1) |
| R1, C1 | 20 mΩ, 2000 F (τ = 40 s) | **estimado** (Anexo B.1) |
| Segunda rama RC | desactivada por defecto (R2 = 0) | Anexo B.1 la deja opcional |
| η culómbica | 0.99, solo en carga | **estimado** (Anexo B.1) |
| v_min / v_max | 2.5 V / 4.2 V por celda | de hoja (Anexo B.1) |
| i_max | 50 A por celda | **estimado** |
| Límites | dan un aviso; el modelo no corta la corriente | decisión: el corte es tarea del BMS, que se puede modelar con bloques de señal |
| Temperatura y envejecimiento | fuera de alcance | Anexo B.1 lo deja como extensión |
| Supercondensador | C·dv_C/dt = −i − v_C/R_fuga, v = v_C − i·R_esr; 100 F, 10 mΩ, 10 kΩ | ecuaciones del Anexo B.2 con el signo de la convención de descarga; valores **estimados** |
| Cargador: control de corriente | PI con prealimentación de v_bajo; ω_c = 2π·f_sw/10, Kp = L·ω_c, Ki = Kp·ω_c/5 | **estimado**: ajuste por cancelación de polo, ancho de banda una décima de f_sw |
| Cargador: lazo de tensión | PI con retrocálculo, Tt = Kp_v/Ki_v; Kp_v = 1 A/V, Ki_v = 10 A/(V·s) | **estimado**. La integración condicional hacía oscilar a BDF en el límite de corriente |
| Cargador: rendimiento | i_alto = d·i_L/η (alto → bajo) o d·i_L·η (bajo → alto); η = 0.97 | Anexo B.3; medido en las pruebas: el balance cierra con error 2e-5 (criterio 1 %) |
| Condensador del cargador | C = 0 por defecto (sin él) | con una batería en bornes, R0·C sería un modo de microsegundos que obliga a pasos diminutos |

## Modelos físicos: dominio mecánico y máquinas (Fase 4 del encargo)

`engine/heliora/lib/mecanica.py` y `engine/heliora/lib/maquinas.py`. Analogía par ↔ corriente, ω ↔ tensión; la
bancada (ω = 0) es la referencia. Ecuaciones de máquinas de Krause, Wasynczuk y Sudhoff, *Analysis of Electric
Machinery and Drive Systems*, IEEE Press / Wiley (cita no verificada).

| Valor | En Heliora | Fuente o motivo |
|---|---|---|
| PMSM | R_s 0.5 Ω, L_d = L_q 4 mH, λ_pm 0.1 Wb, p 4, J 1e-3 kg·m², B 1e-4 N·m·s | **estimado** (Anexo C.2) |
| Máquina de continua | R_a 1 Ω, L_a 10 mH, K_e = K_t 0.5 V·s/rad, J 0.01 kg·m², B 1e-3 N·m·s | **estimado** |
| Inducción (jaula, ~4 kW, 400 V, 4 polos) | R_s 1.5 Ω, R_r 1.2 Ω, L_ls = L_lr 6 mH, L_m 0.18 H, p 2, J 0.02 kg·m², B 2e-3 N·m·s | **estimado**, valores redondos propios |
| Inducción: formulación | cuarto orden en flujos, ejes estacionarios αβ (no necesita ángulo) | Anexo C.3 deja libre el marco |
| Inercia del rotor | dentro de la máquina si J > 0; con J = 0 la velocidad la fija la red mecánica externa | decisión: permite fuentes de velocidad en el eje (pruebas de cortocircuito y de par a deslizamiento fijo) |
| Dos inercias unidas por un engranaje ideal | **no se admite**: la red da un mensaje claro y pide sumar las inercias (con n²) | limitación de reducir la red a EDOs (estado duplicado); es uno de los motivos del diseño DAE de la Fase 5 |
| Comprobación | PMSM: K_t = 1.5·p·λ exacta, par de cortocircuito a 1e-9; inducción: par a deslizamiento impuesto y T_max de Kloss a 1e-8 del circuito equivalente; deslizamiento con carga a 1e-8 | pruebas de `pruebas_maquinas.py` |

## Lo que no se pudo saber

- Cómo se ve y se comporta la ventana original: no se ejecutó el programa, y la identidad de Heliora es propia a propósito.
- Las ecuaciones internas de los bloques cifrados de potencia y fotovoltaica. Las de Heliora vienen de la literatura
  publicada y están en la documentación de cada bloque.
