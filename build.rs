fn main() {
    let lib_path = std::env::var("JARVIS_VOSK_PATH")
        .unwrap_or_else(|_| "libs".to_string());

    println!("cargo:rustc-link-search={}", lib_path);
    println!("cargo:rerun-if-changed={}", lib_path);
    println!("cargo:rerun-if-changed=build.rs");
}
