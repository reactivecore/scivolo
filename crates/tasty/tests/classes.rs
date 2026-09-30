use std::path::Path;
use tasty::{Kind, SymbolId, Universe};

fn stdlib() -> Option<Universe> {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs/scala-library-3.9.0.jar");
    if !jar.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", jar.display());
        return None;
    }
    Some(Universe::open(&[jar]).unwrap())
}

fn class(u: &mut Universe, pkg: &str, name: &str) -> SymbolId {
    let p = u.package(pkg).unwrap();
    u.decl(p, name).unwrap().into_iter().find(|&c| u.symbol(c).kind == Kind::Class).unwrap()
}

fn lin(u: &mut Universe, c: SymbolId) -> Vec<String> {
    u.linearization(c).into_iter().map(|b| u.full_name(b)).collect()
}

#[test]
fn parents_and_linearization() {
    let Some(mut u) = stdlib() else { return };
    let some = class(&mut u, "scala", "Some");
    let parents: Vec<String> = u.parents(some).into_iter().map(|t| u.show(t)).collect();
    assert_eq!(parents, ["scala.Option[scala.Some.this.A]", "scala.Product", "scala.Serializable"]);
    let bases =
        ["scala.Some", "scala.Option", "scala.Product", "scala.Equals", "scala.collection.IterableOnce"];
    assert_eq!(lin(&mut u, some), bases);

    let list = class(&mut u, "scala.collection.immutable", "List");
    let l = lin(&mut u, list);
    let at = |n: &str| l.iter().position(|b| b == n).unwrap_or_else(|| panic!("{n} not in {l:?}"));
    assert_eq!(at("scala.collection.immutable.List"), 0);
    assert!(at("scala.collection.immutable.AbstractSeq") < at("scala.collection.AbstractSeq"));
    assert!(at("scala.collection.immutable.Seq") < at("scala.collection.Seq"));
    assert!(l.iter().filter(|b| *b == "scala.collection.IterableOnce").count() == 1);
}

#[test]
fn derivation_and_inherited_members() {
    let Some(mut u) = stdlib() else { return };
    let list = class(&mut u, "scala.collection.immutable", "List");
    let seq = class(&mut u, "scala.collection", "Seq");
    let option = class(&mut u, "scala", "Option");
    assert!(u.derives_from(list, seq) && !u.derives_from(seq, list) && !u.derives_from(list, option));

    let owners: Vec<String> = u
        .member(list, "foldLeft")
        .into_iter()
        .map(|m| u.symbol(m).owner.unwrap())
        .map(|o| u.full_name(o))
        .collect();
    assert!(
        !owners.is_empty() && owners.iter().all(|o| o != "scala.collection.immutable.List"),
        "{owners:?}"
    );
    assert!(!u.member(list, "map").is_empty());
    assert!(u.member(list, "noSuchMember").is_empty());

    let rich_int = class(&mut u, "scala.runtime", "RichInt");
    let any_val = class(&mut u, "scala", "AnyVal");
    assert!(u.derives_from(rich_int, any_val));
}
