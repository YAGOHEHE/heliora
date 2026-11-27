//! La ventana principal: barra de módulos, proyecto y reparto de mensajes del motor.
use crate::bloques::{self, Editor};
use crate::calculo::Calculo;
use crate::comandos;
use crate::grafica::{self, Figura};
use crate::iconos::Icono;
use crate::herramientas::{self, Herramienta, Pagina};
use crate::motor::{self, Mensaje, Motor};
use crate::plan::Mantenimiento;
use crate::tema::{self, Modulo};
use crate::ui;
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
    /// Diálogo de confirmación de «Nuevo proyecto» abierto
    confirmar_nuevo: bool,
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
    /// Esquema del proyecto plegado (None: según el ancho de la ventana).
    esquema_plegado: Option<bool>,
    /// Pestaña del panel izquierdo: 0 esquema, 1 opciones.
    pestana_esquema: usize,
    filtro_esquema: String,
    /// Nodos del esquema que la persona ha cerrado.
    esquema_cerrados: std::collections::HashSet<String>,
    /// Filtro de las próximas órdenes de la portada: «vencidas», «30» o «pendientes».
    filtro_inicio: String,
    /// Tipo de orden elegido en los chips de la portada.
    tipo_inicio: Option<String>,
    /// Próximas órdenes ordenadas por equipo en lugar de por fecha.
    orden_inicio_equipo: bool,
    /// Tamaño de visualización elegido (factor de zoom de egui).
    zoom: f32,
}

/// Límites y paso del tamaño de visualización.
const ZOOM_MIN: f32 = 0.75;
const ZOOM_MAX: f32 = 2.0;
const ZOOM_PASO: f32 = 0.1;

/// Módulos del menú, en orden.
const MENU_MODULOS: [(Modulo, &str); 9] = [
    (Modulo::Inicio, "Inicio"),
    (Modulo::Bloques, "Bloques"),
    (Modulo::Potencia, "Potencia"),
    (Modulo::Fotovoltaica, "Solar"),
    (Modulo::Calculo, "Cálculo"),
    (Modulo::Estadistica, "Estadística"),
    (Modulo::Aprendizaje, "Aprendizaje"),
    (Modulo::Mantenimiento, "Mantenimiento"),
    (Modulo::Validacion, "Validación"),
];

impl App {
    pub fn nueva(cc: &eframe::CreationContext, op: Opciones) -> App {
        // El modo se guarda entre sesiones; --modo lo fija para las capturas
        let guardado = cc.storage.and_then(|s| s.get_string("modo"));
        let modo = tema::Modo::desde(op.modo.as_deref().or(guardado.as_deref()).unwrap_or("claro"));
        tema::aplicar(&cc.egui_ctx, modo);
        // Las capturas fijan su propia escala; fuera de ellas se recupera la de la sesión anterior
        let zoom = cc.storage.and_then(|s| s.get_string("zoom")).and_then(|z| z.parse::<f32>().ok()).filter(|z| z.is_finite()).unwrap_or(1.0).clamp(ZOOM_MIN, ZOOM_MAX);
        if op.captura.is_none() {
            cc.egui_ctx.set_zoom_factor(zoom);
        }
        crate::iconos::registrar(&cc.egui_ctx);
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
            confirmar_nuevo: false,
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
            esquema_plegado: None,
            pestana_esquema: 0,
            filtro_esquema: String::new(),
            esquema_cerrados: Default::default(),
            filtro_inicio: "30".into(),
            tipo_inicio: None,
            orden_inicio_equipo: false,
            zoom,
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

    /// Barra superior gris de 44 px: marca, guardar, deshacer y rehacer; buscador centrado; píldora
    /// del solver con «Simular», estado del motor y ayuda.
    fn barra_superior(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        egui::TopBottomPanel::top("superior")
            .exact_height(44.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(pal.campo).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                    tema::logo(ui.painter(), r);
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Heliora").family(tema::semi()).size(14.0).color(pal.texto));
                    ui.add_space(12.0);
                    if ui::boton_icono(ui, Icono::Guardar, 18.0, "Guardar el proyecto").clicked() {
                        self.guardar_proyecto();
                    }
                    if ui::boton_icono(ui, Icono::Deshacer, 18.0, "Deshacer en el diagrama").clicked() {
                        self.editor.deshacer_ya();
                    }
                    if ui::boton_icono(ui, Icono::Rehacer, 18.0, "Rehacer en el diagrama").clicked() {
                        self.editor.rehacer_ya();
                    }
                    let ancho_busqueda = 380.0_f32.min(ui.available_width() - 560.0).max(200.0);
                    let centro = ui.max_rect().center().x;
                    let hueco = (centro - ancho_busqueda / 2.0 - ui.cursor().min.x).max(8.0);
                    ui.add_space(hueco);
                    if self.boton_buscar(ui, ancho_busqueda).clicked() {
                        self.paleta.abrir();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 16.0;
                        if ui::boton_texto(ui, Some(Icono::Ayuda), "Ayuda").on_hover_text("Referencia de bloques y funciones").clicked() {
                            self.modulo = Modulo::Referencia;
                        }
                        self.estado_motor(ui);
                        self.pildora_simulacion(ui);
                    });
                });
            });
    }

    /// Buscador que abre la paleta de comandos, con la pista del atajo.
    fn boton_buscar(&self, ui: &mut egui::Ui, ancho: f32) -> egui::Response {
        let pal = tema::p();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho, 30.0), Sense::click());
        let p = ui.painter();
        p.rect(r, 0.0, pal.panel, egui::Stroke::new(1.0_f32, if resp.hovered() { pal.texto_2 } else { pal.borde }), egui::StrokeKind::Inside);
        crate::iconos::dibujar(p, Rect::from_min_size(Pos2::new(r.left() + 10.0, r.center().y - 8.0), Vec2::splat(16.0)), Icono::Buscar, pal.texto_2);
        p.text(Pos2::new(r.left() + 34.0, r.center().y), egui::Align2::LEFT_CENTER, "Buscar bloque, comando o variable", egui::FontId::proportional(13.0), pal.texto_2);
        p.text(Pos2::new(r.right() - 10.0, r.center().y), egui::Align2::RIGHT_CENTER, "Ctrl K", tema::fuente_mono(11.0), pal.texto_2);
        resp.on_hover_text("Paleta de comandos (Ctrl K)")
    }

    /// Píldora con el resumen del solver (clic para cambiarlo) y el botón negro «Simular F5».
    fn pildora_simulacion(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        // La barra se llena de derecha a izquierda: primero el botón, después el resumen. El borde
        // se pinta al final, ajustado a lo que ocupan.
        let fondo = ui.painter().add(egui::Shape::Noop);
        let espacio = ui.spacing().item_spacing.x;
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut ocupado = Rect::NOTHING;
        if self.editor.id_sim.is_some() {
            let d = ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Detener), "Detener", "", true);
            if d.clicked() {
                self.motor.llamar("cancelar", json!({}));
            }
            ocupado = ocupado.union(d.rect);
            ui.add_space(8.0);
            ocupado = ocupado.union(ui.add(egui::ProgressBar::new(self.editor.avance).desired_width(80.0).desired_height(10.0).corner_radius(0)).rect);
        } else {
            let b = ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::SimularLleno), "Simular", "F5", true);
            if b.clicked() {
                self.simular();
            }
            ocupado = ocupado.union(b.rect);
        }
        ui.add_space(4.0);
        let r = ui.add(egui::Button::new(egui::RichText::new(self.editor.resumen_solver()).font(tema::fuente_mono(12.0)).color(pal.texto)).frame(false)).on_hover_text("Solver e intervalo de simulación");
        egui::Popup::menu(&r).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_min_width(260.0);
            self.editor.panel_solver_rapido(ui);
        });
        ocupado = ocupado.union(r.rect);
        ui.add_space(6.0);
        let c = ui.max_rect().center().y;
        let caja = Rect::from_min_max(Pos2::new(ocupado.left() - 10.0, c - 15.0), Pos2::new(ocupado.right() + 1.0, c + 15.0));
        ui.painter().set(fondo, egui::epaint::RectShape::new(caja, 0, pal.panel, egui::Stroke::new(1.0_f32, pal.borde), egui::StrokeKind::Inside));
        ui.spacing_mut().item_spacing.x = espacio;
    }

    /// Lanza la simulación del diagrama de Bloques desde cualquier módulo.
    fn simular(&mut self) {
        if self.editor.id_sim.is_none() && self.motor.listo {
            self.editor.ejecutar(&mut self.motor);
            self.modulo = Modulo::Bloques;
        }
    }

    /// Punto de 8 px y texto del estado del motor; su menú permite reiniciarlo.
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
        let g = ui.painter().layout_no_wrap(texto.to_string(), egui::FontId::proportional(12.0), pal.texto);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 14.0, 24.0), Sense::click());
        ui.painter().circle_filled(Pos2::new(r.left() + 4.0, r.center().y), 4.0, nivel.color());
        ui.painter().galley(Pos2::new(r.left() + 14.0, r.center().y - g.size().y / 2.0), g, pal.texto);
        egui::Popup::menu(&resp).show(|ui| {
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

    /// Ruta «proyecto / módulo / documento» de la barra de estado.
    fn ruta(&self) -> String {
        let proyecto = self.carpeta.as_ref().and_then(|c| c.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Proyecto sin guardar".into());
        let herr = |p: &crate::herramientas::Pagina| p.seleccion().map(|id| crate::herramientas::presentacion(id, id).1);
        let detalle = match self.modulo {
            Modulo::Potencia => herr(&self.potencia),
            Modulo::Fotovoltaica => herr(&self.fotovoltaica),
            Modulo::Estadistica => herr(&self.estadistica),
            Modulo::Aprendizaje => herr(&self.aprendizaje),
            Modulo::Mantenimiento => Some(self.mantenimiento.nombre_pestana().to_string()),
            Modulo::Bloques => {
                let nombre = if self.editor.diagrama.nombre.is_empty() { "Diagrama sin nombre" } else { self.editor.diagrama.nombre.as_str() };
                Some(if self.editor.sucio() { format!("{nombre} (cambios sin guardar)") } else { nombre.to_string() })
            }
            _ => None,
        };
        let mut partes = vec![proyecto, tema::nombre_modulo(self.modulo).to_string()];
        partes.extend(detalle);
        partes.join(" / ")
    }

    /// Barra de estado negra de 28 px: ruta del proyecto a la izquierda; selección o aviso, unidades,
    /// versiones del motor y tema a la derecha.
    fn barra_estado(&mut self, ctx: &egui::Context) {
        let blanco = egui::Color32::WHITE;
        let borde = if self.modo == tema::Modo::Oscuro { tema::p().borde } else { tema::NEGRO };
        egui::TopBottomPanel::bottom("pie")
            .exact_height(28.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(tema::NEGRO).inner_margin(egui::Margin::symmetric(16, 0)).stroke(egui::Stroke::new(1.0_f32, borde)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let f = egui::FontId::proportional(13.0);
                    ui.spacing_mut().item_spacing.x = 24.0;
                    let ruta = self.ruta();
                    // Hasta la primera simulación, las versiones salen de la última validación guardada
                    let versiones = if self.editor.tiene_procedencia() { self.editor.versiones() } else { crate::bloques::versiones_de(&self.validacion.procedencia) };
                    let ancho = ui.available_width();
                    ui.add(egui::Label::new(egui::RichText::new(ruta).font(f.clone()).color(blanco)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(format!("Tema: {}", self.modo.nombre())).font(f.clone()).color(blanco));
                        if !versiones.is_empty() {
                            ui.label(egui::RichText::new(versiones).font(f.clone()).color(blanco));
                        }
                        ui.label(egui::RichText::new("Unidades SI").font(f.clone()).color(blanco));
                        let seleccion = if !self.aviso.is_empty() {
                            recortar(&self.aviso, ((ancho * 0.45) / 7.5) as usize)
                        } else if self.modulo == Modulo::Bloques {
                            format!("{} bloques visibles", self.editor.bloques_visibles())
                        } else {
                            "Sin selección".to_string()
                        };
                        ui.label(egui::RichText::new(&seleccion).font(f).color(blanco)).on_hover_text(&self.aviso);
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
        self.limpiar_informes();
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

    /// Menú de módulos (Archivo y los módulos): el activo en peso medio con subrayado negro de 2 px.
    fn menu_modulos(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        egui::TopBottomPanel::top("menu_modulos")
            .exact_height(35.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(pal.campo).inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 0 }))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment([Pos2::new(r.left() - 12.0, r.bottom()), Pos2::new(r.right() + 12.0, r.bottom())], egui::Stroke::new(1.0_f32, pal.linea));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let archivo = self.pestana_menu(ui, "Archivo", false);
                    egui::Popup::menu(&archivo).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| self.menu_archivo(ui));
                    for (m, nombre) in MENU_MODULOS {
                        if self.pestana_menu(ui, nombre, self.modulo == m).clicked() {
                            self.modulo = m;
                        }
                    }
                });
            });
    }

    /// Una entrada del menú de módulos: 34 px de alto, texto de 14 px.
    fn pestana_menu(&self, ui: &mut egui::Ui, nombre: &str, activo: bool) -> egui::Response {
        let pal = tema::p();
        let fuente = if activo { egui::FontId::new(14.0, tema::semi()) } else { egui::FontId::proportional(14.0) };
        let g = ui.painter().layout_no_wrap(nombre.to_string(), fuente, pal.texto);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 28.0, 34.0), Sense::click());
        if resp.hovered() && !activo {
            ui.painter().rect_filled(r, 0.0, pal.realce);
        }
        ui.painter().galley(Pos2::new(r.left() + 14.0, r.center().y - g.size().y / 2.0), g, pal.texto);
        if activo {
            ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.bottom() - 2.0), r.right_bottom()), 0.0, pal.acento);
        }
        resp
    }

    /// Menú Archivo: proyecto, diagrama, tema y motor.
    fn menu_archivo(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(240.0);
        if ui.button("Nuevo proyecto…").clicked() {
            self.confirmar_nuevo = true;
            ui.close();
        }
        if ui.button("Abrir proyecto…").clicked() {
            self.abrir_proyecto();
            ui.close();
        }
        if ui.button("Guardar proyecto").clicked() {
            self.guardar_proyecto();
            ui.close();
        }
        ui.separator();
        ui::titulo_seccion(ui, "Diagrama");
        ui.add(egui::TextEdit::singleline(&mut self.editor.diagrama.nombre).hint_text("Nombre del diagrama"));
        ui.menu_button("Ejemplos", |ui| {
            for (k, n) in self.editor.ejemplos.clone() {
                if ui.button(n).clicked() {
                    self.editor.pedir_ejemplo(&mut self.motor, &k);
                    self.modulo = Modulo::Bloques;
                    ui.close();
                }
            }
        });
        ui.separator();
        self.control_zoom(ui);
        ui.separator();
        ui::titulo_seccion(ui, "Tema");
        for m in [tema::Modo::Claro, tema::Modo::Oscuro] {
            if ui.radio(self.modo == m, m.nombre()).clicked() {
                self.cambiar_modo(ui.ctx(), m);
            }
        }
        ui.separator();
        if ui.button("Referencia").clicked() {
            self.modulo = Modulo::Referencia;
            ui.close();
        }
        if ui.add_enabled(self.motor.listo, egui::Button::new("Reiniciar el motor")).clicked() {
            self.reiniciar_motor();
            ui.close();
        }
    }

    /// Control del tamaño de visualización: reducir, porcentaje, ampliar y restablecer.
    fn control_zoom(&mut self, ui: &mut egui::Ui) {
        ui::titulo_seccion(ui, "Tamaño de visualización");
        let mut z = ui.ctx().zoom_factor();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui.add_enabled(z > ZOOM_MIN + 1e-3, egui::Button::new("−")).on_hover_text("Reducir · Ctrl −").clicked() {
                z -= ZOOM_PASO;
            }
            ui.add(egui::Slider::new(&mut z, ZOOM_MIN..=ZOOM_MAX).step_by(0.05).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)).custom_parser(|t| t.trim().trim_end_matches('%').trim().parse::<f64>().ok().map(|v| v / 100.0)));
            if ui.add_enabled(z < ZOOM_MAX - 1e-3, egui::Button::new("+")).on_hover_text("Ampliar · Ctrl +").clicked() {
                z += ZOOM_PASO;
            }
            if ui.button("100 %").on_hover_text("Restablecer · Ctrl 0").clicked() {
                z = 1.0;
            }
        });
        let z = (z / 0.05).round() * 0.05;
        let z = z.clamp(ZOOM_MIN, ZOOM_MAX);
        if (z - ui.ctx().zoom_factor()).abs() > 1e-3 {
            ui.ctx().set_zoom_factor(z);
            self.zoom = z;
        }
    }

    /// Cinta blanca de 94 px con los grupos del módulo activo.
    fn cinta(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        egui::TopBottomPanel::top("cinta")
            .exact_height(ui::ALTO_CINTA)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin { left: 0, right: 0, top: 8, bottom: 0 }))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment([Pos2::new(r.left(), r.bottom()), Pos2::new(r.right(), r.bottom())], egui::Stroke::new(1.0_f32, pal.linea));
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let herr = self.herramientas.clone();
                    match self.modulo {
                        Modulo::Potencia => self.potencia.cinta(ui, &herr),
                        Modulo::Fotovoltaica => self.fotovoltaica.cinta(ui, &herr),
                        Modulo::Estadistica => self.estadistica.cinta(ui, &herr),
                        Modulo::Aprendizaje => self.aprendizaje.cinta(ui, &herr),
                        Modulo::Mantenimiento => self.mantenimiento.cinta(ui, &herr),
                        Modulo::Bloques => self.editor.cinta(ui, &mut self.motor),
                        Modulo::Calculo => self.calculo.cinta(ui, &mut self.motor),
                        Modulo::Validacion => self.validacion.cinta(ui, &mut self.motor),
                        Modulo::Inicio | Modulo::Referencia => {
                            ui::grupo_cinta(ui, "Proyecto", |ui| {
                                if ui::boton_cinta(ui, Icono::NuevoDocumento, "Nuevo proyecto", false).clicked() {
                                    self.confirmar_nuevo = true;
                                }
                                if ui::boton_cinta(ui, Icono::Abrir, "Abrir proyecto", false).clicked() {
                                    self.abrir_proyecto();
                                }
                                if ui::boton_cinta(ui, Icono::Guardar, "Guardar proyecto", false).clicked() {
                                    self.guardar_proyecto();
                                }
                            });
                        }
                    }
                });
            });
    }

    /// Panel izquierdo de 280 px «Esquema del proyecto | Opciones» con el árbol del proyecto; se
    /// pliega con «« y, por debajo de 1100 px de ancho, empieza plegado.
    fn esquema(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        let estrecha = ctx.content_rect().width() < ui::ANCHO_APILAR;
        let plegado = self.esquema_plegado.unwrap_or(estrecha);
        if plegado {
            egui::SidePanel::left("esquema_plegado").exact_width(32.0).resizable(false).frame(egui::Frame::new().fill(pal.panel).stroke(egui::Stroke::new(1.0_f32, pal.linea))).show(ctx, |ui| {
                ui.add_space(8.0);
                if ui::boton_icono(ui, Icono::Desplegar, 14.0, "Mostrar el esquema del proyecto").clicked() {
                    self.esquema_plegado = Some(false);
                }
            });
            return;
        }
        egui::SidePanel::left("esquema").default_width(280.0).width_range(200.0..=520.0).resizable(true).frame(egui::Frame::new().fill(pal.panel).stroke(egui::Stroke::new(1.0_f32, pal.linea))).show(ctx, |ui| {
            egui::Frame::new().inner_margin(egui::Margin { left: 10, right: 6, top: 6, bottom: 0 }).show(ui, |ui| {
                let mut pestana = self.pestana_esquema;
                ui::barra_pestanas(ui, &["Esquema del proyecto", "Opciones"], &mut pestana, |ui| {
                    if ui::boton_icono(ui, Icono::Plegar, 14.0, "Plegar el esquema").clicked() {
                        self.esquema_plegado = Some(true);
                    }
                });
                self.pestana_esquema = pestana;
            });
            tema::separador(ui);
            if self.pestana_esquema == 1 {
                egui::Frame::new().inner_margin(16).show(ui, |ui| {
                    self.control_zoom(ui);
                    ui.separator();
                    ui::titulo_seccion(ui, "Tema");
                    for m in [tema::Modo::Claro, tema::Modo::Oscuro] {
                        if ui.radio(self.modo == m, m.nombre()).clicked() {
                            self.cambiar_modo(ui.ctx(), m);
                        }
                    }
                });
                return;
            }
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 12, bottom: 8 }).show(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.filtro_esquema).hint_text("Buscar objetos…").desired_width(f32::INFINITY).margin(egui::Margin::symmetric(8, 7)));
            });
            egui::ScrollArea::vertical().id_salt("arbol_proyecto").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                self.arbol_proyecto(ui);
            });
        });
    }

    /// Árbol del proyecto: diagramas, scripts, datos, análisis, plan de mantenimiento y validación.
    fn arbol_proyecto(&mut self, ui: &mut egui::Ui) {
        use ui::EstadoNodo as E;
        let filtro = self.filtro_esquema.to_lowercase();
        let pasa = |t: &str| filtro.is_empty() || t.to_lowercase().contains(&filtro);
        let proyecto = self.carpeta.as_ref().and_then(|c| c.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Proyecto sin guardar".into());
        let mut ir: Option<Modulo> = None;
        let mut herramienta: Option<(Modulo, String)> = None;
        let nodo = |ui: &mut egui::Ui, abiertos: &mut std::collections::HashSet<String>, clave: &str, nivel: usize, estado: E, icono: Option<Icono>, texto: &str, sufijo: &str, activo: bool, hijos: bool| -> (egui::Response, bool) {
            let abierto = !abiertos.contains(clave);
            let (r, conmutado) = ui::nodo_arbol(ui, nivel, hijos.then_some(abierto), estado, icono, texto, sufijo, activo);
            if conmutado {
                if abierto {
                    abiertos.insert(clave.to_string());
                } else {
                    abiertos.remove(clave);
                }
            }
            (r, abierto && hijos)
        };
        let mut cerrados = std::mem::take(&mut self.esquema_cerrados);
        let (_, abierto) = nodo(ui, &mut cerrados, "proyecto", 0, E::Ninguno, Some(Icono::Carpeta), &proyecto, "", false, true);
        if abierto {
            // Diagramas
            let bloques = self.editor.diagrama.sistema.bloques.len();
            let estado_diagrama = if self.editor.ultima.is_none() {
                E::SinEjecutar
            } else if self.editor.avisos.is_empty() && self.editor.error.is_none() {
                E::Listo
            } else {
                E::Pendiente
            };
            let (_, a) = nodo(ui, &mut cerrados, "diagramas", 1, estado_diagrama, None, "Diagramas", "", false, true);
            let nombre = if self.editor.diagrama.nombre.is_empty() { "Diagrama sin nombre".to_string() } else { self.editor.diagrama.nombre.clone() };
            if a && pasa(&nombre) {
                let (r, _) = nodo(ui, &mut cerrados, "", 2, estado_diagrama, None, &nombre, &format!("{bloques} bloques"), self.modulo == Modulo::Bloques, false);
                if r.clicked() {
                    ir = Some(Modulo::Bloques);
                }
            }
            // Scripts
            let guiones: Vec<String> = self.calculo.guiones.iter().filter(|g| !g.motor).map(|g| g.nombre.clone()).collect();
            if !guiones.is_empty() {
                let (_, a) = nodo(ui, &mut cerrados, "scripts", 1, E::Listo, None, "Scripts", "", false, true);
                if a {
                    for g in guiones.iter().filter(|g| pasa(g)) {
                        let (r, _) = nodo(ui, &mut cerrados, "", 2, E::Listo, None, g, "", self.modulo == Modulo::Calculo, false);
                        if r.clicked() {
                            ir = Some(Modulo::Calculo);
                        }
                    }
                }
            }
            // Datos del espacio de trabajo
            if !self.compartido.variables.is_empty() {
                let (_, a) = nodo(ui, &mut cerrados, "datos", 1, E::Listo, None, "Datos", "", false, true);
                if a {
                    for v in self.compartido.variables.iter().filter(|v| pasa(&v.nombre)) {
                        let icono = if v.columnas.is_empty() { Icono::Variable } else { Icono::Tabla };
                        let (r, _) = nodo(ui, &mut cerrados, "", 2, E::Ninguno, Some(icono), &v.nombre, &v.forma, false, false);
                        if r.on_hover_text(format!("{} {}", v.tipo, v.forma)).clicked() {
                            ir = Some(Modulo::Calculo);
                        }
                    }
                }
            }
            // Análisis calculados en las páginas de herramientas
            let paginas = [
                (Modulo::Potencia, &self.potencia),
                (Modulo::Fotovoltaica, &self.fotovoltaica),
                (Modulo::Estadistica, &self.estadistica),
                (Modulo::Aprendizaje, &self.aprendizaje),
                (Modulo::Mantenimiento, &self.mantenimiento.analisis),
            ];
            let analisis: Vec<(Modulo, String, String, E, String)> = paginas
                .iter()
                .flat_map(|(m, p)| p.resultados_esquema(&self.herramientas).into_iter().map(move |(id, corto, e, s)| (*m, id, format!("{} · {}", tema::nombre_modulo(*m), corto), e, s)))
                .collect();
            if !analisis.is_empty() {
                let estado = if analisis.iter().any(|a| a.3 == E::Pendiente) { E::Pendiente } else { E::Listo };
                let (_, a) = nodo(ui, &mut cerrados, "analisis", 1, estado, None, "Análisis", "", false, true);
                if a {
                    for (m, id, texto, e, s) in analisis.iter().filter(|a| pasa(&a.2)) {
                        let activo = self.modulo == *m && paginas.iter().any(|(pm, p)| pm == m && p.seleccion() == Some(id.as_str()));
                        let (r, _) = nodo(ui, &mut cerrados, "", 2, *e, None, texto, s, activo, false);
                        if r.clicked() {
                            herramienta = Some((*m, id.clone()));
                        }
                    }
                }
            }
            // Plan de mantenimiento
            if let Some(n) = self.mantenimiento.pendientes() {
                let (r, _) = nodo(ui, &mut cerrados, "plan", 1, if n > 0 { E::Pendiente } else { E::Listo }, None, "Plan de mantenimiento", &format!("{n} pendientes"), self.modulo == Modulo::Mantenimiento, false);
                if r.clicked() {
                    ir = Some(Modulo::Mantenimiento);
                }
            }
            // Validación
            let fases = self.validacion.resumen_fases();
            let ejecutadas: Vec<&crate::validacion::ResumenFase> = fases.iter().filter(|f| f.nivel.is_some()).collect();
            let estado = if ejecutadas.is_empty() {
                E::SinEjecutar
            } else if ejecutadas.iter().all(|f| f.nivel == Some(tema::Nivel::Ok)) {
                E::Listo
            } else {
                E::Pendiente
            };
            let (r, _) = nodo(ui, &mut cerrados, "validacion", 1, estado, None, "Validación", &format!("{} fases", fases.len()), self.modulo == Modulo::Validacion, false);
            if r.clicked() {
                ir = Some(Modulo::Validacion);
            }
        }
        self.esquema_cerrados = cerrados;
        if let Some((m, id)) = herramienta {
            self.modulo = m;
            match m {
                Modulo::Potencia => self.potencia.seleccionar(&id),
                Modulo::Fotovoltaica => self.fotovoltaica.seleccionar(&id),
                Modulo::Estadistica => self.estadistica.seleccionar(&id),
                Modulo::Aprendizaje => self.aprendizaje.seleccionar(&id),
                _ => {
                    self.mantenimiento.fijar_pestana("analisis");
                    self.mantenimiento.analisis.seleccionar(&id);
                }
            }
        } else if let Some(m) = ir {
            self.modulo = m;
        }
    }

    /// Barra superior de la portada: blanca, de 40 px, con la marca y el estado del motor.
    fn barra_superior_portada(&mut self, ctx: &egui::Context) {
        let pal = tema::p();
        egui::TopBottomPanel::top("superior_portada")
            .exact_height(40.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(14, 0)))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment([Pos2::new(r.left() - 14.0, r.bottom()), Pos2::new(r.right() + 14.0, r.bottom())], egui::Stroke::new(1.0_f32, pal.linea));
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                    tema::logo(ui.painter(), r);
                    ui.label(egui::RichText::new("Heliora").family(tema::semi()).size(14.0).color(pal.texto));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.estado_motor(ui));
                });
            });
    }

    /// Barra lateral negra de 140 px de la portada: Inicio, Nuevo, Abrir y Referencia arriba; Ajustes
    /// y Cuenta abajo. El activo lleva una barra blanca de 3 px a la izquierda.
    fn barra_lateral_portada(&mut self, ctx: &egui::Context) {
        let borde = if self.modo == tema::Modo::Oscuro { tema::p().borde } else { tema::NEGRO };
        egui::SidePanel::left("lateral_portada")
            .exact_width(140.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(tema::NEGRO).inner_margin(egui::Margin { left: 0, right: 0, top: 8, bottom: 8 }).stroke(egui::Stroke::new(1.0_f32, borde)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if boton_lateral(ui, Icono::Inicio, "Inicio", self.modulo == Modulo::Inicio).clicked() {
                    self.modulo = Modulo::Inicio;
                }
                if boton_lateral(ui, Icono::Nuevo, "Nuevo", false).on_hover_text("Nuevo proyecto").clicked() {
                    self.confirmar_nuevo = true;
                }
                if boton_lateral(ui, Icono::Abrir, "Abrir", false).on_hover_text("Abrir un proyecto").clicked() {
                    self.abrir_proyecto();
                }
                if boton_lateral(ui, Icono::Referencia, "Referencia", false).on_hover_text("Referencia de bloques y funciones").clicked() {
                    self.modulo = Modulo::Referencia;
                }
                ui.add_space((ui.available_height() - 2.0 * 80.0).max(0.0));
                let ajustes = boton_lateral(ui, Icono::Ajustes, "Ajustes", false);
                egui::Popup::menu(&ajustes).show(|ui| {
                    self.control_zoom(ui);
                    ui.separator();
                    ui::titulo_seccion(ui, "Tema");
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
                boton_lateral(ui, Icono::Cuenta, "Cuenta", false).on_hover_text("Heliora no usa cuentas: cada proyecto es una carpeta local");
            });
    }

    /// Portada: cabecera del proyecto, tres tarjetas (continuar, planta y validación), próximas
    /// órdenes y módulos.
    fn pagina_inicio(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().fill(tema::p().fondo).inner_margin(egui::Margin { left: 32, right: 32, top: 26, bottom: 26 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 22.0;
                self.cabecera_inicio(ui);
                let ancho = ui.available_width();
                let apilar = ancho < 900.0;
                let spans: &[usize] = &[4, 4, 4];
                let alto = 310.0;
                if apilar {
                    for i in 0..3 {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::hover());
                        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r));
                        self.tarjeta_inicio(&mut c, i);
                    }
                } else {
                    let (fila, _) = ui.allocate_exact_size(Vec2::new(ancho, alto), Sense::hover());
                    for (i, (x, w)) in ui::celdas(ancho, 12, 20.0, spans).into_iter().enumerate() {
                        let r = Rect::from_min_size(Pos2::new(fila.left() + x, fila.top()), Vec2::new(w, alto));
                        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r));
                        self.tarjeta_inicio(&mut c, i);
                    }
                }
                let hueco = 32.0;
                let ancho_ordenes = if apilar { ancho } else { (ancho - hueco) * 2.0 / 3.0 };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = hueco;
                    ui.vertical(|ui| {
                        ui.set_width(ancho_ordenes);
                        self.lista_ordenes(ui);
                    });
                    if !apilar {
                        ui.vertical(|ui| {
                            ui.set_width(ancho - ancho_ordenes - hueco);
                            self.rejilla_modulos(ui);
                        });
                    }
                });
                if apilar {
                    self.rejilla_modulos(ui);
                }
            });
        });
    }

    /// Una de las tres tarjetas de la portada.
    fn tarjeta_inicio(&mut self, ui: &mut egui::Ui, i: usize) {
        match i {
            0 => self.tarjeta_continuar(ui),
            1 => self.tarjeta_planta(ui),
            _ => self.tarjeta_validacion(ui),
        }
    }

    fn cabecera_inicio(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        let proyecto = self.carpeta.as_ref().and_then(|c| c.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Proyecto sin guardar".into());
        let diagramas = usize::from(!self.editor.diagrama.sistema.bloques.is_empty());
        let scripts = self.calculo.guiones.iter().filter(|g| !g.motor).count();
        let mut partes = vec![format!("{diagramas} {}", if diagramas == 1 { "diagrama" } else { "diagramas" }), format!("{scripts} {}", if scripts == 1 { "script" } else { "scripts" })];
        match self.mantenimiento.pendientes() {
            Some(n) => partes.push(format!("plan de mantenimiento con {n} órdenes pendientes")),
            None => partes.push("sin plan de mantenimiento".into()),
        }
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(egui::RichText::new(proyecto).font(tema::fuente_negrita(28.0)).color(pal.texto));
                ui.label(egui::RichText::new(partes.join(" · ")).size(14.0).color(pal.texto_2));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if ui::boton(ui, ui::EstiloBoton::Secundario, Some(Icono::Nuevo), "Crear carpeta", "", false).on_hover_text("Guarda el proyecto en una carpeta nueva").clicked() {
                    if let Some(c) = rfd::FileDialog::new().set_title("Carpeta nueva para el proyecto").pick_folder() {
                        self.carpeta = Some(c);
                        self.guardar_proyecto();
                    }
                }
                if ui::boton(ui, ui::EstiloBoton::Secundario, Some(Icono::Abrir), "Abrir proyecto", "", false).clicked() {
                    self.abrir_proyecto();
                }
                if ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::NuevoDocumento), "Nuevo proyecto", "", false).clicked() {
                    self.confirmar_nuevo = true;
                }
            });
        });
    }

    /// Proyecto vacío sin carpeta: diagrama, espacio de trabajo, scripts, plan de mantenimiento e
    /// informes nuevos, para que un guardado posterior no lleve nada del proyecto anterior.
    fn nuevo_proyecto(&mut self) {
        self.motor.llamar("limpiar_espacio", json!({}));
        self.compartido.variables.clear();
        self.editor.nuevo_diagrama();
        self.calculo.fijar_guiones(vec![]);
        self.mantenimiento.estado = Value::Null;
        self.mantenimiento.tablero = Value::Null;
        self.mantenimiento.ruta = None;
        self.mantenimiento.iniciar(&mut self.motor);
        self.limpiar_informes();
        self.pkl_pendientes = None;
        self.carpeta = None;
        self.aviso = "Proyecto nuevo sin guardar".into();
    }

    /// Olvida los informes de todas las páginas de análisis.
    fn limpiar_informes(&mut self) {
        for p in [&mut self.potencia, &mut self.fotovoltaica, &mut self.estadistica, &mut self.aprendizaje, &mut self.mantenimiento.analisis] {
            p.limpiar_informes();
        }
    }

    /// Pide confirmación antes de descartar el proyecto abierto.
    fn dialogo_nuevo_proyecto(&mut self, ctx: &egui::Context) {
        if !self.confirmar_nuevo {
            return;
        }
        let mut decision = None;
        let r = egui::Modal::new(egui::Id::new("nuevo_proyecto")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.label(egui::RichText::new("¿Empezar un proyecto nuevo?").size(16.0).family(tema::semi()));
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Se vacían el diagrama, las variables, los scripts y el plan de mantenimiento. Lo que no hayas guardado se pierde.").color(tema::p().texto_2));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if tema::boton_principal(ui, "Empezar uno nuevo").clicked() {
                    decision = Some(true);
                }
                if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Cancelar", "", false).clicked() {
                    decision = Some(false);
                }
            });
        });
        if r.should_close() {
            decision.get_or_insert(false);
        }
        if let Some(si) = decision {
            self.confirmar_nuevo = false;
            if si {
                self.nuevo_proyecto();
            }
        }
    }

    fn tarjeta_continuar(&mut self, ui: &mut egui::Ui) {
        let mut ir_bloques = false;
        let mut ir_calculo = false;
        let hay_diagrama = !self.editor.diagrama.sistema.bloques.is_empty();
        let (titulo, chips) = if hay_diagrama {
            let (con_avisos, estado) = if self.editor.ultima.is_none() {
                (false, "Sin simular".to_string())
            } else if self.editor.avisos.is_empty() && self.editor.error.is_none() {
                (false, "Sin avisos".to_string())
            } else {
                (true, format!("{} avisos", self.editor.avisos.len() + usize::from(self.editor.error.is_some())))
            };
            let partes: Vec<String> = self.editor.resumen_solver().split(" · ").map(|s| s.trim().to_string()).collect();
            let metodo = partes.first().and_then(|m| m.split_whitespace().next()).unwrap_or("").to_string();
            let fin = partes.get(1).and_then(|i| i.split('→').nth(1)).map(|s| s.trim().to_string()).unwrap_or_default();
            let solver = [metodo, fin].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
            let nombre = if self.editor.diagrama.nombre.is_empty() { "Diagrama sin nombre".to_string() } else { self.editor.diagrama.nombre.clone() };
            (nombre, vec![(estado, con_avisos), (format!("{} bloques", self.editor.diagrama.sistema.bloques.len()), false), (solver, false)])
        } else {
            ("Sin diagrama abierto".to_string(), vec![])
        };
        let vista = self.editor.vista_previa();
        let guion = self.calculo.guiones.iter().find(|g| !g.motor).map(|g| g.nombre.clone());
        let debajo = |ui: &mut egui::Ui| {
            ui.label(egui::RichText::new("Continuar donde lo dejaste · Bloques").size(13.0).color(tema::p().texto_2));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (t, oscuro) in &chips {
                    ui::chip(ui, t, *oscuro);
                }
            });
        };
        tarjeta_portada(ui, &titulo, |_| {}, debajo, |ui| {
            if !hay_diagrama {
                if ui::estado_vacio(ui, "No hay ningún diagrama abierto.", "Abrir la planta completa") {
                    self.editor.pedir_ejemplo(&mut self.motor, "planta_promediada");
                    ir_bloques = true;
                }
                return;
            }
            let r = ui.max_rect();
            let pie = 48.0;
            let zona = Rect::from_min_max(r.min + Vec2::new(14.0, 10.0), Pos2::new(r.right() - 14.0, r.bottom() - pie));
            dibujar_vista_previa(ui.painter_at(zona), zona, &vista);
            let mut fila = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(r.left() + 14.0, r.bottom() - pie), Pos2::new(r.right() - 14.0, r.bottom() - 14.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
            fila.spacing_mut().item_spacing.x = 8.0;
            if ui::boton(&mut fila, ui::EstiloBoton::Principal, Some(Icono::Flecha), "Abrir", "", false).clicked() {
                ir_bloques = true;
            }
            if let Some(g) = &guion {
                if ui::boton(&mut fila, ui::EstiloBoton::Secundario, Some(Icono::Documento), g, "", false).clicked() {
                    ir_calculo = true;
                }
            }
        });
        if ir_bloques {
            self.modulo = Modulo::Bloques;
        }
        if ir_calculo {
            self.modulo = Modulo::Calculo;
        }
    }

    fn tarjeta_planta(&mut self, ui: &mut egui::Ui) {
        let ir_analisis = std::cell::Cell::new(false);
        let informe = self.mantenimiento.analisis.informe("mt_kpi");
        let kpis: Vec<(String, String, String)> = informe
            .map(|inf| {
                let mut v: Vec<(String, String, String)> = inf.secciones.iter().find_map(|s| if let herramientas::Seccion::Kv(_, it) = s { Some(it.clone()) } else { None }).unwrap_or_default();
                let inversores = inf.secciones.iter().find_map(|s| if let herramientas::Seccion::Tabla(_, _, filas) = s { Some(filas.len()) } else { None });
                if let Some(n) = inversores {
                    v.push(("Inversores".into(), n.to_string(), String::new()));
                }
                v
            })
            .unwrap_or_default();
        let tabla = self.mantenimiento.analisis.valor_calculado("mt_kpi", "tabla").unwrap_or("").to_string();
        let subtitulo = if kpis.is_empty() || tabla.is_empty() { "Indicadores de la planta a partir de una tabla SCADA.".to_string() } else { format!("Indicadores calculados a partir de la tabla {tabla}.") };
        tarjeta_portada(
            ui,
            "Planta · datos SCADA",
            |ui| {
                if enlace(ui, "Análisis").clicked() {
                    ir_analisis.set(true);
                }
            },
            |ui| {
                ui.label(egui::RichText::new(&subtitulo).size(13.0).color(tema::p().texto_2));
            },
            |ui| {
                if kpis.is_empty() {
                    let hay_scada = self.compartido.variables.iter().any(|v| v.nombre == "scada" && !v.columnas.is_empty());
                    let (texto, accion, herramienta) = if hay_scada {
                        ("Aún no hay KPIs de planta para la tabla «scada».", "Calcular KPIs", "mt_kpi")
                    } else {
                        ("Aún no hay KPIs de planta.", "Importar o generar datos SCADA", "mt_importar")
                    };
                    if ui::estado_vacio(ui, texto, accion) {
                        self.mantenimiento.analisis.seleccionar(herramienta);
                        ir_analisis.set(true);
                    }
                    return;
                }
                let area = ui.max_rect().shrink(14.0);
                let hueco = 8.0;
                let w = (area.width() - 2.0 * hueco) / 3.0;
                let h = 84.0_f32.min((area.height() - hueco) / 2.0);
                for (i, (e, v, u)) in kpis.iter().take(6).enumerate() {
                    let r = Rect::from_min_size(Pos2::new(area.left() + (i % 3) as f32 * (w + hueco), area.top() + (i / 3) as f32 * (h + hueco)), Vec2::new(w, h));
                    let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(r));
                    let (u, _) = herramientas::unidad_o_nota(u);
                    ui::ficha_kpi(&mut hijo, e, v, u, "", None);
                }
            },
        );
        if ir_analisis.get() {
            self.mantenimiento.fijar_pestana("analisis");
            self.modulo = Modulo::Mantenimiento;
        }
    }

    fn tarjeta_validacion(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        let resumen = self.validacion.resumen_fases();
        let cuando = self.validacion.cuando.clone();
        let ocupada = self.validacion.ocupada();
        let historica = self.validacion.historica();
        let listo = self.motor.listo;
        let mut ir_validacion = false;
        let mut ejecutar = false;
        let ejecutadas = resumen.iter().filter(|r| r.nivel.is_some()).count();
        let falla = resumen.iter().filter(|r| matches!(r.nivel, Some(tema::Nivel::Falla))).count();
        let bloqueadas = resumen.iter().filter(|r| matches!(r.nivel, Some(tema::Nivel::Alerta))).count();
        let (estado, texto) = if ejecutadas == 0 {
            (ui::EstadoNodo::SinEjecutar, "Sin ejecutar".to_string())
        } else if let Some(m) = historica {
            (ui::EstadoNodo::Pendiente, format!("Histórica · {m}"))
        } else if falla > 0 {
            (ui::EstadoNodo::Pendiente, format!("{falla} {} con fallos", if falla == 1 { "fase" } else { "fases" }))
        } else if bloqueadas > 0 {
            (ui::EstadoNodo::Pendiente, "Hay fases bloqueadas".to_string())
        } else if ejecutadas == 4 {
            (ui::EstadoNodo::Listo, "Las cuatro fases superadas".to_string())
        } else {
            (ui::EstadoNodo::Listo, format!("{ejecutadas} de 4 fases superadas"))
        };
        let fecha = fecha_validacion(&cuando);
        tarjeta_portada(
            ui,
            "Validación",
            |ui| {
                ui.label(egui::RichText::new(&fecha).font(tema::fuente_mono(12.0)).color(pal.texto_2));
            },
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                    if estado == ui::EstadoNodo::Listo {
                        ui.painter().circle_stroke(r.center(), 8.0, egui::Stroke::new(1.5_f32, pal.ok));
                        crate::iconos::dibujar(ui.painter(), r.shrink(3.0), Icono::Correcto, pal.ok);
                    } else {
                        ui::punto_estado(ui.painter(), r.center(), estado);
                    }
                    ui.label(egui::RichText::new(&texto).size(13.0).color(pal.texto));
                });
            },
            |ui| {
                let r = ui.max_rect();
                let lista = Rect::from_min_max(Pos2::new(r.left() + 14.0, r.top()), Pos2::new(r.right() - 14.0, r.top() + 4.0 + 34.0 * resumen.len() as f32));
                ui.painter().rect(lista, 0.0, pal.panel, egui::Stroke::new(1.0_f32, pal.linea), egui::StrokeKind::Inside);
                for (i, f) in resumen.iter().enumerate() {
                    let fila = Rect::from_min_size(Pos2::new(lista.left() + 16.0, lista.top() + 4.0 + 34.0 * i as f32), Vec2::new(lista.width() - 32.0, 34.0));
                    if i + 1 < resumen.len() {
                        ui.painter().line_segment([fila.left_bottom(), fila.right_bottom()], egui::Stroke::new(1.0_f32, pal.linea));
                    }
                    let e = match (historica, f.nivel) {
                        (Some(_), Some(_)) | (_, Some(tema::Nivel::Falla)) | (_, Some(tema::Nivel::Alerta)) => ui::EstadoNodo::Pendiente,
                        (_, Some(tema::Nivel::Ok)) => ui::EstadoNodo::Listo,
                        _ => ui::EstadoNodo::SinEjecutar,
                    };
                    ui::punto_estado(ui.painter(), Pos2::new(fila.left() + 5.0, fila.center().y), e);
                    ui.painter().text(Pos2::new(fila.left() + 20.0, fila.center().y), egui::Align2::LEFT_CENTER, f.titulo.clone(), egui::FontId::proportional(13.0), pal.texto);
                    let cuenta = match (f.bien, f.total) {
                        (Some(b), Some(t)) => format!("{b}/{t}"),
                        (None, Some(t)) => format!("—/{t}"),
                        _ => String::new(),
                    };
                    ui.painter().text(Pos2::new(fila.right(), fila.center().y), egui::Align2::RIGHT_CENTER, cuenta, tema::fuente_mono(12.0), pal.texto);
                }
                let mut pie = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(r.left() + 14.0, r.bottom() - 48.0), Pos2::new(r.right() - 14.0, r.bottom() - 14.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                pie.spacing_mut().item_spacing.x = 14.0;
                let texto_boton = if historica.is_some() { "Volver a validar" } else { "Ejecutar" };
                if pie.add_enabled_ui(!ocupada && listo, |ui| ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::SimularLleno), texto_boton, "", false)).inner.clicked() {
                    ejecutar = true;
                }
                if enlace(&mut pie, "Ver el error de cada prueba").clicked() {
                    ir_validacion = true;
                }
            },
        );
        if ejecutar {
            self.validacion.ejecutar(&mut self.motor, 4);
        }
        if ir_validacion {
            self.modulo = Modulo::Validacion;
        }
    }

    /// Próximas órdenes: filtros en chips, orden y filas con borde, etiquetas y fecha.
    fn lista_ordenes(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        let hoy = self.mantenimiento.hoy.clone();
        let mut ir_plan = false;
        let mut ir_tablero = false;
        let mut abrir: Option<String> = None;
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Próximas órdenes").size(22.0).family(tema::semi()).color(pal.texto));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if enlace(ui, "Ver todas").clicked() {
                    ir_plan = true;
                }
            });
        });
        ui.add_space(14.0);
        // Órdenes abiertas del tipo elegido: de ellas salen a la vez las cuentas de los chips y la lista
        let limite = sumar_dias(&hoy, 30);
        let en_estado = |o: &Value, clave: &str| {
            let f = o["fecha"].as_str().unwrap_or("");
            match clave {
                "vencidas" => f < hoy.as_str(),
                "30" => f >= hoy.as_str() && limite.as_deref().is_none_or(|l| f <= l),
                _ => true,
            }
        };
        let abiertas: Vec<Value> = self.mantenimiento.estado["ordenes"]
            .as_array()
            .map(|a| a.iter().filter(|o| o["estado"].as_str() != Some("cerrada")).filter(|o| self.tipo_inicio.as_deref().is_none_or(|t| tipo_orden(o["nombre"].as_str().unwrap_or("")) == t)).cloned().collect())
            .unwrap_or_default();
        let hay_plan = !self.mantenimiento.estado.is_null();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (clave, texto) in [("vencidas", "vencidas"), ("30", "en 30 días"), ("pendientes", "pendientes")] {
                if hay_plan {
                    let n = abiertas.iter().filter(|o| en_estado(o, clave)).count();
                    if ui::chip(ui, &format!("{n} {texto}"), self.filtro_inicio == clave).clicked() {
                        self.filtro_inicio = clave.to_string();
                    }
                }
            }
            for tipo in TIPOS_ORDEN {
                let activo = self.tipo_inicio.as_deref() == Some(tipo);
                if ui::chip(ui, tipo, activo).clicked() {
                    self.tipo_inicio = if activo { None } else { Some(tipo.to_string()) };
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let texto = if self.orden_inicio_equipo { "Orden: equipo" } else { "Orden: fecha" };
                let r = ui::boton_texto(ui, None, &format!("{texto} ▾"));
                egui::Popup::menu(&r).show(|ui| {
                    if ui.radio(!self.orden_inicio_equipo, "Fecha").clicked() {
                        self.orden_inicio_equipo = false;
                    }
                    if ui.radio(self.orden_inicio_equipo, "Equipo").clicked() {
                        self.orden_inicio_equipo = true;
                    }
                });
            });
        });
        ui.add_space(10.0);
        let mut ordenes: Vec<&Value> = abiertas.iter().filter(|o| en_estado(o, &self.filtro_inicio)).collect();
        if self.orden_inicio_equipo {
            ordenes.sort_by(|a, b| a["equipo"].as_str().cmp(&b["equipo"].as_str()).then(a["fecha"].as_str().cmp(&b["fecha"].as_str())));
        } else {
            ordenes.sort_by(|a, b| a["fecha"].as_str().cmp(&b["fecha"].as_str()).then(a["id"].as_str().cmp(&b["id"].as_str())));
        }
        if ordenes.is_empty() {
            ui.add_space(20.0);
            let texto = if self.mantenimiento.estado.is_null() { "No hay un plan de mantenimiento abierto." } else { "No hay órdenes con este filtro." };
            ui.label(egui::RichText::new(texto).color(pal.texto_2));
            ui.add_space(8.0);
            if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Abrir Mantenimiento · Tablero", "", true).clicked() {
                ir_tablero = true;
            }
        } else {
            let ancho = ui.available_width();
            let columnas = |r: Rect| {
                let x_fecha = r.right() - 12.0 - 64.0 - 12.0 - 110.0;
                let x_etiquetas = x_fecha - 12.0 - 260.0;
                (r.left() + 12.0, r.left() + 64.0, x_etiquetas, x_fecha, x_fecha + 122.0)
            };
            let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, 28.0), Sense::hover());
            let (_, x_tarea, x_etq, x_fecha, _) = columnas(r);
            for (x, t) in [(x_tarea, "Tarea"), (x_etq, "Etiquetas"), (x_fecha, "Fecha")] {
                ui.painter().text(Pos2::new(x, r.center().y), egui::Align2::LEFT_CENTER, t, egui::FontId::new(14.0, tema::semi()), pal.texto);
            }
            for o in ordenes.iter().take(5) {
                ui.add_space(8.0);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho, 40.0), Sense::click());
                let (x_icono, x_tarea, x_etq, x_fecha, x_marca) = columnas(r);
                let p = ui.painter();
                p.rect(r, 0.0, if resp.hovered() { pal.campo } else { pal.panel }, egui::Stroke::new(1.0_f32, pal.borde), egui::StrokeKind::Inside);
                let caja = Rect::from_min_size(Pos2::new(x_icono, r.center().y - 15.0), Vec2::splat(30.0));
                p.rect_filled(caja, 0.0, pal.chip);
                crate::iconos::dibujar(p, Rect::from_center_size(caja.center(), Vec2::splat(16.0)), Icono::Orden, pal.texto);
                let nombre = o["nombre"].as_str().unwrap_or("");
                let id = o["id"].as_str().unwrap_or("");
                let mut job = egui::text::LayoutJob::default();
                job.append(nombre, 0.0, egui::TextFormat::simple(egui::FontId::new(13.0, tema::semi()), pal.texto));
                job.append(id, 6.0, egui::TextFormat::simple(tema::fuente_mono(13.0), pal.texto_2));
                let g = ui.fonts_mut(|f| f.layout_job(job));
                p.with_clip_rect(Rect::from_min_max(Pos2::new(x_tarea, r.top()), Pos2::new(x_etq - 12.0, r.bottom()))).galley(Pos2::new(x_tarea, r.center().y - g.size().y / 2.0), g, pal.texto);
                let mut etq = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(x_etq, r.top()), Pos2::new(x_fecha - 12.0, r.bottom()))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                etq.spacing_mut().item_spacing.x = 6.0;
                if let Some(e) = o["equipo"].as_str().filter(|e| !e.is_empty()) {
                    ui::chip(&mut etq, e, true);
                }
                ui::chip(&mut etq, tipo_orden(nombre), true);
                let fecha = o["fecha"].as_str().unwrap_or("");
                ui.painter().text(Pos2::new(x_fecha, r.center().y), egui::Align2::LEFT_CENTER, fecha_corta(fecha), tema::fuente_mono(13.0), pal.texto);
                let marca = if fecha == hoy { Some("hoy") } else if fecha < hoy.as_str() { Some("vencida") } else { None };
                if let Some(m) = marca {
                    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(x_marca, r.top()), Pos2::new(r.right() - 8.0, r.bottom()))).layout(egui::Layout::left_to_right(egui::Align::Center)));
                    ui::chip(&mut c, m, false);
                }
                if resp.clicked() {
                    abrir = Some(id.to_string());
                }
            }
        }
        if let Some(id) = abrir {
            self.mantenimiento.abrir_orden(&id);
            self.modulo = Modulo::Mantenimiento;
        }
        if ir_plan {
            self.mantenimiento.ver_ordenes_pendientes();
            self.modulo = Modulo::Mantenimiento;
        }
        if ir_tablero {
            self.mantenimiento.fijar_pestana("tablero");
            self.modulo = Modulo::Mantenimiento;
        }
    }

    /// Módulos en una rejilla de 4 × 2 con icono y nombre.
    fn rejilla_modulos(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        let modulos = [
            (Modulo::Bloques, Icono::Bloques),
            (Modulo::Potencia, Icono::Rayo),
            (Modulo::Fotovoltaica, Icono::Sol),
            (Modulo::Calculo, Icono::Terminal),
            (Modulo::Estadistica, Icono::Histograma),
            (Modulo::Aprendizaje, Icono::Red),
            (Modulo::Mantenimiento, Icono::Calendario),
            (Modulo::Validacion, Icono::Validar),
        ];
        ui.label(egui::RichText::new("Módulos").size(22.0).family(tema::semi()).color(pal.texto));
        ui.add_space(20.0);
        let ancho = ui.available_width();
        let hueco = 10.0;
        let w = (ancho - 3.0 * hueco) / 4.0;
        let h = 110.0;
        let (area, _) = ui.allocate_exact_size(Vec2::new(ancho, 2.0 * h + hueco), Sense::hover());
        for (i, (m, icono)) in modulos.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(area.left() + (i % 4) as f32 * (w + hueco), area.top() + (i / 4) as f32 * (h + hueco)), Vec2::new(w, h));
            let resp = ui.interact(r, ui.id().with(("modulo", i)), Sense::click());
            ui.painter().rect(r, 0.0, if resp.hovered() { pal.campo } else { pal.panel }, egui::Stroke::new(1.0_f32, pal.borde), egui::StrokeKind::Inside);
            crate::iconos::dibujar(ui.painter(), Rect::from_center_size(Pos2::new(r.center().x, r.center().y - 14.0), Vec2::splat(26.0)), *icono, pal.texto);
            ui.painter().with_clip_rect(r).text(Pos2::new(r.center().x, r.center().y + 22.0), egui::Align2::CENTER_CENTER, tema::nombre_modulo(*m), egui::FontId::proportional(13.0), pal.texto);
            if resp.clicked() {
                self.modulo = *m;
            }
        }
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
        // La captura se hace a 1440 × 900 píxeles físicos y a un píxel por punto, como las
        // referencias del diseño, sea cual sea la escala del monitor donde se abre la ventana.
        let nativo = ctx.native_pixels_per_point().unwrap_or(1.0);
        if (ctx.zoom_factor() - 1.0 / nativo).abs() > 1e-3 {
            ctx.set_zoom_factor(1.0 / nativo);
        }
        let fisico = ctx.content_rect().size() * ctx.pixels_per_point();
        let a_medida = (fisico - Vec2::new(1440.0, 900.0)).abs().max_elem() < 1.5;
        if !a_medida && self.cuadros % 10 == 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(1440.0, 900.0) / ctx.pixels_per_point()));
        }
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
        if listo && a_medida {
            self.cuadros_listo += 1;
        }
        // 120 cuadros tras cargar; como mucho, unos 45 s en total
        if !self.captura_pedida && !ocupado && (self.cuadros_listo >= 120 || self.cuadros > 2700) {
            self.captura_pedida = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new("ventana".to_string())));
        }
    }
}

/// Recorta un texto a `n` caracteres, con puntos suspensivos si no cabe.
fn recortar(t: &str, n: usize) -> String {
    if t.chars().count() <= n {
        t.to_string()
    } else {
        format!("{}…", t.chars().take(n.saturating_sub(1)).collect::<String>())
    }
}

/// Tipos de orden de los chips de filtro de la portada, en el orden del diseño.
const TIPOS_ORDEN: [&str; 4] = ["Limpieza", "Inspección", "Ensayo", "Mant. anual"];

/// Tipo de una orden de mantenimiento a partir de su nombre (mismo criterio que sus colores).
fn tipo_orden(nombre: &str) -> &'static str {
    let t = nombre.to_lowercase();
    if t.contains("limpieza") {
        "Limpieza"
    } else if t.contains("inspecc") || t.contains("termograf") {
        "Inspección"
    } else if t.contains("ensayo") || t.contains("curva") {
        "Ensayo"
    } else if t.contains("anual") {
        "Mant. anual"
    } else {
        "Otro"
    }
}

/// Fecha AAAA-MM-DD `n` días después de `fecha` (calendario civil proléptico).
fn sumar_dias(fecha: &str, n: i64) -> Option<String> {
    let mut t = fecha.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, d) = (t.next()??, t.next()??, t.next()??);
    let (y0, m0) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y0.div_euclid(400);
    let ye = y0 - era * 400;
    let doy = (153 * m0 + 2) / 5 + d - 1;
    let z = era * 146_097 + ye * 365 + ye / 4 - ye / 100 + doy + n;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let anio = yoe + era * 400 + i64::from(mes <= 2);
    Some(format!("{anio:04}-{mes:02}-{dia:02}"))
}

/// Fecha de la última validación como «25 nov · 15:19».
fn fecha_validacion(cuando: &str) -> String {
    let mut partes = cuando.split_whitespace();
    match (partes.next(), partes.next()) {
        (Some(d), Some(h)) => format!("{} · {}", fecha_corta(d), h.get(..5).unwrap_or(h)),
        (Some(d), None) => fecha_corta(d),
        _ => String::new(),
    }
}

/// Enlace subrayado de 13 px en peso medio con una flecha («Ver todas →»).
fn enlace(ui: &mut egui::Ui, texto: &str) -> egui::Response {
    let pal = tema::p();
    let g = ui.painter().layout_no_wrap(texto.to_string(), egui::FontId::new(13.0, tema::semi()), pal.texto);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 20.0, 24.0), Sense::click());
    let base = r.center().y + g.size().y / 2.0 + 1.0;
    let ancho_texto = g.size().x;
    ui.painter().galley(Pos2::new(r.left(), r.center().y - g.size().y / 2.0), g, pal.texto);
    ui.painter().line_segment([Pos2::new(r.left(), base), Pos2::new(r.left() + ancho_texto, base)], egui::Stroke::new(1.0_f32, pal.texto));
    crate::iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(r.left() + ancho_texto + 6.0, r.center().y - 7.0), Vec2::splat(14.0)), Icono::Flecha, pal.texto);
    resp
}

/// Botón de la barra lateral de la portada: icono de 22 px sobre el texto de 14 px, en blanco.
fn boton_lateral(ui: &mut egui::Ui, icono: Icono, texto: &str, activo: bool) -> egui::Response {
    let blanco = egui::Color32::WHITE;
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 80.0), Sense::click());
    if resp.hovered() && !activo {
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_gray(28));
    }
    if activo {
        ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.top() + 10.0), Pos2::new(r.left() + 3.0, r.bottom() - 10.0)), 0.0, blanco);
    }
    crate::iconos::dibujar(ui.painter(), Rect::from_min_size(Pos2::new(r.center().x - 11.0, r.top() + 14.0), Vec2::splat(22.0)), icono, blanco);
    let fuente = if activo { egui::FontId::new(14.0, tema::semi()) } else { egui::FontId::proportional(14.0) };
    ui.painter().text(Pos2::new(r.center().x, r.top() + 56.0), egui::Align2::CENTER_CENTER, texto, fuente, blanco);
    resp
}

/// Tarjeta de la portada: parte blanca con el título de 19 px, las acciones a la derecha y lo que
/// dibuje `debajo`; el resto de la tarjeta es una zona gris con `gris`.
fn tarjeta_portada(ui: &mut egui::Ui, titulo: &str, acciones: impl FnOnce(&mut egui::Ui), debajo: impl FnOnce(&mut egui::Ui), gris: impl FnOnce(&mut egui::Ui)) {
    let pal = tema::p();
    let r = ui.max_rect();
    ui.painter().rect(r, 0.0, pal.panel, egui::Stroke::new(1.0_f32, pal.borde), egui::StrokeKind::Inside);
    let mut arriba = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(r.min + Vec2::new(20.0, 18.0), Pos2::new(r.right() - 20.0, r.bottom()))).layout(egui::Layout::top_down(egui::Align::Min)));
    arriba.spacing_mut().item_spacing.y = 8.0;
    arriba.horizontal(|ui| {
        ui.label(egui::RichText::new(titulo).size(19.0).family(tema::semi()).color(pal.texto));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), acciones);
    });
    debajo(&mut arriba);
    let corte = arriba.min_rect().bottom() + 14.0;
    let zona = Rect::from_min_max(Pos2::new(r.left() + 1.0, corte), Pos2::new(r.right() - 1.0, r.bottom() - 1.0));
    ui.painter().rect_filled(zona, 0.0, pal.campo);
    let mut abajo = ui.new_child(egui::UiBuilder::new().max_rect(zona).layout(egui::Layout::top_down(egui::Align::Min)));
    abajo.set_clip_rect(zona.intersect(ui.clip_rect()));
    gris(&mut abajo);
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
        if self.captura.is_none() {
            self.zoom = ctx.zoom_factor();
        }
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
        // La portada es una ventana aparte: barra blanca y lateral negra, sin menú ni cinta
        if self.modulo == Modulo::Inicio {
            self.barra_superior_portada(ctx);
        } else {
            self.barra_superior(ctx);
            self.menu_modulos(ctx);
            self.cinta(ctx);
        }
        self.barra_estado(ctx);
        if self.modulo == Modulo::Inicio {
            self.barra_lateral_portada(ctx);
        }
        self.panel_sin_motor(ctx);
        self.panel_pkl(ctx);
        self.dialogo_nuevo_proyecto(ctx);
        if !matches!(self.modulo, Modulo::Inicio | Modulo::Bloques | Modulo::Referencia) {
            self.esquema(ctx);
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(tema::p().fondo)).show(ctx, |ui| {
            let herr = self.herramientas.clone();
            match self.modulo {
                Modulo::Inicio => self.pagina_inicio(ui),
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
        storage.set_string("zoom", format!("{:.2}", self.zoom));
        storage.set_string("recientes", self.editor.recientes.join(","));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.motor.detener();
    }
}


/// Fecha ISO como «25 nov».
fn fecha_corta(f: &str) -> String {
    const MESES: [&str; 12] = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
    let mut t = f.split('-');
    match (t.next(), t.next().and_then(|m| m.parse::<usize>().ok()), t.next().and_then(|d| d.parse::<u32>().ok())) {
        (Some(_), Some(m @ 1..=12), Some(d)) => format!("{d} {}", MESES[m - 1]),
        _ => f.to_string(),
    }
}

/// Dibuja la vista previa simplificada de un diagrama dentro de `r`, ajustada a su tamaño.
fn dibujar_vista_previa(p: egui::Painter, r: Rect, v: &bloques::VistaPrevia) {
    let pal = tema::p();
    if v.bloques.is_empty() {
        return;
    }
    let (mut min, mut max) = (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN));
    for b in &v.bloques {
        min = Pos2::new(min.x.min(b.min.x), min.y.min(b.min.y));
        max = Pos2::new(max.x.max(b.max.x), max.y.max(b.max.y));
    }
    let interior = r.shrink(14.0);
    let tam = max - min;
    let escala = (interior.width() / tam.x.max(1.0)).min(interior.height() / tam.y.max(1.0)).min(1.0);
    let origen = interior.center() - tam * escala / 2.0;
    let mapa = |q: Pos2| origen + (q - min) * escala;
    for (a, b) in &v.cables {
        let (ra, rb) = (v.bloques[*a], v.bloques[*b]);
        let (pa, pb) = (mapa(Pos2::new(ra.right(), ra.center().y)), mapa(Pos2::new(rb.left(), rb.center().y)));
        let xm = (pa.x + pb.x) / 2.0;
        p.add(egui::Shape::line(vec![pa, Pos2::new(xm, pa.y), Pos2::new(xm, pb.y), pb], egui::Stroke::new(1.5_f32, pal.texto)));
    }
    for b in &v.bloques {
        let rb = Rect::from_min_max(mapa(b.min), mapa(b.max));
        p.rect(rb, 0.0, pal.panel, egui::Stroke::new(1.0_f32, pal.acento), egui::StrokeKind::Inside);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn sumar_dias_cruza_meses_anios_y_bisiestos() {
        assert_eq!(sumar_dias("2026-11-26", 30).as_deref(), Some("2026-12-26"));
        assert_eq!(sumar_dias("2026-12-15", 30).as_deref(), Some("2027-01-14"));
        assert_eq!(sumar_dias("2028-02-28", 1).as_deref(), Some("2028-02-29"));
        assert_eq!(sumar_dias("2027-02-28", 1).as_deref(), Some("2027-03-01"));
        assert_eq!(sumar_dias("fecha", 1), None);
    }

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
