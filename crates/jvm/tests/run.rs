use std::path::Path;
use std::process::Command;

/// Compiles each of tests/jvm and runs it on `java` when that is on the PATH.
#[test]
fn tests_run_on_the_jvm() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    for (name, main) in [("hello", "Hello"), ("arith", "Arith"), ("loops", "Loops")] {
        let out = root.join("tmp/jvm-test").join(name);
        let _ = std::fs::remove_dir_all(&out);
        let source = root.join(format!("tests/jvm/{name}/{name}.scala"));
        let status =
            Command::new(env!("CARGO_BIN_EXE_scivolo-jvm")).arg("-d").arg(&out).arg(&source).status();
        assert!(status.expect("running scivolo-jvm").success(), "{name}");
        let classpath =
            format!("{}:{}", out.display(), root.join("ref/libs/scala-library-3.9.0.jar").display());
        let run = Command::new("java")
            .args(["-Xverify:all", "-cp", &classpath, main])
            .output()
            .expect("running java");
        assert!(run.status.success(), "{name}: {}", String::from_utf8_lossy(&run.stderr));
        let check = root.join(format!("tests/jvm/{name}/{name}.check"));
        let check = std::fs::read_to_string(check).expect("a .check file");
        assert_eq!(String::from_utf8_lossy(&run.stdout), check, "{name}");
    }
}
