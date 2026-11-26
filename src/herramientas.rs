//! Páginas de análisis: cada herramienta del motor se muestra como un formulario con su informe.
use crate::app::{Compartido, Variable};
use crate::iconos::Icono;
use crate::grafica::{self, Accion, Decoracion, EstiloMapa, Figura, Franja, MapaCalor, MarcaV, TipoSerie};
use crate::motor::Motor;
use crate::tema;
use crate::ui;
use egui::{Align, Layout, Pos2, Rect, Sense, Stroke, UiBuilder, Vec2};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub struct Param {
    pub nombre: String,
    pub etiqueta: String,
    pub tipo: String,
    pub defecto: String,
    pub opciones: Vec<String>,
    /// El campo solo se muestra cuando el parámetro indicado vale uno de los valores (separados por `|`).
    pub visible_si: Option<(String, String)>,
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
                                    visible_si: d["visible_si"].as_array().and_then(|v| Some((v.first()?.as_str()?.to_string(), v.get(1)?.as_str()?.to_string()))),
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

/// Nombre de archivo seguro a partir de un título.
fn nombre_de_archivo(t: &str) -> String {
    let n: String = t.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    if n.is_empty() {
        "informe".into()
    } else {
        n
    }
}

/// Campo CSV: entre comillas, con las comillas duplicadas, si contiene el separador, comillas o saltos de línea.
fn csv_campo(campo: &str, sep: char) -> String {
    if campo.contains(sep) || campo.contains(['"', '\n', '\r']) {
        format!("\"{}\"", campo.replace('"', "\"\""))
    } else {
        campo.to_string()
    }
}

/// Tabla en CSV con el separador indicado y saltos de línea LF.
pub fn a_csv(col: &[String], fil: &[Vec<String>], sep: char) -> String {
    let linea = |f: &[String]| f.iter().map(|x| csv_campo(x, sep)).collect::<Vec<_>>().join(&sep.to_string());
    let mut o = linea(col) + "\n";
    for f in fil {
        o += &linea(f);
        o.push('\n');
    }
    o
}

fn html_escapar(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Informe completo en un HTML autocontenido: avisos, tablas y gráficas como SVG en línea.
pub fn informe_a_html(inf: &Informe) -> String {
    let t = html_escapar(&inf.titulo);
    let mut o = format!(
        "<!doctype html>\n<html lang=\"es\"><head><meta charset=\"utf-8\"><title>{t}</title><style>body{{font-family:sans-serif;max-width:960px;margin:2em auto;color:#222}}table{{border-collapse:collapse;margin:1em 0}}th,td{{border:1px solid #ccc;padding:3px 8px;text-align:right}}th:first-child,td:first-child{{text-align:left}}caption{{text-align:left;font-weight:bold;padding:4px 0}}.aviso{{padding:6px 10px;border-left:4px solid #888;background:#f4f4f4}}.falla{{border-color:#c33}}.alerta{{border-color:#d90}}.bueno{{border-color:#393}}svg{{max-width:100%;height:auto}}</style></head><body>\n<h1>{t}</h1>\n"
    );
    for s in &inf.secciones {
        match s {
            Seccion::Texto(x) => o += &format!("<p>{}</p>\n", html_escapar(x)),
            Seccion::Aviso(n, x) => o += &format!("<p class=\"aviso {}\">{}</p>\n", html_escapar(n), html_escapar(x)),
            Seccion::Kv(tit, items) => {
                o += &format!("<table><caption>{}</caption><tbody>\n", html_escapar(tit));
                for (a, b, u) in items {
                    o += &format!("<tr><td>{}</td><td>{}</td></tr>\n", html_escapar(a), html_escapar(format!("{b} {u}").trim()));
                }
                o += "</tbody></table>\n";
            }
            Seccion::Tabla(tit, col, fil) => {
                o += &format!("<table><caption>{}</caption><thead><tr>", html_escapar(tit));
                for c in col {
                    o += &format!("<th>{}</th>", html_escapar(c));
                }
                o += "</tr></thead><tbody>\n";
                for f in fil {
                    o += "<tr>";
                    for c in f {
                        o += &format!("<td>{}</td>", html_escapar(c));
                    }
                    o += "</tr>\n";
                }
                o += "</tbody></table>\n";
            }
            Seccion::Grafica(f) => o += &format!("<figure>{}</figure>\n", f.a_svg()),
        }
    }
    o + "</body></html>\n"
}

pub fn dibujar_informe(ui: &mut egui::Ui, inf: &mut Informe, c: &mut Compartido, base: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&inf.titulo).size(19.0).family(tema::semi()).color(tema::p().texto));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Exportar informe").on_hover_text("HTML con las tablas y las gráficas en SVG").clicked() {
                if let Some(r) = rfd::FileDialog::new().add_filter("HTML", &["html"]).set_file_name(format!("{}.html", nombre_de_archivo(&inf.titulo))).save_file() {
                    let _ = crate::app::escribir_atomico(&r, informe_a_html(inf).as_bytes());
                }
            }
        });
    });
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
                            for (etiqueta, sep) in [("Exportar CSV ,", ','), ("Exportar CSV ;", ';')] {
                                if ui.small_button(etiqueta).on_hover_text(format!("Guarda la tabla en un archivo CSV separado por «{sep}»")).clicked() {
                                    if let Some(r) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{}.csv", nombre_de_archivo(if t.is_empty() { "tabla" } else { t }))).save_file() {
                                        let _ = crate::app::escribir_atomico(&r, a_csv(col, fil, sep).as_bytes());
                                    }
                                }
                            }
                            if ui.small_button("Copiar CSV").clicked() {
                                ui.ctx().copy_text(a_csv(col, fil, ','));
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
    /// Importación en curso: (id de la petición, herramienta, parámetro, nombre de la tabla).
    importando: Option<(u64, String, String, String)>,
    /// Última tabla elegida en cada herramienta, para reiniciar las columnas cuando cambia.
    tabla_previa: HashMap<String, String>,
    busquedas: HashMap<String, Busqueda>,
    /// Valores del formulario con los que se calculó cada informe.
    calculados: HashMap<String, HashMap<String, String>>,
    /// Gráfica elegida en cada herramienta con varias.
    figura_sel: HashMap<String, usize>,
    /// «Calcular» pulsado en la cinta: se atiende en el siguiente dibujo de la página.
    calcular_pedido: bool,
    /// Herramientas abiertas como pestañas de documento, en orden de apertura.
    abiertas: Vec<String>,
    /// Avisos negros ocultados: herramienta y posición del aviso en el informe.
    ocultos: HashSet<(String, usize)>,
    /// Nombre recordado de cada variable de salida desactivada (herramienta, parámetro).
    salida_apagada: HashMap<(String, String), String>,
}

/// Estado del buscador de módulos CEC de un parámetro `modulo_cec`.
#[derive(Default)]
struct Busqueda {
    texto: String,
    resultados: Vec<String>,
    id: Option<u64>,
    error: String,
    buscado: bool,
}

/// Columnas que ofrece una variable como tabla: las de una tabla, o «0», «1»… para una matriz numérica.
fn columnas_de(v: &Variable) -> Option<Vec<String>> {
    if !v.columnas.is_empty() {
        return Some(v.columnas.clone());
    }
    if !v.tipo.starts_with("matriz") || v.tipo.contains("complex") {
        return None;
    }
    let dims: Vec<usize> = v.forma.split('×').filter_map(|d| d.trim().parse().ok()).collect();
    match dims.as_slice() {
        [_] => Some(vec!["0".into()]),
        [_, m] if *m > 0 => Some((0..*m).map(|j| j.to_string()).collect()),
        _ => None,
    }
}

fn valor_de(vals: &HashMap<String, String>, h: &Herramienta, nombre: &str) -> String {
    vals.get(nombre).cloned().or_else(|| h.params.iter().find(|p| p.nombre == nombre).map(|p| p.defecto.clone())).unwrap_or_default()
}

fn es_visible(p: &Param, vals: &HashMap<String, String>, h: &Herramienta) -> bool {
    p.visible_si.as_ref().is_none_or(|(otro, valor)| valor.split('|').any(|v| v == valor_de(vals, h, otro)))
}

/// Nombre de variable válido a partir del nombre de un archivo.
fn nombre_de_variable(ruta: &std::path::Path) -> String {
    let n: String = ruta.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "datos".into()).chars().map(|c| if c.is_alphanumeric() && c.is_ascii() { c } else { '_' }).collect();
    if n.is_empty() || n.starts_with(|c: char| c.is_ascii_digit()) {
        format!("t_{n}")
    } else {
        n
    }
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
            importando: None,
            tabla_previa: HashMap::new(),
            busquedas: HashMap::new(),
            calculados: HashMap::new(),
            figura_sel: HashMap::new(),
            calcular_pedido: false,
            abiertas: vec![],
            ocultos: HashSet::new(),
            salida_apagada: HashMap::new(),
        }
    }

    pub fn ocupada(&self) -> bool {
        self.id_activo.is_some()
    }

    pub fn recibir(&mut self, id: u64, ok: bool, v: &Value, err: &str, c: &mut Compartido) -> bool {
        if self.id_activo.as_ref().is_some_and(|a| a.0 == id) {
            let (_, h) = self.id_activo.take().unwrap();
            if ok {
                let mut inf = informe_desde(v);
                ajustar_informe(&h, &mut inf);
                self.informes.insert(h.clone(), inf);
                self.ocultos.retain(|(t, _)| *t != h);
                self.errores.remove(&h);
                c.fijar_variables(&v["variables"]);
            } else {
                // Un informe anterior parecería el resultado de esta ejecución
                self.informes.remove(&h);
                self.errores.insert(h, err.to_string());
            }
            return true;
        }
        if self.importando.as_ref().is_some_and(|a| a.0 == id) {
            let (_, h, param, nombre) = self.importando.take().unwrap();
            if ok {
                c.fijar_variables(v);
                self.valores.entry(h.clone()).or_default().insert(param, nombre);
                self.errores.remove(&h);
            } else {
                self.errores.insert(h, err.to_string());
            }
            return true;
        }
        if let Some(b) = self.busquedas.values_mut().find(|b| b.id == Some(id)) {
            b.id = None;
            b.buscado = ok;
            b.error.clear();
            b.resultados = if ok { v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() } else { vec![] };
            if !ok {
                b.error = err.to_string();
            }
            return true;
        }
        false
    }

    /// Ejecuta una herramienta con los valores actuales del formulario.
    pub fn ejecutar(&mut self, h: &Herramienta, motor: &mut Motor) {
        let vals = self.valores.entry(h.id.clone()).or_default();
        let mut m = serde_json::Map::new();
        let mut instantanea = HashMap::new();
        for p in &h.params {
            let t = vals.get(&p.nombre).cloned().unwrap_or_else(|| p.defecto.clone());
            m.insert(p.nombre.clone(), json!(t));
            instantanea.insert(p.nombre.clone(), t);
        }
        self.calculados.insert(h.id.clone(), instantanea);
        self.id_activo = Some((motor.llamar("ejecutar_herramienta", json!({"ident": h.id, "valores": Value::Object(m)})), h.id.clone()));
    }

    /// Nombre del módulo para el subtítulo de la pantalla.
    fn nombre_modulo(&self) -> &str {
        match self.modulo.as_str() {
            "potencia" => "Potencia",
            "fv" => "Solar",
            "estadistica" => "Estadística",
            "ml" => "Aprendizaje",
            "mantenimiento" => "Mantenimiento",
            _ => &self.titulo,
        }
    }

    /// Dibuja la página: pestañas de documento con las herramientas abiertas, título, fichas, tarjeta
    /// con la gráfica y tablas secundarias, y el panel de parámetros a la derecha (debajo si la
    /// ventana es estrecha).
    pub fn ui(&mut self, ui: &mut egui::Ui, todas: &[Herramienta], motor: &mut Motor, c: &mut Compartido) {
        let propias: Vec<Herramienta> = todas.iter().filter(|h| h.modulo == self.modulo).cloned().collect();
        if self.seleccion.is_none() {
            self.seleccion = propias.first().map(|h| h.id.clone());
        }
        self.abiertas.retain(|a| propias.iter().any(|h| h.id == *a));
        if let Some(s) = self.seleccion.clone() {
            if !self.abiertas.contains(&s) {
                self.abiertas.push(s);
            }
        }
        let pal = tema::p();
        let atajo = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) || std::mem::take(&mut self.calcular_pedido);
        let Some(id) = self.seleccion.clone() else { return };
        let Some(h) = propias.iter().find(|h| h.id == id) else { return };
        let mut inf = self.informes.remove(&id);
        let valores = self.calculados.get(&id).cloned().unwrap_or_default();
        let mut sel = self.figura_sel.get(&id).copied().unwrap_or(0);
        let vista = inf.as_ref().map(|i| Vista::de(&id, i, &valores, sel));
        let mut calcular = atajo && !self.ocupada();
        let lateral = ui.available_width() >= ui::ANCHO_APILAR;
        if lateral {
            egui::SidePanel::right(egui::Id::new(("parametros_lateral", &self.modulo))).exact_width(360.0).resizable(false).frame(egui::Frame::new().fill(pal.panel)).show_inside(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0_f32, pal.borde));
                self.panel_derecho(ui, h, motor, c, inf.as_ref().zip(vista.as_ref()), &mut calcular);
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.fondo)).show_inside(ui, |ui| {
            let pestanas: Vec<(Icono, String)> = self.abiertas.iter().map(|a| presentacion(a, propias.iter().find(|h| h.id == *a).map_or(a.as_str(), |h| h.titulo.as_str()))).collect();
            let refs: Vec<(Icono, &str)> = pestanas.iter().map(|(i, t)| (*i, t.as_str())).collect();
            let mut activa = self.abiertas.iter().position(|a| *a == id).unwrap_or(0);
            let cerrada = ui::pestanas_documento(ui, &refs, &mut activa, self.abiertas.len() > 1);
            if let Some(k) = cerrada {
                let quitada = self.abiertas.remove(k);
                if quitada == id {
                    self.seleccion = self.abiertas.get(k.min(self.abiertas.len() - 1)).cloned();
                }
            } else if self.abiertas.get(activa) != Some(&id) {
                self.seleccion = self.abiertas.get(activa).cloned();
            }
            let total = ui.available_height();
            let corto = presentacion(&h.id, &h.titulo).1;
            egui::ScrollArea::vertical().id_salt(("pagina", &id)).auto_shrink([false, false]).show(ui, |ui| {
                let y0 = ui.next_widget_position().y;
                ui::pagina(ui, |ui| {
                    ui::titulo_pantalla(ui, &corto, &format!("{} · {}", self.nombre_modulo(), h.grupo));
                    match (inf.as_mut(), vista.as_ref()) {
                        (Some(r), Some(v)) => {
                            ui::fila(ui, &[2; 6], 84.0, |i, ui| {
                                if let Some(f) = v.fichas.get(i) {
                                    ui::ficha_kpi(ui, &f.etiqueta, &f.valor, &f.unidad, &f.nota, None);
                                }
                            });
                            for (k, (_, texto)) in v.avisos.iter().enumerate() {
                                if !self.ocultos.contains(&(id.clone(), k)) && ui.push_id(k, |ui| ui::aviso_negro(ui, texto)).inner {
                                    self.ocultos.insert((id.clone(), k));
                                }
                            }
                            let alto = (total - (ui.next_widget_position().y - y0) - ui::MARGEN).max(380.0);
                            columna_resultados(ui, r, v, c, &mut sel, &id, alto);
                        }
                        _ => {
                            let alto = (total - (ui.next_widget_position().y - y0) - ui::MARGEN).max(300.0);
                            self.columna_vacia(ui, h, alto, &mut calcular);
                        }
                    }
                    if !lateral {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 640.0), Sense::hover());
                        let mut hijo = ui.new_child(UiBuilder::new().max_rect(r));
                        hijo.painter().rect(r, 0.0, pal.panel, Stroke::new(1.0_f32, pal.borde), egui::StrokeKind::Inside);
                        self.panel_derecho(&mut hijo, h, motor, c, inf.as_ref().zip(vista.as_ref()), &mut calcular);
                    }
                });
            });
        });
        self.figura_sel.insert(id.clone(), sel);
        if let Some(i) = inf {
            self.informes.insert(id, i);
        }
        if calcular && !self.ocupada() {
            self.ejecutar(h, motor);
        }
    }

    /// Tarjeta de la zona de resultados antes de calcular, mientras se calcula o tras un error.
    fn columna_vacia(&self, ui: &mut egui::Ui, h: &Herramienta, alto: f32, calcular: &mut bool) {
        let calculando = self.id_activo.as_ref().is_some_and(|a| a.1 == h.id);
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), alto), Sense::hover());
        let mut hijo = ui.new_child(UiBuilder::new().max_rect(r));
        ui::tarjeta_con_titulo(&mut hijo, "", |_| {}, |ui| {
            if let Some(e) = self.errores.get(&h.id) {
                banner(ui, "falla", e);
            }
            if calculando {
                ui.add_space((ui.available_height() / 2.0 - 30.0).max(8.0));
                ui.vertical_centered(|ui| {
                    ui.spinner();
                    ui.label(egui::RichText::new("Calculando…").color(tema::p().texto_2));
                });
            } else if ui::estado_vacio(ui, &format!("{}\n\nPulsa «Calcular» para ver los resultados.", self.introduccion), "Calcular") {
                *calcular = true;
            }
        });
    }

    /// Panel de parámetros: filas de la herramienta, mapa P(G, T) si el informe lo trae y pie con
    /// «Exportar informe» y «Calcular».
    fn panel_derecho(&mut self, ui: &mut egui::Ui, h: &Herramienta, motor: &mut Motor, c: &Compartido, res: Option<(&Informe, &Vista)>, calcular: &mut bool) {
        let ocupada = self.ocupada();
        let mut pie = |ui: &mut egui::Ui| {
            ui.add_enabled_ui(!ocupada, |ui| {
                if ui::boton(ui, ui::EstiloBoton::Principal, None, "Calcular", "Ctrl Intro", false).clicked() {
                    *calcular = true;
                }
            });
            ui.add_enabled_ui(res.is_some(), |ui| {
                if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Exportar informe", "", false).on_hover_text("HTML con las tablas y las gráficas en SVG").clicked() {
                    if let Some((inf, _)) = res {
                        guardar_informe_html(inf);
                    }
                }
            });
        };
        ui::panel_parametros(
            ui,
            "Parámetros",
            &h.titulo,
            &h.doc,
            |ui| {
                self.parametros(ui, h, motor, c);
                if let Some((_, v)) = res.filter(|(_, v)| v.mapa.is_some()) {
                    ui.add_space(8.0);
                    tema::separador(ui);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui::titulo_seccion(ui, &v.mapa_titulo);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(egui::RichText::new(&v.mapa_rango).font(tema::fuente_mono(11.0)).color(tema::p().texto_2));
                        });
                    });
                    if let Some(m) = &v.mapa {
                        m.mostrar(ui, 190.0);
                    }
                }
            },
            Some(&mut pie),
        );
    }

    /// Preselecciona la única tabla disponible y reinicia las columnas que no existen en la tabla recién elegida.
    fn sincronizar_tabla(&mut self, h: &Herramienta, tablas: &[(String, Vec<String>)]) {
        let Some(pt) = h.params.iter().find(|p| p.tipo == "tabla") else { return };
        let vals = self.valores.entry(h.id.clone()).or_default();
        let mut actual = valor_de(vals, h, &pt.nombre);
        if !tablas.iter().any(|t| t.0 == actual) && tablas.len() == 1 {
            actual = tablas[0].0.clone();
            vals.insert(pt.nombre.clone(), actual.clone());
        }
        let previa = self.tabla_previa.insert(h.id.clone(), actual.clone());
        if previa.is_some_and(|p| p != actual) {
            let cols = tablas.iter().find(|t| t.0 == actual).map(|t| t.1.clone()).unwrap_or_default();
            for p in h.params.iter().filter(|p| p.tipo == "columna" || p.tipo == "columna_opc") {
                if !cols.contains(&valor_de(vals, h, &p.nombre)) {
                    vals.insert(p.nombre.clone(), if cols.contains(&p.defecto) { p.defecto.clone() } else { String::new() });
                }
            }
        }
    }

    /// Filas de parámetros: los de salida van al final, bajo su propio encabezado.
    fn parametros(&mut self, ui: &mut egui::Ui, h: &Herramienta, motor: &mut Motor, c: &Compartido) {
        // Tablas disponibles para los selectores: tablas y matrices numéricas del espacio de trabajo
        let tablas: Vec<(String, Vec<String>)> = c.variables.iter().filter_map(|v| columnas_de(v).map(|cols| (v.nombre.clone(), cols))).collect();
        self.sincronizar_tabla(h, &tablas);
        let vals = self.valores.entry(h.id.clone()).or_default();
        let tabla_actual = h.params.iter().find(|p| p.tipo == "tabla").map(|p| valor_de(vals, h, &p.nombre)).unwrap_or_default();
        let columnas: Vec<String> = tablas.iter().find(|t| t.0 == tabla_actual).map(|t| t.1.clone()).unwrap_or_default();
        ui.spacing_mut().item_spacing.y = 8.0;
        let mut encabezado_salidas = false;
        let ordenados = h.params.iter().filter(|p| p.tipo != "salida").chain(h.params.iter().filter(|p| p.tipo == "salida"));
        for p in ordenados {
            if !es_visible(p, vals, h) {
                continue;
            }
            if p.tipo == "salida" && !encabezado_salidas {
                encabezado_salidas = true;
                ui.add_space(6.0);
                tema::separador(ui);
                ui.add_space(6.0);
                ui::titulo_seccion(ui, "Guardar en el espacio de trabajo");
            }
            let (etiqueta, unidad) = separar_unidad(&p.etiqueta);
            let mut t = valor_de(vals, h, &p.nombre);
            let anterior = t.clone();
            match p.tipo.as_str() {
                "num" | "entero" => {
                    ui::fila_parametro(ui, etiqueta, &mut t, unidad);
                }
                "salida" => {
                    let clave = (h.id.clone(), p.nombre.clone());
                    let apagada = self.salida_apagada.get(&clave).cloned();
                    let mut activo = apagada.is_none();
                    let mut nombre = apagada.unwrap_or_else(|| t.clone());
                    ui::fila_guardar(ui, &mut activo, &etiqueta_salida(&p.etiqueta), &mut nombre);
                    if activo {
                        self.salida_apagada.remove(&clave);
                        t = nombre;
                    } else {
                        self.salida_apagada.insert(clave, nombre);
                        t.clear();
                    }
                }
                "opcion" => {
                    ui::desplegable(ui, etiqueta, &p.opciones, &mut t);
                }
                "tabla" => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(etiqueta).size(13.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Importar…", "", true).on_hover_text("Lee un CSV, TSV, TXT o Parquet del ordenador y lo deja en el espacio de trabajo").clicked() {
                                if let Some(r) = rfd::FileDialog::new().add_filter("Datos", &["csv", "tsv", "txt", "parquet"]).pick_file() {
                                    let nombre = nombre_de_variable(&r);
                                    let id = motor.llamar("importar_tabla", json!({"ruta": r.display().to_string(), "nombre": nombre}));
                                    self.importando = Some((id, h.id.clone(), p.nombre.clone(), nombre));
                                }
                            }
                        });
                    });
                    let nombres: Vec<String> = tablas.iter().map(|(n, _)| n.clone()).collect();
                    selector(ui, (&h.id, &p.nombre), "elegir…", &mut t, &nombres, false);
                }
                "columna" | "columna_opc" => {
                    ui.label(egui::RichText::new(etiqueta).size(13.0));
                    selector(ui, (&h.id, &p.nombre), "—", &mut t, &columnas, p.tipo == "columna_opc");
                }
                "ruta" => {
                    // Opciones: «guardar», «guardar_si:param=valor» (diálogo de guardar) y «ext:a,b» (filtro de extensiones)
                    let guardar = p.opciones.iter().any(|o| o == "guardar" || o.strip_prefix("guardar_si:").and_then(|c| c.split_once('=')).is_some_and(|(otro, valor)| valor_de(vals, h, otro) == valor));
                    let exts: Vec<String> = p.opciones.iter().find_map(|o| o.strip_prefix("ext:")).map(|e| e.split(',').map(String::from).collect()).unwrap_or_default();
                    ui.label(egui::RichText::new(&p.etiqueta).size(13.0));
                    ui.horizontal(|ui| {
                        let ancho = ui.available_width() - 34.0;
                        ui.add(egui::TextEdit::singleline(&mut t).font(tema::fuente_mono(12.5)).desired_width(ancho));
                        if ui.small_button("…").clicked() {
                            let mut d = rfd::FileDialog::new();
                            if !exts.is_empty() {
                                d = d.add_filter("Archivo", &exts);
                            }
                            let elegido = if guardar {
                                let nombre = std::path::Path::new(&t).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                d.set_file_name(nombre).save_file()
                            } else {
                                d.pick_file()
                            };
                            if let Some(r) = elegido {
                                t = r.display().to_string();
                            }
                        }
                    });
                }
                "modulo_cec" => {
                    let b = self.busquedas.entry(format!("{}|{}", h.id, p.nombre)).or_default();
                    ui.label(egui::RichText::new(&p.etiqueta).size(13.0));
                    ui.horizontal(|ui| {
                        let ancho = ui.available_width() - 72.0;
                        let r = ui.add(egui::TextEdit::singleline(&mut b.texto).hint_text("buscar en CEC").desired_width(ancho));
                        let pulsado = ui.button("Buscar").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if pulsado && !b.texto.trim().is_empty() && b.id.is_none() {
                            b.id = Some(motor.llamar("buscar_modulos_cec", json!({"texto": b.texto.trim()})));
                        }
                        if b.id.is_some() {
                            ui.spinner();
                        }
                    });
                    egui::ComboBox::from_id_salt((&h.id, &p.nombre)).selected_text(&t).width(ui.available_width() - 8.0).truncate().show_ui(ui, |ui| {
                        for o in &p.opciones {
                            ui.selectable_value(&mut t, o.clone(), o);
                        }
                        if !b.resultados.is_empty() {
                            ui.separator();
                            ui.label(egui::RichText::new(format!("Base CEC: {} resultados", b.resultados.len())).small().color(tema::p().texto_2));
                            for o in &b.resultados {
                                ui.selectable_value(&mut t, o.clone(), o);
                            }
                        }
                    });
                    if !b.error.is_empty() {
                        ui.colored_label(tema::p().falla, &b.error);
                    } else if b.buscado && b.resultados.is_empty() {
                        ui.label(egui::RichText::new("Sin coincidencias en la base CEC").small().color(tema::p().texto_2));
                    }
                }
                _ => {
                    ui.label(egui::RichText::new(etiqueta).size(13.0));
                    ui.add(egui::TextEdit::singleline(&mut t).font(tema::fuente_mono(12.5)).desired_width(f32::INFINITY).margin(egui::Margin::symmetric(8, 4)));
                }
            }
            if t != anterior {
                vals.insert(p.nombre.clone(), t);
            }
        }
    }

    /// Fija un valor del formulario de una herramienta, para acciones rápidas.
    pub fn fijar(&mut self, id: &str, param: &str, valor: &str) {
        self.valores.entry(id.to_string()).or_default().insert(param.to_string(), valor.to_string());
    }

    /// Olvida los informes y errores calculados: pertenecían al proyecto anterior.
    pub fn limpiar_informes(&mut self) {
        self.informes.clear();
        self.errores.clear();
    }

    /// Último informe de una herramienta, si se ha ejecutado con éxito.
    pub fn informe(&self, id: &str) -> Option<&Informe> {
        self.informes.get(id)
    }

    pub fn seleccionar(&mut self, id: &str) {
        self.seleccion = Some(id.to_string());
    }

    /// Cinta del módulo: las herramientas como botones grandes y el grupo Resultados.
    pub fn cinta(&mut self, ui: &mut egui::Ui, todas: &[Herramienta]) {
        let propias: Vec<&Herramienta> = todas.iter().filter(|h| h.modulo == self.modulo).collect();
        if self.seleccion.is_none() {
            self.seleccion = propias.first().map(|h| h.id.clone());
        }
        ui::grupo_cinta(ui, "Herramientas", |ui| {
            for h in &propias {
                let (icono, corto) = presentacion(&h.id, &h.titulo);
                if ui::boton_cinta(ui, icono, &corto, self.seleccion.as_deref() == Some(h.id.as_str())).on_hover_text(&h.titulo).clicked() {
                    self.seleccion = Some(h.id.clone());
                }
            }
        });
        let hay_informe = self.seleccion.as_ref().is_some_and(|id| self.informes.contains_key(id));
        let hay_tabla = self.seleccion.as_ref().and_then(|id| self.informes.get(id)).is_some_and(|i| i.secciones.iter().any(|s| matches!(s, Seccion::Tabla(..))));
        let mut exportar = false;
        let mut copiar = false;
        let ocupada = self.ocupada();
        let mut calcular = false;
        ui::grupo_cinta(ui, "Resultados", |ui| {
            calcular = ui::boton_cinta(ui, Icono::Calcular, "Calcular", false).on_hover_text("Calcular · Ctrl Intro").clicked() && !ocupada;
            match ui::columna_cinta(ui, &[(Icono::Informe, "Exportar informe", hay_informe), (Icono::Descargar, "Copiar CSV", hay_tabla)]) {
                Some(0) => exportar = true,
                Some(_) => copiar = true,
                None => {}
            }
        });
        self.calcular_pedido |= calcular;
        if let Some(inf) = self.seleccion.as_ref().and_then(|id| self.informes.get(id)) {
            if exportar {
                guardar_informe_html(inf);
            }
            if copiar {
                if let Some(Seccion::Tabla(_, col, fil)) = inf.secciones.iter().find(|s| matches!(s, Seccion::Tabla(..))) {
                    ui.ctx().copy_text(a_csv(col, fil, ','));
                }
            }
        }
    }

    /// Herramientas con resultado o error, para el esquema del proyecto: id, título corto y estado.
    pub fn resultados_esquema(&self, todas: &[Herramienta]) -> Vec<(String, String, ui::EstadoNodo, String)> {
        todas
            .iter()
            .filter(|h| h.modulo == self.modulo)
            .filter_map(|h| {
                let (_, corto) = presentacion(&h.id, &h.titulo);
                if self.errores.contains_key(&h.id) {
                    Some((h.id.clone(), corto, ui::EstadoNodo::Pendiente, "error".to_string()))
                } else if let Some(inf) = self.informes.get(&h.id) {
                    let aviso = inf.secciones.iter().any(|s| matches!(s, Seccion::Aviso(n, _) if n == "alerta" || n == "falla"));
                    Some((h.id.clone(), corto, if aviso { ui::EstadoNodo::Pendiente } else { ui::EstadoNodo::Listo }, if aviso { "aviso".to_string() } else { String::new() }))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Herramienta abierta.
    pub fn seleccion(&self) -> Option<&str> {
        self.seleccion.as_deref()
    }
}

/// Icono y título corto de una herramienta en la cinta y en el esquema del proyecto.
pub fn presentacion(id: &str, titulo: &str) -> (Icono, String) {
    let (icono, corto) = match id {
        "pot_perdidas" => (Icono::Rayo, "Pérdidas del inversor"),
        "pot_lcl" => (Icono::Onda, "Filtro LCL"),
        "ctl_bode" => (Icono::Bode, "Bode y márgenes"),
        "ctl_escalon" => (Icono::Escalon, "Respuesta al escalón"),
        "ctl_lugar" => (Icono::Raices, "Lugar de las raíces"),
        "ctl_pi" => (Icono::Pi, "Ajuste de un PI"),
        "fv_hoja" => (Icono::Informe, "Hoja de datos"),
        "fv_curvas" => (Icono::Curva, "Curvas I-V y P-V"),
        "fv_sombra" => (Icono::Sombra, "Sombreado parcial"),
        "fv_sol" => (Icono::Brujula, "Posición solar"),
        "es_muestra" => (Icono::Datos, "Datos de ejemplo"),
        "es_descriptiva" => (Icono::Lista, "Descriptiva"),
        "es_ajuste" => (Icono::Distribucion, "Ajuste de distribuciones"),
        "es_weibull" => (Icono::Curva, "Weibull"),
        "es_hipotesis" => (Icono::Validar, "Pruebas de hipótesis"),
        "es_regresion" => (Icono::Dispersion, "Regresión lineal"),
        "es_no_lineal" => (Icono::Onda, "Regresión no lineal"),
        "es_anova" => (Icono::Histograma, "ANOVA"),
        "es_serie" => (Icono::Anomalias, "Series y ARIMA"),
        "es_spc" => (Icono::Grafica, "Control de procesos"),
        "ml_preparar" => (Icono::Datos, "Preparar datos"),
        "ml_regresion" => (Icono::Dispersion, "Regresión"),
        "ml_clasificacion" => (Icono::Clasificacion, "Clasificación"),
        "ml_clustering" => (Icono::Clustering, "Clustering"),
        "ml_pca" => (Icono::Pca, "PCA"),
        "ml_anomalias" => (Icono::Anomalias, "Anomalías"),
        "ml_guardar" => (Icono::Guardar, "Guardar o cargar"),
        "ml_predecir" => (Icono::Flecha, "Aplicar modelo"),
        "mt_muestra" => (Icono::Datos, "Datos de muestra"),
        "mt_importar" => (Icono::Importar, "Importar SCADA"),
        "mt_kpi" => (Icono::Grafica, "KPIs de la planta"),
        "mt_fallas" => (Icono::Anomalias, "Detección de fallas"),
        "mt_fiabilidad" => (Icono::Curva, "Fiabilidad"),
        _ => (Icono::Informe, titulo),
    };
    (icono, corto.to_string())
}


// ---------------------------------------------------------------- plantilla de herramienta

/// Ficha de resultado de la fila superior.
#[derive(Clone, Default, Debug, PartialEq)]
struct Ficha {
    etiqueta: String,
    valor: String,
    unidad: String,
    nota: String,
}

impl Ficha {
    fn nueva(etiqueta: &str, valor: &str, unidad: &str, nota: &str) -> Ficha {
        Ficha { etiqueta: etiqueta.into(), valor: valor.into(), unidad: unidad.into(), nota: nota.into() }
    }

    /// Ficha de un par del informe. Una «unidad» con espacios es una frase (por ejemplo, «falta
    /// potencia DC» junto a «No disponible») y se muestra como nota bajo el valor.
    fn de_kv(etiqueta: &str, valor: &str, unidad: &str) -> Ficha {
        let (unidad, nota) = unidad_o_nota(unidad);
        Ficha::nueva(etiqueta, valor, unidad, nota)
    }
}

/// Contenido de la tarjeta grande de la zona de resultados.
#[derive(Default, Debug, PartialEq)]
enum Principal {
    /// Una o dos gráficas (índices de sección); con dos, van una al lado de la otra.
    Figuras(Vec<usize>),
    /// Tabla de confusión (índice de sección) dibujada como mapa de calor.
    Confusion(usize),
    /// Tabla (índice de sección) cuando el informe no tiene gráficas.
    Tabla(usize),
    #[default]
    Ninguna,
}

/// Tablas que van bajo la tarjeta principal.
#[derive(Debug, PartialEq)]
enum Secundaria {
    Tabla(usize),
    /// Sección `kv` completa, cuando tiene más valores que fichas.
    Kv(usize),
}

/// Cómo se reparte un informe en la plantilla; los índices son posiciones en `Informe::secciones`.
#[derive(Default)]
struct Vista {
    fichas: Vec<Ficha>,
    avisos: Vec<(String, String)>,
    textos: Vec<String>,
    /// Variables creadas por la herramienta, para el pie de la tarjeta.
    pie: String,
    principal: Principal,
    secundarias: Vec<Secundaria>,
    /// Gráficas entre las que elegir con pestañas pequeñas (índice de sección y título).
    selector: Vec<(usize, String)>,
    deco: Decoracion,
    nota: String,
    titulo: String,
    mapa: Option<MapaCalor>,
    mapa_titulo: String,
    mapa_rango: String,
}

/// Número que escribe el motor (punto decimal) o el formulario.
fn numero_de(t: &str) -> Option<f64> {
    t.trim().replace(',', ".").parse().ok()
}

/// Valor con cinco cifras significativas y sin ceros sobrantes: 2095.4, 500.
fn cifras(x: f64) -> String {
    if x == 0.0 || !x.is_finite() {
        return format!("{x}");
    }
    let dec = (4 - x.abs().log10().floor() as i32).clamp(0, 8) as usize;
    let s = format!("{x:.dec$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Etiqueta corta de una variable de salida: «Variable de los valores de diseño» pasa a «Valores de diseño».
fn etiqueta_salida(e: &str) -> String {
    let sin = ["Variable de la ", "Variable del ", "Variable de los ", "Variable de las ", "Variable de "].iter().find_map(|p| e.strip_prefix(p)).unwrap_or(e);
    let mut c = sin.chars();
    c.next().map_or(String::new(), |a| a.to_uppercase().collect::<String>() + c.as_str())
}

/// Desplegable de tabla o columna a todo el ancho; `vacio` es el texto mientras no hay valor y
/// `con_vacio` añade la opción «—».
fn selector(ui: &mut egui::Ui, id: impl std::hash::Hash, vacio: &str, valor: &mut String, opciones: &[String], con_vacio: bool) {
    let texto = if valor.is_empty() { vacio.to_string() } else { valor.clone() };
    egui::ComboBox::from_id_salt(id).width(ui.available_width() - 8.0).selected_text(egui::RichText::new(texto).size(13.0)).truncate().show_ui(ui, |ui| {
        if con_vacio {
            ui.selectable_value(valor, String::new(), "—");
        }
        for o in opciones {
            ui.selectable_value(valor, o.clone(), o);
        }
    });
}

/// Separa una etiqueta «Nombre [unidad]» en nombre y unidad.
fn separar_unidad(e: &str) -> (&str, &str) {
    match e.rfind(" [") {
        Some(a) if e.ends_with(']') => (&e[..a], &e[a + 2..e.len() - 1]),
        _ => (e, ""),
    }
}

/// Posición, columnas y filas de una tabla del informe.
type TablaRef<'a> = (usize, &'a [String], &'a [Vec<String>]);

impl Informe {
    /// Valor de un par de una sección `kv` cuyo título empieza por `titulo`.
    fn kv(&self, titulo: &str, etiqueta: &str) -> Option<&(String, String, String)> {
        self.secciones.iter().find_map(|s| match s {
            Seccion::Kv(t, items) if t.starts_with(titulo) => items.iter().find(|i| i.0 == etiqueta),
            _ => None,
        })
    }

    /// Primera tabla cuyo título empieza por `titulo`: posición, columnas y filas.
    fn tabla(&self, titulo: &str) -> Option<TablaRef<'_>> {
        self.secciones.iter().enumerate().find_map(|(i, s)| match s {
            Seccion::Tabla(t, col, fil) if t.starts_with(titulo) => Some((i, col.as_slice(), fil.as_slice())),
            _ => None,
        })
    }

    /// Posición de la primera gráfica 2D cuyo título empieza por `titulo`.
    fn figura(&self, titulo: &str) -> Option<usize> {
        self.secciones.iter().position(|s| matches!(s, Seccion::Grafica(f) if matches!(f, Figura::D2(_)) && f.titulo().starts_with(titulo)))
    }
}

impl Vista {
    /// Reparte el informe de la herramienta `id`. `valores` son los del formulario al calcular.
    fn de(id: &str, inf: &Informe, valores: &HashMap<String, String>, sel: usize) -> Vista {
        let mut v = Vista::default();
        let mut items: Vec<&(String, String, String)> = vec![];
        let mut figuras = vec![];
        let mut tablas = vec![];
        let mut kvs = vec![];
        for (i, s) in inf.secciones.iter().enumerate() {
            match s {
                Seccion::Aviso(n, t) if n == "bueno" && t.starts_with("Variables creadas") => v.pie = t.clone(),
                Seccion::Aviso(n, t) => v.avisos.push((n.clone(), t.clone())),
                Seccion::Texto(t) => v.textos.push(t.clone()),
                Seccion::Kv(_, it) => {
                    items.extend(it.iter());
                    kvs.push(i);
                }
                Seccion::Tabla(..) => tablas.push(i),
                Seccion::Grafica(f) => figuras.push((i, f.titulo().to_string())),
            }
        }
        v.fichas = items.iter().take(6).map(|(a, b, u)| Ficha::de_kv(a, b, u)).collect();
        if items.len() > 6 {
            v.secundarias.extend(kvs.iter().map(|i| Secundaria::Kv(*i)));
        }
        v.principal = match (figuras.get(sel.min(figuras.len().saturating_sub(1))), tablas.first()) {
            (Some((i, _)), _) => Principal::Figuras(vec![*i]),
            (None, Some(i)) => Principal::Tabla(*i),
            _ => Principal::Ninguna,
        };
        if figuras.len() > 1 {
            v.selector = figuras;
        }
        v.secundarias.extend(tablas.iter().map(|i| Secundaria::Tabla(*i)));
        v.secundarias.retain(|s| !matches!((s, &v.principal), (Secundaria::Tabla(a), Principal::Tabla(b)) if a == b));
        match id {
            "pot_lcl" => v.lcl(inf, valores),
            "fv_curvas" => v.curvas_fv(inf, valores),
            "ml_clasificacion" => v.clasificacion(inf),
            "mt_kpi" => v.kpis_planta(inf),
            _ => {}
        }
        v
    }

    /// Filtro LCL: marcas de f_red, f_res y f_sw, franja recomendada para f_res y ficha de comprobación.
    fn lcl(&mut self, inf: &Informe, valores: &HashMap<String, String>) {
        let datos = (
            inf.kv("Diseño", "Frecuencia de resonancia").and_then(|k| numero_de(&k.1)),
            valores.get("f0").and_then(|t| numero_de(t)),
            valores.get("fsw").and_then(|t| numero_de(t)),
            inf.figura("Respuesta en frecuencia"),
        );
        let (Some(fr), Some(f0), Some(fsw), Some(fig)) = datos else { return };
        self.deco = Decoracion {
            marcas: vec![
                MarcaV { x: f0, etiqueta: "f red".into(), color: None },
                MarcaV { x: fr, etiqueta: "f res".into(), color: None },
                MarcaV { x: fsw, etiqueta: "f sw".into(), color: None },
            ],
            // Sin franja cuando no existe zona recomendada (10·f_red ≥ f_sw/2): sombrearla invertida engañaría
            franjas: if 10.0 * f0 < fsw / 2.0 { vec![Franja { x0: 10.0 * f0, x1: fsw / 2.0 }] } else { vec![] },
        };
        if !self.deco.franjas.is_empty() {
            self.nota = "zona recomendada para f_res".into();
        }
        self.principal = Principal::Figuras(vec![fig]);
        let cumple = 10.0 * f0 < fr && fr < fsw / 2.0;
        self.fichas.truncate(5);
        self.fichas.push(Ficha::nueva("Comprobación", if cumple { "Correcto" } else { "Incumple" }, "", &format!("{} < {} < {} Hz", cifras(10.0 * f0), cifras(fr), cifras(fsw / 2.0))));
    }

    /// Curvas solares: I-V y P-V juntas, mapa P(G, T) bajo los parámetros y fichas con la potencia máxima.
    fn curvas_fv(&mut self, inf: &Informe, valores: &HashMap<String, String>) {
        let (Some(iv), Some(pv)) = (inf.figura("Curvas I-V"), inf.figura("Curvas P-V")) else { return };
        self.principal = Principal::Figuras(vec![iv, pv]);
        self.selector.clear();
        self.titulo = inf.titulo.split(" (").next().unwrap_or(&inf.titulo).to_string();
        if let Some(Seccion::Grafica(Figura::D2(f))) = inf.secciones.get(pv) {
            let comparacion: Vec<&str> = f.series.iter().filter(|s| s.tipo == TipoSerie::Discontinua).map(|s| s.nombre.as_str()).collect();
            if !comparacion.is_empty() {
                self.nota = format!("discontinuas: {} °C", comparacion.iter().map(|n| n.trim_end_matches("°C").trim()).collect::<Vec<_>>().join(", "));
            }
        }
        if let Some(Seccion::Grafica(Figura::D3(f))) = inf.secciones.iter().find(|s| matches!(s, Seccion::Grafica(Figura::D3(_)))) {
            if let Some(m) = MapaCalor::desde_malla(f) {
                let (lo, hi) = m.rango().unwrap_or((0.0, 0.0));
                let unidad = separar_unidad(&f.zlabel).1;
                self.mapa_titulo = "Mapa P(G, T)".into();
                self.mapa_rango = format!("{} – {} {unidad}", grafica::numero(lo), grafica::numero(hi));
                self.mapa = Some(m);
            }
        }
        if let Some((_, col, fil)) = inf.tabla("Máxima potencia") {
            let pos = |c: &str| col.iter().position(|x| x.starts_with(c));
            if let (Some(g), Some(t), Some(p), 9) = (pos("G"), pos("T"), pos("P_mp"), fil.len()) {
                // Las cinco primeras filas varían G a T fija; las cuatro últimas, T a G fija
                let celda = |f: &Vec<String>, i: usize| f.get(i).cloned().unwrap_or_default();
                let ficha = |f: &Vec<String>| Ficha::nueva(&format!("Pmp · {} W/m², {} °C", celda(f, g), celda(f, t)), &celda(f, p), "W", "");
                let mut fichas = vec![];
                if let Some(m) = valores.get("modelo") {
                    let (nombre, nota) = m.split_once(" (").map_or((m.as_str(), ""), |(a, b)| (a, b.trim_end_matches(')')));
                    fichas.push(Ficha::nueva("Módulo", nombre, "", nota));
                }
                fichas.extend([4, 0, 5, 8].map(|i| ficha(&fil[i])));
                self.fichas = fichas;
            }
        }
    }

    /// KPIs de la planta: los cinco de «Planta», la cobertura como nota de la energía y una sexta
    /// ficha con el número de inversores y sus nombres, tomados de la tabla por inversor.
    fn kpis_planta(&mut self, inf: &Informe) {
        let Some((_, _, fil)) = inf.tabla("Por inversor") else { return };
        if self.fichas.len() < 5 {
            return;
        }
        self.fichas.truncate(5);
        if let (Some(c), Some(f)) = (inf.kv("Cobertura de los datos", "Lecturas en el periodo"), self.fichas.iter_mut().find(|f| f.etiqueta == "Energía producida")) {
            f.nota = format!("lecturas en el {} del periodo", c.1);
        }
        let nombres: Vec<&str> = fil.iter().filter_map(|f| f.first().map(String::as_str)).take(4).collect();
        self.fichas.push(Ficha::nueva("Inversores", &fil.len().to_string(), "", &nombres.join(" · ")));
    }

    /// Clasificación: matriz de confusión como mapa de calor y fichas de validación y entrenamiento.
    fn clasificacion(&mut self, inf: &Informe) {
        let Some((i, _, _)) = inf.tabla("Matriz de confusión") else { return };
        self.principal = Principal::Confusion(i);
        self.selector.clear();
        self.secundarias.retain(|s| *s != Secundaria::Tabla(i));
        let val = |e: &str| inf.kv("Validación", e);
        let ent = |e: &str| inf.kv("Entrenamiento", e);
        let datos = (val("Exactitud"), val("F1 macro"), ent("Exactitud"), ent("F1 macro"), val("Modelo"), val("n usado / n total"));
        let (Some(ev), Some(fv), Some(ee), Some(fe), Some(m), Some(n)) = datos else { return };
        let nota_modelo = match (val("Tipo de validación"), val("Semilla")) {
            (Some(t), Some(s)) => format!("{} · semilla {}", t.1, s.1),
            _ => String::new(),
        };
        let nota_n = val("Filas validadas").map(|f| format!("{} filas validadas", f.1)).unwrap_or_default();
        self.fichas = vec![
            Ficha::nueva("Exactitud · validación", &ev.1, &ev.2, ""),
            Ficha::nueva("F1 macro · validación", &fv.1, &fv.2, ""),
            Ficha::nueva("Exactitud · entrenamiento", &ee.1, &ee.2, ""),
            Ficha::nueva("F1 macro · entrenamiento", &fe.1, &fe.2, ""),
            Ficha::nueva("Modelo", &m.1, "", &nota_modelo),
            Ficha::nueva("n usado / total", &n.1, "", &nota_n),
        ];
    }
}

/// Separa una unidad de verdad («MWh») de una frase explicativa («falta potencia DC»).
///
/// Returns:
///     (unidad, nota): la frase va en la nota y la unidad queda vacía.
pub fn unidad_o_nota(unidad: &str) -> (&str, &str) {
    if unidad.contains(' ') {
        ("", unidad)
    } else {
        (unidad, "")
    }
}

/// Ajustes del informe que dependen de la herramienta, aplicados una vez al recibirlo.
fn ajustar_informe(id: &str, inf: &mut Informe) {
    if id != "fv_curvas" {
        return;
    }
    // Las curvas a otras temperaturas se dibujan discontinuas para distinguirlas de las de irradiancia
    for s in &mut inf.secciones {
        if let Seccion::Grafica(Figura::D2(f)) = s {
            f.series.iter_mut().filter(|s| s.nombre.ends_with("°C")).for_each(|s| s.tipo = TipoSerie::Discontinua);
        }
    }
}

/// Mapa de calor de una tabla de confusión: primera columna con la clase real, el resto con los recuentos.
fn mapa_confusion(col: &[String], fil: &[Vec<String>]) -> Option<MapaCalor> {
    let z: Vec<Vec<f64>> = fil.iter().map(|f| f.iter().skip(1).map(|x| x.trim().parse().ok()).collect::<Option<_>>()).collect::<Option<_>>()?;
    if z.is_empty() || z.iter().any(|f| f.len() != col.len().saturating_sub(1)) {
        return None;
    }
    Some(MapaCalor { filas: fil.iter().map(|f| f.first().cloned().unwrap_or_default()).collect(), columnas: col.get(1..).map(<[String]>::to_vec).unwrap_or_default(), titulo_filas: String::new(), titulo_columnas: String::new(), z, estilo: EstiloMapa::Confusion })
}

/// Pide el archivo y guarda el informe en HTML.
fn guardar_informe_html(inf: &Informe) {
    if let Some(r) = rfd::FileDialog::new().add_filter("HTML", &["html"]).set_file_name(format!("{}.html", nombre_de_archivo(&inf.titulo))).save_file() {
        let _ = crate::app::escribir_atomico(&r, informe_a_html(inf).as_bytes());
    }
}

/// Aviso del informe sobre la gráfica: punto de estado y texto, con el fondo del nivel.
fn banner(ui: &mut egui::Ui, nivel: &str, texto: &str) {
    let pal = tema::p();
    let n = tema::Nivel::desde(nivel);
    let fondo = if n == tema::Nivel::Falla { pal.falla_fondo } else { n.color().gamma_multiply(0.14) };
    egui::Frame::new().fill(fondo).stroke(Stroke::new(1.0_f32, n.color().gamma_multiply(0.45))).corner_radius(tema::radio()).inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (r, _) = ui.allocate_exact_size(Vec2::new(7.0, 14.0), Sense::hover());
            tema::punto(ui.painter(), r.center(), n.color());
            ui.label(egui::RichText::new(texto).size(13.0).color(pal.texto));
        });
    });
}

/// Acciones CSV de la cabecera de una tabla (se llama dentro de un diseño de derecha a izquierda).
fn acciones_tabla(ui: &mut egui::Ui, titulo: &str, col: &[String], fil: &[Vec<String>]) {
    for (etiqueta, sep) in [("CSV ;", ';'), ("CSV ,", ',')] {
        if ui::boton(ui, ui::EstiloBoton::Secundario, None, etiqueta, "", true).on_hover_text(format!("Guarda la tabla en un archivo CSV separado por «{sep}»")).clicked() {
            if let Some(r) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{}.csv", nombre_de_archivo(if titulo.is_empty() { "tabla" } else { titulo }))).save_file() {
                let _ = crate::app::escribir_atomico(&r, a_csv(col, fil, sep).as_bytes());
            }
        }
    }
    if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Copiar CSV", "", true).clicked() {
        ui.ctx().copy_text(a_csv(col, fil, ','));
    }
}

/// Filas de una tabla secundaria que se muestran sin desplazamiento vertical.
const FILAS_VISIBLES: usize = 12;

/// Tarjeta con una tabla desplazable y sus acciones CSV.
fn tarjeta_tabla(ui: &mut egui::Ui, titulo: &str, col: &[String], fil: &[Vec<String>], id: impl std::hash::Hash) {
    ui::tarjeta_con_titulo(
        ui,
        titulo,
        |ui| acciones_tabla(ui, titulo, col, fil),
        |ui| {
            egui::ScrollArea::both().id_salt(id).auto_shrink([false, false]).show(ui, |ui| tabla(ui, col, fil));
        },
    );
}

/// Alto natural de una tarjeta con una tabla de `filas` filas.
fn alto_natural(filas: usize) -> f32 {
    44.0 + 12.0 + 10.0 + 26.0 * (filas.min(FILAS_VISIBLES) + 1) as f32 + 12.0
}

/// Tarjeta principal con la altura `alto` y, debajo, las tablas secundarias a su altura natural.
fn columna_resultados(ui: &mut egui::Ui, inf: &mut Informe, v: &Vista, c: &mut Compartido, sel: &mut usize, base: &str, alto: f32) {
    let ancho = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::hover());
    let mut principal = ui.new_child(UiBuilder::new().max_rect(r));
    tarjeta_principal(&mut principal, inf, v, c, sel, base);
    for s in &v.secundarias {
        let (titulo, col, fil, clave) = match s {
            Secundaria::Tabla(i) => match &inf.secciones[*i] {
                Seccion::Tabla(t, col, fil) => (t.clone(), col.clone(), fil.clone(), ("t", *i)),
                _ => continue,
            },
            Secundaria::Kv(i) => match &inf.secciones[*i] {
                Seccion::Kv(t, items) => (
                    if t.is_empty() { "Resultados".into() } else { t.clone() },
                    vec!["Magnitud".into(), "Valor".into(), "Unidad".into()],
                    items.iter().map(|(a, b, u)| vec![a.clone(), b.clone(), u.clone()]).collect(),
                    ("kv", *i),
                ),
                _ => continue,
            },
        };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ancho, alto_natural(fil.len())), Sense::hover());
        let mut hijo = ui.new_child(UiBuilder::new().max_rect(rect));
        tarjeta_tabla(&mut hijo, &titulo, &col, &fil, (base, clave));
    }
}

/// Gráfica de la sección `i`, si lo es.
fn figura_mut(inf: &mut Informe, i: usize) -> Option<&mut Figura> {
    match inf.secciones.get_mut(i) {
        Some(Seccion::Grafica(f)) => Some(f),
        _ => None,
    }
}

/// Tarjeta grande de la zona de resultados: gráfica, matriz de confusión o tabla principal.
fn tarjeta_principal(ui: &mut egui::Ui, inf: &mut Informe, v: &Vista, c: &mut Compartido, sel: &mut usize, base: &str) {
    match &v.principal {
        Principal::Figuras(idx) => dibujar_figuras(ui, inf, v, idx, c, sel),
        Principal::Confusion(i) => dibujar_confusion(ui, inf, v, *i),
        Principal::Tabla(i) => {
            if let Seccion::Tabla(t, col, fil) = &inf.secciones[*i] {
                ui::tarjeta_con_titulo(
                    ui,
                    t,
                    |ui| acciones_tabla(ui, t, col, fil),
                    |ui| {
                        textos(ui, v);
                        egui::ScrollArea::both().id_salt((base, "principal")).auto_shrink([false, false]).show(ui, |ui| tabla(ui, col, fil));
                    },
                );
            }
        }
        Principal::Ninguna => ui::tarjeta_con_titulo(ui, "", |_| {}, |ui| textos(ui, v)),
    }
}

/// Textos del informe, uno bajo otro.
fn textos(ui: &mut egui::Ui, v: &Vista) {
    for t in &v.textos {
        ui.label(egui::RichText::new(t).size(13.0).color(tema::p().texto_2));
    }
}

/// Pie de la tarjeta: variables creadas, en gris y monoespaciada, alineadas a la derecha.
fn pie(ui: &mut egui::Ui, v: &Vista) {
    if v.pie.is_empty() {
        return;
    }
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.add(egui::Label::new(egui::RichText::new(&v.pie).font(tema::fuente_mono(11.5)).color(tema::p().texto_2)).truncate()).on_hover_text(&v.pie);
    });
}

/// Una gráfica, o dos lado a lado, en una tarjeta con título, leyenda y botones «Encuadrar», «PNG» y «SVG».
fn dibujar_figuras(ui: &mut egui::Ui, inf: &mut Informe, v: &Vista, idx: &[usize], c: &mut Compartido, sel: &mut usize) {
    let pal = tema::p();
    let titulo = if v.titulo.is_empty() { idx.first().and_then(|i| figura_mut(inf, *i)).map(|f| f.titulo().to_string()).unwrap_or_default() } else { v.titulo.clone() };
    let accion: RefCell<Option<Accion>> = RefCell::new(None);
    let encuadrar = Cell::new(false);
    let doble = idx.len() > 1;
    ui::tarjeta_con_titulo(
        ui,
        &titulo,
        |ui| {
            if !doble {
                if ui::boton(ui, ui::EstiloBoton::Secundario, None, "SVG", "", true).on_hover_text("Guardar como SVG").clicked() {
                    *accion.borrow_mut() = Some(Accion::Svg);
                }
                if ui::boton(ui, ui::EstiloBoton::Secundario, None, "PNG", "", true).on_hover_text("Guardar como PNG").clicked() {
                    *accion.borrow_mut() = Some(Accion::Png(Rect::NOTHING));
                }
            }
            if ui::boton(ui, ui::EstiloBoton::Secundario, Some(Icono::Encuadrar), "Encuadrar", "", true).on_hover_text("Volver a ver todos los datos (o doble clic)").clicked() {
                encuadrar.set(true);
            }
        },
        |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            if v.selector.len() > 1 {
                ui.horizontal_wrapped(|ui| {
                    for (k, (_, t)) in v.selector.iter().enumerate() {
                        if ui.selectable_label(k == *sel, t).clicked() {
                            *sel = k;
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                if let Some(Figura::D2(f)) = idx.first().and_then(|i| figura_mut(inf, *i)) {
                    f.leyenda_en_linea(ui, 6);
                }
                if !v.nota.is_empty() {
                    if v.deco.franjas.is_empty() {
                        ui.label(egui::RichText::new(&v.nota).size(12.0).color(pal.texto_2));
                    } else {
                        ui::leyenda(ui, pal.borde, &v.nota, ui::Muestra::Rayada);
                    }
                }
            });
            let alto = (ui.available_height() - if v.pie.is_empty() { 0.0 } else { 26.0 }).max(120.0);
            let ancho = ui.available_width();
            let (zona, _) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::hover());
            let celdas = ui::celdas(ancho, 2, ui::HUECO, &[1, 1]);
            for (k, &i) in idx.iter().enumerate() {
                let rect = if doble { Rect::from_min_size(Pos2::new(zona.left() + celdas[k].0, zona.top()), Vec2::new(celdas[k].1, alto)) } else { zona };
                let mut hijo = ui.new_child(UiBuilder::new().max_rect(rect));
                let Some(f) = figura_mut(inf, i) else { continue };
                let mut propia: Option<Accion> = if doble { None } else { accion.borrow_mut().take() };
                if doble {
                    hijo.horizontal(|ui| {
                        ui.label(egui::RichText::new(f.titulo().trim_start_matches("Curvas ")).family(tema::semi()).size(13.0).color(pal.texto));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui::boton(ui, ui::EstiloBoton::Secundario, None, "SVG", "", true).clicked() {
                                propia = Some(Accion::Svg);
                            }
                            if ui::boton(ui, ui::EstiloBoton::Secundario, None, "PNG", "", true).clicked() {
                                propia = Some(Accion::Png(Rect::NOTHING));
                            }
                        });
                    });
                }
                if encuadrar.get() {
                    if let Figura::D2(g) = f {
                        g.vista = None;
                    }
                }
                let arriba = hijo.cursor().top();
                let alto_figura = rect.bottom() - arriba;
                let lanzada = match f {
                    Figura::D2(g) => g.mostrar_decorada(&mut hijo, alto_figura, false, false, &v.deco),
                    Figura::D3(g) => g.mostrar(&mut hijo, alto_figura - 28.0),
                };
                let area_png = Rect::from_min_max(Pos2::new(rect.left(), arriba), rect.right_bottom());
                let pedida = propia.map(|a| if let Accion::Png(_) = a { Accion::Png(area_png) } else { a });
                for a in lanzada.into_iter().chain(pedida) {
                    c.exportar(hijo.ctx(), a, f);
                }
            }
            ui.horizontal(|ui| pie(ui, v));
        },
    );
}

/// Matriz de confusión como mapa de calor, con su leyenda y las variables creadas al pie.
fn dibujar_confusion(ui: &mut egui::Ui, inf: &Informe, v: &Vista, i: usize) {
    let pal = tema::p();
    let Seccion::Tabla(t, col, fil) = &inf.secciones[i] else { return };
    let (titulo, nota) = t.split_once(" (").map_or((t.as_str(), ""), |(a, b)| (a, b.trim_end_matches(')')));
    ui::tarjeta_con_titulo(
        ui,
        titulo,
        |ui| acciones_tabla(ui, t, col, fil),
        |ui| {
            if !nota.is_empty() {
                ui.label(egui::RichText::new(nota).size(12.0).color(pal.texto_2));
            }
            let alto = (ui.available_height() - 44.0).max(120.0);
            match mapa_confusion(col, fil) {
                Some(m) => m.mostrar(ui, alto),
                None => tabla(ui, col, fil),
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui::leyenda(ui, pal.series[1], "aciertos (diagonal)", ui::Muestra::Cuadrado);
                ui::leyenda(ui, pal.series[0], "confusiones", ui::Muestra::Cuadrado);
                pie(ui, v);
            });
        },
    );
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

#[cfg(test)]
mod pruebas {
    use super::*;

    fn v(nombre: &str, tipo: &str, forma: &str, columnas: &[&str]) -> Variable {
        Variable { nombre: nombre.into(), tipo: tipo.into(), forma: forma.into(), resumen: String::new(), columnas: columnas.iter().map(|c| c.to_string()).collect() }
    }

    fn param(nombre: &str, visible: Option<(&str, &str)>) -> Param {
        Param { nombre: nombre.into(), etiqueta: nombre.into(), tipo: "texto".into(), defecto: "d".into(), opciones: vec![], visible_si: visible.map(|(a, b)| (a.into(), b.into())) }
    }

    #[test]
    fn el_csv_entrecomilla_lo_que_contiene_separador_comillas_o_saltos() {
        let col = vec!["a".to_string(), "b;c".to_string()];
        let fil = vec![vec!["1,5".to_string(), "dice \"hola\"".to_string()], vec!["x\ny".to_string(), "2".to_string()]];
        assert_eq!(a_csv(&col, &fil, ';'), "a;\"b;c\"\n1,5;\"dice \"\"hola\"\"\"\n\"x\ny\";2\n");
        assert_eq!(a_csv(&col, &fil, ','), "a,b;c\n\"1,5\",\"dice \"\"hola\"\"\"\n\"x\ny\",2\n");
    }

    #[test]
    fn el_html_contiene_tablas_graficas_y_escapa_el_texto() {
        let fig = Figura::desde_json(&json!({"clase": "2d", "titulo": "G", "xlabel": "x", "ylabel": "y", "series": [{"x": [0.0, 1.0], "y": [1.0, 2.0], "nombre": "s", "tipo": "linea"}], "lineas_h": [], "log_x": false, "log_y": false}));
        let inf = Informe {
            titulo: "A & B".into(),
            secciones: vec![
                Seccion::Aviso("alerta".into(), "n < 3".into()),
                Seccion::Kv("Datos".into(), vec![("R²".into(), "0.9".into(), "".into())]),
                Seccion::Tabla("T".into(), vec!["c".into()], vec![vec!["<b>".into()]]),
                Seccion::Grafica(fig),
            ],
        };
        let h = informe_a_html(&inf);
        assert!(h.contains("<h1>A &amp; B</h1>") && h.contains("class=\"aviso alerta\">n &lt; 3"));
        assert!(h.contains("<th>c</th>") && h.contains("<td>&lt;b&gt;</td>") && h.contains("<td>R²</td><td>0.9</td>"));
        assert!(h.contains("<svg") && h.contains("</svg>"));
    }

    #[test]
    fn las_matrices_numericas_se_ofrecen_como_tablas_con_columnas_numeradas() {
        assert_eq!(columnas_de(&v("t", "tabla", "3 × 2", &["a", "b"])), Some(vec!["a".into(), "b".into()]));
        assert_eq!(columnas_de(&v("m", "matriz float64", "10 × 3", &[])), Some(vec!["0".into(), "1".into(), "2".into()]));
        assert_eq!(columnas_de(&v("m", "matriz float64", "10", &[])), Some(vec!["0".into()]));
        assert_eq!(columnas_de(&v("z", "matriz complex128", "10 × 3", &[])), None);
        assert_eq!(columnas_de(&v("n", "int", "1", &[])), None);
    }

    #[test]
    fn un_campo_solo_es_visible_si_el_otro_vale_uno_de_los_valores() {
        let h = Herramienta { id: "h".into(), modulo: "m".into(), titulo: "H".into(), grupo: String::new(), doc: String::new(), params: vec![param("modelo", None), param("alfa", Some(("modelo", "ridge|lineal")))] };
        let mut vals = HashMap::new();
        assert!(!es_visible(&h.params[1], &vals, &h));
        vals.insert("modelo".to_string(), "lineal".to_string());
        assert!(es_visible(&h.params[1], &vals, &h));
        assert!(es_visible(&h.params[0], &vals, &h));
    }

    #[test]
    fn el_nombre_de_la_tabla_importada_es_un_identificador() {
        assert_eq!(nombre_de_variable(std::path::Path::new("C:/datos/mi archivo-1.csv")), "mi_archivo_1");
        assert_eq!(nombre_de_variable(std::path::Path::new("2026.csv")), "t_2026");
    }

    fn figura_2d(titulo: &str) -> Figura {
        Figura::desde_json(&json!({"clase": "2d", "titulo": titulo, "xlabel": "x", "ylabel": "y", "series": [{"x": [1.0, 2.0], "y": [1.0, 2.0], "nombre": "s", "tipo": "linea"}], "lineas_h": [], "log_x": true, "log_y": false}))
    }

    fn kv(titulo: &str, items: &[(&str, &str, &str)]) -> Seccion {
        Seccion::Kv(titulo.into(), items.iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect())
    }

    fn valores(pares: &[(&str, &str)]) -> HashMap<String, String> {
        pares.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn informe_lcl(fr: &str) -> Informe {
        Informe {
            titulo: "Filtro LCL".into(),
            secciones: vec![
                kv("Diseño", &[("Frecuencia de resonancia", fr, "Hz"), ("Razón f_res/f_sw", "0.4", ""), ("Razón f_res/f_red", "41", ""), ("Admitancia a f_sw", "0.01", "A/V"), ("Amortiguamiento con Rd (Q ≈)", "3", "")]),
                Seccion::Grafica(figura_2d("Respuesta en frecuencia i2/v1")),
                Seccion::Aviso("bueno".into(), "Variables creadas en el espacio de trabajo: «lcl» (tabla 500 × 3).".into()),
            ],
        }
    }

    #[test]
    fn la_comprobacion_del_lcl_usa_los_valores_con_que_se_calculo() {
        let v = Vista::de("pot_lcl", &informe_lcl("2095.4"), &valores(&[("f0", "50.0"), ("fsw", "5000.0")]), 0);
        assert_eq!(v.fichas.len(), 6);
        assert_eq!(v.fichas[5], Ficha::nueva("Comprobación", "Correcto", "", "500 < 2095.4 < 2500 Hz"));
        assert_eq!(v.deco.marcas.iter().map(|m| m.x).collect::<Vec<_>>(), vec![50.0, 2095.4, 5000.0]);
        assert_eq!((v.deco.franjas[0].x0, v.deco.franjas[0].x1), (500.0, 2500.0));
        assert!(v.pie.starts_with("Variables creadas") && v.avisos.is_empty());
        let fuera = Vista::de("pot_lcl", &informe_lcl("3000"), &valores(&[("f0", "50"), ("fsw", "5000")]), 0);
        assert_eq!(fuera.fichas[5].valor, "Incumple");
    }

    #[test]
    fn sin_las_secciones_esperadas_la_herramienta_usa_la_plantilla_generica() {
        let mut inf = informe_lcl("2095.4");
        inf.secciones[1] = Seccion::Texto("sin gráfica".into());
        let v = Vista::de("pot_lcl", &inf, &valores(&[("f0", "50"), ("fsw", "5000")]), 0);
        assert_eq!(v.fichas.len(), 5);
        assert!(v.deco.marcas.is_empty() && v.principal == Principal::Ninguna);
        let sin_valores = Vista::de("pot_lcl", &informe_lcl("2095.4"), &HashMap::new(), 0);
        assert_eq!(sin_valores.fichas.len(), 5);
        assert_eq!(sin_valores.principal, Principal::Figuras(vec![1]));
    }

    #[test]
    fn mas_de_seis_valores_dejan_las_secciones_completas_como_tablas() {
        let items: Vec<(String, String, String)> = (0..8).map(|i| (format!("m{i}"), i.to_string(), String::new())).collect();
        let inf = Informe { titulo: String::new(), secciones: vec![Seccion::Kv("Todo".into(), items), Seccion::Tabla("T".into(), vec!["a".into()], vec![vec!["1".into()]])] };
        let v = Vista::de("otra", &inf, &HashMap::new(), 0);
        assert_eq!(v.fichas.len(), 6);
        assert_eq!(v.secundarias, vec![Secundaria::Kv(0)]);
        assert_eq!(v.principal, Principal::Tabla(1));
    }

    #[test]
    fn la_clasificacion_muestra_la_confusion_como_mapa_y_fichas_de_validacion_y_entrenamiento() {
        let inf = Informe {
            titulo: "Clasificación".into(),
            secciones: vec![
                kv("Validación (fuera de muestra)", &[("Modelo", "bosque", ""), ("Tipo de validación", "aleatoria", ""), ("Semilla", "0", ""), ("n usado / n total", "500 / 500", ""), ("Filas validadas", "500", ""), ("Exactitud", "0.468", ""), ("F1 macro", "0.46", "")]),
                kv("Entrenamiento (dentro de muestra)", &[("Exactitud", "1", ""), ("F1 macro", "1", "")]),
                Seccion::Aviso("alerta".into(), "posible sobreajuste".into()),
                Seccion::Tabla("Matriz de confusión (filas = real)".into(), vec!["Clase".into(), "A".into(), "B".into()], vec![vec!["A".into(), "5".into(), "1".into()], vec!["B".into(), "2".into(), "7".into()]]),
            ],
        };
        let v = Vista::de("ml_clasificacion", &inf, &HashMap::new(), 0);
        assert_eq!(v.principal, Principal::Confusion(3));
        assert_eq!(v.fichas.iter().map(|f| f.etiqueta.as_str()).collect::<Vec<_>>(), ["Exactitud · validación", "F1 macro · validación", "Exactitud · entrenamiento", "F1 macro · entrenamiento", "Modelo", "n usado / total"]);
        assert_eq!(v.fichas[0].valor, "0.468");
        assert_eq!(v.avisos, vec![("alerta".to_string(), "posible sobreajuste".to_string())]);
        assert!(!v.secundarias.contains(&Secundaria::Tabla(3)));
        let Seccion::Tabla(_, col, fil) = &inf.secciones[3] else { panic!() };
        let m = mapa_confusion(col, fil).unwrap();
        assert_eq!((m.filas, m.columnas, m.z), (vec!["A".to_string(), "B".to_string()], vec!["A".to_string(), "B".to_string()], vec![vec![5.0, 1.0], vec![2.0, 7.0]]));
        assert!(mapa_confusion(&["Clase".to_string(), "A".to_string()], &[vec!["A".to_string(), "x".to_string()]]).is_none());
    }

    #[test]
    fn la_etiqueta_de_una_variable_de_salida_se_acorta_y_se_capitaliza() {
        assert_eq!(etiqueta_salida("Variable de los valores de diseño (SI)"), "Valores de diseño (SI)");
        assert_eq!(etiqueta_salida("Variable de la respuesta en frecuencia"), "Respuesta en frecuencia");
        assert_eq!(etiqueta_salida("Curva guardada"), "Curva guardada");
    }

    #[test]
    fn las_etiquetas_con_unidad_se_separan_y_las_cifras_se_recortan() {
        assert_eq!(separar_unidad("L lado inversor [mH]"), ("L lado inversor", "mH"));
        assert_eq!(separar_unidad("Variable de la curva"), ("Variable de la curva", ""));
        assert_eq!(cifras(2095.4), "2095.4");
        assert_eq!(cifras(500.0), "500");
        assert_eq!(cifras(0.013084), "0.013084");
    }
}
