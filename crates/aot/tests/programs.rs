use std::path::Path;
use std::process::Command;

/// What the executable built from method `entry` of test program `name` prints for `arg`.
fn run(name: &str, entry: &str, arg: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot").join(name);
    let out = Command::new(env!("CARGO_BIN_EXE_scalai-aot"))
        .arg("--cp")
        .arg(root.join("tests/programs").join(name))
        .args(["--entry", entry, "-o"])
        .arg(&bin)
        .output()
        .expect("running scalai-aot");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = Command::new(&bin).arg(arg).output().expect("running the executable");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn fib_runs_as_executable() {
    assert_eq!(run("fib", "fibo", "25"), "75025\n");
}

#[test]
fn negative_literals_and_overflowing_division() {
    let check = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/programs/intops/intops.check"),
    );
    assert_eq!(run("intops", "calc", "20"), check.expect("intops.check"));
}
