//! Identidad visual «Instrumento» (oscuro) con su modo día «Laboratorio» (claro).
//! Mismos nombres de color en los dos modos; todo widget lee su color de `p()` (la paleta del
//! modo activo). Un color, un papel: el acento solo para la acción principal y la selección,
//! señal y eléctrica para cables y puertos, OK / ALERTA / FALLA solo para estados (con forma y texto).
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect, Shadow, Shape, Stroke, TextStyle, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modo {
    Oscuro,
    Claro,
}

impl Modo {
    pub fn clave(self) -> &'static str {
        match self {
            Modo::Oscuro => "oscuro",
            Modo::Claro => "claro",
        }
    }
    pub fn desde(t: &str) -> Modo {
        if t == "claro" {
            Modo::Claro
        } else {
            Modo::Oscuro
        }
    }
}

pub struct Paleta {
    pub fondo: Color32,
    pub panel: Color32,
    pub campo: Color32,
    pub realce: Color32,
    pub linea: Color32,
    pub linea_fuerte: Color32,
    pub texto: Color32,
    pub texto_2: Color32,
    pub texto_3: Color32,
    pub acento: Color32,
    pub sobre_acento: Color32,
    pub acento_2: Color32,
    pub senal: Color32,
    pub electrica: Color32,
    pub ok: Color32,
    pub alerta: Color32,
    pub falla: Color32,
    pub falla_fondo: Color32,
    pub rejilla: Color32,
    pub series: [Color32; 5],
    // Marca y familias de bloques (solo para la franja de cabecera y los cuadrados de categoría)
    pub sol: Color32,
    pub horizonte: Color32,
    pub estructura: Color32,
    pub familias: [Color32; 6],
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const OSCURO: Paleta = Paleta {
    fondo: c(13, 16, 21),           // #0D1015
    panel: c(20, 25, 32),           // #141920
    campo: c(27, 33, 42),           // #1B212A
    realce: c(36, 43, 54),          // #242B36
    linea: c(44, 52, 64),           // #2C3440
    linea_fuerte: c(69, 80, 95),    // #45505F
    texto: c(231, 235, 240),        // #E7EBF0
    texto_2: c(163, 173, 186),      // #A3ADBA
    texto_3: c(123, 134, 150),      // #7B8696
    acento: c(242, 169, 59),        // #F2A93B
    sobre_acento: c(26, 18, 4),     // #1A1204
    acento_2: c(76, 195, 217),      // #4CC3D9
    senal: c(76, 195, 217),         // #4CC3D9
    electrica: c(217, 134, 78),     // #D9864E
    ok: c(69, 196, 130),            // #45C482
    alerta: c(240, 210, 78),        // #F0D24E
    falla: c(242, 90, 95),          // #F25A5F
    falla_fondo: c(58, 23, 25),     // #3A1719
    rejilla: c(42, 50, 61),         // #2A323D
    series: [c(242, 169, 59), c(76, 195, 217), c(179, 157, 240), c(229, 143, 181), c(159, 180, 200)],
    sol: c(242, 169, 59),
    horizonte: c(255, 214, 120),    // #FFD678
    estructura: c(76, 195, 217),
    familias: [c(76, 195, 217), c(217, 134, 78), c(179, 157, 240), c(229, 143, 181), c(159, 180, 200), c(242, 169, 59)],
};

pub const CLARO: Paleta = Paleta {
    fondo: c(247, 248, 250),        // #F7F8FA
    panel: c(255, 255, 255),        // #FFFFFF
    campo: c(242, 244, 247),        // #F2F4F7
    realce: c(231, 234, 239),       // #E7EAEF
    linea: c(211, 216, 223),        // #D3D8DF
    linea_fuerte: c(140, 149, 161), // #8C95A1
    texto: c(17, 21, 27),           // #11151B
    texto_2: c(70, 80, 96),         // #465060
    texto_3: c(95, 104, 118),       // #5F6876
    acento: c(31, 95, 209),         // #1F5FD1
    sobre_acento: c(255, 255, 255),
    acento_2: c(217, 133, 0),       // #D98500 (solo marca)
    senal: c(31, 95, 209),          // #1F5FD1
    electrica: c(181, 82, 15),      // #B5520F
    ok: c(24, 121, 74),             // #18794A
    alerta: c(154, 98, 0),          // #9A6200
    falla: c(196, 43, 43),          // #C42B2B
    falla_fondo: c(251, 228, 228),  // #FBE4E4
    rejilla: c(201, 207, 215),      // #C9CFD7
    series: [c(31, 95, 209), c(217, 119, 6), c(15, 138, 125), c(142, 68, 173), c(91, 100, 114)],
    sol: c(227, 154, 18),           // #E39A12
    horizonte: c(217, 133, 0),
    estructura: c(17, 21, 27),
    familias: [c(31, 95, 209), c(181, 82, 15), c(142, 68, 173), c(15, 138, 125), c(91, 100, 114), c(217, 119, 6)],
};

static CLARO_ACTIVO: AtomicBool = AtomicBool::new(false);

pub fn paleta(m: Modo) -> &'static Paleta {
    match m {
        Modo::Oscuro => &OSCURO,
        Modo::Claro => &CLARO,
    }
}

/// Modo activo (lo fija `aplicar`).
pub fn modo() -> Modo {
    if CLARO_ACTIVO.load(Ordering::Relaxed) {
        Modo::Claro
    } else {
        Modo::Oscuro
    }
}

/// Paleta del modo activo.
pub fn p() -> &'static Paleta {
    paleta(modo())
}

/// Radio de los widgets (2 px; 3 en claro) y de ventanas y tarjetas (4 px; 6 en claro).
pub fn radio() -> u8 {
    if modo() == Modo::Claro {
        3
    } else {
        2
    }
}
pub fn radio_ventana() -> u8 {
    if modo() == Modo::Claro {
        6
    } else {
        4
    }
}

pub fn aplicar(ctx: &egui::Context, m: Modo) {
    CLARO_ACTIVO.store(m == Modo::Claro, Ordering::Relaxed);
    configurar_fuentes(ctx);
    let p = paleta(m);
    let (radio, radio_ventana) = match m {
        Modo::Oscuro => (2, 4),
        Modo::Claro => (3, 6),
    };
    let esq = CornerRadius::same(radio);
    let mut v = if m == Modo::Oscuro { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = Some(p.texto);
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.campo;
    v.faint_bg_color = p.fondo;
    v.code_bg_color = p.fondo;
    v.hyperlink_color = p.acento_2;
    v.selection.bg_fill = p.acento.linear_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, p.acento);
    v.window_stroke = Stroke::new(1.0_f32, p.linea);
    v.window_corner_radius = CornerRadius::same(radio_ventana);
    v.menu_corner_radius = CornerRadius::same(radio_ventana);
    v.window_shadow = match m {
        Modo::Oscuro => Shadow { offset: [0, 2], blur: 8, spread: 0, color: Color32::from_black_alpha(102) },
        Modo::Claro => Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_rgba_unmultiplied(16, 24, 40, 31) },
    };
    v.popup_shadow = v.window_shadow;
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.linea);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.texto_2);
    w.inactive.bg_fill = p.campo;
    w.inactive.weak_bg_fill = p.campo;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, p.linea);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, p.texto);
    w.hovered.bg_fill = p.realce;
    w.hovered.weak_bg_fill = p.realce;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, p.linea_fuerte);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, p.texto);
    w.active.bg_fill = p.realce;
    w.active.weak_bg_fill = p.realce;
    w.active.bg_stroke = Stroke::new(1.0_f32, p.acento);
    w.active.fg_stroke = Stroke::new(1.0_f32, p.texto);
    w.open = w.hovered;
    for e in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        e.corner_radius = esq;
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        let (is, bp, iy, mg, base) = match m {
            Modo::Oscuro => (Vec2::new(6.0, 4.0), Vec2::new(8.0, 3.0), 22.0, 8, 13.0),
            Modo::Claro => (Vec2::new(8.0, 6.0), Vec2::new(10.0, 4.0), 26.0, 12, 14.0),
        };
        s.spacing.item_spacing = is;
        s.spacing.button_padding = bp;
        s.spacing.interact_size.y = iy;
        s.spacing.window_margin = Margin::same(mg);
        s.text_styles.insert(TextStyle::Heading, FontId::new(base + 7.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Body, FontId::new(base, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(base, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(base - 1.5, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(base - 0.5, FontFamily::Monospace));
    });
}

/// IBM Plex Sans + IBM Plex Mono (OFL), incrustadas. Una fuente del sistema queda de respaldo
/// para los símbolos que Plex no tiene (∫, Σ, ω...).
fn configurar_fuentes(ctx: &egui::Context) {
    let mut f = egui::FontDefinitions::default();
    let fuentes: [(&str, &'static [u8]); 4] = [
        ("plex", include_bytes!("../fuentes/IBMPlexSans-Regular.ttf")),
        ("plex_semi", include_bytes!("../fuentes/IBMPlexSans-SemiBold.ttf")),
        ("plex_mono", include_bytes!("../fuentes/IBMPlexMono-Regular.ttf")),
        ("plex_mono_med", include_bytes!("../fuentes/IBMPlexMono-Medium.ttf")),
    ];
    for (n, d) in fuentes {
        f.font_data.insert(n.into(), egui::FontData::from_static(d).into());
    }
    f.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "plex".into());
    f.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "plex_mono".into());
    let mut titulo = vec!["plex_semi".to_string(), "plex".to_string()];
    let mut dato = vec!["plex_mono_med".to_string(), "plex_mono".to_string()];
    for ruta in [
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/STIXGeneral.otf",
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ] {
        if let Ok(datos) = std::fs::read(ruta) {
            f.font_data.insert("respaldo".into(), egui::FontData::from_owned(datos).into());
            f.families.get_mut(&FontFamily::Proportional).unwrap().push("respaldo".into());
            f.families.get_mut(&FontFamily::Monospace).unwrap().push("respaldo".into());
            titulo.push("respaldo".into());
            dato.push("respaldo".into());
            break;
        }
    }
    // Las fuentes por defecto de egui siguen detrás, como último recurso
    for extra in f.families[&FontFamily::Proportional].clone() {
        if !titulo.contains(&extra) {
            titulo.push(extra.clone());
        }
        if !dato.contains(&extra) {
            dato.push(extra);
        }
    }
    f.families.insert(FontFamily::Name("titulo".into()), titulo);
    f.families.insert(FontFamily::Name("dato".into()), dato);
    ctx.set_fonts(f);
}

/// Familia semibold de Plex Sans (títulos, marca, nodos).
pub fn semi() -> FontFamily {
    FontFamily::Name("titulo".into())
}

pub fn fuente_semi(tam: f32) -> FontId {
    FontId::new(tam, semi())
}

pub fn fuente_mono(tam: f32) -> FontId {
    FontId::new(tam, FontFamily::Monospace)
}

/// Título de panel: 11 px, semibold, mayúsculas, TEXTO_2 (en claro, 13 px y TEXTO).
pub fn titulo(ui: &mut egui::Ui, t: &str) {
    let m = modo();
    let p = paleta(m);
    let txt = if m == Modo::Oscuro { t.to_uppercase() } else { t.to_string() };
    let size = if m == Modo::Oscuro { 11.0 } else { 13.0 };
    ui.label(egui::RichText::new(txt).family(semi()).size(size).color(if m == Modo::Oscuro { p.texto_2 } else { p.texto }));
}

/// Única acción primaria por pantalla (p. ej. «Simular»).
pub fn boton_principal(ui: &mut egui::Ui, t: &str) -> egui::Response {
    let p = p();
    ui.add(egui::Button::new(egui::RichText::new(t).color(p.sobre_acento).family(semi())).fill(p.acento).stroke(Stroke::new(1.0_f32, p.acento)))
}

/// Grosor de cable del lienzo: eléctrico 2 px, señal 1,25 px; en falla, 1,75 px discontinuo.
pub fn trazo_cable(electrico: bool, en_falla: bool) -> Stroke {
    let p = p();
    if en_falla {
        return Stroke::new(1.75_f32, p.falla);
    }
    if electrico {
        Stroke::new(2.0_f32, p.electrica)
    } else {
        Stroke::new(1.25_f32, p.senal)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nivel {
    Ok,
    Alerta,
    Falla,
}

impl Nivel {
    pub fn desde(t: &str) -> Nivel {
        match t {
            "falla" | "error" | "mal" => Nivel::Falla,
            "alerta" | "aviso" => Nivel::Alerta,
            _ => Nivel::Ok,
        }
    }
    pub fn color(self) -> Color32 {
        match self {
            Nivel::Ok => p().ok,
            Nivel::Alerta => p().alerta,
            Nivel::Falla => p().falla,
        }
    }
}

/// Estado con forma y texto: un cuadrado de 7 px del color del estado y su etiqueta.
pub fn estado(ui: &mut egui::Ui, n: Nivel, texto: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (r, _) = ui.allocate_exact_size(Vec2::new(7.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(Rect::from_center_size(r.center(), Vec2::splat(7.0)), 0.0, n.color());
        ui.label(egui::RichText::new(texto).color(p().texto));
    })
    .response
}

/// Cuadrado de 7 px (estado o color de familia) pintado en un punto.
pub fn cuadrado(pt: &egui::Painter, centro: Pos2, color: Color32) {
    pt.rect_filled(Rect::from_center_size(centro, Vec2::splat(7.0)), 0.0, color);
}

/// El logotipo: un sol que sale sobre la estructura de un panel.
pub fn logo(pt: &egui::Painter, r: Rect) {
    let p = p();
    let c = r.center();
    let s = r.width().min(r.height()) / 2.0;
    let centro = Pos2::new(c.x, c.y + s * 0.18);
    for i in 0..7 {
        let a = std::f32::consts::PI * (1.0 + i as f32 / 6.0);
        let d = Vec2::new(a.cos(), a.sin());
        pt.line_segment([centro + d * s * 0.62, centro + d * s * 0.95], Stroke::new(s * 0.13, p.sol));
    }
    let mut pts = vec![];
    for i in 0..=20 {
        let a = std::f32::consts::PI * (1.0 + i as f32 / 20.0);
        pts.push(centro + Vec2::new(a.cos(), a.sin()) * s * 0.48);
    }
    pt.add(Shape::convex_polygon(pts, p.sol, Stroke::NONE));
    let base = centro.y + s * 0.08;
    pt.line_segment([Pos2::new(c.x - s * 0.95, base), Pos2::new(c.x + s * 0.95, base)], Stroke::new(s * 0.1, p.horizonte));
    for i in 0..3 {
        let x = c.x + (i as f32 - 1.0) * s * 0.55;
        pt.line_segment([Pos2::new(x, base), Pos2::new(x + (i as f32 - 1.0) * s * 0.3, base + s * 0.5)], Stroke::new(s * 0.07, p.estructura));
    }
    pt.line_segment([Pos2::new(c.x - s * 0.85, base + s * 0.5), Pos2::new(c.x + s * 0.85, base + s * 0.5)], Stroke::new(s * 0.07, p.estructura));
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modulo {
    Inicio,
    Calculo,
    Bloques,
    Potencia,
    Fotovoltaica,
    Estadistica,
    Aprendizaje,
    Mantenimiento,
    Validacion,
    Referencia,
}

pub const MODULOS: [(Modulo, &str); 10] = [
    (Modulo::Inicio, "Inicio"),
    (Modulo::Calculo, "Cálculo"),
    (Modulo::Bloques, "Bloques"),
    (Modulo::Potencia, "Potencia"),
    (Modulo::Fotovoltaica, "Solar"),
    (Modulo::Estadistica, "Estadística"),
    (Modulo::Aprendizaje, "Aprendizaje"),
    (Modulo::Mantenimiento, "Mantenim."),
    (Modulo::Validacion, "Validación"),
    (Modulo::Referencia, "Referencia"),
];

/// Iconos de la barra lateral: trazo uniforme de 1,5 px con extremos redondeados.
pub fn icono(pt: &egui::Painter, r: Rect, m: Modulo, color: Color32) {
    let c = r.center();
    let s = r.width().min(r.height()) / 2.0;
    let t = Stroke::new(1.5_f32, color);
    let q = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    // Segmento con extremos redondeados: la línea y un punto de radio w/2 en cada extremo
    let seg = |a: Pos2, b: Pos2| {
        pt.line_segment([a, b], t);
        pt.circle_filled(a, 0.75, color);
        pt.circle_filled(b, 0.75, color);
    };
    let caja = |a: Pos2, b: Pos2, radio: f32| {
        pt.rect_stroke(Rect::from_min_max(a, b), radio, t, egui::StrokeKind::Middle);
    };
    match m {
        Modulo::Inicio => {
            seg(q(-0.8, 0.0), q(0.0, -0.8));
            seg(q(0.0, -0.8), q(0.8, 0.0));
            caja(q(-0.55, 0.0), q(0.55, 0.8), 1.0);
        }
        Modulo::Calculo => {
            caja(q(-0.85, -0.7), q(0.85, 0.7), 2.0);
            seg(q(-0.55, -0.3), q(-0.15, 0.0));
            seg(q(-0.15, 0.0), q(-0.55, 0.3));
            seg(q(0.05, 0.35), q(0.55, 0.35));
        }
        Modulo::Bloques => {
            caja(q(-0.9, -0.75), q(-0.15, -0.1), 1.5);
            caja(q(0.15, 0.1), q(0.9, 0.75), 1.5);
            seg(q(-0.5, -0.1), q(-0.5, 0.42));
            seg(q(-0.5, 0.42), q(0.15, 0.42));
        }
        Modulo::Potencia => {
            let puntos = [q(0.15, -0.9), q(-0.45, 0.1), q(0.0, 0.1), q(-0.15, 0.9), q(0.5, -0.2), q(0.05, -0.2), q(0.15, -0.9)];
            for w in puntos.windows(2) {
                seg(w[0], w[1]);
            }
        }
        Modulo::Fotovoltaica => {
            pt.circle_stroke(q(0.0, 0.0), s * 0.4, t);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                seg(q(a.cos() * 0.6, a.sin() * 0.6), q(a.cos() * 0.9, a.sin() * 0.9));
            }
        }
        Modulo::Estadistica => {
            for (i, a) in [0.35, 0.8, 0.5, 0.2].iter().enumerate() {
                let x = -0.8 + i as f32 * 0.42;
                caja(q(x, 0.8 - a * 1.6), q(x + 0.3, 0.8), 0.5);
            }
        }
        Modulo::Aprendizaje => {
            for (x, y) in [(-0.7, -0.6), (-0.7, 0.6), (0.0, 0.0), (0.7, -0.6), (0.7, 0.6)] {
                pt.circle_stroke(q(x, y), s * 0.17, t);
            }
            for (x, y) in [(-0.7, -0.6), (-0.7, 0.6), (0.7, -0.6), (0.7, 0.6)] {
                seg(q(x * 0.75, y * 0.75), q(x * 0.25, y * 0.25));
            }
        }
        Modulo::Mantenimiento => {
            caja(q(-0.8, -0.6), q(0.8, 0.8), 1.5);
            seg(q(-0.8, -0.2), q(0.8, -0.2));
            seg(q(-0.4, -0.85), q(-0.4, -0.4));
            seg(q(0.4, -0.85), q(0.4, -0.4));
            seg(q(-0.4, 0.3), q(-0.1, 0.55));
            seg(q(-0.1, 0.55), q(0.4, 0.05));
        }
        Modulo::Validacion => {
            pt.circle_stroke(q(0.0, 0.0), s * 0.85, t);
            seg(q(-0.4, 0.05), q(-0.1, 0.38));
            seg(q(-0.1, 0.38), q(0.45, -0.35));
        }
        Modulo::Referencia => {
            caja(q(-0.7, -0.85), q(0.7, 0.85), 1.5);
            seg(q(-0.4, -0.4), q(0.4, -0.4));
            seg(q(-0.4, 0.0), q(0.4, 0.0));
            seg(q(-0.4, 0.4), q(0.15, 0.4));
        }
    }
}

/// Un grupo con superficie PANEL, borde LINEA de 1 px y radio de ventana.
pub fn tarjeta<R>(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = p();
    egui::Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0_f32, p.linea))
        .corner_radius(CornerRadius::same(radio_ventana()))
        .inner_margin(Margin::same(12))
        .show(ui, f)
        .inner
}

/// Panel acoplable: superficie PANEL.
pub fn marco_panel() -> egui::Frame {
    egui::Frame::new().fill(p().panel).inner_margin(Margin::same(10))
}

/// Superficie de trabajo: FONDO (lienzo, gráficas).
pub fn marco_fondo() -> egui::Frame {
    egui::Frame::new().fill(p().fondo).inner_margin(Margin::same(10))
}

/// Separador de 1 px en LINEA a lo ancho del espacio disponible.
pub fn separador(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 0.0, p().linea);
}

/// Color escrito como #RRGGBB (las series que el motor pide con un color concreto).
pub fn color_hex(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(c((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// Mapa de colores de las superficies 3D: de azul profundo a cian, verde claro y ámbar.
pub fn mapa_color(t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let puntos: [(f32, [f32; 3]); 4] = [(0.0, [22.0, 52.0, 92.0]), (0.4, [30.0, 150.0, 170.0]), (0.72, [180.0, 205.0, 100.0]), (1.0, [242.0, 169.0, 59.0])];
    for w in puntos.windows(2) {
        if t <= w[1].0 {
            let u = (t - w[0].0) / (w[1].0 - w[0].0);
            let m = |k: usize| (w[0].1[k] + (w[1].1[k] - w[0].1[k]) * u) as u8;
            return c(m(0), m(1), m(2));
        }
    }
    c(242, 169, 59)
}

/// Color de familia de una categoría de bloques (para su cuadrado de 7 px y la franja del nodo).
pub fn color_familia(categoria: &str) -> Color32 {
    let f = &p().familias;
    let c = categoria.to_lowercase();
    if c.starts_with("eléctrica") || c.contains("convertidores") || c.contains("filtros") {
        f[1]
    } else if c.contains("fotovolta") || c.contains("solar") {
        f[5]
    } else if c.contains("potencia") {
        f[2]
    } else if c.contains("fuentes") || c.contains("sumideros") {
        f[3]
    } else if c.contains("continuo") || c.contains("discreto") {
        f[0]
    } else {
        f[4]
    }
}
