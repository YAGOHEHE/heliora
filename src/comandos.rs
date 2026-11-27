//! Paleta de comandos (Ctrl K): busca bloques, comandos, módulos, ejemplos y variables.
use crate::tema::{self, Modo, Modulo};
use egui::{Align2, Key, Sense, Stroke, Vec2};

/// Lo que se ejecuta al elegir una entrada.
#[derive(Clone, Debug, PartialEq)]
pub enum Accion {
    Modulo(Modulo),
    /// Añade un bloque del catálogo al centro del lienzo.
    Bloque(String),
    Ejemplo(String),
    Simular,
    Encuadrar,
    Linealizar,
    OrdenarCables,
    NuevoDiagrama,
    AbrirDiagrama,
    GuardarDiagrama,
    AbrirProyecto,
    GuardarProyecto,
    Tema(Modo),
    ReiniciarMotor,
    /// Abre el entorno de cálculo con el nombre de la variable en el portapapeles.
    Variable(String),
}

/// Una fila de la paleta.
#[derive(Clone, Debug)]
pub struct Entrada {
    /// Clase de entrada («Bloque», «Comando», «Módulo», «Ejemplo», «Variable»).
    pub clase: &'static str,
    pub texto: String,
    /// Texto secundario: categoría, atajo o forma de la variable.
    pub detalle: String,
    pub accion: Accion,
}

impl Entrada {
    pub fn nueva(
        clase: &'static str,
        texto: impl Into<String>,
        detalle: impl Into<String>,
        accion: Accion,
    ) -> Entrada {
        Entrada {
            clase,
            texto: texto.into(),
            detalle: detalle.into(),
            accion,
        }
    }
}

/// Comandos fijos de la aplicación (los que no dependen del catálogo ni del espacio de trabajo).
pub fn comandos_fijos() -> Vec<Entrada> {
    let mut v = vec![
        Entrada::nueva("Comando", "Simular", "F5", Accion::Simular),
        Entrada::nueva(
            "Comando",
            "Encuadrar el diagrama",
            "Bloques",
            Accion::Encuadrar,
        ),
        Entrada::nueva(
            "Comando",
            "Ordenar cables (ángulo recto / curvas)",
            "Bloques",
            Accion::OrdenarCables,
        ),
        Entrada::nueva("Comando", "Linealizar aquí", "Bloques", Accion::Linealizar),
        Entrada::nueva(
            "Comando",
            "Nuevo diagrama",
            "Bloques",
            Accion::NuevoDiagrama,
        ),
        Entrada::nueva(
            "Comando",
            "Abrir diagrama…",
            "Bloques",
            Accion::AbrirDiagrama,
        ),
        Entrada::nueva(
            "Comando",
            "Guardar diagrama",
            "Bloques",
            Accion::GuardarDiagrama,
        ),
        Entrada::nueva(
            "Comando",
            "Abrir proyecto…",
            "Proyecto",
            Accion::AbrirProyecto,
        ),
        Entrada::nueva(
            "Comando",
            "Guardar proyecto",
            "Proyecto",
            Accion::GuardarProyecto,
        ),
        Entrada::nueva(
            "Comando",
            "Tema claro",
            "Apariencia",
            Accion::Tema(Modo::Claro),
        ),
        Entrada::nueva(
            "Comando",
            "Tema oscuro",
            "Apariencia",
            Accion::Tema(Modo::Oscuro),
        ),
        Entrada::nueva(
            "Comando",
            "Reiniciar el motor",
            "Borra las variables",
            Accion::ReiniciarMotor,
        ),
    ];
    for g in tema::MODULOS {
        for (m, n) in g.iter() {
            v.push(Entrada::nueva(
                "Módulo",
                format!("Ir a {n}"),
                "",
                Accion::Modulo(*m),
            ));
        }
    }
    v
}

/// Entradas que contienen todas las palabras de la consulta, sin distinguir mayúsculas.
/// Primero las que empiezan por la consulta; después, en el orden de origen.
pub fn filtrar<'a>(entradas: &'a [Entrada], consulta: &str, max: usize) -> Vec<&'a Entrada> {
    let q = consulta.trim().to_lowercase();
    let palabras: Vec<&str> = q.split_whitespace().collect();
    let mut v: Vec<(bool, usize, &Entrada)> = entradas
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let pajar = format!("{} {} {}", e.texto, e.detalle, e.clase).to_lowercase();
            palabras
                .iter()
                .all(|p| pajar.contains(p))
                .then(|| (!e.texto.to_lowercase().starts_with(&q), i, e))
        })
        .collect();
    v.sort_by_key(|(no_prefijo, i, _)| (*no_prefijo, *i));
    v.into_iter().take(max).map(|(_, _, e)| e).collect()
}

/// Estado de la paleta de comandos.
#[derive(Default)]
pub struct Paleta {
    pub abierta: bool,
    consulta: String,
    elegida: usize,
}

impl Paleta {
    pub fn abrir(&mut self) {
        self.abierta = true;
        self.consulta.clear();
        self.elegida = 0;
    }

    /// Dibuja la paleta si está abierta y devuelve la acción elegida.
    pub fn mostrar(&mut self, ctx: &egui::Context, entradas: &[Entrada]) -> Option<Accion> {
        if !self.abierta {
            return None;
        }
        let pal = tema::p();
        let mut elegida = None;
        let area = egui::Modal::default_area(egui::Id::new("paleta_comandos"))
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 72.0));
        let marco = egui::Frame::popup(&ctx.style())
            .fill(pal.panel)
            .inner_margin(8)
            .corner_radius(8);
        let r = egui::Modal::new(egui::Id::new("paleta_comandos"))
            .area(area)
            .frame(marco)
            .backdrop_color(egui::Color32::from_black_alpha(60))
            .show(ctx, |ui| {
                ui.set_width(560.0);
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.consulta)
                        .hint_text("Buscar bloque, comando o variable")
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Body),
                );
                edit.request_focus();
                if edit.changed() {
                    self.elegida = 0;
                }
                let lista = filtrar(entradas, &self.consulta, 12);
            self.elegida = self.elegida.min(lista.len().saturating_sub(1));
                let (arriba, abajo, intro) = ui.input_mut(|i| {
                    (
                        i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                        i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                        i.key_pressed(Key::Enter),
                    )
                });
                if abajo && self.elegida + 1 < lista.len() {
                    self.elegida += 1;
                }
                if arriba && self.elegida > 0 {
                    self.elegida -= 1;
                }
                ui.add_space(4.0);
                if lista.is_empty() {
                    ui.label(egui::RichText::new("Sin resultados").color(pal.texto_3));
                }
                for (k, e) in lista.iter().enumerate() {
                    let (fila, resp) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
                    let activa = k == self.elegida;
                    if activa || resp.hovered() {
                        ui.painter().rect_filled(
                            fila,
                            tema::radio() as f32,
                            if activa { pal.acento_fondo } else { pal.realce },
                        );
                    }
                    if activa {
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(fila.min, Vec2::new(2.0, fila.height())),
                            0.0,
                            pal.acento,
                        );
                    }
                    let p = ui.painter();
                    p.text(
                        fila.left_center() + Vec2::new(10.0, 0.0),
                        Align2::LEFT_CENTER,
                        &e.texto,
                        egui::FontId::proportional(13.0),
                        pal.texto,
                    );
                    let derecha = if e.detalle.is_empty() {
                        e.clase.to_string()
                    } else {
                        format!("{} · {}", e.detalle, e.clase)
                    };
                    p.text(
                        fila.right_center() - Vec2::new(10.0, 0.0),
                        Align2::RIGHT_CENTER,
                        derecha,
                        tema::fuente_mono(11.0),
                        pal.texto_3,
                    );
                    if resp.clicked() || (activa && intro) {
                        elegida = Some(e.accion.clone());
                    }
                }
                ui.add_space(2.0);
                let (r, _) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter().line_segment(
                    [r.left_center(), r.right_center()],
                    Stroke::new(1.0_f32, pal.linea),
                );
                ui.label(
                    egui::RichText::new("↑ ↓ para moverse · Intro para ejecutar · Esc para cerrar")
                        .font(tema::fuente_mono(11.0))
                        .color(pal.texto_3),
                );
            });
        if elegida.is_some() || r.should_close() {
            self.abierta = false;
        }
        elegida
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn entradas() -> Vec<Entrada> {
        vec![
            Entrada::nueva(
                "Bloque",
                "Boost",
                "Potencia · Convertidores",
                Accion::Bloque("boost".into()),
            ),
            Entrada::nueva(
                "Bloque",
                "Arreglo FV",
                "Fotovoltaica",
                Accion::Bloque("arreglo_fv".into()),
            ),
            Entrada::nueva("Comando", "Simular", "F5", Accion::Simular),
            Entrada::nueva(
                "Variable",
                "Vbus",
                "1×20001",
                Accion::Variable("Vbus".into()),
            ),
        ]
    }

    #[test]
    fn todas_las_palabras_deben_aparecer_sin_distinguir_mayusculas() {
        let e = entradas();
        let r = filtrar(&e, "potencia BOOST", 10);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].accion, Accion::Bloque("boost".into()));
        assert!(filtrar(&e, "boost fotovoltaica", 10).is_empty());
    }

    #[test]
    fn el_prefijo_va_primero_y_la_consulta_vacia_lo_lista_todo() {
        let e = entradas();
        // «v» aparece en «Arreglo FV» (antes en la lista) y empieza «Vbus»
        let r = filtrar(&e, "v", 10);
        assert_eq!(r[0].texto, "Vbus");
        assert_eq!(filtrar(&e, "", 10).len(), 4);
        assert_eq!(filtrar(&e, "", 2).len(), 2);
    }
}
