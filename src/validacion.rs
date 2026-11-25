//! Página de validación: pruebas por fases, con el error numérico de cada medida.
use crate::motor::Motor;
use crate::tema;
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
}

impl Validacion {
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
        Validacion { resultado, id: None, avance: 0.0, texto: String::new(), error: String::new(), cuando, catalogo: Value::Null, id_catalogo: None, procedencia }
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
            } else {
                self.error = err.to_string();
            }
            return true;
        }
        false
    }

    pub fn resumen(&self, ui: &mut egui::Ui) {
        let titulos = ["Fase 1: cálculo y solvers", "Fase 2: potencia y fotovoltaica", "Fase 3: estadística y ML", "Fase 4: mantenimiento"];
        let fases = self.resultado.as_array();
        egui::Grid::new("g_resumen").num_columns(3).spacing([12.0, 5.0]).show(ui, |ui| {
            for (i, t) in titulos.iter().enumerate() {
                let f = fases.and_then(|a| a.iter().find(|x| x["fase"].as_u64() == Some(i as u64 + 1)));
                ui.label(*t);
                match f {
                    Some(x) => {
                        // El total sale del catálogo: una fase bloqueada no ejecuta sus pruebas, pero las tiene
                        let pr = self.total_fase(i as u64 + 1).unwrap_or_else(|| x["pruebas"].as_array().map(|p| p.len() as u64).unwrap_or(0));
                        let bien = x["pruebas"].as_array().map(|p| p.iter().filter(|q| q["ok"].as_bool() == Some(true)).count()).unwrap_or(0);
                        if x["ok"].as_bool() == Some(true) {
                            tema::estado(ui, tema::Nivel::Ok, "Superada");
                        } else if x["bloqueada"].as_bool() == Some(true) {
                            tema::estado(ui, tema::Nivel::Alerta, "Bloqueada");
                        } else {
                            tema::estado(ui, tema::Nivel::Falla, "Con fallos");
                        }
                        ui.label(egui::RichText::new(format!("{bien}/{pr}")).font(tema::fuente_mono(12.0)).color(tema::p().texto_2));
                    }
                    None => {
                        ui.label(egui::RichText::new("Sin ejecutar").color(tema::p().texto_2));
                        ui.label(egui::RichText::new(self.total_fase(i as u64 + 1).map(|n| format!("—/{n}")).unwrap_or_default()).font(tema::fuente_mono(12.0)).color(tema::p().texto_2));
                    }
                }
                ui.end_row();
            }
        });
        if !self.cuando.is_empty() {
            ui.label(egui::RichText::new(format!("Última ejecución: {}", self.cuando)).small().color(tema::p().texto_2));
        }
        if let Some(h) = self.procedencia["sha256"].as_str() {
            let l = &self.procedencia["librerias"];
            ui.label(
                egui::RichText::new(format!(
                    "Banco {} · motor {} · numpy {} · scipy {}",
                    &h[..h.len().min(12)],
                    self.procedencia["version"].as_str().unwrap_or("?"),
                    l["numpy"].as_str().unwrap_or("?"),
                    l["scipy"].as_str().unwrap_or("?")
                ))
                .font(tema::fuente_mono(11.0))
                .color(tema::p().texto_3),
            )
            .on_hover_text(h);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, motor: &mut Motor) {
        egui::TopBottomPanel::top("barra_val").frame(tema::marco_panel()).show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Validación por fases").size(19.0).strong().color(tema::p().texto));
                if self.id.is_some() {
                    if ui.button("■ Detener").clicked() {
                        motor.llamar("cancelar", json!({}));
                    }
                    ui.add(egui::ProgressBar::new(self.avance).desired_width(220.0).show_percentage());
                    ui.label(egui::RichText::new(&self.texto).small().color(tema::p().texto_2));
                } else if tema::boton_principal(ui, "▶ Ejecutar todas las pruebas").clicked() {
                    self.id = Some(motor.llamar("validar", json!({"hasta_fase": 4})));
                    self.avance = 0.0;
                }
                if !self.cuando.is_empty() {
                    ui.label(egui::RichText::new(format!("Última ejecución: {}", self.cuando)).small().color(tema::p().texto_2));
                }
            });
            ui.label(egui::RichText::new("Una fase no se ejecuta hasta que la anterior pasa todas sus pruebas. Tarda alrededor de dos minutos.").small().color(tema::p().texto_2));
        });
        egui::CentralPanel::default().frame(tema::marco_fondo()).show_inside(ui, |ui| {
            if !self.error.is_empty() {
                tema::estado(ui, tema::Nivel::Falla, &self.error);
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let Some(fases) = self.resultado.as_array() else {
                    ui.add_space(60.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("Pulsa «Ejecutar todas las pruebas» para comprobar el motor.").size(15.0).color(tema::p().texto_2));
                    });
                    return;
                };
                for f in fases {
                    let ok = f["ok"].as_bool() == Some(true);
                    let bloqueada = f["bloqueada"].as_bool() == Some(true);
                    let (nivel, estado) = if ok {
                        (tema::Nivel::Ok, "Superada")
                    } else if bloqueada {
                        (tema::Nivel::Alerta, "Bloqueada")
                    } else {
                        (tema::Nivel::Falla, "Con fallos")
                    };
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(f["titulo"].as_str().unwrap_or("")).size(16.0).family(tema::semi()));
                        tema::estado(ui, nivel, estado);
                    });
                    if let Some(m) = f["motivo"].as_str() {
                        ui.label(egui::RichText::new(m).color(tema::p().texto_2));
                    }
                    for p in f["pruebas"].as_array().unwrap_or(&vec![]) {
                        let pok = p["ok"].as_bool() == Some(true);
                        let cab = format!("{} {}", if pok { "✔" } else { "✘" }, p["nombre"].as_str().unwrap_or(""));
                        egui::CollapsingHeader::new(egui::RichText::new(cab).color(if pok { tema::p().texto } else { tema::p().falla })).id_salt(p["nombre"].as_str().unwrap_or("")).default_open(!pok).show(ui, |ui| {
                            if let Some(r) = p["referencia"].as_str() {
                                if !r.is_empty() {
                                    ui.label(egui::RichText::new(format!("Referencia: {r}")).color(tema::p().acento_2));
                                }
                            }
                            if let Some(d) = p["doc"].as_str() {
                                if !d.is_empty() {
                                    ui.label(egui::RichText::new(d).small().color(tema::p().texto_2));
                                }
                            }
                            egui::Grid::new(("g_val", p["nombre"].as_str().unwrap_or(""))).striped(true).num_columns(5).spacing([16.0, 3.0]).show(ui, |ui| {
                                for t in ["Comprobación", "Valor", "Referencia", "Error", "Tolerancia"] {
                                    ui.label(egui::RichText::new(t).small().strong().color(tema::p().texto));
                                }
                                ui.end_row();
                                for m in p["medidas"].as_array().unwrap_or(&vec![]) {
                                    let mok = m["ok"].as_bool() == Some(true);
                                    let num = |k: &str| crate::grafica::numero(m[k].as_f64().unwrap_or(f64::NAN));
                                    let es_bool = m["tolerancia"].as_f64() == Some(0.0) && m["referencia"].as_f64() == Some(1.0);
                                    ui.label(egui::RichText::new(m["nombre"].as_str().unwrap_or("")).color(if mok { tema::p().texto } else { tema::p().falla }));
                                    if es_bool {
                                        tema::estado(ui, if mok { tema::Nivel::Ok } else { tema::Nivel::Falla }, if mok { "sí" } else { "no" });
                                        ui.label("sí");
                                        ui.label("—");
                                        ui.label("—");
                                    } else {
                                        let relativo = m["relativo"].as_bool() == Some(true);
                                        let unidad = m["unidad"].as_str().unwrap_or("");
                                        ui.label(egui::RichText::new(format!("{} {}", num("valor"), if relativo { "" } else { unidad })).monospace());
                                        ui.label(egui::RichText::new(num("referencia")).monospace());
                                        let err = m["error"].as_f64().unwrap_or(f64::NAN);
                                        let tol = m["tolerancia"].as_f64().unwrap_or(0.0);
                                        let (te, tt) = if relativo {
                                            (format!("{:.3} %", err * 100.0), format!("{:.2} %", tol * 100.0))
                                        } else {
                                            (format!("{err:.3e}"), format!("{tol:.1e}"))
                                        };
                                        ui.label(egui::RichText::new(te).monospace().strong().color(if mok { tema::p().ok } else { tema::p().falla }));
                                        ui.label(egui::RichText::new(tt).monospace().color(tema::p().texto_2));
                                    }
                                    ui.end_row();
                                }
                            });
                            if let Some(ex) = p["excepcion"].as_str() {
                                ui.label(egui::RichText::new(ex).monospace().small().color(tema::p().falla));
                            }
                        });
                    }
                }
            });
        });
    }
}
