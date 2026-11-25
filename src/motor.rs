//! Comunicación con el motor de cálculo en Python: un proceso aparte que habla JSON por líneas.
//!
//! Supervisión:
//! - Detener: el motor intenta parar de forma cooperativa y, a los 0,3 s, interrumpe el código
//!   de Python. Si en `ESPERA_CANCELAR` no ha respondido (una llamada bloqueada en C), el proceso
//!   se cierra y se abre otro: se pierden las variables y se avisa.
//! - Si el motor se cierra solo, se vuelve a abrir con espera creciente (como mucho
//!   `MAX_REINICIOS` veces por minuto). Las peticiones que no cambian nada se reenvían; las de la
//!   consola reciben un error, porque repetirlas en un espacio de trabajo vacío no tiene sentido.
//!   Una simulación reenviada lleva `_reenviado`, y el motor la rechaza si usa variables del espacio.
//! - El hilo de la ventana no escribe en la tubería: encola cada petición (cola acotada) y un hilo
//!   aparte la escribe. Si la cola está llena o la escritura falla, la petición recibe un error.
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const ESPERA_CANCELAR: Duration = Duration::from_secs(3);
pub const MAX_REINICIOS: usize = 3;
pub const VENTANA_REINICIOS: Duration = Duration::from_secs(60);
/// Peticiones que pueden esperar a ser escritas en la tubería del motor.
pub const CAPACIDAD_ENVIO: usize = 256;

/// Peticiones que se pueden repetir sin efectos: no dependen de variables de la consola.
const REPETIBLES: &[&str] = &[
    "catalogo", "herramientas", "ejemplos", "ejemplo", "puertos", "variables", "leer_archivo", "pruebas", "simular", "validar",
    "plan_inicial", "plan_generar", "plan_tablero", "plan_cerrar", "plan_cargar", "amef_rpn",
];
/// Peticiones que el motor contesta aunque esté ocupado: no cuentan para cancelar.
const SOLO_LECTURA: &[&str] = &["variables", "ver_variable"];

pub enum Mensaje {
    Listo,
    Respuesta { id: u64, ok: bool, valor: Value, error: String },
    Avance { id: u64, fraccion: f32, texto: String },
    Terminado(String),
    /// El motor se volvió a abrir: las variables se perdieron.
    Reiniciado(String),
}

/// Lo que llega de los hilos del proceso: un mensaje del motor o un envío que no se pudo escribir.
enum Evento {
    Mensaje(Mensaje),
    EnvioFallido(u64, String),
}

/// Encola una línea para el hilo escritor sin bloquear: falla si la cola está llena o cerrada.
fn encolar(envio: &SyncSender<(u64, String)>, id: u64, linea: String) -> Result<(), String> {
    envio.try_send((id, linea)).map_err(|e| match e {
        TrySendError::Full(_) => "La cola de envío al motor de cálculo está llena: el motor no está leyendo peticiones.".to_string(),
        TrySendError::Disconnected(_) => "El motor de cálculo no está en marcha.".to_string(),
    })
}

pub struct Motor {
    hijo: Option<Child>,
    envio: Option<SyncSender<(u64, String)>>,
    rx: Receiver<(u64, Evento)>,
    tx: Sender<(u64, Evento)>,
    siguiente: u64,
    pub registro: Arc<Mutex<String>>,
    pub listo: bool,
    pub caido: Option<String>,
    pub carpeta: PathBuf,
    /// Cada proceso tiene su generación; los mensajes de uno ya cerrado se ignoran.
    generacion: u64,
    en_vuelo: BTreeMap<u64, (String, Value)>,
    cancelacion: Option<(u64, Instant)>,
    reinicios: VecDeque<Instant>,
    reabrir_en: Option<Instant>,
    reenviar: Vec<(u64, String, Value)>,
    salientes: Vec<Mensaje>,
}

/// Busca la carpeta `engine` subiendo desde el ejecutable y desde el directorio actual.
pub fn carpeta_motor() -> PathBuf {
    if let Ok(v) = std::env::var("HELIORA_ENGINE") {
        return PathBuf::from(v);
    }
    let mut candidatos: Vec<PathBuf> = Vec::new();
    if let Ok(e) = std::env::current_exe() {
        let mut p = e.as_path();
        while let Some(q) = p.parent() {
            candidatos.push(q.join("engine"));
            candidatos.push(q.join("Resources").join("engine"));
            p = q;
        }
    }
    if let Ok(c) = std::env::current_dir() {
        let mut p = c.as_path();
        candidatos.push(p.join("engine"));
        while let Some(q) = p.parent() {
            candidatos.push(q.join("engine"));
            p = q;
        }
    }
    candidatos
        .into_iter()
        .find(|c| c.join("heliora").join("servidor.py").exists())
        .unwrap_or_else(|| PathBuf::from("engine"))
}

/// El intérprete: variable de entorno, Python incluido en el instalador, entorno privado junto al
/// motor o el del sistema.
pub fn interprete(motor: &Path) -> PathBuf {
    if let Ok(v) = std::env::var("HELIORA_PYTHON") {
        return PathBuf::from(v);
    }
    let base = motor.parent().unwrap_or(motor);
    for rel in ["python/python.exe", "python/bin/python3", ".venv/bin/python", ".venv/bin/python3", ".venv/Scripts/python.exe"] {
        let c = base.join(rel);
        if c.exists() {
            return c;
        }
    }
    // En Windows, «python3» suele ser el acceso directo a la Microsoft Store, no un intérprete
    PathBuf::from(if cfg!(windows) { "python" } else { "python3" })
}

/// Orden para un proceso auxiliar. En Windows se lanza sin ventana de consola: la aplicación es
/// gráfica y cada proceso de consola abriría una propia.
pub fn orden(programa: impl AsRef<std::ffi::OsStr>) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(programa);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

impl Motor {
    pub fn iniciar() -> Motor {
        let (tx, rx) = channel();
        let carpeta = carpeta_motor();
        let mut m = Motor {
            hijo: None,
            envio: None,
            rx,
            tx,
            siguiente: 1,
            registro: Arc::new(Mutex::new(String::new())),
            listo: false,
            caido: None,
            carpeta,
            generacion: 0,
            en_vuelo: BTreeMap::new(),
            cancelacion: None,
            reinicios: VecDeque::new(),
            reabrir_en: None,
            reenviar: vec![],
            salientes: vec![],
        };
        m.arrancar();
        m
    }

    pub fn arrancar(&mut self) {
        self.listo = false;
        self.caido = None;
        self.generacion += 1;
        let gen = self.generacion;
        let py = interprete(&self.carpeta);
        let hijo = orden(&py)
            .args(["-u", "-m", "heliora.servidor"])
            .current_dir(&self.carpeta)
            .env("PYTHONPATH", &self.carpeta)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            // El protocolo y los archivos van en UTF-8 aunque el sistema use otra página de códigos (cp1252 en Windows)
            .env("PYTHONUTF8", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut hijo = match hijo {
            Ok(h) => h,
            Err(e) => {
                self.caido = Some(format!("No se pudo iniciar Python ({}): {e}", py.display()));
                return;
            }
        };
        let mut entrada = hijo.stdin.take().expect("entrada del motor");
        let salida = hijo.stdout.take().expect("salida del motor");
        let err = hijo.stderr.take().expect("errores del motor");
        self.hijo = Some(hijo);
        // Escritor: la escritura y el flush, que pueden bloquear si Python deja de leer, ocurren aquí
        let (envio, cola) = sync_channel::<(u64, String)>(CAPACIDAD_ENVIO);
        self.envio = Some(envio);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            while let Ok((id, linea)) = cola.recv() {
                if let Err(e) = writeln!(entrada, "{linea}").and_then(|_| entrada.flush()) {
                    let _ = tx.send((gen, Evento::EnvioFallido(id, format!("No se pudo enviar la petición al motor de cálculo: {e}"))));
                }
            }
        });
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let enviar = |m: Mensaje| tx.send((gen, Evento::Mensaje(m)));
            for linea in BufReader::new(salida).lines() {
                let Ok(linea) = linea else { break };
                let Ok(v) = serde_json::from_str::<Value>(&linea) else { continue };
                if let Some(ev) = v.get("evento").and_then(|e| e.as_str()) {
                    if ev == "listo" {
                        let _ = enviar(Mensaje::Listo);
                    }
                    continue;
                }
                let id = v.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
                if let Some(a) = v.get("avance").and_then(|a| a.as_f64()) {
                    let texto = v.get("mensaje").and_then(|m| m.as_str()).unwrap_or("").to_string();
                    let _ = enviar(Mensaje::Avance { id, fraccion: a as f32, texto });
                    continue;
                }
                let ok = v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false);
                let error = v.get("error").and_then(|e| e.as_str()).unwrap_or("").to_string();
                let valor = v.get("resultado").cloned().unwrap_or(Value::Null);
                let _ = enviar(Mensaje::Respuesta { id, ok, valor, error });
            }
            let _ = enviar(Mensaje::Terminado("El motor de cálculo se cerró.".into()));
        });
        let reg = self.registro.clone();
        std::thread::spawn(move || {
            for linea in BufReader::new(err).lines() {
                let Ok(linea) = linea else { break };
                if let Ok(mut r) = reg.lock() {
                    r.push_str(&linea);
                    r.push('\n');
                    let largo = r.len();
                    if largo > 200_000 {
                        let mut corte = largo - 100_000;
                        while !r.is_char_boundary(corte) {
                            corte += 1;
                        }
                        *r = r[corte..].to_string();
                    }
                }
            }
        });
    }

    /// Encola la petición para el hilo escritor. Si no hay proceso, el reinicio la reenviará o la
    /// fallará; si la cola está llena, la petición falla en el acto con un error explícito.
    fn escribir(&mut self, id: u64, metodo: &str, params: &Value) {
        let pet = json!({"id": id, "metodo": metodo, "params": params}).to_string();
        let Some(envio) = self.envio.as_ref() else { return };
        if let Err(error) = encolar(envio, id, pet) {
            self.en_vuelo.remove(&id);
            self.salientes.push(Mensaje::Respuesta { id, ok: false, valor: Value::Null, error });
        }
    }

    /// Envía una petición; devuelve su número, para reconocer la respuesta.
    pub fn llamar(&mut self, metodo: &str, params: Value) -> u64 {
        let id = self.siguiente;
        self.siguiente += 1;
        if metodo == "cancelar" {
            // Se vigila la petición más antigua que no es de solo lectura: la que está corriendo
            if let Some((&objetivo, _)) = self.en_vuelo.iter().find(|(_, (m, _))| !SOLO_LECTURA.contains(&m.as_str())) {
                if self.cancelacion.is_none() {
                    self.cancelacion = Some((objetivo, Instant::now()));
                }
            }
        } else {
            self.en_vuelo.insert(id, (metodo.to_string(), params.clone()));
        }
        if self.listo || metodo == "cancelar" {
            self.escribir(id, metodo, &params);
        } else if self.reabrir_en.is_some() || self.hijo.is_none() {
            self.reenviar.push((id, metodo.to_string(), params));
        } else {
            // El motor está arrancando: Python lee la entrada en cuanto está listo
            self.escribir(id, metodo, &params);
        }
        id
    }

    /// Peticiones sin respuesta todavía (sin contar las de solo lectura).
    pub fn ocupado(&self) -> bool {
        self.en_vuelo.values().any(|(m, _)| !SOLO_LECTURA.contains(&m.as_str()))
    }

    fn fallar_en_vuelo(&mut self, texto: &str, reintentar: bool) {
        let todas = std::mem::take(&mut self.en_vuelo);
        for (id, (metodo, mut params)) in todas {
            if reintentar && REPETIBLES.contains(&metodo.as_str()) {
                if metodo == "simular" {
                    // El espacio de trabajo se perdió: el motor rechaza la simulación si dependía de él
                    if let Some(o) = params.as_object_mut() {
                        o.insert("_reenviado".into(), Value::Bool(true));
                    }
                }
                self.reenviar.push((id, metodo.clone(), params.clone()));
                self.en_vuelo.insert(id, (metodo, params));
            } else {
                self.salientes.push(Mensaje::Respuesta { id, ok: false, valor: Value::Null, error: texto.to_string() });
            }
        }
    }

    fn matar(&mut self) {
        self.envio = None;
        if let Some(mut h) = self.hijo.take() {
            let _ = h.kill();
            let _ = h.wait();
        }
        self.listo = false;
    }

    pub fn recibir(&mut self) -> Vec<Mensaje> {
        let mut v = std::mem::take(&mut self.salientes);
        let ahora = Instant::now();
        // Cancelación sin respuesta: el motor está bloqueado y se reemplaza
        if let Some((objetivo, desde)) = self.cancelacion {
            if !self.en_vuelo.contains_key(&objetivo) {
                self.cancelacion = None;
            } else if ahora.duration_since(desde) >= ESPERA_CANCELAR {
                self.cancelacion = None;
                self.matar();
                self.fallar_en_vuelo("Detenido por el usuario. El motor no respondía y se ha reiniciado: las variables se han perdido.", false);
                v.append(&mut self.salientes);
                v.push(Mensaje::Reiniciado("El motor no respondía a Detener y se ha reiniciado. Las variables se han perdido.".into()));
                self.arrancar();
            }
        }
        if let Some(t) = self.reabrir_en {
            if ahora >= t {
                self.reabrir_en = None;
                self.arrancar();
            }
        }
        while let Ok((gen, ev)) = self.rx.try_recv() {
            if gen != self.generacion {
                continue;
            }
            let m = match ev {
                Evento::Mensaje(m) => m,
                Evento::EnvioFallido(id, error) => {
                    // Una petición repetible la reenvía el reinicio que sigue al cierre de la tubería
                    match self.en_vuelo.get(&id) {
                        Some((metodo, _)) if REPETIBLES.contains(&metodo.as_str()) => continue,
                        Some(_) => {
                            self.en_vuelo.remove(&id);
                            Mensaje::Respuesta { id, ok: false, valor: Value::Null, error }
                        }
                        None => continue,
                    }
                }
            };
            match &m {
                Mensaje::Listo => {
                    self.listo = true;
                    for (id, metodo, params) in std::mem::take(&mut self.reenviar) {
                        self.escribir(id, &metodo, &params);
                    }
                }
                Mensaje::Respuesta { id, .. } => {
                    self.en_vuelo.remove(id);
                }
                Mensaje::Terminado(t) => {
                    self.listo = false;
                    self.hijo = None;
                    self.envio = None;
                    let reg = self.registro.lock().map(|r| r.clone()).unwrap_or_default();
                    let cola: String = reg.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                    let texto = if cola.is_empty() { t.clone() } else { format!("{t}\n{cola}") };
                    while self.reinicios.front().is_some_and(|r| ahora.duration_since(*r) > VENTANA_REINICIOS) {
                        self.reinicios.pop_front();
                    }
                    if self.reinicios.len() >= MAX_REINICIOS {
                        // Se cae una y otra vez: se deja parado y se explica por qué
                        self.fallar_en_vuelo("El motor de cálculo se ha cerrado varias veces seguidas.", false);
                        v.append(&mut self.salientes);
                        self.caido = Some(texto);
                        v.push(m);
                        continue;
                    }
                    self.reinicios.push_back(ahora);
                    let espera = Duration::from_millis(250 * (1 << self.reinicios.len().min(4)) as u64);
                    self.reabrir_en = Some(ahora + espera);
                    self.fallar_en_vuelo("El motor se cerró mientras trabajaba y se ha reiniciado: las variables se han perdido. Vuelve a ejecutarlo.", true);
                    v.append(&mut self.salientes);
                    v.push(Mensaje::Reiniciado("El motor de cálculo se cerró y se ha vuelto a abrir. Las variables se han perdido.".into()));
                    continue;
                }
                _ => {}
            }
            v.push(m);
        }
        v
    }

    /// Reinicio pedido por la persona: las peticiones pendientes terminan con un error.
    pub fn reiniciar(&mut self) {
        self.detener();
        self.fallar_en_vuelo("El motor se ha reiniciado.", false);
        self.reenviar.clear();
        self.reinicios.clear();
        self.reabrir_en = None;
        self.cancelacion = None;
        self.arrancar();
    }

    pub fn detener(&mut self) {
        self.generacion += 1;
        self.matar();
    }

    #[cfg(test)]
    fn matar_para_prueba(&mut self) {
        if let Some(h) = self.hijo.as_mut() {
            let _ = h.kill();
        }
    }
}

impl Drop for Motor {
    fn drop(&mut self) {
        self.detener();
    }
}

/// Instala el motor de Python en un entorno privado, en segundo plano.
pub fn instalar_motor(motor: &Path, avisar: Sender<String>) {
    let base = motor.parent().unwrap_or(motor).to_path_buf();
    let req = motor.join("requirements.txt");
    std::thread::spawn(move || {
        let venv = base.join(".venv");
        let ok = (|| -> Result<(), String> {
            // En Windows el instalador de python.org deja el lanzador «py» y «python»; en macOS y Linux, «python3.x»
            let candidatos: &[(&str, &[&str])] = if cfg!(windows) {
                &[("py", &["-3.12"]), ("py", &["-3.11"]), ("py", &["-3"]), ("python", &[])]
            } else {
                &[("python3.12", &[]), ("python3.11", &[]), ("python3", &[])]
            };
            let mut crear = None;
            for (py, previos) in candidatos {
                let r = orden(py).args(*previos).args(["-m", "venv"]).arg(&venv).status();
                if let Ok(s) = r {
                    if s.success() {
                        crear = Some(py);
                        break;
                    }
                }
            }
            if crear.is_none() {
                return Err("No se encontró Python 3 en este ordenador. Instálalo desde python.org.".into());
            }
            let _ = avisar.send("Instalando NumPy, SciPy, pandas, scikit-learn, statsmodels y pvlib...".into());
            let pip = interprete_venv(&venv);
            let s = orden(&pip)
                .args(["-m", "pip", "install", "-q", "-r"])
                .arg(&req)
                .status()
                .map_err(|e| e.to_string())?;
            if !s.success() {
                return Err("No se pudieron instalar las librerías (¿hay conexión a Internet?).".into());
            }
            Ok(())
        })();
        let _ = avisar.send(match ok {
            Ok(_) => "listo".to_string(),
            Err(e) => format!("error: {e}"),
        });
    });
}

fn interprete_venv(venv: &Path) -> PathBuf {
    let u = venv.join("bin").join("python");
    if u.exists() {
        u
    } else {
        venv.join("Scripts").join("python.exe")
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Espera mensajes hasta que `cond` devuelva algo o pase el plazo.
    fn esperar<T>(m: &mut Motor, plazo: Duration, mut cond: impl FnMut(&Mensaje) -> Option<T>) -> (Option<T>, Vec<String>) {
        let fin = Instant::now() + plazo;
        let mut vistos = vec![];
        while Instant::now() < fin {
            for msg in m.recibir() {
                if let Mensaje::Reiniciado(t) = &msg {
                    vistos.push(t.clone());
                }
                if let Some(r) = cond(&msg) {
                    return (Some(r), vistos);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        (None, vistos)
    }

    fn listo(m: &mut Motor) {
        let (r, _) = esperar(m, Duration::from_secs(60), |x| matches!(x, Mensaje::Listo).then_some(()));
        assert!(r.is_some(), "el motor no arrancó: {:?}", m.caido);
    }

    fn respuesta(m: &mut Motor, id: u64, plazo: Duration) -> Option<(bool, Value, String)> {
        esperar(m, plazo, |x| match x {
            Mensaje::Respuesta { id: i, ok, valor, error } if *i == id => Some((*ok, valor.clone(), error.clone())),
            _ => None,
        })
        .0
    }

    #[test]
    fn detener_un_bucle_infinito_conserva_las_variables() {
        let mut m = Motor::iniciar();
        listo(&mut m);
        let a = m.llamar("evaluar", json!({"codigo": "x_guardada = 41"}));
        assert!(respuesta(&mut m, a, Duration::from_secs(20)).is_some());
        let b = m.llamar("evaluar", json!({"codigo": "while True: pass"}));
        std::thread::sleep(Duration::from_millis(300));
        // Mientras trabaja, las variables se pueden leer
        let v = m.llamar("variables", json!({}));
        let (ok, vars, _) = respuesta(&mut m, v, Duration::from_secs(5)).expect("variables mientras trabaja");
        assert!(ok && vars.to_string().contains("x_guardada"));
        m.llamar("cancelar", json!({}));
        let (ok, valor, _) = respuesta(&mut m, b, Duration::from_secs(2)).expect("el bucle no se detuvo");
        assert!(ok && valor["error"].as_str().unwrap_or("").contains("Detenido"), "{valor}");
        let c = m.llamar("evaluar", json!({"codigo": "x_guardada + 1"}));
        let (_, valor, _) = respuesta(&mut m, c, Duration::from_secs(10)).expect("el motor sigue vivo");
        assert_eq!(valor["resultado"].as_str(), Some("42"));
    }

    #[test]
    fn detener_una_llamada_bloqueada_reinicia_el_motor() {
        let mut m = Motor::iniciar();
        listo(&mut m);
        let b = m.llamar("evaluar", json!({"codigo": "import time\ntime.sleep(60)"}));
        std::thread::sleep(Duration::from_millis(300));
        let t0 = Instant::now();
        m.llamar("cancelar", json!({}));
        let (ok, _, error) = respuesta(&mut m, b, Duration::from_secs(10)).expect("no se cerró el motor bloqueado");
        assert!(!ok && error.contains("Detenido"));
        assert!(t0.elapsed() < Duration::from_secs(6));
        listo(&mut m);
        let c = m.llamar("evaluar", json!({"codigo": "1 + 1"}));
        let (_, valor, _) = respuesta(&mut m, c, Duration::from_secs(10)).expect("el motor nuevo responde");
        assert_eq!(valor["resultado"].as_str(), Some("2"));
    }

    #[test]
    fn una_cola_de_envio_llena_falla_sin_bloquear() {
        // Nadie lee la cola, como un motor que dejó de leer su entrada
        let (envio, _cola) = sync_channel::<(u64, String)>(1);
        assert!(encolar(&envio, 1, "a".into()).is_ok());
        let t0 = Instant::now();
        let r = encolar(&envio, 2, "b".into());
        assert!(t0.elapsed() < Duration::from_millis(100));
        assert!(r.unwrap_err().contains("llena"));
    }

    #[test]
    fn una_simulacion_que_usa_el_espacio_no_se_repite_tras_un_reinicio() {
        let mut m = Motor::iniciar();
        listo(&mut m);
        let a = m.llamar("evaluar", json!({"codigo": "ganancia_puente = 2.0"}));
        assert!(respuesta(&mut m, a, Duration::from_secs(20)).is_some());
        // Simulación larga que lee «ganancia_puente» del espacio de trabajo
        let d = json!({"formato": "heliora-diagrama/1", "nombre": "larga", "solver": {"metodo": "rk4", "paso": 1e-6, "t_fin": 100.0},
            "sistema": {"bloques": [
                {"id": "c", "tipo": "constante", "params": {"c": "ganancia_puente"}},
                {"id": "i", "tipo": "integrador", "params": {}},
                {"id": "o", "tipo": "osciloscopio", "params": {"n": 1, "cada": 100000}}],
              "conexiones": [{"de": ["c", "y"], "a": ["i", "u"]}, {"de": ["i", "y"], "a": ["o", "u1"]}]}});
        let s = m.llamar("simular", json!({"diagrama": d}));
        std::thread::sleep(Duration::from_millis(500));
        m.matar_para_prueba();
        let (ok, _, error) = respuesta(&mut m, s, Duration::from_secs(90)).expect("la simulación recibe una respuesta tras el reinicio");
        assert!(!ok && error.contains("reinició") && error.contains("ganancia_puente"), "{error}");
    }

    #[test]
    fn un_motor_que_se_cae_se_reabre_y_repite_lo_repetible() {
        let mut m = Motor::iniciar();
        listo(&mut m);
        let consola = m.llamar("evaluar", json!({"codigo": "import time\ntime.sleep(30)"}));
        let cat = m.llamar("catalogo", json!({}));
        std::thread::sleep(Duration::from_millis(300));
        m.matar_para_prueba();
        let (r, avisos) = esperar(&mut m, Duration::from_secs(10), |x| match x {
            Mensaje::Respuesta { id, ok, error, .. } if *id == consola => Some((*ok, error.clone())),
            _ => None,
        });
        let (ok, error) = r.expect("la consola recibe un error");
        assert!(!ok && error.contains("variables"));
        let (ok, valor, _) = respuesta(&mut m, cat, Duration::from_secs(60)).expect("el catálogo se repite tras reabrir");
        assert!(ok && valor.as_array().map(|a| !a.is_empty()).unwrap_or(false));
        assert!(!avisos.is_empty() || m.listo);
        // Tres caídas seguidas en un minuto: se deja parado
        for _ in 0..MAX_REINICIOS {
            if !m.listo {
                listo(&mut m);
            }
            m.matar_para_prueba();
            esperar(&mut m, Duration::from_secs(5), |x| matches!(x, Mensaje::Reiniciado(_) | Mensaje::Terminado(_)).then_some(()));
        }
        assert!(m.caido.is_some());
    }
}
