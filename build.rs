//! Incrusta `icon/heliora.ico` en el ejecutable de Windows (lo que muestran el Explorador y la barra
//! de tareas al anclar la app). Usa el compilador de recursos `rc.exe` del Windows SDK; si no lo
//! encuentra, avisa y compila sin icono. En el resto de sistemas no hace nada.
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=icon/heliora.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let Some(rc) = buscar_rc() else {
        println!("cargo:warning=No se encontró rc.exe del Windows SDK: el ejecutable se compila sin icono");
        return;
    };
    let salida = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR lo fija cargo"));
    let icono = Path::new(env!("CARGO_MANIFEST_DIR")).join("icon").join("heliora.ico");
    let guion = salida.join("heliora.rc");
    let recurso = salida.join("heliora.res");
    let ruta = icono.display().to_string().replace('\\', "\\\\");
    std::fs::write(&guion, format!("1 ICON \"{ruta}\"\n")).expect("no se pudo escribir heliora.rc");
    let estado = Command::new(&rc).arg("/nologo").arg("/fo").arg(&recurso).arg(&guion).status();
    match estado {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", recurso.display()),
        Ok(s) => println!("cargo:warning=rc.exe terminó con {s}: el ejecutable se compila sin icono"),
        Err(e) => println!("cargo:warning=No se pudo ejecutar rc.exe ({e}): el ejecutable se compila sin icono"),
    }
}

/// `rc.exe`: primero la variable RC; después la versión más reciente del Windows SDK para x64.
fn buscar_rc() -> Option<PathBuf> {
    if let Some(r) = std::env::var_os("RC").map(PathBuf::from).filter(|r| r.exists()) {
        return Some(r);
    }
    let base = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versiones: Vec<PathBuf> = std::fs::read_dir(base).ok()?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join("x64").join("rc.exe").exists()).collect();
    versiones.sort();
    versiones.pop().map(|v| v.join("x64").join("rc.exe"))
}
