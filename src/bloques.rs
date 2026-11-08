//! Módulo 2: editor de simulación por bloques. Lienzo, paleta, parámetros, solver y osciloscopios.
use crate::app::Compartido;
use crate::grafica::{self, Fig2D, Figura, Serie, TipoSerie};
use crate::herramientas;
use crate::motor::Motor;
use crate::tema;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

// ================================================================= Modelo del diagrama

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Sistema {
    #[serde(default)]
    pub bloques: Vec<BloqueD>,
    #[serde(default)]
    pub conexiones: Vec<Conexion>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BloqueD {
    pub id: String,
    pub tipo: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub params: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sistema: Option<Sistema>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Conexion {
    pub de: (String, String),
    pub a: (String, String),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Diagrama {
    #[serde(default = "formato_defecto")]
    pub formato: String,
    #[serde(default)]
    pub nombre: String,
    #[serde(default)]
    pub solver: Map<String, Value>,
    #[serde(default)]
    pub sistema: Sistema,
}

fn formato_defecto() -> String {
    "heliora-diagrama/1".into()
}

pub fn diagrama_vacio() -> Diagrama {
    let mut solver = Map::new();
    solver.insert("metodo".into(), json!("dopri"));
    solver.insert("t_fin".into(), json!(10.0));
    solver.insert("rtol".into(), json!(1e-3));
    solver.insert("atol".into(), json!(1e-6));
    solver.insert("paso".into(), json!(1e-3));
    Diagrama { formato: formato_defecto(), nombre: "Diagrama sin título".into(), solver, sistema: Sistema::default() }
}

fn nivel<'a>(raiz: &'a mut Sistema, ruta: &[String]) -> &'a mut Sistema {
    let mut s = raiz;
    for r in ruta {
        let i = s.bloques.iter().position(|b| &b.id == r);
        match i {
            Some(i) if s.bloques[i].sistema.is_some() => s = s.bloques[i].sistema.as_mut().unwrap(),
            _ => break,
        }
    }
    s
}

// ================================================================= Catálogo

#[derive(Clone, Default)]
pub struct ParamDef {
    pub nombre: String,
    pub etiqueta: String,
    pub defecto: Value,
    pub unidad: String,
    pub tipo: String,
    pub opciones: Vec<String>,
}

#[derive(Clone, Default)]
pub struct Ficha {
    pub tipo: String,
    pub nombre: String,
    pub categoria: String,
    pub doc: String,
    pub params: Vec<ParamDef>,
    pub entradas: Vec<String>,
    pub salidas: Vec<String>,
    pub e: Vec<String>,
    pub e3: Vec<String>,
    pub dinamico: Option<(String, String, String)>,
    pub archivo: String,
    pub linea: usize,
}

fn lista_txt(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

pub fn fichas_desde(v: &Value) -> Vec<Ficha> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|f| Ficha {
                    tipo: f["tipo"].as_str().unwrap_or("").into(),
                    nombre: f["nombre"].as_str().unwrap_or("").into(),
                    categoria: f["categoria"].as_str().unwrap_or("").into(),
                    doc: f["doc"].as_str().unwrap_or("").into(),
                    params: f["params"]
                        .as_array()
                        .map(|p| {
                            p.iter()
                                .map(|d| ParamDef {
                                    nombre: d["nombre"].as_str().unwrap_or("").into(),
                                    etiqueta: d["etiqueta"].as_str().unwrap_or("").into(),
                                    defecto: d["defecto"].clone(),
                                    unidad: d["unidad"].as_str().unwrap_or("").into(),
                                    tipo: d["tipo"].as_str().unwrap_or("num").into(),
                                    opciones: lista_txt(&d["opciones"]),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    entradas: lista_txt(&f["puertos"]["entradas"]),
                    salidas: lista_txt(&f["puertos"]["salidas"]),
                    e: lista_txt(&f["puertos"]["e"]),
                    e3: lista_txt(&f["puertos"]["e3"]),
                    dinamico: f.get("puertos_dinamicos").and_then(|d| {
                        Some((d.get("param")?.as_str()?.to_string(), d.get("modo")?.as_str()?.to_string(), d.get("prefijo")?.as_str()?.to_string()))
                    }),
                    archivo: f["archivo"].as_str().unwrap_or("").into(),
                    linea: f["linea"].as_u64().unwrap_or(1) as usize,
                })
                .collect()
        })
        .unwrap_or_default()
}

// ================================================================= Puertos

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clase {
    Entrada,
    Salida,
    E,
    E3,
}

#[derive(Clone)]
pub struct PuertoD {
    pub nombre: String,
    pub clase: Clase,
    pub izq: bool,
    pub rel: Vec2,
}

pub const ANCHO: f32 = 148.0;

fn valor_txt(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(Value::Number(n)) => {
            // Los valores muy pequeños o grandes se leen mejor en notación científica
            let x = n.as_f64().unwrap_or(0.0);
            if x != 0.0 && (x.abs() < 1e-3 || x.abs() >= 1e7) {
                format!("{x:e}")
            } else {
                n.to_string()
            }
        }
        Some(o) => o.to_string(),
    }
}

/// Nombres de los puertos de un bloque, según su tipo y parámetros.
fn nombres_puertos(b: &BloqueD, cat: &HashMap<String, Ficha>) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    if b.tipo == "subsistema" {
        let mut ent = vec![];
        let mut sal = vec![];
        let mut e = vec![];
        let mut e3 = vec![];
        if let Some(s) = &b.sistema {
            for i in &s.bloques {
                let n: i64 = valor_txt(i.params.get("numero")).parse::<f64>().map(|x| x as i64).unwrap_or(1);
                match i.tipo.as_str() {
                    "entrada" => ent.push((n, format!("in{n}"))),
                    "salida" => sal.push((n, format!("out{n}"))),
                    "puerto_e" => e.push((n, format!("e{n}"))),
                    "puerto_e3" => e3.push((n, format!("e{n}"))),
                    _ => {}
                }
            }
        }
        let f = |mut v: Vec<(i64, String)>| {
            v.sort();
            v.into_iter().map(|a| a.1).collect::<Vec<_>>()
        };
        return (f(ent), f(sal), f(e), f(e3));
    }
    let Some(fi) = cat.get(&b.tipo) else {
        return (vec![], vec![], vec![], vec![]);
    };
    let (mut ent, mut sal) = (fi.entradas.clone(), fi.salidas.clone());
    if let Some((param, modo, prefijo)) = &fi.dinamico {
        let defecto = fi.params.iter().find(|p| &p.nombre == param).map(|p| p.defecto.clone()).unwrap_or(Value::Null);
        let val = b.params.get(param).unwrap_or(&defecto);
        let txt = valor_txt(Some(val));
        let n = match modo.as_str() {
            "largo" => txt.chars().count(),
            _ => txt.parse::<f64>().unwrap_or(1.0).max(0.0) as usize,
        };
        let nombres: Vec<String> = (1..=n).map(|i| format!("{prefijo}{i}")).collect();
        if modo == "valor_salidas" {
            sal = nombres;
        } else {
            ent = nombres;
        }
    }
    (ent, sal, fi.e.clone(), fi.e3.clone())
}

/// Coloca los puertos: entradas a la izquierda, salidas a la derecha.
pub fn puertos_de(b: &BloqueD, cat: &HashMap<String, Ficha>) -> (Vec<PuertoD>, f32) {
    let (ent, sal, e, e3) = nombres_puertos(b, cat);
    let mut izq: Vec<(String, Clase)> = ent.into_iter().map(|n| (n, Clase::Entrada)).collect();
    let mut der: Vec<(String, Clase)> = sal.into_iter().map(|n| (n, Clase::Salida)).collect();
    let fuente = b.tipo.starts_with("fuente_") || b.tipo.starts_with("arreglo_fv");
    if !e3.is_empty() {
        for n in &e {
            izq.push((n.clone(), Clase::E));
        }
        for n in &e3 {
            let a_izq = n == "a" || n == "p" || (b.tipo == "subsistema" && n == "e1");
            if a_izq && !fuente {
                izq.push((n.clone(), Clase::E3));
            } else {
                der.push((n.clone(), Clase::E3));
            }
        }
    } else if fuente {
        for n in &e {
            der.push((n.clone(), Clase::E));
        }
    } else {
        let mitad = if e.len() == 3 { 1 } else { e.len().div_ceil(2) };
        for (i, n) in e.iter().enumerate() {
            if i < mitad {
                izq.push((n.clone(), Clase::E));
            } else {
                der.push((n.clone(), Clase::E));
            }
        }
    }
    let filas = izq.len().max(der.len()).max(1) as f32;
    // Cabecera de 18 px, filas de puertos y una línea final con el parámetro principal
    let alto = (26.0 + filas * 19.0 + 14.0).max(64.0);
    let mut v = vec![];
    let coloca = |lista: &Vec<(String, Clase)>, es_izq: bool, v: &mut Vec<PuertoD>| {
        let n = lista.len() as f32;
        for (i, (nom, c)) in lista.iter().enumerate() {
            let y = 24.0 + (alto - 44.0) * (i as f32 + 0.5) / n;
            v.push(PuertoD { nombre: nom.clone(), clase: *c, izq: es_izq, rel: Vec2::new(if es_izq { 0.0 } else { ANCHO }, y) });
        }
    };
    coloca(&izq, true, &mut v);
    coloca(&der, false, &mut v);
    (v, alto)
}

fn color_categoria(cat: &str) -> Color32 {
    tema::color_familia(cat)
}

fn color_cable(c: Clase) -> Color32 {
    match c {
        Clase::E | Clase::E3 => tema::p().electrica,
        _ => tema::p().senal,
    }
}

/// Texto del parámetro principal de un bloque, para la línea inferior del nodo.
fn parametro_principal(b: &BloqueD, f: Option<&Ficha>) -> String {
    let Some(f) = f else { return String::new() };
    let Some(p) = f.params.iter().find(|p| p.tipo == "num" || p.tipo == "opcion") else { return String::new() };
    let v = valor_txt(b.params.get(&p.nombre).or(Some(&p.defecto)));
    let mut t = format!("{} = {}", p.nombre, v);
    if !p.unidad.is_empty() {
        t.push(' ');
        t.push_str(&p.unidad);
    }
    t
}

// ================================================================= Estado del editor

enum Arrastre {
    Nada,
    Mover(Vec<(usize, Vec2)>),
    Hilo { bloque: String, puerto: String, clase: Clase, inicio: Pos2 },
    Seleccion(Pos2),
    Lienzo,
}

pub struct Editor {
    pub diagrama: Diagrama,
    pub fichas: HashMap<String, Ficha>,
    orden_cat: Vec<String>,
    pub ejemplos: Vec<(String, String)>,
    ruta: Vec<String>,
    pan: Vec2,
    zoom: f32,
    sel: HashSet<String>,
    sel_hilo: Option<usize>,
    arrastre: Arrastre,
    nuevo: Option<String>,
    buscar: String,
    cat_abiertas: HashSet<String>,
    deshacer: Vec<String>,
    rehacer: Vec<String>,
    contador: u32,
    pub archivo: Option<PathBuf>,
    pub id_sim: Option<u64>,
    id_ejemplo: Option<u64>,
    pub avance: f32,
    pub resultado: Vec<(String, Figura)>,
    pestana: usize,
    pub stats: String,
    pub avisos: Vec<String>,
    pub error: Option<String>,
    centrar: bool,
    alto_osc: f32,
    /// Bloques (identificadores completos) de cada lazo algebraico de la última simulación
    pub lazos: Vec<Vec<String>>,
    /// Petición de linealización en curso y su informe (Bode, escalón, polos)
    pub id_lin: Option<u64>,
    lineal: Option<herramientas::Informe>,
    ver_lineal: bool,
}

impl Editor {
    pub fn nuevo() -> Editor {
        Editor {
            diagrama: diagrama_vacio(),
            fichas: HashMap::new(),
            orden_cat: vec![],
            ejemplos: vec![],
            ruta: vec![],
            pan: Vec2::new(30.0, 30.0),
            zoom: 1.0,
            sel: HashSet::new(),
            sel_hilo: None,
            arrastre: Arrastre::Nada,
            nuevo: None,
            buscar: String::new(),
            cat_abiertas: ["Fuentes", "Fotovoltaica"].iter().map(|s| s.to_string()).collect(),
            deshacer: vec![],
            rehacer: vec![],
            contador: 0,
            archivo: None,
            id_sim: None,
            id_ejemplo: None,
            avance: 0.0,
            resultado: vec![],
            pestana: 0,
            stats: String::new(),
            avisos: vec![],
            error: None,
            centrar: false,
            alto_osc: 320.0,
            lazos: vec![],
            id_lin: None,
            lineal: None,
            ver_lineal: false,
        }
    }

    pub fn fijar_catalogo(&mut self, v: &Value) {
        let f = fichas_desde(v);
        let mut cats: Vec<String> = f.iter().map(|x| x.categoria.clone()).collect();
        cats.sort();
        cats.dedup();
        let prio = ["Fuentes", "Matemática", "Continuo", "Discontinuidades", "Señales", "Sumideros", "Subsistemas", "Eléctrica · Pasivos", "Eléctrica · Fuentes", "Eléctrica · Semiconductores", "Eléctrica · Medición", "Potencia · Convertidores", "Potencia · Filtros", "Potencia · Modulación", "Potencia · Control", "Potencia · Medición", "Fotovoltaica"];
        let mut orden: Vec<String> = prio.iter().filter(|p| cats.iter().any(|c| c == *p)).map(|s| s.to_string()).collect();
        for c in cats {
            if !orden.contains(&c) {
                orden.push(c);
            }
        }
        self.orden_cat = orden;
        self.fichas = f.into_iter().map(|x| (x.tipo.clone(), x)).collect();
    }

    pub fn cargar_valor(&mut self, v: &Value) {
        if let Ok(d) = serde_json::from_value::<Diagrama>(v.clone()) {
            self.diagrama = d;
            self.ruta.clear();
            self.sel.clear();
            self.sel_hilo = None;
            self.resultado.clear();
            self.stats.clear();
            self.avisos.clear();
            self.error = None;
            self.centrar = true;
            self.deshacer.clear();
            self.rehacer.clear();
        } else {
            self.error = Some("El archivo no es un diagrama válido.".into());
        }
    }

    pub fn pedir_ejemplo(&mut self, motor: &mut Motor, clave: &str) {
        self.id_ejemplo = Some(motor.llamar("ejemplo", json!({"clave": clave})));
    }

    pub fn recibir(&mut self, id: u64, ok: bool, valor: &Value, err: &str, c: &mut Compartido) -> bool {
        if Some(id) == self.id_lin {
            self.id_lin = None;
            if ok {
                self.lineal = Some(herramientas::informe_desde(&valor["informe"]));
                self.ver_lineal = true;
                c.fijar_variables(&valor["variables"]);
            } else {
                self.error = Some(format!("No se pudo linealizar: {err}"));
            }
            return true;
        }
        if Some(id) == self.id_ejemplo {
            self.id_ejemplo = None;
            if ok {
                self.cargar_valor(valor);
                self.archivo = None;
            } else {
                self.error = Some(err.to_string());
            }
            return true;
        }
        if Some(id) == self.id_sim {
            self.id_sim = None;
            if ok {
                self.error = None;
                self.resultado.clear();
                if let Some(fs) = valor["figuras"].as_array() {
                    for f in fs {
                        let et = f["etiqueta"].as_str().unwrap_or("Osciloscopio").to_string();
                        let mut series = vec![];
                        for t in f["trazas"].as_array().unwrap_or(&vec![]) {
                            let a = |k: &str| -> Vec<f64> { t[k].as_array().map(|v| v.iter().map(|x| x.as_f64().unwrap_or(f64::NAN)).collect()).unwrap_or_default() };
                            series.push(Serie { x: a("x"), y: a("y"), nombre: t["nombre"].as_str().unwrap_or("").into(), tipo: TipoSerie::Linea, color: None, visible: true });
                        }
                        self.resultado.push((et.clone(), Figura::D2(Fig2D { titulo: et, xlabel: "Tiempo [s]".into(), ylabel: String::new(), series, lineas_h: vec![], log_x: false, log_y: false, vista: None })));
                    }
                }
                let e = &valor["estadisticas"];
                let miles = |v: &Value| grafica::miles(v.as_f64().unwrap_or(0.0));
                let mut s = format!(
                    "{} · {} pasos · {} rechazados · {} evaluaciones · {} estados · {} bloques",
                    e["metodo"].as_str().unwrap_or(""),
                    miles(&e["pasos"]),
                    miles(&e["rechazos"]),
                    miles(&e["evaluaciones"]),
                    e["estados"],
                    e["bloques"],
                );
                if e["cruces"].as_u64().unwrap_or(0) > 0 {
                    s.push_str(&format!(" · {} cruces por cero", miles(&e["cruces"])));
                }
                if let Some(l) = e["estado_limitante"].as_str() {
                    s.push_str(&format!(" · paso limitado por {l} ({:.0} %)", e["limitante_pct"].as_f64().unwrap_or(0.0)).replace('.', ","));
                }
                s.push_str(&format!(" · {} s", grafica::decimal(e["segundos"].as_f64().unwrap_or(0.0), 2)));
                self.stats = s;
                self.avisos = lista_txt(&valor["avisos"]);
                self.lazos = valor["lazos"].as_array().map(|a| a.iter().map(lista_txt).collect()).unwrap_or_default();
                if self.pestana >= self.resultado.len() {
                    self.pestana = 0;
                }
                c.fijar_variables(&valor["variables"]);
            } else {
                self.error = Some(err.to_string());
            }
            return true;
        }
        false
    }

    pub fn avance(&mut self, id: u64, f: f32) -> bool {
        if Some(id) == self.id_sim {
            self.avance = f;
            return true;
        }
        false
    }

    fn instantanea(&mut self) {
        if let Ok(t) = serde_json::to_string(&self.diagrama) {
            self.deshacer.push(t);
            if self.deshacer.len() > 80 {
                self.deshacer.remove(0);
            }
            self.rehacer.clear();
        }
    }

    fn deshacer_ya(&mut self) {
        if let Some(t) = self.deshacer.pop() {
            if let Ok(a) = serde_json::to_string(&self.diagrama) {
                self.rehacer.push(a);
            }
            if let Ok(d) = serde_json::from_str(&t) {
                self.diagrama = d;
            }
            self.sel.clear();
        }
    }

    fn rehacer_ya(&mut self) {
        if let Some(t) = self.rehacer.pop() {
            if let Ok(a) = serde_json::to_string(&self.diagrama) {
                self.deshacer.push(a);
            }
            if let Ok(d) = serde_json::from_str(&t) {
                self.diagrama = d;
            }
            self.sel.clear();
        }
    }

    fn sistema(&mut self) -> &mut Sistema {
        let r = self.ruta.clone();
        nivel(&mut self.diagrama.sistema, &r)
    }

    fn id_nuevo(&mut self, tipo: &str) -> String {
        loop {
            self.contador += 1;
            let id = format!("{}{}", tipo.chars().take(3).collect::<String>(), self.contador);
            let existe = {
                let s = self.sistema();
                s.bloques.iter().any(|b| b.id == id)
            };
            if !existe {
                return id;
            }
        }
    }

    fn agregar(&mut self, tipo: &str, pos: Vec2) {
        self.instantanea();
        let id = self.id_nuevo(tipo);
        let nombre = self.fichas.get(tipo).map(|f| f.nombre.clone()).unwrap_or_else(|| tipo.to_string());
        let mut b = BloqueD { id: id.clone(), tipo: tipo.into(), x: pos.x, y: pos.y, nombre, params: Map::new(), sistema: None };
        if let Some(f) = self.fichas.get(tipo) {
            for p in &f.params {
                b.params.insert(p.nombre.clone(), p.defecto.clone());
            }
        }
        if tipo == "subsistema" {
            let num = |n: i64| {
                let mut m = Map::new();
                m.insert("numero".into(), json!(n));
                m
            };
            b.sistema = Some(Sistema {
                bloques: vec![
                    BloqueD { id: "ent".into(), tipo: "entrada".into(), x: 40.0, y: 60.0, nombre: "Entrada 1".into(), params: num(1), sistema: None },
                    BloqueD { id: "sal".into(), tipo: "salida".into(), x: 340.0, y: 60.0, nombre: "Salida 1".into(), params: num(1), sistema: None },
                ],
                conexiones: vec![Conexion { de: ("ent".into(), "y".into()), a: ("sal".into(), "u".into()) }],
            });
        }
        if matches!(tipo, "entrada" | "salida" | "puerto_e" | "puerto_e3") {
            let kind = tipo.to_string();
            let n = self.sistema().bloques.iter().filter(|b| b.tipo == kind).count() as i64 + 1;
            b.params.insert("numero".into(), json!(n));
            b.nombre = format!("{} {n}", b.nombre);
        }
        self.sistema().bloques.push(b);
        self.sel.clear();
        self.sel.insert(id);
    }

    fn borrar_seleccion(&mut self) {
        if self.sel.is_empty() && self.sel_hilo.is_none() {
            return;
        }
        self.instantanea();
        let sel = self.sel.clone();
        let hilo = self.sel_hilo.take();
        let s = self.sistema();
        if let Some(h) = hilo {
            if h < s.conexiones.len() {
                s.conexiones.remove(h);
            }
        }
        s.bloques.retain(|b| !sel.contains(&b.id));
        s.conexiones.retain(|c| !sel.contains(&c.de.0) && !sel.contains(&c.a.0));
        self.sel.clear();
    }

    fn duplicar(&mut self) {
        if self.sel.is_empty() {
            return;
        }
        self.instantanea();
        let sel = self.sel.clone();
        let copias: Vec<BloqueD> = self.sistema().bloques.iter().filter(|b| sel.contains(&b.id)).cloned().collect();
        let mut mapa = HashMap::new();
        let mut nuevas = vec![];
        for mut b in copias {
            let nid = self.id_nuevo(&b.tipo);
            mapa.insert(b.id.clone(), nid.clone());
            b.id = nid;
            b.x += 30.0;
            b.y += 30.0;
            nuevas.push(b);
        }
        let conex: Vec<Conexion> = self.sistema().conexiones.iter().filter(|c| mapa.contains_key(&c.de.0) && mapa.contains_key(&c.a.0)).cloned().collect();
        let s = self.sistema();
        for c in conex {
            s.conexiones.push(Conexion { de: (mapa[&c.de.0].clone(), c.de.1), a: (mapa[&c.a.0].clone(), c.a.1) });
        }
        self.sel = nuevas.iter().map(|b| b.id.clone()).collect();
        self.sistema().bloques.extend(nuevas);
    }

    /// Mete los bloques seleccionados en un subsistema nuevo y conserva sus conexiones.
    fn agrupar(&mut self) {
        if self.sel.len() < 2 {
            return;
        }
        self.instantanea();
        let sel = self.sel.clone();
        let fichas = self.fichas.clone();
        let nid = self.id_nuevo("sub");
        let s = self.sistema();
        let dentro: Vec<BloqueD> = s.bloques.iter().filter(|b| sel.contains(&b.id)).cloned().collect();
        let mx = dentro.iter().map(|b| b.x).sum::<f32>() / dentro.len() as f32;
        let my = dentro.iter().map(|b| b.y).sum::<f32>() / dentro.len() as f32;
        let min_x = dentro.iter().map(|b| b.x).fold(f32::MAX, f32::min);
        let max_x = dentro.iter().map(|b| b.x).fold(f32::MIN, f32::max);
        let clase_puerto = |bid: &str, p: &str, lista: &[BloqueD]| -> Option<Clase> {
            let b = lista.iter().find(|x| x.id == bid)?;
            let (v, _) = puertos_de(b, &fichas);
            v.into_iter().find(|q| q.nombre == p).map(|q| q.clase)
        };
        let num = |n: i32| {
            let mut m = Map::new();
            m.insert("numero".into(), json!(n));
            m
        };
        let mut interior = Sistema { bloques: dentro.clone(), conexiones: vec![] };
        let mut exterior = vec![];
        let (mut ne, mut ns, mut nel) = (0, 0, 0);
        let todos = s.bloques.clone();
        let mut alias: HashMap<(String, String), String> = HashMap::new();
        for c in s.conexiones.iter() {
            let (da, aa) = (sel.contains(&c.de.0), sel.contains(&c.a.0));
            if da && aa {
                interior.conexiones.push(c.clone());
            } else if !da && !aa {
                exterior.push(c.clone());
            } else {
                let (fuera, dent) = if aa { (&c.de, &c.a) } else { (&c.a, &c.de) };
                let clase = clase_puerto(&dent.0, &dent.1, &todos).unwrap_or(Clase::Entrada);
                match clase {
                    Clase::E | Clase::E3 => {
                        let nombre = if let Some(a) = alias.get(dent) {
                            a.clone()
                        } else {
                            nel += 1;
                            let idp = format!("pe{nel}");
                            let tipo = if clase == Clase::E3 { "puerto_e3" } else { "puerto_e" };
                            interior.bloques.push(BloqueD { id: idp.clone(), tipo: tipo.into(), x: min_x - 160.0, y: my + 60.0 * nel as f32, nombre: format!("Puerto {nel}"), params: num(nel), sistema: None });
                            interior.conexiones.push(Conexion { de: (idp, "e".into()), a: dent.clone() });
                            let n = format!("e{nel}");
                            alias.insert(dent.clone(), n.clone());
                            n
                        };
                        exterior.push(Conexion { de: fuera.clone(), a: (nid.clone(), nombre) });
                    }
                    Clase::Entrada => {
                        ne += 1;
                        let idp = format!("ent{ne}");
                        interior.bloques.push(BloqueD { id: idp.clone(), tipo: "entrada".into(), x: min_x - 160.0, y: my + 60.0 * ne as f32 - 60.0, nombre: format!("Entrada {ne}"), params: num(ne), sistema: None });
                        interior.conexiones.push(Conexion { de: (idp, "y".into()), a: dent.clone() });
                        exterior.push(Conexion { de: fuera.clone(), a: (nid.clone(), format!("in{ne}")) });
                    }
                    Clase::Salida => {
                        let nombre = if let Some(a) = alias.get(dent) {
                            a.clone()
                        } else {
                            ns += 1;
                            let idp = format!("sal{ns}");
                            interior.bloques.push(BloqueD { id: idp.clone(), tipo: "salida".into(), x: max_x + 200.0, y: my + 60.0 * ns as f32 - 60.0, nombre: format!("Salida {ns}"), params: num(ns), sistema: None });
                            interior.conexiones.push(Conexion { de: dent.clone(), a: (idp, "u".into()) });
                            let n = format!("out{ns}");
                            alias.insert(dent.clone(), n.clone());
                            n
                        };
                        exterior.push(Conexion { de: (nid.clone(), nombre), a: fuera.clone() });
                    }
                }
            }
        }
        s.bloques.retain(|b| !sel.contains(&b.id));
        s.bloques.push(BloqueD { id: nid.clone(), tipo: "subsistema".into(), x: mx, y: my, nombre: "Subsistema".into(), params: Map::new(), sistema: Some(interior) });
        s.conexiones = exterior;
        self.sel.clear();
        self.sel.insert(nid);
    }

    pub fn ejecutar(&mut self, motor: &mut Motor) {
        match serde_json::to_value(&self.diagrama) {
            Ok(v) => {
                self.error = None;
                self.avance = 0.0;
                self.id_sim = Some(motor.llamar("simular", json!({"diagrama": v})));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn ajustar_vista(&mut self, area: Rect) {
        let fichas = &self.fichas;
        let r = self.ruta.clone();
        let s = nivel(&mut self.diagrama.sistema, &r);
        if s.bloques.is_empty() {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for b in &s.bloques {
            let (_, alto) = puertos_de(b, fichas);
            x0 = x0.min(b.x);
            y0 = y0.min(b.y);
            x1 = x1.max(b.x + ANCHO);
            y1 = y1.max(b.y + alto);
        }
        let z = ((area.width() - 60.0) / (x1 - x0)).min((area.height() - 60.0) / (y1 - y0)).clamp(0.25, 1.3);
        self.zoom = z;
        self.pan = Vec2::new((area.width() - (x1 - x0) * z) / 2.0 - x0 * z, (area.height() - (y1 - y0) * z) / 2.0 - y0 * z);
    }

    /// Linealiza el diagrama en su punto de operación. Si hay un solo bloque seleccionado, su
    /// primera salida es la salida del modelo; si no, la señal del primer osciloscopio. La entrada
    /// es la primera fuente del diagrama (la referencia).
    pub fn linealizar(&mut self, motor: &mut Motor) {
        let v = match serde_json::to_value(&self.diagrama) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let mut params = json!({"diagrama": v});
        if self.sel.len() == 1 {
            let id = self.sel.iter().next().cloned().unwrap_or_default();
            let completo = if self.ruta.is_empty() { id } else { format!("{}/{}", self.ruta.join("/"), id) };
            params["salidas"] = json!([completo]);
        }
        self.error = None;
        self.id_lin = Some(motor.llamar("linealizar", params));
    }

    /// Guarda el diagrama en JSON canónico: claves ordenadas, bloques por id, conexiones por
    /// extremos y sangría fija, para que un cambio pequeño dé un diff de Git pequeño.
    pub fn guardar_en(&self, ruta: &std::path::Path) -> Result<(), String> {
        let t = json_canonico(&self.diagrama)?;
        std::fs::write(ruta, t).map_err(|e| e.to_string())
    }

    pub fn abrir_de(&mut self, ruta: &std::path::Path) -> Result<(), String> {
        let t = std::fs::read_to_string(ruta).map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
        self.cargar_valor(&v);
        self.archivo = Some(ruta.to_path_buf());
        Ok(())
    }

    // ============================================================= Interfaz

    pub fn ui(&mut self, ui: &mut egui::Ui, motor: &mut Motor, c: &mut Compartido) {
        egui::TopBottomPanel::top("barra_bloques").frame(tema::marco_panel()).show_inside(ui, |ui| self.barra(ui, motor));
        egui::SidePanel::left("paleta").default_width(218.0).width_range(170.0..=320.0).frame(tema::marco_panel()).show_inside(ui, |ui| self.paleta(ui));
        egui::SidePanel::right("inspector").default_width(300.0).width_range(240.0..=460.0).frame(tema::marco_panel()).show_inside(ui, |ui| self.inspector(ui, c));
        if !self.resultado.is_empty() || self.error.is_some() || !self.stats.is_empty() {
            egui::TopBottomPanel::bottom("osciloscopios")
                .resizable(true)
                .default_height(self.alto_osc)
                .height_range(120.0..=700.0)
                .frame(tema::marco_panel())
                .show_inside(ui, |ui| self.osciloscopios(ui, c));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(tema::p().fondo)).show_inside(ui, |ui| self.lienzo(ui));
        if self.ver_lineal {
            let mut abierta = true;
            if let Some(inf) = self.lineal.as_mut() {
                egui::Window::new("Modelo linealizado")
                    .open(&mut abierta)
                    .default_size([760.0, 640.0])
                    .default_pos(ui.max_rect().left_top() + Vec2::new(260.0, 30.0))
                    .show(ui.ctx(), |ui| {
                        ui.label(egui::RichText::new("La variable «lin» de la consola guarda A, B, C, D y G(s). Las herramientas de Control de Potencia la usan con «Planta: modelo linealizado».").small().color(tema::p().texto_2));
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            herramientas::dibujar_informe(ui, inf, c, "lin");
                        });
                    });
            }
            self.ver_lineal = abierta;
        }
    }

    fn barra(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("Nuevo").clicked() {
                self.instantanea();
                self.diagrama = diagrama_vacio();
                self.ruta.clear();
                self.archivo = None;
                self.resultado.clear();
                self.stats.clear();
            }
            if ui.button("Abrir").clicked() {
                if let Some(r) = rfd::FileDialog::new().add_filter("Diagrama JSON", &["json"]).pick_file() {
                    if let Err(e) = self.abrir_de(&r) {
                        self.error = Some(e);
                    }
                }
            }
            if ui.button("Guardar").clicked() {
                let ruta = self.archivo.clone().or_else(|| rfd::FileDialog::new().add_filter("Diagrama JSON", &["json"]).set_file_name("diagrama.json").save_file());
                if let Some(r) = ruta {
                    match self.guardar_en(&r) {
                        Ok(_) => self.archivo = Some(r),
                        Err(e) => self.error = Some(e),
                    }
                }
            }
            egui::ComboBox::from_id_salt("ejemplos").selected_text("Ejemplos").width(110.0).show_ui(ui, |ui| {
                for (k, n) in self.ejemplos.clone() {
                    if ui.selectable_label(false, n).clicked() {
                        self.pedir_ejemplo(motor, &k);
                    }
                }
            });
            ui.separator();
            ui.add(egui::TextEdit::singleline(&mut self.diagrama.nombre).desired_width(180.0).hint_text("Nombre del diagrama"));
            ui.separator();
            if ui.add_enabled(!self.deshacer.is_empty(), egui::Button::new("↶")).on_hover_text("Deshacer (Ctrl+Z)").clicked() {
                self.deshacer_ya();
            }
            if ui.add_enabled(!self.rehacer.is_empty(), egui::Button::new("↷")).on_hover_text("Rehacer").clicked() {
                self.rehacer_ya();
            }
            if ui.button("Encuadrar").clicked() {
                self.centrar = true;
            }
            ui.separator();
            let metodo = self.diagrama.solver.get("metodo").and_then(|v| v.as_str()).unwrap_or("dopri").to_string();
            let etiqueta = |m: &str| match m {
                "euler" => "Euler (fijo)",
                "rk4" => "RK4 (fijo)",
                "bdf" => "BDF (rígido)",
                _ => "Dormand-Prince",
            };
            egui::ComboBox::from_id_salt("solver").selected_text(etiqueta(&metodo)).width(130.0).show_ui(ui, |ui| {
                for m in ["euler", "rk4", "dopri", "bdf"] {
                    if ui.selectable_label(metodo == m, etiqueta(m)).clicked() {
                        self.diagrama.solver.insert("metodo".into(), json!(m));
                    }
                }
            });
            ui.label("Hasta");
            let mut tf = valor_txt(self.diagrama.solver.get("t_fin"));
            if ui.add(egui::TextEdit::singleline(&mut tf).desired_width(56.0)).changed() {
                self.diagrama.solver.insert("t_fin".into(), texto_a_valor(&tf));
            }
            ui.label("s");
            if self.id_sim.is_some() {
                if ui.button("■ Detener").clicked() {
                    motor.llamar("cancelar", json!({}));
                }
                ui.add(egui::ProgressBar::new(self.avance).desired_width(120.0).show_percentage());
            } else if tema::boton_principal(ui, "▶ Simular").clicked() {
                self.ejecutar(motor);
            }
            let ocupado = self.id_lin.is_some() || self.id_sim.is_some();
            if self.id_lin.is_some() {
                ui.spinner();
            }
            if ui
                .add_enabled(!ocupado, egui::Button::new("Linealizar aquí"))
                .on_hover_text("Modelo lineal en el punto de operación (simula hasta el final y refina con Newton). Entrada: la primera fuente. Salida: el bloque seleccionado o el primer osciloscopio. Abre su Bode, su escalón y sus polos.")
                .clicked()
            {
                self.linealizar(motor);
            }
            if self.lineal.is_some() && !self.ver_lineal && ui.small_button("Ver modelo lineal").clicked() {
                self.ver_lineal = true;
            }
        });
        // Migas del camino dentro de los subsistemas
        if !self.ruta.is_empty() {
            ui.horizontal(|ui| {
                if ui.small_button("Modelo").clicked() {
                    self.ruta.clear();
                    self.sel.clear();
                    self.centrar = true;
                }
                let r = self.ruta.clone();
                for (i, p) in r.iter().enumerate() {
                    ui.label(">");
                    let n = {
                        let mut s = &self.diagrama.sistema;
                        let mut nom = p.clone();
                        for q in &r[..=i] {
                            if let Some(b) = s.bloques.iter().find(|b| &b.id == q) {
                                nom = if b.nombre.is_empty() { b.id.clone() } else { b.nombre.clone() };
                                if let Some(x) = &b.sistema {
                                    s = x;
                                }
                            }
                        }
                        nom
                    };
                    if ui.small_button(n).clicked() {
                        self.ruta.truncate(i + 1);
                        self.sel.clear();
                        self.centrar = true;
                    }
                }
            });
        }
    }

    fn paleta(&mut self, ui: &mut egui::Ui) {
        tema::titulo(ui, "Librería de bloques");
        ui.add(egui::TextEdit::singleline(&mut self.buscar).hint_text("Buscar bloque...").desired_width(f32::INFINITY));
        ui.add_space(4.0);
        if self.fichas.is_empty() {
            ui.label(egui::RichText::new("Esperando al motor de cálculo...").color(tema::p().texto_2));
            return;
        }
        let filtro = self.buscar.to_lowercase();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for cat in self.orden_cat.clone() {
                let mut items: Vec<Ficha> = self.fichas.values().filter(|f| f.categoria == cat && (filtro.is_empty() || f.nombre.to_lowercase().contains(&filtro))).cloned().collect();
                if items.is_empty() {
                    continue;
                }
                items.sort_by(|a, b| a.nombre.cmp(&b.nombre));
                let abierta = !filtro.is_empty() || self.cat_abiertas.contains(&cat);
                let color = color_categoria(&cat);
                // El color de la familia va en un cuadrado de 7 px; el nombre, en TEXTO
                let cab = ui
                    .horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let a = ui.add(egui::Button::new(egui::RichText::new(if abierta { "▾" } else { "▸" }).color(tema::p().texto_2)).frame(false));
                        let (r, _) = ui.allocate_exact_size(Vec2::new(7.0, 16.0), Sense::hover());
                        tema::cuadrado(ui.painter(), r.center(), color);
                        let b = ui.add(egui::Button::new(egui::RichText::new(&cat).color(tema::p().texto).family(tema::semi())).frame(false));
                        a.union(b)
                    })
                    .inner;
                if cab.clicked() {
                    if abierta {
                        self.cat_abiertas.remove(&cat);
                    } else {
                        self.cat_abiertas.insert(cat.clone());
                    }
                }
                if abierta {
                    for f in items {
                        let r = ui.add(egui::Button::new(&f.nombre).sense(Sense::click_and_drag()).min_size(Vec2::new(ui.available_width(), 22.0)).fill(tema::p().campo));
                        if r.hovered() {
                            r.clone().on_hover_text(primera_linea(&f.doc));
                        }
                        if r.drag_started() {
                            self.nuevo = Some(f.tipo.clone());
                        }
                        if r.double_clicked() {
                            let n = self.sistema().bloques.len() as f32;
                            let pos = Vec2::new(120.0 + n * 16.0, 100.0 + n * 16.0);
                            self.agregar(&f.tipo, pos);
                        }
                    }
                }
            }
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui, c: &mut Compartido) {
        let unico = if self.sel.len() == 1 { self.sel.iter().next().cloned() } else { None };
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(id) = unico {
                self.inspector_bloque(ui, &id, c);
            } else {
                self.panel_solver(ui);
                if self.sel.len() > 1 {
                    ui.add_space(8.0);
                    tema::tarjeta(ui, |ui| {
                        ui.label(format!("{} bloques seleccionados", self.sel.len()));
                        ui.horizontal(|ui| {
                            if ui.button("Crear subsistema").clicked() {
                                self.agrupar();
                            }
                            if ui.button("Duplicar").clicked() {
                                self.duplicar();
                            }
                            if ui.button("Borrar").clicked() {
                                self.borrar_seleccion();
                            }
                        });
                    });
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(
                    "Arrastra bloques desde la librería al lienzo. Une puertos tirando de uno a otro: triángulos para señales, cuadrados para conexiones eléctricas. Doble clic en un subsistema para entrar. Mueve el lienzo con el botón derecho o dos dedos.",
                ).small().color(tema::p().texto_2));
            }
        });
    }

    fn panel_solver(&mut self, ui: &mut egui::Ui) {
        tema::titulo(ui, "Solver");
        let metodo = self.diagrama.solver.get("metodo").and_then(|v| v.as_str()).unwrap_or("dopri").to_string();
        let fijo = metodo == "euler" || metodo == "rk4";
        let d = &mut self.diagrama.solver;
        egui::Grid::new("g_solver").num_columns(3).spacing([6.0, 6.0]).show(ui, |ui| {
            let mut campo = |ui: &mut egui::Ui, etiq: &str, k: &str, unidad: &str, ayuda: &str, def: &str, habilitado: bool| {
                ui.label(etiq);
                let mut t = valor_txt(d.get(k));
                if t.is_empty() {
                    t = def.to_string();
                }
                let r = ui.add_enabled(habilitado, egui::TextEdit::singleline(&mut t).desired_width(90.0)).on_hover_text(ayuda);
                if r.changed() {
                    d.insert(k.into(), texto_a_valor(&t));
                }
                ui.label(egui::RichText::new(unidad).color(tema::p().texto_2));
                ui.end_row();
            };
            campo(ui, "Inicio", "t_ini", "s", "Tiempo inicial de la simulación", "0", true);
            campo(ui, "Fin", "t_fin", "s", "Tiempo final de la simulación", "10", true);
            campo(ui, "Paso fijo", "paso", "s", "Paso de Euler y RK4", "1e-3", fijo);
            campo(ui, "Tol. relativa", "rtol", "", "rtol: error relativo por paso (por defecto 1e-3)", "1e-3", !fijo);
            campo(ui, "Tol. absoluta", "atol", "", "atol: error absoluto por paso (por defecto 1e-6)", "1e-6", !fijo);
            campo(ui, "Paso máximo", "paso_max", "s", "Vacío: 1/10 del intervalo", "", !fijo);
        });
        ui.add_space(4.0);
        let txt = match metodo.as_str() {
            "euler" => "Euler explícito, orden 1. Sencillo; requiere pasos muy pequeños.",
            "rk4" => "Runge-Kutta clásico, orden 4. Recomendado para modelos conmutados con paso pequeño.",
            "bdf" => "BDF de orden variable (1 a 5). Para sistemas rígidos, como cargas con constantes de tiempo muy distintas.",
            _ => "Dormand-Prince 5(4) con paso variable. Recomendado para modelos suaves.",
        };
        ui.label(egui::RichText::new(txt).small().color(tema::p().texto_2));
        ui.add_space(10.0);
        self.panel_diagnostico(ui);
    }

    /// Fallas, alertas y OK de la última simulación, cada uno con su cuadrado y su texto.
    fn panel_diagnostico(&self, ui: &mut egui::Ui) {
        tema::titulo(ui, "Diagnóstico");
        tema::separador(ui);
        let mut hay = false;
        if let Some(e) = &self.error {
            tema::estado(ui, tema::Nivel::Falla, e);
            hay = true;
        }
        for a in &self.avisos {
            tema::estado(ui, tema::Nivel::Alerta, a);
            hay = true;
        }
        if self.error.is_none() && !self.stats.is_empty() {
            tema::estado(ui, tema::Nivel::Ok, "Simulación completada");
            hay = true;
        }
        if !hay {
            ui.label(egui::RichText::new("Simula el modelo para ver su diagnóstico.").small().color(tema::p().texto_2));
        }
    }

    fn inspector_bloque(&mut self, ui: &mut egui::Ui, id: &str, c: &mut Compartido) {
        let fichas = self.fichas.clone();
        let r = self.ruta.clone();
        let mut hacer: Option<&str> = None;
        let mut entrar = false;
        let mut cambio = false;
        {
            let s = nivel(&mut self.diagrama.sistema, &r);
            let Some(b) = s.bloques.iter_mut().find(|b| b.id == id) else { return };
            let ficha = fichas.get(&b.tipo).cloned().unwrap_or_default();
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(8.0, 16.0), Sense::hover());
                tema::cuadrado(ui.painter(), r.center(), color_categoria(&ficha.categoria));
                ui.label(egui::RichText::new(if ficha.nombre.is_empty() { &b.tipo } else { &ficha.nombre }).size(15.0).family(tema::semi()).color(tema::p().texto));
            });
            ui.add(egui::TextEdit::singleline(&mut b.nombre).desired_width(f32::INFINITY).hint_text("Nombre en el lienzo"));
            ui.add_space(4.0);
            if !ficha.params.is_empty() {
                tema::tarjeta(ui, |ui| {
                    egui::Grid::new("g_param").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                        for p in &ficha.params {
                            ui.label(&p.etiqueta);
                            ui.horizontal(|ui| {
                                let mut t = valor_txt(b.params.get(&p.nombre).or(Some(&p.defecto)));
                                match p.tipo.as_str() {
                                    "opcion" => {
                                        egui::ComboBox::from_id_salt(&p.nombre).selected_text(&t).width(130.0).show_ui(ui, |ui| {
                                            for o in &p.opciones {
                                                if ui.selectable_label(&t == o, o).clicked() {
                                                    b.params.insert(p.nombre.clone(), json!(o));
                                                    cambio = true;
                                                }
                                            }
                                        });
                                    }
                                    "codigo" => {
                                        if ui.add(egui::TextEdit::multiline(&mut t).code_editor().desired_rows(4).desired_width(150.0)).changed() {
                                            b.params.insert(p.nombre.clone(), json!(t));
                                            cambio = true;
                                        }
                                    }
                                    "ruta" => {
                                        if ui.add(egui::TextEdit::singleline(&mut t).desired_width(100.0)).changed() {
                                            b.params.insert(p.nombre.clone(), json!(t));
                                        }
                                        if ui.small_button("…").clicked() {
                                            if let Some(a) = rfd::FileDialog::new().add_filter("Datos", &["csv", "parquet"]).pick_file() {
                                                b.params.insert(p.nombre.clone(), json!(a.display().to_string()));
                                            }
                                        }
                                    }
                                    _ => {
                                        if ui.add(egui::TextEdit::singleline(&mut t).desired_width(105.0)).changed() {
                                            let v = if p.tipo == "num" { texto_a_valor(&t) } else { json!(t) };
                                            b.params.insert(p.nombre.clone(), v);
                                            cambio = true;
                                        }
                                        if !p.unidad.is_empty() {
                                            ui.label(egui::RichText::new(&p.unidad).color(tema::p().texto_2));
                                        }
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
                ui.label(egui::RichText::new("Admite expresiones y variables del espacio de trabajo, como 2*pi*50 o Vdc/2.").small().color(tema::p().texto_2));
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if b.tipo == "subsistema" && ui.button("Entrar").clicked() {
                    entrar = true;
                }
                if !ficha.archivo.is_empty() && ui.button("Ver y editar código").clicked() {
                    c.abrir_codigo = Some((ficha.archivo.clone(), ficha.linea));
                }
                if ui.button("Duplicar").clicked() {
                    hacer = Some("duplicar");
                }
                if ui.button("Borrar").clicked() {
                    hacer = Some("borrar");
                }
            });
            ui.add_space(6.0);
            if !ficha.doc.is_empty() {
                tema::titulo(ui, "Ecuaciones");
                ui.label(egui::RichText::new(&ficha.doc).family(egui::FontFamily::Monospace).size(12.5));
            }
        }
        if cambio {
            // Los puertos pueden cambiar con el parámetro: se quitan conexiones huérfanas
            let fichas = self.fichas.clone();
            let s = self.sistema();
            let nombres: HashMap<String, Vec<String>> = s.bloques.iter().map(|b| (b.id.clone(), puertos_de(b, &fichas).0.into_iter().map(|q| q.nombre).collect())).collect();
            s.conexiones.retain(|c| nombres.get(&c.de.0).is_some_and(|n| n.contains(&c.de.1)) && nombres.get(&c.a.0).is_some_and(|n| n.contains(&c.a.1)));
        }
        if entrar {
            self.ruta.push(id.to_string());
            self.sel.clear();
            self.centrar = true;
        }
        match hacer {
            Some("duplicar") => self.duplicar(),
            Some("borrar") => self.borrar_seleccion(),
            _ => {}
        }
    }

    fn osciloscopios(&mut self, ui: &mut egui::Ui, c: &mut Compartido) {
        // Pestañas a la izquierda (la activa con subrayado de 2 px) y el resumen a la derecha,
        // recortado para que nunca se monte sobre ellas
        let pal = tema::p();
        ui.horizontal(|ui| {
            tema::titulo(ui, "Resultados");
            ui.add_space(6.0);
            for (i, (n, _)) in self.resultado.iter().enumerate() {
                let activa = self.pestana == i;
                let r = ui.add(egui::Button::new(egui::RichText::new(n).color(if activa { pal.texto } else { pal.texto_2 })).frame(false));
                if activa {
                    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.rect.left(), r.rect.bottom() - 1.0), Pos2::new(r.rect.right(), r.rect.bottom() + 1.0)), 0.0, pal.acento);
                }
                if r.clicked() {
                    self.pestana = i;
                }
            }
            if !self.stats.is_empty() {
                let ancho = ui.available_width();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_max_width(ancho);
                    ui.add(egui::Label::new(egui::RichText::new(&self.stats).font(tema::fuente_mono(11.0)).color(pal.texto_3)).truncate()).on_hover_text(&self.stats);
                });
            }
        });
        tema::separador(ui);
        if let Some(e) = &self.error {
            tema::estado(ui, tema::Nivel::Falla, e);
        }
        for a in &self.avisos {
            tema::estado(ui, tema::Nivel::Alerta, a);
        }
        let alto = (ui.available_height() - 34.0).max(90.0);
        let i = self.pestana;
        if let Some((_, f)) = self.resultado.get_mut(i) {
            if let Some(acc) = grafica::mostrar(ui, f, egui::Id::new(("osc", i)), alto) {
                c.exportar(ui.ctx(), acc, f);
            }
        } else if self.resultado.is_empty() && self.error.is_none() {
            ui.label(egui::RichText::new("Añade un osciloscopio para ver señales.").color(tema::p().texto_2));
        }
    }

    fn lienzo(&mut self, ui: &mut egui::Ui) {
        let (area, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        if self.centrar {
            self.ajustar_vista(area);
            self.centrar = false;
        }
        let p = ui.painter_at(area);
        p.rect_filled(area, 0.0, tema::p().fondo);
        let origen = area.min.to_vec2() + self.pan;
        let z = self.zoom;
        // Rejilla de puntos cada 20 px; en el modo claro, además líneas cada 100 px
        let paso = 20.0 * z;
        if tema::modo() == tema::Modo::Claro && paso * 5.0 > 12.0 {
            let mayor = paso * 5.0;
            let trazo = Stroke::new(1.0, tema::p().linea);
            let mut x = area.left() + (origen.x - area.left()).rem_euclid(mayor);
            while x < area.right() {
                p.line_segment([Pos2::new(x, area.top()), Pos2::new(x, area.bottom())], trazo);
                x += mayor;
            }
            let mut y = area.top() + (origen.y - area.top()).rem_euclid(mayor);
            while y < area.bottom() {
                p.line_segment([Pos2::new(area.left(), y), Pos2::new(area.right(), y)], trazo);
                y += mayor;
            }
        }
        if paso > 6.0 {
            let c = tema::p().rejilla;
            let inicio_x = area.left() + (origen.x - area.left()).rem_euclid(paso);
            let inicio_y = area.top() + (origen.y - area.top()).rem_euclid(paso);
            let mut x = inicio_x;
            while x < area.right() {
                let mut y = inicio_y;
                while y < area.bottom() {
                    p.circle_filled(Pos2::new(x, y), 1.0, c);
                    y += paso;
                }
                x += paso;
            }
        }
        let a_pantalla = |x: f32, y: f32| Pos2::new(origen.x + x * z, origen.y + y * z);
        let puntero = ui.input(|i| i.pointer.hover_pos());
        let fichas = self.fichas.clone();
        let r = self.ruta.clone();
        let s = nivel(&mut self.diagrama.sistema, &r);
        // Geometría de bloques y puertos
        let geom: Vec<(Rect, Vec<(PuertoD, Pos2)>)> = s
            .bloques
            .iter()
            .map(|b| {
                let (pu, alto) = puertos_de(b, &fichas);
                let rect = Rect::from_min_size(a_pantalla(b.x, b.y), Vec2::new(ANCHO, alto) * z);
                let con: Vec<(PuertoD, Pos2)> = pu
                    .into_iter()
                    .map(|q| {
                        let pos = rect.min + q.rel * z;
                        (q, pos)
                    })
                    .collect();
                (rect, con)
            })
            .collect();
        let ids: Vec<String> = s.bloques.iter().map(|b| b.id.clone()).collect();
        let pos_puerto = |bid: &str, pn: &str| -> Option<(Pos2, Clase, bool)> {
            let i = ids.iter().position(|b| b == bid)?;
            geom[i].1.iter().find(|(q, _)| q.nombre == pn).map(|(q, pp)| (*pp, q.clase, q.izq))
        };
        // Bloques que forman un lazo algebraico en este nivel (un bloque de dentro marca su subsistema)
        let prefijo = if r.is_empty() { String::new() } else { format!("{}/", r.join("/")) };
        let mut en_lazo: HashSet<String> = HashSet::new();
        let mut nombres_lazo: Vec<String> = vec![];
        for lazo in &self.lazos {
            let mut aqui = vec![];
            for full in lazo {
                if let Some(resto) = full.strip_prefix(&prefijo) {
                    let id = resto.split('/').next().unwrap_or(resto).to_string();
                    if ids.contains(&id) && !aqui.contains(&id) {
                        aqui.push(id);
                    }
                }
            }
            if !aqui.is_empty() {
                let etiqueta = |id: &String| s.bloques.iter().find(|b| &b.id == id).map(|b| if b.nombre.is_empty() { fichas.get(&b.tipo).map(|f| f.nombre.clone()).unwrap_or(b.tipo.clone()) } else { b.nombre.clone() }).unwrap_or(id.clone());
                nombres_lazo.push(aqui.iter().take(3).map(etiqueta).collect::<Vec<_>>().join(" ↔ "));
                en_lazo.extend(aqui);
            }
        }
        // Hilos: eléctricos 2 px en cobre, señales 1,25 px en cian; los de un lazo, discontinuos en FALLA
        let mut hilo_cercano = None;
        for (ci, con) in s.conexiones.iter().enumerate() {
            let (Some(a), Some(b)) = (pos_puerto(&con.de.0, &con.de.1), pos_puerto(&con.a.0, &con.a.1)) else { continue };
            let pts = curva(a.0, a.2, b.0, b.2, z);
            let electrico = matches!(a.1, Clase::E | Clase::E3);
            let falla = en_lazo.contains(&con.de.0) && en_lazo.contains(&con.a.0);
            let mut trazo = tema::trazo_cable(electrico, falla);
            trazo.width *= z.clamp(0.7, 1.6);
            let elegido = self.sel_hilo == Some(ci);
            if let Some(pp) = puntero {
                if area.contains(pp) && pts.windows(2).any(|w| dist_seg(pp, w[0], w[1]) < 5.0) {
                    hilo_cercano = Some(ci);
                }
            }
            if elegido {
                p.add(Shape::line(pts.clone(), Stroke::new(trazo.width + 3.0, tema::p().acento.gamma_multiply(0.5))));
            }
            if falla {
                p.extend(Shape::dashed_line(&pts, trazo, 4.0, 3.0));
            } else if a.1 == Clase::E3 {
                // Trifásico: el mismo cable, con una línea central del color del fondo
                p.add(Shape::line(pts.clone(), Stroke::new(trazo.width * 1.6, trazo.color)));
                p.add(Shape::line(pts, Stroke::new(trazo.width * 0.5, tema::p().fondo)));
            } else {
                p.add(Shape::line(pts, trazo));
            }
        }
        // Bloques
        let pal = tema::p();
        let radio = tema::radio() as f32;
        let fuente = tema::fuente_semi((10.5 * z).max(6.5));
        let fuente_p = FontId::proportional((10.0 * z).max(6.0));
        let fuente_param = tema::fuente_mono((10.0 * z).max(6.0));
        for (i, b) in s.bloques.iter().enumerate() {
            let (rect, pu) = &geom[i];
            if !rect.intersects(area) {
                continue;
            }
            let fi = fichas.get(&b.tipo);
            let cat = fi.map(|f| f.categoria.as_str()).unwrap_or("");
            let color = color_categoria(cat);
            let sel = self.sel.contains(&b.id);
            let falla = en_lazo.contains(&b.id);
            p.rect_filled(*rect, radio, pal.campo);
            // Cabecera de 18 px en REALCE con la franja de 2 px de su familia
            let cabeza = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 18.0 * z));
            p.rect_filled(cabeza, egui::CornerRadius { nw: radio as u8, ne: radio as u8, sw: 0, se: 0 }, pal.realce);
            p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width(), 2.0 * z.max(0.5))), egui::CornerRadius { nw: radio as u8, ne: radio as u8, sw: 0, se: 0 }, color);
            p.line_segment([cabeza.left_bottom(), cabeza.right_bottom()], Stroke::new(1.0, pal.linea));
            p.rect_stroke(*rect, radio, Stroke::new(1.0, if falla { pal.falla } else { pal.linea_fuerte }), egui::StrokeKind::Inside);
            if sel {
                // Selección: contorno de 1,5 px a 4 px del nodo y cuatro tiradores de 6 px
                let fuera = rect.expand(4.0);
                p.rect_stroke(fuera, radio, Stroke::new(1.5, pal.acento), egui::StrokeKind::Middle);
                for esquina in [fuera.left_top(), fuera.right_top(), fuera.left_bottom(), fuera.right_bottom()] {
                    p.rect_filled(Rect::from_center_size(esquina, Vec2::splat(6.0)), 0.0, pal.acento);
                }
            }
            let nombre = if b.nombre.is_empty() { fi.map(|f| f.nombre.as_str()).unwrap_or(&b.tipo) } else { &b.nombre };
            let pin = p.with_clip_rect(rect.shrink(2.0));
            pin.text(rect.left_top() + Vec2::new(8.0 * z, 10.0 * z), Align2::LEFT_CENTER, nombre, fuente.clone(), pal.texto);
            if b.tipo == "subsistema" {
                pin.text(rect.right_top() + Vec2::new(-8.0 * z, 10.0 * z), Align2::RIGHT_CENTER, "⊞", fuente.clone(), pal.texto_2);
            }
            if z > 0.45 {
                let par = parametro_principal(b, fi);
                if !par.is_empty() {
                    pin.text(Pos2::new(rect.center().x, rect.bottom() - 8.0 * z), Align2::CENTER_CENTER, par, fuente_param.clone(), pal.texto_3);
                }
            }
            for (q, pp) in pu {
                let color_p = color_cable(q.clase);
                let t = 5.0 * z;
                match q.clase {
                    Clase::E => {
                        p.rect_filled(Rect::from_center_size(*pp, Vec2::splat(t * 1.6)), 0.0, color_p);
                    }
                    Clase::E3 => {
                        p.rect_filled(Rect::from_center_size(*pp, Vec2::new(t * 1.6, t * 2.2)), 0.0, color_p);
                        p.line_segment([*pp + Vec2::new(-t * 0.8, 0.0), *pp + Vec2::new(t * 0.8, 0.0)], Stroke::new(1.0, pal.campo));
                    }
                    Clase::Entrada => {
                        p.add(Shape::convex_polygon(vec![*pp + Vec2::new(-t * 0.5, -t), *pp + Vec2::new(t, 0.0), *pp + Vec2::new(-t * 0.5, t)], color_p, Stroke::NONE));
                    }
                    Clase::Salida => {
                        p.add(Shape::convex_polygon(vec![*pp + Vec2::new(-t, -t), *pp + Vec2::new(t * 0.5, 0.0), *pp + Vec2::new(-t, t)], color_p, Stroke::NONE));
                    }
                }
                if z > 0.4 {
                    let (al, d) = if q.izq { (Align2::LEFT_CENTER, 8.0 * z) } else { (Align2::RIGHT_CENTER, -8.0 * z) };
                    p.text(*pp + Vec2::new(d, 0.0), al, q.nombre.replace('_', " "), fuente_p.clone(), tema::p().texto_2);
                }
            }
        }
        // Puertos bajo el puntero
        let puerto_en = |pp: Pos2| -> Option<(String, String, Clase)> {
            let mut mejor = None;
            let mut d = 10.0f32.max(9.0 * z);
            for (i, bid) in ids.iter().enumerate() {
                for (q, pos) in &geom[i].1 {
                    let dd = pos.distance(pp);
                    if dd < d {
                        d = dd;
                        mejor = Some((bid.clone(), q.nombre.clone(), q.clase));
                    }
                }
            }
            mejor
        };
        let bloque_en = |pp: Pos2| -> Option<usize> { (0..ids.len()).rev().find(|&i| geom[i].0.contains(pp)) };
        if let Some(pp) = puntero.filter(|q| area.contains(*q)) {
            if let Some((_, _, cl)) = puerto_en(pp) {
                p.circle_stroke(pp, 7.0, Stroke::new(1.5, color_cable(cl)));
            }
        }
        // Etiqueta de lazo algebraico, arriba a la izquierda del lienzo
        let mut y_et = area.top() + 10.0;
        for n in &nombres_lazo {
            let texto = format!("LAZO ALGEBRAICO · {n}");
            let gal = ui.painter().layout_no_wrap(texto, tema::fuente_semi(11.0), pal.falla);
            let caja = Rect::from_min_size(Pos2::new(area.left() + 10.0, y_et), gal.size() + Vec2::new(26.0, 10.0));
            p.rect(caja, radio, pal.falla_fondo, Stroke::new(1.0, pal.falla), egui::StrokeKind::Inside);
            tema::cuadrado(&p, Pos2::new(caja.left() + 10.0, caja.center().y), pal.falla);
            p.galley(Pos2::new(caja.left() + 19.0, caja.top() + 5.0), gal, pal.falla);
            y_et += caja.height() + 6.0;
        }

        // ---------------------------------------- interacción
        let shift = ui.input(|i| i.modifiers.shift);
        let boton_sec = ui.input(|i| i.pointer.secondary_down() || i.pointer.middle_down());
        let mut accion_hilo: Option<Conexion> = None;
        let mut mover: Vec<(usize, Pos2)> = vec![];
        let mut entrar: Option<String> = None;
        if resp.double_clicked() {
            if let Some(pp) = resp.interact_pointer_pos() {
                if let Some(i) = bloque_en(pp) {
                    if s.bloques[i].tipo == "subsistema" {
                        entrar = Some(s.bloques[i].id.clone());
                    }
                }
            }
        }
        if resp.drag_started() {
            if let Some(pp) = resp.interact_pointer_pos() {
                if boton_sec {
                    self.arrastre = Arrastre::Lienzo;
                } else if let Some((bid, pn, cl)) = puerto_en(pp) {
                    self.arrastre = Arrastre::Hilo { bloque: bid, puerto: pn, clase: cl, inicio: pp };
                } else if let Some(i) = bloque_en(pp) {
                    let id = s.bloques[i].id.clone();
                    if !self.sel.contains(&id) {
                        if !shift {
                            self.sel.clear();
                        }
                        self.sel.insert(id);
                    }
                    let actual = (pp.to_vec2() - area.min.to_vec2() - self.pan) / z;
                    let m: Vec<(usize, Vec2)> = s.bloques.iter().enumerate().filter(|(_, b)| self.sel.contains(&b.id)).map(|(j, b)| (j, Vec2::new(b.x, b.y) - actual)).collect();
                    self.arrastre = Arrastre::Mover(m);
                    self.sel_hilo = None;
                } else {
                    if !shift {
                        self.sel.clear();
                    }
                    self.sel_hilo = None;
                    self.arrastre = Arrastre::Seleccion(pp);
                }
            }
        }
        if resp.clicked() {
            if let Some(pp) = resp.interact_pointer_pos() {
                if let Some(i) = bloque_en(pp) {
                    let id = s.bloques[i].id.clone();
                    if shift {
                        if !self.sel.remove(&id) {
                            self.sel.insert(id);
                        }
                    } else {
                        self.sel.clear();
                        self.sel.insert(id);
                    }
                    self.sel_hilo = None;
                } else {
                    self.sel.clear();
                    self.sel_hilo = hilo_cercano;
                }
            }
        }
        let mut pan_d = Vec2::ZERO;
        match &self.arrastre {
            Arrastre::Mover(m) if resp.dragged() => {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let actual = (pp.to_vec2() - area.min.to_vec2() - self.pan) / z;
                    for (j, off) in m {
                        let q = actual + *off;
                        mover.push((*j, Pos2::new((q.x / 4.0).round() * 4.0, (q.y / 4.0).round() * 4.0)));
                    }
                }
            }
            Arrastre::Lienzo if resp.dragged() => pan_d = resp.drag_delta(),
            Arrastre::Hilo { clase, inicio, .. } if resp.dragged() => {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let pts = curva(*inicio, *clase == Clase::Entrada, pp, *clase != Clase::Entrada, z);
                    p.add(Shape::line(pts, tema::trazo_cable(matches!(clase, Clase::E | Clase::E3), false)));
                }
            }
            Arrastre::Seleccion(a) if resp.dragged() => {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let r2 = Rect::from_two_pos(*a, pp);
                    p.rect(r2, 2.0, tema::p().acento.gamma_multiply(0.08), Stroke::new(1.0, tema::p().acento), egui::StrokeKind::Middle);
                    self.sel.clear();
                    for (i, bid) in ids.iter().enumerate() {
                        if r2.intersects(geom[i].0) {
                            self.sel.insert(bid.clone());
                        }
                    }
                }
            }
            _ => {}
        }
        if resp.drag_stopped() {
            if let Arrastre::Hilo { bloque, puerto, clase, .. } = &self.arrastre {
                if let Some(pp) = resp.interact_pointer_pos().or(puntero) {
                    if let Some((bb, pb, cl2)) = puerto_en(pp) {
                        if !(bb == *bloque && pb == *puerto) {
                            let c = match (*clase, cl2) {
                                (Clase::Salida, Clase::Entrada) => Some(Conexion { de: (bloque.clone(), puerto.clone()), a: (bb, pb) }),
                                (Clase::Entrada, Clase::Salida) => Some(Conexion { de: (bb, pb), a: (bloque.clone(), puerto.clone()) }),
                                (Clase::E, Clase::E) | (Clase::E3, Clase::E3) => Some(Conexion { de: (bloque.clone(), puerto.clone()), a: (bb, pb) }),
                                _ => None,
                            };
                            accion_hilo = c;
                        }
                    }
                }
            }
            self.arrastre = Arrastre::Nada;
        }
        // Clase del destino, calculada antes de modificar el sistema
        let destino = accion_hilo.as_ref().and_then(|con| {
            ids.iter().position(|b| *b == con.a.0).and_then(|i| geom[i].1.iter().find(|(q, _)| q.nombre == con.a.1).map(|(q, _)| q.clase))
        });
        for (j, q) in mover {
            s.bloques[j].x = q.x;
            s.bloques[j].y = q.y;
        }
        if let Some(con) = accion_hilo {
            // Una entrada de señal solo admite una fuente: el hilo nuevo reemplaza al anterior
            if destino == Some(Clase::Entrada) {
                s.conexiones.retain(|c| c.a != con.a);
            }
            if !s.conexiones.contains(&con) {
                s.conexiones.push(con);
            }
        }
        let vacio = s.bloques.is_empty();
        self.pan += pan_d;
        if resp.hovered() {
            let (scroll, zoom_d) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if zoom_d != 1.0 {
                if let Some(pp) = puntero {
                    let antes = (pp.to_vec2() - area.min.to_vec2() - self.pan) / self.zoom;
                    self.zoom = (self.zoom * zoom_d).clamp(0.2, 2.5);
                    self.pan = pp.to_vec2() - area.min.to_vec2() - antes * self.zoom;
                }
            } else if scroll != Vec2::ZERO {
                self.pan += scroll;
            }
        }
        if let Some(e) = entrar {
            self.ruta.push(e);
            self.sel.clear();
            self.centrar = true;
        }
        // Bloque nuevo arrastrado desde la paleta
        if let Some(tipo) = self.nuevo.clone() {
            if let Some(pp) = puntero {
                let capa = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("fantasma")));
                let r = Rect::from_center_size(pp, Vec2::new(ANCHO, 46.0) * self.zoom);
                capa.rect(r, tema::radio() as f32, tema::p().campo, Stroke::new(1.5, tema::p().acento), egui::StrokeKind::Middle);
                capa.text(r.center(), Align2::CENTER_CENTER, fichas.get(&tipo).map(|f| f.nombre.as_str()).unwrap_or(&tipo), FontId::proportional(12.0), tema::p().texto);
            }
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(pp) = puntero {
                    if area.contains(pp) {
                        let q = (pp.to_vec2() - area.min.to_vec2() - self.pan) / self.zoom - Vec2::new(ANCHO / 2.0, 23.0);
                        self.agregar(&tipo, Vec2::new((q.x / 4.0).round() * 4.0, (q.y / 4.0).round() * 4.0));
                    }
                }
                self.nuevo = None;
            }
        }
        // Teclado
        if !ui.ctx().wants_keyboard_input() {
            let (bor, des, dup, gru, reh) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                    i.modifiers.command && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                    i.modifiers.command && i.key_pressed(egui::Key::D),
                    i.modifiers.command && i.key_pressed(egui::Key::G),
                    i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z),
                )
            });
            if bor {
                self.borrar_seleccion();
            }
            if des {
                self.deshacer_ya();
            }
            if reh {
                self.rehacer_ya();
            }
            if dup {
                self.duplicar();
            }
            if gru {
                self.agrupar();
            }
        }
        // Texto de ayuda con el lienzo vacío
        if vacio {
            p.text(area.center(), Align2::CENTER_CENTER, "Arrastra bloques desde la librería o abre un ejemplo", FontId::proportional(16.0), tema::p().texto_2);
        }
    }
}

fn primera_linea(d: &str) -> String {
    d.lines().take(3).collect::<Vec<_>>().join("\n")
}

/// Texto a número si se puede; si no, se guarda la expresión tal cual.
pub fn texto_a_valor(t: &str) -> Value {
    match t.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => json!(v),
        _ => json!(t),
    }
}

/// Curva suave entre dos puertos, para un trazo propio y fácil de seguir.
fn curva(a: Pos2, a_izq: bool, b: Pos2, b_izq: bool, z: f32) -> Vec<Pos2> {
    let dist = ((b.x - a.x).abs() * 0.5).max(30.0 * z).min(180.0 * z);
    let c1 = a + Vec2::new(if a_izq { -dist } else { dist }, 0.0);
    let c2 = b + Vec2::new(if b_izq { -dist } else { dist }, 0.0);
    (0..=28)
        .map(|i| {
            let t = i as f32 / 28.0;
            let u = 1.0 - t;
            Pos2::new(
                u * u * u * a.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * b.x,
                u * u * u * a.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * b.y,
            )
        })
        .collect()
}

fn dist_seg(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::motor::Mensaje;

    /// Espera la respuesta a una petición del motor real.
    fn esperar(m: &mut Motor, id: u64) -> Value {
        for _ in 0..6000 {
            for r in m.recibir() {
                if let Mensaje::Respuesta { id: i, ok, valor, error } = r {
                    if i == id {
                        assert!(ok, "el motor respondió con un error: {error}");
                        return valor;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("el motor no respondió");
    }

    fn valor_final(res: &Value, traza: usize) -> f64 {
        let y = res["figuras"][0]["trazas"][traza]["y"].as_array().expect("traza");
        y.last().and_then(|v| v.as_f64()).expect("número")
    }

    fn preparar() -> (Motor, Editor) {
        let mut m = Motor::iniciar();
        let id = m.llamar("catalogo", json!({}));
        let cat = esperar(&mut m, id);
        let mut e = Editor::nuevo();
        e.fijar_catalogo(&cat);
        (m, e)
    }

    fn ejemplo(m: &mut Motor, e: &mut Editor, clave: &str) {
        let id = m.llamar("ejemplo", json!({"clave": clave}));
        let v = esperar(m, id);
        e.cargar_valor(&v);
    }

    fn simular(m: &mut Motor, e: &Editor) -> Value {
        let id = m.llamar("simular", json!({"diagrama": serde_json::to_value(&e.diagrama).unwrap()}));
        esperar(m, id)
    }

    /// Crear un subsistema con bloques eléctricos no debe cambiar el resultado.
    #[test]
    fn subsistema_electrico_conserva_el_resultado() {
        let (mut m, mut e) = preparar();
        ejemplo(&mut m, &mut e, "rc");
        let antes = valor_final(&simular(&mut m, &e), 0);
        e.sel = ["r", "c"].iter().map(|s| s.to_string()).collect();
        e.agrupar();
        assert!(e.diagrama.sistema.bloques.iter().any(|b| b.tipo == "subsistema"));
        let despues = valor_final(&simular(&mut m, &e), 0);
        assert!((antes - despues).abs() < 1e-6, "antes {antes}, después {despues}");
        // Sigue funcionando tras guardar y volver a leer el diagrama
        let t = serde_json::to_string(&e.diagrama).unwrap();
        let v: Value = serde_json::from_str(&t).unwrap();
        e.cargar_valor(&v);
        assert!((valor_final(&simular(&mut m, &e), 0) - antes).abs() < 1e-6);
    }

    /// Lo mismo con señales que entran y salen, dentro de un lazo cerrado.
    #[test]
    fn subsistema_de_senales_y_edicion() {
        let (mut m, mut e) = preparar();
        ejemplo(&mut m, &mut e, "lazo");
        let antes = valor_final(&simular(&mut m, &e), 1);
        e.sel = ["s", "pi"].iter().map(|s| s.to_string()).collect();
        e.agrupar();
        let despues = valor_final(&simular(&mut m, &e), 1);
        assert!((antes - despues).abs() < 1e-3, "antes {antes}, después {despues}");
        // Añadir, duplicar y borrar; deshacer recupera el estado anterior
        let n = e.diagrama.sistema.bloques.len();
        e.agregar("ganancia", Vec2::new(10.0, 10.0));
        assert_eq!(e.diagrama.sistema.bloques.len(), n + 1);
        e.duplicar();
        assert_eq!(e.diagrama.sistema.bloques.len(), n + 2);
        e.borrar_seleccion();
        assert_eq!(e.diagrama.sistema.bloques.len(), n + 1);
        e.deshacer_ya();
        assert_eq!(e.diagrama.sistema.bloques.len(), n + 2);
    }

    /// Los puertos que dibuja la ventana existen en el motor, en todos los bloques.
    #[test]
    fn guardado_canonico_y_diff_minimo() {
        let (mut m, mut e) = preparar();
        ejemplo(&mut m, &mut e, "lazo");
        let a = json_canonico(&e.diagrama).unwrap();
        // Desordenar bloques y conexiones no cambia el texto
        e.diagrama.sistema.bloques.reverse();
        e.diagrama.sistema.conexiones.reverse();
        assert_eq!(a, json_canonico(&e.diagrama).unwrap());
        // Cambiar un parámetro cambia una sola línea
        let b = e.diagrama.sistema.bloques.iter_mut().find(|b| b.id == "pi").unwrap();
        b.params.insert("Kp".into(), json!(3.0));
        let c = json_canonico(&e.diagrama).unwrap();
        let distintas = a.lines().zip(c.lines()).filter(|(x, y)| x != y).count();
        assert_eq!(a.lines().count(), c.lines().count());
        assert_eq!(distintas, 1, "el cambio de un parámetro debe tocar una línea");
        // Guardar dos veces: ficheros idénticos byte a byte
        let dir = std::env::temp_dir();
        let (r1, r2) = (dir.join("heliora_canon_1.json"), dir.join("heliora_canon_2.json"));
        e.guardar_en(&r1).unwrap();
        e.guardar_en(&r2).unwrap();
        assert_eq!(std::fs::read(&r1).unwrap(), std::fs::read(&r2).unwrap());
    }

    #[test]
    fn linealizar_el_lazo() {
        let (mut m, mut e) = preparar();
        ejemplo(&mut m, &mut e, "lazo");
        let v = serde_json::to_value(&e.diagrama).unwrap();
        let id = m.llamar("linealizar", json!({"diagrama": v}));
        let r = esperar(&mut m, id);
        let den: Vec<f64> = r["den"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        for (a, b) in den.iter().zip([1.0, 2.0, 3.0, 1.0]) {
            assert!((a - b).abs() < 1e-6, "denominador {den:?}");
        }
        assert!(r["procedencia"]["sha256"].as_str().unwrap().len() == 64);
        assert!(r["informe"]["secciones"].as_array().unwrap().len() > 3);
    }

    #[test]
    fn puertos_dinamicos() {
        let (_m, e) = preparar();
        let mut params = Map::new();
        params.insert("signos".into(), json!("+-+"));
        let b = BloqueD { id: "x".into(), tipo: "suma".into(), x: 0.0, y: 0.0, nombre: String::new(), params, sistema: None };
        let (p, _) = puertos_de(&b, &e.fichas);
        assert_eq!(p.iter().filter(|q| q.clase == Clase::Entrada).count(), 3);
        assert_eq!(p.iter().filter(|q| q.clase == Clase::Salida).count(), 1);
    }
}

/// Agrupa las fichas por categoría, para la página de referencia.
pub fn por_categoria(fichas: &HashMap<String, Ficha>) -> BTreeMap<String, Vec<Ficha>> {
    let mut m: BTreeMap<String, Vec<Ficha>> = BTreeMap::new();
    for f in fichas.values() {
        m.entry(f.categoria.clone()).or_default().push(f.clone());
    }
    for v in m.values_mut() {
        v.sort_by(|a, b| a.nombre.cmp(&b.nombre));
    }
    m
}


/// Ordena de forma estable un valor de diagrama: bloques por id y conexiones por extremos, en
/// todos los niveles. Las claves ya salen ordenadas (los mapas de serde_json son ordenados).
fn ordenar_canonico(v: &mut Value) {
    match v {
        Value::Object(m) => {
            if let Some(Value::Array(b)) = m.get_mut("bloques") {
                b.sort_by(|x, y| x["id"].as_str().unwrap_or("").cmp(y["id"].as_str().unwrap_or("")));
            }
            if let Some(Value::Array(c)) = m.get_mut("conexiones") {
                let clave = |x: &Value| format!("{}\u{1}{}\u{1}{}\u{1}{}", x["de"][0].as_str().unwrap_or(""), x["de"][1].as_str().unwrap_or(""), x["a"][0].as_str().unwrap_or(""), x["a"][1].as_str().unwrap_or(""));
                c.sort_by_key(clave);
            }
            for (_, x) in m.iter_mut() {
                ordenar_canonico(x);
            }
        }
        Value::Array(a) => {
            for x in a.iter_mut() {
                ordenar_canonico(x);
            }
        }
        _ => {}
    }
}

/// Texto JSON canónico de un diagrama: el mismo diagrama da siempre los mismos bytes.
pub fn json_canonico(d: &Diagrama) -> Result<String, String> {
    let mut v = serde_json::to_value(d).map_err(|e| e.to_string())?;
    ordenar_canonico(&mut v);
    let mut t = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    t.push('\n');
    Ok(t)
}
