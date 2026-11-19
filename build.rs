//! Windows only: puts the app icon and version details (assets/windows/fermata.rc) into fermata.exe, with the
//! resource compiler of the MinGW toolchain (windres). Without windres the .exe still builds, without its icon.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows-gnu") {
        return;
    }
    println!("cargo:rerun-if-changed=assets/windows/fermata.rc");
    println!("cargo:rerun-if-changed=assets/windows/fermata.ico");
    let out = std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into());
    let obj = format!("{out}/fermata-res.o");
    // fermata.rc is a template: the version comes from Cargo.toml, so it is never typed by hand
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let commas = version.split(['.', '-']).take(3).collect::<Vec<_>>().join(",");
    let template = std::fs::read_to_string("assets/windows/fermata.rc").expect("assets/windows/fermata.rc");
    let rc = format!("{out}/fermata.rc");
    std::fs::write(&rc, template.replace("@VERSION_COMMAS@", &commas).replace("@VERSION@", &version)).expect("write fermata.rc");
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| if cfg!(windows) { "windres".into() } else { "x86_64-w64-mingw32-windres".into() });
    let ok = std::process::Command::new(&windres)
        .args(["--input-format=rc", "-O", "coff", "-i", &rc, "-o", &obj, "--include-dir", "assets/windows"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        println!("cargo:rustc-link-arg-bins={obj}");
    } else {
        println!("cargo:warning={windres} not found or failed: fermata.exe is built without its icon");
    }
}
