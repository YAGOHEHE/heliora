//! Iconos de trazo de 1,6 px sin relleno: los SVG en línea de los `.html` de `diseno/flacs/`, con el
//! mismo trazado. Se rasterizan con el cargador SVG de `egui_extras` al tamaño exacto en píxeles y se
//! tiñen del color pedido (el SVG se dibuja en blanco para que el tinte dé el color final).
use egui::{Color32, Painter, Rect, TextureOptions};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icono {
    Inicio,
    Nuevo,
    NuevoDocumento,
    Abrir,
    Carpeta,
    Guardar,
    GuardarComo,
    Deshacer,
    Rehacer,
    Encuadrar,
    Codigo,
    Subsistema,
    Bloques,
    Diagrama,
    OrdenarCables,
    Simular,
    SimularLleno,
    Detener,
    Solver,
    Linealizar,
    Osciloscopio,
    Descargar,
    Importar,
    Imagen,
    Informe,
    Calcular,
    Documento,
    Orden,
    Tabla,
    Datos,
    Buscar,
    Ajustes,
    Ayuda,
    Cuenta,
    Referencia,
    Rayo,
    Onda,
    Bode,
    Curva,
    Escalon,
    Raices,
    Pi,
    Sol,
    Sombra,
    Brujula,
    Histograma,
    Grafica,
    Dispersion,
    Distribucion,
    Red,
    Clasificacion,
    Clustering,
    Pca,
    Anomalias,
    Calendario,
    Lista,
    Equipos,
    Amef,
    Historial,
    Validar,
    Correcto,
    Aviso,
    Filtro,
    Cerrar,
    Borrar,
    Flecha,
    Anterior,
    Siguiente,
    Plegar,
    Desplegar,
    Terminal,
    Variable,
    Mas,
}

/// Todas las variantes, para registrar sus SVG al arrancar.
const TODOS: [Icono; 73] = [
    Icono::Inicio, Icono::Nuevo, Icono::NuevoDocumento, Icono::Abrir, Icono::Carpeta, Icono::Guardar, Icono::GuardarComo,
    Icono::Deshacer, Icono::Rehacer, Icono::Encuadrar, Icono::Codigo, Icono::Subsistema, Icono::Bloques, Icono::Diagrama,
    Icono::OrdenarCables, Icono::Simular, Icono::SimularLleno, Icono::Detener, Icono::Solver, Icono::Linealizar,
    Icono::Osciloscopio, Icono::Descargar, Icono::Importar, Icono::Imagen, Icono::Informe, Icono::Calcular, Icono::Documento,
    Icono::Orden, Icono::Tabla, Icono::Datos, Icono::Buscar, Icono::Ajustes, Icono::Ayuda, Icono::Cuenta, Icono::Referencia,
    Icono::Rayo, Icono::Onda, Icono::Bode, Icono::Curva, Icono::Escalon, Icono::Raices, Icono::Pi, Icono::Sol, Icono::Sombra,
    Icono::Brujula, Icono::Histograma, Icono::Grafica, Icono::Dispersion, Icono::Distribucion, Icono::Red,
    Icono::Clasificacion, Icono::Clustering, Icono::Pca, Icono::Anomalias, Icono::Calendario, Icono::Lista, Icono::Equipos,
    Icono::Amef, Icono::Historial, Icono::Validar, Icono::Correcto, Icono::Aviso, Icono::Filtro, Icono::Cerrar, Icono::Borrar,
    Icono::Flecha, Icono::Anterior, Icono::Siguiente, Icono::Plegar, Icono::Desplegar, Icono::Terminal,
    Icono::Variable, Icono::Mas,
];

/// Trazado SVG del icono en una caja de 24 × 24, copiado de los `.html` del diseño. Los pocos que
/// el diseño no incluye (detener, desplegar, variable, más) siguen su mismo estilo.
fn trazado(i: Icono) -> &'static str {
    match i {
        Icono::Inicio => r#"<path d="M4 11l8-7 8 7"/><path d="M6 10v10h12V10"/>"#,
        Icono::Nuevo => r#"<circle cx="12" cy="12" r="8.5"/><path d="M12 8v8M8 12h8"/>"#,
        Icono::NuevoDocumento => r#"<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4M12 11v6M9 14h6"/>"#,
        Icono::Abrir | Icono::Carpeta => r#"<path d="M3 7h6l2 2h10v10H3z"/>"#,
        Icono::Guardar | Icono::GuardarComo => r#"<path d="M4 4h13l3 3v13H4z"/><path d="M8 4v5h8V4M7 20v-6h10v6"/>"#,
        Icono::Deshacer => r#"<path d="M9 7L4 12l5 5"/><path d="M4 12h11a5 5 0 0 1 0 10h-2"/>"#,
        Icono::Rehacer => r#"<path d="M15 7l5 5-5 5"/><path d="M20 12H9a5 5 0 0 0 0 10h2"/>"#,
        Icono::Encuadrar => r#"<path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/>"#,
        Icono::Codigo => r#"<path d="M8 7l-5 5 5 5M16 7l5 5-5 5"/>"#,
        Icono::Subsistema | Icono::Bloques | Icono::Diagrama => r#"<rect x="3" y="4" width="7" height="6"/><rect x="14" y="14" width="7" height="6"/><path d="M10 7h3v10h1"/>"#,
        Icono::OrdenarCables => r#"<path d="M3 6h6v12h6M15 6h6M3 18h3"/><circle cx="15" cy="18" r="1.5"/>"#,
        Icono::Simular => r#"<path d="M7 4l13 8-13 8z"/>"#,
        Icono::SimularLleno => r#"<path d="M7 4l13 8-13 8z" fill="currentColor"/>"#,
        Icono::Detener => r#"<rect x="6" y="6" width="12" height="12"/>"#,
        Icono::Solver => r#"<circle cx="6" cy="12" r="2.5"/><circle cx="18" cy="12" r="2.5"/><path d="M8.5 12h7M6 5v4.5M18 14.5V19"/>"#,
        Icono::Linealizar => r#"<path d="M3 20L21 4M3 12h6M15 12h6"/>"#,
        Icono::Osciloscopio => r#"<rect x="3" y="4" width="18" height="14"/><path d="M5 13l3-4 3 6 3-8 3 6 2-2M8 21h8"/>"#,
        Icono::Descargar => r#"<path d="M12 4v11M7 10l5 5 5-5M4 20h16"/>"#,
        Icono::Importar => r#"<path d="M6 3h8l4 4v14H6z"/><path d="M12 10v7M9 14l3 3 3-3"/>"#,
        Icono::Imagen => r#"<rect x="3" y="5" width="18" height="14"/><circle cx="9" cy="10" r="1.5"/><path d="M21 16l-5-5-8 8"/>"#,
        Icono::Informe => r#"<path d="M6 3h12v18H6z"/><path d="M9 8h6M9 12h6M9 16h3"/>"#,
        Icono::Calcular => r#"<rect x="5" y="3" width="14" height="18"/><path d="M8 7h8M8 12h2M12 12h2M16 12h0M8 16h2M12 16h2"/>"#,
        Icono::Documento => r#"<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/>"#,
        Icono::Orden => r#"<rect x="5" y="3" width="14" height="18"/><path d="M9 3v2h6V3M8 10h8M8 14h8M8 18h5"/>"#,
        Icono::Tabla => r#"<rect x="3" y="4" width="18" height="16"/><path d="M3 9h18M3 14h18M9 4v16"/>"#,
        Icono::Datos => r#"<ellipse cx="12" cy="6" rx="8" ry="3"/><path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3"/>"#,
        Icono::Buscar => r#"<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>"#,
        Icono::Ajustes => r#"<circle cx="12" cy="12" r="3"/><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1"/>"#,
        Icono::Ayuda => r#"<circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .9-1 1.7v.5"/><circle cx="12" cy="17.2" r=".6" fill="currentColor"/>"#,
        Icono::Cuenta => r#"<circle cx="12" cy="8" r="4"/><path d="M4 21c1-4 4-6 8-6s7 2 8 6"/>"#,
        Icono::Referencia => r#"<path d="M5 4h11a3 3 0 0 1 3 3v13H8a3 3 0 0 1-3-3z"/><path d="M5 17a3 3 0 0 1 3-3h11"/>"#,
        Icono::Rayo => r#"<path d="M13 3L5 14h6l-1 7 8-11h-6z"/>"#,
        Icono::Onda => r#"<path d="M2 12c3-8 5-8 7 0s4 8 7 0 4-8 6 0"/>"#,
        Icono::Bode | Icono::Curva => r#"<path d="M3 20c4-14 8-16 18-16"/><path d="M3 4v16h18"/>"#,
        Icono::Escalon => r#"<path d="M3 18h5V8h13"/>"#,
        Icono::Raices => r#"<path d="M3 12h18M12 3v18"/><path d="M7 8l2 2M7 16l2-2M17 8l-2 2M17 16l-2-2"/>"#,
        Icono::Pi => r#"<path d="M5 7h14M9 7v12M15 7v9a3 3 0 0 0 3 3"/>"#,
        Icono::Sol => r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1"/>"#,
        Icono::Sombra => r#"<circle cx="9" cy="9" r="4"/><path d="M13 21l8-8M9 21l12-12M17 21l4-4"/>"#,
        Icono::Brujula => r#"<circle cx="12" cy="12" r="9"/><path d="M15.5 8.5l-2 5-5 2 2-5z"/>"#,
        Icono::Histograma => r#"<path d="M4 20h16M7 17V9M12 17V5M17 17v-5"/>"#,
        Icono::Grafica => r#"<path d="M4 20V10M10 20V4M16 20v-8M22 20H2"/>"#,
        Icono::Dispersion => r#"<path d="M3 3v18h18"/><circle cx="8" cy="15" r="1"/><circle cx="11" cy="11" r="1"/><circle cx="15" cy="9" r="1"/><circle cx="18" cy="6" r="1"/>"#,
        Icono::Distribucion => r#"<path d="M3 20h18"/><path d="M4 19c3 0 4-12 8-12s5 12 8 12"/>"#,
        Icono::Red => r#"<circle cx="5" cy="5" r="2"/><circle cx="19" cy="5" r="2"/><circle cx="5" cy="19" r="2"/><circle cx="19" cy="19" r="2"/><circle cx="12" cy="12" r="2.5"/><path d="M6.5 6.5l3.7 3.7M17.5 6.5l-3.7 3.7M6.5 17.5l3.7-3.7M17.5 17.5l-3.7-3.7"/>"#,
        Icono::Clasificacion => r#"<rect x="9" y="3" width="6" height="5"/><rect x="3" y="16" width="6" height="5"/><rect x="15" y="16" width="6" height="5"/><path d="M12 8v4M6 16v-4h12v4"/>"#,
        Icono::Clustering => r#"<circle cx="7" cy="8" r="3"/><circle cx="17" cy="8" r="3"/><circle cx="12" cy="17" r="3"/>"#,
        Icono::Pca => r#"<path d="M4 20L20 4M4 4l16 16"/><circle cx="12" cy="12" r="2"/>"#,
        Icono::Anomalias => r#"<path d="M3 15l4-3 3 2 4-9 3 7 4-2"/><circle cx="14" cy="5" r="2"/>"#,
        Icono::Calendario => r#"<rect x="3" y="5" width="18" height="16"/><path d="M3 10h18M8 3v4M16 3v4M9 15l2 2 4-4"/>"#,
        Icono::Lista => r#"<path d="M9 6h12M9 12h12M9 18h12M4 6h.01M4 12h.01M4 18h.01"/>"#,
        Icono::Equipos => r#"<path d="M12 3l8 4v10l-8 4-8-4V7z"/><path d="M4 7l8 4 8-4M12 11v10"/>"#,
        Icono::Amef => r#"<path d="M12 3l9 16H3z"/><path d="M12 9v4"/><circle cx="12" cy="16" r=".6" fill="currentColor"/>"#,
        Icono::Historial => r#"<path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 4v5h5M12 7v5l3 2"/>"#,
        Icono::Validar => r#"<circle cx="12" cy="12" r="9"/><path d="M8 12l3 3 5-6"/>"#,
        Icono::Correcto => r#"<path d="M5 12l5 5 9-10"/>"#,
        Icono::Aviso => r#"<path d="M12 3l10 18H2z"/><path d="M12 10v5"/><circle cx="12" cy="18" r=".6" fill="currentColor"/>"#,
        Icono::Filtro => r#"<path d="M4 5h16l-6 8v6l-4 1v-7z"/>"#,
        Icono::Cerrar => r#"<path d="M6 6l12 12M18 6L6 18"/>"#,
        Icono::Borrar => r#"<path d="M4 7h16M10 7V4h4v3M6 7l1 13h10l1-13"/>"#,
        Icono::Flecha => r#"<path d="M4 12h15M13 6l6 6-6 6"/>"#,
        Icono::Anterior => r#"<path d="M15 6l-6 6 6 6"/>"#,
        Icono::Siguiente => r#"<path d="M9 6l6 6-6 6"/>"#,
        Icono::Plegar => r#"<path d="M12 6l-6 6 6 6M19 6l-6 6 6 6"/>"#,
        Icono::Desplegar => r#"<path d="M12 6l6 6-6 6M5 6l6 6-6 6"/>"#,
        Icono::Terminal => r#"<rect x="3" y="4" width="18" height="16"/><path d="M7 9l3 3-3 3M12 15h5"/>"#,
        Icono::Variable => r#"<path d="M8 4H6v16h2M16 4h2v16h-2M10 9l4 6M14 9l-4 6"/>"#,
        Icono::Mas => r#"<path d="M12 5v14M5 12h14"/>"#,
    }
}

/// Dirección con la que se registra el SVG de un icono en los cargadores de egui.
fn uri(i: Icono) -> String {
    format!("bytes://heliora/icono/{i:?}.svg")
}

/// Instala los cargadores de imágenes de `egui_extras` y registra el SVG de cada icono, en blanco.
pub fn registrar(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    for i in TODOS {
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#FFFFFF" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
            trazado(i).replace("currentColor", "#FFFFFF")
        );
        ctx.include_bytes(uri(i), svg.into_bytes());
    }
    let invertido = LOGO.replace("#000000", "#PROVISIONAL").replace("#FFFFFF", "#000000").replace("#PROVISIONAL", "#FFFFFF");
    ctx.include_bytes(uri_logo(false), LOGO.as_bytes().to_vec());
    ctx.include_bytes(uri_logo(true), invertido.into_bytes());
}

/// SVG del logotipo de la barra superior, tal como está en el diseño.
const LOGO: &str = include_str!("../diseno/icono/heliora_icono_pequeno.svg");

/// Dirección del logotipo; la variante invertida se usa en tema oscuro.
fn uri_logo(invertido: bool) -> &'static str {
    if invertido { "bytes://heliora/logo_invertido.svg" } else { "bytes://heliora/logo.svg" }
}

/// Dibuja el logotipo en `r` (cuadrado), rasterizado al tamaño exacto en píxeles.
pub fn logo(pt: &Painter, r: Rect, invertido: bool) {
    dibujar_uri(pt, r, uri_logo(invertido), Color32::WHITE);
}

/// Dibuja `icono` centrado en `r`, rasterizado al tamaño exacto en píxeles y teñido de `color`.
pub fn dibujar(pt: &Painter, r: Rect, icono: Icono, color: Color32) {
    dibujar_uri(pt, r, &uri(icono), color);
}

/// Rasteriza la imagen registrada en `uri` al tamaño de `r` y la pinta teñida de `color`.
fn dibujar_uri(pt: &Painter, r: Rect, uri: &str, color: Color32) {
    let lado = r.width().min(r.height());
    let cuadro = Rect::from_center_size(r.center(), egui::Vec2::splat(lado));
    let ppp = pt.ctx().pixels_per_point();
    let px = (lado * ppp).round().max(1.0) as u32;
    let pista = egui::SizeHint::Size { width: px, height: px, maintain_aspect_ratio: true };
    match pt.ctx().try_load_texture(uri, TextureOptions::LINEAR, pista) {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            pt.image(texture.id, cuadro, uv, color);
        }
        Ok(egui::load::TexturePoll::Pending { .. }) => pt.ctx().request_repaint(),
        Err(_) => {}
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn todos_los_iconos_se_registran_y_se_rasterizan() {
        let ctx = egui::Context::default();
        registrar(&ctx);
        for i in TODOS {
            let r = ctx.try_load_texture(&uri(i), TextureOptions::LINEAR, egui::SizeHint::Size { width: 24, height: 24, maintain_aspect_ratio: true });
            assert!(r.is_ok(), "{i:?}: {:?}", r.err());
        }
        for invertido in [false, true] {
            let r = ctx.try_load_texture(uri_logo(invertido), TextureOptions::LINEAR, egui::SizeHint::Size { width: 22, height: 22, maintain_aspect_ratio: true });
            assert!(r.is_ok(), "logo {invertido}: {:?}", r.err());
        }
    }
}
