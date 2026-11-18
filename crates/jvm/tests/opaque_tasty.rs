//! Opaque types read from the TASTy of tests/jvm/tastylib/opaques/lib.scala: abstract to the
//! programs compiled against it, their right-hand sides in erasure and generic signatures.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Programs dotty rejects, each with the refusal that names why.
const REFUSED: [(&str, &str); 19] = [
    ("val x = O.mkF != null", "compar"),
    ("val x = VF.op != null", "compar"),
    ("val x: V.OI[String] = Nil", "adaptation"),
    ("val x = summon[O.G[Int] <:< Seq[Int]]", "no implicit argument"),
    ("val x = summon[O.F[Int] =:= List[Int]]", "no implicit argument"),
    ("val x: R[O.F[String]] = \"\"", "match type"),
    ("val x = O.mkF.head", "member"),
    ("val x: List[Int] = O.mkB", "adaptation"),
    ("val x = new TF { def f(x: List[Int]) = 1 }", "unimplemented member"),
    ("val x = new TI { def f(x: PF.FB[Int]) = 1 }", "unimplemented member"),
    ("class C extends O.F[Int]", "parent that is not a class type"),
    ("class C extends Prim.M", "parent that is not a class type"),
    ("def g[C[+_]] = 1; val x = g[V.Op]", "bounds"),
    ("def g[C[-_]] = 1; val x = g[V.OI]", "bounds"),
    ("class C extends TB { def f[A <: String](x: A): A = x }", "unimplemented member"),
    ("class C extends TV { val w = 1 }", "unimplemented member"),
    ("class I { def f(x: List[Int]) = 1 }; val x = new I with TF {}", "unimplemented member"),
    ("class I { def f(x: PF.FB[Int]) = 1 }; val x = new I with TI {}", "unimplemented member"),
    (
        "class A { def f(x: O.F[Int]) = 1 }; class B extends A { override def f(x: List[Int]) = 2 }",
        "name clash",
    ),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn compile(dest: &Path, classpath: Option<&Path>, source: &Path) -> Output {
    let mut run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"));
    run.current_dir(root());
    if let Some(classpath) = classpath {
        run.arg("--cp").arg(classpath);
    }
    run.arg("-d").arg(dest).arg(source).output().expect("running scivolo-jvm")
}

/// Compiles lib.scala into a fresh directory `name` under tmp/jvm-test; that directory and the
/// library's output.
fn compiled_lib(name: &str) -> (PathBuf, PathBuf) {
    let out = root().join("tmp/jvm-test").join(name);
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("a test directory");
    let lib = out.join("lib");
    let run = compile(&lib, None, &root().join("tests/jvm/tastylib/opaques/lib.scala"));
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    (out, lib)
}

#[test]
fn opaque_types_from_tasty_stay_abstract() {
    let (out, lib) = compiled_lib("opaquerefused");
    for (i, (code, reason)) in REFUSED.iter().enumerate() {
        let source = out.join(format!("refused{i}.scala"));
        std::fs::write(&source, format!("import lib.*\nobject C:\n  {code}\n")).expect("a source file");
        let run = compile(&out.join(format!("o{i}")), Some(&lib), &source);
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(!run.status.success(), "accepted: {code}");
        assert!(stderr.contains(reason), "{code}: {stderr}");
    }
}

/// use.scala compiles against the library and prints what dotty's classes print (use.check): the
/// values and the generic signatures of members typed by opaque types and their export forwarders.
#[test]
fn opaque_types_from_tasty_compile_run_and_erase() {
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: not run");
        return;
    }
    let (out, lib) = compiled_lib("opaqueused");
    let dir = root().join("tests/jvm/tastylib/opaques");
    let user = out.join("use");
    let run = compile(&user, Some(&lib), &dir.join("use.scala"));
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}:{}", user.display(), lib.display(), library.display());
    let run = Command::new("java").args(["-Xverify:all", "-cp", &classpath, "Use"]).output().expect("java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let check = std::fs::read_to_string(dir.join("use.check")).expect("use.check");
    assert_eq!(String::from_utf8_lossy(&run.stdout), check);
}
