use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(gst_videorate_fix)");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        build_gst_videorate_fix();
    }
    tauri_build::try_build(tauri_build::Attributes::new()).expect("failed to run tauri-build");
}

/// Linux: zkompiluje GStreamer tracer `gst/dpivideoratefix.c` (obejití zamrzání
/// kamery ve WebKitGTK, viz zdroják) do sdílené knihovny, kterou main.rs vloží
/// do binárky. Bez vývojových hlaviček GStreameru se obejití vynechá.
fn build_gst_videorate_fix() {
    let src = "gst/dpivideoratefix.c";
    println!("cargo:rerun-if-changed={src}");

    let flags = match Command::new("pkg-config")
        .args(["--cflags", "--libs", "gstreamer-1.0"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => {
            println!("cargo:warning=gstreamer-1.0 (pkg-config) nenalezen — obejití zamrzání kamery se nezabuduje");
            return;
        }
    };

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("libgstdpivideoratefix.so");
    let compiler = cc::Build::new().get_compiler();
    let status = compiler
        .to_command()
        .args(["-shared", "-fPIC", "-O2", "-o"])
        .arg(&out)
        .arg(src)
        .args(flags.split_whitespace())
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-cfg=gst_videorate_fix"),
        _ => println!("cargo:warning=kompilace {src} selhala — obejití zamrzání kamery se nezabuduje"),
    }
}
