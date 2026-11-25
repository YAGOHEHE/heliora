//! Heliora: cálculo técnico y simulación por bloques para plantas solares fotovoltaicas.
//! La ventana está escrita en Rust con egui; el cálculo lo hace un motor en Python.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bloques;
mod calculo;
mod comandos;
mod grafica;
mod herramientas;
mod motor;
mod plan;
mod tema;
mod validacion;

fn main() -> eframe::Result<()> {
    let mut op = app::Opciones { captura: None, modulo: None, ejemplo: None, simular: false, guion: false, herramientas: vec![], valores: vec![], pestana: None, modo: None, linealizar: false };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => op.captura = args.next().map(std::path::PathBuf::from),
            "--modulo" => op.modulo = args.next(),
            "--ejemplo" => op.ejemplo = args.next(),
            "--simular" => op.simular = true,
            "--guion" => op.guion = true,
            "--pestana" => op.pestana = args.next(),
            "--modo" => op.modo = args.next(),
            "--linealizar" => op.linealizar = true,
            "--herramientas" => op.herramientas = args.next().unwrap_or_default().split(',').map(String::from).collect(),
            "--valor" => {
                if let Some((k, v)) = args.next().as_deref().and_then(|a| a.split_once('=')) {
                    op.valores.push((k.to_string(), v.to_string()));
                }
            }
            _ => {}
        }
    }
    let opciones = eframe::NativeOptions {
        viewport: {
            let v = egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]).with_min_inner_size([900.0, 600.0]).with_title("Heliora");
            match eframe::icon_data::from_png_bytes(include_bytes!("../icon/icon.png")) {
                Ok(i) => v.with_icon(i),
                Err(_) => v,
            }
        },
        ..Default::default()
    };
    eframe::run_native("Heliora", opciones, Box::new(move |cc| Ok(Box::new(app::App::nueva(cc, op)))))
}
