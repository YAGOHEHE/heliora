//! Componentes del estilo FLACS: rejilla de 12 columnas, cinta, esquema del proyecto, tarjetas,
//! fichas, panel de parámetros, pestañas, chips, leyendas y avisos. Leen los colores de `tema::p()`.
use crate::iconos::{self, Icono};
use crate::tema;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};

/// Columnas de la rejilla.
pub const COLUMNAS: usize = 12;
/// Separación entre tarjetas.
pub const HUECO: f32 = 16.0;
/// Margen exterior de cada pantalla.
pub const MARGEN: f32 = 20.0;
/// Por debajo de este ancho, una fila de 8 + 4 columnas se apila (la de 4 pasa debajo).
pub const ANCHO_APILAR: f32 = 1100.0;

/// Reparte `ancho` en celdas que ocupan `spans` columnas de una rejilla de `cols`, con `hueco` entre ellas.
///
/// Returns:
///     Posición x relativa y ancho de cada celda.
pub fn celdas(ancho: f32, cols: usize, hueco: f32, spans: &[usize]) -> Vec<(f32, f32)> {
    let col = ((ancho - hueco * (cols as f32 - 1.0)) / cols as f32).max(0.0);
    let mut x = 0.0;
    spans
        .iter()
        .map(|&s| {
            let w = col * s as f32 + hueco * (s.max(1) as f32 - 1.0);
            let c = (x, w);
            x += w + hueco;
            c
        })
        .collect()
}

/// Indica si una fila con estos `spans` debe apilarse en vertical a este ancho.
pub fn apilar(ancho: f32, spans: &[usize]) -> bool {
    ancho < ANCHO_APILAR && spans.len() > 1 && spans.iter().all(|&s| s >= 4)
}

/// Marca la rueda del ratón como usada en este cuadro.
///
/// El lienzo y las gráficas la usan para el zoom; sin esto, la zona desplazable que los contiene
/// (páginas apiladas en ventanas estrechas) también se movería con el mismo giro.
pub fn consumir_rueda(ui: &Ui) {
    ui.ctx().input_mut(|i| {
        i.smooth_scroll_delta = Vec2::ZERO;
        i.raw_scroll_delta = Vec2::ZERO;
    });
}

/// Coloca una fila de celdas de la rejilla dentro del espacio disponible.
///
/// Cada celda recibe su propio `Ui` con el rectángulo exacto (ancho por columnas, alto `alto`).
/// Si la ventana es estrecha y la fila es de columnas anchas, las celdas se apilan con el mismo alto.
///
/// Args:
///     ui: contenedor.
///     spans: columnas que ocupa cada celda (suman como mucho 12).
///     alto: alto de la fila.
///     contenido: se llama con el índice de la celda y su `Ui`.
pub fn fila(ui: &mut Ui, spans: &[usize], alto: f32, mut contenido: impl FnMut(usize, &mut Ui)) {
    let ancho = ui.available_width();
    if apilar(ancho, spans) {
        for i in 0..spans.len() {
            let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::hover());
            let mut hijo = ui.new_child(UiBuilder::new().max_rect(r));
            contenido(i, &mut hijo);
            if i + 1 < spans.len() {
                ui.add_space(HUECO - ui.spacing().item_spacing.y);
            }
        }
        return;
    }
    let (fila, _) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::hover());
    for (i, (x, w)) in celdas(ancho, COLUMNAS, HUECO, spans).into_iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(fila.left() + x, fila.top()), Vec2::new(w, alto));
        let mut hijo = ui.new_child(UiBuilder::new().max_rect(r));
        contenido(i, &mut hijo);
    }
}

/// Dos paneles cuyo ancho se puede ajustar. En ventanas estrechas se apilan.
pub fn fila_ajustable(ui: &mut Ui, clave: impl std::hash::Hash, alto: f32, mut contenido: impl FnMut(usize, &mut Ui)) {
    let ancho = ui.available_width();
    if apilar(ancho, &[8, 4]) {
        fila(ui, &[8, 4], alto, contenido);
        return;
    }
    let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::hover());
    let proporcion = celdas(ancho, COLUMNAS, HUECO, &[8, 4])[0].1 / (ancho - HUECO);
    let paneles = dividir(ui, clave, r, true, HUECO, proporcion, [420.0, 310.0]);
    for (i, panel) in paneles.into_iter().enumerate() {
        let mut hijo = ui.new_child(UiBuilder::new().max_rect(panel));
        hijo.set_clip_rect(panel.intersect(ui.clip_rect()));
        contenido(i, &mut hijo);
    }
}

/// Guarda la proporción de un separador en las preferencias de egui.
pub fn dividir(
    ui: &Ui, clave: impl std::hash::Hash, r: Rect, lado_a_lado: bool, hueco: f32, inicial: f32, minimos: [f32; 2],
) -> [Rect; 2] {
    let id = egui::Id::new(("distribucion_paneles", clave));
    let eje = if lado_a_lado { Vec2::X } else { Vec2::Y };
    let espacio = (r.size().dot(eje) - hueco).max(1.0);
    let escala = (espacio / minimos.iter().sum::<f32>()).min(1.0);
    let minimo = minimos[0] * escala;
    let maximo = (espacio - minimos[1] * escala).max(minimo);
    let limitar = |fraccion: f32| (fraccion * espacio).clamp(minimo, maximo);
    let posicion = ui.data_mut(|d| d.get_persisted::<f32>(id)).filter(|v| v.is_finite()).unwrap_or(inicial);
    let mut primero = limitar(posicion);
    let separador = |primero: f32| {
        let esquina = r.min + eje * primero;
        Rect::from_min_max(esquina, r.max - eje * (espacio - primero))
    };
    let respuesta = ui.interact(separador(primero), id, Sense::click_and_drag())
        .on_hover_cursor(if lado_a_lado { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical })
        .on_hover_text("Arrastra para ajustar · Doble clic para restablecer");
    let inicio_arrastre = id.with("inicio");
    if respuesta.drag_started() {
        ui.data_mut(|d| d.insert_temp(inicio_arrastre, primero));
    }
    if let Some(delta) = respuesta.total_drag_delta() {
        let anterior = ui.data(|d| d.get_temp::<f32>(inicio_arrastre)).unwrap_or(primero);
        primero = limitar((anterior + delta.dot(eje)) / espacio);
        ui.data_mut(|d| d.insert_persisted(id, primero / espacio));
    } else if respuesta.double_clicked() {
        ui.data_mut(|d| d.remove::<f32>(id));
        primero = limitar(inicial);
    }
    let centro = separador(primero).center();
    let pal = tema::p();
    let color = if respuesta.hovered() || respuesta.dragged() { pal.acento } else { pal.texto_3 };
    let transversal = if lado_a_lado { Vec2::Y } else { Vec2::X };
    for indice in [-1.0, 0.0, 1.0] {
        ui.painter().circle_filled(centro + transversal * (4.0 * indice), 1.25, color);
    }
    [
        Rect::from_min_max(r.min, r.max - eje * (r.size().dot(eje) - primero)),
        Rect::from_min_max(r.min + eje * (primero + hueco), r.max),
    ]
}

/// Alto de las filas que llenan la pantalla: reparte `disponible` (menos huecos y filas fijas) según `pesos`.
pub fn altos(disponible: f32, fijos: f32, pesos: &[f32]) -> Vec<f32> {
    const MINIMO: f32 = 120.0;
    let libres = (disponible - fijos - HUECO * pesos.len() as f32).max(0.0);
    let total: f32 = pesos.iter().sum::<f32>().max(f32::EPSILON);
    let mut a: Vec<f32> = pesos.iter().map(|p| libres * p / total).collect();
    // Las filas que no llegan al mínimo lo reciben a costa de las demás, para no pasarse del alto disponible
    let faltan: f32 = a.iter().map(|x| (MINIMO - x).max(0.0)).sum();
    let sobrante: f32 = a.iter().map(|x| (x - MINIMO).max(0.0)).sum();
    if faltan > 0.0 && sobrante > 0.0 {
        let k = (1.0 - faltan / sobrante).max(0.0);
        a = a.into_iter().map(|x| if x < MINIMO { MINIMO } else { MINIMO + (x - MINIMO) * k }).collect();
    } else if faltan > 0.0 {
        a = a.into_iter().map(|x| x.max(MINIMO)).collect();
    }
    a
}

/// Página con el margen de 20 px y el fondo de la ventana.
pub fn pagina<R>(ui: &mut Ui, contenido: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(tema::p().fondo)
        .inner_margin(egui::Margin::same(MARGEN as i8))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = HUECO;
            contenido(ui)
        })
        .inner
}

/// Fondo y borde de una tarjeta sobre el rectángulo completo del `Ui`: borde de 1 px, sin radio.
fn fondo_tarjeta(ui: &Ui, r: Rect) {
    let pal = tema::p();
    ui.painter().rect(r, 0.0, pal.panel, Stroke::new(1.0_f32, pal.borde), StrokeKind::Inside);
}

/// Texto en mayúsculas con espaciado fino (chips y títulos de sección).
pub fn mayusculas(t: &str) -> String {
    let txt: String = t.to_uppercase().chars().flat_map(|c| [c, '\u{200A}']).collect();
    txt.trim_end().to_string()
}

/// Título de sección en mayúsculas de 12 px, peso medio y gris («GUARDAR EN EL ESPACIO DE TRABAJO»).
pub fn titulo_seccion(ui: &mut Ui, t: &str) {
    ui.label(egui::RichText::new(mayusculas(t)).family(tema::semi()).size(12.0).color(tema::p().texto_2));
}

/// Título de pantalla: 22 px en negrita y, a su derecha, el subtítulo en gris de 13 px.
pub fn titulo_pantalla(ui: &mut Ui, titulo: &str, subtitulo: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.label(egui::RichText::new(titulo).font(tema::fuente_negrita(22.0)).color(tema::p().texto));
        if !subtitulo.is_empty() {
            ui.label(egui::RichText::new(subtitulo).size(13.0).color(tema::p().texto_2));
        }
    });
}

/// Tarjeta que ocupa todo el `Ui`: cabecera de 44 px con el título de 15 px en peso medio y, a la
/// derecha, la leyenda y las acciones; debajo, una línea separadora y el contenido.
/// El contenido se recorta también por el área visible del contenedor (p. ej. una zona desplazable).
///
/// Args:
///     ui: celda de la rejilla.
///     titulo: título de la tarjeta; vacío para no mostrar cabecera.
///     acciones: leyenda y botones pequeños alineados a la derecha del título.
///     contenido: cuerpo de la tarjeta, con márgenes de 16 px.
pub fn tarjeta_con_titulo<R>(ui: &mut Ui, titulo: &str, acciones: impl FnOnce(&mut Ui), contenido: impl FnOnce(&mut Ui) -> R) -> R {
    let pal = tema::p();
    let r = ui.max_rect();
    fondo_tarjeta(ui, r);
    let recorte = r.shrink(1.0).intersect(ui.clip_rect());
    let mut arriba = r.top();
    if !titulo.is_empty() {
        let cabecera = Rect::from_min_size(r.min, Vec2::new(r.width(), 44.0));
        let mut c = ui.new_child(UiBuilder::new().max_rect(cabecera.shrink2(Vec2::new(16.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        c.set_clip_rect(recorte);
        c.label(egui::RichText::new(titulo).family(tema::semi()).size(15.0).color(pal.texto));
        c.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            acciones(ui);
        });
        ui.painter().with_clip_rect(recorte).line_segment([Pos2::new(r.left(), cabecera.bottom()), Pos2::new(r.right(), cabecera.bottom())], Stroke::new(1.0_f32, pal.linea));
        arriba = cabecera.bottom();
    }
    let dentro = Rect::from_min_max(Pos2::new(r.left() + 16.0, arriba + 12.0), Pos2::new(r.right() - 16.0, r.bottom() - 10.0));
    let mut hijo = ui.new_child(UiBuilder::new().max_rect(dentro).layout(egui::Layout::top_down(egui::Align::Min)));
    hijo.set_clip_rect(recorte);
    contenido(&mut hijo)
}

/// Tarjeta sin título ni márgenes interiores: el contenido (lienzo, tabla) llega hasta el borde.
pub fn tarjeta_plena<R>(ui: &mut Ui, contenido: impl FnOnce(&mut Ui) -> R) -> R {
    let r = ui.max_rect();
    fondo_tarjeta(ui, r);
    let mut hijo = ui.new_child(UiBuilder::new().max_rect(r.shrink(1.0)));
    hijo.set_clip_rect(r.shrink(1.0).intersect(ui.clip_rect()));
    contenido(&mut hijo)
}

/// Ficha de resultado: etiqueta de 12 px en gris, valor en monoespaciada de 20 px, unidad y nota en gris.
///
/// Ocupa todo el `Ui`. Si `activa` es Some, la ficha se puede pulsar (filtros de Órdenes, fases de
/// Validación); la seleccionada lleva borde negro de 2 px y fondo gris.
pub fn ficha_kpi(ui: &mut Ui, etiqueta: &str, valor: &str, unidad: &str, nota: &str, activa: Option<bool>) -> egui::Response {
    let pal = tema::p();
    let r = ui.max_rect();
    let resp = ui.interact(r, ui.id().with(("ficha", etiqueta, r.min.x as i32, r.min.y as i32)), if activa.is_some() { Sense::click() } else { Sense::hover() });
    let marcada = activa == Some(true);
    let fondo = if marcada || (activa.is_some() && resp.hovered()) { pal.campo } else { pal.panel };
    let borde = if marcada { Stroke::new(2.0_f32, pal.acento) } else { Stroke::new(1.0_f32, pal.borde) };
    ui.painter().rect(r, 0.0, fondo, borde, StrokeKind::Inside);
    let p = ui.painter_at(r.shrink(2.0));
    let x = r.left() + 12.0;
    let ajustada = recortar_a(ui, etiqueta, FontId::proportional(12.0), r.width() - 24.0);
    p.text(Pos2::new(x, r.top() + 11.0), Align2::LEFT_TOP, ajustada, FontId::proportional(12.0), pal.texto_2);
    let g = p.layout_no_wrap(valor.to_string(), FontId::new(20.0, egui::FontFamily::Name("dato".into())), pal.acento);
    let y_valor = r.top() + 32.0;
    let ancho_valor = g.size().x;
    p.galley(Pos2::new(x, y_valor), g, pal.acento);
    if !unidad.is_empty() {
        p.text(Pos2::new(x + ancho_valor + 4.0, y_valor + 21.0), Align2::LEFT_BOTTOM, unidad, tema::fuente_mono(12.0), pal.texto_2);
    }
    if !nota.is_empty() {
        p.text(Pos2::new(x, r.bottom() - 9.0), Align2::LEFT_BOTTOM, nota, FontId::proportional(11.0), pal.texto_2);
    }
    resp.on_hover_text(format!("{etiqueta}: {valor} {unidad}"))
}

/// Recorta `texto` con puntos suspensivos para que quepa en `ancho` con la fuente dada.
fn recortar_a(ui: &Ui, texto: &str, fuente: FontId, ancho: f32) -> String {
    let mide = |t: &str| ui.painter().layout_no_wrap(t.to_string(), fuente.clone(), Color32::WHITE).size().x;
    if mide(texto) <= ancho {
        return texto.to_string();
    }
    let mut letras: Vec<char> = texto.chars().collect();
    while !letras.is_empty() {
        letras.pop();
        let t: String = letras.iter().collect::<String>().trim_end().to_string() + "…";
        if mide(&t) <= ancho {
            return t;
        }
    }
    "…".to_string()
}

/// Fila de parámetro en rejilla de tres columnas: etiqueta | campo de 128 px con el valor en
/// monoespaciada alineado a la derecha | unidad.
pub fn fila_parametro(ui: &mut Ui, etiqueta: &str, valor: &mut String, unidad: &str) -> egui::Response {
    let pal = tema::p();
    let ancho = ui.available_width();
    let col_unidad = 40.0;
    let col_valor = 128.0_f32.min((ancho - col_unidad - 16.0) * 0.6);
    let col_etiqueta = (ancho - col_valor - col_unidad - 16.0).max(10.0);
    let galeria = ui.painter().layout(etiqueta.to_string(), FontId::proportional(13.0), pal.texto, col_etiqueta);
    let alto = (galeria.size().y + 6.0).max(36.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::hover());
    let r_valor = Rect::from_min_size(Pos2::new(r.left() + col_etiqueta + 8.0, r.center().y - 15.0), Vec2::new(col_valor, 30.0));
    ui.painter().galley(Pos2::new(r.left(), r.center().y - galeria.size().y / 2.0), galeria, pal.texto);
    let resp = ui.put(r_valor, egui::TextEdit::singleline(valor).font(tema::fuente_mono(12.0)).horizontal_align(egui::Align::Max).margin(egui::Margin::symmetric(8, 6)));
    if !unidad.is_empty() {
        ui.painter().text(Pos2::new(r_valor.right() + 8.0, r.center().y), Align2::LEFT_CENTER, unidad, tema::fuente_mono(12.0), pal.texto_2);
    }
    resp.on_hover_text(etiqueta)
}

/// Desplegable a todo el ancho, con la etiqueta encima.
///
/// Returns:
///     true si cambió la opción elegida.
pub fn desplegable(ui: &mut Ui, etiqueta: &str, opciones: &[String], elegida: &mut String) -> bool {
    let antes = elegida.clone();
    if !etiqueta.is_empty() {
        ui.label(egui::RichText::new(etiqueta).size(13.0).color(tema::p().texto));
    }
    egui::ComboBox::from_id_salt(("desplegable", etiqueta))
        .width(ui.available_width() - 8.0)
        .selected_text(egui::RichText::new(elegida.as_str()).size(13.0))
        .show_ui(ui, |ui| {
            for o in opciones {
                ui.selectable_value(elegida, o.clone(), o);
            }
        });
    antes != *elegida
}

/// Fila «Guardar en el espacio de trabajo»: casilla, descripción y nombre de la variable (128 px).
pub fn fila_guardar(ui: &mut Ui, activo: &mut bool, etiqueta: &str, nombre: &mut String) {
    let ancho = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, 34.0), Sense::hover());
    let casilla = Rect::from_min_size(Pos2::new(r.left(), r.center().y - 9.0), Vec2::splat(18.0));
    ui.put(casilla, egui::Checkbox::without_text(activo));
    let campo = Rect::from_min_size(Pos2::new(r.right() - 128.0, r.center().y - 14.0), Vec2::new(128.0, 28.0));
    ui.painter().with_clip_rect(Rect::from_min_max(Pos2::new(casilla.right() + 8.0, r.top()), Pos2::new(campo.left() - 8.0, r.bottom()))).text(
        Pos2::new(casilla.right() + 8.0, r.center().y),
        Align2::LEFT_CENTER,
        etiqueta,
        FontId::proportional(13.0),
        tema::p().texto,
    );
    ui.put(campo, egui::TextEdit::singleline(nombre).font(tema::fuente_mono(12.0)).margin(egui::Margin::symmetric(8, 5)));
}

/// Panel de parámetros (360 px) al estilo de un «Scenario Manager»: título «Parámetros» con
/// «Opciones» a la derecha, nombre y descripción de la herramienta, contenido desplazable y un pie
/// fijo con borde superior para las acciones («Exportar informe», «Calcular»).
///
/// Args:
///     ui: rectángulo completo del panel.
///     titulo: título del panel («Parámetros», «Detalle de la orden»...).
///     herramienta: nombre de la herramienta (14 px, peso medio); vacío para omitirlo.
///     descripcion: texto gris bajo el nombre.
///     contenido: filas de parámetros.
///     pie: acciones del pie; si no dibuja nada, el pie no ocupa sitio.
///
/// Returns:
///     true si se pulsó «Opciones».
pub fn panel_parametros(ui: &mut Ui, titulo: &str, herramienta: &str, descripcion: &str, contenido: impl FnOnce(&mut Ui), pie: Option<&mut dyn FnMut(&mut Ui)>) -> bool {
    let pal = tema::p();
    let r = ui.max_rect();
    ui.painter().rect_filled(r, 0.0, pal.panel);
    let alto_pie = if pie.is_some() { 68.0 } else { 0.0 };
    let cuerpo = Rect::from_min_max(r.min + Vec2::new(20.0, 18.0), Pos2::new(r.right() - 20.0, r.bottom() - alto_pie));
    let mut opciones = false;
    let mut c = ui.new_child(UiBuilder::new().max_rect(cuerpo).layout(egui::Layout::top_down(egui::Align::Min)));
    c.set_clip_rect(Rect::from_min_max(r.min, Pos2::new(r.right(), r.bottom() - alto_pie)).intersect(ui.clip_rect()));
    c.spacing_mut().item_spacing.y = 6.0;
    c.horizontal(|ui| {
        ui.label(egui::RichText::new(titulo).font(tema::fuente_negrita(20.0)).color(pal.texto));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            opciones = boton_texto(ui, Some(Icono::Ajustes), "Opciones").clicked();
        });
    });
    if !herramienta.is_empty() {
        c.add_space(4.0);
        c.label(egui::RichText::new(herramienta).family(tema::semi()).size(14.0).color(pal.texto));
    }
    if !descripcion.is_empty() {
        c.label(egui::RichText::new(descripcion).size(12.0).color(pal.texto_2));
        c.add_space(4.0);
    }
    egui::ScrollArea::vertical().id_salt(("panel_parametros", titulo)).auto_shrink([false, false]).show(&mut c, |ui| {
        ui.set_width(ui.available_width());
        contenido(ui);
    });
    if let Some(pie) = pie {
        let zona = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - alto_pie), r.max);
        ui.painter().line_segment([zona.left_top(), zona.right_top()], Stroke::new(1.0_f32, pal.linea));
        let mut p = ui.new_child(UiBuilder::new().max_rect(zona.shrink2(Vec2::new(20.0, 16.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
        p.spacing_mut().item_spacing.x = 12.0;
        pie(&mut p);
    }
    opciones
}

/// Estilo de un botón: principal (negro), secundario (contorno negro) o de cinta/texto (sin marco).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EstiloBoton {
    Principal,
    Secundario,
}

/// Botón rectangular sin radio, de 34 px (28 px si `pequeno`), con icono opcional y un atajo en
/// monoespaciada gris detrás del texto.
pub fn boton(ui: &mut Ui, estilo: EstiloBoton, icono: Option<Icono>, texto: &str, atajo: &str, pequeno: bool) -> egui::Response {
    let pal = tema::p();
    let (fondo, tinta, borde) = match estilo {
        EstiloBoton::Principal => (pal.acento, pal.sobre_acento, pal.acento),
        EstiloBoton::Secundario => (pal.panel, pal.texto, pal.acento),
    };
    let tam = if pequeno { 12.0 } else { 14.0 };
    let alto = if pequeno { 28.0 } else { 34.0 };
    let g = ui.painter().layout_no_wrap(texto.to_string(), FontId::new(tam, tema::semi()), tinta);
    let ga = (!atajo.is_empty()).then(|| ui.painter().layout_no_wrap(atajo.to_string(), tema::fuente_mono(11.0), pal.borde));
    let lado_icono = if pequeno { 16.0 } else { 18.0 };
    let ancho = 28.0 + g.size().x + icono.map_or(0.0, |_| lado_icono + 8.0) + ga.as_ref().map_or(0.0, |a| a.size().x + 8.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::click());
    let fondo = if resp.hovered() && estilo == EstiloBoton::Secundario { pal.campo } else { fondo };
    ui.painter().rect(r, 0.0, fondo, Stroke::new(1.0_f32, borde), StrokeKind::Inside);
    let mut x = r.left() + 14.0;
    if let Some(i) = icono {
        iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(x, r.center().y - lado_icono / 2.0), Vec2::splat(lado_icono)), i, tinta);
        x += lado_icono + 8.0;
    }
    let ancho_texto = g.size().x;
    ui.painter().galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, tinta);
    if let Some(a) = ga {
        let color = if estilo == EstiloBoton::Principal { pal.borde } else { pal.texto_2 };
        ui.painter().galley_with_override_text_color(Pos2::new(x + ancho_texto + 8.0, r.center().y - a.size().y / 2.0), a, color);
    }
    resp
}

/// Botón sin marco con icono opcional y texto de 13 px (barra superior, «Opciones», «Ver todas →»).
pub fn boton_texto(ui: &mut Ui, icono: Option<Icono>, texto: &str) -> egui::Response {
    let pal = tema::p();
    let g = ui.painter().layout_no_wrap(texto.to_string(), FontId::proportional(13.0), pal.texto);
    let ancho = g.size().x + icono.map_or(0.0, |_| 24.0) + 8.0;
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, pal.realce);
    }
    let mut x = r.left() + 4.0;
    if let Some(i) = icono {
        iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(x, r.center().y - 9.0), Vec2::splat(18.0)), i, pal.texto);
        x += 24.0;
    }
    ui.painter().galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, pal.texto);
    resp
}

/// Botón de icono de 30 × 30 px sin marco (guardar, deshacer, rehacer, cerrar pestaña).
pub fn boton_icono(ui: &mut Ui, icono: Icono, lado_icono: f32, ayuda: &str) -> egui::Response {
    let pal = tema::p();
    let lado = (lado_icono + 12.0).max(22.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(lado), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, pal.realce);
    }
    iconos::dibujar(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(lado_icono)), icono, pal.texto);
    resp.on_hover_text(ayuda)
}

/// Alto de la cinta de opciones.
pub const ALTO_CINTA: f32 = 94.0;

/// Grupo de la cinta: botones en una fila de 66 px, el nombre del grupo debajo en gris y una línea
/// vertical a la derecha.
pub fn grupo_cinta(ui: &mut Ui, nombre: &str, contenido: impl FnOnce(&mut Ui)) {
    let pal = tema::p();
    let inicio = ui.cursor().min.x;
    let arriba = ui.max_rect().top();
    let r = ui
        .vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(0.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.set_height(66.0);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), contenido);
                ui.add_space(12.0);
            });
        })
        .response
        .rect;
    let derecha = r.right().max(inicio);
    ui.painter().text(Pos2::new((inicio + derecha) / 2.0, arriba + 76.0), Align2::CENTER_CENTER, nombre, FontId::proportional(12.0), pal.texto_2);
    ui.painter().line_segment([Pos2::new(derecha, arriba), Pos2::new(derecha, arriba + ALTO_CINTA - 12.0)], Stroke::new(1.0_f32, pal.linea));
}

/// Botón grande de la cinta: icono de 24 px sobre el texto (hasta dos líneas de 12 px). El activo
/// lleva fondo de selección y borde.
pub fn boton_cinta(ui: &mut Ui, icono: Icono, texto: &str, activo: bool) -> egui::Response {
    let pal = tema::p();
    let fuente = if activo { FontId::new(12.0, tema::semi()) } else { FontId::proportional(12.0) };
    let ancho_texto = ui.painter().layout_no_wrap(texto.to_string(), fuente.clone(), pal.texto).size().x;
    let ancho = (ancho_texto + 12.0).clamp(64.0, 96.0);
    let g = ui.painter().layout(texto.to_string(), fuente, pal.texto, ancho - 8.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho, 62.0), Sense::click());
    if activo {
        ui.painter().rect(r, 0.0, pal.realce, Stroke::new(1.0_f32, pal.borde), StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, pal.campo);
    }
    iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(r.center().x - 12.0, r.top() + 6.0), Vec2::splat(24.0)), icono, pal.texto);
    let g_ancho = g.size().x;
    ui.painter().galley(Pos2::new(r.center().x - g_ancho / 2.0, r.top() + 36.0), g, pal.texto);
    resp.on_hover_text(texto)
}

/// Columna de botones pequeños de la cinta (icono de 14 px y texto de 12 px, 20 px de alto).
///
/// Returns:
///     Índice del botón pulsado.
pub fn columna_cinta(ui: &mut Ui, botones: &[(Icono, &str, bool)]) -> Option<usize> {
    let pal = tema::p();
    let mut pulsado = None;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.add_space(4.0);
        for (i, (icono, texto, habilitado)) in botones.iter().enumerate() {
            let tinta = if *habilitado { pal.texto } else { pal.texto_2.gamma_multiply(0.7) };
            let g = ui.painter().layout_no_wrap(texto.to_string(), FontId::proportional(12.0), tinta);
            let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 33.0, 20.0), if *habilitado { Sense::click() } else { Sense::hover() });
            if resp.hovered() && *habilitado {
                ui.painter().rect_filled(r, 0.0, pal.campo);
            }
            iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(r.left() + 4.0, r.center().y - 7.0), Vec2::splat(14.0)), *icono, tinta);
            ui.painter().galley(Pos2::new(r.left() + 25.0, r.center().y - g.size().y / 2.0), g, tinta);
            if resp.clicked() {
                pulsado = Some(i);
            }
        }
    });
    pulsado
}

/// Estado de un nodo del esquema del proyecto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EstadoNodo {
    /// Listo o sin avisos: punto verde.
    Listo,
    /// Pendiente o con aviso: punto rojo.
    Pendiente,
    /// Sin ejecutar: círculo hueco gris.
    SinEjecutar,
    /// Sin punto de estado (carpetas, tablas).
    Ninguno,
}

/// Punto de estado de 10 px del esquema del proyecto.
pub fn punto_estado(pt: &egui::Painter, centro: Pos2, e: EstadoNodo) {
    let pal = tema::p();
    match e {
        EstadoNodo::Listo => {
            pt.circle_filled(centro, 5.0, pal.ok);
        }
        EstadoNodo::Pendiente => {
            pt.circle_filled(centro, 5.0, pal.falla);
        }
        EstadoNodo::SinEjecutar => {
            pt.circle_stroke(centro, 4.5, Stroke::new(1.0_f32, pal.texto_2));
        }
        EstadoNodo::Ninguno => {}
    }
}

/// Fila de 28 px del esquema del proyecto: conmutador redondo (⊕ / ⊖) si `abierto` es Some, punto
/// de estado, icono opcional, texto y sufijo gris de 11 px. La fila activa lleva fondo de selección.
///
/// Returns:
///     Respuesta de la fila y true si se pulsó el conmutador.
#[allow(clippy::too_many_arguments)]
pub fn nodo_arbol(ui: &mut Ui, nivel: usize, abierto: Option<bool>, estado: EstadoNodo, icono: Option<Icono>, texto: &str, sufijo: &str, activo: bool) -> (egui::Response, bool) {
    let pal = tema::p();
    let ancho = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho, 28.0), Sense::click());
    if activo {
        ui.painter().rect_filled(r, 0.0, pal.realce);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 0.0, pal.campo);
    }
    let mut x = r.left() + 12.0 + 18.0 * nivel as f32;
    let mut conmutado = false;
    if let Some(a) = abierto {
        let c = Pos2::new(x + 7.0, r.center().y);
        let zona = Rect::from_center_size(c, Vec2::splat(18.0));
        let rc = ui.interact(zona, ui.id().with(("conmutador", texto, nivel, r.top() as i32)), Sense::click());
        conmutado = rc.clicked();
        ui.painter().circle_filled(Pos2::new(c.x - 9.0, c.y), 1.5, pal.acento);
        ui.painter().circle_filled(c, 6.5, pal.acento);
        ui.painter().line_segment([c - Vec2::new(3.5, 0.0), c + Vec2::new(3.5, 0.0)], Stroke::new(1.6_f32, pal.sobre_acento));
        if !a {
            ui.painter().line_segment([c - Vec2::new(0.0, 3.5), c + Vec2::new(0.0, 3.5)], Stroke::new(1.6_f32, pal.sobre_acento));
        }
        x += 20.0;
    }
    if estado != EstadoNodo::Ninguno {
        punto_estado(ui.painter(), Pos2::new(x + 5.0, r.center().y), estado);
        x += 19.0;
    }
    if let Some(i) = icono {
        iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(x, r.center().y - 8.0), Vec2::splat(16.0)), i, pal.texto);
        x += 25.0;
    }
    let fuente = if abierto.is_some() { FontId::new(13.0, tema::semi()) } else { FontId::proportional(13.0) };
    let gs = (!sufijo.is_empty()).then(|| ui.painter().layout_no_wrap(sufijo.to_string(), FontId::proportional(11.0), pal.texto_2));
    let reserva = gs.as_ref().map_or(0.0, |g| g.size().x + 14.0);
    let limite = r.right() - 8.0 - reserva;
    let g = ui.painter().layout_no_wrap(texto.to_string(), fuente, pal.texto);
    let ancho_texto = g.size().x.min(limite - x);
    ui.painter().with_clip_rect(Rect::from_min_max(Pos2::new(x, r.top()), Pos2::new(limite, r.bottom()))).galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, pal.texto);
    if let Some(gs) = gs {
        ui.painter().galley(Pos2::new(x + ancho_texto + 8.0, r.center().y - gs.size().y / 2.0), gs, pal.texto_2);
    }
    (resp.on_hover_text(texto), conmutado)
}

/// Pestañas con subrayado negro de 2 px en la activa (texto de 14 px) y acciones a la derecha;
/// se usa para «Esquema del proyecto | Opciones», «Diagnóstico | Consola» y similares.
///
/// Returns:
///     true si cambió la pestaña activa.
pub fn barra_pestanas(ui: &mut Ui, pestanas: &[&str], activa: &mut usize, acciones: impl FnOnce(&mut Ui)) -> bool {
    let pal = tema::p();
    let antes = *activa;
    ui.horizontal(|ui| {
        ui.set_min_height(40.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, t) in pestanas.iter().enumerate() {
            let on = i == *activa;
            let fuente = if on { FontId::new(14.0, tema::semi()) } else { FontId::proportional(14.0) };
            let g = ui.painter().layout_no_wrap(t.to_string(), fuente.clone(), pal.texto);
            let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 28.0, 40.0), Sense::click());
            if resp.hovered() && !on {
                ui.painter().rect_filled(r, 0.0, pal.campo);
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, *t, fuente, pal.texto);
            if on {
                ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.bottom() - 2.0), r.right_bottom()), 0.0, pal.acento);
            }
            if resp.clicked() {
                *activa = i;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), acciones);
    });
    antes != *activa
}

/// Pestañas de documento (34 px) sobre fondo gris: la activa en blanco con borde superior negro de
/// 2 px; cada una con su icono y, si `cerrables`, una cruz.
///
/// Returns:
///     Índice de la pestaña cuya cruz se pulsó.
pub fn pestanas_documento(ui: &mut Ui, pestanas: &[(Icono, &str)], activa: &mut usize, cerrables: bool) -> Option<usize> {
    let pal = tema::p();
    let ancho = ui.available_width();
    let (franja, _) = ui.allocate_exact_size(Vec2::new(ancho, 35.0), Sense::hover());
    ui.painter().rect_filled(franja, 0.0, pal.campo);
    ui.painter().line_segment([franja.left_bottom(), franja.right_bottom()], Stroke::new(1.0_f32, pal.linea));
    let mut x = franja.left();
    let mut cerrar = None;
    for (i, (icono, texto)) in pestanas.iter().enumerate() {
        let on = i == *activa;
        let g = ui.painter().layout_no_wrap(texto.to_string(), FontId::proportional(13.0), pal.texto);
        let w = 12.0 + 16.0 + 8.0 + g.size().x + if cerrables { 30.0 } else { 14.0 };
        let r = Rect::from_min_size(Pos2::new(x, franja.top()), Vec2::new(w, 34.0));
        let resp = ui.interact(r, ui.id().with(("pestana_documento", i)), Sense::click());
        ui.painter().rect_filled(r, 0.0, if on { pal.panel } else if resp.hovered() { pal.realce } else { pal.campo });
        if on {
            ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(w, 2.0)), 0.0, pal.acento);
        }
        ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0_f32, pal.linea));
        iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(r.left() + 12.0, r.center().y - 8.0), Vec2::splat(16.0)), *icono, pal.texto);
        ui.painter().galley(Pos2::new(r.left() + 36.0, r.center().y - g.size().y / 2.0), g, pal.texto);
        if cerrables {
            let zona = Rect::from_center_size(Pos2::new(r.right() - 18.0, r.center().y), Vec2::splat(22.0));
            let rc = ui.interact(zona, ui.id().with(("cerrar_documento", i)), Sense::click());
            if rc.hovered() {
                ui.painter().rect_filled(zona, 0.0, pal.realce);
            }
            iconos::dibujar(ui.painter(), Rect::from_center_size(zona.center(), Vec2::splat(14.0)), Icono::Cerrar, pal.texto);
            if rc.clicked() {
                cerrar = Some(i);
            }
        }
        if resp.clicked() {
            *activa = i;
        }
        x += w;
    }
    cerrar
}

/// Chip de 22 px en mayúsculas de 11 px, sin radio: gris claro o gris oscuro con texto blanco
/// (activo o identificador de un equipo).
pub fn chip(ui: &mut Ui, texto: &str, oscuro: bool) -> egui::Response {
    let pal = tema::p();
    let (fondo, tinta) = if oscuro { (pal.chip_activo, pal.sobre_chip_activo) } else { (pal.chip, pal.texto) };
    let g = ui.painter().layout_no_wrap(mayusculas(texto), FontId::new(11.0, tema::semi()), tinta);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 18.0, 22.0), Sense::click());
    ui.painter().rect_filled(r, 0.0, fondo);
    ui.painter().galley(Pos2::new(r.left() + 9.0, r.center().y - g.size().y / 2.0), g, tinta);
    resp
}

/// Fondo de una fila de tabla: todas con su línea inferior de separación.
pub fn fondo_fila(ui: &Ui, r: Rect, _indice: usize) {
    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0_f32, tema::p().linea));
}

/// Etiqueta de estado o categoría: un chip claro con un punto del color dado delante.
pub fn etiqueta(ui: &mut Ui, texto: &str, color: Color32) -> egui::Response {
    let pal = tema::p();
    let g = ui.painter().layout_no_wrap(mayusculas(texto), FontId::new(11.0, tema::semi()), pal.texto);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 30.0, 22.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, pal.chip);
    ui.painter().circle_filled(Pos2::new(r.left() + 11.0, r.center().y), 3.5, color);
    ui.painter().galley(Pos2::new(r.left() + 20.0, r.center().y - g.size().y / 2.0), g, pal.texto);
    resp
}

/// Forma de una marca de leyenda o de un tipo de orden.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Muestra {
    Linea,
    Discontinua,
    Rayada,
    Cuadrado,
    CuadradoContorno,
    Rombo,
}

/// Dibuja una marca de leyenda centrada en `r`.
pub fn dibujar_muestra(pt: &egui::Painter, r: Rect, color: Color32, m: Muestra) {
    let pal = tema::p();
    let c = r.center();
    match m {
        Muestra::Linea => {
            pt.line_segment([Pos2::new(r.left(), c.y), Pos2::new(r.right(), c.y)], Stroke::new(2.0_f32, color));
        }
        Muestra::Discontinua => {
            pt.extend(egui::Shape::dashed_line(&[Pos2::new(r.left(), c.y), Pos2::new(r.right(), c.y)], Stroke::new(2.0_f32, color), 4.0, 2.0));
        }
        Muestra::Rayada => {
            let caja = Rect::from_center_size(c, Vec2::new(14.0, 10.0));
            pt.rect(caja, 0.0, pal.campo, Stroke::new(1.0_f32, pal.borde), StrokeKind::Inside);
            let pt2 = pt.with_clip_rect(caja.shrink(1.0));
            for i in -3..5 {
                let x = caja.left() + i as f32 * 4.0;
                pt2.line_segment([Pos2::new(x, caja.bottom()), Pos2::new(x + 10.0, caja.top())], Stroke::new(1.5_f32, pal.borde));
            }
        }
        Muestra::Cuadrado => {
            pt.rect_filled(Rect::from_center_size(c, Vec2::splat(9.0)), 0.0, color);
        }
        Muestra::CuadradoContorno => {
            pt.rect(Rect::from_center_size(c, Vec2::splat(9.0)), 0.0, color.gamma_multiply(0.35), Stroke::new(1.5_f32, color), StrokeKind::Inside);
        }
        Muestra::Rombo => {
            let d = 5.5;
            pt.add(egui::Shape::convex_polygon(vec![c + Vec2::new(0.0, -d), c + Vec2::new(d, 0.0), c + Vec2::new(0.0, d), c + Vec2::new(-d, 0.0)], color, Stroke::NONE));
        }
    }
}

/// Muestra de leyenda (18 px) y texto de 12 px.
pub fn leyenda(ui: &mut Ui, color: Color32, texto: &str, m: Muestra) {
    let pal = tema::p();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 14.0), Sense::hover());
        dibujar_muestra(ui.painter(), r, color, m);
        ui.label(egui::RichText::new(texto).size(12.0).color(pal.texto));
    });
}

/// Forma con la que se distingue un tipo de orden de mantenimiento además de su color.
pub fn muestra_tipo(tipo: &str) -> Muestra {
    let t = tipo.to_lowercase();
    if t.contains("ensayo") || t.contains("curva") {
        Muestra::CuadradoContorno
    } else if t.contains("anual") {
        Muestra::Rombo
    } else {
        Muestra::Cuadrado
    }
}

/// Marca de 9 px con el color y la forma del tipo de orden de mantenimiento.
pub fn marca_tipo(ui: &mut Ui, tipo: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 16.0), Sense::hover());
    dibujar_muestra(ui.painter(), r, tema::color_tipo_orden(tipo), muestra_tipo(tipo));
}

/// Aviso en una barra negra con texto blanco y un botón blanco «Ocultar».
///
/// Returns:
///     true si se pulsó «Ocultar».
pub fn aviso_negro(ui: &mut Ui, texto: &str) -> bool {
    let ancho = ui.available_width();
    let g = ui.painter().layout(texto.to_string(), FontId::proportional(13.0), Color32::WHITE, ancho - 166.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, (g.size().y + 20.0).max(44.0)), Sense::hover());
    // En tema oscuro el negro se confunde con el fondo: un borde separa la barra
    let borde = if tema::modo() == tema::Modo::Oscuro { Stroke::new(1.0_f32, tema::p().borde) } else { Stroke::NONE };
    ui.painter().rect(r, 0.0, tema::NEGRO, borde, StrokeKind::Inside);
    iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(r.left() + 16.0, r.center().y - 9.0), Vec2::splat(18.0)), Icono::Aviso, Color32::WHITE);
    ui.painter().galley(Pos2::new(r.left() + 44.0, r.center().y - g.size().y / 2.0), g, Color32::WHITE);
    let b = Rect::from_min_size(Pos2::new(r.right() - 96.0, r.center().y - 14.0), Vec2::new(82.0, 28.0));
    let resp = ui.interact(b, ui.id().with("ocultar_aviso"), Sense::click());
    ui.painter().rect_filled(b, 0.0, if resp.hovered() { Color32::from_gray(230) } else { Color32::WHITE });
    ui.painter().text(b.center(), Align2::CENTER_CENTER, "Ocultar", FontId::new(12.0, tema::semi()), tema::NEGRO);
    resp.clicked()
}

/// Estado vacío centrado: un texto que explica qué falta y el botón de la acción que lo genera.
///
/// Returns:
///     true si se pulsó la acción.
pub fn estado_vacio(ui: &mut Ui, texto: &str, accion: &str) -> bool {
    let pal = tema::p();
    let mut pulsado = false;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() / 2.0 - 30.0).max(8.0));
        ui.label(egui::RichText::new(texto).color(pal.texto_2));
        if !accion.is_empty() && boton(ui, EstiloBoton::Secundario, None, accion, "", true).clicked() {
            pulsado = true;
        }
    });
    pulsado
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn el_separador_se_arrastra_se_guarda_y_se_restablece_con_doble_clic() {
        let contexto = egui::Context::default();
        let clave = "prueba_del_arrastre";
        let medir = |ctx: &egui::Context, tiempo, eventos| {
            let mut ancho = 0.0;
            let _ = ctx.run(egui::RawInput { events: eventos, time: Some(tiempo), ..Default::default() }, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let zona = Rect::from_min_size(Pos2::ZERO, Vec2::new(816.0, 300.0));
                    ancho = dividir(ui, clave, zona, true, 16.0, 0.5, [160.0, 180.0])[0].width();
                });
            });
            ancho
        };
        let boton = |pos, pressed| egui::Event::PointerButton {
            pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE,
        };
        let inicial = Pos2::new(408.0, 150.0);
        let desplazada = Pos2::new(558.0, 150.0);
        assert_eq!(medir(&contexto, 0.0, vec![]), 400.0);
        medir(&contexto, 0.1, vec![egui::Event::PointerMoved(inicial)]);
        medir(&contexto, 0.2, vec![boton(inicial, true)]);
        assert_eq!(medir(&contexto, 0.3, vec![egui::Event::PointerMoved(desplazada)]), 550.0);
        medir(&contexto, 0.4, vec![boton(desplazada, false)]);

        #[derive(Default)]
        struct Preferencias(Option<String>);
        impl eframe::Storage for Preferencias {
            fn get_string(&self, _: &str) -> Option<String> { self.0.clone() }
            fn set_string(&mut self, _: &str, valor: String) { self.0 = Some(valor); }
            fn flush(&mut self) {}
        }
        let mut preferencias = Preferencias::default();
        contexto.memory(|m| eframe::set_value(&mut preferencias, "egui", m));
        let reiniciado = egui::Context::default();
        reiniciado.memory_mut(|m| *m = eframe::get_value(&preferencias, "egui").unwrap());
        assert_eq!(medir(&reiniciado, 1.0, vec![]), 550.0);
        medir(&reiniciado, 1.1, vec![egui::Event::PointerMoved(desplazada)]);
        medir(&reiniciado, 1.2, vec![boton(desplazada, true)]);
        medir(&reiniciado, 1.3, vec![boton(desplazada, false)]);
        medir(&reiniciado, 1.4, vec![boton(desplazada, true)]);
        assert_eq!(medir(&reiniciado, 1.5, vec![boton(desplazada, false)]), 400.0);
    }

    #[test]
    fn el_separador_recupera_la_proporcion_despues_de_estrechar_la_ventana() {
        let ctx = egui::Context::default();
        let clave = "prueba_de_distribucion";
        let id = egui::Id::new(("distribucion_paneles", clave));
        ctx.data_mut(|d| d.insert_persisted(id, 0.75_f32));
        let medir = |ancho: f32| {
            let mut anchos = [0.0; 2];
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let r = Rect::from_min_size(ui.next_widget_position(), Vec2::new(ancho, 300.0));
                    anchos = dividir(ui, clave, r, true, 16.0, 0.5, [160.0, 180.0]).map(|r| r.width());
                });
            });
            anchos
        };
        assert_eq!(medir(816.0), [600.0, 200.0]);
        assert_eq!(medir(416.0), [220.0, 180.0]);
        assert_eq!(medir(816.0), [600.0, 200.0]);
    }

    #[test]
    fn la_rejilla_reparte_doce_columnas_con_huecos() {
        // 12 columnas de 60 px y 11 huecos de 16 px: 896 px
        let c = celdas(896.0, 12, 16.0, &[8, 4]);
        assert_eq!(c[0], (0.0, 8.0 * 60.0 + 7.0 * 16.0));
        assert_eq!(c[1].0, c[0].1 + 16.0);
        assert!((c[1].0 + c[1].1 - 896.0).abs() < 1e-3);
        let seis = celdas(896.0, 12, 16.0, &[2; 6]);
        assert!(seis.windows(2).all(|w| (w[0].1 - w[1].1).abs() < 1e-3));
        assert!((seis[5].0 + seis[5].1 - 896.0).abs() < 1e-3);
    }

    #[test]
    fn solo_se_apilan_las_filas_anchas_en_ventanas_estrechas() {
        assert!(apilar(1000.0, &[8, 4]));
        assert!(!apilar(1300.0, &[8, 4]));
        assert!(!apilar(1000.0, &[2; 6]));
    }

    #[test]
    fn el_minimo_por_fila_no_desborda_el_alto_disponible() {
        let a = altos(500.0, 0.0, &[3.0, 1.0, 1.0]);
        assert!(a.iter().all(|x| *x >= 120.0 - 1e-3));
        assert!((a.iter().sum::<f32>() + 3.0 * HUECO - 500.0).abs() < 1e-2);
    }

    #[test]
    fn las_filas_llenan_el_alto_disponible_en_proporcion() {
        let a = altos(800.0, 100.0, &[1.5, 1.0]);
        assert!((a[0] / a[1] - 1.5).abs() < 1e-3);
        assert!((a.iter().sum::<f32>() + 100.0 + 2.0 * HUECO - 800.0).abs() < 1e-3);
    }
}
