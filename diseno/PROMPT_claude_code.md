# Encargo: aplicar el rediseño de interfaz a Heliora (Rust + egui)

## Contexto

Heliora es una aplicación de escritorio en Rust con egui (`src/`) y un motor en Python (`engine/`). Este encargo **solo toca la ventana** (`src/`). No cambies el motor, el protocolo JSON por líneas ni el formato de los proyectos.

El diseño de referencia está en `diseno/rediseno/`: un `.html` y un `.png` por pantalla.

- `inicio`, `bloques`, `calculo`, `potencia`, `solar`, `estadistica`, `aprendizaje`
- `mantenimiento_analisis`, `mantenimiento_tablero`, `mantenimiento_ordenes`, `validacion`

Antes de empezar, abre cada `.png` y léete el `.html` de la pantalla que vayas a implementar: los colores, los tamaños y las separaciones exactas están en sus estilos en línea. Las capturas de la versión actual están en `docs/capturas/`.

Lee también `README.md`, `historial/ESTADO.md` y `src/tema.rs` antes de tocar nada.

## Qué cambia (principios)

1. **Rejilla de 12 columnas para todas las pantallas.** Margen de 20 px, separación de 16 px entre tarjetas.
   - **Fila superior:** 6 fichas de resultados del mismo tamaño (2 columnas cada una).
   - **Zona principal:** contenido en 8 columnas a la izquierda y panel en 4 columnas a la derecha.
   - **Ocupación:** las filas se estiran hasta llenar la altura disponible y no queda espacio vacío al fondo.
2. **Herramientas como pestañas.** En Potencia, Solar, Estadística, Aprendizaje y Mantenimiento, el selector de herramientas pasa a ser una barra de pestañas bajo la cabecera, como ya ocurre en Mantenimiento. El formulario sale del menú lateral.
3. **Parámetros siempre a la derecha (4 columnas).** Las etiquetas van a la izquierda, el valor alineado a la derecha y en monoespaciada, y la unidad en una columna propia. «Calcular» queda fijo abajo de la tarjeta, junto a «Exportar informe».
4. **Resultados numéricos como fichas, no como filas a todo el ancho.** Cada ficha tiene la etiqueta arriba, el valor grande en monoespaciada (unos 22 px) con la unidad en gris y una nota opcional.
5. **Gráficas grandes.** Ocupan la tarjeta de 8 columnas. Las tablas secundarias van debajo, con su altura natural.
6. **Cabecera común.** Contiene, en este orden:
   - el logo y la ruta (proyecto / módulo / herramienta);
   - el buscador «Buscar bloque, comando o variable» con Ctrl K;
   - la píldora del solver con «Simular F5»;
   - el estado del motor.

   El riel de iconos de la izquierda va agrupado con separadores: Inicio | Bloques, Potencia, Solar | Cálculo, Estadística, Aprendizaje | Mantenimiento, Validación. Referencia y Ajustes van abajo. La barra de estado lleva las versiones de Python y las librerías, y el tema.

## Paleta (tema claro, el principal del rediseño)

Añade o ajusta estos valores en `src/tema.rs` (`CLARO`). Usa los nombres de campo que ya existen y añade solo los que falten.

| Uso | Hex |
|---|---|
| Fondo de la ventana | `#F3F4F6` |
| Tarjetas y paneles | `#FFFFFF` |
| Fondo secundario (bloques de valor, filas alternas) | `#F6F7F9` / `#FBFBFC` |
| Borde | `#E3E6EA` (campos `#D5D9DF`) |
| Texto | `#1B2028` |
| Texto secundario | `#5F6875` |
| Acento (botón principal, cables eléctricos, pestaña activa) | `#C97A06`, texto sobre él `#1A1205` |
| Fondo de selección / elemento activo | `#FDF1DC`, texto `#7A4A00` |
| Señal / serie 2 | `#1C7FB0` |
| Serie 3 | `#7C5CD6` |
| Serie 4 | `#0F8C7E` |
| Correcto | `#1E9E63` |

Tipos de orden en Mantenimiento: Limpieza `#C97A06`, Inspección `#1C7FB0`, Ensayo `#7C5CD6`, Mantenimiento anual `#0F8C7E`.

El tema oscuro sigue la misma estructura con estos valores:

| Uso | Hex |
|---|---|
| Fondo | `#0B0D10` |
| Paneles | `#0E1114` |
| Bloques del lienzo | `#14181D` |
| Borde | `#1E232A` |
| Texto | `#E4E7EC` |
| Texto secundario | `#98A0AC` |
| Acento | `#F2A93B` |
| Señal | `#4DB5D9` |

Si alguno de estos colores choca con algo que ya existe, mantén el contraste mínimo: 4.5:1 para texto y 3:1 para trazos.

Tipografía: IBM Plex Sans para la interfaz e IBM Plex Mono para números, código y valores. Ya están en `fuentes/`.

Radios: tarjetas 8 px, botones y campos 6 px, fichas pequeñas 4 px.

## Componentes que conviene crear en `tema.rs` (o en un módulo `ui/` nuevo)

- `rejilla(ui, cols=12, gap=16)`: reparte el ancho en columnas para colocar tarjetas por número de columnas (2, 4, 8, 12).
- `tarjeta_con_titulo(ui, titulo_en_mayusculas, acciones_derecha, contenido)`.
- `ficha_kpi(ui, etiqueta, valor, unidad, nota)`.
- `fila_parametro(ui, etiqueta, &mut valor, unidad)`: rejilla de tres columnas.
- `barra_pestanas(ui, pestañas, &mut activa, acciones_derecha)`.
- `chip(ui, texto, color_opcional)` y `leyenda(ui, color, texto, discontinua)`.

Reutiliza las funciones que ya existen (`tarjeta`, `marco_panel`, `boton_principal`, `estado`, `icono`, `trazo_cable`) cuando encajen. No dupliques.

## Pantalla por pantalla

En cada pantalla, sigue su `.png` y su `.html` de `diseno/rediseno/`.

**Inicio.** Tres tarjetas iguales arriba:
- Continuar: vista previa del diagrama abierto y botón «Abrir».
- Planta: 6 KPIs SCADA en una rejilla de 2 × 3.
- Validación: estado de las 4 fases.

Debajo, en 8 columnas, las próximas órdenes en una tabla, con los contadores en la cabecera. En 4 columnas, los módulos en una rejilla de 4 × 2. Usa datos reales del proyecto. Si no hay datos SCADA o plan, muestra un estado vacío con la acción para crearlos.

**Bloques.** La tarjeta de 8 columnas contiene la librería (unos 200 px) y el lienzo. A la derecha, en 4 columnas, van tres tarjetas apiladas: bloque seleccionado, solver (resumen con «Editar») y última simulación. Abajo, en 8 columnas, los osciloscopios uno al lado del otro (no en pestañas). En 4 columnas, el diagnóstico y la consola en pestañas. Los diagramas se abren encuadrados.

**Cálculo.** Arriba, el script en 8 columnas y el espacio de trabajo en 4. Debajo, la consola en 8 columnas y las figuras en pestañas en 4. Las alturas van en proporción 1.5 : 1.

**Potencia (Filtro LCL), Solar, Estadística (Regresión lineal), Aprendizaje (Clasificación), Mantenimiento · Análisis.** Siguen el patrón de herramienta: pestañas, 6 fichas, gráfica o tabla principal en 8 columnas y parámetros en 4.
- Potencia: marca f_red, f_res y f_sw en el Bode y sombrea la zona 10·f_red–f_sw/2. La sexta ficha es esa comprobación.
- Solar: I-V y P-V juntas en una tarjeta; el mapa P(G, T) (mapa de calor, no superficie 3D) va bajo los parámetros.
- Aprendizaje: matriz de confusión como mapa de calor (diagonal en azul, resto en ámbar) y aviso de sobreajuste encima.
- El resto de herramientas de cada módulo usa la misma plantilla.

**Mantenimiento · Tablero.** 6 fichas: Vencidas, Esta semana, Próximos 30 días, Pendientes, Cerradas y Cumplimiento. En 8 columnas, el «Plan por equipo»:
- una fila por equipo y una columna por semana (12 semanas);
- marcas de color por tipo de orden;
- una línea en el día de hoy;
- la columna con el número de órdenes de cada equipo;
- los controles de generar órdenes en la cabecera.

En 4 columnas, un calendario pequeño del mes y, debajo, la agenda agrupada por día. Las acciones del plan (abrir, guardar, guardar como, plan de ejemplo, nuevo) se agrupan en un menú «Plan ▾».

**Mantenimiento · Órdenes.** Las 6 fichas hacen de filtro: Pendientes, Vencidas, Cerradas, Esta semana, Próximos 30 días y Todas. En 8 columnas va una tabla con las columnas Fecha, OT, Tarea, Equipo, Tipo y Estado. La tabla se agrupa por semana, con paginación, y los filtros de equipo y fechas y «Nueva orden» van en su cabecera. En 4 columnas va el detalle de la orden seleccionada:
- los datos de la orden;
- las notas de cierre;
- los enlaces al equipo, al AMEF y al bloque del diagrama;
- los botones «Cerrar orden» y «Reprogramar».

**Validación.** 4 fases como fichas que se pueden elegir, más una ficha de última ejecución con «Ejecutar todas». En 8 columnas va la tabla de pruebas de la fase elegida, con filtro por categoría y «Solo las que fallan». En 4 columnas va el detalle de la prueba: esperado frente a medido, la gráfica de error y «Ejecutar esta».

## Reglas

- **No inventes datos.** Todo valor que se muestre sale del motor o del proyecto. Si falta, enseña un estado vacío con la acción que lo genera. Las curvas y nubes de puntos de los `.html` son de ejemplo.
- **No cambies nombres de variables, de comandos al motor ni de archivos de proyecto.**
- **Tamaño mínimo.** La ventana debe seguir siendo usable a 1280 × 800. Por debajo de unos 1100 px de ancho, la columna de 4 puede pasar debajo de la de 8.
- **Los dos temas.** Mantén el modo oscuro y el claro, y el cambio desde Ajustes.
- **Por fases, con commits pequeños:**
  1. tema y componentes;
  2. cabecera, riel y barra de estado;
  3. Inicio;
  4. Mantenimiento (Tablero, Órdenes, Análisis);
  5. Validación;
  6. el patrón de herramienta (Potencia, Solar, Estadística, Aprendizaje);
  7. Bloques y Cálculo.

  Al cerrar cada fase, comprueba que `cargo build` y `cargo clippy -- -D warnings` siguen limpios y que `cargo test` pasa.
- **Comparación.** Al terminar cada pantalla, saca una captura a 1440 × 900 en `docs/capturas/` con el mismo nombre que la de `diseno/rediseno/` y compárala con la referencia. Corrige diferencias de proporción, alineación y espaciado.
- **Registro.** Apunta en `historial/` (archivo con la fecha de hoy) qué se cambió y qué queda pendiente, y actualiza `historial/ESTADO.md`.

## Al terminar

Enséñame una tabla con cada pantalla, su estado (hecha, parcial o pendiente) y las diferencias que queden respecto al diseño, con su motivo. Por ejemplo, algo que egui no permite de forma sencilla.
