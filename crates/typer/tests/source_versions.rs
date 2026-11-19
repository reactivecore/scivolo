//! Source versions other than 3.9: what the typer refuses where dotty's older rules apply.

use std::path::Path;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::parse::Parser;
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::library_jars;
use typer::{Fallback, Units};

fn universe() -> Option<Universe> {
    let jars = library_jars(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    Some(Universe::open(&jars).expect("the library"))
}

fn typed(text: &str, source: &str) -> Result<(), String> {
    let Some(mut u) = universe() else { return Ok(()) };
    let file = SourceFile::new("v.scala", text);
    let mut names = syntax::names::Names::default();
    let option = format!("-source:{source}");
    let mut p = Parser::new(&file, &mut names, Config::from_options(&[option.as_str()]));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &file, cfg).unit(root);
    assert!(!p.s.diags.has_errors(), "syntax errors");
    match Units::default().type_unit(&mut u, &mut p.ast, (p.s.names, &mut p.fresh), (root, cfg), None) {
        Ok(_) => Ok(()),
        Err(Fallback::Unsupported { what, .. }) => Err(what),
        Err(Fallback::Errors(d)) => {
            Err(format!("{:?}", d.unique().iter().map(|d| &d.message).collect::<Vec<_>>()))
        }
    }
}

const RANKED: &str = "class Base; class Sub extends Base
object R { given Base = Base(); given Sub = Sub(); val b = summon[Base] }";

const DEFINED_AFTER: &str = "object L { def g: Int = { given k: Long = 1L; given i: Int = summon[Long].toInt; given j: Long = 2L; i } }";

#[test]
fn implicit_priority_changes_are_refused_in_3_6_and_3_7_migration() {
    let refused = Err("implicit priority change between Scala 3.6 and 3.7".to_owned());
    assert_eq!(typed(RANKED, "3.5"), Ok(()));
    assert_eq!(typed(RANKED, "3.6"), refused);
    assert_eq!(typed(RANKED, "3.7-migration"), refused);
    assert_eq!(typed(RANKED, "3.7"), Ok(()));
}

#[test]
fn implicits_defined_after_the_search_are_refused_below_3_6() {
    let refused = Err("an implicit defined after the search below Scala 3.6".to_owned());
    assert_eq!(typed(DEFINED_AFTER, "3.5"), refused);
    assert_eq!(typed(DEFINED_AFTER, "3.6"), Ok(()));
}

const INLINE_RESULT: &str = "object I { transparent inline def mk(): Any = 1
def h[U](u: U)(v: U => Int): Int = v(u); val r = h(mk())(_ + 1) }";

#[test]
fn inline_results_are_constrained_by_wildcards_below_3_4() {
    assert_eq!(typed(INLINE_RESULT, "3.3"), Ok(()));
    assert_eq!(typed(INLINE_RESULT, "3.4"), Ok(()));
}

#[test]
fn legacy_constructs_are_errors_under_future_and_warnings_under_future_migration() {
    let cases = [
        ("object F { def inc(x: Int) = x + 1; val f = inc _ }", "eta-expansion is unnecessary"),
        ("object F { val xs = List(2, 1).sorted(Ordering.Int) }", "provided with a `using` clause"),
        ("object F { val Some(y) = Option(1) }", "is more specialized than"),
    ];
    for (text, error) in cases {
        assert_eq!(typed(text, "future-migration"), Ok(()), "{text}");
        assert!(typed(text, "future").is_err_and(|e| e.contains(error)), "{text}");
    }
}

const NOTHING_ARGUMENT: &str = "final class Bar[+F[_]]; object B { val b: Bar[Nothing] = new Bar[Nothing] }";

const PULL: &str = "abstract class Pull[+F[_], +O, +R]
object Pull:
  abstract class Terminal[+R] extends Pull[Nothing, Nothing, R]
  class StreamPullOps[F[_], O](self: Pull[F, O, Unit]):
    def flatMapOutput[F2[x] >: F[x], O2](): Pull[F2, O2, Unit] =
      self match
        case r: Terminal[?] => r
        case _              => ???
";

#[test]
fn nothing_arguments_of_any_kind_and_intersections_kept_apart_under_future() {
    assert_eq!(typed(NOTHING_ARGUMENT, "future"), Ok(()));
    assert!(typed(PULL, "future").is_err());
}

#[test]
fn source_versions_but_3_0_migration() {
    for v in ["3.0", "3.1", "3.2", "3.2-migration", "3.9-migration", "future-migration", "future"] {
        assert_eq!(typed("object V { val x = 1 }", v), Ok(()), "{v}");
    }
    assert_eq!(typed("object V { val x = 1 }", "3.0-migration"), Err("source version 3.0-migration".into()));
}
