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

    fn formulario(&mut self, ui: &mut egui::Ui, h: &Herramienta, motor: &mut Motor, c: &Compartido) {
        // Tablas disponibles para los selectores: tablas y matrices numéricas del espacio de trabajo
        let tablas: Vec<(String, Vec<String>)> = c.variables.iter().filter_map(|v| columnas_de(v).map(|cols| (v.nombre.clone(), cols))).collect();
        self.sincronizar_tabla(h, &tablas);
        egui::Frame::new().fill(tema::p().campo).corner_radius(8).inner_margin(10).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(&h.doc).small().color(tema::p().texto_2));
            ui.add_space(4.0);
            let vals = self.valores.entry(h.id.clone()).or_default();
            let tabla_actual = h.params.iter().find(|p| p.tipo == "tabla").map(|p| valor_de(vals, h, &p.nombre)).unwrap_or_default();
            let columnas: Vec<String> = tablas.iter().find(|t| t.0 == tabla_actual).map(|t| t.1.clone()).unwrap_or_default();
            egui::Grid::new(("form", &h.id)).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                for p in &h.params {
                    if !es_visible(p, vals, h) {
                        continue;
                    }
                    ui.label(egui::RichText::new(&p.etiqueta).small());
                    let mut t = valor_de(vals, h, &p.nombre);
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
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt((&h.id, &p.nombre)).selected_text(if t.is_empty() { "elegir…" } else { &t }).width(84.0).truncate().show_ui(ui, |ui| {
                                    for (n, _) in &tablas {
                                        ui.selectable_value(&mut t, n.clone(), n);
                                    }
                                });
                                if ui.small_button("Importar…").on_hover_text("Lee un CSV, TSV, TXT o Parquet del ordenador y lo deja en el espacio de trabajo").clicked() {
                                    if let Some(r) = rfd::FileDialog::new().add_filter("Datos", &["csv", "tsv", "txt", "parquet"]).pick_file() {
                                        let nombre = nombre_de_variable(&r);
                                        let id = motor.llamar("importar_tabla", json!({"ruta": r.display().to_string(), "nombre": nombre}));
                                        self.importando = Some((id, h.id.clone(), p.nombre.clone(), nombre));
                                    }
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
                            // Opciones: «guardar», «guardar_si:param=valor» (diálogo de guardar) y «ext:a,b» (filtro de extensiones)
                            let guardar = p.opciones.iter().any(|o| {
                                o == "guardar" || o.strip_prefix("guardar_si:").and_then(|c| c.split_once('=')).is_some_and(|(otro, valor)| valor_de(vals, h, otro) == valor)
                            });
                            let exts: Vec<String> = p.opciones.iter().find_map(|o| o.strip_prefix("ext:")).map(|e| e.split(',').map(String::from).collect()).unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.add(egui::TextEdit::singleline(&mut t).desired_width(112.0));
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
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    let r = ui.add(egui::TextEdit::singleline(&mut b.texto).hint_text("buscar en CEC").desired_width(88.0));
                                    let pulsado = ui.small_button("Buscar").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                    if pulsado && !b.texto.trim().is_empty() && b.id.is_none() {
                                        b.id = Some(motor.llamar("buscar_modulos_cec", json!({"texto": b.texto.trim()})));
                                    }
                                    if b.id.is_some() {
                                        ui.spinner();
                                    }
                                });
                                egui::ComboBox::from_id_salt((&h.id, &p.nombre)).selected_text(&t).width(150.0).truncate().show_ui(ui, |ui| {
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
}
