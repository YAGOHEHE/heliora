//! Módulo 7: planificador de mantenimiento. Catálogo, planes, órdenes, calendario, historial y AMEF.
//!
//! Los datos viven en el motor: cada edición con significado (crear, borrar, reprogramar, renombrar un
//! código, cambiar una frecuencia) se envía como una operación de `plan_operar`. La interfaz solo edita
//! en el sitio los textos libres que no tienen reglas (descripciones, checklist, campos del AMEF).
use crate::app::Compartido;
use crate::herramientas::{Herramienta, Pagina};
use crate::motor::Motor;
use crate::tema;
use egui::{Pos2, Rect, Sense, Vec2};
use serde_json::{json, Value};
use std::cell::Cell;
use std::hash::{Hash, Hasher};

#[derive(PartialEq, Clone, Copy)]
enum Pestana {
    Analisis,
    Tablero,
    Ordenes,
    Catalogo,
    Planes,
    Amef,
    Historial,
}

/// Lo que la persona debe confirmar antes de que se ejecute.
#[derive(Clone)]
enum Confirmar {
    BorrarOrden(String),
    BorrarPlan { id: String, nombre: String, pendientes: usize },
    BorrarEquipo { codigo: String, pendientes: usize },
    Descartar(Continuar),
}

/// Acción que descarta los cambios sin guardar del plan abierto.
#[derive(Clone, Copy)]
enum Continuar {
    Ejemplo,
    Vacio,
    Abrir,
}

/// Efecto pedido por un control de la interfaz; se ejecuta al terminar de dibujar.
enum Accion {
    Operar(&'static str, Value),
    Generar,
    Correctivas,
    Regenerar(String),
    Aviso(String),
    Confirmar(Confirmar),
    Hecho(String, usize, bool),
    Exportar(&'static str, String),
    DiaSeleccionado(String),
    NuevaOrden,
}

/// Datos del formulario de una orden manual.
struct NuevaOrden {
    equipo: String,
    trabajo: String,
    fecha: String,
    tipo: String,
    responsable: String,
}

pub struct Mantenimiento {
    pub analisis: Pagina,
    pestana: Pestana,
    pub estado: Value,
    pub tablero: Value,
    pub hoy: String,
    hasta: String,
    anio: i32,
    mes: u32,
    dia_sel: Option<String>,
    f_estado: String,
    f_equipo: String,
    f_desde: String,
    f_hasta: String,
    pagina: usize,
    id_estado: Option<u64>,
    id_tablero: Option<u64>,
    id_carga: Option<u64>,
    ruta_carga: Option<std::path::PathBuf>,
    /// La próxima respuesta de estado es un plan nuevo: no tiene archivo ni cambios pendientes.
    base_pendiente: bool,
    /// Falló la apertura y no hay plan en pantalla: se vuelve a crear el de ejemplo.
    reiniciar: bool,
    filtro_amef: String,
    f_historial: String,
    pub ruta: Option<std::path::PathBuf>,
    pub mensaje: String,
    mensaje_error: bool,
    id_informe: Option<u64>,
    hash_guardado: Cell<u64>,
    sucio: bool,
    revisar: bool,
    confirmar: Option<Confirmar>,
    nueva: Option<NuevaOrden>,
    regenerar: Option<String>,
}

/// Fecha civil a partir del tiempo del sistema (algoritmo de días civiles).
pub fn hoy_civil() -> (i32, u32, u32) {
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    civil_desde_dias(s / 86400)
}

fn civil_desde_dias(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y as i32, m, d)
}

fn dias_desde_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = y as i64 - if m <= 2 { 1 } else { 0 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn dias_mes(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (dias_desde_civil(ny, nm, 1) - dias_desde_civil(y, m, 1)) as u32
}

pub fn fecha(y: i32, m: u32, d: u32) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// Comprueba que el texto es una fecha AAAA-MM-DD que existe en el calendario.
pub fn fecha_valida(t: &str) -> bool {
    let b = t.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' || !b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit()) {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (t[0..4].parse::<i32>(), t[5..7].parse::<u32>(), t[8..10].parse::<u32>()) else { return false };
    (1..=12).contains(&m) && d >= 1 && d <= dias_mes(y, m)
}

const MESES: [&str; 12] = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
const TIPOS: [&str; 5] = ["Módulos", "Inversores", "Trackers", "Transformadores", "Cableado"];
const TIPOS_ORDEN: [&str; 4] = ["correctiva", "preventiva", "predictiva", "inspección"];
const POR_PAGINA: usize = 40;

/// Criterios de la lista de órdenes.
struct FiltroOrdenes<'a> {
    estado: &'a str,
    equipo: &'a str,
    desde: &'a str,
    hasta: &'a str,
    hoy: &'a str,
}

/// Tipo de una orden: el guardado o, en planes antiguos, correctiva si viene de una falla.
fn tipo_orden(o: &Value) -> &str {
    match o["tipo"].as_str() {
        Some(t) if !t.is_empty() => t,
        _ if o["plan"].as_str() == Some("CORR") => "correctiva",
        _ => "preventiva",
    }
}

/// Indica si una orden pasa el filtro de estado (pendiente, vencida, cerrada, todas), equipo y rango de fechas.
fn orden_visible(o: &Value, f: &FiltroOrdenes) -> bool {
    let est = o["estado"].as_str().unwrap_or("pendiente");
    let fec = o["fecha"].as_str().unwrap_or("");
    let vencida = est != "cerrada" && fec < f.hoy;
    let ok_estado = match f.estado {
        "pendiente" => est != "cerrada" && !vencida,
        "vencida" => vencida,
        "cerrada" => est == "cerrada",
        _ => true,
    };
    ok_estado
        && (f.equipo.is_empty() || o["equipo"].as_str() == Some(f.equipo))
        && (!fecha_valida(f.desde) || fec >= f.desde)
        && (!fecha_valida(f.hasta) || fec <= f.hasta)
}

fn texto<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

/// Huella del estado para saber si cambió desde la última vez que se guardó o abrió.
fn huella(v: &Value) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(v).unwrap_or_default().hash(&mut h);
    h.finish()
}

impl Mantenimiento {
    pub fn nuevo() -> Mantenimiento {
        let (y, m, d) = hoy_civil();
        Mantenimiento {
            analisis: Pagina::nueva(
                "mantenimiento",
                "Análisis de operación",
                "Importa datos SCADA, calcula KPIs, detecta fallas y estima la vida útil. Empieza con «Generar datos SCADA de muestra» si aún no tienes datos.",
            ),
            pestana: Pestana::Analisis,
            estado: Value::Null,
            tablero: Value::Null,
            hoy: fecha(y, m, d),
            hasta: fecha(y + 1, m, d.min(28)),
            anio: y,
            mes: m,
            dia_sel: None,
            f_estado: "pendiente".into(),
            f_equipo: String::new(),
            f_desde: String::new(),
            f_hasta: String::new(),
            pagina: 0,
            id_estado: None,
            id_tablero: None,
            id_carga: None,
            ruta_carga: None,
            base_pendiente: false,
            reiniciar: false,
            filtro_amef: String::new(),
            f_historial: String::new(),
            ruta: None,
            mensaje: String::new(),
            mensaje_error: false,
            id_informe: None,
            hash_guardado: Cell::new(0),
            sucio: false,
            revisar: false,
            confirmar: None,
            nueva: None,
            regenerar: None,
        }
    }

    pub fn fijar_pestana(&mut self, p: &str) {
        self.pestana = match p {
            "tablero" => Pestana::Tablero,
            "ordenes" => Pestana::Ordenes,
            "catalogo" => Pestana::Catalogo,
            "planes" => Pestana::Planes,
            "amef" => Pestana::Amef,
            "historial" => Pestana::Historial,
            _ => Pestana::Analisis,
        };
    }

    /// Crea el plan de ejemplo y genera sus órdenes para el próximo año.
    pub fn iniciar(&mut self, motor: &mut Motor) {
        self.base_pendiente = true;
        let id = motor.llamar("plan_inicial", json!({"hoy": self.hoy, "hasta": self.hasta}));
        self.id_estado = Some(id);
    }

    fn avisar(&mut self, texto: impl Into<String>, error: bool) {
        self.mensaje = texto.into();
        self.mensaje_error = error;
    }

    /// Toma el estado actual como el guardado: no hay cambios pendientes.
    fn fijar_base(&mut self) {
        self.hash_guardado.set(huella(&self.estado));
        self.sucio = false;
    }

    pub fn recibir(&mut self, id: u64, ok: bool, v: &Value, err: &str, c: &mut Compartido) -> bool {
        if self.analisis.recibir(id, ok, v, err, c) {
            return true;
        }
        if Some(id) == self.id_informe {
            self.id_informe = None;
            if ok {
                let pdf = v["pdf"].as_str().unwrap_or("");
                abrir_con_el_sistema(v["html"].as_str().unwrap_or(""));
                self.avisar(format!("Informe escrito: {} hallazgos, {} órdenes próximas, {} vencidas ({} páginas). PDF en {}", v["hallazgos"], v["proximas"], v["vencidas"], v["paginas"], pdf), false);
            } else {
                self.avisar(format!("No se pudo escribir el informe: {err}"), true);
            }
            return true;
        }
        if Some(id) == self.id_carga {
            self.id_carga = None;
            let ruta = self.ruta_carga.take();
            if !ok {
                self.avisar(format!("No se pudo abrir el plan: {err}"), true);
                self.reiniciar = self.estado.is_null();
                return true;
            }
            self.estado = v["estado"].clone();
            self.tablero = v["tablero"].clone();
            self.regenerar = None;
            self.dia_sel = None;
            self.pagina = 0;
            let avisos: Vec<&str> = v["avisos"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
            let nombre = ruta.as_ref().map(|r| r.display().to_string()).unwrap_or_default();
            self.ruta = ruta;
            self.fijar_base();
            if avisos.is_empty() {
                self.avisar(format!("Plan abierto: {nombre}"), false);
            } else {
                self.avisar(format!("Plan abierto con avisos: {}", avisos.join(" · ")), true);
            }
            return true;
        }
        if Some(id) == self.id_estado {
            self.id_estado = None;
            if !ok {
                self.avisar(err.to_string(), true);
                return true;
            }
            self.estado = v["estado"].clone();
            self.tablero = v["tablero"].clone();
            if self.base_pendiente {
                self.base_pendiente = false;
                self.ruta = None;
                self.regenerar = None;
                self.dia_sel = None;
                self.pagina = 0;
                self.fijar_base();
            }
            self.revisar = true;
            if let Some(p) = v.get("regenerar") {
                self.regenerar = p.as_str().map(String::from);
            }
            if let Some(m) = v.get("mensaje").and_then(|m| m.as_str()) {
                let hay_errores = v["errores"].as_array().is_some_and(|e| !e.is_empty());
                self.avisar(m, hay_errores);
            } else if let Some(n) = v.get("nuevas").and_then(|n| n.as_i64()) {
                self.avisar(format!("{n} órdenes nuevas."), false);
            }
            return true;
        }
        if Some(id) == self.id_tablero {
            self.id_tablero = None;
            if ok {
                self.tablero = v.clone();
            } else {
                self.avisar(format!("No se pudo calcular el tablero: {err}"), true);
            }
            return true;
        }
        false
    }

    fn generar(&mut self, motor: &mut Motor) {
        self.id_estado = Some(motor.llamar("plan_generar", json!({"estado": self.estado, "hasta": self.hasta, "hoy": self.hoy})));
    }

    fn operar(&mut self, motor: &mut Motor, operacion: &str, args: Value) {
        self.id_estado = Some(motor.llamar("plan_operar", json!({"estado": self.estado, "operacion": operacion, "args": args, "hoy": self.hoy})));
    }

    pub fn guardar_en(&self, ruta: &std::path::Path) -> Result<(), String> {
        let t = serde_json::to_string_pretty(&self.estado).map_err(|e| e.to_string())?;
        crate::app::escribir_atomico(ruta, t.as_bytes())?;
        self.hash_guardado.set(huella(&self.estado));
        Ok(())
    }

    /// Pide al motor que abra y valide el plan; el estado en pantalla no cambia hasta que la respuesta sea correcta.
    pub fn cargar_de(&mut self, ruta: &std::path::Path, motor: &mut Motor) -> Result<(), String> {
        if !ruta.is_file() {
            return Err(format!("No existe el archivo {}", ruta.display()));
        }
        self.ruta_carga = Some(ruta.to_path_buf());
        self.id_carga = Some(motor.llamar("plan_cargar", json!({"ruta": ruta.display().to_string(), "hoy": self.hoy})));
        self.avisar(format!("Abriendo {}...", ruta.display()), false);
        Ok(())
    }

    fn guardar(&mut self, como: bool) {
        let r = if como { None } else { self.ruta.clone() };
        let r = r.or_else(|| {
            let mut d = rfd::FileDialog::new().add_filter("Plan JSON", &["json"]).set_file_name("mantenimiento.json");
            if let Some(dir) = self.ruta.as_ref().and_then(|r| r.parent()) {
                d = d.set_directory(dir);
            }
            d.save_file()
        });
        if let Some(r) = r {
            match self.guardar_en(&r) {
                Ok(_) => {
                    self.avisar(format!("Plan guardado en {}", r.display()), false);
                    self.sucio = false;
                    self.ruta = Some(r);
                }
                Err(e) => self.avisar(format!("No se pudo guardar: {e}"), true),
            }
        }
    }

    /// Ejecuta una acción que descarta el plan en pantalla (tras confirmar si hay cambios sin guardar).
    fn continuar(&mut self, c: Continuar, motor: &mut Motor) {
        match c {
            Continuar::Ejemplo => self.iniciar(motor),
            Continuar::Vacio => {
                self.base_pendiente = true;
                self.id_estado = Some(motor.llamar("plan_vacio", json!({"hoy": self.hoy})));
            }
            Continuar::Abrir => {
                if let Some(r) = rfd::FileDialog::new().add_filter("Plan JSON", &["json"]).pick_file() {
                    if let Err(e) = self.cargar_de(&r, motor) {
                        self.avisar(e, true);
                    }
                }
            }
        }
    }

    fn pedir(&mut self, c: Continuar, motor: &mut Motor) {
        if self.sucio {
            self.confirmar = Some(Confirmar::Descartar(c));
        } else {
            self.continuar(c, motor);
        }
    }

    fn codigos(&self) -> Vec<String> {
        self.estado["equipos"].as_array().map(|a| a.iter().map(|e| texto(e, "codigo").to_string()).collect()).unwrap_or_default()
    }

    /// Órdenes sin cerrar cuyo campo `clave` vale `valor`.
    fn pendientes_de(&self, clave: &str, valor: &str) -> usize {
        self.estado["ordenes"].as_array().map(|a| a.iter().filter(|o| texto(o, clave) == valor && texto(o, "estado") != "cerrada").count()).unwrap_or(0)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, todas: &[Herramienta], motor: &mut Motor, c: &mut Compartido) {
        if self.reiniciar && self.estado.is_null() && self.id_estado.is_none() {
            self.reiniciar = false;
            self.iniciar(motor);
        }
        let cambio = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::PointerButton { .. } | egui::Event::Paste(_) | egui::Event::Cut)));
        if !self.estado.is_null() && (self.revisar || cambio) {
            self.revisar = false;
            self.sucio = huella(&self.estado) != self.hash_guardado.get();
        }
        egui::TopBottomPanel::top("pest_mant").frame(tema::marco_panel()).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                for (p, t) in [(Pestana::Analisis, "Análisis"), (Pestana::Tablero, "Tablero y calendario"), (Pestana::Ordenes, "Órdenes de trabajo"), (Pestana::Catalogo, "Catálogo de equipos"), (Pestana::Planes, "Planes"), (Pestana::Amef, "AMEF"), (Pestana::Historial, "Historial")] {
                    let marca = if self.sucio && p != Pestana::Analisis { " •" } else { "" };
                    if ui.selectable_label(self.pestana == p, format!("{t}{marca}")).clicked() {
                        self.pestana = p;
                    }
                }
            });
            if self.pestana != Pestana::Analisis {
                ui.horizontal_wrapped(|ui| self.barra_archivo(ui, motor));
            }
        });
        if self.pestana == Pestana::Analisis {
            self.analisis.ui(ui, todas, motor, c);
            self.dialogos(ui.ctx(), motor);
            return;
        }
        let mut acciones = vec![];
        egui::CentralPanel::default().frame(tema::marco_fondo()).show_inside(ui, |ui| {
            if self.estado.is_null() {
                ui.label(egui::RichText::new("Preparando el plan de mantenimiento...").color(tema::p().texto_2));
                return;
            }
            if !self.mensaje.is_empty() {
                let color = if self.mensaje_error { tema::p().falla } else { tema::p().texto };
                ui.label(egui::RichText::new(&self.mensaje).small().color(color));
            }
            let ocupado = self.id_estado.is_some() || self.id_carga.is_some();
            ui.add_enabled_ui(!ocupado, |ui| match self.pestana {
                Pestana::Tablero => self.p_tablero(ui, &mut acciones),
                Pestana::Ordenes => self.p_ordenes(ui, &mut acciones),
                Pestana::Catalogo => self.p_catalogo(ui, &mut acciones),
                Pestana::Planes => self.p_planes(ui, &mut acciones),
                Pestana::Amef => self.p_amef(ui, &mut acciones),
                Pestana::Historial => self.p_historial(ui, &mut acciones),
                Pestana::Analisis => {}
            });
        });
        self.despachar(acciones, motor);
        self.dialogos(ui.ctx(), motor);
    }

    /// Botones de archivo del plan: nuevo, abrir, guardar, informe y estado de los cambios.
    fn barra_archivo(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        if ui.button("Plan de ejemplo").on_hover_text("Sustituye el plan en pantalla por el de ejemplo").clicked() {
            self.pedir(Continuar::Ejemplo, motor);
        }
        if ui.button("Nuevo plan vacío").clicked() {
            self.pedir(Continuar::Vacio, motor);
        }
        if ui.button("Abrir plan").clicked() {
            self.pedir(Continuar::Abrir, motor);
        }
        if ui.button("Guardar").on_hover_text("Guarda el plan en JSON").clicked() {
            self.guardar(false);
        }
        if ui.button("Guardar como…").clicked() {
            self.guardar(true);
        }
        let ocupado = self.id_informe.is_some();
        if ui.add_enabled(!ocupado, egui::Button::new("Informe O&M (HTML y PDF)")).on_hover_text("Indicadores, hallazgos, órdenes y calendario de 30 días").clicked() {
            if let Some(c) = rfd::FileDialog::new().set_title("Carpeta para el informe").pick_folder() {
                self.id_informe = Some(motor.llamar("informe_om", json!({"estado": self.estado, "carpeta": c.display().to_string(), "hoy": self.hoy})));
                self.avisar("Escribiendo el informe...", false);
            }
        }
        ui.separator();
        let archivo = self.ruta.as_ref().and_then(|r| r.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "sin archivo".into());
        ui.label(egui::RichText::new(archivo).small().color(tema::p().texto_2));
        if self.sucio {
            ui.label(egui::RichText::new("• cambios sin guardar").small().color(tema::p().acento));
        }
    }

    /// Ejecuta las acciones pedidas por la interfaz en este fotograma; solo la primera que habla con el motor.
    fn despachar(&mut self, acciones: Vec<Accion>, motor: &mut Motor) {
        let mut lanzada = false;
        for a in acciones {
            match a {
                Accion::Operar(op, args) if !lanzada => {
                    lanzada = true;
                    self.operar(motor, op, args);
                }
                Accion::Generar if !lanzada => {
                    lanzada = true;
                    self.generar(motor);
                }
                Accion::Correctivas if !lanzada => {
                    lanzada = true;
                    self.id_estado = Some(motor.llamar("plan_correctivas", json!({"estado": self.estado, "hoy": self.hoy})));
                }
                Accion::Regenerar(id) if !lanzada => {
                    if fecha_valida(&self.hasta) {
                        lanzada = true;
                        self.operar(motor, "plan_regenerar", json!({"ident": id, "hasta": self.hasta}));
                    } else {
                        self.avisar("Indica una fecha límite válida (AAAA-MM-DD) para regenerar.", true);
                    }
                }
                Accion::Aviso(m) => self.avisar(m, true),
                Accion::Exportar(que, filtro) if !lanzada => {
                    let nombre = if que == "amef" { "amef.csv" } else { "historial.csv" };
                    if let Some(r) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(nombre).save_file() {
                        lanzada = true;
                        self.operar(motor, "exportar", json!({"que": que, "ruta": r.display().to_string(), "filtro": filtro}));
                    }
                }
                Accion::Confirmar(c) => self.confirmar = Some(c),
                Accion::Hecho(id, i, v) => self.marcar_hecho(&id, i, v),
                Accion::DiaSeleccionado(f) => self.dia_sel = Some(f),
                Accion::NuevaOrden => {
                    let equipo = self.codigos().into_iter().next().unwrap_or_default();
                    self.nueva = Some(NuevaOrden { equipo, trabajo: String::new(), fecha: self.hoy.clone(), tipo: "correctiva".into(), responsable: String::new() });
                }
                _ => {}
            }
        }
    }

    /// Marca un paso de la lista de comprobación; no tiene reglas, se guarda en el sitio.
    fn marcar_hecho(&mut self, id: &str, i: usize, v: bool) {
        let Some(ords) = self.estado["ordenes"].as_array_mut() else { return };
        if let Some(c) = ords.iter_mut().find(|o| o["id"].as_str() == Some(id)).and_then(|o| o["checklist"].get_mut(i)) {
            c["hecho"] = json!(v);
        }
    }

    /// Ventanas modales: confirmaciones y formulario de orden nueva.
    fn dialogos(&mut self, ctx: &egui::Context, motor: &mut Motor) {
        if let Some(c) = self.confirmar.clone() {
            let mut decision: Option<Option<Accion>> = None;
            let r = egui::Modal::new(egui::Id::new("conf_mant")).show(ctx, |ui| {
                ui.set_width(400.0);
                match &c {
                    Confirmar::BorrarOrden(id) => {
                        tema::titulo(ui, &format!("Borrar la orden {id}"));
                        ui.label("Se borra la orden y su entrada del historial. No se puede deshacer.");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Borrar").clicked() {
                                decision = Some(Some(Accion::Operar("orden_borrar", json!({"ident": id}))));
                            }
                            if ui.button("Cancelar").clicked() {
                                decision = Some(None);
                            }
                        });
                    }
                    Confirmar::BorrarPlan { id, nombre, pendientes } => {
                        tema::titulo(ui, &format!("Borrar el plan «{nombre}»"));
                        ui.label(format!("Tiene {pendientes} órdenes pendientes. Las cerradas se conservan en el historial."));
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.button(format!("Borrar el plan y sus {pendientes} pendientes")).clicked() {
                                decision = Some(Some(Accion::Operar("plan_borrar", json!({"ident": id, "borrar_pendientes": true}))));
                            }
                            if ui.button("Borrar solo el plan").clicked() {
                                decision = Some(Some(Accion::Operar("plan_borrar", json!({"ident": id, "borrar_pendientes": false}))));
                            }
                            if ui.button("Cancelar").clicked() {
                                decision = Some(None);
                            }
                        });
                    }
                    Confirmar::BorrarEquipo { codigo, pendientes } => {
                        tema::titulo(ui, &format!("Borrar el equipo {codigo}"));
                        ui.label(format!("Tiene {pendientes} órdenes pendientes. Las cerradas y el historial se conservan."));
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.button(format!("Borrar el equipo y sus {pendientes} pendientes")).clicked() {
                                decision = Some(Some(Accion::Operar("equipo_borrar", json!({"codigo": codigo, "borrar_ordenes": true}))));
                            }
                            if ui.button("Borrar solo el equipo").clicked() {
                                decision = Some(Some(Accion::Operar("equipo_borrar", json!({"codigo": codigo, "borrar_ordenes": false}))));
                            }
                            if ui.button("Cancelar").clicked() {
                                decision = Some(None);
                            }
                        });
                    }
                    Confirmar::Descartar(cont) => {
                        tema::titulo(ui, "Cambios sin guardar");
                        ui.label("El plan en pantalla tiene cambios sin guardar. Si continúas se pierden.");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Descartar y continuar").clicked() {
                                self.confirmar = None;
                                self.sucio = false;
                                self.continuar(*cont, motor);
                            }
                            if ui.button("Cancelar").clicked() {
                                decision = Some(None);
                            }
                        });
                    }
                }
            });
            match decision {
                Some(Some(a)) => {
                    self.confirmar = None;
                    self.despachar(vec![a], motor);
                }
                Some(None) => self.confirmar = None,
                None if r.should_close() => self.confirmar = None,
                None => {}
            }
        }
        self.formulario_orden(ctx, motor);
    }

    fn formulario_orden(&mut self, ctx: &egui::Context, motor: &mut Motor) {
        let codigos = self.codigos();
        let Some(n) = self.nueva.as_mut() else { return };
        let mut crear = false;
        let mut cerrar = false;
        let r = egui::Modal::new(egui::Id::new("nueva_orden")).show(ctx, |ui| {
            ui.set_width(420.0);
            tema::titulo(ui, "Nueva orden de trabajo");
            egui::Grid::new("g_nueva").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("Equipo");
                combo_texto(ui, "nueva_eq", &mut n.equipo, &codigos, 200.0);
                ui.end_row();
                ui.label("Trabajo");
                ui.add(egui::TextEdit::singleline(&mut n.trabajo).desired_width(260.0));
                ui.end_row();
                ui.label("Fecha");
                fecha_libre(ui, &mut n.fecha, 100.0, false);
                ui.end_row();
                ui.label("Tipo");
                combo_texto(ui, "nueva_tipo", &mut n.tipo, &TIPOS_ORDEN.map(String::from), 140.0);
                ui.end_row();
                ui.label("Responsable");
                ui.add(egui::TextEdit::singleline(&mut n.responsable).desired_width(200.0));
                ui.end_row();
            });
            let valida = fecha_valida(&n.fecha) && !n.trabajo.trim().is_empty() && codigos.contains(&n.equipo);
            if !valida {
                ui.label(egui::RichText::new("Indica un equipo del catálogo, el trabajo y una fecha AAAA-MM-DD.").small().color(tema::p().texto_2));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(valida, egui::Button::new("Crear orden")).clicked() {
                    crear = true;
                }
                if ui.button("Cancelar").clicked() {
                    cerrar = true;
                }
            });
        });
        if crear {
            if let Some(n) = self.nueva.take() {
                self.operar(motor, "orden_crear", json!({"equipo": n.equipo, "trabajo": n.trabajo, "fecha": n.fecha, "tipo": n.tipo, "responsable": n.responsable}));
            }
        } else if cerrar || r.should_close() {
            self.nueva = None;
        }
    }

    fn p_tablero(&mut self, ui: &mut egui::Ui, acciones: &mut Vec<Accion>) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Generar órdenes hasta");
            let valida = fecha_libre(ui, &mut self.hasta, 90.0, false);
            if ui.add_enabled(valida, egui::Button::new("Generar")).clicked() {
                acciones.push(Accion::Generar);
            }
            if ui.button("Crear correctivas desde fallas").on_hover_text("Usa los hallazgos de «Detección de fallas»; no repite una correctiva abierta").clicked() {
                acciones.push(Accion::Correctivas);
            }
        });
        ui.add_space(4.0);
        let t = &self.tablero;
        let num = |k: &str| t.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
        let cump = t.get("cumplimiento").and_then(|v| v.as_f64()).unwrap_or(0.0);
        ui.horizontal(|ui| {
            cifra(ui, "Vencidas", &num("vencidas").to_string(), if num("vencidas") > 0 { Some(tema::Nivel::Falla) } else { Some(tema::Nivel::Ok) });
            cifra(ui, "Próximos 30 días", &num("proximas_30").to_string(), None);
            cifra(ui, "Pendientes", &num("pendientes").to_string(), None);
            cifra(ui, "Cerradas", &num("cerradas").to_string(), None);
            // Sin órdenes vencidas ni cerradas todavía no hay cumplimiento que medir
            if num("cerradas") + num("vencidas") == 0 {
                cifra(ui, "Cumplimiento", "—", None);
            } else {
                cifra(ui, "Cumplimiento", &format!("{:.0} %", cump * 100.0), Some(if cump >= 0.9 { tema::Nivel::Ok } else if cump >= 0.7 { tema::Nivel::Alerta } else { tema::Nivel::Falla }));
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("◀").clicked() {
                if self.mes == 1 {
                    self.mes = 12;
                    self.anio -= 1;
                } else {
                    self.mes -= 1;
                }
            }
            ui.label(egui::RichText::new(format!("{} de {}", MESES[self.mes as usize - 1], self.anio)).size(16.0).strong());
            if ui.button("▶").clicked() {
                if self.mes == 12 {
                    self.mes = 1;
                    self.anio += 1;
                } else {
                    self.mes += 1;
                }
            }
            if ui.button("Hoy").clicked() {
                let (y, m, _) = hoy_civil();
                self.anio = y;
                self.mes = m;
            }
        });
        let ordenes: Vec<Value> = self.estado["ordenes"].as_array().cloned().unwrap_or_default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if let Some(f) = self.calendario(ui, &ordenes) {
                acciones.push(Accion::DiaSeleccionado(f));
            }
            if let Some(dia) = self.dia_sel.clone() {
                ui.add_space(8.0);
                tema::tarjeta(ui, |ui| {
                    ui.set_width(ui.available_width());
                    tema::titulo(ui, &format!("Órdenes del {dia}"));
                    let del_dia: Vec<&Value> = ordenes.iter().filter(|o| texto(o, "fecha") == dia).collect();
                    if del_dia.is_empty() {
                        ui.label(egui::RichText::new("Ninguna orden este día.").color(tema::p().texto_2));
                    }
                    for o in del_dia {
                        fila_del_dia(ui, o, &self.hoy, &dia, acciones);
                    }
                });
            }
        });
    }

    /// Dibuja el mes y devuelve el día en el que se hizo clic.
    fn calendario(&mut self, ui: &mut egui::Ui, ordenes: &[Value]) -> Option<String> {
        let ancho = ui.available_width();
        let celda = Vec2::new(ancho / 7.0, 74.0);
        let dias = dias_mes(self.anio, self.mes);
        // Lunes = 0
        let primero = ((dias_desde_civil(self.anio, self.mes, 1) + 3).rem_euclid(7)) as u32;
        let filas = (primero + dias).div_ceil(7);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ancho, 22.0 + celda.y * filas as f32), Sense::click());
        let p = ui.painter_at(rect);
        for (i, d) in ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"].iter().enumerate() {
            p.text(rect.min + Vec2::new(celda.x * (i as f32 + 0.5), 10.0), egui::Align2::CENTER_CENTER, *d, egui::FontId::proportional(12.0), tema::p().texto_2);
        }
        let hoy = self.hoy.clone();
        let mut elegido_ahora = None;
        for dia in 1..=dias {
            let idx = primero + dia - 1;
            let (cx, cy) = ((idx % 7) as f32, (idx / 7) as f32);
            let r = Rect::from_min_size(Pos2::new(rect.left() + cx * celda.x, rect.top() + 22.0 + cy * celda.y), celda).shrink(2.0);
            let f = fecha(self.anio, self.mes, dia);
            let elegido = self.dia_sel.as_deref() == Some(&f);
            p.rect(r, tema::radio() as f32, if elegido { tema::p().realce } else { tema::p().campo }, egui::Stroke::new(1.0_f32, tema::p().linea), egui::StrokeKind::Inside);
            if f == hoy {
                p.rect_stroke(r, tema::radio() as f32, egui::Stroke::new(1.5_f32, tema::p().acento), egui::StrokeKind::Middle);
            }
            p.text(r.left_top() + Vec2::new(6.0, 4.0), egui::Align2::LEFT_TOP, dia.to_string(), egui::FontId::proportional(12.0), tema::p().texto);
            let del: Vec<&Value> = ordenes.iter().filter(|o| o["fecha"].as_str() == Some(&f)).collect();
            let mut y = r.top() + 21.0;
            for o in del.iter().take(3) {
                let cerrada = o["estado"].as_str() == Some("cerrada");
                let vencida = !cerrada && f < hoy;
                // Estado con forma y texto: cuadrado de 7 px (cerrada OK, vencida FALLA) y el nombre en TEXTO
                let color = if cerrada {
                    tema::p().ok
                } else if vencida {
                    tema::p().falla
                } else {
                    tema::p().linea_fuerte
                };
                let b = Rect::from_min_size(Pos2::new(r.left() + 4.0, y), Vec2::new(r.width() - 8.0, 14.0));
                tema::cuadrado(&p, Pos2::new(b.left() + 4.5, b.center().y), color);
                let texto: String = o["nombre"].as_str().unwrap_or("").chars().take((r.width() / 6.2) as usize).collect();
                p.with_clip_rect(b).text(b.left_center() + Vec2::new(12.0, 0.0), egui::Align2::LEFT_CENTER, texto, egui::FontId::proportional(10.5), tema::p().texto);
                y += 15.0;
            }
            if del.len() > 3 {
                p.text(r.right_top() + Vec2::new(-5.0, 4.0), egui::Align2::RIGHT_TOP, format!("+{}", del.len() - 3), egui::FontId::proportional(11.0), tema::p().texto_2);
            }
            if resp.clicked() {
                if let Some(q) = resp.interact_pointer_pos() {
                    if r.contains(q) {
                        elegido_ahora = Some(f.clone());
                    }
                }
            }
        }
        elegido_ahora
    }

    fn p_ordenes(&mut self, ui: &mut egui::Ui, acciones: &mut Vec<Accion>) {
        let codigos = self.codigos();
        ui.horizontal_wrapped(|ui| {
            for (v, t) in [("pendiente", "Pendientes"), ("vencida", "Vencidas"), ("cerrada", "Cerradas"), ("todas", "Todas")] {
                if ui.selectable_label(self.f_estado == v, t).clicked() {
                    self.f_estado = v.into();
                    self.pagina = 0;
                }
            }
            ui.separator();
            ui.label("Equipo");
            let antes = self.f_equipo.clone();
            egui::ComboBox::from_id_salt("f_equipo_ord").selected_text(if self.f_equipo.is_empty() { "Todos" } else { &self.f_equipo }).width(110.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.f_equipo, String::new(), "Todos");
                for c in &codigos {
                    ui.selectable_value(&mut self.f_equipo, c.clone(), c);
                }
            });
            ui.label("Desde");
            fecha_libre(ui, &mut self.f_desde, 90.0, true);
            ui.label("Hasta");
            fecha_libre(ui, &mut self.f_hasta, 90.0, true);
            if antes != self.f_equipo {
                self.pagina = 0;
            }
            if ui.button("Limpiar filtros").clicked() {
                self.f_equipo.clear();
                self.f_desde.clear();
                self.f_hasta.clear();
                self.pagina = 0;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if tema::boton_principal(ui, "+ Nueva orden").clicked() {
                acciones.push(Accion::NuevaOrden);
            }
            ui.separator();
            ui.label("Generar hasta");
            let valida = fecha_libre(ui, &mut self.hasta, 90.0, false);
            if ui.add_enabled(valida, egui::Button::new("Generar")).clicked() {
                acciones.push(Accion::Generar);
            }
        });
        let filtro = FiltroOrdenes { estado: &self.f_estado, equipo: &self.f_equipo, desde: &self.f_desde, hasta: &self.f_hasta, hoy: &self.hoy };
        let visibles: Vec<usize> = self.estado["ordenes"].as_array().map(|a| a.iter().enumerate().filter(|(_, o)| orden_visible(o, &filtro)).map(|(i, _)| i).collect()).unwrap_or_default();
        let paginas = visibles.len().div_ceil(POR_PAGINA).max(1);
        self.pagina = self.pagina.min(paginas - 1);
        ui.horizontal(|ui| {
            if ui.add_enabled(self.pagina > 0, egui::Button::new("◀")).clicked() {
                self.pagina -= 1;
            }
            ui.label(format!("{} órdenes · página {} de {}", visibles.len(), self.pagina + 1, paginas));
            if ui.add_enabled(self.pagina + 1 < paginas, egui::Button::new("▶")).clicked() {
                self.pagina += 1;
            }
        });
        let pagina: Vec<Value> = visibles.iter().skip(self.pagina * POR_PAGINA).take(POR_PAGINA).map(|&i| self.estado["ordenes"][i].clone()).collect();
        let hoy = self.hoy.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if pagina.is_empty() {
                ui.label(egui::RichText::new("No hay órdenes en esta vista.").color(tema::p().texto_2));
            }
            for o in &pagina {
                orden(ui, o, &hoy, &codigos, acciones);
            }
        });
    }

    fn p_catalogo(&mut self, ui: &mut egui::Ui, acciones: &mut Vec<Accion>) {
        ui.horizontal(|ui| {
            tema::titulo(ui, "Equipos de la planta");
            if ui.button("+ Añadir equipo").clicked() {
                acciones.push(Accion::Operar("equipo_crear", json!({})));
            }
        });
        let pendientes: Vec<usize> = self.estado["equipos"].as_array().map(|a| a.iter().map(|e| self.pendientes_de("equipo", texto(e, "codigo"))).collect()).unwrap_or_default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let Some(eq) = self.estado["equipos"].as_array_mut() else { return };
            egui::Grid::new("g_eq").striped(true).num_columns(6).spacing([10.0, 5.0]).show(ui, |ui| {
                for t in ["Código", "Tipo", "Descripción", "Ubicación", "Instalado", ""] {
                    ui.label(egui::RichText::new(t).strong().color(tema::p().texto));
                }
                ui.end_row();
                for (i, e) in eq.iter_mut().enumerate() {
                    let codigo = texto(e, "codigo").to_string();
                    if let Some(nuevo) = texto_diferido(ui, ("eq_codigo", &codigo), &codigo, 80.0, 1) {
                        acciones.push(Accion::Operar("equipo_editar", json!({"actual": codigo, "codigo": nuevo})));
                    }
                    let mut tipo = texto(e, "tipo").to_string();
                    egui::ComboBox::from_id_salt(("tipo", i)).selected_text(&tipo).width(120.0).show_ui(ui, |ui| {
                        for t in TIPOS {
                            if ui.selectable_value(&mut tipo, t.to_string(), t).clicked() && tipo != texto(e, "tipo") {
                                acciones.push(Accion::Operar("equipo_editar", json!({"actual": codigo, "tipo": tipo})));
                            }
                        }
                    });
                    campo(ui, e, "descripcion", 280.0);
                    campo(ui, e, "ubicacion", 110.0);
                    match fecha_diferida(ui, ("eq_inst", &codigo), texto(e, "instalado"), 90.0) {
                        Some(Ok(f)) => acciones.push(Accion::Operar("equipo_editar", json!({"actual": codigo, "instalado": f}))),
                        Some(Err(m)) => acciones.push(Accion::Aviso(m)),
                        None => {}
                    }
                    if ui.small_button("×").on_hover_text("Borrar equipo").clicked() {
                        acciones.push(Accion::Confirmar(Confirmar::BorrarEquipo { codigo: codigo.clone(), pendientes: pendientes[i] }));
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn p_planes(&mut self, ui: &mut egui::Ui, acciones: &mut Vec<Accion>) {
        ui.horizontal(|ui| {
            tema::titulo(ui, "Planes de mantenimiento preventivo");
            if ui.button("+ Añadir plan").clicked() {
                acciones.push(Accion::Operar("plan_crear", json!({"nombre": "Plan nuevo", "tipo_equipo": "Módulos", "frecuencia_dias": 90, "checklist": ["Paso 1"]})));
            }
        });
        if let Some(id) = self.regenerar.clone() {
            tema::tarjeta(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(format!("El plan {id} cambió: sus órdenes pendientes conservan las fechas anteriores. Al regenerar se sustituyen por las que corresponden hasta {}.", self.hasta));
                ui.horizontal(|ui| {
                    if ui.add_enabled(fecha_valida(&self.hasta), egui::Button::new("Regenerar órdenes pendientes")).clicked() {
                        acciones.push(Accion::Regenerar(id.clone()));
                        self.regenerar = None;
                    }
                    if ui.button("Ahora no").clicked() {
                        self.regenerar = None;
                    }
                    ui.label("Generar hasta");
                    fecha_libre(ui, &mut self.hasta, 90.0, false);
                });
            });
            ui.add_space(4.0);
        }
        let pendientes: Vec<usize> = self.estado["planes"].as_array().map(|a| a.iter().map(|p| self.pendientes_de("plan", texto(p, "id"))).collect()).unwrap_or_default();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let Some(planes) = self.estado["planes"].as_array_mut() else { return };
            for (i, p) in planes.iter_mut().enumerate() {
                let id = texto(p, "id").to_string();
                tema::tarjeta(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        let mut a = p["activo"].as_bool().unwrap_or(true);
                        if ui.checkbox(&mut a, "").on_hover_text("Plan activo").changed() {
                            acciones.push(Accion::Operar("plan_editar", json!({"ident": id, "activo": a})));
                        }
                        ui.label(egui::RichText::new(&id).monospace().color(tema::p().texto_2));
                        if let Some(n) = texto_diferido(ui, ("pl_nombre", &id), texto(p, "nombre"), 240.0, 1) {
                            acciones.push(Accion::Operar("plan_editar", json!({"ident": id, "nombre": n})));
                        }
                        let mut tipo = texto(p, "tipo_equipo").to_string();
                        egui::ComboBox::from_id_salt(("pt", i)).selected_text(&tipo).width(120.0).show_ui(ui, |ui| {
                            for t in TIPOS {
                                if ui.selectable_value(&mut tipo, t.to_string(), t).clicked() && tipo != texto(p, "tipo_equipo") {
                                    acciones.push(Accion::Operar("plan_editar", json!({"ident": id, "tipo_equipo": tipo})));
                                }
                            }
                        });
                        ui.label("cada");
                        let original = p["frecuencia_dias"].as_i64().unwrap_or(30);
                        let mut f = original;
                        let r = ui.add(egui::DragValue::new(&mut f).range(1..=3650));
                        if (r.drag_stopped() || r.lost_focus()) && f != original {
                            acciones.push(Accion::Operar("plan_editar", json!({"ident": id, "frecuencia_dias": f})));
                        }
                        ui.label("días, desde");
                        match fecha_diferida(ui, ("pl_inicio", &id), texto(p, "inicio"), 90.0) {
                            Some(Ok(f)) => acciones.push(Accion::Operar("plan_editar", json!({"ident": id, "inicio": f}))),
                            Some(Err(m)) => acciones.push(Accion::Aviso(m)),
                            None => {}
                        }
                    });
                    let mut t = p["checklist"].as_array().map(|c| c.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                    ui.label(egui::RichText::new("Lista de comprobación (un paso por línea)").small().color(tema::p().texto_2));
                    if ui.add(egui::TextEdit::multiline(&mut t).desired_rows(2).desired_width(f32::INFINITY)).changed() {
                        p["checklist"] = json!(t.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>());
                    }
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("{} órdenes pendientes", pendientes[i])).small().color(tema::p().texto_2));
                        if ui.small_button("Regenerar pendientes").on_hover_text("Sustituye las pendientes por las que corresponden según la frecuencia y el inicio").clicked() {
                            acciones.push(Accion::Regenerar(id.clone()));
                        }
                        if ui.small_button("Borrar plan").clicked() {
                            acciones.push(Accion::Confirmar(Confirmar::BorrarPlan { id: id.clone(), nombre: texto(p, "nombre").to_string(), pendientes: pendientes[i] }));
                        }
                    });
                });
                ui.add_space(4.0);
            }
        });
    }

    fn p_amef(&mut self, ui: &mut egui::Ui, acciones: &mut Vec<Accion>) {
        ui.horizontal_wrapped(|ui| {
            tema::titulo(ui, "AMEF por equipo");
            egui::ComboBox::from_id_salt("famef").selected_text(if self.filtro_amef.is_empty() { "Todos los equipos" } else { &self.filtro_amef }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.filtro_amef, String::new(), "Todos los equipos");
                for t in TIPOS {
                    ui.selectable_value(&mut self.filtro_amef, t.to_string(), t);
                }
            });
            if ui.button("+ Añadir modo de falla").clicked() {
                let t = if self.filtro_amef.is_empty() { "Módulos".to_string() } else { self.filtro_amef.clone() };
                acciones.push(Accion::Operar("amef_crear", json!({"tipo_equipo": t})));
            }
            if ui.button("Ordenar por RPN").on_hover_text("Reordena la tabla de mayor a menor RPN; las ediciones no la reordenan").clicked() {
                acciones.push(Accion::Operar("amef_ordenar", json!({})));
            }
            if ui.button("Exportar CSV").on_hover_text("Exporta las filas que se ven, con «;» y UTF-8 para Excel").clicked() {
                acciones.push(Accion::Exportar("amef", self.filtro_amef.clone()));
            }
        });
        ui.label(egui::RichText::new("RPN = S·O·D, con severidad, ocurrencia y detección de 1 a 10. Más alto significa actuar antes.").small().color(tema::p().texto_2));
        let filtro = self.filtro_amef.clone();
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            let Some(am) = self.estado["amef"].as_array_mut() else { return };
            let rpn = |a: &Value| a["S"].as_i64().unwrap_or(1) * a["O"].as_i64().unwrap_or(1) * a["D"].as_i64().unwrap_or(1);
            egui::Grid::new("g_amef").striped(true).num_columns(11).spacing([8.0, 5.0]).show(ui, |ui| {
                for t in ["Equipo", "Modo de falla", "Causa", "Efecto", "S", "O", "D", "RPN", "Control", "Acción", ""] {
                    ui.label(egui::RichText::new(t).strong().color(tema::p().texto));
                }
                ui.end_row();
                for (i, a) in am.iter_mut().enumerate() {
                    if !filtro.is_empty() && texto(a, "tipo_equipo") != filtro {
                        continue;
                    }
                    ui.label(texto(a, "tipo_equipo"));
                    campo(ui, a, "modo", 160.0);
                    campo(ui, a, "causa", 160.0);
                    campo(ui, a, "efecto", 160.0);
                    for k in ["S", "O", "D"] {
                        let mut v = a[k].as_i64().unwrap_or(1);
                        if ui.add(egui::DragValue::new(&mut v).range(1..=10)).changed() {
                            a[k] = json!(v);
                        }
                    }
                    let r = rpn(a);
                    let color = if r >= 150 {
                        tema::p().falla
                    } else if r >= 80 {
                        tema::p().acento
                    } else {
                        tema::p().ok
                    };
                    ui.label(egui::RichText::new(r.to_string()).strong().color(color));
                    campo(ui, a, "control", 190.0);
                    campo(ui, a, "accion", 190.0);
                    if ui.small_button("×").on_hover_text("Borrar modo de falla").clicked() {
                        acciones.push(Accion::Operar("amef_borrar", json!({"indice": i})));
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn p_historial(&mut self, ui: &mut egui::Ui, acciones: &mut Vec<Accion>) {
        let h: Vec<Value> = self.estado["historial"].as_array().cloned().unwrap_or_default();
        let mut equipos: Vec<String> = h.iter().map(|r| texto(r, "equipo").to_string()).collect();
        equipos.sort();
        equipos.dedup();
        ui.horizontal_wrapped(|ui| {
            tema::titulo(ui, "Trabajos realizados");
            egui::ComboBox::from_id_salt("f_hist").selected_text(if self.f_historial.is_empty() { "Todos los equipos" } else { &self.f_historial }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.f_historial, String::new(), "Todos los equipos");
                for e in &equipos {
                    ui.selectable_value(&mut self.f_historial, e.clone(), e);
                }
            });
            if ui.add_enabled(!h.is_empty(), egui::Button::new("Exportar CSV")).on_hover_text("Exporta las filas que se ven, con «;» y UTF-8 para Excel").clicked() {
                acciones.push(Accion::Exportar("historial", self.f_historial.clone()));
            }
        });
        if h.is_empty() {
            ui.label(egui::RichText::new("Aún no hay trabajos cerrados. Se registran al cerrar una orden.").color(tema::p().texto_2));
            return;
        }
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("g_hist").striped(true).num_columns(6).spacing([14.0, 4.0]).show(ui, |ui| {
                for t in ["Fecha", "Orden", "Equipo", "Trabajo", "Lista completa", "Notas"] {
                    ui.label(egui::RichText::new(t).strong().color(tema::p().texto));
                }
                ui.end_row();
                for r in h.iter().rev().filter(|r| self.f_historial.is_empty() || texto(r, "equipo") == self.f_historial) {
                    ui.label(texto(r, "fecha"));
                    ui.label(texto(r, "orden"));
                    ui.label(texto(r, "equipo"));
                    ui.label(texto(r, "trabajo"));
                    ui.label(if r["completo"].as_bool().unwrap_or(false) { "sí" } else { "no" });
                    ui.label(texto(r, "notas"));
                    ui.end_row();
                }
            });
        });
    }
}

/// Una orden en la lista: cabecera plegable con su edición, cierre, reapertura y borrado.
fn orden(ui: &mut egui::Ui, o: &Value, hoy: &str, codigos: &[String], acciones: &mut Vec<Accion>) {
    let est = texto(o, "estado");
    let fec = texto(o, "fecha");
    let id = texto(o, "id").to_string();
    let cerrada = est == "cerrada";
    let vencida = !cerrada && fec < hoy;
    let mut cab = format!("{}  ·  {}  ·  {}  ·  {}  ·  {}", fec, id, texto(o, "nombre"), texto(o, "equipo"), tipo_orden(o));
    if cerrada {
        cab += "  ·  cerrada";
    } else if vencida {
        cab += "  ·  vencida";
    }
    let color = if vencida { tema::p().falla } else { tema::p().texto };
    egui::CollapsingHeader::new(egui::RichText::new(cab).color(color)).id_salt(&id).show(ui, |ui| {
        if !cerrada {
            egui::Grid::new(("g_ord", &id)).num_columns(4).spacing([10.0, 5.0]).show(ui, |ui| {
                ui.label("Trabajo");
                if let Some(t) = texto_diferido(ui, ("o_trabajo", &id), texto(o, "nombre"), 280.0, 1) {
                    acciones.push(Accion::Operar("orden_editar", json!({"ident": id, "trabajo": t})));
                }
                ui.label("Equipo");
                let mut eq = texto(o, "equipo").to_string();
                if combo_texto(ui, ("o_equipo", &id), &mut eq, codigos, 110.0) {
                    acciones.push(Accion::Operar("orden_editar", json!({"ident": id, "equipo": eq})));
                }
                ui.end_row();
                ui.label("Fecha");
                match fecha_diferida(ui, ("o_fecha", &id), fec, 90.0) {
                    Some(Ok(f)) => acciones.push(Accion::Operar("orden_editar", json!({"ident": id, "fecha": f}))),
                    Some(Err(m)) => acciones.push(Accion::Aviso(m)),
                    None => {}
                }
                ui.label("Tipo");
                let mut tipo = tipo_orden(o).to_string();
                if combo_texto(ui, ("o_tipo", &id), &mut tipo, &TIPOS_ORDEN.map(String::from), 110.0) {
                    acciones.push(Accion::Operar("orden_editar", json!({"ident": id, "tipo": tipo})));
                }
                ui.end_row();
            });
        }
        ui.horizontal(|ui| {
            ui.label("Responsable");
            if let Some(r) = texto_diferido(ui, ("o_resp", &id), texto(o, "responsable"), 220.0, 1) {
                acciones.push(Accion::Operar("orden_editar", json!({"ident": id, "responsable": r})));
            }
        });
        if let Some(ch) = o["checklist"].as_array() {
            for (i, c) in ch.iter().enumerate() {
                let mut h = c["hecho"].as_bool().unwrap_or(false);
                if ui.checkbox(&mut h, texto(c, "texto")).changed() {
                    acciones.push(Accion::Hecho(id.clone(), i, h));
                }
            }
        }
        ui.label("Notas");
        if let Some(n) = texto_diferido(ui, ("o_notas", &id), texto(o, "notas"), 420.0, 2) {
            acciones.push(Accion::Operar("orden_editar", json!({"ident": id, "notas": n})));
        }
        ui.horizontal(|ui| {
            if cerrada {
                ui.label(egui::RichText::new(format!("Cerrada el {}", texto(o, "cerrada"))).color(tema::p().ok));
                if ui.button("Reabrir").clicked() {
                    acciones.push(Accion::Operar("orden_reabrir", json!({"ident": id})));
                }
            } else {
                let clave = ui.make_persistent_id(("o_cierre", &id));
                let mut f = ui.data(|d| d.get_temp::<String>(clave)).unwrap_or_else(|| hoy.to_string());
                ui.label("Cerrar con fecha");
                let valida = fecha_libre(ui, &mut f, 90.0, false);
                ui.data_mut(|d| d.insert_temp(clave, f.clone()));
                if ui.add_enabled(valida, egui::Button::new("Cerrar orden")).clicked() {
                    acciones.push(Accion::Operar("orden_cerrar", json!({"ident": id, "fecha": f})));
                }
            }
            if ui.button("Borrar").clicked() {
                acciones.push(Accion::Confirmar(Confirmar::BorrarOrden(id.clone())));
            }
        });
    });
}

/// Una orden del día elegido en el calendario, con cierre y reprogramación.
fn fila_del_dia(ui: &mut egui::Ui, o: &Value, hoy: &str, dia: &str, acciones: &mut Vec<Accion>) {
    let id = texto(o, "id").to_string();
    let cerrada = texto(o, "estado") == "cerrada";
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(&id).monospace());
        ui.label(texto(o, "nombre"));
        ui.label(egui::RichText::new(texto(o, "equipo")).color(tema::p().acento_2));
        ui.label(egui::RichText::new(texto(o, "estado")).color(tema::p().texto_2));
        if !cerrada {
            if ui.small_button("Cerrar hoy").on_hover_text(format!("Cierra con fecha {hoy}")).clicked() {
                acciones.push(Accion::Operar("orden_cerrar", json!({"ident": id, "fecha": hoy})));
            }
            let clave = ui.make_persistent_id(("dia_mover", &id));
            let mut f = ui.data(|d| d.get_temp::<String>(clave)).unwrap_or_else(|| dia.to_string());
            let valida = fecha_libre(ui, &mut f, 90.0, false);
            ui.data_mut(|d| d.insert_temp(clave, f.clone()));
            if ui.add_enabled(valida && f != dia, egui::Button::new("Reprogramar").small()).clicked() {
                acciones.push(Accion::Operar("orden_editar", json!({"ident": id, "fecha": f})));
            }
        }
    });
}

fn campo(ui: &mut egui::Ui, v: &mut Value, k: &str, ancho: f32) {
    let mut t = v[k].as_str().unwrap_or("").to_string();
    if ui.add_sized([ancho, 22.0], egui::TextEdit::singleline(&mut t)).on_hover_text(t.clone()).changed() {
        v[k] = json!(t);
    }
}

/// Campo de texto cuyo valor se entrega solo al salir de él (con el cambio hecho), no en cada tecla.
fn texto_diferido(ui: &mut egui::Ui, id: impl Hash, actual: &str, ancho: f32, filas: usize) -> Option<String> {
    let id = ui.make_persistent_id(id);
    let mut t = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| actual.to_string());
    let r = if filas <= 1 { ui.add_sized([ancho, 22.0], egui::TextEdit::singleline(&mut t)) } else { ui.add(egui::TextEdit::multiline(&mut t).desired_rows(filas).desired_width(ancho)) };
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, t.clone()));
    }
    if r.lost_focus() {
        ui.data_mut(|d| d.remove_temp::<String>(id));
        if t != actual {
            return Some(t);
        }
    }
    None
}

/// Campo de fecha AAAA-MM-DD que valida al salir: devuelve la fecha nueva, o el error si no es válida.
fn fecha_diferida(ui: &mut egui::Ui, id: impl Hash, actual: &str, ancho: f32) -> Option<Result<String, String>> {
    let id = ui.make_persistent_id(id);
    let mut t = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| actual.to_string());
    let color = if fecha_valida(&t) { tema::p().texto } else { tema::p().falla };
    let r = ui.add_sized([ancho, 22.0], egui::TextEdit::singleline(&mut t).text_color(color).hint_text("AAAA-MM-DD"));
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, t.clone()));
    }
    if r.lost_focus() {
        ui.data_mut(|d| d.remove_temp::<String>(id));
        if t != actual {
            return Some(if fecha_valida(&t) { Ok(t) } else { Err(format!("La fecha «{t}» no es válida: usa AAAA-MM-DD con un día que exista.")) });
        }
    }
    None
}

/// Campo de fecha que edita el texto en el sitio y marca en rojo lo que no es AAAA-MM-DD; dice si es válida.
fn fecha_libre(ui: &mut egui::Ui, v: &mut String, ancho: f32, admite_vacia: bool) -> bool {
    let valida = fecha_valida(v) || (admite_vacia && v.is_empty());
    let color = if valida { tema::p().texto } else { tema::p().falla };
    ui.add_sized([ancho, 22.0], egui::TextEdit::singleline(v).text_color(color).hint_text("AAAA-MM-DD"));
    valida
}

/// Lista desplegable de textos; dice si la persona eligió otro valor.
fn combo_texto(ui: &mut egui::Ui, id: impl Hash, valor: &mut String, opciones: &[String], ancho: f32) -> bool {
    let mut cambio = false;
    egui::ComboBox::from_id_salt(id).selected_text(valor.as_str()).width(ancho).show_ui(ui, |ui| {
        for o in opciones {
            if ui.selectable_value(valor, o.clone(), o).clicked() {
                cambio = true;
            }
        }
    });
    cambio
}

fn cifra(ui: &mut egui::Ui, t: &str, v: &str, estado: Option<tema::Nivel>) {
    let pal = tema::p();
    egui::Frame::new().fill(pal.panel).stroke(egui::Stroke::new(1.0_f32, pal.linea)).corner_radius(tema::radio_ventana()).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(v).size(22.0).family(egui::FontFamily::Name("dato".into())).color(pal.texto));
            match estado {
                Some(n) => {
                    tema::estado(ui, n, t);
                }
                None => {
                    ui.label(egui::RichText::new(t).small().color(pal.texto_2));
                }
            }
        });
    });
}

/// Abre un archivo con la aplicación predeterminada del sistema (el navegador para un HTML).
fn abrir_con_el_sistema(ruta: &str) {
    if ruta.is_empty() || std::env::var_os("HELIORA_SIN_ABRIR").is_some() {
        return;
    }
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(ruta).spawn();
    #[cfg(target_os = "windows")]
    let r = crate::motor::orden("cmd").args(["/C", "start", "", ruta]).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(ruta).spawn();
    let _ = r;
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn fechas_validas_e_invalidas() {
        for ok in ["2026-01-01", "2024-02-29", "2026-12-31"] {
            assert!(fecha_valida(ok), "{ok}");
        }
        for mal in ["", "2026-1-1", "2026-13-01", "2026-02-29", "2026-04-31", "26-01-01", "2026/01/01", "2026-01-0a", "+026-01-01", "2026-00-10", "2026-01-00", "２０２６-01-01", "2026-01-011"] {
            assert!(!fecha_valida(mal), "{mal}");
        }
    }

    fn orden_de(fecha: &str, estado: &str, equipo: &str) -> Value {
        json!({"id": "OT-1", "fecha": fecha, "estado": estado, "equipo": equipo, "plan": "P001"})
    }

    #[test]
    fn filtro_de_ordenes_por_estado_equipo_y_fechas() {
        let hoy = "2026-06-15";
        let f = |estado: &'static str, equipo: &'static str, desde: &'static str, hasta: &'static str| FiltroOrdenes { estado, equipo, desde, hasta, hoy };
        let futura = orden_de("2026-07-01", "pendiente", "INV-1");
        let vieja = orden_de("2026-05-01", "pendiente", "INV-2");
        let cerrada = orden_de("2026-05-01", "cerrada", "INV-1");
        assert!(orden_visible(&futura, &f("pendiente", "", "", "")));
        assert!(!orden_visible(&vieja, &f("pendiente", "", "", "")), "una vencida no es pendiente");
        assert!(orden_visible(&vieja, &f("vencida", "", "", "")));
        assert!(!orden_visible(&cerrada, &f("vencida", "", "", "")), "una cerrada nunca está vencida");
        assert!(orden_visible(&cerrada, &f("cerrada", "", "", "")));
        assert!(orden_visible(&cerrada, &f("todas", "", "", "")) && orden_visible(&futura, &f("todas", "", "", "")));
        assert!(orden_visible(&futura, &f("todas", "INV-1", "", "")) && !orden_visible(&vieja, &f("todas", "INV-1", "", "")));
        assert!(orden_visible(&vieja, &f("todas", "", "2026-05-01", "2026-05-31")), "los extremos del rango se incluyen");
        assert!(!orden_visible(&futura, &f("todas", "", "", "2026-06-30")));
        assert!(orden_visible(&futura, &f("todas", "", "basura", "otra basura")), "un rango inválido no filtra");
    }

    #[test]
    fn tipo_de_orden_antiguo() {
        assert_eq!(tipo_orden(&json!({"plan": "CORR"})), "correctiva");
        assert_eq!(tipo_orden(&json!({"plan": "P001"})), "preventiva");
        assert_eq!(tipo_orden(&json!({"plan": "P001", "tipo": "inspección"})), "inspección");
    }

    #[test]
    fn la_huella_cambia_con_el_contenido() {
        let a = json!({"x": 1, "y": [1, 2]});
        assert_eq!(huella(&a), huella(&a.clone()));
        assert_ne!(huella(&a), huella(&json!({"x": 1, "y": [1, 3]})));
    }
}
