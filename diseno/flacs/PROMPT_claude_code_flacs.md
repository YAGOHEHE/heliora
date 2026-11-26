# Encargo: aplicar el estilo FLACS-CFD a Heliora (Rust + egui)

## Contexto

Heliora es una aplicación de escritorio en Rust con egui (`src/`) y un motor en Python (`engine/`). Este encargo **solo toca la ventana** (`src/`) y el icono. No cambies el motor, el protocolo JSON por líneas, los nombres de variables y comandos ni el formato de los proyectos.

El diseño de referencia está en `diseno/flacs/`: un `.html` y un `.png` (1440 × 900 a 2×) por pantalla, con los mismos nombres que `diseno/rediseno/`:

- `inicio`, `bloques`, `calculo`, `potencia`, `solar`, `estadistica`, `aprendizaje`
- `mantenimiento_analisis`, `mantenimiento_tablero`, `mantenimiento_ordenes`, `validacion`

El icono nuevo está en `diseno/flacs/icono/`.

Este diseño sustituye al **aspecto** de `diseno/rediseno/` y mantiene su **contenido**: mismos datos, herramientas, tablas, gráficas y reglas de `diseno/PROMPT_claude_code.md` (no inventar datos, estados vacíos con su acción, 1280 × 800 mínimo, temas claro y oscuro). Si ese encargo anterior está a medias, termina las pantallas directamente con este estilo.

Antes de empezar, lee `README.md`, `historial/ESTADO.md`, `src/tema.rs` y `src/ui.rs`. Después, abre cada `.png` y lee el `.html` de la pantalla que vayas a hacer: medidas, colores y separaciones exactas están en sus estilos en línea.

## Qué cambia (principios)

1. **Dos tipos de ventana.**
   - **Inicio (portada):** barra superior blanca de 40 px con el icono y «Heliora»; barra lateral negra de 140 px con Inicio, Nuevo, Abrir y Referencia arriba, y Ajustes y Cuenta abajo (icono encima del texto, blanco; el activo con una barra blanca de 3 px a la izquierda).
   - **Resto (espacio de trabajo):** de arriba abajo:
     - barra superior gris (`#F2F2F2`, 44 px): icono, «Heliora», guardar, deshacer, rehacer; buscador centrado «Buscar bloque, comando o variable · Ctrl K»; píldora del solver con «Simular F5» negro; «Motor listo» y «Ayuda»;
     - menú de módulos (Archivo, Inicio, Bloques, Potencia, Solar, Cálculo, Estadística, Aprendizaje, Mantenimiento, Validación); el activo en negrita con subrayado negro de 2 px;
     - **cinta (ribbon)** blanca de ~94 px con grupos separados por líneas verticales y el nombre del grupo debajo, en gris. Las herramientas de cada módulo son botones grandes (icono de 24 px sobre el texto); la activa lleva fondo `#E8E8E8` y borde `#BDBDBD`. Las acciones menores son columnas de botones pequeños (icono de 14 px + texto). **Esto sustituye a la barra de pestañas de herramientas.**
     - cuerpo: panel izquierdo «Esquema del proyecto | Opciones» (280 px), pestañas de documento, contenido y panel derecho de parámetros (360 px);
     - **barra de estado negra** de 28 px: ruta del proyecto a la izquierda; «Sin selección», «Unidades SI», versiones de Python y librerías y «Tema: Claro» a la derecha.
2. **Esquema del proyecto (árbol).** Nodos con el conmutador redondo negro (⊕ / ⊖), un punto de estado de 10 px y el texto: verde `#3E9B4F` = listo o sin avisos, rojo `#E5675C` = pendiente o con aviso, círculo hueco gris = sin ejecutar. El sufijo en gris (11 px) da el recuento. El elemento abierto lleva fondo `#E8E8E8`. En Bloques, el panel izquierdo muestra la pestaña «Librería».
3. **Panel de parámetros** (como el «Scenario Manager»): título «Parámetros» de 20 px en negrita con «Opciones» y engranaje a la derecha; nombre y descripción de la herramienta; filas en rejilla de tres columnas (etiqueta | campo de 128 px con el valor en monoespaciada alineado a la derecha | unidad); desplegables a todo el ancho; «Guardar en el espacio de trabajo» con casilla y nombre de variable. Pie fijo con borde superior: «Exportar informe» (contorno) y «Calcular · Ctrl Intro» (negro). En Tablero, Órdenes y Validación ese panel muestra el calendario y la agenda, el detalle de la orden o el detalle de la prueba.
4. **Fichas de resultados:** 6 por fila; borde de 1 px `#BDBDBD`, sin radio; etiqueta de 12 px en gris; valor en monoespaciada de 20 px; unidad y nota en gris. La ficha seleccionada (filtros de Órdenes y fases de Validación) lleva borde negro de 2 px y fondo `#F2F2F2`.
5. **Tarjetas:** borde `#BDBDBD`, sin sombra ni radio; cabecera de 44 px con el título de 15 px en peso medio, la leyenda y las acciones a la derecha (botones pequeños con contorno: «Encuadrar», «PNG», «SVG»).
6. **Etiquetas (chips):** mayúsculas de 11 px con espaciado 0.04 em, 22 px de alto, sin radio; gris `#D9D9D9` o gris oscuro `#555555` con texto blanco cuando están activas o identifican un equipo.
7. **Avisos:** barra negra con texto blanco y botón blanco «Ocultar» (por ejemplo, el sobreajuste de Aprendizaje).

## Paleta

La interfaz es **monocroma**. El color solo aparece en los puntos de estado y en las gráficas.

| Uso | Hex |
|---|---|
| Negro (barra lateral, barra de estado, botón principal, texto fuerte) | `#000000` |
| Texto | `#1A1A1A` |
| Texto secundario | `#5C5C5C` |
| Fondo de ventana, tarjetas y paneles | `#FFFFFF` |
| Barra superior, menú, cabeceras de tabla, zonas grises de tarjeta | `#F2F2F2` |
| Selección o elemento activo | `#E8E8E8` |
| Borde de tarjetas y campos | `#BDBDBD` |
| Separadores | `#E0E0E0` |
| Chip / marcador de posición | `#D9D9D9` · activo `#555555` |
| Correcto | `#3E9B4F` |
| Pendiente o aviso | `#E5675C` |

**Gráficas, mapas de calor y plan de mantenimiento: en color.** Series en este orden: `#C97A06`, `#1C7FB0`, `#7C5CD6`, `#0F8C7E`, `#4B5563`. Rejilla `#EEEEEE`; ejes negros de 1 px; etiquetas de ejes en monoespaciada de 10 px y gris.

- **Tipos de orden:** Limpieza `#C97A06` (cuadrado), Inspección `#1C7FB0` (cuadrado), Ensayo `#7C5CD6` (cuadrado con contorno), Mantenimiento anual `#0F8C7E` (rombo). Mantén las formas además del color.
- **Matriz de confusión:** diagonal en azul `#1C7FB0` con texto blanco, resto en ámbar `#C97A06` con opacidad según el valor.
- **Mapa P(G, T):** ámbar con opacidad según la potencia.
- **Diagrama de Bloques:** cables eléctricos negros sólidos de 2 px, señales grises discontinuas; bloques blancos con borde negro de 1 px; el seleccionado con borde de 2 px y halo gris.

**Tema oscuro:** misma estructura invirtiendo los neutros (fondo `#000000`, paneles `#121212`, superficies `#1E1E1E`, bordes `#3A3A3A`, texto `#EDEDED`, secundario `#A3A3A3`, botón principal blanco con texto negro). La barra lateral y la de estado siguen negras, separadas por un borde `#3A3A3A`. Las series de las gráficas pasan a `#F2A93B`, `#4DB5D9`, `#A48BF0` y `#3CC2B1`. Comprueba el contraste: 4.5:1 en texto y 3:1 en trazos.

## Tipografía y forma

- **Roboto** (400, 500 y 700) para la interfaz y **Roboto Mono** para números, código y valores. Añade los `.ttf` a `fuentes/` (licencia Apache 2.0) y cárgalos en egui en lugar de IBM Plex.
- **Tamaños:** título de pantalla 22 px en negrita (Inicio 28 px); títulos de panel 20 px en negrita; texto 13 px; etiquetas 12 px.
- **Radio 0 en todo:** tarjetas, botones, campos y chips. Sin sombras.
- **Botones:** principal negro con texto blanco; secundario blanco con borde negro de 1 px; 34 px de alto (28 px los pequeños).
- **Iconos de trazo de 1.6 px**, sin relleno: los del `.html`, que son SVG en línea.

## Componentes que conviene crear (en `tema.rs` o en un módulo `ui/`)

- `barra_superior`, `menu_modulos(activo)`, `cinta(grupos)` (con grupo, botón grande y columna de botones pequeños) y `barra_estado(ruta)`.
- `barra_lateral_portada(activo)` para Inicio.
- `arbol_proyecto(ui, activo)` con nodo, conmutador y punto de estado.
- `panel_parametros(titulo, herramienta, descripcion, contenido, pie)`, `fila_parametro`, `desplegable` y `fila_guardar`.
- `ficha_kpi(etiqueta, valor, unidad, nota, activa)`, `tarjeta(titulo, leyenda, acciones, contenido)`, `chip(texto, oscuro)` y `leyenda(color, texto, estilo)`.
- `pestanas_documento(...)` y `aviso_negro(texto)`.

Reutiliza lo que ya exista (`tarjeta`, `marco_panel`, `boton_principal`, `estado`, `icono`, `trazo_cable`) adaptándolo. No dupliques.

## Pantalla por pantalla

Sigue el `.png` y el `.html` de `diseno/flacs/`. Resumen de la distribución:

- **Inicio:** cabecera con «Nuevo proyecto» (negro), «Abrir proyecto» y «Crear carpeta». Tres tarjetas del mismo tamaño con la parte inferior gris: Continuar (vista previa del diagrama, «ejemplo.py» y «Abrir»), Planta · datos SCADA (6 KPI en 3 × 2) y Validación (4 fases y «Ejecutar»). Debajo, «Próximas órdenes» como lista de filas con borde y chips, con los filtros en chips; a la derecha, Módulos en una rejilla de 4 × 2.
- **Bloques:**
  - cinta con los grupos Diagrama, Edición, Simulación y Resultados;
  - panel izquierdo con la Librería;
  - lienzo punteado y, debajo, una franja de 190 px con los tres osciloscopios uno al lado del otro;
  - panel derecho con el bloque seleccionado, el solver y la última simulación, y la pestaña Diagnóstico | Consola abajo.
- **Cálculo:** cinta con los grupos Script, Ejecutar, Espacio de trabajo y Figuras. Rejilla de 1.55 : 1: a la izquierda, el script arriba y la consola abajo (en proporción 1.5 : 1); a la derecha, el espacio de trabajo arriba y las figuras en pestañas abajo.
- **Potencia, Solar, Estadística, Aprendizaje y Mantenimiento · Análisis:** herramientas en la cinta más el grupo Resultados; título; 6 fichas; tarjeta principal con la gráfica (y la tabla secundaria debajo, a su altura natural); parámetros a la derecha. Detalles:
  - Potencia: Bode con franja rayada en la zona recomendada y las líneas f red, f res y f sw.
  - Solar: el mapa P(G, T) va en el panel de parámetros.
  - Aprendizaje: el aviso negro va sobre la matriz.
- **Mantenimiento · Tablero y Órdenes:** cinta con Herramientas, Plan (con «Plan ▾» y «sin archivo») e Informe.
  - Tablero: el plan por equipo con la línea de hoy; a la derecha, el calendario (hoy en negro) y la agenda.
  - Órdenes: las fichas hacen de filtro; tabla agrupada por semana con paginación; a la derecha, el detalle de la orden con «Reprogramar» y «Cerrar orden».
- **Validación:** cinta con los grupos Ejecutar, Filtro y Banco. Debajo, 4 fichas de fase seleccionables y la ficha «Última ejecución»; tabla de pruebas con chips de categoría y «Solo las que fallan»; a la derecha, el detalle de la prueba con la gráfica de error y «Ejecutar esta».

## Icono

Sustituye el icono por el de `diseno/flacs/icono/`:

- `heliora.ico` → `icon/heliora.ico` y `diseno/icono/heliora.ico`;
- `heliora_icono_256.png` → `icon/icon.png`;
- `heliora_icono_512.png` → `icon/icon_512.png`;
- los SVG y PNG → `diseno/icono/`, sobrescribiendo los de igual nombre.

Revisa `build.rs` y `packaging/` (Windows y macOS) por si generan o copian el icono desde otra ruta. En la barra superior de la app usa la versión pequeña (`heliora_icono_pequeno.svg`) a unos 22 px.

## Reglas

- **No inventes datos.** Todo valor sale del motor o del proyecto. Las curvas, nubes de puntos y valores de los `.html` son de ejemplo.
- **Tamaño mínimo:** 1280 × 800. Por debajo de unos 1100 px, el panel derecho pasa debajo y el esquema del proyecto se pliega con ««».
- **Mantén los dos temas** y el cambio desde Ajustes.
- **Trabaja por fases, con commits pequeños:**
  1. tema, fuentes, icono y componentes;
  2. barra superior, menú, cinta, esquema y barra de estado;
  3. Inicio;
  4. el patrón de herramienta (Potencia, Solar, Estadística, Aprendizaje y Mantenimiento · Análisis);
  5. Mantenimiento (Tablero y Órdenes);
  6. Validación;
  7. Bloques y Cálculo.

  Al cerrar cada fase, `cargo build`, `cargo clippy -- -D warnings` y `cargo test` tienen que terminar limpios.
- **Comparación:** al terminar cada pantalla, saca una captura a 1440 × 900 en `docs/capturas/` con el mismo nombre que la de `diseno/flacs/` y compárala con la referencia. Corrige diferencias de proporción, alineación y espaciado.
- **Registro:** apunta en `historial/` (en el archivo con la fecha de hoy) qué cambió y qué queda pendiente, y actualiza `historial/ESTADO.md`.

## Al terminar

Enséñame una tabla con cada pantalla, su estado (hecha, parcial o pendiente) y las diferencias que queden respecto al diseño, con su motivo (por ejemplo, algo que egui no permite de forma sencilla).
