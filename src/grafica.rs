//! Gráficas 2D y 3D interactivas, dibujadas por la propia aplicación, con exportación PNG y SVG.
use crate::tema;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use serde_json::Value;

#[derive(Clone, PartialEq)]
pub enum TipoSerie {
    Linea,
    Puntos,
    Barras,
}

#[derive(Clone)]
pub struct Serie {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub nombre: String,
    pub tipo: TipoSerie,
    pub color: Option<Color32>,
    pub visible: bool,
}

#[derive(Clone)]
pub struct Fig2D {
    pub titulo: String,
    pub xlabel: String,
    pub ylabel: String,
    pub series: Vec<Serie>,
    pub lineas_h: Vec<f64>,
    pub log_x: bool,
    pub log_y: bool,
    pub vista: Option<[f64; 4]>,
}

#[derive(Clone)]
pub struct Fig3D {
    pub titulo: String,
    pub xlabel: String,
    pub ylabel: String,
    pub zlabel: String,
    pub linea: bool,
    pub x: Vec<Vec<f64>>,
    pub y: Vec<Vec<f64>>,
    pub z: Vec<Vec<f64>>,
    pub az: f32,
    pub el: f32,
    pub escala: f32,
}

#[derive(Clone)]
pub enum Figura {
    D2(Fig2D),
    D3(Fig3D),
}

/// Lo que pide el usuario desde la barra de una gráfica.
pub enum Accion {
    Png(Rect),
    Svg,
}

fn vec_f64(v: &Value) -> Vec<f64> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect())
        .unwrap_or_default()
}

fn texto(v: &Value, k: &str) -> String {
    v.get(k).and_then(|t| t.as_str()).unwrap_or("").to_string()
}

fn color_hex(s: &str) -> Option<Color32> {
    tema::color_hex(s)
}

impl Figura {
    /// Construye la figura a partir del JSON del motor.
    pub fn desde_json(v: &Value) -> Figura {
        if v.get("clase").and_then(|c| c.as_str()) == Some("3d") {
            let linea = v.get("tipo").and_then(|t| t.as_str()) == Some("linea3d");
            let matriz = |k: &str| -> Vec<Vec<f64>> {
                if linea {
                    vec![vec_f64(&v[k])]
                } else {
                    v[k].as_array().map(|f| f.iter().map(vec_f64).collect()).unwrap_or_default()
                }
            };
            return Figura::D3(Fig3D {
                titulo: texto(v, "titulo"),
                xlabel: texto(v, "xlabel"),
                ylabel: texto(v, "ylabel"),
                zlabel: texto(v, "zlabel"),
                linea,
                x: matriz("x"),
                y: matriz("y"),
                z: matriz("z"),
                az: -0.7,
                el: 0.55,
                escala: 1.0,
            });
        }
        let series = v["series"]
            .as_array()
            .map(|s| {
                s.iter()
                    .map(|a| Serie {
                        x: vec_f64(&a["x"]),
                        y: vec_f64(&a["y"]),
                        nombre: texto(a, "nombre"),
                        tipo: match a.get("tipo").and_then(|t| t.as_str()) {
                            Some("puntos") => TipoSerie::Puntos,
                            Some("barras") => TipoSerie::Barras,
                            _ => TipoSerie::Linea,
                        },
                        color: a.get("color").and_then(|c| c.as_str()).and_then(color_hex),
                        visible: true,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Figura::D2(Fig2D {
            titulo: texto(v, "titulo"),
            xlabel: texto(v, "xlabel"),
            ylabel: texto(v, "ylabel"),
            series,
            lineas_h: vec_f64(&v["lineas_h"]),
            log_x: v["log_x"].as_bool().unwrap_or(false),
            log_y: v["log_y"].as_bool().unwrap_or(false),
            vista: None,
        })
    }

    pub fn titulo(&self) -> &str {
        match self {
            Figura::D2(f) => &f.titulo,
            Figura::D3(f) => &f.titulo,
        }
    }

    pub fn a_svg(&self) -> String {
        match self {
            Figura::D2(f) => f.a_svg(),
            Figura::D3(f) => f.a_svg(),
        }
    }
}

// ---------------------------------------------------------------- utilidades de ejes

fn ticks(a: f64, b: f64, n: usize) -> Vec<f64> {
    if !a.is_finite() || !b.is_finite() || b <= a {
        return vec![];
    }
    let bruto = (b - a) / n as f64;
    let e = bruto.log10().floor();
    let base = 10f64.powf(e);
    let f = bruto / base;
    let paso = if f < 1.5 {
        base
    } else if f < 3.0 {
        2.0 * base
    } else if f < 7.0 {
        5.0 * base
    } else {
        10.0 * base
    };
    let mut v = vec![];
    let mut x = (a / paso).ceil() * paso;
    while x <= b + paso * 1e-6 {
        v.push(if x.abs() < paso * 1e-9 { 0.0 } else { x });
        x += paso;
    }
    v
}

/// Número legible con coma decimal: 0,025 · 3,14 · 230 · 1,20e6.
pub fn numero(x: f64) -> String {
    let a = x.abs();
    let s = if x == 0.0 {
        "0".into()
    } else if !x.is_finite() {
        return if x.is_nan() { "—".into() } else if x > 0.0 { "∞".into() } else { "-∞".into() };
    } else if a >= 1e5 || a < 1e-3 {
        format!("{x:.2e}")
    } else if a >= 100.0 {
        format!("{x:.0}")
    } else if a >= 1.0 {
        let s = format!("{x:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        let s = format!("{x:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    s.replace('.', ",")
}

/// Entero con separador de miles: 20 000.
pub fn miles(x: f64) -> String {
    let n = x.round() as i64;
    let t = n.abs().to_string();
    let mut o = String::new();
    for (i, ch) in t.chars().enumerate() {
        if i > 0 && (t.len() - i) % 3 == 0 {
            o.push('\u{202f}');
        }
        o.push(ch);
    }
    if n < 0 {
        format!("-{o}")
    } else {
        o
    }
}

/// Número con n decimales y coma decimal.
pub fn decimal(x: f64, n: usize) -> String {
    format!("{x:.n$}").replace('.', ",")
}

/// Prefijo SI que deja los valores de un eje entre 1 y 1000: (factor, prefijo).
fn prefijo_si(max_abs: f64) -> (f64, &'static str) {
    if !max_abs.is_finite() || max_abs == 0.0 {
        return (1.0, "");
    }
    for (f, p) in [(1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "µ"), (1e-9, "n")] {
        if max_abs >= f * 0.9999 {
            return (f, p);
        }
    }
    (1e-9, "n")
}

/// Etiqueta de eje con la unidad entre corchetes: «Potencia [W]» con prefijo k → «Potencia [kW]».
/// Sin unidad, el prefijo va como factor: «Potencia (×10³)».
fn etiqueta_con_prefijo(etiqueta: &str, f: f64, p: &str) -> String {
    if p.is_empty() {
        return etiqueta.to_string();
    }
    if let (Some(a), Some(b)) = (etiqueta.rfind('['), etiqueta.rfind(']')) {
        if a < b {
            let unidad = &etiqueta[a + 1..b];
            // Una unidad que ya lleva prefijo o es compuesta se deja con factor
            if !unidad.is_empty() && !unidad.contains('/') && !unidad.contains('%') && unidad != "pu" {
                return format!("{}[{}{}]{}", &etiqueta[..a], p, unidad, &etiqueta[b + 1..]);
            }
        }
    }
    let exp = f.log10().round() as i32;
    let sup: String = exp.to_string().chars().map(|ch| match ch {
        '-' => '⁻', '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴', '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', _ => '⁹',
    }).collect();
    if etiqueta.is_empty() {
        format!("×10{sup}")
    } else {
        format!("{etiqueta} (×10{sup})")
    }
}

/// Valor de una serie en x, interpolando entre sus puntos (x creciente).
fn valor_en(s: &Serie, x: f64) -> Option<f64> {
    if s.x.len() < 2 {
        return s.y.first().copied();
    }
    let i = s.x.partition_point(|&v| v < x);
    if i == 0 || i >= s.x.len() {
        return None;
    }
    let (x0, x1, y0, y1) = (s.x[i - 1], s.x[i], s.y[i - 1], s.y[i]);
    if !(y0.is_finite() && y1.is_finite()) {
        return None;
    }
    Some(if x1 > x0 { y0 + (y1 - y0) * (x - x0) / (x1 - x0) } else { y1 })
}

impl Fig2D {
    fn tx(&self, x: f64) -> f64 {
        if self.log_x {
            x.max(1e-300).log10()
        } else {
            x
        }
    }
    fn ty(&self, y: f64) -> f64 {
        if self.log_y {
            y.max(1e-300).log10()
        } else {
            y
        }
    }

    fn encuadre(&self) -> [f64; 4] {
        let (mut x0, mut x1, mut y0, mut y1) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for s in self.series.iter().filter(|s| s.visible) {
            for (&a, &b) in s.x.iter().zip(s.y.iter()) {
                if a.is_finite() && b.is_finite() {
                    let (a, b) = (self.tx(a), self.ty(b));
                    x0 = x0.min(a);
                    x1 = x1.max(a);
                    y0 = y0.min(b);
                    y1 = y1.max(b);
                }
            }
            if s.tipo == TipoSerie::Barras {
                y0 = y0.min(0.0);
                y1 = y1.max(0.0);
            }
        }
        for &h in &self.lineas_h {
            if h.is_finite() {
                y0 = y0.min(h);
                y1 = y1.max(h);
            }
        }
        if !x0.is_finite() {
            return [0.0, 1.0, 0.0, 1.0];
        }
        if x1 <= x0 {
            x0 -= 0.5;
            x1 += 0.5;
        }
        if y1 <= y0 {
            let d = if y0 == 0.0 { 0.5 } else { y0.abs() * 0.1 };
            y0 -= d;
            y1 += d;
        }
        let m = (y1 - y0) * 0.06;
        [x0, x1, y0 - m, y1 + m]
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, id: egui::Id, alto: f32) -> Option<Accion> {
        let mut accion = None;
        ui.horizontal(|ui| {
            if !self.titulo.is_empty() {
                ui.label(egui::RichText::new(&self.titulo).strong());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("SVG").on_hover_text("Guardar como SVG").clicked() {
                    accion = Some(Accion::Svg);
                }
                if ui.small_button("PNG").on_hover_text("Guardar como PNG").clicked() {
                    accion = Some(Accion::Png(Rect::NOTHING));
                }
                if ui.small_button("Encuadrar").on_hover_text("Volver a ver todos los datos (o doble clic)").clicked() {
                    self.vista = None;
                }
            });
        });
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(100.0), alto), Sense::click_and_drag());
        let resp = resp.on_hover_cursor(egui::CursorIcon::Crosshair);
        let _ = id;
        let pal = tema::p();
        let p = ui.painter_at(rect);
        p.rect_filled(rect, tema::radio() as f32, pal.fondo);
        let hay_leyenda = self.series.len() > 1 || self.series.iter().any(|s| !s.nombre.is_empty());
        let area = Rect::from_min_max(
            Pos2::new(rect.left() + 58.0, rect.top() + 12.0),
            Pos2::new(rect.right() - 14.0, rect.bottom() - if self.xlabel.is_empty() { 26.0 } else { 40.0 }),
        );
        let area = if area.width() < 10.0 || area.height() < 10.0 { rect } else { area };
        let vista = self.vista.unwrap_or_else(|| self.encuadre());
        let [x0, x1, y0, y1] = vista;
        let px = |x: f64| area.left() + ((x - x0) / (x1 - x0)) as f32 * area.width();
        let py = |y: f64| area.bottom() - ((y - y0) / (y1 - y0)) as f32 * area.height();
        let rejilla = Stroke::new(1.0, pal.linea.gamma_multiply(0.55));
        let fuente = tema::fuente_mono(10.5);
        // El número de marcas depende del espacio disponible
        let marcas_x = ((area.width() / 90.0) as usize).clamp(3, 10);
        let marcas_y = ((area.height() / 36.0) as usize).clamp(2, 7);
        let tx = ticks(x0, x1, marcas_x);
        let ty = ticks(y0, y1, marcas_y);
        // Prefijo SI en los ejes lineales: 70 kW en lugar de 70000
        let (fx, prx) = if self.log_x { (1.0, "") } else { prefijo_si(tx.iter().fold(0.0f64, |m, v| m.max(v.abs()))) };
        let (fy, pry) = if self.log_y { (1.0, "") } else { prefijo_si(ty.iter().fold(0.0f64, |m, v| m.max(v.abs()))) };
        for t in tx {
            let x = px(t);
            p.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], rejilla);
            let e = if self.log_x { 10f64.powf(t) } else { t / fx };
            p.text(Pos2::new(x, area.bottom() + 4.0), Align2::CENTER_TOP, numero(e), fuente.clone(), pal.texto_3);
        }
        for t in ty {
            let y = py(t);
            p.line_segment([Pos2::new(area.left(), y), Pos2::new(area.right(), y)], rejilla);
            let e = if self.log_y { 10f64.powf(t) } else { t / fy };
            p.text(Pos2::new(area.left() - 5.0, y), Align2::RIGHT_CENTER, numero(e), fuente.clone(), pal.texto_3);
        }
        p.rect_stroke(area, 0.0, Stroke::new(1.0, pal.linea_fuerte), egui::StrokeKind::Middle);
        let xlabel = etiqueta_con_prefijo(&self.xlabel, fx, prx);
        let ylabel = etiqueta_con_prefijo(&self.ylabel, fy, pry);
        if !xlabel.is_empty() {
            p.text(Pos2::new(area.center().x, rect.bottom() - 4.0), Align2::CENTER_BOTTOM, &xlabel, tema::fuente_mono(11.0), pal.texto_3);
        }
        if !ylabel.is_empty() {
            let g = ui.fonts_mut(|f| f.layout_no_wrap(ylabel.clone(), tema::fuente_mono(11.0), pal.texto_3));
            let pos = Pos2::new(rect.left() + 3.0, area.center().y + g.size().x / 2.0);
            p.add(egui::epaint::TextShape::new(pos, g, pal.texto_3).with_angle(-std::f32::consts::FRAC_PI_2));
        }
        let pa = p.with_clip_rect(area);
        // Líneas de referencia: discontinuas en TEXTO_3 (los colores de estado no se usan aquí)
        for &h in &self.lineas_h {
            let y = py(self.ty(h));
            pa.extend(Shape::dashed_line(&[Pos2::new(area.left(), y), Pos2::new(area.right(), y)], Stroke::new(1.0, pal.texto_3), 6.0, 4.0));
        }
        for (i, s) in self.series.iter().enumerate() {
            if !s.visible {
                continue;
            }
            let c = s.color.unwrap_or(tema::p().series[i % tema::p().series.len()]);
            match s.tipo {
                TipoSerie::Linea => {
                    let mut tramo: Vec<Pos2> = Vec::with_capacity(s.x.len());
                    for (&a, &b) in s.x.iter().zip(s.y.iter()) {
                        if a.is_finite() && b.is_finite() {
                            tramo.push(Pos2::new(px(self.tx(a)), py(self.ty(b))));
                        } else if !tramo.is_empty() {
                            pa.add(Shape::line(std::mem::take(&mut tramo), Stroke::new(1.5, c)));
                        }
                    }
                    if tramo.len() > 1 {
                        pa.add(Shape::line(tramo, Stroke::new(1.5, c)));
                    }
                }
                TipoSerie::Puntos => {
                    for (&a, &b) in s.x.iter().zip(s.y.iter()) {
                        if a.is_finite() && b.is_finite() {
                            pa.circle_filled(Pos2::new(px(self.tx(a)), py(self.ty(b))), 2.4, c);
                        }
                    }
                }
                TipoSerie::Barras => {
                    let ancho = if s.x.len() > 1 {
                        ((px(self.tx(s.x[1])) - px(self.tx(s.x[0]))).abs() * 0.85).max(1.0)
                    } else {
                        10.0
                    };
                    let base = py(0f64.max(y0).min(y1));
                    for (&a, &b) in s.x.iter().zip(s.y.iter()) {
                        if a.is_finite() && b.is_finite() {
                            let x = px(self.tx(a));
                            let y = py(self.ty(b));
                            pa.rect_filled(Rect::from_min_max(Pos2::new(x - ancho / 2.0, y.min(base)), Pos2::new(x + ancho / 2.0, y.max(base))), 0.0, c.gamma_multiply(0.85));
                        }
                    }
                }
            }
        }
        // Leyenda que además oculta y muestra series
        let mut clic_leyenda = None;
        if hay_leyenda {
            let mut y = area.top() + 6.0;
            let ancho_max = self.series.iter().map(|s| s.nombre.chars().count()).max().unwrap_or(5) as f32 * 6.6 + 22.0;
            let fondo = Rect::from_min_size(Pos2::new(area.right() - ancho_max - 6.0, y - 2.0), Vec2::new(ancho_max, self.series.len().min(12) as f32 * 15.0 + 4.0));
            p.rect(fondo, tema::radio() as f32, pal.campo, Stroke::new(1.0, pal.linea), egui::StrokeKind::Inside);
            for (i, s) in self.series.iter().take(12).enumerate() {
                let c = s.color.unwrap_or(tema::p().series[i % tema::p().series.len()]);
                let c = if s.visible { c } else { tema::p().linea };
                let fila = Rect::from_min_size(Pos2::new(fondo.left(), y), Vec2::new(ancho_max, 15.0));
                p.line_segment([Pos2::new(fondo.left() + 4.0, y + 7.0), Pos2::new(fondo.left() + 18.0, y + 7.0)], Stroke::new(2.0, c));
                let nombre = if s.nombre.is_empty() { format!("serie {}", i + 1) } else { s.nombre.clone() };
                p.text(Pos2::new(fondo.left() + 22.0, y + 7.0), Align2::LEFT_CENTER, nombre, FontId::proportional(11.0), if s.visible { tema::p().texto } else { tema::p().texto_2 });
                if resp.clicked() {
                    if let Some(pp) = resp.interact_pointer_pos() {
                        if fila.contains(pp) {
                            clic_leyenda = Some(i);
                        }
                    }
                }
                y += 15.0;
            }
        }
        if let Some(i) = clic_leyenda {
            self.series[i].visible = !self.series[i].visible;
        }
        // Lectura bajo el cursor
        if let Some(pp) = resp.hover_pos() {
            if area.contains(pp) {
                let xv = x0 + ((pp.x - area.left()) / area.width()) as f64 * (x1 - x0);
                let yv = y1 - ((pp.y - area.top()) / area.height()) as f64 * (y1 - y0);
                let dx = if self.log_x { 10f64.powf(xv) } else { xv };
                let dy = if self.log_y { 10f64.powf(yv) } else { yv };
                p.line_segment([Pos2::new(pp.x, area.top()), Pos2::new(pp.x, area.bottom())], Stroke::new(1.0, pal.linea_fuerte));
                // Lectura en un recuadro CAMPO: el tiempo (x) y el valor de cada serie visible en ese x
                let mut lineas = vec![(format!("{} = {}", if self.xlabel.to_lowercase().starts_with("tiempo") { "t" } else { "x" }, numero(dx)), pal.texto)];
                let mut con_serie = false;
                for (i, s) in self.series.iter().enumerate().filter(|(_, s)| s.visible && s.tipo == TipoSerie::Linea).take(4) {
                    if let Some(v) = valor_en(s, dx) {
                        let c = s.color.unwrap_or(pal.series[i % pal.series.len()]);
                        let n = if s.nombre.is_empty() { format!("serie {}", i + 1) } else { s.nombre.clone() };
                        lineas.push((format!("{n}: {}", numero(v)), c));
                        con_serie = true;
                    }
                }
                if !con_serie {
                    lineas.push((format!("y = {}", numero(dy)), pal.texto));
                }
                let f = tema::fuente_mono(11.0);
                let ancho = lineas.iter().map(|(t, _)| ui.fonts_mut(|fo| fo.layout_no_wrap(t.clone(), f.clone(), pal.texto).size().x)).fold(0.0f32, f32::max);
                let caja = Rect::from_min_size(area.left_top() + Vec2::new(6.0, 6.0), Vec2::new(ancho + 14.0, lineas.len() as f32 * 15.0 + 8.0));
                p.rect(caja, tema::radio() as f32, pal.campo, Stroke::new(1.0, pal.linea), egui::StrokeKind::Inside);
                for (k, (t, c)) in lineas.into_iter().enumerate() {
                    p.text(caja.left_top() + Vec2::new(7.0, 4.0 + k as f32 * 15.0), Align2::LEFT_TOP, t, f.clone(), c);
                }
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let k = (-scroll as f64 * 0.003).exp();
                    let nx0 = xv - (xv - x0) * k;
                    let nx1 = xv + (x1 - xv) * k;
                    let solo_x = ui.input(|i| i.modifiers.shift);
                    let (ny0, ny1) = if solo_x { (y0, y1) } else { (yv - (yv - y0) * k, yv + (y1 - yv) * k) };
                    self.vista = Some([nx0, nx1, ny0, ny1]);
                }
            }
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            let dx = -(d.x as f64) / area.width() as f64 * (x1 - x0);
            let dy = (d.y as f64) / area.height() as f64 * (y1 - y0);
            self.vista = Some([x0 + dx, x1 + dx, y0 + dy, y1 + dy]);
        }
        if resp.double_clicked() {
            self.vista = None;
        }
        if let Some(Accion::Png(_)) = accion {
            accion = Some(Accion::Png(rect));
        }
        accion
    }

    pub fn a_svg(&self) -> String {
        let (w, h) = (960.0, 560.0);
        let (l, r, t, b) = (78.0, 950.0, 46.0, 500.0);
        let [x0, x1, y0, y1] = self.vista.unwrap_or_else(|| self.encuadre());
        let px = |x: f64| l + (x - x0) / (x1 - x0) * (r - l);
        let py = |y: f64| b - (y - y0) / (y1 - y0) * (b - t);
        let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"Helvetica,Arial,sans-serif\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n");
        s += &format!("<text x=\"{}\" y=\"28\" text-anchor=\"middle\" font-size=\"17\" font-weight=\"bold\">{}</text>\n", (l + r) / 2.0, esc(&self.titulo));
        for k in ticks(x0, x1, 8) {
            let x = px(k);
            s += &format!("<line x1=\"{x:.1}\" x2=\"{x:.1}\" y1=\"{t}\" y2=\"{b}\" stroke=\"#ddd\"/><text x=\"{x:.1}\" y=\"{}\" text-anchor=\"middle\" font-size=\"12\">{}</text>\n", b + 18.0, numero(if self.log_x { 10f64.powf(k) } else { k }));
        }
        for k in ticks(y0, y1, 6) {
            let y = py(k);
            s += &format!("<line x1=\"{l}\" x2=\"{r}\" y1=\"{y:.1}\" y2=\"{y:.1}\" stroke=\"#ddd\"/><text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"12\">{}</text>\n", l - 6.0, y + 4.0, numero(if self.log_y { 10f64.powf(k) } else { k }));
        }
        s += &format!("<rect x=\"{l}\" y=\"{t}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#444\"/>\n<clipPath id=\"a\"><rect x=\"{l}\" y=\"{t}\" width=\"{}\" height=\"{}\"/></clipPath>\n", r - l, b - t, r - l, b - t);
        s += &format!("<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"14\">{}</text>\n", (l + r) / 2.0, b + 40.0, esc(&self.xlabel));
        s += &format!("<text transform=\"translate(18 {}) rotate(-90)\" text-anchor=\"middle\" font-size=\"14\">{}</text>\n", (t + b) / 2.0, esc(&self.ylabel));
        s += "<g clip-path=\"url(#a)\">\n";
        for &hl in &self.lineas_h {
            let y = py(hl);
            s += &format!("<line x1=\"{l}\" x2=\"{r}\" y1=\"{y:.1}\" y2=\"{y:.1}\" stroke=\"#c33\" stroke-dasharray=\"6 4\"/>\n");
        }
        for (i, se) in self.series.iter().enumerate().filter(|(_, q)| q.visible) {
            let c = se.color.unwrap_or(tema::p().series[i % tema::p().series.len()]);
            let col = format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
            match se.tipo {
                TipoSerie::Linea => {
                    let mut d = String::new();
                    let mut nuevo = true;
                    for (&a, &bb) in se.x.iter().zip(se.y.iter()) {
                        if a.is_finite() && bb.is_finite() {
                            d += &format!("{}{:.1} {:.1} ", if nuevo { "M" } else { "L" }, px(self.tx(a)), py(self.ty(bb)));
                            nuevo = false;
                        } else {
                            nuevo = true;
                        }
                    }
                    s += &format!("<path d=\"{d}\" fill=\"none\" stroke=\"{col}\" stroke-width=\"1.5\"/>\n");
                }
                TipoSerie::Puntos => {
                    for (&a, &bb) in se.x.iter().zip(se.y.iter()) {
                        if a.is_finite() && bb.is_finite() {
                            s += &format!("<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.5\" fill=\"{col}\"/>\n", px(self.tx(a)), py(self.ty(bb)));
                        }
                    }
                }
                TipoSerie::Barras => {
                    let an = if se.x.len() > 1 { ((px(self.tx(se.x[1])) - px(self.tx(se.x[0]))).abs() * 0.85).max(1.0) } else { 10.0 };
                    let base = py(0f64.max(y0).min(y1));
                    for (&a, &bb) in se.x.iter().zip(se.y.iter()) {
                        if a.is_finite() && bb.is_finite() {
                            let y = py(self.ty(bb));
                            s += &format!("<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{an:.1}\" height=\"{:.1}\" fill=\"{col}\"/>\n", px(self.tx(a)) - an / 2.0, y.min(base), (y - base).abs());
                        }
                    }
                }
            }
        }
        s += "</g>\n";
        let mut y = t + 16.0;
        for (i, se) in self.series.iter().enumerate().take(14).filter(|(_, q)| !q.nombre.is_empty()) {
            let c = se.color.unwrap_or(tema::p().series[i % tema::p().series.len()]);
            s += &format!("<line x1=\"{}\" x2=\"{}\" y1=\"{y}\" y2=\"{y}\" stroke=\"#{:02x}{:02x}{:02x}\" stroke-width=\"2\"/><text x=\"{}\" y=\"{}\" font-size=\"12\">{}</text>\n", r - 170.0, r - 150.0, c.r(), c.g(), c.b(), r - 145.0, y + 4.0, esc(&se.nombre));
            y += 16.0;
        }
        s + "</svg>\n"
    }
}

fn esc(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

// ---------------------------------------------------------------- 3D

fn mapa(t: f32) -> Color32 {
    tema::mapa_color(t)
}

impl Fig3D {
    fn rangos(&self) -> ([f64; 2], [f64; 2], [f64; 2]) {
        let r = |m: &Vec<Vec<f64>>| -> [f64; 2] {
            let mut a = f64::INFINITY;
            let mut b = f64::NEG_INFINITY;
            for f in m {
                for &v in f {
                    if v.is_finite() {
                        a = a.min(v);
                        b = b.max(v);
                    }
                }
            }
            if !a.is_finite() {
                return [0.0, 1.0];
            }
            if b <= a {
                b = a + 1.0;
            }
            [a, b]
        };
        (r(&self.x), r(&self.y), r(&self.z))
    }

    fn proyectar(&self, centro: Pos2, s: f32, q: [f32; 3]) -> (Pos2, f32) {
        let (ca, sa) = (self.az.cos(), self.az.sin());
        let (ce, se) = (self.el.cos(), self.el.sin());
        let x1 = q[0] * ca - q[1] * sa;
        let y1 = q[0] * sa + q[1] * ca;
        let y2 = y1 * se - q[2] * ce;
        let prof = y1 * ce + q[2] * se;
        (Pos2::new(centro.x + x1 * s, centro.y + y2 * s), prof)
    }

    pub fn mostrar(&mut self, ui: &mut egui::Ui, alto: f32) -> Option<Accion> {
        let mut accion = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&self.titulo).strong());
            ui.label(egui::RichText::new("arrastra para girar · rueda para acercar").small().color(tema::p().texto_2));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("SVG").clicked() {
                    accion = Some(Accion::Svg);
                }
                if ui.small_button("PNG").clicked() {
                    accion = Some(Accion::Png(Rect::NOTHING));
                }
            });
        });
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(100.0), alto), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 6.0, tema::p().fondo);
        if resp.dragged() {
            self.az += resp.drag_delta().x * 0.01;
            self.el = (self.el + resp.drag_delta().y * 0.01).clamp(-1.5, 1.5);
        }
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                self.escala = (self.escala * (sc * 0.003).exp()).clamp(0.3, 4.0);
            }
        }
        let centro = rect.center();
        let s = rect.width().min(rect.height()) * 0.31 * self.escala;
        let (rx, ry, rz) = self.rangos();
        let norm = |v: f64, r: [f64; 2]| ((v - r[0]) / (r[1] - r[0]) * 2.0 - 1.0) as f32;
        // Caja de ejes
        let aristas = [([-1., -1., -1.], [1., -1., -1.]), ([-1., -1., -1.], [-1., 1., -1.]), ([1., 1., -1.], [1., -1., -1.]), ([1., 1., -1.], [-1., 1., -1.]), ([-1., -1., -1.], [-1., -1., 1.])];
        for (a, b) in aristas {
            let (pa, _) = self.proyectar(centro, s, a);
            let (pb, _) = self.proyectar(centro, s, b);
            p.line_segment([pa, pb], Stroke::new(1.0, tema::p().linea));
        }
        let eti = |q: [f32; 3], t: String, p: &egui::Painter, f: &Fig3D| {
            let (pp, _) = f.proyectar(centro, s, q);
            p.text(pp, Align2::CENTER_CENTER, t, FontId::proportional(11.0), tema::p().texto_2);
        };
        eti([1.15, -1.1, -1.0], format!("{} {}", self.xlabel, numero(rx[1])), &p, self);
        eti([-1.1, -1.15, -1.0], numero(rx[0]), &p, self);
        eti([-1.15, 1.1, -1.0], format!("{} {}", self.ylabel, numero(ry[1])), &p, self);
        eti([-1.05, -1.05, 1.12], format!("{} {}", self.zlabel, numero(rz[1])), &p, self);
        eti([-1.2, -1.05, -1.0], numero(rz[0]), &p, self);
        if self.linea {
            if let (Some(x), Some(y), Some(z)) = (self.x.first(), self.y.first(), self.z.first()) {
                let pts: Vec<Pos2> = (0..x.len().min(y.len()).min(z.len()))
                    .map(|i| self.proyectar(centro, s, [norm(x[i], rx), norm(y[i], ry), norm(z[i], rz)]).0)
                    .collect();
                p.add(Shape::line(pts, Stroke::new(1.6, tema::p().acento)));
            }
        } else {
            let mut caras = self.caras(rx, ry, rz);
            for c in caras.iter_mut() {
                c.2 = c.0.iter().map(|q| self.proyectar(centro, s, *q).1).sum::<f32>() / 4.0;
            }
            caras.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            for (q, col, _) in caras {
                let puntos: Vec<Pos2> = q.iter().map(|v| self.proyectar(centro, s, *v).0).collect();
                let mut m = egui::Mesh::default();
                for pp in &puntos {
                    m.colored_vertex(*pp, col);
                }
                m.add_triangle(0, 1, 2);
                m.add_triangle(0, 2, 3);
                p.add(Shape::mesh(m));
                p.add(Shape::closed_line(puntos, Stroke::new(0.5, tema::p().linea)));
            }
        }
        if let Some(Accion::Png(_)) = accion {
            accion = Some(Accion::Png(rect));
        }
        accion
    }

    fn caras(&self, rx: [f64; 2], ry: [f64; 2], rz: [f64; 2]) -> Vec<([[f32; 3]; 4], Color32, f32)> {
        let norm = |v: f64, r: [f64; 2]| ((v - r[0]) / (r[1] - r[0]) * 2.0 - 1.0) as f32;
        let mut v = vec![];
        let filas = self.z.len();
        for i in 0..filas.saturating_sub(1) {
            let col = self.z[i].len();
            for j in 0..col.saturating_sub(1) {
                let ind = [(i, j), (i, j + 1), (i + 1, j + 1), (i + 1, j)];
                let mut q = [[0f32; 3]; 4];
                let mut zm = 0.0;
                let mut ok = true;
                for (k, (a, b)) in ind.iter().enumerate() {
                    let z = self.z[*a][*b];
                    if !z.is_finite() {
                        ok = false;
                        break;
                    }
                    q[k] = [norm(self.x[*a][*b], rx), norm(self.y[*a][*b], ry), norm(z, rz)];
                    zm += q[k][2];
                }
                if ok {
                    v.push((q, mapa((zm / 4.0 + 1.0) / 2.0), 0.0));
                }
            }
        }
        v
    }

    pub fn a_svg(&self) -> String {
        let (w, h) = (900.0f32, 640.0f32);
        let centro = Pos2::new(w / 2.0, h / 2.0 + 20.0);
        let s = 200.0;
        let (rx, ry, rz) = self.rangos();
        let norm = |v: f64, r: [f64; 2]| ((v - r[0]) / (r[1] - r[0]) * 2.0 - 1.0) as f32;
        let mut o = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" font-family=\"Helvetica,Arial,sans-serif\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/><text x=\"{}\" y=\"30\" text-anchor=\"middle\" font-size=\"18\" font-weight=\"bold\">{}</text>\n", w / 2.0, esc(&self.titulo));
        if self.linea {
            if let (Some(x), Some(y), Some(z)) = (self.x.first(), self.y.first(), self.z.first()) {
                let mut d = String::new();
                for i in 0..x.len().min(y.len()).min(z.len()) {
                    let pp = self.proyectar(centro, s, [norm(x[i], rx), norm(y[i], ry), norm(z[i], rz)]).0;
                    d += &format!("{}{:.1} {:.1} ", if i == 0 { "M" } else { "L" }, pp.x, pp.y);
                }
                o += &format!("<path d=\"{d}\" fill=\"none\" stroke=\"#c47f0a\" stroke-width=\"1.5\"/>\n");
            }
        } else {
            let mut caras = self.caras(rx, ry, rz);
            for c in caras.iter_mut() {
                c.2 = c.0.iter().map(|q| self.proyectar(centro, s, *q).1).sum::<f32>() / 4.0;
            }
            caras.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            for (q, col, _) in caras {
                let d: Vec<String> = q.iter().map(|v| { let pp = self.proyectar(centro, s, *v).0; format!("{:.1},{:.1}", pp.x, pp.y) }).collect();
                o += &format!("<polygon points=\"{}\" fill=\"#{:02x}{:02x}{:02x}\" stroke=\"#0004\" stroke-width=\"0.4\"/>\n", d.join(" "), col.r(), col.g(), col.b());
            }
        }
        o + "</svg>\n"
    }
}

/// Muestra una figura de cualquier clase.
pub fn mostrar(ui: &mut egui::Ui, f: &mut Figura, id: egui::Id, alto: f32) -> Option<Accion> {
    match f {
        Figura::D2(g) => g.mostrar(ui, id, alto),
        Figura::D3(g) => g.mostrar(ui, alto),
    }
}

/// Gestiona la exportación que pide una gráfica: elige archivo y escribe el SVG, o pide la captura.
pub fn exportar(ctx: &egui::Context, accion: Accion, fig: &Figura, cola_png: &mut Option<(Rect, std::path::PathBuf)>) {
    let nombre = if fig.titulo().is_empty() { "grafica" } else { fig.titulo() };
    let limpio: String = nombre.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    match accion {
        Accion::Svg => {
            if let Some(r) = rfd::FileDialog::new().add_filter("SVG", &["svg"]).set_file_name(format!("{limpio}.svg")).save_file() {
                let _ = std::fs::write(r, fig.a_svg());
            }
        }
        Accion::Png(rect) => {
            if let Some(r) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name(format!("{limpio}.png")).save_file() {
                *cola_png = Some((rect, r));
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new("figura".to_string())));
            }
        }
    }
}
