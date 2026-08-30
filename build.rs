use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let vosk_lib_dir = PathBuf::from(&manifest_dir).join("libs").join("vosk");

    if vosk_lib_dir.exists() {
        println!("cargo:rustc-link-search=native={}", vosk_lib_dir.display());
        println!("cargo:rustc-link-lib=dylib=vosk");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", vosk_lib_dir.display());
    }
}
