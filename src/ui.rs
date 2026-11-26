//! Componentes del rediseño: rejilla de 12 columnas, tarjetas con título, fichas de resultados,
//! filas de parámetros, barras de pestañas, chips y leyendas. Leen los colores de `tema::p()`.
use crate::tema;
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};

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

/// Ajusta el alto de dos filas en una página con la separación habitual entre tarjetas.
pub fn filas_ajustables(ui: &mut Ui, clave: impl std::hash::Hash, alturas: [f32; 2], minimos: [f32; 2]) -> [f32; 2] {
    let alto = (alturas[0] + alturas[1]).max(minimos[0] + minimos[1]);
    let ancho = ui.available_width();
    let r = Rect::from_min_size(ui.next_widget_position(), Vec2::new(ancho, alto + HUECO));
    let paneles = dividir(ui, clave, r, false, HUECO, alturas[0] / alturas.iter().sum::<f32>(), minimos);
    [paneles[0].height(), paneles[1].height()]
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

/// Fondo y borde de una tarjeta sobre el rectángulo completo del `Ui`.
fn fondo_tarjeta(ui: &Ui, r: Rect) {
    let pal = tema::p();
    ui.painter().rect(r, tema::radio_ventana() as f32, pal.panel, Stroke::new(1.0_f32, pal.linea), StrokeKind::Inside);
}

/// Texto en mayúsculas, 11 px, semibold, con espaciado, para títulos de tarjeta.
pub fn mayusculas(t: &str) -> String {
    let txt: String = t.to_uppercase().chars().flat_map(|c| [c, '\u{200A}']).collect();
    txt.trim_end().to_string()
}

/// Tarjeta que ocupa todo el `Ui`, con título en mayúsculas, acciones a la derecha y contenido.
/// El contenido se recorta también por el área visible del contenedor (p. ej. una zona desplazable).
///
/// Args:
///     ui: celda de la rejilla.
///     titulo: título de la tarjeta (se muestra en mayúsculas); vacío para no mostrarlo.
///     acciones: botones o datos alineados a la derecha del título.
///     contenido: cuerpo de la tarjeta, con márgenes de 16 px.
pub fn tarjeta_con_titulo<R>(ui: &mut Ui, titulo: &str, acciones: impl FnOnce(&mut Ui), contenido: impl FnOnce(&mut Ui) -> R) -> R {
    let r = ui.max_rect();
    fondo_tarjeta(ui, r);
    let dentro = r.shrink2(Vec2::new(16.0, 14.0));
    let mut hijo = ui.new_child(UiBuilder::new().max_rect(dentro).layout(egui::Layout::top_down(egui::Align::Min)));
    hijo.set_clip_rect(r.shrink(1.0).intersect(ui.clip_rect()));
    if !titulo.is_empty() {
        hijo.horizontal(|ui| {
            ui.set_min_height(22.0);
            ui.label(egui::RichText::new(mayusculas(titulo)).family(tema::semi()).size(11.0).color(tema::p().texto_2));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), acciones);
        });
        hijo.add_space(6.0);
    }
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

/// Ficha de resultado: etiqueta arriba, valor en monoespaciada de 22 px con la unidad en gris y una nota.
///
/// Ocupa todo el `Ui` (una celda de 2 columnas). Si `activa` es Some, la ficha se puede pulsar
/// (se usa como filtro) y la activa se marca con el fondo y el borde de acento.
pub fn ficha_kpi(ui: &mut Ui, etiqueta: &str, valor: &str, unidad: &str, nota: &str, activa: Option<bool>) -> egui::Response {
    let pal = tema::p();
    let r = ui.max_rect();
    let resp = ui.interact(r, ui.id().with(("ficha", etiqueta, r.min.x as i32, r.min.y as i32)), if activa.is_some() { Sense::click() } else { Sense::hover() });
    let marcada = activa == Some(true);
    let fondo = if marcada {
        pal.acento_fondo
    } else if activa.is_some() && resp.hovered() {
        pal.realce
    } else {
        pal.panel
    };
    ui.painter().rect(r, tema::radio_ventana() as f32, fondo, Stroke::new(1.0_f32, if marcada { pal.acento } else { pal.linea }), StrokeKind::Inside);
    let p = ui.painter_at(r.shrink(1.0));
    let x = r.left() + 14.0;
    p.text(Pos2::new(x, r.top() + 12.0), Align2::LEFT_TOP, etiqueta, FontId::proportional(12.5), if marcada { pal.acento_texto } else { pal.texto_2 });
    let g = p.layout_no_wrap(valor.to_string(), tema::fuente_mono(22.0), pal.texto);
    let y_valor = r.top() + 32.0;
    let ancho_valor = g.size().x;
    p.galley(Pos2::new(x, y_valor), g, pal.texto);
    if !unidad.is_empty() {
        p.text(Pos2::new(x + ancho_valor + 6.0, y_valor + 22.0), Align2::LEFT_BOTTOM, unidad, FontId::proportional(13.0), pal.texto_2);
    }
    if !nota.is_empty() {
        p.text(Pos2::new(x, r.bottom() - 12.0), Align2::LEFT_BOTTOM, nota, FontId::proportional(11.5), pal.texto_2);
    }
    resp.on_hover_text(format!("{etiqueta}: {valor} {unidad}"))
}

/// Fila de parámetro en tres columnas: etiqueta a la izquierda, valor en monoespaciada alineado a
/// la derecha y unidad en su propia columna.
pub fn fila_parametro(ui: &mut Ui, etiqueta: &str, valor: &mut String, unidad: &str) -> egui::Response {
    let pal = tema::p();
    let ancho = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, 30.0), Sense::hover());
    let col_unidad = 36.0;
    let col_valor = (ancho * 0.38).clamp(90.0, 160.0);
    let r_valor = Rect::from_min_size(Pos2::new(r.right() - col_unidad - col_valor, r.top() + 1.0), Vec2::new(col_valor, 28.0));
    ui.painter().with_clip_rect(Rect::from_min_max(r.min, Pos2::new(r_valor.left() - 8.0, r.bottom()))).text(
        Pos2::new(r.left(), r.center().y),
        Align2::LEFT_CENTER,
        etiqueta,
        FontId::proportional(13.0),
        pal.texto,
    );
    let resp = ui.put(r_valor, egui::TextEdit::singleline(valor).font(tema::fuente_mono(12.5)).horizontal_align(egui::Align::Max).margin(egui::Margin::symmetric(8, 4)));
    if !unidad.is_empty() {
        ui.painter().text(Pos2::new(r_valor.right() + 8.0, r.center().y), Align2::LEFT_CENTER, unidad, FontId::proportional(12.5), pal.texto_2);
    }
    resp.on_hover_text(etiqueta)
}

/// Barra de pestañas de 40 px con subrayado de acento en la activa y acciones a la derecha.
///
/// Returns:
///     true si cambió la pestaña activa.
pub fn barra_pestanas(ui: &mut Ui, pestanas: &[&str], activa: &mut usize, acciones: impl FnOnce(&mut Ui)) -> bool {
    let pal = tema::p();
    let antes = *activa;
    ui.horizontal(|ui| {
        ui.set_min_height(40.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        for (i, t) in pestanas.iter().enumerate() {
            let on = i == *activa;
            let g = ui.painter().layout_no_wrap(t.to_string(), FontId::proportional(13.0), pal.texto);
            let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 20.0, 40.0), Sense::click());
            if resp.hovered() && !on {
                ui.painter().rect_filled(r.shrink2(Vec2::new(0.0, 6.0)), tema::radio() as f32, pal.realce);
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, *t, FontId::proportional(13.0), if on { pal.texto } else { pal.texto_2 });
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

/// Chip redondeado con texto; con color, lleva un punto de ese color delante.
pub fn chip(ui: &mut Ui, texto: &str, color: Option<Color32>) -> egui::Response {
    let pal = tema::p();
    let g = ui.painter().layout_no_wrap(texto.to_string(), FontId::proportional(12.0), pal.texto);
    let extra = if color.is_some() { 14.0 } else { 0.0 };
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 16.0 + extra, 24.0), Sense::click());
    ui.painter().rect(r, CornerRadius::same(12), pal.panel, Stroke::new(1.0_f32, pal.linea), StrokeKind::Inside);
    let mut x = r.left() + 8.0;
    if let Some(c) = color {
        ui.painter().circle_filled(Pos2::new(x + 3.5, r.center().y), 3.5, c);
        x += extra;
    }
    ui.painter().galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, pal.texto);
    resp
}

/// Fondo de una fila de tabla: las impares con el fondo secundario suave, todas con su línea inferior.
pub fn fondo_fila(ui: &Ui, r: Rect, indice: usize) {
    let pal = tema::p();
    if indice % 2 == 1 {
        ui.painter().rect_filled(r, 0.0, pal.campo_alt);
    }
    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0_f32, pal.realce));
}

/// Etiqueta pequeña (radio de 4 px) con fondo del color dado aclarado y texto del mismo color.
pub fn etiqueta(ui: &mut Ui, texto: &str, color: Color32) -> egui::Response {
    let g = ui.painter().layout_no_wrap(texto.to_string(), FontId::proportional(11.5), color);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 12.0, 20.0), Sense::hover());
    ui.painter().rect_filled(r, tema::RADIO_FICHA, color.gamma_multiply(0.14));
    ui.painter().galley(Pos2::new(r.left() + 6.0, r.center().y - g.size().y / 2.0), g, color);
    resp
}

/// Marca cuadrada de 8 px con el color del tipo de orden de mantenimiento.
pub fn marca_tipo(ui: &mut Ui, tipo: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(8.0, 16.0), Sense::hover());
    ui.painter().rect_filled(Rect::from_center_size(r.center(), Vec2::splat(8.0)), 2.0, tema::color_tipo_orden(tipo));
}

/// Muestra de leyenda: trazo de 14 px (continuo o discontinuo) y texto.
pub fn leyenda(ui: &mut Ui, color: Color32, texto: &str, discontinua: bool) {
    let pal = tema::p();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
        let a = Pos2::new(r.left(), r.center().y);
        let b = Pos2::new(r.right(), r.center().y);
        if discontinua {
            ui.painter().extend(egui::Shape::dashed_line(&[a, b], Stroke::new(2.0_f32, color), 3.0, 2.0));
        } else {
            ui.painter().line_segment([a, b], Stroke::new(2.0_f32, color));
        }
        ui.label(egui::RichText::new(texto).size(12.0).color(pal.texto_2));
    });
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
        if !accion.is_empty() && ui.button(accion).clicked() {
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
