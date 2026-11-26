//! Identidad visual FLACS: interfaz monocroma, radio 0 y sin sombras, en tema claro y oscuro.
//! Mismos nombres de color en los dos temas; todo widget lee su color de `p()` (la paleta del
//! tema activo). El color solo aparece en los puntos de estado y en las gráficas.
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
            Modo::Oscuro => "Oscuro",
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
    /// Chip neutro y chip activo (o que identifica un equipo), con su texto.
    pub chip: Color32,
    pub chip_activo: Color32,
    pub sobre_chip_activo: Color32,
    pub series: [Color32; 5],
}

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub const OSCURO: Paleta = Paleta {
    fondo: c(0, 0, 0),              // #000000
    panel: c(18, 18, 18),           // #121212
    campo: c(30, 30, 30),           // #1E1E1E
    nodo: c(18, 18, 18),            // #121212
    realce: c(42, 42, 42),          // #2A2A2A
    linea: c(42, 42, 42),           // #2A2A2A
    borde: c(58, 58, 58),           // #3A3A3A
    linea_fuerte: c(58, 58, 58),    // #3A3A3A
    texto: c(237, 237, 237),        // #EDEDED
    texto_2: c(163, 163, 163),      // #A3A3A3
    texto_3: c(163, 163, 163),      // #A3A3A3
    acento: c(255, 255, 255),       // #FFFFFF
    sobre_acento: c(0, 0, 0),       // #000000
    acento_fondo: c(42, 42, 42),    // #2A2A2A
    campo_alt: c(22, 22, 22),       // #161616
    acento_2: c(237, 237, 237),     // #EDEDED
    senal: c(163, 163, 163),        // #A3A3A3
    electrica: c(237, 237, 237),    // #EDEDED
    ok: c(62, 155, 79),             // #3E9B4F
    alerta: c(229, 103, 92),        // #E5675C
    falla: c(229, 103, 92),         // #E5675C
    falla_fondo: c(58, 26, 24),     // #3A1A18
    rejilla: c(38, 38, 38),         // #262626
    chip: c(58, 58, 58),            // #3A3A3A
    chip_activo: c(217, 217, 217),  // #D9D9D9
    sobre_chip_activo: c(0, 0, 0),  // #000000
    series: [c(242, 169, 59), c(77, 181, 217), c(164, 139, 240), c(60, 194, 177), c(156, 163, 175)],
};

pub const CLARO: Paleta = Paleta {
    fondo: c(255, 255, 255),          // #FFFFFF
    panel: c(255, 255, 255),          // #FFFFFF
    campo: c(242, 242, 242),          // #F2F2F2
    nodo: c(255, 255, 255),           // #FFFFFF
    realce: c(232, 232, 232),         // #E8E8E8
    linea: c(224, 224, 224),          // #E0E0E0
    borde: c(189, 189, 189),          // #BDBDBD
    linea_fuerte: c(189, 189, 189),   // #BDBDBD
    texto: c(26, 26, 26),             // #1A1A1A
    texto_2: c(92, 92, 92),           // #5C5C5C
    texto_3: c(92, 92, 92),           // #5C5C5C
    acento: c(0, 0, 0),               // #000000
    sobre_acento: c(255, 255, 255),   // #FFFFFF
    acento_fondo: c(232, 232, 232),   // #E8E8E8
    campo_alt: c(250, 250, 250),      // #FAFAFA
    acento_2: c(26, 26, 26),          // #1A1A1A
    senal: c(92, 92, 92),             // #5C5C5C
    electrica: c(0, 0, 0),            // #000000
    ok: c(62, 155, 79),               // #3E9B4F
    alerta: c(229, 103, 92),          // #E5675C
    falla: c(229, 103, 92),           // #E5675C
    falla_fondo: c(251, 233, 231),    // #FBE9E7
    rejilla: c(238, 238, 238),        // #EEEEEE
    chip: c(217, 217, 217),           // #D9D9D9
    chip_activo: c(85, 85, 85),       // #555555
    sobre_chip_activo: c(255, 255, 255), // #FFFFFF
    series: [c(201, 122, 6), c(28, 127, 176), c(124, 92, 214), c(15, 140, 126), c(75, 85, 99)],
};

/// Negro de la barra lateral de la portada y de la barra de estado (igual en los dos temas).
pub const NEGRO: Color32 = c(0, 0, 0);

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

/// Radio de botones y campos: el estilo no redondea esquinas.
pub fn radio() -> u8 {
    0
}
/// Radio de ventanas, nodos y tarjetas.
pub fn radio_ventana() -> u8 {
    0
}

/// Radio de las fichas y etiquetas pequeñas.
pub const RADIO_FICHA: u8 = 0;

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
    v.selection.bg_fill = if m == Modo::Claro { c(200, 200, 200) } else { c(80, 80, 80) };
    v.selection.stroke = Stroke::new(1.0_f32, p.acento);
    v.window_stroke = Stroke::new(1.0_f32, p.borde);
    v.window_corner_radius = CornerRadius::ZERO;
    v.menu_corner_radius = CornerRadius::same(radio_ventana());
    v.window_shadow = Shadow::NONE;
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
        s.text_styles.insert(TextStyle::Heading, FontId::new(20.0, FontFamily::Name("negrita".into())));
        s.text_styles.insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(12.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace));
    });
}

/// Roboto (400, 500 y 700) y Roboto Mono (400 y 500), con licencia OFL e incrustadas. Una fuente
/// del sistema queda de respaldo para los símbolos que Roboto no tiene (∫, Σ, ω...).
fn configurar_fuentes(ctx: &egui::Context) {
    let mut f = egui::FontDefinitions::default();
    let fuentes: [(&str, &'static [u8]); 5] = [
        ("roboto", include_bytes!("../fuentes/Roboto-Regular.ttf")),
        ("roboto_medio", include_bytes!("../fuentes/Roboto-Medium.ttf")),
        ("roboto_negrita", include_bytes!("../fuentes/Roboto-Bold.ttf")),
        ("roboto_mono", include_bytes!("../fuentes/RobotoMono-Regular.ttf")),
        ("roboto_mono_medio", include_bytes!("../fuentes/RobotoMono-Medium.ttf")),
    ];
    for (n, d) in fuentes {
        f.font_data.insert(n.into(), egui::FontData::from_static(d).into());
    }
    f.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "roboto".into());
    f.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "roboto_mono".into());
    let mut titulo = vec!["roboto_medio".to_string(), "roboto".to_string()];
    let mut negrita = vec!["roboto_negrita".to_string(), "roboto".to_string()];
    let mut dato = vec!["roboto_mono_medio".to_string(), "roboto_mono".to_string()];
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
            for v in [&mut titulo, &mut negrita, &mut dato] {
                v.push("respaldo".into());
            }
            break;
        }
    }
    // Las fuentes por defecto de egui siguen detrás, como último recurso
    for extra in f.families[&FontFamily::Proportional].clone() {
        for v in [&mut titulo, &mut negrita, &mut dato] {
            if !v.contains(&extra) {
                v.push(extra.clone());
            }
        }
    }
    f.families.insert(FontFamily::Name("titulo".into()), titulo);
    f.families.insert(FontFamily::Name("negrita".into()), negrita);
    f.families.insert(FontFamily::Name("dato".into()), dato);
    ctx.set_fonts(f);
}

/// Familia de peso medio de Roboto (500): títulos de tarjeta, marca, nodos.
pub fn semi() -> FontFamily {
    FontFamily::Name("titulo".into())
}

/// Familia en negrita de Roboto (700): títulos de pantalla y de panel.
pub fn negrita() -> FontFamily {
    FontFamily::Name("negrita".into())
}

pub fn fuente_negrita(tam: f32) -> FontId {
    FontId::new(tam, negrita())
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

/// Única acción primaria por pantalla: negro con texto blanco (blanco con texto negro en tema oscuro), 34 px.
pub fn boton_principal(ui: &mut egui::Ui, t: &str) -> egui::Response {
    let p = p();
    ui.add(egui::Button::new(egui::RichText::new(t).color(p.sobre_acento).family(semi())).fill(p.acento).stroke(Stroke::new(1.0_f32, p.acento)).corner_radius(0).min_size(Vec2::new(0.0, 34.0)))
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

/// El logotipo de `diseno/icono/heliora_icono_pequeno.svg`: sol naciente sobre un panel, en un
/// cuadrado negro (blanco en tema oscuro). Se dibuja en una rejilla de 512 unidades.
pub fn logo(pt: &egui::Painter, r: Rect) {
    let s = r.width().min(r.height()) / 512.0;
    let o = r.center() - Vec2::splat(256.0 * s);
    let q = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let (fondo, trazo) = if modo() == Modo::Claro { (NEGRO, Color32::WHITE) } else { (Color32::WHITE, NEGRO) };
    pt.rect_filled(Rect::from_min_max(q(16.0, 16.0), q(496.0, 496.0)), 0.0, fondo);
    let mut medio_disco = vec![q(160.0, 292.0)];
    medio_disco.extend((0..=24).map(|i| {
        let a = std::f32::consts::PI * (1.0 + i as f32 / 24.0);
        q(256.0 + 96.0 * a.cos(), 292.0 + 96.0 * a.sin())
    }));
    pt.add(egui::Shape::convex_polygon(medio_disco, trazo, Stroke::NONE));
    let rayo = Stroke::new(32.0 * s, trazo);
    for (a, b) in [((157.3, 220.3), (118.5, 192.1)), ((256.0, 170.0), (256.0, 122.0)), ((354.7, 220.3), (393.5, 192.1))] {
        pt.line_segment([q(a.0, a.1), q(b.0, b.1)], rayo);
    }
    let panel = vec![q(170.0, 322.0), q(342.0, 322.0), q(402.0, 420.0), q(110.0, 420.0)];
    pt.add(egui::Shape::closed_line(panel, Stroke::new(26.0 * s, trazo)));
    let fino = Stroke::new(16.0 * s, trazo);
    for (a, b) in [((140.0, 371.0), (372.0, 371.0)), ((227.0, 322.0), (207.0, 420.0)), ((285.0, 322.0), (305.0, 420.0))] {
        pt.line_segment([q(a.0, a.1), q(b.0, b.1)], fino);
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
