//! Gives the wasm32 module a shadow stack for the interpreter's frame budget and the typer.

/// Bytes of the module's stack: `vm::PORTABLE_STACK` (8 MiB) and as much again for the typer.
const STACK: usize = 16 << 20;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32") {
        println!("cargo::rustc-link-arg-cdylib=-zstack-size={STACK}");
    }
}
