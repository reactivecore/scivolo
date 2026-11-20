use std::process::Command;

#[test]
fn version_option_prints_the_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_scivolo")).arg("--version").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("scivolo {}\n", version::VERSION));
}
