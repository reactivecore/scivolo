//! Several sources compiled as one unit: each sees the definitions of the others, and errors
//! name the file they are in.

mod snippet;

use snippet::{dump, jars};
use std::path::Path;
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

#[test]
fn sources_refer_to_each_other_and_errors_name_their_file() {
    let Some(jars) = jars() else { return };
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
fn qualified_protected_members_are_read_inside_their_qualifier_without_accessors() {
    let Some(jars) = jars() else { return };
    let m = SourceFile::new("m.scala", "package p.q\nclass M(protected[p] var elems: Array[Long])");
    let u = SourceFile::new("u.scala", "package p.r\nobject U { def f(m: p.q.M) = m.elems.length }");
    let u = dump(&jars, &[m, u], "p.r.U$");
    assert!(!u.contains("protected$"), "{u}");
    let c = "package p.q\nclass C { protected[C] def s = 1 }\nobject C { def f(c: C) = c.s }\n\
             object O { protected[O] def t = 2 }\nclass O { def g = O.t }";
    let c = SourceFile::new("c.scala", c);
    for name in ["p.q.C$", "p.q.O"] {
        let d = dump(&jars, std::slice::from_ref(&c), name);
        assert!(!d.contains("protected$"), "the qualifier's companion: {d}");
    }
}

#[test]
fn imports_are_typed_only_when_their_selectors_may_name_what_is_looked_up() {
    let Some(jars) = jars() else { return };
    let a = SourceFile::new("a.scala", "object A { def g = p.V.f }");
    let p = "package p\nimport p.T.X\nimport q.D\nimport q.E\nobject T extends D { class X }\n\
             object V { def f: E = new E {} }";
    let q = SourceFile::new("q.scala", "package q\ntrait D\ntrait E");
    let a = dump(&jars, &[a, SourceFile::new("p.scala", p), q], "A$");
    assert!(a.contains("@p.V$"), "{a}");
    let a = SourceFile::new("a.scala", "object A { val x = B.h }");
    let b = "import scala.collection.mutable.ListBuffer\nobject B { def h = ListBufer }";
    let mut u = Universe::open(&jars).unwrap();
    let sources = [a, SourceFile::new("b.scala", b)];
    let error = lower::compile_sources(&mut u, &sources, Config::from_options(&[]), None).err().unwrap().what;
    assert!(
        error.contains("Not found: ListBufer - did you mean ListBuffer?"),
        "an untyped import's hint: {error}"
    );
}

#[test]
fn a_refused_file_leaves_out_itself_and_the_files_needing_it() {
    let Some(jars) = jars() else { return };
    let sources = [
        SourceFile::new("a.scala", "object A { def f: Int = 1 }"),
        SourceFile::new("b.scala", "object B { def g: Int = \"s\" }"),
        SourceFile::new("c.scala", "object C { def h: Int = B.g + A.f }"),
        SourceFile::new("d.scala", "import scala.language.`3.9-migration`\nobject D"),
        SourceFile::new("e.scala", "import scala.language.`3.9-migration`\nobject E"),
        SourceFile::new("f.scala", "object F { def f = { var i = 0; i += 1; i } }"),
    ];
    let mut open = || Universe::open(&jars).unwrap();
    let isolated = lower::compile_sources_isolated(&mut open, &sources, Config::from_options(&[]), None)
        .unwrap_or_else(|r| panic!("{}", r.what));
    let names: Vec<String> = isolated.out.into_iter().map(|f| f.name).collect();
    assert!(names.contains(&"A".to_owned()) && !names.iter().any(|n| n.starts_with(['B', 'C'])), "{names:?}");
    assert!(names.contains(&"F".to_owned()), "an assignment operator beside another file's error: {names:?}");
    let refused: Vec<&str> = isolated.refused.iter().map(|r| r.file.as_str()).collect();
    assert_eq!(refused, ["d.scala", "e.scala", "b.scala", "c.scala"]);
    assert!(isolated.refused[3].refusal.what.contains("c.scala:1:"), "{}", isolated.refused[3].refusal.what);
}

#[test]
fn refusals_after_typing_are_collected_from_all_files_at_once() {
    let Some(jars) = jars() else { return };
    let deferred = |o: &str| {
        format!(
            "trait T{o} {{ inline def f: Int }}\nobject {o} {{ inline def g(t: T{o}) = t.f; def h(t: T{o}) = g(t) }}"
        )
    };
    let sources = [
        SourceFile::new("a.scala", "object A { def f = ${ ??? } }".to_owned()),
        SourceFile::new("b.scala", deferred("B")),
        SourceFile::new("c.scala", deferred("C")),
        SourceFile::new("d.scala", "object D { def h: Int = 1 }".to_owned()),
        SourceFile::new("e.scala", "object E { def h: Int = \"s\" }".to_owned()),
    ];
    let mut attempts = 0;
    let mut open = || {
        attempts += 1;
        Universe::open(&jars).unwrap()
    };
    let isolated = lower::compile_sources_isolated(&mut open, &sources, Config::from_options(&[]), None)
        .unwrap_or_else(|r| panic!("{}", r.what));
    let refused: Vec<&str> = isolated.refused.iter().map(|r| r.file.as_str()).collect();
    assert_eq!(refused, ["a.scala", "b.scala", "c.scala", "e.scala"]);
    assert!(
        isolated.refused[2].refusal.what.contains("deferred inline"),
        "{}",
        isolated.refused[2].refusal.what
    );
    assert!(isolated.out.iter().any(|f| f.name == "D$"));
    assert_eq!(attempts, 2);
}

#[test]
fn library_java_classes_redefined_and_lifted_classes_in_units() {
    let Some(jars) = jars() else { return };
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
fn files_of_a_unit_lower_as_alone() {
    let Some(jars) = jars() else { return };
    let dumps = |sources: &[SourceFile]| {
        let mut u = Universe::open(&jars).unwrap();
        let (files, _) = lower::compile_sources(&mut u, sources, Config::from_options(&[]), None)
            .unwrap_or_else(|r| panic!("{}", r.what));
        let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
        let mut out: Vec<(String, String)> = files
            .iter()
            .map(|f| {
                let defs = nir::Reader::new(&f.bytes, &mut caches, &mut pool).unwrap().all().unwrap();
                let dump = defs.iter().map(|d| nir::show::defn_with_debug_info(&pool, d));
                (f.name.clone(), dump.collect::<Vec<_>>().join("\n"))
            })
            .collect();
        out.sort();
        out
    };
    let same = |a: &str, b: &str| {
        let (a, b) = (|| SourceFile::new("a.scala", a), || SourceFile::new("b.scala", b));
        let mut alone = dumps(&[a()]);
        alone.extend(dumps(&[b()]));
        alone.sort();
        assert_eq!(dumps(&[a(), b()]), alone);
    };
    let file = |o: &str| {
        format!(
            "case class P{o}(a: Int, b: String)\nobject {o} {{ var r = Array(1)\n def i = r(0) += 1\n \
             def f = List(1, 2).map(_ + 1).filter(_ > 1)\n def t = (8, 9)._1\n \
             def g = for {{ (x, y) <- Option((1, 2)) }} yield x\n \
             lazy val z = 1\n def l = {{ lazy val y = 2; y }}\n def n = new Object {{ override def toString = \"n\" }}\n \
             def m(x: Any) = x match {{ case P{o}(a, _) => a; case _ => 0 }}\n \
             def k(n: Int) = {{ class L {{ lazy val v = n; def f = List(1).map(_ + v) }}; new L().f }}\n \
             @annotation.tailrec def t(k: Int): Int = if k == 0 then 0 else t(k - 1)\n \
             def c = summon[deriving.Mirror.ProductOf[P{o}]].fromProduct((1, \"s\")) }}"
        )
    };
    same(&file("A"), &file("B"));
    same(&["import scala.language.`2.13`\n", &file("A")].concat(), &file("B"));
}

#[test]
fn empty_package_definitions_and_top_level_packages_shadow_root_imports() {
    let Some(jars) = jars() else { return };
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
    let Some(jars) = jars() else { return };
    let a = "given Ordering[String] = Ordering.String.reverse\nobject E { def o = summon[Ordering[String]] }";
    let b = "package p\nobject P { def o = summon[Ordering[String]] }";
    let sources = [SourceFile::new("a.scala", a), SourceFile::new("b.scala", b)];
    let e = dump(&jars, &sources, "E$");
    assert!(e.contains("@a$package$"), "{e}");
    let p = dump(&jars, &sources, "p.P$");
    assert!(!p.contains("@a$package$") && p.contains("@scala.math.Ordering$String$"), "{p}");
}

#[test]
fn array_branches_in_reference_contexts() {
    let Some(jars) = jars() else { return };
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
    let Some(jars) = jars() else { return };
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
    let Some(jars) = jars() else { return };
    let source = SourceFile::new(
        "extern.scala",
        "import scala.scalanative.posix.unistd.*\n\
         object E { def recv: scala.scalanative.posix.unistd.type = { println(\"receiver\"); scala.scalanative.posix.unistd }; \
         def read = recv.environ; def imported = environ; def write = { recv.environ = null } }",
    );
    let dump = dump(&jars, &[source], "E$");
    assert_eq!(dump.matches("load[ptr]").count(), 2, "{dump}");
    assert_eq!(dump.matches("store[ptr]").count(), 1, "{dump}");
    assert!(dump.contains("::extern environ"), "{dump}");
    assert_eq!(dump.matches("recv()").count(), 1, "{dump}");
}

#[test]
fn extern_vararg_declarations() {
    let Some(jars) = jars() else { return };
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
    let Some(mut jars) = jars() else { return };
    jars.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libs/jdk-stubs.jar"));
    let source = SourceFile::new(
        "statics.scala",
        "object S { def now = java.time.Instant.ofEpochMilli(1L); \
         def f(c: Char): Int = (c: @scala.annotation.switch) match { case 'a' => 1; case 'b' => 2; case 'c' => 3; case _ => 4 } }",
    );
    let dump = dump(&jars, &[source], "S$");
    assert!(dump.contains("@java.time.Instant::static ofEpochMilli(long)"), "{dump}");
    assert!(dump.contains("switch"), "{dump}");
}

#[test]
fn javalib_names_members_of_jdk_classes_scala_native_renames() {
    let Some(mut jars) = jars() else { return };
    let jdk = jars.len()..jars.len() + 1;
    jars.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../libs/jdk-stubs.jar"));
    let source = SourceFile::new(
        "loader.scala",
        "package java.lang\nclass Loader { def name: String = this.getClass().getName() }",
    );
    let mut u = Universe::open(&jars).unwrap();
    u.set_jdk_jars(jdk);
    u.set_javalib_sources();
    let refusal = lower::compile(&mut u, &source, &[]).err().map(|r| r.what);
    assert_eq!(refusal, None, "java.lang.Class is public as the JDK's");
}

const LEGACY_UNAPPLY: &str = "package legacy
case class Wrap[T](t: T)
class Dummy
trait Functor[F[_]]
object M {
  type Apply[T[_]] = T[Dummy]
  type Unapply[F[_[_]], T] = T match { case Wrap[Apply[a]] => F[a] }
  val f: Unapply[Functor, Wrap[List[Dummy]]] = new Functor[List] {}
}";

#[test]
fn match_types_reduce_by_the_rules_of_the_code_at_hand() {
    let Some(jars) = jars() else { return };
    let mut u = Universe::open(&jars).unwrap();
    let sources = [SourceFile::new("a.scala", LEGACY_UNAPPLY)];
    let cfg = Config::from_options(&["-source:3.3"]);
    lower::compile_sources(&mut u, &sources, cfg, None).map_err(|r| r.what).unwrap();
    let m = u.package("legacy").and_then(|p| u.decl(p, "M$").ok()).unwrap();
    let m = *m.iter().find(|&&c| u.symbol(c).kind == tasty::Kind::Class).unwrap();
    let unapply = u.decl(m, "Unapply").unwrap()[0];
    let unapply = u.intern_type(tasty::Type::Sym(None, unapply));
    let (functor, wrap, dummy) =
        (u.class_ref("legacy.Functor"), u.class_ref("legacy.Wrap"), u.class_ref("legacy.Dummy"));
    let list = u.class_ref("scala.collection.immutable.List");
    let list_dummy = u.intern_applied(list, &[dummy]);
    let scrutinee = u.intern_applied(wrap, &[list_dummy]);
    let t = u.intern_applied(unapply, &[functor, scrutinee]);

    assert_eq!(u.match_type_rules, tasty::MatchTypeRules::Spec, "the 3.3 unit's rules left behind");
    assert_eq!(u.try_normalize(t), None, "a 3.9 reduction of a pattern outside SIP-56");
    u.match_type_rules = tasty::MatchTypeRules::Legacy;
    let reduced = u.try_normalize(t).map(|r| u.show(r));
    assert!(reduced.is_some_and(|r| r.contains("Functor") && r.contains("List")), "a 3.3 reduction");
    u.match_type_rules = tasty::MatchTypeRules::Both;
    u.reductions_exhausted = false;
    assert_eq!(u.try_normalize(t), None, "differing rules refused");
    assert!(u.reductions_exhausted);
}
