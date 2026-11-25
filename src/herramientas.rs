//! Páginas de análisis: cada herramienta del motor se muestra como un formulario con su informe.
use crate::app::{Compartido, Variable};
use crate::grafica::{self, Figura};
use crate::motor::Motor;
use crate::tema;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone)]
pub struct Param {
    pub nombre: String,
    pub etiqueta: String,
    pub tipo: String,
    pub defecto: String,
    pub opciones: Vec<String>,
}

#[derive(Clone)]
pub struct Herramienta {
    pub id: String,
    pub modulo: String,
    pub titulo: String,
    pub grupo: String,
    pub doc: String,
    pub params: Vec<Param>,
}

pub enum Seccion {
    Texto(String),
    Aviso(String, String),
    Kv(String, Vec<(String, String, String)>),
    Tabla(String, Vec<String>, Vec<Vec<String>>),
    Grafica(Figura),
}

pub struct Informe {
    pub titulo: String,
    pub secciones: Vec<Seccion>,
}

pub fn lista_desde(v: &Value) -> Vec<Herramienta> {
    let txt = |d: &Value| match d {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        o => o.to_string(),
    };
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|h| Herramienta {
                    id: h["id"].as_str().unwrap_or("").into(),
                    modulo: h["modulo"].as_str().unwrap_or("").into(),
                    titulo: h["titulo"].as_str().unwrap_or("").into(),
                    grupo: h["grupo"].as_str().unwrap_or("").into(),
                    doc: h["doc"].as_str().unwrap_or("").into(),
                    params: h["params"]
                        .as_array()
                        .map(|p| {
                            p.iter()
                                .map(|d| Param {
                                    nombre: d["nombre"].as_str().unwrap_or("").into(),
                                    etiqueta: d["etiqueta"].as_str().unwrap_or("").into(),
                                    tipo: d["tipo"].as_str().unwrap_or("texto").into(),
                                    defecto: txt(&d["defecto"]),
                                    opciones: d["opciones"].as_array().map(|o| o.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn informe_desde(v: &Value) -> Informe {
    let mut secciones = vec![];
    for s in v["secciones"].as_array().unwrap_or(&vec![]) {
        let t = |k: &str| s[k].as_str().unwrap_or("").to_string();
        match s["tipo"].as_str().unwrap_or("") {
            "texto" => secciones.push(Seccion::Texto(t("texto"))),
            "aviso" => secciones.push(Seccion::Aviso(t("nivel"), t("texto"))),
            "kv" => {
                let items = s["items"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|f| (f[0].as_str().unwrap_or("").to_string(), f[1].as_str().unwrap_or("").to_string(), f[2].as_str().unwrap_or("").to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                secciones.push(Seccion::Kv(t("titulo"), items));
            }
            "tabla" => {
                let col = s["columnas"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                let fil = s["filas"]
                    .as_array()
                    .map(|a| a.iter().map(|f| f.as_array().map(|c| c.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default()).collect())
                    .unwrap_or_default();
                secciones.push(Seccion::Tabla(t("titulo"), col, fil));
            }
            "grafica" => secciones.push(Seccion::Grafica(Figura::desde_json(&s["figura"]))),
            _ => {}
        }
    }
    Informe { titulo: v["titulo"].as_str().unwrap_or("").into(), secciones }
}

pub fn dibujar_informe(ui: &mut egui::Ui, inf: &mut Informe, c: &mut Compartido, base: &str) {
    ui.label(egui::RichText::new(&inf.titulo).size(19.0).family(tema::semi()).color(tema::p().texto));
    ui.add_space(4.0);
    let total = inf.secciones.len();
    for (i, s) in inf.secciones.iter_mut().enumerate() {
        match s {
            Seccion::Texto(t) => {
                ui.label(t.as_str());
            }
            Seccion::Aviso(n, t) => {
                // Estado con forma y texto: cuadrado de 7 px y la frase, sobre la superficie PANEL
                let nivel = tema::Nivel::desde(n.as_str());
                egui::Frame::new().fill(if nivel == tema::Nivel::Falla { tema::p().falla_fondo } else { tema::p().panel }).stroke(egui::Stroke::new(1.0_f32, tema::p().linea)).corner_radius(tema::radio_ventana()).inner_margin(8).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    tema::estado(ui, nivel, t.as_str());
                });
            }
            Seccion::Kv(t, items) => {
                tema::tarjeta(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if !t.is_empty() {
                        tema::titulo(ui, t);
                    }
                    for (a, b, u) in items.iter() {
                        tema::separador(ui);
                        ui.horizontal(|ui| {
                            ui.set_min_height(24.0);
                            ui.label(egui::RichText::new(a).color(tema::p().texto_2));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("{b} {u}").trim().to_string()).family(egui::FontFamily::Name("dato".into())).color(tema::p().texto));
                            });
                        });
                    }
                });
            }
            Seccion::Tabla(t, col, fil) => {
                tema::tarjeta(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if !t.is_empty() {
                            tema::titulo(ui, t);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Copiar CSV").clicked() {
                                let mut csv = col.join(",") + "\n";
                                for f in fil.iter() {
                                    csv += &f.join(",");
                                    csv.push('\n');
                                }
                                ui.ctx().copy_text(csv);
                            }
                        });
                    });
                    egui::ScrollArea::horizontal().id_salt(("t", base, i)).show(ui, |ui| {
                        tabla(ui, col, fil);
                    });
                });
            }
            Seccion::Grafica(f) => {
                tema::tarjeta(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if let Some(a) = grafica::mostrar(ui, f, egui::Id::new(("g", base, i)), 290.0) {
                        c.exportar(ui.ctx(), a, f);
                    }
                });
            }
        }
        if i + 1 < total {
            ui.add_space(6.0);
        }
    }
}

/// Una página con las herramientas de un módulo.
pub struct Pagina {
    pub modulo: String,
    pub titulo: String,
    pub introduccion: String,
    seleccion: Option<String>,
    valores: HashMap<String, HashMap<String, String>>,
    informes: HashMap<String, Informe>,
    id_activo: Option<(u64, String)>,
    errores: HashMap<String, String>,
}

impl Pagina {
    pub fn nueva(modulo: &str, titulo: &str, introduccion: &str) -> Pagina {
        Pagina {
            modulo: modulo.into(),
            titulo: titulo.into(),
            introduccion: introduccion.into(),
            seleccion: None,
            valores: HashMap::new(),
            informes: HashMap::new(),
            id_activo: None,
            errores: HashMap::new(),
        }
    }

    pub fn ocupada(&self) -> bool {
        self.id_activo.is_some()
    }

    pub fn recibir(&mut self, id: u64, ok: bool, v: &Value, err: &str, c: &mut Compartido) -> bool {
        if self.id_activo.as_ref().is_some_and(|a| a.0 == id) {
            let (_, h) = self.id_activo.take().unwrap();
            if ok {
                self.informes.insert(h.clone(), informe_desde(v));
                self.errores.remove(&h);
                c.fijar_variables(&v["variables"]);
            } else {
                self.errores.insert(h, err.to_string());
            }
            return true;
        }
        false
    }

    /// Ejecuta una herramienta con los valores actuales del formulario.
    pub fn ejecutar(&mut self, h: &Herramienta, motor: &mut Motor) {
        let vals = self.valores.entry(h.id.clone()).or_default();
        let mut m = serde_json::Map::new();
        for p in &h.params {
            let t = vals.get(&p.nombre).cloned().unwrap_or_else(|| p.defecto.clone());
            m.insert(p.nombre.clone(), json!(t));
        }
        self.id_activo = Some((motor.llamar("ejecutar_herramienta", json!({"ident": h.id, "valores": Value::Object(m)})), h.id.clone()));
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, todas: &[Herramienta], motor: &mut Motor, c: &mut Compartido) {
        let propias: Vec<Herramienta> = todas.iter().filter(|h| h.modulo == self.modulo).cloned().collect();
        if self.seleccion.is_none() {
            self.seleccion = propias.first().map(|h| h.id.clone());
        }
        egui::SidePanel::left(format!("lista_{}", self.modulo)).default_width(340.0).width_range(280.0..=460.0).frame(tema::marco_panel()).show_inside(ui, |ui| {
            ui.label(egui::RichText::new(&self.titulo).size(19.0).strong().color(tema::p().texto));
            ui.label(egui::RichText::new(&self.introduccion).small().color(tema::p().texto_2));
            ui.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut grupo = String::new();
                for h in &propias {
                    if h.grupo != grupo {
                        grupo = h.grupo.clone();
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(&grupo).small().strong().color(tema::p().acento_2));
                    }
                    let elegida = self.seleccion.as_deref() == Some(&h.id);
                    if ui.add(egui::Button::selectable(elegida, &h.titulo).min_size(egui::Vec2::new(ui.available_width(), 24.0))).clicked() {
                        self.seleccion = Some(h.id.clone());
                    }
                    if elegida {
                        self.formulario(ui, h, motor, c);
                    }
                }
            });
        });
        egui::CentralPanel::default().frame(tema::marco_fondo()).show_inside(ui, |ui| {
            let Some(id) = self.seleccion.clone() else { return };
            if let Some(e) = self.errores.get(&id) {
                ui.colored_label(tema::p().falla, e);
            }
            if self.id_activo.as_ref().is_some_and(|a| a.1 == id) {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Calculando...");
                });
            }
            egui::ScrollArea::vertical().id_salt(&id).auto_shrink([false, false]).show(ui, |ui| {
                if let Some(inf) = self.informes.get_mut(&id) {
                    dibujar_informe(ui, inf, c, &id);
                } else if !self.id_activo.as_ref().is_some_and(|a| a.1 == id) {
                    ui.add_space(70.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("Completa el formulario y pulsa «Calcular»").size(16.0).color(tema::p().texto_2));
                        if let Some(h) = propias.iter().find(|h| h.id == id) {
                            ui.label(egui::RichText::new(&h.doc).color(tema::p().texto_2));
                        }
                    });
                }
            });
        });
    }

    fn formulario(&mut self, ui: &mut egui::Ui, h: &Herramienta, motor: &mut Motor, c: &Compartido) {
        egui::Frame::new().fill(tema::p().campo).corner_radius(8).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(&h.doc).small().color(tema::p().texto_2));
            ui.add_space(4.0);
            let vals = self.valores.entry(h.id.clone()).or_default();
            // Tablas disponibles para los selectores
            let tablas: Vec<&Variable> = c.variables.iter().filter(|v| !v.columnas.is_empty()).collect();
            let tabla_actual = h.params.iter().find(|p| p.tipo == "tabla").map(|p| vals.get(&p.nombre).cloned().unwrap_or_else(|| p.defecto.clone())).unwrap_or_default();
            let columnas: Vec<String> = c.variables.iter().find(|v| v.nombre == tabla_actual).map(|v| v.columnas.clone()).unwrap_or_default();
            egui::Grid::new(("form", &h.id)).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                for p in &h.params {
                    ui.label(egui::RichText::new(&p.etiqueta).small());
                    let mut t = vals.get(&p.nombre).cloned().unwrap_or_else(|| p.defecto.clone());
                    let anterior = t.clone();
                    match p.tipo.as_str() {
                        "opcion" => {
                            egui::ComboBox::from_id_salt((&h.id, &p.nombre)).selected_text(&t).width(150.0).show_ui(ui, |ui| {
                                for o in &p.opciones {
                                    ui.selectable_value(&mut t, o.clone(), o);
                                }
                            });
                        }
                        "tabla" => {
                            egui::ComboBox::from_id_salt((&h.id, &p.nombre)).selected_text(if t.is_empty() { "elegir…" } else { &t }).width(150.0).show_ui(ui, |ui| {
                                for v in &tablas {
                                    ui.selectable_value(&mut t, v.nombre.clone(), &v.nombre);
                                }
                            });
                        }
                        "columna" | "columna_opc" => {
                            egui::ComboBox::from_id_salt((&h.id, &p.nombre)).selected_text(if t.is_empty() { "—" } else { &t }).width(150.0).show_ui(ui, |ui| {
                                if p.tipo == "columna_opc" {
                                    ui.selectable_value(&mut t, String::new(), "—");
                                }
                                for o in &columnas {
                                    ui.selectable_value(&mut t, o.clone(), o);
                                }
                            });
                        }
                        "ruta" => {
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut t).desired_width(112.0));
                                if ui.small_button("…").clicked() {
                                    if let Some(r) = rfd::FileDialog::new().pick_file() {
                                        t = r.display().to_string();
                                    }
                                }
                            });
                        }
                        _ => {
                            ui.add(egui::TextEdit::singleline(&mut t).desired_width(150.0));
                        }
                    }
                    if t != anterior {
                        vals.insert(p.nombre.clone(), t);
                    }
                    ui.end_row();
                }
            });
            ui.add_space(4.0);
            let ocupado = self.id_activo.is_some();
            ui.add_enabled_ui(!ocupado, |ui| {
                if tema::boton_principal(ui, "Calcular").clicked() {
                    self.ejecutar(h, motor);
                }
            });
        });
    }

    /// Fija un valor del formulario de una herramienta, para acciones rápidas.
    pub fn fijar(&mut self, id: &str, param: &str, valor: &str) {
        self.valores.entry(id.to_string()).or_default().insert(param.to_string(), valor.to_string());
    }

    pub fn seleccionar(&mut self, id: &str) {
        self.seleccion = Some(id.to_string());
    }
}


/// ¿Parece un número (con unidad o porcentaje)? Se alinea a la derecha en mono.
fn es_numero(t: &str) -> bool {
    let t = t.trim();
    let mut c = t.chars();
    match c.next() {
        Some(ch) if ch.is_ascii_digit() || ((ch == '-' || ch == '+') && t.len() > 1 && t[1..].starts_with(|x: char| x.is_ascii_digit())) => {}
        _ => return false,
    }
    !t.contains(' ') || t.ends_with('%') || t.split(' ').count() == 2
}

fn es_estado(t: &str) -> bool {
    matches!(t, "falla" | "alerta" | "normal" | "bueno")
}

/// Tabla: cabecera REALCE en mayúsculas a 11 px, filas de 26 px separadas por LINEA, números en
/// mono a la derecha y el estado con su cuadrado y su texto.
pub fn tabla(ui: &mut egui::Ui, col: &[String], fil: &[Vec<String>]) {
    let pal = tema::p();
    let fuente_cab = tema::fuente_semi(11.0);
    let fuente = egui::FontId::proportional(13.0);
    let mono = tema::fuente_mono(12.5);
    let medir = |ui: &egui::Ui, t: &str, f: &egui::FontId| ui.fonts_mut(|fo| fo.layout_no_wrap(t.to_string(), f.clone(), pal.texto).size().x);
    let anchos: Vec<f32> = (0..col.len())
        .map(|j| {
            let mut w = medir(ui, &col[j].to_uppercase(), &fuente_cab);
            for f in fil.iter().take(400) {
                if let Some(x) = f.get(j) {
                    let extra = if es_estado(x) { 14.0 } else { 0.0 };
                    w = w.max(medir(ui, x, if es_numero(x) { &mono } else { &fuente }) + extra);
                }
            }
            (w + 20.0).min(420.0)
        })
        .collect();
    let total: f32 = anchos.iter().sum::<f32>().max(ui.available_width());
    let alto_fila = 26.0;
    let (r, _) = ui.allocate_exact_size(egui::Vec2::new(total, alto_fila * (fil.len() + 1) as f32), egui::Sense::hover());
    let pt = ui.painter_at(r);
    pt.rect_filled(egui::Rect::from_min_size(r.min, egui::Vec2::new(total, alto_fila)), 0.0, pal.realce);
    let mut x = r.left();
    for (j, h) in col.iter().enumerate() {
        pt.text(egui::Pos2::new(x + 10.0, r.top() + alto_fila / 2.0), egui::Align2::LEFT_CENTER, h.to_uppercase(), fuente_cab.clone(), pal.texto_2);
        x += anchos[j];
    }
    for (i, f) in fil.iter().enumerate() {
        let y = r.top() + alto_fila * (i + 1) as f32;
        pt.line_segment([egui::Pos2::new(r.left(), y), egui::Pos2::new(r.left() + total, y)], egui::Stroke::new(1.0_f32, pal.linea));
        let mut x = r.left();
        for (j, v) in f.iter().enumerate() {
            let w = anchos.get(j).copied().unwrap_or(80.0);
            let centro_y = y + alto_fila / 2.0;
            let recorte = pt.with_clip_rect(egui::Rect::from_min_size(egui::Pos2::new(x, y), egui::Vec2::new(w, alto_fila)));
            if es_estado(v) {
                tema::cuadrado(&recorte, egui::Pos2::new(x + 13.5, centro_y), tema::Nivel::desde(v).color());
                recorte.text(egui::Pos2::new(x + 22.0, centro_y), egui::Align2::LEFT_CENTER, v, fuente.clone(), pal.texto);
            } else if es_numero(v) {
                recorte.text(egui::Pos2::new(x + w - 10.0, centro_y), egui::Align2::RIGHT_CENTER, v, mono.clone(), pal.texto);
            } else {
                recorte.text(egui::Pos2::new(x + 10.0, centro_y), egui::Align2::LEFT_CENTER, v, fuente.clone(), pal.texto);
            }
            x += w;
        }
    }
}
