# Estado de Heliora (2026-11-26)

## Dónde estamos

- Ventana Rust/egui + motor Python; Windows y macOS.
- Interfaz con el rediseño de `diseno/rediseno/` en las 11 pantallas (rejilla de 12 columnas, herramientas
  en pestañas, fichas, parámetros a la derecha). Diferencias con el diseño y su motivo en
  `historial/2026-11-25.md`.
- Paneles con separadores ajustables que se recuerdan entre sesiones.
- KPIs de planta con el paso de cada inversor, cobertura y resultados parciales; la validación guardada se
  marca como histórica si cambió el banco o el motor.
- Instaladores con Python incluido (Windows `.exe`, macOS `.dmg`) generados por la CI; sin firma digital.
- Auditoría de todos los módulos resuelta en lo grave: importar tablas desde archivo, evaluador seguro,
  guardado sin pérdidas, mantenimiento editable, aplicar modelo, CEC. Detalle en `historial/2026-11-25.md`.
- Interfaz adaptada al diseño «Sala de control» (`diseno/opcion_a_*.html`), con paleta de comandos
  (Ctrl K), «Ordenar cables», minimapa, «Recientes» y huella sha256 en el inspector. Detalle en
  `historial/2026-11-25.md`.
- Encargo de modelos físicos: Fases 1-4 construidas; Fase 5 solo como diseño (`docs/diseno_motor_dae.md`),
  **pendiente de revisión humana**.
- Ronda de correcciones de rigor (2026-11-19) cerrada: G1-G3, I1-I7 y menores. Detalle en
  `historial/2026-11-19.md`.
- Invariante aplicado: la red no lineal, el bucle de conmutadores y el paso fijo fallan con error
  diagnóstico en lugar de devolver resultados agotados o no finitos; la linealización rechaza el tiempo
  discreto; los NaN/inf llegan como null con sus rutas en `no_finitos`.
- El guardado de proyecto solo dice «guardado» cuando todas las escrituras (también la del motor) terminan
  bien; escrituras atómicas.
- La escritura al motor ocurre en un hilo con cola acotada; una `simular` reenviada tras un reinicio se
  rechaza si dependía del espacio de trabajo.

## Verificado (2026-11-26, Windows)

- Paneles ajustables y mejoras de KPIs, muestra SCADA y validación histórica: clippy limpio,
  `cargo test` 53/53, `validar_todo.py` 133/133, capturas revisadas. Detalle en `historial/2026-11-26.md`.

## Verificado (2026-11-25, Windows)

- Rediseño: clippy limpio, `cargo test` 50/50, `validar_todo.py` 129/129, capturas comparadas con el
  diseño. Sin probar a mano la interacción ni 1280 × 800.

- Tras auditoría, arreglos en paralelo y revisión independiente: `cargo test` 36/36, clippy limpio,
  `validar_todo.py` 129/129. Motor autocontenido montado y probado en local.
- Flujo «Instaladores» en GitHub correcto (instalador de Windows 137 MB, `.dmg` 220 MB). Sin probar: el
  `.dmg` en un Mac real y el instalador en un Windows limpio.

## Verificado (2026-11-19, Windows)

- `engine/validar_todo.py`: 93/93 pruebas.
- `cargo test`: 12/12 (18/18 tras el rediseño del 2026-11-25). `RUSTFLAGS="-D warnings" cargo build` y `cargo clippy -- -D warnings` limpios.
- Los 9 ejemplos de `engine/ejemplos/` simulan idénticos bit a bit a sus referencias (motor del commit
  `cb4ccbd` para los 5 originales; funciones previas a la ronda para los 4 nuevos).

## No verificado

- CI de GitHub, incluida la entrada nueva `ubuntu-latest`.
- I7 de punta a punta con un motor real que deja de leer (solo la cola, en prueba unitaria).
- Citas bibliográficas de las Fases 1-4: edición y apartado sin comprobar (MECHANICS.md).
- La ventana no se ha usado a mano tras el rediseño (solo capturas automáticas).

## Limitaciones conocidas

- La linealización no rechaza bloques con estado retenido que no son `BloqueDiscreto` (`memoria`, `mppt`,
  `retardo`): se linealizan con ese valor fijo.
- `cargo clippy --all-targets` falla por un aviso previo de `bloques.rs` (funciones tras el módulo de pruebas).
- Dos inercias o condensadores unidos rígidamente no se admiten (motivo del diseño DAE).

## Siguiente paso

- Revisión humana de `docs/diseno_motor_dae.md`.
- Decidir si se amplía G2 a `memoria`, `mppt` y `retardo`.
