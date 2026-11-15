use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Runs scivolo-jvm on `source` into tmp/jvm-test/`name`.
fn compile(name: &str, source: &Path) -> (PathBuf, Output) {
    compile_all(name, &[source.to_owned()])
}

/// Runs scivolo-jvm on `sources` together into tmp/jvm-test/`name`.
fn compile_all(name: &str, sources: &[PathBuf]) -> (PathBuf, Output) {
    let out = root().join("tmp/jvm-test").join(name);
    let _ = std::fs::remove_dir_all(&out);
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm")).arg("-d").arg(&out).args(sources).output();
    (out, run.expect("running scivolo-jvm"))
}

/// Compiles the `.scala` files of tests/jvm/`name` in one run and runs it on `java -Xverify:all`
/// in the en-US locale when that is on the PATH.
fn runs_on_the_jvm(name: &str, main: &str) {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let dir = root().join("tests/jvm").join(name);
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("a test directory")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "scala"))
        .collect();
    sources.sort();
    let (out, compiled) = compile_all(name, &sources);
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
fn several_files_run_on_the_jvm() {
    runs_on_the_jvm("units", "Units");
}

#[test]
fn a_file_of_a_unit_compiles_as_alone() {
    let dir = root().join("tests/jvm/units");
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("a test directory")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "scala"))
        .collect();
    sources.sort();
    let (unit, compiled) = compile_all("units-unit", &sources);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let (alone, compiled) = compile("units-alone", &dir.join("words.scala"));
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let classes = std::fs::read_dir(alone.join("units")).expect("the classes of words.scala");
    for class in classes {
        let name = class.expect("a directory entry").file_name();
        let bytes = |out: &Path| std::fs::read(out.join("units").join(&name)).expect("a class file");
        assert!(bytes(&unit) == bytes(&alone), "{name:?} differs");
    }
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
fn inline_vals_run_on_the_jvm() {
    runs_on_the_jvm("inlinevals", "Inlinevals");
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
fn jdk_mixin_forwarders_run_on_the_jvm() {
    runs_on_the_jvm("jdkmixins", "Jdkmixins");
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
fn jdk_members_under_explicit_nulls_run_on_the_jvm() {
    runs_on_the_jvm("explicitnulls", "Explicitnulls");
}

#[test]
fn java_annotations_run_on_the_jvm() {
    runs_on_the_jvm("javaannotations", "Javaannotations");
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
    let code = "class tag extends scala.annotation.StaticAnnotation\n\
                trait B { def f: Int @tag = 1; def g: Int @tag = 2; def h(b: Boolean) = if b then f else g }";
    refuses("refusedtasty", code, "an annotated type of several annotation trees");
    let code = "import scala.annotation.unchecked.uncheckedVariance as uV\n\
                class C[+A](val m: Map[A @uV, A @uV])";
    refuses("refusedaccessor", code, "an annotated type of several annotation trees");
}

/// Writes `files`, each a name and its code, into tmp/jvm-test/`name`.
fn written(name: &str, files: &[(&str, &str)]) -> Vec<PathBuf> {
    let dir = root().join("tmp/jvm-test").join(name);
    std::fs::create_dir_all(&dir).expect("tmp/jvm-test");
    let write = |&(file, code): &(&str, &str)| {
        let path = dir.join(file);
        std::fs::write(&path, code).expect("writing");
        path
    };
    files.iter().map(write).collect()
}

#[test]
fn an_annotated_type_other_files_give_too_pickles_as_alone() {
    let sources = written(
        "annotatedunit",
        &[
            (
                "a.scala",
                "class tag extends scala.annotation.StaticAnnotation\ntrait A { def f: Int @tag = 1; def h = f }\n",
            ),
            ("b.scala", "trait B { def g: Int @tag = 2 }\n"),
        ],
    );
    let (unit, compiled) = compile_all("annotatedunit-unit", &sources);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let (alone, compiled) = compile("annotatedunit-alone", &sources[0]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    for name in ["A.tasty", "A.class", "tag.tasty"] {
        let bytes = |out: &Path| std::fs::read(out.join(name)).expect("an output file");
        assert!(bytes(&unit) == bytes(&alone), "{name} differs");
    }
}

#[test]
fn an_annotated_type_inferred_from_another_file_with_its_own_tree_is_refused() {
    let sources = written(
        "annotatedfrom",
        &[
            (
                "a.scala",
                "class tag(n: Int) extends scala.annotation.StaticAnnotation\nobject A { def x: Int @tag(1) = 1 }\n",
            ),
            ("b.scala", "object B { def y: Int @tag(2) = 2; def f = A.x }\n"),
        ],
    );
    let (_, compiled) = compile_all("annotatedfrom-out", &sources);
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    assert!(stderr.contains("b.scala: ") && stderr.contains("several annotation trees"), "{stderr}");
}

#[test]
fn a_definition_of_scala_unit_is_refused() {
    let code = "package scala { final abstract class Unit private extends AnyVal }\n\
                package p { class U { def t = summon[scala.reflect.ClassTag[Unit]] } }\n";
    refuses("unitdefined", code, "a definition replacing a well-known library symbol");
}

#[test]
fn any_val_of_a_unit_of_several_files_is_left_out() {
    let any_val = "package scala\nimport scala.language.`2.13`\n\
                   transparent abstract class AnyVal extends Any, Matchable {\n\
                   def getClass(): Class[? <: AnyVal] = null.asInstanceOf[Class[? <: AnyVal]]\n}\n";
    let value = "final class V(private val n: Int) extends AnyVal { def twice: Int = n * 2 }\n";
    let sources = written("anyvalunit", &[("AnyVal.scala", any_val), ("v.scala", value)]);
    let out = root().join("tmp/jvm-test/anyvalunit-out");
    let _ = std::fs::remove_dir_all(&out);
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
        .args(["--scalalib", "--leave-out-refused", "-d"])
        .arg(&out)
        .args(&sources)
        .output()
        .expect("running scivolo-jvm");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("AnyVal.scala: ") && !stderr.contains("v.scala: "), "{stderr}");
    let companion = std::fs::read(out.join("V$.class")).expect("V$.class");
    assert!(companion.windows(15).any(|w| w == b"twice$extension"), "{stderr}");
}

#[test]
fn a_tuple_class_of_the_unit_extends_its_generic_tuple() {
    let tuple2 = "package scala\nimport scala.language.`2.13`\n\
                  final case class Tuple2[+T1, +T2](_1: T1, _2: T2) extends Product2[T1, T2]\n";
    let user =
        "object Use { def t: Tuple = Tuple2(1, \"a\"); def p: Int *: String *: EmptyTuple = (1, \"a\") }\n";
    let sources = written("tupleunit", &[("Tuple2.scala", tuple2), ("use.scala", user)]);
    let unit = root().join("tmp/jvm-test/tupleunit-out");
    let _ = std::fs::remove_dir_all(&unit);
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
        .args(["--scalalib", "-d"])
        .arg(&unit)
        .args(&sources)
        .output()
        .expect("running scivolo-jvm");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let (alone, compiled) = compile("tupleunit-alone", &sources[1]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    for name in ["Use.class", "Use$.class", "Use.tasty"] {
        let bytes = |out: &Path| std::fs::read(out.join(name)).expect("a class file");
        assert!(bytes(&unit) == bytes(&alone), "{name} differs");
    }
}

#[test]
fn an_alias_of_the_unit_a_library_class_refers_to_is_completed() {
    let package = "package object scala { type Iterator[+A] = scala.collection.Iterator[A] }\n";
    let user = "package scala.collection\nobject Use { def s(it: Iterator[Int]) = \
                new convert.impl.IntIteratorStepper(it) }\n";
    let sources = written("aliasunit", &[("use.scala", user), ("package.scala", package)]);
    let out = root().join("tmp/jvm-test/aliasunit-out");
    let _ = std::fs::remove_dir_all(&out);
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
        .args(["--scalalib", "-d"])
        .arg(&out)
        .args(&sources)
        .output()
        .expect("running scivolo-jvm");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
}

#[test]
fn a_class_extending_a_namesake_with_statements_of_another_file_compiles() {
    let sources = written(
        "localdummyunit",
        &[
            ("a.scala", "package p\ntrait M { println(1) }\n"),
            ("b.scala", "package q\ntrait M extends p.M { println(2) }\n"),
        ],
    );
    let (_, compiled) = compile_all("localdummyunit-out", &sources);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
}

#[test]
fn a_javalib_member_the_jdk_lacks_is_not_found() {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let code = "object R { def f = java.lang.ProcessBuilder.nullFile }";
    refuses("refusedjdk", code, "member not found");
    let code = "object R { def f(m: java.util.HashMap[String, String]) = m.usingEntry }";
    refuses("refusedjdkhelper", code, "member not found");
}

#[test]
fn a_java_annotation_with_arguments_is_refused() {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let code = "class R { @java.beans.Transient(false) def f = 1 }";
    refuses("refusedannotation", code, "Java annotations with arguments on the JVM");
    let code = "class R { def f(@java.lang.annotation.Documented x: Int) = x }";
    refuses("refusedparamannotation", code, "Java annotations of parameters on the JVM");
}

#[test]
fn a_refused_file_refuses_the_unit() {
    let dir = root().join("tmp/jvm-test/refusedunit");
    std::fs::create_dir_all(&dir).expect("tmp/jvm-test");
    let files = [
        (
            "a.scala",
            "trait Show { def text: String }\nobject Show { given Show = new Show { def text = \"default\" } }\ndef show(using s: Show): String = s.text\n",
        ),
        (
            "b.scala",
            "import scala.language.`3.9-migration`\ngiven Show = new Show { def text = \"custom\" }\n",
        ),
        ("c.scala", "@main def run(): Unit = println(show)\n"),
    ];
    let sources: Vec<PathBuf> = files
        .iter()
        .map(|(name, code)| {
            let path = dir.join(name);
            std::fs::write(&path, code).expect("writing");
            path
        })
        .collect();
    let (out, compiled) = compile_all("refusedunit-out", &sources);
    assert!(!compiled.status.success());
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    assert!(stderr.contains("b.scala: "), "{stderr}");
    assert!(!out.exists(), "{out:?} written");

    let _ = std::fs::remove_dir_all(&out);
    let left_out = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
        .args(["--leave-out-refused", "-d"])
        .arg(&out)
        .args(&sources)
        .output()
        .expect("running scivolo-jvm");
    assert!(!left_out.status.success());
    assert!(out.join("run.class").exists(), "{}", String::from_utf8_lossy(&left_out.stderr));
}

#[test]
fn a_signature_polymorphic_method_is_refused() {
    let code = "object R { def f = java.lang.invoke.MethodHandles.identity(classOf[String]).invoke(\"a\") }";
    refuses("refusedpolymorphic", code, "member not found");
}
