# Estado de Heliora (2026-11-25)

## Dónde estamos

- Ventana Rust/egui + motor Python; Windows y macOS.
- Interfaz adaptada al diseño «Sala de control» (`diseno/opcion_a_*.html`), con paleta de comandos
  (Ctrl K), «Ordenar cables», minimapa, «Recientes» y huella sha256 en el inspector. Detalle en
  `historial/2026-11-25.md`. Sin commit.
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

## Verificado (2026-11-25, Windows)

- Rediseño: `cargo test` 18/18, clippy limpio, capturas de Bloques en claro y oscuro.
- Paleta de comandos, minimapa y exportación CSV sin prueba manual en la ventana.

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
