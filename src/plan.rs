//! Módulo 7: planificador de mantenimiento. Catálogo, planes, órdenes, calendario, historial y AMEF.
use crate::herramientas::{Herramienta, Pagina};
use crate::app::Compartido;
use crate::motor::Motor;
use crate::tema;
use egui::{Pos2, Rect, Sense, Vec2};
use serde_json::{json, Value};

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
    filtro: String,
    id_estado: Option<u64>,
    id_tablero: Option<u64>,
    filtro_amef: String,
    pub ruta: Option<std::path::PathBuf>,
    pub mensaje: String,
    id_informe: Option<u64>,
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

const MESES: [&str; 12] = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
const TIPOS: [&str; 5] = ["Módulos", "Inversores", "Trackers", "Transformadores", "Cableado"];

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
            filtro: "pendiente".into(),
            id_estado: None,
            id_tablero: None,
            filtro_amef: String::new(),
            ruta: None,
            mensaje: String::new(),
            id_informe: None,
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
        let id = motor.llamar("plan_inicial", json!({"hoy": self.hoy, "hasta": self.hasta}));
        self.id_estado = Some(id);
    }

    pub fn recibir(&mut self, id: u64, ok: bool, v: &Value, err: &str, c: &mut Compartido) -> bool {
        if self.analisis.recibir(id, ok, v, err, c) {
            return true;
        }
        if Some(id) == self.id_informe {
            self.id_informe = None;
            self.mensaje = if ok {
                let pdf = v["pdf"].as_str().unwrap_or("");
                abrir_con_el_sistema(v["html"].as_str().unwrap_or(""));
                format!("Informe escrito: {} hallazgos, {} órdenes próximas, {} vencidas ({} páginas). PDF en {}", v["hallazgos"], v["proximas"], v["vencidas"], v["paginas"], pdf)
            } else {
                format!("No se pudo escribir el informe: {err}")
            };
            return true;
        }
        if Some(id) == self.id_estado {
            self.id_estado = None;
            if !ok {
                self.mensaje = err.to_string();
                return true;
            }
            self.estado = v["estado"].clone();
            self.tablero = v["tablero"].clone();
            if let Some(n) = v.get("nuevas").and_then(|n| n.as_i64()) {
                self.mensaje = format!("{n} órdenes nuevas.");
            }
            return true;
        }
        if Some(id) == self.id_tablero {
            self.id_tablero = None;
            if ok {
                self.tablero = v.clone();
            }
            return true;
        }
        false
    }

    fn generar(&mut self, motor: &mut Motor) {
        self.id_estado = Some(motor.llamar("plan_generar", json!({"estado": self.estado, "hasta": self.hasta, "hoy": self.hoy})));
    }

    fn tablero_pedir(&mut self, motor: &mut Motor) {
        self.id_tablero = Some(motor.llamar("plan_tablero", json!({"estado": self.estado, "hoy": self.hoy})));
    }

    pub fn guardar_en(&self, ruta: &std::path::Path) -> Result<(), String> {
        let t = serde_json::to_string_pretty(&self.estado).map_err(|e| e.to_string())?;
        std::fs::write(ruta, t).map_err(|e| e.to_string())
    }

    pub fn cargar_de(&mut self, ruta: &std::path::Path, motor: &mut Motor) -> Result<(), String> {
        let t = std::fs::read_to_string(ruta).map_err(|e| e.to_string())?;
        self.estado = serde_json::from_str(&t).map_err(|e| e.to_string())?;
        self.ruta = Some(ruta.to_path_buf());
        self.tablero_pedir(motor);
        Ok(())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, todas: &[Herramienta], motor: &mut Motor, c: &mut Compartido) {
        egui::TopBottomPanel::top("pest_mant").frame(tema::marco_panel()).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                for (p, t) in [(Pestana::Analisis, "Análisis"), (Pestana::Tablero, "Tablero y calendario"), (Pestana::Ordenes, "Órdenes de trabajo"), (Pestana::Catalogo, "Catálogo de equipos"), (Pestana::Planes, "Planes"), (Pestana::Amef, "AMEF"), (Pestana::Historial, "Historial")] {
                    if ui.selectable_label(self.pestana == p, t).clicked() {
                        self.pestana = p;
                    }
                }
                if self.pestana != Pestana::Analisis {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Guardar plan").on_hover_text("Guarda el plan en JSON").clicked() {
                            let r = self.ruta.clone().or_else(|| rfd::FileDialog::new().add_filter("Plan JSON", &["json"]).set_file_name("mantenimiento.json").save_file());
                            if let Some(r) = r {
                                self.mensaje = match self.guardar_en(&r) {
                                    Ok(_) => format!("Plan guardado en {}", r.display()),
                                    Err(e) => e,
                                };
                                self.ruta = Some(r);
                            }
                        }
                        let ocupado = self.id_informe.is_some();
                        if ui.add_enabled(!ocupado, egui::Button::new("Informe O&M (HTML y PDF)")).on_hover_text("Indicadores, hallazgos, órdenes y calendario de 30 días").clicked() {
                            if let Some(c) = rfd::FileDialog::new().set_title("Carpeta para el informe").pick_folder() {
                                self.id_informe = Some(motor.llamar("informe_om", json!({"estado": self.estado, "carpeta": c.display().to_string(), "hoy": self.hoy})));
                                self.mensaje = "Escribiendo el informe...".into();
                            }
                        }
                        if ui.button("Abrir plan").clicked() {
                            if let Some(r) = rfd::FileDialog::new().add_filter("Plan JSON", &["json"]).pick_file() {
                                if let Err(e) = self.cargar_de(&r, motor) {
                                    self.mensaje = e;
                                }
                            }
                        }
                    });
                }
            });
        });
        if self.pestana == Pestana::Analisis {
            self.analisis.ui(ui, todas, motor, c);
            return;
        }
        egui::CentralPanel::default().frame(tema::marco_fondo()).show_inside(ui, |ui| {
            if self.estado.is_null() {
                ui.label(egui::RichText::new("Preparando el plan de mantenimiento...").color(tema::p().texto_2));
                return;
            }
            if !self.mensaje.is_empty() {
                ui.label(egui::RichText::new(&self.mensaje).small().color(tema::p().texto));
            }
            match self.pestana {
                Pestana::Tablero => self.p_tablero(ui, motor),
                Pestana::Ordenes => self.p_ordenes(ui, motor),
                Pestana::Catalogo => self.p_catalogo(ui),
                Pestana::Planes => self.p_planes(ui),
                Pestana::Amef => self.p_amef(ui),
                Pestana::Historial => self.p_historial(ui),
                Pestana::Analisis => {}
            }
        });
    }

    fn p_tablero(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Generar órdenes hasta");
            ui.add(egui::TextEdit::singleline(&mut self.hasta).desired_width(90.0));
            if tema::boton_principal(ui, "Generar").clicked() {
                self.generar(motor);
            }
            if ui.button("Crear correctivas desde fallas").on_hover_text("Usa los hallazgos de «Detección de fallas»").clicked() {
                self.id_estado = Some(motor.llamar("plan_correctivas", json!({"estado": self.estado, "hoy": self.hoy})));
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
            self.calendario(ui, &ordenes);
            if let Some(dia) = self.dia_sel.clone() {
                ui.add_space(8.0);
                tema::tarjeta(ui, |ui| {
                    ui.set_width(ui.available_width());
                    tema::titulo(ui, &format!("Órdenes del {dia}"));
                    let del_dia: Vec<&Value> = ordenes.iter().filter(|o| o["fecha"].as_str() == Some(&dia)).collect();
                    if del_dia.is_empty() {
                        ui.label(egui::RichText::new("Ninguna orden este día.").color(tema::p().texto_2));
                    }
                    for o in del_dia {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(o["id"].as_str().unwrap_or("")).monospace());
                            ui.label(o["nombre"].as_str().unwrap_or(""));
                            ui.label(egui::RichText::new(o["equipo"].as_str().unwrap_or("")).color(tema::p().acento_2));
                            ui.label(egui::RichText::new(o["estado"].as_str().unwrap_or("")).color(tema::p().texto_2));
                        });
                    }
                });
            }
        });
    }

    fn calendario(&mut self, ui: &mut egui::Ui, ordenes: &[Value]) {
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
        for dia in 1..=dias {
            let idx = primero + dia - 1;
            let (cx, cy) = ((idx % 7) as f32, (idx / 7) as f32);
            let r = Rect::from_min_size(Pos2::new(rect.left() + cx * celda.x, rect.top() + 22.0 + cy * celda.y), celda).shrink(2.0);
            let f = fecha(self.anio, self.mes, dia);
            let elegido = self.dia_sel.as_deref() == Some(&f);
            p.rect(r, tema::radio() as f32, if elegido { tema::p().realce } else { tema::p().campo }, egui::Stroke::new(1.0, tema::p().linea), egui::StrokeKind::Inside);
            if f == hoy {
                p.rect_stroke(r, tema::radio() as f32, egui::Stroke::new(1.5, tema::p().acento), egui::StrokeKind::Middle);
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
                        self.dia_sel = Some(f.clone());
                    }
                }
            }
        }
    }

    fn p_ordenes(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        ui.horizontal(|ui| {
            for (v, t) in [("pendiente", "Pendientes"), ("cerrada", "Cerradas"), ("todas", "Todas")] {
                if ui.selectable_label(self.filtro == v, t).clicked() {
                    self.filtro = v.into();
                }
            }
            ui.separator();
            ui.label("Generar hasta");
            ui.add(egui::TextEdit::singleline(&mut self.hasta).desired_width(90.0));
            if ui.button("Generar").clicked() {
                self.generar(motor);
            }
        });
        let hoy = self.hoy.clone();
        let filtro = self.filtro.clone();
        let mut cerrar: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let Some(ords) = self.estado["ordenes"].as_array_mut() else { return };
            let mut visibles = 0;
            for o in ords.iter_mut() {
                let est = o["estado"].as_str().unwrap_or("pendiente").to_string();
                if filtro != "todas" && est != filtro {
                    continue;
                }
                visibles += 1;
                if visibles > 200 {
                    ui.label(egui::RichText::new("Se muestran las primeras 200 órdenes.").color(tema::p().texto_2));
                    break;
                }
                let fec = o["fecha"].as_str().unwrap_or("").to_string();
                let vencida = est != "cerrada" && fec < hoy;
                let id = o["id"].as_str().unwrap_or("").to_string();
                let cab = format!("{}  ·  {}  ·  {}  ·  {}", fec, id, o["nombre"].as_str().unwrap_or(""), o["equipo"].as_str().unwrap_or(""));
                let cab = if est == "cerrada" {
                    format!("{cab}  ·  cerrada")
                } else if vencida {
                    format!("{cab}  ·  vencida")
                } else {
                    cab
                };
                egui::CollapsingHeader::new(egui::RichText::new(cab).color(tema::p().texto)).id_salt(&id).show(ui, |ui| {
                    let mut resp = o["responsable"].as_str().unwrap_or("").to_string();
                    ui.horizontal(|ui| {
                        ui.label("Responsable");
                        if ui.text_edit_singleline(&mut resp).changed() {
                            o["responsable"] = json!(resp);
                        }
                    });
                    if let Some(ch) = o["checklist"].as_array_mut() {
                        for c in ch.iter_mut() {
                            let mut h = c["hecho"].as_bool().unwrap_or(false);
                            if ui.checkbox(&mut h, c["texto"].as_str().unwrap_or("")).changed() {
                                c["hecho"] = json!(h);
                            }
                        }
                    }
                    let mut n = o["notas"].as_str().unwrap_or("").to_string();
                    ui.label("Notas");
                    if ui.add(egui::TextEdit::multiline(&mut n).desired_rows(2).desired_width(f32::INFINITY)).changed() {
                        o["notas"] = json!(n);
                    }
                    if est != "cerrada" {
                        if tema::boton_principal(ui, "Cerrar orden").clicked() {
                            cerrar = Some(id.clone());
                        }
                    } else {
                        ui.label(egui::RichText::new(format!("Cerrada el {}", o["cerrada"].as_str().unwrap_or(""))).color(tema::p().ok));
                    }
                });
            }
            if visibles == 0 {
                ui.label(egui::RichText::new("No hay órdenes en esta vista.").color(tema::p().texto_2));
            }
        });
        if let Some(id) = cerrar {
            self.id_estado = Some(motor.llamar("plan_cerrar", json!({"estado": self.estado, "ident": id, "fecha": self.hoy, "hoy": self.hoy})));
        }
    }

    fn p_catalogo(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            tema::titulo(ui, "Equipos de la planta");
            if ui.button("+ Añadir equipo").clicked() {
                if let Some(e) = self.estado["equipos"].as_array_mut() {
                    e.push(json!({"codigo": format!("EQ-{:03}", e.len() + 1), "tipo": "Módulos", "descripcion": "", "ubicacion": "", "instalado": self.hoy}));
                }
            }
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut borrar = None;
            let Some(eq) = self.estado["equipos"].as_array_mut() else { return };
            egui::Grid::new("g_eq").striped(true).num_columns(6).spacing([10.0, 5.0]).show(ui, |ui| {
                for t in ["Código", "Tipo", "Descripción", "Ubicación", "Instalado", ""] {
                    ui.label(egui::RichText::new(t).strong().color(tema::p().texto));
                }
                ui.end_row();
                for (i, e) in eq.iter_mut().enumerate() {
                    campo(ui, e, "codigo", 80.0);
                    let mut tipo = e["tipo"].as_str().unwrap_or("").to_string();
                    egui::ComboBox::from_id_salt(("tipo", i)).selected_text(&tipo).width(120.0).show_ui(ui, |ui| {
                        for t in TIPOS {
                            if ui.selectable_value(&mut tipo, t.to_string(), t).clicked() {
                                e["tipo"] = json!(tipo);
                            }
                        }
                    });
                    campo(ui, e, "descripcion", 280.0);
                    campo(ui, e, "ubicacion", 110.0);
                    campo(ui, e, "instalado", 90.0);
                    if ui.small_button("×").clicked() {
                        borrar = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = borrar {
                eq.remove(i);
            }
        });
    }

    fn p_planes(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            tema::titulo(ui, "Planes de mantenimiento preventivo");
            if ui.button("+ Añadir plan").clicked() {
                if let Some(p) = self.estado["planes"].as_array_mut() {
                    p.push(json!({"id": format!("P{:03}", p.len() + 1), "nombre": "Plan nuevo", "tipo_equipo": "Módulos", "frecuencia_dias": 90, "checklist": ["Paso 1"], "inicio": self.hoy, "activo": true}));
                }
            }
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let Some(planes) = self.estado["planes"].as_array_mut() else { return };
            for (i, p) in planes.iter_mut().enumerate() {
                tema::tarjeta(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        let mut a = p["activo"].as_bool().unwrap_or(true);
                        if ui.checkbox(&mut a, "").changed() {
                            p["activo"] = json!(a);
                        }
                        campo(ui, p, "nombre", 260.0);
                        let mut tipo = p["tipo_equipo"].as_str().unwrap_or("").to_string();
                        egui::ComboBox::from_id_salt(("pt", i)).selected_text(&tipo).width(120.0).show_ui(ui, |ui| {
                            for t in TIPOS {
                                if ui.selectable_value(&mut tipo, t.to_string(), t).clicked() {
                                    p["tipo_equipo"] = json!(tipo);
                                }
                            }
                        });
                        ui.label("cada");
                        let mut f = p["frecuencia_dias"].as_i64().unwrap_or(30);
                        if ui.add(egui::DragValue::new(&mut f).range(1..=3650)).changed() {
                            p["frecuencia_dias"] = json!(f);
                        }
                        ui.label("días, desde");
                        campo(ui, p, "inicio", 90.0);
                    });
                    let mut texto = p["checklist"].as_array().map(|c| c.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
                    ui.label(egui::RichText::new("Lista de comprobación (un paso por línea)").small().color(tema::p().texto_2));
                    if ui.add(egui::TextEdit::multiline(&mut texto).desired_rows(2).desired_width(f32::INFINITY)).changed() {
                        p["checklist"] = json!(texto.lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>());
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    fn p_amef(&mut self, ui: &mut egui::Ui) {
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
                if let Some(a) = self.estado["amef"].as_array_mut() {
                    a.push(json!({"tipo_equipo": t, "modo": "Modo de falla", "causa": "", "efecto": "", "S": 5, "O": 5, "D": 5, "control": "", "accion": ""}));
                }
            }
            if ui.button("Exportar CSV").clicked() {
                if let Some(r) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("amef.csv").save_file() {
                    let mut s = String::from("Equipo,Modo de falla,Causa,Efecto,S,O,D,RPN,Control,Acción\n");
                    for a in self.estado["amef"].as_array().unwrap_or(&vec![]) {
                        let (ss, o, d) = (a["S"].as_i64().unwrap_or(1), a["O"].as_i64().unwrap_or(1), a["D"].as_i64().unwrap_or(1));
                        let t = |k: &str| a[k].as_str().unwrap_or("").replace(',', ";");
                        s += &format!("{},{},{},{},{ss},{o},{d},{},{},{}\n", t("tipo_equipo"), t("modo"), t("causa"), t("efecto"), ss * o * d, t("control"), t("accion"));
                    }
                    let _ = std::fs::write(r, s);
                }
            }
        });
        ui.label(egui::RichText::new("RPN = S·O·D, con severidad, ocurrencia y detección de 1 a 10. Más alto significa actuar antes.").small().color(tema::p().texto_2));
        let filtro = self.filtro_amef.clone();
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            let Some(am) = self.estado["amef"].as_array_mut() else { return };
            let mut orden: Vec<usize> = (0..am.len()).filter(|&i| filtro.is_empty() || am[i]["tipo_equipo"].as_str() == Some(&filtro)).collect();
            let rpn = |a: &Value| a["S"].as_i64().unwrap_or(1) * a["O"].as_i64().unwrap_or(1) * a["D"].as_i64().unwrap_or(1);
            orden.sort_by_key(|&i| -rpn(&am[i]));
            let mut borrar = None;
            egui::Grid::new("g_amef").striped(true).num_columns(10).spacing([8.0, 5.0]).show(ui, |ui| {
                for t in ["Equipo", "Modo de falla", "Causa", "Efecto", "S", "O", "D", "RPN", "Control / acción", ""] {
                    ui.label(egui::RichText::new(t).strong().color(tema::p().texto));
                }
                ui.end_row();
                for i in orden {
                    let a = &mut am[i];
                    ui.label(a["tipo_equipo"].as_str().unwrap_or(""));
                    campo(ui, a, "modo", 170.0);
                    campo(ui, a, "causa", 170.0);
                    campo(ui, a, "efecto", 170.0);
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
                    campo(ui, a, "control", 220.0);
                    if ui.small_button("×").clicked() {
                        borrar = Some(i);
                    }
                    ui.end_row();
                }
            });
            if let Some(i) = borrar {
                am.remove(i);
            }
        });
    }

    fn p_historial(&mut self, ui: &mut egui::Ui) {
        tema::titulo(ui, "Trabajos realizados");
        let h: Vec<Value> = self.estado["historial"].as_array().cloned().unwrap_or_default();
        if h.is_empty() {
            ui.label(egui::RichText::new("Aún no hay trabajos cerrados. Se registran al cerrar una orden.").color(tema::p().texto_2));
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("g_hist").striped(true).num_columns(5).spacing([14.0, 4.0]).show(ui, |ui| {
                for t in ["Fecha", "Orden", "Equipo", "Trabajo", "Lista completa"] {
                    ui.label(egui::RichText::new(t).strong().color(tema::p().texto));
                }
                ui.end_row();
                for r in h.iter().rev() {
                    ui.label(r["fecha"].as_str().unwrap_or(""));
                    ui.label(r["orden"].as_str().unwrap_or(""));
                    ui.label(r["equipo"].as_str().unwrap_or(""));
                    ui.label(r["trabajo"].as_str().unwrap_or(""));
                    ui.label(if r["completo"].as_bool().unwrap_or(false) { "sí" } else { "no" });
                    ui.end_row();
                }
            });
        });
    }
}

fn campo(ui: &mut egui::Ui, v: &mut Value, k: &str, ancho: f32) {
    let mut t = v[k].as_str().unwrap_or("").to_string();
    if ui.add_sized([ancho, 22.0], egui::TextEdit::singleline(&mut t)).on_hover_text(t.clone()).changed() {
        v[k] = json!(t);
    }
}

fn cifra(ui: &mut egui::Ui, t: &str, v: &str, estado: Option<tema::Nivel>) {
    let pal = tema::p();
    egui::Frame::new().fill(pal.panel).stroke(egui::Stroke::new(1.0, pal.linea)).corner_radius(tema::radio_ventana()).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
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
    let r = std::process::Command::new("cmd").args(["/C", "start", "", ruta]).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(ruta).spawn();
    let _ = r;
}
