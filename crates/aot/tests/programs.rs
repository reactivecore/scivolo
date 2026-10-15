use std::path::{Path, PathBuf};
use std::process::Command;

fn program(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/programs").join(name)
}

/// What the executable built from method `entry` of test program `name` prints for `arg`.
fn run(name: &str, entry: &str, arg: &str) -> String {
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot").join(name);
    let out = Command::new(env!("CARGO_BIN_EXE_scalai-aot"))
        .arg("--cp")
        .arg(program(name))
        .args(["--entry", entry, "-o"])
        .arg(&bin)
        .output()
        .expect("running scalai-aot");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = Command::new(&bin).arg(arg).output().expect("running the executable");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// What the executable built from test program `name`, main class `name`, prints.
fn run_main(name: &str) -> String {
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot-main").join(name);
    let out = Command::new(env!("CARGO_BIN_EXE_scalai-aot"))
        .arg("--cp")
        .arg(program(name))
        .arg("-o")
        .arg(&bin)
        .arg(name)
        .output()
        .expect("running scalai-aot");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = Command::new(&bin).output().expect("running the executable");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn hello_and_fib_run_through_their_main() {
    assert_eq!(run_main("hello"), check("hello"));
    assert_eq!(run_main("fib"), check("fib"));
}

fn check(name: &str) -> String {
    std::fs::read_to_string(program(name).join(format!("{name}.check"))).expect("the .check file")
}

#[test]
fn fib_runs_as_executable() {
    assert_eq!(run("fib", "fibo", "25"), "75025\n");
}

#[test]
fn negative_literals_and_overflowing_division() {
    assert_eq!(run("intops", "calc", "20"), check("intops"));
}

#[test]
fn virtual_and_trait_dispatch_type_tests_and_modules() {
    assert_eq!(run("dispatch", "run", "20"), check("dispatch"));
}
