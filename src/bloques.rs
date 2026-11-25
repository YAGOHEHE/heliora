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
    pub ayuda: String,
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
                                    ayuda: d["ayuda"].as_str().unwrap_or("").into(),
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

/// Texto del hover de un parámetro: su etiqueta y, si el bloque la da, su ayuda.
fn texto_ayuda(p: &ParamDef) -> String {
    if p.ayuda.is_empty() {
        p.etiqueta.clone()
    } else {
        format!("{}\n{}", p.etiqueta, p.ayuda)
    }
}

/// Número de bloques «Función Python» del sistema y de sus subsistemas: ejecutan código al simular.
pub fn bloques_con_codigo(s: &Sistema) -> usize {
    s.bloques.iter().map(|b| usize::from(b.tipo == "funcion_python") + b.sistema.as_ref().map_or(0, bloques_con_codigo)).sum()
}

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

/// Franja del nodo y marca de la librería: eléctrica si el bloque tiene puertos eléctricos,
/// señal si procesa señales; ninguna para fuentes y sumideros de señal.
fn color_ficha(f: &Ficha) -> Option<Color32> {
    if !f.e.is_empty() || !f.e3.is_empty() {
        Some(tema::p().electrica)
    } else if matches!(f.categoria.as_str(), "Fuentes" | "Sumideros") {
        None
    } else {
        Some(tema::p().senal)
    }
}

/// Grupo de la librería al que pertenece una categoría del catálogo.
fn grupo_de(cat: &str) -> String {
    match cat {
        "Matemática" | "Señales" => "Matemática y señales".into(),
        "Continuo" | "Discontinuidades" => "Continuo y discontinuidades".into(),
        c => c.split(" · ").next().unwrap_or(c).to_string(),
    }
}

fn etiqueta_metodo(m: &str) -> &'static str {
    match m {
        "euler" => "Euler (fijo)",
        "rk4" => "RK4 (fijo)",
        "bdf" => "BDF (rígido)",
        _ => "Dormand-Prince",
    }
}

/// Cable en ángulo recto: sale y entra en horizontal con un tramo vertical intermedio; si el
/// destino queda detrás del origen, rodea por la altura media.
fn ortogonal(a: Pos2, a_izq: bool, b: Pos2, b_izq: bool, z: f32) -> Vec<Pos2> {
    let tramo = 14.0 * z;
    let sa = if a_izq { -1.0 } else { 1.0 };
    let sb = if b_izq { -1.0 } else { 1.0 };
    let ax = a.x + sa * tramo;
    let bx = b.x + sb * tramo;
    if sa != sb && (bx - ax) * sa >= 0.0 {
        let mx = (ax + bx) / 2.0;
        vec![a, Pos2::new(mx, a.y), Pos2::new(mx, b.y), b]
    } else if sa == sb {
        let x = if sa > 0.0 { ax.max(bx) } else { ax.min(bx) };
        vec![a, Pos2::new(x, a.y), Pos2::new(x, b.y), b]
    } else {
        let my = a.y.max(b.y) + 40.0 * z;
        vec![a, Pos2::new(ax, a.y), Pos2::new(ax, my), Pos2::new(bx, my), Pos2::new(bx, b.y), b]
    }
}

/// Bloques del nivel indicado por la ruta, sin pedir acceso mutable.
fn nivel_ref<'a>(raiz: &'a Sistema, ruta: &[String]) -> &'a Sistema {
    let mut s = raiz;
    for r in ruta {
        match s.bloques.iter().find(|b| &b.id == r).and_then(|b| b.sistema.as_ref()) {
            Some(x) => s = x,
            None => break,
        }
    }
    s
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

/// Versiones de Python y de las librerías principales a partir de un bloque `procedencia` del motor.
pub fn versiones_de(p: &Value) -> String {
    let v = |k: &str| p["librerias"][k].as_str().map(|x| format!(" {x}")).unwrap_or_default();
    let py = p["python"].as_str().map(|x| format!(" {x}")).unwrap_or_default();
    format!("Python{py} · NumPy{} · SciPy{} · pvlib{}", v("numpy"), v("scipy"), v("pvlib"))
}

// ================================================================= Estado del editor

/// Resumen de la última simulación para el inspector.
pub struct Ultima {
    pub solver: String,
    pub paso: String,
    pub huella: String,
}

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
    /// Tipos de bloque añadidos hace poco, el más reciente primero (como mucho 3)
    pub recientes: Vec<String>,
    /// Cables en ángulo recto en lugar de curvas
    pub ordenar_cables: bool,
    /// JSON canónico del diagrama tal como se guardó o abrió; vacío si no se ha guardado
    guardado_json: String,
    /// Solver con el que se lanzó la simulación en curso
    solver_enviado: Map<String, Value>,
    pub ultima: Option<Ultima>,
    /// Procedencia de la última simulación (huellas y versiones del motor)
    procedencia: Value,
    /// Zona del lienzo en el último cuadro, para colocar bloques en su centro
    area_lienzo: Rect,
    /// Aviso al abrir un diagrama de un archivo con bloques que ejecutan código; se cierra con «Entendido»
    pub aviso_apertura: Option<String>,
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
            cat_abiertas: ["Fotovoltaica"].iter().map(|s| s.to_string()).collect(),
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
            recientes: vec![],
            ordenar_cables: false,
            guardado_json: String::new(),
            solver_enviado: Map::new(),
            ultima: None,
            procedencia: Value::Null,
            area_lienzo: Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 500.0)),
            aviso_apertura: None,
        }
    }

    pub fn fijar_catalogo(&mut self, v: &Value) {
        let f = fichas_desde(v);
        let mut cats: Vec<String> = f.iter().map(|x| x.categoria.clone()).collect();
        cats.sort();
        cats.dedup();
        let prio = ["Fuentes", "Matemática", "Señales", "Continuo", "Discontinuidades", "Discreto", "Sumideros", "Eléctrica · Pasivos", "Eléctrica · Fuentes", "Eléctrica · Semiconductores", "Eléctrica · Medición", "Potencia · Convertidores", "Potencia · Filtros", "Potencia · Modulación", "Potencia · Control", "Potencia · Medición", "Fotovoltaica", "Almacenamiento", "Máquinas", "Mecánica", "Térmica", "Subsistemas"];
        let mut orden: Vec<String> = prio.iter().filter(|p| cats.iter().any(|c| c == *p)).map(|s| s.to_string()).collect();
        for c in cats {
            if !orden.contains(&c) {
                orden.push(c);
            }
        }
        self.orden_cat = orden;
        self.fichas = f.into_iter().map(|x| (x.tipo.clone(), x)).collect();
    }

    /// Carga un diagrama desde JSON; si no lo es, deja el actual intacto y devuelve el error.
    pub fn cargar_valor(&mut self, v: &Value) -> Result<(), String> {
        let d = serde_json::from_value::<Diagrama>(v.clone()).map_err(|_| "El archivo no es un diagrama válido.".to_string())?;
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
        self.guardado_json.clear();
        self.aviso_apertura = None;
        self.ultima = None;
        self.lazos.clear();
        Ok(())
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
                match self.cargar_valor(valor) {
                    Ok(()) => self.archivo = None,
                    Err(e) => self.error = Some(e),
                }
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
                self.procedencia = valor["procedencia"].clone();
                if self.pestana >= self.resultado.len() {
                    self.pestana = 0;
                }
                c.fijar_variables(&valor["variables"]);
            } else {
                self.error = Some(err.to_string());
                self.procedencia = Value::Null;
            }
            self.ultima = Some(self.resumen_ultima());
            return true;
        }
        false
    }

    /// Solver, paso y huella de la simulación que acaba de terminar.
    fn resumen_ultima(&self) -> Ultima {
        let d = &self.solver_enviado;
        let metodo = d.get("metodo").and_then(|v| v.as_str()).unwrap_or("dopri");
        let fijo = metodo == "euler" || metodo == "rk4";
        let paso = if fijo { format!("{} s", valor_txt(d.get("paso"))) } else { format!("variable · rtol {}", valor_txt(d.get("rtol"))) };
        Ultima {
            solver: format!("{metodo} · {}", if fijo { "paso fijo" } else { "paso variable" }),
            paso,
            huella: self.procedencia["sha256"].as_str().unwrap_or("").to_string(),
        }
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
        self.recientes.retain(|t| t != tipo);
        self.recientes.insert(0, tipo.to_string());
        self.recientes.truncate(3);
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
                self.solver_enviado = self.diagrama.solver.clone();
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
    /// extremos y sangría fija, para que un cambio pequeño dé un diff de Git pequeño. Esa ruta
    /// pasa a ser el archivo del diagrama, contra el que se miden los cambios sin guardar.
    pub fn guardar_en(&mut self, ruta: &std::path::Path) -> Result<(), String> {
        let t = json_canonico(&self.diagrama)?;
        crate::app::escribir_atomico(ruta, t.as_bytes())?;
        self.guardado_json = t;
        self.archivo = Some(ruta.to_path_buf());
        Ok(())
    }

    pub fn abrir_de(&mut self, ruta: &std::path::Path) -> Result<(), String> {
        let t = std::fs::read_to_string(ruta).map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
        self.cargar_valor(&v)?;
        self.archivo = Some(ruta.to_path_buf());
        self.guardado_json = json_canonico(&self.diagrama).unwrap_or_default();
        let n = bloques_con_codigo(&self.diagrama.sistema);
        if n > 0 {
            self.aviso_apertura = Some(format!("Este diagrama contiene {n} bloque(s) «Función Python»: ejecutan código Python al simular. Ábrelo solo si confías en su origen."));
        }
        Ok(())
    }

    // ============================================================= API para la ventana

    /// Hay cambios sin guardar (o el diagrama nunca se ha guardado).
    pub fn sucio(&self) -> bool {
        self.guardado_json.is_empty() || json_canonico(&self.diagrama).ok().as_deref() != Some(self.guardado_json.as_str())
    }

    /// Resumen del solver para la barra superior, p. ej. «RK4 fijo · 0 → 1.0 s · h 5e-5».
    pub fn resumen_solver(&self) -> String {
        let d = &self.diagrama.solver;
        let metodo = d.get("metodo").and_then(|v| v.as_str()).unwrap_or("dopri");
        let nombre = match metodo {
            "euler" => "Euler fijo",
            "rk4" => "RK4 fijo",
            "bdf" => "BDF",
            _ => "Dormand-Prince",
        };
        let t0 = valor_txt(d.get("t_ini"));
        let t0 = if t0.is_empty() { "0".to_string() } else { t0 };
        let fin = if matches!(metodo, "euler" | "rk4") { format!("h {}", valor_txt(d.get("paso"))) } else { format!("rtol {}", valor_txt(d.get("rtol"))) };
        format!("{nombre} · {t0} → {} s · {fin}", valor_txt(d.get("t_fin")))
    }

    /// Ajustes del solver en el desplegable de la barra superior.
    pub fn panel_solver_rapido(&mut self, ui: &mut egui::Ui) {
        tema::titulo(ui, "Solver");
        ui.add_space(4.0);
        self.panel_solver(ui);
    }

    /// Versiones de Python y de las librerías con las que corrió la última simulación.
    pub fn versiones(&self) -> String {
        versiones_de(&self.procedencia)
    }

    /// Indica si ya hay una simulación con su procedencia (huella y versiones).
    pub fn tiene_procedencia(&self) -> bool {
        !self.procedencia.is_null()
    }

    /// Bloques del nivel que se está viendo.
    pub fn bloques_visibles(&self) -> usize {
        nivel_ref(&self.diagrama.sistema, &self.ruta).bloques.len()
    }

    /// Bloques del catálogo y ejemplos, para la paleta de comandos.
    pub fn entradas_paleta(&self) -> Vec<crate::comandos::Entrada> {
        use crate::comandos::{Accion, Entrada};
        let mut f: Vec<&Ficha> = self.fichas.values().collect();
        f.sort_by(|a, b| a.nombre.cmp(&b.nombre));
        let mut v: Vec<Entrada> = f.into_iter().map(|x| Entrada::nueva("Bloque", x.nombre.clone(), x.categoria.clone(), Accion::Bloque(x.tipo.clone()))).collect();
        v.extend(self.ejemplos.iter().map(|(k, n)| Entrada::nueva("Ejemplo", n.clone(), "", Accion::Ejemplo(k.clone()))));
        v
    }

    pub fn encuadrar(&mut self) {
        self.centrar = true;
    }

    /// Añade un bloque en el centro de la vista actual (rejilla de 4 px).
    pub fn agregar_al_centro(&mut self, tipo: &str) {
        let a = self.area_lienzo;
        let q = (a.center() - a.min - self.pan) / self.zoom - Vec2::new(ANCHO / 2.0, 32.0);
        self.agregar(tipo, Vec2::new((q.x / 4.0).round() * 4.0, (q.y / 4.0).round() * 4.0));
    }

    pub fn nuevo_diagrama(&mut self) {
        self.instantanea();
        self.diagrama = diagrama_vacio();
        self.ruta.clear();
        self.sel.clear();
        self.archivo = None;
        self.guardado_json.clear();
        self.aviso_apertura = None;
        self.resultado.clear();
        self.stats.clear();
        self.ultima = None;
        self.error = None;
        self.avisos.clear();
        self.lazos.clear();
    }

    pub fn abrir_diagrama(&mut self) {
        if let Some(r) = rfd::FileDialog::new().add_filter("Diagrama JSON", &["json"]).pick_file() {
            if let Err(e) = self.abrir_de(&r) {
                self.error = Some(e);
            }
        }
    }

    pub fn guardar_diagrama(&mut self) {
        let ruta = self.archivo.clone().or_else(|| rfd::FileDialog::new().add_filter("Diagrama JSON", &["json"]).set_file_name("diagrama.json").save_file());
        if let Some(r) = ruta {
            if let Err(e) = self.guardar_en(&r) {
                self.error = Some(e);
            }
        }
    }

    // ============================================================= Interfaz

    pub fn ui(&mut self, ui: &mut egui::Ui, motor: &mut Motor, c: &mut Compartido) {
        let pal = tema::p();
        let marco = |m: egui::Margin| egui::Frame::new().fill(pal.panel).inner_margin(m).stroke(Stroke::new(1.0_f32, pal.linea));
        egui::SidePanel::left("paleta").default_width(232.0).width_range(190.0..=340.0).frame(marco(egui::Margin { left: 12, right: 12, top: 12, bottom: 8 })).show_inside(ui, |ui| self.paleta(ui));
        egui::SidePanel::right("inspector").default_width(280.0).width_range(240.0..=460.0).frame(marco(egui::Margin::ZERO)).show_inside(ui, |ui| self.inspector(ui, c));
        egui::TopBottomPanel::top("barra_bloques").exact_height(40.0).frame(marco(egui::Margin::symmetric(10, 0))).show_inside(ui, |ui| self.barra(ui, motor));
        if !self.resultado.is_empty() || self.error.is_some() || !self.stats.is_empty() {
            egui::TopBottomPanel::bottom("osciloscopios")
                .resizable(true)
                .default_height(self.alto_osc)
                .height_range(140.0..=700.0)
                .frame(marco(egui::Margin::ZERO))
                .show_inside(ui, |ui| self.osciloscopios(ui, c));
        }
        if let Some(a) = self.aviso_apertura.clone() {
            let mut cerrar = false;
            egui::TopBottomPanel::top("aviso_codigo").frame(marco(egui::Margin::symmetric(12, 6))).show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("⚠").color(pal.alerta));
                    ui.add(egui::Label::new(egui::RichText::new(&a).color(pal.texto)).wrap());
                    cerrar = ui.button("Entendido").clicked();
                });
            });
            if cerrar {
                self.aviso_apertura = None;
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(pal.fondo)).show_inside(ui, |ui| self.lienzo(ui));
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

    /// Barra del lienzo: edición, vista, linealización, migas de subsistema, leyenda y zoom.
    fn barra(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        let pal = tema::p();
        ui.horizontal_centered(|ui| {
            if ui.add_enabled(!self.deshacer.is_empty(), egui::Button::new("↶").min_size(Vec2::new(30.0, 28.0))).on_hover_text("Deshacer (Ctrl+Z)").clicked() {
                self.deshacer_ya();
            }
            if ui.add_enabled(!self.rehacer.is_empty(), egui::Button::new("↷").min_size(Vec2::new(30.0, 28.0))).on_hover_text("Rehacer (Ctrl+Mayús+Z)").clicked() {
                self.rehacer_ya();
            }
            if ui.button("Encuadrar").on_hover_text("Ajusta la vista a todo el diagrama").clicked() {
                self.encuadrar();
            }
            if ui.add(egui::Button::new("Ordenar cables").selected(self.ordenar_cables)).on_hover_text("Traza los cables en ángulo recto en lugar de en curva").clicked() {
                self.ordenar_cables = !self.ordenar_cables;
            }
            let ocupado = self.id_lin.is_some() || self.id_sim.is_some();
            if ui
                .add_enabled(!ocupado, egui::Button::new("Linealizar aquí"))
                .on_hover_text("Modelo lineal en el punto de operación (simula hasta el final y refina con Newton). Entrada: la primera fuente. Salida: el bloque seleccionado o el primer osciloscopio. Abre su Bode, su escalón y sus polos.")
                .clicked()
            {
                self.linealizar(motor);
            }
            if self.id_lin.is_some() {
                ui.spinner();
            }
            if self.lineal.is_some() && !self.ver_lineal && ui.button("Ver modelo lineal").clicked() {
                self.ver_lineal = true;
            }
            // Migas del camino dentro de los subsistemas
            if !self.ruta.is_empty() {
                ui.add_space(8.0);
                if ui.add(egui::Button::new(egui::RichText::new("Modelo").color(pal.texto_3)).frame(false)).clicked() {
                    self.ruta.clear();
                    self.sel.clear();
                    self.centrar = true;
                }
                let r = self.ruta.clone();
                for (i, p) in r.iter().enumerate() {
                    ui.label(egui::RichText::new("/").color(pal.texto_3));
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
                    let color = if i + 1 == r.len() { pal.texto } else { pal.texto_3 };
                    if ui.add(egui::Button::new(egui::RichText::new(n).color(color)).frame(false)).clicked() {
                        self.ruta.truncate(i + 1);
                        self.sel.clear();
                        self.centrar = true;
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if ui.add(egui::Button::new(egui::RichText::new(format!("{:.0} %", self.zoom * 100.0)).font(tema::fuente_mono(12.0)).color(pal.texto_3)).frame(false)).on_hover_text("Volver al 100 %").clicked() {
                    self.zoom = 1.0;
                }
                for (texto, color) in [("señal", pal.senal), ("eléctrica", pal.electrica)] {
                    ui.label(egui::RichText::new(texto).size(12.0).color(pal.texto_3));
                    let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 2.0), Sense::hover());
                    ui.painter().rect_filled(r, 0.0, color);
                }
            });
        });
    }

    /// Fila de la librería: altura 28 px, fondo REALCE al pasar el puntero; se puede arrastrar.
    fn fila_libreria(ui: &mut egui::Ui, texto: &str, sangria: f32, marca: Option<Color32>, color: Color32) -> egui::Response {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click_and_drag());
        let pal = tema::p();
        if resp.hovered() {
            ui.painter().rect_filled(r, 5.0, pal.realce);
        }
        let mut x = r.left() + 8.0 + sangria;
        if let Some(c) = marca {
            tema::cuadrado(ui.painter(), Pos2::new(x + 4.0, r.center().y), c);
            x += 16.0;
        }
        let recorte = Rect::from_min_max(r.min, Pos2::new(r.right() - 26.0, r.bottom()));
        ui.painter().with_clip_rect(recorte).text(Pos2::new(x, r.center().y), Align2::LEFT_CENTER, texto, FontId::proportional(13.0), color);
        resp
    }

    /// Arrastrar una ficha al lienzo la añade donde se suelta; el doble clic, en cascada.
    fn atender_ficha(&mut self, r: &egui::Response, f: &Ficha) {
        if r.hovered() && !f.doc.is_empty() {
            r.clone().on_hover_text(primera_linea(&f.doc));
        }
        if r.drag_started() {
            self.nuevo = Some(f.tipo.clone());
        }
        if r.double_clicked() {
            self.agregar_al_centro(&f.tipo);
        }
    }

    fn paleta(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        tema::titulo(ui, "Librería");
        ui.add_space(2.0);
        egui::Frame::new().fill(pal.fondo).stroke(Stroke::new(1.0_f32, pal.borde)).corner_radius(6).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::new(13.0, 18.0), Sense::hover());
                let c = r.center() - Vec2::new(1.0, 1.0);
                ui.painter().circle_stroke(c, 4.0, Stroke::new(1.5_f32, pal.texto_3));
                ui.painter().line_segment([c + Vec2::new(3.0, 3.0), c + Vec2::new(5.5, 5.5)], Stroke::new(1.5_f32, pal.texto_3));
                ui.add(egui::TextEdit::singleline(&mut self.buscar).hint_text("Filtrar…").frame(false).desired_width(f32::INFINITY));
            });
        });
        ui.add_space(4.0);
        if self.fichas.is_empty() {
            ui.label(egui::RichText::new("Esperando al motor de cálculo…").color(pal.texto_2));
            return;
        }
        let filtro = self.buscar.to_lowercase();
        let seccion = |ui: &mut egui::Ui, t: &str| {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(t).size(11.0).color(pal.texto_3));
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            if filtro.is_empty() && !self.recientes.is_empty() {
                seccion(ui, "Recientes");
                for t in self.recientes.clone() {
                    let Some(f) = self.fichas.get(&t).cloned() else { continue };
                    let r = Self::fila_libreria(ui, &f.nombre, 0.0, Some(color_ficha(&f).unwrap_or(pal.linea_fuerte)), pal.texto);
                    self.atender_ficha(&r, &f);
                }
            }
            seccion(ui, "Categorías");
            for (grupo, cats) in self.grupos() {
                let mut items: Vec<(String, Ficha)> = vec![];
                for cat in &cats {
                    let mut lista: Vec<Ficha> = self.fichas.values().filter(|f| &f.categoria == cat && (filtro.is_empty() || f.nombre.to_lowercase().contains(&filtro))).cloned().collect();
                    lista.sort_by(|a, b| a.nombre.cmp(&b.nombre));
                    items.extend(lista.into_iter().map(|f| (cat.clone(), f)));
                }
                if items.is_empty() {
                    continue;
                }
                let abierta = !filtro.is_empty() || self.cat_abiertas.contains(&grupo);
                let r = Self::fila_libreria(ui, &grupo, 16.0, None, if abierta { pal.texto } else { pal.texto_2 });
                let flecha = Pos2::new(r.rect.left() + 12.0, r.rect.center().y);
                let puntos = if abierta {
                    vec![flecha + Vec2::new(-3.5, -2.0), flecha + Vec2::new(0.0, 2.0), flecha + Vec2::new(3.5, -2.0)]
                } else {
                    vec![flecha + Vec2::new(-1.5, -3.5), flecha + Vec2::new(2.0, 0.0), flecha + Vec2::new(-1.5, 3.5)]
                };
                ui.painter().add(Shape::line(puntos, Stroke::new(1.4_f32, pal.texto_3)));
                ui.painter().text(r.rect.right_center() - Vec2::new(8.0, 0.0), Align2::RIGHT_CENTER, items.len().to_string(), tema::fuente_mono(11.0), pal.texto_3);
                if r.clicked() {
                    if abierta {
                        self.cat_abiertas.remove(&grupo);
                    } else {
                        self.cat_abiertas.insert(grupo.clone());
                    }
                }
                if abierta {
                    let subtitulos = cats.len() > 1;
                    let mut actual = String::new();
                    for (cat, f) in items {
                        if subtitulos && cat != actual {
                            let sub = cat.split(" · ").last().unwrap_or(&cat).to_string();
                            ui.add_space(3.0);
                            ui.horizontal(|ui| {
                                ui.add_space(26.0);
                                ui.label(egui::RichText::new(sub).size(11.0).color(pal.texto_3));
                            });
                            actual = cat;
                        }
                        let r = Self::fila_libreria(ui, &f.nombre, 18.0, None, pal.texto_2);
                        self.atender_ficha(&r, &f);
                    }
                }
            }
        });
    }

    /// Categorías de la librería agrupadas como en el diseño, en el orden del catálogo.
    fn grupos(&self) -> Vec<(String, Vec<String>)> {
        let mut v: Vec<(String, Vec<String>)> = vec![];
        for cat in &self.orden_cat {
            let g = grupo_de(cat);
            match v.iter_mut().find(|(n, _)| *n == g) {
                Some((_, l)) => l.push(cat.clone()),
                None => v.push((g, vec![cat.clone()])),
            }
        }
        v
    }

    fn inspector(&mut self, ui: &mut egui::Ui, c: &mut Compartido) {
        let unico = if self.sel.len() == 1 { self.sel.iter().next().cloned() } else { None };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let m = egui::Margin { left: 16, right: 16, top: 14, bottom: 14 };
            egui::Frame::new().inner_margin(m).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if let Some(id) = unico {
                    self.inspector_bloque(ui, &id, c);
                } else {
                    tema::titulo(ui, "Solver");
                    ui.add_space(4.0);
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
                    ).small().color(tema::p().texto_3));
                }
            });
            tema::separador(ui);
            egui::Frame::new().inner_margin(m).show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.panel_ultima(ui);
            });
        });
    }

    /// Método, intervalo, paso y tolerancias del solver.
    pub fn panel_solver(&mut self, ui: &mut egui::Ui) {
        let metodo = self.diagrama.solver.get("metodo").and_then(|v| v.as_str()).unwrap_or("dopri").to_string();
        let fijo = metodo == "euler" || metodo == "rk4";
        let d = &mut self.diagrama.solver;
        egui::Grid::new("g_solver").num_columns(3).spacing([8.0, 8.0]).min_col_width(24.0).show(ui, |ui| {
            ui.label(egui::RichText::new("Método").color(tema::p().texto_2));
            egui::ComboBox::from_id_salt("solver").selected_text(etiqueta_metodo(&metodo)).width(130.0).show_ui(ui, |ui| {
                for m in ["euler", "rk4", "dopri", "bdf"] {
                    if ui.selectable_label(metodo == m, etiqueta_metodo(m)).clicked() {
                        d.insert("metodo".into(), json!(m));
                    }
                }
            });
            ui.end_row();
            let mut campo = |ui: &mut egui::Ui, etiq: &str, k: &str, unidad: &str, ayuda: &str, def: &str, habilitado: bool| {
                ui.label(egui::RichText::new(etiq).color(tema::p().texto_2));
                let mut t = valor_txt(d.get(k));
                if t.is_empty() {
                    t = def.to_string();
                }
                let r = ui.add_enabled(habilitado, egui::TextEdit::singleline(&mut t).font(tema::fuente_mono(12.0)).desired_width(120.0)).on_hover_text(ayuda);
                if r.changed() {
                    d.insert(k.into(), texto_a_valor(&t));
                }
                ui.label(egui::RichText::new(unidad).color(tema::p().texto_3));
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
        ui.label(egui::RichText::new(txt).small().color(tema::p().texto_3));
    }

    /// Estado, solver, paso y huella de la última simulación.
    fn panel_ultima(&self, ui: &mut egui::Ui) {
        let pal = tema::p();
        tema::titulo(ui, "Última simulación");
        ui.add_space(2.0);
        let Some(u) = &self.ultima else {
            ui.label(egui::RichText::new("Simula el modelo (F5) para ver su resultado.").color(pal.texto_3));
            return;
        };
        if let Some(e) = &self.error {
            tema::estado(ui, tema::Nivel::Falla, "Terminó con error").on_hover_text(e);
        } else if !self.avisos.is_empty() {
            tema::estado(ui, tema::Nivel::Alerta, &format!("Completada con {} avisos", self.avisos.len()));
        } else {
            tema::estado(ui, tema::Nivel::Ok, "Completada sin avisos");
        }
        ui.add_space(2.0);
        egui::Grid::new("g_ultima").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            let fila = |ui: &mut egui::Ui, k: &str, v: &str, c: Color32| {
                ui.label(egui::RichText::new(k).size(12.0).color(pal.texto_2));
                ui.label(egui::RichText::new(v).font(tema::fuente_mono(12.0)).color(c))
            };
            fila(ui, "Solver", &u.solver, pal.texto);
            ui.end_row();
            fila(ui, "Paso", &u.paso, pal.texto);
            ui.end_row();
            if !u.huella.is_empty() {
                let corta = format!("sha256 {}…", &u.huella[..u.huella.len().min(10)]);
                let r = fila(ui, "Huella", &corta, pal.texto_2).interact(Sense::click()).on_hover_text(format!("{}\nClic para copiar", u.huella));
                if r.clicked() {
                    ui.ctx().copy_text(u.huella.clone());
                }
                ui.end_row();
            }
        });
    }

    fn inspector_bloque(&mut self, ui: &mut egui::Ui, id: &str, c: &mut Compartido) {
        let pal = tema::p();
        let fichas = self.fichas.clone();
        let r = self.ruta.clone();
        let mut hacer: Option<&str> = None;
        let mut entrar = false;
        let mut cambio = false;
        {
            let s = nivel(&mut self.diagrama.sistema, &r);
            let Some(b) = s.bloques.iter_mut().find(|b| b.id == id) else { return };
            let ficha = fichas.get(&b.tipo).cloned().unwrap_or_default();
            tema::titulo(ui, "Bloque seleccionado");
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(if ficha.nombre.is_empty() { &b.tipo } else { &ficha.nombre }).size(18.0).family(tema::semi()).color(pal.texto));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(&ficha.categoria).size(12.0).color(pal.texto_3)).truncate());
                });
            });
            ui.add_space(6.0);
            egui::Grid::new("g_param").num_columns(3).spacing([8.0, 8.0]).min_col_width(24.0).show(ui, |ui| {
                ui.label(egui::RichText::new("Nombre").size(12.0).color(pal.texto_2));
                ui.add(egui::TextEdit::singleline(&mut b.nombre).desired_width(130.0).hint_text("Nombre en el lienzo"));
                ui.end_row();
                for p in &ficha.params {
                    ui.add(egui::Label::new(egui::RichText::new(&p.etiqueta).size(12.0).color(pal.texto_2)).truncate()).on_hover_text(texto_ayuda(p));
                    let mut t = valor_txt(b.params.get(&p.nombre).or(Some(&p.defecto)));
                    match p.tipo.as_str() {
                        "columna" | "variable" => {
                            let opciones = if p.tipo == "variable" { c.variables.iter().filter(|v| v.tipo == "tabla" || v.tipo.starts_with("matriz")).map(|v| v.nombre.clone()).collect() } else { c.columnas_de(&b.params) };
                            if opciones.is_empty() {
                                if ui.add(egui::TextEdit::singleline(&mut t).font(tema::fuente_mono(12.0)).desired_width(130.0)).changed() {
                                    b.params.insert(p.nombre.clone(), json!(t));
                                    cambio = true;
                                }
                            } else {
                                let mostrado = if t.is_empty() { "(por defecto)".to_string() } else { t.clone() };
                                egui::ComboBox::from_id_salt(&p.nombre).selected_text(egui::RichText::new(mostrado).font(tema::fuente_mono(12.0))).width(130.0).show_ui(ui, |ui| {
                                    if p.tipo == "columna" && ui.selectable_label(t.is_empty(), "(por defecto)").clicked() {
                                        b.params.insert(p.nombre.clone(), json!(""));
                                        cambio = true;
                                    }
                                    for o in &opciones {
                                        if ui.selectable_label(&t == o, o).clicked() {
                                            b.params.insert(p.nombre.clone(), json!(o));
                                            cambio = true;
                                        }
                                    }
                                });
                            }
                            ui.label("");
                        }
                        "opcion" => {
                            egui::ComboBox::from_id_salt(&p.nombre).selected_text(egui::RichText::new(&t).font(tema::fuente_mono(12.0))).width(130.0).show_ui(ui, |ui| {
                                for o in &p.opciones {
                                    if ui.selectable_label(&t == o, o).clicked() {
                                        b.params.insert(p.nombre.clone(), json!(o));
                                        cambio = true;
                                    }
                                }
                            });
                            ui.label("");
                        }
                        "codigo" => {
                            if ui.add(egui::TextEdit::multiline(&mut t).code_editor().desired_rows(4).desired_width(130.0)).changed() {
                                b.params.insert(p.nombre.clone(), json!(t));
                                cambio = true;
                            }
                            ui.label("");
                        }
                        "ruta" => {
                            ui.horizontal(|ui| {
                                let mut nueva = None;
                                if ui.add(egui::TextEdit::singleline(&mut t).font(tema::fuente_mono(12.0)).desired_width(100.0)).changed() {
                                    nueva = Some(t.clone());
                                }
                                if ui.small_button("…").clicked() {
                                    if let Some(a) = rfd::FileDialog::new().add_filter("Datos", &["csv", "tsv", "txt", "parquet"]).pick_file() {
                                        nueva = Some(a.display().to_string());
                                    }
                                }
                                if let Some(n) = nueva {
                                    // Las columnas elegidas pertenecen al archivo anterior
                                    for col in ["col_t", "col_y"] {
                                        if b.params.contains_key(col) {
                                            b.params.insert(col.into(), json!(""));
                                        }
                                    }
                                    b.params.insert(p.nombre.clone(), json!(n));
                                    cambio = true;
                                }
                            });
                            ui.label("");
                        }
                        _ => {
                            if ui.add(egui::TextEdit::singleline(&mut t).font(tema::fuente_mono(12.0)).desired_width(130.0)).changed() {
                                let v = if p.tipo == "num" { texto_a_valor(&t) } else { json!(t) };
                                b.params.insert(p.nombre.clone(), v);
                                cambio = true;
                            }
                            ui.label(egui::RichText::new(&p.unidad).size(12.0).color(pal.texto_3));
                        }
                    }
                    ui.end_row();
                }
            });
            if !ficha.params.is_empty() {
                ui.label(egui::RichText::new("Admite expresiones y variables del espacio de trabajo, como 2*pi*50 o Vdc/2.").small().color(pal.texto_3));
            }
            let (puertos, _) = puertos_de(b, &fichas);
            if !puertos.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Puertos").size(12.0).color(pal.texto_2));
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                    for q in &puertos {
                        let (marca, color) = match q.clase {
                            Clase::E | Clase::E3 => ("■", pal.electrica),
                            _ => ("▸", pal.senal),
                        };
                        egui::Frame::new().stroke(Stroke::new(1.0_f32, pal.linea_fuerte)).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                            ui.label(egui::RichText::new(format!("{marca} {}", q.nombre)).font(tema::fuente_mono(11.0)).color(color));
                        });
                    }
                });
            }
            ui.add_space(6.0);
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
            if !ficha.doc.is_empty() {
                ui.add_space(4.0);
                egui::CollapsingHeader::new(egui::RichText::new("Ecuaciones").size(12.0).color(pal.texto_2)).id_salt("ecuaciones").show(ui, |ui| {
                    ui.label(egui::RichText::new(&ficha.doc).family(egui::FontFamily::Monospace).size(12.0));
                });
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

    /// Dock de resultados: una pestaña por osciloscopio y la de diagnóstico; leyenda y
    /// exportación a la derecha de la gráfica.
    fn osciloscopios(&mut self, ui: &mut egui::Ui, c: &mut Compartido) {
        let pal = tema::p();
        let n_diag = self.avisos.len() + usize::from(self.error.is_some()) + self.lazos.len();
        let i_diag = self.resultado.len();
        if self.error.is_some() && self.resultado.is_empty() {
            self.pestana = i_diag;
        }
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 0)).show(ui, |ui| {
            ui.set_height(36.0);
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.spacing_mut().button_padding.x = 12.0;
                let pestana = |ui: &mut egui::Ui, texto: egui::WidgetText, activa: bool| -> bool {
                    let r = ui.add(egui::Button::new(texto).frame(false).min_size(Vec2::new(0.0, 34.0)));
                    if activa {
                        ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.rect.left(), r.rect.bottom() - 2.0), r.rect.right_bottom()), 0.0, pal.acento);
                    }
                    r.clicked()
                };
                for (i, (n, _)) in self.resultado.iter().enumerate() {
                    let activa = self.pestana == i;
                    if pestana(ui, egui::RichText::new(n).color(if activa { pal.texto } else { pal.texto_2 }).into(), activa) {
                        self.pestana = i;
                    }
                }
                let activa = self.pestana == i_diag;
                let color_n = if self.error.is_some() || !self.lazos.is_empty() {
                    pal.falla
                } else if !self.avisos.is_empty() {
                    pal.alerta
                } else {
                    pal.ok
                };
                let mut job = egui::text::LayoutJob::simple_format("Diagnóstico ".into(), egui::TextFormat::simple(FontId::proportional(13.0), if activa { pal.texto } else { pal.texto_2 }));
                job.append(&n_diag.to_string(), 0.0, egui::TextFormat::simple(tema::fuente_mono(11.0), color_n));
                if pestana(ui, job.into(), activa) {
                    self.pestana = i_diag;
                }
                if !self.stats.is_empty() {
                    ui.add_space(12.0);
                    ui.add(egui::Label::new(egui::RichText::new(self.stats.replace("evaluaciones", "eval")).font(tema::fuente_mono(11.0)).color(pal.texto_3)).truncate()).on_hover_text(&self.stats);
                }
            });
        });
        tema::separador(ui);
        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 12 }).show(ui, |ui| {
            if self.pestana >= i_diag {
                self.panel_diagnostico(ui);
                return;
            }
            let alto = ui.available_height().max(90.0);
            let i = self.pestana;
            let Some((_, Figura::D2(f))) = self.resultado.get_mut(i) else { return };
            let mut accion = None;
            let mut rect_graf = Rect::NOTHING;
            let mut rect_leyenda = Rect::NOTHING;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let ancho = (ui.available_width() - 216.0).max(120.0);
                rect_graf = ui
                    .allocate_ui(Vec2::new(ancho, alto), |ui| {
                        accion = f.mostrar_con(ui, alto, false, false);
                    })
                    .response
                    .rect;
                let leyenda = ui.vertical(|ui| {
                    ui.set_width(200.0);
                    ui.spacing_mut().item_spacing.y = 4.0;
                    egui::ScrollArea::vertical().id_salt("leyenda_osc").max_height((alto - 40.0).max(30.0)).auto_shrink([false, true]).show(ui, |ui| {
                        for (k, s) in f.series.iter_mut().enumerate() {
                            let color = s.color.unwrap_or(pal.series[k % pal.series.len()]);
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut s.visible, "");
                                let (r, _) = ui.allocate_exact_size(Vec2::new(12.0, 14.0), Sense::hover());
                                ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(2.0_f32, color));
                                let n = if s.nombre.is_empty() { format!("serie {}", k + 1) } else { s.nombre.replace(": ", " · ") };
                                ui.add(egui::Label::new(egui::RichText::new(n).size(12.0).color(pal.texto)).truncate());
                            });
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("PNG").on_hover_text("Guardar como PNG").clicked() {
                            accion = Some(grafica::Accion::Png(Rect::NOTHING));
                        }
                        if ui.button("SVG").on_hover_text("Guardar como SVG").clicked() {
                            accion = Some(grafica::Accion::Svg);
                        }
                        if ui.button("CSV").on_hover_text("Guardar los datos como CSV").clicked() {
                            accion = Some(grafica::Accion::Csv);
                        }
                    });
                });
                rect_leyenda = leyenda.response.rect;
            });
            if let Some(a) = accion {
                let a = match a {
                    grafica::Accion::Png(r) if r == Rect::NOTHING => grafica::Accion::Png(rect_graf.union(rect_leyenda)),
                    a => a,
                };
                let fig = Figura::D2(f.clone());
                c.exportar(ui.ctx(), a, &fig);
            }
        });
    }

    /// Fallas, alertas y lazos de la última simulación, cada uno con su punto y su texto.
    fn panel_diagnostico(&self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut hay = false;
            if let Some(e) = &self.error {
                tema::estado(ui, tema::Nivel::Falla, e);
                hay = true;
            }
            for l in &self.lazos {
                tema::estado(ui, tema::Nivel::Falla, &format!("Lazo algebraico: {}", l.join(" ↔ ")));
                hay = true;
            }
            for a in &self.avisos {
                tema::estado(ui, tema::Nivel::Alerta, a);
                hay = true;
            }
            if !hay && !self.stats.is_empty() {
                tema::estado(ui, tema::Nivel::Ok, "Simulación completada sin avisos");
                ui.label(egui::RichText::new(&self.stats).font(tema::fuente_mono(11.0)).color(tema::p().texto_3));
            } else if !hay {
                ui.label(egui::RichText::new("Simula el modelo para ver su diagnóstico.").small().color(tema::p().texto_2));
            }
        });
    }

    fn lienzo(&mut self, ui: &mut egui::Ui) {
        let (area, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.area_lienzo = area;
        if self.centrar {
            self.ajustar_vista(area);
            self.centrar = false;
        }
        let p = ui.painter_at(area);
        p.rect_filled(area, 0.0, tema::p().fondo);
        let origen = area.min.to_vec2() + self.pan;
        let z = self.zoom;
        // Rejilla de puntos cada 16 px
        let paso = 16.0 * z;
        if paso > 6.0 {
            let c = tema::p().rejilla;
            let inicio_x = area.left() + (origen.x - area.left()).rem_euclid(paso);
            let inicio_y = area.top() + (origen.y - area.top()).rem_euclid(paso);
            let mut x = inicio_x;
            while x < area.right() {
                let mut y = inicio_y;
                while y < area.bottom() {
                    p.rect_filled(Rect::from_center_size(Pos2::new(x, y), Vec2::splat(1.5)), 0.0, c);
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
        // Hilos: eléctricos 2 px en ámbar, señales 1,5 px en azul; los de un lazo, discontinuos en FALLA
        let mut hilo_cercano = None;
        for (ci, con) in s.conexiones.iter().enumerate() {
            let (Some(a), Some(b)) = (pos_puerto(&con.de.0, &con.de.1), pos_puerto(&con.a.0, &con.a.1)) else { continue };
            let pts = if self.ordenar_cables { ortogonal(a.0, a.2, b.0, b.2, z) } else { curva(a.0, a.2, b.0, b.2, z) };
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
        let radio = tema::radio_ventana() as f32;
        let fuente = tema::fuente_semi((11.0 * z).max(6.5));
        let fuente_p = FontId::proportional((10.0 * z).max(6.0));
        let fuente_param = tema::fuente_mono((10.0 * z).max(6.0));
        for (i, b) in s.bloques.iter().enumerate() {
            let (rect, pu) = &geom[i];
            if !rect.intersects(area) {
                continue;
            }
            let fi = fichas.get(&b.tipo);
            let franja = if pu.iter().any(|(q, _)| matches!(q.clase, Clase::E | Clase::E3)) {
                Some(pal.electrica)
            } else {
                fi.and_then(color_ficha)
            };
            let sel = self.sel.contains(&b.id);
            let falla = en_lazo.contains(&b.id);
            if sel {
                // Selección: halo de 3 px en ACENTO translúcido y borde en ACENTO
                p.rect_filled(rect.expand(3.0), radio + 3.0, pal.acento.gamma_multiply(0.18));
            }
            p.rect_filled(*rect, radio, pal.nodo);
            if let Some(c) = franja {
                p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(rect.width(), 2.0 * z.max(0.75))), egui::CornerRadius { nw: radio as u8, ne: radio as u8, sw: 0, se: 0 }, c);
            }
            let borde = if falla {
                pal.falla
            } else if sel {
                pal.acento
            } else {
                pal.linea_fuerte
            };
            p.rect_stroke(*rect, radio, Stroke::new(1.0_f32, borde), egui::StrokeKind::Inside);
            let nombre = if b.nombre.is_empty() { fi.map(|f| f.nombre.as_str()).unwrap_or(&b.tipo) } else { &b.nombre };
            let pin = p.with_clip_rect(rect.shrink(2.0));
            pin.text(rect.left_top() + Vec2::new(8.0 * z, 11.0 * z), Align2::LEFT_CENTER, nombre, fuente.clone(), pal.texto);
            if b.tipo == "subsistema" {
                pin.text(rect.right_top() + Vec2::new(-8.0 * z, 10.0 * z), Align2::RIGHT_CENTER, "⊞", fuente.clone(), pal.texto_2);
            }
            if z > 0.45 {
                let par = parametro_principal(b, fi);
                if !par.is_empty() {
                    pin.text(Pos2::new(rect.left() + 8.0 * z, rect.bottom() - 9.0 * z), Align2::LEFT_CENTER, par, fuente_param.clone(), pal.texto_3);
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
                        p.line_segment([*pp + Vec2::new(-t * 0.8, 0.0), *pp + Vec2::new(t * 0.8, 0.0)], Stroke::new(1.0_f32, pal.nodo));
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
                p.circle_stroke(pp, 7.0, Stroke::new(1.5_f32, color_cable(cl)));
            }
        }
        // Etiqueta de lazo algebraico, arriba a la izquierda del lienzo
        let mut y_et = area.top() + 10.0;
        for n in &nombres_lazo {
            let texto = format!("LAZO ALGEBRAICO · {n}");
            let gal = ui.painter().layout_no_wrap(texto, tema::fuente_semi(11.0), pal.falla);
            let caja = Rect::from_min_size(Pos2::new(area.left() + 10.0, y_et), gal.size() + Vec2::new(26.0, 10.0));
            p.rect(caja, radio, pal.falla_fondo, Stroke::new(1.0_f32, pal.falla), egui::StrokeKind::Inside);
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
                    let (a_izq, b_izq) = (*clase == Clase::Entrada, *clase != Clase::Entrada);
                    let pts = if self.ordenar_cables { ortogonal(*inicio, a_izq, pp, b_izq, z) } else { curva(*inicio, a_izq, pp, b_izq, z) };
                    p.add(Shape::line(pts, tema::trazo_cable(matches!(clase, Clase::E | Clase::E3), false)));
                }
            }
            Arrastre::Seleccion(a) if resp.dragged() => {
                if let Some(pp) = resp.interact_pointer_pos() {
                    let r2 = Rect::from_two_pos(*a, pp);
                    p.rect(r2, 2.0, tema::p().acento.gamma_multiply(0.08), Stroke::new(1.0_f32, tema::p().acento), egui::StrokeKind::Middle);
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
                capa.rect(r, tema::radio_ventana() as f32, tema::p().nodo, Stroke::new(1.5_f32, tema::p().acento), egui::StrokeKind::Middle);
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
        } else {
            self.minimapa(ui, area);
        }
    }

    /// Minimapa de 140 × 84 px abajo a la derecha: los bloques del nivel y la vista actual.
    /// Un clic o un arrastre centra la vista en ese punto.
    fn minimapa(&mut self, ui: &mut egui::Ui, area: Rect) {
        let pal = tema::p();
        let caja = Rect::from_min_size(area.right_bottom() - Vec2::new(152.0, 96.0), Vec2::new(140.0, 84.0));
        if !area.contains_rect(caja.expand(4.0)) {
            return;
        }
        let fichas = &self.fichas;
        let s = nivel_ref(&self.diagrama.sistema, &self.ruta);
        let bloques: Vec<(Rect, Option<Color32>)> = s
            .bloques
            .iter()
            .map(|b| {
                let (pu, alto) = puertos_de(b, fichas);
                let franja = if pu.iter().any(|q| matches!(q.clase, Clase::E | Clase::E3)) { Some(pal.electrica) } else { fichas.get(&b.tipo).and_then(color_ficha) };
                (Rect::from_min_size(Pos2::new(b.x, b.y), Vec2::new(ANCHO, alto)), franja)
            })
            .collect();
        // La vista, en coordenadas del diagrama
        let vista = Rect::from_min_max((-self.pan / self.zoom).to_pos2(), ((area.size() - self.pan) / self.zoom).to_pos2());
        let mundo = bloques.iter().fold(vista, |m, (r, _)| m.union(*r));
        let dentro = caja.shrink(8.0);
        let k = (dentro.width() / mundo.width()).min(dentro.height() / mundo.height());
        let base = dentro.center() - mundo.center().to_vec2() * k;
        let a_mini = |q: Pos2| base + q.to_vec2() * k;
        let resp = ui.interact(caja, ui.id().with("minimapa"), Sense::click_and_drag());
        let p = ui.painter_at(caja.expand(1.0));
        p.rect(caja, 6.0, pal.panel.gamma_multiply(0.92), Stroke::new(1.0_f32, pal.borde), egui::StrokeKind::Inside);
        for (r, franja) in &bloques {
            let m = Rect::from_min_max(a_mini(r.min), a_mini(r.max));
            p.rect_filled(m, 1.0, franja.unwrap_or(pal.linea_fuerte).gamma_multiply(0.6));
        }
        p.rect_stroke(Rect::from_min_max(a_mini(vista.min), a_mini(vista.max)), 3.0, Stroke::new(1.0_f32, pal.acento.gamma_multiply(0.8)), egui::StrokeKind::Middle);
        if resp.is_pointer_button_down_on() {
            if let Some(pp) = resp.interact_pointer_pos() {
                let w = ((pp - base) / k).to_pos2();
                self.pan = area.size() / 2.0 - w.to_vec2() * self.zoom;
            }
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

    #[test]
    fn un_cable_ortogonal_hacia_atras_no_recorre_la_fila_de_sus_extremos() {
        // Salida a la derecha de un bloque e entrada a la izquierda de otro situado detrás, a la misma altura
        let (a, b) = (Pos2::new(300.0, 100.0), Pos2::new(100.0, 100.0));
        let pts = ortogonal(a, false, b, true, 1.0);
        assert_eq!((pts[0], *pts.last().unwrap()), (a, b));
        for w in pts.windows(2) {
            let horizontal = w[0].y == w[1].y;
            let largo = (w[0].x - w[1].x).abs();
            assert!(!(horizontal && w[0].y == 100.0 && largo > 20.0), "tramo largo sobre la fila: {w:?}");
        }
        // Cada tramo es horizontal o vertical
        assert!(pts.windows(2).all(|w| w[0].x == w[1].x || w[0].y == w[1].y));
    }

    #[test]
    fn abrir_un_diagrama_con_funcion_python_avisa_y_uno_sin_codigo_no() {
        let dir = std::env::temp_dir().join(format!("heliora_codigo_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let con = dir.join("con.json");
        let anidado = r#"{"id": "s", "tipo": "subsistema", "sistema": {"bloques": [{"id": "f", "tipo": "funcion_python"}], "conexiones": []}}"#;
        std::fs::write(&con, format!(r#"{{"sistema": {{"bloques": [{{"id": "a", "tipo": "funcion_python"}}, {anidado}], "conexiones": []}}}}"#)).unwrap();
        let sin = dir.join("sin.json");
        std::fs::write(&sin, r#"{"sistema": {"bloques": [{"id": "g", "tipo": "ganancia"}], "conexiones": []}}"#).unwrap();
        let mut e = Editor::nuevo();
        e.abrir_de(&con).unwrap();
        assert!(e.aviso_apertura.as_deref().is_some_and(|a| a.contains("2 bloque(s)") && a.contains("código Python")), "{:?}", e.aviso_apertura);
        e.abrir_de(&sin).unwrap();
        assert!(e.aviso_apertura.is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn la_ayuda_de_un_parametro_llega_al_hover() {
        let fichas = fichas_desde(&json!([{"tipo": "t", "params": [{"nombre": "k", "etiqueta": "Ganancia", "ayuda": "Factor de escala"}, {"nombre": "j", "etiqueta": "Otro"}]}]));
        let p = &fichas[0].params;
        assert_eq!(p[0].ayuda, "Factor de escala");
        assert_eq!(texto_ayuda(&p[0]), "Ganancia\nFactor de escala");
        assert_eq!(texto_ayuda(&p[1]), "Otro");
    }

    #[test]
    fn abrir_un_json_que_no_es_diagrama_no_cambia_ni_el_archivo_ni_el_estado_guardado() {
        let dir = std::env::temp_dir().join(format!("heliora_abrir_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut e = Editor::nuevo();
        e.diagrama.nombre = "original".into();
        let bueno = dir.join("bueno.json");
        e.guardar_en(&bueno).unwrap();
        assert!(!e.sucio());
        let malo = dir.join("proyecto.json");
        std::fs::write(&malo, br#"{"formato": "heliora-proyecto/1", "sistema": 3}"#).unwrap();
        assert!(e.abrir_de(&malo).is_err());
        assert_eq!(e.archivo.as_deref(), Some(bueno.as_path()));
        assert_eq!(e.diagrama.nombre, "original");
        assert!(!e.sucio());
        e.diagrama.nombre = "editado".into();
        assert!(e.sucio());
        let _ = std::fs::remove_dir_all(&dir);
    }

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
        e.cargar_valor(&v).unwrap();
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
        e.cargar_valor(&v).unwrap();
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
