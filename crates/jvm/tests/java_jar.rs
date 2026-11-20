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
    java_jar(dir, "shapes/Box.java", &[])
}

/// Compiles tests/jvm/javajar/`source` with javac's `flags` into the jar `shapes.jar` of
/// tmp/jvm-test/`dir`, which it returns; none if the JDK's tools are missing.
fn java_jar(dir: &str, source: &str, flags: &[&str]) -> Option<PathBuf> {
    let missing = |tool: &str| Command::new(tool).arg("--version").output().is_err();
    if ["java", "javac", "jar"].into_iter().any(missing) {
        eprintln!("no java, javac or jar on the PATH: skipped");
        return None;
    }
    let out = root().join("tmp/jvm-test").join(dir);
    let _ = std::fs::remove_dir_all(&out);
    let (classes, jar) = (out.join("java"), out.join("shapes.jar"));
    let source = root().join("tests/jvm/javajar").join(source);
    run(Command::new("javac").args(flags).args(["--release", "17", "-d"]).arg(&classes).arg(source));
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

/// Named arguments of methods of a class compiled with `javac -parameters`.
#[test]
fn method_parameters_of_class_files_name_parameters() {
    let Some(jar) = java_jar("javajar-params", "named/Named.java", &["-parameters"]) else { return };
    let compiled = jar.with_file_name("scala");
    run(&mut scivolo(&jar, &compiled, "namedparams"));
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}:{}", compiled.display(), jar.display(), library.display());
    let stdout = run(Command::new("java").args(["-Xverify:all", "-cp", &classpath, "NamedParams"]));
    assert_eq!(stdout, "3\n7\n");
}

/// `MethodParameters` do not name the parameters of a class file's generic method.
#[test]
fn method_parameters_of_generic_methods_name_nothing() {
    let Some(jar) = java_jar("javajar-generic-params", "named/Named.java", &["-parameters"]) else { return };
    let run = scivolo(&jar, &jar.with_file_name("scala"), "namedgeneric").output().expect("scivolo-jvm");
    assert!(!run.status.success(), "named arguments of a generic Java method were accepted");
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

/// The programs that use the Scala 2 class files of a jar (`just scala2-fixture`), each its
/// source, the jar and its main class: tests/jvm/javajar/scala2.scala by the class's path,
/// scala2_shadow.scala through a wildcard import that shadows `scala.StringBuilder`, and
/// tests/jvm/scala2: basics, members, shapes (higher-kinded type parameters, type lambdas of
/// refinements, type members, default, repeated and by-name parameters, existentials, annotated
/// types, nested objects, implicit classes, a package object, case classes), extends.scala and
/// mixins.scala extending Scala 2 classes and traits with methods, sized.scala one with a
/// method of an existential type, values.scala traits with a `var`, a `val` and a `lazy val`,
/// autoapply.scala calling Scala 2 methods with `()` without it, arrays.scala passing arrays of
/// generic, reference and primitive elements, valueclasses.scala using value classes (case,
/// implicit, generic, of an array, with a private value, with a universal trait) through their
/// extension methods, boxed in arrays, collections and generic calls, and matched by type,
/// universal.scala a class and a value class extending a Scala 2 universal trait.
const SCALA2_PROGRAMS: [(&str, &str, &str); 13] = [
    ("tests/jvm/javajar/scala2.scala", "tests/jvm/javajar/legacy/legacy.jar", "Scala2"),
    ("tests/jvm/javajar/scala2_shadow.scala", "tests/jvm/javajar/legacy/legacy.jar", "Scala2Shadow"),
    ("tests/jvm/scala2/basics/basics.scala", "tests/jvm/scala2/basics/basics.jar", "Basics"),
    ("tests/jvm/scala2/members/members.scala", "tests/jvm/scala2/members/members.jar", "Members"),
    ("tests/jvm/scala2/shapes/shapes.scala", "tests/jvm/scala2/shapes/shapes.jar", "Shapes"),
    ("tests/jvm/scala2/basics/extends.scala", "tests/jvm/scala2/basics/basics.jar", "Extends"),
    ("tests/jvm/scala2/mixins/mixins.scala", "tests/jvm/scala2/mixins/mixins.jar", "Mixins"),
    ("tests/jvm/scala2/mixins/sized.scala", "tests/jvm/scala2/mixins/mixins.jar", "SizedMain"),
    ("tests/jvm/scala2/mixins/values.scala", "tests/jvm/scala2/mixins/mixins.jar", "Values"),
    ("tests/jvm/scala2/autoapply/autoapply.scala", "tests/jvm/scala2/autoapply/autoapply.jar", "AutoApply"),
    ("tests/jvm/scala2/arrays/arrays.scala", "tests/jvm/scala2/arrays/arrays.jar", "Arrays"),
    (
        "tests/jvm/scala2/valueclasses/valueclasses.scala",
        "tests/jvm/scala2/valueclasses/valueclasses.jar",
        "ValueClasses",
    ),
    (
        "tests/jvm/scala2/valueclasses/universal.scala",
        "tests/jvm/scala2/valueclasses/valueclasses.jar",
        "Universal",
    ),
];

/// scivolo-jvm compiles the `SCALA2_PROGRAMS` against their jars, read from the classes' Scala 2
/// pickles, and each prints on `java -Xverify:all` what its `.check` file holds, dotty's output;
/// skipped without java.
#[test]
fn scala2_class_files_are_read() {
    if Command::new("java").arg("--version").output().is_err() {
        eprintln!("no java on the PATH: skipped");
        return;
    }
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    for (source, jar, main) in SCALA2_PROGRAMS {
        let (source, jar) = (root().join(source), root().join(jar));
        let out = root().join("tmp/jvm-test/scala2").join(main);
        let _ = std::fs::remove_dir_all(&out);
        let scivolo = env!("CARGO_BIN_EXE_scivolo-jvm");
        run(Command::new(scivolo).arg("--cp").arg(&jar).arg("-d").arg(&out).arg(&source));
        let classpath = format!("{}:{}:{}", out.display(), jar.display(), library.display());
        let stdout = run(Command::new("java").args(["-Xverify:all", "-cp", &classpath, main]));
        let check = std::fs::read_to_string(source.with_extension("check")).expect("a .check file");
        assert_eq!(stdout, check, "{main}");
    }
}

/// scivolo-jvm refuses the programs of tests/jvm/scala2 that use what it does not read of the
/// Scala 2 classes of their jars, by name: refused/nested_refused.scala uses an object whose
/// parent was entered while a refused class (with an implicit member not read) was and refers to
/// it; refused/existential.scala calls a method of an existential type dotty approximates,
/// refused/structural.scala one of a structural type, refused/nested_member.scala one of a nested
/// object of a dependent method type, which an extension method would answer;
/// refused/private_within.scala a member `private[refused]`, which dotty rejects;
/// shapes/case_match.scala matches on a Scala 2 case class; secret.scala extends a trait with a
/// private `val`, memo.scala one with a private `lazy val`, effect.scala one with a `val` of type
/// `Unit` whose getter Scala 2 erases to `()V` (dotty's class lacks it: `NoSuchMethodError`);
/// macros.scala calls a Scala 2 macro, which dotty rejects too; and
/// guard.scala and value.scala call a method and a value class's extension method whose erasure
/// its class file lacks (`just scala2-fixture` swaps the objects' class files).
#[test]
fn scala2_constructs_not_read_are_refused_by_name() {
    let refusals = [
        (
            "refused/nested_refused",
            "Scala 2 class file `refused.A` of a jar: refers to the refused `refused.B`",
        ),
        ("refused/existential", "`refused.Lib$.fbounded` of a jar: an existential type that is no wildcard"),
        ("refused/structural", "`refused.Lib$.struct` of a jar: a structural member `size`"),
        ("refused/nested_member", "`refused.Outer$.Deep$.same` of a jar: a dependent method type"),
        ("refused/private_within", "inaccessible member"),
        ("shapes/case_match", "`shapes.Rect$.unapply` of a jar: the extractor of a case class"),
        (
            "mixins/secret",
            "Scala 2 class `mixins.Secret` of a jar: its abstract method \
             `mixins$Secret$_setter_$mixins$Secret$$hidden_$eq` is not read",
        ),
        ("mixins/memo", "Scala 2 class `mixins.Memo` of a jar: its member `once` has a private lazy value"),
        (
            "mixins/effect",
            "Scala 2 member `mixins.Effect.done` of a jar: erased as `done()Lscala/runtime/BoxedUnit;`, \
             not in the class file `mixins/Effect`",
        ),
        ("macros/macros", "Scala 2 member `macros.M$.one` of a jar: a macro"),
        (
            "guard/guard",
            "Scala 2 member `guard.O$.f` of a jar: erased as `f(I)I`, not in the class file `guard/O$`",
        ),
        (
            "guard/value",
            "Scala 2 member `guard.V$.g$extension` of a jar: erased as `g$extension(II)I`, \
             not in the class file `guard/V$`",
        ),
    ];
    for (name, message) in refusals {
        let source = root().join(format!("tests/jvm/scala2/{name}.scala"));
        let dir = name.split('/').next().expect("a directory");
        let jar = root().join(format!("tests/jvm/scala2/{dir}/{dir}.jar"));
        let out = root().join("tmp/jvm-test/scala2").join(name);
        let scivolo = env!("CARGO_BIN_EXE_scivolo-jvm");
        let refused = output(Command::new(scivolo).arg("--cp").arg(&jar).arg("-d").arg(&out).arg(&source));
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success() && stderr.contains(message), "{name}: {stderr}");
    }
}

/// tests/jvm/javajar/enumvalues.scala against the jar: the `final val` of a Java enum's constant
/// keeps its singleton type and that of another static field of the enum is widened (dotty's
/// `isJavaEnumValue`); the `InnerClasses` entry of the enum has the flags dotty gives it, which
/// javac's `InnerClasses` entry of the enum does not carry.
#[test]
fn java_enum_values_of_a_jar_keep_their_types_and_flags() {
    let Some(jar) = shapes_jar("javajar-enumvalues") else { return };
    let out = jar.with_file_name("scala");
    run(&mut scivolo(&jar, &out, "enumvalues"));

    let bytes = std::fs::read(out.join("shapes/EnumValues.tasty")).expect("our TASTy");
    let file = tasty::Tasty::parse(&bytes).expect("a TASTy file");
    let ast = tasty::Ast::parse(file.section("ASTs").expect("trees").bytes).expect("the trees");
    let printed = tasty::dump(&ast, &file.names);
    let type_of = |name: &str| {
        let line = printed.lines().find(|l| l.trim_start().starts_with(&format!("VALDEF {name}@")));
        line.and_then(|l| l.split_once(": ")).map(|(_, t)| t.to_owned())
    };
    assert_eq!(type_of("on").as_deref(), Some("shapes.Box.Mode.ON final"), "{printed}");
    assert_eq!(type_of("All").as_deref(), Some("java.util.List[shapes.Box.Mode] final"), "{printed}");

    let class = classfile::read(&std::fs::read(out.join("shapes/EnumValues$.class")).unwrap()).unwrap();
    let mode = class.inner_classes.iter().find(|c| c.inner == "shapes/Box$Mode").expect("Box$Mode's entry");
    let dotty_s =
        classfile::ACC_PUBLIC | classfile::ACC_STATIC | classfile::ACC_ABSTRACT | classfile::ACC_ENUM;
    assert_eq!(mode.flags, dotty_s);
}

#[test]
fn inaccessible_java_classes_and_native_classes_are_refused() {
    let Some(jar) = shapes_jar("javajar-refused") else { return };
    for name in ["hidden", "subpackage", "shelf", "native"] {
        let out = output(&mut scivolo(&jar, &jar.with_file_name(name), name));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && stderr.contains("does not support"), "{name}: {stderr}");
    }
}

/// tests/jvm/scala2/shapes/shapes.scala compiles as dotty does: the `InnerClasses` entry of the
/// class of a nested object of a Scala 2 pickle is public static, not final; the synthetic
/// `apply` of a Scala 2 case class takes its default from `$lessinit$greater$default$2`.
#[test]
fn scala2_class_files_are_used_as_dotty_uses_them() {
    let source = root().join("tests/jvm/scala2/shapes/shapes.scala");
    let jar = root().join("tests/jvm/scala2/shapes/shapes.jar");
    let out = root().join("tmp/jvm-test/scala2-inner");
    let _ = std::fs::remove_dir_all(&out);
    run(Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
        .arg("--cp")
        .arg(&jar)
        .arg("-d")
        .arg(&out)
        .arg(&source));
    let bytes = std::fs::read(out.join("Shapes$.class")).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("$lessinit$greater$default$2") && !text.contains("apply$default$2"));
    let class = classfile::read(&bytes).unwrap();
    let nested = class.inner_classes.iter().find(|c| c.inner == "shapes/Tools$Nested$").expect("an entry");
    assert_eq!(nested.flags, classfile::ACC_PUBLIC | classfile::ACC_STATIC);
}
