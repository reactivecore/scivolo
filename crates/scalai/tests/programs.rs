use std::process::Command;

fn run(classes: &str, main: &str) -> String {
    let dir = format!("{}/../../{classes}", env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(env!("CARGO_BIN_EXE_scalai")).args(["--cp", &dir, main]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn hello() {
    assert_eq!(run("crates/nir/tests/fixtures", "hello"), "Hello, world!\n");
}
