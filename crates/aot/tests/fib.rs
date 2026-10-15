use std::path::Path;
use std::process::Command;

#[test]
fn fib_runs_as_executable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot/fib");
    let status = Command::new(env!("CARGO_BIN_EXE_scalai-aot"))
        .arg("--cp")
        .arg(root.join("tests/programs/fib"))
        .args(["--entry", "fibo", "-o"])
        .arg(&bin)
        .status()
        .expect("running scalai-aot");
    assert!(status.success());
    let out = Command::new(&bin).arg("25").output().expect("running the executable");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "75025\n");
}
