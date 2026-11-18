//! Members and references of class files as dotty writes them, read through `javap`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compiles tests/jvm/linkage/`name`.scala into tmp/jvm-test/linkage-`name`.
fn compile(name: &str) -> PathBuf {
    let out = root().join("tmp/jvm-test").join(format!("linkage-{name}"));
    let _ = std::fs::remove_dir_all(&out);
    let source = root().join("tests/jvm/linkage").join(format!("{name}.scala"));
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm")).arg("-d").arg(&out).arg(source).output();
    let run = run.expect("running scivolo-jvm");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    out
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
    let Some(nested) = javap(&out, "generics.Outer$Mid$In") else { return };
    for expected in
        ["final generics.Outer<TA>.Mid $outer;", "generics.Outer<TA>.Mid generics$Outer$Mid$In$$$outer()"]
    {
        assert!(nested.contains(expected), "{expected} in\n{nested}");
    }
}

/// An intersection of nested classes erases to the subclass, whatever their names.
#[test]
fn intersections_of_nested_classes_erase_to_the_subclass() {
    let out = compile("glb");
    let Some(text) = javap(&out, "glb.Nest") else { return };
    assert!(text.contains("public static int glb(glb.Nest$Z)"), "{text}");
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

#[test]
fn raw_pointers_are_ordinary_classes() {
    let out = compile("rawtypes");
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
