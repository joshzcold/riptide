fn main() {
    // On Linux, libcef.so and its resources are copied next to the binary by
    // cef-dll-sys; this lets the binary find them without LD_LIBRARY_PATH.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
}
