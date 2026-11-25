# Encargo para Craft Anything — Ampliar Heliora a modelos físicos

Documento de encargo para generar, con Craft Anything, las capacidades que le faltan al motor de
Heliora para construir modelos físicos más allá del dominio FV + electrónica de potencia actual.

El cuerpo principal es el **prompt maestro** (apto para pegar entero o por fases). El **Anexo A**
desarrolla la física de la Fase 2 (dominio térmico) para que el generador tenga poco margen de
inventar. El resto de fases pueden desarrollarse con el mismo nivel de detalle cuando toque.

---

## Prompt maestro

````markdown
# Encargo para Craft Anything — Ampliar Heliora a modelos físicos

## Contexto del proyecto
Heliora es una app de escritorio (Rust + egui en `src/`) con un motor de cálculo en Python
(`engine/heliora/`) que simula plantas solares FV por bloques. El motor habla con la ventana
por stdin/stdout con JSON por líneas. Vas a ampliar el MOTOR (Python). Lee primero:
- `engine/heliora/sim/diagrama.py` (compilación del diagrama, `REGISTRO`, `cargar_librerias`)
- `engine/heliora/sim/nucleo.py` (clase `Bloque`, puertos, `evaluar_numero` con unidades)
- `engine/heliora/sim/circuito.py` (red eléctrica MNA: clase `Elemento` y sus métodos)
- `engine/heliora/sim/solvers.py` (Euler, RK4, Dormand-Prince, BDF; cruces por cero)
- `engine/heliora/sim/bloques_genericos.py` y `engine/heliora/lib/*.py` (patrones a imitar)
- `engine/heliora/validacion/` (banco de pruebas) y `MECHANICS.md` (hoja de valores/procedencia)

## Reglas duras (no negociables)
1. **Idioma:** todo el código, nombres, docstrings y comentarios en **español**. Términos
   consolidados no se traducen (commit, solver, PWM...).
2. **Estilo:** mínimo y legible. Docstrings Google en tercera persona en toda clase y función.
   Comentarios solo para el *porqué*, no el *qué*. Prohibido comentar el cambio en el propio
   código (`# NUEVO`, `# antes...`, bloques comentados). El registro de qué cambió va aparte.
3. **Encaje arquitectónico:** cada bloque de señal es una clase corta registrada en `REGISTRO`
   con el patrón de `bloques_genericos.py` (`n_estados()`, `x0()`, `preparar`, salidas, y
   `de_evento`/`proximo_evento` si aplica). Cada elemento eléctrico sigue la interfaz `Elemento`
   de `circuito.py` (`estampar_fijo/estado/din`, `rhs`, `derivada_e`, `salida_e`, `actualizar`,
   `cruce_e`, `tras_paso`, banderas `con_estado_discreto`/`no_lineal`). Las librerías nuevas se
   cargan desde `diagrama.cargar_librerias`.
4. **Parámetros con unidad:** declara la unidad de cada parámetro y usa `evaluar_numero` para
   admitir `10 mH`, `1 kΩ`, etc., validando la unidad.
5. **Reutiliza el motor existente:** solver, resolvedor de lazos (región de confianza) y red MNA.
   No reinventes integradores ni álgebra lineal salvo que una fase lo pida explícitamente.
6. **No romper nada:** los diagramas de `engine/ejemplos/*.json` deben seguir simulando igual.
   Mantén el formato de diagrama en **JSON canónico** (claves ordenadas, sangría de 2 espacios).
7. **Procedencia:** valores físicos tomados de fuentes verificables se anotan en `MECHANICS.md`
   y `mechanics.json`, marcando lo **estimado** como tal. No copies código, textos ni formatos
   de MATLAB/Simulink: escribe desde cero a partir de literatura publicada citada en el docstring.
8. **Pruebas:** cada entrega añade pruebas en `engine/heliora/validacion/` con el decorador
   `@prueba` y valores esperados analíticos. Deben pasar con `python validar_todo.py`.
9. **UI (verificado):** la paleta de la ventana se genera sola desde el catálogo que publica el
   motor (`src/bloques.rs`: `m.llamar("catalogo")` → `fijar_catalogo` → `paleta`). Todo bloque
   nuevo registrado en el motor aparece automáticamente en la paleta, con sus parámetros, sin tocar
   Rust. Solo hace falta editar Rust si un bloque introduce una **categoría nueva** y se quiere
   colocar en una posición concreta de la paleta: añadir el nombre de la categoría al array `prio`
   de `src/bloques.rs:398` (si no se añade, la categoría nueva igualmente aparece, al final). Cada
   ficha del catálogo declara su `categoria`; reutiliza las existentes (p. ej. "Potencia · Control")
   cuando encaje, y crea categorías nuevas solo para dominios nuevos (p. ej. "Térmica", "Mecánica",
   "Almacenamiento"), añadiéndolas entonces a `prio`.
10. Entrega cada fase con: lista de archivos tocados, resumen de 5-10 líneas para el `historial/`,
    y la salida de las pruebas nuevas.

---

## FASE 1 — Bloques de señal/control genéricos y tiempo discreto
Añade a `bloques_genericos.py` (o un nuevo `lib/senal.py` si crece):
- Bloques: `pid` (con anti-windup y filtro de derivada N), `rele` (con histéresis), `zona_muerta`,
  `limitador_tasa`, `saturacion_tasa`, `min_max`, `comparador`, `logica` (AND/OR/NOT/XOR),
  `trigonometrica`/`matematica` (sin, cos, exp, log, pow, sqrt...), `lookup_2d`, `lookup_nd`,
  `combinar_bus`/`seleccionar_bus`, `ruido` (blanco/uniforme con semilla), `producto_matricial`.
- **Dominio discreto:** introduce el concepto de tiempo de muestreo. Bloques: `retenedor_zoh`,
  `retardo_unitario`, `integrador_discreto`, `muestreo` y `transicion_tasa`. El solver debe caer
  exactamente en los instantes de muestreo (reutiliza el mecanismo `proximo_evento`).
Criterio de aceptación: un control PID discreto a 10 kHz sobre una planta de primer orden da el
mismo resultado (±1%) que su equivalente continuo con el paso fino. Pruebas analíticas para cada
bloque sin estado.

## FASE 2 — Dominio térmico y acoplamiento electro-térmico
Crea `engine/heliora/lib/termica.py`. Modela el dominio térmico como una red análoga a la eléctrica
(a través = flujo de calor, entre = temperatura), reutilizando la maquinaria de `circuito.py`:
- Elementos: `resistencia_termica` (R_th, K/W), `capacidad_termica` (C_th, J/K), `fuente_calor`
  (W), `fuente_temperatura` (K), `ambiente` (temperatura fija). Puertos térmicos nuevos.
- **Acoplamiento:** conecta las pérdidas que `IGBT`/`InversorConmutado` (`lib/electrica.py`,
  `lib/potencia.py`) **ya calculan** (energía y potencia de conducción y conmutación) a un puerto
  de calor, y haz que los parámetros dependientes de temperatura (Vce0, Ron, Vf) lean la
  temperatura de unión resultante. Cadena unión → cápsula → disipador → ambiente con R_th/C_th.
Criterio de aceptación: con una pérdida constante conocida, la temperatura de unión en régimen
permanente coincide con `T_amb + P·ΣR_th` analítico; el transitorio sigue la constante térmica
`R_th·C_th`. Documenta cada valor por defecto en `MECHANICS.md` (medido vs estimado).
**El detalle físico completo de esta fase está en el Anexo A de este documento.**

## FASE 3 — Almacenamiento (dominio electroquímico)
Crea `engine/heliora/lib/almacenamiento.py`:
- `bateria` (modelo de circuito equivalente: OCV(SoC) por tabla/curva + R interna + 1-2 ramas RC;
  integra el SoC por coulomb counting), `supercondensador`, y un `cargador_dcdc` bidireccional
  (bidirectional buck-boost) como bloque de potencia. Parámetros: capacidad (Ah), OCV(SoC), R0,
  R1/C1, SoC inicial, límites de tensión/corriente.
Criterio de aceptación: carga/descarga a corriente constante reproduce la curva OCV(SoC) y el
balance de energía cierra (±1%). Enlaza el bloque al bus DC de la planta FV de ejemplo.

## FASE 4 — Máquinas eléctricas y dominio mecánico
Crea `engine/heliora/lib/mecanica.py` (dominio mecánico rotacional: `inercia` J, `friccion`,
`resorte_torsional`, `engranaje`, `par_carga`, puertos de par/velocidad) y
`engine/heliora/lib/maquinas.py`:
- `pmsm` (imán permanente en dq), `maquina_induccion` (jaula, en dq), `maquina_dc`. Cada una
  acopla el dominio eléctrico 3φ con el mecánico (par electromagnético ↔ velocidad).
Criterio de aceptación: PMSM en vacío alcanza la velocidad síncrona; el par en cortocircuito y la
constante de par coinciden con la fórmula; arranque de inducción con par de carga converge a un
deslizamiento estable. Pruebas analíticas de régimen permanente.

## FASE 5 — (DECISIÓN DE DISEÑO, no construir aún) Motor acausal multidominio con DAE
Antes de escribir código, entrega un **documento de diseño** (`docs/diseno_motor_dae.md`) que
proponga cómo pasar de la red eléctrica embebida (hoy MNA densa reducida a ODE, resolvedor de no
lineales por punto fijo) a un motor de redes físicas acausales multidominio:
- Formulación DAE con matriz de masa (posible singular), condiciones iniciales consistentes,
  solver implícito (evaluar scipy BDF/IDA vs propio), matrices **dispersas** y resolvedor de red
  por **Newton con Jacobiano** (hoy es punto fijo que puede no converger en silencio).
- Plan de migración que **no rompa** las librerías ya escritas en Fases 1-4.
- Estimación de esfuerzo y riesgos. Esta fase decide si Heliora se queda como simulador eléctrico
  muy bueno o se convierte en un Simscape real; no la implementes sin revisión humana.

## Transversal (todas las fases)
- Ejecuta `python validar_todo.py` y adjunta el resumen. Ninguna prueba previa puede romperse.
- Añade un diagrama de ejemplo en `engine/ejemplos/` por fase que ejercite lo nuevo.
- No toques `.env` ni credenciales. No añadas dependencias nuevas salvo que la fase lo justifique
  (hoy el motor solo usa numpy, scipy, pandas, scikit-learn, statsmodels, pvlib, pyarrow, joblib).
````

---

## Anexo A — Detalle físico de la Fase 2 (dominio térmico)

Objetivo: modelar el camino térmico de los semiconductores y realimentar la temperatura de unión a
las pérdidas que el motor **ya calcula**, reutilizando la maquinaria de red de `circuito.py`.

### A.1 Analogía térmica-eléctrica

El dominio térmico se resuelve con la misma MNA que el eléctrico sustituyendo las magnitudes:

| Magnitud térmica | Unidad | Análoga eléctrica | Elemento de red |
|---|---|---|---|
| Flujo de calor `Q` | W | corriente `i` | `fuente_calor` = fuente de corriente |
| Temperatura de nodo `T` | °C (o K) | tensión de nodo `v` | — |
| Resistencia térmica `R_th` | K/W | resistencia `R` | `resistencia_termica` |
| Capacidad térmica `C_th` | J/K | capacidad `C` | `capacidad_termica` (estado = `T`) |
| Ambiente | °C | nodo de referencia a tensión fija | `ambiente` / `fuente_temperatura` |

Implicación de implementación: `resistencia_termica` estampa una conductancia `1/R_th` entre dos
nodos térmicos (igual que `Resistor`); `capacidad_termica` aporta un estado `T` con `dT/dt = Q/C_th`
(igual que `Capacitor`, cuyo estado es la tensión); `fuente_calor` inyecta `Q` en un nodo leyendo una
señal de entrada (igual que `FuenteIControlada`); `ambiente` fija la temperatura de un nodo (igual que
`FuenteDC`, pero el nodo de referencia térmico está a `T_amb`, no a 0). El ambiente es el nodo de
referencia del subsistema térmico; no se mezcla con la tierra eléctrica.

### A.2 Redes Foster y Cauer

El camino unión→ambiente se describe con una red RC térmica. Hay dos formas, y conviene ofrecer las
dos con un parámetro `modo` ("foster" | "cauer"):

- **Cauer** (física): escalera de `R_th`-`C_th` a referencia. Los nodos tienen temperatura real
  (unión, cápsula, disipador), así que se pueden medir y acoplar con otras redes.
- **Foster** (de hoja de datos): pares `R_i`-`C_i` en serie que reproducen la impedancia térmica
  transitoria publicada `Z_thJC(t) = Σ_i R_i · (1 − e^{−t/τ_i})`, con `τ_i = R_i · C_i`. Los nodos
  internos **no** tienen significado físico y una red Foster **no** se puede poner en serie con otra
  sin convertirla primero. Es la forma en que los fabricantes dan los datos.

Recomendación de modelo por defecto: unión→cápsula con **Foster** (3–4 pares, de la hoja del
módulo), cápsula→disipador con una `R_th` de interfaz (grasa/pad), y disipador→ambiente con **Cauer**
`R_th`-`C_th`. Si se necesita la temperatura física de cápsula, convertir el tramo Foster a Cauer.

### A.3 Pérdidas como fuente de calor

El motor ya calcula las energías de conducción y conmutación del IGBT y del inversor (campos `Eon`,
`Eoff`, `Err`, `Vref`, `Iref`, `Voff` en `lib/electrica.py` y `lib/potencia.py`). La Fase 2 solo debe
encaminar esa potencia a la `fuente_calor` del nodo de unión. Fórmulas estándar (Mohan, Undeland,
Robbins, *Power Electronics*, Wiley):

- **Conducción, IGBT:** `P_cond,Q = Vce0(Tj)·I_media + Ron(Tj)·I_rms²` (solo mientras conduce).
- **Conducción, diodo:** `P_cond,D = Vf(Tj)·I_media + Rd(Tj)·I_rms²`.
- **Conmutación, IGBT:** `P_sw,Q = f_sw · (Eon + Eoff) · (V_dc/Vref) · (I/Iref)^k_i`.
- **Recuperación, diodo:** `P_rr = f_sw · Err · (V_dc/Vref) · (I/Iref)^k_i`.

`k_i` es el exponente de escala de pérdidas con la corriente; por defecto `k_i = 1` (lineal),
ajustable. Separa la pérdida por dispositivo (cada IGBT y cada diodo tienen su propia unión y su
propio nodo térmico), no una sola pérdida agregada del convertidor.

### A.4 Dependencia de los parámetros con la temperatura

La realimentación electro-térmica exige que los parámetros de conducción dependan de `Tj`:

- `Vce0(Tj) = Vce0_ref + α_Vce · (Tj − T_ref)`
- `Ron(Tj)  = Ron_ref · (1 + α_Ron · (Tj − T_ref))`
- `Vf(Tj)`, `Rd(Tj)` análogos para el diodo.
- Energías: `E(Tj) = E_ref · (1 + α_E · (Tj − T_ref))`.

Las hojas de datos dan los valores a 25 °C y a 125 °C; de ahí se obtiene cada coeficiente `α`. Con
`T_ref = 125 °C` (punto de los datos de ejemplo del código) queda coherente con `MECHANICS.md`.

### A.5 Valores por defecto (todos ESTIMADOS — sustituir por hoja de datos)

Para el módulo de ejemplo del código (1200 V / 600 A, tipo FZ600R12KE3). **Son marcadores de
posición**; el usuario debe reemplazarlos por los de su inversor. Anotar como estimados en
`MECHANICS.md`:

| Parámetro | Valor por defecto | Estado |
|---|---|---|
| `R_thJC` IGBT | 0.045 K/W | estimado |
| `R_thJC` diodo | 0.090 K/W | estimado |
| `R_thCH` (cápsula–disipador, con grasa) | 0.020 K/W | estimado |
| `R_thHA` (disipador–ambiente) | parámetro del disipador/caudal | del usuario |
| `τ_i` Foster unión–cápsula | 0.5 / 5 / 50 / 500 ms | estimado |
| `T_amb` | 40 °C | por defecto editable |
| `Tj_max` (aviso) | 150 °C | límite de hoja |
| `α_Ron` | +0.6 %/K | estimado |
| `α_Vce` (tensión umbral) | −2 mV/K | estimado |
| `α_E` (energías de conmutación) | +0.3 %/K | estimado |

Fuentes a citar en los docstrings (verificar edición exacta antes de fijar la cita):
analogía y redes Foster/Cauer — Infineon Application Note AN2008-03 *Thermal equivalent circuit
models* y Semikron *Application Manual Power Semiconductors*; ecuaciones de pérdidas — Mohan,
Undeland, Robbins, *Power Electronics: Converters, Applications and Design*, Wiley.

### A.6 Interfaz con los bloques existentes (retrocompatible)

- `IGBT`, `InversorConmutado` y el diodo ganan un **puerto térmico opcional**. Si no se conecta, el
  bloque se comporta igual que hoy (temperatura constante = `T_ref`): ningún ejemplo existente cambia.
- Si el puerto se conecta: el bloque expone `P_perdidas` hacia el nodo de unión y lee `Tj` de ese
  mismo nodo para evaluar `Vce0(Tj)`, `Ron(Tj)`, etc.
- Nota sobre el lazo: como el nodo de unión lleva una `capacidad_termica` (estado `Tj`), la
  realimentación pérdidas↔temperatura es una dinámica lenta con estado propio, **no un lazo
  algebraico**. No hace falta el resolvedor de lazos salvo que se omita `C_th` (desaconsejado).

### A.7 Validación (pruebas a añadir en `validacion/`)

1. **Régimen permanente:** `fuente_calor` constante `P` → `Tj = T_amb + P·ΣR_th` (analítico).
2. **Transitorio de un polo:** una `R_th`-`C_th` → `Tj(t) = T_amb + P·R_th·(1 − e^{−t/(R_th·C_th)})`;
   comprobar la constante de tiempo.
3. **Foster:** `Z_thJC(t)` simulada reproduce `Σ_i R_i·(1 − e^{−t/τ_i})` dentro de tolerancia.
4. **Lazo electro-térmico:** subir la corriente de carga sube las pérdidas, que suben `Tj`, que sube
   `Vce,sat`; comprobar que converge a un punto de operación estable (no divergente).

---

## Anexo B — Detalle físico de la Fase 3 (almacenamiento)

Objetivo: añadir batería, supercondensador y un cargador DC-DC bidireccional, enlazables al bus DC
de la planta FV. Modelos de circuito equivalente, que encajan en la MNA eléctrica como fuentes con
estado.

### B.1 Batería — modelo de circuito equivalente (ECM / Thevenin)

Convención: `i > 0` en descarga. Tensión en bornes:

```
v = OCV(SoC) − i·R0 − v1 − v2
```

Ramas RC (una o dos) y balance de carga (coulomb counting):

```
dv1/dt = i/C1 − v1/(R1·C1)          (ídem v2 con R2, C2)
dSoC/dt = −(η·i) / (3600·Q_Ah)      (η = eficiencia culómbica, aplicada solo en carga)
```

- **Estados:** `SoC`, `v1` (y `v2` si hay segunda rama).
- **Encaje en la MNA:** es una fuente de tensión con resistencia serie `R0`. En cada evaluación el
  `SoC` y los `v_i` son estados conocidos, así que el valor de la fuente es `OCV(SoC) − v1 − v2` con
  `R0` en serie (`estampar_estado`); la corriente de rama `i` que resuelve la red alimenta las
  derivadas de los estados (`rhs`). El elemento es **no lineal** solo a través de `OCV(SoC)`, que es
  una tabla interpolada; no requiere iteración propia porque `SoC` es estado.
- **Parámetros:** `Q_Ah` (capacidad), `OCV(SoC)` (tabla, p. ej. 11 puntos de 0 a 100 %), `R0`,
  `R1`/`C1` (y `R2`/`C2`), `SoC0`, `η`, límites `v_min`/`v_max` e `i_max`. `N_s`×`N_p` para escalar
  de celda a paquete (`v` ×`N_s`, `Q_Ah` ×`N_p`, `R0` ×`N_s`/`N_p`).
- **Fuera de alcance de la fase (anotar como extensión):** dependencia con temperatura y
  envejecimiento (capacidad y `R0` función de ciclos). Dejar los coeficientes preparados pero fijos.

Valores por defecto por celda, tipo Li-ion NMC (**todos ESTIMADOS**, orden de magnitud; sustituir
por caracterización real):

| Parámetro | Valor | Estado |
|---|---|---|
| Tensión nominal | 3.6 V | referencia |
| `OCV(SoC)` | 3.0 V (0 %) → 4.2 V (100 %), monótona | estimado |
| `R0` | 30 mΩ | estimado |
| `R1`, `C1` | 20 mΩ, 2000 F (τ ≈ 40 s) | estimado |
| `η` (culómbica) | 0.99 | estimado |
| `v_min` / `v_max` | 2.5 V / 4.2 V | de hoja |

Referencia a citar: Plett, *Battery Management Systems, Vol. 1: Battery Modeling*, Artech House.

### B.2 Supercondensador

Modelo simple: condensador grande con resistencia serie equivalente y fuga opcional.

```
dq/dt = i ;  v = q/C − i·R_esr ;  fuga: i_fuga = v/R_fuga
```

Parámetros: `C` (F), `R_esr` (Ω), `R_fuga` (Ω, grande), `v0`. Estado: `q`. Opcional: segunda rama
RC para la respuesta lenta del electrodo poroso.

### B.3 Cargador DC-DC bidireccional (modelo promediado)

Reutiliza el patrón de `BoostPromediado`/`PuenteHPromediado`, pero con corriente en ambos sentidos:

```
v_bajo = d · v_alto           (relación de conversión, d en [0,1])
balance de potencia con rendimiento η:  v_alto·i_alto = η^(±1) · v_bajo·i_bajo
```

Entrada de control: referencia de corriente (o de tensión). Parámetros: `L`, `C`, `f_sw`, `η`, modo
(control de corriente / de tensión), límites. Criterio de aceptación: carga a corriente constante de
la batería reproduce su curva `OCV(SoC)` y el balance de energía cierra (±1 %).

---

## Anexo C — Detalle físico de la Fase 4 (máquinas y dominio mecánico)

Objetivo: dominio mecánico rotacional (nuevo) y tres máquinas que acoplan el dominio eléctrico 3φ
con el mecánico. Las máquinas se formulan en ejes `dq`.

### C.1 Dominio mecánico rotacional (analogía)

| Magnitud mecánica | Unidad | Análoga eléctrica | Elemento |
|---|---|---|---|
| Par `T` | N·m | corriente | `par_carga` = fuente de par |
| Velocidad angular `ω` | rad/s | tensión de nodo | — |
| Inercia `J` | kg·m² | capacidad | `inercia` (estado `ω`; además `dθ/dt = ω`) |
| Fricción viscosa `B` | N·m·s | conductancia (resistor) | `friccion` |
| Rigidez torsional `K` | N·m/rad | inductancia | `resorte_torsional` |
| Relación de engranaje `n` | — | transformador ideal | `engranaje` (`ω2 = ω1/n`, `T2 = n·T1`) |

Referencia mecánica = `ω = 0` (bancada). Se resuelve con la misma MNA. `par_carga` admite par
constante o dependiente de la velocidad (p. ej. ventilador `T ∝ ω²`, bomba).

### C.2 PMSM (imán permanente, ejes dq del rotor)

Ecuaciones estándar (Krause, Wasynczuk, Sudhoff, *Analysis of Electric Machinery and Drive
Systems*, IEEE Press/Wiley):

```
vd = Rs·id + Ld·did/dt − ωe·Lq·iq
vq = Rs·iq + Lq·diq/dt + ωe·(Ld·id + λ_pm)
Te = (3/2)·p·( λ_pm·iq + (Ld − Lq)·id·iq )
ωe = p·ωm
J·dωm/dt = Te − T_carga − B·ωm ;  dθm/dt = ωm
```

- **Estados:** `id`, `iq`, `ωm`, `θm`. `p` = pares de polos.
- **Acoplamiento eléctrico (encaje en la MNA 3φ):** medir `v_abc` en los bornes, transformar a `dq`
  con el ángulo del rotor (reutilizar `park` de `lib/potencia.py`), integrar `id`, `iq`, y reinyectar
  `i_abc` con `park_inv` como fuentes de corriente controladas 3φ (patrón `FuenteIControlada` 3φ).
  Puerto mecánico para `ωm`/`Te`.
- **Parámetros:** `Rs`, `Ld`, `Lq`, `λ_pm` (flujo del imán), `p`, `J`, `B`. Defectos de un servo PMSM
  pequeño (**estimados**): `Rs` 0.5 Ω, `Ld=Lq` 4 mH, `λ_pm` 0.1 Wb, `p` 4, `J` 1e-3 kg·m², `B` 1e-4.

### C.3 Máquina de inducción (jaula de ardilla, ejes dq)

Modelo de 4º orden en `dq` (marco estacionario o síncrono) con estados de flujo o de corriente de
estátor y rotor, par y ecuación mecánica (misma fuente, Krause):

```
Te = (3/2)·p·( ψds·iqs − ψqs·ids )
deslizamiento s = (ωs − ωr)/ωs
J·dωm/dt = Te − T_carga − B·ωm
```

Parámetros: `Rs`, `Rr`, `Lls`, `Llr`, `Lm`, `p`, `J`, `B`. Defectos de un motor de jaula pequeño
(**estimados**), documentados en `MECHANICS.md`. Criterio: arranque con par de carga converge a un
deslizamiento estable.

### C.4 Máquina de corriente continua

```
va = Ra·ia + La·dia/dt + Ke·ωm
Te = Kt·ia      (con Kt = Ke en SI)
J·dωm/dt = Te − T_carga − B·ωm
```

Estados: `ia`, `ωm`. Parámetros: `Ra`, `La`, `Ke=Kt`, `J`, `B`. Régimen permanente de comprobación:
`ia = T_carga/Kt`, `ωm = (va − Ra·ia)/Ke`.

### C.5 Validación (pruebas a añadir en `validacion/`)

1. **PMSM en vacío:** bornes a fuente de tensión 3φ equilibrada, sin carga → `ωm` tiende a la
   síncrona `ωs/p`; `Kt = (3/2)·p·λ_pm` comprobado contra el par medido a corriente dada.
2. **DC en régimen permanente:** `ωm_ss = (va − Ra·T_carga/Kt)/Ke` (analítico).
3. **Inducción:** arranque con par de carga constante → deslizamiento estable; par máximo (de
   descolgamiento) coincide con la fórmula de Kloss dentro de tolerancia.
4. **Dominio mecánico:** `inercia` con par constante → `ω(t) = (T/B)·(1 − e^{−(B/J)t})` (analítico).

---

## Anexo D — Detalle de la Fase 1 (bloques genéricos y tiempo discreto)

Objetivo: cubrir los bloques de señal/control que hoy obligan a usar `funcion_python`, e introducir
el **tiempo de muestreo**, que el motor aún no tiene. Todo encaja en el patrón de
`bloques_genericos.py` (`n_estados`, `x0`, `preparar`, salida, `de_evento`, `proximo_evento`,
`paso_mayor`). Van a `bloques_genericos.py` o a un `lib/senal.py` si el archivo crece.

### D.1 Convenciones de encaje

- **Feedthrough directo** (`directo=True`, pueden entrar en lazos algebraicos, que el motor ya
  resuelve): `matematica`, `comparador`, `logica`, `zona_muerta`, `min_max`, `lookup_2d/nd`,
  `producto_matricial`, `combinar_bus`/`seleccionar_bus`.
- **Con estado continuo:** `pid`, `limitador_tasa`.
- **Con cruces por cero** (declaran `de_evento` sobre una señal `g`, solo en paso variable, como
  `Saturacion`/`Switch`): `rele`, `zona_muerta`, `comparador`, `min_max`.
- **Discretos** (sección D.3): su salida es un valor retenido (estado), actualizado solo en los
  instantes de muestreo.

### D.2 Bloques continuos y algebraicos

**`pid`** — PID paralelo con filtro de derivada y anti-windup por retrocálculo (Åström & Hägglund,
*Advanced PID Control*, ISA). Sobre el error `e = r − y`:

```
P = Kp·e
dI/dt = Ki·e + (1/Tt)·(u_sat − u)          # retrocálculo: frena el integrador al saturar
dd/dt = N·(e − d) ;  D = Kd·N·(e − d)       # derivada filtrada: Kd·s·N/(s+N)
u = P + I + D ;  u_sat = sat(u, u_min, u_max) ;  salida = u_sat
```

- Estados: `I`, `d`. Parámetros: `Kp`, `Ki`, `Kd`, `N` (coef. de filtro, por defecto 10), `Tt`
  (tiempo de seguimiento, por defecto `Kd/Ki` si `Ki>0`), `u_min`, `u_max`.
- Opciones: derivada sobre la medida `y` en vez del error (evita el "golpe de derivada"); pesos de
  consigna `b`, `c` (opcional). Si `u_min`/`u_max` no se fijan, el anti-windup se desactiva.
- Nota: el `PIAntiWindup` de `lib/potencia.py` es un PI; este `pid` lo generaliza. No lo dupliques,
  reutiliza su patrón de anti-windup.

**`rele`** — salida conmutada con histéresis (Schmitt). Sube a `salida_on` cuando `u ≥ punto_subida`,
baja a `salida_off` cuando `u ≤ punto_bajada` (`punto_subida > punto_bajada`). Modo interno discreto;
`de_evento` sobre `u − umbral` del modo activo. Parámetros: `punto_subida`, `punto_bajada`,
`salida_on`, `salida_off`.

**`zona_muerta`** — `y = 0` si `lo ≤ u ≤ hi`; `y = u − hi` si `u > hi`; `y = u − lo` si `u < lo`.
Cruces por cero en `u = lo` y `u = hi`. Parámetros: `lo`, `hi`.

**`limitador_tasa`** — limita la pendiente de la salida (unifica lo que el prompt llamaba
`limitador_tasa` y `saturacion_tasa`, que son lo mismo). Versión continua con estado `y`:

```
dy/dt = max(R_baja, min(R_alta, k·(u − y)))    # k grande: sigue a u salvo que la pendiente sature
```

Parámetros: `R_alta` (≥0), `R_baja` (≤0), `k` (ganancia de seguimiento, por defecto grande). Es una
aproximación continua; la versión exacta por muestra (`|Δy| ≤ R·Ts`) está en D.3.

**`min_max`** — `y = min(u_i)` o `y = max(u_i)` según `modo`; puertos dinámicos. Cruce por cero
cuando cambia la entrada seleccionada (opcional). **`comparador`** — `y ∈ {0,1}` de
`u1 {<,≤,>,≥,==,≠} u2`; cruce por cero en `u1 − u2`. **`logica`** — AND/OR/NOT/XOR/NAND/NOR sobre
entradas interpretadas como verdaderas si `> 0.5`; puertos dinámicos.

**`matematica`** — función unaria o binaria por parámetro `funcion`: `sin, cos, tan, asin, acos,
atan, atan2, exp, log, log10, sqrt, pow, sign, mod, rem, cuadrado, reciproco`. (`abs` ya existe como
`absoluto`.) Ángulos en radianes; añadir `grados`/`radianes` como conversión.

**`lookup_2d` / `lookup_nd`** — interpolación lineal sobre rejilla de puntos de ruptura (2 o n
entradas). Parámetros: vectores de ruptura por eje y tabla (matriz/`ndarray`), más `fuera`
(saturar | extrapolar), igual que el `lookup` 1-D existente.

**`producto_matricial`** — `y = A·u` con `A` matriz constante (parámetro), o `y = U1·U2` con dos
entradas matriciales (`modo`). Valida formas y lanza error con el nombre del bloque si no casan.

**`combinar_bus` / `seleccionar_bus`** — agrupan señales con nombre y seleccionan por nombre. Nota:
`mux`/`demux` ya cubren el caso posicional (concatenar/separar vector); estos añaden solo el
etiquetado. Mantenlos ligeros; si no aportan sobre `mux`/`demux`, decláralo y no los construyas.

**`ruido`** — fuente de ruido blanco de banda limitada: una muestra aleatoria retenida durante el
tiempo de correlación `Tc`, escalada por `sqrt(PSD/Tc)` (como el *Band-Limited White Noise* clásico).
Usa el mecanismo de muestreo de D.3. Parámetros: `PSD` (potencia), `Tc`, `semilla`, `tipo`
(gaussiano | uniforme). Reproducible con la semilla.

### D.3 Dominio discreto: el tiempo de muestreo (cambio de motor)

Hoy el motor no tiene tiempo de muestreo (lo único discreto es `paso_mayor` al final de cada paso
aceptado). Esta parte **lo añade**, sin solver nuevo y reutilizando dos mecanismos existentes:

- **Instantes exactos:** un bloque discreto declara `Ts` (periodo) y `offset`. El modelo agrega el
  próximo tic de todos los bloques discretos y lo expone en `proximo_evento` junto a los eventos
  continuos; el solver ya cae exactamente en ese instante (como hace con `escalon`/`pulso`).
- **Actualización en el tic:** en `paso_mayor`, cada bloque cuyo tic coincide con `t` (dentro de
  `|t − (k·Ts + offset)| ≤ tol`) actualiza su estado discreto. Entre tics, la salida es el valor
  retenido (estado) → desde el lado continuo es una señal escalonada (ZOH). La discontinuidad la
  gestiona el evento programado, **no** un cruce por cero.

Protocolo a añadir al `Bloque`: atributos `Ts`, `offset` y un método `tic(t, u)` que devuelve el
nuevo estado retenido. El agregador de `proximo_evento` y el despacho de `paso_mayor` en
`diagrama.py` deben contemplar los tics. Con paso fijo (Euler/RK4), si `Ts` no es múltiplo del paso
se emite aviso (igual que con los cruces por cero).

Bloques discretos:

- **`muestreo`** — muestrea la entrada continua en cada tic (continuo → discreto).
- **`retenedor_zoh`** — retención de orden cero del último valor muestreado (discreto → continuo).
- **`retardo_unitario`** (`z⁻¹`) — salida en el tic `k` = entrada en el tic `k−1`. Estado: valor
  previo; `valor_inicial` parámetro.
- **`integrador_discreto`** — `x[k+1] = x[k] + Ts·u[k]` (Euler adelante), con `metodo` (adelante |
  atrás | trapezoidal) y límites/anti-windup opcionales, como el `integrador` continuo.
- **`transicion_tasa`** — transferencia segura entre dos tasas (o continuo↔discreto); en simulación
  no de tiempo real equivale a un remuestreo ZOH. Evita leer un valor a medio actualizar.
- **Opcional** `funcion_transferencia_z` — `num(z)/den(z)` a espacio de estados discreto, análogo al
  `funcion_transferencia` continuo.
- Versiones discretas de `pid` y `limitador_tasa`: el PID discreto con `Ts` debe coincidir (±1 %) con
  el continuo a `Ts` pequeño; el limitador exacto cumple `|Δy| ≤ R·Ts` por tic.

### D.4 Validación (pruebas a añadir en `validacion/`)

1. **Bloques sin estado** (`matematica`, `comparador`, `logica`, `zona_muerta`, `lookup_2d/nd`,
   `producto_matricial`): valores exactos contra el cálculo analítico.
2. **PID:** control PID discreto a 10 kHz sobre una planta de primer orden ≈ el continuo con paso
   fino (±1 %); comprobar que el anti-windup frena el integrador en saturación.
3. **Cruces por cero:** `rele` y `zona_muerta` localizan el cambio en el instante exacto (comprobar
   el tiempo del evento).
4. **Discreto:** `retardo_unitario` devuelve exactamente la muestra anterior; `retenedor_zoh` mantiene
   el valor entre tics; `integrador_discreto` (Euler adelante) de entrada constante da `Ts·u·k`.
5. **Instantes de muestreo:** con paso variable, el solver cae exactamente en `k·Ts + offset` (el
   error de tiempo de evento es ≤ tolerancia).
6. **Limitador de tasa:** entrada en rampa más rápida que `R` → la salida sube con pendiente `R`
   exacta.
