use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/app/cat-home.ico");
    println!("cargo:rerun-if-changed=assets/app/cat-home.rc");
    println!("cargo:rerun-if-env-changed=WINDRES");
    println!("cargo:rerun-if-env-changed=RC");
    if !env::var("CARGO_CFG_TARGET_OS").is_ok_and(|v| v == "windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let gnu = env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|v| v == "gnu");
    let object = out.join(if gnu {
        "cat-home-res.o"
    } else {
        "cat-home.res"
    });
    let status = if gnu {
        Command::new(env::var_os("WINDRES").unwrap_or_else(|| "windres".into()))
            .args([
                "--input",
                "assets/app/cat-home.rc",
                "--output-format",
                "coff",
                "--output",
            ])
            .arg(&object)
            .status()
            .expect("windres is required to embed the Windows icon")
    } else {
        Command::new(env::var_os("RC").unwrap_or_else(|| "rc".into()))
            .args(["/nologo", "/fo"])
            .arg(&object)
            .arg("assets/app/cat-home.rc")
            .status()
            .expect("Windows SDK rc is required")
    };
    assert!(status.success(), "Windows icon resource compilation failed");
    println!("cargo:rustc-link-arg={}", object.display());
}
