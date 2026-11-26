//! Módulo 1: consola interactiva, editor de scripts, variables y figuras.
use crate::app::{Compartido, Variable};
use crate::grafica::{self, Figura};
use crate::motor::Motor;
use crate::tema;
use crate::ui as interfaz;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, FontId, Key, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

enum Linea {
    Orden(String),
    Salida(String),
    Error(String),
    Nota(String),
}

pub struct Guion {
    pub nombre: String,
    pub ruta: Option<PathBuf>,
    pub texto: String,
    pub sucio: bool,
    pub motor: bool,
}

/// Tabla que se muestra de una variable: nombre, columnas, filas y número total de filas.
type Tabla = (String, Vec<String>, Vec<Vec<String>>, usize);

pub struct Calculo {
    historia: Vec<Linea>,
    entrada: String,
    ordenes: Vec<String>,
    pos_orden: Option<usize>,
    pub guiones: Vec<Guion>,
    pub activo: usize,
    figuras: Vec<Figura>,
    fig_activa: usize,
    id_eval: Option<u64>,
    id_ver: Option<(u64, String)>,
    id_leer: Option<(u64, String, usize)>,
    id_guardar: Option<u64>,
    id_escribir: Option<(u64, PathBuf)>,
    escritura_fallida: bool,
    tabla: Option<Tabla>,
    enfocar: bool,
    bajar: bool,
    pub aviso: String,
}

const GUION_INICIAL: &str = "# Ejemplo: álgebra lineal, FFT, interpolación, optimización y gráficas.
# Pulsa «Ejecutar» o usa la consola. Están disponibles np, pd, linalg, optimize, interpolate,
# signal, stats, plot(), scatter(), bar(), hist(), surf(), plot3(), integrar(), cargar() y guardar().

A = np.array([[4.0, 1.0], [2.0, 3.0]])
b = np.array([1.0, 2.0])
x = linalg.solve(A, b)
autovalores = linalg.eigvals(A)
print('Solución de A·x = b:', x)
print('Autovalores:', autovalores)

# FFT de una señal con armónicos
fs = 2000.0
t = np.arange(0, 0.2, 1 / fs)
senal = np.sin(2 * np.pi * 50 * t) + 0.2 * np.sin(2 * np.pi * 250 * t)
espectro = np.abs(np.fft.rfft(senal)) * 2 / t.size
frec = np.fft.rfftfreq(t.size, 1 / fs)
plot(frec, espectro, 'Espectro', titulo='FFT de una señal con 5.º armónico', xlabel='Hz', ylabel='Amplitud')

# Interpolación y optimización
xs = np.linspace(0, 10, 11)
spline = interpolate.CubicSpline(xs, np.sin(xs))
fino = np.linspace(0, 10, 300)
plot(fino, spline(fino), 'Spline', titulo='Interpolación cúbica', xlabel='x', ylabel='y')
scatter(xs, np.sin(xs), 'Datos', nueva=False)
mejor = optimize.minimize_scalar(lambda v: (v - 2) ** 2 + 1)
print('Mínimo en', mejor.x)

# Superficie 3D
X, Y = np.meshgrid(np.linspace(-3, 3, 50), np.linspace(-3, 3, 50))
surf(X, Y, np.sin(X) * np.cos(Y), 'Superficie')
";

impl Calculo {
    pub fn nuevo() -> Calculo {
        Calculo {
            historia: vec![Linea::Nota("Consola de Python con NumPy y SciPy. Escribe una expresión y pulsa Intro.".into())],
            entrada: String::new(),
            ordenes: vec![],
            pos_orden: None,
            guiones: vec![Guion { nombre: "ejemplo.py".into(), ruta: None, texto: GUION_INICIAL.into(), sucio: false, motor: false }],
            activo: 0,
            figuras: vec![],
            fig_activa: 0,
            id_eval: None,
            id_ver: None,
            id_leer: None,
            id_guardar: None,
            id_escribir: None,
            escritura_fallida: false,
            tabla: None,
            enfocar: false,
            bajar: false,
            aviso: String::new(),
        }
    }

    /// Sustituye los guiones abiertos por los de un proyecto; sin ninguno, vuelve al guion de ejemplo.
    pub fn fijar_guiones(&mut self, guiones: Vec<Guion>) {
        self.guiones = if guiones.is_empty() { Calculo::nuevo().guiones } else { guiones };
        self.activo = 0;
        self.aviso.clear();
    }

    pub fn abrir_codigo(&mut self, motor: &mut Motor, ruta: String, linea: usize) {
        if let Some(i) = self.guiones.iter().position(|g| g.ruta.as_ref().is_some_and(|r| r.display().to_string() == ruta)) {
            self.activo = i;
            return;
        }
        self.id_leer = Some((motor.llamar("leer_archivo", json!({"ruta": ruta})), ruta, linea));
    }

    pub fn recibir(&mut self, id: u64, ok: bool, v: &Value, err: &str, c: &mut Compartido) -> bool {
        if Some(id) == self.id_eval {
            self.id_eval = None;
            if ok {
                let s = v["salida"].as_str().unwrap_or("");
                if !s.is_empty() {
                    self.historia.push(Linea::Salida(s.trim_end().to_string()));
                }
                if let Some(r) = v["resultado"].as_str() {
                    self.historia.push(Linea::Salida(r.to_string()));
                }
                if let Some(e) = v["error"].as_str() {
                    self.historia.push(Linea::Error(e.to_string()));
                }
                if let Some(f) = v["figuras"].as_array() {
                    if !f.is_empty() {
                        for g in f {
                            self.figuras.push(Figura::desde_json(g));
                        }
                        self.fig_activa = self.figuras.len() - 1;
                    }
                }
                c.fijar_variables(&v["variables"]);
            } else {
                self.historia.push(Linea::Error(err.to_string()));
            }
            self.bajar = true;
            return true;
        }
        if self.id_ver.as_ref().is_some_and(|a| a.0 == id) {
            let nombre = self.id_ver.take().unwrap().1;
            if ok {
                let col = v["columnas"].as_array().map(|c| c.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
                let fil = v["filas"].as_array().map(|f| f.iter().map(|r| r.as_array().map(|c| c.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default()).collect()).unwrap_or_default();
                self.tabla = Some((nombre, col, fil, v["total"].as_u64().unwrap_or(0) as usize));
            }
            return true;
        }
        if self.id_leer.as_ref().is_some_and(|a| a.0 == id) {
            let (_, ruta, linea) = self.id_leer.take().unwrap();
            if ok {
                let texto = v.as_str().unwrap_or("").to_string();
                let p = PathBuf::from(&ruta);
                let nombre = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.guiones.push(Guion { nombre, ruta: Some(p), texto, sucio: false, motor: true });
                self.activo = self.guiones.len() - 1;
                self.aviso = format!("Código de la librería abierto (la clase del bloque empieza en la línea {linea}). Al guardar, el motor recarga los bloques.");
            } else {
                self.aviso = err.to_string();
            }
            return true;
        }
        if self.id_escribir.as_ref().is_some_and(|a| a.0 == id) {
            let (_, ruta) = self.id_escribir.take().unwrap();
            if ok {
                if let Some(g) = self.guiones.iter_mut().find(|g| g.ruta.as_ref() == Some(&ruta)) {
                    g.sucio = false;
                }
            } else {
                self.escritura_fallida = true;
                self.aviso = format!("No se guardó: {err}");
            }
            return true;
        }
        if Some(id) == self.id_guardar {
            self.id_guardar = None;
            if std::mem::take(&mut self.escritura_fallida) {
                return true;
            }
            self.aviso = if ok {
                c.catalogo_nuevo = Some(v.clone());
                "Guardado y recargado.".to_string()
            } else {
                format!("Guardado, pero la recarga falló y siguen activos los bloques anteriores: {err}")
            };
            return true;
        }
        false
    }

    fn enviar(&mut self, motor: &mut Motor) {
        let t = self.entrada.trim().to_string();
        if t.is_empty() || self.id_eval.is_some() {
            return;
        }
        self.historia.push(Linea::Orden(t.clone()));
        self.ordenes.push(t.clone());
        self.pos_orden = None;
        self.entrada.clear();
        self.id_eval = Some(motor.llamar("evaluar", json!({"codigo": t})));
        self.bajar = true;
    }

    pub fn correr_guion(&mut self, motor: &mut Motor) {
        if let Some(g) = self.guiones.get(self.activo) {
            self.historia.push(Linea::Nota(format!("» ejecutando {}", g.nombre)));
            self.id_eval = Some(motor.llamar("ejecutar_script", json!({"codigo": g.texto, "ruta": g.ruta.as_ref().map(|r| r.display().to_string())})));
            self.bajar = true;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, motor: &mut Motor, c: &mut Compartido) {
        let pal = tema::p();
        egui::TopBottomPanel::top("pestanas_calculo")
            .exact_height(41.0)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(20, 0)).stroke(Stroke::new(1.0_f32, pal.linea)))
            .show_inside(ui, |ui| self.barra(ui, motor));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.fondo)).show_inside(ui, |ui| {
            let alturas = interfaz::altos(ui.available_height() - 1.0, 2.0 * interfaz::MARGEN - interfaz::HUECO, &[1.5, 1.0]);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                interfaz::pagina(ui, |ui| {
                    let alturas = interfaz::filas_ajustables(ui, "calculo_alturas", [alturas[0], alturas[1]], [200.0, 180.0]);
                    interfaz::fila_ajustable(ui, "calculo_superior", alturas[0], |i, ui| {
                        ui.scope_builder(egui::UiBuilder::new().id_salt(("calculo_fila_1", i)), |ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            if i == 0 {
                                let acciones = |ui: &mut egui::Ui| {
                                    ui.add(egui::Label::new(egui::RichText::new("Python · np, pd, linalg, optimize, interpolate, signal, stats").size(12.0).color(pal.texto_2)).truncate());
                                };
                                interfaz::tarjeta_con_titulo(ui, "Script", acciones, |ui| self.editor(ui));
                            } else {
                                let (mut borrar, mut importar) = (false, false);
                                let acciones = |ui: &mut egui::Ui| {
                                    borrar = ui.small_button("Borrar todo").clicked();
                                    importar = ui.small_button("Importar datos").on_hover_text("CSV, TSV, TXT o Parquet").clicked();
                                };
                                interfaz::tarjeta_con_titulo(ui, "Espacio de trabajo", acciones, |ui| self.variables(ui, motor, c));
                                self.atender_acciones_variables(motor, c, borrar, importar);
                            }
                        });
                    });
                    interfaz::fila_ajustable(ui, "calculo_inferior", alturas[1], |i, ui| {
                        ui.scope_builder(egui::UiBuilder::new().id_salt(("calculo_fila_2", i)), |ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            if i == 0 {
                                let ejecutando = self.id_eval.is_some();
                                let (mut limpiar, mut detener) = (false, false);
                                let acciones = |ui: &mut egui::Ui| {
                                    limpiar = ui.small_button("Limpiar").clicked();
                                    if ejecutando {
                                        detener = ui.small_button("Detener").clicked();
                                        ui.spinner();
                                    }
                                };
                                interfaz::tarjeta_con_titulo(ui, "Consola", acciones, |ui| self.consola(ui, motor));
                                if limpiar {
                                    self.historia.clear();
                                }
                                if detener {
                                    motor.llamar("cancelar", json!({}));
                                }
                            } else {
                                interfaz::tarjeta_plena(ui, |ui| self.panel_figuras(ui, c));
                            }
                        });
                    });
                });
            });
        });
        self.ventana_tabla(ui.ctx());
    }

    /// Pestañas de los scripts abiertos y sus acciones: abrir, guardar y ejecutar.
    fn barra(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        let n = self.guiones.len();
        let mut nombres: Vec<String> = self.guiones.iter().map(|g| format!("{}{}", g.nombre, if g.sucio { " •" } else { "" })).collect();
        nombres.push("+".into());
        let refs: Vec<&str> = nombres.iter().map(String::as_str).collect();
        let mut activa = self.activo.min(n.saturating_sub(1));
        let (mut cerrar, mut abrir, mut guardar, mut ejecutar) = (false, false, false, false);
        interfaz::barra_pestanas(ui, &refs, &mut activa, |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ejecutar = tema::boton_principal(ui, "▶ Ejecutar").clicked();
            guardar = ui.button("Guardar").clicked();
            abrir = ui.button("Abrir").clicked();
            cerrar = n > 1 && ui.button("Cerrar script").clicked();
        });
        if activa == n {
            let nombre = nombre_libre(&self.guiones.iter().map(|g| g.nombre.as_str()).collect::<Vec<_>>());
            self.guiones.push(Guion { nombre, ruta: None, texto: String::new(), sucio: false, motor: false });
            self.activo = n;
        } else {
            self.activo = activa;
        }
        if cerrar {
            self.guiones.remove(self.activo);
            self.activo = self.activo.min(self.guiones.len() - 1);
        }
        if abrir {
            if let Some(r) = rfd::FileDialog::new().add_filter("Python", &["py"]).pick_file() {
                if let Ok(t) = std::fs::read_to_string(&r) {
                    let nombre = r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.guiones.push(Guion { nombre, ruta: Some(r), texto: t, sucio: false, motor: false });
                    self.activo = self.guiones.len() - 1;
                }
            }
        }
        if guardar {
            self.guardar_guion(motor);
        }
        if ejecutar {
            self.correr_guion(motor);
        }
    }

    /// Cuerpo de la tarjeta del script: aviso del último guardado y editor con números de línea.
    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 4.0;
        if !self.aviso.is_empty() {
            ui.label(egui::RichText::new(&self.aviso).small().color(tema::p().texto));
        }
        let activo = self.activo.min(self.guiones.len().saturating_sub(1));
        egui::ScrollArea::both().id_salt("scroll_guion").auto_shrink([false, false]).show(ui, |ui| {
            let Some(g) = self.guiones.get_mut(activo) else { return };
            let lineas = g.texto.split('\n').count().max(1);
            let numeros: String = (1..=lineas).map(|i| format!("{i:>4}")).collect::<Vec<_>>().join("\n");
            let mut lay = |ui: &egui::Ui, b: &dyn egui::TextBuffer, _ancho: f32| {
                let j = resaltar(b.as_str());
                ui.fonts_mut(|f| f.layout_job(j))
            };
            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new(numeros).font(FontId::monospace(13.5)).color(tema::p().texto_3));
                let r = ui.add(egui::TextEdit::multiline(&mut g.texto).code_editor().frame(false).desired_width(f32::INFINITY).desired_rows(12).layouter(&mut lay));
                if r.changed() {
                    g.sucio = true;
                }
            });
        });
    }

    fn guardar_guion(&mut self, motor: &mut Motor) {
        let Some(g) = self.guiones.get_mut(self.activo) else { return };
        if g.ruta.is_none() {
            g.ruta = rfd::FileDialog::new().add_filter("Python", &["py"]).set_file_name(&g.nombre).save_file();
            if let Some(r) = &g.ruta {
                g.nombre = r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            }
        }
        if let Some(r) = &g.ruta {
            if g.motor {
                // El guion no se da por guardado hasta que el motor confirme la escritura
                self.id_escribir = Some((motor.llamar("escribir_archivo", json!({"ruta": r.display().to_string(), "texto": g.texto})), r.clone()));
                self.id_guardar = Some(motor.llamar("recargar_librerias", json!({})));
            } else {
                match std::fs::write(r, &g.texto) {
                    Ok(()) => {
                        g.sucio = false;
                        self.aviso = format!("Guardado en {}", r.display());
                    }
                    Err(e) => self.aviso = format!("No se guardó {}: {e}", r.display()),
                }
            }
        }
    }

    fn consola(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        let pal = tema::p();
        ui.spacing_mut().item_spacing.y = 8.0;
        let alto = (ui.available_height() - 48.0).max(40.0);
        egui::ScrollArea::vertical().id_salt("scroll_consola").max_height(alto).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            for l in &self.historia {
                match l {
                    Linea::Orden(t) => {
                        ui.label(egui::RichText::new(format!("» {t}")).monospace().color(pal.texto_2));
                    }
                    Linea::Salida(t) => {
                        ui.label(egui::RichText::new(t).monospace());
                    }
                    Linea::Error(t) => {
                        ui.label(egui::RichText::new(t).monospace().color(pal.falla));
                    }
                    Linea::Nota(t) => {
                        ui.label(egui::RichText::new(t).color(pal.texto_2));
                    }
                }
            }
            if self.bajar {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                self.bajar = false;
            }
        });
        egui::Frame::new().fill(pal.campo).stroke(Stroke::new(1.0_f32, pal.borde)).corner_radius(6).inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("»").monospace().color(pal.acento));
                let r = ui.add(
                    egui::TextEdit::multiline(&mut self.entrada)
                        .code_editor()
                        .frame(false)
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .hint_text("Escribe aquí: A = np.eye(3)")
                        .return_key(Some(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, Key::Enter))),
                );
                if self.enfocar {
                    r.request_focus();
                    self.enfocar = false;
                }
                if r.has_focus() {
                    let (enter, arriba, abajo) = ui.input(|i| (i.key_pressed(Key::Enter) && !i.modifiers.shift, i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown)));
                    if enter {
                        self.enviar(motor);
                        self.enfocar = true;
                    }
                    if arriba && !self.entrada.contains('\n') && !self.ordenes.is_empty() {
                        let p = self.pos_orden.map(|x| x.saturating_sub(1)).unwrap_or(self.ordenes.len() - 1);
                        self.pos_orden = Some(p);
                        self.entrada = self.ordenes[p].clone();
                    }
                    if abajo && !self.entrada.contains('\n') {
                        if let Some(p) = self.pos_orden {
                            if p + 1 < self.ordenes.len() {
                                self.pos_orden = Some(p + 1);
                                self.entrada = self.ordenes[p + 1].clone();
                            } else {
                                self.pos_orden = None;
                                self.entrada.clear();
                            }
                        }
                    }
                }
            });
        });
    }

    /// Borra el espacio de trabajo o importa un archivo de datos como variable.
    fn atender_acciones_variables(&mut self, motor: &mut Motor, c: &mut Compartido, borrar: bool, importar: bool) {
        if borrar {
            motor.llamar("limpiar_espacio", json!({}));
            c.variables.clear();
        }
        if importar {
            if let Some(r) = rfd::FileDialog::new().add_filter("Datos", &["csv", "tsv", "txt", "parquet"]).pick_file() {
                let nombre: String = r.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "datos".into()).chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
                c.id_vars = Some(motor.llamar("importar_datos", json!({"ruta": r.display().to_string(), "nombre": nombre})));
            }
        }
    }

    /// Tabla de variables: nombre (abre su contenido), tipo, tamaño, resumen y acciones de exportar y borrar.
    fn variables(&mut self, ui: &mut egui::Ui, motor: &mut Motor, c: &mut Compartido) {
        let pal = tema::p();
        let vars: Vec<Variable> = c.variables.clone();
        if vars.is_empty() {
            ui.label(egui::RichText::new("Aún no hay variables. Ejecuta el script de ejemplo o escribe en la consola.").color(pal.texto_2));
            return;
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        let ancho = ui.available_width();
        let x = [0.0, ancho * 0.26, ancho * 0.48, ancho * 0.62];
        let fin_texto = ancho - 56.0;
        let (cab, _) = ui.allocate_exact_size(Vec2::new(ancho, 26.0), Sense::hover());
        for (t, dx) in ["Nombre", "Tipo", "Tamaño", "Contenido"].into_iter().zip(x) {
            ui.painter().text(Pos2::new(cab.left() + dx, cab.center().y), Align2::LEFT_CENTER, interfaz::mayusculas(t), tema::fuente_semi(10.5), pal.texto_2);
        }
        tema::separador(ui);
        egui::ScrollArea::vertical().id_salt("scroll_vars").auto_shrink([false, false]).show(ui, |ui| {
            for (i, v) in vars.iter().enumerate() {
                let (fila, _) = ui.allocate_exact_size(Vec2::new(ancho, 32.0), Sense::hover());
                interfaz::fondo_fila(ui, fila, i);
                let celda = |dx: f32, fin: f32| Rect::from_min_max(Pos2::new(fila.left() + dx, fila.top()), Pos2::new(fila.left() + fin, fila.bottom()));
                let p = ui.painter();
                let texto = |r: Rect, t: &str, f: FontId, color: egui::Color32| {
                    p.with_clip_rect(r).text(Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, t, f, color);
                };
                let r_nombre = celda(x[0], x[1] - 6.0);
                texto(r_nombre, &v.nombre, tema::fuente_mono(13.0), pal.texto);
                texto(celda(x[1], x[2] - 6.0), &v.tipo, FontId::proportional(12.0), pal.texto_2);
                texto(celda(x[2], x[3] - 6.0), &v.forma, tema::fuente_mono(12.0), pal.texto);
                texto(celda(x[3], fin_texto), &v.resumen.replace('\n', " "), tema::fuente_mono(12.0), pal.texto_2);
                if ui.interact(r_nombre, ui.id().with(("var", &v.nombre)), Sense::click()).on_hover_text("Ver contenido").clicked() {
                    self.id_ver = Some((motor.llamar("ver_variable", json!({"nombre": v.nombre})), v.nombre.clone()));
                }
                let boton = |n: usize| Rect::from_min_size(Pos2::new(fila.right() - 52.0 + 26.0 * n as f32, fila.center().y - 11.0), Vec2::splat(22.0));
                if ui.put(boton(0), egui::Button::new("↓").small()).on_hover_text("Exportar a CSV o Parquet").clicked() {
                    if let Some(r) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).add_filter("Parquet", &["parquet"]).set_file_name(format!("{}.csv", v.nombre)).save_file() {
                        motor.llamar("guardar_variable", json!({"nombre": v.nombre, "ruta": r.display().to_string()}));
                    }
                }
                if ui.put(boton(1), egui::Button::new("×").small()).on_hover_text("Borrar la variable").clicked() {
                    c.id_vars = Some(motor.llamar("borrar_variable", json!({"nombre": v.nombre})));
                }
            }
        });
    }

    /// Figuras en pestañas con la exportación a la derecha de la barra.
    fn panel_figuras(&mut self, ui: &mut egui::Ui, c: &mut Compartido) {
        ui.spacing_mut().item_spacing.y = 0.0;
        let nombres: Vec<String> = self
            .figuras
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let t = f.titulo();
                if t.is_empty() {
                    format!("Figura {}", i + 1)
                } else {
                    t.chars().take(18).collect()
                }
            })
            .collect();
        let refs: Vec<&str> = nombres.iter().map(String::as_str).collect();
        self.fig_activa = self.fig_activa.min(self.figuras.len().saturating_sub(1));
        let es_2d = matches!(self.figuras.get(self.fig_activa), Some(Figura::D2(_)));
        let mut accion = None;
        egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 0)).show(ui, |ui| {
            egui::ScrollArea::horizontal().id_salt("pestanas_figuras").auto_shrink([false, true]).show(ui, |ui| {
                interfaz::barra_pestanas(ui, &refs, &mut self.fig_activa, |_| {});
            });
        });
        tema::separador(ui);
        let mut quitar = false;
        if !refs.is_empty() {
            egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 0 }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if es_2d {
                        ui.label(egui::RichText::new(self.figuras[self.fig_activa].titulo()).size(12.5).color(tema::p().texto_2));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        quitar = ui.small_button("×").on_hover_text("Cerrar la figura").clicked();
                        if es_2d {
                            if ui.small_button("SVG").on_hover_text("Guardar como SVG").clicked() {
                                accion = Some(grafica::Accion::Svg);
                            }
                            if ui.small_button("PNG").on_hover_text("Guardar como PNG").clicked() {
                                accion = Some(grafica::Accion::Png(Rect::NOTHING));
                            }
                        }
                    });
                });
            });
        }
        if quitar {
            self.figuras.remove(self.fig_activa);
            self.fig_activa = self.fig_activa.saturating_sub(1);
            return;
        }
        let i = self.fig_activa;
        egui::Frame::new().inner_margin(egui::Margin::same(12)).show(ui, |ui| {
            let alto = (ui.available_height() - 2.0).max(80.0);
            let Some(f) = self.figuras.get_mut(i) else {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("Las gráficas aparecerán aquí: prueba plot(x, y) o surf(X, Y, Z).").color(tema::p().texto_2));
                });
                return;
            };
            match f {
                Figura::D2(g) => {
                    let area = ui.available_rect_before_wrap();
                    let a = g.mostrar_con(ui, alto, false, true);
                    accion = accion.or(a);
                    if let Some(grafica::Accion::Png(r)) = &mut accion {
                        if *r == Rect::NOTHING {
                            *r = Rect::from_min_size(area.min, Vec2::new(area.width(), alto));
                        }
                    }
                }
                Figura::D3(_) => accion = accion.or_else(|| grafica::mostrar(ui, f, egui::Id::new(("fig", i)), alto - 28.0)),
            }
            if let Some(a) = accion.take() {
                c.exportar(ui.ctx(), a, f);
            }
        });
    }

    fn ventana_tabla(&mut self, ctx: &egui::Context) {
        let mut abierta = self.tabla.is_some();
        let mut titulo = String::new();
        if let Some((n, _, _, t)) = &self.tabla {
            titulo = format!("{n} ({t} filas)");
        }
        egui::Window::new(titulo).id(egui::Id::new("tabla_var")).open(&mut abierta).default_size([640.0, 420.0]).show(ctx, |ui| {
            if let Some((_, col, fil, _)) = &self.tabla {
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("g_tabla").striped(true).num_columns(col.len()).show(ui, |ui| {
                        for c in col {
                            ui.label(egui::RichText::new(c).strong().color(tema::p().texto));
                        }
                        ui.end_row();
                        for f in fil {
                            for x in f {
                                ui.label(egui::RichText::new(x).monospace());
                            }
                            ui.end_row();
                        }
                    });
                });
            }
        });
        if !abierta {
            self.tabla = None;
        }
    }
}

/// Primer nombre `scriptN.py` (N desde 1) que no usa ningún guion abierto.
fn nombre_libre(usados: &[&str]) -> String {
    (1..).map(|n| format!("script{n}.py")).find(|c| !usados.iter().any(|u| u.eq_ignore_ascii_case(c))).unwrap_or_default()
}

/// Nombres de archivo sin repetidos (ni por mayúsculas, que en Windows son el mismo archivo): el segundo `a.py`
/// pasa a `a_2.py`, y así.
pub fn nombres_unicos(nombres: &[String]) -> Vec<String> {
    let mut vistos: Vec<String> = vec![];
    let mut salida = vec![];
    for n in nombres {
        let (raiz, ext) = n.rsplit_once('.').map_or((n.as_str(), String::new()), |(r, e)| (r, format!(".{e}")));
        let mut candidato = n.clone();
        let mut k = 2;
        while vistos.contains(&candidato.to_lowercase()) {
            candidato = format!("{raiz}_{k}{ext}");
            k += 1;
        }
        vistos.push(candidato.to_lowercase());
        salida.push(candidato);
    }
    salida
}

/// Lee los `.py` de la carpeta `scripts` de un proyecto como guiones, ordenados por nombre. Los que no se
/// pueden leer se devuelven como errores; una carpeta ausente no da guiones ni errores.
pub fn guiones_de(carpeta: &Path) -> (Vec<Guion>, Vec<String>) {
    let (mut guiones, mut errores) = (vec![], vec![]);
    let Ok(lista) = std::fs::read_dir(carpeta) else { return (guiones, errores) };
    let mut rutas: Vec<PathBuf> = lista.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "py")).collect();
    rutas.sort();
    for r in rutas {
        match std::fs::read_to_string(&r) {
            Ok(texto) => guiones.push(Guion { nombre: r.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), ruta: Some(r), texto, sucio: false, motor: false }),
            Err(e) => errores.push(format!("{}: {e}", r.display())),
        }
    }
    (guiones, errores)
}

/// Resaltado sencillo de sintaxis de Python.
pub fn resaltar(texto: &str) -> LayoutJob {
    const CLAVES: [&str; 30] = ["def", "class", "return", "if", "else", "elif", "for", "while", "in", "import", "from", "as", "with", "try", "except", "finally", "raise", "lambda", "not", "and", "or", "is", "None", "True", "False", "pass", "break", "continue", "yield", "self"];
    let f = FontId::monospace(13.5);
    let mut j = LayoutJob::default();
    let normal = TextFormat { font_id: f.clone(), color: tema::p().texto, ..Default::default() };
    let clave = TextFormat { font_id: f.clone(), color: tema::p().series[2], ..Default::default() };
    let cadena = TextFormat { font_id: f.clone(), color: tema::p().series[2], ..Default::default() };
    let coment = TextFormat { font_id: f.clone(), color: tema::p().texto_3, ..Default::default() };
    let numero = TextFormat { font_id: f.clone(), color: tema::p().series[1], ..Default::default() };
    let func = TextFormat { font_id: f.clone(), color: tema::p().acento_2, ..Default::default() };
    let c: Vec<(usize, char)> = texto.char_indices().collect();
    let fin_de = |k: usize| if k < c.len() { c[k].0 } else { texto.len() };
    let mut i = 0;
    while i < c.len() {
        let ch = c[i].1;
        let ini = c[i].0;
        if ch == '#' {
            let mut k = i;
            while k < c.len() && c[k].1 != '\n' {
                k += 1;
            }
            j.append(&texto[ini..fin_de(k)], 0.0, coment.clone());
            i = k;
        } else if ch == '"' || ch == '\'' {
            let triple = i + 2 < c.len() && c[i + 1].1 == ch && c[i + 2].1 == ch;
            let mut k = i + if triple { 3 } else { 1 };
            while k < c.len() {
                if c[k].1 == '\\' {
                    k += 2;
                    continue;
                }
                if triple {
                    if k + 2 < c.len() && c[k].1 == ch && c[k + 1].1 == ch && c[k + 2].1 == ch {
                        k += 3;
                        break;
                    }
                } else if c[k].1 == ch || c[k].1 == '\n' {
                    k += 1;
                    break;
                }
                k += 1;
            }
            let k = k.min(c.len());
            j.append(&texto[ini..fin_de(k)], 0.0, cadena.clone());
            i = k;
        } else if ch.is_ascii_digit() {
            let mut k = i;
            while k < c.len() && (c[k].1.is_ascii_alphanumeric() || c[k].1 == '.') {
                k += 1;
            }
            j.append(&texto[ini..fin_de(k)], 0.0, numero.clone());
            i = k;
        } else if ch.is_alphabetic() || ch == '_' {
            let mut k = i;
            while k < c.len() && (c[k].1.is_alphanumeric() || c[k].1 == '_') {
                k += 1;
            }
            let pal = &texto[ini..fin_de(k)];
            let fmt = if CLAVES.contains(&pal) {
                clave.clone()
            } else if k < c.len() && c[k].1 == '(' {
                func.clone()
            } else {
                normal.clone()
            };
            j.append(pal, 0.0, fmt);
            i = k;
        } else {
            let mut k = i + 1;
            while k < c.len() && !(c[k].1.is_alphanumeric() || matches!(c[k].1, '_' | '#' | '"' | '\'')) {
                k += 1;
            }
            j.append(&texto[ini..fin_de(k)], 0.0, normal.clone());
            i = k;
        }
    }
    j
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn un_script_nuevo_no_repite_el_nombre_de_uno_abierto() {
        assert_eq!(nombre_libre(&["ejemplo.py"]), "script1.py");
        // Con script1 cerrado y script2 abierto, el siguiente no puede volver a llamarse script2
        assert_eq!(nombre_libre(&["ejemplo.py", "script2.py"]), "script1.py");
        assert_eq!(nombre_libre(&["script1.py", "Script2.PY"]), "script3.py");
    }

    #[test]
    fn al_guardar_dos_scripts_con_el_mismo_nombre_no_se_pisan() {
        let n: Vec<String> = ["a.py", "b.py", "A.py", "a.py"].iter().map(|s| s.to_string()).collect();
        assert_eq!(nombres_unicos(&n), ["a.py", "b.py", "A_2.py", "a_3.py"]);
    }

    #[test]
    fn los_scripts_de_un_proyecto_se_cargan_con_su_ruta_y_en_orden() {
        let dir = std::env::temp_dir().join(format!("heliora_scripts_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b.py"), "print(2)").unwrap();
        std::fs::write(dir.join("a.py"), "print(1)").unwrap();
        std::fs::write(dir.join("notas.txt"), "no").unwrap();
        let (g, e) = guiones_de(&dir);
        assert!(e.is_empty());
        assert_eq!(g.iter().map(|x| x.nombre.as_str()).collect::<Vec<_>>(), ["a.py", "b.py"]);
        assert_eq!(g[0].ruta.as_deref(), Some(dir.join("a.py").as_path()));
        assert_eq!(g[1].texto, "print(2)");
        let (g, e) = guiones_de(&dir.join("no_existe"));
        assert!(g.is_empty() && e.is_empty());
        let mut c = Calculo::nuevo();
        c.fijar_guiones(vec![]);
        assert_eq!(c.guiones.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
