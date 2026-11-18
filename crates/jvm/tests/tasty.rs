//! The jar of tests/jvm/tastylib: its manifest, the TASTy of its top-level classes that their
//! `TASTY` attributes name and our reader reads, and scala-cli compiling
//! tests/jvm/tastylib/user.scala against it.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compiles tests/jvm/tastylib into the jar tmp/jvm-test/tastylib/geo.jar.
fn geo_jar() -> PathBuf {
    let out = root().join("tmp/jvm-test/tastylib");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("tmp/jvm-test/tastylib");
    let jar = out.join("geo.jar");
    let source = root().join("tests/jvm/tastylib/tastylib.scala");
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
        .args(["--main-class", "geo.Shapes", "-d"])
        .arg(&jar)
        .arg(source)
        .output();
    let run = run.expect("running scivolo-jvm");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    jar
}

fn entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    zip.by_name(name).unwrap_or_else(|_| panic!("{name} in the jar")).read_to_end(&mut bytes).unwrap();
    bytes
}

#[test]
fn the_jar_holds_a_manifest_and_the_tasty_its_classes_name() {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(geo_jar()).unwrap()).unwrap();
    let manifest = String::from_utf8(entry(&mut zip, "META-INF/MANIFEST.MF")).unwrap();
    assert_eq!(
        manifest,
        "Manifest-Version: 1.0\r\nScala-Compiler-Version: 3.9.0\r\nMain-Class: geo.Shapes\r\n\r\n"
    );
    for (class, tasty) in [("Shapes", "Shapes"), ("Rect", "Rect"), ("Shape", "Shape"), ("Cell", "Cell")] {
        let tasty = entry(&mut zip, &format!("geo/{tasty}.tasty"));
        let uuid = tasty::Tasty::parse(&tasty).expect("our TASTy").header.uuid;
        let class = entry(&mut zip, &format!("geo/{class}.class"));
        let attribute = [&[0, 0, 0, 16][..], &uuid].concat();
        assert!(class.windows(attribute.len()).any(|w| w == attribute), "the TASTY attribute of {tasty:?}");
    }
    assert!(zip.by_name("geo/Shapes$.tasty").is_err() && zip.by_name("geo/Rect$.tasty").is_err());
    assert_trees_read_strictly(&mut zip);
}

/// Asserts that our reader reads the trees of each TASTy file in `zip`, a case class's among them,
/// without error or dangling reference.
fn assert_trees_read_strictly(zip: &mut zip::ZipArchive<std::fs::File>) {
    let names: Vec<String> =
        zip.file_names().filter_map(Result::ok).filter(|n| n.ends_with(".tasty")).map(String::from).collect();
    assert!(names.iter().any(|n| n == "geo/Dot.tasty"), "a case class among {names:?}");
    for name in names {
        let bytes = entry(zip, &name);
        let file = tasty::Tasty::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let trees = file.section("ASTs").expect("an ASTs section").bytes;
        let ast = tasty::Ast::parse(trees).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!(ast.dangling_refs(), 0, "{name}");
    }
}

/// The trees, positions and flag attributes of TASTy `bytes`.
fn trees_and_flags(bytes: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let file = tasty::Tasty::parse(bytes).expect("a TASTy file");
    let section = |name| file.section(name).expect(name).bytes.to_vec();
    let attributes = file.attributes().expect("attributes");
    let flags =
        attributes.iter().filter_map(|a| if let tasty::Attribute::Flag(f) = a { Some(*f) } else { None });
    (section("ASTs"), section("Positions"), flags.collect())
}

/// The files of tests/jvm/tastylib and the classes of each whose dotty TASTy lies beside them as
/// `<file>-<class>.tasty`, those of `scala.probe` compiled with `-sourceroot` at the repository's
/// root.
const DOTTY_TASTY: &[(&str, &[&str])] = &[
    ("captures", &["Break"]),
    ("captures38", &["Break"]),
    ("inferredcaptures", &["Lambdas", "Locals"]),
    ("inferredcaptures2", &["Generic", "Conds", "Overrides", "Closures", "Outer"]),
    ("selectorannotations", &["scala/probe/Selectors"]),
    (
        "nestedannotations",
        &["scala/probe/Cell", "scala/probe/Concat", "scala/probe/Ops", "scala/probe/Patterns"],
    ),
    ("superaccessorannotations", &["scala/probe/Lengths", "scala/probe/Sizes"]),
    ("patterndefs", &["scala/probe/Chains"]),
    ("captureproxy", &["scala/probe/Proxy"]),
    ("capturesuper", &["scala/probe/SeqViews"]),
    ("nestedcaptures", &["scala/probe/Outer"]),
    ("genericsuperaccessors", &["scala/probe/GenericLengths", "scala/probe/GenericSizes"]),
    ("typeparamannotations", &["scala/probe/TgLengths", "scala/probe/TgGeneric"]),
    (
        "inlineaccessors",
        &["scala/probe/Counter", "scala/probe/Counters", "scala/probe/Outer", "scala/probe/Derived"],
    ),
    ("constructors", &["scala/probe/Base", "scala/probe/Derived"]),
    ("beanproperties", &["scala/probe/Beans"]),
    ("caseclasses", &["scala/probe/Pair", "scala/probe/Words"]),
    ("views", &["scala/probe/Views"]),
    ("widenedargs", &["W"]),
    ("typercaptures", &["G", "Mk", "ByNames"]),
    ("lambdabounds", &["scala/probe/LambdaBounds"]),
    ("witnessnames", &["scala/probe/WitnessNames"]),
    ("seencaptures", &["Q", "R"]),
    ("librarycaptures", &["scala/probe/Appending", "scala/probe/LibCaptures"]),
    ("thiscaptures", &["Node", "Plain", "Ops", "Ser"]),
    ("depsets", &["DepSets", "Slide"]),
];

/// The trees, positions and flag attributes of the classes of `DOTTY_TASTY` are dotty's: the
/// field `label: Label[T]^{}` of captures.scala keeps its capture set as a type and, under source
/// 3.8, as `new retains[Nothing]()`; inferred types of inferredcaptures.scala keep the capture
/// sets of the types they are inferred from, emptied, those of inferredcaptures2.scala through
/// generic calls, conditionals, matches and closures, a type argument none from an argument whose
/// capture sets are only around it, a type argument the top capture set only of a path argument
/// (widenedargs.scala), the typer's own inferred types in typercaptures.scala, and an overriding member's its overridden member's as they
/// are, their `this` seen from the overriding class (seencaptures.scala), a library member's
/// capture sets (librarycaptures.scala) seen from the class and with the overriding member's or
/// the super accessor's parameters, the type of `this` its class's self type with the self
/// type's capture set (thiscaptures.scala), a typed pattern's or `new`'s written capture set
/// naming a method's or class's parameter as it is, its pattern variable's emptied
/// (depsets.scala); annotated types take the annotation tree of their own
/// origin in match selectors and patterns, inferred types, super accessors and inline accessors,
/// in super accessors with their class's type parameters seen from the accessor's class; secondary constructors have their own type parameters and parameters passed on to the
/// superclass are its parameters' aliases; `@BeanProperty` values get their bean accessors;
/// synthesized case class members refer to package members through the package's `this`, and
/// numeric widenings their companion through the prefix of the widened type; a type lambda
/// written as a wildcard's bound pickles without variances; a type with context bounds keeps its
/// `@WitnessNames` annotation. Needs a JDK (java on
/// the PATH), as dotty's did.
#[test]
fn probes_pickle_as_dotty_s() {
    if Command::new("java").arg("-version").output().is_err() {
        return;
    }
    for &(name, classes) in DOTTY_TASTY {
        let out = root().join("tmp/jvm-test").join(name);
        let _ = std::fs::remove_dir_all(&out);
        let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
            .current_dir(root())
            .arg("-d")
            .arg(&out)
            .arg(format!("tests/jvm/tastylib/{name}.scala"))
            .output()
            .expect("running scivolo-jvm");
        assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
        for class in classes {
            let ours = std::fs::read(out.join(format!("{class}.tasty"))).expect("our TASTy");
            let simple = class.rsplit('/').next().expect("a class name");
            let theirs = std::fs::read(root().join(format!("tests/jvm/tastylib/{name}-{simple}.tasty")));
            let same = trees_and_flags(&ours) == trees_and_flags(&theirs.expect("dotty's"));
            assert!(same, "{name}'s {class}.tasty differs from dotty's");
        }
    }
}

/// A capture-checked file compiled before a plain one in one unit keeps its capture sets, and
/// the plain one gets none: both pickle as dotty's compiled apart.
#[test]
fn capture_sets_are_kept_per_file() {
    let out = root().join("tmp/jvm-test/mixedcaptures");
    let _ = std::fs::remove_dir_all(&out);
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
        .current_dir(root())
        .arg("-d")
        .arg(&out)
        .args(["tests/jvm/tastylib/typercaptures.scala", "tests/jvm/tastylib/plainiterators.scala"])
        .output()
        .expect("running scivolo-jvm");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    for (name, class) in [("typercaptures", "G"), ("plainiterators", "PlainIterators")] {
        let ours = std::fs::read(out.join(format!("{class}.tasty"))).expect("our TASTy");
        let theirs = std::fs::read(root().join(format!("tests/jvm/tastylib/{name}-{class}.tasty")));
        let same = trees_and_flags(&ours) == trees_and_flags(&theirs.expect("dotty's"));
        assert!(same, "{class}.tasty differs from dotty's");
    }
}

/// The annotated types of tests/jvm/tastylib/annotlib.jar, dotty's classes of annotlib/, keep
/// their library's annotation trees in the TASTy of libraryannotations.scala as in dotty's, each
/// type its own, also in a super accessor, and in the quotes of quotedannotations.scala; one whose tree refers to a
/// parameter, which dotty maps, is refused.
#[test]
fn library_annotations_pickle_as_dotty_s() {
    let out = root().join("tmp/jvm-test/libraryannotations");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("tmp/jvm-test/libraryannotations");
    let compile = |source: &str| {
        Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
            .current_dir(root())
            .args(["--cp", "tests/jvm/tastylib/annotlib.jar", "-d"])
            .arg(&out)
            .arg(source)
            .output()
            .expect("running scivolo-jvm")
    };
    let run = compile("tests/jvm/tastylib/libraryannotations.scala");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    for class in ["Uses", "Reversed"] {
        let ours = std::fs::read(out.join(format!("scala/probe/{class}.tasty"))).expect("our TASTy");
        let theirs = root().join(format!("tests/jvm/tastylib/libraryannotations-{class}.tasty"));
        let theirs = std::fs::read(theirs).expect("dotty's");
        assert!(trees_and_flags(&ours) == trees_and_flags(&theirs), "{class}.tasty differs from dotty's");
    }

    let source = out.join("dependent.scala");
    std::fs::write(&source, "class D(b: annotlib.Box[Int]) { def d = b.dependent(2) }").expect("writing");
    let run = compile(source.to_str().expect("a path"));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        !run.status.success() && stderr.contains("referring to other than static definitions"),
        "{stderr}"
    );

    let run = compile("tests/jvm/tastylib/quotedannotations.scala");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let ours = std::fs::read(out.join("quotedannotations$package$.class")).expect("our class");
    let theirs = std::fs::read_to_string(root().join("tests/jvm/tastylib/quotedannotations.quotes"))
        .expect("dotty's pickled quotes");
    for quote in theirs.lines().map(str::as_bytes) {
        assert!(ours.windows(quote.len()).any(|w| w == quote), "a quote differs from dotty's");
    }
}

/// A named tuple type of tests/jvm/tastylib/exportedaliases/lib.scala, read from its TASTy, reduces
/// through the export forwarders of match type aliases: use.scala selects a field of it.
#[test]
fn exported_match_aliases_reduce_from_tasty() {
    let out = root().join("tmp/jvm-test/exportedaliases");
    let _ = std::fs::remove_dir_all(&out);
    let (lib, user) = (out.join("lib"), out.join("use"));
    let compile = |dest: &Path, flags: &[&str], source: &str| {
        let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
            .current_dir(root())
            .args(flags)
            .arg("-d")
            .arg(dest)
            .arg(format!("tests/jvm/tastylib/exportedaliases/{source}.scala"))
            .output()
            .expect("running scivolo-jvm");
        assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    };
    compile(&lib, &[], "lib");
    compile(&user, &["--cp", lib.to_str().expect("a path")], "use");
    if Command::new("java").arg("-version").output().is_err() {
        eprintln!("no java on the PATH: not run");
        return;
    }
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}:{}", user.display(), lib.display(), library.display());
    let run = Command::new("java").args(["-Xverify:all", "-cp", &classpath, "Use"]).output().expect("java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "1\n");
}

/// A dependent function type whose function type parent dotty gives the capture sets of its
/// refinement is refused, and so is the inferred function type of a closure whose parameters have
/// capture sets (`@declared` in dotty's) or whose result's capture sets name its parameter (a
/// dependent function type in dotty's), and a match on `this` of a class whose self type is its
/// own with a capture set, which dotty's patterns keep.
#[test]
fn untold_capture_sets_are_refused() {
    let out = root().join("tmp/jvm-test/untoldcaptures");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("tmp/jvm-test/untoldcaptures");
    let header = "import language.experimental.captureChecking\nclass Label[T]\nclass A\nclass B\n\
                  trait Coll:\n  def label: Label[Int]^{this}\n";
    let cases = [
        (
            "dependent",
            "object D:\n  val f: (x: A^) -> Label[Int]^{x} = x => Label[Int]()\n",
            "drops its refinement's capture sets",
        ),
        (
            "declared",
            "object E:\n  def g() =\n    val f = (x: A^) => 1\n    1\n",
            "a closure whose parameters have capture sets",
        ),
        (
            "dependentclosure",
            "object F:\n  def g() =\n    val f = (x: Coll) => x.label\n    1\n",
            "a closure whose result's capture sets name its parameters",
        ),
        (
            "selfmatch",
            "trait G[+A]:\n  self: G[A]^ =>\n  def m(): Int = self match\n    case s: Product => 1\n    case _ => 2\n",
            "a match on `this` of a class whose self type has a capture set",
        ),
    ];
    for (name, body, refusal) in cases {
        let source = out.join(format!("{name}.scala"));
        std::fs::write(&source, format!("{header}{body}")).expect("writing");
        let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm"))
            .arg("-d")
            .arg(out.join(name))
            .arg(&source)
            .output()
            .expect("running scivolo-jvm");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(!run.status.success() && stderr.contains(refusal), "{name}: {stderr}");
    }
}

/// scala-cli 3.9.0 compiles tests/jvm/tastylib/user.scala against the jar, and the result runs
/// on `java -Xverify:all`, when both are on the PATH.
#[test]
fn scala_cli_compiles_against_the_jar() {
    let missing = |tool: &str| Command::new(tool).arg("--version").output().is_err();
    if missing("scala-cli") || missing("java") {
        eprintln!("no scala-cli or java on the PATH: skipped");
        return;
    }
    let jar = geo_jar();
    let dir = jar.parent().expect("a directory").to_owned();
    let compiled = Command::new("scala-cli")
        .args(["--power", "compile", "-S", "3.9.0", "--server=false", "--quiet", "--workspace"])
        .arg(dir.join("ws"))
        .arg("-d")
        .arg(dir.join("user"))
        .arg("--jar")
        .arg(&jar)
        .arg(root().join("tests/jvm/tastylib/user.scala"))
        .output()
        .expect("running scala-cli");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}:{}", dir.join("user").display(), jar.display(), library.display());
    let run = Command::new("java").args(["-Xverify:all", "-cp", &classpath, "User"]).output().expect("java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let check = std::fs::read_to_string(root().join("tests/jvm/tastylib/user.check")).expect("user.check");
    assert_eq!(String::from_utf8_lossy(&run.stdout), check);
}
