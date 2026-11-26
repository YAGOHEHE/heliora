//! Módulo 7: planificador de mantenimiento. Catálogo, planes, órdenes, calendario, historial y AMEF.
//!
//! Los datos viven en el motor: cada edición con significado (crear, borrar, reprogramar, renombrar un
//! código, cambiar una frecuencia) se envía como una operación de `plan_operar`. La interfaz solo edita
//! en el sitio los textos libres que no tienen reglas (descripciones, checklist, campos del AMEF).
use crate::app::Compartido;
use crate::herramientas::{Herramienta, Pagina};
use crate::motor::Motor;
use crate::tema;
use crate::iconos::Icono;
use crate::ui;
use egui::{Color32, Pos2, Rect, Sense, Vec2};
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

/// Acción pedida desde la cinta; se ejecuta al dibujar la página porque necesita el motor.
#[derive(Clone, Copy)]
enum AccionCinta {
    Pedir(Continuar),
    /// Guardar el plan; `true` pide siempre elegir archivo.
    Guardar(bool),
    Informe,
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
    NuevaOrden,
    /// Selecciona la orden en la pestaña Órdenes, con todas visibles.
    AbrirOrden(String),
    Ir(Pestana),
    /// Abre el AMEF filtrado por un tipo de equipo.
    IrAmef(String),
    /// Texto de notas de una orden abierta; no tiene reglas y se guarda en el sitio.
    Notas(String, String),
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
    orden_sel: Option<String>,
    /// La próxima vez que se dibuje la tabla de órdenes, salta a la página de la orden elegida.
    traer_sel: bool,
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
    cinta_pendiente: Option<AccionCinta>,
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
const POR_PAGINA: usize = 20;
const MESES_CORTOS: [&str; 12] = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
const DIAS_CORTOS: [&str; 7] = ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"];
const PESTANAS: [(Pestana, &str); 7] = [
    (Pestana::Analisis, "Análisis"),
    (Pestana::Tablero, "Tablero y calendario"),
    (Pestana::Ordenes, "Órdenes de trabajo"),
    (Pestana::Catalogo, "Catálogo de equipos"),
    (Pestana::Planes, "Planes"),
    (Pestana::Amef, "AMEF"),
    (Pestana::Historial, "Historial"),
];
/// Filtros de la lista de órdenes, en el orden de las fichas: clave y etiqueta.
const FILTROS: [(&str, &str); 6] = [("pendiente", "Pendientes"), ("vencida", "Vencidas"), ("cerrada", "Cerradas"), ("semana", "Esta semana"), ("proximas", "Próximos 30 días"), ("todas", "Todas")];
const ALTO_FICHA: f32 = 84.0;
const ALTO_PIE: f32 = 44.0;
const ALTO_FILA: f32 = 32.0;
/// Alto de la línea del título de pantalla.
const ALTO_TITULO: f32 = 30.0;
const CELDA_DIA: f32 = 24.0;
const AGENDA_MAX: usize = 60;
/// Semanas que muestra el plan por equipo.
const SEMANAS: i64 = 12;

/// Icono de una vista en la cinta y en la pestaña de documento.
fn icono_pestana(p: Pestana) -> Icono {
    match p {
        Pestana::Analisis => Icono::Grafica,
        Pestana::Tablero => Icono::Calendario,
        Pestana::Ordenes => Icono::Orden,
        Pestana::Catalogo => Icono::Equipos,
        Pestana::Planes => Icono::Lista,
        Pestana::Amef => Icono::Amef,
        Pestana::Historial => Icono::Historial,
    }
}

/// Criterios de la lista de órdenes.
struct FiltroOrdenes<'a> {
    /// Clave de `FILTROS`.
    estado: &'a str,
    equipo: &'a str,
    desde: &'a str,
    hasta: &'a str,
    hoy: &'a str,
    /// Lunes y domingo de la semana de `hoy`, y `hoy` más 30 días.
    lunes: &'a str,
    domingo: &'a str,
    limite: &'a str,
}

/// Tipo de una orden: el guardado o, en planes antiguos, correctiva si viene de una falla.
fn tipo_orden(o: &Value) -> &str {
    match o["tipo"].as_str() {
        Some(t) if !t.is_empty() => t,
        _ if o["plan"].as_str() == Some("CORR") => "correctiva",
        _ => "preventiva",
    }
}

/// Días desde 1970-01-01 de una fecha AAAA-MM-DD, o `None` si no es válida.
fn dias_de(t: &str) -> Option<i64> {
    fecha_valida(t).then(|| dias_desde_civil(t[0..4].parse().unwrap_or(0), t[5..7].parse().unwrap_or(1), t[8..10].parse().unwrap_or(1)))
}

fn fecha_de_dias(d: i64) -> String {
    let (y, m, dia) = civil_desde_dias(d);
    fecha(y, m, dia)
}

/// Posición en la semana de un día (lunes = 0).
fn dia_semana(d: i64) -> i64 {
    (d + 3).rem_euclid(7)
}

fn lunes_de(d: i64) -> i64 {
    d - dia_semana(d)
}

/// Lunes y domingo de la semana de `hoy` y `hoy` más 30 días, como fechas AAAA-MM-DD.
fn limites(hoy: &str) -> (String, String, String) {
    let d = dias_de(hoy).unwrap_or(0);
    (fecha_de_dias(lunes_de(d)), fecha_de_dias(lunes_de(d) + 6), fecha_de_dias(d + 30))
}

/// Día y mes cortos de un día: «23 nov».
fn dia_mes(d: i64) -> String {
    let (_, m, dia) = civil_desde_dias(d);
    format!("{dia} {}", MESES_CORTOS[m as usize - 1])
}

/// Semana que empieza en `lunes`: «23–29 nov», o «30 nov–6 dic» si cambia de mes.
fn rango_semana(lunes: i64) -> String {
    let (_, m1, d1) = civil_desde_dias(lunes);
    let (_, m2, d2) = civil_desde_dias(lunes + 6);
    if m1 == m2 {
        format!("{d1}–{d2} {}", MESES_CORTOS[m2 as usize - 1])
    } else {
        format!("{d1} {}–{d2} {}", MESES_CORTOS[m1 as usize - 1], MESES_CORTOS[m2 as usize - 1])
    }
}

/// Día de la semana y fecha corta de un día: «mié 25 nov».
fn dia_corto(d: i64) -> String {
    format!("{} {}", DIAS_CORTOS[dia_semana(d) as usize], dia_mes(d))
}

/// Indica si una orden pasa el filtro de estado (`FILTROS`), equipo y rango de fechas.
fn orden_visible(o: &Value, f: &FiltroOrdenes) -> bool {
    let est = o["estado"].as_str().unwrap_or("pendiente");
    let fec = o["fecha"].as_str().unwrap_or("");
    let abierta = est != "cerrada";
    let ok_estado = match f.estado {
        "pendiente" => abierta,
        "vencida" => abierta && fec < f.hoy,
        "cerrada" => !abierta,
        "semana" => abierta && fec >= f.lunes && fec <= f.domingo,
        "proximas" => abierta && fec >= f.hoy && fec <= f.limite,
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
            orden_sel: None,
            traer_sel: false,
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
            cinta_pendiente: None,
        }
    }

    /// Nombre de la pestaña abierta, para la ruta de la cabecera.
    pub fn nombre_pestana(&self) -> &'static str {
        match self.pestana {
            Pestana::Analisis => "Análisis",
            Pestana::Tablero => "Tablero y calendario",
            Pestana::Ordenes => "Órdenes de trabajo",
            Pestana::Catalogo => "Catálogo de equipos",
            Pestana::Planes => "Planes",
            Pestana::Amef => "AMEF",
            Pestana::Historial => "Historial",
        }
    }

    /// Cinta del módulo: las vistas, el menú del plan, el informe y, en Análisis, las herramientas de análisis.
    pub fn cinta(&mut self, ui: &mut egui::Ui, todas: &[Herramienta]) {
        ui::grupo_cinta(ui, "Herramientas", |ui| {
            for (p, nombre) in PESTANAS {
                if ui::boton_cinta(ui, icono_pestana(p), nombre, self.pestana == p).clicked() {
                    self.pestana = p;
                }
            }
        });
        let plan_abierto = !self.estado.is_null();
        let archivo = self.nombre_archivo();
        let informe_libre = plan_abierto && self.id_informe.is_none();
        ui::grupo_cinta(ui, "Plan", |ui| {
            let menu = ui::boton_cinta(ui, Icono::Carpeta, "Plan ▾", false);
            egui::Popup::menu(&menu).show(|ui| {
                if ui.button("Plan de ejemplo").on_hover_text("Sustituye el plan en pantalla por el de ejemplo").clicked() {
                    self.cinta_pendiente = Some(AccionCinta::Pedir(Continuar::Ejemplo));
                    ui.close();
                }
                if ui.button("Nuevo plan vacío").clicked() {
                    self.cinta_pendiente = Some(AccionCinta::Pedir(Continuar::Vacio));
                    ui.close();
                }
                if ui.button("Abrir plan…").clicked() {
                    self.cinta_pendiente = Some(AccionCinta::Pedir(Continuar::Abrir));
                    ui.close();
                }
                ui.separator();
                if ui.add_enabled(plan_abierto, egui::Button::new("Guardar como…")).clicked() {
                    self.cinta_pendiente = Some(AccionCinta::Guardar(true));
                    ui.close();
                }
            });
            match ui::columna_cinta(ui, &[(Icono::Documento, &archivo, false), (Icono::Guardar, "Guardar", plan_abierto), (Icono::NuevoDocumento, "Plan de ejemplo", true)]) {
                Some(1) => self.cinta_pendiente = Some(AccionCinta::Guardar(false)),
                Some(2) => self.cinta_pendiente = Some(AccionCinta::Pedir(Continuar::Ejemplo)),
                _ => {}
            }
        });
        ui::grupo_cinta(ui, "Informe", |ui| {
            let r = ui::boton_cinta(ui, Icono::Informe, "Informe O&M", false).on_hover_text("Indicadores, hallazgos, órdenes y calendario de 30 días en HTML y PDF");
            if r.clicked() && informe_libre {
                self.cinta_pendiente = Some(AccionCinta::Informe);
            }
        });
        if self.cinta_pendiente.is_some() {
            ui.ctx().request_repaint();
        }
        if self.pestana == Pestana::Analisis {
            self.analisis.cinta(ui, todas);
        }
    }

    /// Órdenes pendientes del plan abierto, para el esquema del proyecto.
    pub fn pendientes(&self) -> Option<u64> {
        self.tablero["pendientes"].as_u64()
    }

    /// Abre la pestaña Órdenes con el filtro de pendientes (el de las próximas órdenes de Inicio).
    /// Abre la pestaña Órdenes con la orden `id` elegida y sin filtros que la oculten.
    pub fn abrir_orden(&mut self, id: &str) {
        self.orden_sel = Some(id.to_string());
        self.f_estado = "todas".into();
        self.f_equipo.clear();
        self.f_desde.clear();
        self.f_hasta.clear();
        self.traer_sel = true;
        self.pestana = Pestana::Ordenes;
    }

    pub fn ver_ordenes_pendientes(&mut self) {
        self.pestana = Pestana::Ordenes;
        self.f_estado = "pendiente".into();
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
        let pal = tema::p();
        match self.cinta_pendiente.take() {
            Some(AccionCinta::Pedir(c)) => self.pedir(c, motor),
            Some(AccionCinta::Guardar(como)) => self.guardar(como),
            Some(AccionCinta::Informe) => self.escribir_informe(motor),
            None => {}
        }
        if !self.mensaje.is_empty() {
            egui::TopBottomPanel::top("aviso_mant").frame(egui::Frame::new()).show_inside(ui, |ui| {
                if ui::aviso_negro(ui, &self.mensaje) {
                    self.mensaje.clear();
                } else if self.mensaje_error {
                    let r = ui.min_rect();
                    ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(4.0, r.height())), 0.0, pal.falla);
                }
            });
        }
        if self.pestana == Pestana::Analisis {
            self.analisis.ui(ui, todas, motor, c);
            self.dialogos(ui.ctx(), motor);
            return;
        }
        let mut acciones = vec![];
        let ocupado = self.id_estado.is_some() || self.id_carga.is_some();
        let plan_abierto = !self.estado.is_null();
        let ordenes: Vec<Value> = self.estado["ordenes"].as_array().cloned().unwrap_or_default();
        if plan_abierto {
            if matches!(self.pestana, Pestana::Tablero | Pestana::Ordenes) {
                egui::SidePanel::right("panel_mant").default_width(360.0).width_range(280.0..=720.0).resizable(true).frame(egui::Frame::new().fill(pal.panel).stroke(egui::Stroke::new(1.0_f32, pal.linea))).show_inside(ui, |ui| {
                    ui.add_enabled_ui(!ocupado, |ui| {
                        if self.pestana == Pestana::Tablero {
                            self.panel_calendario(ui, &ordenes, &mut acciones);
                        } else {
                            self.panel_orden(ui, &ordenes, &mut acciones);
                        }
                    });
                });
            }
            egui::TopBottomPanel::top("doc_mant").frame(egui::Frame::new()).show_inside(ui, |ui| {
                ui::pestanas_documento(ui, &[(icono_pestana(self.pestana), self.nombre_pestana())], &mut 0, false);
            });
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.fondo)).show_inside(ui, |ui| {
            let alto = ui.available_height();
            if !plan_abierto {
                ui::pagina(ui, |ui| {
                    if ocupado {
                        ui.label(egui::RichText::new("Preparando el plan de mantenimiento...").color(pal.texto_2));
                    } else if ui::estado_vacio(ui, "No hay un plan de mantenimiento abierto.", "Plan de ejemplo") {
                        self.pedir(Continuar::Ejemplo, motor);
                    }
                    if !ocupado {
                        ui.vertical_centered(|ui| {
                            if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Nuevo plan vacío", "", true).clicked() {
                                self.pedir(Continuar::Vacio, motor);
                            }
                        });
                    }
                });
                return;
            }
            let disponible = (alto - 2.0 * ui::MARGEN - 1.0 - ALTO_TITULO - ui::HUECO).max(0.0);
            let archivo = self.nombre_archivo();
            let subtitulo = if self.pestana == Pestana::Ordenes { "Mantenimiento · filtradas por estado".to_string() } else { format!("Mantenimiento · plan {archivo}") };
            ui.add_enabled_ui(!ocupado, |ui| {
                egui::ScrollArea::vertical().id_salt("plan_desplazable_1").auto_shrink([false, false]).show(ui, |ui| {
                    ui::pagina(ui, |ui| {
                        ui::titulo_pantalla(ui, self.nombre_pestana(), &subtitulo);
                        match self.pestana {
                            Pestana::Tablero => self.p_tablero(ui, disponible, &ordenes, &mut acciones),
                            Pestana::Ordenes => self.p_ordenes(ui, disponible, &ordenes, &mut acciones),
                            Pestana::Catalogo => self.marco(ui, disponible, "Equipos de la planta", |s, ui| s.p_catalogo(ui, &mut acciones)),
                            Pestana::Planes => self.marco(ui, disponible, "Planes de mantenimiento preventivo", |s, ui| s.p_planes(ui, &mut acciones)),
                            Pestana::Amef => self.marco(ui, disponible, "AMEF por equipo", |s, ui| s.p_amef(ui, &mut acciones)),
                            Pestana::Historial => self.marco(ui, disponible, "Trabajos realizados", |s, ui| s.p_historial(ui, &mut acciones)),
                            Pestana::Analisis => {}
                        }
                    });
                });
            });
        });
        if !acciones.is_empty() {
            ui.ctx().request_repaint();
        }
        self.despachar(acciones, motor);
        self.dialogos(ui.ctx(), motor);
    }

    /// Nombre del archivo del plan, o «sin archivo»; con «•» si hay cambios sin guardar.
    fn nombre_archivo(&self) -> String {
        let nombre = self.ruta.as_ref().and_then(|r| r.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "sin archivo".into());
        if self.sucio {
            format!("{nombre} •")
        } else {
            nombre
        }
    }

    /// Tarjeta de ancho completo y alto `alto` con el título dado, para las pestañas de edición.
    fn marco(&mut self, ui: &mut egui::Ui, alto: f32, titulo: &str, cuerpo: impl FnOnce(&mut Self, &mut egui::Ui)) {
        let mut cuerpo = Some(cuerpo);
        ui::fila(ui, &[12], alto, |_, ui| {
            ui::tarjeta_con_titulo(
                ui,
                titulo,
                |_| {},
                |ui| {
                    if let Some(f) = cuerpo.take() {
                        f(self, ui);
                    }
                },
            );
        });
    }

    /// Pide al motor el informe O&M en la carpeta que elija la persona.
    fn escribir_informe(&mut self, motor: &mut Motor) {
        if self.id_informe.is_some() || self.estado.is_null() {
            return;
        }
        if let Some(c) = rfd::FileDialog::new().set_title("Carpeta para el informe").pick_folder() {
            self.id_informe = Some(motor.llamar("informe_om", json!({"estado": self.estado, "carpeta": c.display().to_string(), "hoy": self.hoy})));
            self.avisar("Escribiendo el informe...", false);
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
                Accion::AbrirOrden(id) => self.abrir_orden(&id),
                Accion::Ir(p) => self.pestana = p,
                Accion::IrAmef(t) => {
                    self.filtro_amef = t;
                    self.pestana = Pestana::Amef;
                }
                Accion::Notas(id, t) => self.guardar_notas(&id, t),
                Accion::NuevaOrden => {
                    let equipo = self.codigos().into_iter().next().unwrap_or_default();
                    self.nueva = Some(NuevaOrden { equipo, trabajo: String::new(), fecha: self.hoy.clone(), tipo: "correctiva".into(), responsable: String::new() });
                }
                _ => {}
            }
        }
    }

    /// Guarda las notas de una orden abierta en el sitio.
    fn guardar_notas(&mut self, id: &str, t: String) {
        if let Some(o) = self.estado["ordenes"].as_array_mut().and_then(|a| a.iter_mut().find(|o| o["id"].as_str() == Some(id))) {
            o["notas"] = json!(t);
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

    /// Pestaña Tablero: fichas y plan por equipo en 12 semanas.
    fn p_tablero(&mut self, ui: &mut egui::Ui, disponible: f32, ordenes: &[Value], acciones: &mut Vec<Accion>) {
        let hoy = dias_de(&self.hoy).unwrap_or(0);
        let lunes = lunes_de(hoy);
        let (desde, hasta, _) = limites(&self.hoy);
        let esta_semana = ordenes.iter().filter(|o| texto(o, "estado") != "cerrada" && texto(o, "fecha") >= desde.as_str() && texto(o, "fecha") <= hasta.as_str()).count();
        let t = &self.tablero;
        let num = |k: &str| t.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
        let cump = t.get("cumplimiento").and_then(|v| v.as_f64()).unwrap_or(0.0);
        // Sin órdenes cerradas ni vencidas todavía no hay cumplimiento que medir
        let sin_cumplimiento = num("cerradas") + num("vencidas") == 0;
        let fichas: [(&str, String, &str, String); 6] = [
            ("Vencidas", num("vencidas").to_string(), "", if num("vencidas") == 0 { "al día".into() } else { "sin cerrar y con fecha pasada".into() }),
            ("Esta semana", esta_semana.to_string(), "", rango_semana(lunes)),
            ("Próximos 30 días", num("proximas_30").to_string(), "", String::new()),
            ("Pendientes", num("pendientes").to_string(), "", String::new()),
            ("Cerradas", num("cerradas").to_string(), "", String::new()),
            if sin_cumplimiento { ("Cumplimiento", "—".into(), "", "sin cierres aún".into()) } else { ("Cumplimiento", format!("{:.0}", cump * 100.0), "%", "cerradas sobre cerradas y vencidas".into()) },
        ];
        ui::fila(ui, &[2; 6], ALTO_FICHA, |i, ui| {
            let (e, v, u, n) = &fichas[i];
            ui::ficha_kpi(ui, e, v, u, n, None);
        });
        let alto = ui::altos(disponible, ALTO_FICHA, &[1.0])[0];
        ui::fila(ui, &[12], alto, |_, ui| self.plan_por_equipo(ui, ordenes, lunes, hoy, acciones));
    }

    /// Tarjeta «Plan por equipo»: controles de generación y cuadrícula de equipos por semana.
    fn plan_por_equipo(&mut self, ui: &mut egui::Ui, ordenes: &[Value], lunes: i64, hoy: i64, acciones: &mut Vec<Accion>) {
        let pal = tema::p();
        let equipos: Vec<Value> = self.estado["equipos"].as_array().cloned().unwrap_or_default();
        let hay_otras = ordenes.iter().any(|o| tema::color_tipo_orden(texto(o, "nombre")) == tema::color_tipo_orden("otro"));
        ui::tarjeta_plena(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 10 }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_height(28.0);
                    ui.label(egui::RichText::new("Plan por equipo").family(tema::semi()).size(15.0));
                    ui.label(egui::RichText::new(format!("{SEMANAS} semanas")).size(12.0).color(pal.texto_2));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        let mut leyenda = vec![("anual", "Mant. anual"), ("ensayo", "Ensayo"), ("inspección", "Inspección"), ("limpieza", "Limpieza")];
                        if hay_otras {
                            leyenda.insert(0, ("otro", "Otras"));
                        }
                        for (tipo, nombre) in leyenda {
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(nombre).size(12.0));
                            ui::marca_tipo(ui, tipo);
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Generar órdenes hasta").size(12.5));
                    let valida = fecha_libre(ui, &mut self.hasta, 100.0, false);
                    ui.add_enabled_ui(valida, |ui| {
                        if ui::boton(ui, ui::EstiloBoton::Principal, None, "Generar", "", true).clicked() {
                            acciones.push(Accion::Generar);
                        }
                    });
                    if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Crear correctivas desde fallas", "", true).on_hover_text("Usa los hallazgos de «Detección de fallas»; no repite una correctiva abierta").clicked() {
                        acciones.push(Accion::Correctivas);
                    }
                });
            });
            tema::separador(ui);
            if equipos.is_empty() {
                if ui::estado_vacio(ui, "El catálogo no tiene equipos.", "Ir al catálogo") {
                    acciones.push(Accion::Ir(Pestana::Catalogo));
                }
                return;
            }
            cuadricula_semanal(ui, &equipos, ordenes, lunes, hoy, acciones);
        });
    }

    /// Pie de un panel lateral: línea superior y botones alineados a la derecha.
    fn pie_panel(ui: &mut egui::Ui, id: &str, botones: impl FnOnce(&mut egui::Ui)) {
        egui::TopBottomPanel::bottom(id.to_string()).exact_height(64.0).frame(egui::Frame::new()).show_inside(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment([r.left_top(), r.right_top()], egui::Stroke::new(1.0_f32, tema::p().linea));
            let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(Vec2::new(20.0, 14.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
            hijo.spacing_mut().item_spacing.x = 10.0;
            botones(&mut hijo);
        });
    }

    /// Panel derecho del Tablero: calendario del mes, agenda y el botón «Nueva orden».
    fn panel_calendario(&mut self, ui: &mut egui::Ui, ordenes: &[Value], acciones: &mut Vec<Accion>) {
        let pal = tema::p();
        Self::pie_panel(ui, "pie_calendario", |ui| {
            if ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Nuevo), "Nueva orden", "", false).clicked() {
                acciones.push(Accion::NuevaOrden);
            }
        });
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 18))).show_inside(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(egui::RichText::new("Calendario").font(tema::fuente_negrita(20.0)));
            self.calendario_mes(ui, ordenes);
            ui.add_space(8.0);
            tema::separador(ui);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Agenda").font(tema::fuente_negrita(20.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.link(egui::RichText::new("Órdenes →").family(tema::semi()).size(13.0).color(pal.texto)).clicked() {
                        acciones.push(Accion::Ir(Pestana::Ordenes));
                    }
                });
            });
            self.agenda(ui, ordenes, acciones);
        });
    }

    /// Mes en curso: título con navegación, días de la semana y rejilla de días; marca hoy, el día elegido y los días con órdenes.
    fn calendario_mes(&mut self, ui: &mut egui::Ui, ordenes: &[Value]) {
        let pal = tema::p();
        let ancho = ui.available_width();
        let (cab, _) = ui.allocate_exact_size(Vec2::new(ancho, 30.0), Sense::hover());
        ui.painter().text(cab.center(), egui::Align2::CENTER_CENTER, format!("{} de {}", MESES[self.mes as usize - 1], self.anio), tema::fuente_semi(14.0), pal.texto);
        let boton = |ui: &mut egui::Ui, r: Rect, i: Icono, ayuda: &str| ui::boton_icono(&mut ui.new_child(egui::UiBuilder::new().max_rect(r)), i, 16.0, ayuda).clicked();
        if boton(ui, Rect::from_min_size(cab.left_top(), Vec2::splat(30.0)), Icono::Anterior, "Mes anterior") {
            (self.anio, self.mes) = if self.mes == 1 { (self.anio - 1, 12) } else { (self.anio, self.mes - 1) };
        }
        if boton(ui, Rect::from_min_size(Pos2::new(cab.right() - 92.0, cab.top()), Vec2::splat(30.0)), Icono::Siguiente, "Mes siguiente") {
            (self.anio, self.mes) = if self.mes == 12 { (self.anio + 1, 1) } else { (self.anio, self.mes + 1) };
        }
        let mut zona_hoy = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(Pos2::new(cab.right() - 60.0, cab.top()), cab.right_bottom())).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if ui::boton(&mut zona_hoy, ui::EstiloBoton::Secundario, None, "Hoy", "", true).clicked() {
            let (y, m, _) = hoy_civil();
            self.anio = y;
            self.mes = m;
            self.dia_sel = None;
        }
        let dias = dias_mes(self.anio, self.mes);
        let primero = ((dias_desde_civil(self.anio, self.mes, 1) + 3).rem_euclid(7)) as u32;
        let filas = (primero + dias).div_ceil(7);
        let paso = CELDA_DIA + 2.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ancho, 22.0 + paso * filas as f32), Sense::hover());
        let col = ancho / 7.0;
        for (i, d) in ["L", "M", "X", "J", "V", "S", "D"].iter().enumerate() {
            ui.painter().text(rect.left_top() + Vec2::new(col * (i as f32 + 0.5), 10.0), egui::Align2::CENTER_CENTER, *d, egui::FontId::proportional(11.0), pal.texto_2);
        }
        let con_ordenes: std::collections::HashSet<&str> = ordenes.iter().filter(|o| texto(o, "estado") != "cerrada").map(|o| texto(o, "fecha")).collect();
        for dia in 1..=dias {
            let idx = primero + dia - 1;
            let celda = Rect::from_min_size(rect.left_top() + Vec2::new((idx % 7) as f32 * col, 22.0 + (idx / 7) as f32 * paso), Vec2::new(col, CELDA_DIA)).shrink2(Vec2::new(1.0, 0.0));
            let f = fecha(self.anio, self.mes, dia);
            let resp = ui.interact(celda, ui.id().with(("dia", &f)), Sense::click());
            let es_hoy = f == self.hoy;
            let elegido = self.dia_sel.as_deref() == Some(f.as_str());
            let fondo = if es_hoy {
                Some(pal.acento)
            } else if elegido {
                Some(pal.acento_fondo)
            } else if resp.hovered() {
                Some(pal.realce)
            } else {
                None
            };
            if let Some(c) = fondo {
                ui.painter().rect_filled(celda, tema::RADIO_FICHA, c);
            }
            if elegido && !es_hoy {
                ui.painter().rect_stroke(celda, tema::RADIO_FICHA, egui::Stroke::new(1.0_f32, pal.acento), egui::StrokeKind::Inside);
            }
            let color = if es_hoy { pal.sobre_acento } else { pal.texto };
            ui.painter().text(celda.center() - Vec2::new(0.0, 1.0), egui::Align2::CENTER_CENTER, dia.to_string(), tema::fuente_mono(12.0), color);
            if con_ordenes.contains(f.as_str()) {
                ui.painter().rect_filled(Rect::from_center_size(Pos2::new(celda.center().x, celda.bottom() - 4.0), Vec2::splat(3.0)), 0.0, color);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.dia_sel = Some(f);
            }
        }
    }

    /// Órdenes abiertas desde el día elegido (o desde hoy) agrupadas por día.
    fn agenda(&mut self, ui: &mut egui::Ui, ordenes: &[Value], acciones: &mut Vec<Accion>) {
        let pal = tema::p();
        let desde = self.dia_sel.clone().unwrap_or_else(|| self.hoy.clone());
        let mut lista: Vec<&Value> = ordenes.iter().filter(|o| texto(o, "estado") != "cerrada" && texto(o, "fecha") >= desde.as_str()).collect();
        lista.sort_by(|a, b| (texto(a, "fecha"), texto(a, "id")).cmp(&(texto(b, "fecha"), texto(b, "id"))));
        lista.truncate(AGENDA_MAX);
        if lista.is_empty() {
            ui.label(egui::RichText::new("Sin órdenes pendientes desde esta fecha.").size(12.5).color(pal.texto_2));
            return;
        }
        egui::ScrollArea::vertical().id_salt("plan_desplazable_2").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut dia_actual = "";
            for o in lista {
                let f = texto(o, "fecha");
                if f != dia_actual {
                    let primero = dia_actual.is_empty();
                    dia_actual = f;
                    let d = dias_de(f).unwrap_or(0);
                    let t = if f == self.hoy { format!("hoy · {}", dia_corto(d)) } else { dia_corto(d) };
                    ui.add_space(if primero { 6.0 } else { 12.0 });
                    ui::titulo_seccion(ui, &t);
                    ui.add_space(4.0);
                }
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
                if ui.rect_contains_pointer(r) {
                    ui.painter().rect_filled(r, 0.0, pal.realce);
                }
                ui::fondo_fila(ui, r, 0);
                ui::dibujar_muestra(ui.painter(), Rect::from_center_size(Pos2::new(r.left() + 7.0, r.center().y), Vec2::splat(14.0)), color_orden(o), ui::muestra_tipo(texto(o, "nombre")));
                let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(Vec2::new(0.0, 4.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
                let chip = ui::chip(&mut hijo, texto(o, "equipo"), false);
                let limite = chip.rect.left() - 8.0;
                ui.painter().with_clip_rect(Rect::from_min_max(Pos2::new(r.left() + 20.0, r.top()), Pos2::new(limite, r.bottom()))).text(
                    Pos2::new(r.left() + 20.0, r.center().y),
                    egui::Align2::LEFT_CENTER,
                    texto(o, "nombre"),
                    egui::FontId::proportional(13.0),
                    pal.texto,
                );
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() || chip.clicked() {
                    acciones.push(Accion::AbrirOrden(texto(o, "id").to_string()));
                }
            }
        });
    }

    /// Pestaña Órdenes: fichas que filtran y tabla agrupada por semana con paginación.
    fn p_ordenes(&mut self, ui: &mut egui::Ui, disponible: f32, ordenes: &[Value], acciones: &mut Vec<Accion>) {
        let hoy = dias_de(&self.hoy).unwrap_or(0);
        let (lunes, domingo, limite) = limites(&self.hoy);
        let base = |estado: &'static str| FiltroOrdenes { estado, equipo: "", desde: "", hasta: "", hoy: &self.hoy, lunes: &lunes, domingo: &domingo, limite: &limite };
        let fichas: Vec<(&'static str, &str, usize)> = FILTROS.iter().map(|(clave, etq)| (*clave, *etq, ordenes.iter().filter(|o| orden_visible(o, &base(clave))).count())).collect();
        let mut elegido = None;
        ui::fila(ui, &[2; 6], ALTO_FICHA, |i, ui| {
            let (clave, etq, n) = fichas[i];
            let nota = match clave {
                "vencida" if n == 0 => "al día".to_string(),
                "semana" => rango_semana(lunes_de(hoy)),
                _ => String::new(),
            };
            if ui::ficha_kpi(ui, etq, &n.to_string(), "", &nota, Some(self.f_estado == clave)).clicked() {
                elegido = Some(clave);
            }
        });
        if let Some(c) = elegido {
            self.f_estado = c.into();
            self.pagina = 0;
        }
        let alto = ui::altos(disponible, ALTO_FICHA, &[1.0])[0];
        let codigos = self.codigos();
        let filtro = FiltroOrdenes { estado: &self.f_estado, equipo: &self.f_equipo, desde: &self.f_desde, hasta: &self.f_hasta, hoy: &self.hoy, lunes: &lunes, domingo: &domingo, limite: &limite };
        let mut visibles: Vec<&Value> = ordenes.iter().filter(|o| orden_visible(o, &filtro)).collect();
        visibles.sort_by(|a, b| (texto(a, "fecha"), texto(a, "id")).cmp(&(texto(b, "fecha"), texto(b, "id"))));
        let n_visibles = visibles.len();
        let paginas = n_visibles.div_ceil(POR_PAGINA).max(1);
        if std::mem::take(&mut self.traer_sel) {
            if let Some(i) = visibles.iter().position(|o| Some(texto(o, "id")) == self.orden_sel.as_deref()) {
                self.pagina = i / POR_PAGINA;
            }
        }
        self.pagina = self.pagina.min(paginas - 1);
        if self.orden_sel.as_deref().is_none_or(|id| !ordenes.iter().any(|o| texto(o, "id") == id)) {
            self.orden_sel = visibles.first().map(|o| texto(o, "id").to_string());
            ui.ctx().request_repaint();
        }
        let pagina: Vec<&Value> = visibles.iter().skip(self.pagina * POR_PAGINA).take(POR_PAGINA).copied().collect();
        let titulo = FILTROS.iter().find(|(c, _)| *c == self.f_estado).map(|(_, e)| *e).unwrap_or("Órdenes");
        ui::fila(ui, &[12], alto, |_, ui| {
            self.tabla_ordenes(ui, titulo, &pagina, n_visibles, paginas, &codigos, acciones);
        });
    }

    /// Tarjeta con la lista de órdenes de la página actual, sus filtros en la cabecera y la paginación al pie.
    #[allow(clippy::too_many_arguments)]
    fn tabla_ordenes(&mut self, ui: &mut egui::Ui, titulo: &str, pagina: &[&Value], total: usize, paginas: usize, codigos: &[String], acciones: &mut Vec<Accion>) {
        let pal = tema::p();
        ui::tarjeta_plena(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 8, bottom: 8 }).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.set_min_height(28.0);
                    ui.label(egui::RichText::new(titulo).family(tema::semi()).size(15.0));
                    ui.label(egui::RichText::new(format!("{total} órdenes")).size(12.0).color(pal.texto_2));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Nuevo), "Nueva orden", "", true).clicked() {
                            acciones.push(Accion::NuevaOrden);
                        }
                        let mut cambio = fecha_filtro(ui, "Hasta", &mut self.f_hasta);
                        cambio |= fecha_filtro(ui, "Desde", &mut self.f_desde);
                        let antes = self.f_equipo.clone();
                        egui::ComboBox::from_id_salt("f_equipo_ord").selected_text(if self.f_equipo.is_empty() { "Todos" } else { &self.f_equipo }).width(84.0).show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.f_equipo, String::new(), "Todos");
                            for c in codigos {
                                ui.selectable_value(&mut self.f_equipo, c.clone(), c);
                            }
                        });
                        ui.label(egui::RichText::new("Equipo").size(12.0).color(pal.texto_2));
                        if cambio || antes != self.f_equipo {
                            self.pagina = 0;
                        }
                        if (!self.f_equipo.is_empty() || !self.f_desde.is_empty() || !self.f_hasta.is_empty()) && ui.small_button("Limpiar").on_hover_text("Quita los filtros de equipo y fechas").clicked() {
                            self.f_equipo.clear();
                            self.f_desde.clear();
                            self.f_hasta.clear();
                            self.pagina = 0;
                        }
                    });
                });
            });
            let ancho = ui.available_width();
            let cols = columnas_tabla(ancho);
            let (cab, _) = ui.allocate_exact_size(Vec2::new(ancho, 30.0), Sense::hover());
            ui.painter().rect_filled(cab, 0.0, pal.campo);
            ui.painter().line_segment([cab.left_top(), cab.right_top()], egui::Stroke::new(1.0_f32, pal.linea));
            ui.painter().line_segment([cab.left_bottom(), cab.right_bottom()], egui::Stroke::new(1.0_f32, pal.borde));
            for (x, t) in cols.iter().zip(["Fecha", "OT", "Tarea", "Equipo", "Tipo", "Estado"]) {
                ui.painter().text(Pos2::new(cab.left() + x.0, cab.center().y), egui::Align2::LEFT_CENTER, t, tema::fuente_semi(12.5), pal.texto);
            }
            let alto_cuerpo = (ui.available_height() - ALTO_PIE).max(40.0);
            egui::ScrollArea::vertical().id_salt("plan_desplazable_3").auto_shrink([false, false]).max_height(alto_cuerpo).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if pagina.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| ui.label(egui::RichText::new("No hay órdenes en esta vista.").color(pal.texto_2)));
                    return;
                }
                let mut semana = i64::MIN;
                for o in pagina {
                    let d = dias_de(texto(o, "fecha")).unwrap_or(0);
                    if lunes_de(d) != semana {
                        semana = lunes_de(d);
                        let (r, _) = ui.allocate_exact_size(Vec2::new(ancho, 30.0), Sense::hover());
                        ui.painter().rect_filled(r, 0.0, pal.campo_alt);
                        ui::fondo_fila(ui, r, 0);
                        ui.painter().text(Pos2::new(r.left() + cols[0].0, r.center().y), egui::Align2::LEFT_CENTER, ui::mayusculas(&format!("Semana del {}", dia_mes(semana))), tema::fuente_semi(12.0), pal.texto_2);
                    }
                    let elegida = self.orden_sel.as_deref() == Some(texto(o, "id"));
                    if fila_orden(ui, o, &self.hoy, ancho, &cols, elegida) {
                        self.orden_sel = Some(texto(o, "id").to_string());
                        ui.ctx().request_repaint();
                    }
                }
            });
            let (pie, _) = ui.allocate_exact_size(Vec2::new(ancho, ALTO_PIE), Sense::hover());
            ui.painter().line_segment([pie.left_top(), pie.right_top()], egui::Stroke::new(1.0_f32, pal.linea));
            let primera = if total == 0 { 0 } else { self.pagina * POR_PAGINA + 1 };
            ui.painter().text(Pos2::new(pie.left() + 16.0, pie.center().y), egui::Align2::LEFT_CENTER, format!("{primera}–{} de {total}", self.pagina * POR_PAGINA + pagina.len()), egui::FontId::proportional(12.0), pal.texto_2);
            let mut pie_ui = ui.new_child(egui::UiBuilder::new().max_rect(pie.shrink2(Vec2::new(16.0, 6.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
            if pie_ui.add_enabled_ui(self.pagina + 1 < paginas, |ui| ui::boton_icono(ui, Icono::Siguiente, 16.0, "Página siguiente").clicked()).inner {
                self.pagina += 1;
            }
            pie_ui.label(egui::RichText::new(format!("{} / {paginas}", self.pagina + 1)).family(tema::semi()).size(12.0));
            if pie_ui.add_enabled_ui(self.pagina > 0, |ui| ui::boton_icono(ui, Icono::Anterior, 16.0, "Página anterior").clicked()).inner {
                self.pagina -= 1;
            }
        });
    }

    /// Panel derecho de Órdenes: detalle de la orden elegida con sus datos editables, notas, enlaces y acciones.
    fn panel_orden(&self, ui: &mut egui::Ui, ordenes: &[Value], acciones: &mut Vec<Accion>) {
        let pal = tema::p();
        let codigos = self.codigos();
        let orden = self.orden_sel.as_deref().and_then(|id| ordenes.iter().find(|o| texto(o, "id") == id));
        let Some(o) = orden else {
            egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 18))).show_inside(ui, |ui| {
                ui.label(egui::RichText::new("Orden de trabajo").font(tema::fuente_negrita(20.0)));
                ui::estado_vacio(ui, "Elige una orden de la tabla para ver su detalle.", "");
            });
            return;
        };
        let id = texto(o, "id").to_string();
        let fec = texto(o, "fecha");
        let cerrada = texto(o, "estado") == "cerrada";
        let (estado, color_estado) = estado_orden(o, &self.hoy);
        let clave_cierre = egui::Id::new(("o_cierre", &id));
        let clave_fecha = egui::Id::new(("o_fecha", &id, fec));
        let cierre = ui.ctx().data(|d| d.get_temp::<String>(clave_cierre)).unwrap_or_else(|| self.hoy.clone());
        let nueva = ui.ctx().data(|d| d.get_temp::<String>(clave_fecha)).unwrap_or_else(|| fec.to_string());
        let mut boton: Option<Accion> = None;
        Self::pie_panel(ui, "pie_orden", |ui| {
            if cerrada {
                if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Reabrir", "", false).clicked() {
                    boton = Some(Accion::Operar("orden_reabrir", json!({"ident": id})));
                }
                return;
            }
            ui.add_enabled_ui(fecha_valida(&cierre), |ui| {
                if ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Correcto), "Cerrar orden", "", false).clicked() {
                    boton = Some(Accion::Operar("orden_cerrar", json!({"ident": id, "fecha": cierre})));
                }
            });
            ui.add_enabled_ui(fecha_valida(&nueva) && nueva != fec, |ui| {
                if ui::boton(ui, ui::EstiloBoton::Secundario, Some(Icono::Calendario), "Reprogramar", "", false).on_hover_text("Cambia la fecha prevista por la de «Fecha prevista»").clicked() {
                    boton = Some(Accion::Operar("orden_editar", json!({"ident": id, "fecha": nueva})));
                }
            });
        });
        let mut edicion: Option<Accion> = None;
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(20, 18))).show_inside(ui, |ui| {
            ui.label(egui::RichText::new("Orden de trabajo").font(tema::fuente_negrita(20.0)));
            egui::ScrollArea::vertical().id_salt("plan_desplazable_4").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&id).font(tema::fuente_mono(13.0)));
                    chip_estado(ui, estado, color_estado);
                });
                ui.label(egui::RichText::new(texto(o, "nombre")).family(tema::semi()).size(18.0));
                ui.label(egui::RichText::new(format!("{} · {}", texto(o, "equipo"), tipo_orden(o))).size(13.0).color(pal.texto_2));
                ui.add_space(8.0);
                let mono = |ui: &mut egui::Ui, t: &str| ui.label(egui::RichText::new(t).font(tema::fuente_mono(12.0)));
                fila_dato(ui, "Fecha prevista", |ui| {
                    if cerrada {
                        mono(ui, fec);
                    } else {
                        let mut f = nueva.clone();
                        fecha_libre(ui, &mut f, 100.0, false);
                        ui.data_mut(|d| d.insert_temp(clave_fecha, f));
                    }
                });
                fila_dato(ui, "Siguiente", |ui| {
                    match siguiente(ordenes, &self.estado["planes"], o) {
                        Some(s) => mono(ui, &format!("{} · {}", texto(s, "fecha"), texto(s, "id"))),
                        None => mono(ui, "—"),
                    };
                });
                fila_dato(ui, "Periodicidad", |ui| {
                    match self.estado["planes"].as_array().and_then(|p| p.iter().find(|p| texto(p, "id") == texto(o, "plan"))) {
                        Some(p) => mono(ui, &format!("cada {} días", p["frecuencia_dias"].as_i64().unwrap_or(0))),
                        None => ui.label(egui::RichText::new("sin plan").color(pal.texto_2)),
                    };
                });
                fila_dato(ui, "Estado", |ui| {
                    let mut e = estado.to_string();
                    e[..1].make_ascii_uppercase();
                    ui.label(e);
                });
                if cerrada {
                    fila_dato(ui, "Cerrada el", |ui| {
                        mono(ui, texto(o, "cerrada"));
                    });
                }
                fila_dato(ui, "Responsable", |ui| {
                    if let Some(t) = texto_diferido(ui, ("o_resp", &id), texto(o, "responsable"), 170.0, 1) {
                        edicion = Some(Accion::Operar("orden_editar", json!({"ident": id, "responsable": t})));
                    }
                });
                if !cerrada {
                    fila_dato(ui, "Trabajo", |ui| {
                        if let Some(t) = texto_diferido(ui, ("o_trabajo", &id), texto(o, "nombre"), 170.0, 1) {
                            edicion = Some(Accion::Operar("orden_editar", json!({"ident": id, "trabajo": t})));
                        }
                    });
                    fila_dato(ui, "Equipo", |ui| {
                        let mut eq = texto(o, "equipo").to_string();
                        if combo_texto(ui, ("o_equipo", &id), &mut eq, &codigos, 110.0) {
                            edicion = Some(Accion::Operar("orden_editar", json!({"ident": id, "equipo": eq})));
                        }
                    });
                    fila_dato(ui, "Tipo", |ui| {
                        let mut tipo = tipo_orden(o).to_string();
                        if combo_texto(ui, ("o_tipo", &id), &mut tipo, &TIPOS_ORDEN.map(String::from), 110.0) {
                            edicion = Some(Accion::Operar("orden_editar", json!({"ident": id, "tipo": tipo})));
                        }
                    });
                    fila_dato(ui, "Cerrar con fecha", |ui| {
                        let mut f = cierre.clone();
                        fecha_libre(ui, &mut f, 100.0, false);
                        ui.data_mut(|d| d.insert_temp(clave_cierre, f));
                    });
                }
                ui.add_space(10.0);
                ui::titulo_seccion(ui, "Notas de cierre");
                let mut notas = texto(o, "notas").to_string();
                if cerrada {
                    if let Some(t) = texto_diferido(ui, ("o_notas", &id), &notas, ui.available_width(), 5) {
                        edicion = Some(Accion::Operar("orden_editar", json!({"ident": id, "notas": t})));
                    }
                } else if ui.add(egui::TextEdit::multiline(&mut notas).desired_rows(4).desired_width(f32::INFINITY).hint_text("Trabajo realizado, materiales, incidencias…")).changed() {
                    acciones.push(Accion::Notas(id.clone(), notas));
                }
                ui.add_space(10.0);
                ui::titulo_seccion(ui, "Relacionado");
                let equipo = texto(o, "equipo");
                if ui.link(format!("Equipo {equipo} en el catálogo →")).clicked() {
                    acciones.push(Accion::Ir(Pestana::Catalogo));
                }
                let tipo_equipo = self.estado["equipos"].as_array().and_then(|e| e.iter().find(|e| texto(e, "codigo") == equipo)).map(|e| texto(e, "tipo").to_string()).unwrap_or_default();
                if ui.add_enabled(!tipo_equipo.is_empty(), egui::Link::new(format!("Modos de falla de {tipo_equipo} en el AMEF →"))).clicked() {
                    acciones.push(Accion::IrAmef(tipo_equipo));
                }
                ui.add_enabled(false, egui::Link::new("Bloque del diagrama →")).on_disabled_hover_text("El plan no guarda qué bloque del diagrama corresponde a cada equipo, así que no se puede enlazar.");
                if let Some(ch) = o["checklist"].as_array().filter(|c| !c.is_empty()) {
                    ui.add_space(10.0);
                    ui::titulo_seccion(ui, "Lista de comprobación");
                    for (i, c) in ch.iter().enumerate() {
                        let mut h = c["hecho"].as_bool().unwrap_or(false);
                        if ui.checkbox(&mut h, texto(c, "texto")).changed() {
                            acciones.push(Accion::Hecho(id.clone(), i, h));
                        }
                    }
                }
                ui.add_space(10.0);
                if ui::boton_texto(ui, Some(Icono::Borrar), "Borrar orden").clicked() {
                    boton = Some(Accion::Confirmar(Confirmar::BorrarOrden(id.clone())));
                }
            });
        });
        if let Some(a) = boton.or(edicion) {
            acciones.push(a);
        }
    }

    fn p_catalogo(&mut self, ui: &mut egui::Ui, acciones: &mut Vec<Accion>) {
        ui.horizontal(|ui| {
            if ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Mas), "Añadir equipo", "", true).clicked() {
                acciones.push(Accion::Operar("equipo_crear", json!({})));
            }
        });
        let pendientes: Vec<usize> = self.estado["equipos"].as_array().map(|a| a.iter().map(|e| self.pendientes_de("equipo", texto(e, "codigo"))).collect()).unwrap_or_default();
        egui::ScrollArea::vertical().id_salt("plan_desplazable_5").auto_shrink([false, false]).show(ui, |ui| {
            let Some(eq) = self.estado["equipos"].as_array_mut() else { return };
            egui::Grid::new("g_eq").striped(true).num_columns(6).spacing([10.0, 5.0]).show(ui, |ui| {
                for t in ["Código", "Tipo", "Descripción", "Ubicación", "Instalado", ""] {
                    ui.label(egui::RichText::new(t).family(tema::semi()).size(12.5).color(tema::p().texto));
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
            if ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Mas), "Añadir plan", "", true).clicked() {
                acciones.push(Accion::Operar("plan_crear", json!({"nombre": "Plan nuevo", "tipo_equipo": "Módulos", "frecuencia_dias": 90, "checklist": ["Paso 1"]})));
            }
        });
        if let Some(id) = self.regenerar.clone() {
            tarjeta_plana(ui, |ui| {
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
        egui::ScrollArea::vertical().id_salt("plan_desplazable_6").auto_shrink([false, false]).show(ui, |ui| {
            let Some(planes) = self.estado["planes"].as_array_mut() else { return };
            for (i, p) in planes.iter_mut().enumerate() {
                let id = texto(p, "id").to_string();
                tarjeta_plana(ui, |ui| {
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
            egui::ComboBox::from_id_salt("famef").selected_text(if self.filtro_amef.is_empty() { "Todos los equipos" } else { &self.filtro_amef }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.filtro_amef, String::new(), "Todos los equipos");
                for t in TIPOS {
                    ui.selectable_value(&mut self.filtro_amef, t.to_string(), t);
                }
            });
            if ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Mas), "Añadir modo de falla", "", true).clicked() {
                let t = if self.filtro_amef.is_empty() { "Módulos".to_string() } else { self.filtro_amef.clone() };
                acciones.push(Accion::Operar("amef_crear", json!({"tipo_equipo": t})));
            }
            if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Ordenar por RPN", "", true).on_hover_text("Reordena la tabla de mayor a menor RPN; las ediciones no la reordenan").clicked() {
                acciones.push(Accion::Operar("amef_ordenar", json!({})));
            }
            if ui::boton(ui, ui::EstiloBoton::Secundario, None, "Exportar CSV", "", true).on_hover_text("Exporta las filas que se ven, con «;» y UTF-8 para Excel").clicked() {
                acciones.push(Accion::Exportar("amef", self.filtro_amef.clone()));
            }
        });
        ui.label(egui::RichText::new("RPN = S·O·D, con severidad, ocurrencia y detección de 1 a 10. Más alto significa actuar antes.").small().color(tema::p().texto_2));
        let filtro = self.filtro_amef.clone();
        egui::ScrollArea::both().id_salt("plan_desplazable_7").auto_shrink([false, false]).show(ui, |ui| {
            let Some(am) = self.estado["amef"].as_array_mut() else { return };
            let rpn = |a: &Value| a["S"].as_i64().unwrap_or(1) * a["O"].as_i64().unwrap_or(1) * a["D"].as_i64().unwrap_or(1);
            egui::Grid::new("g_amef").striped(true).num_columns(11).spacing([8.0, 5.0]).show(ui, |ui| {
                for t in ["Equipo", "Modo de falla", "Causa", "Efecto", "S", "O", "D", "RPN", "Control", "Acción", ""] {
                    ui.label(egui::RichText::new(t).family(tema::semi()).size(12.5).color(tema::p().texto));
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
            egui::ComboBox::from_id_salt("f_hist").selected_text(if self.f_historial.is_empty() { "Todos los equipos" } else { &self.f_historial }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.f_historial, String::new(), "Todos los equipos");
                for e in &equipos {
                    ui.selectable_value(&mut self.f_historial, e.clone(), e);
                }
            });
            if ui.add_enabled_ui(!h.is_empty(), |ui| ui::boton(ui, ui::EstiloBoton::Secundario, None, "Exportar CSV", "", true)).inner.on_hover_text("Exporta las filas que se ven, con «;» y UTF-8 para Excel").clicked() {
                acciones.push(Accion::Exportar("historial", self.f_historial.clone()));
            }
        });
        if h.is_empty() {
            ui.label(egui::RichText::new("Aún no hay trabajos cerrados. Se registran al cerrar una orden.").color(tema::p().texto_2));
            return;
        }
        egui::ScrollArea::both().id_salt("plan_desplazable_8").auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("g_hist").striped(true).num_columns(6).spacing([14.0, 4.0]).show(ui, |ui| {
                for t in ["Fecha", "Orden", "Equipo", "Trabajo", "Lista completa", "Notas"] {
                    ui.label(egui::RichText::new(t).family(tema::semi()).size(12.5).color(tema::p().texto));
                }
                ui.end_row();
                for r in h.iter().rev().filter(|r| self.f_historial.is_empty() || texto(r, "equipo") == self.f_historial) {
                    ui.label(texto(r, "fecha"));
                    ui.label(texto(r, "orden"));
                    ui::chip(ui, texto(r, "equipo"), false);
                    ui.label(texto(r, "trabajo"));
                    ui.label(if r["completo"].as_bool().unwrap_or(false) { "sí" } else { "no" });
                    ui.label(texto(r, "notas"));
                    ui.end_row();
                }
            });
        });
    }
}

/// Color de una orden en el tablero y la agenda: el de su tarea (limpieza, inspección, ensayo, mantenimiento anual).
fn color_orden(o: &Value) -> Color32 {
    tema::color_tipo_orden(texto(o, "nombre"))
}

/// Texto y color del estado de una orden: cerrada, vencida, hoy o pendiente.
fn estado_orden(o: &Value, hoy: &str) -> (&'static str, Color32) {
    let pal = tema::p();
    let fec = texto(o, "fecha");
    if texto(o, "estado") == "cerrada" {
        ("cerrada", pal.ok)
    } else if fec < hoy {
        ("vencida", pal.falla)
    } else if fec == hoy {
        ("hoy", pal.acento)
    } else {
        ("pendiente", pal.texto_2)
    }
}

/// Orden siguiente del mismo plan y equipo, si la orden pertenece a un plan que existe.
fn siguiente<'a>(ordenes: &'a [Value], planes: &Value, o: &Value) -> Option<&'a Value> {
    let plan = texto(o, "plan");
    if !planes.as_array().is_some_and(|p| p.iter().any(|p| texto(p, "id") == plan)) {
        return None;
    }
    let clave = |x: &'a Value| (texto(x, "fecha"), texto(x, "id"));
    let actual = (texto(o, "fecha"), texto(o, "id"));
    ordenes.iter().filter(|x| texto(x, "plan") == plan && texto(x, "equipo") == texto(o, "equipo") && clave(x) > actual).min_by_key(|x| clave(x))
}

/// Posición x relativa y ancho de las seis columnas de la tabla de órdenes (fecha, OT, tarea, equipo, tipo, estado).
fn columnas_tabla(ancho: f32) -> [(f32, f32); 6] {
    let fijas = [100.0, 84.0, 0.0, 72.0, 92.0, 84.0];
    let tarea = (ancho - 32.0 - fijas.iter().sum::<f32>()).max(120.0);
    let mut x = 16.0;
    fijas.map(|w| {
        let w = if w == 0.0 { tarea } else { w };
        let c = (x, w);
        x += w;
        c
    })
}

/// Etiqueta con su campo de fecha opcional, en orden de derecha a izquierda; dice si cambió el texto.
fn fecha_filtro(ui: &mut egui::Ui, etiqueta: &str, v: &mut String) -> bool {
    let antes = v.clone();
    fecha_libre(ui, v, 100.0, true);
    ui.label(egui::RichText::new(etiqueta).size(12.0).color(tema::p().texto_2));
    *v != antes
}

/// Recuadro con borde de 1 px y sin radio para agrupar contenido dentro de una pestaña de edición.
fn tarjeta_plana<R>(ui: &mut egui::Ui, contenido: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let pal = tema::p();
    egui::Frame::new().fill(pal.panel).stroke(egui::Stroke::new(1.0_f32, pal.borde)).inner_margin(egui::Margin::same(12)).show(ui, contenido).inner
}

/// Chip del estado de una orden: gris claro si está pendiente, oscuro si es de hoy y con punto de color si está vencida o cerrada.
fn chip_estado(ui: &mut egui::Ui, estado: &str, color: Color32) -> egui::Response {
    match estado {
        "pendiente" => ui::chip(ui, estado, false),
        "hoy" => ui::chip(ui, estado, true),
        _ => ui::etiqueta(ui, estado, color),
    }
}

/// Fila de la tabla de órdenes: fecha, OT, tarea, equipo, tipo y estado.
///
/// Returns:
///     true si se pulsó la fila (también sobre los chips).
fn fila_orden(ui: &mut egui::Ui, o: &Value, hoy: &str, ancho: f32, cols: &[(f32, f32); 6], elegida: bool) -> bool {
    let pal = tema::p();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ancho, ALTO_FILA), Sense::click());
    if elegida || ui.rect_contains_pointer(r) {
        ui.painter().rect_filled(r, 0.0, if elegida { pal.acento_fondo } else { pal.realce });
    }
    ui::fondo_fila(ui, r, 0);
    let centro = |i: usize| Pos2::new(r.left() + cols[i].0, r.center().y);
    let recorte = |i: usize| ui.painter().with_clip_rect(Rect::from_min_size(Pos2::new(r.left() + cols[i].0, r.top()), Vec2::new(cols[i].1 - 6.0, r.height())));
    let l = egui::Align2::LEFT_CENTER;
    ui.painter().text(centro(0), l, texto(o, "fecha"), tema::fuente_mono(12.5), pal.texto);
    ui.painter().text(centro(1), l, texto(o, "id"), tema::fuente_mono(12.5), pal.texto);
    recorte(2).text(centro(2), l, texto(o, "nombre"), egui::FontId::proportional(13.0), pal.texto);
    recorte(4).text(centro(4), l, tipo_orden(o), egui::FontId::proportional(13.0), pal.texto);
    let (estado, color) = estado_orden(o, hoy);
    let mut pulsada = resp.clicked();
    let mut celda = |i: usize, contenido: &dyn Fn(&mut egui::Ui) -> egui::Response| {
        let area = Rect::from_min_size(Pos2::new(r.left() + cols[i].0, r.top()), Vec2::new(cols[i].1, r.height()));
        let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::left_to_right(egui::Align::Center)));
        pulsada |= contenido(&mut hijo).clicked();
    };
    celda(3, &|ui| ui::chip(ui, texto(o, "equipo"), false));
    celda(5, &|ui| chip_estado(ui, estado, color));
    pulsada
}

/// Cuadrícula de equipos por semana con una marca por orden, la línea de hoy y el número de órdenes de cada equipo.
///
/// Args:
///     ui: espacio restante de la tarjeta; la cuadrícula llega hasta sus bordes.
///     equipos: catálogo del plan.
///     ordenes: todas las órdenes del plan.
///     lunes: lunes de la primera semana, en días desde 1970-01-01.
///     hoy: día actual, en días desde 1970-01-01.
///     acciones: cola de acciones; un clic en una marca abre la orden.
fn cuadricula_semanal(ui: &mut egui::Ui, equipos: &[Value], ordenes: &[Value], lunes: i64, hoy: i64, acciones: &mut Vec<Accion>) {
    let pal = tema::p();
    let ancho = ui.available_width();
    let col_eq = 170.0_f32.min(ancho * 0.26);
    let col_ot = 44.0;
    let ancho_semanas = ancho - col_eq - col_ot;
    let dia_w = ancho_semanas / (7 * SEMANAS) as f32;
    let linea = egui::Stroke::new(1.0_f32, pal.linea);
    let (cab, _) = ui.allocate_exact_size(Vec2::new(ancho, 30.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(cab, 0.0, pal.campo);
    p.line_segment([cab.left_bottom(), cab.right_bottom()], egui::Stroke::new(1.0_f32, pal.borde));
    p.text(Pos2::new(cab.left() + 16.0, cab.center().y), egui::Align2::LEFT_CENTER, "Equipo", tema::fuente_semi(12.0), pal.texto);
    for s in 0..SEMANAS {
        let (_, m, d) = civil_desde_dias(lunes + 7 * s);
        let x = cab.left() + col_eq + dia_w * 7.0 * s as f32;
        p.line_segment([Pos2::new(x, cab.top() + 7.0), Pos2::new(x, cab.bottom() - 7.0)], linea);
        p.text(Pos2::new(x + 4.0, cab.center().y), egui::Align2::LEFT_CENTER, format!("{d} {}", MESES_CORTOS[m as usize - 1]), tema::fuente_mono(10.0), pal.texto_2);
    }
    p.text(Pos2::new(cab.right() - col_ot / 2.0, cab.center().y), egui::Align2::CENTER_CENTER, "OT", tema::fuente_semi(12.0), pal.texto);
    let disponible = ui.available_height().max(60.0);
    let filas = equipos.len() as f32;
    let alto_fila = (disponible / filas).clamp(40.0, 90.0);
    egui::ScrollArea::vertical().id_salt("plan_desplazable_9").auto_shrink([false, false]).max_height(disponible).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let (cuerpo, _) = ui.allocate_exact_size(Vec2::new(ancho, alto_fila * filas), Sense::hover());
        let p = ui.painter();
        for (k, e) in equipos.iter().enumerate() {
            let y = cuerpo.top() + alto_fila * k as f32;
            let fila = Rect::from_min_size(Pos2::new(cuerpo.left(), y), Vec2::new(ancho, alto_fila));
            p.line_segment([fila.left_bottom(), fila.right_bottom()], linea);
            let codigo = texto(e, "codigo");
            p.text(Pos2::new(fila.left() + 16.0, fila.center().y - 8.0), egui::Align2::LEFT_CENTER, codigo, tema::fuente_mono(13.0), pal.texto);
            p.with_clip_rect(Rect::from_min_size(Pos2::new(fila.left() + 16.0, y), Vec2::new(col_eq - 24.0, alto_fila))).text(
                Pos2::new(fila.left() + 16.0, fila.center().y + 9.0),
                egui::Align2::LEFT_CENTER,
                texto(e, "descripcion"),
                egui::FontId::proportional(11.5),
                pal.texto_2,
            );
            let mut cuenta = 0;
            for o in ordenes.iter().filter(|o| texto(o, "equipo") == codigo) {
                let Some(d) = dias_de(texto(o, "fecha")) else { continue };
                let desfase = d - lunes;
                if !(0..7 * SEMANAS).contains(&desfase) {
                    continue;
                }
                cuenta += 1;
                let centro = Pos2::new(cuerpo.left() + col_eq + dia_w * (desfase as f32 + 0.5), fila.center().y);
                let marca = Rect::from_center_size(centro, Vec2::splat(14.0));
                let resp = ui.interact(marca, ui.id().with(("ot", texto(o, "id"))), Sense::click()).on_hover_text(format!("{} · {}\n{} · {}", texto(o, "id"), texto(o, "nombre"), texto(o, "fecha"), texto(o, "estado")));
                let color = color_orden(o);
                ui::dibujar_muestra(p, marca, if texto(o, "estado") == "cerrada" { color.gamma_multiply(0.4) } else { color }, ui::muestra_tipo(texto(o, "nombre")));
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    acciones.push(Accion::AbrirOrden(texto(o, "id").to_string()));
                }
            }
            p.text(Pos2::new(cuerpo.right() - col_ot / 2.0, fila.center().y), egui::Align2::CENTER_CENTER, cuenta.to_string(), tema::fuente_mono(12.0), pal.texto);
        }
        for s in 0..SEMANAS {
            let x = cuerpo.left() + col_eq + dia_w * 7.0 * s as f32;
            p.line_segment([Pos2::new(x, cuerpo.top()), Pos2::new(x, cuerpo.bottom())], linea);
        }
        let desfase = hoy - lunes;
        if (0..7 * SEMANAS).contains(&desfase) {
            let x = cuerpo.left() + col_eq + dia_w * (desfase as f32 + 0.5);
            p.rect_filled(Rect::from_min_max(Pos2::new(x - 1.0, cuerpo.top()), Pos2::new(x + 1.0, cuerpo.bottom())), 0.0, pal.acento);
            let g = p.layout_no_wrap("hoy".into(), tema::fuente_semi(10.0), pal.texto);
            let origen = Pos2::new(x + 4.0, cuerpo.top() + 2.0);
            p.rect_filled(Rect::from_min_size(origen - Vec2::new(3.0, 0.0), g.size() + Vec2::new(6.0, 0.0)), 0.0, pal.panel);
            p.galley(origen, g, pal.texto);
        }
    });
}

/// Fila de la ficha de una orden: etiqueta gris a la izquierda, el contenido a la derecha y una línea inferior.
fn fila_dato(ui: &mut egui::Ui, etiqueta: &str, valor: impl FnOnce(&mut egui::Ui)) {
    let pal = tema::p();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
    ui.painter().text(Pos2::new(r.left(), r.center().y), egui::Align2::LEFT_CENTER, etiqueta, egui::FontId::proportional(13.0), pal.texto_2);
    let mut hijo = ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::right_to_left(egui::Align::Center)));
    valor(&mut hijo);
    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], egui::Stroke::new(1.0_f32, pal.linea));
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
        let f = |estado: &'static str, equipo: &'static str, desde: &'static str, hasta: &'static str| FiltroOrdenes { estado, equipo, desde, hasta, hoy, lunes: "2026-06-15", domingo: "2026-06-21", limite: "2026-07-15" };
        let futura = orden_de("2026-07-01", "pendiente", "INV-1");
        let vieja = orden_de("2026-05-01", "pendiente", "INV-2");
        let cerrada = orden_de("2026-05-01", "cerrada", "INV-1");
        assert!(orden_visible(&futura, &f("pendiente", "", "", "")));
        assert!(orden_visible(&vieja, &f("pendiente", "", "", "")), "una vencida sigue pendiente de cerrar");
        assert!(!orden_visible(&cerrada, &f("pendiente", "", "", "")));
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
    fn semanas_y_limites_de_fechas() {
        // 2026-11-25 es miércoles
        let d = dias_de("2026-11-25").unwrap();
        assert_eq!(dia_semana(d), 2);
        assert_eq!(limites("2026-11-25"), ("2026-11-23".into(), "2026-11-29".into(), "2026-12-25".into()));
        assert_eq!(rango_semana(lunes_de(d)), "23–29 nov");
        assert_eq!(rango_semana(dias_de("2026-11-30").unwrap()), "30 nov–6 dic");
        assert_eq!(dia_corto(d), "mié 25 nov");
        assert_eq!(dias_de("2026-02-30"), None);
    }

    #[test]
    fn filtros_de_semana_y_proximos_dias() {
        let (lunes, domingo, limite) = limites("2026-11-25");
        let f = |estado: &'static str| FiltroOrdenes { estado, equipo: "", desde: "", hasta: "", hoy: "2026-11-25", lunes: &lunes, domingo: &domingo, limite: &limite };
        let esta = orden_de("2026-11-28", "pendiente", "A");
        let luego = orden_de("2026-12-20", "pendiente", "A");
        let lejos = orden_de("2027-02-01", "pendiente", "A");
        let cerrada = orden_de("2026-11-28", "cerrada", "A");
        assert!(orden_visible(&esta, &f("semana")) && orden_visible(&esta, &f("proximas")));
        assert!(!orden_visible(&luego, &f("semana")) && orden_visible(&luego, &f("proximas")));
        assert!(!orden_visible(&lejos, &f("proximas")));
        assert!(!orden_visible(&cerrada, &f("semana")) && !orden_visible(&cerrada, &f("proximas")));
    }

    #[test]
    fn siguiente_orden_del_mismo_plan_y_equipo() {
        let planes = json!([{"id": "P001"}]);
        let o = |id: &str, fecha: &str, equipo: &str, plan: &str| json!({"id": id, "fecha": fecha, "equipo": equipo, "plan": plan});
        let todas = vec![o("OT-1", "2026-01-01", "A", "P001"), o("OT-3", "2026-03-01", "A", "P001"), o("OT-2", "2026-02-01", "A", "P001"), o("OT-4", "2026-01-15", "B", "P001")];
        assert_eq!(siguiente(&todas, &planes, &todas[0]).map(|x| texto(x, "id")), Some("OT-2"));
        assert!(siguiente(&todas, &planes, &todas[1]).is_none());
        let corr = vec![o("OT-9", "2026-01-01", "A", "CORR"), o("OT-10", "2026-02-01", "A", "CORR")];
        assert!(siguiente(&corr, &planes, &corr[0]).is_none(), "sin plan no hay siguiente");
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
