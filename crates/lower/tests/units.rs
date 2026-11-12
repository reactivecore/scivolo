//! Several sources compiled as one unit: each sees the definitions of the others, and errors
//! name the file they are in.

use std::path::Path;
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

#[test]
fn sources_refer_to_each_other_and_errors_name_their_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let compile = |a: &str, b: &str| {
        let sources = [SourceFile::new("a.scala", a), SourceFile::new("b.scala", b)];
        let mut u = Universe::open(&jars).unwrap();
        lower::compile_sources(&mut u, &sources, Config::from_options(&[]), None)
            .map(|(files, _)| files.into_iter().map(|f| f.name).collect::<Vec<_>>())
            .map_err(|r| r.what)
    };
    let a = "object A { def f: Int = B.g + 1 }";
    let names = compile(a, "object B { def g: Int = 2; def h: Int = A.f }").expect("compiled");
    assert!(names.contains(&"A".to_owned()) && names.contains(&"B".to_owned()), "{names:?}");
    let error = compile(a, "object B { def g: Int = \"s\" }").expect_err("a type error");
    assert!(error.starts_with("type errors:") && error.contains("b.scala:1:"), "{error}");
    let error = compile("sealed trait S", "class C extends S").expect_err("a sealed parent");
    assert!(
        error.contains("b.scala:1:7: Cannot extend sealed trait S in a different source file"),
        "{error}"
    );
    let error = compile("class C", "object C").expect_err("split companions");
    assert!(error.contains("companion object in different source files"), "{error}");
    let error = compile("import scala.language.`3.9-migration`\nobject A", "object B");
    assert!(error.expect_err("a migrating source version").contains("source versions"));
    let a = "import scala.language.`3.7`\nobject A { def f = B.g }";
    compile(a, "import scala.language.implicitConversions\nobject B { def g = 1 }")
        .expect("per-file imports");
    let a = "object A { def f = V(1, 2) }";
    let v = "case class V private (a: Int, b: Int)\nobject V { def apply(a: Int, b: Int): V = new V(a, b) }";
    compile(a, v).expect("the companion's apply replaces the private synthetic one");
}

#[test]
fn a_refused_file_leaves_out_itself_and_the_files_needing_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let sources = [
        SourceFile::new("a.scala", "object A { def f: Int = 1 }"),
        SourceFile::new("b.scala", "object B { def g: Int = \"s\" }"),
        SourceFile::new("c.scala", "object C { def h: Int = B.g + A.f }"),
        SourceFile::new("d.scala", "import scala.language.`3.9-migration`\nobject D"),
        SourceFile::new("e.scala", "import scala.language.`3.9-migration`\nobject E"),
    ];
    let mut open = || Universe::open(&jars).unwrap();
    let isolated = lower::compile_sources_isolated(&mut open, &sources, Config::from_options(&[]), None)
        .unwrap_or_else(|r| panic!("{}", r.what));
    let names: Vec<String> = isolated.out.into_iter().map(|f| f.name).collect();
    assert!(names.contains(&"A".to_owned()) && !names.iter().any(|n| n.starts_with(['B', 'C'])), "{names:?}");
    let refused: Vec<&str> = isolated.refused.iter().map(|r| r.file.as_str()).collect();
    assert_eq!(refused, ["d.scala", "e.scala", "b.scala", "c.scala"]);
    assert!(isolated.refused[3].refusal.what.contains("c.scala:1:"), "{}", isolated.refused[3].refusal.what);
}

#[test]
fn library_java_classes_redefined_and_lifted_classes_in_units() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let dump = |sources: &[SourceFile], name: &str| dump(&jars, sources, name);
    let a = SourceFile::new("a.scala", "package p\nobject A { val x = new Object {} }");
    let anon = dump(&[a, SourceFile::new("b.scala", "package p\nclass B")], "p.A$$anon$1");
    assert_eq!(anon.matches("class @p.A$$anon$1").count(), 1, "{anon}");
    let unit = SourceFile::new("j.scala", "object J { def f(): Object = scala.runtime.BoxedUnit.UNIT }");
    assert!(dump(&[unit], "J$").contains("ret unit"));
    let boxed_unit = SourceFile::new(
        "u.scala",
        "package scala.runtime\nclass BoxedUnit extends java.io.Serializable\nobject BoxedUnit { def UNIT = ??? }",
    );
    let native = SourceFile::new(
        "n.scala",
        "package scala.scalanative.runtime\nobject BoxedUnit extends scala.runtime.BoxedUnit",
    );
    let class = dump(&[boxed_unit, native], "scala.runtime.BoxedUnit");
    assert!(class.contains("class @scala.runtime.BoxedUnit : @java.lang.Object\n"), "{class}");
    let a = "class A { var r = Array(1)\n def f = r(0) += 1\n def g = for { t <- Option(1); (x, y) = (t, t) } yield x + y }";
    let b = "object B { def h(p: (Int, Int, Int)) = { val (a, b, c) = p; a + b + c } }";
    let b = dump(&[SourceFile::new("a.scala", a), SourceFile::new("b.scala", b)], "B$");
    assert!(b.contains("\"$2$\"") && !b.contains("\"$3$\""), "{b}");
    let a = SourceFile::new("a.scala", "object A { def f = p.Lib.f; def s = new p.S with p.T }");
    let b = "package p\nimport q.O.*\nobject x\ntrait T\nclass S { self: T => }\nobject Lib { transparent inline def f: Any = x }";
    let c = SourceFile::new("c.scala", "package q\nobject O { object x; trait T }");
    let a = dump(&[a, SourceFile::new("b.scala", b), c], "A$");
    assert!(a.contains("@p.x$") && !a.contains("@q.O$x$"), "{a}");
}

#[test]
fn empty_package_definitions_and_top_level_packages_shadow_root_imports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let a = "object Thread { def apply(s: String) = s }\nclass Record(val n: Int)";
    let b = "package p\nobject B { def f = io.Top.n }";
    let c = "object C { def f = Thread(\"x\"); def g = new Record(1).n; def h = io.Top.n + p.B.f }";
    let io = SourceFile::new("io.scala", "package io\nobject Top { def n = 1 }");
    let sources =
        [SourceFile::new("a.scala", a), SourceFile::new("b.scala", b), SourceFile::new("c.scala", c), io];
    let c = dump(&jars, &sources, "C$");
    assert!(c.contains("@Thread$") && c.contains("@Record") && c.contains("@io.Top$"), "{c}");
    assert!(!c.contains("@java.lang.Thread") && !c.contains("@java.lang.Record"), "{c}");
    assert!(dump(&jars, &sources, "p.B$").contains("@io.Top$"));
}

#[test]
fn empty_package_givens_are_not_seen_from_a_named_package() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let a = "given Ordering[String] = Ordering.String.reverse\nobject E { def o = summon[Ordering[String]] }";
    let b = "package p\nobject P { def o = summon[Ordering[String]] }";
    let sources = [SourceFile::new("a.scala", a), SourceFile::new("b.scala", b)];
    let e = dump(&jars, &sources, "E$");
    assert!(e.contains("@a$package$"), "{e}");
    let p = dump(&jars, &sources, "p.P$");
    assert!(!p.contains("@a$package$") && p.contains("@scala.math.Ordering$String$"), "{p}");
}

/// Lowers `sources` together and shows the NIR of class `name`.
fn dump(jars: &[std::path::PathBuf], sources: &[SourceFile], name: &str) -> String {
    let mut u = Universe::open(jars).unwrap();
    let files = match sources {
        [one] => lower::compile(&mut u, one, &[]).map_err(|r| r.what),
        many => lower::compile_sources(&mut u, many, Config::from_options(&[]), None)
            .map(|(files, _)| files)
            .map_err(|r| r.what),
    };
    let files = files.expect("lowered");
    let file = files.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} missing"));
    let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
    let defs = nir::Reader::new(&file.bytes, &mut caches, &mut pool).unwrap().all().unwrap();
    defs.iter().map(|d| nir::show::defn_with_debug_info(&pool, d)).collect::<Vec<_>>().join("\n")
}

#[test]
fn array_branches_in_reference_contexts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let source = SourceFile::new(
        "arrays.scala",
        "object Arrays { def f(c: Boolean, a: Array[Int], b: Array[Int]): AnyRef = if c then a else b; \
         def g(a: Array[String]): AnyRef = try a catch { case _: Exception => a }; \
         def h(c: Boolean, a: Array[Int], b: Array[String]): AnyRef = if c then a else b; \
         def k(c: Boolean): Any = if c then Array(1) else \"s\" }",
    );
    let mut u = Universe::open(&jars).unwrap();
    lower::compile(&mut u, &source, &[]).expect("array branches lower");
}

#[test]
fn default_alignment_is_emitted_on_fields() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let mut u = Universe::open(&jars).unwrap();
    let source =
        SourceFile::new("align.scala", "@scala.scalanative.annotation.align class A { var x: Long = 0L }");
    let files = lower::compile(&mut u, &source, &[]).unwrap();
    let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
    let mut reader = nir::Reader::new(&files[0].bytes, &mut caches, &mut pool).unwrap();
    let defns = reader.all().unwrap();
    assert!(
        defns.iter().any(|d| d.attrs.iter().any(|a| matches!(a, nir::Attr::Align { size: -1, group: None })))
    );
    let mut u = Universe::open(&jars).unwrap();
    let source = SourceFile::new(
        "align.scala",
        "@scala.scalanative.annotation.align(64) class A { var x: Long = 0L }",
    );
    assert!(lower::compile(&mut u, &source, &[]).err().unwrap().what.contains("with arguments"));
}

#[test]
fn reads_an_extern_field_from_the_library() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let source = SourceFile::new(
        "extern.scala",
        "import scala.scalanative.posix.unistd.*\n\
         object E { def recv: scala.scalanative.posix.unistd.type = { println(\"receiver\"); scala.scalanative.posix.unistd }; \
         def read = recv.environ; def imported = environ; def write = { recv.environ = null } }",
    );
    let mut u = Universe::open(&jars).unwrap();
    let files = lower::compile(&mut u, &source, &[]).unwrap();
    let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
    let mut reader =
        nir::Reader::new(&files.iter().find(|f| f.name == "E$").unwrap().bytes, &mut caches, &mut pool)
            .unwrap();
    let defs = reader.all().unwrap();
    let dump = defs.iter().map(|d| nir::show::defn_with_debug_info(&pool, d)).collect::<Vec<_>>().join("\n");
    assert_eq!(dump.matches("load[ptr]").count(), 2, "{dump}");
    assert_eq!(dump.matches("store[ptr]").count(), 1, "{dump}");
    assert!(dump.contains("::extern environ"), "{dump}");
    assert_eq!(dump.matches("recv()").count(), 1, "{dump}");
}

#[test]
fn extern_vararg_declarations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let source = SourceFile::new(
        "varargs.scala",
        "import scala.scalanative.unsafe.*\n@extern object libc { def printf(f: CString, xs: CVarArg*): CInt = extern }",
    );
    let mut u = Universe::open(&jars).unwrap();
    let files = lower::compile(&mut u, &source, &[]).unwrap();
    let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
    let mut reader =
        nir::Reader::new(&files.iter().find(|f| f.name == "libc$").unwrap().bytes, &mut caches, &mut pool)
            .unwrap();
    let defs = reader.all().unwrap();
    assert!(defs.iter().any(|d| match d.kind {
        nir::DefnKind::Declare { ty } => pool.is_variadic(ty),
        _ => false,
    }));
    let source = SourceFile::new(
        "erased.scala",
        "import language.experimental.erasedDefinitions\nimport scala.scalanative.unsafe.*\n@extern object C { def f(erased x: Int, xs: CVarArg*): CInt = extern }",
    );
    let mut u = Universe::open(&jars).unwrap();
    lower::compile(&mut u, &source, &[]).expect("erased parameters do not count in the extern signature");
}

#[test]
fn library_static_members_and_annotated_switch_selectors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    jars.push(root.join("libs/jdk-stubs.jar"));
    let source = SourceFile::new(
        "statics.scala",
        "object S { def now = java.time.Instant.ofEpochMilli(1L); \
         def f(c: Char): Int = (c: @scala.annotation.switch) match { case 'a' => 1; case 'b' => 2; case 'c' => 3; case _ => 4 } }",
    );
    let mut u = Universe::open(&jars).unwrap();
    let files = lower::compile(&mut u, &source, &[]).unwrap();
    let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
    let mut reader =
        nir::Reader::new(&files.iter().find(|f| f.name == "S$").unwrap().bytes, &mut caches, &mut pool)
            .unwrap();
    let defs = reader.all().unwrap();
    let dump = defs.iter().map(|d| nir::show::defn_with_debug_info(&pool, d)).collect::<Vec<_>>().join("\n");
    assert!(dump.contains("@java.time.Instant::static ofEpochMilli(long)"), "{dump}");
    assert!(dump.contains("switch"), "{dump}");
}
