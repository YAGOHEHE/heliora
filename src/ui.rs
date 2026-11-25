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

/// Alto de las filas que llenan la pantalla: reparte `disponible` (menos huecos y filas fijas) según `pesos`.
pub fn altos(disponible: f32, fijos: f32, pesos: &[f32]) -> Vec<f32> {
    let libres = (disponible - fijos - HUECO * pesos.len() as f32).max(0.0);
    let total: f32 = pesos.iter().sum::<f32>().max(f32::EPSILON);
    pesos.iter().map(|p| (libres * p / total).max(120.0)).collect()
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
    hijo.set_clip_rect(r.shrink(1.0));
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
    hijo.set_clip_rect(r.shrink(1.0));
    contenido(&mut hijo)
}

/// Ficha de resultado: etiqueta arriba, valor en monoespaciada de 22 px con la unidad en gris y una nota.
///
/// Ocupa todo el `Ui` (una celda de 2 columnas). Si `activa` es Some, la ficha se puede pulsar
/// (se usa como filtro) y la activa se marca con el fondo y el borde de acento.
pub fn ficha_kpi(ui: &mut Ui, etiqueta: &str, valor: &str, unidad: &str, nota: &str, activa: Option<bool>) -> egui::Response {
    let pal = tema::p();
    let r = ui.max_rect();
    let resp = ui.interact(r, ui.id().with(("ficha", etiqueta)), if activa.is_some() { Sense::click() } else { Sense::hover() });
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
    fn las_filas_llenan_el_alto_disponible_en_proporcion() {
        let a = altos(800.0, 100.0, &[1.5, 1.0]);
        assert!((a[0] / a[1] - 1.5).abs() < 1e-3);
        assert!((a.iter().sum::<f32>() + 100.0 + 2.0 * HUECO - 800.0).abs() < 1e-3);
    }
}
