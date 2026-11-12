//! tests/jvm/javajar compiled against a jar of the Java classes javac compiles from
//! tests/jvm/javajar/shapes, and run on `java -Xverify:all`.

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

/// Compiles tests/jvm/javajar/shapes into the jar `shapes.jar` of tmp/jvm-test/`dir`, which it
/// returns; none if the JDK's tools are missing.
fn shapes_jar(dir: &str) -> Option<PathBuf> {
    let missing = |tool: &str| Command::new(tool).arg("--version").output().is_err();
    if ["java", "javac", "jar"].into_iter().any(missing) {
        eprintln!("no java, javac or jar on the PATH: skipped");
        return None;
    }
    let out = root().join("tmp/jvm-test").join(dir);
    let _ = std::fs::remove_dir_all(&out);
    let (classes, jar) = (out.join("java"), out.join("shapes.jar"));
    let source = root().join("tests/jvm/javajar/shapes/Box.java");
    run(Command::new("javac").args(["--release", "17", "-d"]).arg(&classes).arg(source));
    run(Command::new("jar").arg("cf").arg(&jar).arg("-C").arg(&classes).arg("."));
    Some(jar)
}

/// scivolo-jvm compiling tests/jvm/javajar/`name`.scala against `jar` into `out`.
fn scivolo(jar: &Path, out: &Path, name: &str) -> Command {
    let mut scivolo = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"));
    let source = root().join("tests/jvm/javajar").join(format!("{name}.scala"));
    scivolo.arg("--cp").arg(jar).arg("-d").arg(out).arg(source);
    scivolo
}

#[test]
fn a_java_jar_on_the_classpath_is_used() {
    let Some(jar) = shapes_jar("javajar") else { return };
    let compiled = jar.with_file_name("scala");
    run(&mut scivolo(&jar, &compiled, "javajar"));
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}:{}", compiled.display(), jar.display(), library.display());
    let stdout = run(Command::new("java").args(["-Xverify:all", "-cp", &classpath, "shapes.Javajar"]));
    let check =
        std::fs::read_to_string(root().join("tests/jvm/javajar/javajar.check")).expect("javajar.check");
    assert_eq!(stdout, check);
}

#[test]
fn protected_classes_outside_their_package_and_native_classes_are_refused() {
    let Some(jar) = shapes_jar("javajar-refused") else { return };
    for name in ["hidden", "native"] {
        let out = output(&mut scivolo(&jar, &jar.with_file_name(name), name));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && stderr.contains("does not support"), "{name}: {stderr}");
    }
}
