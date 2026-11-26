# Desarrollo de Heliora

Guía para compilar, probar y modificar Heliora. La presentación del proyecto está en el
[README](../README.md).

## Entorno local

    cargo run

La aplicación busca Python junto al motor: primero el Python incluido en los instaladores
(`python/`), después `.venv/`. Si no hay ninguno, ofrece un botón «Instalar el motor».
También se puede preparar a mano:

    python3 -m venv .venv
    .venv/bin/pip install -r engine/requirements.txt

En Windows (PowerShell), con Python 3.11 o 3.12 de python.org y Rust con la cadena MSVC
(el instalador `rustup-init` ofrece instalar Visual Studio Build Tools si faltan):

    py -3 -m venv .venv
    .venv\Scripts\python -m pip install -r engine\requirements.txt
    cargo run

En Windows la aplicación busca `.venv\Scripts\python.exe` y, si no está, `python`. La instalación
desde el botón usa el lanzador `py`.

| Variable | Uso |
|---|---|
| `HELIORA_PYTHON` | Ruta del intérprete de Python, si no está en `python/` ni en `.venv/` |
| `HELIORA_ENGINE` | Ruta de la carpeta `engine` |

Para montar localmente el motor autocontenido de los instaladores:

    bash packaging/preparar_motor.sh dist/Heliora x86_64-pc-windows-msvc   # o aarch64-apple-darwin

## Dónde está cada cosa

| Tema | Archivo |
|---|---|
| Ventana: aplicación, barra superior, Inicio | `src/app.rs` |
| Componentes de la interfaz (rejilla, tarjetas, fichas, separadores) | `src/ui.rs`, `src/tema.rs` |
| Editor de bloques | `src/bloques.rs` |
| Páginas de análisis y gráficas | `src/herramientas.rs`, `src/grafica.rs` |
| Planificador de mantenimiento | `src/plan.rs` |
| Comunicación con el motor | `src/motor.rs` |
| Protocolo del motor (órdenes JSON) | `engine/heliora/servidor.py` |
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
| Estadística | `engine/heliora/lib/estadistica.py` |
| Aprendizaje automático | `engine/heliora/lib/aprendizaje.py` |
| Mantenimiento (SCADA, KPIs, fallas, fiabilidad) | `engine/heliora/lib/mantenimiento.py` |
| Formularios de análisis | `engine/heliora/herramientas.py` |
| Validación de entradas de los formularios | `engine/heliora/entradas.py` |
| Lectura de CSV, TSV y Parquet | `engine/heliora/lectura.py` |
| Guardado del espacio de trabajo | `engine/heliora/almacen.py` |
| Pruebas por fases | `engine/heliora/validacion/` |
| Diagramas de ejemplo en JSON | `engine/ejemplos/` |
| Instaladores (Windows y macOS) | `packaging/`, `.github/workflows/instaladores.yml` |
| Diseño propuesto del motor DAE (sin construir) | `docs/diseno_motor_dae.md` |
| Origen de cada valor y cuáles son estimados | `MECHANICS.md` |

Cada bloque es una clase corta con sus ecuaciones en su documentación. Desde la aplicación se abre con
«Ver y editar código»; al guardar, el motor vuelve a cargar los bloques.

## Pruebas

Ventana (Rust):

    cargo clippy -- -D warnings
    cargo test

Motor (Python), todas las fases con su resumen (en Windows, `..\.venv\Scripts\python validar_todo.py`):

    cd engine
    ../.venv/bin/python validar_todo.py

Una sola fase:

    ../.venv/bin/python -c "from heliora.validacion import banco; banco.imprimir(banco.ejecutar(4))"

Las pruebas nuevas se registran con el decorador `@prueba(fase, nombre, referencia)` en un archivo
`engine/heliora/validacion/pruebas_*.py` listado en `pruebas.py`. Cada medida declara su referencia y
su tolerancia.

La validación guardada lleva la huella del banco (`sha256`) y la del código del motor
(`sha256_motor`). Si alguna ya no coincide con la del código actual, la aplicación la marca como
histórica y ofrece volver a validar.

## Procedencia de los resultados

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

## Proyectos y Git

Un proyecto es una carpeta: `proyecto.json`, `diagramas/*.json`, `datos/*.csv` o `*.parquet`,
`scripts/*.py` y `mantenimiento.json`. Cada carpeta de proyecto puede ser un repositorio.

Heliora guarda los diagramas en **JSON canónico**: claves en orden alfabético, bloques ordenados por
identificador, conexiones por sus extremos y sangría fija de 2 espacios. Guardar dos veces el mismo
diagrama da el mismo archivo, byte a byte, y cambiar un parámetro cambia una sola línea, así que el diff
de Git muestra justo lo que cambió.

    cd mi_proyecto
    git init
    git add proyecto.json diagramas scripts mantenimiento.json .gitattributes
    git commit -m "Planta inicial"
    # ... cambia Kp del PI en la aplicación y guarda ...
    git diff diagramas/principal.json      # una línea: "Kp": 2.0 → "Kp": 3.0

Al guardar un proyecto se crea su `.gitattributes` (JSON y scripts como texto con fin de línea LF;
Parquet, PNG y PDF como binarios). Los datos grandes (`datos/*.parquet`) conviene dejarlos fuera del
repositorio o usar Git LFS.

## Integración continua

| Flujo | Archivo | Qué hace |
|---|---|---|
| Comprobación | `.github/workflows/ci.yml` | En Windows y macOS: clippy, `cargo test` y el banco del motor. En Linux, solo el motor (la ventana necesita X11 o Wayland). |
| Instaladores | `.github/workflows/instaladores.yml` | Instalador de Windows (Inno Setup) y `.dmg` de macOS con Python 3.12 incluido. En cada push a `main` quedan como artefactos; con una etiqueta `v*`, en una Release. |

Los instaladores no llevan firma digital. Para quitar los avisos de SmartScreen y Gatekeeper hace
falta un certificado (Apple Developer Program para macOS; firma de código o Azure Trusted Signing para
Windows) guardado como secreto del repositorio; el flujo admite añadir ese paso.
