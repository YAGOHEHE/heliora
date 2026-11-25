//! La ventana principal: barra de módulos, proyecto y reparto de mensajes del motor.
use crate::bloques::{self, Editor};
use crate::calculo::Calculo;
use crate::comandos;
use crate::grafica::{self, Figura};
use crate::herramientas::{self, Herramienta, Pagina};
use crate::motor::{self, Mensaje, Motor};
use crate::plan::Mantenimiento;
use crate::tema::{self, Modulo};
use egui::{Pos2, Rect, Sense, Vec2};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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
    /// Columnas de los archivos de datos que consultan los bloques de tabla, por ruta
    pub columnas: HashMap<String, Vec<String>>,
    /// Archivo cuyas columnas necesita el inspector y aún no están en `columnas`
    pub pedir_columnas: Option<String>,
    /// Catálogo de bloques recién recargado, que el editor aún debe adoptar
    pub catalogo_nuevo: Option<Value>,
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

    /// Columnas que ofrece el desplegable de un bloque de tabla: las del archivo de su parámetro `ruta`
    /// (que se piden al motor la primera vez) o las de la variable de su parámetro `variable`.
    /// Vacío mientras no se conozcan: el inspector cae entonces a un campo de texto.
    pub fn columnas_de(&mut self, params: &serde_json::Map<String, Value>) -> Vec<String> {
        if let Some(Value::String(r)) = params.get("ruta") {
            if r.is_empty() {
                return vec![];
            }
            return match self.columnas.get(r) {
                Some(c) => c.clone(),
                None => {
                    self.pedir_columnas = Some(r.clone());
                    vec![]
                }
            };
        }
        let Some(Value::String(v)) = params.get("variable") else { return vec![] };
        self.variables.iter().find(|x| &x.nombre == v).map(|x| x.columnas.clone()).unwrap_or_default()
    }
}

/// Escribe un archivo entero o no lo toca: primero en un temporal al lado y después lo renombra
/// sobre el destino (en Windows, `rename` reemplaza el archivo existente).
pub fn escribir_atomico(ruta: &std::path::Path, contenido: &[u8]) -> Result<(), String> {
    let mut tmp = ruta.as_os_str().to_owned();
    tmp.push(".tmp~");
    let tmp = PathBuf::from(tmp);
    let r = std::fs::write(&tmp, contenido).and_then(|_| std::fs::rename(&tmp, ruta));
    if let Err(e) = r {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {e}", ruta.display()));
    }
    Ok(())
}

/// Aviso final del guardado: «guardado» solo si no hubo ningún problema.
fn resumen_guardado(carpeta: &std::path::Path, problemas: &[String]) -> String {
    if problemas.is_empty() {
        format!("Proyecto guardado en {}", carpeta.display())
    } else {
        format!("El proyecto NO se guardó completo ({} problemas): {}", problemas.len(), problemas.join("; "))
    }
}

/// Guardado en curso: las escrituras locales ya hechas esperan a que el motor guarde los datos.
struct Guardado {
    id: u64,
    carpeta: PathBuf,
    problemas: Vec<String>,
}

/// Variables que el motor no pudo guardar, como problemas legibles («nombre (motivo)»), de su respuesta a `guardar_espacio`.
fn omitidos_del_espacio(v: &Value) -> Vec<String> {
    let lista: Vec<String> = v["omitidos"].as_array().into_iter().flatten().map(|o| format!("{} ({})", o["nombre"].as_str().unwrap_or("?"), o["motivo"].as_str().unwrap_or("sin motivo"))).collect();
    if lista.is_empty() {
        vec![]
    } else {
        vec![format!("variables sin guardar: {}", lista.join(", "))]
    }
}

/// Apertura en curso: lo cargado en local espera a que el motor cargue los datos del espacio de trabajo.
struct Abriendo {
    id: u64,
    carpeta: PathBuf,
    problemas: Vec<String>,
}

/// Aviso final de la apertura: «abierto» a secas solo si todo se leyó.
fn resumen_apertura(carpeta: &Path, problemas: &[String]) -> String {
    if problemas.is_empty() {
        format!("Proyecto abierto: {}", carpeta.display())
    } else {
        format!("Proyecto abierto con {} problemas ({}): {}", problemas.len(), carpeta.display(), problemas.join("; "))
    }
}

/// Comprueba que la carpeta sea un proyecto de Heliora: un `proyecto.json` con el formato esperado o, en
/// las carpetas anteriores a ese archivo, el diagrama principal.
fn validar_proyecto(c: &Path) -> Result<(), String> {
    let doc = c.join("proyecto.json");
    if doc.exists() {
        let t = std::fs::read_to_string(&doc).map_err(|e| format!("{}: {e}", doc.display()))?;
        let v: Value = serde_json::from_str(&t).map_err(|e| format!("proyecto.json no es un JSON válido ({e})"))?;
        return match v["formato"].as_str() {
            Some("heliora-proyecto/1") => Ok(()),
            otro => Err(format!("proyecto.json tiene el formato «{}» y se esperaba «heliora-proyecto/1»", otro.unwrap_or("(ninguno)"))),
        };
    }
    if c.join("diagramas").join("principal.json").exists() {
        return Ok(());
    }
    Err(format!("«{}» no es un proyecto de Heliora: no tiene proyecto.json ni diagramas/principal.json", c.display()))
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
    guardado: Option<Guardado>,
    abriendo: Option<Abriendo>,
    /// Objetos de Python (`.pkl`) de un proyecto abierto que esperan confirmación: carpeta y nombres
    pkl_pendientes: Option<(PathBuf, Vec<String>)>,
    id_pkl: Option<u64>,
    pidiendo_columnas: HashMap<u64, String>,
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
    paleta: comandos::Paleta,
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
        let mut editor = Editor::nuevo();
        if let Some(r) = cc.storage.and_then(|s| s.get_string("recientes")) {
            editor.recientes = r.split(',').filter(|t| !t.is_empty()).map(String::from).collect();
        }
        let mut mantenimiento = Mantenimiento::nuevo();
        if let Some(p) = &op.pestana {
            mantenimiento.fijar_pestana(p);
        }
        App {
            motor: Motor::iniciar(),
            modulo,
            compartido: Compartido::default(),
            editor,
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
            guardado: None,
            abriendo: None,
            pkl_pendientes: None,
            id_pkl: None,
            pidiendo_columnas: HashMap::new(),
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
            paleta: comandos::Paleta::default(),
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
        if let Some(mut g) = self.guardado.take_if(|g| g.id == id) {
            if ok {
                g.problemas.extend(omitidos_del_espacio(v));
            } else {
                g.problemas.push(format!("datos del espacio de trabajo: {err}"));
            }
            self.aviso = resumen_guardado(&g.carpeta, &g.problemas);
            return;
        }
        if let Some(mut a) = self.abriendo.take_if(|a| a.id == id) {
            if ok {
                self.compartido.fijar_variables(&v["variables"]);
                a.problemas.extend(v["errores"].as_array().into_iter().flatten().filter_map(|e| e.as_str()).map(|e| format!("datos: {e}")));
                let pendientes: Vec<String> = v["pendientes"].as_array().into_iter().flatten().filter_map(|e| e.as_str().map(String::from)).collect();
                if !pendientes.is_empty() {
                    self.pkl_pendientes = Some((a.carpeta.join("datos"), pendientes));
                }
            } else {
                a.problemas.push(format!("datos del espacio de trabajo: {err}"));
            }
            self.aviso = resumen_apertura(&a.carpeta, &a.problemas);
            return;
        }
        if Some(id) == self.id_pkl {
            self.id_pkl = None;
            if ok {
                self.compartido.fijar_variables(&v["variables"]);
                let errores: Vec<&str> = v["errores"].as_array().into_iter().flatten().filter_map(|e| e.as_str()).collect();
                self.aviso = if errores.is_empty() { "Objetos de Python cargados".into() } else { format!("Objetos de Python con errores: {}", errores.join("; ")) };
            } else {
                self.aviso = format!("No se pudieron cargar los objetos de Python: {err}");
            }
            return;
        }
        if let Some(ruta) = self.pidiendo_columnas.remove(&id) {
            // Un archivo ilegible se anota sin columnas: el inspector vuelve al campo de texto
            let cols = if ok { v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect() } else { vec![] };
            if !ok {
                self.aviso = format!("No se pudieron leer las columnas de {ruta}: {err}");
            }
            self.compartido.columnas.insert(ruta, cols);
            return;
        }
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
            .exact_height(48.0)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin { left: 0, right: 12, top: 0, bottom: 0 }).stroke(egui::Stroke::new(1.0_f32, pal.linea)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // El logo ocupa el ancho del riel para alinearse con sus iconos
                    let (r, _) = ui.allocate_exact_size(Vec2::new(56.0, 26.0), Sense::hover());
                    tema::logo(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(26.0)));
                    ui.add_space(8.0);
                    self.ruta(ui);
                    let ancho_busqueda = 330.0;
                    let derecha = 420.0;
                    let hueco = ((ui.available_width() - ancho_busqueda - derecha) / 2.0).max(8.0);
                    ui.add_space(hueco);
                    if self.boton_buscar(ui, ancho_busqueda).clicked() {
                        self.paleta.abrir();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.estado_motor(ui);
                        ui.add_space(6.0);
                        self.pildora_simulacion(ui);
                    });
                });
            });
    }

    /// Ruta «proyecto / módulo / diagrama ●»: el proyecto y el diagrama abren su menú.
    fn ruta(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        ui.spacing_mut().item_spacing.x = 6.0;
        let proyecto = self.carpeta.as_ref().and_then(|c| c.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Proyecto sin guardar".into());
        let r = ui.add(egui::Button::new(egui::RichText::new(proyecto).color(pal.texto_3)).frame(false));
        egui::Popup::menu(&r).show(|ui| {
            if ui.button("Abrir proyecto…").clicked() {
                self.abrir_proyecto();
            }
            if ui.button("Guardar proyecto").clicked() {
                self.guardar_proyecto();
            }
        });
        ui.label(egui::RichText::new("/").color(pal.texto_3));
        // El último tramo de la ruta va en TEXTO; los anteriores, en TEXTO_3
        let detalle = match self.modulo {
            Modulo::Potencia => self.potencia.titulo_seleccion(&self.herramientas),
            Modulo::Fotovoltaica => self.fotovoltaica.titulo_seleccion(&self.herramientas),
            Modulo::Estadistica => self.estadistica.titulo_seleccion(&self.herramientas),
            Modulo::Aprendizaje => self.aprendizaje.titulo_seleccion(&self.herramientas),
            Modulo::Mantenimiento => Some(self.mantenimiento.nombre_pestana().to_string()),
            _ => None,
        };
        let ultimo = detalle.is_none() && self.modulo != Modulo::Bloques;
        ui.label(egui::RichText::new(tema::nombre_modulo(self.modulo)).color(if ultimo { pal.texto } else { pal.texto_3 }));
        if let Some(d) = detalle {
            ui.label(egui::RichText::new("/").color(pal.texto_3));
            ui.label(egui::RichText::new(d).color(pal.texto));
        }
        if self.modulo != Modulo::Bloques {
            return;
        }
        ui.label(egui::RichText::new("/").color(pal.texto_3));
        let nombre = if self.editor.diagrama.nombre.is_empty() { "Diagrama sin nombre".to_string() } else { self.editor.diagrama.nombre.clone() };
        let r = ui.add(egui::Button::new(egui::RichText::new(nombre).color(pal.texto).family(tema::semi())).frame(false));
        egui::Popup::menu(&r).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_min_width(220.0);
            ui.add(egui::TextEdit::singleline(&mut self.editor.diagrama.nombre).hint_text("Nombre del diagrama"));
            if ui.button("Nuevo diagrama").clicked() {
                self.editor.nuevo_diagrama();
                ui.close();
            }
            if ui.button("Abrir diagrama…").clicked() {
                self.editor.abrir_diagrama();
                ui.close();
            }
            if ui.button("Guardar diagrama").clicked() {
                self.editor.guardar_diagrama();
                ui.close();
            }
            ui.menu_button("Ejemplos", |ui| {
                for (k, n) in self.editor.ejemplos.clone() {
                    if ui.button(n).clicked() {
                        self.editor.pedir_ejemplo(&mut self.motor, &k);
                        ui.close();
                    }
                }
            });
        });
        if self.editor.sucio() {
            let (r, _) = ui.allocate_exact_size(Vec2::new(6.0, 6.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 3.0, pal.acento);
            ui.interact(r, ui.id().with("sucio"), Sense::hover()).on_hover_text("Cambios sin guardar");
        }
    }

    /// Botón que abre la paleta de comandos, con la pista del atajo.
    fn boton_buscar(&self, ui: &mut egui::Ui, ancho: f32) -> egui::Response {
        let pal = tema::p();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho, 30.0), Sense::click());
        let p = ui.painter();
        p.rect(r, 6.0, if resp.hovered() { pal.realce } else { pal.panel }, egui::Stroke::new(1.0_f32, pal.borde), egui::StrokeKind::Inside);
        let c = Pos2::new(r.left() + 16.0, r.center().y);
        p.circle_stroke(c - Vec2::new(1.0, 1.0), 4.5, egui::Stroke::new(1.5_f32, pal.texto_3));
        p.line_segment([c + Vec2::new(2.5, 2.5), c + Vec2::new(5.5, 5.5)], egui::Stroke::new(1.5_f32, pal.texto_3));
        p.text(Pos2::new(r.left() + 30.0, r.center().y), egui::Align2::LEFT_CENTER, "Buscar bloque, comando o variable", egui::FontId::proportional(13.0), pal.texto_3);
        let g = p.layout_no_wrap("Ctrl K".into(), tema::fuente_mono(11.0), pal.texto_3);
        let caja = Rect::from_min_size(Pos2::new(r.right() - g.size().x - 18.0, r.center().y - 9.0), Vec2::new(g.size().x + 10.0, 18.0));
        p.rect_stroke(caja, 4.0, egui::Stroke::new(1.0_f32, pal.linea_fuerte), egui::StrokeKind::Inside);
        p.galley(caja.center() - g.size() / 2.0, g, pal.texto_3);
        resp.on_hover_text("Paleta de comandos (Ctrl K)")
    }

    /// Píldora con el resumen del solver (clic para cambiarlo) y el botón Simular.
    fn pildora_simulacion(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        // La barra se llena de derecha a izquierda: primero el botón, después el resumen. El fondo
        // se pinta al final, ajustado a lo que ocupan, para que la píldora no se estire.
        let fondo = ui.painter().add(egui::Shape::Noop);
        ui.add_space(4.0);
        let mut ocupado = Rect::NOTHING;
        if self.editor.id_sim.is_some() {
            let d = ui.small_button("■ Detener");
            if d.clicked() {
                self.motor.llamar("cancelar", json!({}));
            }
            ocupado = ocupado.union(d.rect);
            ocupado = ocupado.union(ui.add(egui::ProgressBar::new(self.editor.avance).desired_width(90.0).desired_height(14.0)).rect);
        } else {
            let p = tema::p();
            let texto = egui::text::LayoutJob::simple_format("▶ Simular".into(), egui::TextFormat::simple(tema::fuente_semi(13.0), p.sobre_acento));
            let mut job = texto;
            job.append("  F5", 0.0, egui::TextFormat::simple(tema::fuente_mono(11.0), p.sobre_acento.gamma_multiply(0.7)));
            let b = ui.add(egui::Button::new(job).fill(p.acento).stroke(egui::Stroke::NONE).corner_radius(5).min_size(Vec2::new(0.0, 24.0)));
            if b.clicked() {
                self.simular();
            }
            ocupado = ocupado.union(b.rect);
        }
        let r = ui.add(egui::Button::new(egui::RichText::new(self.editor.resumen_solver()).font(tema::fuente_mono(12.0)).color(pal.texto_2)).frame(false)).on_hover_text("Solver e intervalo de simulación");
        egui::Popup::menu(&r).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_min_width(260.0);
            self.editor.panel_solver_rapido(ui);
        });
        ocupado = ocupado.union(r.rect);
        ui.add_space(6.0);
        let c = ui.max_rect().center().y;
        let caja = Rect::from_min_max(Pos2::new(ocupado.left() - 6.0, c - 16.0), Pos2::new(ocupado.right() + 4.0, c + 16.0));
        ui.painter().set(fondo, egui::epaint::RectShape::new(caja, 7, pal.campo, egui::Stroke::new(1.0_f32, pal.borde), egui::StrokeKind::Inside));
    }

    /// Lanza la simulación del diagrama de Bloques desde cualquier módulo.
    fn simular(&mut self) {
        if self.editor.id_sim.is_none() && self.motor.listo {
            self.editor.ejecutar(&mut self.motor);
            self.modulo = Modulo::Bloques;
        }
    }

    /// Punto y texto del estado del motor; su menú permite reiniciarlo.
    fn estado_motor(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        let (nivel, texto) = if self.motor.listo && self.motor.ocupado() {
            (tema::Nivel::Ok, "Motor trabajando")
        } else if self.motor.listo {
            (tema::Nivel::Ok, "Motor listo")
        } else if self.motor.caido.is_some() {
            (tema::Nivel::Falla, "Motor sin conexión")
        } else {
            (tema::Nivel::Alerta, "Iniciando el motor…")
        };
        let r = ui.add(egui::Button::new(egui::RichText::new(texto).size(12.0).color(pal.texto_2)).frame(false));
        let (p, _) = ui.allocate_exact_size(Vec2::new(7.0, 14.0), Sense::hover());
        tema::punto(ui.painter(), p.center(), nivel.color());
        egui::Popup::menu(&r).show(|ui| {
            if ui.add_enabled(self.motor.listo, egui::Button::new("Reiniciar el motor")).on_hover_text("Borra las variables y vuelve a cargar el motor").clicked() {
                self.reiniciar_motor();
            }
        });
    }

    fn reiniciar_motor(&mut self) {
        self.motor.reiniciar();
        self.iniciado = false;
        self.compartido.variables.clear();
    }

    fn cambiar_modo(&mut self, ctx: &egui::Context, m: tema::Modo) {
        if self.modo != m {
            self.modo = m;
            tema::aplicar(ctx, m);
        }
    }

    /// Barra de estado: versiones del motor, avisos, bloques visibles y tema.
    fn pie(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        egui::TopBottomPanel::bottom("pie")
            .exact_height(24.0)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(12, 0)).stroke(egui::Stroke::new(1.0_f32, pal.linea)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let f = tema::fuente_mono(11.0);
                    ui.spacing_mut().item_spacing.x = 16.0;
                    // Hasta la primera simulación, las versiones salen de la última validación guardada
                    let versiones = if self.editor.tiene_procedencia() { self.editor.versiones() } else { crate::bloques::versiones_de(&self.validacion.procedencia) };
                    ui.label(egui::RichText::new(versiones).font(f.clone()).color(pal.texto_3));
                    let ancho = ui.available_width();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("Tema: {}", self.modo.nombre())).font(f.clone()).color(pal.texto_3));
                        if self.modulo == Modulo::Bloques {
                            ui.label(egui::RichText::new(format!("{} bloques visibles", self.editor.bloques_visibles())).font(f.clone()).color(pal.texto_3));
                        }
                        if !self.aviso.is_empty() {
                            ui.set_max_width(ancho - 40.0);
                            ui.add(egui::Label::new(egui::RichText::new(&self.aviso).font(f).color(pal.texto_2)).truncate()).on_hover_text(&self.aviso);
                        }
                    });
                });
            });
    }

    /// Entradas de la paleta: comandos, bloques del catálogo, ejemplos y variables.
    fn entradas_paleta(&self) -> Vec<comandos::Entrada> {
        let mut v = comandos::comandos_fijos();
        v.extend(self.editor.entradas_paleta());
        for x in &self.compartido.variables {
            v.push(comandos::Entrada::nueva("Variable", x.nombre.clone(), format!("{} {}", x.tipo, x.forma), comandos::Accion::Variable(x.nombre.clone())));
        }
        v
    }

    fn ejecutar_comando(&mut self, ctx: &egui::Context, a: comandos::Accion) {
        use comandos::Accion as A;
        match a {
            A::Modulo(m) => self.modulo = m,
            A::Bloque(t) => {
                self.modulo = Modulo::Bloques;
                self.editor.agregar_al_centro(&t);
            }
            A::Ejemplo(k) => {
                self.modulo = Modulo::Bloques;
                self.editor.pedir_ejemplo(&mut self.motor, &k);
            }
            A::Simular => self.simular(),
            A::Encuadrar => {
                self.modulo = Modulo::Bloques;
                self.editor.encuadrar();
            }
            A::OrdenarCables => {
                self.modulo = Modulo::Bloques;
                self.editor.ordenar_cables = !self.editor.ordenar_cables;
            }
            A::Linealizar => {
                self.modulo = Modulo::Bloques;
                if self.editor.id_lin.is_none() && self.editor.id_sim.is_none() {
                    self.editor.linealizar(&mut self.motor);
                }
            }
            A::NuevoDiagrama => {
                self.modulo = Modulo::Bloques;
                self.editor.nuevo_diagrama();
            }
            A::AbrirDiagrama => {
                self.modulo = Modulo::Bloques;
                self.editor.abrir_diagrama();
            }
            A::GuardarDiagrama => self.editor.guardar_diagrama(),
            A::AbrirProyecto => self.abrir_proyecto(),
            A::GuardarProyecto => self.guardar_proyecto(),
            A::Tema(m) => self.cambiar_modo(ctx, m),
            A::ReiniciarMotor => self.reiniciar_motor(),
            A::Variable(n) => {
                ctx.copy_text(n.clone());
                self.aviso = format!("«{n}» copiado al portapapeles");
                self.modulo = Modulo::Calculo;
            }
        }
    }

    fn abrir_proyecto(&mut self) {
        let Some(c) = rfd::FileDialog::new().set_title("Carpeta del proyecto").pick_folder() else { return };
        if let Err(e) = validar_proyecto(&c) {
            self.aviso = format!("No se abrió el proyecto: {e}");
            return;
        }
        let mut problemas = vec![];
        // Lo de la sesión anterior no debe mezclarse con el proyecto que se abre
        self.motor.llamar("limpiar_espacio", json!({}));
        self.compartido.variables.clear();
        self.editor.nuevo_diagrama();
        let d = c.join("diagramas").join("principal.json");
        if d.exists() {
            if let Err(e) = self.editor.abrir_de(&d) {
                problemas.push(format!("diagrama: {e}"));
            }
        }
        let m = c.join("mantenimiento.json");
        self.mantenimiento.estado = Value::Null;
        self.mantenimiento.tablero = Value::Null;
        self.mantenimiento.ruta = None;
        self.pkl_pendientes = None;
        // La carga del plan es asíncrona: el plan de ejemplo solo se pide si el proyecto no trae uno,
        // porque su respuesta llegaría después y sustituiría al plan del proyecto
        let cargando_plan = m.exists()
            && match self.mantenimiento.cargar_de(&m, &mut self.motor) {
                Ok(()) => true,
                Err(e) => {
                    problemas.push(format!("mantenimiento: {e}"));
                    false
                }
            };
        if !cargando_plan {
            self.mantenimiento.iniciar(&mut self.motor);
        }
        let (guiones, errores) = crate::calculo::guiones_de(&c.join("scripts"));
        self.calculo.fijar_guiones(guiones);
        problemas.extend(errores.into_iter().map(|e| format!("script {e}")));
        let datos = c.join("datos");
        if datos.exists() {
            let id = self.motor.llamar("cargar_espacio", json!({"carpeta": datos.display().to_string()}));
            self.abriendo = Some(Abriendo { id, carpeta: c.clone(), problemas });
            self.aviso = "Abriendo el proyecto: esperando a que el motor cargue los datos...".into();
        } else {
            self.aviso = resumen_apertura(&c, &problemas);
        }
        self.carpeta = Some(c);
    }

    /// Un proyecto es una carpeta: diagramas en JSON y datos en CSV/Parquet.
    fn guardar_proyecto(&mut self) {
        if self.carpeta.is_none() {
            self.carpeta = rfd::FileDialog::new().set_title("Carpeta para el proyecto").pick_folder();
        }
        let Some(c) = self.carpeta.clone() else { return };
        let mut problemas = vec![];
        for d in ["diagramas", "scripts"] {
            if let Err(e) = std::fs::create_dir_all(c.join(d)) {
                problemas.push(format!("{}: {e}", c.join(d).display()));
            }
        }
        let mut anotar = |r: Result<(), String>| {
            if let Err(e) = r {
                problemas.push(e);
            }
        };
        // Cada proyecto es una carpeta lista para Git: diagramas JSON como texto, datos binarios aparte
        if !c.join(".gitattributes").exists() {
            anotar(escribir_atomico(&c.join(".gitattributes"), GITATTRIBUTES_PROYECTO.as_bytes()));
        }
        anotar(self.editor.guardar_en(&c.join("diagramas").join("principal.json")));
        if !self.mantenimiento.estado.is_null() {
            anotar(self.mantenimiento.guardar_en(&c.join("mantenimiento.json")));
        }
        let propios: Vec<&crate::calculo::Guion> = self.calculo.guiones.iter().filter(|g| !g.motor).collect();
        let nombres = crate::calculo::nombres_unicos(&propios.iter().map(|g| g.nombre.clone()).collect::<Vec<_>>());
        for (g, nombre) in propios.iter().zip(&nombres) {
            anotar(escribir_atomico(&c.join("scripts").join(nombre), g.texto.as_bytes()));
        }
        let doc = json!({"formato": "heliora-proyecto/1", "diagrama": "diagramas/principal.json", "mantenimiento": "mantenimiento.json", "datos": "datos", "scripts": "scripts"});
        anotar(serde_json::to_string_pretty(&doc).map_err(|e| e.to_string()).and_then(|t| escribir_atomico(&c.join("proyecto.json"), t.as_bytes())));
        // El aviso final espera a la respuesta del motor, que guarda los datos del espacio de trabajo
        let id = self.motor.llamar("guardar_espacio", json!({"carpeta": c.join("datos").display().to_string()}));
        self.guardado = Some(Guardado { id, carpeta: c, problemas });
        self.aviso = "Guardando el proyecto: esperando a que el motor guarde los datos...".into();
    }

    fn barra_modulos(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        egui::SidePanel::left("rail")
            .exact_width(56.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(8, 10)).stroke(egui::Stroke::new(1.0_f32, pal.linea)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                let grupos = tema::MODULOS.len();
                for (g, grupo) in tema::MODULOS.iter().enumerate() {
                    // El último grupo (consulta) va abajo, junto a los ajustes
                    if g == grupos - 1 {
                        ui.add_space((ui.available_height() - 44.0 * grupo.len() as f32 - 44.0).max(0.0));
                    } else if g > 0 {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(40.0, 9.0), Sense::hover());
                        ui.painter().line_segment([Pos2::new(r.center().x - 12.0, r.center().y), Pos2::new(r.center().x + 12.0, r.center().y)], egui::Stroke::new(1.0_f32, pal.linea));
                    }
                    for (m, nombre) in grupo.iter() {
                        if self.boton_riel(ui, nombre, self.modulo == *m, |pt, r, c| tema::icono(pt, r, *m, c)).clicked() {
                            self.modulo = *m;
                        }
                    }
                }
                let r = self.boton_riel(ui, "Ajustes y tema", false, tema::icono_ajustes);
                egui::Popup::menu(&r).show(|ui| {
                    tema::titulo(ui, "Tema");
                    for m in [tema::Modo::Claro, tema::Modo::Oscuro] {
                        if ui.radio(self.modo == m, m.nombre()).clicked() {
                            self.cambiar_modo(ui.ctx(), m);
                        }
                    }
                    ui.separator();
                    if ui.add_enabled(self.motor.listo, egui::Button::new("Reiniciar el motor")).clicked() {
                        self.reiniciar_motor();
                    }
                });
            });
    }

    /// Botón cuadrado de 40 px del riel; el activo lleva fondo de acento y barra de 2 px.
    fn boton_riel(&self, ui: &mut egui::Ui, nombre: &str, activo: bool, dibujar: impl FnOnce(&egui::Painter, Rect, egui::Color32)) -> egui::Response {
        let pal = tema::p();
        let (r, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
        let p = ui.painter();
        if activo {
            p.rect_filled(r, 8.0, pal.acento_fondo);
            p.rect_filled(Rect::from_min_size(Pos2::new(r.left(), r.top() + 6.0), Vec2::new(2.0, r.height() - 12.0)), 1.0, pal.acento);
        } else if resp.hovered() {
            p.rect_filled(r, 8.0, pal.realce);
        }
        let color = if activo { pal.acento } else if resp.hovered() { pal.texto } else { pal.texto_3 };
        dibujar(p, Rect::from_center_size(r.center(), Vec2::splat(19.0)), color);
        resp.on_hover_text(nombre)
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
                        ui.painter().rect(r, tema::radio_ventana() as f32, if hover { pal.campo } else { pal.panel }, egui::Stroke::new(1.0_f32, if hover { pal.linea_fuerte } else { pal.linea }), egui::StrokeKind::Inside);
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
                        egui::Grid::new("g_ref").striped(true).num_columns(4).spacing([18.0, 4.0]).show(ui, |ui| {
                            for p in &x.params {
                                ui.label(&p.etiqueta).on_hover_text(if p.ayuda.is_empty() { &p.etiqueta } else { &p.ayuda });
                                ui.label(egui::RichText::new(match &p.defecto {
                                    Value::String(s) => s.clone(),
                                    o => o.to_string(),
                                }).monospace());
                                ui.label(egui::RichText::new(&p.unidad).color(tema::p().texto_2));
                                ui.label(egui::RichText::new(&p.ayuda).small().color(tema::p().texto_2));
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

    /// Aviso de los objetos de Python de un proyecto que no se cargaron al abrirlo: leer un `.pkl`
    /// puede ejecutar código, así que solo se cargan si la persona confía en el proyecto.
    fn panel_pkl(&mut self, ctx: &egui::Context) {
        let Some((carpeta, nombres)) = self.pkl_pendientes.clone() else { return };
        let pal = tema::p();
        egui::TopBottomPanel::top("pkl_pendientes").frame(egui::Frame::new().fill(pal.campo).inner_margin(10).stroke(egui::Stroke::new(1.0_f32, pal.borde))).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                tema::estado(ui, tema::Nivel::Alerta, &format!("El proyecto trae {} objetos de Python sin cargar ({}).", nombres.len(), nombres.join(", ")));
                ui.label(egui::RichText::new("Leerlos puede ejecutar código: cárgalos solo si confías en quien te pasó el proyecto.").small().color(pal.texto_2));
                if ui.button("Cargar objetos").clicked() {
                    self.id_pkl = Some(self.motor.llamar("cargar_espacio", json!({"carpeta": carpeta.display().to_string(), "confiar": true, "solo": nombres})));
                    self.pkl_pendientes = None;
                }
                if ui.button("No cargar").clicked() {
                    self.pkl_pendientes = None;
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
        if let Some(v) = self.compartido.catalogo_nuevo.take() {
            self.editor.fijar_catalogo(&v);
        }
        if let Some(ruta) = self.compartido.pedir_columnas.take() {
            if !self.pidiendo_columnas.values().any(|r| *r == ruta) {
                let id = self.motor.llamar("columnas", json!({"ruta": ruta}));
                self.pidiendo_columnas.insert(id, ruta);
            }
        }
        if let Some((ruta, linea)) = self.compartido.abrir_codigo.take() {
            self.calculo.abrir_codigo(&mut self.motor, ruta, linea);
            self.modulo = Modulo::Calculo;
        }
        // Atajos globales: F5 simula y Ctrl K abre la paleta de comandos
        let (f5, ctrl_k) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, egui::Key::F5), i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)));
        // Con un campo de texto activo, F5 no saca a la persona de lo que está escribiendo
        if f5 && !ctx.wants_keyboard_input() && !self.paleta.abierta {
            self.simular();
        }
        if ctrl_k {
            self.paleta.abrir();
        }
        self.barra_superior(ctx);
        self.pie(ctx);
        self.panel_sin_motor(ctx);
        self.panel_pkl(ctx);
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
        if self.paleta.abierta {
            let entradas = self.entradas_paleta();
            if let Some(a) = self.paleta.mostrar(ctx, &entradas) {
                self.ejecutar_comando(ctx, a);
            }
        }
    }

    /// Fondo de la ventana del color de los paneles: así ninguna junta entre paneles queda oscura.
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("modo", self.modo.clave().to_string());
        storage.set_string("recientes", self.editor.recientes.join(","));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.motor.detener();
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn la_escritura_atomica_reemplaza_y_no_deja_temporales() {
        let dir = std::env::temp_dir().join(format!("heliora_atomico_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("proyecto.json");
        escribir_atomico(&ruta, b"uno").unwrap();
        escribir_atomico(&ruta, b"dos").unwrap();
        assert_eq!(std::fs::read(&ruta).unwrap(), b"dos");
        assert!(!dir.join("proyecto.json.tmp~").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_guardado_con_una_escritura_fallida_no_se_anuncia_como_guardado() {
        // Una «carpeta» que en realidad es un archivo: ninguna escritura dentro puede funcionar
        let dir = std::env::temp_dir().join(format!("heliora_bloqueo_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bloqueo = dir.join("no_es_carpeta");
        std::fs::write(&bloqueo, b"x").unwrap();
        let r = escribir_atomico(&bloqueo.join("scripts.py"), b"print(1)");
        assert!(r.is_err());
        let problemas = vec![r.unwrap_err()];
        let aviso = resumen_guardado(&dir, &problemas);
        assert!(aviso.contains("NO se guardó") && aviso.contains("scripts.py"), "{aviso}");
        assert!(resumen_guardado(&dir, &[]).starts_with("Proyecto guardado"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_guardado_que_omite_variables_no_se_anuncia_como_completo() {
        let v = json!({"guardados": ["a"], "omitidos": [{"nombre": "f", "motivo": "function: no serializable"}]});
        let problemas = omitidos_del_espacio(&v);
        let aviso = resumen_guardado(Path::new("/p"), &problemas);
        assert!(aviso.contains("NO se guardó") && aviso.contains("f (function: no serializable)"), "{aviso}");
        assert!(omitidos_del_espacio(&json!({"guardados": ["a"], "omitidos": []})).is_empty());
    }

    #[test]
    fn solo_se_abre_una_carpeta_que_es_un_proyecto() {
        let dir = std::env::temp_dir().join(format!("heliora_proyecto_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("diagramas")).unwrap();
        let vacia = dir.join("vacia");
        std::fs::create_dir_all(&vacia).unwrap();
        assert!(validar_proyecto(&vacia).unwrap_err().contains("no es un proyecto de Heliora"));
        // Carpeta anterior a proyecto.json: vale si tiene el diagrama principal
        std::fs::write(dir.join("diagramas").join("principal.json"), "{}").unwrap();
        assert!(validar_proyecto(&dir).is_ok());
        std::fs::write(dir.join("proyecto.json"), r#"{"formato": "otro/9"}"#).unwrap();
        assert!(validar_proyecto(&dir).unwrap_err().contains("heliora-proyecto/1"));
        std::fs::write(dir.join("proyecto.json"), "no es json").unwrap();
        assert!(validar_proyecto(&dir).unwrap_err().contains("JSON"));
        std::fs::write(dir.join("proyecto.json"), r#"{"formato": "heliora-proyecto/1"}"#).unwrap();
        assert!(validar_proyecto(&dir).is_ok());
        assert!(resumen_apertura(&dir, &["script x".into()]).contains("1 problemas"));
        assert!(resumen_apertura(&dir, &[]).starts_with("Proyecto abierto:"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn las_columnas_de_un_archivo_se_piden_una_vez_y_las_de_una_variable_salen_de_la_lista() {
        let mut c = Compartido::default();
        let con_ruta = json!({"ruta": "/datos/a.csv"}).as_object().unwrap().clone();
        assert!(c.columnas_de(&con_ruta).is_empty());
        assert_eq!(c.pedir_columnas.as_deref(), Some("/datos/a.csv"));
        c.columnas.insert("/datos/a.csv".into(), vec!["t".into(), "p".into()]);
        assert_eq!(c.columnas_de(&con_ruta), ["t", "p"]);
        c.variables.push(Variable { nombre: "d".into(), columnas: vec!["x".into()], ..Default::default() });
        let con_variable = json!({"variable": "d"}).as_object().unwrap().clone();
        assert_eq!(c.columnas_de(&con_variable), ["x"]);
        assert!(c.columnas_de(&json!({"variable": "otra"}).as_object().unwrap().clone()).is_empty());
    }
}
