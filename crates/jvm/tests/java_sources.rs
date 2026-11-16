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
    runs_as_checked("mixed", "mixed.javasources", "javasources.check");
}

/// Java text blocks with Java's whitespace, meta-annotations through type aliases, and members of
/// package-private Java classes through public subclasses.
#[test]
fn java_text_blocks_alias_meta_annotations_and_package_private_receivers_run() {
    runs_as_checked("access", "Access", "access.check");
}

/// Compiles tests/jvm/javasources/`dir`, runs `main` on `java -Xverify:all` and compares its
/// output with tests/jvm/javasources/`check`.
fn runs_as_checked(dir: &str, main: &str, check: &str) {
    let Some((mut scivolo, out)) = scivolo(dir) else { return };
    run(&mut scivolo);
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}", out.display(), library.display());
    let stdout = run(Command::new("java").args(["-Xverify:all", "-cp", &classpath, main]));
    let check = std::fs::read_to_string(root().join("tests/jvm/javasources").join(check)).expect("the check");
    assert_eq!(stdout, check);
}

/// The compilation group of test source `path` as dotty's run tests number them: the number after
/// the last `_` of its stem, else 0.
fn group(path: &Path) -> u32 {
    let stem = path.file_stem().and_then(|s| s.to_str()).expect("a file name");
    stem.rsplit_once('_').and_then(|(_, n)| n.parse().ok()).unwrap_or(0)
}

/// The run tests under tests/jvm/javasources/runs, dotty's tests/run with Java sources among them,
/// compile and their `Test` prints its `<name>.check`, or nothing: earlier groups, Java only, by
/// javac, the last by scivolo-jvm.
#[test]
fn run_tests_with_java_sources_print_their_check() {
    if ["java", "javac"].into_iter().any(|tool| Command::new(tool).arg("--version").output().is_err()) {
        eprintln!("no java or javac on the PATH: skipped");
        return;
    }
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let tests = root().join("tests/jvm/javasources/runs");
    let mut names: Vec<String> = std::fs::read_dir(&tests)
        .expect("the tests")
        .map(|e| e.expect("an entry").file_name().into_string().expect("a name"))
        .collect();
    names.sort();
    for name in names {
        let out = root().join("tmp/jvm-test/javasources-runs").join(&name);
        let _ = std::fs::remove_dir_all(&out);
        let mut files: Vec<PathBuf> = std::fs::read_dir(tests.join(&name))
            .expect("the sources")
            .map(|e| e.expect("an entry").path())
            .collect();
        files.retain(|f| f.extension().is_some_and(|e| e == "java" || e == "scala"));
        files.sort();
        let last = files.iter().map(|f| group(f)).max().expect("sources");
        let (earlier, last): (Vec<PathBuf>, Vec<PathBuf>) = files.into_iter().partition(|f| group(f) < last);

        let classes = out.join("classes");
        let earlier_classes = out.join("earlier");
        let mut scivolo = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"));
        if !earlier.is_empty() {
            run(Command::new("javac").arg("-d").arg(&earlier_classes).args(&earlier));
            scivolo.arg("--cp").arg(&earlier_classes);
        }
        run(scivolo.arg("-d").arg(&classes).args(&last));

        let classpath = format!("{}:{}:{}", classes.display(), earlier_classes.display(), library.display());
        let stdout = run(Command::new("java").args(["-Xverify:all", "-cp", &classpath, "Test"]));
        let check =
            std::fs::read_to_string(tests.join(&name).join(format!("{name}.check"))).unwrap_or_default();
        assert_eq!(stdout, check, "{name}");
    }
}

/// The varargs forwarders of methods with repeated parameters bounded by `Int` and `Double` wrap
/// their arrays as dotty's do, by `wrapIntArray` and `wrapDoubleArray`.
#[test]
fn varargs_forwarders_wrap_primitive_bounds_by_their_primitive() {
    let out = root().join("tmp/jvm-test/varargs-bounds");
    let _ = std::fs::remove_dir_all(&out);
    let source = root().join("tests/jvm/javasources/runs/varargs-bounds/bounds.scala");
    run(Command::new(env!("CARGO_BIN_EXE_scivolo-jvm")).arg("-d").arg(&out).arg(source));
    let class = std::fs::read(out.join("Bounds.class")).expect("Bounds.class");
    for wrap in ["wrapIntArray", "wrapDoubleArray"] {
        assert!(class.windows(wrap.len()).any(|w| w == wrap.as_bytes()), "{wrap} in Bounds.class");
    }
}

#[test]
fn a_private_constructor_is_refused_as_inaccessible() {
    let Some((mut scivolo, _)) = scivolo("private") else { return };
    let refused = output(&mut scivolo);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success() && stderr.contains("inaccessible member"), "{stderr}");
}

/// A Java abstract class whose only constructor is private is no SAM type: a lambda is not
/// converted to it.
#[test]
fn a_private_constructor_makes_no_sam_class() {
    let Some((mut scivolo, _)) = scivolo("privatesam") else { return };
    let refused = output(&mut scivolo);
    assert!(!refused.status.success(), "{}", String::from_utf8_lossy(&refused.stdout));
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
