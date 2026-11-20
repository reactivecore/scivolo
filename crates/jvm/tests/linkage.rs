//! Members and references of class files as dotty writes them, read through `javap`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Scala Native's library, whose classes the JVM has only when it is on the class path.
const NATIVELIB: &str = "ref/libs/nativelib_native0.5_3-0.5.12.jar";

/// Runs scivolo-jvm on tests/jvm/linkage/`name`.scala with `args`; the output directory and run.
fn scivolo(name: &str, args: &[&Path]) -> (PathBuf, Output) {
    let out = root().join("tmp/jvm-test").join(format!("linkage-{name}"));
    let _ = std::fs::remove_dir_all(&out);
    let source = root().join("tests/jvm/linkage").join(format!("{name}.scala"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"));
    for &cp in args {
        command.arg("--cp").arg(cp);
    }
    let run = command.arg("-d").arg(&out).arg(source).output().expect("running scivolo-jvm");
    (out, run)
}

/// Compiles tests/jvm/linkage/`name`.scala into tmp/jvm-test/linkage-`name`, seeing jars `cp`.
fn compile_with(name: &str, cp: &[&Path]) -> PathBuf {
    let (out, run) = scivolo(name, cp);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    out
}

fn compile(name: &str) -> PathBuf {
    compile_with(name, &[])
}

/// The `javap -c -p` text of `class` under `out`; none without a `javap` on the PATH.
fn javap(out: &Path, class: &str) -> Option<String> {
    let run = Command::new("javap").args(["-c", "-p", "-cp"]).arg(out).arg(class).output().ok()?;
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    Some(String::from_utf8_lossy(&run.stdout).into_owned())
}

/// Static forwarders of JDK default methods see the object's type arguments; the outer reference
/// of a class nested in an inner class of a generic class names the generic outer class; a class
/// in a method of a local class names that method's type parameter.
#[test]
fn generic_signatures_of_forwarders_and_outer_references() {
    let out = compile("generics");
    let Some(local) = javap(&out, "generics.Blocks$$anon$2") else { return };
    assert!(local.contains("implements generics.Marker, scala.Function0<T>"), "{local}");
    let Some(forwarders) = javap(&out, "generics.ByLength") else { return };
    let expected = "public static java.util.Comparator<java.lang.String> reversed()";
    assert!(forwarders.contains(expected), "{forwarders}");
    let Some(selfish) = javap(&out, "generics.Selfish$Inner") else { return };
    assert!(selfish.contains("final generics.Selfish $outer;"), "{selfish}");
    let Some(nested) = javap(&out, "generics.Outer$Mid$In") else { return };
    for expected in
        ["final generics.Outer<TA>.Mid $outer;", "generics.Outer<TA>.Mid generics$Outer$Mid$In$$$outer()"]
    {
        assert!(nested.contains(expected), "{expected} in\n{nested}");
    }
}

/// An intersection of nested classes erases to the subclass, whatever their names; static
/// forwarders come in the order of the methods' source names.
#[test]
fn intersections_of_nested_classes_erase_to_the_subclass() {
    let out = compile("glb");
    let Some(text) = javap(&out, "glb.Nest") else { return };
    assert!(text.contains("public static int glb(glb.Nest$Z)"), "{text}");
    let Some(names) = javap(&out, "glb.FullNames$") else { return };
    for expected in ["named(glb.Ab, glb.Ab$c)", "swapped(glb.Ab, glb.Ab$c)", "values(int)"] {
        assert!(names.contains(expected), "{expected} in\n{names}");
    }
    let Some(mirror) = javap(&out, "glb.Operators") else { return };
    let at = |name: &str| mirror.find(&format!("int {name}(int)")).expect(name);
    assert!(at("$plus") < at("$less") && at("$less") < at("a"), "{mirror}");
}

/// Hoisted superclass constructor arguments take the type parameters of the class and run.
#[test]
fn hoisted_super_arguments_are_polymorphic() {
    let out = compile("superargs");
    let Some(text) = javap(&out, "superargs.Top") else { return };
    let expected = "private static <A> scala.Function0<A> Top$superArg$1(A)";
    assert!(text.contains(expected), "{expected} in\n{text}");
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}", out.display(), library.display());
    let run = Command::new("java").args(["-Xverify:all", "-cp", &classpath, "superargs.Main"]).output();
    let run = run.expect("running java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "first\n(1,c)\nd\n");
}

/// A `Null$` call in a branch of a join of type `Null` jumps without being replaced by `null`;
/// the value is adapted after the join, and the code verifies and runs.
#[test]
fn null_values_are_adapted_after_their_joins() {
    let out = compile("nulljoin");
    let Some(text) = javap(&out, "nulljoin.Main$") else { return };
    let lines: Vec<&str> = text.lines().collect();
    let call = "Method none:()Lscala/runtime/Null$;";
    let jumps = lines.windows(2).filter(|w| w[0].contains(call) && w[1].contains("goto")).count();
    assert_eq!(jumps, 2, "{text}");
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}", out.display(), library.display());
    let run = Command::new("java").args(["-Xverify:all", "-cp", &classpath, "nulljoin.Main"]).output();
    let run = run.expect("running java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "null\nnull\nnull\nread\n");
}

/// The `javap -v -p -l` text of `class` under `out`; none without a `javap` on the PATH.
fn javap_verbose(out: &Path, class: &str) -> Option<String> {
    let run = Command::new("javap").args(["-v", "-p", "-l", "-cp"]).arg(out).arg(class).output().ok()?;
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    Some(String::from_utf8_lossy(&run.stdout).into_owned())
}

/// The line marks and the `Deprecated` attribute of the member of `javap_verbose` text `text`
/// declared by `header`.
fn member_lines<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
    let start = text.find(header).unwrap_or_else(|| panic!("{header} in\n{text}"));
    let member = &text[start..];
    let member = &member[..member.find("\n\n").unwrap_or(member.len())];
    member.lines().map(str::trim).filter(|l| l.starts_with("line ") || l.starts_with("Deprecated")).collect()
}

/// Line marks and `Deprecated` as dotty writes them: a value class extension method's object of
/// the receiver's line, an array update after its operands, a field initializer's `this` of its
/// value's line, no line for the `()` ending a secondary constructor, a branch's match marked again
/// on its scrutinee's line after the cases' line, a lazy value's `VarHandle` set on the line its
/// class's parents end, an `InnerClasses` entry for an object with an operator name, and the
/// conversion of an implicit class deprecated for companion methods, the class only if for
/// companion classes.
#[test]
fn line_marks_and_deprecations_follow_dotty() {
    let out = compile("lines");
    let Some(calls) = javap_verbose(&out, "lines.Calls$") else { return };
    let extension = member_lines(&calls, "java.lang.String[] extension(");
    assert_eq!(extension, ["line 5: 0", "line 5: 6", "line 6: 22", "line 7: 30"], "{calls}");
    assert_eq!(member_lines(&calls, "void update(").last(), Some(&"line 12: 42"), "{calls}");
    for conversion in ["A Wrapped(A)", "A Both(A)"] {
        assert!(member_lines(&calls, conversion).contains(&"Deprecated: true"), "{calls}");
    }
    let Some(fields) = javap_verbose(&out, "lines.Fields") else { return };
    let primary = member_lines(&fields, "public lines.Fields(int, int)");
    assert_eq!(primary, ["line 25: 0", "line 27: 4", "line 26: 12"], "{fields}");
    assert_eq!(member_lines(&fields, "public lines.Fields(int);"), ["line 31: 0", "line 32: 2"], "{fields}");
    let Some(branches) = javap_verbose(&out, "lines.Branches$") else { return };
    let contracted = member_lines(&branches, "java.lang.Object contracted(");
    assert_eq!(
        contracted,
        ["line 36: 0", "line 36: 11", "line 37: 15", "line 38: 31", "line 39: 33"],
        "{branches}"
    );
    let Some(handles) = javap_verbose(&out, "lines.Handles") else { return };
    assert_eq!(member_lines(&handles, "static {}"), ["line 43: 0"], "{handles}");
    let Some(operators) = javap_verbose(&out, "lines.Operators$") else { return };
    assert!(operators.contains("// $colon$plus$=class scala/collection/package$$colon$plus$"), "{operators}");
    for (class, deprecated) in [("lines.Calls$Wrapped", false), ("lines.Calls$Both", true)] {
        let Some(text) = javap_verbose(&out, class) else { return };
        assert_eq!(text.lines().any(|l| l == "Deprecated: true"), deprecated, "{text}");
    }
}

/// The opcodes of the member of `javap` text `text` declared by `header`.
fn member_ops<'a>(text: &'a str, header: &str) -> Vec<&'a str> {
    let start = text.find(header).unwrap_or_else(|| panic!("{header} in\n{text}"));
    let member = &text[start..];
    let member = &member[..member.find("\n\n").unwrap_or(member.len())];
    let ops =
        member.lines().filter_map(|l| l.trim().split_once(": ").filter(|(at, _)| at.parse::<u32>().is_ok()));
    ops.filter_map(|(_, op)| op.split_whitespace().next()).collect()
}

/// Whether `ops` has the run `run`.
fn has_run(ops: &[&str], run: &[&str]) -> bool {
    ops.windows(run.len()).any(|w| w == run)
}

/// As dotty compiling the library with the source: the forwarder of a bridge overriding a library
/// class's bridge, without a signature; a library class's bridge not forwarded where a member it
/// overrides is, mixed in or not; the `@throws` of a library method a bridge overrides.
#[test]
fn forwarders_see_the_bridges_and_mixin_forwarders_of_the_library() {
    let out = root().join("tmp/jvm-test/linkage-libforwarders");
    let _ = std::fs::remove_dir_all(&out);
    let source = root().join("tests/jvm/linkage/libforwarders.scala");
    let mut command = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"));
    let run = command.args(["--scalalib", "-d"]).arg(&out).arg(source).output().expect("running scivolo-jvm");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let Some(text) = javap_verbose(&out, "libforwarders.Nl") else { return };
    let flags = |header: &str| {
        let start = text.find(header).unwrap_or_else(|| panic!("{header} in\n{text}"));
        let flags = text[start..].lines().find(|l| l.trim().starts_with("flags:")).unwrap_or_default();
        flags.contains("ACC_SYNTHETIC")
    };
    assert!(flags("public static boolean isDefinedAt(java.lang.Object);"), "{text}");
    assert!(text.contains("public static scala.collection.IterableFactory iterableFactory();"), "{text}");
    assert!(text.contains("public static scala.collection.View<scala.runtime.Nothing$> view();"), "{text}");
    let and_then = "scala.Function1<java.lang.Object, A> andThen(";
    assert!(!flags(and_then), "{text}");
    let thrown = "public static java.lang.Object apply(int) throws java.lang.IndexOutOfBoundsException;";
    assert!(text.contains(thrown), "{text}");
}

/// As dotty's member lookup: a concrete member of a superclass forwarded over a deferred one of a
/// trait mixed in later; a trait member reached by a path past a class's bridge forwarded over
/// that bridge, so the forwarder is not synthetic.
#[test]
fn forwarders_take_the_member_dotty_selects() {
    let out = compile("forwardermerge");
    let Some(m) = javap_verbose(&out, "forwardermerge.M") else { return };
    assert!(m.contains("public static int m();"), "{m}");
    let Some(w) = javap_verbose(&out, "forwardermerge.W") else { return };
    let start = w.find("public static java.lang.Object view();").unwrap_or_else(|| panic!("{w}"));
    let flags = w[start..].lines().find(|l| l.trim().starts_with("flags:")).unwrap_or_default();
    assert!(!flags.contains("ACC_SYNTHETIC"), "{w}");
}

/// As dotty: the `this` qualifier of an enum case kept as a statement, a `Null$` or `Nothing$`
/// cast returned as it is, and, in a method with a protected range, a dead block after each store.
#[test]
fn phantom_values_and_static_qualifiers_follow_dotty() {
    let out = compile("phantoms");
    let Some(color) = javap(&out, "phantoms.Color$") else { return };
    let value_of = member_ops(&color, "phantoms.Color valueOf(");
    assert!(has_run(&value_of, &["aload_0", "pop", "getstatic", "areturn"]), "{color}");
    let init = member_ops(&color, "static {}");
    assert!(has_run(&init, &["iconst_0", "getstatic", "pop", "getstatic", "aastore"]), "{color}");
    let Some(prim) = javap(&out, "phantoms.Prim") else { return };
    let nul = member_ops(&prim, "scala.runtime.Null$ nul()");
    assert!(!nul.contains(&"pop") && has_run(&nul, &["checkcast", "areturn"]), "{prim}");
    let nothing = member_ops(&prim, "scala.runtime.Nothing$ nothing()");
    assert!(!nothing.contains(&"athrow") && has_run(&nothing, &["checkcast", "areturn"]), "{prim}");
    let init = member_ops(&prim, "java.lang.Object nothing$lzyINIT1()");
    assert!(has_run(&init, &["athrow", "athrow", "nop"]), "{prim}");
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}", out.display(), library.display());
    let run = Command::new("java").args(["-Xverify:all", "-cp", &classpath, "phantoms.Main"]).output();
    let run = run.expect("running java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "Red\nList(Red, Green)\nnull\nno value\n");
}

/// The static forwarder of a synthetic trait method, the body of a lambda an anonymous class
/// calls, is public and static only, as dotty's `makeStatifiedDefSymbol` makes it.
#[test]
fn trait_forwarders_of_synthetic_methods_are_not_synthetic() {
    let out = compile("forwarderflags");
    let Some(text) = javap_verbose(&out, "forwarderflags.Ord") else { return };
    let forwarder = "forwarderflags$Ord$$_$orElse$$anonfun$1$(";
    let start = text.find(forwarder).unwrap_or_else(|| panic!("{forwarder} in\n{text}"));
    let flags = text[start..].lines().find(|l| l.trim().starts_with("flags:"));
    assert_eq!(flags.map(str::trim), Some("flags: (0x0009) ACC_PUBLIC, ACC_STATIC"), "{text}");
}

/// A call of a generic value class's method on a receiver evaluated once casts its result once;
/// `new` of a Scala 2 class is not pure (dotty sets `NoInits` on Scala 2 traits only), so its
/// receiver is evaluated once too.
#[test]
fn generic_value_class_calls_on_receivers_evaluated_once() {
    let out = compile_with("vcevalonce", &[&root().join("tests/jvm/scala2/valueclasses/valueclasses.jar")]);
    let Some(text) = javap(&out, "VcEvalOnce$") else { return };
    let method = |name: &str| {
        let start = text.find(&format!(" {name}();")).unwrap_or_else(|| panic!("{name} in\n{text}"));
        let end = text[start..].find("\n\n").map_or(text.len(), |e| start + e);
        text[start..end].to_owned()
    };
    let mapped = method("mapped");
    assert_eq!(mapped.matches("checkcast").count(), 1, "{mapped}");
    let scala2 = method("scala2");
    let stored = scala2.find("astore").unwrap_or_else(|| panic!("a stored receiver in\n{scala2}"));
    assert!(stored < scala2.find("valueclasses/Box$.MODULE$").expect("the extension's object"), "{scala2}");
}

/// Without Scala Native's library on the class path its API does not exist, as for dotty.
#[test]
fn scala_native_api_without_its_jar_is_refused() {
    let (_, run) = scivolo("rawtypes", &[]);
    assert!(!run.status.success());
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("scala.scalanative.runtime.RawPtr"), "{stderr}");
}

#[test]
fn raw_pointers_are_ordinary_classes() {
    let out = compile_with("rawtypes", &[&root().join(NATIVELIB)]);
    let Some(text) = javap(&out, "RawTypes$") else { return };
    for expected in [
        "public boolean isNull(scala.scalanative.runtime.RawPtr)",
        "public boolean sameSize(scala.scalanative.runtime.RawSize, scala.scalanative.runtime.RawSize)",
        "public java.lang.Object kept(scala.scalanative.runtime.RawPtr)",
        "public scala.scalanative.runtime.RawPtr[] held(scala.scalanative.runtime.RawPtr)",
        "ifnonnull",
    ] {
        assert!(text.contains(expected), "{expected} in\n{text}");
    }
}
