//! Iconos de trazo de 1,6 px sin relleno, dibujados con el pintor de egui en una rejilla de 24
//! unidades (la de los SVG del diseño). Se escalan al rectángulo que reciben: 24 px en los botones
//! grandes de la cinta, 14-18 px en los botones pequeños.
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icono {
    Inicio,
    Nuevo,
    NuevoDocumento,
    Abrir,
    Guardar,
    GuardarComo,
    Deshacer,
    Rehacer,
    Encuadrar,
    Codigo,
    Subsistema,
    OrdenarCables,
    Simular,
    Solver,
    Linealizar,
    Osciloscopio,
    Descargar,
    Imagen,
    Informe,
    Tabla,
    Buscar,
    Ajustes,
    Ayuda,
    Cuenta,
    Referencia,
    Carpeta,
    Rayo,
    Onda,
    Bode,
    Escalon,
    Raices,
    Pi,
    Sol,
    Curva,
    Histograma,
    Dispersion,
    Red,
    Calendario,
    Lista,
    Validar,
    Filtro,
    Cerrar,
    Flecha,
    Terminal,
    Variable,
    Grafica,
    Llave,
    Bloques,
    Detener,
    Copiar,
    Mas,
    Plegar,
    Desplegar,
    Correcto,
    Diagrama,
}

/// Dibuja `icono` centrado en `r` con trazo de 1,6 px (escalado si el rectángulo no mide 24 px).
pub fn dibujar(pt: &Painter, r: Rect, icono: Icono, color: Color32) {
    let lado = r.width().min(r.height());
    let s = lado / 24.0;
    let o = r.center() - Vec2::splat(12.0 * s);
    let q = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let t = Stroke::new((1.6 * s).max(1.2), color);
    let linea = |puntos: &[(f32, f32)]| {
        pt.add(Shape::line(puntos.iter().map(|&(x, y)| q(x, y)).collect(), t));
    };
    let cerrada = |puntos: &[(f32, f32)]| {
        pt.add(Shape::closed_line(puntos.iter().map(|&(x, y)| q(x, y)).collect(), t));
    };
    let caja = |x0: f32, y0: f32, x1: f32, y1: f32| {
        pt.rect_stroke(Rect::from_min_max(q(x0, y0), q(x1, y1)), 0.0, t, egui::StrokeKind::Middle);
    };
    let circulo = |x: f32, y: f32, radio: f32| {
        pt.circle_stroke(q(x, y), radio * s, t);
    };
    let arco = |x: f32, y: f32, radio: f32, desde: f32, hasta: f32| {
        let n = 20;
        let puntos: Vec<Pos2> = (0..=n).map(|i| {
            let a = desde + (hasta - desde) * i as f32 / n as f32;
            q(x + radio * a.cos(), y + radio * a.sin())
        }).collect();
        pt.add(Shape::line(puntos, t));
    };
    let documento = || {
        linea(&[(14.0, 3.0), (6.0, 3.0), (6.0, 21.0), (18.0, 21.0), (18.0, 7.0), (14.0, 3.0), (14.0, 7.0), (18.0, 7.0)]);
    };
    match icono {
        Icono::Inicio => {
            linea(&[(3.0, 11.0), (12.0, 3.5), (21.0, 11.0)]);
            linea(&[(5.5, 9.5), (5.5, 20.0), (18.5, 20.0), (18.5, 9.5)]);
        }
        Icono::Nuevo => {
            circulo(12.0, 12.0, 9.0);
            linea(&[(12.0, 7.5), (12.0, 16.5)]);
            linea(&[(7.5, 12.0), (16.5, 12.0)]);
        }
        Icono::NuevoDocumento => {
            documento();
            linea(&[(12.0, 11.0), (12.0, 17.0)]);
            linea(&[(9.0, 14.0), (15.0, 14.0)]);
        }
        Icono::Abrir | Icono::Carpeta => {
            linea(&[(3.0, 6.0), (9.0, 6.0), (11.0, 8.0), (21.0, 8.0), (21.0, 19.0), (3.0, 19.0), (3.0, 6.0)]);
        }
        Icono::Guardar | Icono::GuardarComo => {
            linea(&[(4.0, 4.0), (17.0, 4.0), (20.0, 7.0), (20.0, 20.0), (4.0, 20.0), (4.0, 4.0)]);
            caja(8.0, 4.0, 15.0, 9.0);
            caja(7.5, 13.0, 16.5, 20.0);
            if icono == Icono::GuardarComo {
                linea(&[(10.0, 16.5), (14.0, 16.5)]);
            }
        }
        Icono::Deshacer | Icono::Rehacer => {
            let espejo = |x: f32| if icono == Icono::Rehacer { 24.0 - x } else { x };
            linea(&[(espejo(8.0), 5.0), (espejo(4.0), 9.0), (espejo(8.0), 13.0)]);
            let mut puntos = vec![(espejo(4.0), 9.0), (espejo(14.0), 9.0)];
            for i in 0..=10 {
                let a = -FRAC_PI_2 + PI * i as f32 / 10.0;
                puntos.push((espejo(14.0 + 5.0 * a.cos()), 14.0 + 5.0 * a.sin()));
            }
            puntos.push((espejo(9.0), 19.0));
            linea(&puntos);
        }
        Icono::Encuadrar => {
            for (x, y, dx, dy) in [(4.0, 4.0, 1.0, 1.0), (20.0, 4.0, -1.0, 1.0), (4.0, 20.0, 1.0, -1.0), (20.0, 20.0, -1.0, -1.0)] {
                linea(&[(x, y + 5.0 * dy), (x, y), (x + 5.0 * dx, y)]);
            }
        }
        Icono::Codigo => {
            linea(&[(8.0, 7.0), (3.0, 12.0), (8.0, 17.0)]);
            linea(&[(16.0, 7.0), (21.0, 12.0), (16.0, 17.0)]);
        }
        Icono::Subsistema | Icono::Bloques => {
            caja(3.0, 3.0, 11.0, 10.0);
            caja(13.0, 14.0, 21.0, 21.0);
            linea(&[(7.0, 10.0), (7.0, 17.5), (13.0, 17.5)]);
        }
        Icono::OrdenarCables => {
            linea(&[(3.0, 6.0), (9.0, 6.0), (9.0, 18.0), (21.0, 18.0)]);
            linea(&[(3.0, 12.0), (21.0, 12.0)]);
        }
        Icono::Simular => {
            cerrada(&[(7.0, 4.0), (19.0, 12.0), (7.0, 20.0)]);
        }
        Icono::Detener => caja(6.0, 6.0, 18.0, 18.0),
        Icono::Solver => {
            circulo(5.5, 12.0, 2.5);
            circulo(18.5, 12.0, 2.5);
            linea(&[(8.0, 12.0), (16.0, 12.0)]);
            linea(&[(12.0, 6.0), (12.0, 18.0)]);
        }
        Icono::Linealizar => {
            linea(&[(3.0, 18.0), (21.0, 6.0)]);
            linea(&[(5.0, 8.0), (9.0, 12.0), (14.0, 11.0), (19.0, 16.0)]);
        }
        Icono::Osciloscopio => {
            caja(3.0, 4.0, 21.0, 18.0);
            linea(&[(5.0, 12.0), (8.0, 12.0), (10.0, 7.0), (13.0, 15.0), (15.0, 10.0), (19.0, 10.0)]);
            linea(&[(9.0, 21.0), (15.0, 21.0)]);
        }
        Icono::Descargar => {
            linea(&[(12.0, 3.0), (12.0, 15.0)]);
            linea(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            linea(&[(4.0, 20.0), (20.0, 20.0)]);
        }
        Icono::Imagen => {
            caja(3.0, 4.0, 21.0, 20.0);
            linea(&[(3.0, 17.0), (9.0, 11.0), (14.0, 16.0), (17.0, 13.0), (21.0, 17.0)]);
            circulo(16.0, 8.5, 1.5);
        }
        Icono::Informe => {
            documento();
            for y in [11.0, 14.0, 17.0] {
                linea(&[(9.0, y), (15.0, y)]);
            }
        }
        Icono::Tabla => {
            caja(3.0, 4.0, 21.0, 20.0);
            linea(&[(3.0, 9.0), (21.0, 9.0)]);
            linea(&[(3.0, 14.5), (21.0, 14.5)]);
            linea(&[(9.0, 4.0), (9.0, 20.0)]);
        }
        Icono::Buscar => {
            circulo(10.5, 10.5, 6.5);
            linea(&[(15.5, 15.5), (21.0, 21.0)]);
        }
        Icono::Ajustes => {
            circulo(12.0, 12.0, 3.2);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                linea(&[(12.0 + 6.0 * a.cos(), 12.0 + 6.0 * a.sin()), (12.0 + 9.0 * a.cos(), 12.0 + 9.0 * a.sin())]);
            }
            circulo(12.0, 12.0, 6.0);
        }
        Icono::Ayuda => {
            circulo(12.0, 12.0, 9.0);
            arco(12.0, 9.5, 2.8, PI, TAU + 0.6);
            linea(&[(14.2, 11.2), (12.0, 13.0), (12.0, 14.5)]);
            pt.circle_filled(q(12.0, 17.3), 1.0 * s, color);
        }
        Icono::Cuenta => {
            circulo(12.0, 8.0, 4.0);
            arco(12.0, 21.0, 8.0, PI, TAU);
        }
        Icono::Referencia => {
            caja(5.0, 3.0, 19.0, 21.0);
            linea(&[(5.0, 16.0), (19.0, 16.0)]);
        }
        Icono::Rayo => linea(&[(13.0, 2.5), (5.0, 13.5), (11.5, 13.5), (10.5, 21.5), (19.0, 10.0), (12.5, 10.0), (13.0, 2.5)]),
        Icono::Onda => {
            let puntos: Vec<(f32, f32)> = (0..=32).map(|i| {
                let x = 3.0 + 18.0 * i as f32 / 32.0;
                (x, 12.0 - 6.0 * ((x - 3.0) / 18.0 * TAU * 1.5).sin())
            }).collect();
            linea(&puntos);
        }
        Icono::Bode => {
            linea(&[(4.0, 3.0), (4.0, 20.0), (21.0, 20.0)]);
            linea(&[(4.0, 7.0), (10.0, 7.0), (19.0, 16.0)]);
        }
        Icono::Escalon => linea(&[(3.0, 18.0), (9.0, 18.0), (9.0, 7.0), (21.0, 7.0)]),
        Icono::Raices => {
            linea(&[(12.0, 3.0), (12.0, 21.0)]);
            linea(&[(3.0, 12.0), (21.0, 12.0)]);
            linea(&[(6.0, 6.0), (18.0, 18.0)]);
            linea(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icono::Pi => {
            linea(&[(4.0, 6.0), (20.0, 6.0)]);
            linea(&[(9.0, 6.0), (8.0, 20.0)]);
            linea(&[(15.0, 6.0), (15.0, 18.0), (17.0, 20.0)]);
        }
        Icono::Sol => {
            circulo(12.0, 12.0, 4.0);
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                linea(&[(12.0 + 6.5 * a.cos(), 12.0 + 6.5 * a.sin()), (12.0 + 9.5 * a.cos(), 12.0 + 9.5 * a.sin())]);
            }
        }
        Icono::Curva => {
            linea(&[(4.0, 3.0), (4.0, 20.0), (21.0, 20.0)]);
            linea(&[(4.0, 7.0), (13.0, 7.5), (16.0, 9.0), (18.0, 13.0), (19.0, 20.0)]);
        }
        Icono::Histograma => {
            linea(&[(3.0, 21.0), (21.0, 21.0)]);
            for (x, y) in [(6.0, 14.0), (10.0, 6.0), (14.0, 10.0), (18.0, 16.0)] {
                linea(&[(x, 21.0), (x, y)]);
            }
        }
        Icono::Dispersion => {
            linea(&[(4.0, 3.0), (4.0, 20.0), (21.0, 20.0)]);
            for (x, y) in [(8.0, 15.0), (11.0, 12.0), (14.0, 13.0), (17.0, 8.0), (19.0, 6.0)] {
                circulo(x, y, 1.2);
            }
        }
        Icono::Red => {
            for (x, y) in [(5.0, 5.0), (19.0, 5.0), (5.0, 19.0), (19.0, 19.0)] {
                circulo(x, y, 2.2);
                linea(&[(x + (12.0 - x) * 0.25, y + (12.0 - y) * 0.25), (x + (12.0 - x) * 0.7, y + (12.0 - y) * 0.7)]);
            }
            circulo(12.0, 12.0, 2.5);
        }
        Icono::Calendario => {
            caja(3.5, 5.0, 20.5, 20.5);
            linea(&[(3.5, 10.0), (20.5, 10.0)]);
            linea(&[(8.0, 3.0), (8.0, 7.0)]);
            linea(&[(16.0, 3.0), (16.0, 7.0)]);
            linea(&[(8.5, 15.0), (11.0, 17.5), (15.5, 13.0)]);
        }
        Icono::Lista => {
            for y in [6.0, 12.0, 18.0] {
                linea(&[(8.0, y), (21.0, y)]);
                pt.circle_filled(q(4.5, y), 1.1 * s, color);
            }
        }
        Icono::Validar | Icono::Correcto => {
            if icono == Icono::Validar {
                circulo(12.0, 12.0, 9.0);
            }
            linea(&[(7.5, 12.5), (10.5, 15.5), (16.5, 9.0)]);
        }
        Icono::Filtro => linea(&[(3.0, 5.0), (21.0, 5.0), (14.0, 13.0), (14.0, 20.0), (10.0, 18.0), (10.0, 13.0), (3.0, 5.0)]),
        Icono::Cerrar => {
            linea(&[(6.0, 6.0), (18.0, 18.0)]);
            linea(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icono::Flecha => {
            linea(&[(4.0, 12.0), (20.0, 12.0)]);
            linea(&[(14.0, 6.0), (20.0, 12.0), (14.0, 18.0)]);
        }
        Icono::Terminal => {
            caja(3.0, 4.0, 21.0, 20.0);
            linea(&[(7.0, 9.0), (10.5, 12.0), (7.0, 15.0)]);
            linea(&[(12.5, 15.5), (17.0, 15.5)]);
        }
        Icono::Variable => {
            linea(&[(8.0, 4.0), (5.5, 4.0), (5.5, 20.0), (8.0, 20.0)]);
            linea(&[(16.0, 4.0), (18.5, 4.0), (18.5, 20.0), (16.0, 20.0)]);
            linea(&[(9.5, 9.0), (14.5, 15.0)]);
            linea(&[(14.5, 9.0), (9.5, 15.0)]);
        }
        Icono::Grafica => {
            linea(&[(4.0, 3.0), (4.0, 20.0), (21.0, 20.0)]);
            linea(&[(4.0, 16.0), (9.0, 11.0), (13.0, 14.0), (20.0, 6.0)]);
        }
        Icono::Llave => {
            circulo(8.0, 16.0, 4.5);
            linea(&[(11.2, 12.8), (20.0, 4.0)]);
            linea(&[(16.5, 7.5), (19.0, 10.0)]);
        }
        Icono::Copiar => {
            caja(8.0, 8.0, 20.0, 20.0);
            linea(&[(16.0, 8.0), (16.0, 4.0), (4.0, 4.0), (4.0, 16.0), (8.0, 16.0)]);
        }
        Icono::Mas => {
            linea(&[(12.0, 5.0), (12.0, 19.0)]);
            linea(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icono::Plegar | Icono::Desplegar => {
            let d = if icono == Icono::Plegar { 1.0 } else { -1.0 };
            for x in [9.0, 15.0] {
                linea(&[(x + 3.0 * d, 6.0), (x - 3.0 * d, 12.0), (x + 3.0 * d, 18.0)]);
            }
        }
        Icono::Diagrama => {
            caja(3.0, 9.0, 9.0, 15.0);
            caja(15.0, 3.0, 21.0, 9.0);
            caja(15.0, 15.0, 21.0, 21.0);
            linea(&[(9.0, 12.0), (12.0, 12.0), (12.0, 6.0), (15.0, 6.0)]);
            linea(&[(12.0, 12.0), (12.0, 18.0), (15.0, 18.0)]);
        }
    }
}
