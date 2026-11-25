//! Identidad visual «Sala de control»: modo oscuro «Instrumento» y modo claro «Laboratorio».
//! Mismos nombres de color en los dos modos; todo widget lee su color de `p()` (la paleta del
//! modo activo). Un color, un papel: el ámbar para la acción principal, la selección y los cables
//! eléctricos; el azul para las señales; OK / ALERTA / FALLA solo para estados (con forma y texto).
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect, Shadow, Stroke, TextStyle, Vec2};
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
    /// Nombre del modo en la interfaz.
    pub fn nombre(self) -> &'static str {
        match self {
            Modo::Oscuro => "Instrumento",
            Modo::Claro => "Claro",
        }
    }
}

pub struct Paleta {
    /// Lienzo y superficies de trabajo.
    pub fondo: Color32,
    /// Barras, paneles laterales y dock.
    pub panel: Color32,
    /// Campos destacados (la píldora del solver, filas activas).
    pub campo: Color32,
    /// Cuerpo de los nodos del lienzo y de las tarjetas.
    pub nodo: Color32,
    /// Fondo al pasar el puntero.
    pub realce: Color32,
    /// Separadores entre zonas.
    pub linea: Color32,
    /// Borde de botones y campos.
    pub borde: Color32,
    /// Borde de nodos y bordes al pasar el puntero.
    pub linea_fuerte: Color32,
    pub texto: Color32,
    pub texto_2: Color32,
    pub texto_3: Color32,
    pub acento: Color32,
    pub sobre_acento: Color32,
    /// Fondo del módulo activo en el riel y de los elementos seleccionados.
    pub acento_fondo: Color32,
    /// Texto sobre `acento_fondo` (pestaña lateral o fila activa).
    pub acento_texto: Color32,
    /// Fondo secundario muy suave: filas alternas de las tablas.
    pub campo_alt: Color32,
    /// Enlaces.
    pub acento_2: Color32,
    pub senal: Color32,
    pub electrica: Color32,
    pub ok: Color32,
    pub alerta: Color32,
    pub falla: Color32,
    pub falla_fondo: Color32,
    pub rejilla: Color32,
    pub series: [Color32; 5],
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const OSCURO: Paleta = Paleta {
    fondo: c(11, 13, 16),           // #0B0D10
    panel: c(14, 17, 20),           // #0E1114
    campo: c(17, 21, 26),           // #11151A
    nodo: c(20, 24, 29),            // #14181D
    realce: c(23, 27, 33),          // #171B21
    linea: c(30, 35, 42),           // #1E232A
    borde: c(38, 43, 51),           // #262B33
    linea_fuerte: c(42, 48, 57),    // #2A3039
    texto: c(228, 231, 236),        // #E4E7EC
    texto_2: c(152, 160, 172),      // #98A0AC
    texto_3: c(125, 133, 146),      // #7D8592
    acento: c(242, 169, 59),        // #F2A93B
    sobre_acento: c(26, 18, 5),     // #1A1205
    acento_fondo: c(26, 23, 18),    // #1A1712
    acento_texto: c(255, 196, 107), // #FFC46B
    campo_alt: c(16, 19, 23),       // #101317
    acento_2: c(255, 196, 107),     // #FFC46B
    senal: c(77, 181, 217),         // #4DB5D9
    electrica: c(242, 169, 59),     // #F2A93B
    ok: c(63, 178, 127),            // #3FB27F
    alerta: c(240, 210, 78),        // #F0D24E
    falla: c(242, 90, 95),          // #F25A5F
    falla_fondo: c(58, 23, 25),     // #3A1719
    rejilla: c(28, 33, 40),         // #1C2128
    series: [c(242, 169, 59), c(77, 181, 217), c(167, 139, 250), c(63, 191, 174), c(159, 180, 200)],
};

pub const CLARO: Paleta = Paleta {
    fondo: c(243, 244, 246),        // #F3F4F6
    panel: c(255, 255, 255),        // #FFFFFF
    campo: c(246, 247, 249),        // #F6F7F9
    nodo: c(255, 255, 255),         // #FFFFFF
    realce: c(238, 240, 243),       // #EEF0F3
    linea: c(227, 230, 234),        // #E3E6EA
    borde: c(213, 217, 223),        // #D5D9DF
    linea_fuerte: c(205, 210, 217), // #CDD2D9
    texto: c(27, 32, 40),           // #1B2028
    texto_2: c(95, 104, 117),       // #5F6875
    texto_3: c(95, 104, 117),       // #5F6875
    acento: c(201, 122, 6),         // #C97A06
    sobre_acento: c(26, 18, 5),     // #1A1205
    acento_fondo: c(253, 241, 220), // #FDF1DC
    acento_texto: c(122, 74, 0),    // #7A4A00
    campo_alt: c(251, 251, 252),    // #FBFBFC
    acento_2: c(154, 93, 0),        // #9A5D00
    senal: c(28, 127, 176),         // #1C7FB0
    electrica: c(201, 122, 6),      // #C97A06
    ok: c(30, 158, 99),             // #1E9E63
    alerta: c(154, 98, 0),          // #9A6200
    falla: c(196, 43, 43),          // #C42B2B
    falla_fondo: c(251, 228, 228),  // #FBE4E4
    rejilla: c(211, 216, 222),      // #D3D8DE
    series: [c(201, 122, 6), c(28, 127, 176), c(124, 92, 214), c(15, 140, 126), c(91, 100, 114)],
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

/// Radio de botones y campos (6 px) y de ventanas, nodos y tarjetas (8 px).
pub fn radio() -> u8 {
    6
}
pub fn radio_ventana() -> u8 {
    8
}

/// Radio de las fichas y etiquetas pequeñas (4 px).
pub const RADIO_FICHA: u8 = 4;

/// Color de un tipo de orden de mantenimiento: limpieza, inspección, ensayo y mantenimiento anual
/// usan las series 1 a 4; cualquier otro tipo, la quinta.
pub fn color_tipo_orden(tipo: &str) -> Color32 {
    let s = &p().series;
    let t = tipo.to_lowercase();
    if t.contains("limpieza") {
        s[0]
    } else if t.contains("inspecc") || t.contains("termograf") {
        s[1]
    } else if t.contains("ensayo") || t.contains("curva") {
        s[2]
    } else if t.contains("anual") {
        s[3]
    } else {
        s[4]
    }
}

pub fn aplicar(ctx: &egui::Context, m: Modo) {
    CLARO_ACTIVO.store(m == Modo::Claro, Ordering::Relaxed);
    configurar_fuentes(ctx);
    let p = paleta(m);
    let esq = CornerRadius::same(radio());
    let mut v = if m == Modo::Oscuro { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = Some(p.texto);
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.panel;
    v.faint_bg_color = p.campo;
    v.code_bg_color = p.fondo;
    v.hyperlink_color = p.acento_2;
    v.selection.bg_fill = p.acento.linear_multiply(0.30);
    v.selection.stroke = Stroke::new(1.0_f32, p.acento);
    v.window_stroke = Stroke::new(1.0_f32, p.borde);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(radio_ventana());
    v.window_shadow = match m {
        Modo::Oscuro => Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(120) },
        Modo::Claro => Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_rgba_unmultiplied(16, 24, 40, 28) },
    };
    v.popup_shadow = v.window_shadow;
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.linea);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.texto_2);
    // Botones transparentes con borde, como en el diseño; al pasar el puntero, fondo REALCE
    w.inactive.bg_fill = p.panel;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, p.borde);
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
        s.spacing.item_spacing = Vec2::new(6.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 4.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.window_margin = Margin::same(12);
        s.text_styles.insert(TextStyle::Heading, FontId::new(18.0, FontFamily::Name("titulo".into())));
        s.text_styles.insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(11.5, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace));
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

/// Título de sección: 11 px, semibold, mayúsculas con espaciado fino, en TEXTO_3.
pub fn titulo(ui: &mut egui::Ui, t: &str) {
    let txt: String = t.to_uppercase().chars().flat_map(|c| [c, '\u{200A}']).collect();
    ui.label(egui::RichText::new(txt.trim_end()).family(semi()).size(11.0).color(p().texto_3));
}

/// Única acción primaria por pantalla (p. ej. «Simular»).
pub fn boton_principal(ui: &mut egui::Ui, t: &str) -> egui::Response {
    let p = p();
    ui.add(egui::Button::new(egui::RichText::new(t).color(p.sobre_acento).family(semi())).fill(p.acento).stroke(Stroke::new(1.0_f32, p.acento)))
}

/// Grosor de cable del lienzo: eléctrico 2 px, señal 1,5 px; en falla, 1,75 px discontinuo.
pub fn trazo_cable(electrico: bool, en_falla: bool) -> Stroke {
    let p = p();
    if en_falla {
        return Stroke::new(1.75_f32, p.falla);
    }
    if electrico {
        Stroke::new(2.0_f32, p.electrica)
    } else {
        Stroke::new(1.5_f32, p.senal)
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

/// Estado con forma y texto: un punto de 7 px del color del estado y su etiqueta.
pub fn estado(ui: &mut egui::Ui, n: Nivel, texto: &str) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let (r, _) = ui.allocate_exact_size(Vec2::new(7.0, 14.0), egui::Sense::hover());
        punto(ui.painter(), r.center(), n.color());
        ui.label(egui::RichText::new(texto).color(p().texto));
    })
    .response
}

/// Punto de estado de 7 px.
pub fn punto(pt: &egui::Painter, centro: Pos2, color: Color32) {
    pt.circle_filled(centro, 3.5, color);
}

/// Cuadrado de 8 px con esquinas de 2 px (familia de un bloque: eléctrica o señal).
pub fn cuadrado(pt: &egui::Painter, centro: Pos2, color: Color32) {
    pt.rect_filled(Rect::from_center_size(centro, Vec2::splat(8.0)), 2.0, color);
}

/// El logotipo: un sol que sale sobre el horizonte, en trazo de ACENTO (rejilla de 26 unidades).
pub fn logo(pt: &egui::Painter, r: Rect) {
    let s = r.width().min(r.height()) / 26.0;
    let o = r.center() - Vec2::splat(13.0 * s);
    let q = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let t = Stroke::new(1.6 * s, p().acento);
    let arco: Vec<Pos2> = (0..=24)
        .map(|i| {
            let a = std::f32::consts::PI * (1.0 + i as f32 / 24.0);
            q(13.0 + 9.0 * a.cos(), 18.0 + 9.0 * a.sin())
        })
        .collect();
    pt.add(egui::Shape::line(arco, t));
    for (a, b) in [((13.0, 4.0), (13.0, 7.0)), ((5.5, 7.5), (7.5, 9.5)), ((20.5, 7.5), (18.5, 9.5)), ((2.0, 18.0), (24.0, 18.0)), ((6.0, 22.0), (20.0, 22.0))] {
        pt.line_segment([q(a.0, a.1), q(b.0, b.1)], t);
        pt.circle_filled(q(a.0, a.1), 0.8 * s, t.color);
        pt.circle_filled(q(b.0, b.1), 0.8 * s, t.color);
    }
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

/// Módulos del riel, por grupos: inicio; modelado; análisis; operación; consulta.
pub const MODULOS: [&[(Modulo, &str)]; 5] = [
    &[(Modulo::Inicio, "Inicio")],
    &[(Modulo::Bloques, "Bloques"), (Modulo::Potencia, "Potencia"), (Modulo::Fotovoltaica, "Solar")],
    &[(Modulo::Calculo, "Cálculo"), (Modulo::Estadistica, "Estadística"), (Modulo::Aprendizaje, "Aprendizaje")],
    &[(Modulo::Mantenimiento, "Mantenimiento"), (Modulo::Validacion, "Validación")],
    &[(Modulo::Referencia, "Referencia")],
];

/// Nombre de un módulo en la interfaz.
pub fn nombre_modulo(m: Modulo) -> &'static str {
    MODULOS.iter().flat_map(|g| g.iter()).find(|(x, _)| *x == m).map(|(_, n)| *n).unwrap_or("")
}

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

/// Icono de ajustes: un engranaje esquemático.
pub fn icono_ajustes(pt: &egui::Painter, r: Rect, color: Color32) {
    let c = r.center();
    let s = r.width().min(r.height()) / 2.0;
    let t = Stroke::new(1.5_f32, color);
    pt.circle_stroke(c, s * 0.28, t);
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::FRAC_PI_4;
        let d = Vec2::new(a.cos(), a.sin());
        pt.line_segment([c + d * s * 0.55, c + d * s * 0.9], t);
    }
}

/// Un grupo con superficie PANEL, borde LINEA de 1 px y radio de ventana.
pub fn tarjeta<R>(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = p();
    egui::Frame::new()
        .fill(p.nodo)
        .stroke(Stroke::new(1.0_f32, p.linea_fuerte))
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
