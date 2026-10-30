fn main() {
    let manifest = "src/native/ao3reader-native.manifest";
    println!("cargo:rerun-if-changed={manifest}");

    // Embed the app manifest (per-monitor DPI v2, Common Controls v6, UTF-8) into the native
    // binary through the MSVC linker, so no resource compiler is needed.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let path = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(manifest);
        println!("cargo:rustc-link-arg-bin=ao3reader-native=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bin=ao3reader-native=/MANIFESTINPUT:{}", path.display());
    }
}
