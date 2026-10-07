//! Units typed one after another in one universe, each seeing the objects of the ones before.

use std::path::{Path, PathBuf};
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::{Kind, Universe};
use typer::dump::library_jars;
use typer::{Fallback, Units};

fn universe() -> Option<Universe> {
    let libs = std::env::var_os("SCALAI_LIBS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"));
    let jars = library_jars(&libs);
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    Some(Universe::open(&jars).expect("the library"))
}

fn type_unit(u: &mut Universe, units: &mut Units, name: &str, text: &str) -> Result<(), String> {
    let source = SourceFile::new(name, text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::from_options(&[]));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &source, cfg).unit(root);
    assert!(!p.s.diags.has_errors(), "syntax errors in {name}");
    match units.type_unit(u, &mut p.ast, p.s.names, root, cfg.features) {
        Ok(_) => Ok(()),
        Err(Fallback::Unsupported { what, .. }) => Err(format!("unsupported {what}")),
        Err(Fallback::Errors(d)) => {
            let messages: Vec<&str> = d.unique().iter().map(|d| d.message.as_str()).collect();
            Err(format!("type errors {messages:?}"))
        }
    }
}

/// The type of member `member` of top-level object `object`.
fn member_type(u: &mut Universe, object: &str, member: &str) -> String {
    let root = u.root();
    let class = u.decl(root, &format!("{object}$")).expect("a lookup");
    let class = *class.iter().find(|&&c| u.symbol(c).kind == Kind::Class).expect("the module class");
    let found = u.decl(class, member).expect("a lookup");
    let t = u.info(found[0]);
    u.show(t)
}

/// Types `lines` in order as units `rs$line$N`, each importing the ones before.
fn type_lines(u: &mut Universe, lines: &[&str]) {
    let mut units = Units::default();
    for (i, line) in lines.iter().enumerate() {
        let imports: String = (1..=i).map(|n| format!("import rs$line${n}.{{given, *}}; ")).collect();
        let text = format!("object rs$line${} {{ {imports}{line} }}", i + 1);
        assert_eq!(type_unit(u, &mut units, "line.scala", &text), Ok(()), "{text}");
    }
}

#[test]
fn second_unit_imports_the_first() {
    let Some(mut u) = universe() else { return };
    type_lines(
        &mut u,
        &["val x = 1; def f(y: Int): Int = y + x; class C(val a: Int)", "val y = f(x) + new C(2).a"],
    );
    assert_eq!(member_type(&mut u, "rs$line$2", "y"), "scala.Int");
}

#[test]
fn classes_of_earlier_units() {
    let Some(mut u) = universe() else { return };
    type_lines(
        &mut u,
        &[
            "sealed trait S; case class P(x: Int) extends S; class C(val a: Int)",
            "case class Q(y: Int) extends S; def h(s: S): Int = s match { case P(n) => n; case _ => 0 }",
            "val y = h(P(1)) + P(2).x + C(3).a + h(Q(4))",
        ],
    );
    assert_eq!(member_type(&mut u, "rs$line$3", "y"), "scala.Int");
}

#[test]
fn unit_after_a_failed_one() {
    let Some(mut u) = universe() else { return };
    let mut units = Units::default();
    let first = "object rs$line$1 { def f(xs: Int*): Int = xs.sum }";
    assert_eq!(type_unit(&mut u, &mut units, "line.scala", first), Ok(()));
    let failed = "object rs$line$2 { import rs$line$1.*; val s: String = f(1) }";
    assert!(type_unit(&mut u, &mut units, "line.scala", failed).is_err());
    let third = "object rs$line$3 { import rs$line$1.*; val y = f(1, 2) + f() }";
    assert_eq!(type_unit(&mut u, &mut units, "line.scala", third), Ok(()));
    assert_eq!(member_type(&mut u, "rs$line$3", "y"), "scala.Int");
}

#[test]
fn definition_left_incomplete_by_a_failed_unit() {
    let Some(mut u) = universe() else { return };
    let mut units = Units::default();
    let first = "object rs$line$1 { trait T(x: Int) { def g = 1 } }";
    assert_eq!(
        type_unit(&mut u, &mut units, "line.scala", first),
        Err("unsupported trait parameters".into())
    );
    let second = "object rs$line$2 { import rs$line$1.*; def h(t: T) = t.g }";
    assert_eq!(
        type_unit(&mut u, &mut units, "line.scala", second),
        Err("unsupported definition left incomplete by a failed unit".into())
    );
}

#[test]
fn deferred_members_of_earlier_units() {
    let Some(mut u) = universe() else { return };
    type_lines(&mut u, &["trait T { def a: Int; def b: Int = 1 }", "class C extends T { def a = 2 }"]);
    let root = u.root();
    let object = u.decl(root, "rs$line$1$").expect("a lookup");
    let object = *object.iter().find(|&&c| u.symbol(c).kind == Kind::Class).expect("the module class");
    let t = u.decl(object, "T").expect("a lookup")[0];
    let a = u.decl(t, "a").expect("a lookup")[0];
    let b = u.decl(t, "b").expect("a lookup")[0];
    assert!(u.is_deferred(a));
    assert!(!u.is_deferred(b));
}

#[test]
fn synthetic_symbols_are_shared() {
    let Some(mut u) = universe() else { return };
    let line = "def f(xs: Int*): Int = if xs.isEmpty then throw Exception() else xs.sum; val y = f(1, 2)";
    for n in 1..=2 {
        let text = format!("object rs$line${n} {{ {line} }}");
        assert_eq!(type_unit(&mut u, &mut Units::default(), "line.scala", &text), Ok(()));
    }
    let scala = u.package("scala").expect("scala");
    let repeated = u.symbol(scala).decls.iter().filter(|&&d| u.symbol(d).name == "<repeated>").count();
    assert_eq!(repeated, 1);
    let ops = u.package("<special-ops>").expect("<special-ops>");
    assert_eq!(u.symbol(ops).decls.len(), 1);
}
