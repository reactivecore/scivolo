//! tests/jvm/javasources: Scala and Java sources compiled in one scivolo-jvm run, the Java ones
//! by javac against the Scala classes, and run on `java -Xverify:all`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn output(command: &mut Command) -> Output {
    command.output().unwrap_or_else(|e| panic!("{command:?}: {e}"))
}

fn run(command: &mut Command) -> String {
    let out = output(command);
    assert!(out.status.success(), "{command:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// scivolo-jvm compiling the files of tests/jvm/javasources/`dir` into tmp/jvm-test/javasources-`dir`,
/// returned with the command; none if java or javac is missing.
fn scivolo(dir: &str) -> Option<(Command, PathBuf)> {
    if ["java", "javac"].into_iter().any(|tool| Command::new(tool).arg("--version").output().is_err()) {
        eprintln!("no java or javac on the PATH: skipped");
        return None;
    }
    let out = root().join("tmp/jvm-test").join(format!("javasources-{dir}"));
    let _ = std::fs::remove_dir_all(&out);
    let sources = root().join("tests/jvm/javasources").join(dir);
    let mut files: Vec<PathBuf> =
        std::fs::read_dir(&sources).expect("the sources").map(|e| e.expect("an entry").path()).collect();
    files.sort();
    let mut scivolo = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"));
    scivolo.arg("-d").arg(&out).args(files);
    Some((scivolo, out))
}

#[test]
fn scala_and_java_sources_that_use_each_other_compile_and_run() {
    let Some((mut scivolo, out)) = scivolo("mixed") else { return };
    run(&mut scivolo);
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}", out.display(), library.display());
    let stdout = run(Command::new("java").args(["-Xverify:all", "-cp", &classpath, "mixed.javasources"]));
    let check =
        std::fs::read_to_string(root().join("tests/jvm/javasources/javasources.check")).expect("the check");
    assert_eq!(stdout, check);
}

#[test]
fn a_private_constructor_is_refused_as_inaccessible() {
    let Some((mut scivolo, _)) = scivolo("private") else { return };
    let refused = output(&mut scivolo);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success() && stderr.contains("inaccessible member"), "{stderr}");
}

#[test]
fn a_receiver_parameter_is_refused_by_name() {
    let Some((mut scivolo, _)) = scivolo("refused") else { return };
    let refused = output(&mut scivolo);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success() && stderr.contains("does not support a receiver parameter"),
        "{stderr}"
    );
}
