//! Source versions below 3.7: what the typer refuses where dotty's older rules apply.

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
fn implicits_are_ranked_from_3_7() {
    assert_eq!(typed(RANKED, "3.7"), Ok(()));
    assert_eq!(typed(RANKED, "3.6"), Err("ranking implicits below Scala 3.7".into()));
}

#[test]
fn implicits_defined_after_the_search_are_refused_below_3_6() {
    let refused = Err("an implicit defined after the search below Scala 3.6".to_owned());
    assert_eq!(typed(DEFINED_AFTER, "3.5"), refused);
    assert_eq!(typed(DEFINED_AFTER, "3.6"), Ok(()));
}

#[test]
fn source_versions_from_3_3() {
    assert_eq!(typed("object V { val x = 1 }", "3.3"), Ok(()));
    assert_eq!(typed("object V { val x = 1 }", "3.2"), Err("source versions other than 3.3 to 3.9".into()));
}
