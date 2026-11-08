//! La ventana principal: barra de módulos, proyecto y reparto de mensajes del motor.
use crate::bloques::{self, Editor};
use crate::calculo::Calculo;
use crate::grafica::{self, Figura};
use crate::herramientas::{self, Herramienta, Pagina};
use crate::motor::{self, Mensaje, Motor};
use crate::plan::Mantenimiento;
use crate::tema::{self, Modulo};
use egui::{Pos2, Rect, Sense, Vec2};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Default)]
pub struct Variable {
    pub nombre: String,
    pub tipo: String,
    pub forma: String,
    pub resumen: String,
    pub columnas: Vec<String>,
}

/// Estado que comparten los módulos.
#[derive(Default)]
pub struct Compartido {
    pub variables: Vec<Variable>,
    pub id_vars: Option<u64>,
    pub abrir_codigo: Option<(String, usize)>,
    pub png: Option<(Rect, PathBuf)>,
}

impl Compartido {
    pub fn fijar_variables(&mut self, v: &Value) {
        let Some(a) = v.as_array() else { return };
        self.variables = a
            .iter()
            .map(|f| Variable {
                nombre: f["nombre"].as_str().unwrap_or("").into(),
                tipo: f["tipo"].as_str().unwrap_or("").into(),
                forma: f["forma"].as_str().unwrap_or("").into(),
                resumen: f["resumen"].as_str().unwrap_or("").into(),
                columnas: f["columnas"].as_array().map(|c| c.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
            })
            .collect();
    }

    pub fn exportar(&mut self, ctx: &egui::Context, a: grafica::Accion, f: &Figura) {
        grafica::exportar(ctx, a, f, &mut self.png);
    }
}

/// Atributos de Git para la carpeta de un proyecto (los mismos que el `.gitattributes` de la app).
const GITATTRIBUTES_PROYECTO: &str = "# Proyecto de Heliora\n*.json text eol=lf diff\n*.py text eol=lf diff=python\n*.csv text eol=lf\n*.md text eol=lf\n*.parquet binary\n*.png binary\n*.pdf binary\n";

pub struct Opciones {
    pub captura: Option<PathBuf>,
    pub modulo: Option<String>,
    pub ejemplo: Option<String>,
    pub simular: bool,
    pub guion: bool,
    pub herramientas: Vec<String>,
    pub valores: Vec<(String, String)>,
    pub pestana: Option<String>,
    pub modo: Option<String>,
    pub linealizar: bool,
}

pub struct App {
    motor: Motor,
    modulo: Modulo,
    compartido: Compartido,
    editor: Editor,
    calculo: Calculo,
    potencia: Pagina,
    fotovoltaica: Pagina,
    estadistica: Pagina,
    aprendizaje: Pagina,
    mantenimiento: Mantenimiento,
    herramientas: Vec<Herramienta>,
    validacion: crate::validacion::Validacion,
    ref_buscar: String,
    ref_sel: Option<String>,
    id_catalogo: Option<u64>,
    id_herr: Option<u64>,
    id_ejemplos: Option<u64>,
    iniciado: bool,
    carpeta: Option<PathBuf>,
    aviso: String,
    instalando: Option<Receiver<String>>,
    estado_inst: String,
    // Captura de pantalla para la comprobación automática
    captura: Option<PathBuf>,
    cuadros_listo: u32,
    cuadros: u32,
    captura_pedida: bool,
    op: Opciones,
    cuadro_sim: bool,
    modo: tema::Modo,
}

impl App {
    pub fn nueva(cc: &eframe::CreationContext, op: Opciones) -> App {
        // El modo se guarda entre sesiones; --modo lo fija para las capturas
        let guardado = cc.storage.and_then(|s| s.get_string("modo"));
        let modo = tema::Modo::desde(op.modo.as_deref().or(guardado.as_deref()).unwrap_or("oscuro"));
        tema::aplicar(&cc.egui_ctx, modo);
        let modulo = match op.modulo.as_deref() {
            Some("calculo") => Modulo::Calculo,
            Some("bloques") => Modulo::Bloques,
            Some("potencia") => Modulo::Potencia,
            Some("solar") => Modulo::Fotovoltaica,
            Some("estadistica") => Modulo::Estadistica,
            Some("aprendizaje") => Modulo::Aprendizaje,
            Some("mantenimiento") => Modulo::Mantenimiento,
            Some("validacion") => Modulo::Validacion,
            Some("referencia") => Modulo::Referencia,
            _ => Modulo::Inicio,
        };
        let mut mantenimiento = Mantenimiento::nuevo();
        if let Some(p) = &op.pestana {
            mantenimiento.fijar_pestana(p);
        }
        App {
            motor: Motor::iniciar(),
            modulo,
            compartido: Compartido::default(),
            editor: Editor::nuevo(),
            calculo: Calculo::nuevo(),
            potencia: Pagina::nueva("potencia", "Electrónica de potencia", "Cálculos de apoyo para el diseño. Los modelos dinámicos están en la librería del editor de Bloques."),
            fotovoltaica: Pagina::nueva("fv", "Fotovoltaica", "Modelo de un diodo, sombreado y recurso solar con pvlib."),
            estadistica: Pagina::nueva("estadistica", "Estadística", "Descripción, distribuciones, pruebas, regresión, ANOVA, series temporales y control de procesos."),
            aprendizaje: Pagina::nueva("ml", "Aprendizaje automático", "Prepara datos, entrena con validación cruzada y detecta anomalías."),
            mantenimiento,
            herramientas: vec![],
            validacion: crate::validacion::Validacion::nueva(),
            ref_buscar: String::new(),
            ref_sel: Some("arreglo_fv".to_string()),
            id_catalogo: None,
            id_herr: None,
            id_ejemplos: None,
            iniciado: false,
            carpeta: None,
            aviso: String::new(),
            instalando: None,
            estado_inst: String::new(),
            captura: op.captura.clone(),
            cuadros_listo: 0,
            cuadros: 0,
            captura_pedida: false,
            op,
            cuadro_sim: false,
            modo,
        }
    }

    fn tras_inicio(&mut self) {
        self.iniciado = true;
        self.id_catalogo = Some(self.motor.llamar("catalogo", json!({})));
        self.id_herr = Some(self.motor.llamar("herramientas", json!({})));
        self.id_ejemplos = Some(self.motor.llamar("ejemplos", json!({})));
        self.validacion.pedir_catalogo(&mut self.motor);
        // Tras un reinicio del motor no se pisa lo que la persona tenía abierto
        if self.mantenimiento.estado.is_null() {
            self.mantenimiento.iniciar(&mut self.motor);
        }
        if self.editor.diagrama.sistema.bloques.is_empty() {
            let clave = self.op.ejemplo.clone().unwrap_or_else(|| "planta_promediada".to_string());
            self.editor.pedir_ejemplo(&mut self.motor, &clave);
        }
    }

    fn atender_motor(&mut self) {
        for m in self.motor.recibir() {
            match m {
                Mensaje::Listo => self.tras_inicio(),
                Mensaje::Avance { id, fraccion, texto } => {
                    if !self.editor.avance(id, fraccion) {
                        self.validacion.avance(id, fraccion, &texto);
                    }
                }
                Mensaje::Terminado(_) => {
                    self.iniciado = false;
                }
                Mensaje::Reiniciado(t) => {
                    self.iniciado = false;
                    self.compartido.variables.clear();
                    self.aviso = t;
                }
                Mensaje::Respuesta { id, ok, valor, error } => self.despachar(id, ok, &valor, &error),
            }
        }
    }

    fn despachar(&mut self, id: u64, ok: bool, v: &Value, err: &str) {
        if Some(id) == self.id_catalogo {
            self.id_catalogo = None;
            if ok {
                self.editor.fijar_catalogo(v);
            }
            return;
        }
        if Some(id) == self.id_herr {
            self.id_herr = None;
            if ok {
                self.herramientas = herramientas::lista_desde(v);
            }
            return;
        }
        if Some(id) == self.id_ejemplos {
            self.id_ejemplos = None;
            if let Some(a) = v.as_array() {
                self.editor.ejemplos = a.iter().map(|e| (e["clave"].as_str().unwrap_or("").to_string(), e["nombre"].as_str().unwrap_or("").to_string())).collect();
            }
            return;
        }
        if self.compartido.id_vars == Some(id) {
            self.compartido.id_vars = None;
            if ok {
                self.compartido.fijar_variables(v);
            } else {
                self.aviso = err.to_string();
            }
            return;
        }
        let c = &mut self.compartido;
        if self.editor.recibir(id, ok, v, err, c)
            || self.calculo.recibir(id, ok, v, err, c)
            || self.potencia.recibir(id, ok, v, err, c)
            || self.fotovoltaica.recibir(id, ok, v, err, c)
            || self.estadistica.recibir(id, ok, v, err, c)
            || self.aprendizaje.recibir(id, ok, v, err, c)
            || self.mantenimiento.recibir(id, ok, v, err, c)
            || self.validacion.recibir(id, ok, v, err)
        {
            return;
        }
        if !ok && !err.is_empty() {
            self.aviso = err.to_string();
        }
    }

    fn atender_capturas(&mut self, ctx: &egui::Context) {
        let eventos = ctx.input(|i| i.events.clone());
        for e in eventos {
            if let egui::Event::Screenshot { image, user_data, .. } = e {
                let etiqueta = user_data.data.as_ref().and_then(|d| d.downcast_ref::<String>()).cloned().unwrap_or_default();
                if etiqueta == "figura" {
                    if let Some((rect, ruta)) = self.compartido.png.take() {
                        let ppp = ctx.pixels_per_point();
                        let recorte = image.region(&rect, Some(ppp));
                        guardar_png(&recorte, &ruta);
                    }
                } else if let Some(r) = &self.captura {
                    guardar_png(&image, r);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    fn barra_superior(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        egui::TopBottomPanel::top("superior")
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(12, 6)).stroke(egui::Stroke::new(1.0, pal.linea)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                    tema::logo(ui.painter(), r);
                    ui.label(egui::RichText::new("Heliora").size(17.0).family(tema::semi()).color(pal.texto));
                    ui.label(egui::RichText::new("cálculo técnico y plantas solares").small().color(pal.texto_2));
                    ui.separator();
                    let n = self.carpeta.as_ref().and_then(|c| c.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Proyecto sin guardar".into());
                    ui.label(egui::RichText::new(n).color(pal.texto));
                    if ui.small_button("Abrir proyecto").clicked() {
                        self.abrir_proyecto();
                    }
                    if ui.small_button("Guardar proyecto").clicked() {
                        self.guardar_proyecto();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Conmutador de modo: Instrumento (oscuro) y Laboratorio (claro)
                        for (m, t) in [(tema::Modo::Claro, "Laboratorio"), (tema::Modo::Oscuro, "Instrumento")] {
                            if ui.selectable_label(self.modo == m, t).on_hover_text(if m == tema::Modo::Claro { "Modo día, claro" } else { "Modo oscuro" }).clicked() && self.modo != m {
                                self.modo = m;
                                tema::aplicar(ui.ctx(), m);
                            }
                        }
                        ui.separator();
                        let (nivel, texto) = if self.motor.listo && self.motor.ocupado() {
                            (tema::Nivel::Ok, "Motor trabajando")
                        } else if self.motor.listo {
                            (tema::Nivel::Ok, "Motor Python listo")
                        } else if self.motor.caido.is_some() {
                            (tema::Nivel::Falla, "Motor sin conexión")
                        } else {
                            (tema::Nivel::Alerta, "Iniciando el motor...")
                        };
                        if self.motor.listo && ui.small_button("Reiniciar motor").on_hover_text("Borra las variables y vuelve a cargar el motor").clicked() {
                            self.motor.reiniciar();
                            self.iniciado = false;
                            self.compartido.variables.clear();
                        }
                        ui.label(egui::RichText::new(texto).small().color(pal.texto_2));
                        let (r, _) = ui.allocate_exact_size(Vec2::new(7.0, 14.0), Sense::hover());
                        tema::cuadrado(ui.painter(), r.center(), nivel.color());
                        if !self.aviso.is_empty() {
                            ui.add(egui::Label::new(egui::RichText::new(&self.aviso).small().color(pal.texto)).truncate());
                        }
                    });
                });
            });
    }

    fn abrir_proyecto(&mut self) {
        let Some(c) = rfd::FileDialog::new().set_title("Carpeta del proyecto").pick_folder() else { return };
        let d = c.join("diagramas").join("principal.json");
        if d.exists() {
            if let Err(e) = self.editor.abrir_de(&d) {
                self.aviso = e;
            }
        }
        let m = c.join("mantenimiento.json");
        if m.exists() {
            if let Err(e) = self.mantenimiento.cargar_de(&m, &mut self.motor) {
                self.aviso = e;
            }
        }
        let datos = c.join("datos");
        if datos.exists() {
            self.compartido.id_vars = Some(self.motor.llamar("cargar_espacio", json!({"carpeta": datos.display().to_string()})));
        }
        self.aviso = format!("Proyecto abierto: {}", c.display());
        self.carpeta = Some(c);
    }

    /// Un proyecto es una carpeta: diagramas en JSON y datos en CSV/Parquet.
    fn guardar_proyecto(&mut self) {
        if self.carpeta.is_none() {
            self.carpeta = rfd::FileDialog::new().set_title("Carpeta para el proyecto").pick_folder();
        }
        let Some(c) = self.carpeta.clone() else { return };
        let _ = std::fs::create_dir_all(c.join("diagramas"));
        let _ = std::fs::create_dir_all(c.join("scripts"));
        // Cada proyecto es una carpeta lista para Git: diagramas JSON como texto, datos binarios aparte
        if !c.join(".gitattributes").exists() {
            let _ = std::fs::write(c.join(".gitattributes"), GITATTRIBUTES_PROYECTO);
        }
        let mut problemas = vec![];
        if let Err(e) = self.editor.guardar_en(&c.join("diagramas").join("principal.json")) {
            problemas.push(e);
        }
        if !self.mantenimiento.estado.is_null() {
            if let Err(e) = self.mantenimiento.guardar_en(&c.join("mantenimiento.json")) {
                problemas.push(e);
            }
        }
        for g in &self.calculo.guiones {
            if !g.motor {
                let _ = std::fs::write(c.join("scripts").join(&g.nombre), &g.texto);
            }
        }
        self.motor.llamar("guardar_espacio", json!({"carpeta": c.join("datos").display().to_string()}));
        let doc = json!({"formato": "heliora-proyecto/1", "diagrama": "diagramas/principal.json", "mantenimiento": "mantenimiento.json", "datos": "datos", "scripts": "scripts"});
        let _ = std::fs::write(c.join("proyecto.json"), serde_json::to_string_pretty(&doc).unwrap_or_default());
        self.aviso = if problemas.is_empty() { format!("Proyecto guardado en {}", c.display()) } else { problemas.join("; ") };
    }

    fn barra_modulos(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        egui::SidePanel::left("rail")
            .exact_width(78.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(4, 8)).stroke(egui::Stroke::new(1.0, pal.linea)))
            .show(ctx, |ui| {
                for (m, nombre) in tema::MODULOS {
                    let activo = self.modulo == m;
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(70.0, 56.0), Sense::click());
                    if activo {
                        // Activo: fondo CAMPO, barra de 2 px en ACENTO e icono en ACENTO
                        ui.painter().rect_filled(r, tema::radio() as f32, pal.campo);
                        ui.painter().rect_filled(Rect::from_min_size(Pos2::new(r.left() - 4.0, r.top() + 10.0), Vec2::new(2.0, r.height() - 20.0)), 0.0, pal.acento);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, tema::radio() as f32, pal.realce);
                    }
                    let color = if activo { pal.acento } else { pal.texto_2 };
                    let ic = Rect::from_center_size(r.center() - Vec2::new(0.0, 8.0), Vec2::splat(22.0));
                    tema::icono(ui.painter(), ic, m, color);
                    ui.painter().text(r.center() + Vec2::new(0.0, 15.0), egui::Align2::CENTER_CENTER, nombre, egui::FontId::proportional(10.5), if activo { pal.texto } else { pal.texto_2 });
                    if resp.clicked() {
                        self.modulo = m;
                    }
                }
            });
    }

    fn pagina_inicio(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                tema::logo(ui.painter(), r);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Heliora").size(32.0).family(tema::semi()).color(pal.texto));
                    ui.label(egui::RichText::new("Modela tu planta solar, analiza su operación y planifica su mantenimiento.").size(15.0).color(pal.texto_2));
                });
            });
            ui.add_space(14.0);
            let tarjetas: [(Modulo, &str, &str); 8] = [
                (Modulo::Calculo, "Entorno de cálculo", "Consola, scripts en Python, variables y gráficas 2D y 3D con exportación a PNG y SVG."),
                (Modulo::Bloques, "Simulación por bloques", "Lienzo, subsistemas y cuatro solvers: Euler, RK4, Dormand-Prince y BDF. Incluye el ejemplo completo de la planta."),
                (Modulo::Potencia, "Electrónica de potencia", "Convertidores conmutados y promediados, PWM, SVPWM, control dq, PLL y filtros."),
                (Modulo::Fotovoltaica, "Fotovoltaica", "Modelo de un diodo desde la hoja de datos, sombreado, MPPT y recurso solar con pvlib."),
                (Modulo::Estadistica, "Estadística", "Distribuciones, Weibull, pruebas, regresión, ANOVA, ARIMA y cartas de control."),
                (Modulo::Aprendizaje, "Aprendizaje automático", "Preprocesamiento, regresión, clasificación, clustering, PCA y detección de anomalías."),
                (Modulo::Mantenimiento, "Mantenimiento", "SCADA, KPIs, detección de fallas, fiabilidad, planificador con calendario y AMEF."),
                (Modulo::Validacion, "Validación", "Pruebas por fases contra soluciones analíticas, con el error numérico de cada una."),
            ];
            // Rejilla de 4 × 2 tarjetas con 8 px de separación (2 columnas si la ventana es estrecha)
            let hueco = 8.0;
            let ancho = ui.available_width();
            let cols = if ancho >= 900.0 { 4 } else { 2 };
            let w = (ancho - hueco * (cols as f32 - 1.0)) / cols as f32;
            for (fila_i, fila) in tarjetas.chunks(cols).enumerate() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = hueco;
                    for (k, (m, t, d)) in fila.iter().enumerate() {
                        let n = fila_i * cols + k + 1;
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 126.0), Sense::click());
                        let hover = resp.hovered();
                        ui.painter().rect(r, tema::radio_ventana() as f32, if hover { pal.campo } else { pal.panel }, egui::Stroke::new(1.0, if hover { pal.linea_fuerte } else { pal.linea }), egui::StrokeKind::Inside);
                        let ic = Rect::from_min_size(r.min + Vec2::new(14.0, 14.0), Vec2::splat(22.0));
                        tema::icono(ui.painter(), ic, *m, pal.acento);
                        ui.painter().text(Pos2::new(r.right() - 14.0, r.top() + 25.0), egui::Align2::RIGHT_CENTER, format!("{n:02}"), tema::fuente_mono(11.0), pal.texto_3);
                        ui.painter().text(r.min + Vec2::new(14.0, 54.0), egui::Align2::LEFT_CENTER, *t, tema::fuente_semi(15.0), pal.texto);
                        let galley = ui.fonts_mut(|f| f.layout(d.to_string(), egui::FontId::proportional(12.5), pal.texto_2, w - 28.0));
                        ui.painter().galley(r.min + Vec2::new(14.0, 70.0), galley, pal.texto_2);
                        if resp.clicked() {
                            self.modulo = *m;
                        }
                    }
                });
                ui.add_space(hueco);
            }
            ui.add_space(6.0);
            ui.columns(2, |col| {
                tema::tarjeta(&mut col[0], |ui| {
                    ui.set_width(ui.available_width());
                    tema::titulo(ui, "Para empezar");
                    ui.add_space(2.0);
                    let filas: [(&str, u8); 4] = [
                        ("Abrir la planta completa: arreglo → boost → inversor → LCL → red", 0),
                        ("Probar el entorno de cálculo con un script de ejemplo", 1),
                        ("Extraer parámetros de un módulo comercial", 2),
                        ("Analizar datos SCADA de muestra", 3),
                    ];
                    for (t, k) in filas {
                        tema::separador(ui);
                        let r = ui.add(egui::Button::new(egui::RichText::new(format!("{t}  →")).color(pal.texto)).frame(false).min_size(Vec2::new(ui.available_width(), 26.0)));
                        if r.clicked() {
                            match k {
                                0 => {
                                    self.editor.pedir_ejemplo(&mut self.motor, "planta_promediada");
                                    self.modulo = Modulo::Bloques;
                                }
                                1 => self.modulo = Modulo::Calculo,
                                2 => {
                                    self.fotovoltaica.seleccionar("fv_hoja");
                                    self.modulo = Modulo::Fotovoltaica;
                                }
                                _ => {
                                    self.mantenimiento.analisis.seleccionar("mt_muestra");
                                    self.modulo = Modulo::Mantenimiento;
                                }
                            }
                        }
                    }
                });
                tema::tarjeta(&mut col[1], |ui| {
                    ui.set_width(ui.available_width());
                    tema::titulo(ui, "Fases de validación");
                    ui.add_space(2.0);
                    self.validacion.resumen(ui);
                    tema::separador(ui);
                    if ui.add(egui::Button::new(egui::RichText::new("Ver el detalle y el error de cada prueba  →").color(pal.texto)).frame(false)).clicked() {
                        self.modulo = Modulo::Validacion;
                    }
                });
            });
        });
    }

    fn pagina_referencia(&mut self, ui: &mut egui::Ui) {
        egui::SidePanel::left("ref_lista").default_width(290.0).frame(tema::marco_panel()).show_inside(ui, |ui| {
            tema::titulo(ui, "Bloques y sus ecuaciones");
            ui.add(egui::TextEdit::singleline(&mut self.ref_buscar).hint_text("Buscar...").desired_width(f32::INFINITY));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let f = self.ref_buscar.to_lowercase();
                for (cat, lista) in bloques::por_categoria(&self.editor.fichas) {
                    let filtrada: Vec<_> = lista.iter().filter(|x| f.is_empty() || x.nombre.to_lowercase().contains(&f) || x.doc.to_lowercase().contains(&f)).collect();
                    if filtrada.is_empty() {
                        continue;
                    }
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(cat).small().family(tema::semi()).color(tema::p().texto));
                    for x in filtrada {
                        if ui.selectable_label(self.ref_sel.as_deref() == Some(&x.tipo), &x.nombre).clicked() {
                            self.ref_sel = Some(x.tipo.clone());
                        }
                    }
                }
            });
        });
        egui::CentralPanel::default().frame(tema::marco_fondo()).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let Some(t) = self.ref_sel.clone() else {
                    ui.add_space(60.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("Elige un bloque para ver sus ecuaciones, sus parámetros y su código.").size(15.0).color(tema::p().texto_2));
                    });
                    return;
                };
                let Some(x) = self.editor.fichas.get(&t).cloned() else { return };
                ui.label(egui::RichText::new(&x.nombre).size(22.0).strong().color(tema::p().texto));
                ui.label(egui::RichText::new(&x.categoria).color(tema::p().texto_2));
                ui.add_space(6.0);
                tema::tarjeta(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(&x.doc).family(egui::FontFamily::Monospace).size(13.5));
                });
                ui.add_space(6.0);
                if !x.params.is_empty() {
                    tema::tarjeta(ui, |ui| {
                        ui.set_width(ui.available_width());
                        tema::titulo(ui, "Parámetros");
                        egui::Grid::new("g_ref").striped(true).num_columns(3).spacing([18.0, 4.0]).show(ui, |ui| {
                            for p in &x.params {
                                ui.label(&p.etiqueta);
                                ui.label(egui::RichText::new(match &p.defecto {
                                    Value::String(s) => s.clone(),
                                    o => o.to_string(),
                                }).monospace());
                                ui.label(egui::RichText::new(&p.unidad).color(tema::p().texto_2));
                                ui.end_row();
                            }
                        });
                    });
                    ui.add_space(6.0);
                }
                ui.horizontal(|ui| {
                    let puertos: Vec<String> = x.entradas.iter().map(|p| format!("entrada {p}")).chain(x.salidas.iter().map(|p| format!("salida {p}"))).chain(x.e.iter().map(|p| format!("eléctrico {p}"))).chain(x.e3.iter().map(|p| format!("trifásico {p}"))).collect();
                    ui.label(egui::RichText::new(format!("Puertos: {}", if puertos.is_empty() { "ninguno".to_string() } else { puertos.join(", ") })).color(tema::p().texto_2));
                });
                if !x.archivo.is_empty() && ui.button("Ver y editar el código de este bloque").clicked() {
                    self.compartido.abrir_codigo = Some((x.archivo.clone(), x.linea));
                }
            });
        });
    }

    fn panel_sin_motor(&mut self, ctx: &egui::Context) {
        let Some(c) = self.motor.caido.clone() else { return };
        egui::TopBottomPanel::top("sin_motor").frame(egui::Frame::new().fill(tema::p().falla_fondo).inner_margin(10)).show(ctx, |ui| {
            ui.colored_label(tema::p().falla, "El motor de cálculo en Python no está disponible.");
            ui.label(egui::RichText::new(c).small());
            ui.horizontal(|ui| {
                if self.instalando.is_none() {
                    if tema::boton_principal(ui, "Instalar el motor").on_hover_text("Crea un entorno de Python e instala NumPy, SciPy, pandas, scikit-learn, statsmodels y pvlib").clicked() {
                        let (tx, rx) = channel();
                        motor::instalar_motor(&self.motor.carpeta, tx);
                        self.instalando = Some(rx);
                        self.estado_inst = "Creando el entorno de Python...".into();
                    }
                    if ui.button("Reintentar").clicked() {
                        self.motor.reiniciar();
                        self.iniciado = false;
                    }
                } else {
                    ui.spinner();
                    ui.label(&self.estado_inst);
                }
            });
        });
        let mut termino = false;
        if let Some(rx) = &self.instalando {
            while let Ok(m) = rx.try_recv() {
                if m == "listo" {
                    termino = true;
                } else {
                    if m.starts_with("error") {
                        self.aviso = m.clone();
                        termino = true;
                    }
                    self.estado_inst = m;
                }
            }
        }
        if termino {
            self.instalando = None;
            self.motor.reiniciar();
            self.iniciado = false;
        }
    }

    fn pagina_ocupada(&self) -> bool {
        self.potencia.ocupada() || self.fotovoltaica.ocupada() || self.estadistica.ocupada() || self.aprendizaje.ocupada() || self.mantenimiento.analisis.ocupada()
    }

    fn logica_captura(&mut self, ctx: &egui::Context) {
        if self.captura.is_none() {
            return;
        }
        self.cuadros += 1;
        ctx.request_repaint();
        if self.op.simular && !self.cuadro_sim && !self.editor.fichas.is_empty() && !self.editor.diagrama.sistema.bloques.is_empty() {
            self.cuadro_sim = true;
            self.editor.ejecutar(&mut self.motor);
        }
        // Linealizar en cuanto el ejemplo está cargado (para la captura del flujo diagrama → Bode)
        if self.op.linealizar && !self.editor.fichas.is_empty() && !self.editor.diagrama.sistema.bloques.is_empty() && self.editor.id_sim.is_none() {
            self.op.linealizar = false;
            self.editor.linealizar(&mut self.motor);
        }
        if self.op.guion && !self.herramientas.is_empty() {
            self.op.guion = false;
            self.calculo.correr_guion(&mut self.motor);
        }
        // Herramientas en secuencia, para comprobar una página de análisis sin tocar la ventana
        let pagina_libre = !self.pagina_ocupada();
        if pagina_libre && !self.herramientas.is_empty() && !self.op.herramientas.is_empty() {
            let id = self.op.herramientas.remove(0);
            if let Some(h) = self.herramientas.iter().find(|h| h.id == id).cloned() {
                let m = &mut self.motor;
                let p = match h.modulo.as_str() {
                    "potencia" => &mut self.potencia,
                    "fv" => &mut self.fotovoltaica,
                    "estadistica" => &mut self.estadistica,
                    "ml" => &mut self.aprendizaje,
                    _ => &mut self.mantenimiento.analisis,
                };
                for (k, v) in &self.op.valores {
                    p.fijar(&id, k, v);
                }
                p.seleccionar(&id);
                p.ejecutar(&h, m);
            }
        }
        let ocupado = self.editor.id_sim.is_some() || self.editor.id_lin.is_some() || self.op.linealizar || self.pagina_ocupada() || !self.op.herramientas.is_empty();
        let listo = self.motor.caido.is_some() || (self.motor.listo && !self.editor.fichas.is_empty() && !self.herramientas.is_empty() && !self.mantenimiento.estado.is_null() && !ocupado);
        if listo {
            self.cuadros_listo += 1;
        }
        // 120 cuadros tras cargar; como mucho, unos 45 s en total
        if !self.captura_pedida && !ocupado && (self.cuadros_listo >= 120 || self.cuadros > 2700) {
            self.captura_pedida = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new("ventana".to_string())));
        }
    }
}

fn guardar_png(img: &egui::ColorImage, ruta: &std::path::Path) {
    let [w, h] = img.size;
    let mut datos = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        datos.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
    }
    if let Some(d) = ruta.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Some(b) = image::RgbaImage::from_raw(w as u32, h as u32, datos) {
        let _ = b.save(ruta);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.atender_motor();
        self.atender_capturas(ctx);
        self.logica_captura(ctx);
        // Hay trabajo en curso: seguir refrescando
        if !self.motor.listo || self.editor.id_sim.is_some() || self.validacion.ocupada() || self.instalando.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        if let Some((ruta, linea)) = self.compartido.abrir_codigo.take() {
            self.calculo.abrir_codigo(&mut self.motor, ruta, linea);
            self.modulo = Modulo::Calculo;
        }
        self.barra_superior(ctx);
        self.panel_sin_motor(ctx);
        self.barra_modulos(ctx);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(tema::p().fondo)).show(ctx, |ui| {
            let herr = self.herramientas.clone();
            match self.modulo {
                Modulo::Inicio => {
                    egui::Frame::new().inner_margin(14).show(ui, |ui| self.pagina_inicio(ui));
                }
                Modulo::Calculo => self.calculo.ui(ui, &mut self.motor, &mut self.compartido),
                Modulo::Bloques => self.editor.ui(ui, &mut self.motor, &mut self.compartido),
                Modulo::Potencia => self.potencia.ui(ui, &herr, &mut self.motor, &mut self.compartido),
                Modulo::Fotovoltaica => self.fotovoltaica.ui(ui, &herr, &mut self.motor, &mut self.compartido),
                Modulo::Estadistica => self.estadistica.ui(ui, &herr, &mut self.motor, &mut self.compartido),
                Modulo::Aprendizaje => self.aprendizaje.ui(ui, &herr, &mut self.motor, &mut self.compartido),
                Modulo::Mantenimiento => self.mantenimiento.ui(ui, &herr, &mut self.motor, &mut self.compartido),
                Modulo::Validacion => self.validacion.ui(ui, &mut self.motor),
                Modulo::Referencia => self.pagina_referencia(ui),
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("modo", self.modo.clave().to_string());
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.motor.detener();
    }
}
