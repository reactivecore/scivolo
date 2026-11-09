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

/// Compiles tests/jvm/`name` and runs it on `java -Xverify:all` in the en-US locale when that is
/// on the PATH.
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
    let locale = ["-Duser.language=en", "-Duser.country=US"];
    let run = Command::new("java")
        .args(["-Xverify:all"])
        .args(locale)
        .args(["-cp", &classpath, main])
        .output()
        .expect("running java");
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
fn classes_run_on_the_jvm() {
    runs_on_the_jvm("classes", "Classes");
}

#[test]
fn traits_run_on_the_jvm() {
    runs_on_the_jvm("traits", "Traits");
}

#[test]
fn closures_run_on_the_jvm() {
    runs_on_the_jvm("closures", "Closures");
}

#[test]
fn concatenations_run_on_the_jvm() {
    runs_on_the_jvm("concat", "Concat");
}

#[test]
fn trycatch_runs_on_the_jvm() {
    runs_on_the_jvm("trycatch", "Trycatch");
}

#[test]
fn matches_run_on_the_jvm() {
    runs_on_the_jvm("matches", "Matches");
}

#[test]
fn manylambdas_run_on_the_jvm() {
    runs_on_the_jvm("manylambdas", "Manylambdas");
}

#[test]
fn lazyvals_run_on_the_jvm() {
    runs_on_the_jvm("lazyvals", "Lazyvals");
}

#[test]
fn statics_run_on_the_jvm() {
    runs_on_the_jvm("statics", "Statics");
}

#[test]
fn javaenums_run_on_the_jvm() {
    runs_on_the_jvm("javaenums", "Javaenums");
}

#[test]
fn jdkstatics_run_on_the_jvm() {
    runs_on_the_jvm("jdkstatics", "Jdkstatics");
}

#[test]
fn attributes_run_on_the_jvm() {
    runs_on_the_jvm("attributes", "Attributes");
}

#[test]
fn streams_run_on_the_jvm() {
    runs_on_the_jvm("streams", "Streams");
}

#[test]
fn super_accessors_run_on_the_jvm() {
    runs_on_the_jvm("superaccessors", "Superaccessors");
}

#[test]
fn jdk_classes_javalib_lacks_run_on_the_jvm() {
    runs_on_the_jvm("jdkclasses", "Jdkclasses");
}

#[test]
fn jumps_over_32_kib_run_on_the_jvm() {
    runs_on_the_jvm("widejumps", "Widejumps");
}

#[test]
fn a_macro_call_runs_over_the_macro_classpath() {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let jar = root().join("tests/macros/lib/macros.jar");
    let source = root().join("tmp/jvm-test/MacroCall.scala");
    std::fs::create_dir_all(source.parent().expect("a directory")).expect("tmp/jvm-test");
    let code = "object MacroCall:\n  def main(args: Array[String]) = println(macrolib.Macros.twice(3))\n";
    std::fs::write(&source, code).expect("writing");
    let out = root().join("tmp/jvm-test/macrocall");
    let compile = |macro_cp: &[&Path]| {
        let _ = std::fs::remove_dir_all(&out);
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"));
        cmd.arg("--cp").arg(&jar);
        macro_cp.iter().for_each(|p| _ = cmd.arg("--macro-cp").arg(p));
        cmd.arg("-d").arg(&out).arg(&source).output().expect("running scivolo-jvm")
    };
    let refused = compile(&[]);
    assert!(String::from_utf8_lossy(&refused.stderr).contains("macro call"));
    let compiled = compile(&[&jar]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}:{}", out.display(), jar.display(), library.display());
    let run = Command::new("java").args(["-Xverify:all", "-cp", &classpath, "MacroCall"]).output();
    assert_eq!(String::from_utf8_lossy(&run.expect("running java").stdout), "6\n");
}

/// Compiles `code` as tmp/jvm-test/`name`.scala and expects a refusal containing `message`.
fn refuses(name: &str, code: &str, message: &str) {
    let source = root().join(format!("tmp/jvm-test/{name}.scala"));
    std::fs::create_dir_all(source.parent().expect("a directory")).expect("tmp/jvm-test");
    std::fs::write(&source, code).expect("writing");
    let (_, compiled) = compile(name, &source);
    assert!(!compiled.status.success());
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    assert!(stderr.contains(message), "{stderr}");
}

#[test]
fn a_definition_the_tasty_pickler_lacks_is_refused() {
    let code = "trait A { def f(using x: Int): Int = x }\n\
        trait B extends A { override def f(using x: Int): Int = super.f + 1 }";
    refuses("refusedtasty", code, "pickling a super call of a method with implicit parameters into TASTy");
}

#[test]
fn a_javalib_value_the_jdk_lacks_is_refused() {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let code = "object R { def f = java.lang.ProcessBuilder.nullFile }";
    refuses("refusedjdk", code, "ProcessBuilder.nullFile, a static member the JDK lacks");
}

#[test]
fn a_class_nested_in_a_jdk_class_javalib_lacks_is_refused() {
    let code = "object R { def f = java.time.zone.ZoneOffsetTransitionRule.TimeDefinition.UTC }";
    refuses("refusednested", code, "member not found");
}

#[test]
fn a_signature_polymorphic_method_is_refused() {
    let code = "object R { def f = java.lang.invoke.MethodHandles.identity(classOf[String]).invoke(\"a\") }";
    refuses("refusedpolymorphic", code, "member not found");
}
