# Heliora

Aplicación de escritorio de cálculo técnico y simulación por bloques, para modelar una planta solar
fotovoltaica con sus inversores, analizar sus datos de operación y planificar su mantenimiento.

- **Ventana:** Rust con egui (`src/`).
- **Motor de cálculo:** Python con NumPy, SciPy, pandas, scikit-learn, statsmodels y pvlib (`engine/heliora/`).
  Se comunica con la ventana por entrada y salida estándar, con JSON por líneas.

## Ejecutar

    cargo run

La aplicación busca Python en `.venv/` junto al motor. Si no está, ofrece un botón «Instalar el motor».
También se puede preparar a mano:

    python3 -m venv .venv
    .venv/bin/pip install -r engine/requirements.txt

En Windows (PowerShell), con Python 3.11 o 3.12 de python.org y Rust con la cadena MSVC
(el instalador `rustup-init` ofrece instalar Visual Studio Build Tools si faltan):

    py -3 -m venv .venv
    .venv\Scripts\python -m pip install -r engine\requirements.txt
    cargo run

En Windows la aplicación busca `.venv\Scripts\python.exe` y, si no está, `python`. La instalación
desde el botón usa el lanzador `py`. Si Python está en otro sitio, la variable `HELIORA_PYTHON` indica
la ruta del intérprete; `HELIORA_ENGINE`, la de la carpeta `engine`.

## Dónde está cada cosa

| Tema | Archivo |
|---|---|
| Solvers | `engine/heliora/sim/solvers.py` |
| Bloques genéricos | `engine/heliora/sim/bloques_genericos.py` |
| Red eléctrica (análisis nodal) | `engine/heliora/sim/circuito.py` |
| Elementos eléctricos | `engine/heliora/lib/electrica.py` |
| Convertidores, modulación, control y medida | `engine/heliora/lib/potencia.py` |
| Fotovoltaica | `engine/heliora/lib/fotovoltaica.py` |
| Señal, control (PID) y tiempo discreto | `engine/heliora/lib/senal.py` |
| Dominio térmico y uniones de semiconductores | `engine/heliora/lib/termica.py` |
| Batería, supercondensador y cargador DC-DC | `engine/heliora/lib/almacenamiento.py` |
| Dominio mecánico rotacional | `engine/heliora/lib/mecanica.py` |
| Máquinas (continua, PMSM, inducción) | `engine/heliora/lib/maquinas.py` |
| Diseño propuesto del motor DAE (sin construir) | `docs/diseno_motor_dae.md` |
| Estadística | `engine/heliora/lib/estadistica.py` |
| Aprendizaje automático | `engine/heliora/lib/aprendizaje.py` |
| Mantenimiento | `engine/heliora/lib/mantenimiento.py` |
| Formularios de análisis | `engine/heliora/herramientas.py` |
| Pruebas por fases | `engine/heliora/validacion/` |
| Diagramas de ejemplo en JSON | `engine/ejemplos/` |

Cada bloque es una clase corta con sus ecuaciones en su documentación. Desde la aplicación se abre con
«Ver y editar código»; al guardar, el motor vuelve a cargar los bloques.

## Proyectos

Un proyecto es una carpeta: `proyecto.json`, `diagramas/*.json`, `datos/*.csv` o `*.parquet`, `scripts/*.py`
y `mantenimiento.json`.

### Trabajar con Git

Cada carpeta de proyecto puede ser un repositorio (carpeta = proyecto). Heliora guarda los diagramas en
**JSON canónico**: claves en orden alfabético, bloques ordenados por identificador, conexiones por sus
extremos y sangría fija de 2 espacios. Guardar dos veces el mismo diagrama da el mismo archivo, byte a byte,
y cambiar un parámetro cambia una sola línea, así que el diff de Git muestra justo lo que cambió.

    cd mi_proyecto
    git init
    git add proyecto.json diagramas scripts mantenimiento.json .gitattributes
    git commit -m "Planta inicial"
    # ... cambia Kp del PI en la aplicación y guarda ...
    git diff diagramas/principal.json      # una línea: "Kp": 2.0 → "Kp": 3.0

Al guardar un proyecto se crea su `.gitattributes` (JSON y scripts como texto con fin de línea LF;
Parquet, PNG y PDF como binarios). Los datos grandes (`datos/*.parquet`) conviene dejarlos fuera del
repositorio o usar Git LFS.

## Pruebas

En la aplicación, módulo Validación. Desde la línea de órdenes:

    cd engine
    ../.venv/bin/python -c "from heliora.validacion import banco; banco.imprimir(banco.ejecutar(4))"

O todas las fases con su resumen (en Windows, `..\.venv\Scripts\python validar_todo.py`):

    ../.venv/bin/python validar_todo.py

Cada simulación, linealización y validación devuelve un bloque `procedencia`:

- `sha256`: huella de la entrada (el diagrama tal como se envió, o el código del banco de pruebas).
- En simulación y linealización, además, `sha256_resuelto` (los parámetros de cada bloque ya evaluados
  con el espacio de trabajo: el mismo diagrama con otra variable da otra huella), `sha256_codigo` (el
  código de los bloques usados y del núcleo de simulación, incluidas las ediciones locales) y la lista
  `archivos_codigo`.
- Las versiones del motor, de Python y de NumPy, SciPy, pandas, scikit-learn, statsmodels, pvlib, joblib
  y pyarrow.

Son huellas, no copias: identifican qué se ejecutó, pero el código y los datos no se guardan con el
resultado. Para reproducirlo hace falta conservar esos archivos (por ejemplo, en Git).

## Linealización

En Bloques, «Linealizar aquí» calcula el modelo lineal (A, B, C, D) del diagrama en su punto de
operación: simula hasta el final, refina con Newton (f = 0) y deriva por diferencias centrales. La
entrada es la primera fuente del diagrama; la salida, el bloque seleccionado o la señal del primer
osciloscopio. Abre su Bode, su respuesta al escalón y sus polos, y deja la variable `lin` en la consola,
que usan las herramientas de Control con «Planta: modelo linealizado».

## Datos de los valores

`MECHANICS.md` explica de dónde sale cada valor y cuáles son estimados.
