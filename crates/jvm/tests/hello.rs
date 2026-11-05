use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Runs scivolo-jvm on `source` into tmp/jvm-test/`name`.
fn compile(name: &str, source: &Path) -> (PathBuf, Output) {
    let out = root().join("tmp/jvm-test").join(name);
    let _ = std::fs::remove_dir_all(&out);
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm")).arg("-d").arg(&out).arg(source).output();
    (out, run.expect("running scivolo-jvm"))
}

/// Compiles tests/jvm/`name` and runs it on `java -Xverify:all` when that is on the PATH.
fn runs_on_the_jvm(name: &str, main: &str) {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let dir = root().join("tests/jvm").join(name);
    let (out, compiled) = compile(name, &dir.join(format!("{name}.scala")));
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let classpath =
        format!("{}:{}", out.display(), root().join("ref/libs/scala-library-3.9.0.jar").display());
    let run =
        Command::new("java").args(["-Xverify:all", "-cp", &classpath, main]).output().expect("running java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let check = std::fs::read_to_string(dir.join(format!("{name}.check"))).expect("a .check file");
    assert_eq!(String::from_utf8_lossy(&run.stdout), check);
}

#[test]
fn hello_runs_on_the_jvm() {
    runs_on_the_jvm("hello", "Hello");
}

#[test]
fn objects_run_on_the_jvm() {
    runs_on_the_jvm("objects", "Objects");
}

#[test]
fn arith_runs_on_the_jvm() {
    runs_on_the_jvm("arith", "Arith");
}

#[test]
fn loops_run_on_the_jvm() {
    runs_on_the_jvm("loops", "Loops");
}

#[test]
fn frames_run_on_the_jvm() {
    runs_on_the_jvm("frames", "Frames");
}

#[test]
fn primitive_operations_are_refused() {
    let source = root().join("tmp/jvm-test/refused.scala");
    std::fs::create_dir_all(source.parent().expect("a directory")).expect("tmp/jvm-test");
    std::fs::write(&source, "object R { def f(a: String, b: String): Boolean = a == b }").expect("writing");
    let (_, compiled) = compile("refused", &source);
    assert!(!compiled.status.success());
    assert!(String::from_utf8_lossy(&compiled.stderr).contains("primitive operation =="));
}
