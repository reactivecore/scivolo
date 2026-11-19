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
fn operands_ascribed_to_nothing_throw_on_the_jvm() {
    runs_on_the_jvm("deadoperands", "deadoperands");
}

#[test]
fn searches_in_this_calls_of_local_classes_skip_their_members() {
    runs_on_the_jvm("thiscallsearch", "Test");
}

#[test]
fn inline_methods_returning_match_aliases_do_not_constrain_type_variables() {
    runs_on_the_jvm("inlinematchalias", "Test");
}

#[test]
fn array_spreads_of_named_arguments_beside_defaults_stay_arrays() {
    runs_on_the_jvm("namedarrayspread", "Test");
}

#[test]
fn wildcard_imports_being_typed_are_skipped() {
    runs_on_the_jvm("completingimport", "completingimport.Test");
}

#[test]
fn parents_completed_under_an_import_being_typed_keep_their_types() {
    runs_on_the_jvm("completingparent", "Test");
}

#[test]
fn legacy_match_types_keep_the_outer_constraint_frozen() {
    runs_on_the_jvm("legacymatchvars", "Test");
}

#[test]
fn partial_applications_take_apply_under_scala_3_3() {
    runs_on_the_jvm("partialapply", "Test");
}

#[test]
fn blocks_selecting_structural_members_of_local_selectables_run() {
    runs_on_the_jvm("selectableblock", "Test");
}

#[test]
fn several_files_run_on_the_jvm() {
    runs_on_the_jvm("units", "Units");
}

#[test]
fn a_case_class_parameter_capturing_another_parameter_refused() {
    let source = root().join("tests/jvm/refused/fieldcaptures.scala");
    let (_, compiled) = compile("fieldcaptures", &source);
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    let refusal = "does not support pickling a synthetic type tree of a capture set";
    assert!(!compiled.status.success() && stderr.contains(refusal), "{stderr}");
}

/// A super accessor through prefix `other.Views` whose capture set names the enclosing
/// `Outer.this`, which dotty writes as `{other}`, is refused.
#[test]
fn a_super_accessor_capturing_an_enclosing_this_through_another_prefix_refused() {
    let source = root().join("tests/jvm/refused/supercaptures.scala");
    let (_, compiled) = compile("supercaptures", &source);
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    let refusal = "a capture set that refers to other than static definitions";
    assert!(!compiled.status.success() && stderr.contains(refusal), "{stderr}");
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
fn members_of_parent_refinements_are_dropped_on_the_jvm() {
    runs_on_the_jvm("parentrefinements", "ParentRefinements");
}

#[test]
fn conversions_for_members_diverge_by_their_prototypes() {
    runs_on_the_jvm("viewdivergence", "ViewDivergence");
}

#[test]
fn inline_arguments_narrowed_to_constants_keep_their_skolems() {
    runs_on_the_jvm("namedtupleinit", "NamedTupleInit");
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
fn classes_implementing_java_annotations_run_on_the_jvm() {
    runs_on_the_jvm("annotationimpls", "Annotationimpls");
}

#[test]
fn named_arguments_of_repeated_methods_run_on_the_jvm() {
    runs_on_the_jvm("namedvarargs", "Namedvarargs");
}

#[test]
fn signature_polymorphic_calls_run_on_the_jvm() {
    runs_on_the_jvm("sigpoly", "Sigpoly");
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
fn arrays_of_type_members_cast_as_dotty_on_the_jvm() {
    runs_on_the_jvm("memberarrays", "Memberarrays");
}

#[test]
fn arrays_of_overridden_higher_kinded_members_erase_as_dotty_on_the_jvm() {
    runs_on_the_jvm("hkmemberarrays", "Hkmemberarrays");
}

#[test]
fn private_objects_keep_their_names_on_the_jvm() {
    runs_on_the_jvm("privateobjects", "Privateobjects");
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
fn a_jdk_method_s_signature_erases_a_type_parameter_to_its_bound() {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let source = root().join("tmp/jvm-test/boundsig.scala");
    std::fs::create_dir_all(source.parent().expect("a directory")).expect("tmp/jvm-test");
    let code = "object Boundsig { def d = classOf[String].getAnnotation(classOf[Deprecated]) }";
    std::fs::write(&source, code).expect("writing");
    let (out, compiled) = compile("boundsig", &source);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let tasty = std::fs::read(out.join("Boundsig.tasty")).expect("Boundsig.tasty");
    assert!(tasty.windows(b"Annotation".len()).any(|w| w == b"Annotation"), "a signature of `Annotation`");
}

#[test]
fn a_definition_the_tasty_pickler_lacks_is_refused() {
    let code = "class tag extends scala.annotation.StaticAnnotation\n\
                trait B { def f: Int @tag = 1; def g: Int @tag = 2; def h(b: Boolean) = if b then f else g }";
    refuses("refusedtasty", code, "an annotated type of several annotation trees");
    let code = "import scala.annotation.unchecked.uncheckedVariance as uV\n\
                class C[+A](val m: Map[A @uV, A @uV])";
    refuses("refusedaccessor", code, "an annotated type of several annotation trees");
    let code = "class ann(x: String) extends scala.annotation.StaticAnnotation\n\
                class Box[A](val v: (A, Int @ann(\"a\")))\n\
                object M { def b: Box[Int @ann(\"b\")] = ???; val r = b.v }";
    refuses("refusedtraced", code, "traced to one of several annotation trees with arguments");
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
fn a_use_of_a_well_known_class_the_unit_removes_is_refused() {
    let code = "package scala { object Unit }\npackage p { class U { def t = () } }\n";
    refuses("unitremoved", code, "a reference to a library symbol a definition of the unit replaces");
}

#[test]
fn a_definition_of_scala_unit_is_the_well_known_unit() {
    let source = root().join("tmp/jvm-test/unitdefined.scala");
    std::fs::create_dir_all(source.parent().expect("a directory")).expect("tmp/jvm-test");
    let code = "package scala { final abstract class Unit private extends AnyVal }\n\
                package p { class U { def t = summon[scala.reflect.ClassTag[Unit]] } }\n";
    std::fs::write(&source, code).expect("writing");
    let (out, compiled) = compile("unitdefined", &source);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let u = std::fs::read(out.join("p/U.class")).expect("p/U.class");
    assert!(u.windows(12).any(|w| w == b"UnitManifest"), "ClassTag.Unit not referred to");
}

#[test]
fn any_val_of_a_unit_of_several_files_is_the_well_known_any_val() {
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
    assert!(!stderr.contains("AnyVal.scala: ") && !stderr.contains("v.scala: "), "{stderr}");
    let companion = std::fs::read(out.join("V$.class")).expect("V$.class");
    assert!(companion.windows(15).any(|w| w == b"twice$extension"), "{stderr}");
}

#[test]
fn a_tuple_class_of_the_unit_extends_its_generic_tuple() {
    let tuple2 = "package scala\nimport scala.language.`2.13`\n\
                  final case class Tuple2[+T1, +T2](_1: T1, _2: T2) extends Product2[T1, T2]\n";
    let user =
        "object Use { def t: Tuple = Tuple2(1, \"a\"); def p: Int *: String *: EmptyTuple = (1, \"a\") }\n";
    let main = "object Main {\n\
                \x20 def main(args: Array[String]): Unit = {\n\
                \x20   val tup2: Tuple = 1 *: 2 *: Tuple()\n\
                \x20   tup2 match\n\
                \x20     case Tuple() => println(\"empty\")\n\
                \x20     case (1, 2) => println(\"pair\")\n\
                \x20   val any: Any = (3, 4)\n\
                \x20   println(any.isInstanceOf[NonEmptyTuple])\n\
                \x20   any match\n\
                \x20     case h *: t => println(s\"cons $h $t\")\n\
                \x20     case _ => println(\"other\")\n\
                \x20 }\n\
                }\n";
    let sources =
        written("tupleunit", &[("Tuple2.scala", tuple2), ("use.scala", user), ("main.scala", main)]);
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
    let class = std::fs::read(unit.join("scala/Tuple2.class")).expect("Tuple2.class");
    let has = |s: &str| class.windows(s.len()).any(|w| w == s.as_bytes());
    assert!(!has("$times$colon") && has("productIterator"), "Tuple2.class extends *: or lacks forwarders");
    if Command::new("java").arg("-version").output().is_err() {
        return;
    }
    let classpath =
        format!("{}:{}", root().join("ref/libs/scala-library-3.9.0.jar").display(), unit.display());
    let run = Command::new("java")
        .args(["-Xverify:all", "-cp", &classpath, "Main"])
        .output()
        .expect("running java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "pair\ntrue\ncons 3 (4)\n");
}

#[test]
fn var_handles_of_mixed_in_lazy_values_come_first() {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let code = "trait T { lazy val t: String = \"t\" }\n\
                class Lz extends T { lazy val a: String = \"a\"; lazy val b: String = \"b\" }\n";
    let sources = written("lazyhandles-src", &[("lz.scala", code)]);
    let (out, compiled) = compile("lazyhandles", &sources[0]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let class = std::fs::read(out.join("Lz.class")).expect("Lz.class");
    let at = |n: &str| class.windows(n.len()).position(|w| w == n.as_bytes()).expect(n);
    assert!(at("t$lzy1$lzyHandle") < at("b$lzy1$lzyHandle"));
    assert!(at("b$lzy1$lzyHandle") < at("a$lzy1$lzyHandle"));
}

#[test]
fn lazy_values_initialize_through_var_handles_on_the_jvm() {
    runs_on_the_jvm("lazyhandles", "Lazyhandles");
}

/// A class with lazy values and a `given` alias.
const LAZY_CLASS: &str = "class NoJdk { lazy val a: String = \"a\"; given Int = 1 }\n";

/// Runs scivolo-jvm on `source` into tmp/jvm-test/`name` with `JAVA_HOME` set to `java_home`, no
/// `java` on the `PATH`.
fn compile_with_java_home(name: &str, source: &Path, java_home: Option<&Path>) -> (PathBuf, Output) {
    let out = root().join("tmp/jvm-test").join(name);
    let _ = std::fs::remove_dir_all(&out);
    let mut command = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"));
    command.env_remove("JAVA_HOME").env("PATH", "");
    if let Some(home) = java_home {
        command.env("JAVA_HOME", home);
    }
    (out.clone(), command.arg("-d").arg(&out).arg(source).output().expect("running scivolo-jvm"))
}

/// Class file bytes of `name` with `methods`, each its flags, name and descriptor.
fn class_bytes(name: &str, methods: &[(u16, &str, &str)]) -> Vec<u8> {
    let mut class = classfile::ClassFile::new(classfile::ACC_PUBLIC, name, "java/lang/Object", &[]);
    class.inner_classes(&[classfile::InnerClass {
        inner: "java/lang/invoke/MethodHandles$Lookup".into(),
        outer: Some("java/lang/invoke/MethodHandles".into()),
        name: Some("Lookup".into()),
        flags: classfile::ACC_PUBLIC | classfile::ACC_STATIC | classfile::ACC_FINAL,
    }]);
    for &(access, method, descriptor) in methods {
        class.method(access | classfile::ACC_ABSTRACT, method, descriptor, None);
    }
    class.bytes(&mut |_, _| None).expect("a class file")
}

/// Creates tmp/jvm-test/`name`, a JDK whose `lib/ct.sym` holds `classes`, names and class files.
fn jdk_of(name: &str, classes: &[(&str, Vec<u8>)]) -> PathBuf {
    let home = root().join("tmp/jvm-test").join(name);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("bin")).expect("bin");
    std::fs::create_dir_all(home.join("lib")).expect("lib");
    std::fs::write(home.join("bin/java"), "").expect("bin/java");
    let entry = |(class, bytes): &(&str, Vec<u8>)| (format!("H/java.base/{class}.sig"), bytes.clone());
    let entries: Vec<(String, Vec<u8>)> = classes.iter().map(entry).collect();
    let files: Vec<(String, &[u8])> = entries.iter().map(|(n, b)| (n.clone(), &b[..])).collect();
    jvmgen::jar::write(&home.join("lib/ct.sym"), &files, None).expect("ct.sym");
    home
}

#[test]
fn lazy_values_compile_the_same_without_a_readable_jdk() {
    let sources = written("nojdk-src", &[("nojdk.scala", LAZY_CLASS)]);
    let (without, compiled) = compile_with_java_home("nojdk", &sources[0], None);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: not compared");
        return;
    }
    let (with, compiled) = compile("withjdk", &sources[0]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let class = |dir: &Path| std::fs::read(dir.join("NoJdk.class")).expect("NoJdk.class");
    assert!(class(&without) == class(&with));
}

#[test]
fn a_lazy_value_on_a_jdk_without_var_handle_is_refused() {
    let object = class_bytes("java/lang/Object", &[]);
    let home = jdk_of("jdk8", &[("java/lang/Object", object)]);
    let sources = written("oldjdk-src", &[("nojdk.scala", LAZY_CLASS)]);
    let (out, compiled) = compile_with_java_home("oldjdk", &sources[0], Some(&home));
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    assert!(!compiled.status.success(), "{stderr}");
    assert!(stderr.contains("lazy values on a JDK without VarHandle"), "{stderr}");
    assert!(!out.join("NoJdk.class").exists());
}

#[test]
fn a_lazy_value_on_a_jdk_without_polymorphic_compare_and_set_is_refused() {
    use classfile::{ACC_PUBLIC, ACC_STATIC};
    let classes = [
        (
            "java/lang/invoke/MethodHandles",
            class_bytes(
                "java/lang/invoke/MethodHandles",
                &[(ACC_PUBLIC | ACC_STATIC, "lookup", "()Ljava/lang/invoke/MethodHandles$Lookup;")],
            ),
        ),
        (
            "java/lang/invoke/MethodHandles$Lookup",
            class_bytes(
                "java/lang/invoke/MethodHandles$Lookup",
                &[(
                    ACC_PUBLIC,
                    "findVarHandle",
                    "(Ljava/lang/Class;Ljava/lang/String;Ljava/lang/Class;)Ljava/lang/invoke/VarHandle;",
                )],
            ),
        ),
        (
            "java/lang/invoke/VarHandle",
            class_bytes(
                "java/lang/invoke/VarHandle",
                &[(ACC_PUBLIC, "compareAndSet", "([Ljava/lang/Object;)Z")],
            ),
        ),
    ];
    let home = jdk_of("jdk-plain-cas", &classes);
    let sources = written("plaincas-src", &[("nojdk.scala", LAZY_CLASS)]);
    let (out, compiled) = compile_with_java_home("plaincas", &sources[0], Some(&home));
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    assert!(!compiled.status.success(), "{stderr}");
    assert!(stderr.contains("compareAndSet is not signature polymorphic"), "{stderr}");
    assert!(!out.join("NoJdk.class").exists());
}

#[test]
fn a_pattern_of_a_generic_java_class_its_scrutinee_conforms_to_tests_null() {
    let java = "package jn;\nabstract class MainN<K, V> {}\n";
    let scala = "package jn\nclass Use[K, V] { def h(p: MainN[K, V]): Int = p match { case vn: MainN[?, ?] => 2 } }\n";
    let sources = written("javapattern-src", &[("MainN.java", java), ("Use.scala", scala)]);
    let (out, compiled) = compile_all("javapattern", &sources);
    let stderr = String::from_utf8_lossy(&compiled.stderr);
    let javac = Command::new("javac").arg("--version").output().is_ok();
    assert!(compiled.status.success() || !javac && stderr.contains("javac"), "{stderr}");
    let class = std::fs::read(out.join("jn/Use.class")).expect("jn/Use.class");
    let null_test = [0x2b, 0x4d, 0x2c, 0xc6];
    assert!(class.windows(4).any(|w| w == null_test), "no `ifnull` on the scrutinee");
}

#[test]
fn the_enclosing_method_descriptor_adds_no_inner_class() {
    let code = "class Enc {\n  def sp: java.util.Map.Entry[String, String] =\n    \
                new java.util.AbstractMap.SimpleEntry[String, String](\"a\", \"b\") {}\n}\n";
    let sources = written("enclosingmethod-src", &[("enc.scala", code)]);
    let (out, compiled) = compile("enclosingmethod", &sources[0]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let class = std::fs::read(out.join("Enc$$anon$1.class")).expect("Enc$$anon$1.class");
    let entry = [&[1, 0, 19][..], b"java/util/Map$Entry"].concat();
    assert!(!class.windows(entry.len()).any(|w| w == entry), "an InnerClasses entry of Map$Entry");
}

#[test]
fn the_library_sees_the_jdk_17_api() {
    let code = "package p\nabstract class E extends java.util.concurrent.Executor\n\
                trait T extends java.util.concurrent.ExecutorService\n\
                abstract class A extends E with T\n";
    let sources = written("jdk17view", &[("a.scala", code)]);
    let out = root().join("tmp/jvm-test/jdk17view-out");
    let _ = std::fs::remove_dir_all(&out);
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
        .args(["--scalalib", "-d"])
        .arg(&out)
        .args(&sources)
        .output()
        .expect("running scivolo-jvm");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let class = std::fs::read(out.join("p/A.class")).expect("p/A.class");
    assert!(!class.windows(5).any(|w| w == b"close"), "a forwarder to JDK 19's ExecutorService.close");
}

#[test]
fn a_tuple_class_with_a_class_parent_is_refused() {
    let code = "package scala\nimport scala.language.`2.13`\nabstract class Base\n\
                final case class Tuple2[+T1, +T2](_1: T1, _2: T2) extends Base with Product2[T1, T2]\n";
    refuses("tupleclassparent", code, "a tuple class with a class parent");
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
fn a_java_annotation_with_an_argument_that_is_no_constant_is_refused() {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let code = "class R { @java.beans.Transient(sys.props.isEmpty) def f = 1 }";
    refuses("refusedannotation", code, "Java annotation arguments other than constants");
}

#[test]
fn a_refused_file_refuses_the_unit() {
    let dir = root().join("tmp/jvm-test/refusedunit");
    std::fs::create_dir_all(&dir).expect("tmp/jvm-test");
    let files = [
        (
            "a.scala",
            "trait Show { def text: String }\nobject Show { given Show with { def text = \"default\" } }\ndef show(using s: Show): String = s.text\n",
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
