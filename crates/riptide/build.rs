fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    // On Linux, libcef.so and its resources are copied next to the binary by
    // cef-dll-sys; this lets the binary find them without LD_LIBRARY_PATH.
    if os == "linux" {
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
    // CEF on Windows expects the app's manifest inside the exe.
    if os == "windows" && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("riptide.exe.manifest");
        println!("cargo::rerun-if-changed={}", manifest.display());
        println!("cargo::rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo::rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
    }
}
