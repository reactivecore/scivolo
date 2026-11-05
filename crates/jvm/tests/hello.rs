use std::path::Path;
use std::process::Command;

/// Compiles tests/jvm/hello and runs it on `java` when that is on the PATH.
#[test]
fn hello_runs_on_the_jvm() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let out = root.join("tmp/jvm-test/hello");
    let _ = std::fs::remove_dir_all(&out);
    let source = root.join("tests/jvm/hello/hello.scala");
    let status = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm")).arg("-d").arg(&out).arg(&source).status();
    assert!(status.expect("running scivolo-jvm").success());
    let classpath = format!("{}:{}", out.display(), root.join("ref/libs/scala-library-3.9.0.jar").display());
    let run = Command::new("java")
        .args(["-Xverify:all", "-cp", &classpath, "Hello"])
        .output()
        .expect("running java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let check = std::fs::read_to_string(root.join("tests/jvm/hello/hello.check")).expect("hello.check");
    assert_eq!(String::from_utf8_lossy(&run.stdout), check);
}
