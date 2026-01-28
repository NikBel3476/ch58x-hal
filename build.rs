use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    fs::write(out_dir.join("libISP592.a"), include_bytes!("vendor/libISP592.a")).unwrap();

    if env::var("CARGO_FEATURE_BLE").is_ok() {
        fs::write(out_dir.join("libCH59xBLE.a"), include_bytes!("vendor/LIBCH59xBLE.a")).unwrap();
        println!("cargo:rustc-link-lib=CH59xBLE");
    }

    fs::write(out_dir.join("memory.x"), include_bytes!("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out_dir.display());

    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");
}
