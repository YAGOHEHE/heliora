//! Página de validación: pruebas por fases, con el error numérico de cada medida.
use crate::iconos::Icono;
use crate::motor::Motor;
use crate::tema;
use crate::ui;
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::{json, Value};

pub struct Validacion {
    resultado: Value,
    id: Option<u64>,
    avance: f32,
    texto: String,
    error: String,
    pub cuando: String,
    /// Catálogo del banco (cada fase con sus pruebas), para contar aunque no se hayan ejecutado
    catalogo: Value,
    id_catalogo: Option<u64>,
    /// Huella y versiones de la última validación
    pub procedencia: Value,
    fase_sel: usize,
    categoria: String,
    solo_fallan: bool,
    prueba_sel: Option<String>,
    aviso_oculto: bool,
}

impl Validacion {
    /// Cinta de Validación: ejecutar (todas o hasta la fase elegida), detener, filtrar y refrescar el catálogo.
    pub fn cinta(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        let libre = !self.ocupada();
        ui::grupo_cinta(ui, "Ejecutar", |ui| {
            if ui::boton_cinta(ui, Icono::Simular, "Ejecutar todas", false).clicked() && libre {
                self.ejecutar(motor, 4);
            }
            let botones = [(Icono::Simular, "Ejecutar esta", libre && motor.listo), (Icono::Detener, "Detener", !libre)];
            match ui::columna_cinta(ui, &botones) {
                Some(0) => self.ejecutar(motor, self.fase_sel as u64 + 1),
                Some(1) => {
                    motor.llamar("cancelar", json!({}));
                }
                _ => {}
            }
        });
        ui::grupo_cinta(ui, "Filtro", |ui| match ui::columna_cinta(ui, &[(Icono::Filtro, "Solo las que fallan", true), (Icono::Lista, "Todas las categorías", true)]) {
            Some(0) => self.solo_fallan = !self.solo_fallan,
            Some(1) => self.categoria.clear(),
            _ => {}
        });
        ui::grupo_cinta(ui, "Banco", |ui| {
            if ui::boton_cinta(ui, Icono::Validar, "Actualizar catálogo", false).clicked() {
                self.pedir_catalogo(motor);
            }
        });
    }

    pub fn nueva() -> Validacion {
        // El último resultado guardado, para verlo sin esperar a repetir las pruebas
        let carpeta = crate::motor::carpeta_motor();
        let mut resultado = Value::Null;
        let mut cuando = String::new();
        let mut procedencia = Value::Null;
        if let Ok(t) = std::fs::read_to_string(carpeta.join("ultima_validacion.json")) {
            if let Ok(v) = serde_json::from_str::<Value>(&t) {
                cuando = v["cuando"].as_str().unwrap_or("").to_string();
                resultado = v["fases"].clone();
                procedencia = v["procedencia"].clone();
            }
        }
        Validacion { resultado, id: None, avance: 0.0, texto: String::new(), error: String::new(), cuando, catalogo: Value::Null, id_catalogo: None, procedencia, fase_sel: 0, categoria: String::new(), solo_fallan: false, prueba_sel: None, aviso_oculto: false }
    }

    /// Pide al motor el catálogo de pruebas por fase.
    pub fn pedir_catalogo(&mut self, motor: &mut Motor) {
        self.id_catalogo = Some(motor.llamar("pruebas", json!({})));
    }

    /// Número de pruebas de una fase según el catálogo del motor.
    fn total_fase(&self, fase: u64) -> Option<u64> {
        self.catalogo["fases"].as_array()?.iter().find(|f| f["fase"].as_u64() == Some(fase))?["total"].as_u64()
    }

    pub fn ocupada(&self) -> bool {
        self.id.is_some()
    }

    pub fn avance(&mut self, id: u64, f: f32, t: &str) -> bool {
        if Some(id) == self.id {
            self.avance = f;
            if !t.is_empty() {
                self.texto = t.to_string();
            }
            return true;
        }
        false
    }

    pub fn recibir(&mut self, id: u64, ok: bool, v: &Value, err: &str) -> bool {
        if Some(id) == self.id_catalogo {
            self.id_catalogo = None;
            if ok {
                self.catalogo = v.clone();
            }
            return true;
        }
        if Some(id) == self.id {
            self.id = None;
            if ok {
                self.resultado = v["fases"].clone();
                self.procedencia = v["procedencia"].clone();
                self.cuando = v["cuando"].as_str().unwrap_or("").to_string();
                self.error.clear();
                self.aviso_oculto = false;
            } else {
                self.error = err.to_string();
            }
            return true;
        }
        false
    }

    /// Lanza la validación hasta la fase indicada (4 = todas).
    pub fn ejecutar(&mut self, motor: &mut Motor, hasta_fase: u64) {
        if self.id.is_none() {
            self.id = Some(motor.llamar("validar", json!({"hasta_fase": hasta_fase})));
            self.avance = 0.0;
        }
    }

    /// Motivo por el que la última validación ya no describe el código actual, o None si sigue valiendo
    /// (o aún no se sabe: sin resultado o sin la huella actual del motor).
    pub fn historica(&self) -> Option<&'static str> {
        if self.resultado.is_null() {
            return None;
        }
        motivo_historica(&self.procedencia, &self.catalogo["huella"])
    }

    /// Resumen de cada fase para la portada: título, estado y pruebas superadas sobre el total.
    pub fn resumen_fases(&self) -> Vec<ResumenFase> {
        let fases = self.resultado.as_array();
        (1..=4u64)
            .map(|n| {
                let titulo = format!("Fase {n} · {}", TITULOS_FASE[n as usize - 1]);
                let total = self.total_fase(n);
                match fases.and_then(|a| a.iter().find(|x| x["fase"].as_u64() == Some(n))) {
                    Some(f) => {
                        let (nivel, _) = estado_fase(f);
                        let bien = f["pruebas"].as_array().map_or(0, |p| p.iter().filter(|q| q["ok"].as_bool() == Some(true)).count() as u64);
                        let total = total.or_else(|| f["pruebas"].as_array().map(|p| p.len() as u64));
                        ResumenFase { titulo, nivel: Some(nivel), bien: Some(bien), total }
                    }
                    None => ResumenFase { titulo, nivel: None, bien: None, total },
                }
            })
            .collect()
    }

    fn fase(&self, n: u64) -> Option<&Value> {
        self.resultado.as_array()?.iter().find(|x| x["fase"].as_u64() == Some(n))
    }

    /// Pantalla de Validación: título, fichas de fase, tabla de pruebas y detalle de la prueba elegida.
    pub fn ui(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        ui::pagina(ui, |ui| {
            ui.horizontal(|ui| {
                ui::titulo_pantalla(ui, "Validación por fases", "Una fase no se ejecuta hasta que la anterior pasa todas sus pruebas · alrededor de dos minutos");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| self.accion_principal(ui, motor));
            });
            if !self.error.is_empty() {
                tema::estado(ui, tema::Nivel::Falla, &self.error.clone());
            }
            if let Some(m) = self.historica().filter(|_| !self.aviso_oculto) {
                if ui::aviso_negro(ui, &format!("Resultados históricos: {m} desde esta ejecución. Vuelve a validar para comprobar la versión actual.")) {
                    self.aviso_oculto = true;
                }
            }
            let resumen = self.resumen_fases();
            ui::fila(ui, &[2, 2, 2, 2, 4], 84.0, |i, ui| {
                if i < 4 {
                    if self.ficha_fase(ui, i, &resumen[i]).clicked() {
                        self.fase_sel = i;
                        self.categoria.clear();
                        self.prueba_sel = None;
                    }
                } else {
                    self.ficha_ultima(ui);
                }
            });
            let alto = (ui.available_height() - ui::MARGEN).max(420.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui::fila_ajustable(ui, "validacion_detalle", alto, |i, ui| {
                    if i == 0 {
                        self.tabla_pruebas(ui);
                    } else {
                        self.detalle(ui, motor);
                    }
                });
            });
        });
    }

    /// Botón principal del título: ejecutar todas las fases o, durante la ejecución, detenerla.
    fn accion_principal(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        if self.ocupada() {
            if ui::boton(ui, ui::EstiloBoton::Secundario, Some(Icono::Detener), "Detener", "", false).clicked() {
                motor.llamar("cancelar", json!({}));
            }
        } else {
            let texto = if self.historica().is_some() { "Volver a validar" } else { "Ejecutar todas" };
            if ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Simular), texto, "", false).clicked() {
                self.ejecutar(motor, 4);
            }
        }
    }

    /// Ficha seleccionable de una fase: estado con su punto, pruebas superadas sobre el total y nombre.
    fn ficha_fase(&self, ui: &mut egui::Ui, i: usize, r: &ResumenFase) -> egui::Response {
        let pal = tema::p();
        let rect = ui.max_rect();
        let (color, texto) = match r.nivel {
            Some(tema::Nivel::Ok) => (pal.ok, "Superada"),
            Some(tema::Nivel::Alerta) => (pal.alerta, "Bloqueada"),
            Some(tema::Nivel::Falla) => (pal.falla, "Con fallos"),
            None => (pal.texto_3, "Sin ejecutar"),
        };
        let cuenta = match (r.bien, r.total) {
            (Some(b), Some(t)) => format!("{b}/{t}"),
            (None, Some(t)) => format!("—/{t}"),
            _ => String::new(),
        };
        // Los espacios iniciales dejan sitio al punto de estado, que se dibuja sobre la ficha.
        let resp = ui::ficha_kpi(ui, &format!("    Fase {} · {texto}", i + 1), &cuenta, "", TITULOS_FASE[i], Some(self.fase_sel == i));
        ui.painter().circle_filled(Pos2::new(rect.left() + 17.0, rect.top() + 18.0), 4.0, color);
        resp
    }

    /// Ficha de la última ejecución: fecha, huella del banco y versión del motor; avance mientras se ejecuta.
    fn ficha_ultima(&self, ui: &mut egui::Ui) {
        if self.ocupada() {
            ui::ficha_kpi(ui, "Última ejecución", &format!("{:.0} %", self.avance * 100.0), "", &self.texto, None);
        } else if self.cuando.is_empty() {
            ui::ficha_kpi(ui, "Última ejecución", "Sin ejecutar", "", "", None);
        } else {
            let nota = self.procedencia["sha256"].as_str().map_or(String::new(), |h| {
                let linea = format!("banco {} · motor {}", &h[..h.len().min(12)], self.procedencia["version"].as_str().unwrap_or("?"));
                if self.historica().is_some() { format!("{linea} · histórica") } else { linea }
            });
            ui::ficha_kpi(ui, "Última ejecución", &self.cuando, "", &nota, None);
        }
    }

    /// Pruebas de la fase elegida tras aplicar el filtro de categoría y «Solo las que fallan».
    fn pruebas_visibles(&self, fase: &Value) -> Vec<Value> {
        fase["pruebas"]
            .as_array()
            .map(|a| a.iter().filter(|p| (self.categoria.is_empty() || categoria(p) == self.categoria) && (!self.solo_fallan || p["ok"].as_bool() != Some(true))).cloned().collect())
            .unwrap_or_default()
    }

    fn tabla_pruebas(&mut self, ui: &mut egui::Ui) {
        let pal = tema::p();
        let n = self.fase_sel as u64 + 1;
        let titulo = format!("Fase {n} · {}", TITULOS_FASE[n as usize - 1]);
        let fase = self.fase(n).cloned();
        ui::tarjeta_plena(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let ancho = ui.available_width();
            ui.allocate_ui(Vec2::new(ancho, 44.0), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new(titulo).size(15.0).family(tema::semi()).color(pal.texto));
                    if let Some(f) = &fase {
                        let total = f["pruebas"].as_array().map_or(0, Vec::len);
                        ui.label(egui::RichText::new(format!("{total} pruebas")).size(12.5).color(pal.texto_2));
                    }
                });
            });
            if let Some(f) = &fase {
                let mut cats: Vec<(String, usize)> = vec![];
                for p in f["pruebas"].as_array().into_iter().flatten() {
                    let c = categoria(p);
                    match cats.iter_mut().find(|(k, _)| *k == c) {
                        Some(e) => e.1 += 1,
                        None => cats.push((c, 1)),
                    }
                }
                cats.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                ui.allocate_ui(Vec2::new(ancho, 44.0), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add_space(16.0);
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if ui::chip(ui, "Todas", self.categoria.is_empty()).clicked() {
                            self.categoria.clear();
                        }
                        for (c, _) in cats.iter().take(5) {
                            if ui::chip(ui, c, self.categoria == *c).clicked() {
                                self.categoria = c.clone();
                            }
                        }
                        ui.add_space(8.0);
                        ui.checkbox(&mut self.solo_fallan, "Solo las que fallan");
                    });
                });
            }
            let (linea, _) = ui.allocate_exact_size(Vec2::new(ancho, 1.0), Sense::hover());
            ui.painter().rect_filled(linea, 0.0, pal.linea);
            let Some(f) = fase else {
                ui::estado_vacio(ui, "Esta fase aún no se ha ejecutado.", "");
                return;
            };
            if let Some(m) = f["motivo"].as_str() {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    tema::estado(ui, tema::Nivel::Alerta, m);
                });
            }
            let visibles = self.pruebas_visibles(&f);
            if self.prueba_sel.as_ref().is_none_or(|s| !visibles.iter().any(|p| p["nombre"].as_str() == Some(s))) {
                let primera = visibles.iter().find(|p| p["ok"].as_bool() != Some(true)).or(visibles.first());
                self.prueba_sel = primera.and_then(|p| p["nombre"].as_str().map(String::from));
            }
            self.cabecera_columnas(ui);
            egui::ScrollArea::vertical().id_salt(("val_filas", n)).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if visibles.is_empty() {
                    ui::estado_vacio(ui, "Ninguna prueba coincide con el filtro.", "");
                }
                for (i, p) in visibles.iter().enumerate() {
                    let nombre = p["nombre"].as_str().unwrap_or("");
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 43.0), Sense::hover());
                    let marcada = self.prueba_sel.as_deref() == Some(nombre);
                    if marcada {
                        ui.painter().rect_filled(r, 0.0, pal.campo);
                    } else if ui.rect_contains_pointer(r) {
                        ui.painter().rect_filled(r, 0.0, pal.realce);
                    }
                    ui::fondo_fila(ui, r, i);
                    let ok = p["ok"].as_bool() == Some(true);
                    let (c_nombre, c_cat, c_err) = columnas(r);
                    ui::punto_estado(ui.painter(), Pos2::new(r.left() + 24.0, r.center().y), if ok { ui::EstadoNodo::Listo } else { ui::EstadoNodo::Pendiente });
                    ui.painter().with_clip_rect(c_nombre).text(Pos2::new(c_nombre.left(), r.center().y), Align2::LEFT_CENTER, nombre, FontId::proportional(13.0), pal.texto);
                    let rc = Rect::from_min_size(Pos2::new(c_cat.left(), r.center().y - 11.0), Vec2::new(c_cat.width(), 22.0));
                    let mut chip = ui.new_child(egui::UiBuilder::new().max_rect(rc));
                    chip.set_clip_rect(rc.intersect(ui.clip_rect()));
                    ui::chip(&mut chip, &categoria(p), false);
                    let err = peor_error(p).unwrap_or_else(|| "—".into());
                    ui.painter().text(Pos2::new(c_err.right(), r.center().y), Align2::RIGHT_CENTER, err, tema::fuente_mono(12.5), pal.texto);
                    // La interacción se crea la última para que la fila reciba el clic sobre el chip.
                    if ui.interact(r, ui.id().with(("fila_prueba", i)), Sense::click()).clicked() {
                        self.prueba_sel = Some(nombre.to_string());
                    }
                }
            });
        });
    }

    fn cabecera_columnas(&self, ui: &mut egui::Ui) {
        let pal = tema::p();
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, pal.fondo);
        let (c_nombre, c_cat, c_err) = columnas(r);
        for (t, x, a) in [("Prueba", c_nombre.left(), Align2::LEFT_CENTER), ("Categoría", c_cat.left(), Align2::LEFT_CENTER), ("Error numérico", c_err.right(), Align2::RIGHT_CENTER)] {
            ui.painter().text(Pos2::new(x, r.center().y), a, t, FontId::new(13.0, tema::semi()), pal.texto);
        }
        ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0_f32, pal.linea));
    }

    fn detalle(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        let pal = tema::p();
        let n = self.fase_sel as u64 + 1;
        let prueba = self.fase(n).and_then(|f| f["pruebas"].as_array()).and_then(|a| a.iter().find(|p| p["nombre"].as_str() == self.prueba_sel.as_deref())).cloned();
        ui::tarjeta_con_titulo(
            ui,
            "Prueba",
            |_| {},
            |ui| {
                let Some(p) = prueba else {
                    ui::estado_vacio(ui, "Elige una prueba de la tabla para ver lo esperado y lo medido.", "");
                    return;
                };
                let ok = p["ok"].as_bool() == Some(true);
                let alto = (ui.available_height() - 50.0).max(100.0);
                egui::ScrollArea::vertical().id_salt("val_detalle").max_height(alto).auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui::chip(ui, &categoria(&p), false);
                        let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 22.0), Sense::hover());
                        ui::punto_estado(ui.painter(), r.center(), if ok { ui::EstadoNodo::Listo } else { ui::EstadoNodo::Pendiente });
                        ui.label(egui::RichText::new(if ok { "Superada" } else { "Con fallos" }).size(13.0).color(pal.texto));
                    });
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(p["nombre"].as_str().unwrap_or("")).size(16.0).family(tema::semi()).color(pal.texto));
                    if let Some(d) = p["doc"].as_str().filter(|d| !d.is_empty()) {
                        ui.label(egui::RichText::new(d).color(pal.texto_2));
                    }
                    if let Some(r) = p["referencia"].as_str().filter(|r| !r.is_empty()) {
                        ui.label(egui::RichText::new(format!("Referencia: {r}")).font(tema::fuente_mono(11.5)).color(pal.texto_2));
                    }
                    ui.add_space(10.0);
                    medidas(ui, &p);
                    if let Some(ex) = p["excepcion"].as_str() {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(ex).font(tema::fuente_mono(11.0)).color(pal.texto));
                    }
                });
                ui.add_space(8.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let libre = !self.ocupada() && motor.listo;
                    let ayuda = format!("El motor solo ejecuta por fases: esto ejecuta las fases 1 a {n}, no solo esta prueba.");
                    let r = ui.add_enabled_ui(libre, |ui| ui::boton(ui, ui::EstiloBoton::Principal, Some(Icono::Simular), "Ejecutar esta", "", false)).inner;
                    if r.on_hover_text(&ayuda).on_disabled_hover_text(&ayuda).clicked() {
                        self.ejecutar(motor, n);
                    }
                });
            },
        );
    }
}
/// Resumen de una fase para la portada.
/// Compara la procedencia de una validación guardada con la huella actual del banco y del motor.
///
/// Returns:
///     El motivo si ya no corresponde al código actual; None si coincide o si falta la huella actual.
pub fn motivo_historica(procedencia: &Value, actual: &Value) -> Option<&'static str> {
    let actual_banco = actual["sha256"].as_str()?;
    let actual_motor = actual["sha256_motor"].as_str()?;
    if procedencia["sha256"].as_str() != Some(actual_banco) {
        Some("el banco de pruebas cambió")
    } else if procedencia["sha256_motor"].as_str() != Some(actual_motor) {
        Some("el motor cambió")
    } else {
        None
    }
}

pub struct ResumenFase {
    pub titulo: String,
    /// None si la fase no se ha ejecutado
    pub nivel: Option<tema::Nivel>,
    pub bien: Option<u64>,
    pub total: Option<u64>,
}

const TITULOS_FASE: [&str; 4] = ["Cálculo y solvers", "Potencia y fotovoltaica", "Estadística y ML", "Mantenimiento"];

/// Estado de una fase del resultado del motor.
fn estado_fase(f: &Value) -> (tema::Nivel, &'static str) {
    if f["ok"].as_bool() == Some(true) {
        (tema::Nivel::Ok, "Superada")
    } else if f["bloqueada"].as_bool() == Some(true) {
        (tema::Nivel::Alerta, "Bloqueada")
    } else {
        (tema::Nivel::Falla, "Con fallos")
    }
}

/// Categoría de una prueba: el prefijo de su nombre antes de «:», o «General» si no lo tiene.
fn categoria(p: &Value) -> String {
    let n = p["nombre"].as_str().unwrap_or("");
    match n.split_once(':') {
        Some((c, _)) if !c.trim().is_empty() && c.chars().count() <= 24 => c.trim().to_string(),
        _ => "General".into(),
    }
}

/// Cociente error/tolerancia de una medida numérica; None en las comprobaciones sí/no.
fn razon(m: &Value) -> Option<f64> {
    let tol = m["tolerancia"].as_f64()?;
    let err = m["error"].as_f64()?;
    if es_booleana(m) || !err.is_finite() {
        return None;
    }
    Some(if tol > 0.0 { err / tol } else { err })
}

fn es_booleana(m: &Value) -> bool {
    m["tolerancia"].as_f64() == Some(0.0) && m["referencia"].as_f64() == Some(1.0)
}

fn texto_error(m: &Value) -> String {
    let err = m["error"].as_f64().unwrap_or(f64::NAN);
    if m["relativo"].as_bool() == Some(true) {
        format!("{:.3} %", err * 100.0)
    } else {
        format!("{err:.2e}")
    }
}

/// Error numérico de la prueba: el de su medida más cercana a la tolerancia.
fn peor_error(p: &Value) -> Option<String> {
    let ms = p["medidas"].as_array()?;
    ms.iter().filter_map(|m| razon(m).map(|r| (r, m))).max_by(|a, b| a.0.total_cmp(&b.0)).map(|(_, m)| texto_error(m))
}

/// Rectángulos de las tres columnas de texto de una fila de la tabla: prueba, categoría y error.
fn columnas(r: Rect) -> (Rect, Rect, Rect) {
    let ancho_err = 120.0;
    let ancho_cat = 150.0;
    let fin_err = r.right() - 16.0;
    let c_err = Rect::from_min_max(Pos2::new(fin_err - ancho_err, r.top()), Pos2::new(fin_err, r.bottom()));
    let c_cat = Rect::from_min_max(Pos2::new(c_err.left() - 24.0 - ancho_cat, r.top()), Pos2::new(c_err.left() - 24.0, r.bottom()));
    let c_nombre = Rect::from_min_max(Pos2::new(r.left() + 44.0, r.top()), Pos2::new(c_cat.left() - 12.0, r.bottom()));
    (c_nombre, c_cat, c_err)
}

/// Esperado frente a medido de cada comprobación y, si hay varias, su error relativo a la tolerancia.
fn medidas(ui: &mut egui::Ui, p: &Value) {
    let pal = tema::p();
    let ms: Vec<&Value> = p["medidas"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    if ms.is_empty() {
        ui.label(egui::RichText::new("Esta prueba no devuelve medidas numéricas.").color(pal.texto_2));
        return;
    }
    ui::titulo_seccion(ui, "Esperado frente a medido");
    ui.add_space(4.0);
    for m in &ms {
        let mok = m["ok"].as_bool() == Some(true);
        let num = |k: &str| crate::grafica::numero(m[k].as_f64().unwrap_or(f64::NAN));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 18.0), Sense::hover());
            ui::punto_estado(ui.painter(), r.center(), if mok { ui::EstadoNodo::Listo } else { ui::EstadoNodo::Pendiente });
            ui.label(egui::RichText::new(m["nombre"].as_str().unwrap_or("")).color(pal.texto));
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            if es_booleana(m) {
                ui.label(egui::RichText::new(if mok { "se cumple" } else { "no se cumple" }).font(tema::fuente_mono(12.0)).color(pal.texto));
            } else {
                let relativo = m["relativo"].as_bool() == Some(true);
                let unidad = if relativo { "" } else { m["unidad"].as_str().unwrap_or("") };
                let tol = m["tolerancia"].as_f64().unwrap_or(0.0);
                let tol = if relativo { format!("{:.2} %", tol * 100.0) } else { format!("{tol:.1e}") };
                ui.label(egui::RichText::new(format!("esp. {}", num("referencia"))).font(tema::fuente_mono(12.0)).color(pal.texto_2));
                ui.label(egui::RichText::new(format!("med. {} {unidad}", num("valor"))).font(tema::fuente_mono(12.0)).color(pal.texto));
                ui.label(egui::RichText::new(format!("err. {} / {tol}", texto_error(m))).font(tema::fuente_mono(12.0)).color(pal.texto));
            }
        });
        ui.add_space(6.0);
    }
    let razones: Vec<(String, f64, bool)> = ms.iter().filter_map(|m| razon(m).map(|r| (m["nombre"].as_str().unwrap_or("").to_string(), r, m["ok"].as_bool() == Some(true)))).collect();
    if razones.len() < 2 {
        return;
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new(ui::mayusculas("Error frente a la tolerancia")).size(10.5).family(tema::semi()).color(pal.texto_2));
    ui.add_space(4.0);
    let escala = razones.iter().map(|x| x.1).fold(1.0_f64, f64::max) * 1.05;
    let ancho = ui.available_width();
    let x0 = ancho * 0.42;
    let (area, _) = ui.allocate_exact_size(Vec2::new(ancho, 20.0 * razones.len() as f32), Sense::hover());
    let pt = ui.painter_at(area);
    for (i, (nombre, r, ok)) in razones.iter().enumerate() {
        let y = area.top() + 20.0 * i as f32;
        pt.with_clip_rect(Rect::from_min_size(Pos2::new(area.left(), y), Vec2::new(x0 - 8.0, 20.0))).text(Pos2::new(area.left(), y + 10.0), Align2::LEFT_CENTER, nombre, FontId::proportional(11.5), pal.texto_2);
        let largo = ((r / escala) as f32 * (ancho - x0)).max(1.0);
        pt.rect_filled(Rect::from_min_size(Pos2::new(area.left() + x0, y + 5.0), Vec2::new(largo, 10.0)), 2.0, if *ok { pal.acento } else { pal.falla });
    }
    let x_tol = area.left() + x0 + (1.0 / escala) as f32 * (ancho - x0);
    pt.extend(egui::Shape::dashed_line(&[Pos2::new(x_tol, area.top()), Pos2::new(x_tol, area.bottom())], Stroke::new(1.0_f32, pal.texto_2), 3.0, 2.0));
    ui.label(egui::RichText::new("La línea discontinua marca la tolerancia: a su derecha, la prueba falla.").size(11.0).color(pal.texto_3));
}

#[cfg(test)]
mod pruebas {
    use super::motivo_historica;
    use serde_json::json;

    #[test]
    fn la_validacion_guardada_es_historica_si_cambia_el_banco_o_el_motor() {
        let actual = json!({"sha256": "b1", "sha256_motor": "m1"});
        assert_eq!(motivo_historica(&json!({"sha256": "b1", "sha256_motor": "m1"}), &actual), None);
        assert_eq!(motivo_historica(&json!({"sha256": "b0", "sha256_motor": "m1"}), &actual), Some("el banco de pruebas cambió"));
        assert_eq!(motivo_historica(&json!({"sha256": "b1", "sha256_motor": "m0"}), &actual), Some("el motor cambió"));
        // Una validación anterior a la huella del motor no puede confirmar que siga valiendo
        assert_eq!(motivo_historica(&json!({"sha256": "b1"}), &actual), Some("el motor cambió"));
        // Sin la huella actual (motor aún arrancando) no se marca nada
        assert_eq!(motivo_historica(&json!({"sha256": "b0"}), &json!(null)), None);
    }
}
