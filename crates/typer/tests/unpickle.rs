//! Reading the bodies of the library's inline methods: overloads selected by signature, and the
//! bodies refused with the kind of their first construct outside the subset.

use std::path::Path;
use syntax::names::Names;
use tasty::{Kind, SymbolId, Universe};
use typer::dump::library_jars;
use typer::tpd::Tpd;
use typer::tpd::print::Printer;

fn universe() -> Option<Universe> {
    let jars = library_jars(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs"));
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", missing.display());
        return None;
    }
    Some(Universe::open(&jars).expect("the library"))
}

/// The inline methods named `name` of class `class`, of package `pkg`.
fn inline_defs(u: &mut Universe, pkg: &str, class: &str, name: &str) -> Vec<SymbolId> {
    let p = u.package(pkg).expect("a package");
    let c = u.decl(p, class).unwrap().into_iter().find(|&c| u.symbol(c).kind == Kind::Class).unwrap();
    let defs = u.decl(c, name).unwrap();
    defs.into_iter().filter(|&d| u.symbol(d).flags.has(tasty::tags::INLINE)).collect()
}

/// The body of `meth` printed, or why it is refused.
fn read(u: &mut Universe, meth: SymbolId) -> Result<String, &'static str> {
    let (mut tpd, mut names) = (Tpd::default(), Names::default());
    let def = typer::unpickle::inline_def(u, &mut tpd, &mut names, meth)?;
    let mut p = Printer { tpd: &tpd, names: &names, u, out: String::new(), source_len: 0 };
    p.tree(def);
    Ok(p.out)
}

#[test]
fn overloads_are_selected_by_signature() {
    let Some(mut u) = universe() else { return };
    let asserts = inline_defs(&mut u, "scala", "Predef$", "assert");
    assert_eq!(asserts.len(), 2);
    let bodies: Vec<String> = asserts.into_iter().map(|a| read(&mut u, a).unwrap()).collect();
    assert!(bodies.iter().any(|b| b.contains("assertFailed#()")));
    assert!(bodies.iter().any(|b| b.contains("assertFailed#(java.lang.Object)")));
}

#[test]
fn local_methods_and_closures_are_read_with_their_symbols() {
    let Some(mut u) = universe() else { return };
    let defs = inline_defs(&mut u, "scala.scalanative.runtime", "Continuations$", "break");
    assert!(!defs.is_empty());
    for d in defs {
        let body = read(&mut u, d).unwrap();
        assert!(body.contains("(Closure"), "{body}");
        assert!(
            body.contains("(DefDef 1..1@1 sym=\"scala.scalanative.runtime.Continuations$._$$anonfun"),
            "{body}"
        );
    }
}

#[test]
fn bodies_outside_the_subset_are_refused() {
    let Some(mut u) = universe() else { return };
    let cases = [
        ("scala", "Predef$", "valueOf", "summonFrom in a library inline body"),
        ("scala", "Tuple", "size", "match type in a library inline body"),
        ("scala", "Tuple", "map", "undecoded type in a library inline body"),
        ("scala.util", "boundary$", "apply", "selection in an unknown class in a library inline body"),
    ];
    for (pkg, class, name, why) in cases {
        let defs = inline_defs(&mut u, pkg, class, name);
        assert!(!defs.is_empty(), "{class}.{name}");
        for d in defs {
            assert_eq!(read(&mut u, d).err(), Some(why), "{class}.{name}");
        }
    }
}
