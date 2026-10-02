fn main() {
    // libcef.so and its resources are copied next to the binary by cef-dll-sys.
    println!("cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
}
